//! Erros do motor.

use std::fmt;

/// Erro do `nvt-engine`.
#[derive(Debug)]
pub enum ErroEngine {
    /// Falha da NVAPI/DRS.
    Drs(nvt_drs::Error),
    /// Regras embutidas inválidas.
    Regras(nvt_core::ErrosRegras),
    /// ID DRS fora da lista permitida (catálogo `regras/settings.toml`).
    IdNaoPermitido { id: u32 },
    /// Valor não aceito pelo catálogo para a setting.
    ValorNaoPermitido { setting: String, valor: u32 },
    /// Nenhuma GPU NVIDIA encontrada.
    SemGpu,
    /// Leitura ou gravação do estado persistente.
    Io(String),
    /// JSON do estado inválido.
    Json(String),
    /// Gravação pedida a um estado aberto com `Armazem::abrir_somente_leitura`.
    SomenteLeitura,
}

impl fmt::Display for ErroEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Drs(e) => write!(f, "DRS: {e}"),
            Self::Regras(e) => write!(f, "regras: {e}"),
            Self::IdNaoPermitido { id } => write!(
                f,
                "setting 0x{id:08X} fora da lista permitida (catálogo): gravação recusada"
            ),
            Self::ValorNaoPermitido { setting, valor } => write!(
                f,
                "valor {valor} (0x{valor:X}) não aceito pelo catálogo para \"{setting}\""
            ),
            Self::SemGpu => f.write_str("nenhuma GPU NVIDIA encontrada"),
            Self::Io(m) => write!(f, "estado: {m}"),
            Self::Json(m) => write!(f, "estado (JSON): {m}"),
            Self::SomenteLeitura => {
                f.write_str("estado aberto em modo somente leitura: gravação recusada")
            }
        }
    }
}

impl std::error::Error for ErroEngine {}

impl From<nvt_drs::Error> for ErroEngine {
    fn from(e: nvt_drs::Error) -> Self {
        Self::Drs(e)
    }
}

impl From<nvt_core::ErrosRegras> for ErroEngine {
    fn from(e: nvt_core::ErrosRegras) -> Self {
        Self::Regras(e)
    }
}

/// Resultado do `nvt-engine`.
pub type Resultado<T> = Result<T, ErroEngine>;
