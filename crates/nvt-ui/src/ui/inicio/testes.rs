//! Testes da tela Início.

use super::*;
use crate::modelo::{Saude, TipoSinal, veredito};
use core::prelude::v1::test;

fn pronto() -> Fatos {
    Fatos {
        placa_nvidia: Some(true),
        plano_com_mudancas: true,
        ..Default::default()
    }
}

/// (caso, fatos, saúde, título esperado, aplicar, desfazer, voltar)
type Caso<'a> = (
    &'a str,
    Fatos,
    Option<&'a Saude>,
    String,
    Option<BotaoAplicar>,
    bool,
    bool,
);

fn feito(aceitas: usize, adiada: bool) -> Result<RespostaInicio, FalhaInicio> {
    Ok(RespostaInicio::Feito { aceitas, adiada })
}

#[test]
fn variacoes_do_veredito_e_botoes() {
    let s = Saude {
        em_atencao: vec![TipoSinal::EspacoLivre],
    };
    let hab = Some(BotaoAplicar::Habilitado);
    let des = Some(BotaoAplicar::Desabilitado(t::APLICAR_INDISPONIVEL));
    // (caso, fatos, saúde, título esperado, aplicar, desfazer, voltar)
    let casos: Vec<Caso> = Vec::from([
        (
            "otimizado",
            pronto(),
            None,
            t::OTIMIZADO_TITULO.into(),
            None,
            true,
            false,
        ),
        (
            "nada a fazer",
            Fatos {
                plano_com_mudancas: false,
                ..pronto()
            },
            None,
            t::NADA_A_FAZER_TITULO.into(),
            None,
            false,
            false,
        ),
        (
            "melhorias",
            Fatos {
                melhorias: 3,
                ..pronto()
            },
            None,
            t::MELHORIAS_TITULO.into(),
            hab,
            true,
            false,
        ),
        (
            "piorou",
            Fatos {
                melhorias: 3,
                ..pronto()
            },
            Some(&s),
            "Algo piorou: pouco espaço livre".into(),
            hab,
            true,
            false,
        ),
        (
            "esperando o jogo: aplicar continua habilitado",
            Fatos {
                melhorias: 2,
                esperando_jogo: true,
                ..pronto()
            },
            None,
            t::ESPERANDO_TITULO.into(),
            hab,
            true,
            false,
        ),
        (
            "desligado: só voltar a ajustar",
            Fatos {
                melhorias: 2,
                ajustes_desligados: true,
                ..pronto()
            },
            None,
            t::PAUSADO_TITULO.into(),
            None,
            false,
            true,
        ),
        (
            "sem placa",
            Fatos {
                placa_nvidia: Some(false),
                ..pronto()
            },
            None,
            t::SEM_PLACA_TITULO.into(),
            None,
            false,
            false,
        ),
        (
            "serviço ausente: aplicar desabilitado com motivo",
            Fatos {
                servico_ausente: true,
                melhorias: 2,
                ..pronto()
            },
            None,
            t::INDISPONIVEL_TITULO.into(),
            des,
            false,
            false,
        ),
        (
            "serviço ausente sem melhorias",
            Fatos {
                servico_ausente: true,
                ..pronto()
            },
            None,
            t::INDISPONIVEL_TITULO.into(),
            None,
            false,
            false,
        ),
        (
            "protocolo incompatível",
            Fatos {
                protocolo_incompativel: true,
                melhorias: 2,
                ..pronto()
            },
            None,
            t::INDISPONIVEL_TITULO.into(),
            None,
            false,
            false,
        ),
        (
            "verificando",
            Fatos {
                conectando: true,
                ..Default::default()
            },
            None,
            t::VERIFICANDO_TITULO.into(),
            None,
            false,
            false,
        ),
    ]);
    for (nome, f, saude, titulo, aplicar, desfazer, voltar) in casos {
        let v = veredito(&f, saude);
        assert_eq!(t::veredito(&v).titulo, titulo, "{nome}");
        let b = botoes(&v, &f);
        assert_eq!(b.aplicar, aplicar, "{nome}: aplicar");
        assert_eq!(b.desfazer, desfazer, "{nome}: desfazer");
        assert_eq!(b.voltar, voltar, "{nome}: voltar");
    }
}

#[test]
fn modo_visualizacao_diz_que_nao_esta_ativo() {
    // R-UI-14: somente leitura e frase de reinstalação, sem services.msc.
    let f = Fatos {
        servico_ausente: true,
        ..pronto()
    };
    let texto = t::veredito(&veredito(&f, None));
    assert_eq!(texto.titulo, "O nv-tuner não está ativo neste PC");
    assert_eq!(
        texto.frase,
        "Reinstale o nv-tuner para que ele volte a cuidar do PC."
    );
    assert!(!texto.frase.contains("services.msc"));
    let b = botoes(&veredito(&f, None), &f);
    assert!(!b.desfazer && !b.voltar);
}

#[test]
fn servico_iniciando_ou_sem_resposta_mostra_diagnostico_sem_acoes() {
    for (f, titulo) in [
        (
            Fatos {
                servico_iniciando: true,
                ..pronto()
            },
            "Iniciando o nv-tuner…",
        ),
        (
            Fatos {
                servico_sem_resposta: true,
                ..pronto()
            },
            "O nv-tuner não está respondendo",
        ),
    ] {
        let v = veredito(&f, None);
        assert_eq!(t::veredito(&v).titulo, titulo);
        assert_eq!(
            botoes(&v, &f),
            Botoes {
                aplicar: None,
                desfazer: false,
                voltar: false
            }
        );
    }
}

#[test]
fn desfazer_so_aparece_com_algo_a_desfazer() {
    let com = pronto();
    assert!(botoes(&veredito(&com, None), &com).desfazer);
    let sem = Fatos {
        plano_com_mudancas: false,
        ..pronto()
    };
    assert!(!botoes(&veredito(&sem, None), &sem).desfazer);
    let desligado = Fatos {
        ajustes_desligados: true,
        ..pronto()
    };
    let b = botoes(&veredito(&desligado, None), &desligado);
    assert!(!b.desfazer && b.voltar);
}

#[test]
fn resultado_de_aplicar_recomendadas() {
    let a = AcaoInicio::AplicarRecomendadas;
    assert_eq!(
        texto_resultado(a, &feito(3, false)),
        (
            Tom::Sucesso,
            "Aplicamos 3 melhorias. Para voltar atrás, use Desfazer tudo.".into()
        )
    );
    let (tom, adiada) = texto_resultado(a, &feito(3, true));
    assert_eq!(tom, Tom::Informacao);
    assert!(adiada.starts_with("As melhorias serão aplicadas quando o jogo fechar."));
    assert!(adiada.contains("Desfazer tudo"));
    assert_eq!(texto_resultado(a, &feito(0, false)).1, t::NENHUMA_APLICADA);
    assert_eq!(
        texto_resultado(a, &Err(FalhaInicio::RecursoAusente)),
        (Tom::Informacao, t::RECOMENDADAS_AUSENTE.into())
    );
    assert_eq!(
        texto_resultado(a, &Err(FalhaInicio::SemServico)).0,
        Tom::Informacao
    );
    assert_eq!(
        texto_resultado(a, &Err(FalhaInicio::Outra("x".into()))),
        (Tom::Erro, "x".into())
    );
}

#[test]
fn desfazer_e_voltar_mudam_a_pausa() {
    let mut e = EstadoInicio::default();
    assert_eq!(
        e.concluir(AcaoInicio::DesfazerTudo, &feito(0, false)),
        Some(true)
    );
    assert_eq!(e.resultado, Some((Tom::Sucesso, t::DESFEITO_OK.into())));
    assert_eq!(
        e.concluir(AcaoInicio::VoltarAAjustar, &Err(FalhaInicio::SemServico)),
        None,
        "falha não religa"
    );
    assert_eq!(
        e.concluir(AcaoInicio::VoltarAAjustar, &feito(0, true)),
        Some(false)
    );
    assert_eq!(
        e.resultado,
        Some((Tom::Informacao, t::VOLTAR_ADIADO.into()))
    );
    e.concluir(AcaoInicio::DesfazerTudo, &feito(0, true));
    assert_eq!(
        e.resultado,
        Some((Tom::Informacao, t::DESFAZER_ADIADO.into()))
    );
    assert_eq!(e.ocupado, None);
}

#[test]
fn confirmacao_de_desfazer_tudo() {
    let c = CONFIRMAR_DESFAZER;
    assert_eq!(c.titulo, "Desfazer tudo?");
    assert_eq!(
        c.linhas,
        [
            "Todos os ajustes que o nv-tuner fez voltam a ser como eram antes.",
            "Se um jogo estiver aberto, isso acontece quando ele fechar.",
            "O nv-tuner para de ajustar até você pedir de novo.",
        ]
    );
    assert_eq!((c.cancelar, c.confirmar), ("Cancelar", "Desfazer tudo"));
    assert_eq!(c.acao, AcaoInicio::DesfazerTudo);
    assert_eq!(c.acao.requisicao(), nvt_ipc::Requisicao::RestaurarOriginal);
}

#[test]
fn saude_so_com_dados() {
    assert!(!mostra_saude(None));
    let s = crate::demonstracao::Cenario::Piorou.saude().unwrap();
    assert!(mostra_saude(Some(&s)));
    // "Algo piorou" vem dos sinais em atenção da resposta (R-S-06).
    let resumo = Saude::de_ipc(&s);
    assert_eq!(
        resumo.em_atencao,
        vec![TipoSinal::EspacoLivre, TipoSinal::PlanoEnergia]
    );
    let f = pronto();
    assert_eq!(
        t::veredito(&veredito(&f, Some(&resumo))).titulo,
        "Algo piorou: pouco espaço livre"
    );
    // Programas novos não pioram o veredito; só sinais em `Atencao`.
    let novos = crate::demonstracao::Cenario::Saude.saude().unwrap();
    assert!(Saude::de_ipc(&novos).em_atencao.is_empty());
}

#[test]
fn textos_novos_sem_sigla_e_curtos() {
    const SIGLAS: &[&str] = &[
        "GPU", "VRAM", "DLSS", "VRR", "G-SYNC", "FPS", "BAR", "DRS", "HAGS", "MPO", "SID", "IPC",
        "ETW", "MSI", "CPU", "RAM", "SSD", "API",
    ];
    let mut todos: Vec<String> = t::TODOS_L2.iter().map(|s| (*s).to_owned()).collect();

    todos.push(format!("{} {}", t::APLICAR_ADIADO, t::PARA_VOLTAR_ATRAS));
    for s in &todos {
        for sigla in SIGLAS {
            assert!(
                !s.split(|c: char| !c.is_alphanumeric() && c != '-')
                    .any(|p| p == *sigla),
                "{s:?} tem {sigla}"
            );
        }
        assert!(s.chars().all(|c| (c as u32) < 0x2600), "{s:?}");
        assert!(!s.contains("serviço"), "{s:?}");
    }
}
