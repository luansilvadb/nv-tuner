//! Leitura de PE: imports (normais e delay-load) e VERSIONINFO.

use std::path::Path;

use pelite::{FileMap, PeFile, Wrap};

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
pub fn imports(caminho: &Path) -> Result<Imports, String> {
    let map = FileMap::open(caminho).map_err(|e| e.to_string())?;
    let pe = PeFile::from_bytes(&map).map_err(|e| e.to_string())?;
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

pub fn versao(caminho: &Path) -> Result<Versao, String> {
    let map = FileMap::open(caminho).map_err(|e| e.to_string())?;
    let pe = PeFile::from_bytes(&map).map_err(|e| e.to_string())?;
    let res = pe.resources().map_err(|e| e.to_string())?;
    let vi = res.version_info().map_err(|e| e.to_string())?;
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
