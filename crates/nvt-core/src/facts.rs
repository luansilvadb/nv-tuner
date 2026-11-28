//! Fatos sobre cada jogo (API gráfica, DLSS, Reflex, anticheat de kernel)
//! obtidos pela detecção. Campos `Option` valem `None` quando a detecção não
//! conseguiu concluir; condições sobre fatos desconhecidos nunca casam.

use serde::{Deserialize, Serialize};

use crate::jogos::{IdJogo, Orientacao};
use crate::preferences::enum_nomeado;
use crate::versao::Versao;

enum_nomeado! {
    /// API gráfica principal do jogo.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
    pub enum Api {
        Dx9 => "dx9",
        Dx11 => "dx11",
        Dx12 => "dx12",
        Vulkan => "vulkan",
        OpenGL => "opengl",
        #[default]
        Desconhecida => "desconhecida",
    }
}

impl Api {
    /// Nome usado nas condições das regras (`quando.jogo_api`).
    pub const fn nome_regra(self) -> &'static str {
        self.nome_variante()
    }

    /// Nomes aceitos em `quando.jogo_api`.
    pub const NOMES_REGRA: [&'static str; 6] = Self::NOMES_VARIANTES;
}

/// O que se sabe sobre um jogo instalado.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GameFacts {
    /// Identificador do alvo no driver: nome do perfil DRS (ou o exe, quando
    /// ainda não há perfil). É a chave de `Plan::por_jogo`.
    pub id: String,
    pub api: Api,
    /// Versão do `nvngx_dlss.dll` encontrado; `None` = jogo sem DLSS.
    pub dlss: Option<Versao>,
    /// Reflex nativo; `None` = não determinado.
    pub reflex: Option<bool>,
    /// Anticheat de kernel; `None` = não determinado. Para jogo da base
    /// curada, vem da base (pode ser `false`, ex.: VAC).
    pub anticheat_kernel: Option<bool>,
    /// Jogo casado na base curada (`regras/jogos.toml`). Fora da base = `false`.
    #[serde(default)]
    pub competitivo: bool,
    /// Limitador de FPS nativo (base curada); `None` = jogo fora da base.
    #[serde(default)]
    pub limitador_nativo: Option<bool>,
    /// Id do jogo na base curada, quando casado.
    #[serde(default)]
    pub jogo_base: Option<IdJogo>,
    /// Códigos de orientação da base curada, antes do filtro por preferências
    /// (o plano traz os filtrados em `ResumoJogo::orientacoes`).
    #[serde(default)]
    pub orientacoes: Vec<Orientacao>,
}

impl GameFacts {
    /// Fatos mínimos: só o identificador, o resto desconhecido.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Self::default()
        }
    }
}
