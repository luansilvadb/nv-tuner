use std::collections::BTreeSet;

use nvt_core::usuario::{
    self, ConjuntoRegrasUsuario, FatosUsuario, GestaoUsuario, MotivoRespeitadaUsuario,
    RefreshRacional, SettingUsuario, TelaUsuario, ValorUsuario, WINDOWS_TOML,
};
use nvt_core::{
    Alvo, Arquitetura, Capabilities, CodigoExplicacao, ConjuntoRegras, EscolhaUsuario,
    EstadoExperimental, EstadoVrr, Preferences, Qualidade, Sincronismo, Status, ValorAcao,
    avaliar_com_aceites,
};

const REFRESH: &str = "windows-refresh-maximo";
const MOUSE: &str = "windows-mouse-sem-aceleracao";
const SWAP: &str = "windows-otimizacoes-janela";
const CAPTURA: &str = "windows-captura-historica-off";
const DRS: &str = "sincronismo-gsync-limite";
const ANTIGA: RefreshRacional = RefreshRacional {
    numerador: 60_000,
    denominador: 1001,
};
const MAXIMA: RefreshRacional = RefreshRacional {
    numerador: 120_000,
    denominador: 1001,
};

fn fatos() -> FatosUsuario {
    FatosUsuario {
        windows_build: Some(22000),
        tela: Some(TelaUsuario {
            alvo: "monitor".into(),
            atual: ANTIGA,
            maximo_nativo: Some(MAXIMA),
        }),
        mouse_aceleracao: Some([6, 10, 1]),
        dx_swap_effect_upgrade: Some(ValorUsuario::Ausente),
        gamedvr_captura_historica: Some(ValorUsuario::Dword(1)),
        ..Default::default()
    }
}

#[test]
fn valida_catalogo_e_regras_windows() {
    let drs = ConjuntoRegras::embutido().unwrap();
    assert!(ConjuntoRegrasUsuario::embutido(&drs).is_ok());
    for (original, invalido) in [
        ("versao_schema = 2", "versao_schema = 1"),
        (REFRESH, "Windows_Refresh"),
        (REFRESH, MOUSE),
        (REFRESH, DRS),
        ("escopo = \"usuario\"", "escopo = \"global\""),
        ("aplicar = { tela_refresh", "aplicar = { bcd"),
        ("tela_refresh = \"maximo_nativo\"", "tela_refresh = 1"),
        ("tela_refresh = \"maximo_nativo\"", "tela_refresh = \"on\""),
        ("off = 0", "off = \"0\""),
        ("off = 0", "off = 2"),
        ("windows_build_min = 22000", "windows_build_min = \"22000\""),
    ] {
        let toml = WINDOWS_TOML.replacen(original, invalido, 1);
        assert_ne!(toml, WINDOWS_TOML);
        assert!(
            ConjuntoRegrasUsuario::carregar("teste.toml", &toml, &drs).is_err(),
            "{invalido}"
        );
    }
}

#[test]
fn predicados_windows_requerem_leituras_conhecidas() {
    let regras = ConjuntoRegrasUsuario::embutido(&ConjuntoRegras::embutido().unwrap()).unwrap();
    let ids = |f: &FatosUsuario| {
        usuario::avaliar(f, Qualidade::Fidelidade, &regras, &BTreeSet::new())
            .experimentais
            .into_iter()
            .map(|s| s.regra)
            .collect::<Vec<_>>()
    };
    assert!(ids(&FatosUsuario::default()).is_empty());
    let mut f = fatos();
    let todos = vec![REFRESH, MOUSE, SWAP, CAPTURA];
    assert_eq!(ids(&f), todos);
    f.dx_swap_effect_upgrade = Some(ValorUsuario::Token("0".into()));
    assert_eq!(ids(&f), todos);
    f.windows_build = None;
    assert_eq!(ids(&f), [REFRESH, MOUSE, CAPTURA]);
    f.windows_build = Some(21999);
    assert_eq!(ids(&f), [REFRESH, MOUSE, CAPTURA]);
    f.windows_build = Some(22000);
    f.dx_swap_effect_upgrade = None;
    assert_eq!(ids(&f), [REFRESH, MOUSE, CAPTURA]);
    f.dx_swap_effect_upgrade = Some(ValorUsuario::Token("1".into()));
    f.gamedvr_captura_historica = Some(ValorUsuario::Ausente);
    f.mouse_aceleracao = Some([6, 10, 0]);
    f.tela.as_mut().unwrap().atual = MAXIMA;
    assert!(ids(&f).is_empty());
}

#[test]
fn aceite_windows_mantem_correcao_e_preserva_deriva() {
    let regras = ConjuntoRegrasUsuario::embutido(&ConjuntoRegras::embutido().unwrap()).unwrap();
    let mut f = fatos();
    let aceites = [REFRESH, MOUSE, SWAP, CAPTURA].map(str::to_owned).into();
    let avaliar = |f: &FatosUsuario, aceites: &BTreeSet<String>| {
        usuario::avaliar(f, Qualidade::Fidelidade, &regras, aceites)
    };
    let sugestoes = avaliar(&f, &BTreeSet::new());
    assert!(sugestoes.resumo.mudancas.is_empty());
    assert_eq!(
        sugestoes.resumo.estado_experimental[SWAP],
        EstadoExperimental::Aceitavel
    );
    let plano = avaliar(&f, &aceites);
    assert_eq!(plano.resumo.mudancas.len(), 4);
    for (&setting, m) in &plano.resumo.mudancas {
        assert_eq!(m.codigo, Some(CodigoExplicacao::ExperimentalAceitaPorVoce));
        f.gestao.insert(
            setting,
            GestaoUsuario {
                antes: m.atual.clone(),
                esperado: m.valor.clone(),
                alvo_tela: m.alvo_tela.clone(),
            },
        );
    }
    f.tela.as_mut().unwrap().atual = RefreshRacional {
        numerador: 240_000,
        denominador: 2002,
    };
    f.mouse_aceleracao = Some([0, 0, 0]);
    f.dx_swap_effect_upgrade = Some(ValorUsuario::Token("1".into()));
    f.gamedvr_captura_historica = Some(ValorUsuario::Dword(0));
    assert_eq!(avaliar(&f, &aceites).resumo.mudancas.len(), 4);
    let retirada = avaliar(&f, &BTreeSet::new());
    assert!(retirada.resumo.mudancas.is_empty());
    assert_eq!(retirada.experimentais.len(), 4);

    f.mouse_aceleracao = Some([7, 10, 2]);
    f.windows_build = None;
    let plano = avaliar(&f, &aceites);
    assert_eq!(plano.resumo.mudancas.len(), 2);
    assert_eq!(
        plano.resumo.respeitadas[&SettingUsuario::MouseAceleracao],
        MotivoRespeitadaUsuario::AlteradaForaDaGestao
    );
    assert_eq!(
        plano.resumo.estado_experimental[MOUSE],
        EstadoExperimental::Aceita
    );
    assert_eq!(
        plano.resumo.estado_experimental[SWAP],
        EstadoExperimental::Aceita
    );

    f.tela.as_mut().unwrap().alvo = "outro monitor".into();
    assert!(
        !avaliar(&f, &aceites)
            .resumo
            .mudancas
            .contains_key(&SettingUsuario::TelaRefresh)
    );
}

#[test]
fn retirada_windows_recalcula_o_vencedor() {
    let mut fallback = WINDOWS_TOML
        .split("[[regra]]")
        .nth(2)
        .unwrap()
        .replace(MOUSE, "windows-mouse-reserva")
        .replace("experimental", "estavel");
    fallback.push_str("\nprioridade = -1\n");
    let toml = format!("{WINDOWS_TOML}\n[[regra]]{fallback}");
    let regras =
        ConjuntoRegrasUsuario::carregar("teste.toml", &toml, &ConjuntoRegras::embutido().unwrap())
            .unwrap();
    let aceites = [MOUSE.to_owned()].into();
    let avaliar = |aceites: &BTreeSet<String>| {
        usuario::avaliar(&fatos(), Qualidade::Fidelidade, &regras, aceites)
    };
    assert_eq!(
        avaliar(&aceites).resumo.mudancas[&SettingUsuario::MouseAceleracao].regra,
        MOUSE
    );
    assert_eq!(
        avaliar(&BTreeSet::new()).resumo.mudancas[&SettingUsuario::MouseAceleracao].regra,
        "windows-mouse-reserva"
    );
}

#[test]
fn aceite_drs_respeita_precedencia_filtros_e_retirada() {
    let mut regras = ConjuntoRegras::embutido().unwrap();
    let caps = Capabilities {
        refresh_mhz: Some(240_000),
        vrr: EstadoVrr {
            possivel: Some(true),
            ativo: Some(true),
        },
        ..Capabilities::tipicas(Arquitetura::Ada)
    };
    let mut prefs = Preferences {
        sincronismo: Sincronismo::GSync,
        ..Default::default()
    };
    let aceites = [DRS.to_owned()].into();
    let avaliar =
        |caps: &Capabilities,
         prefs: &Preferences,
         regras: &ConjuntoRegras,
         ids: &BTreeSet<String>| { avaliar_com_aceites(caps, &[], prefs, regras, ids) };
    let sugestao = avaliar(&caps, &prefs, &regras, &BTreeSet::new());
    assert_eq!(
        sugestao.estado_experimental[DRS],
        EstadoExperimental::Aceitavel
    );
    assert!(sugestao.mudanca(&Alvo::Global, "limite_fps").is_none());

    let mut reserva = regras.regras.iter().find(|r| r.id == DRS).unwrap().clone();
    reserva.id = "limite-reserva".into();
    reserva.status = Status::Estavel;
    reserva.prioridade -= 1;
    reserva
        .aplicar
        .insert("limite_fps".into(), ValorAcao::Numero(120));
    regras.regras.push(reserva);
    let aceito = avaliar(&caps, &prefs, &regras, &aceites);
    assert_eq!(
        aceito.mudanca(&Alvo::Global, "limite_fps").unwrap().valor,
        224
    );
    assert_eq!(
        aceito.codigo_explicacao(&Alvo::Global, "limite_fps"),
        Some(CodigoExplicacao::ExperimentalAceitaPorVoce)
    );
    let retirado = avaliar(&caps, &prefs, &regras, &BTreeSet::new());
    assert_eq!(
        retirado.mudanca(&Alvo::Global, "limite_fps").unwrap().valor,
        120
    );
    assert_eq!(
        retirado.codigo_explicacao(&Alvo::Global, "limite_fps"),
        None
    );

    let sem_refresh = Capabilities {
        refresh_mhz: None,
        ..caps.clone()
    };
    let inativo = avaliar(&sem_refresh, &prefs, &regras, &aceites);
    assert_eq!(inativo.estado_experimental[DRS], EstadoExperimental::Aceita);
    assert!(inativo.mudanca(&Alvo::Global, "limite_fps").is_none());

    prefs.limite_fps = Some(144);
    let preferencia = avaliar(&caps, &prefs, &regras, &aceites);
    assert_eq!(
        preferencia
            .mudanca(&Alvo::Global, "limite_fps")
            .unwrap()
            .valor,
        144
    );
    assert_eq!(
        preferencia.codigo_explicacao(&Alvo::Global, "limite_fps"),
        None
    );
    prefs.escolhas_usuario.push(EscolhaUsuario {
        alvo: Alvo::Global,
        setting: "limite_fps".into(),
    });
    assert!(
        avaliar(&caps, &prefs, &regras, &aceites)
            .mudanca(&Alvo::Global, "limite_fps")
            .is_none()
    );
}
