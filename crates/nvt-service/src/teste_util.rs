//! Utilitários compartilhados pelos testes do crate.

use std::cell::{Cell, RefCell};
use std::io;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use nvt_ipc::pipe::IdentidadeCliente;

use crate::usuario::{Armazenamento, Coordenador, DadosUsuarios};

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

impl DirTemp {
    /// `<dir>\nv-tuner`: o diretório de estado dentro do temporário.
    pub(crate) fn estado(&self) -> PathBuf {
        self.0.join(nvt_core::produto::PASTA_DADOS)
    }
}

/// Pasta de atualização fixa dos testes (`C:\ProgramData\nv-tuner\atualizacao`),
/// montada como [`crate::atualizacao::arquivo::pasta_padrao`] sem consultar o Windows.
pub(crate) fn pasta_atualizacao_falsa() -> PathBuf {
    PathBuf::from(r"C:\ProgramData")
        .join(nvt_core::produto::PASTA_DADOS)
        .join("atualizacao")
}

/// `usuarios.json` em memória, com contagem de gravações e falha forçada.
#[derive(Clone, Default)]
pub(crate) struct Memoria {
    pub(crate) dados: Rc<RefCell<DadosUsuarios>>,
    pub(crate) gravacoes: Rc<Cell<u32>>,
    pub(crate) falhar: Rc<Cell<bool>>,
}

impl Armazenamento for Memoria {
    fn carregar(&self) -> io::Result<DadosUsuarios> {
        Ok(self.dados.borrow().clone())
    }

    fn salvar(&self, dados: &DadosUsuarios) -> io::Result<()> {
        if self.falhar.get() {
            return Err(io::Error::other("disco cheio"));
        }
        self.gravacoes.set(self.gravacoes.get() + 1);
        *self.dados.borrow_mut() = dados.clone();
        Ok(())
    }
}

impl Memoria {
    /// Dados atuais serializados.
    pub(crate) fn json(&self) -> String {
        serde_json::to_string(&*self.dados.borrow()).unwrap()
    }

    /// A regra aparece nos dados gravados.
    pub(crate) fn contem(&self, regra: &str) -> bool {
        self.json().contains(&format!("\"{regra}\""))
    }
}

/// Coordenador sobre `memoria` com as regras embutidas (build 22631, instante 0).
pub(crate) fn coordenador(memoria: &Memoria) -> Coordenador {
    Coordenador::abrir(
        memoria.clone(),
        &nvt_core::ConjuntoRegras::embutido().unwrap(),
        Some(22631),
        0,
    )
    .unwrap()
}

/// Identidade de cliente de teste.
pub(crate) fn cliente(sid: &str, sessao: u32) -> IdentidadeCliente {
    IdentidadeCliente {
        sid: sid.into(),
        sessao,
    }
}
