//! F3-11 e N10: edição por token de LaunchOptions.

use nvt_jogocfg::Erro;
use nvt_jogocfg::lancamento::{PROIBIDO, definir_fps_max, ler_fps_max, restaurar};

#[test]
fn acrescenta_preservando_o_resto() {
    assert_eq!(definir_fps_max("", 240).unwrap(), "+fps_max 240");
    assert_eq!(
        definir_fps_max("-novid -console", 240).unwrap(),
        "-novid -console +fps_max 240"
    );
    assert_eq!(
        definir_fps_max("-novid  -console ", 240).unwrap(),
        "-novid  -console +fps_max 240"
    );
    assert_eq!(
        definir_fps_max("-novid +exec \"a b.cfg\"", 60).unwrap(),
        "-novid +exec \"a b.cfg\" +fps_max 60"
    );
}

#[test]
fn troca_so_o_numero() {
    assert_eq!(
        definir_fps_max("-novid +fps_max  300\t-console", 240).unwrap(),
        "-novid +fps_max  240\t-console"
    );
    assert_eq!(definir_fps_max("+FPS_MAX 0", 144).unwrap(), "+FPS_MAX 144");
    assert_eq!(ler_fps_max("-x +fps_max 99").unwrap(), Some(99));
    assert_eq!(ler_fps_max("-x").unwrap(), None);
}

#[test]
fn formas_ambiguas_sao_recusadas() {
    for s in [
        "+fps_max",
        "+fps_max abc",
        "+fps_max 1 +fps_max 2",
        "+fps_max -1",
    ] {
        assert!(
            matches!(definir_fps_max(s, 1), Err(Erro::FormatoInesperado(_))),
            "{s}"
        );
    }
}

#[test]
fn trava_allow_third_party_software_em_qualquer_entrada() {
    for s in [
        "-allow_third_party_software",
        "-novid -allow_third_party_software +fps_max 1",
        "-ALLOW_THIRD_PARTY_SOFTWARE",
        "+fps_max 60 -allow_third_party_software",
    ] {
        assert!(
            matches!(definir_fps_max(s, 240), Err(Erro::TokenProibido)),
            "{s}"
        );
        assert!(matches!(ler_fps_max(s), Err(Erro::TokenProibido)), "{s}");
        assert!(
            matches!(restaurar(s, "", s), Err(Erro::TokenProibido)),
            "{s}"
        );
    }
    // Nenhuma saída contém o token proibido.
    for s in ["", "-novid", "+fps_max 1", "-a -b -c"] {
        for n in [0, 60, 240, u32::MAX] {
            let o = definir_fps_max(s, n).unwrap();
            assert!(!o.to_ascii_lowercase().contains(PROIBIDO));
        }
    }
}

#[test]
fn restaurar_devolve_original_se_a_string_nao_mudou() {
    let original = "-novid  -console ";
    let gravado = definir_fps_max(original, 240).unwrap();
    assert_eq!(restaurar(&gravado, original, &gravado).unwrap(), original);
}

#[test]
fn restaurar_remove_so_o_token_do_app() {
    let gravado = definir_fps_max("-novid", 240).unwrap();
    // O usuário acrescentou algo depois da gravação.
    let atual = format!("{gravado} -high");
    assert_eq!(
        restaurar(&atual, "-novid", &gravado).unwrap(),
        "-novid -high"
    );
    let atual = format!("-low {gravado}");
    assert_eq!(
        restaurar(&atual, "-novid", &gravado).unwrap(),
        "-low -novid"
    );
    let gravado = definir_fps_max("", 240).unwrap();
    assert_eq!(restaurar("+fps_max 240 -x", "", &gravado).unwrap(), "-x");
}

#[test]
fn restaurar_devolve_numero_original_quando_o_token_ja_existia() {
    let original = "-novid +fps_max 300";
    let gravado = definir_fps_max(original, 240).unwrap();
    let atual = format!("{gravado} -high");
    assert_eq!(
        restaurar(&atual, original, &gravado).unwrap(),
        "-novid +fps_max 300 -high"
    );
}

#[test]
fn token_mudado_por_outro_nao_e_tocado() {
    let gravado = definir_fps_max("-novid", 240).unwrap();
    for atual in ["-novid +fps_max 100", "-novid", "-x -y"] {
        assert!(matches!(
            restaurar(atual, "-novid", &gravado),
            Err(Erro::ArquivoAlteradoPeloJogo)
        ));
    }
}
