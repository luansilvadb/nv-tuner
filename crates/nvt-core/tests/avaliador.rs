//! Testes do avaliador e da carga das regras reais de `regras/`.

use nvt_core::{
    Alvo, Api, Arquitetura, Capabilities, ConjuntoRegras, EscolhaUsuario, EstadoVrr, GameFacts,
    Origem, Plan, Preferences, Qualidade, Sincronismo, Versao, avaliar,
};

fn regras() -> ConjuntoRegras {
    ConjuntoRegras::embutido().unwrap_or_else(|e| panic!("regras reais inválidas:\n{e}"))
}

fn jogo_dlss(id: &str) -> GameFacts {
    GameFacts {
        id: id.into(),
        api: Api::Dx12,
        dlss: Some(Versao::new(310, 6, 0, 0)),
        reflex: Some(true),
        anticheat_kernel: Some(true),
        ..GameFacts::default()
    }
}

fn jogo_sem_dlss(id: &str) -> GameFacts {
    GameFacts {
        id: id.into(),
        api: Api::Dx11,
        dlss: None,
        reflex: Some(false),
        anticheat_kernel: Some(true),
        ..GameFacts::default()
    }
}

fn gtx16() -> Capabilities {
    Capabilities {
        tem_tensor_cores: false,
        ..Capabilities::tipicas(Arquitetura::Turing)
    }
}

fn plano(caps: &Capabilities, jogos: &[GameFacts], prefs: &Preferences) -> Plan {
    avaliar(caps, jogos, prefs, &regras())
}

fn jogo(id: &str) -> Alvo {
    Alvo::Jogo(id.into())
}

fn valor(plan: &Plan, alvo: &Alvo, setting: &str) -> Option<u32> {
    plan.mudanca(alvo, setting).map(|m| m.valor)
}

fn tem_preset_e(plan: &Plan, id: &str) -> bool {
    let a = jogo(id);
    valor(plan, &a, "dlss_override") == Some(1)
        && valor(plan, &a, "dlss_perfil_preset") == Some(2)
        && valor(plan, &a, "dlss_preset") == Some(5)
}

/// O perfil do jogo só recebe a energia por jogo (F1-11), que vale para todo
/// jogo: o que antes era "nenhuma mudança por jogo".
fn so_energia(plan: &Plan, id: &str) -> bool {
    let m = plan.mudancas(&jogo(id));
    m.len() == 1 && m[0].setting == "energia"
}

fn por_jogo_so_energia(plan: &Plan) -> bool {
    plan.por_jogo.keys().all(|id| so_energia(plan, id))
}

#[test]
fn carrega_regras_reais() {
    let r = regras();
    // Spec competitivo §7.4/F1-11: energia global trocada pela por jogo (mesma
    // contagem) e nova `gsync-reflex-sem-limite-driver` (+1).
    assert_eq!(r.regras.len(), 12);
    assert!(r.catalogo.len() >= 21);
    let ids = r.catalogo.ids_permitidos();
    for id in [0x10E41E01, 0x00634291, 0x10E41DF3, 0x00A879CF, 0x10835002] {
        assert!(ids.contains(&id), "ID 0x{id:08X} fora do catálogo");
    }
}

#[test]
fn turing_com_dlss_recebe_preset_e() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let plan = plano(&caps, &[jogo_dlss("jogo-a")], &Preferences::default());
    assert!(tem_preset_e(&plan, "jogo-a"));
    let m = plan.mudanca(&jogo("jogo-a"), "dlss_preset").unwrap();
    assert_eq!(m.id, 0x10E41DF3);
    assert_eq!(m.valor_nome.as_deref(), Some("e"));
    assert_eq!(
        m.origem,
        Origem::Regra {
            id: "dlss-preset-e-turing".into()
        }
    );
    assert!(!m.explicacao.is_empty());
    assert!(plan.experimentais.is_empty());
}

#[test]
fn ampere_so_gera_sugestao_experimental() {
    let caps = Capabilities::tipicas(Arquitetura::Ampere);
    let plan = plano(&caps, &[jogo_dlss("jogo-a")], &Preferences::default());
    assert!(so_energia(&plan, "jogo-a"));
    assert_eq!(plan.experimentais.len(), 1);
    let s = &plan.experimentais[0];
    assert_eq!(s.regra, "dlss-preset-e-ampere");
    assert_eq!(s.alvo, jogo("jogo-a"));
    assert_eq!(s.mudancas.len(), 3);
}

#[test]
fn ada_e_blackwell_nao_recebem_override() {
    for arq in [Arquitetura::Ada, Arquitetura::Blackwell] {
        let caps = Capabilities::tipicas(arq);
        let plan = plano(&caps, &[jogo_dlss("jogo-a")], &Preferences::default());
        assert!(por_jogo_so_energia(&plan), "{arq}: {:?}", plan.por_jogo);
        assert!(plan.experimentais.is_empty());
        // F1-11: energia saiu do global e vale no perfil do jogo.
        assert_eq!(valor(&plan, &Alvo::Global, "energia"), None);
        assert_eq!(valor(&plan, &jogo("jogo-a"), "energia"), Some(1));
    }
}

#[test]
fn gtx16_sem_tensor_cores_nao_recebe_override() {
    let plan = plano(&gtx16(), &[jogo_dlss("jogo-a")], &Preferences::default());
    assert!(por_jogo_so_energia(&plan));
    assert!(plan.experimentais.is_empty());
}

#[test]
fn arquitetura_desconhecida_nao_recebe_override() {
    let caps = Capabilities {
        tem_tensor_cores: true,
        ..Capabilities::tipicas(Arquitetura::Unknown(0x1C0))
    };
    let plan = plano(&caps, &[jogo_dlss("jogo-a")], &Preferences::default());
    assert!(por_jogo_so_energia(&plan));
}

#[test]
fn jogo_sem_dlss_nao_recebe_override() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let plan = plano(
        &caps,
        &[jogo_sem_dlss("jogo-b"), jogo_dlss("jogo-a")],
        &Preferences::default(),
    );
    assert!(so_energia(&plan, "jogo-b"));
    assert!(tem_preset_e(&plan, "jogo-a"));
}

#[test]
fn dlss_desconhecido_conta_como_sem_dlss() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let plan = plano(&caps, &[GameFacts::new("jogo-c")], &Preferences::default());
    assert!(por_jogo_so_energia(&plan));
}

#[test]
fn fidelidade_bloqueia_preset_e() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let prefs = Preferences {
        qualidade: Qualidade::Fidelidade,
        ..Preferences::default()
    };
    let plan = plano(&caps, &[jogo_dlss("jogo-a")], &prefs);
    assert!(por_jogo_so_energia(&plan));
    assert!(plan.descartes.iter().any(|d| {
        d.regra.as_deref() == Some("dlss-preset-e-turing")
            && d.alvo == jogo("jogo-a")
            && d.motivo.contains("teto")
    }));
    // Regras sem impacto visual continuam.
    assert_eq!(valor(&plan, &Alvo::Global, "low_latency_cpl"), Some(2));
}

#[test]
fn desempenho_permite_preset_e() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let prefs = Preferences {
        qualidade: Qualidade::Desempenho,
        ..Preferences::default()
    };
    let plan = plano(&caps, &[jogo_dlss("jogo-a")], &prefs);
    assert!(tem_preset_e(&plan, "jogo-a"));
}

#[test]
fn baseline_global_completo() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let plan = plano(&caps, &[], &Preferences::default());
    let g = Alvo::Global;
    assert_eq!(valor(&plan, &g, "low_latency_cpl"), Some(2));
    assert_eq!(valor(&plan, &g, "low_latency_enabled"), Some(1));
    // F1-11: energia não é mais global (regra por jogo).
    assert_eq!(valor(&plan, &g, "energia"), None);
    assert_eq!(valor(&plan, &g, "shader_cache"), Some(1));
    assert_eq!(valor(&plan, &g, "indicador_vrr"), Some(0));
    assert_eq!(valor(&plan, &g, "af_modo"), Some(0));
    assert_eq!(valor(&plan, &g, "af_otimizacao"), Some(0));
    assert_eq!(valor(&plan, &g, "af_otimizacao_amostras"), Some(0));
    assert_eq!(valor(&plan, &g, "aa_modo"), Some(0));
    // Ordenado por setting.
    let nomes: Vec<&str> = plan.global.iter().map(|m| m.setting.as_str()).collect();
    let mut ordenados = nomes.clone();
    ordenados.sort_unstable();
    assert_eq!(nomes, ordenados);
}

#[test]
fn sem_sync_desliga_vsync_e_gsync() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let plan = plano(&caps, &[], &Preferences::default());
    let g = Alvo::Global;
    assert_eq!(valor(&plan, &g, "vsync"), Some(0x08416747));
    assert_eq!(valor(&plan, &g, "gsync_global"), Some(0));
    assert_eq!(valor(&plan, &g, "gsync_app_state"), Some(1));
    assert_eq!(valor(&plan, &g, "limite_fps"), None);
    let m = plan.mudanca(&g, "vsync").unwrap();
    assert_eq!(
        m.origem,
        Origem::Preferencia {
            preferencia: "sincronismo".into(),
            regra: Some("sincronismo-sem-sync".into())
        }
    );
}

#[test]
fn gsync_com_refresh_conhecido_limita_abaixo_do_refresh() {
    // Spec competitivo §7.4: `sincronismo-gsync-limite` passou a exigir VRR
    // ativo, usa `limite_vrr` (224 a 240 Hz, não mais 237) e é experimental:
    // vira sugestão, não entra no plano automático.
    let caps = Capabilities {
        refresh_mhz: Some(239_760),
        vrr: EstadoVrr {
            possivel: Some(true),
            ativo: Some(true),
        },
        ..Capabilities::tipicas(Arquitetura::Ada)
    };
    let prefs = Preferences {
        sincronismo: Sincronismo::GSync,
        ..Preferences::default()
    };
    let plan = plano(&caps, &[], &prefs);
    let g = Alvo::Global;
    assert_eq!(valor(&plan, &g, "gsync_global"), Some(1));
    assert_eq!(valor(&plan, &g, "gsync_app_mode"), Some(2));
    assert_eq!(valor(&plan, &g, "gsync_app_state"), Some(0));
    assert_eq!(valor(&plan, &g, "vsync"), Some(0x47814940));
    assert_eq!(valor(&plan, &g, "limite_fps"), None);
    let s = plan
        .experimentais
        .iter()
        .find(|s| s.regra == "sincronismo-gsync-limite")
        .expect("sugestão do limite para VRR");
    assert_eq!(s.alvo, g);
    assert_eq!(s.mudancas.len(), 1);
    assert_eq!(s.mudancas[0].valor, 224);
    assert!(s.mudancas[0].explicacao.contains("240 Hz"));
}

#[test]
fn gsync_sem_refresh_nao_grava_limite() {
    let caps = Capabilities::tipicas(Arquitetura::Ada);
    let prefs = Preferences {
        sincronismo: Sincronismo::GSync,
        ..Preferences::default()
    };
    let plan = plano(&caps, &[], &prefs);
    assert_eq!(valor(&plan, &Alvo::Global, "limite_fps"), None);
    assert_eq!(valor(&plan, &Alvo::Global, "gsync_global"), Some(1));
}

#[test]
fn vsync_liga_vsync_e_desliga_gsync() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let prefs = Preferences {
        sincronismo: Sincronismo::VSync,
        ..Preferences::default()
    };
    let plan = plano(&caps, &[], &prefs);
    assert_eq!(valor(&plan, &Alvo::Global, "vsync"), Some(0x47814940));
    // Escolher V-Sync depois de G-SYNC não pode deixar o VRR ligado.
    assert_eq!(valor(&plan, &Alvo::Global, "gsync_global"), Some(0));
    assert_eq!(valor(&plan, &Alvo::Global, "gsync_app_state"), Some(1));
    assert_eq!(valor(&plan, &Alvo::Global, "gsync_app_mode"), None);
}

#[test]
fn limite_do_usuario_vence_limite_da_regra() {
    let caps = Capabilities {
        refresh_mhz: Some(144_000),
        vrr: EstadoVrr {
            possivel: Some(true),
            ativo: Some(true),
        },
        ..Capabilities::tipicas(Arquitetura::Turing)
    };
    let prefs = Preferences {
        sincronismo: Sincronismo::GSync,
        limite_fps: Some(120),
        ..Preferences::default()
    };
    let plan = plano(&caps, &[], &prefs);
    let m = plan.mudanca(&Alvo::Global, "limite_fps").unwrap();
    assert_eq!(m.valor, 120);
    assert_eq!(
        m.origem,
        Origem::Preferencia {
            preferencia: "limite_fps".into(),
            regra: None
        }
    );
    // Spec competitivo §7.4: `sincronismo-gsync-limite` é experimental, então
    // não disputa a setting (antes: descartada como "suplantada"); fica só
    // como sugestão, com o valor de `limite_vrr` (138 a 144 Hz).
    assert!(
        !plan
            .descartes
            .iter()
            .any(|d| d.regra.as_deref() == Some("sincronismo-gsync-limite"))
    );
    assert!(
        plan.experimentais
            .iter()
            .any(|s| s.regra == "sincronismo-gsync-limite" && s.mudancas[0].valor == 138)
    );
}

#[test]
fn limite_do_usuario_fora_da_faixa_e_descartado() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let prefs = Preferences {
        limite_fps: Some(5),
        ..Preferences::default()
    };
    let plan = plano(&caps, &[], &prefs);
    assert_eq!(valor(&plan, &Alvo::Global, "limite_fps"), None);
    assert!(
        plan.descartes
            .iter()
            .any(|d| d.setting.as_deref() == Some("limite_fps") && d.regra.is_none())
    );
}

#[test]
fn escolha_travada_pelo_usuario_e_respeitada() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let prefs = Preferences {
        escolhas_usuario: vec![
            EscolhaUsuario {
                alvo: jogo("jogo-a"),
                setting: "dlss_preset".into(),
            },
            // F1-11: energia passou a ser por jogo; a trava acompanha o alvo
            // (antes, a trava era no global).
            EscolhaUsuario {
                alvo: jogo("jogo-a"),
                setting: "energia".into(),
            },
        ],
        ..Preferences::default()
    };
    let plan = plano(&caps, &[jogo_dlss("jogo-a")], &prefs);
    assert_eq!(valor(&plan, &jogo("jogo-a"), "dlss_preset"), None);
    assert_eq!(valor(&plan, &jogo("jogo-a"), "dlss_override"), Some(1));
    assert_eq!(valor(&plan, &jogo("jogo-a"), "energia"), None);
    assert_eq!(plan.respeitadas.len(), 2);
    assert!(
        plan.respeitadas
            .iter()
            .any(|r| r.alvo == jogo("jogo-a") && r.mudanca.setting == "dlss_preset")
    );
}

#[test]
fn plano_e_deterministico() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let jogos = [jogo_dlss("z"), jogo_sem_dlss("m"), jogo_dlss("a")];
    let a = plano(&caps, &jogos, &Preferences::default());
    let mut invertidos = jogos.clone();
    invertidos.reverse();
    let b = plano(&caps, &invertidos, &Preferences::default());
    assert_eq!(a.global, b.global);
    assert_eq!(a.por_jogo, b.por_jogo);
    // F1-11: todo jogo recebe a energia por jogo, inclusive o sem DLSS.
    assert_eq!(a.por_jogo.keys().collect::<Vec<_>>(), ["a", "m", "z"]);
}
