//! Relatos do agente, projeção e resumo do sistema para a sessão.

use super::*;

impl Coordenador {
    pub(super) fn relatar(
        &mut self,
        identidade: &IdentidadeCliente,
        referencia: ReferenciaTarefa,
        resultados: &[ResultadoUsuario],
        resultados_jogo: &[ResultadoJogo],
        agora: Instant,
        unix: u64,
    ) -> Result<Resposta, Erro> {
        let desconhecida = || invalida("tarefa desconhecida ou revisão obsoleta");
        if let Some(anterior) = self
            .dados
            .por_sid
            .get(&identidade.sid)
            .and_then(|d| d.recibos.get(&identidade.sessao))
            .and_then(|recibos| recibos.iter().find(|r| r.tarefa == referencia))
        {
            if anterior.resultados != resultados || anterior.resultados_jogo != resultados_jogo {
                return Err(invalida("relato repetido com resultados diferentes"));
            }
            if let Some(s) = self.sessoes.get_mut(identidade) {
                s.contato = Some(agora);
            }
            return Ok(Resposta::Ok);
        }
        let s = self.sessoes.get_mut(identidade).ok_or_else(desconhecida)?;
        if s.em_voo_jogo
            .as_ref()
            .is_some_and(|t| t.tarefa == referencia && s.revisao == referencia.revisao)
        {
            if !resultados.is_empty() {
                return Err(invalida("tarefa de jogo com resultados do Windows"));
            }
            let catalogo = &self.catalogo_jogo;
            let destinos =
                validar_resultados_jogo(s, resultados_jogo, catalogo).map_err(invalida)?;
            let serial = &mut self.serial;
            transacao(&mut self.dados, &*self.armazenamento, |dados| {
                let sid = dados.por_sid.entry(identidade.sid.clone()).or_default();
                let recibos = sid.recibos.entry(identidade.sessao).or_default();
                recibos.push_front(Recibo {
                    tarefa: referencia,
                    resultados: vec![],
                    resultados_jogo: resultados_jogo.to_vec(),
                });
                recibos.truncate(RECIBOS_POR_SESSAO);
                for resultado in resultados_jogo {
                    *serial += 1;
                    sid.historico.push_front(entrada_jogo(
                        *serial,
                        unix,
                        referencia.operacao,
                        resultado,
                    ));
                }
                sid.historico.truncate(HISTORICO_POR_SID);
            })?;
            let tarefa = s.em_voo_jogo.take().expect("tarefa de jogo conferida");
            for (resultado, destino) in resultados_jogo.iter().zip(destinos) {
                registrar_jogo(s, &tarefa, resultado, destino);
            }
            s.contato = Some(agora);
            return Ok(Resposta::Ok);
        }
        if !resultados_jogo.is_empty() {
            return Err(desconhecida());
        }
        let tarefa = s
            .em_voo
            .as_ref()
            .filter(|t| t.tarefa == referencia && s.revisao == referencia.revisao)
            .ok_or_else(desconhecida)?;
        validar_resultados(s, tarefa, resultados).map_err(invalida)?;
        let snapshot_concluido = tarefa.acoes == [AcaoUsuario::RestaurarSnapshot]
            && resultados
                .iter()
                .all(|r| matches!(r.status, StatusMudanca::Aplicada | StatusMudanca::JaNoValor));

        let serial = &mut self.serial;
        transacao(&mut self.dados, &*self.armazenamento, |dados| {
            let sid = dados.por_sid.entry(identidade.sid.clone()).or_default();
            let recibos = sid.recibos.entry(identidade.sessao).or_default();
            recibos.push_front(Recibo {
                tarefa: referencia,
                resultados: resultados.to_vec(),
                resultados_jogo: vec![],
            });
            recibos.truncate(RECIBOS_POR_SESSAO);
            for resultado in resultados {
                *serial += 1;
                sid.historico
                    .push_front(entrada(*serial, unix, referencia.operacao, resultado));
            }
            sid.historico.truncate(HISTORICO_POR_SID);
        })?;
        if snapshot_concluido {
            s.snapshot_confirmado = s.snapshot_ofertado;
        }

        for resultado in resultados {
            if referencia.operacao == OperacaoUsuario::Aplicar
                && resultado.status == StatusMudanca::Aplicada
                && let Some(ajuste) = &resultado.mudanca
            {
                let (antes, depois) = valores(ajuste);
                let gestao = s
                    .gestao
                    .entry(resultado.destino.clone())
                    .or_insert_with(|| GestaoUsuario {
                        antes,
                        esperado: depois.clone(),
                        alvo_tela: alvo(&resultado.destino).map(str::to_owned),
                    });
                gestao.esperado = depois;
            }
            if resultado.status == StatusMudanca::Travada {
                s.travas.insert(resultado.destino.clone());
            }
            if referencia.operacao != OperacaoUsuario::Aplicar
                && matches!(
                    resultado.status,
                    StatusMudanca::Aplicada | StatusMudanca::JaNoValor | StatusMudanca::Travada
                )
            {
                s.gestao.remove(&resultado.destino);
                s.reversoes.remove(&resultado.destino);
            }
        }
        // Uma nova consulta deve trazer a releitura antes de outra aplicação.
        s.fatos = None;
        s.contato = Some(agora);
        s.em_voo = None;
        Ok(Resposta::Ok)
    }

    pub(crate) fn projetar(
        &self,
        identidade: &IdentidadeCliente,
        resposta: &mut Resposta,
        limite: usize,
        agora: Instant,
    ) {
        let s = self.sessoes.get(identidade);
        let presente = s.is_some_and(|s| s.presente(agora));
        match resposta {
            Resposta::Estado(estado) => {
                estado.agente = EstadoAgente {
                    presente,
                    restauracao_pendente: self.geracao > s.map_or(0, |s| s.restaurada),
                };
                estado.atualizacao.nao_vista = self.nao_vista(&identidade.sid, &estado.atualizacao);
                if let Some(s) = s.filter(|_| presente) {
                    estado.diagnosticos.extend(s.diagnosticos.clone());
                }
            }
            Resposta::Plano(plano) => {
                plano.sistema = self.resumo(identidade, s.filter(|_| presente));
                let mut config = self.config_jogo_ui(identidade, agora);
                for jogo in &mut plano.jogos {
                    if let Some(ui) = jogo.competitivo.as_ref().and_then(|id| config.remove(id)) {
                        jogo.config_jogo = Some(ui);
                    }
                }
            }
            Resposta::Historico { entradas } => {
                if let Some(dados) = self.dados.por_sid.get(&identidade.sid) {
                    entradas.extend(dados.historico.iter().take(limite).cloned());
                    entradas.sort_by_key(|e| std::cmp::Reverse((e.quando_unix, e.id)));
                    entradas.truncate(limite);
                }
            }
            _ => {}
        }
    }

    pub(super) fn resumo(
        &self,
        identidade: &IdentidadeCliente,
        sessao: Option<&Sessao>,
    ) -> ResumoSistema {
        let mut resumo = ResumoSistema::default();
        let aceites = self.dados.por_sid.get(&identidade.sid).map(|d| &d.aceites);
        let vazio = BTreeSet::new();
        let fatos = sessao.and_then(|s| {
            let mut fatos = s.fatos.clone()?;
            preencher_gestao(&mut fatos, s);
            Some(fatos)
        });
        let plano = fatos.as_ref().map(|fatos| {
            usuario::avaliar(
                fatos,
                Qualidade::default(),
                &self.regras,
                aceites.unwrap_or(&vazio),
            )
        });
        for meta in self
            .metadados
            .values()
            .filter(|r| r.status == Status::Experimental)
        {
            let estado = if aceites.is_some_and(|a| a.contains(&meta.id)) {
                EstadoExperimental::Aceita
            } else if plano
                .as_ref()
                .is_some_and(|p| p.resumo.estado_experimental.contains_key(&meta.id))
            {
                EstadoExperimental::Aceitavel
            } else {
                continue;
            };
            resumo.sugestoes.push(SugestaoExperimental {
                regra: meta.id.clone(),
                descricao: meta.descricao.clone(),
                camada: Camada::Usuario,
                impacto_visual: impacto(meta.impacto_visual),
                estado,
            });
        }
        let plano_jogo = sessao.map(|s| self.plano_jogo(&identidade.sid, s));
        for regra in self
            .catalogo_jogo
            .regras()
            .iter()
            .filter(|r| r.status == Status::Experimental)
        {
            let estado = if aceites.is_some_and(|a| a.contains(&regra.id)) {
                EstadoExperimental::Aceita
            } else if plano_jogo
                .as_ref()
                .is_some_and(|p| p.estado_experimental.contains_key(&regra.id))
            {
                EstadoExperimental::Aceitavel
            } else {
                continue;
            };
            resumo.sugestoes.push(SugestaoExperimental {
                regra: regra.id.clone(),
                descricao: regra.descricao.clone(),
                camada: Camada::Usuario,
                impacto_visual: impacto(regra.impacto_visual),
                estado,
            });
        }
        if let (Some(plano), Some(fatos)) = (plano, fatos) {
            resumo
                .mudancas
                .extend(plano.resumo.mudancas.values().map(|m| {
                    let destino =
                        destino(m.setting, m.alvo_tela.as_deref()).expect("destino do avaliador");
                    let meta = &self.metadados[&m.regra];
                    Mudanca {
                        camada: Camada::Usuario,
                        alvo_tela: alvo(&destino).map(str::to_owned),
                        setting: nome(&destino).into(),
                        nome_legivel: m.descricao.clone(),
                        antes: valor_legivel(&m.atual),
                        depois: valor_legivel(&m.valor),
                        motivo: if m.codigo.is_some() {
                            format!("{} (experimental aceita por você)", m.motivo)
                        } else {
                            m.motivo.clone()
                        },
                        regra: m.regra.clone(),
                        impacto_visual: impacto(meta.impacto_visual),
                        experimental: if m.codigo.is_some() {
                            EstadoExperimental::Aceita
                        } else {
                            EstadoExperimental::Estavel
                        },
                        status: if corresponde(&m.atual, &m.valor) {
                            StatusMudanca::JaNoValor
                        } else {
                            StatusMudanca::Pendente
                        },
                    }
                }));
            resumo
                .mudancas
                .extend(
                    plano
                        .resumo
                        .respeitadas
                        .iter()
                        .filter_map(|(&setting, motivo)| {
                            let tela = fatos.tela.as_ref().map(|tela| tela.alvo.as_str());
                            let destino = destino(setting, tela)?;
                            Some(Mudanca {
                                camada: Camada::Usuario,
                                alvo_tela: alvo(&destino).map(str::to_owned),
                                setting: nome(&destino).into(),
                                nome_legivel: nome(&destino).into(),
                                antes: None,
                                depois: None,
                                motivo: match motivo {
                                    usuario::MotivoRespeitadaUsuario::Travada => {
                                        "Ajuste travado pelo usuário"
                                    }
                                    usuario::MotivoRespeitadaUsuario::AlteradaForaDaGestao => {
                                        "Alteração manual preservada"
                                    }
                                }
                                .into(),
                                regra: String::new(),
                                impacto_visual: ImpactoVisual::Nenhum,
                                experimental: EstadoExperimental::Estavel,
                                status: StatusMudanca::Travada,
                            })
                        }),
                );
        }
        resumo
    }
}
