//! Testes unitários (sem driver).

use std::mem::{offset_of, size_of};

use crate::drs::{Setting, SettingLocation, SettingValue};
use crate::error::{Error, Status};
use crate::ffi::{self, NvdrsSettingV1, NvdrsValue};
use crate::snapshot::{
    ProfileRef, ProfileSnapshot, RestoreOp, SettingSnapshot, Snapshot, iso8601_utc, restore_plan,
};
use crate::strings::{from_short, from_wide, to_unicode_string};

#[test]
fn layouts_e_versoes() {
    assert_eq!(size_of::<NvdrsSettingV1>(), 12320);
    assert_eq!(NvdrsSettingV1::VER, 0x0001_3020);
    assert_eq!(offset_of!(NvdrsSettingV1, setting_name), 4);
    assert_eq!(offset_of!(NvdrsSettingV1, setting_type), 4104);
    assert_eq!(offset_of!(NvdrsSettingV1, setting_location), 4108);
    assert_eq!(offset_of!(NvdrsSettingV1, is_current_predefined), 4112);
    assert_eq!(offset_of!(NvdrsSettingV1, is_predefined_valid), 4116);
    assert_eq!(size_of::<ffi::NvdrsProfileV1>(), 4116);
    assert_eq!(ffi::NvdrsProfileV1::VER, 0x0001_1014);
    assert_eq!(offset_of!(ffi::NvdrsProfileV1, num_of_settings), 4112);
    assert_eq!(size_of::<ffi::NvdrsApplicationV4>(), 20492);
    assert_eq!(ffi::NvdrsApplicationV4::VER, 0x0004_500C);
    assert_eq!(offset_of!(ffi::NvdrsApplicationV4, command_line), 16396);
    assert_eq!(ffi::NvGpuArchInfoV2::VER, 0x0002_0010);
    assert_eq!(NvdrsSettingV1::ZERO.version, NvdrsSettingV1::VER);
}

#[test]
fn utf16_ida_e_volta() {
    let b = to_unicode_string("Base Profile — ação").expect("cabe");
    assert_eq!(b.len(), 2048);
    assert_eq!(from_wide(&b[..]), "Base Profile — ação");
    // Caractere fora do BMP vira par substituto e volta igual.
    let b = to_unicode_string("jogo 🎮.exe").expect("cabe");
    assert_eq!(from_wide(&b[..]), "jogo 🎮.exe");
    // Sem terminador: usa o buffer inteiro.
    assert_eq!(from_wide(&[0x61, 0x62]), "ab");
    // Substituto solto não entra em pânico.
    assert_eq!(from_wide(&[0xD800, 0x61, 0]), "\u{FFFD}a");
}

#[test]
fn utf16_limites() {
    let max = "a".repeat(2047);
    let b = to_unicode_string(&max).expect("2047 cabe");
    assert_eq!(b[2047], 0);
    assert!(matches!(
        to_unicode_string(&"a".repeat(2048)),
        Err(Error::InvalidString { .. })
    ));
    assert!(matches!(
        to_unicode_string("a\0b"),
        Err(Error::InvalidString { .. })
    ));
}

#[test]
fn short_string() {
    let mut s = [0u8; 64];
    s[..7].copy_from_slice(b"r621_20");
    assert_eq!(from_short(&s), "r621_20");
    assert_eq!(from_short(&[b'x'; 64]), "x".repeat(64));
}

#[test]
fn mapeamento_de_erros() {
    for c in [
        -1, -2, -3, -4, -5, -6, -7, -8, -9, -14, -137, -160, -161, -162, -163, -164, -165, -166,
        -167, -168, -169, -182, -999,
    ] {
        assert_eq!(Status::from_code(c).code(), c);
    }
    assert_eq!(Status::from_code(-160), Status::SettingNotFound);
    assert_eq!(Status::from_code(-7).name(), "NVAPI_END_ENUMERATION");
    assert_eq!(Status::from_code(-999), Status::Other(-999));

    let e = Error::from_status("NvAPI_DRS_SaveSettings", -137, "msg".into());
    assert!(e.is_privilege());
    assert_eq!(e.code(), Some(-137));
    assert!(e.to_string().contains("administrador"));

    let e = Error::from_status("NvAPI_DRS_GetSetting", -9, "versão".into());
    assert_eq!(
        e,
        Error::Nvapi {
            function: "NvAPI_DRS_GetSetting",
            status: Status::IncompatibleStructVersion,
            message: "versão".into()
        }
    );
    assert_eq!(e.code(), Some(-9));
    assert!(
        e.to_string()
            .contains("-9 NVAPI_INCOMPATIBLE_STRUCT_VERSION")
    );
    assert_eq!(Error::DllNotFound.code(), None);
}

#[test]
fn valores_da_uniao() {
    let mut v = NvdrsValue::ZERO;
    v.set_u64(0x1122_3344_5566_7788);
    assert_eq!(v.u64(), 0x1122_3344_5566_7788);
    v.set_binary(&[1, 2, 3, 4, 5]);
    assert_eq!(v.raw[0], 5);
    assert_eq!(v.binary(), vec![1, 2, 3, 4, 5]);
    v.set_wide(&to_unicode_string("ção").expect("cabe"));
    assert_eq!(v.wstr(), "ção");
    v.set_u32(1.5f32.to_bits());
    assert_eq!(v.f32(), 1.5);
}

#[test]
fn setting_user_specified() {
    let mut raw = NvdrsSettingV1::ZERO;
    raw.setting_id = 0x10E4_1DF3;
    raw.setting_type = ffi::NVDRS_DWORD_TYPE;
    raw.current_value.set_u32(5);
    raw.is_predefined_valid = 1;
    raw.predefined_value.set_u32(0);
    let s = Setting::from_raw(&raw).expect("tipo conhecido");
    assert_eq!(s.value, SettingValue::Dword(5));
    assert_eq!(s.predefined, Some(SettingValue::Dword(0)));
    assert_eq!(s.location, SettingLocation::Current);
    assert!(s.is_user_specified());

    raw.is_current_predefined = 1;
    assert!(!Setting::from_raw(&raw).expect("ok").is_user_specified());
    raw.is_current_predefined = 0;
    raw.setting_location = 2;
    assert!(!Setting::from_raw(&raw).expect("ok").is_user_specified());
    raw.setting_type = 99;
    assert!(Setting::from_raw(&raw).is_none());
}

#[test]
fn data_iso() {
    assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00Z");
    assert_eq!(iso8601_utc(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(iso8601_utc(1_795_620_645), "2026-11-25T15:30:45Z");
}

fn snap(profiles: Vec<ProfileSnapshot>) -> Snapshot {
    Snapshot {
        format: crate::SNAPSHOT_FORMAT,
        driver_version: 62140,
        driver_branch: "r621_20".into(),
        captured_at: iso8601_utc(0),
        captured_at_unix: 0,
        profiles,
        missing_applications: vec![],
    }
}

fn st(id: u32, v: u32, predefined_valid: bool) -> SettingSnapshot {
    SettingSnapshot {
        id,
        name: String::new(),
        value: SettingValue::Dword(v),
        predefined_valid,
    }
}

fn perfil(r: ProfileRef, apps: &[&str], settings: Vec<SettingSnapshot>) -> ProfileSnapshot {
    ProfileSnapshot {
        name: match &r {
            ProfileRef::Base => "Base Profile".into(),
            ProfileRef::Named(n) => n.clone(),
        },
        profile: r,
        is_predefined: false,
        applications: apps.iter().map(|s| s.to_string()).collect(),
        settings,
    }
}

#[test]
fn json_ida_e_volta() {
    let s = snap(vec![
        perfil(ProfileRef::Base, &[], vec![st(0x0005_F543, 2, false)]),
        perfil(
            ProfileRef::Named("WARDOGS".into()),
            &["wardogsclient-win64-shipping.exe"],
            vec![SettingSnapshot {
                id: 1,
                name: "x".into(),
                value: SettingValue::WString("ção".into()),
                predefined_valid: true,
            }],
        ),
    ]);
    let j = s.to_json().expect("serializa");
    assert!(j.contains("\"tipo\": \"Dword\""));
    assert_eq!(Snapshot::from_json(&j).expect("desserializa"), s);
    let outro = j.replace("\"format\": 1", "\"format\": 9");
    assert!(matches!(
        Snapshot::from_json(&outro),
        Err(Error::Snapshot(_))
    ));
}

#[test]
fn plano_de_restauracao() {
    let wd = ProfileRef::Named("WARDOGS".into());
    let alvo = snap(vec![
        perfil(
            ProfileRef::Base,
            &[],
            vec![st(10, 1, false), st(11, 2, false)],
        ),
        perfil(wd.clone(), &["w.exe"], vec![st(20, 5, true)]),
        perfil(
            ProfileRef::Named("Novo".into()),
            &["n.exe"],
            vec![st(30, 7, false)],
        ),
    ]);
    let atual = snap(vec![
        perfil(
            ProfileRef::Base,
            &[],
            vec![st(10, 1, false), st(11, 3, false), st(12, 4, true)],
        ),
        perfil(wd.clone(), &["W.EXE"], vec![st(21, 1, false)]),
    ]);
    let plano = restore_plan(&alvo, &atual);
    assert_eq!(
        plano.ops,
        vec![
            RestoreOp::Set {
                profile: ProfileRef::Base,
                id: 11,
                value: SettingValue::Dword(2)
            },
            RestoreOp::Reset {
                profile: ProfileRef::Base,
                id: 12,
                predefined_valid: true
            },
            RestoreOp::Set {
                profile: wd.clone(),
                id: 20,
                value: SettingValue::Dword(5)
            },
            RestoreOp::Reset {
                profile: wd,
                id: 21,
                predefined_valid: false
            },
            RestoreOp::CreateProfile {
                name: "Novo".into()
            },
            RestoreOp::CreateApplication {
                profile: ProfileRef::Named("Novo".into()),
                exe: "n.exe".into()
            },
            RestoreOp::Set {
                profile: ProfileRef::Named("Novo".into()),
                id: 30,
                value: SettingValue::Dword(7)
            },
        ]
    );
    assert!(restore_plan(&alvo, &alvo).is_empty());
}
