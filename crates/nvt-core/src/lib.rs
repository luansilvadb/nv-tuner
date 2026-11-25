//! Núcleo do nv-tuner.
//!
//! Contém o modelo de domínio puro (sem FFI nem E/S de sistema): o que a GPU
//! sabe fazer, o que se sabe sobre cada jogo, o que o usuário prefere, as
//! regras declarativas e o avaliador que transforma tudo isso em um plano de
//! configuração a ser aplicado pelo serviço.

pub mod capabilities;
pub mod catalogo;
pub mod evaluator;
pub mod explicacao;
pub mod facts;
pub mod plan;
pub mod preferences;
pub mod rules;
pub mod versao;

pub use capabilities::{Arquitetura, Capabilities};
pub use catalogo::{Catalogo, DefSetting, Faixa, PreferenciaGovernante};
pub use evaluator::avaliar;
pub use explicacao::{
    Falha, MotivoDeclarado, MotivoNaoAplicada, RegraAplicada, RegraNaoAplicada, ResumoJogo,
    TipoMotivo,
};
pub use facts::{Api, GameFacts};
pub use plan::{Descarte, Origem, Plan, Respeitada, SettingChange, Sugestao};
pub use preferences::{Alvo, EscolhaUsuario, Preferences, Qualidade, Sincronismo};
pub use rules::{
    Condicoes, ConjuntoRegras, ErroRegra, ErrosRegras, Escopo, ImpactoVisual, Rule, Status,
    ValorAcao,
};
pub use versao::Versao;

/// Conteúdo de `regras/settings.toml`, embutido no binário.
pub const SETTINGS_TOML: &str = include_str!("../../../regras/settings.toml");

/// Conteúdo de `regras/base.toml`, embutido no binário.
pub const BASE_TOML: &str = include_str!("../../../regras/base.toml");

impl ConjuntoRegras {
    /// Catálogo e regras embutidos (`regras/settings.toml` + `regras/base.toml`).
    pub fn embutido() -> Result<Self, ErrosRegras> {
        Self::from_toml(SETTINGS_TOML, &[("base.toml", BASE_TOML)])
    }
}
