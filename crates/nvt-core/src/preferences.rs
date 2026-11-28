//! Preferências do usuário: sincronismo, teto de qualidade visual, limite de
//! FPS e escolhas manuais (settings travadas) que o automático deve respeitar.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::rules::ImpactoVisual;

/// Gera um enum sem dados a partir de uma única lista `Variante => "nome"`,
/// com itens internos derivados dela, na ordem da lista:
///
/// - `VARIANTES`: todas as variantes;
/// - `NOMES_VARIANTES`: os nomes;
/// - `nome_variante(self)`: variante para nome;
/// - `de_nome_variante(nome)`: nome para variante.
///
/// A API pública de cada enum continua escrita à mão (com a sua própria
/// documentação) e só delega a esses itens. Atributos do enum e das variantes
/// (derives, serde, docs, `#[default]`) passam intactos.
macro_rules! enum_nomeado {
    (
        $(#[$meta:meta])*
        $vis:vis enum $tipo:ident {
            $(
                $(#[$meta_var:meta])*
                $var:ident => $nome:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $tipo {
            $(
                $(#[$meta_var])*
                $var,
            )+
        }

        #[allow(dead_code)]
        impl $tipo {
            /// Todas as variantes, na ordem da declaração.
            pub(crate) const VARIANTES: [Self; [$($nome),+].len()] = [$(Self::$var),+];

            /// Nomes das variantes, na ordem da declaração.
            pub(crate) const NOMES_VARIANTES: [&'static str; [$($nome),+].len()] = [$($nome),+];

            /// Nome da variante.
            pub(crate) const fn nome_variante(self) -> &'static str {
                match self {
                    $(Self::$var => $nome,)+
                }
            }

            /// Variante pelo nome exato.
            pub(crate) fn de_nome_variante(nome: &str) -> Option<Self> {
                match nome {
                    $($nome => Some(Self::$var),)+
                    _ => None,
                }
            }
        }
    };
}

pub(crate) use enum_nomeado;

enum_nomeado! {
    /// Estratégia de sincronismo de tela.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
    pub enum Sincronismo {
        /// Sem G-SYNC e sem V-Sync: menor latência, aceita tearing.
        #[default]
        SemSync => "sem_sync",
        /// G-SYNC (VRR) com V-Sync e limite abaixo do refresh.
        GSync => "gsync",
        /// V-Sync tradicional, sem VRR.
        VSync => "vsync",
    }
}

impl Sincronismo {
    /// Nome usado nas condições das regras (`quando.sincronismo`).
    pub const fn nome_regra(self) -> &'static str {
        self.nome_variante()
    }

    /// Nomes aceitos em `quando.sincronismo`.
    pub const NOMES_REGRA: [&'static str; 3] = Self::NOMES_VARIANTES;
}

enum_nomeado! {
    /// Teto de qualidade visual: quanto impacto visual uma regra pode ter.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
    pub enum Qualidade {
        /// Aceita regras de impacto até `Forte`.
        Desempenho => "desempenho",
        /// Aceita regras de impacto até `Leve`.
        #[default]
        Equilibrado => "equilibrado",
        /// Aceita só regras sem impacto visual.
        Fidelidade => "fidelidade",
    }
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
        self.nome_variante()
    }

    /// Nomes aceitos em `quando.qualidade`.
    pub const NOMES_REGRA: [&'static str; 3] = Self::NOMES_VARIANTES;
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

    /// Settings travadas como conjunto `(alvo, setting)`, para consultas
    /// repetidas sem varrer `escolhas_usuario` a cada vez. Consultar o conjunto
    /// equivale a [`Preferences::travada`].
    pub fn travas(&self) -> HashSet<(&Alvo, &str)> {
        self.escolhas_usuario
            .iter()
            .map(|e| (&e.alvo, e.setting.as_str()))
            .collect()
    }
}
