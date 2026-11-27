//! Fase 3 no núcleo (L1-b): tipos de destino
//! de jogo, catálogo `[[arquivo]]`/`[[regra]]` de `regras/jogos.toml`, avaliação
//! das regras `config_jogo` (F3-02, D9) e F3-13 (DRS `limite_fps = off`).

use std::collections::{BTreeMap, BTreeSet};

use nvt_core::config_jogo::{Arredondamento, FaixaJogo, TipoValorJogo};
use nvt_core::usuario::{FatosUsuario, MotivoRespeitadaUsuario, PlanoUsuario};
use nvt_core::{
    Alvo, Arquitetura, BaseJogos, Capabilities, ConjuntoRegras, DefChaveJogo, DestinoJogo,
    ErroConfigJogo, ErroJogo, EstadoDestinoJogo, FatosConfigJogo, GameFacts, GestaoJogo,
    JOGOS_TOML, MotivoDescarteJogo, PlanoJogo, Preferences, SettingJogo, Status, ValorJogo,
    avaliar_com_aceites, avaliar_config_jogo, desligar_limite_drs, limites_no_jogo,
};

const PROIBIDOS_OU_ORIENTAR: [&str; 8] = [
    "valorant",
    "lol",
    "marvel-rivals",
    "cod",
    "apex",
    "overwatch2",
    "pubg",
    "the-finals",
];

fn base() -> BaseJogos {
    BaseJogos::embutida().unwrap_or_else(|e| panic!("jogos.toml inválido:\n{e}"))
}

fn erros_com(extra: &str) -> Vec<ErroConfigJogo> {
    let texto = format!("{JOGOS_TOML}\n{extra}");
    BaseJogos::from_toml("teste.toml", &texto)
        .expect_err("deveria recusar")
        .0
        .into_iter()
        .map(|e| match e {
            ErroJogo::ConfigJogo(e) => e,
            outro => panic!("erro inesperado: {outro}"),
        })
        .collect()
}

fn destino(setting: SettingJogo) -> DestinoJogo {
    DestinoJogo::novo(setting, Some("abc123")).unwrap()
}

fn lido(valor: ValorJogo) -> EstadoDestinoJogo {
    EstadoDestinoJogo {
        atual: Some(valor),
        ..EstadoDestinoJogo::default()
    }
}

fn fatos_todos() -> FatosConfigJogo {
    let mut f = FatosConfigJogo::default();
    for (s, v) in [
        (SettingJogo::R6FpsLimit, ValorJogo::Inteiro(0)),
        (SettingJogo::Cs2FpsMax, ValorJogo::TokenLancamento(None)),
        (SettingJogo::Dota2FpsMax, ValorJogo::TokenLancamento(None)),
        (SettingJogo::RlOneFrameThreadLag, ValorJogo::Booleano(true)),
        (SettingJogo::FortniteFrameRateLimit, ValorJogo::Inteiro(0)),
        (
            SettingJogo::DeadlockFpsMax,
            ValorJogo::TokenLancamento(None),
        ),
    ] {
        f.destinos.insert(destino(s), lido(v));
    }
    f
}

fn prefs(limite: Option<u32>) -> Preferences {
    Preferences {
        limite_fps: limite,
        ..Preferences::default()
    }
}

fn todas_aceitas() -> BTreeSet<String> {
    base()
        .config_jogo()
        .regras()
        .iter()
        .map(|r| r.id.clone())
        .collect()
}

fn plano(fatos: &FatosConfigJogo, limite: Option<u32>, refresh: Option<u32>) -> PlanoJogo {
    let b = base();
    avaliar_config_jogo(
        fatos,
        &prefs(limite),
        refresh,
        b.config_jogo(),
        &todas_aceitas(),
    )
}

fn valor(p: &PlanoJogo, s: SettingJogo) -> Option<&ValorJogo> {
    p.mudancas.get(&destino(s)).map(|m| &m.valor)
}

// ── Carga ───────────────────────────────────────────────────────────────────

#[test]
fn carrega_base_real_com_so_o_confirmado_gravavel() {
    let b = base();
    let c = b.config_jogo();
    let ids: Vec<_> = c.regras().iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "jogo-limite-no-jogo-r6",
            "jogo-limite-launch-source2",
            "jogo-dota2-fps-max-padrao",
            "jogo-rl-one-frame-thread-lag-off",
        ]
    );
    assert_eq!(
        c.gravaveis(),
        BTreeSet::from([
            SettingJogo::R6FpsLimit,
            SettingJogo::RlOneFrameThreadLag,
            SettingJogo::Cs2FpsMax,
            SettingJogo::Dota2FpsMax,
        ])
    );
    for s in SettingJogo::TODAS {
        let def = c.chave(s).unwrap_or_else(|| panic!("{s} fora do catálogo"));
        assert_eq!(def.a_confirmar, !c.gravaveis().contains(&s), "{s}");
    }
    for r in c.regras() {
        assert_eq!(r.status, Status::Experimental, "{}", r.id);
        assert!(r.evidencia.contains("https://"), "{}", r.id);
    }
    let r6 = c.chave(SettingJogo::R6FpsLimit).unwrap();
    assert_eq!(r6.secao.as_deref(), Some("DISPLAY"));
    assert_eq!(r6.chave, "FPSLimit");
    assert_eq!(r6.faixa.unwrap().min, 30);
    assert_eq!(
        c.chave(SettingJogo::Dota2FpsMax)
            .unwrap()
            .faixa
            .unwrap()
            .max,
        240
    );
}

#[test]
fn trava_jogos_proibidos_e_orientar_sem_nada_gravavel() {
    let b = base();
    let c = b.config_jogo();
    for id in PROIBIDOS_OU_ORIENTAR {
        assert!(b.jogo(id).is_some(), "{id}");
        assert!(c.arquivos().iter().all(|a| a.jogo.as_str() != id), "{id}");
        assert!(SettingJogo::TODAS.iter().all(|s| s.jogo() != id), "{id}");
        assert!(
            c.regras()
                .iter()
                .all(|r| r.quando.jogo.iter().all(|j| j.as_str() != id)),
            "{id}"
        );
        let arquivo = format!(
            "[[arquivo]]\nid = \"x-{id}\"\njogo = \"{id}\"\nraiz = \"documentos\"\ncaminho = 'a\\b.ini'\nformato = \"ini\"\nevidencia = \"x\"\n[[arquivo.chave]]\nsetting = \"r6_reflex\"\nsecao = \"S\"\ntipo = \"inteiro\"\na_confirmar = true\n"
        );
        let e = erros_com(&arquivo);
        assert!(
            e.iter()
                .any(|e| matches!(e, ErroConfigJogo::JogoSemEdicao { jogo, .. } if jogo == id)),
            "{id}: {e:?}"
        );
        let regra = format!(
            "[[regra]]\nid = \"jogo-x\"\ndescricao = \"x\"\nstatus = \"experimental\"\nescopo = \"config_jogo\"\nquando = {{ jogo = [\"{id}\"] }}\naplicar = {{ rl_one_frame_thread_lag = {{ booleano = false }} }}\nimpacto_visual = \"nenhum\"\nmotivo = \"x\"\nevidencia = \"x\"\n"
        );
        let e = erros_com(&regra);
        assert!(
            e.iter()
                .any(|e| matches!(e, ErroConfigJogo::JogoSemEdicao { jogo, .. } if jogo == id)),
            "{id}: {e:?}"
        );
    }
}

fn regra(aplicar: &str, extra: &str) -> String {
    format!(
        "[[regra]]\nid = \"jogo-teste\"\ndescricao = \"x\"\nstatus = \"experimental\"\nescopo = \"config_jogo\"\nquando = {{ jogo = [\"r6-siege\", \"fortnite\", \"dota2\"], limite_fps_definido = true }}\naplicar = {{ {aplicar} }}\nimpacto_visual = \"nenhum\"\nmotivo = \"x\"\nevidencia = \"x\"\n{extra}"
    )
}

#[test]
fn recusa_regra_que_grava_chave_a_confirmar() {
    let e = erros_com(&regra(
        "fortnite_frame_rate_limit = { preferencia = \"limite_fps\" }",
        "",
    ));
    assert!(
        matches!(
            &e[..],
            [ErroConfigJogo::ChaveAConfirmar {
                setting: SettingJogo::FortniteFrameRateLimit,
                ..
            }]
        ),
        "{e:?}"
    );
}

#[test]
fn recusa_chave_fora_do_catalogo_e_valor_fora_do_menu() {
    let texto = format!(
        "{JOGOS_TOML}\n{}",
        regra("chave_nova = { inteiro = 1 }", "")
    );
    assert!(matches!(
        &BaseJogos::from_toml("t", &texto).unwrap_err().0[..],
        [ErroJogo::Toml { .. }]
    ));
    let e = erros_com(&regra("r6_fps_limit = { inteiro = 20 }", ""));
    assert!(
        matches!(&e[..], [ErroConfigJogo::ValorForaDoMenu { valor: 20, .. }]),
        "{e:?}"
    );
    let e = erros_com(&regra("dota2_fps_max = { inteiro = 300 }", ""));
    assert!(
        matches!(&e[..], [ErroConfigJogo::ValorForaDoMenu { valor: 300, .. }]),
        "{e:?}"
    );
    // Setting de outro jogo dentro de um arquivo.
    let arquivo = "[[arquivo]]\nid = \"x\"\njogo = \"rocket-league\"\nraiz = \"documentos\"\ncaminho = 'a\\b.ini'\nformato = \"ini\"\nprocessos_fechados = [\"RocketLeague.exe\"]\nevidencia = \"x\"\n[[arquivo.chave]]\nsetting = \"r6_fps_limit\"\nsecao = \"S\"\nchave = \"K\"\ntipo = \"inteiro\"\nfaixa = { min = 30, max = 60 }\n";
    let e = erros_com(arquivo);
    assert!(
        e.iter()
            .any(|e| matches!(e, ErroConfigJogo::ChaveForaDoCatalogo { .. })),
        "{e:?}"
    );
}

#[test]
fn recusa_regra_estavel_com_impacto_ou_sem_prefixo() {
    let e = erros_com(
        &regra("r6_fps_limit = { preferencia = \"limite_fps\" }", "")
            .replace("\"experimental\"", "\"estavel\""),
    );
    assert!(
        matches!(&e[..], [ErroConfigJogo::RegraInvalida { .. }]),
        "{e:?}"
    );
    let e = erros_com(
        &regra("r6_fps_limit = { preferencia = \"limite_fps\" }", "")
            .replace("impacto_visual = \"nenhum\"", "impacto_visual = \"leve\""),
    );
    assert!(
        matches!(&e[..], [ErroConfigJogo::RegraInvalida { .. }]),
        "{e:?}"
    );
    let e = erros_com(
        &regra("r6_fps_limit = { preferencia = \"limite_fps\" }", "")
            .replace("jogo-teste", "teste"),
    );
    assert!(
        matches!(&e[..], [ErroConfigJogo::RegraInvalida { .. }]),
        "{e:?}"
    );
    // refresh_limitado_ao_menu fora do Dota 2 padrão.
    let e = erros_com(&regra("dota2_fps_max = \"refresh_limitado_ao_menu\"", ""));
    assert!(
        matches!(&e[..], [ErroConfigJogo::RegraInvalida { .. }]),
        "{e:?}"
    );
}

#[test]
fn recusa_caminho_na_instalacao_e_nomes_proibidos() {
    for caminho in [
        r"steamapps\common\Counter-Strike Global Offensive\game\csgo\cfg\x.cfg",
        r"Program Files\Jogo\x.ini",
        r"<steamid3>\730\local\cfg\autoexec.cfg",
        r"cfg\video.txt",
        r"x\boot.vcfg",
        r"game\gameinfo.gi",
        r"C:\Users\Usuario\x.ini",
        r"..\x.ini",
        r"<instalacao>\x.ini",
    ] {
        let arquivo = format!(
            "[[arquivo]]\nid = \"x\"\njogo = \"rocket-league\"\nraiz = \"documentos\"\ncaminho = '{caminho}'\nformato = \"ini\"\nprocessos_fechados = [\"RocketLeague.exe\"]\nevidencia = \"x\"\n[[arquivo.chave]]\nsetting = \"rl_max_fps\"\nsecao = \"S\"\ntipo = \"inteiro\"\na_confirmar = true\n"
        );
        let e = erros_com(&arquivo);
        assert!(
            e.iter()
                .any(|e| matches!(e, ErroConfigJogo::CaminhoProibido { .. })),
            "{caminho}: {e:?}"
        );
    }
}

#[test]
fn arredonda_para_baixo_ao_valor_do_menu() {
    let def = DefChaveJogo {
        setting: SettingJogo::FortniteFrameRateLimit,
        secao: Some("S".into()),
        chave: "FrameRateLimit".into(),
        token: None,
        tipo: TipoValorJogo::Decimal,
        valores: vec![30, 60, 120, 144, 240, 360],
        faixa: None,
        arredondar: Arredondamento::Baixo,
        a_confirmar: false,
    };
    assert_eq!(def.ajustar(141), Some(120));
    assert_eq!(def.ajustar(144), Some(144));
    assert_eq!(def.ajustar(1000), Some(360));
    assert_eq!(def.ajustar(20), None);
    assert!(def.aceita(&ValorJogo::Inteiro(240)));
    assert!(!def.aceita(&ValorJogo::Inteiro(200)));
    let exato = DefChaveJogo {
        arredondar: Arredondamento::Exato,
        ..def.clone()
    };
    assert_eq!(exato.ajustar(141), None);
    let a_confirmar = DefChaveJogo {
        a_confirmar: true,
        ..def
    };
    assert_eq!(a_confirmar.ajustar(144), None);
    let faixa = DefChaveJogo {
        valores: vec![],
        faixa: Some(FaixaJogo { min: 60, max: 240 }),
        arredondar: Arredondamento::Exato,
        tipo: TipoValorJogo::TokenLancamento,
        a_confirmar: false,
        ..exato
    };
    assert!(faixa.aceita(&ValorJogo::TokenLancamento(Some(240))));
    assert!(!faixa.aceita(&ValorJogo::TokenLancamento(Some(241))));
    assert!(faixa.aceita(&ValorJogo::TokenLancamento(None)));
}

// ── Avaliação ───────────────────────────────────────────────────────────────

#[test]
fn sem_aceite_so_sugere() {
    let b = base();
    let p = avaliar_config_jogo(
        &fatos_todos(),
        &prefs(Some(141)),
        Some(144),
        b.config_jogo(),
        &BTreeSet::new(),
    );
    assert!(p.mudancas.is_empty());
    assert!(limites_no_jogo(&p).is_empty());
    let sugeridas: BTreeSet<_> = p.experimentais.iter().map(|s| s.regra.as_str()).collect();
    assert_eq!(
        sugeridas,
        BTreeSet::from([
            "jogo-limite-no-jogo-r6",
            "jogo-limite-launch-source2",
            "jogo-rl-one-frame-thread-lag-off",
        ])
    );
}

#[test]
fn limite_do_usuario_vai_para_o_jogo_com_aceite() {
    let p = plano(&fatos_todos(), Some(141), Some(144));
    assert_eq!(
        valor(&p, SettingJogo::R6FpsLimit),
        Some(&ValorJogo::Inteiro(141))
    );
    assert_eq!(
        valor(&p, SettingJogo::Cs2FpsMax),
        Some(&ValorJogo::TokenLancamento(Some(141)))
    );
    assert_eq!(
        valor(&p, SettingJogo::Dota2FpsMax),
        Some(&ValorJogo::TokenLancamento(Some(141)))
    );
    assert_eq!(
        valor(&p, SettingJogo::RlOneFrameThreadLag),
        Some(&ValorJogo::Booleano(false))
    );
    // a_confirmar nunca entra no plano.
    for s in [
        SettingJogo::FortniteFrameRateLimit,
        SettingJogo::DeadlockFpsMax,
    ] {
        assert!(valor(&p, s).is_none(), "{s}");
        assert!(
            p.experimentais
                .iter()
                .all(|e| e.mudancas.iter().all(|m| m.destino.setting != s))
        );
    }
    assert!(p.mudancas[&destino(SettingJogo::R6FpsLimit)].limite_no_jogo);
    assert!(!p.mudancas[&destino(SettingJogo::RlOneFrameThreadLag)].limite_no_jogo);
}

#[test]
fn fora_do_menu_e_descartado() {
    let p = plano(&fatos_todos(), Some(25), None);
    assert!(valor(&p, SettingJogo::R6FpsLimit).is_none());
    assert!(
        p.descartes
            .iter()
            .any(|d| d.destino.setting == SettingJogo::R6FpsLimit
                && d.motivo == MotivoDescarteJogo::ForaDoMenu { pedido: 25 })
    );
    assert!(valor(&p, SettingJogo::Cs2FpsMax).is_none());
    let p = plano(&fatos_todos(), Some(45), None);
    assert_eq!(
        valor(&p, SettingJogo::R6FpsLimit),
        Some(&ValorJogo::Inteiro(45))
    );
    assert!(valor(&p, SettingJogo::Cs2FpsMax).is_none());
    let p = plano(&fatos_todos(), Some(300), None);
    assert!(valor(&p, SettingJogo::Dota2FpsMax).is_none());
    assert!(
        p.descartes
            .iter()
            .any(|d| d.destino.setting == SettingJogo::Dota2FpsMax
                && d.motivo == MotivoDescarteJogo::ForaDoMenu { pedido: 300 })
    );
    assert_eq!(
        valor(&p, SettingJogo::R6FpsLimit),
        Some(&ValorJogo::Inteiro(300))
    );
}

#[test]
fn sem_limite_so_o_dota2_padrao_sobe() {
    let p = plano(&fatos_todos(), None, Some(165));
    let limites: Vec<_> = p.mudancas.keys().map(|d| d.setting).collect();
    assert_eq!(
        limites,
        [SettingJogo::RlOneFrameThreadLag, SettingJogo::Dota2FpsMax]
    );
    assert_eq!(
        valor(&p, SettingJogo::Dota2FpsMax),
        Some(&ValorJogo::TokenLancamento(Some(165)))
    );
    assert!(limites_no_jogo(&p).is_empty());
    // Teto do menu.
    let p = plano(&fatos_todos(), None, Some(360));
    assert_eq!(
        valor(&p, SettingJogo::Dota2FpsMax),
        Some(&ValorJogo::TokenLancamento(Some(240)))
    );
    // Não cria limite igual ou menor que o padrão 120.
    for hz in [60, 120] {
        let p = plano(&fatos_todos(), None, Some(hz));
        assert!(valor(&p, SettingJogo::Dota2FpsMax).is_none(), "{hz}");
    }
    // Refresh desconhecido: nada.
    assert!(valor(&plano(&fatos_todos(), None, None), SettingJogo::Dota2FpsMax).is_none());
    // Valor existente igual ou maior (ou 0 = ilimitado): não baixa.
    for existente in [0, 200, 240] {
        let mut f = fatos_todos();
        f.destinos.insert(
            destino(SettingJogo::Dota2FpsMax),
            lido(ValorJogo::TokenLancamento(Some(existente))),
        );
        let p = plano(&f, None, Some(165));
        assert!(valor(&p, SettingJogo::Dota2FpsMax).is_none(), "{existente}");
        assert!(
            p.descartes
                .iter()
                .any(|d| d.motivo == MotivoDescarteJogo::NaoSobeOLimite)
        );
    }
}

#[test]
fn bloqueios_travas_e_deriva() {
    let mut f = fatos_todos();
    f.destinos
        .get_mut(&destino(SettingJogo::R6FpsLimit))
        .unwrap()
        .somente_leitura = true;
    f.destinos
        .get_mut(&destino(SettingJogo::Cs2FpsMax))
        .unwrap()
        .conta_ambigua = true;
    f.travadas.insert(destino(SettingJogo::RlOneFrameThreadLag));
    f.gestao.insert(
        destino(SettingJogo::Dota2FpsMax),
        GestaoJogo {
            antes: ValorJogo::TokenLancamento(None),
            esperado: ValorJogo::TokenLancamento(Some(141)),
            hash_antes: None,
            hash_depois: None,
            backup: None,
        },
    );
    // O jogo mudou o valor desde a gravação: deriva.
    f.destinos.insert(
        destino(SettingJogo::Dota2FpsMax),
        lido(ValorJogo::TokenLancamento(Some(90))),
    );
    let p = plano(&f, Some(141), None);
    assert!(p.mudancas.is_empty(), "{:?}", p.mudancas);
    assert_eq!(
        p.respeitadas
            .get(&destino(SettingJogo::RlOneFrameThreadLag)),
        Some(&MotivoRespeitadaUsuario::Travada)
    );
    assert_eq!(
        p.respeitadas.get(&destino(SettingJogo::Dota2FpsMax)),
        Some(&MotivoRespeitadaUsuario::AlteradaForaDaGestao)
    );
    let motivos: BTreeSet<_> = p
        .descartes
        .iter()
        .map(|d| (d.destino.setting, format!("{:?}", d.motivo)))
        .collect();
    assert!(motivos.contains(&(SettingJogo::R6FpsLimit, "SomenteLeitura".into())));
    assert!(motivos.contains(&(SettingJogo::Cs2FpsMax, "ContaAmbigua".into())));
    // Gestão conferindo: continua no plano.
    f.destinos.insert(
        destino(SettingJogo::Dota2FpsMax),
        lido(ValorJogo::TokenLancamento(Some(141))),
    );
    let p = plano(&f, Some(141), None);
    assert_eq!(
        valor(&p, SettingJogo::Dota2FpsMax),
        Some(&ValorJogo::TokenLancamento(Some(141)))
    );
}

// ── F3-13 ───────────────────────────────────────────────────────────────────

fn jogo_drs(base: &BaseJogos, perfil: &str, id: &str) -> GameFacts {
    GameFacts {
        jogo_base: Some(base.jogo(id).unwrap().id.clone()),
        competitivo: true,
        ..GameFacts::new(perfil)
    }
}

#[test]
fn f3_13_drs_off_so_com_limite_no_jogo_no_plano() {
    let b = base();
    let drs = ConjuntoRegras::embutido().unwrap();
    let caps = Capabilities::tipicas(Arquitetura::Ada);
    let jogos = [
        jogo_drs(&b, "Rainbow Six Siege", "r6-siege"),
        jogo_drs(&b, "Dota 2", "dota2"),
        jogo_drs(&b, "Fortnite", "fortnite"),
    ];
    let p = prefs(Some(141));
    let drs_plan = avaliar_com_aceites(&caps, &jogos, &p, &drs, &BTreeSet::new());
    assert_eq!(
        drs_plan.mudanca(&Alvo::Global, "limite_fps").unwrap().valor,
        141
    );

    // Sem aceite: nada muda no DRS.
    let sem = avaliar_config_jogo(&fatos_todos(), &p, None, b.config_jogo(), &BTreeSet::new());
    let mut plan = drs_plan.clone();
    desligar_limite_drs(&mut plan, &jogos, &limites_no_jogo(&sem), &drs.catalogo, &p);
    assert_eq!(plan, drs_plan);

    // Só R6 aceito: só o perfil do R6 fica sem limite; Dota e Fortnite herdam o do driver.
    let so_r6 = BTreeSet::from(["jogo-limite-no-jogo-r6".to_owned()]);
    let pj = avaliar_config_jogo(&fatos_todos(), &p, None, b.config_jogo(), &so_r6);
    let limites = limites_no_jogo(&pj);
    assert_eq!(limites.len(), 1);
    let mut plan = drs_plan.clone();
    desligar_limite_drs(&mut plan, &jogos, &limites, &drs.catalogo, &p);
    let r6 = Alvo::Jogo("Rainbow Six Siege".into());
    let off = plan.mudanca(&r6, "limite_fps").unwrap();
    assert_eq!((off.valor, off.valor_nome.as_deref()), (0, Some("off")));
    assert_eq!(off.origem.regra(), Some("jogo-limite-no-jogo-r6"));
    assert!(
        !plan.resumos["Rainbow Six Siege"]
            .herda_do_global
            .contains(&"limite_fps".to_owned())
    );
    for outro in ["Dota 2", "Fortnite"] {
        assert!(
            plan.mudanca(&Alvo::Jogo(outro.into()), "limite_fps")
                .is_none(),
            "{outro}"
        );
    }
    // Global intacto: um único limitador por jogo.
    assert_eq!(plan.global, drs_plan.global);

    // Arquivo somente leitura: o limite não vai ao plano do jogo e o driver mantém o seu.
    let mut f = fatos_todos();
    f.destinos
        .get_mut(&destino(SettingJogo::R6FpsLimit))
        .unwrap()
        .somente_leitura = true;
    let pj = avaliar_config_jogo(&f, &p, None, b.config_jogo(), &so_r6);
    assert!(limites_no_jogo(&pj).is_empty());

    // Dota 2 padrão (sem limite do usuário) não mexe no DRS.
    let sem_limite = prefs(None);
    let pj = avaliar_config_jogo(
        &fatos_todos(),
        &sem_limite,
        Some(165),
        b.config_jogo(),
        &todas_aceitas(),
    );
    assert!(valor(&pj, SettingJogo::Dota2FpsMax).is_some());
    assert!(limites_no_jogo(&pj).is_empty());

    // Escolha travada do usuário no perfil do jogo é respeitada.
    let mut travada = p.clone();
    travada.escolhas_usuario.push(nvt_core::EscolhaUsuario {
        alvo: r6.clone(),
        setting: "limite_fps".into(),
    });
    let mut plan = drs_plan.clone();
    desligar_limite_drs(&mut plan, &jogos, &limites, &drs.catalogo, &travada);
    assert!(plan.mudanca(&r6, "limite_fps").is_none());
}

// ── Serialização e compatibilidade v6 ───────────────────────────────────────

#[test]
fn destino_tem_forma_textual_estavel() {
    let d = destino(SettingJogo::R6FpsLimit);
    assert_eq!(d.to_string(), "r6_fps_limit@abc123");
    assert_eq!(d.instancia.jogo.as_str(), "r6-siege");
    assert_eq!(d.instancia.arquivo, "r6-game-settings");
    assert_eq!("r6_fps_limit@abc123".parse::<DestinoJogo>().unwrap(), d);
    let sem = "dota2_fps_max".parse::<DestinoJogo>().unwrap();
    assert_eq!(sem.instancia.conta, None);
    for ruim in [
        "",
        "x@a",
        "r6_fps_limit@",
        "r6_fps_limit@a/b",
        "r6_fps_limit@a@b",
    ] {
        assert!(ruim.parse::<DestinoJogo>().is_err(), "{ruim}");
    }
    for s in SettingJogo::TODAS {
        assert_eq!(SettingJogo::de_codigo(s.codigo()), Some(s));
        assert!(DestinoJogo::novo(s, None).unwrap().coerente());
    }
}

#[test]
fn fatos_e_plano_ida_e_volta_e_v6_sem_familia_jogo() {
    let mut f = FatosUsuario::default();
    f.jogo.destinos.insert(
        destino(SettingJogo::R6FpsLimit),
        lido(ValorJogo::Inteiro(60)),
    );
    f.jogo.gestao.insert(
        destino(SettingJogo::R6FpsLimit),
        GestaoJogo {
            antes: ValorJogo::Inteiro(0),
            esperado: ValorJogo::Inteiro(60),
            hash_antes: Some("aa".into()),
            hash_depois: Some("bb".into()),
            backup: Some("r6".into()),
        },
    );
    let texto = toml::to_string(&f).unwrap();
    assert!(texto.contains("r6_fps_limit@abc123"), "{texto}");
    let volta: FatosUsuario = toml::from_str(&texto).unwrap();
    assert_eq!(volta, f);

    // Fatos e plano v6 (sem `jogo`).
    let v6: FatosUsuario = toml::from_str("windows_build = 22000\n").unwrap();
    assert_eq!(v6.jogo, FatosConfigJogo::default());
    let plano: PlanoUsuario = toml::from_str(
        "experimentais = []\n[resumo]\nmudancas = {}\nrespeitadas = {}\nestado_experimental = {}\n",
    )
    .unwrap();
    assert_eq!(plano.jogo, PlanoJogo::default());

    // Destino incoerente na chave do mapa é recusado.
    let ruim = texto.replace("r6_fps_limit@abc123", "r6_fps_limit@a b");
    assert!(toml::from_str::<FatosUsuario>(&ruim).is_err());
    let _ = BTreeMap::<DestinoJogo, ()>::new();
}

#[test]
fn catalogo_valida_relato_do_agente() {
    let b = base();
    let c = b.config_jogo();
    let r6 = destino(SettingJogo::R6FpsLimit);
    assert!(c.aceita(&r6, &ValorJogo::Inteiro(141)));
    assert!(!c.aceita(&r6, &ValorJogo::Inteiro(29)));
    assert!(!c.aceita(&r6, &ValorJogo::Booleano(true)));
    assert!(!c.aceita(
        &destino(SettingJogo::FortniteFrameRateLimit),
        &ValorJogo::Inteiro(240)
    ));
    let mut incoerente = r6.clone();
    incoerente.instancia.jogo = b.jogo("valorant").unwrap().id.clone();
    assert!(!c.aceita(&incoerente, &ValorJogo::Inteiro(141)));
}
