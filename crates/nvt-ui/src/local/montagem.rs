//! Montagem dos [`Dados`] da UI a partir do plano: mudanças, reversões e jogos.

use super::*;

pub(super) fn montar_dados<S: DriverStore>(
    e: &Engine<S>,
    diff: &Diff,
    gpu: Option<&GpuInfo>,
    ramo: Option<String>,
    alertas_deriva: Vec<AlertaDeriva>,
) -> Dados {
    let c = &diff.capacidades;
    let est = e.estado();

    // Alvos na ordem: global, depois jogos.
    let mut alvos: Vec<Alvo> = vec![Alvo::Global];
    alvos.extend(diff.jogos.iter().map(|j| Alvo::Jogo(j.perfil.clone())));
    let mut grupos = Vec::new();
    for a in &alvos {
        let mut itens: Vec<ItemMudanca> = diff
            .mudancas_de(a)
            .map(|m: &MudancaPlanejada| item(e, &m.mudanca, StatusMudanca::Pendente, false))
            .collect();
        itens.extend(
            diff.conformes_de(a)
                .map(|m| item(e, &m.mudanca, StatusMudanca::JaNoValor, false)),
        );
        for s in diff.plano.experimentais.iter().filter(|s| &s.alvo == a) {
            itens.extend(
                s.mudancas
                    .iter()
                    .map(|m| item(e, m, StatusMudanca::Pendente, true)),
            );
        }
        if !itens.is_empty() {
            grupos.push(GrupoMudancas {
                perfil: perfil_de_alvo(a),
                nome_legivel: nome_do_alvo(a),
                itens,
            });
        }
    }

    let respeitadas = diff
        .plano
        .respeitadas
        .iter()
        .map(|r| Respeitada {
            perfil: perfil_de_alvo(&r.alvo),
            nome_perfil: nome_do_alvo(&r.alvo),
            setting: r.mudanca.setting.clone(),
            nome_legivel: nome_legivel(e, &r.mudanca.setting),
            motivo: est
                .travadas
                .iter()
                .find(|t| t.alvo == r.alvo && t.setting == r.mudanca.setting)
                .map_or_else(|| r.mudanca.explicacao.clone(), |t| t.motivo.clone()),
        })
        .collect();

    let historico: Vec<EntradaHistorico> = est
        .historico
        .iter()
        .enumerate()
        .rev()
        .take(HISTORICO_EXIBIDO)
        .map(|(i, h)| EntradaHistorico {
            camada: nvt_ipc::Camada::Driver,
            id: i as u64 + 1,
            quando_unix: ler_iso(&h.data).unwrap_or(0),
            acao: acao_de_origem(&h.origem),
            perfil: Some(h.perfil.clone()),
            setting: (!h.setting.is_empty()).then(|| nome_legivel(e, &h.setting)),
            antes: valor(e, &h.setting, h.antes),
            depois: valor(e, &h.setting, h.depois),
            descricao: h.motivo.clone(),
        })
        .collect();

    let ultima_aplicacao = est
        .historico
        .iter()
        .rev()
        .find(|h| matches!(h.origem, OrigemRegistro::Plano { .. }))
        .and_then(|h| ler_iso(&h.data))
        .map(|quando| Aplicacao {
            quando_unix: quando,
            mudancas: est.esperado.len() as u32,
            falhas: 0,
        });

    let estado = Estado {
        agente: Default::default(),
        diagnosticos: Vec::new(),
        versao_servico: String::new(),
        driver: Some(Driver {
            versao: c.driver_version.to_string(),
            ramo,
        }),
        gpu: gpu.map(|g| Gpu {
            nome: g.name.clone(),
            arquitetura: c.arquitetura.nome_regra().into(),
            vram_mb: c.vram_mb,
            capacidades: Capacidades {
                dlss: c.tem_tensor_cores,
                geracao_quadros: c.frame_gen,
                // Reflex vale para toda GeForce suportada pelo driver atual.
                reflex: c.arquitetura.nome_regra() != "desconhecida",
                rebar: c.rebar_suportado,
                refresh_mhz: c.refresh_mhz,
                vrr: nvt_ipc::Vrr {
                    possivel: c.vrr.possivel,
                    ativo: c.vrr.ativo,
                },
            },
        }),
        preferencias: preferencias_ipc(&est.preferencias),
        jogos: diff.jogos.len() as u32,
        ultima_aplicacao,
        alertas_deriva,
        avisos: Vec::new(),
        adiamento: None,
        atualizacao: Default::default(),
        autonomia_pausada: false,
    };

    // O modo local só lê o estado compartilhado do driver.
    let sugestoes_driver = diff
        .plano
        .experimentais
        .iter()
        .map(|s| &s.regra)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter_map(|id| {
            let regra = e.regras().regras.iter().find(|r| &r.id == id)?;
            Some(nvt_ipc::SugestaoExperimental {
                regra: id.clone(),
                descricao: regra.descricao.clone(),
                camada: nvt_ipc::Camada::Driver,
                impacto_visual: impacto_ipc(regra.impacto_visual),
                estado: nvt_ipc::EstadoExperimental::Aceitavel,
            })
        })
        .collect();

    Dados {
        estado: Some(estado),
        sistema: Default::default(),
        sugestoes_driver,
        grupos,
        respeitadas,
        historico,
        reversoes: reversoes(e, diff),
        ignorados: ignorados(diff),
        jogos: Vec::new(),
        saude: None,
    }
}

/// Settings que saíram do plano, como o serviço as mostraria (pendentes).
pub fn reversoes<S: DriverStore>(e: &Engine<S>, diff: &Diff) -> Vec<Reversao> {
    diff.reverter
        .iter()
        .map(|r| {
            let x = &r.esperado;
            let (acao, original) = match &r.acao {
                AcaoMotor::Gravar(v) => (
                    AcaoReversao::GravarOriginal,
                    valor(e, &x.setting, v.as_u32()),
                ),
                AcaoMotor::Remover { .. } => (AcaoReversao::Remover, None),
                AcaoMotor::SoDeixarDeVigiar => (AcaoReversao::SoDeixarDeVigiar, None),
            };
            let causa = match &x.origem {
                Origem::Regra { id } => textos::causa_regra(id),
                Origem::Preferencia { preferencia, regra } => {
                    textos::causa_preferencia(preferencia, regra.as_deref())
                }
            };
            Reversao {
                perfil: perfil_de_alvo(&x.alvo),
                nome_perfil: nome_do_alvo(&x.alvo),
                setting: x.setting.clone(),
                nome_legivel: nome_legivel(e, &x.setting),
                atual: valor(e, &x.setting, r.atual),
                original,
                acao,
                motivo: textos::motivo_reversao(&causa, acao),
                status: StatusMudanca::Pendente,
            }
        })
        .collect()
}

pub fn ignorados(diff: &Diff) -> Vec<JogoIgnorado> {
    diff.ignorados
        .iter()
        .map(|j| JogoIgnorado {
            nome: j.nome.clone(),
            exe: j.exe.clone(),
            motivo: j.motivo.clone(),
        })
        .collect()
}

/// Jogos do diff (fatos, perfil e explicação) no formato do protocolo, como
/// o serviço os envia (`nvt-service`, `nucleo::jogos_ipc`).
pub fn jogos_ipc<S: DriverStore>(e: &Engine<S>, diff: &Diff) -> Vec<Jogo> {
    use nvt_core::MotivoNaoAplicada as M;
    let regras = &e.regras().regras;
    let descricao = |id: &str| {
        regras
            .iter()
            .find(|r| r.id == id)
            .map_or_else(String::new, |r| r.descricao.clone())
    };
    let motivo = |m: &M| match m {
        M::JogoSemDlss => MotivoNaoAplicada::JogoSemDlss,
        M::ArquiteturaNaoPrecisa { arquitetura } => MotivoNaoAplicada::ArquiteturaNaoPrecisa {
            arquitetura: arquitetura.nome_regra().to_owned(),
        },
        M::SemTensorCores => MotivoNaoAplicada::SemTensorCores,
        M::Experimental => MotivoNaoAplicada::Experimental,
        M::BloqueadaPeloTetoDeQualidade { teto, .. } => {
            MotivoNaoAplicada::BloqueadaPeloTetoDeQualidade {
                teto: preferencias_ipc(&nvt_core::Preferences {
                    qualidade: *teto,
                    ..Default::default()
                })
                .qualidade,
            }
        }
        M::EscolhaDoUsuario { setting } => MotivoNaoAplicada::EscolhaDoUsuario {
            setting: setting.clone(),
        },
        M::ConflitoComRegraMaisEspecifica { regra } => {
            MotivoNaoAplicada::ConflitoComRegraMaisEspecifica {
                regra: regra.clone(),
            }
        }
        M::FatoDesconhecido { fato } => MotivoNaoAplicada::FatoDesconhecido { fato: fato.clone() },
        // Mesmos motivos específicos do serviço (`nvt-service::nucleo::jogos_ipc`).
        M::CondicaoNaoAtendida {
            condicao,
            esperado,
            encontrado,
        } if condicao == "vrr_ativo" && esperado == "true" && encontrado == "false" => {
            MotivoNaoAplicada::VrrInativo
        }
        M::CondicaoNaoAtendida {
            condicao,
            esperado,
            encontrado,
        } if condicao == "jogo_reflex" && esperado == "false" && encontrado == "true" => {
            MotivoNaoAplicada::JogoComReflex
        }
        M::CondicaoNaoAtendida {
            condicao,
            esperado,
            encontrado,
        } => MotivoNaoAplicada::CondicaoNaoAtendida {
            condicao: condicao.clone(),
            esperado: esperado.clone(),
            encontrado: encontrado.clone(),
        },
    };
    let base = e.base_jogos();
    diff.jogos
        .iter()
        .map(|j| {
            let r = diff
                .plano
                .resumos
                .get(&j.perfil)
                .cloned()
                .unwrap_or_default();
            let curado = nvt_engine::casar_na_base(base, &j.entrada);
            Jogo {
                nome: j.entrada.nome.clone(),
                perfil: j.perfil.clone(),
                origem_perfil: match j.origem {
                    OrigemPerfil::Driver => OrigemPerfilIpc::Driver,
                    OrigemPerfil::Usuario => OrigemPerfilIpc::Usuario,
                    OrigemPerfil::CriadoPeloApp => OrigemPerfilIpc::CriadoPeloApp,
                    OrigemPerfil::ACriar => OrigemPerfilIpc::ACriar,
                },
                launcher: j.entrada.launcher.clone(),
                api: j.entrada.api.nome_regra().to_owned(),
                dlss: j.entrada.dlss.map(|v| {
                    if v == nvt_core::Versao::default() {
                        String::new()
                    } else {
                        v.to_string()
                    }
                }),
                // Jogo da base curada: Reflex e anticheat vêm da base (como no serviço).
                reflex: curado.map_or(j.entrada.reflex, |c| Some(c.reflex)),
                anticheat_kernel: curado
                    .map_or(j.entrada.anticheat_kernel, |c| Some(c.anticheat_kernel)),
                competitivo: curado.map(|c| c.id.as_str().to_owned()),
                em_execucao: j.entrada.em_execucao,
                orientacoes: r
                    .orientacoes
                    .iter()
                    .map(|o| o.codigo().to_owned())
                    .collect(),
                config_jogo: None,
                resumo: ResumoJogo {
                    aplicadas: r
                        .aplicadas
                        .iter()
                        .map(|a| RegraAplicada {
                            regra: a.regra.clone(),
                            descricao: descricao(&a.regra),
                            settings: a.settings.clone(),
                        })
                        .collect(),
                    nao_aplicadas: r
                        .nao_aplicadas
                        .iter()
                        .map(|n| RegraNaoAplicada {
                            regra: n.regra.clone(),
                            descricao: descricao(&n.regra),
                            motivo: motivo(&n.motivo),
                        })
                        .collect(),
                    herda_do_global: r.herda_do_global,
                },
            }
        })
        .collect()
}
