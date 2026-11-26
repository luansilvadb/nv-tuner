use std::fmt;
use std::io;
use std::path::PathBuf;

/// Erros do crate. [`Erro::motivo`] dá o motivo de relato (IPC v7).
#[derive(Debug)]
#[non_exhaustive]
pub enum Erro {
    /// Codificação, estrutura ou valor que o editor não reconhece por inteiro.
    FormatoInesperado(String),
    /// A chave não existe; o editor nunca a cria.
    ChaveAusente(String),
    /// `FILE_ATTRIBUTE_READONLY`: não tocado (o app nunca marca nem desmarca).
    ArquivoSomenteLeitura(PathBuf),
    /// Link, junção ou outro reparse point no caminho.
    ReparseRecusado(PathBuf),
    ArquivoAusente(PathBuf),
    /// O arquivo efetivo está na pasta sincronizada (`remote`) da Steam Cloud.
    ArquivoNaNuvem(PathBuf),
    /// O jogo mudou o que o app gravou; nada foi revertido.
    ArquivoAlteradoPeloJogo,
    /// O conteúdo atual não tem o hash esperado (mudou desde a leitura).
    HashDivergente,
    /// Mais de uma conta candidata sem desempate.
    ContaAmbigua,
    /// Entrada com `-allow_third_party_software` (N10): recusada.
    TokenProibido,
    /// Já existe backup com conteúdo diferente; nunca sobrescrito.
    BackupDivergente(PathBuf),
    /// O backup não confere com o `.sha256` gravado.
    BackupCorrompido(PathBuf),
    Io(io::Error),
}

/// Motivo de relato correspondente ao erro.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motivo {
    FormatoInesperado,
    ArquivoSomenteLeitura,
    ReparseRecusado,
    ArquivoAusente,
    ArquivoNaNuvem,
    ArquivoAlteradoPeloJogo,
    ContaAmbigua,
    FalhaBackup,
    FalhaIo,
}

pub type Resultado<T> = Result<T, Erro>;

impl Erro {
    pub fn motivo(&self) -> Motivo {
        match self {
            Erro::FormatoInesperado(_) | Erro::ChaveAusente(_) | Erro::TokenProibido => {
                Motivo::FormatoInesperado
            }
            Erro::ArquivoSomenteLeitura(_) => Motivo::ArquivoSomenteLeitura,
            Erro::ReparseRecusado(_) => Motivo::ReparseRecusado,
            Erro::ArquivoAusente(_) => Motivo::ArquivoAusente,
            Erro::ArquivoNaNuvem(_) => Motivo::ArquivoNaNuvem,
            Erro::ArquivoAlteradoPeloJogo | Erro::HashDivergente => Motivo::ArquivoAlteradoPeloJogo,
            Erro::ContaAmbigua => Motivo::ContaAmbigua,
            Erro::BackupDivergente(_) | Erro::BackupCorrompido(_) => Motivo::FalhaBackup,
            Erro::Io(_) => Motivo::FalhaIo,
        }
    }

    pub(crate) fn formato(msg: impl Into<String>) -> Self {
        Erro::FormatoInesperado(msg.into())
    }
}

impl fmt::Display for Erro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Erro::FormatoInesperado(m) => write!(f, "formato inesperado: {m}"),
            Erro::ChaveAusente(c) => write!(f, "chave ausente: {c}"),
            Erro::ArquivoSomenteLeitura(p) => write!(f, "{} é somente leitura", p.display()),
            Erro::ReparseRecusado(p) => write!(f, "{} é link/junção; recusado", p.display()),
            Erro::ArquivoAusente(p) => write!(f, "{} não existe", p.display()),
            Erro::ArquivoNaNuvem(p) => write!(f, "{} está na Steam Cloud", p.display()),
            Erro::ArquivoAlteradoPeloJogo => write!(f, "arquivo alterado pelo jogo"),
            Erro::HashDivergente => write!(f, "conteúdo mudou desde a leitura"),
            Erro::ContaAmbigua => write!(f, "conta ambígua"),
            Erro::TokenProibido => write!(f, "-allow_third_party_software recusado"),
            Erro::BackupDivergente(p) => write!(f, "backup {} já existe e difere", p.display()),
            Erro::BackupCorrompido(p) => write!(f, "backup {} não confere", p.display()),
            Erro::Io(e) => write!(f, "E/S: {e}"),
        }
    }
}

impl std::error::Error for Erro {}

impl From<io::Error> for Erro {
    fn from(e: io::Error) -> Self {
        Erro::Io(e)
    }
}
