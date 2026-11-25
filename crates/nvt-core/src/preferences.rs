//! Preferências do usuário: sincronismo, teto de qualidade visual, limite de
//! FPS e escolhas manuais (settings travadas) que o automático deve respeitar.

use serde::{Deserialize, Serialize};

use crate::rules::ImpactoVisual;

/// Estratégia de sincronismo de tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Sincronismo {
    /// Sem G-SYNC e sem V-Sync: menor latência, aceita tearing.
    #[default]
    SemSync,
    /// G-SYNC (VRR) com V-Sync e limite abaixo do refresh.
    GSync,
    /// V-Sync tradicional, sem VRR.
    VSync,
}

impl Sincronismo {
    /// Nome usado nas condições das regras (`quando.sincronismo`).
    pub const fn nome_regra(self) -> &'static str {
        match self {
            Self::SemSync => "sem_sync",
            Self::GSync => "gsync",
            Self::VSync => "vsync",
        }
    }

    /// Nomes aceitos em `quando.sincronismo`.
    pub const NOMES_REGRA: [&'static str; 3] = ["sem_sync", "gsync", "vsync"];
}

/// Teto de qualidade visual: quanto impacto visual uma regra pode ter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Qualidade {
    /// Aceita regras de impacto até `Forte`.
    Desempenho,
    /// Aceita regras de impacto até `Leve`.
    #[default]
    Equilibrado,
    /// Aceita só regras sem impacto visual.
    Fidelidade,
}

impl Qualidade {
    /// Maior impacto visual permitido por este teto.
    pub const fn impacto_maximo(self) -> ImpactoVisual {
        match self {
            Self::Desempenho => ImpactoVisual::Forte,
            Self::Equilibrado => ImpactoVisual::Leve,
            Self::Fidelidade => ImpactoVisual::Nenhum,
        }
    }

    /// Nome usado nas condições das regras (`quando.qualidade`).
    pub const fn nome_regra(self) -> &'static str {
        match self {
            Self::Desempenho => "desempenho",
            Self::Equilibrado => "equilibrado",
            Self::Fidelidade => "fidelidade",
        }
    }

    /// Nomes aceitos em `quando.qualidade`.
    pub const NOMES_REGRA: [&'static str; 3] = ["desempenho", "equilibrado", "fidelidade"];
}

/// Onde uma setting é gravada: perfil global ou perfil de um jogo.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Alvo {
    Global,
    /// Identificador do jogo (`GameFacts::id`).
    Jogo(String),
}

/// Setting travada por uma mudança manual do usuário: o plano não a toca
/// até o usuário liberá-la.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EscolhaUsuario {
    pub alvo: Alvo,
    /// Nome simbólico da setting (catálogo `regras/settings.toml`).
    pub setting: String,
}

/// Preferências do usuário.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Preferences {
    /// Padrão configurável: `SemSync`.
    pub sincronismo: Sincronismo,
    /// Padrão: `Equilibrado`.
    pub qualidade: Qualidade,
    /// Limite global de FPS escolhido pelo usuário; `None` = automático.
    pub limite_fps: Option<u32>,
    /// Settings travadas por mudança manual.
    pub escolhas_usuario: Vec<EscolhaUsuario>,
}

impl Preferences {
    /// Verdadeiro se a setting está travada para o alvo.
    pub fn travada(&self, alvo: &Alvo, setting: &str) -> bool {
        self.escolhas_usuario
            .iter()
            .any(|e| &e.alvo == alvo && e.setting == setting)
    }
}
