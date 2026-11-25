//! Imprime driver, GPUs e as settings gravadas no perfil base (somente leitura).
//!
//! ```text
//! cargo run -p nvt-drs --example dump
//! ```

use std::process::ExitCode;

use nvt_drs::{NvApi, Session, SettingValue};

fn executar() -> nvt_drs::Result<()> {
    let api = NvApi::load()?;
    let v = api.driver_version()?;
    println!("Driver: {v} (branch {})", v.branch);
    for g in api.physical_gpus()? {
        let arch = g
            .arch
            .map(|a| {
                format!(
                    "arquitetura 0x{:X} {}, implementação 0x{:X}, revisão 0x{:X}",
                    a.architecture,
                    a.architecture_name().unwrap_or("desconhecida"),
                    a.implementation,
                    a.revision
                )
            })
            .unwrap_or_else(|| "arquitetura indisponível".into());
        let vram = g
            .vram_kib
            .map(|k| format!("{} MiB", k / 1024))
            .unwrap_or_else(|| "?".into());
        println!("GPU {}: {} — {arch}, VRAM {vram}", g.index, g.name);
    }
    println!("Entradas DRS: {:?}", api.drs_entry_points());

    let s = Session::open(&api)?;
    let base = s.base_profile()?;
    let info = s.profile_info(base)?;
    println!(
        "\nPerfil base “{}”: {} settings, predefinido {}",
        info.name, info.num_settings, info.is_predefined
    );
    for st in s.enum_settings(base)? {
        let valor = match &st.value {
            SettingValue::Dword(v) => format!("0x{v:08X}"),
            outro => format!("{outro:?}"),
        };
        let nome = if st.name.is_empty() {
            api.setting_name(st.id).unwrap_or_else(|| "(oculta)".into())
        } else {
            st.name.clone()
        };
        println!(
            "  0x{:08X} {:<50} {:>12} {}",
            st.id,
            nome,
            valor,
            if st.is_user_specified() {
                "usuário"
            } else {
                "predefinido"
            }
        );
    }
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
