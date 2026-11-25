//! Varredura da máquina real (somente leitura). Roda só com `--ignored`:
//! `cargo test -p nvt-detect -- --ignored --nocapture`.
//!
//! Espera os dois jogos medidos em `docs/pesquisa/deteccao.md`.

use nvt_detect::{
    ApiGrafica, DetectedGame, GameTraits, Launcher, ServicosSistema, analisar_jogo,
    varrer_bibliotecas,
};

fn imprimir(j: &DetectedGame, f: &GameTraits) {
    println!("== {} [{}] {}", j.nome, j.launcher, j.pasta.display());
    println!("   exe: {:?}", f.exe_principal);
    println!(
        "   api: {:?} ({:?}: {}) ofuscados={}",
        f.api.valor, f.api.confianca, f.api.evidencia, f.imports_ofuscados
    );
    println!(
        "   dlss: {} | streamline {:?} {:?}",
        f.dlss.evidencia, f.dlss.valor.streamline_versao, f.dlss.valor.streamline_modulos
    );
    println!(
        "   reflex: {} ({:?}: {})",
        f.reflex.valor, f.reflex.confianca, f.reflex.evidencia
    );
    for a in &f.anticheats {
        println!(
            "   anticheat: {} kernel={} {:?} {:?}",
            a.nome, a.kernel, a.confianca, a.evidencias
        );
    }
}

#[test]
#[ignore = "varre a máquina real"]
fn encontra_lol_e_wardogs() {
    let rel = varrer_bibliotecas();
    for falha in &rel.falhas {
        println!("falha em {}: {}", falha.launcher, falha.erro);
    }
    let servicos = ServicosSistema::ler();
    let mut vistos = Vec::new();
    for j in &rel.jogos {
        let f = analisar_jogo(j, &servicos);
        imprimir(j, &f);
        vistos.push((j.clone(), f));
    }

    let (lol, flol) = vistos
        .iter()
        .find(|(j, _)| j.nome == "League of Legends")
        .expect("League of Legends não encontrado");
    assert_eq!(lol.launcher, Launcher::Riot);
    assert!(
        flol.exe_principal
            .as_ref()
            .unwrap()
            .ends_with("League of Legends.exe")
    );
    assert_eq!(flol.api.valor, Some(ApiGrafica::Dx11));
    assert!(!flol.dlss.valor.presente());
    assert!(
        flol.anticheats
            .iter()
            .any(|a| a.nome == "Riot Vanguard" && a.kernel)
    );

    let (wd, fwd) = vistos
        .iter()
        .find(|(j, _)| j.nome.eq_ignore_ascii_case("WARDOGS"))
        .expect("WARDOGS não encontrado");
    assert_eq!(wd.launcher, Launcher::Steam);
    assert_eq!(wd.app_id.as_deref(), Some("1867240"));
    assert!(
        fwd.exe_principal
            .as_ref()
            .unwrap()
            .to_string_lossy()
            .to_lowercase()
            .ends_with("wardogsclient-win64-shipping.exe")
    );
    assert_eq!(fwd.api.valor, Some(ApiGrafica::Dx12));
    assert!(
        fwd.imports_ofuscados,
        "imports do WARDOGS deveriam ser ilegíveis"
    );
    let v = fwd
        .dlss
        .valor
        .super_resolution
        .as_ref()
        .and_then(|d| d.versao.clone());
    assert!(
        v.as_deref().is_some_and(|v| v.starts_with("310.")),
        "DLSS {v:?}"
    );
    assert!(fwd.reflex.valor);
    assert!(
        fwd.anticheats
            .iter()
            .any(|a| a.nome == "Elytra" && a.kernel)
    );
    assert!(!fwd.anticheats.iter().any(|a| a.nome == "Riot Vanguard"));
}
