use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::*;
use crate::protocolo::{Requisicao, Resposta};
use crate::quadro::{codificar, decodificar};
use crate::usuario::{FatosUsuario, OperacaoUsuario, PlanoAgente};

const H1: &str = "0000000000000000000000000000000000000000000000000000000000000001";
const H2: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

pub(crate) fn destino() -> DestinoJogoIpc {
    DestinoJogoIpc {
        alvo: AlvoIpc::ConfigJogo {
            jogo: "r6-siege".into(),
            arquivo: "game_settings".into(),
        },
        setting: "fps_limit".into(),
        instancia: "conta-3f9a".into(),
    }
}

fn rotulo() -> RotuloCaminho {
    RotuloCaminho(
        r"%USERPROFILE%\Documents\My Games\Rainbow Six - Siege\<conta>\GameSettings.ini".into(),
    )
}

fn fato() -> FatoConfigJogo {
    FatoConfigJogo {
        alvo: destino().alvo,
        instancia: "conta-3f9a".into(),
        rotulo: rotulo(),
        presente: true,
        hash: Some(H1.into()),
        somente_leitura: false,
        na_nuvem: false,
        conta_ambigua: false,
        backup: EstadoBackup::Integro,
        valores: vec![ValorAtualJogo {
            setting: "fps_limit".into(),
            valor: Fato::Conhecido(ValorJogoIpc::Inteiro(0)),
        }],
        gestao: vec![GestaoJogoIpc {
            setting: "fps_limit".into(),
            antes: ValorJogoIpc::Inteiro(0),
            esperado: ValorJogoIpc::Inteiro(141),
            hash_antes: H1.into(),
            hash_depois: H2.into(),
            travado: false,
        }],
    }
}

fn consulta(config_jogo: Vec<FatoConfigJogo>) -> Requisicao {
    Requisicao::AgenteConsultar {
        fatos: FatosUsuario {
            tela: Fato::Desconhecido,
            mouse: Fato::Desconhecido,
            swap_effect_upgrade: Fato::Ausente,
            captura_historica: Fato::Desconhecido,
            diagnosticos: vec![],
            gestao: vec![],
            config_jogo,
        },
        ultima_tarefa: None,
        geracao_restaurada: 0,
    }
}

fn resultado() -> ResultadoJogo {
    ResultadoJogo {
        destino: destino(),
        status: StatusMudanca::Recusada,
        motivo: Some(MotivoNaoAplicada::JanelaFechadaExigida {
            processos: vec!["steam.exe".into()],
        }),
        antes: Some(ValorJogoIpc::Inteiro(0)),
        depois: None,
        hash_antes: Some(H1.into()),
        hash_depois: None,
        chaves_alteradas: vec![],
    }
}

fn relato(resultados_jogo: Vec<ResultadoJogo>) -> Requisicao {
    Requisicao::AgenteRelatar {
        id_tarefa: 1,
        operacao: OperacaoUsuario::Aplicar,
        revisao: 2,
        resultados: vec![],
        resultados_jogo,
        saude: None,
    }
}

fn tarefa(acoes: Vec<AcaoJogo>) -> TarefaJogo {
    TarefaJogo {
        tarefa: crate::ReferenciaTarefa {
            id_tarefa: 1,
            operacao: OperacaoUsuario::Aplicar,
            revisao: 2,
        },
        acoes,
    }
}

fn acoes() -> Vec<AcaoJogo> {
    vec![
        AcaoJogo::Aplicar {
            destino: destino(),
            valor: ValorJogoIpc::Inteiro(141),
            hash_esperado: H1.into(),
        },
        AcaoJogo::Restaurar {
            destino: DestinoJogoIpc {
                setting: "max_fps".into(),
                ..destino()
            },
            hash_esperado: H2.into(),
        },
        AcaoJogo::Reverter {
            destino: DestinoJogoIpc {
                alvo: AlvoIpc::ConfigJogo {
                    jogo: "cs2".into(),
                    arquivo: "localconfig".into(),
                },
                setting: "launch_fps_max".into(),
                instancia: "steam-a1b2".into(),
            },
            hash_esperado: H2.into(),
        },
    ]
}

fn plano() -> PlanoAgente {
    PlanoAgente {
        revisao: 2,
        geracao_restauracao: 0,
        tarefa: None,
        processos_vigiados: Default::default(),
        tarefa_jogo: Some(tarefa(acoes())),
    }
}

fn projecao() -> ConfigJogoUi {
    ConfigJogoUi {
        arquivos: vec![ArquivoJogoUi {
            alvo: destino().alvo,
            rotulo: rotulo(),
            instancia: destino().instancia,
            conta: Some("conta 1 de 2".into()),
            backup: EstadoBackup::Integro,
            mudancas: vec![MudancaArquivoJogo {
                setting: "fps_limit".into(),
                antes: ValorJogoIpc::Inteiro(0),
                depois: ValorJogoIpc::Inteiro(141),
                status: StatusMudanca::Pendente,
                motivo: Some(MotivoNaoAplicada::JanelaFechadaExigida {
                    processos: vec!["steam.exe".into()],
                }),
            }],
            reversivel: true,
        }],
        orientacoes: vec![
            OrientacaoConfigJogo::FecharSteam,
            OrientacaoConfigJogo::FecharJogo {
                processos: vec!["RainbowSix.exe".into()],
            },
            OrientacaoConfigJogo::DesmarcarSomenteLeitura,
            OrientacaoConfigJogo::AjustarNoJogo,
            OrientacaoConfigJogo::EscolherConta,
        ],
        experimentais: vec![SugestaoConfigJogo {
            regra: "jogo-limite-no-jogo-r6".into(),
            descricao: "d".into(),
            impacto_visual: crate::protocolo::ImpactoVisual::Nenhum,
            destinos: vec![destino()],
            estado: EstadoExperimental::Aceitavel,
        }],
    }
}

fn ida_e_volta<T>(v: &T)
where
    T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(v).unwrap();
    assert_eq!(&serde_json::from_str::<T>(&json).unwrap(), v, "{json}");
}

#[test]
fn ida_e_volta_dos_dtos() {
    ida_e_volta(&fato());
    ida_e_volta(&resultado());
    ida_e_volta(&plano());
    ida_e_volta(&projecao());
    for v in [
        ValorJogoIpc::Ausente,
        ValorJogoIpc::Inteiro(-1),
        ValorJogoIpc::Booleano(true),
        ValorJogoIpc::Texto("High".into()),
        ValorJogoIpc::TokenLancamento(None),
        ValorJogoIpc::TokenLancamento(Some(240)),
    ] {
        ida_e_volta(&v);
    }
    for e in [
        EstadoBackup::SemBackup,
        EstadoBackup::Integro,
        EstadoBackup::Comprometido,
    ] {
        ida_e_volta(&e);
    }
}

#[test]
fn ida_e_volta_pelo_envelope_v7() {
    for req in [
        consulta(vec![fato()]),
        relato(vec![resultado()]),
        Requisicao::ReverterConfigJogo { destino: destino() },
    ] {
        assert_eq!(req.validar(), Ok(()));
        let bytes = codificar(&req).unwrap();
        assert!(bytes.starts_with(br#"{"versao":8,"#));
        assert_eq!(decodificar::<Requisicao>(&bytes).unwrap(), req);
    }
    let resp = Resposta::PlanoAgente(plano());
    let bytes = codificar(&resp).unwrap();
    assert_eq!(decodificar::<Resposta>(&bytes).unwrap(), resp);
    assert_eq!(plano().validar(), Ok(()));
}

#[test]
fn formato_json_estavel() {
    assert_eq!(
        serde_json::to_value(destino()).unwrap(),
        json!({
            "alvo": {"tipo":"config_jogo","jogo":"r6-siege","arquivo":"game_settings"},
            "setting":"fps_limit","instancia":"conta-3f9a"
        })
    );
    assert_eq!(
        serde_json::to_value(&acoes()[0]).unwrap()["tipo"],
        json!("aplicar")
    );
    assert_eq!(
        serde_json::to_value(ValorJogoIpc::TokenLancamento(Some(240))).unwrap(),
        json!({"tipo":"token_lancamento","valor":240})
    );
    assert_eq!(
        serde_json::to_value(MotivoNaoAplicada::JanelaFechadaExigida {
            processos: vec!["steam.exe".into()]
        })
        .unwrap(),
        json!({"tipo":"janela_fechada_exigida","processos":["steam.exe"]})
    );
    assert_eq!(
        serde_json::to_value(OrientacaoConfigJogo::FecharSteam).unwrap(),
        json!({"tipo":"fechar_steam"})
    );
}

#[test]
fn campos_opcionais_omitidos_tem_padrao() {
    let f: FatosUsuario = serde_json::from_value(json!({
        "tela":{"estado":"desconhecido"},"mouse":{"estado":"desconhecido"},
        "swap_effect_upgrade":{"estado":"ausente"},
        "captura_historica":{"estado":"desconhecido"},"diagnosticos":[]
    }))
    .unwrap();
    assert!(f.config_jogo.is_empty());
    assert!(
        serde_json::to_value(&f)
            .unwrap()
            .get("config_jogo")
            .is_none()
    );
    let p: PlanoAgente = serde_json::from_value(json!({
        "revisao":1,"geracao_restauracao":0,"tarefa":null,"processos_vigiados":[]
    }))
    .unwrap();
    assert!(p.tarefa_jogo.is_none());
}

fn com_extra(v: impl Serialize) -> Value {
    let mut v = serde_json::to_value(v).unwrap();
    v.as_object_mut()
        .unwrap()
        .insert("caminho".into(), json!(r"C:\Users\fulano\x.ini"));
    v
}

#[test]
fn campos_e_variantes_desconhecidos_recusados() {
    assert!(serde_json::from_value::<FatoConfigJogo>(com_extra(fato())).is_err());
    assert!(serde_json::from_value::<DestinoJogoIpc>(com_extra(destino())).is_err());
    assert!(serde_json::from_value::<AcaoJogo>(com_extra(&acoes()[0])).is_err());
    assert!(serde_json::from_value::<ResultadoJogo>(com_extra(resultado())).is_err());
    assert!(serde_json::from_value::<ConfigJogoUi>(com_extra(projecao())).is_err());
    assert!(serde_json::from_value::<ArquivoJogoUi>(com_extra(&projecao().arquivos[0])).is_err());
    assert!(serde_json::from_value::<TarefaJogo>(com_extra(tarefa(vec![]))).is_err());
    assert!(serde_json::from_value::<GestaoJogoIpc>(com_extra(&fato().gestao[0])).is_err());
    assert!(serde_json::from_value::<AlvoIpc>(json!({"tipo":"perfil_jogo","perfil":"x"})).is_err());
    assert!(
        serde_json::from_value::<AlvoIpc>(
            json!({"tipo":"config_jogo","jogo":"a","arquivo":"b","caminho":"c"})
        )
        .is_err()
    );
    let d = serde_json::to_value(destino()).unwrap();
    assert!(
        serde_json::from_value::<AcaoJogo>(json!({"tipo":"apagar","destino":d,"hash_esperado":H1}))
            .is_err()
    );
    // Aplicar sem hash_esperado.
    assert!(
        serde_json::from_value::<AcaoJogo>(
            json!({"tipo":"aplicar","destino":d,"valor":{"tipo":"inteiro","valor":1}})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<ValorJogoIpc>(json!({"tipo":"flutuante","valor":1.5})).is_err()
    );
    assert!(serde_json::from_value::<EstadoBackup>(json!("apagado")).is_err());
    let mut p = serde_json::to_value(projecao()).unwrap();
    p["orientacoes"][0] = json!({"tipo":"formatar"});
    assert!(serde_json::from_value::<ConfigJogoUi>(p).is_err());
}

#[test]
fn rotulo_so_anonimizado() {
    for ok in [
        r"%USERPROFILE%\Documents\x.ini",
        r"%LOCALAPPDATA%\FortniteGame\Saved\Config\WindowsClient\GameUserSettings.ini",
        r"%STEAM%\userdata\<steamid3>\config\localconfig.vdf",
    ] {
        assert_eq!(RotuloCaminho(ok.into()).validar(), Ok(()), "{ok}");
    }
    for ruim in [
        r"C:\Users\fulano\Documents\x.ini",
        r"\\servidor\c$\x.ini",
        r"%USERPROFILE%\..\fulano\x.ini",
        r"%USERPROFILE%\",
        r"%USERPROFILE%\a\\b.ini",
        r"%USERPROFILE%\a/b.ini",
        r"%USERNAME%\x.ini",
        r"%USERPROFILE%\%USERNAME%\x.ini",
        r"%LOCALAPPDATA%\x\C:\Users\fulano\y",
        "%USERPROFILE%\\a\nb",
    ] {
        assert!(RotuloCaminho(ruim.into()).validar().is_err(), "{ruim}");
    }
    let longo = format!(r"%USERPROFILE%\{}", "a".repeat(ROTULO_CAMINHO_MAX));
    assert!(RotuloCaminho(longo).validar().is_err());
    let mut f = fato();
    f.rotulo = RotuloCaminho(r"C:\Users\fulano\x.ini".into());
    assert!(consulta(vec![f]).validar().is_err());
}

#[test]
fn validacao_recusa_valores_invalidos() {
    let mut f = fato();
    f.hash = Some("XYZ".into());
    assert!(consulta(vec![f]).validar().is_err());

    let mut f = fato();
    f.instancia = "Conta Do Fulano".into();
    assert!(consulta(vec![f]).validar().is_err());

    let mut f = fato();
    f.valores.push(f.valores[0].clone());
    assert!(consulta(vec![f]).validar().is_err());

    let mut f = fato();
    f.valores[0].valor = Fato::Conhecido(ValorJogoIpc::Texto("a".repeat(TEXTO_JOGO_MAX + 1)));
    assert!(consulta(vec![f]).validar().is_err());

    let mut f = fato();
    f.alvo = AlvoIpc::ConfigJogo {
        jogo: "R6".into(),
        arquivo: "x".into(),
    };
    assert!(consulta(vec![f]).validar().is_err());

    assert!(consulta(vec![fato(), fato()]).validar().is_err());

    let mut r = resultado();
    r.motivo = Some(MotivoNaoAplicada::VrrInativo);
    assert!(relato(vec![r]).validar().is_err());

    for processos in [vec![r"C:\Steam\steam.exe".to_string()], vec![]] {
        let mut r = resultado();
        r.motivo = Some(MotivoNaoAplicada::JanelaFechadaExigida { processos });
        assert!(relato(vec![r]).validar().is_err());
    }
    assert!(relato(vec![resultado(), resultado()]).validar().is_err());

    let mut a = acoes();
    a.push(a[0].clone());
    assert!(tarefa(a).validar().is_err());
    assert!(
        tarefa(vec![AcaoJogo::Restaurar {
            destino: destino(),
            hash_esperado: H1.to_uppercase().replace('0', "G"),
        }])
        .validar()
        .is_err()
    );
    assert!(
        Requisicao::ReverterConfigJogo {
            destino: DestinoJogoIpc {
                setting: "FPS".into(),
                ..destino()
            }
        }
        .validar()
        .is_err()
    );
}

#[test]
fn limites_de_quantidade() {
    let muitos: Vec<FatoConfigJogo> = (0..=ARQUIVOS_JOGO_MAX)
        .map(|i| FatoConfigJogo {
            instancia: format!("c{i}"),
            ..fato()
        })
        .collect();
    assert_eq!(
        consulta(muitos[..ARQUIVOS_JOGO_MAX].to_vec()).validar(),
        Ok(())
    );
    assert!(consulta(muitos).validar().is_err());

    let mut f = fato();
    f.valores = (0..=CHAVES_ARQUIVO_MAX)
        .map(|i| ValorAtualJogo {
            setting: format!("k{i}"),
            valor: Fato::Ausente,
        })
        .collect();
    assert!(consulta(vec![f]).validar().is_err());

    let resultados: Vec<ResultadoJogo> = (0..=ACOES_JOGO_MAX)
        .map(|i| ResultadoJogo {
            destino: DestinoJogoIpc {
                setting: format!("k{i}"),
                ..destino()
            },
            ..resultado()
        })
        .collect();
    assert!(relato(resultados).validar().is_err());

    let acoes: Vec<AcaoJogo> = (0..=ACOES_JOGO_MAX)
        .map(|i| AcaoJogo::Reverter {
            destino: DestinoJogoIpc {
                setting: format!("k{i}"),
                ..destino()
            },
            hash_esperado: H1.into(),
        })
        .collect();
    assert!(tarefa(acoes).validar().is_err());

    let mut p = projecao();
    let base = p.arquivos[0].mudancas[0].clone();
    p.arquivos[0].mudancas = (0..=CHAVES_ARQUIVO_MAX)
        .map(|i| MudancaArquivoJogo {
            setting: format!("k{i}"),
            ..base.clone()
        })
        .collect();
    assert!(p.validar().is_err());
    assert_eq!(projecao().validar(), Ok(()));

    let mut r = resultado();
    r.motivo = Some(MotivoNaoAplicada::JanelaFechadaExigida {
        processos: vec!["a.exe".to_string(); PROCESSOS_MAX + 1],
    });
    assert!(relato(vec![r]).validar().is_err());
}

#[test]
fn mensagem_v6_com_campos_de_jogo_recusada_pelo_envelope() {
    let corpo = serde_json::to_value(consulta(vec![fato()])).unwrap();
    let payload = serde_json::to_vec(&json!({"versao":6,"corpo":corpo})).unwrap();
    assert!(matches!(
        decodificar::<Requisicao>(&payload),
        Err(crate::ErroIpc::VersaoIncompativel {
            recebida: 6,
            esperada: 8
        })
    ));
}

#[test]
fn projecao_valida_instancia_e_sugestoes_experimentais() {
    assert!(projecao().validar().is_ok());
    let mut p = projecao();
    p.arquivos[0].instancia = "Fulano de Tal".into();
    assert!(p.validar().is_err());
    let mut p = projecao();
    p.experimentais[0].estado = EstadoExperimental::Estavel;
    assert!(p.validar().is_err());
    let mut p = projecao();
    p.experimentais[0].destinos[0].instancia = "C:\\Users".into();
    assert!(p.validar().is_err());
    // Campo novo com default: projeção sem `experimentais` ainda decodifica.
    let mut v = serde_json::to_value(projecao()).unwrap();
    v.as_object_mut().unwrap().remove("experimentais");
    assert!(serde_json::from_value::<ConfigJogoUi>(v).is_ok());
    let mut s = serde_json::to_value(&projecao().experimentais[0]).unwrap();
    s.as_object_mut()
        .unwrap()
        .insert("caminho".into(), json!("x"));
    assert!(serde_json::from_value::<SugestaoConfigJogo>(s).is_err());
}
