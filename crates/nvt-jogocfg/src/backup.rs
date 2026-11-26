//! Backup imutável do arquivo original (F3-04).
//!
//! `<dir_backups>\<jogo>\<nome>.orig` + `<nome>.orig.sha256` (hex). Criado uma única
//! vez: se já existe com o mesmo conteúdo, a chamada é idempotente; com conteúdo
//! diferente, [`Erro::BackupDivergente`] e nada é sobrescrito. Nenhum componente sob
//! `dir_backups` pode ser reparse point.

use crate::arquivo::verificar_componentes;
use crate::{Erro, Hash, Resultado, sha256};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Backup existente ou recém-criado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backup {
    pub caminho: PathBuf,
    pub hash: Hash,
    /// `true` se esta chamada criou o backup.
    pub criado_agora: bool,
}

fn nome_valido(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 120
        && !n.starts_with('.')
        && n.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn caminhos(dir: &Path, jogo: &str, nome: &str) -> Resultado<(PathBuf, PathBuf)> {
    if !nome_valido(jogo) || !nome_valido(nome) {
        return Err(Erro::formato("nome de backup inválido"));
    }
    let pasta = dir.join(jogo);
    Ok((
        pasta.join(format!("{nome}.orig")),
        pasta.join(format!("{nome}.orig.sha256")),
    ))
}

/// Cria (uma vez) o backup de `bytes`.
pub fn criar(dir_backups: &Path, jogo: &str, nome: &str, bytes: &[u8]) -> Resultado<Backup> {
    let (orig, arq_hash) = caminhos(dir_backups, jogo, nome)?;
    let pasta = orig.parent().expect("caminho com pasta");
    fs::create_dir_all(pasta)?;
    verificar_componentes(dir_backups, pasta)?;
    let hash = sha256(bytes);
    let mut criado_agora = false;
    match escrever_novo(&orig, bytes) {
        Ok(()) => criado_agora = true,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            let existente = crate::arquivo::ler(dir_backups, &orig)?;
            if existente.hash != hash {
                return Err(Erro::BackupDivergente(orig));
            }
        }
        Err(e) => return Err(e.into()),
    }
    match escrever_novo(&arq_hash, hash.hex().as_bytes()) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            let gravado = crate::arquivo::ler(dir_backups, &arq_hash)?;
            let h = std::str::from_utf8(&gravado.bytes)
                .ok()
                .and_then(Hash::de_hex);
            if h != Some(hash) {
                return Err(Erro::BackupDivergente(arq_hash));
            }
        }
        Err(e) => return Err(e.into()),
    }
    Ok(Backup {
        caminho: orig,
        hash,
        criado_agora,
    })
}

/// Lê o backup e confere com o `.sha256`.
pub fn ler(dir_backups: &Path, jogo: &str, nome: &str) -> Resultado<Vec<u8>> {
    let (orig, arq_hash) = caminhos(dir_backups, jogo, nome)?;
    let dados = crate::arquivo::ler(dir_backups, &orig)?;
    let gravado = crate::arquivo::ler(dir_backups, &arq_hash)?;
    let h = std::str::from_utf8(&gravado.bytes)
        .ok()
        .and_then(Hash::de_hex);
    if h != Some(dados.hash) {
        return Err(Erro::BackupCorrompido(orig));
    }
    Ok(dados.bytes)
}

fn escrever_novo(p: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(p)?;
    let r = f.write_all(bytes).and_then(|()| f.sync_all());
    if r.is_err() {
        drop(f);
        let _ = fs::remove_file(p);
    }
    r
}
