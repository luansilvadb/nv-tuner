use std::{io, mem::size_of, path::Path, ptr};

use windows_sys::{
    Wdk::System::SystemServices::RtlGetVersion,
    Win32::{
        Devices::DeviceAndDriverInstallation::{
            DIGCF_PRESENT, GUID_DEVCLASS_DISPLAY, HDEVINFO, SP_DEVINFO_DATA,
            SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
            SetupDiGetDeviceInstanceIdW,
        },
        Foundation::{ERROR_NO_MORE_ITEMS, GetLastError, LocalFree, TBS_E_TPM_NOT_FOUND},
        System::{
            LibraryLoader::{GetModuleHandleW, GetProcAddress},
            Power::PowerGetActiveScheme,
            SystemInformation::OSVERSIONINFOW,
            TpmBaseServices::{
                TBS_SUCCESS, TPM_DEVICE_INFO, TPM_VERSION_12, TPM_VERSION_20, Tbsi_GetDeviceInfo,
            },
        },
    },
    core::GUID,
};
use winreg::{
    HKEY, RegKey, RegValue,
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE},
};

use super::{ConsultaMaquina, ConsultaSessao, DeviceGuard, FonteDiagnosticos};
use crate::{
    registro::{DIRECTX, GLOBAL, dword, sz},
    token,
};

mod wmi;

#[derive(Debug, Default)]
pub struct DiagnosticosWindows;

impl DiagnosticosWindows {
    pub fn windows_build(&self) -> Option<u32> {
        let mut versao = OSVERSIONINFOW {
            dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
            ..Default::default()
        };
        // SAFETY: estrutura gravavel com tamanho inicializado.
        let status = unsafe { RtlGetVersion(&mut versao) };
        (status == 0 && versao.dwBuildNumber != 0).then_some(versao.dwBuildNumber)
    }
}

impl FonteDiagnosticos for DiagnosticosWindows {
    fn ler_sessao(&self, consulta: ConsultaSessao<'_>) -> Option<String> {
        match consulta {
            ConsultaSessao::GameMode => booleano(uint(
                HKEY_CURRENT_USER,
                r"Software\Microsoft\GameBar",
                "AutoGameModeEnabled",
            )?),
            ConsultaSessao::Vrr => {
                let raw = ler(HKEY_CURRENT_USER, DIRECTX, GLOBAL).ok()??;
                Some(
                    token::ler(&sz(&raw)?, "VRROptimizeEnable")
                        .ok()??
                        .to_owned(),
                )
            }
            ConsultaSessao::Fso(exe) => fso(exe),
        }
    }

    fn ler_maquina(&self, consulta: ConsultaMaquina) -> Option<String> {
        use ConsultaMaquina::*;
        match consulta {
            Hags => uint(
                HKEY_LOCAL_MACHINE,
                r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers",
                "HwSchMode",
            )
            .map(|n| n.to_string()),
            Mpo => uint(
                HKEY_LOCAL_MACHINE,
                r"SOFTWARE\Microsoft\Windows\Dwm",
                "OverlayTestMode",
            )
            .map(|n| n.to_string()),
            PlanoEnergia => energia(),
            CpuX3d => cpu(),
            MsiGpu => msi(),
            Tpm => tpm(),
            SecureBoot => booleano(uint(
                HKEY_LOCAL_MACHINE,
                r"SYSTEM\CurrentControlSet\Control\SecureBoot\State",
                "UEFISecureBootEnabled",
            )?),
            WindowsBuild => self.windows_build().map(|build| build.to_string()),
        }
    }

    fn ler_device_guard(&self) -> DeviceGuard {
        wmi::device_guard().unwrap_or_default()
    }
}

fn ler(raiz: HKEY, caminho: &str, nome: &str) -> io::Result<Option<RegValue<'static>>> {
    opcional(
        RegKey::predef(raiz)
            .open_subkey_with_flags(caminho, KEY_QUERY_VALUE)
            .and_then(|chave| chave.get_raw_value(nome)),
    )
}

fn opcional<T>(resultado: io::Result<T>) -> io::Result<Option<T>> {
    match resultado {
        Ok(valor) => Ok(Some(valor)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn uint(raiz: HKEY, caminho: &str, nome: &str) -> Option<u32> {
    dword(&ler(raiz, caminho, nome).ok()??)
}

fn booleano(n: u32) -> Option<String> {
    (n <= 1).then(|| n.to_string())
}

fn fso(exe: &str) -> Option<String> {
    if !Path::new(exe).is_absolute()
        || !Path::new(exe).extension()?.eq_ignore_ascii_case("exe")
        || exe.chars().any(char::is_control)
    {
        return None;
    }
    let bruto = ler(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers",
        exe,
    )
    .ok()?;
    let desativado = match bruto {
        None => false,
        Some(bruto) => sz(&bruto)?
            .split_ascii_whitespace()
            .any(|flag| flag.eq_ignore_ascii_case("DISABLEDXMAXIMIZEDWINDOWEDMODE")),
    };
    Some(format!("{},{exe}", u8::from(desativado)))
}

fn cpu() -> Option<String> {
    let nome = sz(&ler(
        HKEY_LOCAL_MACHINE,
        r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
        "ProcessorNameString",
    )
    .ok()??)?;
    let nome = nome.trim();
    if nome.is_empty() || nome.chars().any(char::is_control) {
        return None;
    }
    let maiusculo = nome.to_ascii_uppercase();
    let dual_ccd = maiusculo.split_ascii_whitespace().any(|palavra| {
        matches!(
            palavra,
            "7900X3D" | "7950X3D" | "9900X3D" | "9950X3D" | "9950X3D2"
        )
    });
    let servico = opcional(RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(
        r"SYSTEM\CurrentControlSet\Services\amd3dvcache",
        KEY_QUERY_VALUE,
    ))
    .ok()?
    .is_some();
    Some(format!(
        "{},{},{}",
        u8::from(dual_ccd),
        u8::from(servico),
        nome
    ))
}

fn msi() -> Option<String> {
    let dispositivos = DispositivosDisplay::presentes()?;
    let mut valores = Vec::new();
    for indice in 0.. {
        let Some(id) = dispositivos.id(indice).ok()? else {
            break;
        };
        if !id.to_ascii_uppercase().starts_with(r"PCI\VEN_10DE&") {
            continue;
        }
        let caminho = format!(
            r"SYSTEM\CurrentControlSet\Enum\{id}\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties"
        );
        let estado = match ler(HKEY_LOCAL_MACHINE, &caminho, "MSISupported").ok()? {
            Some(bruto) => booleano(dword(&bruto)?)?,
            None => "ausente".into(),
        };
        valores.push(format!("{id}={estado}"));
    }
    valores.sort_unstable();
    (!valores.is_empty()).then(|| valores.join(";"))
}

struct DispositivosDisplay(HDEVINFO);

impl DispositivosDisplay {
    fn presentes() -> Option<Self> {
        // SAFETY: GUID estatico da classe de display, sem enumerador ou janela.
        let conjunto = unsafe {
            SetupDiGetClassDevsW(
                &GUID_DEVCLASS_DISPLAY,
                ptr::null(),
                ptr::null_mut(),
                DIGCF_PRESENT,
            )
        };
        (conjunto != -1).then(|| Self(conjunto))
    }

    fn id(&self, indice: u32) -> Result<Option<String>, ()> {
        let mut info = SP_DEVINFO_DATA {
            cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        // SAFETY: conjunto valido e estrutura inicializada para receber o dispositivo.
        if unsafe { SetupDiEnumDeviceInfo(self.0, indice, &mut info) } == 0 {
            // SAFETY: le o ultimo erro da chamada na mesma thread.
            return if unsafe { GetLastError() } == ERROR_NO_MORE_ITEMS {
                Ok(None)
            } else {
                Err(())
            };
        }
        let mut id = [0; 200]; // MAX_DEVICE_ID_LEN, incluindo o terminador NUL.
        // SAFETY: info veio deste conjunto e o comprimento esta em unidades UTF-16.
        if unsafe {
            SetupDiGetDeviceInstanceIdW(
                self.0,
                &info,
                id.as_mut_ptr(),
                id.len() as u32,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(());
        }
        let fim = id.iter().position(|&u| u == 0).ok_or(())?;
        String::from_utf16(&id[..fim]).map(Some).map_err(|_| ())
    }
}

impl Drop for DispositivosDisplay {
    fn drop(&mut self) {
        // SAFETY: a guarda possui um conjunto retornado com sucesso e o libera uma vez.
        unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

fn tpm() -> Option<String> {
    let mut info = TPM_DEVICE_INFO::default();
    // SAFETY: buffer gravavel do tamanho exato de TPM_DEVICE_INFO.
    let status = unsafe {
        Tbsi_GetDeviceInfo(
            size_of::<TPM_DEVICE_INFO>() as u32,
            ptr::from_mut(&mut info).cast(),
        )
    };
    match (status, info.tpmVersion) {
        (TBS_SUCCESS, TPM_VERSION_12) => Some("1.2".into()),
        (TBS_SUCCESS, TPM_VERSION_20) => Some("2.0".into()),
        (status, _) if status == TBS_E_TPM_NOT_FOUND as u32 => Some("ausente".into()),
        _ => None,
    }
}

fn energia() -> Option<String> {
    let mut guid: *mut GUID = ptr::null_mut();
    // SAFETY: a saida recebe um GUID alocado pela API e liberado com LocalFree.
    if unsafe { PowerGetActiveScheme(ptr::null_mut(), &mut guid) } != 0 || guid.is_null() {
        return None;
    }
    // SAFETY: sucesso e ponteiro nao nulo identificam um GUID valido.
    let plano = unsafe {
        let plano = formatar_guid(*guid);
        LocalFree(guid.cast());
        plano
    };
    Some(match overlay() {
        Some(overlay) => format!("{plano},{}", formatar_guid(overlay)),
        None => plano,
    })
}

fn overlay() -> Option<GUID> {
    // SAFETY: o modulo esta ligado por PowerGetActiveScheme. Os nomes sao NUL
    // terminados e o export, se presente, recebe um unico ponteiro para GUID.
    unsafe {
        let modulo = GetModuleHandleW(windows_sys::w!("powrprof.dll"));
        if modulo.is_null() {
            return None;
        }
        let export = GetProcAddress(modulo, c"PowerGetActualOverlayScheme".as_ptr().cast())?;
        let consultar: unsafe extern "system" fn(*mut GUID) -> u32 = std::mem::transmute(export);
        let mut guid = GUID::from_u128(0);
        (consultar(&mut guid) == 0).then_some(guid)
    }
}

fn formatar_guid(guid: GUID) -> String {
    let GUID {
        data1,
        data2,
        data3,
        data4,
    } = guid;
    let [a, b, c, d, e, f, g, h] = data4;
    format!(
        "{data1:08x}-{data2:04x}-{data3:04x}-{a:02x}{b:02x}-{c:02x}{d:02x}{e:02x}{f:02x}{g:02x}{h:02x}"
    )
}
