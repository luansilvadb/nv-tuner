//! F3-01: ida e volta byte a byte e edição que só troca os bytes do valor.

mod comum;

use comum::*;
use nvt_jogocfg::codificacao::{codificar, decodificar};
use nvt_jogocfg::{Chave, Codificacao, Erro, definir_valor, ler_valor};

/// (amostra, chave, valor atual, valor novo)
fn casos() -> Vec<(&'static str, Chave, &'static str, &'static str)> {
    vec![
        (
            "r6_GameSettings.ini",
            Chave::ini("DISPLAY", "FPSLimit"),
            "0",
            "141",
        ),
        (
            "r6_GameSettings.ini",
            Chave::ini("display_settings", "fpslimit"),
            "999",
            "60",
        ),
        (
            "fortnite_GameUserSettings.ini",
            Chave::ini(
                "/Script/FortniteGame.FortGameUserSettings",
                "FrameRateLimit",
            ),
            "240.000000",
            "144.000000",
        ),
        (
            "rl_TASystemSettings.ini",
            Chave::ini("SystemSettings", "OneFrameThreadLag"),
            "True",
            "False",
        ),
        (
            "rl_TASystemSettings.ini",
            Chave::ini("SystemSettings", "StaticDecals"),
            "True",
            "False",
        ),
        (
            "misto_fim_de_linha.ini",
            Chave::ini("Secao", "C"),
            "3",
            "42",
        ),
        ("misto_fim_de_linha.ini", Chave::ini("Secao", "A"), "1", "0"),
        (
            "localconfig.vdf",
            Chave::vdf("UserLocalConfigStore/Software/Valve/Steam/apps/730/LaunchOptions"),
            "-novid -console +exec \"auto.cfg\"",
            "-novid -console +exec \"auto.cfg\" +fps_max 240",
        ),
        (
            "localconfig.vdf",
            Chave::vdf("UserLocalConfigStore/Software/Valve/Steam/apps/570/LaunchOptions"),
            "",
            "+fps_max 240",
        ),
        (
            "cs2_video.txt",
            Chave::vdf("video.cfg/setting.fps_max"),
            "0",
            "300",
        ),
    ]
}

const TODAS: &[&str] = &[
    "r6_GameSettings.ini",
    "fortnite_GameUserSettings.ini",
    "rl_TASystemSettings.ini",
    "rl_TASystemSettings_utf16le.ini",
    "misto_fim_de_linha.ini",
    "localconfig.vdf",
    "loginusers.vdf",
    "cs2_video.txt",
];

#[test]
fn ida_e_volta_byte_a_byte_em_todas_as_amostras_e_codificacoes() {
    for nome in TODAS {
        let b = amostra(nome);
        let t = decodificar(&b).unwrap();
        assert_eq!(codificar(t.codificacao, &t.texto), b, "{nome}");
        for c in CODIFICACOES {
            for texto in [t.texto.clone(), t.texto.replace("\r\n", "\n")] {
                let v = codificar(c, &texto);
                let d = decodificar(&v).unwrap();
                assert_eq!(d.codificacao, c);
                assert_eq!(codificar(d.codificacao, &d.texto), v, "{nome} {c:?}");
            }
        }
    }
}

#[test]
fn amostra_utf16le_e_reconhecida_e_editada() {
    let b = amostra("rl_TASystemSettings_utf16le.ini");
    assert_eq!(
        decodificar(&b).unwrap().codificacao,
        Codificacao::Utf16LeBom
    );
    let chave = Chave::ini("SystemSettings", "OneFrameThreadLag");
    assert_eq!(ler_valor(&b, &chave).unwrap(), "True");
    let novo = definir_valor(&b, &chave, "False").unwrap();
    let c = Codificacao::Utf16LeBom;
    assert!(so_trecho_mudou(
        &b,
        &novo,
        &corpo(c, "True"),
        &corpo(c, "False")
    ));
}

#[test]
fn so_os_bytes_do_valor_mudam_em_todas_as_codificacoes_e_fins_de_linha() {
    for (nome, chave, velho, novo) in casos() {
        let original = decodificar(&amostra(nome)).unwrap().texto;
        let lf = original.replace("\r\n", "\n");
        for texto in [original.clone(), lf] {
            for c in CODIFICACOES {
                let b = codificar(c, &texto);
                assert_eq!(ler_valor(&b, &chave).unwrap(), velho, "{nome} {chave}");
                let depois = definir_valor(&b, &chave, novo).unwrap();
                assert_eq!(ler_valor(&depois, &chave).unwrap(), novo);
                assert_eq!(decodificar(&depois).unwrap().codificacao, c);
                let (bv, bn) = bruto(nome, velho, novo);
                assert!(
                    so_trecho_mudou(&b, &depois, &corpo(c, &bv), &corpo(c, &bn)),
                    "{nome} {chave} {c:?}"
                );
                // ida e volta: gravar o valor antigo devolve os bytes originais.
                assert_eq!(definir_valor(&depois, &chave, velho).unwrap(), b);
            }
        }
    }
}

/// Forma do valor dentro do arquivo (VDF entre aspas usa escapes).
fn bruto(nome: &str, velho: &str, novo: &str) -> (String, String) {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    if nome.ends_with(".vdf") || nome.ends_with(".txt") {
        (esc(velho), esc(novo))
    } else {
        (velho.into(), novo.into())
    }
}

#[test]
fn chave_ausente_nao_e_criada() {
    let b = amostra("r6_GameSettings.ini");
    for chave in [
        Chave::ini("DISPLAY", "NaoExiste"),
        Chave::ini("SemSecao", "FPSLimit"),
    ] {
        assert!(matches!(
            definir_valor(&b, &chave, "1"),
            Err(Erro::ChaveAusente(_))
        ));
    }
    let v = amostra("localconfig.vdf");
    let e = definir_valor(
        &v,
        &Chave::vdf("UserLocalConfigStore/Software/Valve/Steam/apps/1422450/LaunchOptions"),
        "+fps_max 120",
    )
    .unwrap_err();
    assert!(matches!(e, Erro::ChaveAusente(_)));
    assert_eq!(e.motivo(), nvt_jogocfg::Motivo::FormatoInesperado);
    // Bloco não é folha.
    assert!(definir_valor(&v, &Chave::vdf("UserLocalConfigStore/Software"), "x").is_err());
}

#[test]
fn formato_irreconhecivel_aborta() {
    let ini = Chave::ini("Secao", "A");
    let e = definir_valor(&amostra("invalido_linha.ini"), &ini, "2").unwrap_err();
    assert!(matches!(e, Erro::FormatoInesperado(_)), "{e}");
    let e = definir_valor(
        &amostra("invalido_chave_duplicada.ini"),
        &Chave::ini("DISPLAY", "FPSLimit"),
        "2",
    )
    .unwrap_err();
    assert!(matches!(e, Erro::FormatoInesperado(_)), "{e}");
    for nome in ["invalido_desbalanceado.vdf", "invalido_duplicado.vdf"] {
        let e = definir_valor(&amostra(nome), &Chave::vdf("Raiz/a"), "2").unwrap_err();
        assert!(matches!(e, Erro::FormatoInesperado(_)), "{nome}: {e}");
    }
    // Outras formas irreconhecíveis.
    for t in [
        "\"a\" { \"b\" }",
        "\"a\" \"1\" }",
        "\"a\" [$WIN32] \"1\"",
        "\"a\" \"sem fim",
        "{ \"a\" \"1\" }",
    ] {
        assert!(nvt_jogocfg::vdf::folhas(t).is_err(), "{t}");
    }
    for t in ["[sec\nA=1\n", "[s]\n=1\n", "[s]\nA=1\rB=2\n"] {
        assert!(nvt_jogocfg::ini::ler(t, "s", "A").is_err(), "{t:?}");
    }
}

#[test]
fn codificacoes_nao_suportadas_sao_recusadas() {
    let sem_bom: Vec<u8> = "[s]\nA=1\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut be = vec![0xFE, 0xFF];
    be.extend("[s]".encode_utf16().flat_map(u16::to_be_bytes));
    let impar = vec![0xFF, 0xFE, 0x41];
    let utf8_invalido = b"[s]\nA=\xff\n".to_vec();
    let surrogate = vec![0xFF, 0xFE, 0x00, 0xD8, 0x41, 0x00];
    let bom_duplo = [&[0xEF, 0xBB, 0xBF][..], &[0xEF, 0xBB, 0xBF], b"[s]"].concat();
    for b in [sem_bom, be, impar, utf8_invalido, surrogate, bom_duplo] {
        assert!(
            matches!(decodificar(&b), Err(Erro::FormatoInesperado(_))),
            "{b:?}"
        );
        assert!(definir_valor(&b, &Chave::ini("s", "A"), "2").is_err());
    }
}

#[test]
fn valor_novo_invalido_e_recusado_sem_alterar() {
    let b = amostra("r6_GameSettings.ini");
    let c = Chave::ini("DISPLAY", "FPSLimit");
    for v in ["1\r\n[X]", " 1", "1\t", "a\nb"] {
        assert!(definir_valor(&b, &c, v).is_err(), "{v:?}");
    }
    // Valor sem aspas no VDF só aceita token simples.
    let t = "\"r\"\n{\n\tchave valor\n}\n";
    assert_eq!(nvt_jogocfg::vdf::ler(t, &["r", "chave"]).unwrap(), "valor");
    let novo = nvt_jogocfg::vdf::definir(t, &["r", "chave"], "outro").unwrap();
    assert_eq!(novo, "\"r\"\n{\n\tchave outro\n}\n");
    assert!(nvt_jogocfg::vdf::definir(t, &["r", "chave"], "com espaco").is_err());
}

#[test]
fn vdf_escapes_e_comentarios_preservados() {
    let t = texto("localconfig.vdf");
    let c = [
        "UserLocalConfigStore",
        "Software",
        "Valve",
        "Steam",
        "apps",
        "730",
        "LaunchOptions",
    ];
    let novo = nvt_jogocfg::vdf::definir(&t, &c, "a \\ \"b\"").unwrap();
    assert!(novo.contains("\"LaunchOptions\"\t\t\"a \\\\ \\\"b\\\"\""));
    assert!(novo.contains("// comentário sintético"));
    assert_eq!(nvt_jogocfg::vdf::ler(&novo, &c).unwrap(), "a \\ \"b\"");
    // Chaves sem caixa ASCII.
    let mut minusc: Vec<String> = c.iter().map(|s| s.to_ascii_lowercase()).collect();
    minusc[6] = "launchoptions".into();
    let m: Vec<&str> = minusc.iter().map(String::as_str).collect();
    assert_eq!(
        nvt_jogocfg::vdf::ler(&t, &m).unwrap(),
        "-novid -console +exec \"auto.cfg\""
    );
}

#[test]
fn ini_preserva_espacos_ao_redor_do_valor() {
    let b = amostra("rl_TASystemSettings.ini");
    let c = Chave::ini("SystemSettings", "OneFrameThreadLag");
    let depois = definir_valor(&b, &c, "False").unwrap();
    let s = String::from_utf8(depois).unwrap();
    assert!(s.contains("OneFrameThreadLag\t=\tFalse  \r\n"), "{s:?}");
}
