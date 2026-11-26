//! Fachada real, somente leitura: registro (só `KEY_READ`), pastas Inicializar,
//! Agendador de Tarefas por COM em processo e contadores do sistema.

use std::{ffi::OsString, io, mem, os::windows::ffi::OsStringExt, path::PathBuf, ptr};

use windows::{
    Win32::{
        Foundation::{RPC_E_CHANGED_MODE, VARIANT_FALSE},
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize, IPersistFile, STGM_READ,
            },
            TaskScheduler::{
                IExecAction, IRegisteredTask, ITaskFolder, ITaskService, TASK_ACTION_EXEC,
                TASK_ACTION_TYPE, TASK_ENUM_HIDDEN, TASK_TRIGGER_BOOT, TASK_TRIGGER_LOGON,
                TASK_TRIGGER_TYPE2, TaskScheduler,
            },
        },
        UI::Shell::{IShellLinkW, SLGP_RAWPATH, ShellLink},
    },
    core::{BSTR, HSTRING, Interface, PCWSTR, VARIANT},
};
use windows_sys::Win32::{
    Foundation::{FILETIME, LocalFree},
    Storage::FileSystem::GetDiskFreeSpaceExW,
    System::{
        Com::CoTaskMemFree,
        Power::PowerGetActiveScheme,
        SystemInformation::{GetSystemWindowsDirectoryW, GlobalMemoryStatusEx, MEMORYSTATUSEX},
        Threading::GetSystemTimes,
    },
    UI::Shell::{FOLDERID_CommonStartup, FOLDERID_Startup, SHGetKnownFolderPath},
};
use winreg::{
    RegKey,
    enums::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, REG_BINARY, REG_DWORD,
        REG_EXPAND_SZ, REG_SZ,
    },
};

use super::{
    inicio::{Aprovacao, ArquivoPasta, ChaveRun, LeitorInicio, PastaInicio, TarefaBruta},
    sinais::{LeitorSinaisMaquina, LeitorSinaisSessao, LeituraDword, TemposCpu},
};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_ONCE: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";
const RUN_32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APROVADO: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";
const STORAGE: &str =
    r"Software\Microsoft\Windows\CurrentVersion\StorageSense\Parameters\StoragePolicy";
const CLASSE_VIDEO: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
/// Limites de varredura (custo previsível).
const VALORES_MAX: usize = 512;
const TAREFAS_MAX: usize = 4096;
const PROFUNDIDADE_MAX: usize = 16;

/// Fachada real. Implementa todos os leitores; não tem estado.
#[derive(Debug, Default, Clone, Copy)]
pub struct SaudeWindows;

/// `Ok(None)` = chave ausente.
fn abrir(raiz: isize, caminho: &str) -> Option<Option<RegKey>> {
    let raiz = RegKey::predef(raiz as _);
    match raiz.open_subkey_with_flags(caminho, KEY_READ | KEY_WOW64_64KEY) {
        Ok(c) => Some(Some(c)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Some(None),
        Err(_) => None,
    }
}

fn utf16(bytes: &[u8]) -> Option<String> {
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut u: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    while u.last() == Some(&0) {
        u.pop();
    }
    String::from_utf16(&u).ok()
}

fn valores<T>(
    raiz: isize,
    caminho: &str,
    mut f: impl FnMut(&winreg::RegValue) -> Option<T>,
) -> Option<Vec<(String, T)>> {
    let Some(chave) = abrir(raiz, caminho)? else {
        return Some(vec![]);
    };
    let mut saida = Vec::new();
    for item in chave.enum_values().take(VALORES_MAX) {
        let Ok((nome, valor)) = item else { continue };
        if let Some(v) = f(&valor) {
            saida.push((nome, v));
        }
    }
    Some(saida)
}

impl LeitorInicio for SaudeWindows {
    fn run(&self, chave: ChaveRun) -> Option<Vec<(String, String)>> {
        let (raiz, caminho) = match chave {
            ChaveRun::RunUsuario => (HKEY_CURRENT_USER, RUN),
            ChaveRun::RunOnceUsuario => (HKEY_CURRENT_USER, RUN_ONCE),
            ChaveRun::RunMaquina => (HKEY_LOCAL_MACHINE, RUN),
            ChaveRun::RunOnceMaquina => (HKEY_LOCAL_MACHINE, RUN_ONCE),
            ChaveRun::RunMaquina32 => (HKEY_LOCAL_MACHINE, RUN_32),
        };
        valores(raiz as isize, caminho, |v| match v.vtype {
            REG_SZ | REG_EXPAND_SZ => utf16(&v.bytes),
            _ => None,
        })
    }

    fn aprovacao(&self, chave: Aprovacao) -> Option<Vec<(String, Vec<u8>)>> {
        let (raiz, sub) = match chave {
            Aprovacao::RunUsuario => (HKEY_CURRENT_USER, "Run"),
            Aprovacao::PastaUsuario => (HKEY_CURRENT_USER, "StartupFolder"),
            Aprovacao::RunMaquina => (HKEY_LOCAL_MACHINE, "Run"),
            Aprovacao::Run32Maquina => (HKEY_LOCAL_MACHINE, "Run32"),
            Aprovacao::PastaMaquina => (HKEY_LOCAL_MACHINE, "StartupFolder"),
        };
        // Tipo inesperado vira vetor vazio, que decodifica como desconhecido.
        valores(raiz as isize, &format!(r"{APROVADO}\{sub}"), |v| {
            Some(if v.vtype == REG_BINARY {
                v.bytes.to_vec()
            } else {
                vec![]
            })
        })
    }

    fn pasta(&self, pasta: PastaInicio) -> Option<Vec<ArquivoPasta>> {
        let dir = pasta_conhecida(pasta)?;
        let leitura = match std::fs::read_dir(&dir) {
            Ok(l) => l,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Some(vec![]),
            Err(_) => return None,
        };
        let com = Com::iniciar();
        let mut saida = Vec::new();
        for item in leitura.take(VALORES_MAX).flatten() {
            if !item.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let Ok(nome_arquivo) = item.file_name().into_string() else {
                continue;
            };
            let alvo = if nome_arquivo.to_ascii_lowercase().ends_with(".lnk") && com.is_some() {
                alvo_atalho(&item.path())
            } else {
                None
            };
            saida.push(ArquivoPasta { nome_arquivo, alvo });
        }
        Some(saida)
    }

    fn tarefas(&self) -> Option<Vec<TarefaBruta>> {
        let _com = Com::iniciar()?;
        // SAFETY: COM ativo nesta thread; interfaces gerenciadas pelo crate windows.
        // Só leitura: Connect local, enumeração e propriedades. Nada é executado.
        unsafe {
            let servico: ITaskService =
                CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER).ok()?;
            let v = VARIANT::default();
            servico.Connect(&v, &v, &v, &v).ok()?;
            let raiz = servico.GetFolder(&BSTR::from("\\")).ok()?;
            let mut saida = Vec::new();
            varrer_pasta(&raiz, 0, &mut saida);
            Some(saida)
        }
    }
}

struct Com {
    desfazer: bool,
}

impl Com {
    fn iniciar() -> Option<Self> {
        // SAFETY: inicialização pareada pela guarda; modo já definido é aceito sem desfazer.
        let r = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if r.is_ok() {
            Some(Self { desfazer: true })
        } else if r == RPC_E_CHANGED_MODE {
            Some(Self { desfazer: false })
        } else {
            None
        }
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.desfazer {
            // SAFETY: pareado com o CoInitializeEx bem-sucedido.
            unsafe { CoUninitialize() };
        }
    }
}

fn pasta_conhecida(pasta: PastaInicio) -> Option<PathBuf> {
    let id = match pasta {
        PastaInicio::Usuario => &FOLDERID_Startup,
        PastaInicio::Comum => &FOLDERID_CommonStartup,
    };
    let mut p: *mut u16 = ptr::null_mut();
    // SAFETY: a API aloca a cadeia; liberada com CoTaskMemFree em qualquer caso.
    unsafe {
        let r = SHGetKnownFolderPath(id, 0, ptr::null_mut(), &mut p);
        let caminho = (r == 0 && !p.is_null()).then(|| {
            let len = (0..).take_while(|&i| *p.add(i) != 0).count();
            PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(p, len)))
        });
        CoTaskMemFree(p.cast());
        caminho
    }
}

/// Lê o alvo gravado no `.lnk` sem resolver (sem busca nem rede).
fn alvo_atalho(arquivo: &std::path::Path) -> Option<String> {
    // SAFETY: COM ativo; Load em modo leitura; GetPath escreve no buffer fornecido.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let arquivo_com: IPersistFile = link.cast().ok()?;
        let nome = HSTRING::from(arquivo.as_os_str());
        arquivo_com.Load(PCWSTR(nome.as_ptr()), STGM_READ).ok()?;
        let mut buf = [0u16; 1024];
        link.GetPath(&mut buf, ptr::null_mut(), SLGP_RAWPATH.0 as u32)
            .ok()?;
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let s = String::from_utf16(&buf[..len]).ok()?;
        (!s.is_empty()).then_some(s)
    }
}

/// # Safety
/// COM ativo na thread.
unsafe fn varrer_pasta(pasta: &ITaskFolder, nivel: usize, saida: &mut Vec<TarefaBruta>) {
    if nivel > PROFUNDIDADE_MAX || saida.len() >= TAREFAS_MAX {
        return;
    }
    // SAFETY: COM ativo (contrato da função).
    unsafe {
        if let Ok(tarefas) = pasta.GetTasks(TASK_ENUM_HIDDEN.0) {
            let n = tarefas.Count().unwrap_or(0);
            for i in 1..=n {
                if saida.len() >= TAREFAS_MAX {
                    return;
                }
                if let Ok(t) = tarefas.get_Item(&VARIANT::from(i))
                    && let Some(b) = tarefa(&t)
                {
                    saida.push(b);
                }
            }
        }
        if let Ok(sub) = pasta.GetFolders(0) {
            let n = sub.Count().unwrap_or(0);
            for i in 1..=n {
                if let Ok(p) = sub.get_Item(&VARIANT::from(i)) {
                    // Tarefas do sistema: não descer em \Microsoft.
                    if p.Path()
                        .is_ok_and(|c| c.to_string().eq_ignore_ascii_case(r"\Microsoft"))
                    {
                        continue;
                    }
                    varrer_pasta(&p, nivel + 1, saida);
                }
            }
        }
    }
}

/// # Safety
/// COM ativo na thread.
unsafe fn tarefa(t: &IRegisteredTask) -> Option<TarefaBruta> {
    // SAFETY: COM ativo (contrato da função); saídas em variáveis locais.
    unsafe {
        let def = t.Definition().ok()?;
        let gatilhos = def.Triggers().ok()?;
        let mut n = 0i32;
        gatilhos.Count(&mut n).ok()?;
        let mut de_logon = false;
        for i in 1..=n {
            let Ok(g) = gatilhos.get_Item(i) else {
                continue;
            };
            let mut tipo = TASK_TRIGGER_TYPE2::default();
            if g.Type(&mut tipo).is_ok()
                && (tipo == TASK_TRIGGER_LOGON || tipo == TASK_TRIGGER_BOOT)
            {
                de_logon = true;
                break;
            }
        }
        if !de_logon {
            return None;
        }
        let executavel = (|| {
            let acoes = def.Actions().ok()?;
            let mut n = 0i32;
            acoes.Count(&mut n).ok()?;
            if n < 1 {
                return None;
            }
            let a = acoes.get_Item(1).ok()?;
            let mut tipo = TASK_ACTION_TYPE::default();
            a.Type(&mut tipo).ok()?;
            if tipo != TASK_ACTION_EXEC {
                return None;
            }
            let exec: IExecAction = a.cast().ok()?;
            let mut p = BSTR::new();
            exec.Path(&mut p).ok()?;
            Some(p.to_string())
        })();
        Some(TarefaBruta {
            nome: t.Name().ok()?.to_string(),
            caminho_tarefa: t.Path().ok()?.to_string(),
            executavel,
            habilitada: t.Enabled().is_ok_and(|e| e != VARIANT_FALSE),
        })
    }
}

fn filetime(f: FILETIME) -> u64 {
    (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)
}

impl LeitorSinaisMaquina for SaudeWindows {
    fn disco_sistema(&self) -> Option<(u64, u64)> {
        let mut dir = [0u16; 260];
        // SAFETY: buffer local com o tamanho informado.
        let n = unsafe { GetSystemWindowsDirectoryW(dir.as_mut_ptr(), dir.len() as u32) };
        if n < 3 || n as usize >= dir.len() {
            return None;
        }
        let raiz = [dir[0], dir[1], u16::from(b'\\'), 0];
        let (mut livres, mut total) = (0u64, 0u64);
        // SAFETY: raiz NUL-terminada; saídas locais.
        let ok =
            unsafe { GetDiskFreeSpaceExW(raiz.as_ptr(), &mut livres, &mut total, ptr::null_mut()) };
        (ok != 0).then_some((livres, total))
    }

    fn memoria_comprometida(&self) -> Option<(u64, u64)> {
        // SAFETY: estrutura zerada com dwLength correto.
        unsafe {
            let mut m: MEMORYSTATUSEX = mem::zeroed();
            m.dwLength = mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut m) == 0 {
                return None;
            }
            let limite = m.ullTotalPageFile;
            Some((limite.checked_sub(m.ullAvailPageFile)?, limite))
        }
    }

    fn tempos_cpu(&self) -> Option<TemposCpu> {
        // SAFETY: saídas locais.
        unsafe {
            let (mut o, mut k, mut u) = (mem::zeroed(), mem::zeroed(), mem::zeroed());
            (GetSystemTimes(&mut o, &mut k, &mut u) != 0).then(|| TemposCpu {
                ocioso: filetime(o),
                kernel: filetime(k),
                usuario: filetime(u),
            })
        }
    }

    fn plano_ativo(&self) -> Option<u128> {
        let mut g = ptr::null_mut();
        // SAFETY: a API aloca o GUID, liberado com LocalFree.
        unsafe {
            if PowerGetActiveScheme(ptr::null_mut(), &mut g) != 0 || g.is_null() {
                return None;
            }
            let v: windows_sys::core::GUID = *g;
            LocalFree(g.cast());
            Some(
                (u128::from(v.data1) << 96)
                    | (u128::from(v.data2) << 80)
                    | (u128::from(v.data3) << 64)
                    | u128::from(u64::from_be_bytes(v.data4)),
            )
        }
    }

    fn driver_nvidia(&self) -> Option<u32> {
        let classe = abrir(HKEY_LOCAL_MACHINE as isize, CLASSE_VIDEO)??;
        classe
            .enum_keys()
            .take(64)
            .flatten()
            .filter(|n| n.len() == 4 && n.bytes().all(|b| b.is_ascii_digit()))
            .filter_map(|n| classe.open_subkey_with_flags(&n, KEY_READ).ok())
            .filter(|k| {
                k.get_value::<String, _>("ProviderName")
                    .is_ok_and(|p| p.to_ascii_uppercase().contains("NVIDIA"))
            })
            .filter_map(|k| versao_nvidia(&k.get_value::<String, _>("DriverVersion").ok()?))
            .max()
    }
}

/// `32.0.15.6603` -> `56603` (566.03). Formato fora do esperado = desconhecido.
pub(super) fn versao_nvidia(v: &str) -> Option<u32> {
    let partes: Vec<u32> = v
        .split('.')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    let [_, _, c, d] = partes[..] else {
        return None;
    };
    (d < 10_000).then_some((c % 10) * 10_000 + d)
}

impl LeitorSinaisSessao for SaudeWindows {
    fn storage_policy_01(&self) -> Option<LeituraDword> {
        let Some(chave) = abrir(HKEY_CURRENT_USER as isize, STORAGE)? else {
            return Some(LeituraDword::Ausente);
        };
        Some(match chave.get_raw_value("01") {
            Ok(v) if v.vtype == REG_DWORD && v.bytes.len() == 4 => {
                LeituraDword::Valor(u32::from_le_bytes([
                    v.bytes[0], v.bytes[1], v.bytes[2], v.bytes[3],
                ]))
            }
            Ok(_) => LeituraDword::Outro,
            Err(e) if e.kind() == io::ErrorKind::NotFound => LeituraDword::Ausente,
            Err(_) => return None,
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::saude::{
        ParametrosSinais, ler_inicio_maquina, ler_inicio_sessao, ler_sinais_maquina,
        ler_sinais_sessao,
    };
    use nvt_ipc::saude::{Gravidade, SituacaoInicio};

    #[test]
    fn versao_do_driver() {
        assert_eq!(versao_nvidia("32.0.15.6603"), Some(56603));
        assert_eq!(versao_nvidia("31.0.15.5222"), Some(55222));
        assert_eq!(versao_nvidia("32.0.15"), None);
        assert_eq!(versao_nvidia("a.b.c.d"), None);
    }

    #[test]
    fn utf16_tolera_terminador() {
        assert_eq!(utf16(&[0x61, 0, 0x62, 0, 0, 0]).as_deref(), Some("ab"));
        assert_eq!(utf16(&[0x61]), None);
    }

    /// Lê a máquina real e imprime só contagens.
    #[test]
    #[ignore = "lê a máquina real"]
    fn maquina_real() {
        let w = SaudeWindows;
        for (rotulo, l) in [
            ("sessao", ler_inicio_sessao(&w)),
            ("maquina", ler_inicio_maquina(&w)),
        ] {
            let desligados = l
                .entradas
                .iter()
                .filter(|e| e.habilitado == Some(false))
                .count();
            let desconhecidos = l.entradas.iter().filter(|e| e.habilitado.is_none()).count();
            for e in &l.entradas {
                e.programa(SituacaoInicio::NaBase, 0).validar().unwrap();
            }
            println!(
                "{rotulo}: {} entradas, {desligados} desligadas, {desconhecidos} desconhecidas, indisponiveis {:?}",
                l.entradas.len(),
                l.fontes_indisponiveis
            );
            let mut por_fonte = std::collections::BTreeMap::new();
            for e in &l.entradas {
                *por_fonte.entry(e.fonte).or_insert(0) += 1;
            }
            println!("{rotulo} por fonte: {por_fonte:?}");
        }
        let p = ParametrosSinais {
            janela_cpu: Some(std::time::Duration::from_secs(2)),
            ..ParametrosSinais::default()
        };
        let mut sinais = ler_sinais_maquina(&w, &p, std::thread::sleep, 0);
        sinais.extend(ler_sinais_sessao(&w, 0));
        for s in &sinais {
            s.validar().unwrap();
            println!("{:?} {:?} {:?}", s.tipo, s.gravidade, s.valor);
        }
        println!("driver atual: {:?}", w.driver_nvidia());
        assert!(
            sinais
                .iter()
                .any(|s| s.gravidade != Gravidade::Indisponivel)
        );
    }
}
