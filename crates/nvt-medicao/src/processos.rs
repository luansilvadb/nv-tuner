//! Snapshot de processos por `TH32CS_SNAPPROCESS` (R-M-02): só PID e nome do arquivo.
//! Nenhum handle para os processos listados.

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};

use crate::fachada::{FonteProcessos, ProcessoVisto};

#[derive(Debug, Default)]
pub struct SnapshotProcessos;

impl FonteProcessos for SnapshotProcessos {
    fn snapshot(&mut self) -> Result<Vec<ProcessoVisto>, String> {
        // SAFETY: snapshot só de processos; o handle é do snapshot, não de um processo.
        let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snap == INVALID_HANDLE_VALUE {
            return Err("snapshot de processos falhou".into());
        }
        let mut lista = Vec::new();
        let mut entrada = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        // SAFETY: snapshot válido e estrutura com `dwSize` preenchido.
        let mut ok = unsafe { Process32FirstW(snap, &mut entrada) } != 0;
        while ok {
            let fim = entrada
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(260);
            lista.push(ProcessoVisto {
                pid: entrada.th32ProcessID,
                executavel: String::from_utf16_lossy(&entrada.szExeFile[..fim]),
            });
            // SAFETY: idem.
            ok = unsafe { Process32NextW(snap, &mut entrada) } != 0;
        }
        // SAFETY: handle do snapshot aberto acima.
        unsafe { CloseHandle(snap) };
        Ok(lista)
    }
}
