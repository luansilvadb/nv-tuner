//! Avisos estruturados do motor (abertura do estado).
//!
//! Quem consome (serviço, UI) decide pelo [`CodigoAviso`], nunca pelo texto:
//! [`Aviso::mensagem`] é a frase para exibição (pt-BR) e [`Aviso::detalhe`] o
//! texto técnico (caminhos, erro do JSON), que pode mudar entre versões.

use std::fmt;

/// Gravidade de um [`Aviso`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NivelAviso {
    /// Informativo: nada a fazer.
    Info,
    /// Merece atenção: algo foi contornado automaticamente.
    Atencao,
    /// Algo deixou de funcionar e precisa de uma pessoa.
    Erro,
}

/// Código estável de um [`Aviso`].
///
/// [`CodigoAviso::como_str`] devolve o mesmo texto de `nvt_ipc::codigo_aviso`
/// (protocolo v2); um código novo exige um código novo no protocolo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodigoAviso {
    /// `estado.json` inválido (no modo normal, guardado à parte como
    /// `.corrompido-<data>`; no somente leitura, mantido no lugar).
    EstadoCorrompido,
    /// Estado recuperado de `estado.anterior.json`.
    EstadoRecuperado,
    /// Sem cópia válida: o estado recomeçou limpo.
    EstadoReiniciado,
    /// `snapshot-original.json` ilegível: restauração desabilitada.
    SnapshotIndisponivel,
    /// O diretório de estado foi posto em quarentena e o snapshot original
    /// ficou lá: o snapshot atual (ou o próximo) é só uma base pós-quarentena.
    SnapshotOriginalPerdido,
}

impl CodigoAviso {
    /// Texto estável do código (igual ao do protocolo IPC).
    pub const fn como_str(self) -> &'static str {
        match self {
            Self::EstadoCorrompido => "estado_corrompido",
            Self::EstadoRecuperado => "estado_recuperado",
            Self::EstadoReiniciado => "estado_reiniciado",
            Self::SnapshotIndisponivel => "snapshot_indisponivel",
            Self::SnapshotOriginalPerdido => "snapshot_original_perdido",
        }
    }
}

impl fmt::Display for CodigoAviso {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.como_str())
    }
}

/// Aviso estruturado do motor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aviso {
    pub nivel: NivelAviso,
    pub codigo: CodigoAviso,
    /// Frase para exibição (pt-BR).
    pub mensagem: String,
    /// Detalhe técnico (caminhos, erro original).
    pub detalhe: Option<String>,
}

impl Aviso {
    pub fn novo(
        nivel: NivelAviso,
        codigo: CodigoAviso,
        mensagem: impl Into<String>,
        detalhe: Option<String>,
    ) -> Self {
        Self {
            nivel,
            codigo,
            mensagem: mensagem.into(),
            detalhe,
        }
    }
}

impl fmt::Display for Aviso {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.mensagem)?;
        if let Some(d) = &self.detalhe {
            write!(f, " ({d})")?;
        }
        Ok(())
    }
}
