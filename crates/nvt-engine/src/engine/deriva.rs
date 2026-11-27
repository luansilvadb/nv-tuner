//! Verificação e tratamento de deriva entre o estado esperado e o DRS.

use super::*;

impl<S: DriverStore> Engine<S> {
    // ── Deriva ─────────────────────────────────────────────────────────────

    /// Compara o estado esperado com o DRS atual e classifica. Somente leitura.
    pub fn verificar_deriva(&self) -> Resultado<Deriva> {
        let versao = self.store.versao_driver()?.raw;
        let mut divergencias = Vec::new();
        for e in &self.armazem.estado.esperado {
            let lido: Option<Setting> = match self.store.ler_setting(&e.perfil, e.id) {
                Ok(s) => s,
                // Perfil sumiu: tudo dele voltou ao padrão.
                Err(err) if perfil_ausente(&err) => None,
                // Erro de um perfil de jogo: isolado (fica para o próximo ciclo).
                Err(_) if matches!(e.perfil, ProfileRef::Named(_)) => continue,
                Err(err) => return Err(err.into()),
            };
            let atual = lido.as_ref().and_then(|s| s.value.as_u32());
            if atual == Some(e.valor) {
                continue;
            }
            let voltou_ao_padrao = lido.as_ref().is_none_or(|s| !s.is_user_specified());
            divergencias.push(Divergencia {
                esperado: e.clone(),
                atual,
                voltou_ao_padrao,
            });
        }
        let classe = classificar_deriva(
            self.armazem.estado.esperado.len(),
            &divergencias,
            self.armazem.estado.driver_visto,
            versao,
        );
        Ok(Deriva {
            classe,
            divergencias,
            versao_driver: versao,
        })
    }

    /// Trata a deriva. **Só age com `confirmar = true`.**
    ///
    /// - Reset em massa: regrava as settings que voltaram ao padrão; as que mudaram para
    ///   outro valor gravado são tratadas como pontuais.
    /// - Pontual: cada divergência vira escolha travada (o avaliador deixa de tocá-la) e
    ///   sai do estado esperado.
    ///
    /// Perfil de jogo que sumiu do driver (instalação limpa, apagado no NPI) não
    /// derruba o tratamento: suas settings saem do estado esperado
    /// ([`TratamentoDeriva::removidas`]) e o próximo plano recria o perfil se alguma
    /// regra ainda o exigir. Outros erros de um perfil de jogo ficam em
    /// [`TratamentoDeriva::erros`]; o perfil global sempre segue.
    pub fn tratar_deriva(
        &mut self,
        deriva: &Deriva,
        confirmar: bool,
    ) -> Resultado<TratamentoDeriva> {
        let mut t = TratamentoDeriva::default();
        if !confirmar {
            return Ok(t);
        }
        self.exigir_gravacao()?;
        let agora = agora_iso();
        let massa = matches!(deriva.classe, ClasseDeriva::ResetEmMassa(_));
        let (reaplicar, travar): (Vec<&Divergencia>, Vec<&Divergencia>) = deriva
            .divergencias
            .iter()
            .partition(|d| massa && d.voltou_ao_padrao);

        // Portão (T-02) só se houver o que regravar: travar uma mudança
        // manual mexe apenas no estado do app, não no driver.
        let licenca = if reaplicar.is_empty() {
            None
        } else {
            Some(self.licenca(&[])?)
        };
        let mut gravadas: Vec<&Divergencia> = Vec::new();
        for d in reaplicar {
            let Some(licenca) = &licenca else { break };
            let e = &d.esperado;
            match self.gravar_com(licenca, &e.perfil, e.id, e.valor) {
                Ok(()) => gravadas.push(d),
                Err(err) if matches!(e.perfil, ProfileRef::Named(_)) => {
                    if matches!(&err, ErroEngine::Drs(e) if perfil_ausente(e)) {
                        self.armazem.estado.remover_esperado(&e.perfil, e.id);
                        t.removidas.push(e.clone());
                    } else {
                        t.erros.push(format!(
                            "reaplicação de {} em “{}” falhou: {err}",
                            e.setting,
                            nome_perfil(&e.perfil)
                        ));
                    }
                }
                Err(err) => return Err(err),
            }
        }
        let reaplicar = gravadas;
        if let Some(licenca) = &licenca
            && !reaplicar.is_empty()
        {
            self.store.escrita(licenca).salvar()?;
        }
        let motivo_massa = match &deriva.classe {
            ClasseDeriva::ResetEmMassa(MotivoReset::VersaoDriver { antes, agora }) => {
                format!("driver mudou de {antes} para {agora}: settings voltaram ao padrão")
            }
            ClasseDeriva::ResetEmMassa(MotivoReset::MuitasSettings { revertidas, total }) => {
                format!("{revertidas} de {total} settings geridas voltaram ao padrão juntas")
            }
            _ => String::new(),
        };
        for d in reaplicar {
            let msg = |err: &ErroEngine| {
                format!(
                    "releitura de {} em “{}” falhou: {err}",
                    d.esperado.setting,
                    nome_perfil(&d.esperado.perfil)
                )
            };
            let lido = self.ler_u32(&d.esperado.perfil, d.esperado.id);
            let Some(lido) = isolar(&d.esperado.perfil, lido, &mut t.erros, msg)? else {
                continue;
            };
            if lido == Some(d.esperado.valor) {
                self.registrar(RegistroHistorico {
                    data: agora.clone(),
                    perfil: nome_perfil(&d.esperado.perfil),
                    setting: d.esperado.setting.clone(),
                    id: d.esperado.id,
                    antes: d.atual,
                    depois: Some(d.esperado.valor),
                    origem: OrigemRegistro::Reaplicacao {
                        origem: d.esperado.origem.clone(),
                    },
                    motivo: format!("reaplicada: {motivo_massa}"),
                });
                t.reaplicadas.push(d.esperado.clone());
            } else {
                t.falhas.push((d.esperado.clone(), lido));
            }
        }
        for d in travar {
            let trava = Travada {
                alvo: d.esperado.alvo.clone(),
                setting: d.esperado.setting.clone(),
                valor_usuario: d.atual,
                data: agora.clone(),
                motivo: "mudança manual detectada numa setting gerida pelo app".into(),
            };
            let e = &d.esperado;
            self.armazem.estado.remover_esperado(&e.perfil, e.id);
            if !self
                .armazem
                .estado
                .travadas
                .iter()
                .any(|x| mesmo_alvo(&x.alvo, &trava.alvo) && x.setting == trava.setting)
            {
                self.armazem.estado.travadas.push(trava.clone());
            }
            self.registrar(RegistroHistorico {
                data: agora.clone(),
                perfil: nome_perfil(&e.perfil),
                setting: e.setting.clone(),
                id: e.id,
                antes: Some(e.valor),
                depois: d.atual,
                origem: OrigemRegistro::Usuario,
                motivo: "mudança manual: travada como escolha do usuário".into(),
            });
            t.travadas.push(trava);
        }
        self.armazem.estado.driver_visto = Some(deriva.versao_driver);
        self.armazem.salvar()?;
        t.aplicado = true;
        Ok(t)
    }
}
