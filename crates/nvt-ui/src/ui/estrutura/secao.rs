//! Seções da navegação lateral: título, ícone e chave.

use super::*;

pub(super) const NAV_INICIO: &str = "Início";
pub(super) const NAV_JOGOS: &str = "Jogos & Otimização";

/// Item da navegação lateral.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Secao {
    #[default]
    Inicio,
    Jogos,
    #[cfg(feature = "medicao-partidas")]
    Partidas,
}

impl Secao {
    /// Itens visíveis na lateral, na ordem (R-UI-01).
    pub fn visiveis() -> &'static [Secao] {
        #[cfg(not(feature = "medicao-partidas"))]
        {
            &[Secao::Inicio, Secao::Jogos]
        }
        #[cfg(feature = "medicao-partidas")]
        {
            &[Secao::Inicio, Secao::Jogos, Secao::Partidas]
        }
    }

    pub fn titulo(self) -> &'static str {
        match self {
            Secao::Inicio => NAV_INICIO,
            Secao::Jogos => NAV_JOGOS,
            #[cfg(feature = "medicao-partidas")]
            Secao::Partidas => t::partidas::NAV,
        }
    }

    pub fn icone(self) -> IconName {
        match self {
            Secao::Inicio => IconName::House,
            Secao::Jogos => IconName::Gamepad2,
            #[cfg(feature = "medicao-partidas")]
            Secao::Partidas => IconName::ChartLine,
        }
    }

    /// Chave de `--tela`.
    pub fn chave(self) -> &'static str {
        match self {
            Secao::Inicio => "inicio",
            Secao::Jogos => "jogos",
            #[cfg(feature = "medicao-partidas")]
            Secao::Partidas => "partidas",
        }
    }

    /// Interpreta `--tela` usando as seções disponíveis na navegação.
    pub fn de_chave(s: &str) -> Option<Secao> {
        Secao::visiveis()
            .iter()
            .copied()
            .find(|secao| secao.chave() == s)
    }
}
