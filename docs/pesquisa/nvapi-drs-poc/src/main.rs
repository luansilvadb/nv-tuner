//! POC SOMENTE LEITURA do banco de perfis do driver NVIDIA (DRS) via NVAPI, com FFI própria.
//!
//! Sequência: LoadLibraryW("nvapi64.dll") → GetProcAddress("nvapi_QueryInterface") →
//! NvAPI_Initialize → versão do driver, GPU (nome, arquitetura) → DRS_CreateSession →
//! DRS_LoadSettings → perfil base + settings do global.nip → FindApplicationByName(WARDOGS) →
//! settings de DLSS → DRS_DestroySession → NvAPI_Unload.
//!
//! NUNCA chama DRS_SaveSettings, SetSetting, DeleteProfileSetting, CreateProfile,
//! CreateApplication nem RestoreProfileDefaultSetting. Não exige administrador.
//!
//! Uso: nvapi-drs-poc [caminho\global.nip] [caminho\wardogs-dlss-e.nip]

mod ffi;

use ffi::*;
use std::ffi::c_void;
use std::process::ExitCode;

const NIP_GLOBAL: &str = r"profiles\global.nip";
const NIP_WARDOGS: &str = r"profiles\wardogs-dlss-e.nip";
const EXE_WARDOGS: &str = "wardogsclient-win64-shipping.exe";

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

type QueryInterface = unsafe extern "C" fn(u32) -> *mut c_void;

struct NvApi {
    dll: *mut c_void,
    qi: QueryInterface,
}

impl NvApi {
    fn carregar() -> Result<Self, String> {
        let nome: Vec<u16> = "nvapi64.dll\0".encode_utf16().collect();
        // SAFETY: string terminada em zero; módulo liberado em Drop.
        let dll = unsafe { LoadLibraryW(nome.as_ptr()) };
        if dll.is_null() {
            return Err("nvapi64.dll não encontrada (driver NVIDIA ausente?)".into());
        }
        // SAFETY: nome ANSI terminado em zero.
        let p = unsafe { GetProcAddress(dll, c"nvapi_QueryInterface".as_ptr() as *const u8) };
        if p.is_null() {
            // SAFETY: dll válido.
            unsafe { FreeLibrary(dll) };
            return Err("nvapi64.dll sem a exportação nvapi_QueryInterface".into());
        }
        // SAFETY: a exportação tem a assinatura `void* __cdecl nvapi_QueryInterface(unsigned int)`.
        let qi = unsafe { std::mem::transmute::<*mut c_void, QueryInterface>(p) };
        Ok(Self { dll, qi })
    }

    fn ptr(&self, id: u32) -> Option<*mut c_void> {
        // SAFETY: qi é a função exportada pela DLL carregada.
        let p = unsafe { (self.qi)(id) };
        (!p.is_null()).then_some(p)
    }

    /// Resolve a função `id`; `T` deve ser o tipo de ponteiro de função correto para esse ID.
    fn f<T: Copy>(&self, id: u32) -> Option<T> {
        assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<*mut c_void>());
        // SAFETY: T é um `unsafe extern "C" fn` do mesmo tamanho de ponteiro (verificado acima).
        self.ptr(id)
            .map(|p| unsafe { std::mem::transmute_copy::<*mut c_void, T>(&p) })
    }

    fn erro(&self, st: NvStatus) -> String {
        let mut msg: NvShortString = [0; 64];
        if let Some(f) = self.f::<FnGetErrorMessage>(id::GET_ERROR_MESSAGE) {
            // SAFETY: buffer de 64 bytes conforme NvAPI_ShortString.
            unsafe { f(st, &mut msg) };
        }
        format!("{st} ({})", from_short(&msg))
    }
}

impl Drop for NvApi {
    fn drop(&mut self) {
        if let Some(f) = self.f::<FnVoid>(id::UNLOAD) {
            // SAFETY: sem argumentos.
            unsafe { f() };
        }
        // SAFETY: dll obtido de LoadLibraryW.
        unsafe { FreeLibrary(self.dll) };
    }
}

/// Setting esperada lida de um .nip (XML do NPI).
struct Esperada {
    nome: String,
    id: u32,
    valor: u32,
}

fn tag<'a>(bloco: &'a str, t: &str) -> Option<&'a str> {
    let a = format!("<{t}>");
    let f = format!("</{t}>");
    let i = bloco.find(&a)? + a.len();
    let j = bloco[i..].find(&f)? + i;
    Some(bloco[i..j].trim())
}

fn ler_nip(caminho: &str) -> Result<Vec<Esperada>, String> {
    let txt = std::fs::read_to_string(caminho).map_err(|e| format!("{caminho}: {e}"))?;
    let mut v = Vec::new();
    for bloco in txt.split("<ProfileSetting>").skip(1) {
        let id = tag(bloco, "SettingID").and_then(|s| s.parse().ok());
        let valor = tag(bloco, "SettingValue").and_then(|s| s.parse().ok());
        if let (Some(id), Some(valor)) = (id, valor) {
            let nome = tag(bloco, "SettingNameInfo").unwrap_or("").to_string();
            v.push(Esperada { nome, id, valor });
        }
    }
    Ok(v)
}

struct Drs<'a> {
    api: &'a NvApi,
    sessao: Handle,
    get_pub: FnGetSetting,
    get_int: Option<FnGetSettingInterno>,
    nome_int: Option<FnGetSettingNameFromId>,
    nome_pub: Option<FnGetSettingNameFromId>,
}

impl Drop for Drs<'_> {
    fn drop(&mut self) {
        if let Some(f) = self.api.f::<FnSession>(id::DRS_DESTROY_SESSION) {
            // SAFETY: sessão válida; sem DRS_SaveSettings, nada é gravado.
            unsafe { f(self.sessao) };
        }
    }
}

struct Lida {
    status_pub: NvStatus,
    status_int: NvStatus,
    s: Box<NvdrsSettingV1>,
}

impl Drs<'_> {
    /// Lê pela entrada interna (como o NPI) e também pela pública, para mostrar quem dá -160.
    fn ler(&self, perfil: Handle, sid: u32) -> Lida {
        let mut sp = NvdrsSettingV1::new();
        // SAFETY: estrutura com versão V1 preenchida, sessão e perfil válidos.
        let status_pub = unsafe { (self.get_pub)(self.sessao, perfil, sid, &mut *sp) };
        let mut si = NvdrsSettingV1::new();
        let status_int = match self.get_int {
            Some(f) => {
                let mut x = 0u32;
                // SAFETY: assinatura interna com 5º parâmetro de saída, como no NPI 3.0.2.1.
                unsafe { f(self.sessao, perfil, sid, &mut *si, &mut x) }
            }
            None => NVAPI_NO_IMPLEMENTATION,
        };
        let s = if status_int == NVAPI_OK { si } else { sp };
        Lida {
            status_pub,
            status_int,
            s,
        }
    }

    fn nome(&self, sid: u32) -> String {
        for f in [self.nome_int, self.nome_pub].into_iter().flatten() {
            let mut b = Box::new([0u16; NVAPI_UNICODE_STRING_MAX]);
            // SAFETY: buffer de 2048 u16.
            let st = unsafe { f(sid, &mut *b) };
            let n = from_wide(&b[..]).trim().to_string();
            if st == NVAPI_OK && !n.is_empty() {
                return n;
            }
        }
        String::new()
    }

    fn contar(&self, perfil: Handle, f: FnEnumSettings) -> Result<usize, NvStatus> {
        const LOTE: usize = 64;
        let mut buf: Vec<NvdrsSettingV1> = (0..LOTE).map(|_| *NvdrsSettingV1::new()).collect();
        let mut total = 0usize;
        loop {
            for s in buf.iter_mut() {
                *s = *NvdrsSettingV1::new();
            }
            let mut n = LOTE as u32;
            // SAFETY: vetor de LOTE estruturas V1 com versão preenchida.
            let st = unsafe { f(self.sessao, perfil, total as u32, &mut n, buf.as_mut_ptr()) };
            if st == NVAPI_END_ENUMERATION {
                return Ok(total);
            }
            if st != NVAPI_OK {
                return Err(st);
            }
            total += n as usize;
            if (n as usize) < LOTE {
                return Ok(total);
            }
        }
    }
}

fn local(l: u32) -> &'static str {
    match l {
        NVDRS_CURRENT_PROFILE_LOCATION => "atual",
        NVDRS_GLOBAL_PROFILE_LOCATION => "global",
        NVDRS_BASE_PROFILE_LOCATION => "base",
        NVDRS_DEFAULT_PROFILE_LOCATION => "padrão",
        _ => "?",
    }
}

fn arquitetura(a: u32) -> &'static str {
    match a {
        0x130 => "Pascal (GP100)",
        0x140 | 0x150 => "Volta (GV100/GV110)",
        0x160 => "Turing (TU100)",
        0x170 => "Ampere (GA100)",
        0x190 => "Ada Lovelace (AD100)",
        0x1B0 => "Blackwell (GB200)",
        _ => "desconhecida",
    }
}

/// Compara as settings esperadas com o que o driver devolve. Retorna (ok, divergentes).
fn comparar(drs: &Drs, perfil: Handle, esperadas: &[Esperada]) -> (usize, usize) {
    let (mut ok, mut dif) = (0, 0);
    println!(
        "  {:<10} {:<44} {:>10} {:>10}  {:<7} {:>5} {:>5}  resultado",
        "ID", "nome (driver / .nip)", "lido", "esperado", "local", "pub", "int"
    );
    for e in esperadas {
        let l = drs.ler(perfil, e.id);
        let nome_drv = drs.nome(e.id);
        let nome = if nome_drv.is_empty() {
            format!("(oculta) {}", e.nome)
        } else {
            nome_drv
        };
        let nome: String = nome.chars().take(44).collect();
        let (lido, loc, res) = if l.status_int == NVAPI_OK || l.status_pub == NVAPI_OK {
            let v = l.s.current_value.u32();
            let loc = local(l.s.setting_location);
            let res = if v == e.valor && l.s.setting_location == NVDRS_CURRENT_PROFILE_LOCATION {
                ok += 1;
                "OK"
            } else if v == e.valor {
                ok += 1;
                "OK (herdado)"
            } else {
                dif += 1;
                "DIVERGE"
            };
            (format!("0x{v:08X}"), loc, res)
        } else {
            dif += 1;
            ("—".to_string(), "—", "NÃO LIDA")
        };
        println!(
            "  0x{:08X} {:<44} {:>10} 0x{:08X}  {:<7} {:>5} {:>5}  {}",
            e.id, nome, lido, e.valor, loc, l.status_pub, l.status_int, res
        );
    }
    (ok, dif)
}

fn executar() -> Result<bool, String> {
    let args: Vec<String> = std::env::args().collect();
    let nip_global = args.get(1).map(String::as_str).unwrap_or(NIP_GLOBAL);
    let nip_wardogs = args.get(2).map(String::as_str).unwrap_or(NIP_WARDOGS);

    let api = NvApi::carregar()?;
    let init: FnVoid = api
        .f(id::INITIALIZE)
        .ok_or("NvAPI_Initialize indisponível")?;
    // SAFETY: sem argumentos.
    let st = unsafe { init() };
    if st != NVAPI_OK {
        return Err(format!("NvAPI_Initialize: {}", api.erro(st)));
    }

    // Driver
    if let Some(f) = api.f::<FnGetDriverAndBranch>(id::SYS_GET_DRIVER_AND_BRANCH_VERSION) {
        let (mut v, mut br) = (0u32, [0u8; 64]);
        // SAFETY: ponteiros válidos.
        let st = unsafe { f(&mut v, &mut br) };
        if st == NVAPI_OK {
            println!(
                "Driver: {}.{:02} (branch {})",
                v / 100,
                v % 100,
                from_short(&br)
            );
        } else {
            println!("Driver: erro {}", api.erro(st));
        }
    }

    // GPUs
    let enum_gpus: FnEnumPhysicalGpus = api
        .f(id::ENUM_PHYSICAL_GPUS)
        .ok_or("NvAPI_EnumPhysicalGPUs indisponível")?;
    let mut gpus: [Handle; NVAPI_MAX_PHYSICAL_GPUS] =
        [std::ptr::null_mut(); NVAPI_MAX_PHYSICAL_GPUS];
    let mut ngpu = 0u32;
    // SAFETY: vetor de 64 handles.
    let st = unsafe { enum_gpus(&mut gpus, &mut ngpu) };
    if st != NVAPI_OK {
        return Err(format!("NvAPI_EnumPhysicalGPUs: {}", api.erro(st)));
    }
    for (i, &g) in gpus.iter().take(ngpu as usize).enumerate() {
        let mut nome = [0u8; 64];
        if let Some(f) = api.f::<FnGpuGetFullName>(id::GPU_GET_FULL_NAME) {
            // SAFETY: handle de GPU e buffer de 64 bytes.
            unsafe { f(g, &mut nome) };
        }
        let mut arch = NvGpuArchInfoV2 {
            version: NvGpuArchInfoV2::VER,
            ..Default::default()
        };
        let st = match api.f::<FnGpuGetArchInfo>(id::GPU_GET_ARCH_INFO) {
            // SAFETY: estrutura V2 com versão preenchida.
            Some(f) => unsafe { f(g, &mut arch) },
            None => NVAPI_NO_IMPLEMENTATION,
        };
        if st == NVAPI_OK {
            println!(
                "GPU {i}: {} — arquitetura 0x{:X} {}, implementação 0x{:X}, revisão 0x{:X}",
                from_short(&nome),
                arch.architecture,
                arquitetura(arch.architecture),
                arch.implementation,
                arch.revision
            );
        } else {
            println!(
                "GPU {i}: {} — GetArchInfo erro {}",
                from_short(&nome),
                api.erro(st)
            );
        }
    }

    // Sessão DRS
    let criar: FnCreateSession = api.f(id::DRS_CREATE_SESSION).ok_or("DRS_CreateSession")?;
    let mut sessao: Handle = std::ptr::null_mut();
    // SAFETY: ponteiro de saída válido.
    let st = unsafe { criar(&mut sessao) };
    if st != NVAPI_OK {
        return Err(format!("DRS_CreateSession: {}", api.erro(st)));
    }
    let drs = Drs {
        api: &api,
        sessao,
        get_pub: api.f(id::DRS_GET_SETTING).ok_or("DRS_GetSetting")?,
        get_int: api.f(id::interno::DRS_GET_SETTING),
        nome_int: api.f(id::interno::DRS_GET_SETTING_NAME_FROM_ID),
        nome_pub: api.f(id::DRS_GET_SETTING_NAME_FROM_ID),
    };
    println!(
        "Entradas internas: GetSetting {}, GetSettingNameFromId {}, EnumSettings {}",
        if drs.get_int.is_some() {
            "presente"
        } else {
            "AUSENTE"
        },
        if drs.nome_int.is_some() {
            "presente"
        } else {
            "AUSENTE"
        },
        if api.ptr(id::interno::DRS_ENUM_SETTINGS).is_some() {
            "presente"
        } else {
            "AUSENTE"
        },
    );

    let load: FnSession = api.f(id::DRS_LOAD_SETTINGS).ok_or("DRS_LoadSettings")?;
    // SAFETY: sessão válida.
    let st = unsafe { load(sessao) };
    if st != NVAPI_OK {
        return Err(format!("DRS_LoadSettings: {}", api.erro(st)));
    }

    // Contagens: IDs disponíveis e settings do perfil base, pública vs interna.
    for (rot, idf) in [
        ("pública", id::DRS_ENUM_AVAILABLE_SETTING_IDS),
        ("interna", id::interno::DRS_ENUM_AVAILABLE_SETTING_IDS),
    ] {
        if let Some(f) = api.f::<FnEnumAvailableSettingIds>(idf) {
            let mut ids = vec![0u32; 8192];
            let mut n = ids.len() as u32;
            // SAFETY: vetor com capacidade n.
            let st = unsafe { f(ids.as_mut_ptr(), &mut n) };
            println!("EnumAvailableSettingIds ({rot}): status {st}, {n} IDs");
        }
    }

    // Perfil base
    let base_f: FnGetProfile = api
        .f(id::DRS_GET_BASE_PROFILE)
        .ok_or("DRS_GetBaseProfile")?;
    let mut base: Handle = std::ptr::null_mut();
    // SAFETY: sessão e saída válidas.
    let st = unsafe { base_f(sessao, &mut base) };
    if st != NVAPI_OK {
        return Err(format!("DRS_GetBaseProfile: {}", api.erro(st)));
    }
    let info_f: FnGetProfileInfo = api
        .f(id::DRS_GET_PROFILE_INFO)
        .ok_or("DRS_GetProfileInfo")?;
    let info = |h: Handle| -> Result<Box<NvdrsProfileV1>, String> {
        let mut p = Box::new(NvdrsProfileV1 {
            version: NvdrsProfileV1::VER,
            profile_name: [0; NVAPI_UNICODE_STRING_MAX],
            gpu_support: 0,
            is_predefined: 0,
            num_of_apps: 0,
            num_of_settings: 0,
        });
        // SAFETY: estrutura V1 com versão preenchida.
        let st = unsafe { info_f(sessao, h, &mut *p) };
        if st != NVAPI_OK {
            return Err(format!("DRS_GetProfileInfo: {}", api.erro(st)));
        }
        Ok(p)
    };
    let pb = info(base)?;
    let n_pub = api
        .f::<FnEnumSettings>(id::DRS_ENUM_SETTINGS)
        .map(|f| drs.contar(base, f));
    let n_int = api
        .f::<FnEnumSettings>(id::interno::DRS_ENUM_SETTINGS)
        .map(|f| drs.contar(base, f));
    println!(
        "\nPerfil base: \"{}\" (numOfSettings {}, predefinido {}); EnumSettings pública {:?}, interna {:?}",
        from_wide(&pb.profile_name),
        pb.num_of_settings,
        pb.is_predefined,
        n_pub,
        n_int
    );

    let mut tudo_ok = true;
    let esp_global = ler_nip(nip_global)?;
    println!("Settings do {nip_global} ({}):", esp_global.len());
    let (ok, dif) = comparar(&drs, base, &esp_global);
    println!("  => {ok} conferem, {dif} divergem/não lidas");
    tudo_ok &= dif == 0;

    // Perfil da aplicação
    let find_app: FnFindApplicationByName = api
        .f(id::DRS_FIND_APPLICATION_BY_NAME)
        .ok_or("DRS_FindApplicationByName")?;
    let nome_app = to_unicode_string(EXE_WARDOGS);
    let mut app = Box::new(NvdrsApplicationV4 {
        version: NvdrsApplicationV4::VER,
        is_predefined: 0,
        app_name: [0; NVAPI_UNICODE_STRING_MAX],
        user_friendly_name: [0; NVAPI_UNICODE_STRING_MAX],
        launcher: [0; NVAPI_UNICODE_STRING_MAX],
        file_in_folder: [0; NVAPI_UNICODE_STRING_MAX],
        flags: 0,
        command_line: [0; NVAPI_UNICODE_STRING_MAX],
    });
    let mut perfil_app: Handle = std::ptr::null_mut();
    // SAFETY: nome UTF-16 de 2048 u16, saída e estrutura V4 válidas.
    let st = unsafe { find_app(sessao, &*nome_app, &mut perfil_app, &mut *app) };
    if st != NVAPI_OK {
        println!(
            "\nFindApplicationByName(\"{EXE_WARDOGS}\"): {}",
            api.erro(st)
        );
        return Ok(false);
    }
    let pa = info(perfil_app)?;
    println!(
        "\nAplicação \"{}\" → perfil \"{}\" (apps {}, settings {}, predefinido {})",
        from_wide(&app.app_name),
        from_wide(&pa.profile_name),
        pa.num_of_apps,
        pa.num_of_settings,
        pa.is_predefined
    );
    let esp_dlss: Vec<Esperada> = ler_nip(nip_wardogs)?
        .into_iter()
        .filter(|e| [0x10E4_1E01, 0x0063_4291, 0x10E4_1DF3].contains(&e.id))
        .collect();
    println!("Settings de DLSS esperadas em {nip_wardogs}:");
    let (ok, dif) = comparar(&drs, perfil_app, &esp_dlss);
    println!("  => {ok} conferem, {dif} divergem/não lidas");
    tudo_ok &= dif == 0 && esp_dlss.len() == 3;

    println!("\nSessão destruída sem DRS_SaveSettings (nada gravado).");
    Ok(tudo_ok)
}

fn main() -> ExitCode {
    match executar() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => {
            eprintln!("Há divergências (ver acima).");
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("Erro: {e}");
            ExitCode::from(2)
        }
    }
}
