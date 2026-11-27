//! Varredura limitada da pasta do jogo.

use std::path::{Path, PathBuf};

/// Profundidade máxima da varredura (WARDOGS chega a 11 níveis).
pub const PROFUNDIDADE_MAX: usize = 14;
/// Teto de arquivos listados por jogo.
pub const ARQUIVOS_MAX: usize = 200_000;

/// Arquivo encontrado na instalação.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arquivo {
    pub caminho: PathBuf,
    /// Caminho relativo à pasta do jogo, minúsculo, com `\`.
    pub rel: String,
    /// Nome do arquivo em minúsculas.
    pub nome: String,
    pub tamanho: u64,
}

impl Arquivo {
    /// Componentes de pasta (minúsculos) do caminho relativo.
    pub fn pastas(&self) -> impl Iterator<Item = &str> {
        let mut partes: Vec<&str> = self.rel.split('\\').collect();
        partes.pop();
        partes.into_iter()
    }

    pub fn em_pasta(&self, nome_lc: &str) -> bool {
        self.pastas().any(|p| p == nome_lc)
    }
}

/// Lista arquivos e pastas (minúsculos) da instalação.
#[derive(Debug, Clone, Default)]
pub struct Listagem {
    pub arquivos: Vec<Arquivo>,
    /// Caminhos relativos de pastas, minúsculos.
    pub pastas: Vec<String>,
    pub truncada: bool,
}

pub fn listar(raiz: &Path) -> Listagem {
    listar_impl(raiz, false)
}

/// Lista metadados sem abrir o conteúdo dos arquivos. Falhas e limites de
/// percurso impedem usar a listagem para descartar executáveis em execução.
pub(super) fn listar_para_verificar_processos(raiz: &Path) -> Listagem {
    listar_impl(raiz, true)
}

fn listar_impl(raiz: &Path, exigir_completa: bool) -> Listagem {
    let mut l = Listagem::default();
    let mut pilha: Vec<(PathBuf, String, usize)> = vec![(raiz.to_path_buf(), String::new(), 0)];
    while let Some((dir, rel, prof)) = pilha.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            l.truncada |= exigir_completa;
            continue;
        };
        for e in rd {
            let Ok(e) = e else {
                l.truncada |= exigir_completa;
                continue;
            };
            let Ok(ft) = e.file_type() else {
                l.truncada |= exigir_completa;
                continue;
            };
            let nome = e.file_name().to_string_lossy().to_lowercase();
            let rel_e = if rel.is_empty() {
                nome.clone()
            } else {
                format!("{rel}\\{nome}")
            };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                l.pastas.push(rel_e.clone());
                if prof + 1 < PROFUNDIDADE_MAX {
                    pilha.push((e.path(), rel_e, prof + 1));
                } else {
                    l.truncada |= exigir_completa;
                }
            } else if ft.is_file() {
                if l.arquivos.len() >= ARQUIVOS_MAX {
                    l.truncada = true;
                    return l;
                }
                let tamanho = e.metadata().map(|m| m.len()).unwrap_or(0);
                l.arquivos.push(Arquivo {
                    caminho: e.path(),
                    rel: rel_e,
                    nome,
                    tamanho,
                });
            }
        }
    }
    l.arquivos.sort_by(|a, b| a.rel.cmp(&b.rel));
    l.pastas.sort();
    l
}
