//! Interface do nv-tuner.
//!
//! Processo comum (sem administrador). Fala com o serviço pelo named pipe
//! numa thread de fundo; sem serviço, entra no modo de visualização
//! (cálculo local somente leitura). Telas e fluxo: `docs/ui.md`.
//!
//! Opções:
//!
//! - `--pipe <nome>`: outro named pipe (testes); padrão `\\.\pipe\nv-tuner`.
//! - `--tela <chave>`: seção inicial (`inicio`, `jogos`; `partidas` no build de teste).
//! - `--sair-apos <segundos>`: fecha sozinha (capturas automatizadas).
//! - `--expandir <jogo>`: abre já expandida a linha do jogo com esse nome ou perfil
//!   na tela Jogos (repetível; para capturas).
//! - `--demonstracao <cenario>`: dados fictícios fixos, sem serviço e sem ler a
//!   máquina (ver `demonstracao`).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod abrir;
mod arquivo_jogo;
mod demonstracao;
mod fuso;
mod local;
mod modelo;
mod servico;
mod tema;
mod textos;
mod ui;

use std::time::Duration;

use gpui_kit::*;

use crate::ui::Secao;

/// Opções da linha de comando.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Opcoes {
    pipe: String,
    tela: Secao,
    sair_apos: Option<u64>,
    expandir: Vec<String>,
    demonstracao: Option<demonstracao::Cenario>,
}

fn ler_opcoes(args: impl IntoIterator<Item = String>) -> Result<Opcoes, String> {
    let mut o = Opcoes {
        pipe: nvt_ipc::NOME_PIPE.to_string(),
        tela: Secao::default(),
        sair_apos: None,
        expandir: Vec::new(),
        demonstracao: None,
    };
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let mut valor = |nome: &str| it.next().ok_or(format!("{nome} exige um valor"));
        match a.as_str() {
            "--pipe" => o.pipe = valor("--pipe")?,
            "--tela" => {
                let v = valor("--tela")?;
                o.tela = Secao::de_chave(&v).ok_or(format!("tela desconhecida: {v}"))?;
            }
            "--sair-apos" => {
                let v = valor("--sair-apos")?;
                o.sair_apos = Some(v.parse().map_err(|_| format!("número inválido: {v}"))?);
            }
            "--expandir" => {
                let v = valor("--expandir")?;
                o.expandir.push(v);
            }
            "--demonstracao" => {
                let v = valor("--demonstracao")?;
                o.demonstracao = Some(demonstracao::Cenario::de_chave(&v).ok_or(format!(
                    "cenário desconhecido: {v} (use {})",
                    demonstracao::Cenario::chaves()
                ))?);
            }
            outro => return Err(format!("opção desconhecida: {outro}")),
        }
    }
    Ok(o)
}

fn main() {
    let opcoes = match ler_opcoes(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("nvt-ui: {e}");
            std::process::exit(2);
        }
    };
    // Demonstração: canal simulado no lugar do serviço (nunca os dois).
    let canal = match opcoes.demonstracao {
        Some(c) => {
            demonstracao::ativar(c);
            demonstracao::iniciar(c)
        }
        None => servico::iniciar(opcoes.pipe.clone()),
    };

    // Pacote Lucide completo: a navegação usa ícones fora do conjunto padrão.
    let app = gpui_kit::application().with_assets(gpui_kit::assets::AllAssets);
    app.run(move |cx| {
        gpui_kit::init(cx);
        gpui_kit::base::apply_system_reduce_motion(cx);
        // Textos internos do gpui-component (busca, diálogos) em português.
        gpui_kit::component::set_locale("pt-BR");
        tema::aplicar(cx);
        let opcoes_janela = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(tema::LARGURA_JANELA), px(tema::ALTURA_JANELA)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some(if opcoes.demonstracao.is_some() {
                    textos::TITULO_JANELA_DEMONSTRACAO.into()
                } else {
                    textos::TITULO_JANELA.into()
                }),
                ..gpui_kit::component::TitleBar::title_bar_options()
            }),
            ..gpui_kit::component::TitleBar::window_options()
        };
        let tela = opcoes.tela;
        let expandir = opcoes.expandir.clone();
        gpui_kit::open_window(opcoes_janela, cx, move |_, cx| {
            cx.new(|cx| ui::Janela::new(tela, expandir, canal, cx))
        })
        .expect("falha ao abrir a janela");
        if let Some(s) = opcoes.sair_apos {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_secs(s)).await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        }
    });
}

#[cfg(test)]
mod testes {
    use super::{Secao, ler_opcoes};

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn opcoes_padrao() {
        let o = ler_opcoes(args(&[])).unwrap();
        assert_eq!(o.pipe, nvt_ipc::NOME_PIPE);
        assert_eq!(o.tela, Secao::default());
        assert_eq!(o.sair_apos, None);
        assert!(o.expandir.is_empty());
        assert_eq!(o.demonstracao, None);
    }

    #[test]
    fn opcao_demonstracao() {
        let o = ler_opcoes(args(&["--demonstracao", "jogo-aberto"])).unwrap();
        assert_eq!(
            o.demonstracao,
            Some(crate::demonstracao::Cenario::JogoAberto)
        );
        assert!(ler_opcoes(args(&["--demonstracao", "x"])).is_err());
        assert!(ler_opcoes(args(&["--demonstracao"])).is_err());
    }

    #[test]
    fn opcoes_completas() {
        let o = ler_opcoes(args(&[
            "--pipe",
            r"\\.\pipe\teste",
            "--tela",
            "jogos",
            "--sair-apos",
            "10",
            "--expandir",
            "WARDOGS",
            "--expandir",
            "League of Legends",
        ]))
        .unwrap();
        assert_eq!(o.pipe, r"\\.\pipe\teste");
        assert_eq!(o.tela, Secao::Jogos);
        assert_eq!(o.sair_apos, Some(10));
        assert_eq!(o.expandir, ["WARDOGS", "League of Legends"]);
    }

    #[test]
    fn opcoes_invalidas() {
        assert!(ler_opcoes(args(&["--tela", "nada"])).is_err());
        assert!(ler_opcoes(args(&["--pipe"])).is_err());
        assert!(ler_opcoes(args(&["--expandir"])).is_err());
        assert!(ler_opcoes(args(&["--sair-apos", "x"])).is_err());
        assert!(ler_opcoes(args(&["--xyz"])).is_err());
    }
}
