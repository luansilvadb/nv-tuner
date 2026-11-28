//! Listas de identificação de launchers a partir de `data/launchers.toml`.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::model::Launcher;

pub const LAUNCHERS_TOML: &str = include_str!("../../data/launchers.toml");

#[derive(Debug, Deserialize)]
struct Bruto {
    #[allow(dead_code)]
    versao_schema: u32,
    nomes_launcher: Vec<String>,
    nao_jogo: Vec<String>,
    steam_nao_jogos: Vec<String>,
    publisher: Vec<PublisherBruto>,
}

#[derive(Debug, Deserialize)]
struct PublisherBruto {
    contem: String,
    launcher: String,
}

/// Tabela carregada e validada.
#[derive(Debug)]
pub(crate) struct Tabela {
    /// Comparação por igualdade com o nome em minúsculas.
    pub nomes_launcher: Vec<String>,
    /// Comparação por substring no nome em minúsculas.
    pub nao_jogo: Vec<String>,
    /// AppIDs da Steam que não são jogos (igualdade).
    pub steam_nao_jogos: Vec<String>,
    /// Publisher (substring, minúsculo) -> launcher, em ordem; a primeira vence.
    pub publisher: Vec<(String, Launcher)>,
}

/// Launchers que podem ser atribuídos por publisher.
fn launcher_por_nome(s: &str) -> Option<Launcher> {
    match s {
        "BattleNet" => Some(Launcher::BattleNet),
        "Ea" => Some(Launcher::Ea),
        _ => None,
    }
}

fn carregar(src: &str) -> Result<Tabela, String> {
    let b: Bruto = toml::from_str(src).map_err(|e| e.to_string())?;
    let publisher = b
        .publisher
        .into_iter()
        .map(|p| {
            launcher_por_nome(&p.launcher)
                .map(|l| (p.contem, l))
                .ok_or_else(|| format!("launcher desconhecido em [[publisher]]: {}", p.launcher))
        })
        .collect::<Result<_, _>>()?;
    Ok(Tabela {
        nomes_launcher: b.nomes_launcher,
        nao_jogo: b.nao_jogo,
        steam_nao_jogos: b.steam_nao_jogos,
        publisher,
    })
}

pub(crate) fn tabela() -> &'static Tabela {
    static T: OnceLock<Tabela> = OnceLock::new();
    T.get_or_init(|| carregar(LAUNCHERS_TOML).expect("data/launchers.toml inválido"))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn toml_embutido_igual_as_listas_originais() {
        let t = carregar(LAUNCHERS_TOML).unwrap();
        assert_eq!(
            t.nomes_launcher,
            [
                "ea app",
                "origin",
                "battle.net",
                "steam",
                "epic games launcher",
                "riot client",
                "ubisoft connect",
                "uplay",
                "gog galaxy",
                "xbox",
            ]
        );
        assert_eq!(
            t.nao_jogo,
            [
                "vanguard",
                "redistributable",
                "runtime",
                "anti-cheat",
                "anticheat",
                "driver",
                "sdk",
                "launcher",
                "riot client",
                "battle.net",
                "ea app",
                "gog galaxy",
                "ubisoft connect",
            ]
        );
        assert_eq!(
            t.steam_nao_jogos,
            ["228980", "1070560", "1391110", "1493710", "1628350"]
        );
        assert_eq!(
            t.publisher,
            [
                ("blizzard entertainment".to_string(), Launcher::BattleNet),
                ("electronic arts".to_string(), Launcher::Ea),
            ]
        );
    }

    #[test]
    fn entradas_em_minusculas() {
        let t = tabela();
        let todas = t
            .nomes_launcher
            .iter()
            .chain(&t.nao_jogo)
            .chain(t.publisher.iter().map(|(p, _)| p));
        for s in todas {
            assert_eq!(*s, s.to_lowercase());
        }
    }

    #[test]
    fn launcher_desconhecido_e_erro() {
        let src = LAUNCHERS_TOML.replace("\"BattleNet\"", "\"Outro\"");
        assert!(carregar(&src).is_err());
    }
}
