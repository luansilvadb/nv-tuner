//! Despacho das requisições da sessão e consulta do agente, com reversões.

use super::*;

impl Coordenador {
    pub(crate) fn tratar(
        &mut self,
        identidade: &IdentidadeCliente,
        req: &Requisicao,
        agora: Instant,
        unix: u64,
    ) -> Option<Resposta> {
        let resultado = match req {
            Requisicao::AgenteConsultar {
                fatos,
                ultima_tarefa,
                geracao_restaurada,
            } => req
                .validar()
                .and_then(|()| {
                    self.consultar(
                        identidade,
                        fatos,
                        ultima_tarefa.as_ref(),
                        *geracao_restaurada,
                        agora,
                    )
                })
                .map_err(invalida),
            Requisicao::AgenteRelatar {
                id_tarefa,
                operacao,
                revisao,
                resultados,
                resultados_jogo,
                saude,
            } => req.validar().map_err(invalida).and_then(|()| {
                let saude_recebida = saude
                    .as_ref()
                    .is_some_and(|s| self.receber_saude(identidade, s, agora));
                let r = self.relatar(
                    identidade,
                    ReferenciaTarefa {
                        id_tarefa: *id_tarefa,
                        operacao: *operacao,
                        revisao: *revisao,
                    },
                    resultados,
                    resultados_jogo,
                    agora,
                    unix,
                );
                // Relato só de saúde (sem resultados): aceito mesmo sem tarefa em voo.
                match r {
                    Err(_)
                        if saude_recebida
                            && resultados.is_empty()
                            && resultados_jogo.is_empty() =>
                    {
                        Ok(Resposta::Ok)
                    }
                    r => r,
                }
            }),
            Requisicao::ReverterConfigJogo { destino } => req
                .validar()
                .and_then(|()| self.reverter_jogo(identidade, destino))
                .map_err(invalida),
            Requisicao::DefinirAceiteExperimental { regra, aceito } => {
                self.definir_aceite(identidade, regra, *aceito)
            }
            Requisicao::ReverterAjusteUsuario { destino, .. } => req
                .validar()
                .and_then(|()| self.reverter(identidade, destino))
                .map_err(invalida),
            Requisicao::MarcarAtualizacaoVista { versao } => req
                .validar()
                .map_err(invalida)
                .and_then(|()| self.marcar_atualizacao_vista(identidade, versao)),
            _ => return None,
        };
        Some(resultado.unwrap_or_else(|(codigo, mensagem)| Resposta::erro(codigo, mensagem)))
    }

    pub(super) fn consultar(
        &mut self,
        identidade: &IdentidadeCliente,
        fatos: &nvt_ipc::FatosUsuario,
        ultima: Option<&EstadoTarefa>,
        restaurada: u64,
        agora: Instant,
    ) -> Result<Resposta, String> {
        if restaurada > self.geracao {
            return Err("geração de restauração desconhecida".into());
        }
        let geracao = self.geracao();
        let aceites = self.dados.por_sid.get(&identidade.sid).map(|d| &d.aceites);
        let vazio = BTreeSet::new();
        let s = self
            .sessoes
            .entry(identidade.clone())
            .or_insert_with(|| Sessao {
                revisao: self.serial,
                ..Sessao::default()
            });
        // Uma releitura pode já refletir a escrita que o agente ainda vai relatar.
        if ultima.is_some_and(|ultima| {
            ultima.progresso == ProgressoTarefa::RelatoPendente
                && (s.em_voo.as_ref().is_some_and(|t| t.tarefa == ultima.tarefa)
                    || s.em_voo_jogo
                        .as_ref()
                        .is_some_and(|t| t.tarefa == ultima.tarefa))
        }) {
            s.contato = Some(agora);
            s.diagnosticos.clone_from(&fatos.diagnosticos);
            return Ok(Resposta::PlanoAgente(PlanoAgente {
                revisao: s.revisao,
                geracao_restauracao: geracao,
                tarefa: None,
                processos_vigiados: self.processos.clone(),
                tarefa_jogo: None,
            }));
        }
        if restaurada > s.restaurada
            && s.snapshot_ofertado == Some(restaurada)
            && s.snapshot_confirmado != Some(restaurada)
        {
            return Err("restauração do snapshot ainda não confirmada".into());
        }
        let mut fatos_convertidos = fatos_core(fatos, self.windows_build);
        if s.fatos.as_ref() != Some(&fatos_convertidos) || restaurada != s.restaurada {
            s.invalidar();
        }
        if restaurada > s.restaurada {
            s.gestao.clear();
            s.travas.clear();
            s.reversoes.clear();
            s.gestao_jogo.clear();
            s.travas_jogo.clear();
            s.reversoes_jogo.clear();
            s.falhas_jogo.clear();
        }
        if reidratar(s, &fatos.gestao) {
            s.invalidar();
        }
        let catalogo_jogo = &self.catalogo_jogo;
        let novos_destinos = destinos_jogo(&fatos.config_jogo, catalogo_jogo);
        if s.fatos_jogo != fatos.config_jogo || s.destinos_jogo != novos_destinos {
            s.em_voo_jogo = None;
        }
        if reidratar_jogo(s, &fatos.config_jogo, catalogo_jogo) {
            s.em_voo_jogo = None;
        }
        s.fatos_jogo.clone_from(&fatos.config_jogo);
        s.destinos_jogo = novos_destinos;
        s.refresh_hz = fatos_convertidos.tela.as_ref().and_then(|t| {
            (t.atual.denominador > 0).then(|| t.atual.numerador / t.atual.denominador)
        });
        s.restaurada = restaurada;
        s.contato = Some(agora);
        s.diagnosticos.clone_from(&fatos.diagnosticos);
        s.fatos = Some(fatos_convertidos.clone());
        preencher_gestao(&mut fatos_convertidos, s);
        let plano = usuario::avaliar(
            &fatos_convertidos,
            Qualidade::default(),
            &self.regras,
            aceites.unwrap_or(&vazio),
        );

        let desejada = if SETTINGS
            .iter()
            .all(|&setting| fatos_convertidos.valor_atual(setting).is_none())
        {
            None
        } else if restaurada < self.geracao {
            s.snapshot_ofertado = Some(geracao);
            s.snapshot_confirmado = None;
            Some((
                OperacaoUsuario::Restaurar,
                vec![AcaoUsuario::RestaurarSnapshot],
            ))
        } else {
            planejar(s, &fatos_convertidos, &plano, self.pausado)
        };
        let tarefa = match desejada {
            Some((operacao, acoes)) => {
                if !s
                    .em_voo
                    .as_ref()
                    .is_some_and(|t| t.tarefa.operacao == operacao && t.acoes == acoes)
                {
                    s.invalidar();
                    self.serial += 1;
                    s.em_voo = Some(TarefaUsuario {
                        tarefa: ReferenciaTarefa {
                            id_tarefa: self.serial,
                            operacao,
                            revisao: s.revisao,
                        },
                        acoes,
                    });
                }
                s.em_voo.clone()
            }
            None => {
                if s.em_voo.is_some() {
                    s.invalidar();
                }
                None
            }
        };

        let plano_jogo = avaliar_config_jogo(
            &s.fatos_jogo_core(),
            &self.prefs,
            s.refresh_hz,
            catalogo_jogo,
            aceites.unwrap_or(&vazio),
        );
        // Restauração por geração cobre a config de jogo (inclusive pausado).
        let desejada_jogo = if restaurada < self.geracao {
            restaurar_jogo(s)
        } else {
            planejar_jogo(s, &plano_jogo, self.pausado)
        };
        let tarefa_jogo = match desejada_jogo {
            Some((operacao, acoes)) => {
                if !s
                    .em_voo_jogo
                    .as_ref()
                    .is_some_and(|t| t.tarefa.operacao == operacao && t.acoes == acoes)
                {
                    self.serial += 1;
                    s.em_voo_jogo = Some(TarefaJogo {
                        tarefa: ReferenciaTarefa {
                            id_tarefa: self.serial,
                            operacao,
                            revisao: s.revisao,
                        },
                        acoes,
                    });
                }
                s.em_voo_jogo.clone()
            }
            None => {
                s.em_voo_jogo = None;
                None
            }
        };
        Ok(Resposta::PlanoAgente(PlanoAgente {
            revisao: s.revisao,
            geracao_restauracao: geracao,
            tarefa,
            processos_vigiados: self.processos.clone(),
            tarefa_jogo,
        }))
    }

    pub(super) fn reverter(
        &mut self,
        identidade: &IdentidadeCliente,
        destino: &SettingUsuario,
    ) -> Result<Resposta, String> {
        let s = self
            .sessoes
            .get_mut(identidade)
            .filter(|s| s.gestao.contains_key(destino))
            .ok_or("ajuste sem valor anterior conhecido nesta sessão")?;
        if s.reversoes.insert(destino.clone()) {
            s.travas.insert(destino.clone());
            s.invalidar();
        }
        Ok(Resposta::Ok)
    }

    pub(super) fn reverter_jogo(
        &mut self,
        identidade: &IdentidadeCliente,
        destino: &DestinoJogoIpc,
    ) -> Result<Resposta, String> {
        let destino =
            destino_core(destino, &self.catalogo_jogo).ok_or("destino de jogo fora do catálogo")?;
        let s = self
            .sessoes
            .get_mut(identidade)
            .filter(|s| s.gestao_jogo.contains_key(&destino))
            .ok_or("ajuste de jogo sem valor anterior conhecido nesta sessão")?;
        if s.reversoes_jogo.insert(destino.clone()) {
            s.travas_jogo.insert(destino);
            s.invalidar();
        }
        Ok(Resposta::Ok)
    }
}
