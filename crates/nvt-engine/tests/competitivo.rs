//! Fase competitiva 1 no motor (spec `docs/spec/competitivo.md` §7): refresh e
//! VRR na detecção (F1-01/F1-02), fatos da base curada (F1-03/F1-04), jogo em
//! execução (F1-09), energia por jogo com reversão do global (F1-11),
//! orientações (F1-12) e avisos (F1-15).

use std::path::PathBuf;

use nvt_core::{
    Alvo, Api, AvisoPlano, BASE_TOML, ConjuntoRegras, Orientacao, Preferences, SETTINGS_TOML,
    Sincronismo, Versao,
};
use nvt_detect::{DetectedGame, Launcher, ProcessosFalsos, ServicosSistema};
use nvt_drs::{FonteFalsa, ProfileRef, VrrBruto};
use nvt_engine::{
    AcaoReversao, Armazem, DetectorJogos, DriverFalso, Engine, JogoEntrada, capacidades,
    capacidades_com_exibicao,
};

const ENERGIA: u32 = 0x1057_EB71;

fn dir_teste(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nvt-competitivo-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn vrr(possivel: bool, ativo: bool) -> Option<VrrBruto> {
    Some(VrrBruto { possivel, ativo })
}

// ── F1-01 / F1-02 ───────────────────────────────────────────────────────────

#[test]
fn capacidades_leem_refresh_e_vrr_da_exibicao() {
    let d = DriverFalso::turing();
    let c = capacidades(&d).unwrap();
    assert_eq!(c.refresh_mhz, None, "sem NVAPI de exibição: desconhecido");
    assert_eq!(c.vrr.possivel, None);
    assert_eq!(c.vrr.ativo, None);

    d.definir_exibicao(FonteFalsa::monitor(7, 239_760, vrr(true, false)));
    let c = capacidades(&d).unwrap();
    assert_eq!(c.refresh_mhz, Some(239_760), "guardado em milésimos");
    assert_eq!(c.refresh_hz(), Some(240), "arredondado só no uso");
    assert_eq!(c.vrr.possivel, Some(true));
    assert_eq!(c.vrr.ativo, Some(false));

    // VRR indisponível (função ausente): refresh conhecido, VRR desconhecido.
    d.definir_exibicao(FonteFalsa::monitor(7, 144_000, None));
    let c = capacidades(&d).unwrap();
    assert_eq!(c.refresh_hz(), Some(144));
    assert_eq!(c.vrr.possivel, None);

    // Fonte injetada diretamente.
    let info = nvt_drs::ler_exibicao_principal(&FonteFalsa::monitor(1, 60_000, vrr(true, true)));
    let c = capacidades_com_exibicao(&d, info).unwrap();
    assert_eq!(c.refresh_hz(), Some(60));
    assert_eq!(c.vrr.ativo, Some(true));
}

// ── F1-03 / F1-04 ───────────────────────────────────────────────────────────

#[test]
fn base_curada_vence_a_deteccao_em_disco() {
    let e = Engine::new(DriverFalso::turing(), Armazem::em_memoria()).unwrap();
    let mut cs2 = JogoEntrada::new("Counter-Strike 2", "CS2.EXE");
    cs2.steam_appid = Some(730);
    cs2.reflex = None; // a detecção em disco não acha Reflex via NVAPI
    cs2.anticheat_kernel = Some(true); // indício errado: VAC não é de kernel
    cs2.api = Api::Dx11;
    let mut outro_appid = JogoEntrada::new("Outro", "cs2.exe");
    outro_appid.steam_appid = Some(12345);
    let mut fora = JogoEntrada::new("WARDOGS", "WardogsClient-Win64-Shipping.exe");
    fora.reflex = Some(true);
    fora.anticheat_kernel = Some(true);
    let lol = JogoEntrada::new("League of Legends", "League of Legends.exe");

    let r = e.resolver_jogos(&[cs2, outro_appid, fora, lol]).unwrap();
    let fatos = |nome: &str| {
        &r.iter()
            .find(|j| j.entrada.nome == nome)
            .unwrap_or_else(|| panic!("{nome}"))
            .fatos
    };
    let f = fatos("Counter-Strike 2");
    assert!(f.competitivo);
    assert_eq!(f.reflex, Some(true));
    assert_eq!(f.limitador_nativo, Some(true));
    assert_eq!(f.anticheat_kernel, Some(false));
    assert_eq!(f.jogo_base.as_ref().map(|i| i.as_str()), Some("cs2"));
    assert_eq!(f.api, Api::Dx11, "fatos de disco não curados seguem");

    // Exe genérico com AppID diferente não casa.
    let f = fatos("Outro");
    assert!(!f.competitivo);
    assert_eq!(f.limitador_nativo, None);

    // Fora da base: comportamento anterior.
    let f = fatos("WARDOGS");
    assert!(!f.competitivo);
    assert_eq!(f.reflex, Some(true));
    assert_eq!(f.anticheat_kernel, Some(true));
    assert_eq!(f.limitador_nativo, None);
    assert!(f.orientacoes.is_empty());

    // Base declara "sem Reflex" (LoL): false, não desconhecido.
    let f = fatos("League of Legends");
    assert!(f.competitivo);
    assert_eq!(f.reflex, Some(false));
    assert_eq!(f.anticheat_kernel, Some(true));
}

// ── F1-12 / F1-15 ───────────────────────────────────────────────────────────

#[test]
fn plano_traz_orientacoes_filtradas_e_avisos_de_vrr() {
    let d = DriverFalso::turing();
    d.definir_exibicao(FonteFalsa::monitor(7, 240_000, vrr(false, false)));
    let mut e = Engine::new(d.clone(), Armazem::em_memoria()).unwrap();
    let mut cs2 = JogoEntrada::new("Counter-Strike 2", "cs2.exe");
    cs2.steam_appid = Some(730);
    let jogos = [cs2];

    let diff = e.planejar(&jogos).unwrap();
    let perfil = diff.jogos[0].perfil.clone();
    let o = &diff.plano.resumos[&perfil].orientacoes;
    assert!(o.contains(&Orientacao::VsyncJogoOff));
    assert!(!o.contains(&Orientacao::VsyncDriverComGsync));
    assert!(diff.plano.avisos.is_empty(), "sem G-SYNC, sem aviso");

    e.definir_preferencias(Preferences {
        sincronismo: Sincronismo::GSync,
        ..Preferences::default()
    })
    .unwrap();
    let diff = e.planejar(&jogos).unwrap();
    let o = &diff.plano.resumos[&perfil].orientacoes;
    assert!(o.contains(&Orientacao::VsyncDriverComGsync));
    assert!(!o.contains(&Orientacao::VsyncJogoOff));
    assert_eq!(diff.plano.avisos, [AvisoPlano::GsyncSemVrr]);

    d.definir_exibicao(FonteFalsa::monitor(7, 240_000, vrr(true, false)));
    let diff = e.planejar(&jogos).unwrap();
    assert_eq!(diff.plano.avisos, [AvisoPlano::VrrDesligadoNoPainel]);
    // VRR desligado: o limite para VRR nem vira sugestão.
    assert!(
        !diff
            .plano
            .experimentais
            .iter()
            .any(|s| s.regra == "sincronismo-gsync-limite")
    );

    d.definir_exibicao(FonteFalsa::monitor(7, 240_000, vrr(true, true)));
    let diff = e.planejar(&jogos).unwrap();
    assert!(diff.plano.avisos.is_empty());
    let s = diff
        .plano
        .experimentais
        .iter()
        .find(|s| s.regra == "sincronismo-gsync-limite")
        .expect("sugestão com VRR ativo");
    assert_eq!(s.mudancas[0].valor, 224);
    // Experimental: o limite global não é gravado.
    assert!(diff.plano.mudanca(&Alvo::Global, "limite_fps").is_none());
}

// ── F1-11 ───────────────────────────────────────────────────────────────────

/// Regra global de energia como era antes de F1-11.
const ENERGIA_GLOBAL_ANTIGA: &str = r#"
versao_schema = 1

[[regra]]
id = "baseline-energia-desempenho-maximo"
descricao = "Gerenciamento de energia em desempenho máximo"
status = "estavel"
escopo = "global"
impacto_visual = "nenhum"
prioridade = 50
motivo = "Regra antiga (global), para o teste de reversão."
evidencia = "Laboratório."

[regra.aplicar]
energia = "desempenho_maximo"
"#;

fn regras_antigas() -> ConjuntoRegras {
    ConjuntoRegras::from_toml(
        SETTINGS_TOML,
        &[
            ("base.toml", BASE_TOML),
            ("antiga.toml", ENERGIA_GLOBAL_ANTIGA),
        ],
    )
    .unwrap()
}

#[test]
fn energia_global_volta_ao_valor_de_antes_da_gestao() {
    let dir = dir_teste("energia");
    let d = DriverFalso::turing();
    d.definir_padrao(ENERGIA, 5); // Normal/Otimizado
    d.adicionar_perfil_predefinido("Jogo", &["jogo.exe"], &[]);
    let mut jogo = JogoEntrada::new("Jogo", "jogo.exe");
    jogo.dlss = Some(Versao::new(310, 0, 0, 0));
    let jogos = [jogo];

    // Versão anterior: energia gerida no global.
    {
        let mut e = Engine::com_regras(d.clone(), Armazem::abrir(&dir).unwrap(), regras_antigas());
        let diff = e.planejar(&jogos).unwrap();
        e.aplicar(&diff, true).unwrap();
        assert_eq!(d.valor(&ProfileRef::Base, ENERGIA), Some(1));
        assert!(
            e.estado()
                .esperado
                .iter()
                .any(|x| x.perfil == ProfileRef::Base && x.id == ENERGIA)
        );
    }

    // Regras atuais: o global sai do plano e volta ao original; o perfil do
    // jogo fica com desempenho máximo.
    let mut e = Engine::new(d.clone(), Armazem::abrir(&dir).unwrap()).unwrap();
    let diff = e.planejar(&jogos).unwrap();
    let rev: Vec<_> = diff
        .reverter
        .iter()
        .filter(|r| r.esperado.perfil == ProfileRef::Base && r.esperado.id == ENERGIA)
        .collect();
    assert_eq!(rev.len(), 1, "{:?}", diff.reverter);
    assert!(matches!(rev[0].acao, AcaoReversao::Remover { .. }));
    assert!(diff.plano.mudanca(&Alvo::Global, "energia").is_none());
    assert_eq!(
        diff.plano
            .mudanca(&Alvo::Jogo("Jogo".into()), "energia")
            .map(|m| m.valor),
        Some(1)
    );

    let r = e.aplicar(&diff, true).unwrap();
    assert!(r.aplicada);
    assert!(r.falhas.is_empty() && r.erros.is_empty());
    assert_eq!(
        d.valor(&ProfileRef::Base, ENERGIA),
        Some(5),
        "global de volta ao valor de antes da gestão"
    );
    assert!(!d.proprias(&ProfileRef::Base).contains_key(&ENERGIA));
    assert_eq!(d.valor(&ProfileRef::Named("Jogo".into()), ENERGIA), Some(1));
    assert!(
        !e.estado()
            .esperado
            .iter()
            .any(|x| x.perfil == ProfileRef::Base && x.id == ENERGIA)
    );
    // Ciclo seguinte: nada a fazer (não regrava o global).
    assert!(e.planejar(&jogos).unwrap().vazio());
    let _ = std::fs::remove_dir_all(&dir);
}

// ── F1-09 ───────────────────────────────────────────────────────────────────

fn cs2_em(pasta: &std::path::Path) -> DetectedGame {
    let mut j = DetectedGame::new(Launcher::Steam, "Counter-Strike 2", pasta).with_app_id("730");
    j.exe_indicado = Some(pasta.join("cs2.exe"));
    j
}

#[test]
fn jogo_em_execucao_reaproveita_fatos_e_nao_le_a_pasta() {
    let dir = dir_teste("execucao");
    std::fs::write(dir.join("cs2.exe"), b"MZ").unwrap();
    std::fs::write(dir.join("nvngx_dlss.dll"), b"MZ").unwrap();
    let jogos = [cs2_em(&dir)];
    let sv = ServicosSistema::default();
    let fechado = ProcessosFalsos::com_nomes(&["explorer.exe"]);
    let aberto = ProcessosFalsos::com_nomes(&["explorer.exe", "CS2.EXE"]);

    let mut det = DetectorJogos::embutido();
    let r = det.detectar_jogos(&jogos, &[], &sv, &fechado);
    assert_eq!(r.jogos.len(), 1, "{:?}", r.avisos);
    let antes = r.jogos[0].clone();
    assert!(!antes.em_execucao);
    assert_eq!(antes.exe, "cs2.exe");
    assert!(antes.dlss.is_some(), "DLSS lido da pasta");
    assert_eq!(antes.steam_appid, Some(730));
    assert_eq!(antes.pasta.as_deref(), Some(dir.as_path()));
    assert_eq!(antes.id_jogo.as_ref().map(|i| i.as_str()), Some("cs2"));
    assert!(r.processos_em_execucao.is_empty());

    // Com o jogo aberto, a pasta some: se fosse lida, o jogo perderia o exe.
    std::fs::remove_dir_all(&dir).unwrap();
    let r = det.detectar_jogos(&jogos, &[], &sv, &aberto);
    assert!(r.avisos.is_empty(), "{:?}", r.avisos);
    let j = &r.jogos[0];
    assert!(j.em_execucao);
    assert_eq!(j.dlss, antes.dlss, "fatos anteriores reaproveitados");
    assert_eq!(j.exe, "cs2.exe");
    assert_eq!(r.em_execucao().count(), 1);
    assert!(
        r.processos_em_execucao
            .iter()
            .any(|p| p.eq_ignore_ascii_case("cs2.exe"))
    );

    // Sem varredura anterior: fatos desconhecidos, exe pela base (AppID) e
    // casamento com a base mantido.
    let mut novo = DetectorJogos::embutido();
    let mut sem_indicado = cs2_em(&dir);
    sem_indicado.exe_indicado = None;
    let r = novo.detectar_jogos(&[sem_indicado], &[], &sv, &aberto);
    let j = &r.jogos[0];
    assert!(j.em_execucao);
    assert_eq!(j.exe, "cs2.exe");
    assert_eq!(j.dlss, None);
    assert_eq!(j.api, Api::Desconhecida);
    assert_eq!(j.reflex, None);
    assert_eq!(j.id_jogo.as_ref().map(|i| i.as_str()), Some("cs2"));

    // O motor ainda monta os fatos curados do jogo aberto.
    let e = Engine::new(DriverFalso::turing(), Armazem::em_memoria()).unwrap();
    let res = e.resolver_jogos(&r.jogos).unwrap();
    assert!(res[0].fatos.competitivo);
    assert_eq!(res[0].fatos.reflex, Some(true));
    assert!(res[0].entrada.em_execucao);
}

#[test]
fn anticheat_durante_o_jogo_aparece_nos_processos_vigiados() {
    let mut det = DetectorJogos::embutido();
    let procs = ProcessosFalsos::com_nomes(&["easyanticheat.exe", "notepad.exe"]);
    let r = det.detectar_jogos(&[], &[], &ServicosSistema::default(), &procs);
    assert_eq!(r.processos_em_execucao, ["EasyAntiCheat.exe"]);
    assert!(r.jogos.is_empty());
}
