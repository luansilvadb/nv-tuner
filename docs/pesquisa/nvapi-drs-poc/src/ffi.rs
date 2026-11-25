//! Tipos e IDs da NVAPI (subconjunto DRS) transcritos do SDK oficial
//! github.com/NVIDIA/nvapi (MIT): nvapi.h, nvapi_lite_common.h, nvapi_interface.h.
//! As asserções de tamanho no fim do arquivo conferem os layouts em tempo de compilação.

#![allow(dead_code)]

use std::ffi::c_void;

pub type NvStatus = i32;
pub type NvU32 = u32;
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
pub const NVAPI_ERROR: NvStatus = -1;
pub const NVAPI_LIBRARY_NOT_FOUND: NvStatus = -2;
pub const NVAPI_NO_IMPLEMENTATION: NvStatus = -3;
pub const NVAPI_API_NOT_INITIALIZED: NvStatus = -4;
pub const NVAPI_INVALID_ARGUMENT: NvStatus = -5;
pub const NVAPI_NVIDIA_DEVICE_NOT_FOUND: NvStatus = -6;
pub const NVAPI_END_ENUMERATION: NvStatus = -7;
pub const NVAPI_INVALID_HANDLE: NvStatus = -8;
pub const NVAPI_INCOMPATIBLE_STRUCT_VERSION: NvStatus = -9;
pub const NVAPI_INVALID_POINTER: NvStatus = -14;
pub const NVAPI_INVALID_USER_PRIVILEGE: NvStatus = -137;
pub const NVAPI_SETTING_NOT_FOUND: NvStatus = -160;
pub const NVAPI_SETTING_SIZE_TOO_LARGE: NvStatus = -161;
pub const NVAPI_TOO_MANY_SETTINGS_IN_PROFILE: NvStatus = -162;
pub const NVAPI_PROFILE_NOT_FOUND: NvStatus = -163;
pub const NVAPI_PROFILE_NAME_IN_USE: NvStatus = -164;
pub const NVAPI_PROFILE_NAME_EMPTY: NvStatus = -165;
pub const NVAPI_EXECUTABLE_NOT_FOUND: NvStatus = -166;
pub const NVAPI_EXECUTABLE_ALREADY_IN_USE: NvStatus = -167;
pub const NVAPI_DATATYPE_MISMATCH: NvStatus = -168;
pub const NVAPI_PROFILE_REMOVED: NvStatus = -169;

// ---------------------------------------------------------------- IDs públicos (nvapi_interface.h)
pub mod id {
    pub const INITIALIZE: u32 = 0x0150_E828;
    pub const UNLOAD: u32 = 0xD22B_DD7E;
    pub const GET_ERROR_MESSAGE: u32 = 0x6C2D_048C;
    pub const SYS_GET_DRIVER_AND_BRANCH_VERSION: u32 = 0x2926_AAAD;
    pub const ENUM_PHYSICAL_GPUS: u32 = 0xE5AC_921F;
    pub const GPU_GET_FULL_NAME: u32 = 0xCEEE_8E9F;
    pub const GPU_GET_ARCH_INFO: u32 = 0xD826_5D24;
    pub const DRS_CREATE_SESSION: u32 = 0x0694_D52E;
    pub const DRS_DESTROY_SESSION: u32 = 0xDAD9_CFF8;
    pub const DRS_LOAD_SETTINGS: u32 = 0x375D_BD6B;
    pub const DRS_SAVE_SETTINGS: u32 = 0xFCBC_7E14; // escrita: NÃO usado na POC
    pub const DRS_GET_BASE_PROFILE: u32 = 0xDA84_66A0;
    pub const DRS_GET_CURRENT_GLOBAL_PROFILE: u32 = 0x617B_FF9F;
    pub const DRS_FIND_PROFILE_BY_NAME: u32 = 0x7E4A_9A0B;
    pub const DRS_FIND_APPLICATION_BY_NAME: u32 = 0xEEE5_66B2;
    pub const DRS_ENUM_PROFILES: u32 = 0xBC37_1EE0;
    pub const DRS_GET_PROFILE_INFO: u32 = 0x61CD_6FD6;
    pub const DRS_ENUM_APPLICATIONS: u32 = 0x7FA2_173A;
    pub const DRS_CREATE_PROFILE: u32 = 0xCC17_6068; // escrita
    pub const DRS_CREATE_APPLICATION: u32 = 0x4347_A9DE; // escrita
    pub const DRS_GET_SETTING: u32 = 0x73BF_8338;
    pub const DRS_SET_SETTING: u32 = 0x577D_D202; // escrita
    pub const DRS_DELETE_PROFILE_SETTING: u32 = 0xE4A2_6362; // escrita
    pub const DRS_RESTORE_PROFILE_DEFAULT_SETTING: u32 = 0x53F0_381E; // escrita
    pub const DRS_ENUM_SETTINGS: u32 = 0xAE30_39DA;
    pub const DRS_GET_SETTING_NAME_FROM_ID: u32 = 0xD61C_BE6E;
    pub const DRS_ENUM_AVAILABLE_SETTING_IDS: u32 = 0xF020_614A;
    pub const DRS_GET_NUM_PROFILES: u32 = 0x1DAE_4FBC;

    /// Entradas internas (não documentadas) usadas pelo NPI 3.0.2.1
    /// (NvapiDrsWrapper.cs, `GetDelegate(interno, out f, público)`). Não filtram as settings
    /// "ocultas" que as públicas recusam com -160.
    pub mod interno {
        pub const DRS_GET_SETTING: u32 = 0xEA99_498D; // + 5º parâmetro `*mut u32` (NPI passa 0)
        pub const DRS_SET_SETTING: u32 = 0x8A2C_F5F5; // + 4º e 5º parâmetros u32 (NPI passa 0, 0)
        pub const DRS_ENUM_SETTINGS: u32 = 0xCFD6_983E;
        pub const DRS_ENUM_AVAILABLE_SETTING_IDS: u32 = 0xE5DE_48E5;
        pub const DRS_GET_SETTING_NAME_FROM_ID: u32 = 0x1EB1_3791;
        pub const DRS_DELETE_PROFILE_SETTING: u32 = 0xD20D_29DF;
        pub const DRS_RESTORE_PROFILE_DEFAULT_SETTING: u32 = 0x7DD5_B261;
    }
}

// ---------------------------------------------------------------- estruturas DRS

pub const NVDRS_DWORD_TYPE: u32 = 0;
pub const NVDRS_BINARY_TYPE: u32 = 1;
pub const NVDRS_STRING_TYPE: u32 = 2;
pub const NVDRS_WSTRING_TYPE: u32 = 3;
pub const NVDRS_QWORD_TYPE: u32 = 4;
pub const NVDRS_FLOAT_TYPE: u32 = 5;

pub const NVDRS_CURRENT_PROFILE_LOCATION: u32 = 0;
pub const NVDRS_GLOBAL_PROFILE_LOCATION: u32 = 1;
pub const NVDRS_BASE_PROFILE_LOCATION: u32 = 2;
pub const NVDRS_DEFAULT_PROFILE_LOCATION: u32 = 3;

/// União de valor de NVDRS_SETTING_V1 sob `#pragma pack(push, 4)`: o maior membro é
/// `NVDRS_BINARY_SETTING` (4 + 4096 = 4100 bytes) e o `NvU64` fica com alinhamento 4.
/// Em Rust: armazenamento opaco de 1025 × u32 (alinhamento 4) com acessores.
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
        (self.raw[0] as u64) | ((self.raw[1] as u64) << 32)
    }
    pub fn f32(&self) -> f32 {
        f32::from_bits(self.raw[0])
    }
    /// Binário: `valueLength` (u32) seguido de até 4096 bytes.
    pub fn binary(&self) -> &[u8] {
        let len = (self.raw[0] as usize).min(NVAPI_BINARY_DATA_MAX);
        // SAFETY: raw[1..] tem 4096 bytes contíguos, alinhamento de u8 é 1.
        let bytes = unsafe {
            std::slice::from_raw_parts(self.raw[1..].as_ptr() as *const u8, NVAPI_BINARY_DATA_MAX)
        };
        &bytes[..len]
    }
    /// Unicode: os primeiros 4096 bytes como `NvAPI_UnicodeString`.
    pub fn wstr(&self) -> String {
        // SAFETY: 4096 bytes = 2048 u16, alinhamento 4 ≥ 2.
        let w = unsafe {
            std::slice::from_raw_parts(self.raw.as_ptr() as *const u16, NVAPI_UNICODE_STRING_MAX)
        };
        from_wide(w)
    }
}

/// `NVDRS_SETTING_V1` — 12320 bytes, versão `MAKE_NVAPI_VERSION(NVDRS_SETTING_V1, 1)` = 0x0001_3020.
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
    pub fn new() -> Box<Self> {
        let mut s = Box::new(Self {
            version: 0,
            setting_name: [0; NVAPI_UNICODE_STRING_MAX],
            setting_id: 0,
            setting_type: 0,
            setting_location: 0,
            is_current_predefined: 0,
            is_predefined_valid: 0,
            predefined_value: NvdrsValue::ZERO,
            current_value: NvdrsValue::ZERO,
        });
        s.version = Self::VER;
        s
    }
}

/// `NVDRS_PROFILE_V1` — 4116 bytes, versão 0x0001_1014.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NvdrsProfileV1 {
    pub version: u32,
    pub profile_name: NvUnicodeString,
    /// Campo de bits NVDRS_GPU_SUPPORT: bit0 geforce, bit1 quadro, bit2 nvs (somente leitura).
    pub gpu_support: u32,
    pub is_predefined: u32,
    pub num_of_apps: u32,
    pub num_of_settings: u32,
}

impl NvdrsProfileV1 {
    pub const VER: u32 = make_version::<Self>(1);
}

/// `NVDRS_APPLICATION_V4` — 20492 bytes, versão 0x0004_500C.
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
}

/// `NVDRS_SETTING_VALUES` — 414112 bytes (`#pragma pack(push, 4)`). Atenção: o tamanho (0x651A0)
/// passa de 16 bits e invade o campo de versão; `MAKE_NVAPI_VERSION(..., 1)` dá 0x0007_51A0.
/// Grande demais para a pilha: alocar em `Box`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NvdrsSettingValues {
    pub version: u32,
    pub num_setting_values: u32,
    pub setting_type: u32,
    pub default_value: NvdrsValue,
    pub setting_values: [NvdrsValue; NVAPI_SETTING_MAX_VALUES],
}

impl NvdrsSettingValues {
    pub const VER: u32 = make_version::<Self>(1);
}

/// `NV_GPU_ARCH_INFO_V2` — 16 bytes, versão 0x0002_0010.
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

// Conferência dos layouts contra o SDK.
const _: () = assert!(std::mem::size_of::<NvdrsValue>() == 4100);
const _: () = assert!(std::mem::size_of::<NvdrsSettingV1>() == 12320);
const _: () = assert!(NvdrsSettingV1::VER == 0x0001_3020);
const _: () = assert!(std::mem::size_of::<NvdrsProfileV1>() == 4116);
const _: () = assert!(NvdrsProfileV1::VER == 0x0001_1014);
const _: () = assert!(std::mem::size_of::<NvdrsApplicationV4>() == 20492);
const _: () = assert!(NvdrsApplicationV4::VER == 0x0004_500C);
const _: () = assert!(std::mem::size_of::<NvdrsSettingValues>() == 414112);
const _: () = assert!(NvdrsSettingValues::VER == 0x0007_51A0);
const _: () = assert!(std::mem::size_of::<NvGpuArchInfoV2>() == 16);
const _: () = assert!(NvGpuArchInfoV2::VER == 0x0002_0010);

// ---------------------------------------------------------------- assinaturas (cdecl; no x64 = "C")
pub type FnVoid = unsafe extern "C" fn() -> NvStatus;
pub type FnGetErrorMessage = unsafe extern "C" fn(NvStatus, *mut NvShortString) -> NvStatus;
pub type FnGetDriverAndBranch = unsafe extern "C" fn(*mut u32, *mut NvShortString) -> NvStatus;
pub type FnEnumPhysicalGpus =
    unsafe extern "C" fn(*mut [Handle; NVAPI_MAX_PHYSICAL_GPUS], *mut u32) -> NvStatus;
pub type FnGpuGetFullName = unsafe extern "C" fn(Handle, *mut NvShortString) -> NvStatus;
pub type FnGpuGetArchInfo = unsafe extern "C" fn(Handle, *mut NvGpuArchInfoV2) -> NvStatus;
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
pub type FnGetProfileInfo = unsafe extern "C" fn(Handle, Handle, *mut NvdrsProfileV1) -> NvStatus;
pub type FnGetSetting = unsafe extern "C" fn(Handle, Handle, u32, *mut NvdrsSettingV1) -> NvStatus;
pub type FnGetSettingInterno =
    unsafe extern "C" fn(Handle, Handle, u32, *mut NvdrsSettingV1, *mut u32) -> NvStatus;
pub type FnEnumSettings =
    unsafe extern "C" fn(Handle, Handle, u32, *mut u32, *mut NvdrsSettingV1) -> NvStatus;
pub type FnEnumAvailableSettingIds = unsafe extern "C" fn(*mut u32, *mut u32) -> NvStatus;
pub type FnGetSettingNameFromId = unsafe extern "C" fn(u32, *mut NvUnicodeString) -> NvStatus;

// ---------------------------------------------------------------- utilitários de string
pub fn from_wide(w: &[u16]) -> String {
    let n = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..n])
}

pub fn from_short(s: &NvShortString) -> String {
    let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..n]).into_owned()
}

pub fn to_unicode_string(s: &str) -> Box<NvUnicodeString> {
    let mut b = Box::new([0u16; NVAPI_UNICODE_STRING_MAX]);
    for (i, c) in s
        .encode_utf16()
        .take(NVAPI_UNICODE_STRING_MAX - 1)
        .enumerate()
    {
        b[i] = c;
    }
    b
}
