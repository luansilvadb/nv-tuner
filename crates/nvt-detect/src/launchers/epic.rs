//! Epic Games: `ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item` (JSON).

use std::path::{Path, PathBuf};

use crate::model::{DetectedGame, Launcher};

/// Converte um manifesto `.item` em jogo (ignora instalações incompletas e não-jogos).
pub fn jogo_de_item(src: &str) -> Result<Option<DetectedGame>, String> {
    let v: serde_json::Value = serde_json::from_str(src).map_err(|e| e.to_string())?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).filter(|x| !x.is_empty());
    if v.get("bIsIncompleteInstall").and_then(|x| x.as_bool()) == Some(true) {
        return Ok(None);
    }
    if let Some(cats) = v.get("AppCategories").and_then(|x| x.as_array()) {
        if !cats.iter().any(|c| c.as_str() == Some("games")) {
            return Ok(None);
        }
    }
    let Some(pasta) = s("InstallLocation") else {
        return Ok(None);
    };
    let pasta = PathBuf::from(pasta.replace('/', "\\"));
    let nome = s("DisplayName").or(s("AppName")).unwrap_or("?");
    let mut j = DetectedGame::new(Launcher::Epic, nome, &pasta);
    j.app_id = s("AppName").map(str::to_string);
    j.exe_indicado = s("LaunchExecutable").map(|e| pasta.join(e.replace('/', "\\")));
    Ok(Some(j))
}

pub fn varrer_dir(manifests: &Path) -> Result<Vec<DetectedGame>, String> {
    let Ok(rd) = std::fs::read_dir(manifests) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("item"))
        {
            if let Ok(src) = std::fs::read_to_string(&p) {
                if let Ok(Some(j)) = jogo_de_item(&src) {
                    out.push(j);
                }
            }
        }
    }
    Ok(out)
}

pub fn varrer() -> Result<Vec<DetectedGame>, String> {
    varrer_dir(
        &super::program_data()
            .join("Epic")
            .join("EpicGamesLauncher")
            .join("Data")
            .join("Manifests"),
    )
}
