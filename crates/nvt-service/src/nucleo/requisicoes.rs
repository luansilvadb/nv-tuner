//! Tratamento das requisições IPC: estado, plano, aplicação, restauração,
//! preferências, travas e histórico.

use super::*;

impl<'a, S: DriverStore> Nucleo<'a, S> {
    /// Trata uma requisição (já validada na forma pelo `nvt-ipc`).
    pub fn tratar(&mut self, req: Requisicao, agora: Instant) -> Resposta {
        if grava_com_driver(&req) && !self.pode_gravar() {
            return Resposta::erro(
                CodigoErro::Interno,
                "serviço em modo somente leitura: nenhuma gravação é feita",
            );
        }
        // Pedidos de outro tratador (chamada direta, sem o despacho do laço).
        match rota(&req) {
            Rota::Nucleo => {}
            // v8: partidas só existem com `medicao-partidas` (L5).
            Rota::Partidas => {
                return Resposta::erro(CodigoErro::RecursoAusente, "recurso ainda não disponível");
            }
            Rota::Usuario | Rota::Recomendadas | Rota::Saude => return requer_coordenador(),
        }
        let r = match req {
            Requisicao::ObterEstado => self.obter_estado(),
            Requisicao::ObterPlano => self.obter_plano(),
            Requisicao::AplicarAgora => self.aplicar_agora(agora),
            Requisicao::RestaurarOriginal => self.restaurar_original(agora),
            Requisicao::DefinirPreferencias {
                sincronismo,
                qualidade,
                limite_fps,
            } => self.definir_preferencias(sincronismo, qualidade, limite_fps, agora),
            Requisicao::ListarHistorico { limite } => Ok(self.listar_historico(limite)),
            Requisicao::TravarSetting { perfil, setting } => self.travar(&perfil, &setting),
            Requisicao::DestravarSetting { perfil, setting } => {
                self.destravar(&perfil, &setting, agora)
            }
            Requisicao::ReescanearJogos => {
                self.varrer(agora);
                self.ciclo(agora, "pedido de nova varredura");
                Ok(Resposta::Ok)
            }
            // Já recusados acima pela `rota`.
            _ => return requer_coordenador(),
        };
        r.unwrap_or_else(|e| {
            error!("requisição IPC falhou: {e}");
            resposta_de_erro(&e)
        })
    }

    pub(super) fn obter_estado(&mut self) -> Resultado<Resposta> {
        let lido = self.renovar().and_then(|()| {
            let store = self.engine.store();
            let versao = store.versao_driver()?;
            let gpus = store.gpus()?;
            let caps = self.engine.capacidades()?;
            Ok((versao, gpus, caps))
        });
        let (driver, gpu) = match lido {
            Ok((v, gpus, caps)) => (
                Some(nvt_ipc::Driver {
                    versao: v.to_string(),
                    ramo: (!v.branch.is_empty()).then(|| v.branch.clone()),
                }),
                gpus.first().map(|g| nvt_ipc::Gpu {
                    nome: g.name.clone(),
                    arquitetura: caps.arquitetura.to_string(),
                    vram_mb: caps.vram_mb,
                    capacidades: Capacidades {
                        dlss: caps.tem_tensor_cores,
                        geracao_quadros: caps.frame_gen,
                        reflex: !matches!(caps.arquitetura, nvt_core::Arquitetura::Unknown(_)),
                        rebar: caps.rebar_ativo.unwrap_or(caps.rebar_suportado),
                        refresh_mhz: caps.refresh_mhz,
                        vrr: nvt_ipc::Vrr {
                            possivel: caps.vrr.possivel,
                            ativo: caps.vrr.ativo,
                        },
                    },
                }),
            ),
            Err(e) => {
                warn!("estado sem driver: {e}");
                (None, None)
            }
        };
        let mut estado = estado_ipc(
            &self.engine.estado().preferencias,
            self.servico.ultima_aplicacao,
        );
        estado.driver = driver;
        estado.gpu = gpu;
        estado.jogos = u32::try_from(self.jogos.len()).unwrap_or(u32::MAX);
        estado.alertas_deriva = self.alertas.clone();
        estado.avisos = self.avisos();
        estado.adiamento = self.adiamento_ipc();
        estado.autonomia_pausada = self.servico.autonomia_pausada;
        Ok(Resposta::Estado(estado))
    }

    pub(super) fn obter_plano(&mut self) -> Resultado<Resposta> {
        self.renovar()?;
        let diff = self.planejar()?;
        Ok(Resposta::Plano(self.plano_ipc(&diff, None)))
    }

    /// Plano sem aplicar, com a ação pendente (resposta de um pedido adiado).
    pub(super) fn plano_adiado(&mut self) -> Resultado<Resposta> {
        let diff = self.planejar()?;
        let mut p = self.plano_ipc(&diff, None);
        p.adiada = self.adiamento_ipc();
        Ok(Resposta::Plano(p))
    }

    pub(super) fn aplicar_agora(&mut self, agora: Instant) -> Resultado<Resposta> {
        if self.restauracao_pendente() {
            // A restauração pendente vence qualquer outra ação (F1-07).
            // Pendente por erro (não por jogo aberto): mostra o erro.
            if let Some(e) = self.pendente.as_ref().and_then(|p| p.ultimo_erro.clone()) {
                info!("aplicar agora ignorado: a restauração pendente falhou ({e})");
                return Ok(Resposta::erro(
                    CodigoErro::Interno,
                    format!(
                        "a restauração pendente falhou ({e}); nova tentativa em {} s; nada foi \
                         aplicado",
                        self.cfg.intervalo_adiamento.as_secs()
                    ),
                ));
            }
            info!("aplicar agora ignorado: restauração pendente até o jogo fechar");
            self.renovar()?;
            return self.plano_adiado();
        }
        self.servico
            .retomar_autonomia(self.dir.as_deref())
            .map_err(|e| ErroEngine::Io(e.to_string()))?;
        self.renovar()?;
        match self.tratar_e_aplicar(agora, "aplicar agora")? {
            Some((diff, ap)) => {
                self.alertas.clear();
                self.erros_pendente.clear();
                Ok(Resposta::Plano(self.plano_ipc(&diff, Some(&ap))))
            }
            None => self.plano_adiado(),
        }
    }

    /// R-AR: experimentais DRS aplicáveis agora (condição casada, ao menos um
    /// setting fora de trava) e ainda não aceitas pela sessão de console.
    pub(crate) fn experimentais_drs_aplicaveis(&mut self) -> Resultado<BTreeSet<String>> {
        self.renovar()?;
        let diff = self.planejar()?;
        Ok(diff
            .plano
            .estado_experimental
            .iter()
            .filter(|(id, e)| {
                **e == nvt_core::EstadoExperimental::Aceitavel && !self.aceites_drs.contains(*id)
            })
            .map(|(id, _)| id.clone())
            .collect())
    }

    pub(super) fn restaurar_original(&mut self, agora: Instant) -> Resultado<Resposta> {
        self.servico
            .pedir_restauracao(self.dir.as_deref())
            .map_err(|e| ErroEngine::Io(e.to_string()))?;
        if !self.restauracao_pendente() {
            self.pendente = Some(Pendente {
                acao: AcaoPendente::RestaurarOriginal,
                adiamento: Adiamento {
                    processos: Vec::new(),
                    desde_unix: self
                        .servico
                        .restauracao_pendente_desde_unix
                        .unwrap_or_else(agora_unix),
                    lista_indisponivel: false,
                },
                tentativas: 0,
                ultimo_erro: None,
            });
            self.proxima_reavaliacao = Some(agora + self.cfg.intervalo_adiamento);
        }
        self.executar_restauracao(agora)
    }

    pub(super) fn executar_restauracao(&mut self, agora: Instant) -> Resultado<Resposta> {
        self.renovar()?;
        let restaurado = match self.engine.restaurar(true) {
            Err(ErroEngine::Adiada(a)) => {
                self.adiar(AcaoPendente::RestaurarOriginal, a, agora);
                return Ok(Resposta::Restauracao {
                    adiada: self.adiamento_ipc(),
                });
            }
            r => r?,
        };
        let mut concluido = self.servico.clone();
        concluido.restauracao_pendente_desde_unix = None;
        self.servico
            .persistir(concluido, self.dir.as_deref())
            .map_err(|e| ErroEngine::Io(e.to_string()))?;
        self.concluir_pendente(AcaoPendente::RestaurarOriginal);
        self.erros_pendente.clear();
        match restaurado {
            None => info!("restauração: não há snapshot original (nada a restaurar)"),
            Some(r) => {
                if r.aplicada {
                    self.marcar_gravacao(agora);
                }
                info!(
                    "restauração: {} operação(ões), {} ignorada(s) fora do catálogo; autonomia pausada",
                    r.ops.len(),
                    r.ignoradas.len()
                );
            }
        }
        self.alertas.clear();
        Ok(Resposta::Restauracao { adiada: None })
    }

    pub(super) fn definir_preferencias(
        &mut self,
        sincronismo: nvt_ipc::Sincronismo,
        qualidade: nvt_ipc::Qualidade,
        limite_fps: Option<u32>,
        agora: Instant,
    ) -> Resultado<Resposta> {
        self.renovar()?;
        let prefs = prefs_core(sincronismo, qualidade, limite_fps);
        // Mudança manual pendente vira escolha travada antes do plano novo.
        // Com jogo aberto, as preferências são gravadas no estado e a
        // aplicação fica pendente.
        let autonomia = self.autonomia_ativa() && !self.restauracao_pendente();
        let mut adiada = false;
        if autonomia {
            match self.tratar_deriva_pendente(agora, "preferências", true) {
                Err(ErroEngine::Adiada(a)) => {
                    self.adiar(AcaoPendente::TratarDeriva, a, agora);
                    adiada = true;
                }
                r => r?,
            }
        }
        self.engine.definir_preferencias(prefs)?;
        info!("preferências atualizadas");
        if autonomia && !adiada {
            adiada = self.aplicar_ou_adiar(agora)?;
        } else if let Err(e) = self.planejar() {
            // Só para atualizar os avisos do plano (G-SYNC sem VRR).
            warn!("plano após as preferências: {e}");
        }
        Ok(self.ok_ou_adiada(adiada))
    }

    /// Aplica o plano; com jogo aberto, deixa `AplicarPlano` pendente e
    /// devolve `true`.
    pub(super) fn aplicar_ou_adiar(&mut self, agora: Instant) -> Resultado<bool> {
        match self.aplicar_plano(agora) {
            Err(ErroEngine::Adiada(a)) => {
                self.adiar(AcaoPendente::AplicarPlano, a, agora);
                Ok(true)
            }
            Err(e) => Err(e),
            Ok(_) => {
                self.concluir_pendente(AcaoPendente::AplicarPlano);
                Ok(false)
            }
        }
    }

    /// Alvo e perfil de um nome vindo do IPC, com o nome **canônico** do DRS
    /// (a busca por nome não diferencia maiúsculas: "LEAGUE OF LEGENDS" e
    /// "league of legends" são o mesmo perfil e viram a mesma trava).
    ///
    /// `None` se o perfil não existe ou não é gerenciável: só o global, os
    /// perfis dos jogos detectados, os criados pelo app e os que já têm
    /// setting gerida ou trava.
    pub(super) fn alvo_de(&self, perfil: &str) -> Resultado<Option<(Alvo, ProfileRef)>> {
        if perfil == PERFIL_GLOBAL {
            return Ok(Some((Alvo::Global, ProfileRef::Base)));
        }
        let Some(nome) = self.engine.store().nome_canonico(perfil)? else {
            return Ok(None);
        };
        let estado = self.engine.estado();
        let gerenciavel = estado.criado_pelo_app(&nome)
            || estado
                .esperado
                .iter()
                .any(|e| matches!(&e.perfil, ProfileRef::Named(n) if mesmo_nome_perfil(n, &nome)))
            || estado
                .travadas
                .iter()
                .any(|t| mesmo_alvo(&t.alvo, &Alvo::Jogo(nome.clone())))
            || self
                .engine
                .resolver_jogos(&self.jogos)?
                .iter()
                .any(|j| mesmo_nome_perfil(&j.perfil, &nome));
        if !gerenciavel {
            return Ok(None);
        }
        Ok(Some((Alvo::Jogo(nome.clone()), ProfileRef::Named(nome))))
    }

    /// Trava como escolha do usuário. Usa o tratamento de mudança pontual do
    /// motor (sai do estado esperado, vira `Travada`, registra no histórico),
    /// sem gravar nada no driver.
    pub(super) fn travar(&mut self, perfil: &str, setting: &str) -> Resultado<Resposta> {
        self.renovar()?;
        let Some(def) = self.def(setting).cloned() else {
            return Ok(nao_permitida(setting));
        };
        let Some((alvo, pref)) = self.alvo_de(perfil)? else {
            return Ok(perfil_inexistente(perfil));
        };
        let estado = self.engine.estado();
        if estado.travada(&alvo, setting) {
            return Ok(Resposta::Ok);
        }
        if estado.travadas.len() >= MAX_TRAVADAS {
            return Ok(Resposta::erro(
                CodigoErro::Interno,
                format!(
                    "limite de {MAX_TRAVADAS} escolhas travadas atingido: destrave alguma \
                     antes de travar outra"
                ),
            ));
        }
        let atual = self
            .engine
            .store()
            .ler_setting(&pref, def.id)?
            .and_then(|s| s.value.as_u32());
        let esperado = estado
            .esperado
            .iter()
            .find(|e| e.perfil == pref && e.id == def.id)
            .cloned()
            .unwrap_or(Esperado {
                alvo,
                perfil: pref,
                setting: def.nome.clone(),
                id: def.id,
                valor: atual.unwrap_or(0),
                origem: Origem::Preferencia {
                    preferencia: "usuario".into(),
                    regra: None,
                },
                antes_da_gestao: None,
            });
        let deriva = Deriva {
            classe: ClasseDeriva::Pontual,
            divergencias: vec![Divergencia {
                esperado,
                atual,
                voltou_ao_padrao: false,
            }],
            versao_driver: self.engine.store().versao_driver()?.raw,
        };
        self.engine.tratar_deriva(&deriva, true)?;
        info!("setting {setting} travada em {perfil} pelo usuário");
        Ok(Resposta::Ok)
    }

    pub(super) fn destravar(
        &mut self,
        perfil: &str,
        setting: &str,
        agora: Instant,
    ) -> Resultado<Resposta> {
        self.renovar()?;
        if self.def(setting).is_none() {
            return Ok(nao_permitida(setting));
        }
        let alvo = if perfil == PERFIL_GLOBAL {
            Alvo::Global
        } else {
            Alvo::Jogo(perfil.to_owned())
        };
        let autonomia = self.autonomia_ativa() && !self.restauracao_pendente();
        let mut adiada = false;
        if autonomia && self.engine.estado().travada(&alvo, setting) {
            // Mudança manual pendente vira escolha travada antes do plano novo.
            match self.tratar_deriva_pendente(agora, "destravar", true) {
                Err(ErroEngine::Adiada(a)) => {
                    self.adiar(AcaoPendente::TratarDeriva, a, agora);
                    adiada = true;
                }
                r => r?,
            }
        }
        // `liberar_travada` compara o perfil sem diferenciar maiúsculas.
        if self.engine.liberar_travada(&alvo, setting)? {
            info!("setting {setting} destravada em {perfil}");
            self.alertas
                .retain(|a| !(mesmo_nome_perfil(&a.perfil, perfil) && a.setting == setting));
            if autonomia && !adiada {
                adiada = self.aplicar_ou_adiar(agora)?;
            }
        }
        Ok(self.ok_ou_adiada(adiada))
    }

    pub(super) fn listar_historico(&self, limite: u32) -> Resposta {
        let cat = &self.engine.regras().catalogo;
        let hist = &self.engine.estado().historico;
        let entradas = hist
            .iter()
            .enumerate()
            .rev()
            .take(limite as usize)
            .map(|(i, r)| {
                let def = cat.get(&r.setting);
                let acao = match &r.origem {
                    OrigemRegistro::Plano { .. } | OrigemRegistro::CriacaoPerfil => {
                        AcaoHistorico::Aplicacao
                    }
                    OrigemRegistro::Reaplicacao { .. } => AcaoHistorico::DerivaDetectada,
                    OrigemRegistro::Usuario => AcaoHistorico::Travamento,
                    OrigemRegistro::Restauracao => AcaoHistorico::Restauracao,
                };
                EntradaHistorico {
                    camada: nvt_ipc::Camada::Driver,
                    id: i as u64 + 1,
                    quando_unix: iso_para_unix(&r.data),
                    acao,
                    perfil: (!r.perfil.is_empty()).then(|| {
                        if r.perfil == nvt_engine::NOME_PERFIL_BASE {
                            PERFIL_GLOBAL.to_owned()
                        } else {
                            r.perfil.clone()
                        }
                    }),
                    setting: (!r.setting.is_empty()).then(|| r.setting.clone()),
                    antes: nomes::valor(def, r.antes),
                    depois: nomes::valor(def, r.depois),
                    descricao: r.motivo.clone(),
                }
            })
            .collect();
        Resposta::Historico { entradas }
    }
}

fn requer_coordenador() -> Resposta {
    Resposta::erro(
        CodigoErro::RequisicaoInvalida,
        "pedido requer coordenador de usuário",
    )
}

pub(super) fn nao_permitida(setting: &str) -> Resposta {
    Resposta::erro(
        CodigoErro::SettingNaoPermitida,
        format!("setting \"{setting}\" fora da lista permitida"),
    )
}

pub(super) fn perfil_inexistente(perfil: &str) -> Resposta {
    Resposta::erro(
        CodigoErro::PerfilNaoEncontrado,
        format!("perfil \"{perfil}\" não existe no driver"),
    )
}
