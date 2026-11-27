//! Carregamento da `nvapi64.dll`, `NvAPI_Initialize`/`NvAPI_Unload` (RAII) e consultas de
//! sistema/GPU.

use std::ffi::c_void;
use std::fmt;

use crate::error::{Error, Result, Status};
use crate::ffi::{self, Handle, NvStatus, id};
use crate::strings::from_short;

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn LoadLibraryW(name: *const u16) -> *mut c_void;
        pub fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
        pub fn FreeLibrary(module: *mut c_void) -> i32;
    }
}

type QueryInterface = unsafe extern "C" fn(u32) -> *mut c_void;

/// Biblioteca NVAPI carregada e inicializada. `Drop` chama `NvAPI_Unload` e libera a DLL.
///
/// Não é `Send`/`Sync`: use a partir de uma única thread (o serviço serializa o acesso).
pub struct NvApi {
    dll: *mut c_void,
    qi: QueryInterface,
}

/// Origem de uma entrada resolvida por *interno com fallback para público*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOrigin {
    /// ID interno (não documentado, sem o filtro de settings ocultas).
    Internal,
    /// ID público do SDK (settings ocultas dão -160).
    Public,
}

impl NvApi {
    /// Carrega `nvapi64.dll`, obtém `nvapi_QueryInterface` e chama `NvAPI_Initialize`.
    #[cfg(windows)]
    pub fn load() -> Result<Self> {
        let nome: Vec<u16> = "nvapi64.dll\0".encode_utf16().collect();
        // SAFETY: string UTF-16 terminada em zero; o módulo é liberado em `Drop`.
        let dll = unsafe { sys::LoadLibraryW(nome.as_ptr()) };
        if dll.is_null() {
            return Err(Error::DllNotFound);
        }
        // SAFETY: `dll` é válido; nome ANSI terminado em zero.
        let p = unsafe { sys::GetProcAddress(dll, c"nvapi_QueryInterface".as_ptr().cast()) };
        if p.is_null() {
            // SAFETY: `dll` veio de `LoadLibraryW` e não será mais usado.
            unsafe { sys::FreeLibrary(dll) };
            return Err(Error::QueryInterfaceNotFound);
        }
        // SAFETY: a exportação tem a assinatura `void* __cdecl nvapi_QueryInterface(unsigned)`.
        let qi = unsafe { std::mem::transmute::<*mut c_void, QueryInterface>(p) };
        let api = Self { dll, qi };
        let init: ffi::FnVoid = api.resolve(id::INITIALIZE, "NvAPI_Initialize")?;
        // SAFETY: função sem argumentos.
        let st = unsafe { init() };
        if st != ffi::NVAPI_OK {
            // `Drop` chama Unload (inócuo sem Initialize) e FreeLibrary.
            return Err(api.error("NvAPI_Initialize", st));
        }
        Ok(api)
    }

    /// Fora do Windows não há NVAPI.
    #[cfg(not(windows))]
    pub fn load() -> Result<Self> {
        Err(Error::DllNotFound)
    }

    /// Ponteiro bruto da função `id`, ou `None` se o driver não a tiver.
    pub(crate) fn ptr(&self, id: u32) -> Option<*mut c_void> {
        // SAFETY: `qi` é a exportação da DLL carregada, válida enquanto `self` viver.
        let p = unsafe { (self.qi)(id) };
        (!p.is_null()).then_some(p)
    }

    /// Resolve `id` como o ponteiro de função `T`, sem verificar a assinatura.
    ///
    /// Quem chama garante que `T` é o `unsafe extern "C" fn` correto para `id` (tabela em
    /// `ffi`). O tamanho é conferido em tempo de compilação.
    pub(crate) fn get<T: Copy>(&self, id: u32) -> Option<T> {
        const { assert!(std::mem::size_of::<T>() == std::mem::size_of::<*mut c_void>()) };
        // SAFETY: `T` é um ponteiro de função do tamanho de um ponteiro (conferido acima).
        self.ptr(id)
            .map(|p| unsafe { std::mem::transmute_copy::<*mut c_void, T>(&p) })
    }

    pub(crate) fn resolve<T: Copy>(&self, id: u32, function: &'static str) -> Result<T> {
        self.get(id).ok_or(Error::FunctionUnavailable { function })
    }

    /// Resolve pela entrada interna e, se ausente, pela pública (mesma assinatura).
    pub(crate) fn resolve_fallback<T: Copy>(
        &self,
        interno: u32,
        publico: u32,
        function: &'static str,
    ) -> Result<(T, EntryOrigin)> {
        if let Some(f) = self.get(interno) {
            return Ok((f, EntryOrigin::Internal));
        }
        self.resolve(publico, function)
            .map(|f| (f, EntryOrigin::Public))
    }

    /// Mensagem de `NvAPI_GetErrorMessage` para `st`, ou o nome da constante.
    pub fn error_message(&self, st: NvStatus) -> String {
        let mut msg: ffi::NvShortString = [0; ffi::NVAPI_SHORT_STRING_MAX];
        if let Some(f) = self.get::<ffi::FnGetErrorMessage>(id::GET_ERROR_MESSAGE) {
            // SAFETY: buffer de 64 bytes conforme `NvAPI_ShortString`.
            if unsafe { f(st, &mut msg) } == ffi::NVAPI_OK {
                let m = from_short(&msg);
                if !m.is_empty() {
                    return m;
                }
            }
        }
        Status::from_code(st).name().to_string()
    }

    /// Erro tipado de uma chamada que devolveu `st`.
    pub(crate) fn error(&self, function: &'static str, st: NvStatus) -> Error {
        Error::from_status(function, st, self.error_message(st))
    }

    pub(crate) fn check(&self, function: &'static str, st: NvStatus) -> Result<()> {
        if st == ffi::NVAPI_OK {
            Ok(())
        } else {
            Err(self.error(function, st))
        }
    }

    /// Versão e branch do driver (`NvAPI_SYS_GetDriverAndBranchVersion`).
    pub fn driver_version(&self) -> Result<DriverVersion> {
        const F: &str = "NvAPI_SYS_GetDriverAndBranchVersion";
        let f: ffi::FnGetDriverAndBranch =
            self.resolve(id::SYS_GET_DRIVER_AND_BRANCH_VERSION, F)?;
        let (mut v, mut br) = (0u32, [0u8; ffi::NVAPI_SHORT_STRING_MAX]);
        // SAFETY: ponteiros de saída válidos (u32 e buffer de 64 bytes).
        self.check(F, unsafe { f(&mut v, &mut br) })?;
        Ok(DriverVersion {
            raw: v,
            branch: from_short(&br),
        })
    }

    /// GPUs físicas NVIDIA (`NvAPI_EnumPhysicalGPUs` + nome, arquitetura e VRAM).
    pub fn physical_gpus(&self) -> Result<Vec<GpuInfo>> {
        const F: &str = "NvAPI_EnumPhysicalGPUs";
        let enum_gpus: ffi::FnEnumPhysicalGpus = self.resolve(id::ENUM_PHYSICAL_GPUS, F)?;
        let mut gpus: [Handle; ffi::NVAPI_MAX_PHYSICAL_GPUS] =
            [std::ptr::null_mut(); ffi::NVAPI_MAX_PHYSICAL_GPUS];
        let mut n = 0u32;
        // SAFETY: vetor de 64 handles e contador de saída.
        self.check(F, unsafe { enum_gpus(&mut gpus, &mut n) })?;
        let n = (n as usize).min(ffi::NVAPI_MAX_PHYSICAL_GPUS);
        gpus[..n]
            .iter()
            .enumerate()
            .map(|(index, &h)| self.gpu_info(index, h))
            .collect()
    }

    fn gpu_info(&self, index: usize, h: Handle) -> Result<GpuInfo> {
        const F_NAME: &str = "NvAPI_GPU_GetFullName";
        const F_ARCH: &str = "NvAPI_GPU_GetArchInfo";
        let name_f: ffi::FnGpuGetFullName = self.resolve(id::GPU_GET_FULL_NAME, F_NAME)?;
        let mut nome = [0u8; ffi::NVAPI_SHORT_STRING_MAX];
        // SAFETY: handle de GPU obtido da enumeração e buffer de 64 bytes.
        self.check(F_NAME, unsafe { name_f(h, &mut nome) })?;

        let arch = match self.get::<ffi::FnGpuGetArchInfo>(id::GPU_GET_ARCH_INFO) {
            Some(f) => {
                let mut a = ffi::NvGpuArchInfoV2 {
                    version: ffi::NvGpuArchInfoV2::VER,
                    ..Default::default()
                };
                // SAFETY: estrutura V2 com versão preenchida.
                self.check(F_ARCH, unsafe { f(h, &mut a) })?;
                Some(ArchInfo {
                    architecture: a.architecture,
                    implementation: a.implementation,
                    revision: a.revision,
                })
            }
            None => None,
        };

        let vram_kib = self
            .get::<ffi::FnGpuGetMemoryInfo>(id::GPU_GET_MEMORY_INFO)
            .and_then(|f| {
                let mut m = ffi::NvDisplayDriverMemoryInfoV2 {
                    version: ffi::NvDisplayDriverMemoryInfoV2::VER,
                    ..Default::default()
                };
                // SAFETY: estrutura V2 com versão preenchida.
                let st = unsafe { f(h, &mut m) };
                (st == ffi::NVAPI_OK && m.dedicated_video_memory > 0)
                    .then_some(m.dedicated_video_memory)
            });

        Ok(GpuInfo {
            index,
            name: from_short(&nome),
            arch,
            vram_kib,
        })
    }
}

impl Drop for NvApi {
    fn drop(&mut self) {
        if let Some(f) = self.get::<ffi::FnVoid>(id::UNLOAD) {
            // SAFETY: função sem argumentos; nenhuma sessão sobrevive a `self` (lifetimes).
            unsafe { f() };
        }
        #[cfg(windows)]
        // SAFETY: `dll` veio de `LoadLibraryW` e não é usado depois daqui.
        unsafe {
            sys::FreeLibrary(self.dll);
        }
    }
}

impl fmt::Debug for NvApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NvApi").field("dll", &self.dll).finish()
    }
}

/// Versão do driver: `raw` = 62140 para 621.40.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverVersion {
    pub raw: u32,
    pub branch: String,
}

impl DriverVersion {
    pub fn major(&self) -> u32 {
        self.raw / 100
    }

    pub fn minor(&self) -> u32 {
        self.raw % 100
    }
}

impl fmt::Display for DriverVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:02}", self.major(), self.minor())
    }
}

/// `NV_GPU_ARCH_INFO_V2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchInfo {
    /// `NV_GPU_ARCHITECTURE_ID` (ex.: `0x160` Turing).
    pub architecture: u32,
    /// `NV_GPU_ARCH_IMPLEMENTATION_ID` (ex.: `0x06` TU106 dentro de Turing).
    pub implementation: u32,
    /// `NV_GPU_CHIP_REVISION`.
    pub revision: u32,
}

impl ArchInfo {
    pub const TURING: u32 = 0x160;

    /// Nome da arquitetura (ou `None` se desconhecida).
    pub fn architecture_name(&self) -> Option<&'static str> {
        Some(match self.architecture {
            0x130 => "Pascal",
            0x140 | 0x150 => "Volta",
            0x160 => "Turing",
            0x170 => "Ampere",
            0x180 => "Hopper",
            0x190 => "Ada Lovelace",
            0x1A0 | 0x1B0 => "Blackwell",
            _ => return None,
        })
    }
}

/// GPU física.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInfo {
    /// Posição na enumeração da NVAPI.
    pub index: usize,
    /// `NvAPI_GPU_GetFullName` (ex.: “NVIDIA GeForce RTX 2060 SUPER”).
    pub name: String,
    /// `None` se o driver não tiver `NvAPI_GPU_GetArchInfo`.
    pub arch: Option<ArchInfo>,
    /// VRAM dedicada em KiB (`NvAPI_GPU_GetMemoryInfo`, `dedicatedVideoMemory`); `None` se
    /// a função estiver ausente ou falhar.
    pub vram_kib: Option<u32>,
}
