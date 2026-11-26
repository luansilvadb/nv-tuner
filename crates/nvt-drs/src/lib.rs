//! FFI própria da NVAPI DRS (Driver Settings).
//!
//! Responsável por carregar `nvapi64.dll`, abrir a sessão DRS, ler e gravar
//! configurações no perfil global e nos perfis de aplicativo. Todo `unsafe`
//! do projeto relacionado ao driver deve ficar confinado neste crate.
//!
//! Especificação: `docs/pesquisa/nvapi-drs.md`. Pontos principais:
//!
//! - [`NvApi::load`] carrega a DLL e chama `NvAPI_Initialize`; `Drop` chama `NvAPI_Unload`.
//! - [`Session::open`] cria e carrega a sessão DRS; `Drop` a destrói sem gravar.
//! - Leituras e gravações de settings usam as entradas **internas** (sem o filtro de
//!   settings ocultas, que dariam -160) com fallback para as públicas.
//! - [`exibicao`] lê refresh e VRR do monitor principal (`NvAPI_DISP_GetDisplayConfig`,
//!   `NvAPI_Disp_GetVRRInfo`), sem APIs Win32 de modo de vídeo (funciona na sessão 0).
//! - Só [`Session::save`] grava no disco. No 621.40 ele grava mesmo sem administrador;
//!   `-137` (quando o driver recusa) vira [`Error::PrivilegeRequired`].
//!
//! IDs, constantes e layouts transcritos do NVAPI SDK — Copyright (c) 2019-2026 NVIDIA
//! CORPORATION & AFFILIATES, licença MIT. IDs internos observados no NVIDIA Profile
//! Inspector — Copyright (c) 2016 Orbmu2k, licença MIT.

mod api;
mod drs;
mod error;
pub mod exibicao;
pub mod ffi;
mod snapshot;
mod strings;

pub use api::{ArchInfo, DriverVersion, EntryOrigin, GpuInfo, NvApi};
pub use drs::{
    Application, EntryPoints, Profile, ProfileInfo, Session, Setting, SettingLocation, SettingValue,
};
pub use error::{Error, Result, Status};
pub use exibicao::{
    AlvoExibicao, CaminhoExibicao, FonteExibicao, FonteFalsa, InfoExibicao, VrrBruto,
    ler_exibicao_principal,
};
pub use snapshot::{
    ProfileRef, ProfileSnapshot, RestoreOp, RestorePlan, SNAPSHOT_FORMAT, SettingSnapshot,
    Snapshot, apply_restore, capture, restore_plan,
};
pub use strings::{from_wide, to_unicode_string};

#[cfg(test)]
mod tests;
