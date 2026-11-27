//! Identidade do produto compartilhada pelos crates: nomes que viram pastas em
//! disco e nomes registrados no Windows. Os valores são contrato (instalador,
//! dados já gravados) e não mudam.

/// Nome da pasta de dados em `%ProgramData%` e `%LOCALAPPDATA%`.
pub const PASTA_DADOS: &str = "nv-tuner";

/// Nome do serviço no SCM (contrato com `instalador/nv-tuner.wxs`).
pub const NOME_SERVICO: &str = "nvt-service";
