//! Steam: registro → `libraryfolders.vdf` → `appmanifest_*.acf`.

use std::path::{Path, PathBuf};

use crate::model::{DetectedGame, Launcher};
use crate::registro::{self, Hive};
use crate::vdf;

/// AppIDs que não são jogos (redistribuíveis, ferramentas).
const NAO_JOGOS: &[&str] = &["228980", "1070560", "1391110", "1493710", "1628350"];

/// Pasta raiz da Steam pelo registro (barras e caixa normalizadas pelo chamador).
pub fn raiz() -> Option<PathBuf> {
    registro::texto(Hive::Hkcu, r"Software\Valve\Steam", "SteamPath")
        .or_else(|| {
            registro::texto(
                Hive::Hklm,
                r"SOFTWARE\WOW6432Node\Valve\Steam",
                "InstallPath",
            )
        })
        .map(|s| PathBuf::from(s.replace('/', "\\")))
}

/// Pastas de biblioteca listadas em `libraryfolders.vdf` (formato novo e antigo).
pub fn bibliotecas(src: &str) -> Result<Vec<PathBuf>, String> {
    let doc = vdf::parse(src)?;
    let raiz = doc
        .get("libraryfolders")
        .ok_or("chave 'libraryfolders' ausente")?;
    let mut out = Vec::new();
    for (k, v) in raiz.itens() {
        if !k.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let caminho = match v {
            vdf::Vdf::Texto(p) => Some(p.as_str()),
            vdf::Vdf::Objeto(_) => v.get_texto("path"),
        };
        if let Some(p) = caminho {
            out.push(PathBuf::from(p.replace('/', "\\")));
        }
    }
    Ok(out)
}

/// Converte um `appmanifest_<id>.acf` em jogo, se estiver instalado e for jogo.
pub fn jogo_de_manifesto(src: &str, biblioteca: &Path) -> Result<Option<DetectedGame>, String> {
    let doc = vdf::parse(src)?;
    let st = doc.get("AppState").ok_or("chave 'AppState' ausente")?;
    let appid = st.get_texto("appid").ok_or("appid ausente")?;
    let nome = st.get_texto("name").unwrap_or(appid);
    let installdir = st.get_texto("installdir").ok_or("installdir ausente")?;
    let flags: u32 = st
        .get_texto("StateFlags")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    // Bit 4 = StateFullyInstalled.
    if flags & 4 == 0 {
        return Ok(None);
    }
    let nome_lc = nome.to_lowercase();
    if NAO_JOGOS.contains(&appid)
        || nome_lc.contains("redistributable")
        || nome_lc.contains("steamvr")
        || nome_lc.starts_with("proton")
    {
        return Ok(None);
    }
    let pasta = biblioteca.join("steamapps").join("common").join(installdir);
    Ok(Some(
        DetectedGame::new(Launcher::Steam, nome, pasta).with_app_id(appid),
    ))
}

/// Varre todas as bibliotecas a partir da raiz da Steam.
pub fn varrer_raiz(raiz: &Path) -> Result<Vec<DetectedGame>, String> {
    let lf = raiz.join("steamapps").join("libraryfolders.vdf");
    let mut libs = match std::fs::read_to_string(&lf) {
        Ok(s) => bibliotecas(&s)?,
        Err(e) => return Err(format!("{}: {e}", lf.display())),
    };
    if !libs
        .iter()
        .any(|l| crate::model::pasta_canonica(l) == crate::model::pasta_canonica(raiz))
    {
        libs.insert(0, raiz.to_path_buf());
    }
    let mut out = Vec::new();
    for lib in libs {
        let Ok(rd) = std::fs::read_dir(lib.join("steamapps")) else {
            continue;
        };
        for e in rd.flatten() {
            let nome = e.file_name().to_string_lossy().to_lowercase();
            if !(nome.starts_with("appmanifest_") && nome.ends_with(".acf")) {
                continue;
            }
            // Manifesto corrompido não derruba a biblioteca.
            if let Ok(src) = std::fs::read_to_string(e.path()) {
                if let Ok(Some(j)) = jogo_de_manifesto(&src, &lib) {
                    out.push(j);
                }
            }
        }
    }
    Ok(out)
}

pub fn varrer() -> Result<Vec<DetectedGame>, String> {
    match raiz() {
        Some(r) => varrer_raiz(&r),
        None => Ok(Vec::new()),
    }
}
