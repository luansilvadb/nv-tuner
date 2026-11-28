//! Textos da navegação lateral e da barra superior de cada seção.

pub const INICIO: &str = "Início";
pub const JOGOS: &str = "Jogos & Otimização";
#[cfg(feature = "medicao-partidas")]
pub use super::partidas::NAV as PARTIDAS;

pub const DESCRICAO_INICIO: &str = "Painel de desempenho e drivers";
pub const DESCRICAO_JOGOS: &str = "Biblioteca e preferências rápidas";
