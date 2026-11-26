//! F3-05 e F3-06: reparse points, somente leitura e gravação atômica.

mod comum;

use comum::*;
use nvt_jogocfg::arquivo::{existe, gravar, ler};
use nvt_jogocfg::{Erro, sha256};
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::Path;

fn largo(p: &Path) -> Vec<u16> {
    p.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[test]
fn le_e_grava_atomicamente() {
    let t = Temp::novo("gravar");
    let p = t.0.join("GameSettings.ini");
    fs::write(&p, amostra("r6_GameSettings.ini")).unwrap();
    let lido = ler(&t.0, &p).unwrap();
    assert!(!lido.somente_leitura);
    assert_eq!(lido.hash, sha256(&lido.bytes));
    let novo = b"[DISPLAY]\r\nFPSLimit=60\r\n";
    let h = gravar(&t.0, &p, novo, &lido.hash).unwrap();
    assert_eq!(h, sha256(novo));
    assert_eq!(fs::read(&p).unwrap(), novo);
    // Nenhum temporário sobra na pasta.
    assert_eq!(fs::read_dir(&t.0).unwrap().count(), 1);
}

#[test]
fn hash_divergente_nao_grava() {
    let t = Temp::novo("hash");
    let p = t.0.join("a.ini");
    fs::write(&p, b"[s]\nA=1\n").unwrap();
    let e = gravar(&t.0, &p, b"x", &sha256(b"outro")).unwrap_err();
    assert!(matches!(e, Erro::HashDivergente));
    assert_eq!(fs::read(&p).unwrap(), b"[s]\nA=1\n");
}

#[test]
fn somente_leitura_e_recusado_sem_alterar_atributos() {
    let t = Temp::novo("ro");
    let p = t.0.join("GameUserSettings.ini");
    fs::write(&p, b"[s]\nA=1\n").unwrap();
    let mut perm = fs::metadata(&p).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(&p, perm).unwrap();
    let antes = fs::metadata(&p).unwrap().file_attributes();
    let lido = ler(&t.0, &p).unwrap();
    assert!(lido.somente_leitura);
    let e = gravar(&t.0, &p, b"[s]\nA=2\n", &lido.hash).unwrap_err();
    assert!(matches!(e, Erro::ArquivoSomenteLeitura(_)));
    assert_eq!(e.motivo(), nvt_jogocfg::Motivo::ArquivoSomenteLeitura);
    assert_eq!(fs::metadata(&p).unwrap().file_attributes(), antes);
    assert_eq!(fs::read(&p).unwrap(), b"[s]\nA=1\n");
    assert_eq!(fs::read_dir(&t.0).unwrap().count(), 1);
}

#[test]
fn juncao_em_componente_e_recusada() {
    let t = Temp::novo("juncao");
    let real = t.0.join("real");
    fs::create_dir(&real).unwrap();
    fs::write(real.join("a.ini"), b"[s]\nA=1\n").unwrap();
    let link = t.0.join("link");
    juncao(&link, &real);
    let p = link.join("a.ini");
    assert!(matches!(ler(&t.0, &p), Err(Erro::ReparseRecusado(_))));
    let h = sha256(b"[s]\nA=1\n");
    assert!(matches!(
        gravar(&t.0, &p, b"x", &h),
        Err(Erro::ReparseRecusado(_))
    ));
    assert!(matches!(existe(&t.0, &p), Err(Erro::ReparseRecusado(_))));
    // A própria raiz como junção também é recusada.
    assert!(matches!(ler(&link, &p), Err(Erro::ReparseRecusado(_))));
    assert_eq!(fs::read(real.join("a.ini")).unwrap(), b"[s]\nA=1\n");
}

#[test]
fn caminho_final_como_juncao_ou_link_e_recusado() {
    let t = Temp::novo("final");
    let real = t.0.join("real");
    fs::create_dir(&real).unwrap();
    let p = t.0.join("a.ini");
    juncao(&p, &real);
    assert!(matches!(ler(&t.0, &p), Err(Erro::ReparseRecusado(_))));
    // Link simbólico de arquivo (exige modo desenvolvedor ou privilégio; se não der para
    // criar, a junção acima já cobre o caso).
    let alvo = t.0.join("alvo.ini");
    fs::write(&alvo, b"[s]\nA=1\n").unwrap();
    let s = t.0.join("s.ini");
    if std::os::windows::fs::symlink_file(&alvo, &s).is_ok() {
        assert!(matches!(ler(&t.0, &s), Err(Erro::ReparseRecusado(_))));
        let h = sha256(b"[s]\nA=1\n");
        assert!(gravar(&t.0, &s, b"x", &h).is_err());
        assert_eq!(fs::read(&alvo).unwrap(), b"[s]\nA=1\n");
    }
}

#[test]
fn fora_da_raiz_e_ausente() {
    let t = Temp::novo("fora");
    let outra = Temp::novo("fora-outra");
    let p = outra.0.join("a.ini");
    fs::write(&p, b"x").unwrap();
    assert!(matches!(ler(&t.0, &p), Err(Erro::FormatoInesperado(_))));
    assert!(matches!(
        ler(&t.0, &t.0.join("x").join("..").join("a.ini")),
        Err(Erro::ArquivoAusente(_) | Erro::FormatoInesperado(_))
    ));
    assert!(matches!(
        ler(&t.0, &t.0.join("nao.ini")),
        Err(Erro::ArquivoAusente(_))
    ));
    assert!(!existe(&t.0, &t.0.join("nao.ini")).unwrap());
}

#[test]
fn replacefilew_preserva_atributos_acl_e_criacao() {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW,
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GetFileSecurityW, PSECURITY_DESCRIPTOR, SetFileSecurityW,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_HIDDEN, SetFileAttributesW,
    };

    let t = Temp::novo("acl");
    let p = t.0.join("TASystemSettings.ini");
    fs::write(&p, b"[s]\nA=1\n").unwrap();
    let w = largo(&p);
    // DACL protegida e explícita: dono com controle total, Todos com leitura.
    let sddl: Vec<u16> = "D:P(A;;FA;;;OW)(A;;FR;;;WD)\0".encode_utf16().collect();
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: SDDL terminada em NUL; `sd` é liberado com LocalFree.
    unsafe {
        assert_ne!(
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut()
            ),
            0
        );
        assert_ne!(
            SetFileSecurityW(w.as_ptr(), DACL_SECURITY_INFORMATION, sd),
            0
        );
        LocalFree(sd);
        assert_ne!(
            SetFileAttributesW(w.as_ptr(), FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_ARCHIVE),
            0
        );
    }
    let dacl = |p: &[u16]| -> String {
        let mut n = 0u32;
        // SAFETY: primeira chamada só mede; buffer do tamanho pedido na segunda.
        unsafe {
            GetFileSecurityW(
                p.as_ptr(),
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                0,
                &mut n,
            );
            let mut buf = vec![0u8; n as usize];
            assert_ne!(
                GetFileSecurityW(
                    p.as_ptr(),
                    DACL_SECURITY_INFORMATION,
                    buf.as_mut_ptr().cast(),
                    n,
                    &mut n
                ),
                0
            );
            let mut s: *mut u16 = std::ptr::null_mut();
            assert_ne!(
                ConvertSecurityDescriptorToStringSecurityDescriptorW(
                    buf.as_mut_ptr().cast(),
                    SDDL_REVISION_1,
                    DACL_SECURITY_INFORMATION,
                    &mut s,
                    std::ptr::null_mut()
                ),
                0
            );
            let mut k = 0;
            while *s.add(k) != 0 {
                k += 1;
            }
            let r = String::from_utf16_lossy(std::slice::from_raw_parts(s, k));
            LocalFree(s.cast());
            r
        }
    };
    let acl_antes = dacl(&w);
    let m_antes = fs::metadata(&p).unwrap();
    let lido = ler(&t.0, &p).unwrap();
    gravar(&t.0, &p, b"[s]\nA=2\n", &lido.hash).unwrap();
    let m = fs::metadata(&p).unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"[s]\nA=2\n");
    assert_ne!(m.file_attributes() & FILE_ATTRIBUTE_HIDDEN, 0);
    assert_eq!(m.creation_time(), m_antes.creation_time());
    // As ACEs explícitas e a proteção (P) se mantêm; o ReplaceFileW pode acrescentar a
    // marca AI (auto-herdada), que não muda o acesso.
    let aces = |s: &str| s[s.find('(').unwrap()..].to_owned();
    let depois = dacl(&w);
    assert_eq!(aces(&depois), aces(&acl_antes));
    assert!(depois.starts_with("D:P"), "{depois}");
    assert!(acl_antes.starts_with("D:P"), "{acl_antes}");
}

#[test]
fn escritor_externo_aberto_impede_a_gravacao() {
    use std::os::windows::fs::OpenOptionsExt;
    let t = Temp::novo("trava");
    let p = t.0.join("a.ini");
    fs::write(&p, b"[s]\nA=1\n").unwrap();
    let lido = ler(&t.0, &p).unwrap();
    // Escritor externo com compartilhamento total: a trava sem FILE_SHARE_WRITE não abre.
    let w = fs::OpenOptions::new()
        .write(true)
        .share_mode(7)
        .open(&p)
        .unwrap();
    assert!(gravar(&t.0, &p, b"x", &lido.hash).is_err());
    drop(w);
    assert_eq!(fs::read(&p).unwrap(), b"[s]\nA=1\n");
    // Sem o escritor, a gravação passa (ReplaceFileW convive com a trava).
    gravar(&t.0, &p, b"x", &lido.hash).unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"x");
    assert_eq!(fs::read_dir(&t.0).unwrap().count(), 1);
}
