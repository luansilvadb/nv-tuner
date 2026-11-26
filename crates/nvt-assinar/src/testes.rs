use std::sync::atomic::{AtomicU32, Ordering};

use nvt_atualizacao::{Chave, verificar_manifesto};

use super::*;

/// Pasta temporária exclusiva por teste (removida ao sair).
struct Tmp(PathBuf);

impl Tmp {
    fn nova() -> Tmp {
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "nvt-assinar-teste-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Cenario {
    tmp: Tmp,
    pk: PublicKey,
    versao: Versao,
}

impl Cenario {
    fn msi(&self) -> PathBuf {
        self.tmp.0.join(nome_msi(self.versao))
    }

    fn verificar(&self) -> Result<String, String> {
        let chave = Chave::de_base64(&self.pk.to_base64()).unwrap();
        verificar_com(&self.tmp.0, &self.pk, |m, s| {
            verificar_manifesto(m, s, &chave)
        })
    }
}

/// Gera chave em memória, cria um MSI falso e assina com a linha base64 da secreta.
fn assinado() -> Cenario {
    let par = KeyPair::generate_encrypted_keypair(Some(String::new())).unwrap();
    let sec = par.sk.to_box(None).unwrap().into_string();
    let linha = sec.lines().nth(1).unwrap().to_string();
    let tmp = Tmp::nova();
    let versao: Versao = "1.2.3".parse().unwrap();
    let c = Cenario {
        tmp,
        pk: par.pk,
        versao,
    };
    fs::write(c.msi(), b"MSI falso para teste").unwrap();
    assinar(versao, &c.msi(), Some(&linha)).unwrap();
    c
}

#[test]
fn assina_e_verifica() {
    let c = assinado();
    let ok = c.verificar().unwrap();
    assert!(ok.contains("1.2.3"));
    let sig = fs::read_to_string(c.tmp.0.join(ASSINATURA)).unwrap();
    assert!(sig.contains("nv-tuner atualizacao formato=1 versao=1.2.3"));
    let sig_msi =
        fs::read_to_string(c.tmp.0.join(format!("{}.minisig", nome_msi(c.versao)))).unwrap();
    assert!(sig_msi.contains(&format!(
        "nv-tuner msi versao=1.2.3 sha256={}",
        sha256_hex(b"MSI falso para teste")
    )));
}

#[test]
fn aceita_key_inteiro_de_duas_linhas() {
    let par = KeyPair::generate_encrypted_keypair(Some(String::new())).unwrap();
    let sec = par.sk.to_box(None).unwrap().into_string();
    assert!(chave_secreta(Some(&sec)).is_ok());
}

#[test]
fn recusa_manifesto_adulterado() {
    let c = assinado();
    let p = c.tmp.0.join(MANIFESTO);
    let t = fs::read_to_string(&p)
        .unwrap()
        .replace("\"tamanho\": ", "\"tamanho\": 1");
    fs::write(&p, t).unwrap();
    assert!(c.verificar().unwrap_err().contains("manifesto recusado"));
}

#[test]
fn recusa_msi_adulterado() {
    let c = assinado();
    fs::write(c.msi(), b"MSI falso para teste!").unwrap();
    assert!(c.verificar().unwrap_err().contains("tamanho"));
    fs::write(c.msi(), b"MSI falso para tesTe").unwrap();
    assert!(c.verificar().unwrap_err().contains("SHA-256"));
}

#[test]
fn recusa_versao_adulterada() {
    let c = assinado();
    let p = c.tmp.0.join(MANIFESTO);
    let t = fs::read_to_string(&p).unwrap().replace("1.2.3", "1.2.4");
    fs::write(&p, t).unwrap();
    assert!(c.verificar().unwrap_err().contains("manifesto recusado"));
}

#[test]
fn recusa_outra_chave() {
    let mut c = assinado();
    c.pk = KeyPair::generate_encrypted_keypair(Some(String::new()))
        .unwrap()
        .pk;
    assert!(c.verificar().is_err());
}

#[test]
fn secret_ausente_falha() {
    let tmp = Tmp::nova();
    let v: Versao = "1.2.3".parse().unwrap();
    let msi = tmp.0.join(nome_msi(v));
    fs::write(&msi, b"x").unwrap();
    for c in [None, Some(""), Some("  \n")] {
        let e = assinar(v, &msi, c).unwrap_err();
        assert!(e.contains("NV_TUNER_MINISIGN_KEY ausente"), "{e}");
    }
    assert!(!tmp.0.join(MANIFESTO).exists());
}

#[test]
fn erro_de_chave_nao_ecoa_conteudo() {
    let e = chave_secreta(Some("SEGREDO-INVALIDO-123")).unwrap_err();
    assert!(!e.contains("SEGREDO"));
}

#[test]
fn gerar_chave_nao_sobrescreve() {
    let tmp = Tmp::nova();
    let (p, s) = (tmp.0.join("a.pub"), tmp.0.join("a.key"));
    let msg = gerar_chave(&p, &s).unwrap();
    assert!(msg.contains("NV_TUNER_MINISIGN_KEY"));
    let antes = fs::read(&s).unwrap();
    assert!(gerar_chave(&p, &s).is_err());
    assert!(gerar_chave(&tmp.0.join("b.pub"), &s).is_err());
    assert_eq!(fs::read(&s).unwrap(), antes);
    assert!(!tmp.0.join("b.pub").exists());
    // A chave gerada assina e verifica.
    let linha = String::from_utf8(antes).unwrap();
    assert!(chave_secreta(Some(&linha)).is_ok());
}

#[test]
fn verificar_oficial_recusa_chave_provisoria() {
    if chave_oficial_provisoria() {
        let tmp = Tmp::nova();
        assert!(
            verificar_oficial(&tmp.0)
                .unwrap_err()
                .contains("provisória")
        );
    }
}
