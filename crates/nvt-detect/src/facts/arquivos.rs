//! Varredura limitada da pasta do jogo.

use std::path::{Path, PathBuf};

/// Profundidade máxima da varredura (WARDOGS chega a 11 níveis).
pub const PROFUNDIDADE_MAX: usize = 14;
/// Teto de entradas enumeradas por jogo, incluindo diretórios.
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
    listar_limitado(raiz, raiz).0
}

pub(super) fn listar_limitado(raiz: &Path, pasta: &Path) -> (Listagem, bool) {
    let mut listagem = Listagem::default();
    let fim = percorrer(raiz, pasta, PROFUNDIDADE_MAX, ARQUIVOS_MAX, |e, m| {
        let nome = e.file_name().to_string_lossy().to_lowercase();
        let caminho = e.path();
        let rel = caminho
            .strip_prefix(pasta)
            .expect("entrada sob a raiz")
            .to_string_lossy()
            .replace('/', "\\")
            .to_lowercase();
        if m.is_dir() {
            listagem.pastas.push(rel);
        } else if m.is_file() {
            listagem.arquivos.push(Arquivo {
                caminho,
                rel,
                nome,
                tamanho: m.len(),
            });
        }
    });
    listagem.truncada = fim != Fim::Completo;
    listagem.arquivos.sort_by(|a, b| a.rel.cmp(&b.rel));
    listagem.pastas.sort();
    (listagem, fim == Fim::Falha)
}

/// Teto de profundidade da verificação ampliada de nomes.
const PROFUNDIDADE_NOMES: usize = 64;
/// Inclui diretórios e outros tipos de entrada para limitar o trabalho total.
const ENTRADAS_NOMES: usize = 2_000_000;

pub(super) fn coletar_exes(
    raiz: &Path,
    xbox: bool,
    nomes: &mut std::collections::HashSet<String>,
) -> bool {
    // Na raiz externa do Xbox, inclui o nível de Content e a listagem
    // dos diretórios do último nível para verificar se estão vazios.
    let profundidade = PROFUNDIDADE_NOMES + 2 * usize::from(xbox);
    percorrer(raiz, raiz, profundidade, ENTRADAS_NOMES, |e, m| {
        let nome = e.file_name();
        if m.is_file()
            && nome
                .as_encoded_bytes()
                .last_chunk::<4>()
                .is_some_and(|sufixo| sufixo.eq_ignore_ascii_case(b".exe"))
        {
            nomes.insert(nome.to_string_lossy().to_lowercase());
        }
    }) == Fim::Completo
}

#[derive(PartialEq, Eq)]
enum Fim {
    Completo,
    Limite,
    Teto,
    Falha,
}

fn percorrer(
    raiz: &Path,
    pasta: &Path,
    profundidade: usize,
    mut teto: usize,
    mut visitar: impl FnMut(&std::fs::DirEntry, &std::fs::Metadata),
) -> Fim {
    let Ok(_guardas) = proteger_diretorios(raiz, pasta) else {
        return Fim::Falha;
    };
    percorrer_diretorio(pasta, profundidade, &mut teto, &mut visitar)
}

fn percorrer_diretorio(
    dir: &Path,
    profundidade: usize,
    restantes: &mut usize,
    visitar: &mut impl FnMut(&std::fs::DirEntry, &std::fs::Metadata),
) -> Fim {
    let Ok(entradas) = std::fs::read_dir(dir) else {
        return Fim::Falha;
    };
    let mut fim = Fim::Completo;
    for entrada in entradas {
        let Ok(e) = entrada else {
            return Fim::Falha;
        };
        if *restantes == 0 {
            return Fim::Teto;
        }
        *restantes -= 1;
        let Ok(m) = e.metadata() else {
            return Fim::Falha;
        };
        if ponto_de_reanalise(&m) {
            continue;
        }
        visitar(&e, &m);
        if m.is_dir() {
            let subpasta = if profundidade > 1 {
                let pasta = e.path();
                let Ok(_guarda) = proteger_diretorio(&pasta, false) else {
                    return Fim::Falha;
                };
                percorrer_diretorio(&pasta, profundidade - 1, restantes, visitar)
            } else {
                Fim::Limite
            };
            match subpasta {
                Fim::Falha | Fim::Teto => return subpasta,
                Fim::Limite => fim = Fim::Limite,
                Fim::Completo => {}
            }
        }
    }
    fim
}

pub(crate) fn ponto_de_reanalise(m: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

/// Mantém os diretórios verificados abertos com compartilhamento apenas para
/// leitura até terminar a operação. Segue pontos de reanálise até a raiz da
/// instalação, inclusive, e abre os componentes abaixo dela sem segui-los.
pub(super) fn proteger_diretorios(raiz: &Path, dir: &Path) -> std::io::Result<Vec<std::fs::File>> {
    use std::path::Component;

    let invalido = || std::io::Error::other("caminho de instalação não verificado");
    let relativo = dir.strip_prefix(raiz).map_err(|_| invalido())?;
    let mut caminho = raiz.to_path_buf();
    let mut guardas = vec![proteger_diretorio(raiz, true)?];
    for componente in relativo.components() {
        let Component::Normal(nome) = componente else {
            return Err(invalido());
        };
        caminho.push(nome);
        guardas.push(proteger_diretorio(&caminho, false)?);
    }
    Ok(guardas)
}

fn proteger_diretorio(caminho: &Path, raiz: bool) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
        FILE_SHARE_READ,
    };

    let flags = if raiz {
        FILE_FLAG_BACKUP_SEMANTICS
    } else {
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT
    };
    let guarda = std::fs::OpenOptions::new()
        .access_mode(FILE_LIST_DIRECTORY)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(flags)
        .open(caminho)?;
    let dados = guarda.metadata()?;
    if ponto_de_reanalise(&dados) {
        return Err(std::io::Error::other(
            "caminho de instalação não verificado",
        ));
    }
    if !dados.is_dir() {
        return Err(std::io::ErrorKind::NotADirectory.into());
    }
    Ok(guarda)
}
