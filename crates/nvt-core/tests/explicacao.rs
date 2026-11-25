//! Explicação por jogo: regras aplicadas, não aplicadas com motivo
//! estruturado e herança do global, com as regras reais e sintéticas.

use nvt_core::{
    Alvo, Api, Arquitetura, Capabilities, Catalogo, ConjuntoRegras, ErroRegra, EscolhaUsuario,
    GameFacts, MotivoNaoAplicada, Plan, Preferences, Qualidade, RegraAplicada, RegraNaoAplicada,
    ResumoJogo, SETTINGS_TOML, Versao, avaliar,
};

const TURING: &str = "dlss-preset-e-turing";
const AMPERE: &str = "dlss-preset-e-ampere";

fn regras() -> ConjuntoRegras {
    ConjuntoRegras::embutido().unwrap_or_else(|e| panic!("regras reais inválidas:\n{e}"))
}

fn com_dlss(id: &str) -> GameFacts {
    GameFacts {
        id: id.into(),
        api: Api::Dx12,
        dlss: Some(Versao::new(310, 6, 0, 0)),
        reflex: Some(true),
        anticheat_kernel: Some(true),
    }
}

fn sem_dlss(id: &str) -> GameFacts {
    GameFacts {
        id: id.into(),
        api: Api::Dx11,
        dlss: None,
        reflex: Some(false),
        anticheat_kernel: Some(true),
    }
}

fn plano(caps: &Capabilities, jogos: &[GameFacts], prefs: &Preferences) -> Plan {
    avaliar(caps, jogos, prefs, &regras())
}

fn resumo<'a>(p: &'a Plan, id: &str) -> &'a ResumoJogo {
    p.resumos
        .get(id)
        .unwrap_or_else(|| panic!("sem resumo para {id}: {:?}", p.resumos))
}

fn motivo<'a>(r: &'a ResumoJogo, regra: &str) -> &'a MotivoNaoAplicada {
    &r.nao_aplicadas
        .iter()
        .find(|n| n.regra == regra)
        .unwrap_or_else(|| panic!("{regra} não está em nao_aplicadas: {r:?}"))
        .motivo
}

fn dlss_settings() -> Vec<String> {
    ["dlss_override", "dlss_perfil_preset", "dlss_preset"]
        .map(String::from)
        .to_vec()
}

#[test]
fn turing_com_dlss_aplica_preset_e() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let p = plano(&caps, &[com_dlss("WARDOGS")], &Preferences::default());
    let r = resumo(&p, "WARDOGS");
    assert_eq!(
        r.aplicadas,
        vec![RegraAplicada {
            regra: TURING.into(),
            settings: dlss_settings(),
        }]
    );
    // A regra de Ampere não vale para Turing, e não é "arquitetura não precisa".
    assert_eq!(
        motivo(r, AMPERE),
        &MotivoNaoAplicada::CondicaoNaoAtendida {
            condicao: "arquitetura".into(),
            esperado: "ampere".into(),
            encontrado: "turing".into(),
        }
    );
    assert_eq!(r.nao_aplicadas.len(), 1);
    // Herda todo o global (o jogo não sobrescreve nenhuma setting global).
    assert_eq!(r.herda_do_global.len(), p.global.len());
    assert!(r.herda_do_global.contains(&"energia".to_string()));
}

#[test]
fn ampere_com_dlss_e_experimental() {
    let caps = Capabilities::tipicas(Arquitetura::Ampere);
    let p = plano(&caps, &[com_dlss("j")], &Preferences::default());
    let r = resumo(&p, "j");
    assert!(r.aplicadas.is_empty());
    assert_eq!(motivo(r, AMPERE), &MotivoNaoAplicada::Experimental);
    assert!(matches!(
        motivo(r, TURING),
        MotivoNaoAplicada::CondicaoNaoAtendida { condicao, .. } if condicao == "arquitetura"
    ));
}

#[test]
fn ada_e_blackwell_com_dlss_nao_precisam() {
    for arq in [Arquitetura::Ada, Arquitetura::Blackwell] {
        let caps = Capabilities::tipicas(arq);
        let p = plano(&caps, &[com_dlss("j")], &Preferences::default());
        let r = resumo(&p, "j");
        assert!(r.aplicadas.is_empty(), "{arq}");
        for regra in [TURING, AMPERE] {
            assert_eq!(
                motivo(r, regra),
                &MotivoNaoAplicada::ArquiteturaNaoPrecisa { arquitetura: arq },
                "{arq} {regra}"
            );
        }
        assert!(!r.herda_do_global.is_empty());
    }
}

#[test]
fn gtx16_sem_tensor_cores() {
    let caps = Capabilities {
        tem_tensor_cores: false,
        ..Capabilities::tipicas(Arquitetura::Turing)
    };
    let p = plano(&caps, &[com_dlss("j")], &Preferences::default());
    let r = resumo(&p, "j");
    // Tensor Cores vêm antes da arquitetura: as duas regras dizem o mesmo.
    assert_eq!(motivo(r, TURING), &MotivoNaoAplicada::SemTensorCores);
    assert_eq!(motivo(r, AMPERE), &MotivoNaoAplicada::SemTensorCores);
}

#[test]
fn jogo_sem_dlss_em_qualquer_gpu() {
    for arq in [
        Arquitetura::Turing,
        Arquitetura::Ampere,
        Arquitetura::Ada,
        Arquitetura::Blackwell,
    ] {
        let caps = Capabilities::tipicas(arq);
        let p = plano(&caps, &[sem_dlss("LoL")], &Preferences::default());
        let r = resumo(&p, "LoL");
        assert!(r.aplicadas.is_empty());
        // Fatos do jogo vêm antes das capacidades.
        assert_eq!(motivo(r, TURING), &MotivoNaoAplicada::JogoSemDlss, "{arq}");
        assert_eq!(motivo(r, AMPERE), &MotivoNaoAplicada::JogoSemDlss, "{arq}");
    }
}

#[test]
fn fidelidade_bloqueia_pelo_teto() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let prefs = Preferences {
        qualidade: Qualidade::Fidelidade,
        ..Default::default()
    };
    let p = plano(&caps, &[com_dlss("j")], &prefs);
    let r = resumo(&p, "j");
    assert!(r.aplicadas.is_empty());
    assert_eq!(
        motivo(r, TURING),
        &MotivoNaoAplicada::BloqueadaPeloTetoDeQualidade {
            teto: Qualidade::Fidelidade,
            impacto: nvt_core::ImpactoVisual::Leve,
        }
    );
}

#[test]
fn escolha_travada_e_do_usuario() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let prefs = Preferences {
        escolhas_usuario: vec![EscolhaUsuario {
            alvo: Alvo::Jogo("j".into()),
            setting: "dlss_preset".into(),
        }],
        ..Default::default()
    };
    let p = plano(&caps, &[com_dlss("j")], &prefs);
    let r = resumo(&p, "j");
    // As outras duas settings seguem aplicadas; a travada aparece com o motivo.
    assert_eq!(
        r.aplicadas,
        vec![RegraAplicada {
            regra: TURING.into(),
            settings: vec!["dlss_override".into(), "dlss_perfil_preset".into()],
        }]
    );
    assert!(r.nao_aplicadas.contains(&RegraNaoAplicada {
        regra: TURING.into(),
        motivo: MotivoNaoAplicada::EscolhaDoUsuario {
            setting: "dlss_preset".into(),
        },
    }));
}

#[test]
fn todo_jogo_tem_resumo_e_heranca_desconta_o_que_o_jogo_sobrescreve() {
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let p = plano(
        &caps,
        &[com_dlss("a"), sem_dlss("b")],
        &Preferences::default(),
    );
    assert_eq!(p.resumos.len(), 2);
    assert_eq!(
        resumo(&p, "b").herda_do_global.len(),
        p.global.len(),
        "o jogo sem regras herda tudo"
    );
}

// ── Regras sintéticas ───────────────────────────────────────────────────────

fn catalogo() -> Catalogo {
    Catalogo::from_toml("settings.toml", SETTINGS_TOML).unwrap()
}

fn carregar(regras: &str) -> Result<ConjuntoRegras, Vec<ErroRegra>> {
    ConjuntoRegras::carregar(catalogo(), &[("teste.toml", regras)]).map_err(|e| e.0)
}

fn regra(id: &str, extra: &str) -> String {
    format!(
        r#"
[[regra]]
id = "{id}"
descricao = "teste"
status = "estavel"
escopo = "por_jogo"
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
fn fato_desconhecido_e_conflito() {
    let r = carregar(&arquivo(&[
        regra(
            "reflex",
            "[regra.quando]\njogo_reflex = true\n[regra.aplicar]\nshader_cache = \"on\"\n",
        ),
        regra("geral", "[regra.aplicar]\nshader_cache = \"off\"\n"),
        regra(
            "especifica",
            "[regra.quando]\njogo_api = [\"dx12\"]\n[regra.aplicar]\nshader_cache = \"on\"\n",
        ),
    ]))
    .unwrap();
    let caps = Capabilities::tipicas(Arquitetura::Turing);
    let jogo = GameFacts {
        api: Api::Dx12,
        ..GameFacts::new("j")
    };
    let p = avaliar(&caps, &[jogo], &Preferences::default(), &r);
    let res = &p.resumos["j"];
    assert_eq!(
        motivo(res, "reflex"),
        &MotivoNaoAplicada::FatoDesconhecido {
            fato: "jogo_reflex".into()
        }
    );
    assert_eq!(
        motivo(res, "geral"),
        &MotivoNaoAplicada::ConflitoComRegraMaisEspecifica {
            regra: "especifica".into()
        }
    );
    assert_eq!(res.aplicadas[0].regra, "especifica");
}

#[test]
fn motivo_declarado_sempre_substitui_o_padrao() {
    let r = carregar(&arquivo(&[regra(
        "x",
        "[regra.quando]\njogo_dlss = true\n\
         [regra.motivo_quando_nao_aplica]\njogo_dlss = \"arquitetura_nao_precisa\"\n\
         [regra.aplicar]\nshader_cache = \"on\"\n",
    )]))
    .unwrap();
    let caps = Capabilities::tipicas(Arquitetura::Ada);
    let p = avaliar(&caps, &[GameFacts::new("j")], &Preferences::default(), &r);
    assert_eq!(
        motivo(&p.resumos["j"], "x"),
        &MotivoNaoAplicada::ArquiteturaNaoPrecisa {
            arquitetura: Arquitetura::Ada
        }
    );
}

#[test]
fn motivo_quando_nao_aplica_invalido() {
    let casos = [
        // Condição ausente em `quando`.
        "[regra.motivo_quando_nao_aplica]\narquitetura = \"sem_tensor_cores\"\n",
        // Motivo desconhecido.
        "[regra.quando]\njogo_dlss = true\n[regra.motivo_quando_nao_aplica]\njogo_dlss = \"qualquer\"\n",
        // Tabela em condição que não é lista.
        "[regra.quando]\njogo_dlss = true\n[regra.motivo_quando_nao_aplica]\njogo_dlss = { ada = \"jogo_sem_dlss\" }\n",
        // Valor que está na lista (nunca falharia).
        "[regra.quando]\narquitetura = [\"ada\"]\n[regra.motivo_quando_nao_aplica]\narquitetura = { ada = \"arquitetura_nao_precisa\" }\n",
        // Valor desconhecido.
        "[regra.quando]\narquitetura = [\"ada\"]\n[regra.motivo_quando_nao_aplica]\narquitetura = { rtx = \"arquitetura_nao_precisa\" }\n",
    ];
    for extra in casos {
        let e = carregar(&arquivo(&[regra(
            "x",
            &format!("{extra}[regra.aplicar]\nshader_cache = \"on\"\n"),
        )]))
        .expect_err(extra);
        assert!(
            e.iter()
                .any(|e| matches!(e, ErroRegra::MotivoInvalido { regra, .. } if regra == "x")),
            "{extra}: {e:?}"
        );
        assert!(e[0].to_string().contains("motivo_quando_nao_aplica"));
    }
}

#[test]
fn falhas_e_casa_concordam() {
    // `casa` é `falhas().is_empty()` para regras por jogo.
    let r = regras();
    for arq in [
        Arquitetura::Turing,
        Arquitetura::Ampere,
        Arquitetura::Ada,
        Arquitetura::Unknown(0x140),
    ] {
        let caps = Capabilities::tipicas(arq);
        for j in [com_dlss("a"), sem_dlss("b"), GameFacts::new("c")] {
            for regra in &r.regras {
                let prefs = Preferences::default();
                let casa = regra.quando.casa(&caps, Some(&j), &prefs);
                let falhas = regra.quando.falhas(&caps, Some(&j), &prefs);
                assert_eq!(casa, falhas.is_empty(), "{} {arq} {}", regra.id, j.id);
            }
        }
    }
}
