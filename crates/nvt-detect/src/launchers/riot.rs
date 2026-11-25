//! Riot: `RiotClientInstalls.json` + `Metadata\<produto>\<produto>.product_settings.yaml`.

use std::path::{Path, PathBuf};

use crate::model::{DetectedGame, Launcher, pasta_canonica};

/// Pastas de jogo em `associated_client` (chaves do objeto).
pub fn pastas_associadas(src: &str) -> Result<Vec<PathBuf>, String> {
    let v: serde_json::Value = serde_json::from_str(src).map_err(|e| e.to_string())?;
    Ok(v.get("associated_client")
        .and_then(|a| a.as_object())
        .map(|o| {
            o.keys()
                .map(|k| PathBuf::from(k.replace('/', "\\")))
                .collect()
        })
        .unwrap_or_default())
}

/// Campos relevantes do YAML de produto.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ProdutoRiot {
    pub pasta: Option<PathBuf>,
    pub vanguard: bool,
}

fn sem_aspas(s: &str) -> &str {
    let s = s.trim();
    s.strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .or_else(|| s.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')))
        .unwrap_or(s)
}

/// Lê à mão só as duas chaves necessárias, sem depender de um parser YAML.
pub fn produto(src: &str) -> ProdutoRiot {
    let mut p = ProdutoRiot::default();
    let mut em_dependencias = false;
    for linha in src.lines() {
        let indentada = linha.starts_with(' ') || linha.starts_with('\t');
        let t = linha.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if !indentada {
            em_dependencias = t == "dependencies:";
            if let Some(v) = t.strip_prefix("product_install_full_path:") {
                let v = sem_aspas(v);
                if !v.is_empty() {
                    p.pasta = Some(PathBuf::from(v.replace('/', "\\")));
                }
            }
        } else if em_dependencias {
            if let Some(v) = t.strip_prefix("vanguard:") {
                p.vanguard = sem_aspas(v).eq_ignore_ascii_case("true");
            }
        }
    }
    p
}

/// Nome legível de um produto Riot pelo ID.
pub fn nome_produto(id: &str) -> String {
    match id {
        "league_of_legends" => "League of Legends".into(),
        "valorant" => "VALORANT".into(),
        "bacon" => "Legends of Runeterra".into(),
        "teamfighttactics" => "Teamfight Tactics".into(),
        "lion" => "2XKO".into(),
        outro => outro.replace('_', " "),
    }
}

/// Varre a pasta `Riot Games` de `ProgramData`.
pub fn varrer_dir(riot: &Path) -> Result<Vec<DetectedGame>, String> {
    let mut out: Vec<DetectedGame> = Vec::new();
    let meta = riot.join("Metadata");
    if let Ok(rd) = std::fs::read_dir(&meta) {
        let mut dirs: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        // Ordem estável; LoL vem antes de TFT (mesma pasta) e vence a deduplicação.
        dirs.sort();
        for d in dirs {
            let dirnome = d
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let Some(id) = dirnome
                .strip_suffix(".live")
                .or_else(|| dirnome.rsplit_once('.').map(|(a, _)| a))
            else {
                continue;
            };
            let yaml = d.join(format!("{dirnome}.product_settings.yaml"));
            let Ok(src) = std::fs::read_to_string(&yaml) else {
                continue;
            };
            let p = produto(&src);
            let Some(pasta) = p.pasta else { continue };
            let mut j = DetectedGame::new(Launcher::Riot, nome_produto(id), pasta)
                .with_app_id(dirnome.clone());
            j.exige_vanguard = p.vanguard;
            out.push(j);
        }
    }
    let installs = riot.join("RiotClientInstalls.json");
    if let Ok(src) = std::fs::read_to_string(&installs) {
        for pasta in pastas_associadas(&src)? {
            let chave = pasta_canonica(&pasta);
            if out.iter().any(|j| j.chave() == chave) {
                continue;
            }
            let nome = pasta
                .components()
                .next_back()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .unwrap_or_default();
            out.push(DetectedGame::new(Launcher::Riot, nome, pasta));
        }
    }
    Ok(out)
}

pub fn varrer() -> Result<Vec<DetectedGame>, String> {
    varrer_dir(&super::program_data().join("Riot Games"))
}
