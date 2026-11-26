//! GPU via NVML (R-M-03): `nvml.dll` do driver carregada pelo caminho completo em
//! System32, uso e temperatura da GPU 0. Falha em qualquer leitura = campo ausente.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

use crate::fachada::{FonteGpu, LeituraGpu};

#[repr(C)]
#[derive(Default)]
struct Utilizacao {
    gpu: u32,
    memoria: u32,
}

type FnInit = unsafe extern "C" fn() -> i32;
type FnHandle = unsafe extern "C" fn(u32, *mut *mut c_void) -> i32;
type FnUso = unsafe extern "C" fn(*mut c_void, *mut Utilizacao) -> i32;
type FnTemp = unsafe extern "C" fn(*mut c_void, u32, *mut u32) -> i32;
type FnShutdown = unsafe extern "C" fn() -> i32;
type Simbolo = unsafe extern "system" fn() -> isize;

const NVML_SUCCESS: i32 = 0;
const NVML_TEMPERATURE_GPU: u32 = 0;

/// Sessão NVML aberta; `Drop` faz `nvmlShutdown` e libera a DLL.
pub struct GpuNvml {
    lib: HMODULE,
    dispositivo: *mut c_void,
    uso: FnUso,
    temp: FnTemp,
    shutdown: FnShutdown,
}

// SAFETY: as funções da NVML são thread-safe; o handle é usado por um dono só.
unsafe impl Send for GpuNvml {}

fn caminho_nvml() -> Option<Vec<u16>> {
    let mut buf = [0u16; 260];
    // SAFETY: buffer local com o tamanho informado.
    let n = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 || n >= buf.len() {
        return None;
    }
    let mut c: Vec<u16> = buf[..n].to_vec();
    c.extend("\nvml.dll".encode_utf16());
    c.push(0);
    Some(c)
}

impl GpuNvml {
    pub fn abrir() -> Result<Self, String> {
        let caminho = caminho_nvml().ok_or("pasta do sistema indisponível")?;
        // SAFETY: caminho absoluto terminado em zero; dependências só de System32.
        let lib = unsafe {
            LoadLibraryExW(
                caminho.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
        if lib.is_null() {
            return Err("nvml.dll ausente".into());
        }
        let simbolo = |nome: &[u8]| {
            // SAFETY: nome ASCII terminado em zero; módulo carregado.
            unsafe { GetProcAddress(lib, nome.as_ptr()) }
        };
        let (Some(init), Some(handle), Some(uso), Some(temp), Some(shutdown)) = (
            simbolo(b"nvmlInit_v2\0"),
            simbolo(b"nvmlDeviceGetHandleByIndex_v2\0"),
            simbolo(b"nvmlDeviceGetUtilizationRates\0"),
            simbolo(b"nvmlDeviceGetTemperature\0"),
            simbolo(b"nvmlShutdown\0"),
        ) else {
            // SAFETY: módulo carregado acima.
            unsafe { FreeLibrary(lib) };
            return Err("nvml.dll sem as funções esperadas".into());
        };
        // SAFETY: assinaturas conforme nvml.h.
        let (init, handle, uso, temp, shutdown): (FnInit, FnHandle, FnUso, FnTemp, FnShutdown) = unsafe {
            (
                std::mem::transmute::<Simbolo, FnInit>(init),
                std::mem::transmute::<Simbolo, FnHandle>(handle),
                std::mem::transmute::<Simbolo, FnUso>(uso),
                std::mem::transmute::<Simbolo, FnTemp>(temp),
                std::mem::transmute::<Simbolo, FnShutdown>(shutdown),
            )
        };
        // SAFETY: funções resolvidas da DLL carregada.
        unsafe {
            if init() != NVML_SUCCESS {
                FreeLibrary(lib);
                return Err("nvmlInit falhou".into());
            }
            let mut dispositivo = std::ptr::null_mut();
            if handle(0, &mut dispositivo) != NVML_SUCCESS {
                shutdown();
                FreeLibrary(lib);
                return Err("GPU 0 indisponível na NVML".into());
            }
            Ok(Self {
                lib,
                dispositivo,
                uso,
                temp,
                shutdown,
            })
        }
    }
}

impl FonteGpu for GpuNvml {
    fn ler(&mut self) -> LeituraGpu {
        let mut u = Utilizacao::default();
        let mut t = 0u32;
        // SAFETY: dispositivo válido enquanto a sessão NVML está aberta.
        let (ru, rt) = unsafe {
            (
                (self.uso)(self.dispositivo, &mut u),
                (self.temp)(self.dispositivo, NVML_TEMPERATURE_GPU, &mut t),
            )
        };
        LeituraGpu {
            uso_pct: (ru == NVML_SUCCESS && u.gpu <= 100).then_some(u.gpu as u8),
            temp_c: (rt == NVML_SUCCESS && t < 255).then_some(t as u8),
        }
    }
}

impl Drop for GpuNvml {
    fn drop(&mut self) {
        // SAFETY: sessão aberta em `abrir`; a DLL é liberada por último.
        unsafe {
            (self.shutdown)();
            FreeLibrary(self.lib);
        }
    }
}
