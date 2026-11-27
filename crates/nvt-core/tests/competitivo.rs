//! Fase competitiva 1 no núcleo:
//! `limite_vrr`, condições de VRR e de jogo curado, regras do driver revistas,
//! avisos de G-SYNC/VRR e orientações por jogo.

use nvt_core::{
    Alvo, Api, Arquitetura, AvisoPlano, Capabilities, ConjuntoRegras, Escopo, EstadoVrr, GameFacts,
    MotivoNaoAplicada, Orientacao, Plan, Preferences, SETTINGS_TOML, Sincronismo, Status, avaliar,
    limite_vrr,
};

fn regras() -> ConjuntoRegras {
    ConjuntoRegras::embutido().unwrap_or_else(|e| panic!("regras reais inválidas:\n{e}"))
}

fn plano(caps: &Capabilities, jogos: &[GameFacts], prefs: &Preferences) -> Plan {
    avaliar(caps, jogos, prefs, &regras())
}

fn jogo(id: &str) -> Alvo {
    Alvo::Jogo(id.into())
}

fn gsync() -> Preferences {
    Preferences {
        sincronismo: Sincronismo::GSync,
        ..Preferences::default()
    }
}

fn com_vrr(hz: u32, possivel: Option<bool>, ativo: Option<bool>) -> Capabilities {
    Capabilities {
        refresh_mhz: Some(hz * 1000),
        vrr: EstadoVrr { possivel, ativo },
        ..Capabilities::tipicas(Arquitetura::Ada)
    }
}

fn competitivo(id: &str, reflex: bool) -> GameFacts {
    GameFacts {
        api: Api::Dx11,
        reflex: Some(reflex),
        anticheat_kernel: Some(false),
        competitivo: true,
        limitador_nativo: Some(true),
        orientacoes: vec![
            Orientacao::ReflexOnBoost,
            Orientacao::LimitadorDoJogo,
            Orientacao::TelaCheia,
            Orientacao::VsyncJogoOff,
        ],
        ..GameFacts::new(id)
    }
}

fn sugestoes<'a>(plan: &'a Plan, regra: &str) -> Vec<&'a Alvo> {
    plan.experimentais
        .iter()
        .filter(|s| s.regra == regra)
        .map(|s| &s.alvo)
        .collect()
}

/// Tabela obrigatória de §7.4.
#[test]
fn limite_vrr_tabela_da_spec() {
    for (hz, esperado) in [
        (60, 57),
        (100, 97),
        (120, 116),
        (144, 138),
        (165, 157),
        (240, 224),
        (360, 324),
        (480, 416),
    ] {
        assert_eq!(limite_vrr(hz), Some(esperado), "{hz} Hz");
    }
    assert_eq!(limite_vrr(0), None);
    assert_eq!(limite_vrr(3), None);
}

/// O avaliador chega ao valor da tabela a partir do refresh em milésimos de
/// Hz, com o piso do refresh (59,94 Hz conta como 59, não 60).
#[test]
fn gsync_limite_usa_a_tabela_no_avaliador() {
    for (mhz, esperado) in [
        (60_000, 57),
        (59_940, 56),
        (144_000, 138),
        (143_980, 137),
        (239_760, 223),
        (480_000, 416),
    ] {
        let caps = Capabilities {
            refresh_mhz: Some(mhz),
            ..com_vrr(0, Some(true), Some(true))
        };
        let plan = plano(&caps, &[], &gsync());
        let s = plan
            .experimentais
            .iter()
            .find(|s| s.regra == "sincronismo-gsync-limite")
            .unwrap_or_else(|| panic!("{mhz} mHz: sem sugestão"));
        assert_eq!(s.mudancas[0].valor, esperado, "{mhz} mHz");
        // Experimental: nada no plano automático.
        assert!(plan.mudanca(&Alvo::Global, "limite_fps").is_none());
    }
}

#[test]
fn gsync_limite_exige_vrr_ativo_conhecido() {
    for (possivel, ativo) in [(None, None), (Some(true), Some(false))] {
        let plan = plano(&com_vrr(240, possivel, ativo), &[], &gsync());
        assert!(
            sugestoes(&plan, "sincronismo-gsync-limite").is_empty(),
            "VRR {possivel:?}/{ativo:?}"
        );
    }
}

#[test]
fn gsync_reflex_sem_limite_driver_so_em_competitivo_com_reflex() {
    let caps = com_vrr(240, Some(true), Some(true));
    let fora_da_base = GameFacts {
        reflex: Some(true),
        ..GameFacts::new("outro")
    };
    let jogos = [
        competitivo("cs2", true),
        competitivo("lol", false),
        fora_da_base,
    ];
    let plan = plano(&caps, &jogos, &gsync());
    assert_eq!(
        sugestoes(&plan, "gsync-reflex-sem-limite-driver"),
        [&jogo("cs2")]
    );
    let s = plan
        .experimentais
        .iter()
        .find(|s| s.regra == "gsync-reflex-sem-limite-driver")
        .unwrap();
    assert_eq!(s.mudancas[0].setting, "limite_fps");
    assert_eq!(s.mudancas[0].valor, 0);
    // Experimental: o perfil do jogo só recebe a energia; o resumo explica.
    let cs2 = plan.mudancas(&jogo("cs2"));
    assert_eq!(cs2.len(), 1);
    assert_eq!(cs2[0].setting, "energia");
    assert!(plan.resumos["cs2"].nao_aplicadas.iter().any(|n| {
        n.regra == "gsync-reflex-sem-limite-driver" && n.motivo == MotivoNaoAplicada::Experimental
    }));

    // Com limite do usuário, a regra não age.
    let prefs = Preferences {
        limite_fps: Some(200),
        ..gsync()
    };
    let plan = plano(&caps, &jogos, &prefs);
    assert!(sugestoes(&plan, "gsync-reflex-sem-limite-driver").is_empty());
    // Sem G-SYNC, também não.
    let plan = plano(&caps, &jogos, &Preferences::default());
    assert!(sugestoes(&plan, "gsync-reflex-sem-limite-driver").is_empty());
}

#[test]
fn energia_por_jogo_e_estavel_e_nao_fala_de_latencia() {
    let r = regras();
    let e = r
        .regras
        .iter()
        .find(|r| r.id == "energia-desempenho-maximo-por-jogo")
        .expect("regra de energia por jogo");
    assert_eq!(e.escopo, Escopo::PorJogo);
    assert_eq!(e.status, Status::Estavel);
    // §11 item 19: só jogos competitivos (`jogo_competitivo = true`).
    assert_eq!(e.quando.quantidade(), 1);
    assert_eq!(e.quando.jogo_competitivo, Some(true));
    assert!(e.motivo.contains("clock"));
    assert!(!e.motivo.to_lowercase().contains("latência"));
    assert!(
        !r.regras
            .iter()
            .any(|r| r.id == "baseline-energia-desempenho-maximo")
    );
    // A baixa latência Ultra global continua igual (§7.4, item 1).
    let ultra = r
        .regras
        .iter()
        .find(|r| r.id == "baseline-baixa-latencia-ultra")
        .unwrap();
    assert_eq!(ultra.escopo, Escopo::Global);
    assert_eq!(ultra.status, Status::Estavel);
    let p = plano(
        &Capabilities::tipicas(Arquitetura::Turing),
        &[GameFacts::new("j"), competitivo("cs2", true)],
        &Preferences::default(),
    );
    assert!(p.mudanca(&Alvo::Global, "energia").is_none());
    // Jogo competitivo recebe a energia no próprio perfil.
    assert_eq!(p.mudanca(&jogo("cs2"), "energia").map(|m| m.valor), Some(1));
    // Jogo fora da base: nem energia nem perfil só por isso.
    assert!(!p.por_jogo.contains_key("j"));
    assert!(p.resumos["j"].aplicadas.is_empty());
    assert!(p.resumos["j"].nao_aplicadas.iter().any(|n| {
        n.regra == "energia-desempenho-maximo-por-jogo"
            && matches!(
                &n.motivo,
                MotivoNaoAplicada::CondicaoNaoAtendida { condicao, .. }
                    if condicao == "jogo_competitivo"
            )
    }));
    assert_eq!(
        p.mudanca(&Alvo::Global, "low_latency_cpl").map(|m| m.valor),
        Some(2)
    );
}

#[test]
fn regras_do_driver_revistas_status_e_condicoes() {
    let r = regras();
    let lim = r
        .regras
        .iter()
        .find(|r| r.id == "sincronismo-gsync-limite")
        .unwrap();
    assert_eq!(lim.status, Status::Experimental);
    assert_eq!(lim.quando.vrr_ativo, Some(true));
    assert_eq!(lim.quando.refresh_conhecido, Some(true));
    let reflex = r
        .regras
        .iter()
        .find(|r| r.id == "gsync-reflex-sem-limite-driver")
        .unwrap();
    assert_eq!(reflex.status, Status::Experimental);
    assert_eq!(reflex.escopo, Escopo::PorJogo);
    assert_eq!(reflex.quando.jogo_competitivo, Some(true));
    assert_eq!(reflex.quando.jogo_reflex, Some(true));
    assert_eq!(reflex.quando.limite_fps_definido, Some(false));
}

#[test]
fn avisos_de_gsync_e_vrr() {
    let sem_vrr = plano(&com_vrr(144, Some(false), Some(false)), &[], &gsync());
    assert_eq!(sem_vrr.avisos, [AvisoPlano::GsyncSemVrr]);
    let desligado = plano(&com_vrr(144, Some(true), Some(false)), &[], &gsync());
    assert_eq!(desligado.avisos, [AvisoPlano::VrrDesligadoNoPainel]);
    let ok = plano(&com_vrr(144, Some(true), Some(true)), &[], &gsync());
    assert!(ok.avisos.is_empty());
    let desconhecido = plano(&com_vrr(144, None, None), &[], &gsync());
    assert!(desconhecido.avisos.is_empty());
    // Sem G-SYNC escolhido, nenhum aviso.
    let sem_sync = plano(
        &com_vrr(144, Some(false), Some(false)),
        &[],
        &Preferences::default(),
    );
    assert!(sem_sync.avisos.is_empty());
}

#[test]
fn orientacoes_filtradas_pelo_sincronismo() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let jogos = [competitivo("cs2", true), GameFacts::new("outro")];
    let p = plano(&caps, &jogos, &Preferences::default());
    assert_eq!(
        p.resumos["cs2"].orientacoes,
        [
            Orientacao::ReflexOnBoost,
            Orientacao::LimitadorDoJogo,
            Orientacao::TelaCheia,
            Orientacao::VsyncJogoOff
        ]
    );
    assert!(p.resumos["outro"].orientacoes.is_empty());
    let p = plano(&caps, &jogos, &gsync());
    assert_eq!(
        p.resumos["cs2"].orientacoes,
        [
            Orientacao::ReflexOnBoost,
            Orientacao::LimitadorDoJogo,
            Orientacao::TelaCheia,
            Orientacao::VsyncDriverComGsync
        ]
    );
}

fn regra_teste(quando: &str, escopo: &str, aplicar: &str) -> String {
    format!(
        "versao_schema = 1\n[[regra]]\nid = \"r\"\ndescricao = \"d\"\nstatus = \"estavel\"\n\
         escopo = \"{escopo}\"\nimpacto_visual = \"nenhum\"\nmotivo = \"m\"\nevidencia = \"e\"\n\
         [regra.quando]\n{quando}\n[regra.aplicar]\n{aplicar}\n"
    )
}

fn carregar(quando: &str, escopo: &str, aplicar: &str) -> Result<ConjuntoRegras, String> {
    let t = regra_teste(quando, escopo, aplicar);
    ConjuntoRegras::from_toml(SETTINGS_TOML, &[("t.toml", &t)]).map_err(|e| e.to_string())
}

#[test]
fn condicoes_novas_casam_so_com_fato_conhecido() {
    let shader = "shader_cache = \"on\"";
    // VRR: vale no global; desconhecido não casa.
    let r = carregar("vrr_possivel = true\nvrr_ativo = true", "global", shader).unwrap();
    let casa = |caps: &Capabilities| {
        avaliar(caps, &[], &Preferences::default(), &r)
            .mudanca(&Alvo::Global, "shader_cache")
            .is_some()
    };
    assert!(casa(&com_vrr(144, Some(true), Some(true))));
    assert!(!casa(&com_vrr(144, Some(true), None)));
    assert!(!casa(&com_vrr(144, None, None)));
    // Fatos curados: só por jogo.
    assert!(carregar("jogo_competitivo = true", "global", shader).is_err());
    assert!(carregar("jogo_limitador_nativo = true", "global", shader).is_err());
    let r = carregar(
        "jogo_competitivo = true\njogo_limitador_nativo = true",
        "por_jogo",
        shader,
    )
    .unwrap();
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let p = avaliar(
        &caps,
        &[competitivo("cs2", true), GameFacts::new("fora")],
        &Preferences::default(),
        &r,
    );
    assert!(p.mudanca(&jogo("cs2"), "shader_cache").is_some());
    assert!(p.mudanca(&jogo("fora"), "shader_cache").is_none());
    // Fora da base: limitador desconhecido explica como fato desconhecido.
    let r = carregar("jogo_limitador_nativo = true", "por_jogo", shader).unwrap();
    let p = avaliar(
        &caps,
        &[GameFacts::new("fora")],
        &Preferences::default(),
        &r,
    );
    assert!(p.resumos["fora"].nao_aplicadas.iter().any(|n| {
        n.motivo
            == MotivoNaoAplicada::FatoDesconhecido {
                fato: "jogo_limitador_nativo".into(),
            }
    }));
}

#[test]
fn limite_vrr_exige_refresh_conhecido_e_vrr_ativo() {
    let aplicar = "limite_fps = { limite_vrr = true }";
    let base = "sincronismo = [\"gsync\"]";
    assert!(
        carregar(
            &format!("{base}\nrefresh_conhecido = true\nvrr_ativo = true"),
            "global",
            aplicar
        )
        .is_ok()
    );
    for q in ["refresh_conhecido = true", "vrr_ativo = true", ""] {
        let e = carregar(&format!("{base}\n{q}"), "global", aplicar).expect_err(q);
        assert!(e.contains("limite_vrr"), "{q}: {e}");
    }
    // Só `true` é aceito.
    assert!(
        carregar(
            &format!("{base}\nrefresh_conhecido = true\nvrr_ativo = true"),
            "global",
            "limite_fps = { limite_vrr = false }"
        )
        .is_err()
    );
}
