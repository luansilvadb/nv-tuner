//! Xbox / Microsoft Store: `<unidade>:\XboxGames\<jogo>\Content\MicrosoftGame.config`.

use std::path::{Path, PathBuf};

use crate::model::{DetectedGame, Launcher};

/// Campos relevantes de `MicrosoftGame.config`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ConfigXbox {
    pub nome: Option<String>,
    pub exe: Option<String>,
    pub identidade: Option<String>,
}

/// Valor de um atributo dentro da primeira tag `<tag ...>` encontrada.
fn atributo(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let abre = format!("<{tag}");
    let mut resto = xml;
    while let Some(i) = resto.find(&abre) {
        let depois = &resto[i + abre.len()..];
        // Garante que é a tag exata (não <ExecutableList> para <Executable>).
        if depois.starts_with(|c: char| c.is_whitespace() || c == '/' || c == '>') {
            let fim = depois.find('>').unwrap_or(depois.len());
            let corpo = &depois[..fim];
            let chave = format!("{attr}=");
            let mut pos = 0;
            while let Some(j) = corpo[pos..].find(&chave) {
                let ini = pos + j;
                let antes_ok = ini == 0 || corpo[..ini].ends_with(|c: char| c.is_whitespace());
                let v = &corpo[ini + chave.len()..];
                if antes_ok {
                    let aspas = v.chars().next()?;
                    if aspas == '"' || aspas == '\'' {
                        let v = &v[1..];
                        return v.find(aspas).map(|f| desescapar(&v[..f]));
                    }
                }
                pos = ini + chave.len();
            }
        }
        resto = depois;
    }
    None
}

fn desescapar(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

pub fn config(xml: &str) -> ConfigXbox {
    ConfigXbox {
        nome: atributo(xml, "ShellVisuals", "DefaultDisplayName")
            .filter(|s| !s.starts_with("ms-resource:")),
        exe: atributo(xml, "Executable", "Name"),
        identidade: atributo(xml, "Identity", "Name"),
    }
}

/// Varre uma pasta `XboxGames`.
pub fn varrer_dir(xbox_games: &Path) -> Vec<DetectedGame> {
    let Ok(rd) = std::fs::read_dir(xbox_games) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let base = e.path();
        if !base.is_dir() {
            continue;
        }
        let content = base.join("Content");
        let (pasta, cfg) = [content.clone(), base.clone()]
            .into_iter()
            .find_map(|p| {
                std::fs::read_to_string(p.join("MicrosoftGame.config"))
                    .ok()
                    .map(|s| (p, config(&s)))
            })
            .unwrap_or_else(|| (base.clone(), ConfigXbox::default()));
        let nome = cfg
            .nome
            .clone()
            .unwrap_or_else(|| e.file_name().to_string_lossy().to_string());
        let mut j = DetectedGame::new(Launcher::Xbox, nome, &pasta);
        j.app_id = cfg.identidade;
        j.exe_indicado = cfg.exe.map(|x| pasta.join(x.replace('/', "\\")));
        out.push(j);
    }
    out
}

/// Unidades com `XboxGames` na raiz.
pub fn pastas_xbox() -> Vec<PathBuf> {
    (b'A'..=b'Z')
        .map(|l| PathBuf::from(format!("{}:\\XboxGames", l as char)))
        .filter(|p| p.is_dir())
        .collect()
}

pub fn varrer() -> Result<Vec<DetectedGame>, String> {
    Ok(pastas_xbox().iter().flat_map(|p| varrer_dir(p)).collect())
}
