//! Testes de integração que **gravam** no DRS real. Ignorados por padrão e, mesmo com
//! `--ignored`, só agem com a variável de ambiente correspondente:
//!
//! ```text
//! # elevado (administrador): cria, lê de volta e apaga o perfil descartável
//! set NVT_TESTE_ESCRITA=1
//! cargo test -p nvt-drs --test escrita_admin -- --ignored --exact escrita_perfil_descartavel
//!
//! # NÃO elevado: SaveSettings recusado com PrivilegeRequired (-137) ou gravado
//! set NVT_TESTE_SEM_ADMIN=1
//! cargo test -p nvt-drs --test escrita_admin -- --ignored --exact save_sem_admin
//! ```
//!
//! Os dois só tocam o perfil `nv-tuner-teste-escrita` (aplicação
//! `nv-tuner-teste-escrita.exe`, que não existe), e um guard RAII apaga esse perfil e grava
//! mesmo se uma asserção falhar. Faça backup de `C:\ProgramData\NVIDIA Corporation\Drs`
//! antes de rodar (ver `docs\pesquisa\nvapi-drs.md`, seção 9).

use nvt_drs::{NvApi, Session, SettingLocation, SettingValue};

const PERFIL: &str = "nv-tuner-teste-escrita";
const EXE: &str = "nv-tuner-teste-escrita.exe";
/// `Power management - mode` (pública): 1 = máximo desempenho.
const PUBLICA: u32 = 0x1057_EB71;
/// `Ultra Low Latency - CPL State` (oculta da entrada pública, -160): 2 = Ultra.
const OCULTA: u32 = 0x0005_F543;

#[cfg(windows)]
mod token {
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn IsUserAnAdmin() -> i32;
    }

    /// `true` se o token tem o grupo Administradores habilitado (`CheckTokenMembership`).
    ///
    /// Não usa `TokenElevation`: `runas /trustlevel:0x20000` a partir de um processo
    /// elevado gera um token com integridade alta e `TokenElevation = 1`, mas com
    /// Administradores “apenas para negar” — e a NVAPI o trata como não administrador.
    pub fn elevado() -> bool {
        // SAFETY: sem argumentos.
        unsafe { IsUserAnAdmin() != 0 }
    }
}

#[cfg(not(windows))]
mod token {
    pub fn elevado() -> bool {
        false
    }
}

fn habilitado(var: &str) -> bool {
    if std::env::var(var).as_deref() == Ok("1") {
        true
    } else {
        eprintln!("{var} != 1: teste não executado");
        false
    }
}

fn api() -> NvApi {
    NvApi::load().unwrap_or_else(|e| panic!("NvApi::load: {e}"))
}

/// Apaga o perfil de teste (se existir) e grava, numa sessão nova. Roda no `Drop`, inclusive
/// durante um pânico.
struct Limpeza;

impl Drop for Limpeza {
    fn drop(&mut self) {
        let r = (|| -> nvt_drs::Result<bool> {
            let api = NvApi::load()?;
            let s = Session::open(&api)?;
            match s.find_profile_by_name(PERFIL)? {
                Some(p) => {
                    s.delete_profile(p)?;
                    s.save()?;
                    Ok(true)
                }
                None => Ok(false),
            }
        })();
        match r {
            Ok(true) => eprintln!("guard: perfil “{PERFIL}” apagado e gravado"),
            Ok(false) => {}
            Err(e) => eprintln!("guard: FALHA ao apagar “{PERFIL}”: {e} — remova à mão"),
        }
    }
}

/// Valor da setting gravado no próprio perfil (`None` se herdado ou ausente).
fn no_perfil(s: &Session<'_>, perfil: &str, id: u32) -> Option<SettingValue> {
    let p = s
        .find_profile_by_name(perfil)
        .expect("find_profile_by_name")
        .expect("perfil de teste");
    s.get_setting(p, id)
        .expect("get_setting")
        .filter(|t| t.location == SettingLocation::Current)
        .map(|t| t.value)
}

#[test]
#[ignore = "GRAVA no DRS real; exige administrador e NVT_TESTE_ESCRITA=1"]
fn escrita_perfil_descartavel() {
    if !habilitado("NVT_TESTE_ESCRITA") {
        return;
    }
    assert!(token::elevado(), "precisa de administrador");
    let api = api();

    {
        let s = Session::open(&api).expect("sessão");
        assert!(
            s.find_profile_by_name(PERFIL).expect("find").is_none(),
            "perfil “{PERFIL}” já existe: sobra de execução anterior? confira e apague à mão"
        );
        assert!(s.find_application(EXE).expect("find").is_none());
    }
    let _limpeza = Limpeza;

    // 1. Cria perfil + aplicação + uma setting pública e uma oculta, e grava.
    {
        let s = Session::open(&api).expect("sessão");
        let p = s.create_profile(PERFIL).expect("create_profile");
        s.create_application(p, EXE, None)
            .expect("create_application");
        s.set_setting_u32(p, PUBLICA, 1).expect("set pública");
        s.set_setting_u32(p, OCULTA, 2).expect("set oculta");
        s.save().expect("save 1");
    }

    // 2. Sessão nova: tudo voltou do disco.
    {
        let s = Session::open(&api).expect("sessão");
        let p = s
            .find_profile_by_name(PERFIL)
            .expect("find")
            .expect("criado");
        let info = s.profile_info(p).expect("info");
        assert_eq!(info.name, PERFIL);
        assert!(!info.is_predefined);
        assert_eq!(info.num_apps, 1);
        let (pa, app) = s.find_application(EXE).expect("find").expect("app gravada");
        assert_eq!(pa, p);
        assert!(app.app_name.eq_ignore_ascii_case(EXE), "{}", app.app_name);
        assert_eq!(no_perfil(&s, PERFIL, PUBLICA), Some(SettingValue::Dword(1)));
        assert_eq!(no_perfil(&s, PERFIL, OCULTA), Some(SettingValue::Dword(2)));
        let todas = s.enum_settings(p).expect("enum");
        for id in [PUBLICA, OCULTA] {
            let t = todas.iter().find(|t| t.id == id).expect("no enum");
            assert!(t.is_user_specified(), "0x{id:08X}: {t:?}");
        }

        // 3. restore_setting_default na pública e delete_setting na oculta.
        s.restore_setting_default(p, PUBLICA)
            .expect("restore_setting_default");
        s.delete_setting(p, OCULTA).expect("delete_setting");
        s.save().expect("save 2");
    }

    // 4. Sessão nova: as duas deixaram de estar gravadas no perfil.
    {
        let s = Session::open(&api).expect("sessão");
        assert_eq!(no_perfil(&s, PERFIL, PUBLICA), None);
        assert_eq!(no_perfil(&s, PERFIL, OCULTA), None);
        let p = s
            .find_profile_by_name(PERFIL)
            .expect("find")
            .expect("perfil");
        let todas = s.enum_settings(p).expect("enum");
        assert!(
            !todas.iter().any(|t| t.id == PUBLICA || t.id == OCULTA),
            "{todas:?}"
        );

        // 5. delete_application.
        s.delete_application(p, EXE).expect("delete_application");
        s.save().expect("save 3");
    }

    // 6. Sessão nova: sem a aplicação; apaga o perfil.
    {
        let s = Session::open(&api).expect("sessão");
        assert!(s.find_application(EXE).expect("find").is_none());
        let p = s
            .find_profile_by_name(PERFIL)
            .expect("find")
            .expect("perfil");
        assert_eq!(s.profile_info(p).expect("info").num_apps, 0);
        s.delete_profile(p).expect("delete_profile");
        s.save().expect("save 4");
    }

    // 7. Sessão nova: o perfil não existe mais.
    {
        let s = Session::open(&api).expect("sessão");
        assert!(s.find_profile_by_name(PERFIL).expect("find").is_none());
        assert!(s.find_application(EXE).expect("find").is_none());
    }
}

/// Sem administrador, `save()` com alteração no perfil descartável tem de: ou ser recusado
/// como [`nvt_drs::Error::PrivilegeRequired`] (-137), ou gravar de verdade — nunca outro
/// erro. No 621.40 (medido em 2026-11-25, processo de integridade média e Administradores
/// “apenas para negar”) o driver **grava**: a escrita passa pelo serviço
/// `NVDisplay.ContainerLocalSystem`. O teste aceita os dois resultados e imprime qual foi;
/// se gravou, confere a persistência e o guard apaga o perfil.
#[test]
#[ignore = "exige processo NÃO elevado e NVT_TESTE_SEM_ADMIN=1; pode gravar no DRS real"]
fn save_sem_admin() {
    if !habilitado("NVT_TESTE_SEM_ADMIN") {
        return;
    }
    assert!(!token::elevado(), "rode sem administrador");
    let api = api();
    {
        let s = Session::open(&api).expect("sessão");
        assert!(
            s.find_profile_by_name(PERFIL).expect("find").is_none(),
            "perfil “{PERFIL}” já existe: sobra de execução anterior? confira e apague à mão"
        );
    }

    let limpeza = Limpeza;
    let gravou = {
        let s = Session::open(&api).expect("sessão");
        let p = s.create_profile(PERFIL).expect("create_profile em memória");
        s.set_setting_u32(p, PUBLICA, 1).expect("set em memória");
        match s.save() {
            Ok(()) => true,
            Err(e) => {
                println!("save sem admin recusado: {e}");
                assert!(e.is_privilege(), "{e:?}");
                assert_eq!(e.code(), Some(-137));
                false
            }
        }
    };
    let s = Session::open(&api).expect("sessão");
    if gravou {
        println!("save sem admin ACEITO pelo driver: conferindo persistência");
        assert_eq!(no_perfil(&s, PERFIL, PUBLICA), Some(SettingValue::Dword(1)));
    } else {
        assert!(s.find_profile_by_name(PERFIL).expect("find").is_none());
    }
    drop(s);
    drop(limpeza);
    let s = Session::open(&api).expect("sessão");
    assert!(s.find_profile_by_name(PERFIL).expect("find").is_none());
}
