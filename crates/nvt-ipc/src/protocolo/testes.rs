//! Testes do formato no fio e das validações do protocolo.

use super::*;
use crate::quadro::{codificar, decodificar};

fn prefs() -> Preferencias {
    Preferencias {
        sincronismo: Sincronismo::Desligado,
        qualidade: Qualidade::Equilibrado,
        limite_fps: Some(141),
    }
}

/// Um exemplar de cada requisição. O `match` garante (em tempo de
/// compilação) que nenhuma variante nova fique sem teste.
fn consulta() -> Requisicao {
    use crate::Fato;
    Requisicao::AgenteConsultar {
        fatos: FatosUsuario {
            config_jogo: vec![],
            tela: Fato::Desconhecido,
            mouse: Fato::Desconhecido,
            swap_effect_upgrade: Fato::Ausente,
            captura_historica: Fato::Conhecido(1),
            diagnosticos: vec![Diagnostico {
                codigo: "drr".into(),
                estado: crate::EstadoDiagnostico::Indisponivel,
            }],
            gestao: vec![],
        },
        ultima_tarefa: None,
        geracao_restaurada: 0,
    }
}

fn todas_requisicoes() -> Vec<Requisicao> {
    let v = vec![
        Requisicao::ObterEstado,
        Requisicao::ObterPlano,
        Requisicao::AplicarAgora,
        Requisicao::RestaurarOriginal,
        Requisicao::DefinirPreferencias {
            sincronismo: Sincronismo::GSync,
            qualidade: Qualidade::Qualidade,
            limite_fps: None,
        },
        Requisicao::DefinirPreferencias {
            sincronismo: Sincronismo::VSync,
            qualidade: Qualidade::Desempenho,
            limite_fps: Some(60),
        },
        Requisicao::ListarHistorico { limite: 50 },
        Requisicao::TravarSetting {
            perfil: "WARDOGS".into(),
            setting: "dlss_preset".into(),
        },
        Requisicao::DestravarSetting {
            perfil: "global".into(),
            setting: "vsync".into(),
        },
        Requisicao::ReescanearJogos,
        consulta(),
        Requisicao::AgenteRelatar {
            resultados_jogo: vec![],
            saude: None,
            id_tarefa: 1,
            operacao: OperacaoUsuario::Aplicar,
            revisao: 7,
            resultados: vec![ResultadoUsuario {
                destino: SettingUsuario::GameDvrCapturaHistorica,
                status: StatusMudanca::Aplicada,
                detalhe: None,
                mudanca: Some(crate::AjusteUsuario::GameDvrCapturaHistorica {
                    antes: Some(1),
                    depois: Some(0),
                }),
            }],
        },
        Requisicao::DefinirAceiteExperimental {
            regra: "sincronismo-gsync-limite".into(),
            aceito: true,
        },
        Requisicao::ReverterAjusteUsuario {
            alvo_tela: None,
            destino: SettingUsuario::MouseAceleracao,
        },
        Requisicao::ReverterAjusteUsuario {
            alvo_tela: Some("monitor-estavel".into()),
            destino: SettingUsuario::Refresh {
                alvo: "monitor-estavel".into(),
            },
        },
        Requisicao::ReverterConfigJogo {
            destino: crate::config_jogo::testes::destino(),
        },
        Requisicao::AplicarRecomendadas,
        Requisicao::ObterSaude,
        Requisicao::MarcarInicioVisto {
            fonte: crate::FonteInicio::Tarefa,
            nome: "OneDrive Reporting".into(),
        },
        Requisicao::ListarPartidas {
            desde_unix: 0,
            limite: PARTIDAS_MAX,
        },
        Requisicao::ObterPartida { id: 1 },
        Requisicao::ApagarPartidas,
        Requisicao::MarcarAtualizacaoVista {
            versao: "1.4.0".into(),
        },
    ];
    for r in &v {
        match r {
            Requisicao::ObterEstado
            | Requisicao::ObterPlano
            | Requisicao::AplicarAgora
            | Requisicao::RestaurarOriginal
            | Requisicao::DefinirPreferencias { .. }
            | Requisicao::ListarHistorico { .. }
            | Requisicao::TravarSetting { .. }
            | Requisicao::DestravarSetting { .. }
            | Requisicao::ReescanearJogos
            | Requisicao::AgenteConsultar { .. }
            | Requisicao::AgenteRelatar { .. }
            | Requisicao::DefinirAceiteExperimental { .. }
            | Requisicao::ReverterAjusteUsuario { .. }
            | Requisicao::ReverterConfigJogo { .. }
            | Requisicao::MarcarAtualizacaoVista { .. }
            | Requisicao::AplicarRecomendadas
            | Requisicao::ObterSaude
            | Requisicao::MarcarInicioVisto { .. }
            | Requisicao::ListarPartidas { .. }
            | Requisicao::ObterPartida { .. }
            | Requisicao::ApagarPartidas => {}
        }
    }
    v
}

/// Um motivo de cada tipo. O `match` garante que nenhum fique sem teste.
fn todos_os_motivos() -> Vec<MotivoNaoAplicada> {
    let v = vec![
        MotivoNaoAplicada::JogoSemDlss,
        MotivoNaoAplicada::ArquiteturaNaoPrecisa {
            arquitetura: "ada".into(),
        },
        MotivoNaoAplicada::SemTensorCores,
        MotivoNaoAplicada::Experimental,
        MotivoNaoAplicada::BloqueadaPeloTetoDeQualidade {
            teto: Qualidade::Qualidade,
        },
        MotivoNaoAplicada::EscolhaDoUsuario {
            setting: "dlss_preset".into(),
        },
        MotivoNaoAplicada::ConflitoComRegraMaisEspecifica {
            regra: "outra".into(),
        },
        MotivoNaoAplicada::FatoDesconhecido {
            fato: "jogo_reflex".into(),
        },
        MotivoNaoAplicada::CondicaoNaoAtendida {
            condicao: "arquitetura".into(),
            esperado: "ampere".into(),
            encontrado: "turing".into(),
        },
        MotivoNaoAplicada::VrrInativo,
        MotivoNaoAplicada::JogoComReflex,
        MotivoNaoAplicada::ConfigPolitica {
            politica: "orientar".into(),
        },
        MotivoNaoAplicada::JanelaFechadaExigida {
            processos: vec!["steam.exe".into()],
        },
        MotivoNaoAplicada::ArquivoNaNuvem,
        MotivoNaoAplicada::ArquivoSomenteLeitura,
        MotivoNaoAplicada::FormatoInesperado,
        MotivoNaoAplicada::ArquivoAlteradoPeloJogo,
        MotivoNaoAplicada::ContaAmbigua,
        MotivoNaoAplicada::ArquivoAusente,
        MotivoNaoAplicada::ReparseRecusado,
    ];
    for m in &v {
        match m {
            MotivoNaoAplicada::JogoSemDlss
            | MotivoNaoAplicada::ArquiteturaNaoPrecisa { .. }
            | MotivoNaoAplicada::SemTensorCores
            | MotivoNaoAplicada::Experimental
            | MotivoNaoAplicada::BloqueadaPeloTetoDeQualidade { .. }
            | MotivoNaoAplicada::EscolhaDoUsuario { .. }
            | MotivoNaoAplicada::ConflitoComRegraMaisEspecifica { .. }
            | MotivoNaoAplicada::FatoDesconhecido { .. }
            | MotivoNaoAplicada::CondicaoNaoAtendida { .. }
            | MotivoNaoAplicada::VrrInativo
            | MotivoNaoAplicada::JogoComReflex
            | MotivoNaoAplicada::ConfigPolitica { .. }
            | MotivoNaoAplicada::JanelaFechadaExigida { .. }
            | MotivoNaoAplicada::ArquivoNaNuvem
            | MotivoNaoAplicada::ArquivoSomenteLeitura
            | MotivoNaoAplicada::FormatoInesperado
            | MotivoNaoAplicada::ArquivoAlteradoPeloJogo
            | MotivoNaoAplicada::ContaAmbigua
            | MotivoNaoAplicada::ArquivoAusente
            | MotivoNaoAplicada::ReparseRecusado => {}
        }
    }
    v
}

fn jogo_exemplo() -> Jogo {
    Jogo {
        config_jogo: None,
        nome: "WARDOGS".into(),
        perfil: "WARDOGS".into(),
        origem_perfil: OrigemPerfil::Usuario,
        launcher: "Steam".into(),
        api: "dx12".into(),
        dlss: Some("310.6.0.0".into()),
        reflex: Some(true),
        anticheat_kernel: None,
        competitivo: Some("cs2".into()),
        em_execucao: true,
        orientacoes: vec!["reflex_on_boost".into(), "vsync_jogo_off".into()],
        resumo: ResumoJogo {
            aplicadas: vec![RegraAplicada {
                regra: "dlss-preset-e-turing".into(),
                descricao: "DLSS-SR com preset E".into(),
                settings: vec!["dlss_override".into(), "dlss_preset".into()],
            }],
            nao_aplicadas: todos_os_motivos()
                .into_iter()
                .map(|motivo| RegraNaoAplicada {
                    regra: "r".into(),
                    descricao: "d".into(),
                    motivo,
                })
                .collect(),
            herda_do_global: vec!["energia".into(), "vsync".into()],
        },
    }
}

#[test]
fn formato_json_do_jogo_e_dos_motivos() {
    use serde_json::{from_value, json, to_value};
    for m in todos_os_motivos() {
        assert_eq!(
            from_value::<MotivoNaoAplicada>(to_value(&m).unwrap()).unwrap(),
            m
        );
    }
    assert_eq!(
        to_value(MotivoNaoAplicada::ArquiteturaNaoPrecisa {
            arquitetura: "ada".into()
        })
        .unwrap(),
        json!({"tipo":"arquitetura_nao_precisa","arquitetura":"ada"})
    );
    assert_eq!(
        to_value(MotivoNaoAplicada::JogoSemDlss).unwrap(),
        json!({"tipo":"jogo_sem_dlss"})
    );
    assert_eq!(
        to_value(MotivoNaoAplicada::BloqueadaPeloTetoDeQualidade {
            teto: Qualidade::Qualidade
        })
        .unwrap(),
        json!({"tipo":"bloqueada_pelo_teto_de_qualidade","teto":"qualidade"})
    );
    for (o, s) in [
        (OrigemPerfil::Driver, "driver"),
        (OrigemPerfil::Usuario, "usuario"),
        (OrigemPerfil::CriadoPeloApp, "criado_pelo_app"),
        (OrigemPerfil::ACriar, "a_criar"),
    ] {
        assert_eq!(to_value(o).unwrap(), json!(s));
    }
    let j = to_value(jogo_exemplo()).unwrap();
    assert_eq!(j["origem_perfil"], json!("usuario"));
    assert_eq!(
        j["resumo"]["aplicadas"][0]["regra"],
        json!("dlss-preset-e-turing")
    );
    assert_eq!(j["resumo"]["herda_do_global"], json!(["energia", "vsync"]));
    assert_eq!(from_value::<Jogo>(j.clone()).unwrap(), jogo_exemplo());
    // Campo desconhecido no jogo é recusado.
    let mut extra = j;
    extra["extra"] = json!(1);
    assert!(from_value::<Jogo>(extra).is_err());
    // Plano da versão 2 (sem `jogos`) não é aceito.
    assert!(
        from_value::<Plano>(json!({
            "gerado_unix":0,"perfis":[],"reversoes":[],"ignorados":[]
        }))
        .is_err()
    );
}

fn todas_respostas() -> Vec<Resposta> {
    let v = vec![
        Resposta::Estado(Estado {
            agente: EstadoAgente::default(),
            diagnosticos: vec![],
            versao_servico: "0.0.1".into(),
            driver: Some(Driver {
                versao: "581.57".into(),
                ramo: Some("r580_00".into()),
            }),
            gpu: Some(Gpu {
                nome: "NVIDIA GeForce RTX 4070".into(),
                arquitetura: "ada".into(),
                vram_mb: 12282,
                capacidades: Capacidades {
                    dlss: true,
                    geracao_quadros: true,
                    reflex: true,
                    rebar: false,
                    refresh_mhz: Some(239_757),
                    vrr: Vrr {
                        possivel: Some(true),
                        ativo: Some(false),
                    },
                },
            }),
            preferencias: prefs(),
            jogos: 12,
            ultima_aplicacao: Some(Aplicacao {
                quando_unix: 1_790_000_000,
                mudancas: 7,
                falhas: 0,
            }),
            alertas_deriva: vec![AlertaDeriva {
                perfil: "WARDOGS".into(),
                setting: "dlss_preset".into(),
                nome_legivel: "Preset do DLSS".into(),
                esperado: "E".into(),
                encontrado: None,
                detectado_unix: 1_790_000_100,
            }],
            avisos: vec![
                Aviso::snapshot_indisponivel("snapshot-original.json ilegível", 1_790_000_000),
                Aviso::erro_perfil("perfil X: falha", true, 1_790_000_001),
                Aviso::novo(
                    NivelAviso::Info,
                    codigo_aviso::ESTADO_RECUPERADO,
                    "Estado recuperado",
                    None,
                    1_790_000_002,
                ),
                Aviso::gsync_sem_vrr(1_790_000_003),
                Aviso::vrr_desligado_no_painel(1_790_000_004),
            ],
            adiamento: Some(adiamento(AcaoAdiada::TratarDeriva)),
            atualizacao: atualizacao_exemplo(),
            autonomia_pausada: false,
        }),
        Resposta::Estado(Estado {
            agente: EstadoAgente::default(),
            diagnosticos: vec![],
            versao_servico: "0.0.1".into(),
            driver: None,
            gpu: None,
            preferencias: prefs(),
            jogos: 0,
            ultima_aplicacao: None,
            alertas_deriva: vec![],
            avisos: vec![],
            adiamento: None,
            atualizacao: EstadoAtualizacao::default(),
            autonomia_pausada: false,
        }),
        Resposta::Plano(Plano {
            sistema: ResumoSistema::default(),
            sugestoes_driver: vec![],
            gerado_unix: 1,
            perfis: vec![],
            reversoes: vec![],
            ignorados: vec![],
            jogos: vec![],
            adiada: Some(adiamento(AcaoAdiada::AplicarPlano)),
        }),
        Resposta::Plano(Plano {
            sistema: ResumoSistema::default(),
            sugestoes_driver: vec![],
            gerado_unix: 1_790_000_000,
            perfis: vec![PlanoPerfil {
                perfil: "global".into(),
                nome_legivel: "Perfil global".into(),
                mudancas: vec![Mudanca {
                    alvo_tela: None,
                    camada: Camada::Driver,
                    experimental: EstadoExperimental::Estavel,
                    setting: "vsync".into(),
                    nome_legivel: "Sincronização vertical".into(),
                    antes: Some("Controlado pelo aplicativo".into()),
                    depois: Some("Forçar desligado".into()),
                    motivo: "Preferência: sincronismo desligado".into(),
                    regra: "sincronismo.desligado".into(),
                    impacto_visual: ImpactoVisual::Baixo,
                    status: StatusMudanca::Pendente,
                }],
            }],
            reversoes: vec![Reversao {
                perfil: "global".into(),
                nome_perfil: "Perfil global".into(),
                setting: "limite_fps".into(),
                nome_legivel: "Limitador de FPS".into(),
                atual: Some("141 FPS".into()),
                original: None,
                acao: AcaoReversao::Remover,
                motivo: "Saiu do plano: preferência limite_fps".into(),
                status: StatusMudanca::Aplicada,
            }],
            ignorados: vec![JogoIgnorado {
                nome: "Jogo\u{0}Ruim".into(),
                exe: String::new(),
                motivo: "executável vazio".into(),
            }],
            jogos: vec![jogo_exemplo()],
            adiada: None,
        }),
        Resposta::Restauracao { adiada: None },
        Resposta::Restauracao {
            adiada: Some(adiamento(AcaoAdiada::RestaurarOriginal)),
        },
        Resposta::Historico {
            entradas: vec![EntradaHistorico {
                camada: Camada::Driver,
                id: 1,
                quando_unix: 1_790_000_000,
                acao: AcaoHistorico::Aplicacao,
                perfil: Some("WARDOGS".into()),
                setting: Some("dlss_preset".into()),
                antes: None,
                depois: Some("E".into()),
                descricao: "Aplicação automática".into(),
            }],
        },
        Resposta::Historico { entradas: vec![] },
        Resposta::Adiada {
            adiamento: adiamento(AcaoAdiada::AplicarPlano),
        },
        Resposta::Adiada {
            adiamento: Adiamento {
                processos: vec![],
                desde_unix: 1_790_000_600,
                acao: AcaoAdiada::TratarDeriva,
                lista_indisponivel: true,
            },
        },
        Resposta::PlanoAgente(PlanoAgente {
            tarefa_jogo: None,
            revisao: 7,
            geracao_restauracao: 2,
            tarefa: Some(crate::TarefaUsuario {
                tarefa: crate::ReferenciaTarefa {
                    id_tarefa: 1,
                    operacao: OperacaoUsuario::Aplicar,
                    revisao: 7,
                },
                acoes: ajustes_exemplo()
                    .into_iter()
                    .map(|ajuste| crate::AcaoUsuario::Aplicar { ajuste })
                    .collect(),
            }),
            processos_vigiados: ["cs2.exe".into()].into(),
        }),
        Resposta::Recomendadas {
            aceitas: vec!["sincronismo-gsync-limite".into()],
            adiada: true,
        },
        Resposta::Saude(crate::saude::testes::saude()),
        Resposta::Partidas {
            resumos: vec![crate::saude::testes::partida().resumo],
        },
        Resposta::Partida(crate::saude::testes::partida()),
        Resposta::erro(CodigoErro::RecursoAusente, "ainda não implementado"),
        Resposta::Encerrar,
        Resposta::Ok,
        Resposta::erro(CodigoErro::SettingNaoPermitida, "setting fora da lista"),
    ];
    for r in &v {
        match r {
            Resposta::Estado(_)
            | Resposta::Plano(_)
            | Resposta::Historico { .. }
            | Resposta::Restauracao { .. }
            | Resposta::Adiada { .. }
            | Resposta::PlanoAgente(_)
            | Resposta::Recomendadas { .. }
            | Resposta::Saude(_)
            | Resposta::Partidas { .. }
            | Resposta::Partida(_)
            | Resposta::Encerrar
            | Resposta::Ok
            | Resposta::Erro { .. } => {}
        }
    }
    v
}

fn ajustes_exemplo() -> [crate::AjusteUsuario; 4] {
    use crate::{AjusteUsuario, MouseSpi, RefreshRacional};
    [
        AjusteUsuario::Refresh {
            alvo: "alvo-estavel".into(),
            antes: RefreshRacional {
                numerador: 60,
                denominador: 1,
            },
            depois: RefreshRacional {
                numerador: 144_000,
                denominador: 1001,
            },
        },
        AjusteUsuario::MouseAceleracao {
            antes: MouseSpi {
                limiar1: 6,
                limiar2: 10,
                aceleracao: 1,
            },
            depois: MouseSpi {
                limiar1: 6,
                limiar2: 10,
                aceleracao: 0,
            },
        },
        AjusteUsuario::DxSwapEffectUpgrade {
            antes: None,
            depois: Some("1".into()),
        },
        AjusteUsuario::GameDvrCapturaHistorica {
            antes: Some(1),
            depois: Some(0),
        },
    ]
}

fn adiamento(acao: AcaoAdiada) -> Adiamento {
    Adiamento {
        processos: vec!["cs2.exe".into(), "EasyAntiCheat_EOS.exe".into()],
        desde_unix: 1_790_000_500,
        acao,
        lista_indisponivel: false,
    }
}

#[test]
fn formato_json_v4() {
    use serde_json::{from_value, json, to_value};
    let a = to_value(adiamento(AcaoAdiada::RestaurarOriginal)).unwrap();
    assert_eq!(
        a,
        json!({
            "processos": ["cs2.exe", "EasyAntiCheat_EOS.exe"],
            "desde_unix": 1_790_000_500,
            "acao": "restaurar_original",
            "lista_indisponivel": false
        })
    );
    for (acao, s) in [
        (AcaoAdiada::AplicarPlano, "aplicar_plano"),
        (AcaoAdiada::TratarDeriva, "tratar_deriva"),
        (AcaoAdiada::RestaurarOriginal, "restaurar_original"),
    ] {
        assert_eq!(to_value(acao).unwrap(), json!(s));
        assert_eq!(from_value::<AcaoAdiada>(json!(s)).unwrap(), acao);
    }
    let mut extra = a;
    extra["extra"] = json!(1);
    assert!(from_value::<Adiamento>(extra).is_err());
    assert_eq!(
        to_value(Resposta::Restauracao { adiada: None }).unwrap(),
        json!({"tipo": "restauracao", "adiada": null})
    );
    let mut sem_lista = adiamento(AcaoAdiada::AplicarPlano);
    sem_lista.processos.clear();
    sem_lista.lista_indisponivel = true;
    assert_eq!(
        to_value(Resposta::Adiada {
            adiamento: sem_lista
        })
        .unwrap(),
        json!({
            "tipo": "adiada",
            "adiamento": {
                "processos": [],
                "desde_unix": 1_790_000_500,
                "acao": "aplicar_plano",
                "lista_indisponivel": true
            }
        })
    );
    assert_eq!(
        to_value(MotivoNaoAplicada::VrrInativo).unwrap(),
        json!({"tipo": "vrr_inativo"})
    );
    assert_eq!(
        to_value(MotivoNaoAplicada::JogoComReflex).unwrap(),
        json!({"tipo": "jogo_com_reflex"})
    );
    assert_eq!(
        to_value(Vrr {
            possivel: Some(true),
            ativo: None
        })
        .unwrap(),
        json!({"possivel": true, "ativo": null})
    );
    // Capacidades da v3 (`vrr` booleano) não são aceitas pela v4.
    assert!(
        from_value::<Capacidades>(json!({
            "dlss": true, "geracao_quadros": false, "reflex": true,
            "rebar": false, "vrr": false
        }))
        .is_err()
    );
    let j = to_value(jogo_exemplo()).unwrap();
    assert_eq!(j["competitivo"], json!("cs2"));
    assert_eq!(j["em_execucao"], json!(true));
    assert_eq!(
        j["orientacoes"],
        json!(["reflex_on_boost", "vsync_jogo_off"])
    );
    // Jogo da v3 (sem `em_execucao`/`orientacoes`) não é aceito pela v4.
    let mut v3 = j;
    let o = v3.as_object_mut().unwrap();
    o.remove("competitivo");
    o.remove("em_execucao");
    o.remove("orientacoes");
    assert!(from_value::<Jogo>(v3).is_err());
}

#[test]
fn mensagens_da_v4_a_v7_sao_recusadas() {
    for (versao, payload) in [
        (7, &br#"{"versao":7,"corpo":{"tipo":"obter_estado"}}"#[..]),
        (6, br#"{"versao":6,"corpo":{"tipo":"obter_estado"}}"#),
        (6, br#"{"versao":6,"corpo":{"tipo":"ok"}}"#),
        (6, br#"{"versao":6,"corpo":{"tipo":"formatar_disco"}}"#),
        (4, br#"{"versao":4,"corpo":{"tipo":"obter_estado"}}"#),
        (4, br#"{"versao":4,"corpo":{"tipo":"ok"}}"#),
        (5, br#"{"versao":5,"corpo":{"tipo":"obter_estado"}}"#),
        (5, br#"{"versao":5,"corpo":{"tipo":"ok"}}"#),
        // Corpo inválido: a recusa vem do envelope, antes do corpo.
        (5, br#"{"versao":5,"corpo":{"tipo":"formatar_disco"}}"#),
    ] {
        assert!(matches!(
            decodificar::<Requisicao>(payload),
            Err(crate::ErroIpc::VersaoIncompativel {
                recebida,
                esperada: 8
            }) if recebida == versao
        ));
        assert!(matches!(
            decodificar::<Resposta>(payload),
            Err(crate::ErroIpc::VersaoIncompativel { recebida, .. }) if recebida == versao
        ));
    }
}

fn atualizacao_exemplo() -> EstadoAtualizacao {
    EstadoAtualizacao {
        versao_instalada: "1.4.0".into(),
        ultima_verificacao_unix: Some(1_790_000_000),
        proxima_verificacao_unix: Some(1_790_021_600),
        ultima_atualizacao: Some(AtualizacaoConcluida {
            de: "1.3.2".into(),
            para: "1.4.0".into(),
            quando_unix: 1_789_990_000,
            reinicio_pendente: true,
        }),
        nao_vista: true,
        pendente: Some(AtualizacaoPendente {
            versao: "1.5.0".into(),
            motivo: MotivoPendente::JogoAberto,
            desde_unix: 1_790_000_100,
        }),
        falha: Some(FalhaAtualizacao {
            versao: "1.5.0".into(),
            codigo: 1603,
            tentativas: 2,
            abandonada: false,
            quando_unix: 1_790_000_050,
        }),
        erro_verificacao: Some(ErroVerificacao::RecusadoHttp(403)),
    }
}

#[test]
fn formato_json_v6() {
    use serde_json::{from_value, json, to_value};
    let a = to_value(atualizacao_exemplo()).unwrap();
    assert_eq!(
        a,
        json!({
            "versao_instalada": "1.4.0",
            "ultima_verificacao_unix": 1_790_000_000u64,
            "proxima_verificacao_unix": 1_790_021_600u64,
            "ultima_atualizacao": {
                "de": "1.3.2", "para": "1.4.0",
                "quando_unix": 1_789_990_000u64, "reinicio_pendente": true
            },
            "nao_vista": true,
            "pendente": {
                "versao": "1.5.0", "motivo": "jogo_aberto", "desde_unix": 1_790_000_100u64
            },
            "falha": {
                "versao": "1.5.0", "codigo": 1603, "tentativas": 2,
                "abandonada": false, "quando_unix": 1_790_000_050u64
            },
            "erro_verificacao": {"tipo": "recusado_http", "status": 403}
        })
    );
    assert_eq!(
        from_value::<EstadoAtualizacao>(a.clone()).unwrap(),
        atualizacao_exemplo()
    );
    let mut extra = a.clone();
    extra["extra"] = json!(1);
    assert!(from_value::<EstadoAtualizacao>(extra).is_err());
    let mut extra = a;
    extra["falha"]["extra"] = json!(1);
    assert!(from_value::<EstadoAtualizacao>(extra).is_err());
    for (m, s) in [
        (MotivoPendente::JogoAberto, "jogo_aberto"),
        (MotivoPendente::ListaIndisponivel, "lista_indisponivel"),
        (MotivoPendente::OutraInstalacao, "outra_instalacao"),
        (MotivoPendente::Baixando, "baixando"),
        (MotivoPendente::Instalando, "instalando"),
    ] {
        assert_eq!(to_value(m).unwrap(), json!(s));
        assert_eq!(from_value::<MotivoPendente>(json!(s)).unwrap(), m);
    }
    for (e, j) in [
        (ErroVerificacao::SemConexao, json!({"tipo": "sem_conexao"})),
        (
            ErroVerificacao::RecusadoHttp(404),
            json!({"tipo": "recusado_http", "status": 404}),
        ),
        (ErroVerificacao::Assinatura, json!({"tipo": "assinatura"})),
        (ErroVerificacao::Manifesto, json!({"tipo": "manifesto"})),
        (ErroVerificacao::Disco, json!({"tipo": "disco"})),
    ] {
        assert_eq!(to_value(e).unwrap(), j);
        assert_eq!(from_value::<ErroVerificacao>(j).unwrap(), e);
    }
    assert_eq!(
        to_value(EstadoAtualizacao::default()).unwrap(),
        json!({
            "versao_instalada": "", "ultima_verificacao_unix": null,
            "proxima_verificacao_unix": null, "ultima_atualizacao": null,
            "nao_vista": false, "pendente": null, "falha": null,
            "erro_verificacao": null
        })
    );
    assert_eq!(
        to_value(Requisicao::MarcarAtualizacaoVista {
            versao: "1.4.0".into()
        })
        .unwrap(),
        json!({"tipo": "marcar_atualizacao_vista", "versao": "1.4.0"})
    );
    assert!(
        from_value::<Requisicao>(json!({
            "tipo": "marcar_atualizacao_vista", "versao": "1.4.0", "sid": "S-1-5-18"
        }))
        .is_err()
    );
    assert_eq!(
        to_value(CodigoErro::EntradaInvalida).unwrap(),
        json!("entrada_invalida")
    );
}

#[test]
fn ida_e_volta_de_todas_as_requisicoes() {
    for r in todas_requisicoes() {
        let bytes = codificar(&r).unwrap();
        assert_eq!(decodificar::<Requisicao>(&bytes).unwrap(), r);
    }
}

#[test]
fn ida_e_volta_de_todas_as_respostas() {
    for r in todas_respostas() {
        let bytes = codificar(&r).unwrap();
        assert_eq!(decodificar::<Resposta>(&bytes).unwrap(), r);
    }
}

#[test]
fn ida_e_volta_de_todos_os_enums_simples() {
    use serde_json::{from_value, to_value};
    for c in [
        CodigoErro::VersaoIncompativel,
        CodigoErro::MensagemGrande,
        CodigoErro::MensagemInvalida,
        CodigoErro::RequisicaoInvalida,
        CodigoErro::IdentidadeIndisponivel,
        CodigoErro::SettingNaoPermitida,
        CodigoErro::PerfilNaoEncontrado,
        CodigoErro::DriverIndisponivel,
        CodigoErro::Ocupado,
        CodigoErro::Interno,
    ] {
        assert_eq!(from_value::<CodigoErro>(to_value(c).unwrap()).unwrap(), c);
    }
    for s in [
        StatusMudanca::Pendente,
        StatusMudanca::Aplicada,
        StatusMudanca::JaNoValor,
        StatusMudanca::Travada,
        StatusMudanca::Recusada,
        StatusMudanca::Falhou,
    ] {
        assert_eq!(
            from_value::<StatusMudanca>(to_value(s).unwrap()).unwrap(),
            s
        );
    }
    for i in [
        ImpactoVisual::Nenhum,
        ImpactoVisual::Baixo,
        ImpactoVisual::Medio,
        ImpactoVisual::Alto,
    ] {
        assert_eq!(
            from_value::<ImpactoVisual>(to_value(i).unwrap()).unwrap(),
            i
        );
    }
    for a in [
        AcaoHistorico::Aplicacao,
        AcaoHistorico::Restauracao,
        AcaoHistorico::Rollback,
        AcaoHistorico::Travamento,
        AcaoHistorico::Destravamento,
        AcaoHistorico::Preferencias,
        AcaoHistorico::DerivaDetectada,
    ] {
        assert_eq!(
            from_value::<AcaoHistorico>(to_value(a).unwrap()).unwrap(),
            a
        );
    }
}

#[test]
fn formato_json_estavel() {
    let json = String::from_utf8(
        codificar(&Requisicao::TravarSetting {
            perfil: "WARDOGS".into(),
            setting: "dlss_preset".into(),
        })
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        json,
        r#"{"versao":8,"corpo":{"tipo":"travar_setting","perfil":"WARDOGS","setting":"dlss_preset"}}"#
    );
    let json = String::from_utf8(codificar(&Resposta::Ok).unwrap()).unwrap();
    assert_eq!(json, r#"{"versao":8,"corpo":{"tipo":"ok"}}"#);
}

#[test]
fn formato_json_de_aviso_reversao_e_ignorado() {
    use serde_json::{json, to_value};
    let a = Aviso::snapshot_indisponivel("x", 7);
    assert_eq!(
        to_value(&a).unwrap()["nivel"],
        json!("erro"),
        "nível em snake_case"
    );
    assert_eq!(
        to_value(&a).unwrap()["codigo"],
        json!("snapshot_indisponivel")
    );
    assert_eq!(to_value(NivelAviso::Atencao).unwrap(), json!("atencao"));
    for n in [NivelAviso::Info, NivelAviso::Atencao, NivelAviso::Erro] {
        assert_eq!(
            serde_json::from_value::<NivelAviso>(to_value(n).unwrap()).unwrap(),
            n
        );
    }
    for (acao, s) in [
        (AcaoReversao::GravarOriginal, "gravar_original"),
        (AcaoReversao::Remover, "remover"),
        (AcaoReversao::SoDeixarDeVigiar, "so_deixar_de_vigiar"),
    ] {
        assert_eq!(to_value(acao).unwrap(), json!(s));
        assert_eq!(
            serde_json::from_value::<AcaoReversao>(json!(s)).unwrap(),
            acao
        );
    }
    let j = to_value(JogoIgnorado {
        nome: "A".into(),
        exe: "a.exe".into(),
        motivo: "m".into(),
    })
    .unwrap();
    assert_eq!(j, json!({"nome":"A","exe":"a.exe","motivo":"m"}));
    // Campo desconhecido continua recusado (mudança incompatível = nova versão).
    assert!(
        serde_json::from_value::<Aviso>(json!({
            "nivel":"info","codigo":"x","mensagem":"m","detalhe":null,"quando_unix":0,"extra":1
        }))
        .is_err()
    );
}

#[test]
fn avisos_ordenados_e_restauracao_bloqueada() {
    let mut v = vec![
        Aviso::novo(NivelAviso::Info, "a", "a", None, 5),
        Aviso::erro_perfil("p", false, 1),
        Aviso::jogos_ignorados(
            &[JogoIgnorado {
                nome: "J".into(),
                exe: "j.exe".into(),
                motivo: "nome inválido".into(),
            }],
            9,
        ),
        Aviso::snapshot_indisponivel("s", 3),
    ];
    ordenar_avisos(&mut v);
    let codigos: Vec<&str> = v.iter().map(|a| a.codigo.as_str()).collect();
    assert_eq!(
        codigos,
        [
            codigo_aviso::SNAPSHOT_INDISPONIVEL,
            codigo_aviso::ERRO_PERFIL_APLICACAO,
            codigo_aviso::JOGOS_IGNORADOS,
            "a"
        ]
    );
    assert_eq!(v[2].detalhe.as_deref(), Some("J: nome inválido"));
    assert!(v[2].mensagem.starts_with("1 jogo ficou"));
    let mut e = match &todas_respostas()[1] {
        Resposta::Estado(e) => e.clone(),
        _ => unreachable!(),
    };
    assert!(e.restauracao_bloqueada().is_none());
    e.avisos = v;
    assert_eq!(
        e.restauracao_bloqueada().map(|a| a.nivel),
        Some(NivelAviso::Erro)
    );
}

#[test]
fn validacao_de_forma() {
    for r in todas_requisicoes() {
        assert_eq!(r.validar(), Ok(()), "{r:?}");
    }
    let invalidas = [
        Requisicao::MarcarInicioVisto {
            fonte: crate::FonteInicio::RunUsuario,
            nome: r"C:\Users\fulano\app".into(),
        },
        Requisicao::MarcarInicioVisto {
            fonte: crate::FonteInicio::RunUsuario,
            nome: String::new(),
        },
        Requisicao::ListarPartidas {
            desde_unix: 0,
            limite: 0,
        },
        Requisicao::ListarPartidas {
            desde_unix: 0,
            limite: PARTIDAS_MAX + 1,
        },
        Requisicao::AgenteRelatar {
            resultados_jogo: vec![],
            saude: Some(crate::SaudeSessao {
                inicializacao: vec![crate::ProgramaInicio {
                    executavel: r"C:\Users\fulano\a.exe".into(),
                    ..crate::saude::testes::programa()
                }],
                ..Default::default()
            }),
            id_tarefa: 1,
            operacao: OperacaoUsuario::Aplicar,
            revisao: 1,
            resultados: vec![],
        },
        Requisicao::MarcarAtualizacaoVista {
            versao: String::new(),
        },
        Requisicao::MarcarAtualizacaoVista {
            versao: "1.0/../x".into(),
        },
        Requisicao::DefinirAceiteExperimental {
            regra: "regra/1".into(),
            aceito: true,
        },
        Requisicao::ReverterAjusteUsuario {
            alvo_tela: None,
            destino: SettingUsuario::Refresh {
                alvo: String::new(),
            },
        },
        Requisicao::ReverterAjusteUsuario {
            alvo_tela: Some("outro-monitor".into()),
            destino: SettingUsuario::Refresh {
                alvo: "monitor-do-plano".into(),
            },
        },
        Requisicao::AgenteRelatar {
            resultados_jogo: vec![],
            saude: None,
            id_tarefa: 1,
            operacao: OperacaoUsuario::Aplicar,
            revisao: 7,
            resultados: vec![ResultadoUsuario {
                destino: SettingUsuario::MouseAceleracao,
                status: StatusMudanca::Aplicada,
                detalhe: None,
                mudanca: Some(crate::AjusteUsuario::GameDvrCapturaHistorica {
                    antes: Some(1),
                    depois: Some(0),
                }),
            }],
        },
        Requisicao::ListarHistorico { limite: 0 },
        Requisicao::ListarHistorico {
            limite: HISTORICO_MAX + 1,
        },
        Requisicao::DefinirPreferencias {
            sincronismo: Sincronismo::Desligado,
            qualidade: Qualidade::Equilibrado,
            limite_fps: Some(5),
        },
        Requisicao::TravarSetting {
            perfil: String::new(),
            setting: "vsync".into(),
        },
        Requisicao::TravarSetting {
            perfil: "a\u{0}b".into(),
            setting: "vsync".into(),
        },
        Requisicao::TravarSetting {
            perfil: "x".repeat(PERFIL_MAX + 1),
            setting: "vsync".into(),
        },
        Requisicao::DestravarSetting {
            perfil: "global".into(),
            setting: "0x10A879CF".into(),
        },
        Requisicao::DestravarSetting {
            perfil: "global".into(),
            setting: "a".repeat(SETTING_MAX + 1),
        },
        Requisicao::DestravarSetting {
            perfil: "global".into(),
            setting: "VSync".into(),
        },
    ];
    for r in invalidas {
        assert!(r.validar().is_err(), "{r:?}");
    }
}
