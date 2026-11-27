//! Leitura do estado da sessão: saúde, escopo, fatos, gestão e deriva
//! relatados ao serviço.

use super::*;

impl<A: LeituraUsuario + EscritaUsuario, T: Transporte, P: Persistencia> Execucao<'_, A, T, P> {
    pub(crate) fn sessao(&self) -> &crate::estado::Sessao {
        &self.estado.sessoes[&self.agente.sessao]
    }

    pub(super) fn sessao_mut(&mut self) -> &mut crate::estado::Sessao {
        self.estado
            .sessoes
            .get_mut(&self.agente.sessao)
            .expect("sessão inicializada")
    }

    pub(crate) fn salvar(&self) -> io::Result<()> {
        self.p.salvar(&self.estado)
    }

    /// Leituras de saúde vencidas, sob o portão com a lista atual do serviço.
    #[cfg(windows)]
    pub(super) fn saude(&mut self, agora: Instant) -> io::Result<()> {
        let Some(agenda) = self.agente.saude.as_mut() else {
            return Ok(());
        };
        let portao = crate::saude::PortaoAgente {
            portao: &self.agente.portao,
            base: &self.agente.base,
            extras: self
                .vigiados
                .as_ref()
                .map(|v| v.iter().map(String::as_str).collect()),
        };
        let agora_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if agenda.passo(agora, agora_unix, &portao, &mut self.estado.saude) {
            self.salvar()?;
        }
        Ok(())
    }

    #[cfg(not(windows))]
    pub(super) fn saude(&mut self, _: Instant) -> io::Result<()> {
        Ok(())
    }

    #[cfg(windows)]
    pub(super) fn saude_pendente(&self) -> Option<nvt_ipc::SaudeSessao> {
        self.agente.saude.as_ref()?.pendente.clone()
    }

    #[cfg(not(windows))]
    pub(super) fn saude_pendente(&self) -> Option<nvt_ipc::SaudeSessao> {
        None
    }

    pub(super) fn saude_confirmada(&mut self) {
        #[cfg(windows)]
        if let Some(agenda) = self.agente.saude.as_mut() {
            agenda.pendente = None;
        }
    }

    pub(super) fn escopo(&self, destino: &SettingUsuario) -> Option<u32> {
        matches!(destino, SettingUsuario::Refresh { .. }).then_some(self.agente.sessao)
    }

    pub(super) fn indice(&self, destino: &SettingUsuario) -> Option<usize> {
        self.estado
            .destinos
            .iter()
            .position(|d| d.destino == *destino && d.sessao == self.escopo(destino))
    }

    pub(super) fn acessivel(&self, d: &Destino) -> bool {
        d.sessao.is_none() || d.sessao == Some(self.agente.sessao)
    }

    pub(super) fn recuperar_escopo_tela(
        &mut self,
        tela: &Fato<nvt_ipc::FatoTela>,
    ) -> io::Result<()> {
        let (Some(sessoes), Fato::Conhecido(tela)) = (&self.agente.sessoes_existentes, tela) else {
            return Ok(());
        };
        let destino = SettingUsuario::Refresh {
            alvo: tela.alvo.clone(),
        };
        if self.indice(&destino).is_some() {
            return Ok(());
        }
        let mut orfaos =
            self.estado.destinos.iter_mut().filter(|d| {
                d.destino == destino && d.sessao.is_some_and(|s| !sessoes.contains(&s))
            });
        if let Some(d) = orfaos.next().filter(|_| orfaos.next().is_none()) {
            d.sessao = Some(self.agente.sessao);
            self.salvar()?;
        }
        Ok(())
    }

    pub(super) fn ler(&self, destino: &SettingUsuario) -> io::Result<ValorLido> {
        let valor = self.agente.armazem.ler(destino)?;
        if !valido(destino, &valor) {
            return Err(erro("leitura incompatível com o destino"));
        }
        Ok(valor)
    }

    pub(super) fn fato<V>(
        &self,
        destino: SettingUsuario,
        extrair: impl FnOnce(ValorUsuario) -> Option<V>,
    ) -> Fato<V> {
        let Ok(atual) = self.ler(&destino) else {
            return Fato::Desconhecido;
        };
        if atual.valor == ValorUsuario::Ausente {
            return Fato::Ausente;
        }
        extrair(atual.valor).map_or(Fato::Desconhecido, Fato::Conhecido)
    }

    pub(super) fn fatos(&self, tela: Fato<nvt_ipc::FatoTela>) -> FatosUsuario {
        use SettingUsuario::*;
        let armazem = &self.agente.armazem;
        FatosUsuario {
            tela,
            mouse: self.fato(MouseAceleracao, |v| match v {
                ValorUsuario::Spi(t) => Some(mouse(t)),
                _ => None,
            }),
            swap_effect_upgrade: self.fato(DxSwapEffectUpgrade, |v| match v {
                ValorUsuario::Token(t) => Some(t),
                _ => None,
            }),
            captura_historica: self.fato(GameDvrCapturaHistorica, |v| match v {
                ValorUsuario::Dword(t) => Some(t),
                _ => None,
            }),
            diagnosticos: armazem.diagnosticos(),
            gestao: self.gestao(),
            config_jogo: self.fatos_jogo(),
        }
    }

    /// O serviço reidrata dessa lista a gestão perdida em reinício ou nova sessão.
    pub(super) fn gestao(&self) -> Vec<nvt_ipc::GestaoAgente> {
        self.estado
            .destinos
            .iter()
            .filter(|d| self.acessivel(d) && (d.travado || d.antes_gestao.is_some()))
            .map(|d| nvt_ipc::GestaoAgente {
                destino: d.destino.clone(),
                valores: d
                    .antes_gestao
                    .clone()
                    .zip(d.esperado.clone())
                    .and_then(|(antes, esperado)| mudanca(&d.destino, antes, esperado)),
                travado: d.travado,
            })
            .take(64)
            .collect()
    }

    pub(super) fn deriva(&mut self) -> io::Result<()> {
        let mut mudou = false;
        for i in 0..self.estado.destinos.len() {
            let d = &self.estado.destinos[i];
            if !self.acessivel(d) || d.travado || d.intencao.is_some() {
                continue;
            }
            if let (Some(esperado), Ok(atual)) = (&d.esperado, self.ler(&d.destino)) {
                if !iguais(&atual, esperado) {
                    self.estado.destinos[i].travado = true;
                    mudou = true;
                }
            }
        }
        if mudou {
            self.salvar()?;
        }
        Ok(())
    }
}
