//! Único ponto de `CreateProcessW` (msiexec) e `CreateProcessAsUserW` (agente) (spec §9.2).
//!
//! Tudo o que decide *o quê* lançar (caminho, linha de comando, flags, ambiente, pasta
//! de instalação, conferência de SID) é função pura testável; a parte Windows só
//! executa a decisão. Nenhum teste chama a criação de processo.

use std::fmt;
use std::path::{Component, Path, PathBuf, Prefix};

/// Nome do executável do instalador, sempre sob `GetSystemDirectoryW` (U-30).
pub(crate) const MSIEXEC: &str = "msiexec.exe";
/// Executável fixo do agente, sob a pasta de instalação do próprio serviço (U-44).
pub(crate) const AGENTE: &str = "nvt-agente.exe";
/// Área de trabalho interativa da sessão (U-44).
pub(crate) const DESKTOP: &str = "winsta0\\default";

pub(crate) const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
pub(crate) const CREATE_UNICODE_ENVIRONMENT: u32 = 0x0000_0400;
pub(crate) const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Falha do lançador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ErroLancador {
    /// Caminho com aspas, `%`, controle, prefixo `\\?\` ou sem `X:\` (U-31).
    CaminhoInvalido(String),
    /// Serviço fora de `%ProgramFiles%` (instalação de desenvolvimento, U-45).
    ForaDeProgramFiles(String),
    /// Componente da pasta de instalação é reparse point (U-45).
    Reparse(String),
    /// A sessão pertence a outro usuário (U-43).
    SidDivergente {
        sessao: u32,
        esperado: String,
        obtido: String,
    },
    /// Chamada Win32 falhou.
    Win32 { api: &'static str, codigo: u32 },
}

impl fmt::Display for ErroLancador {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CaminhoInvalido(p) => write!(f, "caminho recusado para lançamento: {p}"),
            Self::ForaDeProgramFiles(p) => write!(f, "serviço fora de Program Files: {p}"),
            Self::Reparse(p) => write!(f, "pasta de instalação com reparse point: {p}"),
            Self::SidDivergente {
                sessao,
                esperado,
                obtido,
            } => write!(
                f,
                "sessão {sessao} é de outro usuário (esperado {esperado}, obtido {obtido})"
            ),
            Self::Win32 { api, codigo } => write!(f, "{api} falhou: {codigo}"),
        }
    }
}

impl std::error::Error for ErroLancador {}

/// Resultado de um lançamento do msiexec (sem espera).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Lancamento {
    pub pid: u32,
    pub flags: u32,
    /// O serviço estava em job e o breakaway foi recusado (lançado sem o flag).
    pub breakaway_recusado: bool,
}

/// Fachada de criação de processos (real em Windows; falsa nos testes).
pub(crate) trait Lancador {
    /// Lança `msiexec /i "<msi>" /qn /norestart /l*v "<log>"` e devolve o PID sem esperar.
    fn lancar_msiexec(&self, msi: &Path, log: &Path) -> Result<Lancamento, ErroLancador>;
    /// Relança o agente uma vez em cada `(sessão, SID)`; devolve o PID ou a falha de cada.
    fn relancar_agente(&self, sessoes: &[(u32, String)]) -> Vec<(u32, Result<u32, ErroLancador>)>;
}

// ---------------------------------------------------------------- funções puras

/// Recusa caminho que não seja `X:\...` absoluto e limpo (U-31), por defesa.
pub(crate) fn validar_caminho(p: &str) -> Result<(), ErroLancador> {
    let recusa = || Err(ErroLancador::CaminhoInvalido(p.to_owned()));
    let b = p.as_bytes();
    if b.len() < 4 || !b[0].is_ascii_alphabetic() || b[1] != b':' || b[2] != b'\\' {
        return recusa();
    }
    if p.chars().any(|c| c == '"' || c == '%' || c.is_control())
        || p.contains('/')
        || p.ends_with('\\')
        || p.split('\\').any(|s| s == ".." || s == ".")
    {
        return recusa();
    }
    Ok(())
}

fn texto(p: &Path) -> Result<&str, ErroLancador> {
    p.to_str()
        .ok_or_else(|| ErroLancador::CaminhoInvalido(p.to_string_lossy().into_owned()))
}

/// `<System32>\msiexec.exe`, para `lpApplicationName` (U-30).
pub(crate) fn caminho_msiexec(sistema: &str) -> Result<String, ErroLancador> {
    validar_caminho(sistema)?;
    Ok(format!("{sistema}\\{MSIEXEC}"))
}

/// Linha de comando exata do msiexec (U-31); nenhuma propriedade pública.
pub(crate) fn linha_msiexec(sistema: &str, msi: &str, log: &str) -> Result<String, ErroLancador> {
    let exe = caminho_msiexec(sistema)?;
    validar_caminho(msi)?;
    validar_caminho(log)?;
    Ok(format!(
        "\"{exe}\" /i \"{msi}\" /qn /norestart /l*v \"{log}\""
    ))
}

/// Sequência de flags a tentar (U-32): em job, primeiro com breakaway, depois sem.
pub(crate) fn tentativas_flags(em_job: bool) -> Vec<u32> {
    let base = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_UNICODE_ENVIRONMENT;
    if em_job {
        vec![base | CREATE_BREAKAWAY_FROM_JOB, base]
    } else {
        vec![base]
    }
}

/// Flags do agente (U-44).
pub(crate) fn flags_agente() -> u32 {
    CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW
}

/// Bloco de ambiente mínimo UTF-16 do msiexec (U-32): `SystemRoot`, `windir`,
/// `TEMP`/`TMP` = pasta protegida; ordenado, cada par terminado em zero, e zero final.
pub(crate) fn ambiente_minimo(sistema: &str, pasta: &str) -> Result<Vec<u16>, ErroLancador> {
    validar_caminho(sistema)?;
    validar_caminho(pasta)?;
    let raiz = match sistema.rfind('\\') {
        Some(i) if i > 2 => &sistema[..i],
        _ => return Err(ErroLancador::CaminhoInvalido(sistema.to_owned())),
    };
    let mut bloco = Vec::new();
    for (k, v) in [
        ("SystemRoot", raiz),
        ("TEMP", pasta),
        ("TMP", pasta),
        ("windir", raiz),
    ] {
        bloco.extend(format!("{k}={v}").encode_utf16());
        bloco.push(0);
    }
    bloco.push(0);
    Ok(bloco)
}

fn componentes_normais(p: &Path) -> Option<(char, Vec<String>)> {
    let mut it = p.components();
    let letra = match it.next()? {
        Component::Prefix(px) => match px.kind() {
            Prefix::Disk(l) => (l as char).to_ascii_uppercase(),
            _ => return None,
        },
        _ => return None,
    };
    if it.next()? != Component::RootDir {
        return None;
    }
    let mut v = Vec::new();
    for c in it {
        match c {
            Component::Normal(s) => v.push(s.to_str()?.to_lowercase()),
            _ => return None,
        }
    }
    Some((letra, v))
}

/// `<INSTALLFOLDER>` = pasta do executável do serviço, estritamente dentro de
/// `%ProgramFiles%` (U-45). Recusa `\\?\`, `..` e caminhos fora.
pub(crate) fn pasta_instalacao(exe: &Path, program_files: &Path) -> Result<PathBuf, ErroLancador> {
    let fora = || ErroLancador::ForaDeProgramFiles(exe.to_string_lossy().into_owned());
    validar_caminho(texto(exe)?)?;
    validar_caminho(texto(program_files)?)?;
    let pasta = exe.parent().ok_or_else(fora)?;
    let (l1, c1) = componentes_normais(pasta).ok_or_else(fora)?;
    let (l2, c2) = componentes_normais(program_files).ok_or_else(fora)?;
    if l1 != l2 || c1.len() <= c2.len() || c1[..c2.len()] != c2[..] {
        return Err(fora());
    }
    Ok(pasta.to_path_buf())
}

/// Linha de comando do agente: só o executável entre aspas (U-44).
pub(crate) fn linha_agente(pasta: &str) -> Result<String, ErroLancador> {
    validar_caminho(pasta)?;
    Ok(format!("\"{pasta}\\{AGENTE}\""))
}

/// O SID do token é o gravado com a sessão antes do upgrade (U-43).
pub(crate) fn conferir_sid(sessao: u32, esperado: &str, obtido: &str) -> Result<(), ErroLancador> {
    if esperado.starts_with("S-") && esperado.eq_ignore_ascii_case(obtido) {
        Ok(())
    } else {
        Err(ErroLancador::SidDivergente {
            sessao,
            esperado: esperado.to_owned(),
            obtido: obtido.to_owned(),
        })
    }
}

// ---------------------------------------------------------------- Windows

#[cfg(windows)]
pub(crate) use real::LancadorReal;

#[cfg(windows)]
mod real {
    use super::*;
    use std::ffi::{OsString, c_void};
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{GetLastError, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_USER, TokenUser};
    use windows_sys::Win32::System::Environment::{
        CreateEnvironmentBlock, DestroyEnvironmentBlock,
    };
    use windows_sys::Win32::System::RemoteDesktop::WTSQueryUserToken;
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, PROCESS_INFORMATION, STARTUPINFOW,
    };
    use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath};

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn IsProcessInJob(processo: HANDLE, job: HANDLE, resultado: *mut i32) -> i32;
        fn GetModuleFileNameW(modulo: *mut c_void, buf: *mut u16, tamanho: u32) -> u32;
    }
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoTaskMemFree(pv: *const c_void);
    }

    fn erro(api: &'static str) -> ErroLancador {
        // SAFETY: sem pré-condições.
        let codigo = unsafe { GetLastError() };
        ErroLancador::Win32 { api, codigo }
    }

    use crate::diretorio::utf16 as wide;

    /// Fecha o handle (se houver) assumindo a posse dele.
    fn fechar(h: HANDLE) {
        if !h.is_null() {
            // SAFETY: handle próprio, fechado uma única vez.
            drop(unsafe { OwnedHandle::from_raw_handle(h) });
        }
    }

    /// Destrói o bloco de ambiente ao sair do escopo.
    struct Ambiente(*mut c_void);
    impl Drop for Ambiente {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: bloco devolvido por CreateEnvironmentBlock, destruído uma vez.
                unsafe { DestroyEnvironmentBlock(self.0) };
            }
        }
    }

    fn fechar_processo(pi: &PROCESS_INFORMATION) {
        fechar(pi.hThread);
        fechar(pi.hProcess);
    }

    fn pasta_sistema() -> Result<String, ErroLancador> {
        let mut buf = [0u16; 260];
        // SAFETY: buffer gravável com o tamanho informado.
        let n = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
        if n == 0 || n >= buf.len() {
            return Err(erro("GetSystemDirectoryW"));
        }
        String::from_utf16(&buf[..n]).map_err(|_| ErroLancador::CaminhoInvalido("System32".into()))
    }

    fn em_job() -> Result<bool, ErroLancador> {
        let mut r = 0;
        // SAFETY: pseudo-handle do próprio processo; job nulo = qualquer job.
        if unsafe { IsProcessInJob(GetCurrentProcess(), null_mut(), &mut r) } == 0 {
            return Err(erro("IsProcessInJob"));
        }
        Ok(r != 0)
    }

    fn program_files() -> Result<PathBuf, ErroLancador> {
        let mut p: windows_sys::core::PWSTR = null_mut();
        // SAFETY: GUID estático; `p` recebe memória do CoTaskMemAlloc.
        let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, 0, null_mut(), &mut p) };
        let r = if hr < 0 || p.is_null() {
            Err(ErroLancador::Win32 {
                api: "SHGetKnownFolderPath",
                codigo: hr as u32,
            })
        } else {
            // SAFETY: cadeia terminada em zero devolvida pela API.
            let n = (0..).take_while(|&i| unsafe { *p.add(i) } != 0).count();
            let s = unsafe { std::slice::from_raw_parts(p, n) };
            Ok(PathBuf::from(OsString::from_wide(s)))
        };
        // SAFETY: liberar mesmo em falha é permitido (nulo é ignorado).
        unsafe { CoTaskMemFree(p.cast()) };
        r
    }

    fn exe_servico() -> Result<PathBuf, ErroLancador> {
        let mut buf = vec![0u16; 1024];
        // SAFETY: módulo nulo = executável do processo; buffer com o tamanho informado.
        let n = unsafe { GetModuleFileNameW(null_mut(), buf.as_mut_ptr(), buf.len() as u32) };
        if n == 0 || n as usize >= buf.len() {
            return Err(erro("GetModuleFileNameW"));
        }
        Ok(PathBuf::from(OsString::from_wide(&buf[..n as usize])))
    }

    /// Recusa reparse em cada componente de `pasta` (até a raiz) e no executável.
    fn sem_reparse(pasta: &Path, exe: &Path) -> Result<(), ErroLancador> {
        for p in std::iter::once(exe).chain(pasta.ancestors()) {
            if p.parent().is_none() {
                break;
            }
            let m = std::fs::symlink_metadata(p)
                .map_err(|_| ErroLancador::Reparse(p.display().to_string()))?;
            if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(ErroLancador::Reparse(p.display().to_string()));
            }
        }
        Ok(())
    }

    fn sid_do_token(token: HANDLE) -> Result<String, ErroLancador> {
        let mut buffer = [0usize; 32];
        let mut tamanho = 0;
        // SAFETY: buffer alinhado e suficiente para TOKEN_USER e o SID anexado.
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                size_of_val(&buffer) as u32,
                &mut tamanho,
            )
        };
        if ok == 0 {
            return Err(erro("GetTokenInformation"));
        }
        // SAFETY: TokenUser preencheu o buffer e o ponteiro referencia o SID nele.
        let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        Ok(crate::diretorio::sid_texto(sid))
    }

    /// Implementação real (serviço em LocalSystem).
    pub(crate) struct LancadorReal;

    impl LancadorReal {
        fn criar_msiexec(
            app: &[u16],
            linha: &str,
            ambiente: &[u16],
            dir: &[u16],
            flags: u32,
        ) -> Result<u32, ErroLancador> {
            let mut cmd = wide(linha);
            // SAFETY: STARTUPINFOW/PROCESS_INFORMATION são POD; zero é válido.
            let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
            si.cb = size_of::<STARTUPINFOW>() as u32;
            let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
            // SAFETY: cadeias terminadas em zero vivas durante a chamada; `cmd` é
            // gravável como a API exige; sem herança de handles (U-32).
            let ok = unsafe {
                windows_sys::Win32::System::Threading::CreateProcessW(
                    app.as_ptr(),
                    cmd.as_mut_ptr(),
                    null(),
                    null(),
                    0,
                    flags,
                    ambiente.as_ptr().cast(),
                    dir.as_ptr(),
                    &si,
                    &mut pi,
                )
            };
            if ok == 0 {
                return Err(erro("msiexec"));
            }
            let pid = pi.dwProcessId;
            fechar_processo(&pi);
            Ok(pid)
        }

        fn relancar_um(
            sessao: u32,
            sid: &str,
            pasta: &str,
            app: &[u16],
        ) -> Result<u32, ErroLancador> {
            let mut t: HANDLE = null_mut();
            // SAFETY: saída gravável; o serviço roda como LocalSystem.
            if unsafe { WTSQueryUserToken(sessao, &mut t) } == 0 {
                return Err(erro("WTSQueryUserToken"));
            }
            // SAFETY: token próprio devolvido pela chamada, fechado uma vez.
            let token = unsafe { OwnedHandle::from_raw_handle(t) };
            conferir_sid(sessao, sid, &sid_do_token(token.as_raw_handle())?)?;
            let mut env: *mut c_void = null_mut();
            // SAFETY: token válido; sem herdar o ambiente do serviço.
            if unsafe { CreateEnvironmentBlock(&mut env, token.as_raw_handle(), 0) } == 0 {
                return Err(erro("CreateEnvironmentBlock"));
            }
            let env = Ambiente(env);
            let mut cmd = wide(&linha_agente(pasta)?);
            let dir = wide(pasta);
            let mut desktop = wide(DESKTOP);
            // SAFETY: POD; zero é válido.
            let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
            si.cb = size_of::<STARTUPINFOW>() as u32;
            si.lpDesktop = desktop.as_mut_ptr();
            let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
            // SAFETY: token e bloco de ambiente vivos (guardas); cadeias terminadas em
            // zero vivas durante a chamada; sem herança de handles (U-44).
            let ok = unsafe {
                windows_sys::Win32::System::Threading::CreateProcessAsUserW(
                    token.as_raw_handle(),
                    app.as_ptr(),
                    cmd.as_mut_ptr(),
                    null(),
                    null(),
                    0,
                    flags_agente(),
                    env.0,
                    dir.as_ptr(),
                    &si,
                    &mut pi,
                )
            };
            if ok == 0 {
                return Err(erro("agente"));
            }
            let pid = pi.dwProcessId;
            fechar_processo(&pi);
            Ok(pid)
        }
    }

    impl Lancador for LancadorReal {
        fn lancar_msiexec(&self, msi: &Path, log: &Path) -> Result<Lancamento, ErroLancador> {
            let sistema = pasta_sistema()?;
            let (msi, log) = (texto(msi)?, texto(log)?);
            let linha = linha_msiexec(&sistema, msi, log)?;
            let pasta = msi
                .rfind('\\')
                .map(|i| &msi[..i])
                .ok_or_else(|| ErroLancador::CaminhoInvalido(msi.to_owned()))?;
            let ambiente = ambiente_minimo(&sistema, pasta)?;
            let app = wide(&caminho_msiexec(&sistema)?);
            let dir = wide(&sistema);
            let job = em_job()?;
            let mut breakaway_recusado = false;
            let mut ultimo = None;
            for flags in tentativas_flags(job) {
                match Self::criar_msiexec(&app, &linha, &ambiente, &dir, flags) {
                    Ok(pid) => {
                        return Ok(Lancamento {
                            pid,
                            flags,
                            breakaway_recusado,
                        });
                    }
                    Err(e) if flags & CREATE_BREAKAWAY_FROM_JOB != 0 => {
                        tracing::warn!("serviço em job: breakaway recusado ({e}); tentando sem");
                        breakaway_recusado = true;
                        ultimo = Some(e);
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(ultimo.unwrap_or(ErroLancador::Win32 {
                api: "msiexec",
                codigo: 0,
            }))
        }

        fn relancar_agente(
            &self,
            sessoes: &[(u32, String)],
        ) -> Vec<(u32, Result<u32, ErroLancador>)> {
            let preparo = (|| {
                let exe = exe_servico()?;
                let pasta = pasta_instalacao(&exe, &program_files()?)?;
                let agente = pasta.join(AGENTE);
                sem_reparse(&pasta, &agente)?;
                Ok::<_, ErroLancador>(texto(&pasta)?.to_owned())
            })();
            sessoes
                .iter()
                .map(|(sessao, sid)| {
                    let r = preparo.clone().and_then(|pasta| {
                        let app = wide(format!("{pasta}\\{AGENTE}"));
                        Self::relancar_um(*sessao, sid, &pasta, &app)
                    });
                    (*sessao, r)
                })
                .collect()
        }
    }
}

// ---------------------------------------------------------------- falso

/// Lançador falso para testes de outros módulos: registra chamadas e devolve respostas fixas.
#[cfg(test)]
pub(crate) struct LancadorFalso {
    pub msiexec: std::sync::Mutex<Vec<(PathBuf, PathBuf)>>,
    pub agentes: std::sync::Mutex<Vec<(u32, String)>>,
    pub resposta_msiexec: Result<Lancamento, ErroLancador>,
    /// Sessões cujo relançamento falha.
    pub falham: Vec<u32>,
}

#[cfg(test)]
impl Default for LancadorFalso {
    fn default() -> Self {
        Self {
            msiexec: Default::default(),
            agentes: Default::default(),
            resposta_msiexec: Ok(Lancamento {
                pid: 4242,
                flags: tentativas_flags(false)[0],
                breakaway_recusado: false,
            }),
            falham: Vec::new(),
        }
    }
}

#[cfg(test)]
impl Lancador for LancadorFalso {
    fn lancar_msiexec(&self, msi: &Path, log: &Path) -> Result<Lancamento, ErroLancador> {
        self.msiexec
            .lock()
            .unwrap()
            .push((msi.to_path_buf(), log.to_path_buf()));
        self.resposta_msiexec.clone()
    }

    fn relancar_agente(&self, sessoes: &[(u32, String)]) -> Vec<(u32, Result<u32, ErroLancador>)> {
        self.agentes.lock().unwrap().extend_from_slice(sessoes);
        sessoes
            .iter()
            .map(|(s, _)| {
                let r = if self.falham.contains(s) {
                    Err(ErroLancador::Win32 {
                        api: "agente",
                        codigo: 5,
                    })
                } else {
                    Ok(1000 + s)
                };
                (*s, r)
            })
            .collect()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const SYS: &str = "C:\\Windows\\System32";
    fn pasta() -> String {
        crate::teste_util::pasta_atualizacao_falsa()
            .display()
            .to_string()
    }

    #[test]
    fn linha_exata_com_espacos() {
        let msi = "C:\\Program Data X\\nv-tuner-1.2.3-x64.msi";
        let log = "C:\\Program Data X\\instalacao-1.2.3.log";
        assert_eq!(
            linha_msiexec(SYS, msi, log).unwrap(),
            "\"C:\\Windows\\System32\\msiexec.exe\" /i \"C:\\Program Data X\\nv-tuner-1.2.3-x64.msi\" /qn /norestart /l*v \"C:\\Program Data X\\instalacao-1.2.3.log\""
        );
        assert_eq!(
            caminho_msiexec(SYS).unwrap(),
            "C:\\Windows\\System32\\msiexec.exe"
        );
    }

    #[test]
    fn recusa_caminhos_perigosos() {
        for p in [
            "C:\\a\"b.msi",
            "C:\\%TEMP%\\x.msi",
            "C:\\a\nb.msi",
            "C:\\a\u{7}\\x.msi",
            "\\\\?\\UNC\\srv\\c\\x.msi",
            "\\\\?\\C:\\x.msi",
            "\\\\srv\\c\\x.msi",
            "x.msi",
            "C:x.msi",
            "C:/a/x.msi",
            "C:\\a\\..\\x.msi",
            "C:\\a\\",
            "",
        ] {
            assert!(validar_caminho(p).is_err(), "{p:?}");
            assert!(linha_msiexec(SYS, p, "C:\\a\\l.log").is_err(), "{p:?}");
            assert!(linha_msiexec(SYS, "C:\\a\\m.msi", p).is_err(), "{p:?}");
        }
        assert!(linha_msiexec("C:\\Win\"dows", "C:\\a\\m.msi", "C:\\a\\l.log").is_err());
    }

    #[test]
    fn flags_e_breakaway() {
        let base = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_UNICODE_ENVIRONMENT;
        assert_eq!(tentativas_flags(false), vec![base]);
        assert_eq!(
            tentativas_flags(true),
            vec![base | CREATE_BREAKAWAY_FROM_JOB, base]
        );
        assert_eq!(base & 0x8, 0, "nunca DETACHED_PROCESS");
        assert_eq!(
            flags_agente(),
            CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW
        );
    }

    #[test]
    fn ambiente_minimo_ordenado() {
        let p = pasta();
        let b = ambiente_minimo(SYS, &p).unwrap();
        let s = String::from_utf16(&b).unwrap();
        assert_eq!(
            s,
            format!("SystemRoot=C:\\Windows\0TEMP={p}\0TMP={p}\0windir=C:\\Windows\0\0")
        );
        assert!(ambiente_minimo(SYS, "C:\\%x%").is_err());
    }

    #[test]
    fn pasta_instalacao_so_dentro_de_program_files() {
        let pf = Path::new("C:\\Program Files");
        assert_eq!(
            pasta_instalacao(
                Path::new("C:\\Program Files\\nv-tuner\\nvt-service.exe"),
                pf
            )
            .unwrap(),
            PathBuf::from("C:\\Program Files\\nv-tuner")
        );
        assert!(
            pasta_instalacao(Path::new("c:\\program files\\NV-tuner\\s.exe"), pf).is_ok(),
            "comparação sem caixa"
        );
        for exe in [
            "C:\\Program Files\\s.exe",
            "C:\\Program Files (x86)\\nv\\s.exe",
            "D:\\Program Files\\nv\\s.exe",
            "C:\\dev\\target\\debug\\s.exe",
            "C:\\Program Files\\..\\x\\s.exe",
            "\\\\?\\C:\\Program Files\\nv\\s.exe",
        ] {
            assert!(pasta_instalacao(Path::new(exe), pf).is_err(), "{exe}");
        }
    }

    #[test]
    fn linha_do_agente() {
        assert_eq!(
            linha_agente("C:\\Program Files\\nv-tuner").unwrap(),
            "\"C:\\Program Files\\nv-tuner\\nvt-agente.exe\""
        );
        assert!(linha_agente("C:\\a\"b").is_err());
    }

    #[test]
    fn sid_confere() {
        assert!(conferir_sid(1, "S-1-5-21-1-2-3-1001", "S-1-5-21-1-2-3-1001").is_ok());
        assert!(matches!(
            conferir_sid(1, "S-1-5-21-1-2-3-1001", "S-1-5-21-1-2-3-1002"),
            Err(ErroLancador::SidDivergente { sessao: 1, .. })
        ));
        assert!(conferir_sid(1, "", "").is_err());
    }

    #[test]
    fn falso_registra() {
        let f = LancadorFalso {
            falham: vec![2],
            ..Default::default()
        };
        assert_eq!(
            f.lancar_msiexec(Path::new("C:\\a\\m.msi"), Path::new("C:\\a\\l.log"))
                .unwrap()
                .pid,
            4242
        );
        let r = f.relancar_agente(&[(1, "S-1".into()), (2, "S-2".into())]);
        assert!(r[0].1.is_ok() && r[1].1.is_err());
        assert_eq!(f.agentes.lock().unwrap().len(), 2);
    }
}
