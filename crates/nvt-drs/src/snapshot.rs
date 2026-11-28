//! Snapshot das settings gravadas pelo usuário e plano de restauração.
//!
//! [`capture`] só lê. [`restore_plan`] é puro (compara dois snapshots). [`apply_restore`]
//! altera a sessão e chama [`Session::save`], que grava no disco.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::drs::{Session, SettingValue};
use crate::error::{Error, Result};

/// Versão do formato JSON.
pub const SNAPSHOT_FORMAT: u32 = 1;

/// Como achar o perfil ao restaurar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", content = "nome")]
pub enum ProfileRef {
    /// Perfil base (`DRS_GetBaseProfile`), independente do nome.
    Base,
    /// Perfil pelo nome (`DRS_FindProfileByName`).
    Named(String),
}

/// Setting gravada pelo usuário.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingSnapshot {
    pub id: u32,
    /// Nome no driver (pode ser vazio para settings ocultas).
    #[serde(default)]
    pub name: String,
    #[serde(flatten)]
    pub value: SettingValue,
    /// A NVIDIA tem valor predefinido para ela neste perfil (restaurar = voltar a ele).
    #[serde(default)]
    pub predefined_valid: bool,
}

/// Um perfil no snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileSnapshot {
    pub profile: ProfileRef,
    /// Nome do perfil no momento da captura.
    pub name: String,
    pub is_predefined: bool,
    /// Executáveis pedidos na captura que resolveram para este perfil.
    pub applications: Vec<String>,
    /// Somente settings gravadas pelo usuário (`UserSpecified`).
    pub settings: Vec<SettingSnapshot>,
}

/// Estado do DRS relevante ao `nv-tuner`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub format: u32,
    /// Versão bruta do driver (62140 = 621.40).
    pub driver_version: u32,
    pub driver_branch: String,
    /// Data e hora UTC da captura, ISO 8601 (`2026-11-25T13:45:00Z`).
    pub captured_at: String,
    pub captured_at_unix: u64,
    pub profiles: Vec<ProfileSnapshot>,
    /// Executáveis pedidos que não pertencem a nenhum perfil.
    pub missing_applications: Vec<String>,
}

impl Snapshot {
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| Error::Snapshot(e.to_string()))
    }

    pub fn from_json(s: &str) -> Result<Self> {
        let snap: Self = serde_json::from_str(s).map_err(|e| Error::Snapshot(e.to_string()))?;
        if snap.format != SNAPSHOT_FORMAT {
            return Err(Error::Snapshot(format!(
                "formato {} não suportado (esperado {SNAPSHOT_FORMAT})",
                snap.format
            )));
        }
        Ok(snap)
    }
}

/// Captura o perfil base e os perfis dos executáveis `exes` (somente leitura).
pub fn capture(session: &Session<'_>, exes: &[&str]) -> Result<Snapshot> {
    let drv = session.api().driver_version()?;
    let agora = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut profiles = Vec::new();
    let base = session.base_profile()?;
    profiles.push(profile_snapshot(
        session,
        base,
        ProfileRef::Base,
        Vec::new(),
    )?);

    // Nome do perfil nomeado -> posição em `profiles` (o base nunca casa com `Named`).
    let mut indice: HashMap<String, usize> = HashMap::new();
    let mut missing = Vec::new();
    for &exe in exes {
        match session.find_application(exe)? {
            None => missing.push(exe.to_string()),
            Some((p, _)) => {
                let info = session.profile_info(p)?;
                if let Some(&i) = indice.get(&info.name) {
                    profiles[i].applications.push(exe.to_string());
                } else {
                    indice.insert(info.name.clone(), profiles.len());
                    let r = ProfileRef::Named(info.name.clone());
                    profiles.push(snapshot_com_info(
                        session,
                        p,
                        r,
                        info,
                        vec![exe.to_string()],
                    )?);
                }
            }
        }
    }

    Ok(Snapshot {
        format: SNAPSHOT_FORMAT,
        driver_version: drv.raw,
        driver_branch: drv.branch,
        captured_at: iso8601_utc(agora),
        captured_at_unix: agora,
        profiles,
        missing_applications: missing,
    })
}

fn profile_snapshot(
    session: &Session<'_>,
    p: crate::drs::Profile<'_>,
    r: ProfileRef,
    applications: Vec<String>,
) -> Result<ProfileSnapshot> {
    let info = session.profile_info(p)?;
    snapshot_com_info(session, p, r, info, applications)
}

/// Como [`profile_snapshot`], com o `ProfileInfo` já lido (evita outra ida ao driver).
fn snapshot_com_info(
    session: &Session<'_>,
    p: crate::drs::Profile<'_>,
    r: ProfileRef,
    info: crate::drs::ProfileInfo,
    applications: Vec<String>,
) -> Result<ProfileSnapshot> {
    let settings = session
        .enum_settings(p)?
        .into_iter()
        .filter(|s| s.is_user_specified())
        .map(|s| SettingSnapshot {
            id: s.id,
            name: s.name,
            predefined_valid: s.predefined.is_some(),
            value: s.value,
        })
        .collect();
    Ok(ProfileSnapshot {
        profile: r,
        name: info.name,
        is_predefined: info.is_predefined,
        applications,
        settings,
    })
}

/// Operação de restauração.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum RestoreOp {
    /// O perfil não existe mais: recriar.
    CreateProfile { name: String },
    /// Reassociar o executável ao perfil.
    CreateApplication { profile: ProfileRef, exe: String },
    /// Gravar o valor do snapshot.
    Set {
        profile: ProfileRef,
        id: u32,
        value: SettingValue,
    },
    /// A setting foi gravada depois do snapshot: voltar ao predefinido (se houver) ou apagar.
    Reset {
        profile: ProfileRef,
        id: u32,
        predefined_valid: bool,
    },
}

/// O que aplicar sobre `current` para voltar ao estado `target`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RestorePlan {
    pub ops: Vec<RestoreOp>,
}

impl RestorePlan {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// Calcula o plano para levar o estado `current` ao snapshot `target`. Só considera os
/// perfis presentes em `target`; perfis que existem apenas em `current` ficam intocados.
pub fn restore_plan(target: &Snapshot, current: &Snapshot) -> RestorePlan {
    // Chave do perfil: `None` = base, `Some(nome)` = nomeado. Fica a primeira ocorrência,
    // como na busca linear de `Snapshot::profile`.
    let chave = |r: &ProfileRef| match r {
        ProfileRef::Base => None,
        ProfileRef::Named(n) => Some(n.clone()),
    };
    let mut por_perfil: HashMap<Option<String>, &ProfileSnapshot> = HashMap::new();
    for p in &current.profiles {
        por_perfil.entry(chave(&p.profile)).or_insert(p);
    }

    let mut ops = Vec::new();
    for tp in &target.profiles {
        let cp = por_perfil.get(&chave(&tp.profile)).copied();
        if cp.is_none() {
            if let ProfileRef::Named(name) = &tp.profile {
                ops.push(RestoreOp::CreateProfile { name: name.clone() });
            }
        }
        // `eq_ignore_ascii_case` equivale a comparar as formas em minúsculas ASCII.
        let apps_atuais: HashSet<String> = cp
            .map(|c| {
                c.applications
                    .iter()
                    .map(|a| a.to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default();
        for exe in &tp.applications {
            if !apps_atuais.contains(&exe.to_ascii_lowercase()) {
                ops.push(RestoreOp::CreateApplication {
                    profile: tp.profile.clone(),
                    exe: exe.clone(),
                });
            }
        }
        let atuais: &[SettingSnapshot] = cp.map(|c| c.settings.as_slice()).unwrap_or(&[]);
        // id -> valores atuais (vetor para preservar o caso de id repetido).
        let mut valores: HashMap<u32, Vec<&SettingValue>> = HashMap::new();
        for cs in atuais {
            valores.entry(cs.id).or_default().push(&cs.value);
        }
        let ids_alvo: HashSet<u32> = tp.settings.iter().map(|ts| ts.id).collect();
        for ts in &tp.settings {
            let igual = valores
                .get(&ts.id)
                .is_some_and(|vs| vs.iter().any(|v| **v == ts.value));
            if !igual {
                ops.push(RestoreOp::Set {
                    profile: tp.profile.clone(),
                    id: ts.id,
                    value: ts.value.clone(),
                });
            }
        }
        for cs in atuais {
            if !ids_alvo.contains(&cs.id) {
                ops.push(RestoreOp::Reset {
                    profile: tp.profile.clone(),
                    id: cs.id,
                    predefined_valid: cs.predefined_valid,
                });
            }
        }
    }
    RestorePlan { ops }
}

/// Aplica o plano na sessão e grava (`DRS_SaveSettings`). Se o driver recusar por
/// privilégio, [`Session::save`] devolve [`Error::PrivilegeRequired`] e nada chega ao disco
/// (no 621.40 ele grava mesmo sem administrador). Para no primeiro erro, sem gravar.
pub fn apply_restore(session: &Session<'_>, plan: &RestorePlan) -> Result<()> {
    // Cache de handles só desta chamada. O handle vale enquanto a sessão viver (ver
    // `drs::Profile`); nenhuma operação do plano apaga perfil nem recarrega a sessão.
    // Um nome só entra no cache depois de achado, e `CreateProfile` só aparece para nome
    // ausente (a busca anterior teria falhado e abortado), então o cache nunca fica velho.
    let mut cache: HashMap<Option<String>, crate::drs::Profile<'_>> = HashMap::new();
    let mut perfil = |r: &ProfileRef| -> Result<crate::drs::Profile<'_>> {
        let chave = match r {
            ProfileRef::Base => None,
            ProfileRef::Named(n) => Some(n.clone()),
        };
        if let Some(&p) = cache.get(&chave) {
            return Ok(p);
        }
        let p = match r {
            ProfileRef::Base => session.base_profile()?,
            ProfileRef::Named(n) => session.find_profile_by_name(n)?.ok_or(Error::Nvapi {
                function: "NvAPI_DRS_FindProfileByName",
                status: crate::error::Status::ProfileNotFound,
                message: format!("perfil “{n}” não encontrado"),
            })?,
        };
        cache.insert(chave, p);
        Ok(p)
    };
    for op in &plan.ops {
        match op {
            RestoreOp::CreateProfile { name } => {
                session.create_profile(name)?;
            }
            RestoreOp::CreateApplication { profile, exe } => {
                session.create_application(perfil(profile)?, exe, None)?;
            }
            RestoreOp::Set { profile, id, value } => {
                session.set_setting(perfil(profile)?, *id, value)?;
            }
            RestoreOp::Reset {
                profile,
                id,
                predefined_valid,
            } => {
                let p = perfil(profile)?;
                if *predefined_valid {
                    session.restore_setting_default(p, *id)?;
                } else {
                    session.delete_setting(p, *id)?;
                }
            }
        }
    }
    session.save()
}

/// Segundos Unix para ISO 8601 UTC (algoritmo `civil_from_days` de Howard Hinnant).
pub(crate) fn iso8601_utc(secs: u64) -> String {
    let dias = (secs / 86_400) as i64;
    let s = secs % 86_400;
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        s / 3600,
        (s / 60) % 60,
        s % 60
    )
}
