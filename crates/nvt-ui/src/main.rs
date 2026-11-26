//! Interface do nv-tuner.
//!
//! Processo comum (sem administrador). Fala com o serviço pelo named pipe
//! numa thread de fundo; sem serviço, entra no modo de visualização
//! (cálculo local somente leitura). Telas e fluxo: `docs/ui.md`.
//!
//! Opções:
//!
//! - `--pipe <nome>`: outro named pipe (testes); padrão `\\.\pipe\nv-tuner`.
//! - `--tela <chave>`: tela inicial (`visao-geral`, `jogos`, `mudancas`,
//!   `windows`, `preferencias`, `historico`, `restaurar`).
//! - `--sair-apos <segundos>`: fecha sozinha (capturas automatizadas).
//! - `--expandir <jogo>`: abre já expandida a linha do jogo com esse nome ou perfil
//!   na tela Jogos (repetível; para capturas).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod arquivo_jogo;
mod fuso;
mod local;
mod modelo;
mod servico;
mod textos;
mod ui;

use std::time::Duration;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

use crate::modelo::Tela;

/// Opções da linha de comando.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Opcoes {
    pipe: String,
    tela: Tela,
    sair_apos: Option<u64>,
    expandir: Vec<String>,
}

fn ler_opcoes(args: impl IntoIterator<Item = String>) -> Result<Opcoes, String> {
    let mut o = Opcoes {
        pipe: nvt_ipc::NOME_PIPE.to_string(),
        tela: Tela::VisaoGeral,
        sair_apos: None,
        expandir: Vec::new(),
    };
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let mut valor = |nome: &str| it.next().ok_or(format!("{nome} exige um valor"));
        match a.as_str() {
            "--pipe" => o.pipe = valor("--pipe")?,
            "--tela" => {
                let v = valor("--tela")?;
                o.tela = Tela::de_chave(&v).ok_or(format!("tela desconhecida: {v}"))?;
            }
            "--sair-apos" => {
                let v = valor("--sair-apos")?;
                o.sair_apos = Some(v.parse().map_err(|_| format!("número inválido: {v}"))?);
            }
            "--expandir" => {
                let v = valor("--expandir")?;
                o.expandir.push(v);
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
    let canal = servico::iniciar(opcoes.pipe.clone());

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        let opcoes_janela = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1200.), px(800.)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some(textos::TITULO_JANELA.into()),
                ..Default::default()
            }),
            ..Default::default()
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
    use super::{Tela, ler_opcoes};

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn opcoes_padrao() {
        let o = ler_opcoes(args(&[])).unwrap();
        assert_eq!(o.pipe, nvt_ipc::NOME_PIPE);
        assert_eq!(o.tela, Tela::VisaoGeral);
        assert_eq!(o.sair_apos, None);
        assert!(o.expandir.is_empty());
    }

    #[test]
    fn opcoes_completas() {
        let o = ler_opcoes(args(&[
            "--pipe",
            r"\\.\pipe\teste",
            "--tela",
            "historico",
            "--sair-apos",
            "10",
            "--expandir",
            "WARDOGS",
            "--expandir",
            "League of Legends",
        ]))
        .unwrap();
        assert_eq!(o.pipe, r"\\.\pipe\teste");
        assert_eq!(o.tela, Tela::Historico);
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
