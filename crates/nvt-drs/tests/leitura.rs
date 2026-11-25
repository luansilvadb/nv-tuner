//! Testes de integração SOMENTE LEITURA contra o driver instalado.
//!
//! Ignorados por padrão (dependem da máquina de referência: RTX 2060 SUPER, driver 621.40,
//! perfil base e WARDOGS conforme `nvidia-inspector\profiles\*.nip`). Rodar com:
//!
//! ```text
//! cargo test -p nvt-drs -- --ignored
//! ```
//!
//! Nenhum teste chama `set_setting`, `delete_setting`, `restore_setting_default`,
//! `create_profile`, `create_application`, `save` ou `apply_restore`.

use nvt_drs::{ArchInfo, NvApi, ProfileRef, Session, SettingLocation, SettingValue, capture};

const EXE_WARDOGS: &str = "wardogsclient-win64-shipping.exe";

/// As 20 settings do `global.nip` (ID e valor em decimal, como no arquivo).
const GLOBAL: [(&str, u32, u32); 20] = [
    ("Vertical Sync", 11_041_231, 138_504_007),
    ("Vertical Sync - Tear Control", 5_912_412, 2_525_368_439),
    ("Ultra Low Latency - CPL State", 390_467, 2),
    ("Ultra Low Latency - Enabled", 277_041_152, 1),
    ("Frame Rate Limiter V3", 277_041_154, 0),
    ("Power Management - Mode", 274_197_361, 1),
    ("GSYNC - Global Feature", 278_196_567, 0),
    ("GSYNC - Application Mode", 294_973_784, 2),
    ("GSYNC - Application State", 279_476_687, 1),
    ("Variable Refresh Rate - Control", 279_476_686, 1),
    ("Shader Cache - Enabled", 1_675_263, 1),
    ("Shader Cache - Cache Size", 11_306_135, 16_384),
    ("Variable Refresh Rate - Overlay Indicator", 278_262_127, 0),
    ("Anisotropic Filtering - Mode", 282_245_910, 0),
    ("Anisotropic Filter - Optimization", 8_703_344, 0),
    ("Anisotropic Filter - Sample Optimization", 15_151_633, 0),
    ("Antialiasing - Mode", 276_757_595, 0),
    ("Indicator MFAA", 6_710_885, 0),
    ("Indicator Triple Buffering", 6_776_373, 0),
    ("Platform Boost", 277_041_150, 0),
];

fn api() -> NvApi {
    match NvApi::load() {
        Ok(a) => a,
        Err(e) => panic!("NvApi::load: {e}"),
    }
}

#[test]
#[ignore = "exige driver NVIDIA (somente leitura)"]
fn versao_do_driver() {
    let api = api();
    let v = api.driver_version().expect("driver_version");
    println!("driver {v} ({})", v.branch);
    assert!(v.raw >= 50000, "versão improvável: {}", v.raw);
    assert_eq!(v.to_string(), "621.40");
    assert_eq!(v.branch, "r621_20");
}

#[test]
#[ignore = "exige driver NVIDIA (somente leitura)"]
fn gpu_turing() {
    let api = api();
    let gpus = api.physical_gpus().expect("physical_gpus");
    assert!(!gpus.is_empty());
    let g = &gpus[0];
    println!("{g:?}");
    assert!(g.name.contains("RTX 2060 SUPER"), "{}", g.name);
    let arch = g.arch.expect("arch info");
    assert_eq!(arch.architecture, ArchInfo::TURING);
    assert_eq!(arch.architecture_name(), Some("Turing"));
    assert_eq!(arch.implementation, 0x06);
    let vram = g.vram_kib.expect("VRAM via NvAPI_GPU_GetMemoryInfo");
    assert_eq!(vram / 1024, 8192);
}

#[test]
#[ignore = "exige driver NVIDIA (somente leitura)"]
fn perfil_base_20_settings() {
    let api = api();
    let s = Session::open(&api).expect("sessão");
    let base = s.base_profile().expect("perfil base");
    let info = s.profile_info(base).expect("info");
    assert_eq!(info.name, "Base Profile");
    let mut divergentes = Vec::new();
    for (nome, id, esperado) in GLOBAL {
        match s.get_setting(base, id).expect("get_setting") {
            Some(st)
                if st.value == SettingValue::Dword(esperado)
                    && st.location == SettingLocation::Current => {}
            outro => divergentes.push(format!("{nome} 0x{id:08X}: {outro:?}")),
        }
    }
    assert!(divergentes.is_empty(), "{divergentes:#?}");

    // A enumeração interna inclui as ocultas (93 no 621.40, 90 pela pública).
    let todas = s.enum_settings(base).expect("enum_settings");
    assert_eq!(todas.len() as u32, info.num_settings);
    for (_, id, _) in GLOBAL {
        assert!(todas.iter().any(|t| t.id == id), "0x{id:08X} fora do enum");
    }
}

#[test]
#[ignore = "exige driver NVIDIA (somente leitura)"]
fn wardogs_dlss() {
    let api = api();
    let s = Session::open(&api).expect("sessão");
    let (p, app) = s
        .find_application(EXE_WARDOGS)
        .expect("find_application")
        .expect("WARDOGS no DRS");
    assert!(app.app_name.eq_ignore_ascii_case(EXE_WARDOGS));
    let info = s.profile_info(p).expect("info");
    assert_eq!(info.name, "WARDOGS");
    for (id, esperado) in [(0x10E4_1E01, 1), (0x0063_4291, 2), (0x10E4_1DF3, 5)] {
        let st = s.get_setting(p, id).expect("get_setting").expect("gravada");
        assert_eq!(st.value, SettingValue::Dword(esperado), "0x{id:08X}");
        assert_eq!(st.location, SettingLocation::Current, "0x{id:08X}");
    }
    let apps = s.enum_applications(p).expect("enum_applications");
    assert_eq!(apps.len() as u32, info.num_apps);
    assert!(
        s.enum_applications(s.base_profile().expect("base"))
            .expect("base")
            .is_empty()
    );
    assert_eq!(
        s.find_application("nao-existe-nvtuner.exe").expect("ok"),
        None
    );
    assert_eq!(
        s.find_profile_by_name("nao existe nvtuner").expect("ok"),
        None
    );
    assert!(s.find_profile_by_name("WARDOGS").expect("ok").is_some());
}

#[test]
#[ignore = "exige driver NVIDIA (somente leitura)"]
fn snapshot_e_plano_vazio() {
    let api = api();
    let s = Session::open(&api).expect("sessão");
    let a = capture(&s, &[EXE_WARDOGS, "nao-existe-nvtuner.exe"]).expect("capture");
    assert_eq!(a.driver_version, 62140);
    assert_eq!(a.profiles[0].profile, ProfileRef::Base);
    assert!(a.profiles[0].settings.len() >= 20);
    assert_eq!(a.profiles[1].profile, ProfileRef::Named("WARDOGS".into()));
    assert!(a.profiles[1].settings.iter().any(|t| t.id == 0x0063_4291));
    assert_eq!(a.missing_applications, vec!["nao-existe-nvtuner.exe"]);
    let j = a.to_json().expect("json");
    let b = nvt_drs::Snapshot::from_json(&j).expect("volta");
    assert!(nvt_drs::restore_plan(&b, &a).is_empty());
    let profiles = s.enum_profiles().expect("enum_profiles");
    assert!(profiles.len() > 1000, "{} perfis", profiles.len());
    let ep = api.drs_entry_points();
    println!("{ep:?}");
    assert_eq!(ep.get_setting, Some(nvt_drs::EntryOrigin::Internal));
}
