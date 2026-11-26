//! Protocolo do named pipe entre a UI (sem privilégios) e o serviço do
//! Windows (privilegiado).
//!
//! - [`protocolo`]: mensagens de alto nível ([`Requisicao`] e [`Resposta`]).
//!   Configurações do driver aparecem apenas por **nome simbólico**; o
//!   serviço traduz nomes para IDs DRS usando a própria lista permitida.
//! - [`quadro`]: envelope versionado em JSON e enquadramento por tamanho
//!   (`u32` little-endian + payload, limitado a [`TAMANHO_MAXIMO`]).
//! - [`pipe`] (somente Windows): servidor para o serviço e cliente
//!   bloqueante para a UI sobre `\\.\pipe\nv-tuner`.
//!
//! Detalhes do protocolo e da política de segurança: `docs/ipc.md`.

mod erro;
#[cfg(windows)]
pub mod pipe;
pub mod protocolo;
pub mod quadro;

pub use erro::ErroIpc;
pub use protocolo::*;
pub use quadro::{TAMANHO_MAXIMO, codificar, decodificar, enviar, ler_quadro, receber};

/// Versão do protocolo; incrementada a cada mudança incompatível.
pub const PROTOCOL_VERSION: u32 = 4;

/// Nome do named pipe usado pelo serviço.
pub const NOME_PIPE: &str = r"\\.\pipe\nv-tuner";
