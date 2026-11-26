//! Janela principal: estrutura comum ([`estrutura`]) e as seções Início,
//! Jogos e Avançado.

mod avancado;
mod boas_vindas;
mod estrutura;
mod inicio;
mod jogos;
#[cfg(feature = "medicao-partidas")]
mod partidas;

pub use estrutura::{Destino, Janela};
