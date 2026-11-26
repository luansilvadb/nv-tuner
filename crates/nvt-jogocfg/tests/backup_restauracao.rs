//! F3-04 e F3-07: backup imutável e restauração por hash.

mod comum;

use comum::*;
use nvt_jogocfg::restauracao::{ChaveGravada, Modo, Restauracao, planejar};
use nvt_jogocfg::steam::caminho_launch_options;
use nvt_jogocfg::{Chave, Erro, backup, definir_valor, lancamento, ler_valor, sha256};
use std::fs;

#[test]
fn backup_criado_uma_vez_e_imutavel() {
    let t = Temp::novo("backup");
    let original = amostra("r6_GameSettings.ini");
    let b = backup::criar(&t.0, "r6", "GameSettings.ini", &original).unwrap();
    assert!(b.criado_agora);
    assert_eq!(b.hash, sha256(&original));
    assert!(b.caminho.ends_with(r"r6\GameSettings.ini.orig"));
    assert_eq!(
        fs::read_to_string(t.0.join(r"r6\GameSettings.ini.orig.sha256")).unwrap(),
        b.hash.hex()
    );
    // Idempotente com o mesmo conteúdo.
    let de_novo = backup::criar(&t.0, "r6", "GameSettings.ini", &original).unwrap();
    assert!(!de_novo.criado_agora);
    // Recusa sobrescrever com conteúdo diferente.
    let e = backup::criar(&t.0, "r6", "GameSettings.ini", b"outro").unwrap_err();
    assert!(matches!(e, Erro::BackupDivergente(_)));
    assert_eq!(
        backup::ler(&t.0, "r6", "GameSettings.ini").unwrap(),
        original
    );
}

#[test]
fn backup_corrompido_e_detectado_e_nomes_invalidos_recusados() {
    let t = Temp::novo("backup-corr");
    backup::criar(&t.0, "fortnite", "GameUserSettings.ini", b"[s]\nA=1\n").unwrap();
    fs::write(
        t.0.join(r"fortnite\GameUserSettings.ini.orig"),
        b"adulterado",
    )
    .unwrap();
    assert!(matches!(
        backup::ler(&t.0, "fortnite", "GameUserSettings.ini"),
        Err(Erro::BackupCorrompido(_))
    ));
    for (j, n) in [
        ("..", "a"),
        ("r6", r"..\x"),
        ("r6", "a/b"),
        ("", "a"),
        ("r6", ".oculto"),
    ] {
        assert!(backup::criar(&t.0, j, n, b"x").is_err(), "{j} {n}");
    }
}

#[test]
fn backup_recusa_pasta_juncao() {
    let t = Temp::novo("backup-juncao");
    let fora = t.0.join("fora");
    fs::create_dir(&fora).unwrap();
    juncao(&t.0.join("r6"), &fora);
    assert!(matches!(
        backup::criar(&t.0, "r6", "GameSettings.ini", b"x"),
        Err(Erro::ReparseRecusado(_))
    ));
    assert_eq!(fs::read_dir(&fora).unwrap().count(), 0);
}

fn r6() -> (Vec<u8>, Chave, Vec<u8>, ChaveGravada) {
    let original = amostra("r6_GameSettings.ini");
    let chave = Chave::ini("DISPLAY", "FPSLimit");
    let gravado = definir_valor(&original, &chave, "141").unwrap();
    let g = ChaveGravada {
        chave: chave.clone(),
        modo: Modo::Valor,
        valor_antes: "0".into(),
        valor_gravado: "141".into(),
    };
    (original, chave, gravado, g)
}

#[test]
fn hash_igual_devolve_o_original_inteiro() {
    let (original, _, gravado, g) = r6();
    let r = planejar(&gravado, &original, &sha256(&gravado), &[g]).unwrap();
    assert_eq!(r, Restauracao::Original(original));
}

#[test]
fn jogo_regravou_outra_chave_reverte_so_a_do_app() {
    let (original, chave, gravado, g) = r6();
    let regravado = definir_valor(&gravado, &Chave::ini("AUDIO", "Volume"), "35").unwrap();
    let r = planejar(&regravado, &original, &sha256(&gravado), &[g]).unwrap();
    let Restauracao::Parcial {
        bytes,
        revertidas,
        alteradas_pelo_jogo,
    } = r
    else {
        panic!("esperava parcial");
    };
    assert_eq!(revertidas, vec![chave.clone()]);
    assert!(alteradas_pelo_jogo.is_empty());
    assert_eq!(ler_valor(&bytes, &chave).unwrap(), "0");
    assert_eq!(
        ler_valor(&bytes, &Chave::ini("AUDIO", "Volume")).unwrap(),
        "35"
    );
    assert!(so_trecho_mudou(&regravado, &bytes, b"141", b"0"));
}

#[test]
fn chave_mudada_pelo_jogo_nao_e_tocada() {
    let (original, chave, gravado, g) = r6();
    let mudado = definir_valor(&gravado, &chave, "90").unwrap();
    let e = planejar(
        &mudado,
        &original,
        &sha256(&gravado),
        std::slice::from_ref(&g),
    )
    .unwrap_err();
    assert!(matches!(e, Erro::ArquivoAlteradoPeloJogo));

    // Com duas chaves, uma mudada pelo jogo: só a outra volta e a mudada é informada.
    let c2 = Chave::ini("DISPLAY", "VSync");
    let gravado2 = definir_valor(&gravado, &c2, "1").unwrap();
    let g2 = ChaveGravada {
        chave: c2.clone(),
        modo: Modo::Valor,
        valor_antes: "0".into(),
        valor_gravado: "1".into(),
    };
    let mudado = definir_valor(&gravado2, &chave, "90").unwrap();
    let Restauracao::Parcial {
        bytes,
        revertidas,
        alteradas_pelo_jogo,
    } = planejar(&mudado, &original, &sha256(&gravado2), &[g, g2]).unwrap()
    else {
        panic!("esperava parcial");
    };
    assert_eq!(revertidas, vec![c2.clone()]);
    assert_eq!(alteradas_pelo_jogo, vec![chave.clone()]);
    assert_eq!(ler_valor(&bytes, &chave).unwrap(), "90");
    assert_eq!(ler_valor(&bytes, &c2).unwrap(), "0");
}

#[test]
fn launch_options_restaura_so_o_token() {
    let original = amostra("localconfig.vdf");
    let chave = caminho_launch_options(730);
    let antes = ler_valor(&original, &chave).unwrap();
    let novo = lancamento::definir_fps_max(&antes, 240).unwrap();
    let gravado = definir_valor(&original, &chave, &novo).unwrap();
    // O usuário (Steam) acrescentou uma opção depois da gravação.
    let atual = definir_valor(&gravado, &chave, &format!("{novo} -high")).unwrap();
    let g = ChaveGravada {
        chave: chave.clone(),
        modo: Modo::TokenLancamento,
        valor_antes: antes.clone(),
        valor_gravado: novo,
    };
    let Restauracao::Parcial { bytes, .. } =
        planejar(&atual, &original, &sha256(&gravado), &[g]).unwrap()
    else {
        panic!("esperava parcial");
    };
    assert_eq!(ler_valor(&bytes, &chave).unwrap(), format!("{antes} -high"));
}
