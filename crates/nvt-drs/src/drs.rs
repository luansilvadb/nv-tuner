//! Sessão DRS (Driver Settings): perfis, aplicações e settings.
//!
//! Leituras funcionam sem administrador. As operações que alteram a sessão
//! ([`Session::set_setting`], [`Session::delete_setting`], [`Session::create_profile`] etc.)
//! só mudam a cópia em memória; nada chega ao disco sem [`Session::save`]. Se o driver
//! recusar a gravação por privilégio, `save` devolve [`Error::PrivilegeRequired`]; no
//! 621.40 ele grava mesmo sem administrador.

use std::fmt;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use crate::api::{EntryOrigin, NvApi};
use crate::error::{Error, Result};
use crate::ffi::{self, Handle, id};
use crate::strings::{from_wide, to_unicode_string};

/// Quantas estruturas pedir por chamada nas enumerações em lote.
const LOTE: usize = 32;

/// Sessão DRS carregada (`CreateSession` + `LoadSettings`). `Drop` chama `DestroySession`,
/// descartando tudo que não passou por [`Session::save`].
pub struct Session<'a> {
    api: &'a NvApi,
    h: Handle,
}

/// Handle de perfil, válido enquanto a sessão que o produziu viver.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Profile<'s> {
    h: Handle,
    _s: PhantomData<&'s ()>,
}

impl fmt::Debug for Profile<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Profile({:p})", self.h)
    }
}

impl Profile<'_> {
    fn new(h: Handle) -> Self {
        Self { h, _s: PhantomData }
    }
}

/// `NVDRS_PROFILE_V1` convertido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileInfo {
    pub name: String,
    pub is_predefined: bool,
    /// Campo de bits `NVDRS_GPU_SUPPORT`: bit0 geforce, bit1 quadro, bit2 nvs.
    pub gpu_support: u32,
    pub num_apps: u32,
    pub num_settings: u32,
}

/// `NVDRS_APPLICATION_V4` convertido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Application {
    pub app_name: String,
    pub friendly_name: String,
    pub launcher: String,
    pub file_in_folder: String,
    pub command_line: String,
    pub is_predefined: bool,
    pub is_metro: bool,
    pub is_command_line: bool,
}

impl Application {
    fn from_raw(a: &ffi::NvdrsApplicationV4) -> Self {
        Self {
            app_name: from_wide(&a.app_name),
            friendly_name: from_wide(&a.user_friendly_name),
            launcher: from_wide(&a.launcher),
            file_in_folder: from_wide(&a.file_in_folder),
            command_line: from_wide(&a.command_line),
            is_predefined: a.is_predefined != 0,
            is_metro: a.flags & 1 != 0,
            is_command_line: a.flags & 2 != 0,
        }
    }
}

/// `NVDRS_SETTING_LOCATION`: de onde veio o valor lido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingLocation {
    /// Gravado no próprio perfil consultado.
    Current,
    /// Herdado do perfil global atual.
    Global,
    /// Herdado do perfil base.
    Base,
    /// Valor padrão do driver.
    Default,
    Other(u32),
}

impl SettingLocation {
    pub fn from_raw(v: u32) -> Self {
        match v {
            0 => Self::Current,
            1 => Self::Global,
            2 => Self::Base,
            3 => Self::Default,
            o => Self::Other(o),
        }
    }
}

/// Valor de setting, com o tipo `NVDRS_SETTING_TYPE`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", content = "valor")]
pub enum SettingValue {
    Dword(u32),
    Binary(Vec<u8>),
    /// `NVDRS_STRING_TYPE`; a NVAPI devolve o texto no mesmo buffer UTF-16 do `WString`.
    String(String),
    WString(String),
    Qword(u64),
    Float(f32),
}

impl SettingValue {
    fn from_raw(ty: u32, v: &ffi::NvdrsValue) -> Option<Self> {
        Some(match ty {
            ffi::NVDRS_DWORD_TYPE => Self::Dword(v.u32()),
            ffi::NVDRS_BINARY_TYPE => Self::Binary(v.binary()),
            ffi::NVDRS_STRING_TYPE => Self::String(v.wstr()),
            ffi::NVDRS_WSTRING_TYPE => Self::WString(v.wstr()),
            ffi::NVDRS_QWORD_TYPE => Self::Qword(v.u64()),
            ffi::NVDRS_FLOAT_TYPE => Self::Float(v.f32()),
            _ => return None,
        })
    }

    /// Código `NVDRS_SETTING_TYPE`.
    pub fn type_code(&self) -> u32 {
        match self {
            Self::Dword(_) => ffi::NVDRS_DWORD_TYPE,
            Self::Binary(_) => ffi::NVDRS_BINARY_TYPE,
            Self::String(_) => ffi::NVDRS_STRING_TYPE,
            Self::WString(_) => ffi::NVDRS_WSTRING_TYPE,
            Self::Qword(_) => ffi::NVDRS_QWORD_TYPE,
            Self::Float(_) => ffi::NVDRS_FLOAT_TYPE,
        }
    }

    /// Valor DWORD, se for desse tipo.
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Self::Dword(v) => Some(*v),
            _ => None,
        }
    }

    fn write_raw(&self, dst: &mut ffi::NvdrsValue) -> Result<()> {
        match self {
            Self::Dword(v) => dst.set_u32(*v),
            Self::Qword(v) => dst.set_u64(*v),
            Self::Float(v) => dst.set_u32(v.to_bits()),
            Self::Binary(b) => {
                if b.len() > ffi::NVAPI_BINARY_DATA_MAX {
                    return Err(Error::InvalidValue {
                        reason: "binário com mais de 4096 bytes",
                    });
                }
                dst.set_binary(b);
            }
            Self::String(s) | Self::WString(s) => dst.set_wide(&*to_unicode_string(s)?),
        }
        Ok(())
    }
}

/// `NVDRS_SETTING_V1` convertido.
#[derive(Debug, Clone, PartialEq)]
pub struct Setting {
    pub id: u32,
    /// Nome devolvido pelo driver (vazio para várias settings ocultas).
    pub name: String,
    pub location: SettingLocation,
    /// `isCurrentPredefined`: o valor atual é o predefinido pela NVIDIA.
    pub is_current_predefined: bool,
    /// Valor predefinido pela NVIDIA, quando `isPredefinedValid`.
    pub predefined: Option<SettingValue>,
    pub value: SettingValue,
}

impl Setting {
    /// `None` se o tipo for desconhecido.
    pub(crate) fn from_raw(s: &ffi::NvdrsSettingV1) -> Option<Self> {
        let value = SettingValue::from_raw(s.setting_type, &s.current_value)?;
        let predefined = if s.is_predefined_valid != 0 {
            SettingValue::from_raw(s.setting_type, &s.predefined_value)
        } else {
            None
        };
        Some(Self {
            id: s.setting_id,
            name: from_wide(&s.setting_name),
            location: SettingLocation::from_raw(s.setting_location),
            is_current_predefined: s.is_current_predefined != 0,
            predefined,
            value,
        })
    }

    /// Gravada pelo usuário neste perfil (o “UserSpecified” do NPI): está no próprio perfil
    /// e o valor atual não é o predefinido.
    pub fn is_user_specified(&self) -> bool {
        self.location == SettingLocation::Current && !self.is_current_predefined
    }
}

/// Qual entrada (interna ou pública) cada operação DRS usa neste driver; `None` = ausente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryPoints {
    pub get_setting: Option<EntryOrigin>,
    pub set_setting: Option<EntryOrigin>,
    pub enum_settings: Option<EntryOrigin>,
    pub delete_setting: Option<EntryOrigin>,
    pub restore_setting_default: Option<EntryOrigin>,
    pub setting_name: Option<EntryOrigin>,
}

impl NvApi {
    /// Diagnóstico: quais IDs internos o driver tem.
    pub fn drs_entry_points(&self) -> EntryPoints {
        let o = |interno: u32, publico: u32| {
            if self.ptr(interno).is_some() {
                Some(EntryOrigin::Internal)
            } else if self.ptr(publico).is_some() {
                Some(EntryOrigin::Public)
            } else {
                None
            }
        };
        EntryPoints {
            get_setting: o(id::interno::DRS_GET_SETTING, id::DRS_GET_SETTING),
            set_setting: o(id::interno::DRS_SET_SETTING, id::DRS_SET_SETTING),
            enum_settings: o(id::interno::DRS_ENUM_SETTINGS, id::DRS_ENUM_SETTINGS),
            delete_setting: o(
                id::interno::DRS_DELETE_PROFILE_SETTING,
                id::DRS_DELETE_PROFILE_SETTING,
            ),
            restore_setting_default: o(
                id::interno::DRS_RESTORE_PROFILE_DEFAULT_SETTING,
                id::DRS_RESTORE_PROFILE_DEFAULT_SETTING,
            ),
            setting_name: o(
                id::interno::DRS_GET_SETTING_NAME_FROM_ID,
                id::DRS_GET_SETTING_NAME_FROM_ID,
            ),
        }
    }

    /// Nome da setting no driver (interno, depois público); `None` se vazio/desconhecido.
    pub fn setting_name(&self, setting_id: u32) -> Option<String> {
        for idf in [
            id::interno::DRS_GET_SETTING_NAME_FROM_ID,
            id::DRS_GET_SETTING_NAME_FROM_ID,
        ] {
            if let Some(f) = self.get::<ffi::FnGetSettingNameFromId>(idf) {
                let mut b = Box::new([0u16; ffi::NVAPI_UNICODE_STRING_MAX]);
                // SAFETY: buffer de 2048 u16.
                let st = unsafe { f(setting_id, &mut *b) };
                let n = from_wide(&b[..]).trim().to_string();
                if st == ffi::NVAPI_OK && !n.is_empty() {
                    return Some(n);
                }
            }
        }
        None
    }
}

impl<'a> Session<'a> {
    /// `DRS_CreateSession` + `DRS_LoadSettings`.
    pub fn open(api: &'a NvApi) -> Result<Self> {
        const F: &str = "NvAPI_DRS_CreateSession";
        let create: ffi::FnCreateSession = api.resolve(id::DRS_CREATE_SESSION, F)?;
        let mut h: Handle = std::ptr::null_mut();
        // SAFETY: ponteiro de saída válido.
        api.check(F, unsafe { create(&mut h) })?;
        let s = Self { api, h };
        const L: &str = "NvAPI_DRS_LoadSettings";
        let load: ffi::FnSession = api.resolve(id::DRS_LOAD_SETTINGS, L)?;
        // SAFETY: sessão recém-criada; em erro, `Drop` a destrói.
        api.check(L, unsafe { load(s.h) })?;
        Ok(s)
    }

    /// A NVAPI desta sessão.
    pub fn api(&self) -> &'a NvApi {
        self.api
    }

    fn get_profile(&self, idf: u32, function: &'static str) -> Result<Profile<'_>> {
        let f: ffi::FnGetProfile = self.api.resolve(idf, function)?;
        let mut p: Handle = std::ptr::null_mut();
        // SAFETY: sessão válida e ponteiro de saída.
        self.api.check(function, unsafe { f(self.h, &mut p) })?;
        Ok(Profile::new(p))
    }

    /// Perfil base (onde o painel grava as configurações globais).
    pub fn base_profile(&self) -> Result<Profile<'_>> {
        self.get_profile(id::DRS_GET_BASE_PROFILE, "NvAPI_DRS_GetBaseProfile")
    }

    /// Perfil pelo nome; `Ok(None)` se não existir (-163).
    pub fn find_profile_by_name(&self, name: &str) -> Result<Option<Profile<'_>>> {
        const F: &str = "NvAPI_DRS_FindProfileByName";
        let f: ffi::FnFindProfileByName = self.api.resolve(id::DRS_FIND_PROFILE_BY_NAME, F)?;
        let nome = to_unicode_string(name)?;
        let mut p: Handle = std::ptr::null_mut();
        // SAFETY: nome com 2048 u16 terminado em zero; saída válida.
        let st = unsafe { f(self.h, &*nome, &mut p) };
        if st == ffi::NVAPI_PROFILE_NOT_FOUND {
            return Ok(None);
        }
        self.api.check(F, st)?;
        Ok(Some(Profile::new(p)))
    }

    /// Perfil que contém o executável `exe`; `Ok(None)` se nenhum (-166).
    pub fn find_application(&self, exe: &str) -> Result<Option<(Profile<'_>, Application)>> {
        const F: &str = "NvAPI_DRS_FindApplicationByName";
        let f: ffi::FnFindApplicationByName =
            self.api.resolve(id::DRS_FIND_APPLICATION_BY_NAME, F)?;
        let nome = to_unicode_string(exe)?;
        let mut app = ffi::NvdrsApplicationV4::boxed();
        let mut p: Handle = std::ptr::null_mut();
        // SAFETY: nome de 2048 u16, saída e estrutura V4 com versão preenchida.
        let st = unsafe { f(self.h, &*nome, &mut p, &mut *app) };
        if st == ffi::NVAPI_EXECUTABLE_NOT_FOUND {
            return Ok(None);
        }
        self.api.check(F, st)?;
        Ok(Some((Profile::new(p), Application::from_raw(&app))))
    }

    /// Informações do perfil.
    pub fn profile_info(&self, p: Profile<'_>) -> Result<ProfileInfo> {
        const F: &str = "NvAPI_DRS_GetProfileInfo";
        let f: ffi::FnGetProfileInfo = self.api.resolve(id::DRS_GET_PROFILE_INFO, F)?;
        let mut info = Box::new(ffi::NvdrsProfileV1::ZERO);
        // SAFETY: estrutura V1 com versão preenchida; handles válidos (lifetime).
        self.api.check(F, unsafe { f(self.h, p.h, &mut *info) })?;
        Ok(ProfileInfo {
            name: from_wide(&info.profile_name),
            is_predefined: info.is_predefined != 0,
            gpu_support: info.gpu_support,
            num_apps: info.num_of_apps,
            num_settings: info.num_of_settings,
        })
    }

    /// Todos os perfis da sessão.
    pub fn enum_profiles(&self) -> Result<Vec<Profile<'_>>> {
        const F: &str = "NvAPI_DRS_EnumProfiles";
        let f: ffi::FnEnumProfiles = self.api.resolve(id::DRS_ENUM_PROFILES, F)?;
        let mut v = Vec::new();
        for i in 0u32.. {
            let mut p: Handle = std::ptr::null_mut();
            // SAFETY: sessão válida e saída válida.
            let st = unsafe { f(self.h, i, &mut p) };
            if st == ffi::NVAPI_END_ENUMERATION {
                break;
            }
            self.api.check(F, st)?;
            v.push(Profile::new(p));
        }
        Ok(v)
    }

    /// Executáveis do perfil (vazio no perfil base, que dá -7 de imediato).
    pub fn enum_applications(&self, p: Profile<'_>) -> Result<Vec<Application>> {
        const F: &str = "NvAPI_DRS_EnumApplications";
        let f: ffi::FnEnumApplications = self.api.resolve(id::DRS_ENUM_APPLICATIONS, F)?;
        let mut buf = vec![ffi::NvdrsApplicationV4::ZERO; LOTE];
        let mut v = Vec::new();
        loop {
            buf.fill(ffi::NvdrsApplicationV4::ZERO);
            let mut n = LOTE as u32;
            // SAFETY: vetor de LOTE estruturas V4 com versão preenchida; `n` = capacidade.
            let st = unsafe { f(self.h, p.h, v.len() as u32, &mut n, buf.as_mut_ptr()) };
            if st == ffi::NVAPI_END_ENUMERATION {
                break;
            }
            self.api.check(F, st)?;
            let n = (n as usize).min(LOTE);
            v.extend(buf[..n].iter().map(Application::from_raw));
            if n < LOTE {
                break;
            }
        }
        Ok(v)
    }

    /// Lê a setting `setting_id` do perfil pela entrada interna (sem o filtro de ocultas)
    /// com fallback para a pública. `Ok(None)` = não gravada neste perfil nem herdável (-160).
    ///
    /// Em perfil de aplicação, uma setting não gravada no próprio perfil volta com
    /// [`SettingLocation`] ≠ `Current` (herdada). Se só a entrada pública existir, -160 é
    /// ambíguo (pode ser setting oculta); ver [`NvApi::drs_entry_points`].
    pub fn get_setting(&self, p: Profile<'_>, setting_id: u32) -> Result<Option<Setting>> {
        const F: &str = "NvAPI_DRS_GetSetting";
        let mut s = ffi::NvdrsSettingV1::boxed();
        let st = if let Some(f) = self
            .api
            .get::<ffi::FnGetSettingInterno>(id::interno::DRS_GET_SETTING)
        {
            let mut x = 0u32;
            // SAFETY: assinatura interna (5º parâmetro de saída, como no NPI 3.0.2.1);
            // estrutura V1 com versão preenchida.
            unsafe { f(self.h, p.h, setting_id, &mut *s, &mut x) }
        } else {
            let f: ffi::FnGetSetting = self.api.resolve(id::DRS_GET_SETTING, F)?;
            // SAFETY: estrutura V1 com versão preenchida.
            unsafe { f(self.h, p.h, setting_id, &mut *s) }
        };
        if st == ffi::NVAPI_SETTING_NOT_FOUND {
            return Ok(None);
        }
        self.api.check(F, st)?;
        Setting::from_raw(&s).map(Some).ok_or(Error::InvalidValue {
            reason: "tipo de setting desconhecido",
        })
    }

    /// Settings gravadas no perfil (interna com fallback pública). Settings de tipo
    /// desconhecido são omitidas.
    pub fn enum_settings(&self, p: Profile<'_>) -> Result<Vec<Setting>> {
        const F: &str = "NvAPI_DRS_EnumSettings";
        let (f, _): (ffi::FnEnumSettings, _) =
            self.api
                .resolve_fallback(id::interno::DRS_ENUM_SETTINGS, id::DRS_ENUM_SETTINGS, F)?;
        let mut buf = vec![ffi::NvdrsSettingV1::ZERO; LOTE];
        let mut v = Vec::new();
        let mut inicio = 0u32;
        loop {
            buf.fill(ffi::NvdrsSettingV1::ZERO);
            let mut n = LOTE as u32;
            // SAFETY: vetor de LOTE estruturas V1 com versão preenchida; `n` = capacidade.
            let st = unsafe { f(self.h, p.h, inicio, &mut n, buf.as_mut_ptr()) };
            if st == ffi::NVAPI_END_ENUMERATION {
                break;
            }
            self.api.check(F, st)?;
            let n = (n as usize).min(LOTE);
            inicio += n as u32;
            v.extend(buf[..n].iter().filter_map(Setting::from_raw));
            if n < LOTE {
                break;
            }
        }
        Ok(v)
    }

    /// Grava (em memória) o valor da setting no perfil, pela entrada interna com fallback.
    /// Persistir exige [`Session::save`].
    pub fn set_setting(&self, p: Profile<'_>, setting_id: u32, value: &SettingValue) -> Result<()> {
        const F: &str = "NvAPI_DRS_SetSetting";
        let mut s = ffi::NvdrsSettingV1::boxed();
        s.setting_id = setting_id;
        s.setting_type = value.type_code();
        value.write_raw(&mut s.current_value)?;
        let st = if let Some(f) = self
            .api
            .get::<ffi::FnSetSettingInterno>(id::interno::DRS_SET_SETTING)
        {
            // SAFETY: assinatura interna com dois u32 extras (0, 0 como no NPI 3.0.2.1).
            unsafe { f(self.h, p.h, &mut *s, 0, 0) }
        } else {
            let f: ffi::FnSetSetting = self.api.resolve(id::DRS_SET_SETTING, F)?;
            // SAFETY: estrutura V1 com versão, id, tipo e valor preenchidos.
            unsafe { f(self.h, p.h, &mut *s) }
        };
        self.api.check(F, st)
    }

    /// Atalho para settings DWORD.
    pub fn set_setting_u32(&self, p: Profile<'_>, setting_id: u32, value: u32) -> Result<()> {
        self.set_setting(p, setting_id, &SettingValue::Dword(value))
    }

    fn profile_setting_op(
        &self,
        p: Profile<'_>,
        setting_id: u32,
        interno: u32,
        publico: u32,
        function: &'static str,
    ) -> Result<()> {
        let (f, _): (ffi::FnProfileSettingId, _) =
            self.api.resolve_fallback(interno, publico, function)?;
        // SAFETY: sessão e perfil válidos.
        self.api
            .check(function, unsafe { f(self.h, p.h, setting_id) })
    }

    /// Remove (em memória) o valor gravado pelo usuário no perfil.
    pub fn delete_setting(&self, p: Profile<'_>, setting_id: u32) -> Result<()> {
        self.profile_setting_op(
            p,
            setting_id,
            id::interno::DRS_DELETE_PROFILE_SETTING,
            id::DRS_DELETE_PROFILE_SETTING,
            "NvAPI_DRS_DeleteProfileSetting",
        )
    }

    /// Volta (em memória) a setting ao valor predefinido pela NVIDIA.
    pub fn restore_setting_default(&self, p: Profile<'_>, setting_id: u32) -> Result<()> {
        self.profile_setting_op(
            p,
            setting_id,
            id::interno::DRS_RESTORE_PROFILE_DEFAULT_SETTING,
            id::DRS_RESTORE_PROFILE_DEFAULT_SETTING,
            "NvAPI_DRS_RestoreProfileDefaultSetting",
        )
    }

    /// Cria (em memória) um perfil de usuário.
    pub fn create_profile(&self, name: &str) -> Result<Profile<'_>> {
        const F: &str = "NvAPI_DRS_CreateProfile";
        let f: ffi::FnCreateProfile = self.api.resolve(id::DRS_CREATE_PROFILE, F)?;
        let mut info = Box::new(ffi::NvdrsProfileV1::ZERO);
        info.profile_name = *to_unicode_string(name)?;
        let mut p: Handle = std::ptr::null_mut();
        // SAFETY: estrutura V1 com versão e nome preenchidos; saída válida.
        self.api
            .check(F, unsafe { f(self.h, &mut *info, &mut p) })?;
        Ok(Profile::new(p))
    }

    /// Associa (em memória) o executável `exe` ao perfil.
    pub fn create_application(
        &self,
        p: Profile<'_>,
        exe: &str,
        friendly_name: Option<&str>,
    ) -> Result<()> {
        const F: &str = "NvAPI_DRS_CreateApplication";
        let f: ffi::FnCreateApplication = self.api.resolve(id::DRS_CREATE_APPLICATION, F)?;
        let mut app = ffi::NvdrsApplicationV4::boxed();
        app.app_name = *to_unicode_string(exe)?;
        app.user_friendly_name = *to_unicode_string(friendly_name.unwrap_or(exe))?;
        // SAFETY: estrutura V4 com versão e nomes preenchidos.
        self.api.check(F, unsafe { f(self.h, p.h, &mut *app) })
    }

    /// Apaga (em memória) o perfil `p`. O handle `p` não deve ser usado depois; perfis
    /// predefinidos pela NVIDIA também podem ser apagados, então confira antes de chamar.
    pub fn delete_profile(&self, p: Profile<'_>) -> Result<()> {
        const F: &str = "NvAPI_DRS_DeleteProfile";
        let f: ffi::FnDeleteProfile = self.api.resolve(id::DRS_DELETE_PROFILE, F)?;
        // SAFETY: sessão e perfil válidos (lifetime).
        self.api.check(F, unsafe { f(self.h, p.h) })
    }

    /// Remove (em memória) o executável `exe` do perfil.
    pub fn delete_application(&self, p: Profile<'_>, exe: &str) -> Result<()> {
        const F: &str = "NvAPI_DRS_DeleteApplication";
        let f: ffi::FnDeleteApplication = self.api.resolve(id::DRS_DELETE_APPLICATION, F)?;
        let nome = to_unicode_string(exe)?;
        // SAFETY: sessão e perfil válidos; nome com 2048 u16 terminado em zero.
        self.api.check(F, unsafe { f(self.h, p.h, &*nome) })
    }

    /// `DRS_SaveSettings`: grava a sessão no disco. `-137` vira [`Error::PrivilegeRequired`].
    ///
    /// **Não conte com essa recusa como proteção**: no 621.40 o driver gravou alterações a
    /// partir de um processo sem administrador (integridade média). Quem chama decide se a
    /// gravação é permitida.
    pub fn save(&self) -> Result<()> {
        const F: &str = "NvAPI_DRS_SaveSettings";
        let f: ffi::FnSession = self.api.resolve(id::DRS_SAVE_SETTINGS, F)?;
        // SAFETY: sessão válida.
        self.api.check(F, unsafe { f(self.h) })
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        if let Some(f) = self.api.get::<ffi::FnSession>(id::DRS_DESTROY_SESSION) {
            // SAFETY: sessão válida; nenhum `Profile` sobrevive a ela (lifetime).
            unsafe { f(self.h) };
        }
    }
}

impl fmt::Debug for Session<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Session({:p})", self.h)
    }
}
