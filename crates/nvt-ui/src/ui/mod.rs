//! Janela principal: estrutura comum ([`estrutura`]) e as seções Início,
//! Jogos e Partidas (esta só com a feature `medicao-partidas`).

mod boas_vindas;
mod estrutura;
mod inicio;
mod jogos;
mod movimento;
#[cfg(feature = "medicao-partidas")]
mod partidas;

pub use estrutura::{Janela, Secao};
