//! Ciclo do núcleo: varredura, planejamento, aplicação, deriva, avisos e
//! temporizadores.

use super::*;

impl<'a, S: DriverStore> Nucleo<'a, S> {
    pub(super) fn renovar(&mut self) -> Resultado<()> {
        let store = (self.fabrica)()?;
        let armazem = match &self.dir {
            Some(d) => abrir_armazem(d, &self.cfg, &mut self.avisos_estado)?,
            None => self.engine.armazem().clone(),
        };
        // O motor classifica e reverte o plano DRS v1. Habilitar as regras
        // aceitas nesta cópia mantém essa classificação e a persistência do motor.
        let mut regras = self.regras_drs.clone();
        for regra in &mut regras.regras {
            if self.aceites_drs.contains(&regra.id) {
                regra.status = nvt_core::Status::Estavel;
            }
        }
        self.engine = Engine::com_regras(store, armazem, regras).com_portao(self.portao.clone());
        if self.dir.is_some() {
            self.servico = EstadoServico::carregar(self.dir.as_deref());
        }
        Ok(())
    }

    pub(super) fn salvar_servico(&self) {
        if !self.pode_gravar() {
            return;
        }
        if let Some(d) = &self.dir
            && let Err(e) = self.servico.salvar(d)
        {
            error!("falha ao gravar servico.json: {e}");
        }
    }

    pub(super) fn marcar_gravacao(&mut self, agora: Instant) {
        self.ultima_gravacao = Some(agora);
        self.assinatura_propria = (self.assinador)();
    }

    /// Extras atuais do portão compartilhado com o atualizador: os executáveis
    /// dos jogos detectados (os mesmos que as gravações DRS vigiam).
    pub fn portao_extras(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .jogos
            .iter()
            .map(|j| j.exe.clone())
            .filter(|e| !e.is_empty())
            .collect();
        v.sort();
        v.dedup();
        v
    }

    pub fn processos_vigiados(&self) -> std::collections::BTreeSet<String> {
        self.portao
            .vigiados(self.engine.base_jogos(), &[])
            .into_iter()
            .collect()
    }

    /// Varre os jogos instalados.
    /// Os exes encontrados passam a ser vigiados pelo portão de gravação.
    pub fn varrer(&mut self, agora: Instant) {
        self.jogos = (self.varredor)();
        self.portao
            .vigiar_jogos(self.jogos.iter().map(|j| j.exe.as_str()));
        self.proxima_varredura = agora + self.cfg.intervalo_varredura;
        info!(
            "varredura: {} jogo(s) com executável principal, {} em execução",
            self.jogos.len(),
            self.jogos.iter().filter(|j| j.em_execucao).count()
        );
    }

    /// Início do serviço: varredura e primeiro ciclo.
    pub fn iniciar(&mut self, agora: Instant) {
        self.varrer(agora);
        if self.restauracao_pendente() {
            info!("restauração pendente de antes do reinício: retomando");
            self.retomar_pendente(agora);
        }
        if self.servico.autonomia_pausada {
            info!("autonomia pausada (restauração anterior): só verificação");
        }
        self.ciclo(agora, "início");
    }

    /// Verifica a deriva e, com autonomia, trata-a e aplica o plano.
    pub fn ciclo(&mut self, agora: Instant, motivo: &str) {
        self.proxima_verificacao = agora + self.cfg.intervalo_verificacao;
        if let Err(e) = self.ciclo_interno(agora, motivo) {
            error!("ciclo ({motivo}) falhou: {e}");
        }
    }

    pub(super) fn ciclo_interno(&mut self, agora: Instant, motivo: &str) -> Resultado<()> {
        debug!("ciclo: {motivo}");
        self.renovar()?;
        if self.autonomia_ativa() {
            self.tratar_e_aplicar(agora, motivo)?;
        } else {
            self.tratar_deriva_pendente(agora, motivo, false)?;
        }
        Ok(())
    }

    /// Trata a deriva e aplica o plano. Com jogo aberto (portão do motor),
    /// nada é gravado: a ação fica pendente e o retorno é `None`. Com
    /// restauração pendente, só lê a deriva (a restauração vence).
    pub(super) fn tratar_e_aplicar(
        &mut self,
        agora: Instant,
        motivo: &str,
    ) -> Resultado<Option<(Diff, Aplicacao)>> {
        if self.restauracao_pendente() {
            self.tratar_deriva_pendente(agora, motivo, false)?;
            return Ok(None);
        }
        match self.tratar_deriva_pendente(agora, motivo, true) {
            Err(ErroEngine::Adiada(a)) => {
                self.adiar(AcaoPendente::TratarDeriva, a, agora);
                return Ok(None);
            }
            r => r?,
        }
        match self.aplicar_plano(agora) {
            Err(ErroEngine::Adiada(a)) => {
                self.adiar(AcaoPendente::AplicarPlano, a, agora);
                Ok(None)
            }
            Err(e) => Err(e),
            Ok(r) => {
                self.concluir_pendente(AcaoPendente::AplicarPlano);
                Ok(Some(r))
            }
        }
    }

    /// Verifica a deriva e, com `tratar`, trata-a (mudança manual vira escolha
    /// travada; reset em massa é reaplicado). Precisa vir **antes** de
    /// qualquer [`Nucleo::aplicar_plano`]: uma mudança manual ainda não vista
    /// pelo ciclo seria tratada como valor do app e revertida pelo plano novo.
    pub(super) fn tratar_deriva_pendente(
        &mut self,
        agora: Instant,
        motivo: &str,
        tratar: bool,
    ) -> Resultado<()> {
        let mut deriva = self.engine.verificar_deriva()?;
        // Um reset não deve reaplicar uma experimental cujo aceite deixou de
        // valer. Alterações manuais continuam no tratamento normal de deriva.
        if tratar && matches!(deriva.classe, ClasseDeriva::ResetEmMassa(_)) {
            deriva.divergencias.retain(|d| {
                !d.voltou_ao_padrao
                    || d.esperado.origem.regra().is_none_or(|id| {
                        !self
                            .regras_drs
                            .regras
                            .iter()
                            .any(|r| r.id == id && r.status == nvt_core::Status::Experimental)
                            || self.aceites_drs.contains(id)
                    })
            });
        }
        if deriva.classe == ClasseDeriva::Nenhuma {
            self.erros_deriva.clear();
            return Ok(());
        }
        info!(
            "deriva ({motivo}): {:?}, {} divergência(s)",
            deriva.classe,
            deriva.divergencias.len()
        );
        self.registrar_alertas(&deriva);
        if !tratar {
            return Ok(());
        }
        let t = self.engine.tratar_deriva(&deriva, true)?;
        self.registrar_erros_deriva(&t.erros);
        if !t.reaplicadas.is_empty() {
            self.marcar_gravacao(agora);
            // Reaplicadas: o alerta deixa de valer.
            for e in &t.reaplicadas {
                let p = perfil_ipc(&e.perfil);
                self.alertas
                    .retain(|a| !(a.perfil == p && a.setting == e.setting));
            }
        }
        info!(
            "deriva tratada: {} reaplicada(s), {} travada(s), {} falha(s)",
            t.reaplicadas.len(),
            t.travadas.len(),
            t.falhas.len()
        );
        Ok(())
    }

    /// Planeja e aplica. O motor captura o snapshot original antes da primeira gravação.
    pub(super) fn aplicar_plano(&mut self, agora: Instant) -> Resultado<(Diff, Aplicacao)> {
        let diff = self.planejar()?;
        if diff.vazio() {
            debug!("plano: nada a mudar ({} conforme(s))", diff.conformes.len());
            self.erros_aplicacao.clear();
            return Ok((diff, Aplicacao::default()));
        }
        let ap = self.engine.aplicar(&diff, true)?;
        self.marcar_gravacao(agora);
        let quando = agora_unix();
        self.erros_aplicacao = ap
            .erros
            .iter()
            .map(|e| {
                error!("erro isolado num perfil ao aplicar: {e}");
                Aviso::erro_perfil(e.clone(), false, quando)
            })
            .collect();
        for r in &diff.reverter {
            let ok = ap.revertidas.contains(&r.esperado);
            let acao = match r.acao {
                AcaoReversao::Gravar(_) => "valor original gravado",
                AcaoReversao::Remover { .. } => "removida do perfil",
                AcaoReversao::SoDeixarDeVigiar => "deixou de ser vigiada",
            };
            if ok {
                info!(
                    "saiu do plano: {} em {} ({acao})",
                    r.esperado.setting,
                    perfil_ipc(&r.esperado.perfil)
                );
            } else {
                warn!(
                    "saiu do plano, mas a reversão não foi confirmada: {} em {}",
                    r.esperado.setting,
                    perfil_ipc(&r.esperado.perfil)
                );
            }
        }
        info!(
            "aplicação: {} verificada(s), {} falha(s), {} perfil(is) criado(s){}",
            ap.verificadas.len(),
            ap.falhas.len(),
            ap.perfis_criados.len(),
            if ap.snapshot_capturado {
                "; snapshot original capturado"
            } else {
                ""
            }
        );
        for (m, lido) in &ap.falhas {
            warn!(
                "releitura divergente: {} em {} (esperado {}, lido {lido:?})",
                m.mudanca.setting,
                perfil_ipc(&m.perfil),
                m.mudanca.valor
            );
        }
        self.servico.ultima_aplicacao = Some(UltimaAplicacao {
            quando_unix: agora_unix(),
            mudancas: u32::try_from(ap.verificadas.len()).unwrap_or(u32::MAX),
            falhas: u32::try_from(ap.falhas.len()).unwrap_or(u32::MAX),
        });
        self.salvar_servico();
        Ok((diff, ap))
    }

    pub(super) fn registrar_alertas(&mut self, deriva: &Deriva) {
        let quando = agora_unix();
        for d in &deriva.divergencias {
            let def = self.def(&d.esperado.setting);
            let a = nvt_ipc::AlertaDeriva {
                perfil: perfil_ipc(&d.esperado.perfil),
                setting: d.esperado.setting.clone(),
                nome_legivel: def.map_or_else(|| d.esperado.setting.clone(), nomes::nome_setting),
                esperado: nomes::valor(def, Some(d.esperado.valor)).unwrap_or_default(),
                encontrado: nomes::valor(def, d.atual),
                detectado_unix: quando,
            };
            self.alertas
                .retain(|x| !(x.perfil == a.perfil && x.setting == a.setting));
            self.alertas.push(a);
        }
        let excesso = self.alertas.len().saturating_sub(ALERTAS_MAX);
        self.alertas.drain(..excesso);
    }

    /// Nome da setting `limite_fps` no catálogo (`SETTING_LIMITE_FPS` do núcleo).
    const LIMITE_FPS: &str = "limite_fps";

    /// F3-13: aplica [`nvt_core::desligar_limite_drs`] ao plano e reclassifica
    /// o `limite_fps` dos perfis afetados contra o driver (o motor planejou o
    /// limite do usuário; aqui ele vira `off`). Sem limite efetivo no jogo
    /// (arquivo falhou, adiado ou não aplicado), o limite do driver permanece.
    pub(super) fn desligar_limite_drs(
        &self,
        diff: &mut Diff,
        fatos: &[nvt_core::GameFacts],
    ) -> Resultado<()> {
        if self.limites_jogo.is_empty() {
            return Ok(());
        }
        let prefs = self.engine.estado().preferencias_efetivas();
        nvt_core::desligar_limite_drs(
            &mut diff.plano,
            fatos,
            &self.limites_jogo,
            &self.regras_drs.catalogo,
            &prefs,
        );
        for r in &diff.jogos {
            let Some(regra) = r
                .fatos
                .jogo_base
                .as_ref()
                .and_then(|b| self.limites_jogo.get(b))
            else {
                continue;
            };
            let alvo = Alvo::Jogo(r.perfil.clone());
            let Some(m) = diff
                .plano
                .mudancas(&alvo)
                .iter()
                .find(|m| m.setting == Self::LIMITE_FPS && m.origem.regra() == Some(regra.as_str()))
                .cloned()
            else {
                continue;
            };
            let perfil = ProfileRef::Named(r.perfil.clone());
            let mesmo = |x: &nvt_engine::MudancaPlanejada| x.alvo == alvo && x.mudanca.id == m.id;
            diff.mudancas.retain(|x| !mesmo(x));
            diff.conformes.retain(|x| !mesmo(x));
            diff.reverter.retain(|x| {
                !(x.esperado.id == m.id
                    && matches!(&x.esperado.perfil, ProfileRef::Named(n) if n.eq_ignore_ascii_case(&r.perfil)))
            });
            let existe = r.origem != nvt_engine::OrigemPerfil::ACriar;
            let atual = if existe {
                self.engine
                    .store()
                    .ler_setting(&perfil, m.id)?
                    .and_then(|s| s.value.as_u32())
            } else {
                None
            };
            let mut mudanca = m.clone();
            mudanca.valor_anterior = atual;
            let item = nvt_engine::MudancaPlanejada {
                alvo,
                perfil,
                mudanca,
            };
            if atual == Some(m.valor) {
                diff.conformes.push(item);
            } else {
                if !existe && !diff.criar.iter().any(|c| c.nome == r.perfil) {
                    diff.criar.push(nvt_engine::PerfilCriado {
                        nome: r.perfil.clone(),
                        exe: r.entrada.exe.clone(),
                    });
                }
                diff.mudancas.push(item);
            }
        }
        Ok(())
    }

    /// Planeja e guarda os jogos ignorados (registrando no log quando mudam).
    pub(super) fn planejar(&mut self) -> Resultado<Diff> {
        let mut diff = self.engine.planejar(&self.jogos)?;
        let fatos = diff
            .jogos
            .iter()
            .map(|j| j.fatos.clone())
            .collect::<Vec<_>>();
        diff.plano = nvt_core::avaliar_com_aceites(
            &diff.capacidades,
            &fatos,
            &self.engine.estado().preferencias_efetivas(),
            &self.regras_drs,
            &self.aceites_drs,
        );
        self.desligar_limite_drs(&mut diff, &fatos)?;
        let ignorados: Vec<nvt_ipc::JogoIgnorado> = diff
            .ignorados
            .iter()
            .map(|j| nvt_ipc::JogoIgnorado {
                nome: j.nome.clone(),
                exe: j.exe.clone(),
                motivo: j.motivo.clone(),
            })
            .collect();
        if ignorados != self.ignorados {
            for j in &ignorados {
                warn!("jogo ignorado pelo plano: {} ({})", j.nome, j.motivo);
            }
            self.ignorados_unix = agora_unix();
            self.ignorados = ignorados;
        }
        if diff.plano.avisos != self.avisos_plano {
            for a in &diff.plano.avisos {
                warn!("aviso do plano: {a:?}");
            }
            self.avisos_plano_unix = agora_unix();
            self.avisos_plano = diff.plano.avisos.clone();
        }
        Ok(diff)
    }

    pub(super) fn registrar_erros_deriva(&mut self, erros: &[String]) {
        let quando = agora_unix();
        self.erros_deriva = erros
            .iter()
            .map(|e| {
                error!("erro isolado num perfil ao tratar a deriva: {e}");
                Aviso::erro_perfil(e.clone(), true, quando)
            })
            .collect();
    }

    /// Avisos para a UI: estado, snapshot, erros do último ciclo e jogos
    /// ignorados, do mais grave para o menos grave.
    pub fn avisos(&self) -> Vec<Aviso> {
        let mut v = self.avisos_estado.clone();
        if let Some(motivo) = self.engine.armazem().snapshot_indisponivel()
            && !v
                .iter()
                .any(|a| a.codigo == nvt_ipc::codigo_aviso::SNAPSHOT_INDISPONIVEL)
        {
            v.push(Aviso::snapshot_indisponivel(motivo, agora_unix()));
        }
        v.extend(self.erros_pendente.iter().cloned());
        v.extend(self.erros_deriva.iter().cloned());
        v.extend(self.erros_aplicacao.iter().cloned());
        if !self.ignorados.is_empty() {
            v.push(Aviso::jogos_ignorados(&self.ignorados, self.ignorados_unix));
        }
        v.extend(
            self.avisos_plano
                .iter()
                .map(|a| aviso_do_plano(*a, self.avisos_plano_unix)),
        );
        nvt_ipc::ordenar_avisos(&mut v);
        v
    }

    pub(super) fn def(&self, setting: &str) -> Option<&DefSetting> {
        self.engine.regras().catalogo.get(setting)
    }

    // ── Vigia do DRS ───────────────────────────────────────────────────────

    /// A mudança no diretório do DRS é provavelmente nossa? (dentro da janela
    /// após a última gravação própria, ou assinatura igual à registrada logo
    /// depois dela).
    pub(super) fn mudanca_propria(&mut self, agora: Instant) -> bool {
        if self
            .ultima_gravacao
            .is_some_and(|t| agora.saturating_duration_since(t) < self.cfg.janela_propria)
        {
            return true;
        }
        match &self.assinatura_propria {
            Some(a) => (self.assinador)().as_ref() == Some(a),
            None => false,
        }
    }

    /// Notificação do diretório do DRS. Devolve `false` se foi ignorada
    /// (gravação própria).
    pub fn notificar_mudanca_drs(&mut self, agora: Instant) -> bool {
        if self.mudanca_propria(agora) {
            debug!("mudança no DRS ignorada (gravação do próprio serviço)");
            return false;
        }
        self.mudanca_pendente = Some(agora);
        true
    }

    /// Há uma mudança externa aguardando o debounce?
    #[cfg(test)]
    pub fn mudanca_pendente(&self) -> bool {
        self.mudanca_pendente.is_some()
    }

    /// Próxima consulta ao portão com ação pendente (`None` = sem pendência).
    #[cfg(test)]
    pub fn proxima_reavaliacao(&self) -> Option<Instant> {
        self.proxima_reavaliacao
    }

    /// Temporizadores: ação pendente (jogo aberto), debounce do DRS,
    /// verificação e varredura periódicas. Sem pendência, os processos só
    /// são consultados quando há o que gravar.
    pub fn tick(&mut self, agora: Instant) {
        if self.proxima_reavaliacao.is_some_and(|t| agora >= t) {
            self.reavaliar_pendente(agora);
        }
        if let Some(t) = self.mudanca_pendente
            && agora.saturating_duration_since(t) >= self.cfg.debounce
        {
            self.mudanca_pendente = None;
            self.ciclo(agora, "mudança no DRS");
        }
        if agora >= self.proxima_varredura {
            self.varrer(agora);
            self.ciclo(agora, "varredura periódica");
        }
        if agora >= self.proxima_verificacao {
            self.ciclo(agora, "verificação periódica");
        }
    }

    // ── IPC ────────────────────────────────────────────────────────────────
}
