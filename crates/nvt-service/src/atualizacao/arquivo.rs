//! Operações de arquivo da atualização: pasta protegida, `.parcial`, hash e renomeação pelo handle (spec §9.2).
//!
//! Fluxo (U-20..U-25):
//!
//! 1. [`preparar_pasta`]: nenhum componente do caminho é reparse point e a pasta
//!    tem dono Administradores e DACL protegida só SYSTEM/Administradores
//!    ([`crate::diretorio::preparar_subpasta_privada`]);
//! 2. [`Download::criar`]: `nv-tuner-X.Y.Z-x64.msi.parcial` com `CREATE_NEW`,
//!    `FILE_FLAG_OPEN_REPARSE_POINT` e compartilhamento só de leitura (ninguém
//!    mais escreve nem apaga enquanto o handle existir);
//! 3. [`Download::concluir`]: SHA-256 e tamanho lidos **pelo mesmo handle**;
//! 4. [`Baixado::finalizar`]: renomeia pelo handle (sem substituir) para o nome
//!    final e troca o handle de escrita por um só de leitura que nega escrita e
//!    exclusão a terceiros ([`MsiProtegido`]), mantido até o msiexec abrir.
//!
//! *Troca de handle.* `ReOpenFile(GENERIC_READ, FILE_SHARE_READ)` direto do handle
//! de escrita é impossível (o novo handle negaria a escrita que o antigo tem:
//! violação de compartilhamento). Por isso a troca passa por um handle de
//! leitura intermediário que compartilha tudo, aberto por `ReOpenFile` (mesmo
//! objeto, sem resolver caminho); fecha-se o de escrita; o handle final é
//! `ReOpenFile(intermediário, GENERIC_READ, FILE_SHARE_READ)`, que só abre se
//! nenhum outro handle tiver escrita/exclusão; e o SHA-256 e o tamanho são
//! recalculados pelo handle final e comparados. Uma escrita na janela (que exige
//! já poder escrever na pasta, ou seja, ser administrador) é detectada.
// Integrado ao serviço no lote 3.
#![allow(dead_code)]

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{Component, Path, PathBuf};
use std::ptr::null_mut;

use nvt_atualizacao::{LIMITE_MSI, Versao, nome_log, nome_msi, sha256_hex};
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FileRenameInfo, GetFileInformationByHandle, GetFinalPathNameByHandleW,
    ReOpenFile, SetFileInformationByHandle,
};
use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramData, SHGetKnownFolderPath};

const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const DELETE: u32 = 0x0001_0000;
const SUFIXO_PARCIAL: &str = ".parcial";

#[link(name = "ole32")]
unsafe extern "system" {
    fn CoTaskMemFree(pv: *const core::ffi::c_void);
}

fn recusa(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, msg.into())
}

/// `%ProgramData%` por `SHGetKnownFolderPath(FOLDERID_ProgramData)` (nunca pela
/// variável de ambiente, U-31).
pub(crate) fn pasta_programdata() -> io::Result<PathBuf> {
    let mut p: windows_sys::core::PWSTR = null_mut();
    // SAFETY: GUID estático; `p` recebe memória do CoTaskMemAlloc.
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramData, 0, null_mut(), &mut p) };
    let r = if hr < 0 || p.is_null() {
        Err(io::Error::other(format!(
            "SHGetKnownFolderPath(ProgramData) falhou: 0x{hr:08x}"
        )))
    } else {
        // SAFETY: cadeia terminada em zero devolvida pela API.
        let n = (0..).take_while(|&i| unsafe { *p.add(i) } != 0).count();
        let s = unsafe { std::slice::from_raw_parts(p, n) };
        Ok(PathBuf::from(std::ffi::OsString::from_wide(s)))
    };
    // SAFETY: liberar mesmo em falha é permitido (nulo é ignorado).
    unsafe { CoTaskMemFree(p.cast()) };
    r
}

/// `%ProgramData%\nv-tuner\atualizacao`.
pub(crate) fn pasta_padrao() -> io::Result<PathBuf> {
    Ok(pasta_programdata()?.join("nv-tuner").join("atualizacao"))
}

/// Recusa caminho relativo ou com algum componente existente que seja reparse
/// point (junção, link simbólico), da unidade até `dir`.
pub(crate) fn componentes_sem_reparse(dir: &Path) -> io::Result<()> {
    if !dir.is_absolute() {
        return Err(recusa(format!("{} não é absoluto", dir.display())));
    }
    let mut atual = PathBuf::new();
    for c in dir.components() {
        if matches!(c, Component::ParentDir | Component::CurDir) {
            return Err(recusa(format!("{} tem `.`/`..`", dir.display())));
        }
        atual.push(c);
        if !matches!(c, Component::Normal(_)) {
            continue;
        }
        match fs::symlink_metadata(&atual) {
            Ok(m) if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 => {
                return Err(recusa(format!(
                    "{} é junção/link (reparse point); recusado",
                    atual.display()
                )));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Caminho de diretório absoluto e canônico, sem `\\?\`.
fn canonico(dir: &Path) -> io::Result<PathBuf> {
    let c = fs::canonicalize(dir)?;
    let s = c.to_string_lossy();
    Ok(PathBuf::from(
        s.strip_prefix(r"\\?\").unwrap_or(&s).to_owned(),
    ))
}

/// Prepara e confere a pasta de trabalho (U-20): componentes sem reparse point,
/// dono e DACL esperados. Devolve o caminho canônico, sem `\\?\`.
pub(crate) fn preparar_pasta(dir: &Path) -> io::Result<PathBuf> {
    componentes_sem_reparse(dir)?;
    if let Some(pai) = dir.parent() {
        fs::create_dir_all(pai)?;
    }
    crate::diretorio::preparar_subpasta_privada(dir).map_err(|p| {
        recusa(format!(
            "pasta da atualização insegura: {}",
            crate::diretorio::resumo(&p)
        ))
    })?;
    componentes_sem_reparse(dir)?;
    canonico(dir)
}

/// Caminho pronto para a linha de comando do msiexec (U-31): sem `\\?\`,
/// começando por letra de unidade + `:\`, sem `"`, `%` nem caracteres de
/// controle.
pub(crate) fn caminho_para_comando(p: &Path) -> io::Result<String> {
    let s = p
        .to_str()
        .ok_or_else(|| recusa(format!("{} não é Unicode válido", p.display())))?;
    let s = s.strip_prefix(r"\\?\").unwrap_or(s);
    let b = s.as_bytes();
    let unidade = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\';
    if !unidade {
        return Err(recusa(format!("{s} não começa por unidade (X:\\)")));
    }
    if s.chars().any(|c| c == '"' || c == '%' || c.is_control()) {
        return Err(recusa(format!("{s} tem aspas, % ou caractere de controle")));
    }
    if s.contains('/') || s[2..].contains(':') {
        return Err(recusa(format!("{s} tem separador ou `:` inesperado")));
    }
    Ok(s.to_owned())
}

/// Caminho do log do MSI (`<pasta>\instalacao-X.Y.Z.log`) para o comando.
pub(crate) fn caminho_log(dir: &Path, versao: Versao) -> io::Result<String> {
    caminho_para_comando(&dir.join(nome_log(versao)))
}

fn info(f: &File) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
    // SAFETY: estrutura POD preenchida pela chamada; handle válido.
    let mut i: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(f.as_raw_handle() as HANDLE, &mut i) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(i)
}

fn caminho_final(f: &File) -> io::Result<String> {
    let mut buf = vec![0u16; 1024];
    loop {
        // SAFETY: buffer com `len` posições.
        let n = unsafe {
            GetFinalPathNameByHandleW(
                f.as_raw_handle() as HANDLE,
                buf.as_mut_ptr(),
                buf.len() as u32,
                0,
            )
        } as usize;
        if n == 0 {
            return Err(io::Error::last_os_error());
        }
        if n < buf.len() {
            let s = String::from_utf16_lossy(&buf[..n]);
            return Ok(s.strip_prefix(r"\\?\").unwrap_or(&s).to_owned());
        }
        buf.resize(n + 1, 0);
    }
}

/// O handle é de um arquivo comum, com um vínculo, exatamente em `esperado`.
fn conferir_handle(f: &File, esperado: &Path) -> io::Result<()> {
    let i = info(f)?;
    if i.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || i.nNumberOfLinks != 1
    {
        return Err(recusa(format!(
            "{} não é um arquivo comum com um vínculo",
            esperado.display()
        )));
    }
    let real = caminho_final(f)?;
    if !real.eq_ignore_ascii_case(&esperado.to_string_lossy()) {
        return Err(recusa(format!(
            "{} foi aberto em {real}; recusado",
            esperado.display()
        )));
    }
    Ok(())
}

/// Lê o arquivo inteiro pelo handle (até [`LIMITE_MSI`]): (SHA-256, tamanho).
fn hash_pelo_handle(f: &mut File) -> io::Result<(String, u64)> {
    f.seek(SeekFrom::Start(0))?;
    let mut dados = Vec::new();
    Read::by_ref(f)
        .take(LIMITE_MSI + 1)
        .read_to_end(&mut dados)?;
    let tamanho = dados.len() as u64;
    if tamanho > LIMITE_MSI {
        return Err(io::Error::other("MSI acima do limite"));
    }
    Ok((sha256_hex(&dados), tamanho))
}

fn reabrir(f: &File, acesso: u32, compartilhamento: u32) -> io::Result<File> {
    // SAFETY: handle válido; o novo handle passa a ser de posse do File.
    let h = unsafe {
        ReOpenFile(
            f.as_raw_handle() as HANDLE,
            acesso,
            compartilhamento,
            FILE_FLAG_OPEN_REPARSE_POINT,
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_handle(h) })
}

/// Renomeia pelo handle (precisa de `DELETE`) para o caminho Win32 completo `destino`, sem
/// substituir.
fn renomear_pelo_handle(f: &File, destino: &Path) -> io::Result<()> {
    let nome16: Vec<u16> = destino.as_os_str().encode_wide().collect();
    let base = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
    let bytes = base + (nome16.len() + 1) * 2;
    let mut buf = vec![0u64; bytes.div_ceil(8)];
    let info = buf.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    // SAFETY: buffer alinhado e grande o bastante para o cabeçalho e o nome.
    unsafe {
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).RootDirectory = null_mut();
        (*info).FileNameLength = (nome16.len() * 2) as u32;
        let destino = (&raw mut (*info).FileName).cast::<u16>();
        std::ptr::copy_nonoverlapping(nome16.as_ptr(), destino, nome16.len());
        if SetFileInformationByHandle(
            f.as_raw_handle() as HANDLE,
            FileRenameInfo,
            info.cast(),
            bytes as u32,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// `.parcial` em download, com handle exclusivo de escrita (U-21).
#[derive(Debug)]
pub(crate) struct Download {
    arquivo: Option<File>,
    dir: PathBuf,
    parcial: PathBuf,
    versao: Versao,
}

impl Download {
    /// Cria `<dir>\nv-tuner-X.Y.Z-x64.msi.parcial` (`dir` vindo de
    /// [`preparar_pasta`]); recusa entrada existente e reparse points.
    pub(crate) fn criar(dir: &Path, versao: Versao) -> io::Result<Download> {
        componentes_sem_reparse(dir)?;
        let dir = canonico(dir)?;
        let parcial = dir.join(format!("{}{SUFIXO_PARCIAL}", nome_msi(versao)));
        let f = OpenOptions::new()
            .access_mode(GENERIC_READ | GENERIC_WRITE | DELETE)
            .share_mode(FILE_SHARE_READ)
            .create_new(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&parcial)?;
        let mut d = Download {
            arquivo: Some(f),
            dir,
            parcial,
            versao,
        };
        if let Err(e) = conferir_handle(d.arquivo.as_ref().expect("aberto"), &d.parcial) {
            d.descartar();
            return Err(e);
        }
        Ok(d)
    }

    /// Handle de escrita do download.
    pub(crate) fn arquivo(&mut self) -> &mut File {
        self.arquivo.as_mut().expect("aberto até concluir")
    }

    /// Caminho do `.parcial`.
    pub(crate) fn caminho(&self) -> &Path {
        &self.parcial
    }

    /// Fim do download: SHA-256 e tamanho pelo mesmo handle. Em erro, o
    /// `.parcial` é apagado (U-24).
    pub(crate) fn concluir(mut self) -> io::Result<Baixado> {
        let mut f = self.arquivo.take().expect("aberto até concluir");
        let r = f.flush().and_then(|()| hash_pelo_handle(&mut f));
        match r {
            Ok((sha256, tamanho)) => Ok(Baixado {
                arquivo: Some(f),
                dir: std::mem::take(&mut self.dir),
                parcial: std::mem::take(&mut self.parcial),
                versao: self.versao,
                sha256,
                tamanho,
            }),
            Err(e) => {
                drop(f);
                let _ = fs::remove_file(&self.parcial);
                Err(e)
            }
        }
    }

    /// Fecha e apaga o `.parcial`.
    pub(crate) fn descartar(&mut self) {
        if self.arquivo.take().is_some() {
            let _ = fs::remove_file(&self.parcial);
        }
    }
}

impl Drop for Download {
    fn drop(&mut self) {
        self.descartar();
    }
}

/// `.parcial` completo, ainda com o handle de escrita, hash já calculado.
#[derive(Debug)]
pub(crate) struct Baixado {
    arquivo: Option<File>,
    dir: PathBuf,
    parcial: PathBuf,
    versao: Versao,
    sha256: String,
    tamanho: u64,
}

impl Baixado {
    /// SHA-256 (hex minúsculo) lido pelo handle do download.
    pub(crate) fn sha256(&self) -> &str {
        &self.sha256
    }

    pub(crate) fn tamanho(&self) -> u64 {
        self.tamanho
    }

    /// Fecha e apaga o `.parcial` (verificação falhou).
    pub(crate) fn descartar(mut self) {
        self.apagar();
    }

    fn apagar(&mut self) {
        if self.arquivo.take().is_some() {
            let _ = fs::remove_file(&self.parcial);
        }
    }
    /// Já verificado: renomeia pelo handle para o nome final (U-22, sem
    /// substituir) e troca para um handle só de leitura que nega escrita e
    /// exclusão a terceiros; confere de novo hash e tamanho por ele. Em erro,
    /// apaga só o arquivo deste download (nunca um MSI pré-existente).
    pub(crate) fn finalizar(mut self) -> io::Result<MsiProtegido> {
        let destino = self.dir.join(nome_msi(self.versao));
        let escrita = self.arquivo.as_ref().expect("aberto até finalizar");
        if let Err(e) = renomear_pelo_handle(escrita, &destino) {
            self.apagar();
            return Err(e);
        }
        // Daqui em diante o arquivo deste download tem o nome final.
        self.parcial = destino.clone();
        let r = self.trocar_handle(&destino);
        if r.is_err() {
            if self.arquivo.is_some() {
                self.apagar();
            } else {
                let _ = fs::remove_file(&destino);
            }
        }
        r
    }

    fn trocar_handle(&mut self, destino: &Path) -> io::Result<MsiProtegido> {
        let escrita = self.arquivo.as_ref().expect("aberto até finalizar");
        conferir_handle(escrita, destino)?;
        let ponte = reabrir(
            escrita,
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        )?;
        drop(self.arquivo.take());
        let mut leitura = reabrir(&ponte, GENERIC_READ, FILE_SHARE_READ)?;
        drop(ponte);
        conferir_handle(&leitura, destino)?;
        let (sha256, tamanho) = hash_pelo_handle(&mut leitura)?;
        if sha256 != self.sha256 || tamanho != self.tamanho {
            return Err(recusa("MSI mudou durante a troca de handle"));
        }
        Ok(MsiProtegido {
            leitura,
            caminho: destino.to_owned(),
            sha256,
            tamanho,
        })
    }
}
impl Drop for Baixado {
    fn drop(&mut self) {
        self.apagar();
    }
}

/// MSI final, aberto só para leitura negando escrita e exclusão a terceiros.
/// Manter vivo até o msiexec abrir o pacote (U-22).
#[derive(Debug)]
pub(crate) struct MsiProtegido {
    leitura: File,
    caminho: PathBuf,
    sha256: String,
    tamanho: u64,
}

impl MsiProtegido {
    pub(crate) fn caminho(&self) -> &Path {
        &self.caminho
    }

    /// Caminho do MSI para a linha de comando (U-31).
    pub(crate) fn caminho_comando(&self) -> io::Result<String> {
        caminho_para_comando(&self.caminho)
    }

    pub(crate) fn sha256(&self) -> &str {
        &self.sha256
    }

    pub(crate) fn tamanho(&self) -> u64 {
        self.tamanho
    }

    pub(crate) fn handle(&self) -> &File {
        &self.leitura
    }
}

fn eh_msi(n: &str) -> bool {
    n.starts_with("nv-tuner-") && n.ends_with("-x64.msi")
}

fn eh_log(n: &str) -> bool {
    n.starts_with("instalacao-") && n.ends_with(".log")
}

/// Limpeza (U-25): apaga `*.parcial` e MSIs/logs que não sejam de `manter`;
/// nunca segue links (um reparse point com esses nomes é apagado como link,
/// sem tocar no destino) e não desce em subpastas. Devolve o que apagou.
pub(crate) fn limpar(dir: &Path, manter: Option<Versao>) -> io::Result<Vec<PathBuf>> {
    componentes_sem_reparse(dir)?;
    let (msi, log) = match manter {
        Some(v) => (nome_msi(v), nome_log(v)),
        None => (String::new(), String::new()),
    };
    let mut apagados = Vec::new();
    for e in fs::read_dir(dir)? {
        let e = e?;
        let nome = e.file_name();
        let Some(n) = nome.to_str() else { continue };
        let lixo =
            n.ends_with(SUFIXO_PARCIAL) || (eh_msi(n) && n != msi) || (eh_log(n) && n != log);
        if !lixo {
            continue;
        }
        let p = e.path();
        let m = fs::symlink_metadata(&p)?;
        let reparse = m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
        let r = if m.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0 {
            if !reparse {
                continue;
            }
            fs::remove_dir(&p)
        } else {
            fs::remove_file(&p)
        };
        match r {
            Ok(()) => apagados.push(p),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(apagados)
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;
    use windows_sys::Win32::System::IO::DeviceIoControl;

    const V: Versao = Versao::nova(1, 2, 3);

    struct Temp(PathBuf);

    impl Temp {
        fn novo(nome: &str) -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let p = std::env::temp_dir().join(format!(
                "nvt-arq-{nome}-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Self(canonico(&p).unwrap())
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Junção `link` → `alvo` por `FSCTL_SET_REPARSE_POINT` (sem processo externo).
    fn juncao(link: &Path, alvo: &Path) {
        fs::create_dir(link).unwrap();
        let d = OpenOptions::new()
            .access_mode(GENERIC_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(link)
            .unwrap();
        let alvo: Vec<u16> = format!(r"\??\{}", alvo.display()).encode_utf16().collect();
        let sub = (alvo.len() * 2) as u16;
        let mut caminho = alvo.clone();
        caminho.extend([0, 0]); // fim do nome substituto + nome de exibição vazio
        let dados = 8 + (caminho.len() * 2) as u16;
        let mut buf: Vec<u8> = Vec::new();
        buf.extend(0xA000_0003u32.to_le_bytes()); // IO_REPARSE_TAG_MOUNT_POINT
        buf.extend(dados.to_le_bytes());
        buf.extend(0u16.to_le_bytes());
        for x in [0u16, sub, sub + 2, 0] {
            buf.extend(x.to_le_bytes());
        }
        for c in caminho {
            buf.extend(c.to_le_bytes());
        }
        let mut n = 0u32;
        // SAFETY: buffer REPARSE_DATA_BUFFER montado acima.
        let ok = unsafe {
            DeviceIoControl(
                d.as_raw_handle() as HANDLE,
                0x0009_00A4, // FSCTL_SET_REPARSE_POINT
                buf.as_ptr().cast(),
                buf.len() as u32,
                null_mut(),
                0,
                &mut n,
                null_mut(),
            )
        };
        assert_ne!(ok, 0, "junção: {}", io::Error::last_os_error());
    }

    fn baixar(dir: &Path, dados: &[u8]) -> Baixado {
        let mut d = Download::criar(dir, V).unwrap();
        d.arquivo().write_all(dados).unwrap();
        d.concluir().unwrap()
    }

    #[test]
    fn programdata_vem_da_api() {
        let p = pasta_programdata().unwrap();
        assert!(p.is_absolute() && p.is_dir(), "{p:?}");
        assert!(pasta_padrao().unwrap().ends_with(r"nv-tuner\atualizacao"));
    }

    #[test]
    fn reparse_e_recusado() {
        let t = Temp::novo("reparse");
        let alvo = t.0.join("alvo");
        fs::create_dir(&alvo).unwrap();
        let link = t.0.join("link");
        juncao(&link, &alvo);
        assert!(componentes_sem_reparse(&link).is_err());
        assert!(componentes_sem_reparse(&link.join("sub")).is_err());
        assert!(Download::criar(&link, V).is_err());
        assert!(preparar_pasta(&link.join("atualizacao")).is_err());
        assert!(limpar(&link, None).is_err());
        assert_eq!(fs::read_dir(&alvo).unwrap().count(), 0, "seguiu a junção");
    }

    #[test]
    fn escrita_e_remocao_de_terceiro_negadas() {
        let t = Temp::novo("terceiro");
        let mut d = Download::criar(&t.0, V).unwrap();
        d.arquivo().write_all(b"abc").unwrap();
        let parcial = d.caminho().to_owned();
        assert!(OpenOptions::new().write(true).open(&parcial).is_err());
        assert!(fs::remove_file(&parcial).is_err());
        // Pré-existente: CREATE_NEW recusa.
        assert!(Download::criar(&t.0, V).is_err());
        let msi = d.concluir().unwrap().finalizar().unwrap();
        assert!(!parcial.exists());
        assert_eq!(msi.caminho(), t.0.join(nome_msi(V)));
        assert!(OpenOptions::new().write(true).open(msi.caminho()).is_err());
        assert!(fs::remove_file(msi.caminho()).is_err());
        // Leitura com negação de escrita (como o Windows Installer) é aceita.
        let mut s = String::new();
        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(msi.caminho())
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        assert_eq!(s, "abc");
        let caminho = msi.caminho().to_owned();
        drop(msi);
        fs::remove_file(&caminho).unwrap();
    }

    #[test]
    fn hash_pelo_mesmo_handle() {
        let t = Temp::novo("hash");
        let dados = vec![7u8; 100_000];
        let b = baixar(&t.0, &dados);
        assert_eq!(b.sha256(), sha256_hex(&dados));
        assert_eq!(b.tamanho(), dados.len() as u64);
        let msi = b.finalizar().unwrap();
        assert_eq!(msi.sha256(), sha256_hex(&dados));
        assert_eq!(msi.tamanho(), 100_000);
    }

    #[test]
    fn finalizar_nao_substitui_e_descartar_apaga() {
        let t = Temp::novo("subst");
        fs::write(t.0.join(nome_msi(V)), "antigo").unwrap();
        let b = baixar(&t.0, b"novo");
        assert!(b.finalizar().is_err());
        assert!(!t.0.join(format!("{}.parcial", nome_msi(V))).exists());
        fs::remove_file(t.0.join(nome_msi(V))).unwrap();
        baixar(&t.0, b"x").descartar();
        let d = Download::criar(&t.0, V).unwrap();
        drop(d);
        assert_eq!(fs::read_dir(&t.0).unwrap().count(), 0);
    }

    #[test]
    fn limpeza() {
        let t = Temp::novo("limpar");
        let v0 = Versao::nova(1, 2, 2);
        for n in [
            nome_msi(v0),
            nome_log(v0),
            nome_msi(V),
            nome_log(V),
            format!("{}.parcial", nome_msi(V)),
            "estado.json".into(),
        ] {
            fs::write(t.0.join(n), "x").unwrap();
        }
        let alvo = t.0.join("alvo");
        fs::create_dir(&alvo).unwrap();
        fs::write(alvo.join("dentro.txt"), "x").unwrap();
        juncao(&t.0.join("velho.msi.parcial"), &alvo);
        let apagados = limpar(&t.0, Some(V)).unwrap();
        assert_eq!(apagados.len(), 4, "{apagados:?}");
        let mut restam: Vec<String> = fs::read_dir(&t.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        restam.sort();
        assert_eq!(
            restam,
            ["alvo", "estado.json", &nome_log(V), &nome_msi(V)].map(String::from)
        );
        assert!(alvo.join("dentro.txt").exists(), "seguiu a junção");
        limpar(&t.0, None).unwrap();
        assert!(!t.0.join(nome_msi(V)).exists());
    }

    #[test]
    fn caminho_invalido_recusado() {
        assert_eq!(
            caminho_para_comando(Path::new(r"\\?\C:\Program Data\nv-tuner\a.msi")).unwrap(),
            r"C:\Program Data\nv-tuner\a.msi"
        );
        for p in [
            r#"C:\a"b\x.msi"#,
            r"C:\%TEMP%\x.msi",
            "C:\\a\nb\\x.msi",
            "C:\\a\u{7}\\x.msi",
            r"\\servidor\c\x.msi",
            r"\\?\UNC\servidor\c\x.msi",
            r"x.msi",
            r"C:x.msi",
            r"C:\a\b:c.msi",
        ] {
            assert!(caminho_para_comando(Path::new(p)).is_err(), "{p:?}");
        }
        let log = caminho_log(Path::new(r"C:\ProgramData\nv-tuner\atualizacao"), V).unwrap();
        assert_eq!(
            log,
            r"C:\ProgramData\nv-tuner\atualizacao\instalacao-1.2.3.log"
        );
    }

    #[test]
    fn pasta_preparada_tem_dacl_privada() {
        let t = Temp::novo("pasta");
        let dir = t.0.join("atualizacao");
        match preparar_pasta(&dir) {
            Ok(c) => {
                assert_eq!(c, dir);
                // Segunda vez: já existe e continua aceita.
                assert_eq!(preparar_pasta(&dir).unwrap(), dir);
            }
            Err(e) => {
                // Sem privilégio, só o dono (o próprio usuário) é recusado.
                assert!(!crate::diretorio::processo_privilegiado(), "{e}");
                assert!(e.to_string().contains("pertence a"), "{e}");
            }
        }
        // Pasta comum (herda permissões do temp) é recusada.
        let comum = t.0.join("comum");
        fs::create_dir(&comum).unwrap();
        assert!(preparar_pasta(&comum).is_err());
    }
}
