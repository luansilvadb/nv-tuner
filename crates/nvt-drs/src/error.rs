//! Erros tipados do `nvt-drs`.

use std::fmt;

/// Códigos `NvAPI_Status` relevantes ao DRS (`nvapi_lite_common.h`). Códigos não mapeados
/// ficam em [`Status::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    Error,
    LibraryNotFound,
    NoImplementation,
    ApiNotInitialized,
    InvalidArgument,
    NvidiaDeviceNotFound,
    EndEnumeration,
    InvalidHandle,
    IncompatibleStructVersion,
    InvalidPointer,
    InvalidUserPrivilege,
    SettingNotFound,
    SettingSizeTooLarge,
    TooManySettingsInProfile,
    ProfileNotFound,
    ProfileNameInUse,
    ProfileNameEmpty,
    ExecutableNotFound,
    ExecutableAlreadyInUse,
    DatatypeMismatch,
    ProfileRemoved,
    ExecutablePathIsAmbiguous,
    /// Qualquer outro código negativo.
    Other(i32),
}

impl Status {
    /// Converte o código bruto. `0` (sucesso) não é um erro e vira `Other(0)`.
    pub fn from_code(code: i32) -> Self {
        match code {
            -1 => Self::Error,
            -2 => Self::LibraryNotFound,
            -3 => Self::NoImplementation,
            -4 => Self::ApiNotInitialized,
            -5 => Self::InvalidArgument,
            -6 => Self::NvidiaDeviceNotFound,
            -7 => Self::EndEnumeration,
            -8 => Self::InvalidHandle,
            -9 => Self::IncompatibleStructVersion,
            -14 => Self::InvalidPointer,
            -137 => Self::InvalidUserPrivilege,
            -160 => Self::SettingNotFound,
            -161 => Self::SettingSizeTooLarge,
            -162 => Self::TooManySettingsInProfile,
            -163 => Self::ProfileNotFound,
            -164 => Self::ProfileNameInUse,
            -165 => Self::ProfileNameEmpty,
            -166 => Self::ExecutableNotFound,
            -167 => Self::ExecutableAlreadyInUse,
            -168 => Self::DatatypeMismatch,
            -169 => Self::ProfileRemoved,
            -182 => Self::ExecutablePathIsAmbiguous,
            c => Self::Other(c),
        }
    }

    /// Código bruto `NvAPI_Status`.
    pub fn code(self) -> i32 {
        match self {
            Self::Error => -1,
            Self::LibraryNotFound => -2,
            Self::NoImplementation => -3,
            Self::ApiNotInitialized => -4,
            Self::InvalidArgument => -5,
            Self::NvidiaDeviceNotFound => -6,
            Self::EndEnumeration => -7,
            Self::InvalidHandle => -8,
            Self::IncompatibleStructVersion => -9,
            Self::InvalidPointer => -14,
            Self::InvalidUserPrivilege => -137,
            Self::SettingNotFound => -160,
            Self::SettingSizeTooLarge => -161,
            Self::TooManySettingsInProfile => -162,
            Self::ProfileNotFound => -163,
            Self::ProfileNameInUse => -164,
            Self::ProfileNameEmpty => -165,
            Self::ExecutableNotFound => -166,
            Self::ExecutableAlreadyInUse => -167,
            Self::DatatypeMismatch => -168,
            Self::ProfileRemoved => -169,
            Self::ExecutablePathIsAmbiguous => -182,
            Self::Other(c) => c,
        }
    }

    /// Nome da constante no SDK (`NVAPI_*`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Error => "NVAPI_ERROR",
            Self::LibraryNotFound => "NVAPI_LIBRARY_NOT_FOUND",
            Self::NoImplementation => "NVAPI_NO_IMPLEMENTATION",
            Self::ApiNotInitialized => "NVAPI_API_NOT_INITIALIZED",
            Self::InvalidArgument => "NVAPI_INVALID_ARGUMENT",
            Self::NvidiaDeviceNotFound => "NVAPI_NVIDIA_DEVICE_NOT_FOUND",
            Self::EndEnumeration => "NVAPI_END_ENUMERATION",
            Self::InvalidHandle => "NVAPI_INVALID_HANDLE",
            Self::IncompatibleStructVersion => "NVAPI_INCOMPATIBLE_STRUCT_VERSION",
            Self::InvalidPointer => "NVAPI_INVALID_POINTER",
            Self::InvalidUserPrivilege => "NVAPI_INVALID_USER_PRIVILEGE",
            Self::SettingNotFound => "NVAPI_SETTING_NOT_FOUND",
            Self::SettingSizeTooLarge => "NVAPI_SETTING_SIZE_TOO_LARGE",
            Self::TooManySettingsInProfile => "NVAPI_TOO_MANY_SETTINGS_IN_PROFILE",
            Self::ProfileNotFound => "NVAPI_PROFILE_NOT_FOUND",
            Self::ProfileNameInUse => "NVAPI_PROFILE_NAME_IN_USE",
            Self::ProfileNameEmpty => "NVAPI_PROFILE_NAME_EMPTY",
            Self::ExecutableNotFound => "NVAPI_EXECUTABLE_NOT_FOUND",
            Self::ExecutableAlreadyInUse => "NVAPI_EXECUTABLE_ALREADY_IN_USE",
            Self::DatatypeMismatch => "NVAPI_DATATYPE_MISMATCH",
            Self::ProfileRemoved => "NVAPI_PROFILE_REMOVED",
            Self::ExecutablePathIsAmbiguous => "NVAPI_EXECUTABLE_PATH_IS_AMBIGUOUS",
            Self::Other(_) => "NVAPI_?",
        }
    }
}

/// Erro do `nvt-drs`.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// `nvapi64.dll` não pôde ser carregada (driver NVIDIA ausente ou sistema não Windows).
    DllNotFound,
    /// A DLL não exporta `nvapi_QueryInterface`.
    QueryInterfaceNotFound,
    /// `nvapi_QueryInterface` devolveu nulo para a função (nem interna nem pública).
    FunctionUnavailable { function: &'static str },
    /// Uma chamada da NVAPI devolveu erro.
    Nvapi {
        function: &'static str,
        status: Status,
        /// Mensagem de `NvAPI_GetErrorMessage` (ou o nome da constante, se indisponível).
        message: String,
    },
    /// A operação exige administrador (`NVAPI_INVALID_USER_PRIVILEGE`, -137).
    PrivilegeRequired {
        function: &'static str,
        message: String,
    },
    /// Texto inválido para `NvAPI_UnicodeString`.
    InvalidString { reason: &'static str },
    /// Valor fora dos limites da estrutura (ex.: binário com mais de 4096 bytes).
    InvalidValue { reason: &'static str },
    /// Falha ao (des)serializar um snapshot.
    Snapshot(String),
}

impl Error {
    /// Constrói o erro de uma chamada que devolveu `code` (≠ 0). Privilégio vira
    /// [`Error::PrivilegeRequired`].
    pub fn from_status(function: &'static str, code: i32, message: String) -> Self {
        let status = Status::from_code(code);
        if status == Status::InvalidUserPrivilege {
            Self::PrivilegeRequired { function, message }
        } else {
            Self::Nvapi {
                function,
                status,
                message,
            }
        }
    }

    /// Código `NvAPI_Status`, quando o erro veio da NVAPI.
    pub fn code(&self) -> Option<i32> {
        match self {
            Self::Nvapi { status, .. } => Some(status.code()),
            Self::PrivilegeRequired { .. } => Some(Status::InvalidUserPrivilege.code()),
            _ => None,
        }
    }

    /// `true` se o erro é falta de privilégio de administrador.
    pub fn is_privilege(&self) -> bool {
        matches!(self, Self::PrivilegeRequired { .. })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DllNotFound => f.write_str("nvapi64.dll não encontrada (driver NVIDIA ausente?)"),
            Self::QueryInterfaceNotFound => {
                f.write_str("nvapi64.dll sem a exportação nvapi_QueryInterface")
            }
            Self::FunctionUnavailable { function } => {
                write!(f, "{function} indisponível neste driver")
            }
            Self::Nvapi {
                function,
                status,
                message,
            } => write!(
                f,
                "{function}: {} {} ({message})",
                status.code(),
                status.name()
            ),
            Self::PrivilegeRequired { function, message } => write!(
                f,
                "{function}: exige administrador (-137 NVAPI_INVALID_USER_PRIVILEGE: {message})"
            ),
            Self::InvalidString { reason } => write!(f, "texto inválido para a NVAPI: {reason}"),
            Self::InvalidValue { reason } => write!(f, "valor inválido: {reason}"),
            Self::Snapshot(m) => write!(f, "snapshot: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// Resultado do `nvt-drs`.
pub type Result<T> = std::result::Result<T, Error>;
