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
pub mod processos;
pub mod registro;
pub mod tela;
pub mod vdf;

pub use arch::{CustoTransformer, FamiliaGpu, GpuArchFacts, arch_facts};
pub use facts::{
    AnaliseJogo, ApiGrafica, CandidatoExe, Confianca, DllNvidia, Fato, FatoAnticheat, FatosDlss,
    GameTraits, MotivoExe,
};
pub use launchers::{FalhaScanner, RelatorioVarredura};
pub use model::{DetectedGame, Launcher, deduplicar, pasta_canonica};
pub use processos::{
    ANTICHEAT_SO_DURANTE_JOGO, FonteProcessos, ListaIndisponivel, Processo, ProcessosDoSistema,
    ProcessosFalsos, em_execucao, listar_processos,
};
pub use registro::ServicosSistema;

/// Varre todas as bibliotecas de jogos instaladas.
pub fn varrer_bibliotecas() -> RelatorioVarredura {
    launchers::varrer_tudo()
}

/// Levanta os fatos de um jogo (lê a pasta e, para anticheat, os serviços).
///
/// Esta API de diagnóstico não garante F1-09. O serviço e a interface usam
/// [`facts::analisar_com_lista_de_processos`].
pub fn analisar_jogo(jogo: &DetectedGame, servicos: &ServicosSistema) -> GameTraits {
    facts::analisar(jogo, servicos)
}

/// Como [`analisar_jogo`], mas não abre nem lista nenhum arquivo da pasta do
/// jogo se algum exe conhecido dele estiver em `em_execucao`. `exes_conhecidos`
/// são os exes já sabidos do jogo (ex.: o exe principal da varredura anterior),
/// além do `exe_indicado` pelo launcher. Com [`AnaliseJogo::EmExecucao`], o
/// chamador reaproveita os fatos anteriores.
///
/// Esta API de diagnóstico não garante F1-09. O serviço e a interface usam
/// [`facts::analisar_com_lista_de_processos`].
pub fn analisar_jogo_se_fechado<S: AsRef<str>, T: AsRef<str>>(
    jogo: &DetectedGame,
    servicos: &ServicosSistema,
    exes_conhecidos: &[S],
    em_execucao: &[T],
) -> AnaliseJogo {
    facts::analisar_se_fechado(jogo, servicos, exes_conhecidos, em_execucao)
}
