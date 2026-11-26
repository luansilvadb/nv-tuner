//! Tipos, IDs de interface e assinaturas da NVAPI (subconjunto DRS, consultas de GPU e
//! exibição).
//!
//! Transcrito do SDK oficial <https://github.com/NVIDIA/nvapi> (`nvapi.h`,
//! `nvapi_lite_common.h`, `nvapi_interface.h`):
//! Copyright (c) 2019-2026 NVIDIA CORPORATION & AFFILIATES. SPDX-License-Identifier: MIT.
//!
//! Os IDs internos (não documentados) vêm do NVIDIA Profile Inspector 3.0.2.1
//! (`NvapiDrsWrapper.cs`), Copyright (c) 2016 Orbmu2k, licença MIT.
//!
//! As asserções `const` no fim do arquivo conferem tamanhos e versões em tempo de compilação.

use std::ffi::c_void;

/// `NvAPI_Status` (0 = sucesso, negativo = erro).
pub type NvStatus = i32;
/// Handle opaco (`NvDRSSessionHandle`, `NvDRSProfileHandle`, `NvPhysicalGpuHandle`).
pub type Handle = *mut c_void;

pub const NVAPI_UNICODE_STRING_MAX: usize = 2048;
pub const NVAPI_BINARY_DATA_MAX: usize = 4096;
pub const NVAPI_SHORT_STRING_MAX: usize = 64;
pub const NVAPI_MAX_PHYSICAL_GPUS: usize = 64;
pub const NVAPI_SETTING_MAX_VALUES: usize = 100;

/// `NvAPI_UnicodeString`: `NvU16[2048]` (UTF-16, terminado em zero).
pub type NvUnicodeString = [u16; NVAPI_UNICODE_STRING_MAX];
/// `NvAPI_ShortString`: `char[64]` (ANSI, terminado em zero).
pub type NvShortString = [u8; NVAPI_SHORT_STRING_MAX];

/// `MAKE_NVAPI_VERSION(typeName, ver) = sizeof(typeName) | (ver << 16)`.
pub const fn make_version<T>(ver: u32) -> u32 {
    (std::mem::size_of::<T>() as u32) | (ver << 16)
}

// ---------------------------------------------------------------- status

pub const NVAPI_OK: NvStatus = 0;
pub const NVAPI_NO_IMPLEMENTATION: NvStatus = -3;
pub const NVAPI_END_ENUMERATION: NvStatus = -7;
pub const NVAPI_INCOMPATIBLE_STRUCT_VERSION: NvStatus = -9;
pub const NVAPI_INVALID_USER_PRIVILEGE: NvStatus = -137;
pub const NVAPI_SETTING_NOT_FOUND: NvStatus = -160;
pub const NVAPI_PROFILE_NOT_FOUND: NvStatus = -163;
pub const NVAPI_EXECUTABLE_NOT_FOUND: NvStatus = -166;

// ---------------------------------------------------------------- IDs (nvapi_interface.h)

/// IDs públicos de `nvapi_interface.h`.
pub mod id {
    pub const INITIALIZE: u32 = 0x0150_E828;
    pub const UNLOAD: u32 = 0xD22B_DD7E;
    pub const GET_ERROR_MESSAGE: u32 = 0x6C2D_048C;
    pub const SYS_GET_DRIVER_AND_BRANCH_VERSION: u32 = 0x2926_AAAD;
    pub const ENUM_PHYSICAL_GPUS: u32 = 0xE5AC_921F;
    pub const GPU_GET_FULL_NAME: u32 = 0xCEEE_8E9F;
    pub const GPU_GET_ARCH_INFO: u32 = 0xD826_5D24;
    pub const GPU_GET_MEMORY_INFO: u32 = 0x07F9_B368;
    pub const DRS_CREATE_SESSION: u32 = 0x0694_D52E;
    pub const DRS_DESTROY_SESSION: u32 = 0xDAD9_CFF8;
    pub const DRS_LOAD_SETTINGS: u32 = 0x375D_BD6B;
    pub const DRS_SAVE_SETTINGS: u32 = 0xFCBC_7E14;
    pub const DRS_GET_BASE_PROFILE: u32 = 0xDA84_66A0;
    pub const DRS_GET_CURRENT_GLOBAL_PROFILE: u32 = 0x617B_FF9F;
    pub const DRS_FIND_PROFILE_BY_NAME: u32 = 0x7E4A_9A0B;
    pub const DRS_FIND_APPLICATION_BY_NAME: u32 = 0xEEE5_66B2;
    pub const DRS_ENUM_PROFILES: u32 = 0xBC37_1EE0;
    pub const DRS_GET_PROFILE_INFO: u32 = 0x61CD_6FD6;
    pub const DRS_ENUM_APPLICATIONS: u32 = 0x7FA2_173A;
    pub const DRS_CREATE_PROFILE: u32 = 0xCC17_6068;
    pub const DRS_CREATE_APPLICATION: u32 = 0x4347_A9DE;
    pub const DRS_DELETE_PROFILE: u32 = 0x1709_3206;
    pub const DRS_DELETE_APPLICATION: u32 = 0x2C69_4BC6;
    pub const DRS_GET_SETTING: u32 = 0x73BF_8338;
    pub const DRS_SET_SETTING: u32 = 0x577D_D202;
    pub const DRS_DELETE_PROFILE_SETTING: u32 = 0xE4A2_6362;
    pub const DRS_RESTORE_PROFILE_DEFAULT_SETTING: u32 = 0x53F0_381E;
    pub const DRS_ENUM_SETTINGS: u32 = 0xAE30_39DA;
    pub const DRS_GET_SETTING_NAME_FROM_ID: u32 = 0xD61C_BE6E;
    pub const DISP_GET_DISPLAY_CONFIG: u32 = 0x11AB_CCF8;
    pub const DISP_GET_VRR_INFO: u32 = 0xDF8F_DA57;

    /// Entradas internas (não documentadas) usadas pelo NPI 3.0.2.1. Não filtram as
    /// settings “ocultas” que as públicas recusam com -160.
    pub mod interno {
        /// Público + 5º parâmetro `*mut u32` (o NPI passa 0).
        pub const DRS_GET_SETTING: u32 = 0xEA99_498D;
        /// Público + dois `u32` extras no fim (o NPI passa 0, 0).
        pub const DRS_SET_SETTING: u32 = 0x8A2C_F5F5;
        pub const DRS_ENUM_SETTINGS: u32 = 0xCFD6_983E;
        pub const DRS_GET_SETTING_NAME_FROM_ID: u32 = 0x1EB1_3791;
        pub const DRS_DELETE_PROFILE_SETTING: u32 = 0xD20D_29DF;
        pub const DRS_RESTORE_PROFILE_DEFAULT_SETTING: u32 = 0x7DD5_B261;
    }
}

// ---------------------------------------------------------------- estruturas

pub const NVDRS_DWORD_TYPE: u32 = 0;
pub const NVDRS_BINARY_TYPE: u32 = 1;
pub const NVDRS_STRING_TYPE: u32 = 2;
pub const NVDRS_WSTRING_TYPE: u32 = 3;
pub const NVDRS_QWORD_TYPE: u32 = 4;
pub const NVDRS_FLOAT_TYPE: u32 = 5;

/// União de valor de `NVDRS_SETTING_V1` sob `#pragma pack(push, 4)`: o maior membro é
/// `NVDRS_BINARY_SETTING` (4 + 4096 = 4100 bytes) e o `NvU64` fica com alinhamento 4.
/// Uma `union` Rust com `u64` alinharia a 8 e mudaria o tamanho; por isso é armazenamento
/// opaco de 1025 × `u32` com acessores.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NvdrsValue {
    pub raw: [u32; 1025],
}

impl NvdrsValue {
    pub const ZERO: Self = Self { raw: [0; 1025] };

    pub fn u32(&self) -> u32 {
        self.raw[0]
    }

    pub fn u64(&self) -> u64 {
        u64::from(self.raw[0]) | (u64::from(self.raw[1]) << 32)
    }

    pub fn f32(&self) -> f32 {
        f32::from_bits(self.raw[0])
    }

    /// Binário: `valueLength` (u32) seguido de até 4096 bytes.
    pub fn binary(&self) -> Vec<u8> {
        let len = (self.raw[0] as usize).min(NVAPI_BINARY_DATA_MAX);
        self.raw[1..]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .take(len)
            .collect()
    }

    /// Unicode: os primeiros 4096 bytes como `NvAPI_UnicodeString`.
    pub fn wstr(&self) -> String {
        let w: Vec<u16> = self.raw[..NVAPI_UNICODE_STRING_MAX / 2]
            .iter()
            .flat_map(|&x| [(x & 0xFFFF) as u16, (x >> 16) as u16])
            .collect();
        crate::strings::from_wide(&w)
    }

    pub fn set_u32(&mut self, v: u32) {
        self.raw = [0; 1025];
        self.raw[0] = v;
    }

    pub fn set_u64(&mut self, v: u64) {
        self.raw = [0; 1025];
        self.raw[0] = v as u32;
        self.raw[1] = (v >> 32) as u32;
    }

    /// Grava binário; bytes além de 4096 são recusados por quem chama.
    pub fn set_binary(&mut self, data: &[u8]) {
        self.raw = [0; 1025];
        let n = data.len().min(NVAPI_BINARY_DATA_MAX);
        self.raw[0] = n as u32;
        for (i, chunk) in data[..n].chunks(4).enumerate() {
            let mut b = [0u8; 4];
            b[..chunk.len()].copy_from_slice(chunk);
            self.raw[1 + i] = u32::from_le_bytes(b);
        }
    }

    /// Grava UTF-16 terminado em zero (já validado por quem chama).
    pub fn set_wide(&mut self, w: &NvUnicodeString) {
        self.raw = [0; 1025];
        for (i, par) in w.chunks(2).enumerate() {
            self.raw[i] = u32::from(par[0]) | (u32::from(par[1]) << 16);
        }
    }
}

/// `NVDRS_SETTING_V1` — 12320 bytes, versão `0x0001_3020`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NvdrsSettingV1 {
    pub version: u32,
    pub setting_name: NvUnicodeString,
    pub setting_id: u32,
    pub setting_type: u32,
    pub setting_location: u32,
    pub is_current_predefined: u32,
    pub is_predefined_valid: u32,
    pub predefined_value: NvdrsValue,
    pub current_value: NvdrsValue,
}

impl NvdrsSettingV1 {
    pub const VER: u32 = make_version::<Self>(1);
    pub const ZERO: Self = Self {
        version: Self::VER,
        setting_name: [0; NVAPI_UNICODE_STRING_MAX],
        setting_id: 0,
        setting_type: 0,
        setting_location: 0,
        is_current_predefined: 0,
        is_predefined_valid: 0,
        predefined_value: NvdrsValue::ZERO,
        current_value: NvdrsValue::ZERO,
    };

    /// Estrutura zerada no heap com `version` preenchido.
    pub fn boxed() -> Box<Self> {
        Box::new(Self::ZERO)
    }
}

/// `NVDRS_PROFILE_V1` — 4116 bytes, versão `0x0001_1014`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NvdrsProfileV1 {
    pub version: u32,
    pub profile_name: NvUnicodeString,
    /// Campo de bits `NVDRS_GPU_SUPPORT`: bit0 geforce, bit1 quadro, bit2 nvs (somente leitura).
    pub gpu_support: u32,
    pub is_predefined: u32,
    pub num_of_apps: u32,
    pub num_of_settings: u32,
}

impl NvdrsProfileV1 {
    pub const VER: u32 = make_version::<Self>(1);
    pub const ZERO: Self = Self {
        version: Self::VER,
        profile_name: [0; NVAPI_UNICODE_STRING_MAX],
        gpu_support: 0,
        is_predefined: 0,
        num_of_apps: 0,
        num_of_settings: 0,
    };
}

/// `NVDRS_APPLICATION_V4` — 20492 bytes, versão `0x0004_500C`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NvdrsApplicationV4 {
    pub version: u32,
    pub is_predefined: u32,
    pub app_name: NvUnicodeString,
    pub user_friendly_name: NvUnicodeString,
    pub launcher: NvUnicodeString,
    pub file_in_folder: NvUnicodeString,
    /// Campo de bits: bit0 isMetro, bit1 isCommandLine, bits 2..31 reservados (0).
    pub flags: u32,
    pub command_line: NvUnicodeString,
}

impl NvdrsApplicationV4 {
    pub const VER: u32 = make_version::<Self>(4);
    pub const ZERO: Self = Self {
        version: Self::VER,
        is_predefined: 0,
        app_name: [0; NVAPI_UNICODE_STRING_MAX],
        user_friendly_name: [0; NVAPI_UNICODE_STRING_MAX],
        launcher: [0; NVAPI_UNICODE_STRING_MAX],
        file_in_folder: [0; NVAPI_UNICODE_STRING_MAX],
        flags: 0,
        command_line: [0; NVAPI_UNICODE_STRING_MAX],
    };

    pub fn boxed() -> Box<Self> {
        Box::new(Self::ZERO)
    }
}

/// `NV_GPU_ARCH_INFO_V2` — 16 bytes, versão `0x0002_0010`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NvGpuArchInfoV2 {
    pub version: u32,
    pub architecture: u32,
    pub implementation: u32,
    pub revision: u32,
}

impl NvGpuArchInfoV2 {
    pub const VER: u32 = make_version::<Self>(2);
}

/// `NV_DISPLAY_DRIVER_MEMORY_INFO_V2` — 24 bytes, versão `0x0002_0018`. Valores em KiB.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NvDisplayDriverMemoryInfoV2 {
    pub version: u32,
    pub dedicated_video_memory: u32,
    pub available_dedicated_video_memory: u32,
    pub system_video_memory: u32,
    pub shared_system_memory: u32,
    pub cur_available_dedicated_video_memory: u32,
}

impl NvDisplayDriverMemoryInfoV2 {
    pub const VER: u32 = make_version::<Self>(2);
}

// Conferência dos layouts contra o SDK.
const _: () = assert!(std::mem::size_of::<NvdrsValue>() == 4100);
const _: () = assert!(std::mem::align_of::<NvdrsValue>() == 4);
const _: () = assert!(std::mem::size_of::<NvdrsSettingV1>() == 12320);
const _: () = assert!(std::mem::offset_of!(NvdrsSettingV1, setting_id) == 4100);
const _: () = assert!(std::mem::offset_of!(NvdrsSettingV1, predefined_value) == 4120);
const _: () = assert!(std::mem::offset_of!(NvdrsSettingV1, current_value) == 8220);
const _: () = assert!(NvdrsSettingV1::VER == 0x0001_3020);
const _: () = assert!(std::mem::size_of::<NvdrsProfileV1>() == 4116);
const _: () = assert!(NvdrsProfileV1::VER == 0x0001_1014);
const _: () = assert!(std::mem::size_of::<NvdrsApplicationV4>() == 20492);
const _: () = assert!(std::mem::offset_of!(NvdrsApplicationV4, flags) == 16392);
const _: () = assert!(NvdrsApplicationV4::VER == 0x0004_500C);
const _: () = assert!(std::mem::size_of::<NvGpuArchInfoV2>() == 16);
const _: () = assert!(NvGpuArchInfoV2::VER == 0x0002_0010);
const _: () = assert!(std::mem::size_of::<NvDisplayDriverMemoryInfoV2>() == 24);
const _: () = assert!(NvDisplayDriverMemoryInfoV2::VER == 0x0002_0018);

// ---------------------------------------------------------------- exibição (nvapi.h, pack 8)
//
// `NvAPI_DISP_GetDisplayConfig` e `NvAPI_Disp_GetVRRInfo`. Campos de bits C (`NvU32 x : 1`)
// viram um `u32` com máscaras; no MSVC o primeiro campo de bits ocupa o bit 0.

/// `NV_TIMINGEXT` — 64 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NvTimingExt {
    pub flag: u32,
    pub rr: u16,
    pub rrx1k: u32,
    pub aspect: u32,
    pub rep: u16,
    pub status: u32,
    pub name: [u8; 40],
}

/// `NV_TIMING` — 96 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NvTiming {
    pub h_visible: u16,
    pub h_border: u16,
    pub h_front_porch: u16,
    pub h_sync_width: u16,
    pub h_total: u16,
    pub h_sync_pol: u8,
    pub v_visible: u16,
    pub v_border: u16,
    pub v_front_porch: u16,
    pub v_sync_width: u16,
    pub v_total: u16,
    pub v_sync_pol: u8,
    pub interlaced: u16,
    pub pclk: u32,
    pub etc: NvTimingExt,
}

/// `NV_DISPLAYCONFIG_PATH_ADVANCED_TARGET_INFO_V1` — 128 bytes, versão `0x0001_0080`.
/// Os enums C (`NV_ROTATE`, `NV_SCALING`, `NV_GPU_CONNECTOR_TYPE`, `NV_DISPLAY_TV_FORMAT`,
/// `NV_TIMING_OVERRIDE`) têm 4 bytes e ficam como `u32`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NvDisplayConfigPathAdvancedTargetInfoV1 {
    pub version: u32,
    pub rotation: u32,
    pub scaling: u32,
    /// Refresh do modo (não entrelaçado) × 1000, como o driver informa ao SO; 0 = ignorado.
    pub refresh_rate_1k: u32,
    /// Bits: 0 `interlaced`, 1 `primary` (primário do clone, **não** o primário GDI), …
    pub flags: u32,
    pub connector: u32,
    pub tv_format: u32,
    pub timing_override: u32,
    pub timing: NvTiming,
}

impl NvDisplayConfigPathAdvancedTargetInfoV1 {
    pub const VER: u32 = make_version::<Self>(1);
    pub const FLAG_PRIMARY: u32 = 1 << 1;

    /// Estrutura zerada com `version` preenchido.
    pub fn zeroed() -> Self {
        // SAFETY: só inteiros e vetores de bytes; o padrão todo-zero é válido.
        let mut s: Self = unsafe { std::mem::zeroed() };
        s.version = Self::VER;
        s
    }
}

/// `NV_DISPLAYCONFIG_PATH_TARGET_INFO_V2` — 24 bytes (x64). Sem versão própria.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NvDisplayConfigPathTargetInfoV2 {
    pub display_id: u32,
    /// Pode ser nulo; nulo também para monitor fora da NVIDIA.
    pub details: *mut NvDisplayConfigPathAdvancedTargetInfoV1,
    pub target_id: u32,
}

/// `NV_DISPLAYCONFIG_SOURCE_MODE_INFO_V1` — 32 bytes.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NvDisplayConfigSourceModeInfoV1 {
    /// `NV_RESOLUTION`: largura, altura, profundidade de cor.
    pub resolution: [u32; 3],
    pub color_format: u32,
    /// `NV_POSITION`: x, y.
    pub position: [i32; 2],
    pub spanning_orientation: u32,
    /// Bits: 0 `bGDIPrimary`, 1 `bSLIFocus`.
    pub flags: u32,
}

impl NvDisplayConfigSourceModeInfoV1 {
    pub const FLAG_GDI_PRIMARY: u32 = 1 << 0;
}

/// `NV_DISPLAYCONFIG_PATH_INFO_V2` — 48 bytes (x64), versão `0x0002_0030`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NvDisplayConfigPathInfoV2 {
    pub version: u32,
    /// União `sourceId` / `reserved_sourceId`.
    pub source_id: u32,
    pub target_info_count: u32,
    pub target_info: *mut NvDisplayConfigPathTargetInfoV2,
    pub source_mode_info: *mut NvDisplayConfigSourceModeInfoV1,
    /// Bits: 0 `IsNonNVIDIAAdapter`.
    pub flags: u32,
    pub os_adapter_id: *mut c_void,
}

impl NvDisplayConfigPathInfoV2 {
    pub const VER: u32 = make_version::<Self>(2);
    pub const FLAG_NON_NVIDIA_ADAPTER: u32 = 1 << 0;

    /// Estrutura com ponteiros nulos e `version` preenchido.
    pub fn zeroed() -> Self {
        Self {
            version: Self::VER,
            source_id: 0,
            target_info_count: 0,
            target_info: std::ptr::null_mut(),
            source_mode_info: std::ptr::null_mut(),
            flags: 0,
            os_adapter_id: std::ptr::null_mut(),
        }
    }
}

/// `NV_GET_VRR_INFO_V1` — 24 bytes, versão `0x0001_0018`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NvGetVrrInfoV1 {
    pub version: u32,
    /// Bits: 0 `bIsVRREnabled`, 1 `bIsVRRPossible`, 2 `bIsVRRRequested`,
    /// 3 `bIsVRRIndicatorEnabled`, 4 `bIsDisplayInVRRMode`.
    pub flags: u32,
    pub reserved_ex: [u32; 4],
}

impl NvGetVrrInfoV1 {
    pub const VER: u32 = make_version::<Self>(1);
    pub const FLAG_ENABLED: u32 = 1 << 0;
    pub const FLAG_POSSIBLE: u32 = 1 << 1;
}

// Conferência dos layouts de exibição contra o SDK (x64).
#[cfg(target_pointer_width = "64")]
const _: () = {
    use std::mem::{offset_of, size_of};
    assert!(size_of::<NvTimingExt>() == 64);
    assert!(size_of::<NvTiming>() == 96);
    assert!(offset_of!(NvTiming, pclk) == 28);
    assert!(size_of::<NvDisplayConfigPathAdvancedTargetInfoV1>() == 128);
    assert!(offset_of!(NvDisplayConfigPathAdvancedTargetInfoV1, refresh_rate_1k) == 12);
    assert!(NvDisplayConfigPathAdvancedTargetInfoV1::VER == 0x0001_0080);
    assert!(size_of::<NvDisplayConfigPathTargetInfoV2>() == 24);
    assert!(size_of::<NvDisplayConfigSourceModeInfoV1>() == 32);
    assert!(offset_of!(NvDisplayConfigSourceModeInfoV1, flags) == 28);
    assert!(size_of::<NvDisplayConfigPathInfoV2>() == 48);
    assert!(offset_of!(NvDisplayConfigPathInfoV2, target_info) == 16);
    assert!(offset_of!(NvDisplayConfigPathInfoV2, flags) == 32);
    assert!(NvDisplayConfigPathInfoV2::VER == 0x0002_0030);
    assert!(size_of::<NvGetVrrInfoV1>() == 24);
    assert!(NvGetVrrInfoV1::VER == 0x0001_0018);
};

// ---------------------------------------------------------------- assinaturas (cdecl = "C" no x64)

pub type FnVoid = unsafe extern "C" fn() -> NvStatus;
pub type FnGetErrorMessage = unsafe extern "C" fn(NvStatus, *mut NvShortString) -> NvStatus;
pub type FnGetDriverAndBranch = unsafe extern "C" fn(*mut u32, *mut NvShortString) -> NvStatus;
pub type FnEnumPhysicalGpus =
    unsafe extern "C" fn(*mut [Handle; NVAPI_MAX_PHYSICAL_GPUS], *mut u32) -> NvStatus;
pub type FnGpuGetFullName = unsafe extern "C" fn(Handle, *mut NvShortString) -> NvStatus;
pub type FnGpuGetArchInfo = unsafe extern "C" fn(Handle, *mut NvGpuArchInfoV2) -> NvStatus;
pub type FnGpuGetMemoryInfo =
    unsafe extern "C" fn(Handle, *mut NvDisplayDriverMemoryInfoV2) -> NvStatus;
pub type FnCreateSession = unsafe extern "C" fn(*mut Handle) -> NvStatus;
pub type FnSession = unsafe extern "C" fn(Handle) -> NvStatus;
pub type FnGetProfile = unsafe extern "C" fn(Handle, *mut Handle) -> NvStatus;
pub type FnFindProfileByName =
    unsafe extern "C" fn(Handle, *const NvUnicodeString, *mut Handle) -> NvStatus;
pub type FnFindApplicationByName = unsafe extern "C" fn(
    Handle,
    *const NvUnicodeString,
    *mut Handle,
    *mut NvdrsApplicationV4,
) -> NvStatus;
pub type FnEnumProfiles = unsafe extern "C" fn(Handle, u32, *mut Handle) -> NvStatus;
pub type FnGetProfileInfo = unsafe extern "C" fn(Handle, Handle, *mut NvdrsProfileV1) -> NvStatus;
pub type FnEnumApplications =
    unsafe extern "C" fn(Handle, Handle, u32, *mut u32, *mut NvdrsApplicationV4) -> NvStatus;
pub type FnCreateProfile =
    unsafe extern "C" fn(Handle, *mut NvdrsProfileV1, *mut Handle) -> NvStatus;
pub type FnCreateApplication =
    unsafe extern "C" fn(Handle, Handle, *mut NvdrsApplicationV4) -> NvStatus;
pub type FnDeleteProfile = unsafe extern "C" fn(Handle, Handle) -> NvStatus;
pub type FnDeleteApplication =
    unsafe extern "C" fn(Handle, Handle, *const NvUnicodeString) -> NvStatus;
pub type FnGetSetting = unsafe extern "C" fn(Handle, Handle, u32, *mut NvdrsSettingV1) -> NvStatus;
pub type FnGetSettingInterno =
    unsafe extern "C" fn(Handle, Handle, u32, *mut NvdrsSettingV1, *mut u32) -> NvStatus;
pub type FnSetSetting = unsafe extern "C" fn(Handle, Handle, *mut NvdrsSettingV1) -> NvStatus;
pub type FnSetSettingInterno =
    unsafe extern "C" fn(Handle, Handle, *mut NvdrsSettingV1, u32, u32) -> NvStatus;
pub type FnProfileSettingId = unsafe extern "C" fn(Handle, Handle, u32) -> NvStatus;
pub type FnEnumSettings =
    unsafe extern "C" fn(Handle, Handle, u32, *mut u32, *mut NvdrsSettingV1) -> NvStatus;
pub type FnGetSettingNameFromId = unsafe extern "C" fn(u32, *mut NvUnicodeString) -> NvStatus;
pub type FnDispGetDisplayConfig =
    unsafe extern "C" fn(*mut u32, *mut NvDisplayConfigPathInfoV2) -> NvStatus;
pub type FnDispGetVrrInfo = unsafe extern "C" fn(u32, *mut NvGetVrrInfoV1) -> NvStatus;
