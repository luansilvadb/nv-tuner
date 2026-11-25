//! Erros de transporte e de protocolo.

use std::fmt;
use std::io;

/// Falha ao enviar, receber ou interpretar uma mensagem.
///
/// Erros *de negócio* do serviço não aparecem aqui: eles chegam como
/// [`crate::Resposta::Erro`].
#[derive(Debug)]
pub enum ErroIpc {
    /// Falha de E/S (inclui `TimedOut` quando um tempo limite expira).
    Io(io::Error),
    /// O outro lado encerrou a conexão antes de iniciar um novo quadro.
    Fechado,
    /// A conexão terminou no meio de um quadro.
    Truncado,
    /// Quadro declarado (ou a enviar) maior que o limite.
    Grande { tamanho: u64, limite: u32 },
    /// O envelope traz uma versão de protocolo diferente da nossa.
    VersaoIncompativel { recebida: u64, esperada: u32 },
    /// Envelope sem o formato esperado (`versao` + `corpo`).
    EnvelopeInvalido(String),
    /// JSON inválido ou corpo que não corresponde a nenhuma mensagem.
    Json(serde_json::Error),
    /// O pipe não existe: o serviço não está em execução.
    ServicoIndisponivel,
    /// O dono do pipe não é uma conta privilegiada (possível impostor).
    ServidorNaoConfiavel,
}

impl fmt::Display for ErroIpc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "falha de E/S no pipe: {e}"),
            Self::Fechado => f.write_str("conexão encerrada pelo outro lado"),
            Self::Truncado => f.write_str("mensagem truncada"),
            Self::Grande { tamanho, limite } => {
                write!(f, "mensagem de {tamanho} bytes excede o limite de {limite}")
            }
            Self::VersaoIncompativel { recebida, esperada } => write!(
                f,
                "versão de protocolo incompatível: recebida {recebida}, esperada {esperada}"
            ),
            Self::EnvelopeInvalido(m) => write!(f, "envelope inválido: {m}"),
            Self::Json(e) => write!(f, "mensagem inválida: {e}"),
            Self::ServicoIndisponivel => f.write_str("serviço do nv-tuner não está em execução"),
            Self::ServidorNaoConfiavel => {
                f.write_str("o pipe não pertence a uma conta privilegiada; conexão recusada")
            }
        }
    }
}

impl std::error::Error for ErroIpc {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ErroIpc {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for ErroIpc {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
