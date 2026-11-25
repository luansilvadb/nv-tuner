//! Detecção do ambiente (somente leitura).
//!
//! - GPU e capacidades: [`arch_facts`] converte a arquitetura informada pela
//!   NVAPI em fatos de hardware ([`GpuArchFacts`]).
//! - Jogos e bibliotecas: [`varrer_bibliotecas`] executa os scanners de Steam,
//!   Riot, Epic, GOG, Ubisoft, EA, Battle.net, Xbox e o genérico pelas chaves de
//!   desinstalação, devolvendo [`DetectedGame`] deduplicados.
//! - Fatos do jogo: [`analisar_jogo`] levanta exe principal, API gráfica, DLSS,
//!   Reflex e anticheat ([`GameTraits`]), cada fato com [`Confianca`].
//!
//! Os tipos aqui são independentes de `nvt-core`/`nvt-drs`; a conversão é
//! feita na integração.

pub mod arch;
pub mod facts;
pub mod launchers;
pub mod model;
pub mod registro;
pub mod vdf;

pub use arch::{CustoTransformer, FamiliaGpu, GpuArchFacts, arch_facts};
pub use facts::{
    ApiGrafica, CandidatoExe, Confianca, DllNvidia, Fato, FatoAnticheat, FatosDlss, GameTraits,
    MotivoExe,
};
pub use launchers::{FalhaScanner, RelatorioVarredura};
pub use model::{DetectedGame, Launcher, deduplicar, pasta_canonica};
pub use registro::ServicosSistema;

/// Varre todas as bibliotecas de jogos instaladas.
pub fn varrer_bibliotecas() -> RelatorioVarredura {
    launchers::varrer_tudo()
}

/// Levanta os fatos de um jogo (lê a pasta e, para anticheat, os serviços).
pub fn analisar_jogo(jogo: &DetectedGame, servicos: &ServicosSistema) -> GameTraits {
    facts::analisar(jogo, servicos)
}
