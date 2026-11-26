#![allow(dead_code)]

use nvt_jogocfg::Codificacao;
use std::fs;
use std::path::{Path, PathBuf};

pub fn amostras() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("amostras")
}

pub fn amostra(nome: &str) -> Vec<u8> {
    fs::read(amostras().join(nome)).unwrap()
}

pub fn texto(nome: &str) -> String {
    String::from_utf8(amostra(nome)).unwrap()
}

/// Texto codificado sem BOM (para localizar trechos).
pub fn corpo(c: Codificacao, s: &str) -> Vec<u8> {
    match c {
        Codificacao::Utf16LeBom => s.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        _ => s.as_bytes().to_vec(),
    }
}

pub const CODIFICACOES: [Codificacao; 3] = [
    Codificacao::Utf8,
    Codificacao::Utf8Bom,
    Codificacao::Utf16LeBom,
];

/// Existe `k` tal que só `velho` em `antes[k..]` virou `novo` em `depois[k..]`.
pub fn so_trecho_mudou(antes: &[u8], depois: &[u8], velho: &[u8], novo: &[u8]) -> bool {
    if antes.len() + novo.len() != depois.len() + velho.len() {
        return false;
    }
    (0..=antes.len().saturating_sub(velho.len())).any(|k| {
        antes[k..].starts_with(velho)
            && depois[k..].starts_with(novo)
            && antes[..k] == depois[..k]
            && antes[k + velho.len()..] == depois[k + novo.len()..]
    })
}

/// Pasta temporária própria, removida no fim (limpa somente leitura antes).
pub struct Temp(pub PathBuf);

impl Temp {
    pub fn novo(nome: &str) -> Temp {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "nvt-jogocfg-{nome}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Temp(p)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        limpar_somente_leitura(&self.0);
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn limpar_somente_leitura(p: &Path) {
    let Ok(m) = fs::symlink_metadata(p) else {
        return;
    };
    if m.is_dir() && !m.file_type().is_symlink() {
        if let Ok(rd) = fs::read_dir(p) {
            for e in rd.flatten() {
                limpar_somente_leitura(&e.path());
            }
        }
    } else if m.permissions().readonly() {
        let mut perm = m.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        let _ = fs::set_permissions(p, perm);
    }
}

/// Junção `link` → `alvo` por `FSCTL_SET_REPARSE_POINT` (sem processo externo).
pub fn juncao(link: &Path, alvo: &Path) {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{GENERIC_WRITE, HANDLE};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    fs::create_dir(link).unwrap();
    let d = OpenOptions::new()
        .access_mode(GENERIC_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(link)
        .unwrap();
    let alvo: Vec<u16> = format!(r"\??\{}", alvo.display()).encode_utf16().collect();
    let sub = (alvo.len() * 2) as u16;
    let mut caminho = alvo.clone();
    caminho.extend([0, 0]);
    let dados = 8 + (caminho.len() * 2) as u16;
    let mut buf: Vec<u8> = Vec::new();
    buf.extend(0xA000_0003u32.to_le_bytes()); // IO_REPARSE_TAG_MOUNT_POINT
    buf.extend(dados.to_le_bytes());
    buf.extend(0u16.to_le_bytes());
    for x in [0u16, sub, sub + 2, 0] {
        buf.extend(x.to_le_bytes());
    }
    for c in caminho {
        buf.extend(c.to_le_bytes());
    }
    let mut n = 0u32;
    // SAFETY: buffer REPARSE_DATA_BUFFER montado acima; handle válido durante a chamada.
    let ok = unsafe {
        DeviceIoControl(
            d.as_raw_handle() as HANDLE,
            0x0009_00A4, // FSCTL_SET_REPARSE_POINT
            buf.as_ptr().cast(),
            buf.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut n,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(ok, 0, "junção: {}", std::io::Error::last_os_error());
}
