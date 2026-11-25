//! GOG, Ubisoft Connect, EA app e Battle.net (registro e chaves de desinstalação).

use std::path::PathBuf;

use crate::model::{DetectedGame, Launcher};
use crate::registro::{self, EntradaDesinstalacao, Hive};

fn caminho(s: &str) -> PathBuf {
    PathBuf::from(s.trim().trim_matches('"').replace('/', "\\"))
}

fn ultimo_componente(p: &std::path::Path) -> String {
    p.components()
        .next_back()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .unwrap_or_default()
}

/// GOG: `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<id>` (`gameName`, `path`, `exe`).
pub fn gog() -> Result<Vec<DetectedGame>, String> {
    let base = r"SOFTWARE\WOW6432Node\GOG.com\Games";
    let mut out = Vec::new();
    for id in registro::subchaves(Hive::Hklm, base) {
        let k = format!(r"{base}\{id}");
        let Some(pasta) = registro::texto(Hive::Hklm, &k, "path") else {
            continue;
        };
        let pasta = caminho(&pasta);
        let nome = registro::texto(Hive::Hklm, &k, "gameName").unwrap_or_else(|| id.clone());
        let mut j = DetectedGame::new(Launcher::Gog, nome, &pasta).with_app_id(id);
        j.exe_indicado = registro::texto(Hive::Hklm, &k, "exe").map(|e| caminho(&e));
        out.push(j);
    }
    Ok(out)
}

/// Ubisoft Connect: `HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\<id>\InstallDir`.
pub fn ubisoft() -> Result<Vec<DetectedGame>, String> {
    let base = r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs";
    let mut out = Vec::new();
    for id in registro::subchaves(Hive::Hklm, base) {
        let Some(dir) = registro::texto(Hive::Hklm, &format!(r"{base}\{id}"), "InstallDir") else {
            continue;
        };
        let pasta = caminho(&dir);
        out.push(
            DetectedGame::new(Launcher::Ubisoft, ultimo_componente(&pasta), pasta).with_app_id(id),
        );
    }
    Ok(out)
}

/// Nomes de aplicativos de launcher (não são jogos).
const LAUNCHERS: &[&str] = &[
    "ea app",
    "origin",
    "battle.net",
    "steam",
    "epic games launcher",
    "riot client",
    "ubisoft connect",
    "uplay",
    "gog galaxy",
    "xbox",
];

fn e_launcher(nome: &str) -> bool {
    let n = nome.to_lowercase();
    LAUNCHERS.iter().any(|l| n == *l)
}

/// Classifica uma entrada de desinstalação como jogo da EA ou da Blizzard.
pub fn classificar_publisher(e: &EntradaDesinstalacao) -> Option<DetectedGame> {
    let nome = e.display_name.as_deref()?;
    let pasta = e.install_location.as_deref()?;
    if e_launcher(nome) {
        return None;
    }
    let publ = e.publisher.as_deref().unwrap_or("").to_lowercase();
    let launcher = if publ.contains("blizzard entertainment") {
        Launcher::BattleNet
    } else if publ.contains("electronic arts") {
        Launcher::Ea
    } else {
        return None;
    };
    Some(DetectedGame::new(launcher, nome, caminho(pasta)).with_app_id(e.chave.clone()))
}

/// EA app: `...\Electronic Arts\<jogo>\Install Dir` e `...\EA Games\<jogo>`, mais `Uninstall`.
pub fn ea(entradas: &[EntradaDesinstalacao]) -> Result<Vec<DetectedGame>, String> {
    let mut out = Vec::new();
    for base in [
        r"SOFTWARE\WOW6432Node\Electronic Arts",
        r"SOFTWARE\WOW6432Node\EA Games",
        r"SOFTWARE\Electronic Arts",
    ] {
        for nome in registro::subchaves(Hive::Hklm, base) {
            if e_launcher(&nome) || nome.eq_ignore_ascii_case("EA Desktop") {
                continue;
            }
            let k = format!(r"{base}\{nome}");
            let Some(dir) = registro::texto(Hive::Hklm, &k, "Install Dir")
                .or_else(|| registro::texto(Hive::Hklm, &k, "InstallDir"))
            else {
                continue;
            };
            out.push(DetectedGame::new(Launcher::Ea, nome, caminho(&dir)));
        }
    }
    out.extend(
        entradas
            .iter()
            .filter_map(classificar_publisher)
            .filter(|j| j.launcher == Launcher::Ea),
    );
    Ok(out)
}

/// Battle.net: melhor esforço pelas entradas com `Publisher = Blizzard Entertainment`.
pub fn battlenet(entradas: &[EntradaDesinstalacao]) -> Result<Vec<DetectedGame>, String> {
    Ok(entradas
        .iter()
        .filter_map(classificar_publisher)
        .filter(|j| j.launcher == Launcher::BattleNet)
        .collect())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn e(nome: &str, pasta: &str, publ: &str) -> EntradaDesinstalacao {
        EntradaDesinstalacao {
            chave: "x".into(),
            display_name: Some(nome.into()),
            install_location: Some(pasta.into()),
            publisher: Some(publ.into()),
        }
    }

    #[test]
    fn publisher() {
        let b = classificar_publisher(&e("Jogo B", r"D:\B", "Blizzard Entertainment")).unwrap();
        assert_eq!(b.launcher, Launcher::BattleNet);
        let a = classificar_publisher(&e("Jogo A", r"D:\A", "Electronic Arts")).unwrap();
        assert_eq!(a.launcher, Launcher::Ea);
        assert!(
            classificar_publisher(&e("Battle.net", r"D:\BN", "Blizzard Entertainment")).is_none()
        );
        assert!(classificar_publisher(&e("Editor", r"D:\E", "Outra")).is_none());
    }
}
