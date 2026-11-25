//! Imprime em JSON o estado completo do perfil base e do perfil de um executável
//! (somente leitura), para comparar o DRS antes e depois de um teste de escrita.
//!
//! ```text
//! cargo run -p nvt-drs --example estado -- [exe] > estado.json
//! ```
//!
//! Sem argumento usa `wardogsclient-win64-shipping.exe`. Inclui todas as settings do perfil
//! (não só as do usuário), os executáveis, o total de perfis, a existência do perfil de
//! teste `nv-tuner-teste-escrita` e o [`nvt_drs::capture`] dos mesmos perfis.

use std::process::ExitCode;

use nvt_drs::{Profile, Session, capture};
use serde_json::{Value, json};

const PERFIL_TESTE: &str = "nv-tuner-teste-escrita";

fn perfil(s: &Session<'_>, p: Profile<'_>) -> nvt_drs::Result<Value> {
    let info = s.profile_info(p)?;
    let settings: Vec<Value> = s
        .enum_settings(p)?
        .into_iter()
        .map(|t| {
            json!({
                "id": format!("0x{:08X}", t.id),
                "nome": t.name,
                "location": t.location,
                "is_current_predefined": t.is_current_predefined,
                "predefined": t.predefined,
                "value": t.value,
            })
        })
        .collect();
    let mut apps: Vec<String> = s
        .enum_applications(p)?
        .into_iter()
        .map(|a| a.app_name)
        .collect();
    apps.sort();
    Ok(json!({
        "nome": info.name,
        "is_predefined": info.is_predefined,
        "gpu_support": info.gpu_support,
        "num_apps": info.num_apps,
        "num_settings": info.num_settings,
        "aplicacoes": apps,
        "settings": settings,
    }))
}

fn executar() -> nvt_drs::Result<()> {
    let exe = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "wardogsclient-win64-shipping.exe".into());
    let api = nvt_drs::NvApi::load()?;
    let s = Session::open(&api)?;
    let base = perfil(&s, s.base_profile()?)?;
    let app = match s.find_application(&exe)? {
        Some((p, _)) => perfil(&s, p)?,
        None => Value::Null,
    };
    let mut snap = serde_json::to_value(capture(&s, &[exe.as_str()])?)
        .map_err(|e| nvt_drs::Error::Snapshot(e.to_string()))?;
    // A data da captura muda a cada execução; fora dela o JSON deve ser idêntico.
    if let Some(o) = snap.as_object_mut() {
        o.remove("captured_at");
        o.remove("captured_at_unix");
    }
    let saida = json!({
        "total_perfis": s.enum_profiles()?.len(),
        "perfil_teste_existe": s.find_profile_by_name(PERFIL_TESTE)?.is_some(),
        "app_teste_existe": s.find_application(&format!("{PERFIL_TESTE}.exe"))?.is_some(),
        "base": base,
        "aplicacao": { "exe": exe, "perfil": app },
        "snapshot": snap,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&saida)
            .map_err(|e| nvt_drs::Error::Snapshot(e.to_string()))?
    );
    Ok(())
}

fn main() -> ExitCode {
    match executar() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Erro: {e}");
            ExitCode::FAILURE
        }
    }
}
