//! Ciclo interativo, independente da implementação dos ajustes do Windows.
mod ciclo;
pub mod config_jogo;
mod estado;
mod valores;
#[cfg(windows)]
pub mod windows;

pub use ciclo::{Agente, Controle, Instalacao, Transporte};
pub use estado::{Estado, Persistencia};
pub use valores::{EscritaUsuario, LeituraUsuario, ValorLido};
