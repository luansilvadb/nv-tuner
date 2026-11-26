//! Integração em pasta temporária real (R-L-03, R-L-05, R-L-06).
#![cfg(windows)]

use std::collections::BTreeMap;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use nvt_manutencao::windows::{ExcluidorReal, MutexMsi, SistemaArquivosWindows, raiz_do_alvo};
use nvt_manutencao::{
    AlvoLimpeza, Contexto, Instalacao, ModoLimpeza, MotivoInterrupcao, MotivoPulo, PoliticaDiaria,
    Portao, Raiz, Relogio, ResultadoLimpeza, limpar,
};

struct Futuro;
impl Relogio for Futuro {
    fn agora_ms(&self) -> u64 {
        let agora = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        (agora.as_secs() + 30 * 24 * 3600) * 1000
    }
}
struct Agora;
impl Relogio for Agora {
    fn agora_ms(&self) -> u64 {
        nvt_manutencao::RelogioSistema.agora_ms()
    }
}
struct Flag(bool);
impl Portao for Flag {
    fn jogo_aberto(&self) -> bool {
        self.0
    }
}
impl Instalacao for Flag {
    fn em_andamento(&self) -> bool {
        self.0
    }
}

/// Pasta de teste `<tmp>/nvt-manut-<nome>-<pid>/{Temp,fora}`.
struct Pasta {
    base: PathBuf,
}
impl Pasta {
    fn nova(nome: &str) -> Self {
        let base = std::env::temp_dir().join(format!("nvt-manut-{nome}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("Temp")).unwrap();
        fs::create_dir_all(base.join("fora")).unwrap();
        Self { base }
    }
    fn temp(&self) -> PathBuf {
        self.base.join("Temp")
    }
    fn raiz(&self) -> Raiz {
        Raiz::nova(AlvoLimpeza::TempUsuario, self.temp()).unwrap()
    }
}
impl Drop for Pasta {
    fn drop(&mut self) {
        for e in walk(&self.base) {
            let _ = fs::metadata(&e).map(|m| {
                let mut p = m.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                p.set_readonly(false);
                let _ = fs::set_permissions(&e, p);
            });
        }
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn walk(d: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(it) = fs::read_dir(d) {
        for e in it.flatten() {
            let p = e.path();
            let m = fs::symlink_metadata(&p).unwrap();
            v.push(p.clone());
            if m.is_dir() && m.file_attributes() & 0x400 == 0 {
                v.extend(walk(&p));
            }
        }
    }
    v
}

/// Árvore completa: caminho -> (atributos, escrita, conteúdo).
fn foto(d: &Path) -> BTreeMap<PathBuf, (u32, u64, Option<Vec<u8>>)> {
    walk(d)
        .into_iter()
        .map(|p| {
            let m = fs::symlink_metadata(&p).unwrap();
            let c = m.is_file().then(|| fs::read(&p).unwrap());
            (p, (m.file_attributes(), m.last_write_time(), c))
        })
        .collect()
}

fn rodar(
    modo: ModoLimpeza,
    raiz: &Raiz,
    relogio: &dyn Relogio,
    portao: &dyn Portao,
    inst: &dyn Instalacao,
) -> ResultadoLimpeza {
    let mut pol = PoliticaDiaria::default();
    let mut ctx = Contexto {
        relogio,
        portao,
        instalacao: inst,
        politica: &mut pol,
    };
    limpar(
        modo,
        raiz,
        &SistemaArquivosWindows,
        &mut ExcluidorReal,
        &mut ctx,
    )
    .unwrap()
}

fn ativa(raiz: &Raiz) -> ResultadoLimpeza {
    rodar(
        ModoLimpeza::Ativa,
        raiz,
        &Futuro,
        &Flag(false),
        &Flag(false),
    )
}

/// Cria uma junção `link` -> `alvo` com FSCTL_SET_REPARSE_POINT.
fn juncao(link: &Path, alvo: &Path) {
    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;

    fs::create_dir(link).unwrap();
    let sub: Vec<u16> = format!(r"\??\{}", alvo.display()).encode_utf16().collect();
    let imp: Vec<u16> = alvo.as_os_str().encode_wide().collect();
    let mut caminhos = Vec::new();
    caminhos.extend_from_slice(&sub);
    caminhos.push(0);
    caminhos.extend_from_slice(&imp);
    caminhos.push(0);
    let dados_len = 8 + caminhos.len() * 2;
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(&0xA000_0003u32.to_le_bytes());
    buf.extend_from_slice(&(dados_len as u16).to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&((sub.len() * 2) as u16).to_le_bytes());
    buf.extend_from_slice(&(((sub.len() + 1) * 2) as u16).to_le_bytes());
    buf.extend_from_slice(&((imp.len() * 2) as u16).to_le_bytes());
    for c in caminhos {
        buf.extend_from_slice(&c.to_le_bytes());
    }
    let nome: Vec<u16> = link.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let h = CreateFileW(
            nome.as_ptr(),
            GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        );
        assert_ne!(h, INVALID_HANDLE_VALUE);
        let mut n = 0u32;
        let ok = DeviceIoControl(
            h,
            FSCTL_SET_REPARSE_POINT,
            buf.as_ptr().cast(),
            buf.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut n,
            std::ptr::null_mut(),
        );
        CloseHandle(h);
        assert_ne!(ok, 0, "{}", std::io::Error::last_os_error());
    }
}

#[test]
fn apaga_velhos_e_diretorios_vazios() {
    let p = Pasta::nova("basico");
    fs::write(p.temp().join("a.tmp"), b"12345").unwrap();
    fs::create_dir(p.temp().join("d")).unwrap();
    fs::write(p.temp().join(r"d\b.tmp"), b"123").unwrap();
    fs::write(p.temp().join("x.log"), b"log").unwrap();
    let r = ativa(&p.raiz());
    assert_eq!((r.arquivos, r.bytes), (2, 8), "{r:?}");
    assert!(!p.temp().join("a.tmp").exists());
    assert!(!p.temp().join("d").exists());
    assert!(p.temp().join("x.log").exists());
    assert!(p.temp().exists());
}

#[test]
fn recentes_ficam() {
    let p = Pasta::nova("idade");
    fs::write(p.temp().join("a.tmp"), b"1").unwrap();
    let r = rodar(
        ModoLimpeza::Ativa,
        &p.raiz(),
        &Agora,
        &Flag(false),
        &Flag(false),
    );
    assert_eq!(r.arquivos, 0);
    assert_eq!(r.pulados[&MotivoPulo::Recente], 1);
    assert!(p.temp().join("a.tmp").exists());
}

#[test]
fn juncao_recusada_nada_fora_apagado() {
    let p = Pasta::nova("juncao");
    let fora = p.base.join("fora");
    fs::write(fora.join("vitima.txt"), b"nao apague").unwrap();
    fs::create_dir(fora.join("sub")).unwrap();
    fs::write(fora.join(r"sub\v2.txt"), b"x").unwrap();
    let antes = foto(&fora);
    juncao(&p.temp().join("j"), &fora);
    let r = ativa(&p.raiz());
    assert_eq!(r.pulados[&MotivoPulo::ReparsePoint], 1, "{r:?}");
    assert_eq!(r.arquivos, 0);
    assert_eq!(foto(&fora), antes);
    assert!(p.temp().join("j").exists());
    // A junção em si é removida por remove_dir sem tocar o alvo.
    fs::remove_dir(p.temp().join("j")).unwrap();
}

#[test]
fn arquivo_em_uso_pulado() {
    let p = Pasta::nova("emuso");
    let c = p.temp().join("aberto.tmp");
    fs::write(&c, b"em uso").unwrap();
    let f = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&c)
        .unwrap();
    let r = ativa(&p.raiz());
    assert_eq!(r.pulados[&MotivoPulo::EmUso], 1, "{r:?}");
    assert_eq!(r.arquivos, 0);
    drop(f);
    assert_eq!(fs::read(&c).unwrap(), b"em uso");
}

#[test]
fn somente_leitura_pulado() {
    let p = Pasta::nova("ro");
    let c = p.temp().join("ro.tmp");
    fs::write(&c, b"ro").unwrap();
    let mut perm = fs::metadata(&c).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(&c, perm).unwrap();
    let r = ativa(&p.raiz());
    assert_eq!(r.pulados[&MotivoPulo::SomenteLeitura], 1);
    assert!(c.exists());
}

#[test]
fn mutex_msi_interrompe() {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let nome = format!(r"Local\nvt-manut-teste-msi-{}", std::process::id());
    let msi = MutexMsi::com_nome(&nome);
    assert!(!msi.em_andamento());
    let largo: Vec<u16> = nome.encode_utf16().chain(Some(0)).collect();
    let h = unsafe { CreateMutexW(std::ptr::null(), 0, largo.as_ptr()) };
    assert!(!h.is_null());
    assert!(msi.em_andamento());

    let p = Pasta::nova("msi");
    fs::write(p.temp().join("a.tmp"), b"1").unwrap();
    let r = rodar(ModoLimpeza::Ativa, &p.raiz(), &Futuro, &Flag(false), &msi);
    assert_eq!(r.interrompida, Some(MotivoInterrupcao::Instalacao));
    assert!(p.temp().join("a.tmp").exists());
    unsafe { CloseHandle(h) };
    assert!(!msi.em_andamento());
    // O nome real só é consultado (nunca criado).
    let _ = MutexMsi::padrao().em_andamento();
}

#[test]
fn portao_interrompe_durante_a_passada() {
    struct Fecha(std::cell::Cell<u32>);
    impl Portao for Fecha {
        fn jogo_aberto(&self) -> bool {
            self.0.set(self.0.get() + 1);
            self.0.get() > 1
        }
    }
    let p = Pasta::nova("portao");
    for i in 0..450 {
        fs::write(p.temp().join(format!("f{i}.tmp")), b"1").unwrap();
    }
    let r = rodar(
        ModoLimpeza::Ativa,
        &p.raiz(),
        &Futuro,
        &Fecha(std::cell::Cell::new(0)),
        &Flag(false),
    );
    assert_eq!(r.interrompida, Some(MotivoInterrupcao::JogoAberto));
    assert_eq!(r.arquivos, nvt_manutencao::PERIODO_PORTAO - 1);
    assert_eq!(fs::read_dir(p.temp()).unwrap().count(), 450 - 199);
}

#[test]
fn simulacao_nao_altera_um_byte() {
    let p = Pasta::nova("simulacao");
    fs::write(p.temp().join("a.tmp"), b"conteudo a").unwrap();
    fs::create_dir_all(p.temp().join(r"d\e")).unwrap();
    fs::write(p.temp().join(r"d\e\b.tmp"), vec![7u8; 4096]).unwrap();
    fs::write(p.temp().join("x.log"), b"log").unwrap();
    let c = p.temp().join("aberto.tmp");
    fs::write(&c, b"z").unwrap();
    fs::write(p.base.join(r"fora\v.txt"), b"fora").unwrap();
    juncao(&p.temp().join("j"), &p.base.join("fora"));
    let antes = foto(&p.base);
    let f = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&c)
        .unwrap();
    let r = rodar(
        ModoLimpeza::Simulacao,
        &p.raiz(),
        &Futuro,
        &Flag(false),
        &Flag(false),
    );
    drop(f);
    assert_eq!(foto(&p.base), antes);
    // Em simulação o arquivo em uso conta como apagável: não há abertura.
    assert_eq!((r.arquivos, r.bytes), (3, 10 + 4096 + 1), "{r:?}");
    assert_eq!(r.pulados[&MotivoPulo::ReparsePoint], 1);
    assert_eq!(r.modo, ModoLimpeza::Simulacao);
    fs::remove_dir(p.temp().join("j")).unwrap();
}

#[test]
fn raizes_dos_alvos() {
    let w = raiz_do_alvo(AlvoLimpeza::TempWindows).unwrap();
    assert!(w.caminho().ends_with("Temp"));
    assert_eq!(w.alvo(), AlvoLimpeza::TempWindows);
    if let Ok(u) = raiz_do_alvo(AlvoLimpeza::TempUsuario) {
        assert_eq!(u.alvo(), AlvoLimpeza::TempUsuario);
    }
}
