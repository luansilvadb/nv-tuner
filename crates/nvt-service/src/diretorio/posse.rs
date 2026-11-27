//! Abertura sem reparse, posse do diretório e privilégio do processo.

use super::*;

/// Abre um arquivo sem seguir reparse points e recusa o que não for um
/// arquivo comum com um único vínculo. `opcoes` define leitura/escrita/criação.
pub fn abrir_sem_reparse(caminho: &Path, opcoes: &mut OpenOptions) -> io::Result<File> {
    let f = opcoes
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(caminho)?;
    let i = info(&f)?;
    if i.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other(format!(
            "{} é junção/link; recusado",
            caminho.display()
        )));
    }
    if i.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 || i.nNumberOfLinks > 1 {
        return Err(io::Error::other(format!(
            "{} não é um arquivo comum com um único vínculo; recusado",
            caminho.display()
        )));
    }
    Ok(f)
}

/// O diretório existe e não é reparse point (para subpastas como `logs`).
pub fn diretorio_comum(p: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(p) {
        Ok(m) => {
            use std::os::windows::fs::MetadataExt;
            Ok(m.is_dir() && m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// SID de Administradores (`S-1-5-32-544`) num buffer próprio.
pub(super) fn sid_administradores() -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; SECURITY_MAX_SID_SIZE as usize];
    let mut n = SECURITY_MAX_SID_SIZE;
    // SAFETY: buffer do tamanho máximo de um SID.
    if unsafe {
        CreateWellKnownSid(
            WinBuiltinAdministratorsSid,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut n,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    buf.truncate(n as usize);
    Ok(buf)
}

/// Torna Administradores o dono de `p` (sem seguir reparse points).
pub(super) fn tomar_posse(p: &Path) -> io::Result<()> {
    let f = OpenOptions::new()
        .access_mode(READ_CONTROL | WRITE_OWNER | FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(p)?;
    if info(&f)?.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("reparse point"));
    }
    let mut sid = sid_administradores()?;
    // SAFETY: handle com WRITE_OWNER; `sid` válido durante a chamada.
    let r = unsafe {
        SetSecurityInfo(
            f.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            sid.as_mut_ptr().cast(),
            null_mut(),
            null(),
            null(),
        )
    };
    if r != 0 {
        return Err(io::Error::from_raw_os_error(r as i32));
    }
    Ok(())
}

/// Faz de Administradores o dono padrão dos objetos que este processo criar
/// (num administrador elevado, o padrão pode ser o próprio usuário, que
/// manteria `WRITE_DAC` implícito sobre os arquivos de estado mesmo sem
/// elevação). Só funciona em processo privilegiado.
pub fn definir_dono_padrao() -> io::Result<()> {
    let mut sid = sid_administradores()?;
    let mut token: HANDLE = null_mut();
    // SAFETY: pseudo-handle do processo atual; `token` recebe um handle próprio.
    if unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_DEFAULT | TOKEN_QUERY,
            &mut token,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let dono = TOKEN_OWNER {
        Owner: sid.as_mut_ptr().cast(),
    };
    // SAFETY: estrutura válida durante a chamada; o token é fechado em seguida.
    let ok = unsafe {
        SetTokenInformation(
            token,
            TokenOwner,
            (&raw const dono).cast(),
            size_of::<TOKEN_OWNER>() as u32,
        )
    } != 0;
    let erro = io::Error::last_os_error();
    // SAFETY: handle aberto acima.
    unsafe { CloseHandle(token) };
    if ok { Ok(()) } else { Err(erro) }
}

/// Processo com privilégio de administrador (inclui SYSTEM).
pub fn processo_privilegiado() -> bool {
    // SAFETY: sem pré-condições.
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

/// Modo adequado ao processo atual.
pub fn modo_do_processo() -> Modo {
    if processo_privilegiado() {
        Modo::Exigir
    } else {
        Modo::Avisar
    }
}
