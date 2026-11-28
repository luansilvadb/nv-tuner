//! GOG, Ubisoft Connect, EA app e Battle.net (registro e chaves de desinstalação).

use std::path::PathBuf;

use super::{Contexto, ultimo_componente};
use crate::model::{DetectedGame, Launcher, caminho_windows};
use crate::registro::{self, EntradaDesinstalacao, Hive};

fn caminho(s: &str) -> PathBuf {
    caminho_windows(s.trim().trim_matches('"'))
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

/// Nome de aplicativo de launcher (não é jogo): igualdade com o nome em minúsculas,
/// lista `nomes_launcher` de `data/launchers.toml`.
fn e_launcher(nome: &str) -> bool {
    super::dados::tabela()
        .nomes_launcher
        .contains(&nome.to_lowercase())
}

/// Classifica uma entrada de desinstalação como jogo da EA ou da Blizzard.
pub fn classificar_publisher(e: &EntradaDesinstalacao) -> Option<DetectedGame> {
    let nome = e.display_name.as_deref()?;
    let pasta = e.install_location.as_deref()?;
    if e_launcher(nome) {
        return None;
    }
    let publ = e.publisher.as_deref().unwrap_or("").to_lowercase();
    // Primeira regra `[[publisher]]` de `data/launchers.toml` cujo texto esteja contido.
    let launcher = super::dados::tabela()
        .publisher
        .iter()
        .find(|(p, _)| publ.contains(p.as_str()))
        .map(|(_, l)| *l)?;
    Some(DetectedGame::new(launcher, nome, caminho(pasta)).with_app_id(e.chave.clone()))
}

/// EA app: `...\Electronic Arts\<jogo>\Install Dir` e `...\EA Games\<jogo>`, mais `Uninstall`.
pub fn ea(entradas: &[EntradaDesinstalacao]) -> Result<Vec<DetectedGame>, String> {
    let mut out = ea_registro();
    out.extend(
        entradas
            .iter()
            .filter_map(classificar_publisher)
            .filter(|j| j.launcher == Launcher::Ea),
    );
    Ok(out)
}

/// Parte do scanner da EA que lê só as chaves próprias do registro.
fn ea_registro() -> Vec<DetectedGame> {
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
    out
}

/// Jogos de `launcher` entre os já classificados por publisher, na ordem original.
fn do_launcher(classificados: &[DetectedGame], launcher: Launcher) -> Vec<DetectedGame> {
    classificados
        .iter()
        .filter(|j| j.launcher == launcher)
        .cloned()
        .collect()
}

/// EA app pelo contexto (a classificação por publisher é feita uma vez só).
pub(crate) fn ea_ctx(ctx: &Contexto) -> Result<Vec<DetectedGame>, String> {
    let mut out = ea_registro();
    out.extend(do_launcher(ctx.por_publisher(), Launcher::Ea));
    Ok(out)
}

/// Battle.net pelo contexto.
pub(crate) fn battlenet_ctx(ctx: &Contexto) -> Result<Vec<DetectedGame>, String> {
    Ok(do_launcher(ctx.por_publisher(), Launcher::BattleNet))
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
