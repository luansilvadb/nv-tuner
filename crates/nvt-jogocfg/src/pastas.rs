//! Pastas conhecidas do usuário atrás de uma fachada injetável.

use crate::Resultado;
use std::path::PathBuf;

/// Fonte das pastas do perfil (Documentos pode estar redirecionado ao OneDrive).
pub trait PastasConhecidas {
    fn documentos(&self) -> Resultado<PathBuf>;
    fn local_app_data(&self) -> Resultado<PathBuf>;
}

/// `SHGetKnownFolderPath` do usuário do processo.
#[derive(Clone, Copy, Debug, Default)]
pub struct PastasWindows;

impl PastasConhecidas for PastasWindows {
    fn documentos(&self) -> Resultado<PathBuf> {
        crate::arquivo::pasta_conhecida(&windows_sys::Win32::UI::Shell::FOLDERID_Documents)
    }

    fn local_app_data(&self) -> Resultado<PathBuf> {
        crate::arquivo::pasta_conhecida(&windows_sys::Win32::UI::Shell::FOLDERID_LocalAppData)
    }
}

/// Pastas fixas (testes).
#[derive(Clone, Debug)]
pub struct PastasFixas {
    pub documentos: PathBuf,
    pub local_app_data: PathBuf,
}

impl PastasConhecidas for PastasFixas {
    fn documentos(&self) -> Resultado<PathBuf> {
        Ok(self.documentos.clone())
    }

    fn local_app_data(&self) -> Resultado<PathBuf> {
        Ok(self.local_app_data.clone())
    }
}
