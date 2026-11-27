//! Testes do estado da interface.

use super::*;
use nvt_ipc::{CodigoErro, MotivoNaoAplicada, Mudanca, OrigemPerfil, PlanoPerfil, ResumoJogo};

fn mudanca(setting: &str, status: StatusMudanca) -> Mudanca {
    Mudanca {
        alvo_tela: None,
        camada: Camada::Driver,
        experimental: EstadoExperimental::Estavel,
        setting: setting.into(),
        nome_legivel: format!("Nome {setting}"),
        antes: None,
        depois: Some("Ligado".into()),
        motivo: "motivo".into(),
        regra: "regra.x".into(),
        impacto_visual: ImpactoVisual::Baixo,
        status,
    }
}

fn plano() -> Plano {
    Plano {
        sistema: Default::default(),
        sugestoes_driver: vec![],
        gerado_unix: 0,
        perfis: vec![
            PlanoPerfil {
                perfil: "global".into(),
                nome_legivel: "Perfil global".into(),
                mudancas: vec![
                    mudanca("vsync", StatusMudanca::Aplicada),
                    mudanca("energia", StatusMudanca::Pendente),
                ],
            },
            PlanoPerfil {
                perfil: "WARDOGS".into(),
                nome_legivel: "WARDOGS".into(),
                mudancas: vec![
                    mudanca("dlss_preset", StatusMudanca::JaNoValor),
                    mudanca("dlss_override", StatusMudanca::Travada),
                    mudanca("shader_cache", StatusMudanca::Falhou),
                ],
            },
            PlanoPerfil {
                perfil: "Vazio".into(),
                nome_legivel: "Vazio".into(),
                mudancas: vec![mudanca("vsync", StatusMudanca::Travada)],
            },
        ],
        reversoes: vec![],
        ignorados: vec![],
        jogos: vec![],
        adiada: None,
    }
}

fn estado(prefs: Preferencias) -> Estado {
    Estado {
        agente: Default::default(),
        diagnosticos: vec![],
        versao_servico: "0.0.1".into(),
        driver: None,
        gpu: None,
        preferencias: prefs,
        jogos: 0,
        ultima_aplicacao: None,
        alertas_deriva: vec![],
        avisos: vec![],
        adiamento: None,
        atualizacao: Default::default(),
        autonomia_pausada: false,
    }
}

#[test]
fn plano_vira_grupos_e_respeitadas() {
    let (g, r) = grupos_do_plano(&plano());
    assert_eq!(g.len(), 2, "perfil só com travadas não vira grupo");
    assert_eq!(g[1].itens.len(), 2);
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].perfil, "WARDOGS");
    assert_eq!(r[0].setting, "dlss_override");
}

#[test]
fn escrita_so_com_servico_ocioso() {
    let mut ui = EstadoUi::new();
    assert!(!ui.pode_escrever(), "conectando");
    ui.aplicar(Evento::Visualizacao(
        Conexao::Visualizacao,
        Ok(Dados::default()),
    ));
    assert_eq!(ui.conexao, Conexao::Visualizacao);
    assert!(!ui.pode_escrever());
    assert!(!ui.pode_executar(&Comando::RestaurarOriginal));
    assert!(ui.pode_executar(&Comando::Reescanear));
    ui.aplicar(Evento::Servico(Dados::default()));
    assert!(ui.pode_escrever());
    ui.aplicar(Evento::Ocupado(Comando::RestaurarOriginal));
    assert!(!ui.pode_escrever());
    assert!(!ui.pode_executar(&Comando::Atualizar));
    ui.aplicar(Evento::Concluido {
        comando: Comando::RestaurarOriginal,
        resultado: Ok(Desfecho::Feito),
    });
    assert!(ui.pode_escrever());
    assert_eq!(
        ui.mensagem,
        Some((TipoMensagem::Sucesso, textos::OK_RESTAURADO.into()))
    );
}

#[test]
fn diagnostico_muda_sem_trocar_dados_locais_ate_reconectar() {
    let mut ui = EstadoUi::new();
    ui.aplicar(Evento::Visualizacao(
        Conexao::Iniciando,
        Err("sem driver".into()),
    ));
    for conexao in [
        Conexao::Iniciando,
        Conexao::SemResposta,
        Conexao::Visualizacao,
    ] {
        ui.aplicar(Evento::Conexao(conexao));
        assert_eq!(ui.conexao, conexao);
        assert_eq!(ui.erro_local.as_deref(), Some("sem driver"));
        assert!(!ui.pode_escrever());
        assert!(!ui.pode_executar(&Comando::RestaurarOriginal));
        assert!(ui.pode_executar(&Comando::Reescanear));
    }
    ui.aplicar(Evento::Servico(Dados::default()));
    assert_eq!(ui.conexao, Conexao::Servico);
    assert!(ui.erro_local.is_none());
    assert!(ui.pode_escrever());
}

#[test]
fn queda_do_servico_libera_operacao_pendente() {
    let mut ui = EstadoUi::new();
    ui.aplicar(Evento::Servico(Dados::default()));
    ui.aplicar(Evento::Ocupado(Comando::RestaurarOriginal));
    ui.aplicar(Evento::Visualizacao(
        Conexao::Visualizacao,
        Err("sem driver".into()),
    ));
    assert_eq!(ui.ocupado, None);
    assert_eq!(ui.erro_local.as_deref(), Some("sem driver"));
    assert!(!ui.pode_escrever());
}

#[test]
fn erro_aparece_como_mensagem() {
    let mut ui = EstadoUi::new();
    ui.aplicar(Evento::Concluido {
        comando: Comando::RestaurarOriginal,
        resultado: Err(textos::erro_servico(CodigoErro::Ocupado, "aguarde")),
    });
    let (tipo, texto) = ui.mensagem.clone().unwrap();
    assert_eq!(tipo, TipoMensagem::Erro);
    assert!(texto.contains("ocupado"), "{texto}");
}

#[test]
fn rascunho_de_preferencias() {
    let mut ui = EstadoUi::new();
    let prefs = Preferencias {
        sincronismo: Sincronismo::GSync,
        qualidade: Qualidade::Qualidade,
        limite_fps: Some(141),
    };
    ui.aplicar(Evento::Servico(Dados {
        estado: Some(estado(prefs.clone())),
        ..Default::default()
    }));
    assert_eq!(ui.rascunho, prefs, "rascunho segue o serviço");
    ui.rascunho.qualidade = Qualidade::Desempenho;
    assert!(ui.rascunho_alterado());
    // Atualização do serviço com as mesmas preferências não apaga a edição.
    ui.aplicar(Evento::Servico(Dados {
        estado: Some(estado(prefs.clone())),
        ..Default::default()
    }));
    assert_eq!(ui.rascunho.qualidade, Qualidade::Desempenho);
    ui.aplicar(Evento::Concluido {
        comando: Comando::DefinirPreferencias(ui.rascunho.clone()),
        resultado: Ok(Desfecho::Feito),
    });
    assert!(!ui.rascunho_alterado());
}

#[test]
fn escolha_feita_durante_o_envio_nao_se_perde() {
    let mut ui = EstadoUi::new();
    let prefs = preferencias_padrao();
    ui.aplicar(Evento::Servico(Dados {
        estado: Some(estado(prefs.clone())),
        ..Default::default()
    }));
    ui.rascunho.qualidade = Qualidade::Desempenho;
    let enviadas = ui.rascunho.clone();
    // Nova escolha antes da resposta do envio anterior.
    ui.rascunho.qualidade = Qualidade::Qualidade;
    ui.aplicar(Evento::Concluido {
        comando: Comando::DefinirPreferencias(enviadas.clone()),
        resultado: Ok(Desfecho::Feito),
    });
    assert_eq!(ui.rascunho.qualidade, Qualidade::Qualidade);
    // Falha devolve ao estado do serviço.
    ui.aplicar(Evento::Concluido {
        comando: Comando::DefinirPreferencias(enviadas),
        resultado: Err("falhou".into()),
    });
    assert!(!ui.rascunho_alterado());
}

fn jogo(nome: &str, resumo: ResumoJogo) -> Jogo {
    Jogo {
        config_jogo: None,
        nome: nome.into(),
        perfil: nome.into(),
        origem_perfil: OrigemPerfil::Usuario,
        launcher: "Steam".into(),
        api: "dx12".into(),
        dlss: Some("310.6".into()),
        reflex: Some(true),
        anticheat_kernel: None,
        competitivo: None,
        em_execucao: false,
        orientacoes: vec![],
        resumo,
    }
}

#[test]
fn jogos_vem_do_servico_ou_do_calculo_local() {
    let mut ui = EstadoUi::new();
    ui.aplicar(Evento::Visualizacao(
        Conexao::Visualizacao,
        Ok(Dados {
            jogos: vec![jogo("local", ResumoJogo::default())],
            ..Default::default()
        }),
    ));
    assert_eq!(ui.jogos()[0].nome, "local");
    ui.aplicar(Evento::Servico(Dados {
        jogos: vec![
            jogo("Zeta", ResumoJogo::default()),
            jogo("alfa", ResumoJogo::default()),
        ],
        ..Default::default()
    }));
    let nomes: Vec<String> = ui.jogos().iter().map(|j| j.nome.clone()).collect();
    assert_eq!(nomes, ["alfa", "Zeta"], "do serviço, em ordem alfabética");
}

#[test]
fn versao_incompativel_mostra_instrucao_ate_reconectar() {
    let mut ui = EstadoUi::new();
    let m = textos::versao_incompativel(3, 2);
    assert!(m.contains("v3") && m.contains("v2"), "{m}");
    assert!(m.contains("mais novo"));
    assert!(m.contains("Atualize o nv-tuner"));
    assert!(textos::versao_incompativel(1, 2).contains("mais antigo"));
    ui.aplicar(Evento::Incompativel(Some(m.clone())));
    ui.aplicar(Evento::Visualizacao(
        Conexao::Visualizacao,
        Ok(Dados::default()),
    ));
    assert_eq!(ui.incompativel.as_deref(), Some(m.as_str()));
    ui.aplicar(Evento::Servico(Dados::default()));
    assert_eq!(ui.incompativel, None, "serviço compatível limpa o aviso");
}

#[test]
fn queda_durante_upgrade_vira_aviso_de_reabrir() {
    let versao_ui = "1.1.0";
    let com = |versao: &str, motivo: Option<MotivoPendente>| Estado {
        versao_servico: versao.into(),
        atualizacao: EstadoAtualizacao {
            pendente: motivo.map(|motivo| nvt_ipc::AtualizacaoPendente {
                versao: "1.2.0".into(),
                motivo,
                desde_unix: 0,
            }),
            ..Default::default()
        },
        ..estado(preferencias_padrao())
    };
    let mesma = com(versao_ui, None);
    let instalando = com(versao_ui, Some(MotivoPendente::Instalando));
    let baixando = com(versao_ui, Some(MotivoPendente::Baixando));
    let jogo = com(versao_ui, Some(MotivoPendente::JogoAberto));
    // Pipe caiu: só é atualização se estava baixando/instalando.
    assert!(upgrade_detectado(Some(&instalando), None, versao_ui));
    assert!(upgrade_detectado(Some(&baixando), None, versao_ui));
    assert!(!upgrade_detectado(Some(&mesma), None, versao_ui));
    assert!(!upgrade_detectado(Some(&jogo), None, versao_ui));
    assert!(!upgrade_detectado(None, None, versao_ui));
    // Serviço respondeu com outra versão.
    assert!(upgrade_detectado(
        None,
        Some(&com("1.2.0", None)),
        versao_ui
    ));
    assert!(!upgrade_detectado(None, Some(&mesma), versao_ui));
    assert!(!upgrade_detectado(None, Some(&com("", None)), versao_ui));

    let mut ui = EstadoUi::new();
    ui.aplicar(Evento::Servico(Dados {
        estado: Some(instalando),
        ..Default::default()
    }));
    ui.aplicar(Evento::Ocupado(Comando::RestaurarOriginal));
    ui.aplicar(Evento::Atualizado);
    assert!(ui.atualizado);
    assert_eq!(ui.ocupado, None);
    assert!(!ui.pode_escrever());
    assert!(!ui.pode_executar(&Comando::Atualizar));
    assert!(textos::ATUALIZADO_REABRA.contains("foi atualizado"));
    assert!(textos::ATUALIZADO_REABRA.contains("abra de novo"));
}

#[test]
fn todo_codigo_de_orientacao_tem_texto() {
    for o in nvt_core::Orientacao::TODAS {
        let texto =
            textos::orientacao(o.codigo()).unwrap_or_else(|| panic!("sem texto: {}", o.codigo()));
        assert!(texto.ends_with('.'), "{texto}");
        assert!(
            !texto.contains(o.codigo()),
            "código vazou no texto: {texto}"
        );
    }
    assert_eq!(
        textos::orientacao("reflex_on_boost"),
        Some("Ligue o NVIDIA Reflex em On + Boost (Ativado + Turbo).")
    );
    // Código desconhecido (serviço mais novo): não quebra, mostra o código.
    assert_eq!(textos::orientacao("reflex_turbo"), None);
    assert_eq!(textos::orientacao_ou_codigo("reflex_turbo"), "reflex_turbo");
}

fn adiamento(acao: nvt_ipc::AcaoAdiada) -> Adiamento {
    Adiamento {
        processos: vec!["cs2.exe".into()],
        desde_unix: 1,
        acao,
        lista_indisponivel: false,
    }
}

#[test]
fn adiamento_vira_mensagem_e_nao_erro() {
    use nvt_ipc::AcaoAdiada;
    let mut p = plano();
    assert_eq!(
        Desfecho::da_resposta(&Resposta::Plano(p.clone())),
        Desfecho::Feito
    );
    p.adiada = Some(adiamento(AcaoAdiada::AplicarPlano));
    let d = Desfecho::da_resposta(&Resposta::Plano(p));
    assert_eq!(d, Desfecho::Adiado(adiamento(AcaoAdiada::AplicarPlano)));

    let mut ui = EstadoUi::new();
    ui.aplicar(Evento::Servico(Dados::default()));
    ui.aplicar(Evento::Ocupado(Comando::RestaurarOriginal));
    ui.aplicar(Evento::Concluido {
        comando: Comando::RestaurarOriginal,
        resultado: Ok(d),
    });
    assert_eq!(
        ui.mensagem,
        Some((
            TipoMensagem::Adiada,
            "Jogo aberto (cs2.exe): as alterações serão aplicadas quando você fechar o jogo."
                .into()
        ))
    );
    assert!(ui.pode_escrever());

    let r = Desfecho::da_resposta(&Resposta::Restauracao {
        adiada: Some(adiamento(AcaoAdiada::RestaurarOriginal)),
    });
    ui.aplicar(Evento::Concluido {
        comando: Comando::RestaurarOriginal,
        resultado: Ok(r),
    });
    let (tipo, texto) = ui.mensagem.clone().unwrap();
    assert_eq!(tipo, TipoMensagem::Adiada);
    assert!(
        texto.contains("valores originais serão restaurados"),
        "{texto}"
    );
    assert_eq!(
        Desfecho::da_resposta(&Resposta::Restauracao { adiada: None }),
        Desfecho::Feito
    );
    assert!(
        textos::adiamento(&adiamento(AcaoAdiada::TratarDeriva))
            .contains("será tratada quando você fechar o jogo")
    );
}

#[test]
fn faixa_de_adiamento_vem_do_estado_sem_repetir_a_mensagem() {
    use nvt_ipc::AcaoAdiada;
    let mut ui = EstadoUi::new();
    let mut e = estado(preferencias_padrao());
    ui.aplicar(Evento::Servico(Dados {
        estado: Some(e.clone()),
        ..Default::default()
    }));
    assert_eq!(ui.faixa_adiamento(), None);
    e.adiamento = Some(adiamento(AcaoAdiada::AplicarPlano));
    ui.aplicar(Evento::Servico(Dados {
        estado: Some(e),
        ..Default::default()
    }));
    let faixa = ui.faixa_adiamento().unwrap();
    assert!(faixa.starts_with("Jogo aberto (cs2.exe):"), "{faixa}");
    ui.mensagem = Some((TipoMensagem::Adiada, faixa));
    assert_eq!(ui.mensagem_visivel(), None, "a faixa já diz o mesmo");
    ui.mensagem = Some((TipoMensagem::Sucesso, textos::OK_PREFS.into()));
    assert!(ui.mensagem_visivel().is_some());
}

/// Preferências e Destravar adiados respondem `Resposta::Adiada`: a UI
/// mostra o adiamento, não “feito”.
#[test]
fn preferencias_e_destravar_adiados_mostram_o_adiamento() {
    use nvt_ipc::AcaoAdiada;
    let r = Resposta::Adiada {
        adiamento: adiamento(AcaoAdiada::AplicarPlano),
    };
    assert_eq!(
        Desfecho::da_resposta(&r),
        Desfecho::Adiado(adiamento(AcaoAdiada::AplicarPlano))
    );
    assert_eq!(Desfecho::da_resposta(&Resposta::Ok), Desfecho::Feito);
    let mut ui = EstadoUi::new();
    ui.aplicar(Evento::Servico(Dados::default()));
    {
        let comando = Comando::DefinirPreferencias(preferencias_padrao());
        ui.aplicar(Evento::Concluido {
            comando,
            resultado: Ok(Desfecho::da_resposta(&r)),
        });
        let (tipo, texto) = ui.mensagem.clone().unwrap();
        assert_eq!(tipo, TipoMensagem::Adiada);
        assert!(texto.starts_with("Jogo aberto (cs2.exe):"), "{texto}");
        assert_ne!(texto, textos::OK_PREFS);
    }
}

/// Lista de processos indisponível: sinal próprio do protocolo, texto
/// próprio na UI (nunca “Jogo aberto (lista de processos indisponível)”).
#[test]
fn lista_de_processos_indisponivel_tem_texto_proprio() {
    use nvt_ipc::AcaoAdiada;
    let a = Adiamento {
        processos: vec![],
        desde_unix: 1,
        acao: AcaoAdiada::AplicarPlano,
        lista_indisponivel: true,
    };
    let texto = textos::adiamento(&a);
    assert_eq!(
        texto,
        "Não foi possível verificar se há jogo aberto; as alterações ficam pendentes."
    );
    assert_eq!(textos::titulo_adiamento(&texto), "Alterações pendentes");
    assert_eq!(
        textos::titulo_adiamento(&textos::adiamento(&adiamento(AcaoAdiada::AplicarPlano))),
        textos::ADIAMENTO_TITULO
    );
    let mut ui = EstadoUi::new();
    let mut e = estado(preferencias_padrao());
    e.adiamento = Some(a);
    ui.aplicar(Evento::Servico(Dados {
        estado: Some(e),
        ..Default::default()
    }));
    let faixa = ui.faixa_adiamento().unwrap();
    assert_eq!(faixa, textos::ADIAMENTO_SEM_LISTA);
    assert!(!faixa.contains("Jogo aberto"));
}

fn com_vrr(possivel: Option<bool>, ativo: Option<bool>) -> Estado {
    let mut e = estado(Preferencias {
        sincronismo: Sincronismo::GSync,
        ..preferencias_padrao()
    });
    e.gpu = Some(nvt_ipc::Gpu {
        nome: "GPU".into(),
        arquitetura: "ada".into(),
        vram_mb: 8192,
        capacidades: nvt_ipc::Capacidades {
            dlss: true,
            geracao_quadros: true,
            reflex: true,
            rebar: true,
            refresh_mhz: Some(239_757),
            vrr: nvt_ipc::Vrr { possivel, ativo },
        },
    });
    e
}

#[test]
fn aviso_de_sincronismo_segue_o_rascunho_e_o_vrr() {
    let mut ui = EstadoUi::new();
    ui.aplicar(Evento::Servico(Dados {
        estado: Some(com_vrr(Some(false), Some(false))),
        ..Default::default()
    }));
    assert_eq!(ui.aviso_sincronismo(), Some(textos::GSYNC_SEM_VRR));
    assert!(textos::GSYNC_SEM_VRR.starts_with("Seu monitor não tem G-SYNC/VRR"));
    ui.rascunho.sincronismo = Sincronismo::Desligado;
    assert_eq!(ui.aviso_sincronismo(), None, "sem G-SYNC, sem aviso");

    let mut ui = EstadoUi::new();
    ui.aplicar(Evento::Servico(Dados {
        estado: Some(com_vrr(Some(true), Some(false))),
        ..Default::default()
    }));
    assert_eq!(
        ui.aviso_sincronismo(),
        Some(textos::VRR_DESLIGADO_NO_PAINEL)
    );
    assert!(textos::VRR_DESLIGADO_NO_PAINEL.contains("Configurar G-SYNC"));

    let mut ui = EstadoUi::new();
    ui.aplicar(Evento::Servico(Dados {
        estado: Some(com_vrr(Some(true), Some(true))),
        ..Default::default()
    }));
    assert_eq!(ui.aviso_sincronismo(), None);

    // VRR desconhecido: vale o aviso do serviço.
    let mut e = com_vrr(None, None);
    e.avisos = vec![Aviso::gsync_sem_vrr(1)];
    let mut ui = EstadoUi::new();
    ui.aplicar(Evento::Servico(Dados {
        estado: Some(e),
        ..Default::default()
    }));
    assert_eq!(ui.aviso_sincronismo(), Some(textos::GSYNC_SEM_VRR));
}

#[test]
fn motivos_vrr_inativo_e_jogo_com_reflex_tem_texto_proprio() {
    let generico = textos::motivo_curto(&MotivoNaoAplicada::CondicaoNaoAtendida {
        condicao: "x".into(),
        esperado: "a".into(),
        encontrado: "b".into(),
    });
    for m in [
        MotivoNaoAplicada::VrrInativo,
        MotivoNaoAplicada::JogoComReflex,
    ] {
        assert_ne!(textos::motivo_curto(&m), generico);
    }
}

#[test]
fn comandos_de_escrita() {
    assert!(!Comando::Atualizar.escreve());
    assert!(!Comando::Reescanear.escreve());
    assert!(Comando::RestaurarOriginal.escreve());
    assert!(Comando::DefinirPreferencias(preferencias_padrao()).escreve());
}

#[test]
fn dados_de_usuario_sao_descartados_se_a_leitura_local_falhar() {
    let mut ui = EstadoUi::new();
    let mut e = estado(preferencias_padrao());
    e.agente.presente = true;
    e.agente.restauracao_pendente = true;
    let mut ajuste = mudanca("mouse_aceleracao", StatusMudanca::Aplicada);
    ajuste.camada = Camada::Usuario;
    ajuste.experimental = EstadoExperimental::Aceita;
    ui.aplicar(Evento::Servico(Dados {
        estado: Some(e),
        sistema: ResumoSistema {
            mudancas: vec![ajuste],
            sugestoes: vec![],
        },
        ..Dados::default()
    }));
    assert!(ui.restauracao_usuario_pendente());
    ui.aplicar(Evento::Concluido {
        comando: Comando::RestaurarOriginal,
        resultado: Ok(Desfecho::Feito),
    });
    assert_eq!(ui.mensagem_visivel().unwrap().1, textos::DRIVER_RESTAURADO);

    let escrita = Comando::RestaurarOriginal;
    assert!(ui.pode_executar(&escrita));
    ui.aplicar(Evento::Visualizacao(
        Conexao::Visualizacao,
        Err("sem driver".into()),
    ));
    assert_eq!(ui.dados, Dados::default());
    assert!(!ui.restauracao_usuario_pendente());
    assert!(!ui.pode_executar(&escrita));
}

#[test]
fn datas() {
    assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
    assert_eq!(ler_iso("2026-09-21T14:13:20Z"), Some(1_790_000_000));
    assert_eq!(ler_iso("2024-02-29T23:59:59.123Z"), Some(1_709_251_199));
    assert_eq!(ler_iso("2026-13-01T00:00:00Z"), None);
    assert_eq!(ler_iso("lixo"), None);
    for t in [0_i64, 951_782_400, 1_709_251_199, 4_102_444_800] {
        let (y, m, d, h, mi, s) = civil(t);
        assert_eq!(unix_de_civil(y, m, d, h, mi, s), t);
    }
}
