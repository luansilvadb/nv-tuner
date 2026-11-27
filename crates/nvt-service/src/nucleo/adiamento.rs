//! Ações pendentes: adiamento, retomada, conclusão e falhas da restauração
//! e da aplicação.

use super::*;

impl<'a, S: DriverStore> Nucleo<'a, S> {
    /// Gravação adiada por jogo aberto, se houver.
    #[cfg(test)]
    pub fn pendente(&self) -> Option<&Pendente> {
        self.pendente.as_ref()
    }

    pub(super) fn restauracao_pendente(&self) -> bool {
        self.pendente
            .as_ref()
            .is_some_and(|p| p.acao == AcaoPendente::RestaurarOriginal)
    }

    /// Registra a ação adiada pelo portão. A mais recente substitui as
    /// anteriores, exceto `RestaurarOriginal`, que vence qualquer outra.
    pub(super) fn adiar(&mut self, acao: AcaoPendente, adiamento: Adiamento, agora: Instant) {
        let acao = if self.restauracao_pendente() {
            AcaoPendente::RestaurarOriginal
        } else {
            acao
        };
        if self.pendente.as_ref().map(|p| p.acao) != Some(acao) {
            info!(
                "gravação adiada ({acao:?}): {adiamento}; reavaliação a cada {} s",
                self.cfg.intervalo_adiamento.as_secs()
            );
        }
        // Mesma ação: as falhas anteriores continuam contando.
        let tentativas = self
            .pendente
            .as_ref()
            .filter(|p| p.acao == acao)
            .map_or(0, |p| p.tentativas);
        if acao == AcaoPendente::RestaurarOriginal
            && self.servico.restauracao_pendente_desde_unix.is_none()
        {
            self.servico.restauracao_pendente_desde_unix = Some(adiamento.desde_unix);
            self.salvar_servico();
        }
        self.pendente = Some(Pendente {
            acao,
            adiamento,
            tentativas,
            ultimo_erro: None,
        });
        self.proxima_reavaliacao = Some(agora + self.cfg.intervalo_adiamento);
    }

    /// A gravação de `acao` foi feita (ou não havia o que gravar): a pendência
    /// equivalente deixa de valer. `RestaurarOriginal` pendente só sai quando
    /// a própria restauração roda.
    pub(super) fn concluir_pendente(&mut self, acao: AcaoPendente) {
        let sai = match self.pendente.as_ref().map(|p| p.acao) {
            None => false,
            Some(AcaoPendente::RestaurarOriginal) => acao == AcaoPendente::RestaurarOriginal,
            Some(_) => true,
        };
        if sai {
            self.pendente = None;
            self.proxima_reavaliacao = None;
        }
        if acao == AcaoPendente::RestaurarOriginal {
            self.esquecer_restauracao_persistida();
        }
    }

    /// Tira de `servico.json` a restauração pendente (feita ou descartada).
    pub(super) fn esquecer_restauracao_persistida(&mut self) {
        if self
            .servico
            .restauracao_pendente_desde_unix
            .take()
            .is_some()
        {
            self.salvar_servico();
        }
    }

    /// `apply` explícito do usuário (CLI sem o serviço no ar): descarta a
    /// restauração pendente, inclusive a persistida em `servico.json`, para
    /// que a aplicação pedida grave uma vez só. Retorna se havia alguma.
    pub fn descartar_restauracao_pendente(&mut self) -> bool {
        let havia =
            self.restauracao_pendente() || self.servico.restauracao_pendente_desde_unix.is_some();
        if self.restauracao_pendente() {
            self.pendente = None;
            self.proxima_reavaliacao = None;
        }
        self.esquecer_restauracao_persistida();
        if havia {
            info!("restauração pendente descartada: o usuário pediu para aplicar");
        }
        havia
    }

    /// Adiamento atual no formato do protocolo. Com a lista de processos
    /// indisponível, vai o sinal próprio, nunca um nome de processo. Só há
    /// adiamento quando o portão de fato adiou ([`Pendente::adiamento_real`]).
    pub(super) fn adiamento_ipc(&self) -> Option<nvt_ipc::Adiamento> {
        let p = self.pendente.as_ref().filter(|p| p.adiamento_real())?;
        Some(nvt_ipc::Adiamento {
            processos: if p.adiamento.lista_indisponivel {
                Vec::new()
            } else {
                p.adiamento.processos.clone()
            },
            desde_unix: p.adiamento.desde_unix,
            acao: p.acao.ipc(),
            lista_indisponivel: p.adiamento.lista_indisponivel,
        })
    }

    /// Resposta de um pedido aceito: [`Resposta::Adiada`] se a gravação
    /// ficou pendente, senão [`Resposta::Ok`].
    pub(super) fn ok_ou_adiada(&self, adiada: bool) -> Resposta {
        match self.adiamento_ipc().filter(|_| adiada) {
            Some(adiamento) => Resposta::Adiada { adiamento },
            None => Resposta::Ok,
        }
    }

    /// Retoma a ação pendente carregada do estado (restauração adiada antes
    /// de o serviço parar), se o portão liberar.
    pub fn retomar_pendente(&mut self, agora: Instant) {
        if self.pendente.is_some() {
            self.reavaliar_pendente(agora);
        }
    }

    /// Com ação pendente, consulta o portão; livre, executa a ação. A
    /// pendência só sai depois que a ação der certo (ou de
    /// [`TENTATIVAS_PENDENTE`] falhas seguidas).
    pub(super) fn reavaliar_pendente(&mut self, agora: Instant) {
        let Some(acao) = self.pendente.as_ref().map(|p| p.acao) else {
            self.proxima_reavaliacao = None;
            return;
        };
        if let Err(a) = self.engine.pode_gravar() {
            if let Some(p) = &mut self.pendente {
                p.adiamento = a;
            }
            self.proxima_reavaliacao = Some(agora + self.cfg.intervalo_adiamento);
            return;
        }
        // Sem reavaliação marcada: se a ação for adiada de novo, `adiar`
        // marca outra; se não, ela terminou.
        self.proxima_reavaliacao = None;
        info!("portão livre: executando a ação pendente ({acao:?})");
        let r = match acao {
            AcaoPendente::RestaurarOriginal => self.executar_restauracao(agora).map(|_| ()),
            AcaoPendente::AplicarPlano | AcaoPendente::TratarDeriva => {
                self.proxima_verificacao = agora + self.cfg.intervalo_verificacao;
                self.ciclo_interno(agora, "jogo fechado (ação pendente)")
            }
        };
        match r {
            Ok(()) => {
                if self.proxima_reavaliacao.is_none() {
                    self.pendente = None;
                    self.erros_pendente.clear();
                }
            }
            Err(ErroEngine::Adiada(a)) => self.adiar(acao, a, agora),
            Err(e) => self.falha_pendente(acao, &e, agora),
        }
    }

    /// A ação pendente falhou por erro que não é adiamento: tenta de novo no
    /// mesmo intervalo; esgotadas as tentativas, avisa e descarta.
    pub(super) fn falha_pendente(&mut self, acao: AcaoPendente, e: &ErroEngine, agora: Instant) {
        let Some(p) = &mut self.pendente else {
            return;
        };
        p.tentativas += 1;
        p.ultimo_erro = Some(e.to_string());
        let n = p.tentativas;
        if n < TENTATIVAS_PENDENTE {
            warn!(
                "ação pendente ({acao:?}) falhou ({n} de {TENTATIVAS_PENDENTE}): {e}; nova tentativa em {} s",
                self.cfg.intervalo_adiamento.as_secs()
            );
            self.proxima_reavaliacao = Some(agora + self.cfg.intervalo_adiamento);
            return;
        }
        error!("ação pendente ({acao:?}) descartada após {n} falha(s): {e}");
        self.pendente = None;
        self.proxima_reavaliacao = None;
        if acao == AcaoPendente::RestaurarOriginal {
            self.esquecer_restauracao_persistida();
        }
        self.erros_pendente = vec![Aviso::acao_pendente_descartada(
            acao.ipc(),
            n,
            e.to_string(),
            agora_unix(),
        )];
    }
}
