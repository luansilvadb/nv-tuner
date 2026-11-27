//! Aplicação das ações do usuário: validação do plano, escrita autorizada
//! e transição de cada destino.

use super::*;

impl<A: LeituraUsuario + EscritaUsuario, T: Transporte, P: Persistencia> Execucao<'_, A, T, P> {
    pub(super) fn validar_plano(&self, plano: &PlanoAgente) -> io::Result<()> {
        if plano.geracao_restauracao < self.estado.geracao_vista
            || plano.revisao < self.sessao().revisao
        {
            return Err(erro("plano obsoleto"));
        }
        if plano.validar().is_err()
            || plano
                .tarefa_jogo
                .as_ref()
                .is_some_and(|t| t.tarefa.revisao != plano.revisao)
        {
            return Err(erro("tarefa de jogo obsoleta ou inválida"));
        }
        if plano.processos_vigiados.len() > 4096
            || plano.processos_vigiados.iter().any(|nome| {
                nome.is_empty()
                    || nome.len() > 260
                    || nome.trim() != nome
                    || nome.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
                    || nome.chars().any(char::is_control)
            })
        {
            return Err(erro("lista de processos vigiados inválida"));
        }
        if let Some(t) = &plano.tarefa {
            let mut destinos = BTreeSet::new();
            if t.tarefa.revisao != plano.revisao || t.acoes.len() > 256 {
                return Err(erro("tarefa obsoleta ou inválida"));
            }
            for acao in &t.acoes {
                let (destino, operacao) = match acao {
                    AcaoUsuario::Aplicar { ajuste } => (ajuste.destino(), OperacaoUsuario::Aplicar),
                    AcaoUsuario::Restaurar { destino } => (destino.clone(), t.tarefa.operacao),
                    AcaoUsuario::Reverter { destino } => {
                        (destino.clone(), OperacaoUsuario::Reverter)
                    }
                    AcaoUsuario::RestaurarSnapshot
                        if t.acoes.len() == 1
                            && t.tarefa.operacao == OperacaoUsuario::Restaurar =>
                    {
                        continue;
                    }
                    AcaoUsuario::RestaurarSnapshot => {
                        return Err(erro("tarefa de snapshot inválida"));
                    }
                };
                if t.tarefa.operacao != operacao
                    || !destinos.insert(destino.clone())
                    || matches!(destino, SettingUsuario::Refresh { alvo } if alvo.is_empty() || alvo.len() > 1024 || alvo.chars().any(char::is_control))
                {
                    return Err(erro("destino ou operação inválida"));
                }
            }
        }
        Ok(())
    }

    pub(super) fn escrever_autorizado(
        &mut self,
        destino: &SettingUsuario,
        atual: &ValorLido,
        pedido: &ValorLido,
        restauracao: bool,
    ) -> Result<io::Result<()>, &'static str> {
        let extras: Vec<_> = self
            .vigiados
            .as_ref()
            .ok_or("lista do serviço indisponível")?
            .iter()
            .map(String::as_str)
            .collect();
        let mut autorizado = false;
        let mut bloqueio = None;
        let portao = &self.agente.portao;
        let base = &self.agente.base;
        let mut autorizar = || {
            if let Some(motivo) = bloqueio {
                return Err(erro(motivo));
            }
            match portao.verificar(base, &extras) {
                Ok(_) => {
                    autorizado = true;
                    Ok(())
                }
                Err(a) => {
                    let motivo = if a.lista_indisponivel {
                        "enumeração de processos indisponível"
                    } else {
                        "jogo ou anticheat em execução"
                    };
                    bloqueio = Some(motivo);
                    Err(erro(motivo))
                }
            }
        };
        let escrita =
            self.agente
                .armazem
                .escrever(destino, atual, pedido, restauracao, &mut autorizar);
        if !autorizado && let Some(motivo) = bloqueio {
            return Err(motivo);
        }
        Ok(escrita.and_then(|()| {
            autorizado
                .then_some(())
                .ok_or_else(|| erro("fachada não consultou o portão"))
        }))
    }

    pub(super) fn iniciar(&mut self, tarefa: &ReferenciaTarefa) -> io::Result<()> {
        if self
            .sessao()
            .trabalho
            .as_ref()
            .is_some_and(|t| t.tarefa == *tarefa)
        {
            return Ok(());
        }
        self.sessao_mut().trabalho = Some(Trabalho {
            tarefa: *tarefa,
            resultados: Vec::new(),
            pronto: false,
            confirmado: false,
            geracao_concluida: None,
            resultados_jogo: Vec::new(),
        });
        self.salvar()
    }

    pub(super) fn executar(
        &mut self,
        acao: AcaoUsuario,
        tarefa: &ReferenciaTarefa,
        geracao: u64,
    ) -> io::Result<()> {
        let (destino, operacao, ajuste) = match acao {
            AcaoUsuario::Aplicar { ajuste } => (ajuste.destino(), Operacao::Aplicar, Some(ajuste)),
            AcaoUsuario::Restaurar { destino } => (destino, Operacao::Retirar, None),
            AcaoUsuario::Reverter { destino } => (destino, Operacao::Reverter, None),
            AcaoUsuario::RestaurarSnapshot => return Ok(()), // Tratada antes das demais ações.
        };
        if !self.resultado_registrado(&destino) {
            let r = self.transicao(&destino, operacao, ajuste.as_ref(), Some(tarefa), geracao)?;
            self.registrar(self.agente.sessao, Some(tarefa), r);
            self.salvar()?;
        }
        Ok(())
    }

    pub(super) fn transicao(
        &mut self,
        destino: &SettingUsuario,
        operacao: Operacao,
        ajuste: Option<&nvt_ipc::AjusteUsuario>,
        tarefa: Option<&ReferenciaTarefa>,
        geracao: u64,
    ) -> io::Result<ResultadoUsuario> {
        use StatusMudanca::*;
        let indice = self.indice(destino);
        if indice.is_some_and(|i| self.estado.destinos[i].intencao.is_some()) {
            return Ok(resultado(
                destino,
                Pendente,
                Some("intenção anterior aguarda recuperação"),
            ));
        }

        let Ok(atual) = self.ler(destino) else {
            return Ok(resultado(destino, Pendente, Some("destino indisponível")));
        };
        if let Some(i) = indice {
            let d = &mut self.estado.destinos[i];
            let divergiu = d.esperado.as_ref().is_some_and(|v| !iguais(v, &atual));
            if operacao == Operacao::Snapshot
                && d.bloquear_snapshot_ate
                    .is_some_and(|limite| geracao <= limite)
                && !atingiu(&atual, &d.snapshot)
            {
                return Ok(resultado(
                    destino,
                    Travada,
                    Some("intenção interrompida com valor divergente"),
                ));
            }
            if !operacao.snapshot() && (d.travado || divergiu) {
                d.travado = true;
                self.salvar()?;
                return Ok(resultado(
                    destino,
                    Travada,
                    Some("alteração local preservada"),
                ));
            }
        }

        let pedido = if let Some(ajuste) = ajuste {
            let (antes, depois) = valores(ajuste, &atual);
            if !valido(destino, &antes) || !valido(destino, &depois) {
                return Ok(resultado(destino, Recusada, Some("valor fora do catálogo")));
            }
            if !iguais(&antes, &atual) && !iguais(&depois, &atual) {
                return Ok(resultado(
                    destino,
                    Recusada,
                    Some("precondição do plano mudou"),
                ));
            }
            depois
        } else {
            indice
                .and_then(|i| {
                    let d = &self.estado.destinos[i];
                    if operacao.snapshot() {
                        Some(d.snapshot.clone())
                    } else {
                        d.antes_gestao.clone()
                    }
                })
                .unwrap_or_else(|| atual.clone())
        };

        if atingiu(&atual, &pedido) {
            if operacao == Operacao::Reverter {
                let i = indice.unwrap_or_else(|| self.capturar(destino, atual, geracao));
                self.estado.destinos[i].travado = true;
            } else if let Some(i) = indice {
                if operacao != Operacao::Aplicar {
                    self.concluir(i, operacao, &atual, &atual);
                }
            }
            return Ok(resultado(destino, JaNoValor, None));
        }

        let i = indice.unwrap_or_else(|| self.capturar(destino, atual.clone(), geracao));
        self.estado.destinos[i].intencao = Some(Intencao {
            anterior: atual.clone(),
            pedido: pedido.clone(),
            operacao,
            geracao,
            rollback: None,
            sessao: self.agente.sessao,
            tarefa: tarefa.cloned(),
        });
        self.salvar()?;
        let escrita =
            match self.escrever_autorizado(destino, &atual, &pedido, operacao != Operacao::Aplicar)
            {
                Ok(escrita) => escrita,
                Err(motivo) => {
                    self.estado.destinos[i].intencao = None;
                    if indice.is_none() {
                        self.estado.destinos.remove(i);
                    }
                    self.salvar()?;
                    return Ok(resultado(destino, Pendente, Some(motivo)));
                }
            };
        let Ok(lido) = self.ler(destino) else {
            return Ok(resultado(
                destino,
                Pendente,
                Some("releitura indisponível; intenção mantida"),
            ));
        };
        if atingiu(&lido, &pedido) {
            return self.finalizar(i, Aplicada, None);
        }
        if iguais(&lido, &atual) {
            return self.finalizar(i, Falhou, Some("releitura encontrou o valor anterior"));
        }
        #[cfg(windows)]
        let compensacao_falhou = escrita
            .as_ref()
            .err()
            .is_some_and(crate::windows::refresh_divergente);
        #[cfg(not(windows))]
        let compensacao_falhou = false;
        if matches!(destino, SettingUsuario::Refresh { .. })
            && (escrita.is_ok() || compensacao_falhou)
        {
            self.estado.destinos[i]
                .intencao
                .as_mut()
                .expect("intenção persistida")
                .rollback = Some(lido);
            self.salvar()?;
            if compensacao_falhou {
                return Ok(resultado(
                    destino,
                    Pendente,
                    Some("refresh divergente; compensação pendente"),
                ));
            }
            return self.compensar(i);
        }
        self.finalizar(
            i,
            Travada,
            Some("releitura divergente; alteração local preservada"),
        )
    }
}
