//! Conta Ubisoft do Rainbow Six Siege (F3-12).

use crate::{Erro, Resultado, arquivo};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Pasta do jogo relativa a Documentos.
pub const R6_RELATIVO: &str = r"My Games\Rainbow Six - Siege";
/// Arquivo por conta.
pub const ARQUIVO: &str = "GameSettings.ini";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContaUbisoft {
    /// Nome da pasta da conta (id da conta Ubisoft Connect).
    pub id: String,
    pub arquivo: PathBuf,
}

/// Escolhe entre candidatos `(id, mtime)`: um → ele; vários → o mais recente; empate no
/// mais recente → [`Erro::ContaAmbigua`]; nenhum → `None`.
pub fn escolher(candidatos: &[(String, SystemTime)]) -> Resultado<Option<String>> {
    let Some(max) = candidatos.iter().map(|c| c.1).max() else {
        return Ok(None);
    };
    let mut topo = candidatos.iter().filter(|c| c.1 == max);
    let primeiro = topo.next().map(|c| c.0.clone());
    if topo.next().is_some() {
        return Err(Erro::ContaAmbigua);
    }
    Ok(primeiro)
}

/// Varre `pasta_jogo` (ex.: `Documentos\My Games\Rainbow Six - Siege`, que deve ficar
/// sob `raiz`) atrás de subpastas de conta com `GameSettings.ini`. Subpastas ou arquivos
/// que sejam reparse points são ignorados.
pub fn escolher_conta(raiz: &Path, pasta_jogo: &Path) -> Resultado<ContaUbisoft> {
    arquivo::verificar_componentes(raiz, pasta_jogo)?;
    let mut candidatos = Vec::new();
    for e in fs::read_dir(pasta_jogo)? {
        let e = e?;
        let p = e.path();
        if arquivo::verificar_componentes(pasta_jogo, &p).is_err() || !e.file_type()?.is_dir() {
            continue;
        }
        let ini = p.join(ARQUIVO);
        if !matches!(arquivo::existe(pasta_jogo, &ini), Ok(true)) {
            continue;
        }
        let Some(id) = e.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        candidatos.push((id, fs::symlink_metadata(&ini)?.modified()?));
    }
    match escolher(&candidatos)? {
        Some(id) => Ok(ContaUbisoft {
            arquivo: pasta_jogo.join(&id).join(ARQUIVO),
            id,
        }),
        None => Err(Erro::ArquivoAusente(
            pasta_jogo.join("<conta>").join(ARQUIVO),
        )),
    }
}
