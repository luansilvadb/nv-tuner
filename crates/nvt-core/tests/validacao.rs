//! Testes de parsing, validação e resolução de conflitos com regras sintéticas.

use nvt_core::{
    Alvo, Arquitetura, Capabilities, Catalogo, ConjuntoRegras, ErroRegra, GameFacts, Origem,
    Preferences, SETTINGS_TOML, avaliar,
};

fn catalogo() -> Catalogo {
    Catalogo::from_toml("settings.toml", SETTINGS_TOML).expect("catálogo real válido")
}

fn carregar(regras: &str) -> Result<ConjuntoRegras, Vec<ErroRegra>> {
    ConjuntoRegras::carregar(catalogo(), &[("teste.toml", regras)]).map_err(|e| e.0)
}

fn erros(regras: &str) -> Vec<ErroRegra> {
    carregar(regras).expect_err("deveria falhar")
}

/// Regra mínima válida com corpo extra (condições/ações) anexado.
fn regra(id: &str, escopo: &str, extra: &str) -> String {
    format!(
        r#"
[[regra]]
id = "{id}"
descricao = "teste"
status = "estavel"
escopo = "{escopo}"
impacto_visual = "nenhum"
motivo = "m"
evidencia = "e"
{extra}
"#
    )
}

fn arquivo(regras: &[String]) -> String {
    format!("versao_schema = 1\n{}", regras.concat())
}

#[test]
fn regra_minima_valida() {
    let r = carregar(&arquivo(&[regra(
        "ok",
        "global",
        "[regra.aplicar]\nenergia = \"desempenho_maximo\"\n",
    )]))
    .unwrap();
    assert_eq!(r.regras.len(), 1);
    assert_eq!(r.regras[0].prioridade, 0);
}

#[test]
fn setting_desconhecida() {
    let e = erros(&arquivo(&[regra(
        "x",
        "global",
        "[regra.aplicar]\nturbo = \"on\"\n",
    )]));
    assert!(
        matches!(&e[..], [ErroRegra::SettingDesconhecida { setting, .. }] if setting == "turbo")
    );
    assert!(e[0].to_string().contains("não existe no catálogo"));
}

#[test]
fn valor_nomeado_invalido() {
    let e = erros(&arquivo(&[regra(
        "x",
        "global",
        "[regra.aplicar]\nenergia = \"turbo\"\n",
    )]));
    assert!(
        matches!(&e[..], [ErroRegra::ValorInvalido { aceitos, .. }] if aceitos.contains("desempenho_maximo"))
    );
}

#[test]
fn valor_numerico_fora_da_faixa_e_negativo() {
    let quando = "[regra.quando]\nlimite_fps_definido = false\n";
    for v in ["5", "-1", "2000"] {
        let e = erros(&arquivo(&[regra(
            "x",
            "global",
            &format!("{quando}[regra.aplicar]\nlimite_fps = {v}\n"),
        )]));
        assert!(
            matches!(&e[..], [ErroRegra::ValorInvalido { .. }]),
            "{v}: {e:?}"
        );
    }
    let ok = carregar(&arquivo(&[regra(
        "x",
        "global",
        &format!("{quando}[regra.aplicar]\nlimite_fps = 141\n"),
    )]));
    assert!(ok.is_ok());
}

#[test]
fn numero_igual_a_valor_nomeado_e_aceito() {
    assert!(
        carregar(&arquivo(&[regra(
            "x",
            "global",
            "[regra.aplicar]\nenergia = 1\n"
        )]))
        .is_ok()
    );
}

#[test]
fn condicao_com_nome_desconhecido() {
    let e = erros(&arquivo(&[regra(
        "x",
        "global",
        "[regra.quando]\narquitetura = [\"pascal\"]\n[regra.aplicar]\nenergia = \"desempenho_maximo\"\n",
    )]));
    assert!(
        matches!(&e[..], [ErroRegra::CondicaoInvalida { mensagem, .. }] if mensagem.contains("pascal"))
    );
}

#[test]
fn condicao_desconhecida_e_erro_de_toml() {
    let e = erros(&arquivo(&[regra(
        "x",
        "global",
        "[regra.quando]\ntem_rtx = true\n[regra.aplicar]\nenergia = \"desempenho_maximo\"\n",
    )]));
    assert!(matches!(&e[..], [ErroRegra::Toml { mensagem, .. }] if mensagem.contains("tem_rtx")));
}

#[test]
fn condicao_de_jogo_em_regra_global() {
    let e = erros(&arquivo(&[regra(
        "x",
        "global",
        "[regra.quando]\njogo_dlss = true\n[regra.aplicar]\nenergia = \"desempenho_maximo\"\n",
    )]));
    assert!(matches!(&e[..], [ErroRegra::CondicaoInvalida { .. }]));
}

#[test]
fn condicoes_contraditorias() {
    let e = erros(&arquivo(&[regra(
        "x",
        "por_jogo",
        "[regra.quando]\njogo_dlss = false\njogo_dlss_min = \"310.0\"\n[regra.aplicar]\nenergia = \"desempenho_maximo\"\n",
    )]));
    assert!(matches!(&e[..], [ErroRegra::CondicaoInvalida { .. }]));
}

#[test]
fn versao_invalida_em_condicao() {
    let e = erros(&arquivo(&[regra(
        "x",
        "global",
        "[regra.quando]\ndriver_min = \"abc\"\n[regra.aplicar]\nenergia = \"desempenho_maximo\"\n",
    )]));
    assert!(
        matches!(&e[..], [ErroRegra::Toml { mensagem, .. }] if mensagem.contains("versão inválida"))
    );
}

#[test]
fn setting_governada_exige_condicao_na_preferencia() {
    let e = erros(&arquivo(&[regra(
        "x",
        "global",
        "[regra.aplicar]\nvsync = \"force_off\"\n",
    )]));
    assert!(matches!(&e[..], [ErroRegra::SettingGovernada { setting, .. }] if setting == "vsync"));
}

#[test]
fn refresh_menos_exige_refresh_conhecido() {
    let e = erros(&arquivo(&[regra(
        "x",
        "global",
        "[regra.quando]\nsincronismo = [\"gsync\"]\n[regra.aplicar]\nlimite_fps = { refresh_menos = 3 }\n",
    )]));
    assert!(
        matches!(&e[..], [ErroRegra::CondicaoInvalida { mensagem, .. }] if mensagem.contains("refresh_conhecido"))
    );
}

#[test]
fn campos_obrigatorios_e_ids() {
    let sem_motivo = arquivo(&[regra("x", "global", "[regra.aplicar]\nenergia = 1\n")
        .replace("motivo = \"m\"", "motivo = \" \"")]);
    assert!(matches!(
        &erros(&sem_motivo)[..],
        [ErroRegra::CampoVazio {
            campo: "motivo",
            ..
        }]
    ));

    let id_ruim = arquivo(&[regra(
        "Com Espaço",
        "global",
        "[regra.aplicar]\nenergia = 1\n",
    )]);
    assert!(matches!(
        &erros(&id_ruim)[..],
        [ErroRegra::IdInvalido { .. }]
    ));

    let dup = arquivo(&[
        regra("a", "global", "[regra.aplicar]\nenergia = 1\n"),
        regra("a", "global", "[regra.aplicar]\nenergia = 0\n"),
    ]);
    assert!(matches!(
        &erros(&dup)[..],
        [ErroRegra::RegraDuplicada { .. }]
    ));

    let sem_status = arquivo(&[regra("a", "global", "[regra.aplicar]\nenergia = 1\n")
        .replace("status = \"estavel\"\n", "")]);
    assert!(matches!(&erros(&sem_status)[..], [ErroRegra::Toml { .. }]));
}

#[test]
fn reporta_varios_erros_de_uma_vez() {
    let e = erros(&arquivo(&[
        regra("a", "global", "[regra.aplicar]\nturbo = 1\n"),
        regra("b", "global", "[regra.aplicar]\nenergia = \"x\"\n"),
    ]));
    assert_eq!(e.len(), 2);
}

#[test]
fn versao_de_schema_errada() {
    let e = erros("versao_schema = 2\n");
    assert!(matches!(
        &e[..],
        [ErroRegra::VersaoSchema { encontrada: 2, .. }]
    ));
}

#[test]
fn catalogo_invalido() {
    let toml = r#"
versao_schema = 1
[[setting]]
nome = "a"
id = 1
descricao = "x"
valores = { on = 1 }
[[setting]]
nome = "B"
id = 1
descricao = "x"
"#;
    let e = Catalogo::from_toml("settings.toml", toml).unwrap_err().0;
    // "B": nome inválido, sem valores, ID repetido.
    assert_eq!(e.len(), 3, "{e:?}");
}

#[test]
fn conflito_resolvido_por_especificidade_prioridade_e_id() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let prefs = Preferences::default();
    let aplicar = |v: &str| format!("[regra.aplicar]\nenergia = \"{v}\"\n");

    // Mais condições vence menos, mesmo com prioridade menor.
    let r = carregar(&arquivo(&[
        regra(
            "geral",
            "global",
            &format!("prioridade = 99\n{}", aplicar("adaptativo")),
        ),
        regra(
            "especifica",
            "global",
            &format!(
                "[regra.quando]\narquitetura = [\"turing\"]\n{}",
                aplicar("otimizado")
            ),
        ),
    ]))
    .unwrap();
    let plan = avaliar(&caps, &[], &prefs, &r);
    assert_eq!(plan.mudanca(&Alvo::Global, "energia").unwrap().valor, 5);
    assert!(
        plan.descartes
            .iter()
            .any(|d| d.regra.as_deref() == Some("geral"))
    );

    // Mesma especificidade: maior prioridade vence.
    let r = carregar(&arquivo(&[
        regra(
            "a",
            "global",
            &format!("prioridade = 1\n{}", aplicar("adaptativo")),
        ),
        regra(
            "b",
            "global",
            &format!("prioridade = 2\n{}", aplicar("otimizado")),
        ),
    ]))
    .unwrap();
    let plan = avaliar(&caps, &[], &prefs, &r);
    assert_eq!(plan.mudanca(&Alvo::Global, "energia").unwrap().valor, 5);

    // Empate total: menor id vence, independente da ordem no arquivo.
    for ordem in [["b", "a"], ["a", "b"]] {
        let regras: Vec<String> = ordem
            .iter()
            .map(|id| {
                let v = if *id == "a" {
                    "adaptativo"
                } else {
                    "otimizado"
                };
                regra(id, "global", &aplicar(v))
            })
            .collect();
        let plan = avaliar(&caps, &[], &prefs, &carregar(&arquivo(&regras)).unwrap());
        let m = plan.mudanca(&Alvo::Global, "energia").unwrap();
        assert_eq!(m.valor, 0);
        assert_eq!(m.origem, Origem::Regra { id: "a".into() });
    }
}

#[test]
fn regra_por_jogo_casa_por_fatos() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let r = carregar(&arquivo(&[regra(
        "anticheat",
        "por_jogo",
        "[regra.quando]\njogo_anticheat_kernel = true\njogo_api = [\"dx11\"]\n[regra.aplicar]\nenergia = 1\n",
    )]))
    .unwrap();
    let mut com = GameFacts::new("com");
    com.anticheat_kernel = Some(true);
    com.api = nvt_core::Api::Dx11;
    let mut desconhecido = GameFacts::new("desconhecido");
    desconhecido.api = nvt_core::Api::Dx11;
    let plan = avaliar(&caps, &[com, desconhecido], &Preferences::default(), &r);
    assert!(plan.por_jogo.contains_key("com"));
    assert!(!plan.por_jogo.contains_key("desconhecido"));
    assert!(plan.global.is_empty());
}

/// `refresh_menos` usa o piso do refresh: 59,94 Hz conta como 59 (− 3 = 56),
/// nunca como 60 (− 3 = 57, a menos de 3 FPS do refresh real).
#[test]
fn refresh_menos_usa_piso_do_refresh() {
    let regras = carregar(&arquivo(&[regra(
        "x",
        "global",
        "[regra.quando]\nsincronismo = [\"gsync\"]\nrefresh_conhecido = true\n[regra.aplicar]\nlimite_fps = { refresh_menos = 3 }\n",
    )]))
    .expect("regra válida");
    for (mhz, esperado, exibido) in [
        (59_940, 56, Some("≈60 Hz, contado como 59 Hz")),
        (143_980, 140, Some("≈144 Hz, contado como 143 Hz")),
        (144_000, 141, None),
    ] {
        let caps = Capabilities {
            refresh_mhz: Some(mhz),
            ..Capabilities::tipicas(Arquitetura::Ada)
        };
        let plan = avaliar(
            &caps,
            &[],
            &Preferences {
                sincronismo: nvt_core::Sincronismo::GSync,
                ..Preferences::default()
            },
            &regras,
        );
        let m = plan
            .mudanca(&Alvo::Global, "limite_fps")
            .unwrap_or_else(|| panic!("{mhz} mHz: sem limite"));
        assert_eq!(m.valor, esperado, "{mhz} mHz");
        match exibido {
            Some(t) => assert!(m.explicacao.contains(t), "{mhz} mHz: {}", m.explicacao),
            None => assert!(!m.explicacao.contains("contado como"), "{}", m.explicacao),
        }
    }
}
