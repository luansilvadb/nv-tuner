//! Vigia do diretório do DRS (`C:\ProgramData\NVIDIA Corporation\Drs`) com
//! `ReadDirectoryChangesW` em E/S sobreposta, para poder parar a qualquer
//! momento. O debounce e o filtro das gravações próprias ficam no
//! [`crate::nucleo::Nucleo`]; aqui só se avisa que algo mudou.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use tracing::{info, warn};
use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY,
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadDirectoryChangesW,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Threading::{CreateEventW, ResetEvent, WaitForSingleObject};

use crate::nucleo::Assinatura;

/// Diretório padrão do banco de perfis do driver.
pub fn diretorio_drs() -> PathBuf {
    let base = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    base.join(r"NVIDIA Corporation\Drs")
}

/// Assinatura dos arquivos do diretório (nome, tamanho, modificação em ns).
pub fn assinatura(dir: &Path) -> Option<Assinatura> {
    let mut v: Assinatura = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| {
            let e = e.ok()?;
            let m = e.metadata().ok()?;
            if !m.is_file() {
                return None;
            }
            let t = m
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_nanos();
            Some((e.file_name().to_string_lossy().into_owned(), m.len(), t))
        })
        .collect();
    v.sort();
    Some(v)
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: handle próprio, válido, fechado uma vez.
        unsafe { CloseHandle(self.0) };
    }
}

fn utf16(p: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str().encode_wide().chain([0]).collect()
}

/// Vigia `dir` até `parar` ficar verdadeiro, chamando `ao_mudar` a cada lote
/// de notificações. Se o diretório não existir (ou sumir), tenta de novo a
/// cada minuto.
pub fn vigiar(dir: &Path, parar: &Arc<AtomicBool>, mut ao_mudar: impl FnMut()) {
    while !parar.load(Ordering::Relaxed) {
        match vigiar_uma_vez(dir, parar, &mut ao_mudar) {
            Ok(()) => return,
            Err(e) => {
                warn!(
                    "vigia do DRS ({}): {e}; nova tentativa em 60 s",
                    dir.display()
                );
                for _ in 0..120 {
                    if parar.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    }
}

fn vigiar_uma_vez(
    dir: &Path,
    parar: &Arc<AtomicBool>,
    ao_mudar: &mut impl FnMut(),
) -> io::Result<()> {
    let nome = utf16(dir);
    // SAFETY: nome terminado em zero; retorno conferido.
    let h = unsafe {
        CreateFileW(
            nome.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
            null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let dir_h = Handle(h);
    // SAFETY: sem pré-condições; retorno conferido.
    let ev = unsafe { CreateEventW(null(), 1, 0, null()) };
    if ev.is_null() {
        return Err(io::Error::last_os_error());
    }
    let evento = Handle(ev);
    info!("vigiando {}", dir.display());
    // Alinhado a DWORD, como exige ReadDirectoryChangesW.
    let mut buf = vec![0u32; 16 * 1024];
    loop {
        // SAFETY: evento válido.
        unsafe { ResetEvent(evento.0) };
        // SAFETY: OVERLAPPED é POD; zerado é válido.
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.hEvent = evento.0;
        let tamanho = u32::try_from(buf.len() * 4).unwrap_or(u32::MAX);
        // SAFETY: buffer e OVERLAPPED vivem até a conclusão ou o cancelamento
        // (aguardados abaixo antes de sair do escopo).
        let ok = unsafe {
            ReadDirectoryChangesW(
                dir_h.0,
                buf.as_mut_ptr().cast(),
                tamanho,
                0,
                FILE_NOTIFY_CHANGE_LAST_WRITE
                    | FILE_NOTIFY_CHANGE_SIZE
                    | FILE_NOTIFY_CHANGE_FILE_NAME,
                null_mut(),
                &mut ov,
                None,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        loop {
            if parar.load(Ordering::Relaxed) {
                let mut n = 0u32;
                // SAFETY: cancela e espera a operação pendente terminar antes
                // de liberar `buf` e `ov`.
                unsafe {
                    CancelIoEx(dir_h.0, &ov);
                    GetOverlappedResult(dir_h.0, &ov, &mut n, 1);
                }
                return Ok(());
            }
            // SAFETY: evento válido.
            match unsafe { WaitForSingleObject(evento.0, 500) } {
                WAIT_OBJECT_0 => break,
                WAIT_TIMEOUT => {}
                _ => return Err(io::Error::last_os_error()),
            }
        }
        let mut n = 0u32;
        // SAFETY: a operação terminou (evento sinalizado).
        if unsafe { GetOverlappedResult(dir_h.0, &ov, &mut n, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // n == 0: estouro do buffer; ainda assim houve mudança.
        ao_mudar();
    }
}
