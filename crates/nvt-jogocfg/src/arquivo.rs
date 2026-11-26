//! Acesso ao sistema de arquivos (F3-05, F3-06; S3). Único módulo com `unsafe`.
//!
//! - Todo caminho é validado sob uma raiz conhecida: a raiz e cada componente abaixo
//!   dela (inclusive o arquivo) não podem ser reparse points (link, junção, nuvem).
//! - Leitura com compartilhamento total e handle curto, sem seguir reparse point.
//! - Gravação: nunca toca arquivo com `FILE_ATTRIBUTE_READONLY` (nem altera o atributo);
//!   confere o hash esperado, escreve um temporário na mesma pasta, `FlushFileBuffers`
//!   e troca com `ReplaceFileW` (preserva atributos, ACL e data de criação do original).

use crate::{Erro, Hash, Resultado, sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    ReplaceFileW,
};

/// Tamanho máximo aceito para um arquivo de configuração.
pub const TAMANHO_MAXIMO: u64 = 16 * 1024 * 1024;

/// Conteúdo lido e estado do arquivo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lido {
    pub bytes: Vec<u8>,
    pub hash: Hash,
    /// `FILE_ATTRIBUTE_READONLY` presente (a gravação será recusada).
    pub somente_leitura: bool,
}

/// Confere que `caminho` está sob `raiz` (sem `..`) e que nem a raiz nem nenhum
/// componente abaixo dela é reparse point. Componente ausente → [`Erro::ArquivoAusente`].
pub fn verificar_componentes(raiz: &Path, caminho: &Path) -> Resultado<()> {
    let rel = caminho
        .strip_prefix(raiz)
        .map_err(|_| Erro::formato("caminho fora da raiz conhecida"))?;
    let mut atual = raiz.to_path_buf();
    verificar_um(&atual)?;
    for c in rel.components() {
        match c {
            Component::Normal(n) => atual.push(n),
            _ => return Err(Erro::formato("componente de caminho não permitido")),
        }
        verificar_um(&atual)?;
    }
    Ok(())
}

fn verificar_um(p: &Path) -> Resultado<()> {
    match fs::symlink_metadata(p) {
        Ok(m) if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 => {
            Err(Erro::ReparseRecusado(p.to_path_buf()))
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(Erro::ArquivoAusente(p.into())),
        Err(e) => Err(e.into()),
    }
}

/// Existe como arquivo comum sob `raiz`? Reparse no caminho é erro; ausência é `false`.
pub fn existe(raiz: &Path, caminho: &Path) -> Resultado<bool> {
    match verificar_componentes(raiz, caminho) {
        Ok(()) => Ok(fs::symlink_metadata(caminho)?.is_file()),
        Err(Erro::ArquivoAusente(_)) => Ok(false),
        Err(e) => Err(e),
    }
}

fn abrir(caminho: &Path) -> Resultado<(File, u32)> {
    let f = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(caminho)
        .map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => Erro::ArquivoAusente(caminho.into()),
            _ => e.into(),
        })?;
    let m = f.metadata()?;
    let a = m.file_attributes();
    if a & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(Erro::ReparseRecusado(caminho.into()));
    }
    if a & FILE_ATTRIBUTE_DIRECTORY != 0 {
        return Err(Erro::formato("o caminho é uma pasta"));
    }
    if m.len() > TAMANHO_MAXIMO {
        return Err(Erro::formato("arquivo grande demais"));
    }
    Ok((f, a))
}

/// Lê o arquivo inteiro (compartilhamento total; o handle é fechado ao retornar).
pub fn ler(raiz: &Path, caminho: &Path) -> Resultado<Lido> {
    verificar_componentes(raiz, caminho)?;
    let (mut f, atributos) = abrir(caminho)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut f)
        .take(TAMANHO_MAXIMO + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > TAMANHO_MAXIMO {
        return Err(Erro::formato("arquivo grande demais"));
    }
    Ok(Lido {
        hash: sha256(&bytes),
        somente_leitura: atributos & FILE_ATTRIBUTE_READONLY != 0,
        bytes,
    })
}

static SEQUENCIA: AtomicU32 = AtomicU32::new(0);

/// Grava `novo` em `caminho` de forma atômica se o conteúdo atual tiver `hash_esperado`.
/// Devolve o hash do conteúdo relido após a troca.
pub fn gravar(raiz: &Path, caminho: &Path, novo: &[u8], hash_esperado: &Hash) -> Resultado<Hash> {
    let atual = ler(raiz, caminho)?;
    if atual.somente_leitura {
        return Err(Erro::ArquivoSomenteLeitura(caminho.into()));
    }
    if atual.hash != *hash_esperado {
        return Err(Erro::HashDivergente);
    }
    let pasta = caminho
        .parent()
        .ok_or_else(|| Erro::formato("caminho sem pasta"))?;
    let nome = caminho
        .file_name()
        .ok_or_else(|| Erro::formato("caminho sem nome"))?;
    let temp = pasta.join(format!(
        "{}.nvt-{}-{}.tmp",
        nome.to_string_lossy(),
        std::process::id(),
        SEQUENCIA.fetch_add(1, Ordering::Relaxed)
    ));
    let r = escrever_e_trocar(caminho, &temp, novo);
    if r.is_err() {
        let _ = fs::remove_file(&temp);
    }
    r?;
    let relido = ler(raiz, caminho)?;
    if relido.hash != sha256(novo) {
        return Err(Erro::HashDivergente);
    }
    Ok(relido.hash)
}

fn escrever_e_trocar(caminho: &Path, temp: &Path, novo: &[u8]) -> Resultado<()> {
    {
        let mut t = OpenOptions::new().write(true).create_new(true).open(temp)?;
        t.write_all(novo)?;
        t.sync_all()?; // FlushFileBuffers
    }
    verificar_um(caminho)?;
    let alvo = largo(caminho.as_os_str());
    let subst = largo(temp.as_os_str());
    // SAFETY: as duas cadeias são UTF-16 terminadas em NUL e vivem até o fim da
    // chamada; o nome de backup e os ponteiros reservados são nulos, como a API permite.
    let ok = unsafe {
        ReplaceFileW(
            alvo.as_ptr(),
            subst.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}

fn largo(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

/// Pasta conhecida do usuário atual por `SHGetKnownFolderPath` (Documentos segue o
/// redirecionamento para o OneDrive).
pub(crate) fn pasta_conhecida(id: &windows_sys::core::GUID) -> Resultado<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;

    let mut p: windows_sys::core::PWSTR = std::ptr::null_mut();
    // SAFETY: `id` aponta para um GUID válido; token nulo = usuário atual; `p` recebe
    // um buffer alocado pela API, liberado abaixo com CoTaskMemFree em qualquer caso.
    let hr = unsafe { SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut p) };
    let r = if hr < 0 || p.is_null() {
        Err(Erro::Io(io::Error::from_raw_os_error(hr)))
    } else {
        // SAFETY: em sucesso `p` é uma cadeia UTF-16 terminada em NUL.
        let fatia = unsafe {
            let mut n = 0;
            while *p.add(n) != 0 {
                n += 1;
            }
            std::slice::from_raw_parts(p, n)
        };
        Ok(PathBuf::from(std::ffi::OsString::from_wide(fatia)))
    };
    // SAFETY: `p` veio de SHGetKnownFolderPath (ou é nulo, aceito pela API).
    unsafe { CoTaskMemFree(p.cast()) };
    r
}
