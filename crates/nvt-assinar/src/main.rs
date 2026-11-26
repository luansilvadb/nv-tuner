//! `nvt-assinar`: ferramenta do mantenedor/CI para a atualização automática
//! (docs/spec/atualizacao.md §6.2, §8.7, §11.2, §11.3, Q11). Nunca é empacotada no MSI.
//!
//! - `gerar-chave --publica <arq.pub> --secreta <arq.key>`
//! - `assinar --versao X.Y.Z --msi <arq>` (chave secreta só de `NV_TUNER_MINISIGN_KEY`)
//! - `verificar --dir <pasta>` (só com a chave oficial embutida)

use std::fs::{self, OpenOptions};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use minisign::{KeyPair, PublicKey, SecretKey, SecretKeyBox, SignatureBox};
use nvt_atualizacao::{
    CHAVE_OFICIAL, Manifesto, Versao, chave_oficial_provisoria, comentario_manifesto,
    comentario_msi, gerar_manifesto_json, manifesto::ErroVerificacao, nome_msi, sha256_hex,
    verificar_manifesto_oficial,
};

/// Variável de ambiente com a chave secreta (Q11: de preferência só a linha base64).
const VAR_CHAVE: &str = "NV_TUNER_MINISIGN_KEY";
const MANIFESTO: &str = "atualizacao.json";
const ASSINATURA: &str = "atualizacao.json.minisig";

const USO: &str = "uso:
  nvt-assinar gerar-chave --publica <arq.pub> --secreta <arq.key>
  nvt-assinar assinar --versao X.Y.Z --msi <arq>   (chave em NV_TUNER_MINISIGN_KEY)
  nvt-assinar verificar --dir <pasta>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match executar(&args) {
        Ok(msg) => {
            println!("{msg}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("erro: {e}");
            ExitCode::FAILURE
        }
    }
}

fn executar(args: &[String]) -> Result<String, String> {
    let (cmd, resto) = args.split_first().ok_or_else(|| USO.to_string())?;
    match cmd.as_str() {
        "gerar-chave" => {
            let o = opcoes(resto, &["--publica", "--secreta"])?;
            gerar_chave(Path::new(&o[0]), Path::new(&o[1]))
        }
        "assinar" => {
            let o = opcoes(resto, &["--versao", "--msi"])?;
            let versao = versao(&o[0])?;
            let chave = std::env::var(VAR_CHAVE).ok();
            assinar(versao, Path::new(&o[1]), chave.as_deref())
        }
        "verificar" => {
            let o = opcoes(resto, &["--dir"])?;
            verificar_oficial(Path::new(&o[0]))
        }
        _ => Err(USO.to_string()),
    }
}

/// Lê exatamente as opções `nomes` (cada uma uma vez, com valor), na ordem dada.
fn opcoes(args: &[String], nomes: &[&str]) -> Result<Vec<String>, String> {
    let mut valores: Vec<Option<String>> = vec![None; nomes.len()];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let i = nomes
            .iter()
            .position(|n| n == a)
            .ok_or_else(|| format!("opção desconhecida: {a}\n{USO}"))?;
        let v = it.next().ok_or_else(|| format!("{a} exige um valor"))?;
        if valores[i].replace(v.clone()).is_some() {
            return Err(format!("{a} repetida"));
        }
    }
    valores
        .into_iter()
        .zip(nomes)
        .map(|(v, n)| v.ok_or_else(|| format!("falta {n}\n{USO}")))
        .collect()
}

fn versao(s: &str) -> Result<Versao, String> {
    s.parse()
        .map_err(|_| format!("versão inválida: {s} (esperado X.Y.Z)"))
}

fn gerar_chave(publica: &Path, secreta: &Path) -> Result<String, String> {
    for p in [publica, secreta] {
        if p.exists() {
            return Err(format!("{} já existe; não sobrescrevo", p.display()));
        }
    }
    // Sem senha; `generate_unencrypted_keypair` (0.7.9) não grava a soma de conferência.
    let par = KeyPair::generate_encrypted_keypair(Some(String::new()))
        .map_err(|e| format!("gerar chave: {e}"))?;
    let pub_txt = par
        .pk
        .to_box()
        .map_err(|e| format!("codificar pública: {e}"))?
        .into_string();
    let sec_txt = par
        .sk
        .to_box(None)
        .map_err(|_| "codificar secreta".to_string())?
        .into_string();
    gravar_novo(secreta, sec_txt.as_bytes())?;
    gravar_novo(publica, pub_txt.as_bytes())?;
    Ok(format!(
        "Chaves geradas (sem senha).
Pública: {pub}  (linha base64: {b64})
Secreta: {sec}

Próximos passos:
1. Grave no secret {VAR_CHAVE} SÓ a segunda linha (base64) de {sec} (Q11: o GitHub
   mascara por linha), ex.: (Get-Content {sec})[1] | gh secret set {VAR_CHAVE}
2. Copie {pub} para instalador/nv-tuner-minisign.pub e faça commit.
3. Faça backup offline de {sec} (dois meios, ex.: pendrive cifrado + cofre de senhas)
   e apague a cópia desta máquina.",
        pub = publica.display(),
        sec = secreta.display(),
        b64 = par.pk.to_base64(),
    ))
}

fn gravar_novo(p: &Path, dados: &[u8]) -> Result<(), String> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(p)
        .map_err(|e| format!("criar {}: {e}", p.display()))?;
    f.write_all(dados)
        .map_err(|e| format!("gravar {}: {e}", p.display()))
}

/// Interpreta a chave secreta: só a linha base64 (Q11) ou o `.key` inteiro.
/// Nenhuma mensagem de erro contém o conteúdo.
fn chave_secreta(conteudo: Option<&str>) -> Result<SecretKey, String> {
    let ausente = || {
        format!(
            "Secret {VAR_CHAVE} ausente: a release não é publicada sem assinatura (ver CONTRIBUTING.md)"
        )
    };
    let c = conteudo
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .ok_or_else(ausente)?;
    let linhas: Vec<&str> = c.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let b64 = match linhas.as_slice() {
        [l] => *l,
        [_, l] => *l,
        _ => {
            return Err(format!(
                "{VAR_CHAVE} malformada (esperada a linha base64 do .key)"
            ));
        }
    };
    let caixa = SecretKeyBox::from_string(&format!("untrusted comment: nvt-assinar\n{b64}\n"))
        .map_err(|_| format!("{VAR_CHAVE} malformada"))?;
    caixa
        .into_secret_key(Some(String::new()))
        .map_err(|_| format!("{VAR_CHAVE} inválida (chave com senha ou corrompida)"))
}

fn assinar(versao: Versao, msi: &Path, chave: Option<&str>) -> Result<String, String> {
    let sk = chave_secreta(chave)?;
    let pk = PublicKey::from_secret_key(&sk).map_err(|_| "derivar chave pública".to_string())?;
    let esperado = nome_msi(versao);
    if msi.file_name().and_then(|n| n.to_str()) != Some(esperado.as_str()) {
        return Err(format!(
            "o MSI deve se chamar {esperado} (recebido {})",
            msi.display()
        ));
    }
    let bytes = fs::read(msi).map_err(|e| format!("ler {}: {e}", msi.display()))?;
    let sha = sha256_hex(&bytes);
    let dir = msi.parent().map(Path::to_path_buf).unwrap_or_default();
    let manifesto = gerar_manifesto_json(versao, bytes.len() as u64, &sha);

    let sig_man = assinar_bytes(
        &pk,
        &sk,
        manifesto.as_bytes(),
        &comentario_manifesto(versao),
    )?;
    let sig_msi = assinar_bytes(&pk, &sk, &bytes, &comentario_msi(versao, &sha))?;

    let p_sig_msi = PathBuf::from(format!("{}.minisig", msi.display()));
    for (p, dados) in [
        (dir.join(MANIFESTO), manifesto.as_bytes()),
        (dir.join(ASSINATURA), sig_man.as_bytes()),
        (p_sig_msi.clone(), sig_msi.as_bytes()),
    ] {
        fs::write(&p, dados).map_err(|e| format!("gravar {}: {e}", p.display()))?;
    }
    Ok(format!(
        "Assinado {esperado} ({} bytes, sha256 {sha}) com a chave {}:\n  {}\n  {}\n  {}",
        bytes.len(),
        pk.to_base64(),
        dir.join(MANIFESTO).display(),
        dir.join(ASSINATURA).display(),
        p_sig_msi.display()
    ))
}

/// Assinatura pré-hash `ED` com o comentário confiável dado.
fn assinar_bytes(
    pk: &PublicKey,
    sk: &SecretKey,
    dados: &[u8],
    comentario: &str,
) -> Result<String, String> {
    minisign::sign(Some(pk), sk, Cursor::new(dados), Some(comentario), None)
        .map(SignatureBox::into_string)
        .map_err(|e| format!("assinar: {e}"))
}

/// `verificar` da release: só a chave oficial embutida; provisória ⇒ falha.
fn verificar_oficial(dir: &Path) -> Result<String, String> {
    if chave_oficial_provisoria() {
        return Err(
            "a chave oficial embutida é a provisória: nenhuma release pode ser verificada"
                .to_string(),
        );
    }
    let pk = PublicKey::from_base64(CHAVE_OFICIAL)
        .map_err(|e| format!("chave oficial inválida: {e}"))?;
    verificar_com(dir, &pk, verificar_manifesto_oficial)
}

/// Verificação parametrizada pela chave (os testes injetam uma chave temporária).
fn verificar_com(
    dir: &Path,
    pk: &PublicKey,
    verificar_manifesto: impl Fn(&[u8], &[u8]) -> Result<Manifesto, ErroVerificacao>,
) -> Result<String, String> {
    let ler = |nome: &str| {
        let p = dir.join(nome);
        fs::read(&p).map_err(|e| format!("ler {}: {e}", p.display()))
    };
    let m = verificar_manifesto(&ler(MANIFESTO)?, &ler(ASSINATURA)?)
        .map_err(|e| format!("manifesto recusado: {e:?}"))?;
    let msi = ler(&m.msi)?;
    if msi.len() as u64 != m.tamanho {
        return Err(format!(
            "tamanho do MSI {} ≠ manifesto {}",
            msi.len(),
            m.tamanho
        ));
    }
    let sha = sha256_hex(&msi);
    if sha != m.sha256 {
        return Err(format!("SHA-256 do MSI {sha} ≠ manifesto {}", m.sha256));
    }
    let sig_txt = String::from_utf8(ler(&format!("{}.minisig", m.msi))?)
        .map_err(|_| "assinatura do MSI malformada".to_string())?;
    let sig = SignatureBox::from_string(&sig_txt)
        .map_err(|e| format!("assinatura do MSI malformada: {e}"))?;
    if !sig.is_prehashed() {
        return Err("assinatura do MSI legada (sem pré-hash)".to_string());
    }
    let comentario = sig
        .trusted_comment()
        .map_err(|e| format!("comentário do MSI: {e}"))?;
    let esperado = comentario_msi(m.versao, &m.sha256);
    if comentario != esperado {
        return Err(format!(
            "comentário confiável do MSI {comentario:?} ≠ {esperado:?}"
        ));
    }
    minisign::verify(pk, &sig, Cursor::new(&msi), true, false, false)
        .map_err(|e| format!("assinatura do MSI recusada: {e}"))?;
    Ok(format!(
        "OK: {} versão {} ({} bytes, sha256 {})",
        m.msi, m.versao, m.tamanho, m.sha256
    ))
}

#[cfg(test)]
mod testes;
