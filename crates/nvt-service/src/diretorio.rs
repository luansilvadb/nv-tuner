//! Segurança do diretório de estado (`%ProgramData%\nv-tuner`).
//!
//! Em `C:\ProgramData`, usuários comuns podem criar subpastas e viram donos
//! delas. Se alguém pré-criar o diretório de estado (ou plantar junções,
//! links simbólicos ou arquivos nele), o serviço, como SYSTEM, leria um
//! snapshot forjado ou escreveria logs onde o atacante quisesse. Por isso,
//! antes de usar o diretório, [`preparar`]:
//!
//! 1. cria o diretório, se faltar, com descritor próprio ([`SDDL_ESTADO`]):
//!    dono Administradores, DACL protegida (sem herança), SYSTEM e
//!    Administradores com controle total e Usuários só com leitura;
//! 2. percorre o diretório e todo o conteúdo **sem seguir reparse points**
//!    e confere, em cada item: não é reparse point (junção, link simbólico
//!    etc.), não tem mais de um vínculo (hard link), o dono é SYSTEM ou
//!    Administradores e a DACL não concede nenhum direito de escrita a
//!    outras contas;
//! 3. se o único problema for o dono de itens internos (a raiz é segura e
//!    nenhuma DACL concede escrita a outros), Administradores toma posse
//!    deles: são arquivos criados por um administrador elevado cujo dono
//!    padrão era o próprio usuário (ver [`definir_dono_padrao`]);
//! 4. qualquer outro problema: em [`Modo::Exigir`] move o diretório inteiro
//!    para uma quarentena ao lado (`nv-tuner.inseguro-<unix>-<pid>`, nada é
//!    apagado), cria um novo com o descritor seguro e grava nele o marcador
//!    `snapshot-perdido.json` (o snapshot original ficou na quarentena: o
//!    próximo é só uma base pós-quarentena, com aviso do motor); se algo
//!    disso falhar, devolve erro e quem chama recusa operar.
//!
//! Item interno que some durante a verificação (`NotFound`) não é problema,
//! e uma verificação com falha de leitura é repetida antes de corrigir.
//!
//! O conteúdo de um diretório que já foi gravável por usuários comuns não é
//! confiável (um `snapshot-original.json` forjado seria restaurado como se
//! fosse o original), por isso a correção é a quarentena, e não só tomar
//! posse e reescrever a DACL.
//!
//! [`abrir_sem_reparse`] abre arquivos com `FILE_FLAG_OPEN_REPARSE_POINT` e
//! recusa o que não for um arquivo comum com um único vínculo.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INVALID_OWNER, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_FILE_OBJECT, SetSecurityInfo,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, CreateWellKnownSid, DACL_SECURITY_INFORMATION, GetAce,
    INHERIT_ONLY_ACE, IsWellKnownSid, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    SECURITY_ATTRIBUTES, SECURITY_MAX_SID_SIZE, SetTokenInformation, TOKEN_ADJUST_DEFAULT,
    TOKEN_OWNER, TOKEN_QUERY, TokenOwner, WinBuiltinAdministratorsSid, WinCreatorOwnerSid,
    WinLocalSystemSid,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateDirectoryW, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FindClose, FindFirstFileNameW, FindNextFileNameW, GetFileInformationByHandle,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Descritor do diretório de estado (o mesmo do instalador, com dono).
pub const SDDL_ESTADO: &str = "O:BAD:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)";
/// Igual, sem dono (fica o dono padrão do processo, SYSTEM no serviço).
const SDDL_ESTADO_SEM_DONO: &str = "D:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)";

/// Limite de itens percorridos (proteção contra diretórios gigantes).
const ITENS_MAX: usize = 20_000;

const READ_CONTROL: u32 = 0x0002_0000;
const FILE_READ_ATTRIBUTES: u32 = 0x0080;
const WRITE_OWNER: u32 = 0x0008_0000;

/// Direitos que permitem alterar o item, sua ACL, dono ou conteúdo.
const DIREITOS_ESCRITA: u32 = 0x0000_0002 // FILE_WRITE_DATA / FILE_ADD_FILE
    | 0x0000_0004 // FILE_APPEND_DATA / FILE_ADD_SUBDIRECTORY
    | 0x0000_0010 // FILE_WRITE_EA
    | 0x0000_0040 // FILE_DELETE_CHILD
    | 0x0000_0100 // FILE_WRITE_ATTRIBUTES
    | 0x0001_0000 // DELETE
    | 0x0004_0000 // WRITE_DAC
    | 0x0008_0000 // WRITE_OWNER
    | 0x1000_0000 // GENERIC_ALL
    | 0x4000_0000; // GENERIC_WRITE

const ACE_PERMITIR: u8 = 0x00; // ACCESS_ALLOWED_ACE_TYPE
const ACE_NEGAR: u8 = 0x01; // ACCESS_DENIED_ACE_TYPE
const ACE_PERMITIR_CALLBACK: u8 = 0x09;
const ACE_NEGAR_CALLBACK: u8 = 0x0A;

/// Um problema encontrado num item do diretório de estado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problema {
    /// Junção, link simbólico ou outro reparse point.
    ReparsePoint(PathBuf),
    /// Arquivo com mais de um vínculo (hard link).
    VariosVinculos(PathBuf),
    /// O caminho do diretório de estado não é um diretório.
    NaoEDiretorio(PathBuf),
    /// Dono diferente de SYSTEM e Administradores (SID em texto).
    Dono(PathBuf, String),
    /// DACL nula (acesso total a todos).
    DaclNula(PathBuf),
    /// Conta (SID) com algum direito de escrita.
    Escrita(PathBuf, String),
    /// ACE de tipo não reconhecido numa DACL.
    AceDesconhecida(PathBuf, u8),
    /// Itens demais para verificar.
    Demais,
    /// Falha ao ler o item.
    Io(PathBuf, String),
}

impl fmt::Display for Problema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReparsePoint(p) => write!(f, "{} é junção/link (reparse point)", p.display()),
            Self::VariosVinculos(p) => write!(f, "{} tem mais de um vínculo", p.display()),
            Self::NaoEDiretorio(p) => write!(f, "{} não é um diretório", p.display()),
            Self::Dono(p, s) => write!(f, "{} pertence a {s}", p.display()),
            Self::DaclNula(p) => write!(f, "{} não tem DACL (acesso total)", p.display()),
            Self::Escrita(p, s) => write!(f, "{} concede escrita a {s}", p.display()),
            Self::AceDesconhecida(p, t) => {
                write!(f, "{} tem ACE de tipo desconhecido ({t})", p.display())
            }
            Self::Demais => write!(f, "itens demais no diretório de estado"),
            Self::Io(p, e) => write!(f, "{}: {e}", p.display()),
        }
    }
}

/// O que fazer com um diretório inseguro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modo {
    /// Corrige (quarentena + diretório novo) ou falha. Processos privilegiados.
    Exigir,
    /// Só cria o diretório se faltar e relata os problemas, sem mexer em
    /// nada. Processos sem privilégio (desenvolvimento): não ganham nada que
    /// já não tenham.
    Avisar,
}

/// Resultado de [`preparar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Situacao {
    /// Já existia e passou na verificação.
    Seguro,
    /// Não existia e, em [`Modo::Avisar`], não foi criado.
    Ausente,
    /// Não existia; foi criado com o descritor seguro.
    Criado,
    /// Itens internos com dono errado (e só isso): Administradores tomou posse.
    PosseTomada { itens: usize },
    /// Estava inseguro; foi movido para `quarentena` e recriado.
    Corrigido {
        quarentena: PathBuf,
        problemas: Vec<Problema>,
    },
    /// Inseguro em [`Modo::Avisar`] (nada foi alterado).
    Inseguro(Vec<Problema>),
}

fn utf16(p: &Path) -> Vec<u16> {
    p.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// Lista legível dos primeiros problemas.
pub fn resumo(problemas: &[Problema]) -> String {
    let mut s: Vec<String> = problemas.iter().take(5).map(ToString::to_string).collect();
    if problemas.len() > 5 {
        s.push(format!("e mais {}", problemas.len() - 5));
    }
    s.join("; ")
}

/// Abre `p` sem seguir reparse points (o handle é do próprio link, se for um).
fn abrir_item(p: &Path) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(p)
}

fn info(f: &File) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
    // SAFETY: estrutura POD preenchida pela chamada; handle válido.
    let mut i: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(f.as_raw_handle() as HANDLE, &mut i) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(i)
}

fn sid_texto(sid: PSID) -> String {
    let mut s: windows_sys::core::PWSTR = null_mut();
    // SAFETY: `sid` válido; `s` recebe memória LocalAlloc, liberada abaixo.
    if unsafe { ConvertSidToStringSidW(sid, &mut s) } == 0 || s.is_null() {
        return "SID desconhecido".into();
    }
    // SAFETY: `s` é UTF-16 terminado em zero.
    let txt = unsafe {
        let mut n = 0;
        while *s.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(s, n))
    };
    // SAFETY: alocado por ConvertSidToStringSidW.
    unsafe { LocalFree(s.cast()) };
    txt
}

fn privilegiado(sid: PSID) -> bool {
    // SAFETY: `sid` aponta para um SID válido.
    unsafe {
        IsWellKnownSid(sid, WinLocalSystemSid) != 0
            || IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
    }
}

/// Confere dono e DACL de um item aberto.
fn conferir_seguranca(f: &File, p: &Path, problemas: &mut Vec<Problema>) {
    let mut dono: PSID = null_mut();
    let mut dacl: *mut ACL = null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: handle com READ_CONTROL; saídas apontam para dentro de `sd`.
    let r = unsafe {
        GetSecurityInfo(
            f.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut dono,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut sd,
        )
    };
    if r != 0 {
        problemas.push(Problema::Io(
            p.to_owned(),
            io::Error::from_raw_os_error(r as i32).to_string(),
        ));
        return;
    }
    if dono.is_null() || !privilegiado(dono) {
        let s = if dono.is_null() {
            "ninguém".into()
        } else {
            sid_texto(dono)
        };
        problemas.push(Problema::Dono(p.to_owned(), s));
    }
    if dacl.is_null() {
        problemas.push(Problema::DaclNula(p.to_owned()));
    } else {
        // SAFETY: `dacl` válido enquanto `sd` existir.
        let n = u32::from(unsafe { (*dacl).AceCount });
        for i in 0..n {
            let mut ace: *mut core::ffi::c_void = null_mut();
            // SAFETY: índice dentro de AceCount.
            if unsafe { GetAce(dacl, i, &mut ace) } == 0 || ace.is_null() {
                continue;
            }
            // SAFETY: toda ACE começa com ACE_HEADER.
            let cab = unsafe { *(ace as *const ACE_HEADER) };
            match cab.AceType {
                ACE_PERMITIR | ACE_PERMITIR_CALLBACK => {
                    // SAFETY: os dois tipos têm o layout de ACCESS_ALLOWED_ACE.
                    let a = ace as *const ACCESS_ALLOWED_ACE;
                    let mascara = unsafe { (*a).Mask };
                    let sid: PSID = unsafe { (&raw const (*a).SidStart) as PSID };
                    if mascara & DIREITOS_ESCRITA == 0 || privilegiado(sid) {
                        continue;
                    }
                    // CREATOR OWNER só herdável vira o dono do filho (SYSTEM
                    // ou Administradores, pois só eles criam itens aqui).
                    let herdavel_so = u32::from(cab.AceFlags) & INHERIT_ONLY_ACE != 0;
                    // SAFETY: `sid` válido.
                    if herdavel_so && unsafe { IsWellKnownSid(sid, WinCreatorOwnerSid) } != 0 {
                        continue;
                    }
                    problemas.push(Problema::Escrita(p.to_owned(), sid_texto(sid)));
                }
                ACE_NEGAR | ACE_NEGAR_CALLBACK => {}
                t => problemas.push(Problema::AceDesconhecida(p.to_owned(), t)),
            }
        }
    }
    // SAFETY: alocado por GetSecurityInfo.
    unsafe { LocalFree(sd) };
}

/// Caminho de `dir` relativo ao volume, em minúsculas e sem barra final
/// (`\programdata\nv-tuner`), no formato de `FindFirstFileNameW`.
fn caminho_no_volume(dir: &Path) -> Option<String> {
    let c = fs::canonicalize(dir).ok()?;
    let s = c.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    // Só caminhos com letra de unidade (`C:\...`); UNC fica de fora.
    let resto = s.get(2..).filter(|_| s.as_bytes().get(1) == Some(&b':'))?;
    Some(resto.trim_end_matches('\\').to_lowercase())
}

/// Todos os nomes (hard links) de `arquivo` estão dentro do diretório cujo
/// caminho no volume é `prefixo`? Em erro, `false`.
fn vinculos_internos(arquivo: &Path, prefixo: &str) -> bool {
    let nome = utf16(arquivo);
    let mut buf = vec![0u16; 1024];
    let dentro = |buf: &[u16], n: u32| {
        let n = (n as usize).min(buf.len());
        let s = String::from_utf16_lossy(&buf[..n]);
        let s = s.trim_end_matches('\0').to_lowercase();
        s.strip_prefix(prefixo).is_some_and(|r| r.starts_with('\\'))
    };
    let mut n = buf.len() as u32;
    // SAFETY: nome terminado em zero; `buf` com `n` posições.
    let h = unsafe { FindFirstFileNameW(nome.as_ptr(), 0, &mut n, buf.as_mut_ptr()) };
    if h == INVALID_HANDLE_VALUE {
        return false;
    }
    let mut ok = dentro(&buf, n);
    let mut vistos = 1;
    while ok && vistos < 64 {
        n = buf.len() as u32;
        // SAFETY: handle de busca válido; `buf` com `n` posições.
        if unsafe { FindNextFileNameW(h, &mut n, buf.as_mut_ptr()) } == 0 {
            break;
        }
        ok = dentro(&buf, n);
        vistos += 1;
    }
    if vistos >= 64 {
        ok = false;
    }
    // SAFETY: handle de busca aberto acima.
    unsafe { FindClose(h) };
    ok
}

/// Verifica `dir` (que precisa existir) e todo o conteúdo, sem seguir
/// reparse points. Lista vazia = seguro.
///
/// Item interno que some durante a verificação (rotação de log,
/// `estado.json.tmp` renomeado pelo motor) não é problema: o que não existe
/// não pode ser usado contra o serviço. Só a raiz sumida é erro.
pub fn verificar(dir: &Path) -> Vec<Problema> {
    verificar_com(dir, |_| {})
}

/// [`verificar`] com um gancho chamado antes de abrir cada item (testes de
/// corrida).
fn verificar_com(dir: &Path, mut antes_de_abrir: impl FnMut(&Path)) -> Vec<Problema> {
    let prefixo_volume = caminho_no_volume(dir);
    let mut problemas = Vec::new();
    let mut pendentes = vec![(dir.to_path_buf(), true)];
    let mut vistos = 0usize;
    while let Some((p, raiz)) = pendentes.pop() {
        vistos += 1;
        if vistos > ITENS_MAX {
            problemas.push(Problema::Demais);
            break;
        }
        antes_de_abrir(&p);
        let f = match abrir_item(&p) {
            Ok(f) => f,
            Err(e) if !raiz && e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => {
                problemas.push(Problema::Io(p, e.to_string()));
                continue;
            }
        };
        let i = match info(&f) {
            Ok(i) => i,
            Err(e) => {
                problemas.push(Problema::Io(p, e.to_string()));
                continue;
            }
        };
        let eh_dir = i.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
        if i.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            // Não entra nem confere o destino.
            problemas.push(Problema::ReparsePoint(p));
            continue;
        }
        if raiz && !eh_dir {
            problemas.push(Problema::NaoEDiretorio(p));
            continue;
        }
        // Vínculos internos (o motor publica o snapshot por hard link a
        // partir de um temporário) são aceitos; só um nome fora do
        // diretório é problema.
        if !eh_dir
            && i.nNumberOfLinks > 1
            && !prefixo_volume
                .as_deref()
                .is_some_and(|pre| vinculos_internos(&p, pre))
        {
            problemas.push(Problema::VariosVinculos(p.clone()));
        }
        conferir_seguranca(&f, &p, &mut problemas);
        drop(f);
        if eh_dir {
            match fs::read_dir(&p) {
                Ok(it) => {
                    for e in it {
                        match e {
                            Ok(e) => pendentes.push((e.path(), false)),
                            Err(e) => problemas.push(Problema::Io(p.clone(), e.to_string())),
                        }
                    }
                }
                Err(e) if !raiz && e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => problemas.push(Problema::Io(p.clone(), e.to_string())),
            }
        }
    }
    problemas
}

/// Cria `dir` (o pai precisa existir) com [`SDDL_ESTADO`]. Sem privilégio
/// para definir o dono, usa o dono padrão do processo.
pub fn criar_seguro(dir: &Path) -> io::Result<()> {
    match criar_com(dir, SDDL_ESTADO) {
        Err(e) if e.raw_os_error() == Some(ERROR_INVALID_OWNER as i32) => {
            criar_com(dir, SDDL_ESTADO_SEM_DONO)
        }
        r => r,
    }
}

fn criar_com(dir: &Path, sddl: &str) -> io::Result<()> {
    let sddl16: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: SDDL terminado em zero; `sd` recebe memória LocalAlloc.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl16.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd,
        bInheritHandle: 0,
    };
    let nome = utf16(dir);
    // SAFETY: ponteiros válidos durante a chamada. Falha se já existir
    // (inclusive como junção ou link): nada pré-existente é reaproveitado.
    let ok = unsafe { CreateDirectoryW(nome.as_ptr(), &sa) } != 0;
    let erro = io::Error::last_os_error();
    // SAFETY: alocado por ConvertStringSecurityDescriptor…
    unsafe { LocalFree(sd) };
    if ok { Ok(()) } else { Err(erro) }
}

fn quarentena_para(dir: &Path) -> PathBuf {
    let nome = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "estado".into());
    dir.with_file_name(format!(
        "{nome}.inseguro-{}-{}",
        crate::nucleo::agora_unix(),
        std::process::id()
    ))
}

/// Garante que `dir` é seguro antes do uso (ver o módulo). `Err` = o
/// processo deve recusar operar com o diretório.
pub fn preparar(dir: &Path, modo: Modo) -> Result<Situacao, String> {
    let existe = match fs::symlink_metadata(dir) {
        Ok(_) => true,
        Err(e) if e.kind() == io::ErrorKind::NotFound => false,
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    if !existe {
        if modo == Modo::Avisar {
            // Sem privilégio, criar aqui deixaria o usuário como dono.
            return Ok(Situacao::Ausente);
        }
        if let Some(pai) = dir.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(pai).map_err(|e| format!("{}: {e}", pai.display()))?;
        }
        criar_seguro(dir).map_err(|e| format!("criar {}: {e}", dir.display()))?;
        let problemas = verificar(dir);
        if problemas.is_empty() {
            return Ok(Situacao::Criado);
        }
        return Err(format!(
            "diretório de estado recém-criado continua inseguro: {}",
            resumo(&problemas)
        ));
    }
    let mut problemas = verificar(dir);
    // Falha de leitura pode ser passageira (item trocado no meio da
    // verificação): confere de novo antes de qualquer correção.
    if problemas.iter().any(|x| matches!(x, Problema::Io(..))) {
        problemas = verificar(dir);
    }
    if problemas.is_empty() {
        return Ok(Situacao::Seguro);
    }
    if modo == Modo::Avisar {
        return Ok(Situacao::Inseguro(problemas));
    }
    // Só o dono de itens internos está errado (a raiz é segura e nenhuma
    // DACL concede escrita a outros): foram criados por um administrador
    // elevado cujo dono padrão é o próprio usuário. Basta tomar posse.
    let so_dono_interno = problemas
        .iter()
        .all(|x| matches!(x, Problema::Dono(p, _) if p != dir));
    if so_dono_interno {
        let mut falhas = Vec::new();
        for x in &problemas {
            if let Problema::Dono(p, _) = x
                && let Err(e) = tomar_posse(p)
            {
                falhas.push(format!("{}: {e}", p.display()));
            }
        }
        if falhas.is_empty() && verificar(dir).is_empty() {
            return Ok(Situacao::PosseTomada {
                itens: problemas.len(),
            });
        }
    }
    let quarentena = quarentena_para(dir);
    // Renomear não segue junções: move o próprio link, se `dir` for um.
    fs::rename(dir, &quarentena).map_err(|e| {
        format!(
            "diretório de estado inseguro ({}) e não foi possível movê-lo para {}: {e}",
            resumo(&problemas),
            quarentena.display()
        )
    })?;
    criar_seguro(dir).map_err(|e| {
        format!(
            "diretório de estado inseguro movido para {}, mas recriá-lo falhou: {e}",
            quarentena.display()
        )
    })?;
    let depois = verificar(dir);
    if !depois.is_empty() {
        return Err(format!(
            "diretório de estado recriado continua inseguro: {}",
            resumo(&depois)
        ));
    }
    // O snapshot original (se havia) ficou na quarentena e não é confiável:
    // sem este marcador, a próxima aplicação capturaria como "original" um
    // driver que pode já ter mudanças do app. Com ele, o motor avisa e trata
    // o snapshot novo como base pós-quarentena.
    nvt_engine::Armazem::marcar_snapshot_perdido(
        dir,
        &format!(
            "quarentena em {} ({})",
            quarentena.display(),
            resumo(&problemas)
        ),
    )
    .map_err(|e| {
        format!(
            "diretório de estado movido para {}, mas marcar o snapshot original como \
             perdido falhou: {e}",
            quarentena.display()
        )
    })?;
    Ok(Situacao::Corrigido {
        quarentena,
        problemas,
    })
}

/// Abre um arquivo sem seguir reparse points e recusa o que não for um
/// arquivo comum com um único vínculo. `opcoes` define leitura/escrita/criação.
pub fn abrir_sem_reparse(caminho: &Path, opcoes: &mut OpenOptions) -> io::Result<File> {
    let f = opcoes
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(caminho)?;
    let i = info(&f)?;
    if i.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other(format!(
            "{} é junção/link; recusado",
            caminho.display()
        )));
    }
    if i.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 || i.nNumberOfLinks > 1 {
        return Err(io::Error::other(format!(
            "{} não é um arquivo comum com um único vínculo; recusado",
            caminho.display()
        )));
    }
    Ok(f)
}

/// O diretório existe e não é reparse point (para subpastas como `logs`).
pub fn diretorio_comum(p: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(p) {
        Ok(m) => {
            use std::os::windows::fs::MetadataExt;
            Ok(m.is_dir() && m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// SID de Administradores (`S-1-5-32-544`) num buffer próprio.
fn sid_administradores() -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; SECURITY_MAX_SID_SIZE as usize];
    let mut n = SECURITY_MAX_SID_SIZE;
    // SAFETY: buffer do tamanho máximo de um SID.
    if unsafe {
        CreateWellKnownSid(
            WinBuiltinAdministratorsSid,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut n,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    buf.truncate(n as usize);
    Ok(buf)
}

/// Torna Administradores o dono de `p` (sem seguir reparse points).
fn tomar_posse(p: &Path) -> io::Result<()> {
    let f = OpenOptions::new()
        .access_mode(READ_CONTROL | WRITE_OWNER | FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(p)?;
    if info(&f)?.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("reparse point"));
    }
    let mut sid = sid_administradores()?;
    // SAFETY: handle com WRITE_OWNER; `sid` válido durante a chamada.
    let r = unsafe {
        SetSecurityInfo(
            f.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            sid.as_mut_ptr().cast(),
            null_mut(),
            null(),
            null(),
        )
    };
    if r != 0 {
        return Err(io::Error::from_raw_os_error(r as i32));
    }
    Ok(())
}

/// Faz de Administradores o dono padrão dos objetos que este processo criar
/// (num administrador elevado, o padrão pode ser o próprio usuário, que
/// manteria `WRITE_DAC` implícito sobre os arquivos de estado mesmo sem
/// elevação). Só funciona em processo privilegiado.
pub fn definir_dono_padrao() -> io::Result<()> {
    let mut sid = sid_administradores()?;
    let mut token: HANDLE = null_mut();
    // SAFETY: pseudo-handle do processo atual; `token` recebe um handle próprio.
    if unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_DEFAULT | TOKEN_QUERY,
            &mut token,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let dono = TOKEN_OWNER {
        Owner: sid.as_mut_ptr().cast(),
    };
    // SAFETY: estrutura válida durante a chamada; o token é fechado em seguida.
    let ok = unsafe {
        SetTokenInformation(
            token,
            TokenOwner,
            (&raw const dono).cast(),
            size_of::<TOKEN_OWNER>() as u32,
        )
    } != 0;
    let erro = io::Error::last_os_error();
    // SAFETY: handle aberto acima.
    unsafe { CloseHandle(token) };
    if ok { Ok(()) } else { Err(erro) }
}

/// Processo com privilégio de administrador (inclui SYSTEM).
pub fn processo_privilegiado() -> bool {
    // SAFETY: sem pré-condições.
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

/// Modo adequado ao processo atual.
pub fn modo_do_processo() -> Modo {
    if processo_privilegiado() {
        Modo::Exigir
    } else {
        Modo::Avisar
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct Temp(PathBuf);

    impl Temp {
        fn novo(nome: &str) -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let p = std::env::temp_dir().join(format!(
                "nvt-dir-{nome}-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn juncao(link: &Path, alvo: &Path) {
        let s = Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(alvo)
            .output()
            .expect("cmd");
        assert!(s.status.success(), "mklink /J falhou: {s:?}");
    }

    #[test]
    fn juncao_e_detectada_sem_ser_seguida() {
        let t = Temp::novo("juncao");
        let estado = t.0.join("estado");
        let alvo = t.0.join("alvo");
        fs::create_dir_all(estado.join("logs-real")).unwrap();
        fs::create_dir_all(&alvo).unwrap();
        fs::write(alvo.join("segredo.txt"), "x").unwrap();
        juncao(&estado.join("logs"), &alvo);
        let p = verificar(&estado);
        assert!(
            p.contains(&Problema::ReparsePoint(estado.join("logs"))),
            "junção não detectada: {p:?}"
        );
        // Não seguiu a junção: nada do alvo aparece.
        assert!(
            !p.iter().any(|x| x.to_string().contains("segredo")),
            "seguiu a junção: {p:?}"
        );
    }

    #[test]
    fn diretorio_raiz_como_juncao_e_detectado() {
        let t = Temp::novo("raiz");
        let alvo = t.0.join("alvo");
        fs::create_dir_all(&alvo).unwrap();
        let estado = t.0.join("estado");
        juncao(&estado, &alvo);
        assert_eq!(
            verificar(&estado),
            vec![Problema::ReparsePoint(estado.clone())]
        );
        assert!(!diretorio_comum(&estado).unwrap());
        assert!(diretorio_comum(&alvo).unwrap());
    }

    #[test]
    fn hard_link_e_detectado_e_recusado_na_abertura() {
        let t = Temp::novo("vinculo");
        let estado = t.0.join("estado");
        fs::create_dir_all(&estado).unwrap();
        let fora = t.0.join("fora.txt");
        fs::write(&fora, "x").unwrap();
        let dentro = estado.join("estado.json");
        fs::hard_link(&fora, &dentro).unwrap();
        assert!(verificar(&estado).contains(&Problema::VariosVinculos(dentro.clone())));
        // Vínculo só interno (snapshot publicado por hard link) é aceito.
        let interno = t.0.join("interno");
        fs::create_dir_all(&interno).unwrap();
        fs::write(interno.join("snapshot-original.json.tmp"), "s").unwrap();
        fs::hard_link(
            interno.join("snapshot-original.json.tmp"),
            interno.join("snapshot-original.json"),
        )
        .unwrap();
        assert!(
            !verificar(&interno)
                .iter()
                .any(|x| matches!(x, Problema::VariosVinculos(_))),
            "vínculo interno não deveria ser problema"
        );
        assert!(abrir_sem_reparse(&dentro, OpenOptions::new().read(true)).is_err());
        assert!(abrir_sem_reparse(&estado.join("x"), OpenOptions::new().read(true)).is_err());
        let novo = estado.join("novo.txt");
        fs::write(&novo, "y").unwrap();
        assert!(abrir_sem_reparse(&novo, OpenOptions::new().read(true)).is_ok());
    }

    #[test]
    fn link_simbolico_e_recusado_na_abertura() {
        let t = Temp::novo("symlink");
        let alvo = t.0.join("alvo.txt");
        fs::write(&alvo, "x").unwrap();
        let link = t.0.join("link.txt");
        // Links simbólicos exigem privilégio ou modo de desenvolvedor.
        if std::os::windows::fs::symlink_file(&alvo, &link).is_err() {
            return;
        }
        let e = abrir_sem_reparse(&link, OpenOptions::new().append(true)).unwrap_err();
        assert!(e.to_string().contains("junção/link"), "{e}");
        assert_eq!(fs::read_to_string(&alvo).unwrap(), "x");
    }

    #[test]
    fn pasta_temporaria_comum_nao_passa() {
        // A pasta temporária do usuário pertence ao usuário (ou, elevado, a
        // Administradores) e concede escrita a ele: nunca é "segura".
        let t = Temp::novo("comum");
        let p = verificar(&t.0);
        assert!(
            p.iter()
                .any(|x| matches!(x, Problema::Dono(..) | Problema::Escrita(..))),
            "{p:?}"
        );
    }

    #[test]
    fn criado_com_descritor_seguro_so_tem_problema_de_dono_sem_privilegio() {
        let t = Temp::novo("criado");
        let estado = t.0.join("estado");
        assert_eq!(preparar(&estado, Modo::Avisar), Ok(Situacao::Ausente));
        assert!(!estado.exists(), "Avisar não cria");
        criar_seguro(&estado).unwrap();
        let p = verificar(&estado);
        if processo_privilegiado() {
            assert!(p.is_empty(), "{p:?}");
        } else {
            // Sem privilégio, só o dono (o próprio usuário) falha: a DACL
            // protegida não concede escrita a ninguém além de SY/BA.
            assert!(p.iter().all(|x| matches!(x, Problema::Dono(..))), "{p:?}");
        }
        // Arquivos criados dentro herdam a DACL protegida (sem privilégio,
        // nem o dono consegue gravar nele: só SY/BA escrevem).
        if processo_privilegiado() {
            definir_dono_padrao().unwrap();
            fs::write(estado.join("estado.json"), "{}").unwrap();
            let p = verificar(&estado);
            assert!(p.is_empty(), "{p:?}");
        } else {
            assert!(fs::write(estado.join("estado.json"), "{}").is_err());
        }
        // Já existe: criar de novo falha (nada pré-existente é reaproveitado).
        assert!(criar_seguro(&estado).is_err());
    }

    #[test]
    fn exigir_poe_em_quarentena_e_recria() {
        let t = Temp::novo("quarentena");
        let estado = t.0.join("nv-tuner");
        let alvo = t.0.join("alvo");
        fs::create_dir_all(&estado).unwrap();
        fs::create_dir_all(&alvo).unwrap();
        fs::write(estado.join("snapshot-original.json"), "forjado").unwrap();
        juncao(&estado.join("logs"), &alvo);
        let r = preparar(&estado, Modo::Exigir);
        // Sem privilégio o diretório recriado tem o usuário como dono e a
        // verificação final falha; com privilégio, fica seguro. Nos dois
        // casos o conteúdo inseguro foi para a quarentena.
        let quarentenas: Vec<_> = fs::read_dir(&t.0)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("nv-tuner.inseguro-")
            })
            .collect();
        assert_eq!(quarentenas.len(), 1);
        let q = quarentenas[0].path();
        assert_eq!(
            fs::read_to_string(q.join("snapshot-original.json")).unwrap(),
            "forjado"
        );
        assert!(diretorio_comum(&estado).unwrap());
        let restantes: Vec<String> = fs::read_dir(&estado)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "snapshot-perdido.json")
            .collect();
        assert!(
            restantes.is_empty(),
            "recriado vazio (só o marcador): {restantes:?}"
        );
        if processo_privilegiado() {
            assert!(matches!(r, Ok(Situacao::Corrigido { .. })), "{r:?}");
            // Reverificação, item 6: o próximo ciclo não captura em silêncio
            // um "original" que pode já ter mudanças do app.
            assert!(estado.join("snapshot-perdido.json").exists());
            let a = nvt_engine::Armazem::abrir(&estado).unwrap();
            assert!(a.base_pos_quarentena().is_some());
            assert!(
                a.avisos()
                    .iter()
                    .any(|x| x.codigo == nvt_engine::CodigoAviso::SnapshotOriginalPerdido),
                "{:?}",
                a.avisos()
            );
        } else {
            assert!(r.unwrap_err().contains("continua inseguro"));
        }
        // O alvo da junção ficou intacto.
        assert!(alvo.exists());
    }

    /// Reverificação, item 6: arquivo que some entre a listagem e a abertura
    /// (rotação de log, `estado.json.tmp`) não é problema nem leva à
    /// quarentena.
    #[test]
    fn arquivo_que_some_durante_a_verificacao_nao_e_problema() {
        let t = Temp::novo("some");
        let estado = t.0.join("nv-tuner");
        fs::create_dir_all(estado.join("logs")).unwrap();
        let tmp = estado.join("estado.json.tmp");
        let log = estado.join("logs").join("nv-tuner.log.1");
        fs::write(&tmp, "x").unwrap();
        fs::write(&log, "y").unwrap();
        let mut sumidos = 0;
        let p = verificar_com(&estado, |item| {
            if item == tmp || item == log {
                fs::remove_file(item).unwrap();
                sumidos += 1;
            }
        });
        assert_eq!(sumidos, 2);
        assert!(
            !p.iter().any(|x| matches!(x, Problema::Io(..))),
            "sumiço tratado como erro: {p:?}"
        );
        // Diretório interno que some antes de ser listado também é benigno.
        let sub = estado.join("logs");
        let p = verificar_com(&estado, |item| {
            if item == sub {
                fs::remove_dir_all(item).unwrap();
            }
        });
        assert!(!p.iter().any(|x| matches!(x, Problema::Io(..))), "{p:?}");
        // A raiz sumida continua sendo erro.
        let p = verificar_com(&estado, |item| {
            if item == estado {
                fs::remove_dir_all(item).unwrap();
            }
        });
        assert!(matches!(p.as_slice(), [Problema::Io(..)]), "{p:?}");
    }

    /// Reverificação, item 6, com privilégio: diretório seguro com um
    /// arquivo que some no meio da verificação continua `Seguro`, sem
    /// quarentena.
    #[test]
    fn exigir_nao_poe_em_quarentena_por_arquivo_sumido() {
        if !processo_privilegiado() {
            return; // criar com O:BA e gravar dentro exigem privilégio
        }
        definir_dono_padrao().unwrap();
        let t = Temp::novo("sumido-exigir");
        let estado = t.0.join("nv-tuner");
        criar_seguro(&estado).unwrap();
        fs::write(estado.join("estado.json"), "{}").unwrap();
        fs::write(estado.join("estado.json.tmp"), "{}").unwrap();
        // Simula o sumiço entre duas verificações: a primeira vê o arquivo,
        // ele some, e o resultado final não pode ser quarentena.
        let p = verificar_com(&estado, |item| {
            if item.ends_with("estado.json.tmp") {
                fs::remove_file(item).unwrap();
            }
        });
        assert!(p.is_empty(), "{p:?}");
        assert_eq!(preparar(&estado, Modo::Exigir), Ok(Situacao::Seguro));
        let quarentenas = fs::read_dir(&t.0)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".inseguro-"))
            .count();
        assert_eq!(quarentenas, 0);
    }

    #[test]
    fn dono_errado_so_em_item_interno_toma_posse() {
        if !processo_privilegiado() {
            return; // definir dono e criar com O:BA exigem privilégio
        }
        let t = Temp::novo("posse");
        let estado = t.0.join("nv-tuner");
        criar_seguro(&estado).unwrap();
        let arq = estado.join("estado.json");
        fs::write(&arq, "{}").unwrap();
        let usuario = std::env::var("USERNAME").unwrap();
        let s = Command::new("icacls")
            .arg(&arq)
            .arg("/setowner")
            .arg(&usuario)
            .output()
            .unwrap();
        assert!(s.status.success(), "{s:?}");
        assert!(matches!(verificar(&estado).as_slice(), [Problema::Dono(p, _)] if *p == arq));
        assert_eq!(
            preparar(&estado, Modo::Exigir),
            Ok(Situacao::PosseTomada { itens: 1 })
        );
        assert!(verificar(&estado).is_empty());
        assert_eq!(
            fs::read_to_string(&arq).unwrap(),
            "{}",
            "conteúdo preservado"
        );
    }

    #[test]
    fn avisar_nao_altera_nada() {
        let t = Temp::novo("avisar");
        let estado = t.0.join("nv-tuner");
        fs::create_dir_all(&estado).unwrap();
        fs::write(estado.join("estado.json"), "{}").unwrap();
        let r = preparar(&estado, Modo::Avisar).unwrap();
        assert!(
            matches!(r, Situacao::Inseguro(ref p) if !p.is_empty()),
            "{r:?}"
        );
        assert!(estado.join("estado.json").exists());
        assert_eq!(fs::read_dir(&t.0).unwrap().count(), 1);
    }
}
