//! Utilitários compartilhados pelos testes do crate.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

/// Diretório temporário único, removido no `Drop`.
pub(crate) struct DirTemp(pub(crate) PathBuf);

impl DirTemp {
    /// Caminho único em `%TEMP%`, ainda inexistente.
    pub(crate) fn novo(nome: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "nvt-{nome}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        Self(p)
    }

    /// Como [`DirTemp::novo`], já criado.
    pub(crate) fn criado(nome: &str) -> Self {
        let t = Self::novo(nome);
        std::fs::create_dir_all(&t.0).unwrap();
        t
    }
}

impl Drop for DirTemp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
