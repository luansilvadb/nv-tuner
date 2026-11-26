//! Leitura de PE: imports (normais e delay-load) e VERSIONINFO.

use std::fmt;
use std::fs::OpenOptions;
use std::io::Read;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;

use pelite::{PeFile, Wrap};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

/// Tamanho máximo de arquivo lido para análise de PE (256 MiB).
pub const LIMITE_LEITURA: u64 = 256 * 1024 * 1024;

/// Falha ao ler um PE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroPe {
    /// Arquivo acima de [`LIMITE_LEITURA`]: o fato fica desconhecido.
    GrandeDemais(u64),
    /// Falha de E/S ao abrir ou ler.
    Io(String),
    /// Conteúdo não é um PE válido (ou falta a estrutura pedida).
    Formato(String),
}

impl fmt::Display for ErroPe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroPe::GrandeDemais(n) => write!(f, "arquivo grande demais ({n} bytes)"),
            ErroPe::Io(e) => write!(f, "E/S: {e}"),
            ErroPe::Formato(e) => write!(f, "PE inválido: {e}"),
        }
    }
}

impl std::error::Error for ErroPe {}

fn formato(e: impl fmt::Display) -> ErroPe {
    ErroPe::Formato(e.to_string())
}

/// Lê o arquivo inteiro para um buffer com compartilhamento total
/// (`FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`), sem mapeamento.
/// O handle é fechado antes de retornar (regra S3 / F1-08).
pub fn ler_compartilhado(caminho: &Path) -> Result<Vec<u8>, ErroPe> {
    ler_com_limite(caminho, LIMITE_LEITURA, |_| {})
}

/// Núcleo de [`ler_compartilhado`]; `durante` roda com o handle ainda aberto
/// (usado nos testes de compartilhamento).
fn ler_com_limite(
    caminho: &Path,
    limite: u64,
    durante: impl FnOnce(&Path),
) -> Result<Vec<u8>, ErroPe> {
    let arquivo = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(caminho)
        .map_err(|e| ErroPe::Io(e.to_string()))?;
    let tamanho = arquivo
        .metadata()
        .map_err(|e| ErroPe::Io(e.to_string()))?
        .len();
    if tamanho > limite {
        return Err(ErroPe::GrandeDemais(tamanho));
    }
    let mut buf = Vec::with_capacity(tamanho as usize);
    // O arquivo pode crescer entre o metadata e a leitura: nunca passa do limite.
    let mut leitor = arquivo.take(limite + 1);
    let lido = leitor.read_to_end(&mut buf);
    durante(caminho);
    // Fecha o handle antes de qualquer parse.
    drop(leitor);
    lido.map_err(|e| ErroPe::Io(e.to_string()))?;
    if buf.len() as u64 > limite {
        return Err(ErroPe::GrandeDemais(buf.len() as u64));
    }
    Ok(buf)
}

/// DLLs importadas pelo exe.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Imports {
    /// Nomes em minúsculas, sem repetição.
    pub dlls: Vec<String>,
    /// Entradas de delay-load que não puderam ser lidas (nome inválido/cifrado).
    pub delay_ilegiveis: usize,
}

fn nome_valido(s: &str) -> bool {
    !s.is_empty()
        && s.len() < 128
        && s.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
        && s.to_ascii_lowercase().ends_with(".dll")
}

fn adicionar(dlls: &mut Vec<String>, n: &str) {
    let n = n.to_ascii_lowercase();
    if !dlls.contains(&n) {
        dlls.push(n);
    }
}

/// `IMAGE_DIRECTORY_ENTRY_IMPORT`.
const DIR_IMPORT: usize = 1;
/// `IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT`.
const DIR_DELAY_IMPORT: usize = 13;

/// Lê `n` palavras de 32 bits a partir de um RVA, sem exigir alinhamento
/// (exes protegidos, como o do LoL, deixam a tabela em endereço desalinhado).
fn palavras(pe: &PeFile<'_>, rva: u32, n: usize) -> Option<Vec<u32>> {
    let b = pe.derva_slice::<u8>(rva, n * 4).ok()?;
    Some(
        b.chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

fn nome_em(pe: &PeFile<'_>, rva: u32) -> Option<String> {
    let c = pe.derva_c_str(rva).ok()?;
    let s = c.to_str().ok()?;
    nome_valido(s).then(|| s.to_string())
}

/// Percorre uma tabela de descritores (`IMAGE_IMPORT_DESCRIPTOR` ou `ImgDelayDescr`).
fn tabela(pe: &PeFile<'_>, dir_idx: usize, out: &mut Imports) {
    let Some(dir) = pe.data_directory().get(dir_idx) else {
        return;
    };
    if dir.VirtualAddress == 0 || dir.Size == 0 {
        return;
    }
    let delay = dir_idx == DIR_DELAY_IMPORT;
    let (tam, campo_nome) = if delay { (8, 1) } else { (5, 3) };
    let base = match pe.optional_header() {
        Wrap::T32(h) => h.ImageBase as u64,
        Wrap::T64(h) => h.ImageBase,
    };
    for i in 0..512u32 {
        let rva = dir.VirtualAddress.wrapping_add(i * tam as u32 * 4);
        let Some(d) = palavras(pe, rva, tam) else {
            if delay {
                out.delay_ilegiveis += 1;
            }
            break;
        };
        if d.iter().all(|x| *x == 0) {
            break;
        }
        // Delay-load: bit 0 de grAttrs indica RVA; senão é VA antigo (subtrair ImageBase).
        let nome_rva = if delay && d[0] & 1 == 0 {
            (d[campo_nome] as u64).wrapping_sub(base) as u32
        } else {
            d[campo_nome]
        };
        match nome_em(pe, nome_rva) {
            Some(s) => adicionar(&mut out.dlls, &s),
            None if delay => out.delay_ilegiveis += 1,
            None => {}
        }
    }
}

/// Lê imports e delay-imports de um exe.
pub fn imports(caminho: &Path) -> Result<Imports, ErroPe> {
    let bytes = ler_compartilhado(caminho)?;
    let pe = PeFile::from_bytes(&bytes).map_err(formato)?;
    let mut out = Imports::default();
    tabela(&pe, DIR_IMPORT, &mut out);
    tabela(&pe, DIR_DELAY_IMPORT, &mut out);
    Ok(out)
}

/// Dados de VERSIONINFO de uma DLL.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Versao {
    /// `FileVersion` fixo (`Major.Minor.Patch.Build`), ex.: "310.6.0.0".
    pub versao: Option<String>,
    pub descricao: Option<String>,
    pub produto: Option<String>,
}

pub fn versao(caminho: &Path) -> Result<Versao, ErroPe> {
    let bytes = ler_compartilhado(caminho)?;
    let pe = PeFile::from_bytes(&bytes).map_err(formato)?;
    let res = pe.resources().map_err(formato)?;
    let vi = res.version_info().map_err(formato)?;
    let mut v = Versao {
        versao: vi.fixed().map(|f| f.dwFileVersion.to_string()),
        ..Default::default()
    };
    if let Some(lang) = vi.translation().first().copied() {
        v.descricao = vi.value(lang, "FileDescription");
        v.produto = vi.value(lang, "ProductName");
    }
    if v.descricao.is_none() {
        // Algumas DLLs não têm tabela de tradução; percorre as strings disponíveis.
        let mut desc = None;
        let mut prod = None;
        vi.visit(&mut ColetorStrings {
            desc: &mut desc,
            prod: &mut prod,
        });
        v.descricao = desc;
        v.produto = v.produto.or(prod);
    }
    Ok(v)
}

struct ColetorStrings<'b> {
    desc: &'b mut Option<String>,
    prod: &'b mut Option<String>,
}

impl<'a> pelite::resources::version_info::Visit<'a> for ColetorStrings<'_> {
    fn string(&mut self, key: &'a [u16], value: &'a [u16]) {
        let k = String::from_utf16_lossy(key);
        let v = String::from_utf16_lossy(value)
            .trim_end_matches('\0')
            .to_string();
        match k.trim_end_matches('\0') {
            "FileDescription" if self.desc.is_none() => *self.desc = Some(v),
            "ProductName" if self.prod.is_none() => *self.prod = Some(v),
            _ => {}
        }
    }
}

/// Compara versões "a.b.c.d" numericamente.
pub fn comparar_versoes(a: &str, b: &str) -> std::cmp::Ordering {
    let p = |s: &str| -> Vec<u32> { s.split('.').map(|x| x.parse().unwrap_or(0)).collect() };
    p(a).cmp(&p(b))
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::path::PathBuf;

    fn temporario(nome: &str, conteudo: &[u8]) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("nvt-detect-pe-{}-{nome}.bin", std::process::id()));
        std::fs::write(&p, conteudo).unwrap();
        p
    }

    #[test]
    fn leitura_compartilhada_e_handle_fechado() {
        let p = temporario("share", b"MZ conteudo qualquer");
        let mut escrita_ok = false;
        let bytes = ler_com_limite(&p, LIMITE_LEITURA, |c| {
            // Com o handle de leitura aberto, outro handle abre para escrita.
            escrita_ok = OpenOptions::new()
                .write(true)
                .share_mode(FILE_SHARE_READ)
                .open(c)
                .is_ok();
        })
        .unwrap();
        assert!(escrita_ok, "o handle de leitura não compartilha escrita");
        assert_eq!(bytes, b"MZ conteudo qualquer");
        // Depois de retornar, não resta handle: abertura exclusiva funciona.
        let exclusivo = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&p);
        assert!(exclusivo.is_ok(), "handle ficou aberto: {exclusivo:?}");
        drop(exclusivo);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn acima_do_limite_e_desconhecido() {
        let p = temporario("grande", &[0u8; 64]);
        assert_eq!(
            ler_com_limite(&p, 16, |_| {}),
            Err(ErroPe::GrandeDemais(64))
        );
        let exclusivo = OpenOptions::new().read(true).share_mode(0).open(&p);
        assert!(exclusivo.is_ok());
        drop(exclusivo);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn nao_pe_e_erro_de_formato() {
        let p = temporario("naope", b"isto nao e um PE");
        assert!(matches!(imports(&p), Err(ErroPe::Formato(_))));
        assert!(matches!(versao(&p), Err(ErroPe::Formato(_))));
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn le_pe_real_do_sistema() {
        let k = Path::new(r"C:\Windows\System32\kernel32.dll");
        if k.exists() {
            let imp = imports(k).unwrap();
            assert!(!imp.dlls.is_empty());
            assert!(versao(k).unwrap().versao.is_some());
        }
    }
}
