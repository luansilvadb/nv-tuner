//! Adaptador Windows da limpeza (R-L-01, R-L-03, R-L-05). Único lugar com a abertura
//! `DELETE` e a marcação de exclusão (`ExcluidorReal`).

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, GetLastError, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    self as fsys, CreateFileW, DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_READONLY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM, FILE_BASIC_INFO,
    FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_INFO_EX, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, GetFileInformationByHandleEx,
    OPEN_EXISTING, SetFileInformationByHandle,
};
use windows_sys::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use windows_sys::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};

use crate::{
    AlvoLimpeza, Entrada, ErroRaiz, Excluidor, Instalacao, MotivoPulo, Raiz, SistemaArquivos,
};

/// 100 ns entre 1601-01-01 e 1970-01-01.
const EPOCA_FILETIME: u64 = 116_444_736_000_000_000;

fn filetime_unix(ft: u64) -> u64 {
    ft.saturating_sub(EPOCA_FILETIME) / 10_000_000
}

fn largo(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

/// Raiz do alvo pedido pelo chamador: `TempUsuario` = temporário da sessão
/// (`GetTempPath2W` via `std`); `TempWindows` = `GetSystemWindowsDirectoryW` + `Temp`.
pub fn raiz_do_alvo(alvo: AlvoLimpeza) -> Result<Raiz, ErroRaiz> {
    let caminho = match alvo {
        AlvoLimpeza::TempUsuario => {
            let t = std::env::temp_dir();
            PathBuf::from(t.to_string_lossy().trim_end_matches('\\'))
        }
        AlvoLimpeza::TempWindows => {
            let mut buf = [0u16; 512];
            // SAFETY: buffer válido com o tamanho informado.
            let n = unsafe { GetSystemWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as u32) };
            if n == 0 || n as usize >= buf.len() {
                return Err(ErroRaiz::NaoNormalizada);
            }
            PathBuf::from(String::from_utf16_lossy(&buf[..n as usize])).join("Temp")
        }
    };
    Raiz::nova(alvo, caminho)
}

/// Enumeração por `std::fs::read_dir` (`FindFirstFileExW`): os metadados vêm da própria
/// enumeração e nunca seguem reparse points.
#[derive(Debug, Default, Clone, Copy)]
pub struct SistemaArquivosWindows;

impl SistemaArquivos for SistemaArquivosWindows {
    fn listar(&self, dir: &Path) -> Result<Vec<Entrada>, MotivoPulo> {
        let it = std::fs::read_dir(dir).map_err(|_| MotivoPulo::AcessoNegado)?;
        let mut v = Vec::new();
        for e in it {
            let Ok(e) = e else {
                continue;
            };
            let Ok(m) = e.metadata() else {
                continue;
            };
            let a = m.file_attributes();
            v.push(Entrada {
                nome: e.file_name(),
                diretorio: a & FILE_ATTRIBUTE_DIRECTORY != 0,
                reparse: a & FILE_ATTRIBUTE_REPARSE_POINT != 0,
                somente_leitura: a & FILE_ATTRIBUTE_READONLY != 0,
                sistema: a & FILE_ATTRIBUTE_SYSTEM != 0,
                criacao_unix: filetime_unix(m.creation_time()),
                escrita_unix: filetime_unix(m.last_write_time()),
                tamanho: m.file_size(),
            });
        }
        Ok(v)
    }
}

/// Exclusão real (R-L-03): abre com `DELETE`, compartilhamento 0,
/// `FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS`; reconfere os atributos
/// pelo handle; marca a exclusão sem `POSIX_SEMANTICS` nem `IGNORE_READONLY`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ExcluidorReal;

impl Excluidor for ExcluidorReal {
    fn excluir(&mut self, caminho: &Path, entrada: &Entrada) -> Result<(), MotivoPulo> {
        let nome = largo(caminho.as_os_str());
        // SAFETY: nome terminado em zero; demais ponteiros nulos são aceitos.
        let h = unsafe {
            CreateFileW(
                nome.as_ptr(),
                DELETE | FILE_READ_ATTRIBUTES,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            // SAFETY: sem pré-condições.
            let e = unsafe { GetLastError() };
            return Err(if e == ERROR_SHARING_VIOLATION {
                MotivoPulo::EmUso
            } else {
                MotivoPulo::AcessoNegado
            });
        }
        // SAFETY: handle válido recém-aberto por CreateFileW; fechado uma vez no drop.
        let h = unsafe { OwnedHandle::from_raw_handle(h) };

        // SAFETY: FILE_BASIC_INFO é POD; zero é válido.
        let mut info: FILE_BASIC_INFO = unsafe { std::mem::zeroed() };
        // SAFETY: handle válido; buffer do tamanho informado.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                h.as_raw_handle(),
                fsys::FileBasicInfo,
                (&raw mut info).cast(),
                size_of::<FILE_BASIC_INFO>() as u32,
            )
        };
        if ok == 0 {
            return Err(MotivoPulo::AcessoNegado);
        }
        let a = info.FileAttributes;
        if a & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(MotivoPulo::ReparsePoint);
        }
        if a & FILE_ATTRIBUTE_SYSTEM != 0 {
            return Err(MotivoPulo::Sistema);
        }
        if a & FILE_ATTRIBUTE_READONLY != 0 {
            return Err(MotivoPulo::SomenteLeitura);
        }
        if (a & FILE_ATTRIBUTE_DIRECTORY != 0) != entrada.diretorio {
            return Err(MotivoPulo::AcessoNegado);
        }
        // Diretório muda de data ao perder filhos; só arquivo é reconferido.
        if !entrada.diretorio && filetime_unix(info.LastWriteTime as u64) > entrada.escrita_unix {
            return Err(MotivoPulo::Recente);
        }

        let disp = FILE_DISPOSITION_INFO_EX {
            Flags: FILE_DISPOSITION_FLAG_DELETE,
        };
        // SAFETY: handle aberto com DELETE; estrutura do tamanho informado.
        let ok = unsafe {
            SetFileInformationByHandle(
                h.as_raw_handle(),
                fsys::FileDispositionInfoEx,
                (&raw const disp).cast(),
                size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
            )
        };
        if ok == 0 {
            return Err(MotivoPulo::AcessoNegado);
        }
        Ok(())
    }
}

/// Instalação MSI em andamento: o mutex `Global\_MSIExecute` existe (`OpenMutexW`).
#[derive(Debug, Clone)]
pub struct MutexMsi {
    nome: Vec<u16>,
}

impl MutexMsi {
    pub const NOME: &'static str = r"Global\_MSIExecute";

    pub fn padrao() -> Self {
        Self::com_nome(Self::NOME)
    }

    /// Nome alternativo (testes sem privilégio para criar objetos `Global\`).
    pub fn com_nome(nome: &str) -> Self {
        Self {
            nome: largo(OsStr::new(nome)),
        }
    }
}

impl Instalacao for MutexMsi {
    fn em_andamento(&self) -> bool {
        // SAFETY: nome terminado em zero.
        let h = unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, 0, self.nome.as_ptr()) };
        if h.is_null() {
            // SAFETY: sem pré-condições.
            return unsafe { GetLastError() } == ERROR_ACCESS_DENIED;
        }
        // SAFETY: handle válido aberto por OpenMutexW; fechado uma vez no drop.
        drop(unsafe { OwnedHandle::from_raw_handle(h) });
        true
    }
}
