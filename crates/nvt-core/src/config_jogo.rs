//! Configuração de jogo (fase 3, D10): tipos dos
//! destinos de jogo, catálogo `[[arquivo]]`/`[[regra]]` de `regras/jogos.toml`
//! (schema 2) e avaliação das regras `config_jogo`, sem E/S.
//!
//! O serviço avalia; o agente resolve caminhos, lê e grava (D7). Aqui só há o
//! modelo, a validação do catálogo (F3-02, trava de 9.5.4, N8) e o plano.
//! F3-13 (DRS `limite_fps = off` no perfil do jogo) fica em
//! [`desligar_limite_drs`].

mod avaliacao;
mod definicoes;
mod validacao;

pub use avaliacao::*;
pub use definicoes::*;
pub use validacao::*;

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::catalogo::{Catalogo, PreferenciaGovernante};
use crate::evaluator::SETTING_LIMITE_FPS;
use crate::explicacao::RegraAplicada;
use crate::facts::GameFacts;
use crate::jogos::{IdJogo, JOGOS_SEM_EDICAO, JogoCurado, PoliticaConfig};
use crate::plan::{CodigoExplicacao, EstadoExperimental, Origem, Plan, SettingChange};
use crate::preferences::{Alvo, Preferences};
use crate::rules::{ImpactoVisual, Status, id_valido};
use crate::usuario::MotivoRespeitadaUsuario;

/// Padrão de `fps_max` do Dota 2: `jogo-dota2-fps-max-padrao` só sobe acima dele.
pub const FPS_MAX_PADRAO_DOTA2: i64 = 120;

/// Token de limite nas opções de inicialização (único que o app grava, F3-11).
pub const TOKEN_FPS_MAX: &str = "+fps_max";

/// Nomes de arquivo da pasta de instalação que o app nunca toca (§9.1).
pub const ARQUIVOS_PROIBIDOS: [&str; 4] = ["autoexec.cfg", "video.txt", "boot.vcfg", "gameinfo.gi"];

/// Componentes de caminho que indicam pasta de instalação (N8).
const PASTAS_DE_INSTALACAO: [&str; 4] = [
    "steamapps",
    "common",
    "program files",
    "program files (x86)",
];

/// Marcadores aceitos em `caminho`.
const MARCADORES: [&str; 3] = ["<conta>", "<steamid3>", "<appid>"];

/// Chaves do catálogo de jogo: lista fechada. Cada uma pertence a um só
/// arquivo lógico de um só jogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingJogo {
    R6FpsLimit,
    R6Reflex,
    FortniteFrameRateLimit,
    FortniteReflex,
    RlOneFrameThreadLag,
    RlMaxFps,
    Cs2FpsMax,
    Cs2Reflex,
    Dota2FpsMax,
    DeadlockFpsMax,
}

impl SettingJogo {
    pub const TODAS: [Self; 10] = [
        Self::R6FpsLimit,
        Self::R6Reflex,
        Self::FortniteFrameRateLimit,
        Self::FortniteReflex,
        Self::RlOneFrameThreadLag,
        Self::RlMaxFps,
        Self::Cs2FpsMax,
        Self::Cs2Reflex,
        Self::Dota2FpsMax,
        Self::DeadlockFpsMax,
    ];

    /// Código usado no TOML, no IPC e na forma textual do destino.
    pub const fn codigo(self) -> &'static str {
        match self {
            Self::R6FpsLimit => "r6_fps_limit",
            Self::R6Reflex => "r6_reflex",
            Self::FortniteFrameRateLimit => "fortnite_frame_rate_limit",
            Self::FortniteReflex => "fortnite_reflex",
            Self::RlOneFrameThreadLag => "rl_one_frame_thread_lag",
            Self::RlMaxFps => "rl_max_fps",
            Self::Cs2FpsMax => "cs2_fps_max",
            Self::Cs2Reflex => "cs2_reflex",
            Self::Dota2FpsMax => "dota2_fps_max",
            Self::DeadlockFpsMax => "deadlock_fps_max",
        }
    }

    /// Id do jogo na base curada.
    pub const fn jogo(self) -> &'static str {
        match self {
            Self::R6FpsLimit | Self::R6Reflex => "r6-siege",
            Self::FortniteFrameRateLimit | Self::FortniteReflex => "fortnite",
            Self::RlOneFrameThreadLag | Self::RlMaxFps => "rocket-league",
            Self::Cs2FpsMax | Self::Cs2Reflex => "cs2",
            Self::Dota2FpsMax => "dota2",
            Self::DeadlockFpsMax => "deadlock",
        }
    }

    /// Limite padrão do próprio jogo, abaixo do qual `refresh_limitado_ao_menu`
    /// não sobe; `None` = a ação não vale para esta setting.
    pub const fn limite_padrao(self) -> Option<i64> {
        match self {
            Self::Dota2FpsMax => Some(FPS_MAX_PADRAO_DOTA2),
            _ => None,
        }
    }

    /// Id do arquivo lógico (`[[arquivo]].id`).
    pub const fn arquivo(self) -> &'static str {
        match self {
            Self::R6FpsLimit | Self::R6Reflex => "r6-game-settings",
            Self::FortniteFrameRateLimit | Self::FortniteReflex => "fortnite-game-user-settings",
            Self::RlOneFrameThreadLag | Self::RlMaxFps => "rl-ta-system-settings",
            Self::Cs2FpsMax => "cs2-launch",
            Self::Cs2Reflex => "cs2-video",
            Self::Dota2FpsMax => "dota2-launch",
            Self::DeadlockFpsMax => "deadlock-launch",
        }
    }

    pub fn de_codigo(codigo: &str) -> Option<Self> {
        Self::TODAS.into_iter().find(|s| s.codigo() == codigo)
    }
}

impl fmt::Display for SettingJogo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

/// Onde está o arquivo: jogo, arquivo lógico e conta resolvida pelo agente
/// (hash, nunca o identificador em claro).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct InstanciaJogo {
    pub jogo: IdJogo,
    pub arquivo: String,
    pub conta: Option<String>,
}

/// Destino de uma escrita de jogo. Forma textual canônica estável
/// (`"r6_fps_limit@<hash-da-conta>"` ou `"dota2_fps_max"` sem conta), usada
/// como chave de mapa na serialização.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DestinoJogo {
    pub setting: SettingJogo,
    pub instancia: InstanciaJogo,
}

/// Forma textual de destino inválida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinoInvalido(pub String);

impl fmt::Display for DestinoInvalido {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "destino de jogo inválido: \"{}\"", self.0)
    }
}

impl std::error::Error for DestinoInvalido {}

fn conta_valida(conta: &str) -> bool {
    (1..=64).contains(&conta.len())
        && conta
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl DestinoJogo {
    /// Destino de `setting` na conta `conta` (hash). `None` se a conta não
    /// for um hash aceito (1 a 64 de `[A-Za-z0-9_-]`).
    pub fn novo(setting: SettingJogo, conta: Option<&str>) -> Option<Self> {
        if conta.is_some_and(|c| !conta_valida(c)) {
            return None;
        }
        Some(Self {
            setting,
            instancia: InstanciaJogo {
                jogo: IdJogo::interno(setting.jogo()),
                arquivo: setting.arquivo().to_owned(),
                conta: conta.map(str::to_owned),
            },
        })
    }

    /// `true` se jogo e arquivo da instância são os da setting e a conta é válida.
    pub fn coerente(&self) -> bool {
        self.instancia.jogo.as_str() == self.setting.jogo()
            && self.instancia.arquivo == self.setting.arquivo()
            && self.instancia.conta.as_deref().is_none_or(conta_valida)
    }
}

impl fmt::Display for DestinoJogo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.instancia.conta {
            Some(conta) => write!(f, "{}@{conta}", self.setting.codigo()),
            None => f.write_str(self.setting.codigo()),
        }
    }
}

impl FromStr for DestinoJogo {
    type Err = DestinoInvalido;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (codigo, conta) = match s.split_once('@') {
            Some((c, conta)) => (c, Some(conta)),
            None => (s, None),
        };
        SettingJogo::de_codigo(codigo)
            .and_then(|setting| Self::novo(setting, conta))
            .ok_or_else(|| DestinoInvalido(s.to_owned()))
    }
}

impl Serialize for DestinoJogo {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for DestinoJogo {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let texto = String::deserialize(d)?;
        texto.parse().map_err(serde::de::Error::custom)
    }
}

/// Valor de uma chave de jogo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValorJogo {
    /// Chave ausente (só como leitura/original; o app não cria chave).
    Ausente,
    Inteiro(i64),
    Booleano(bool),
    /// Texto curto, ASCII imprimível (até 64 bytes).
    Texto(String),
    /// Token `+fps_max <n>` nas opções de inicialização; `None` = sem o token.
    TokenLancamento(Option<u32>),
}

impl ValorJogo {
    fn texto_valido(t: &str) -> bool {
        t.len() <= 64 && t.bytes().all(|b| (0x20..0x7f).contains(&b))
    }
}

/// Gestão de um destino de jogo pelo app (snapshot e backup, F3-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GestaoJogo {
    pub antes: ValorJogo,
    pub esperado: ValorJogo,
    pub hash_antes: Option<String>,
    pub hash_depois: Option<String>,
    /// Id do backup `.orig` (nunca um caminho completo).
    pub backup: Option<String>,
}

/// Leitura de um destino resolvido pelo agente.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EstadoDestinoJogo {
    /// `None` = leitura desconhecida.
    pub atual: Option<ValorJogo>,
    pub somente_leitura: bool,
    pub na_nuvem: bool,
    pub conta_ambigua: bool,
    pub hash: Option<String>,
}

/// Fatos de configuração de jogo de uma sessão (agente + coordenador).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FatosConfigJogo {
    pub destinos: BTreeMap<DestinoJogo, EstadoDestinoJogo>,
    pub gestao: BTreeMap<DestinoJogo, GestaoJogo>,
    pub travadas: BTreeSet<DestinoJogo>,
}
