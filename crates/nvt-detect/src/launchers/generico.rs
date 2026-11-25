//! Rede de segurança: chaves de desinstalação do Windows.
//!
//! Aceita entradas com padrão conhecido de launcher (`Steam App <id>`,
//! `Riot Game <produto>`) ou cuja pasta tenha marcadores de jogo.

use std::path::{Path, PathBuf};

use crate::model::{DetectedGame, Launcher};
use crate::registro::EntradaDesinstalacao;

/// Nomes que indicam software que não é jogo.
const NAO_JOGO: &[&str] = &[
    "vanguard",
    "redistributable",
    "runtime",
    "anti-cheat",
    "anticheat",
    "driver",
    "sdk",
    "launcher",
    "riot client",
    "battle.net",
    "ea app",
    "gog galaxy",
    "ubisoft connect",
];

/// Arquivos/pastas cuja presença indica uma instalação de jogo.
fn tem_marcador(pasta: &Path, profundidade: u32) -> bool {
    let Ok(rd) = std::fs::read_dir(pasta) else {
        return false;
    };
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().to_lowercase();
        let dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let marcador = matches!(
            n.as_str(),
            "unityplayer.dll" | "nvngx_dlss.dll" | "easyanticheat" | "battleye" | "d3d12core.dll"
        ) || (n.ends_with("-shipping.exe"))
            || (dir && n == "binaries" && e.path().join("Win64").is_dir());
        if marcador {
            return true;
        }
        if dir && profundidade > 0 && tem_marcador(&e.path(), profundidade - 1) {
            return true;
        }
    }
    false
}

/// Pastas que nunca são instalação de jogo (Windows, raiz de unidade, Program Files).
pub fn pasta_de_sistema(pasta: &Path) -> bool {
    let s = pasta
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase();
    let windows = std::env::var("SystemRoot")
        .unwrap_or_else(|_| r"C:\Windows".into())
        .to_lowercase();
    let raiz = s.len() <= 2;
    let program_files = s.ends_with(r":\program files") || s.ends_with(r":\program files (x86)");
    raiz || program_files || s == windows || s.starts_with(&format!("{windows}\\"))
}

/// Classifica uma entrada; `sonda` decide pela pasta (injetável nos testes).
pub fn classificar(
    e: &EntradaDesinstalacao,
    sonda: impl Fn(&Path) -> bool,
) -> Option<DetectedGame> {
    let pasta = PathBuf::from(
        e.install_location
            .as_deref()?
            .trim_matches('"')
            .replace('/', "\\"),
    );
    if pasta_de_sistema(&pasta) {
        return None;
    }
    let nome = e.display_name.clone().unwrap_or_else(|| e.chave.clone());
    if let Some(id) = e.chave.strip_prefix("Steam App ") {
        return Some(DetectedGame::new(Launcher::Steam, nome, pasta).with_app_id(id));
    }
    if let Some(id) = e.chave.strip_prefix("Riot Game ") {
        let base = id.split('.').next().unwrap_or(id);
        // O próprio cliente da Riot também aparece como "Riot Game Riot_Client.".
        if base.eq_ignore_ascii_case("riot_client") {
            return None;
        }
        return Some(
            DetectedGame::new(Launcher::Riot, super::riot::nome_produto(base), pasta)
                .with_app_id(id),
        );
    }
    let n = nome.to_lowercase();
    if NAO_JOGO.iter().any(|x| n.contains(x)) {
        return None;
    }
    sonda(&pasta).then(|| DetectedGame::new(Launcher::Generico, nome, pasta))
}

pub fn varrer(entradas: &[EntradaDesinstalacao]) -> Result<Vec<DetectedGame>, String> {
    Ok(entradas
        .iter()
        .filter_map(|e| classificar(e, |p| tem_marcador(p, 2)))
        .collect())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn e(chave: &str, nome: &str, pasta: &str) -> EntradaDesinstalacao {
        EntradaDesinstalacao {
            chave: chave.into(),
            display_name: Some(nome.into()),
            install_location: Some(pasta.into()),
            publisher: None,
        }
    }

    #[test]
    fn padroes_de_launcher() {
        let s = classificar(&e("Steam App 10", "Jogo", r"D:\J"), |_| false).unwrap();
        assert_eq!(s.launcher, Launcher::Steam);
        assert_eq!(s.app_id.as_deref(), Some("10"));
        let r = classificar(
            &e(
                "Riot Game league_of_legends.live",
                "League of Legends",
                "C:/Riot Games/LoL",
            ),
            |_| false,
        )
        .unwrap();
        assert_eq!(r.launcher, Launcher::Riot);
        assert_eq!(r.nome, "League of Legends");
    }

    #[test]
    fn filtra_nao_jogos_e_usa_sonda() {
        assert!(classificar(&e("x", "Riot Vanguard", r"C:\RV"), |_| true).is_none());
        assert!(
            classificar(
                &e("Riot Game Riot_Client.", "Riot Client", r"C:\RC"),
                |_| true
            )
            .is_none()
        );
        assert!(classificar(&e("x", "mstsc", r"C:\Windows\System32\"), |_| true).is_none());
        assert!(classificar(&e("x", "Algo", r"D:\"), |_| true).is_none());
        assert!(classificar(&e("x", "Editor de texto", r"C:\E"), |_| false).is_none());
        let g = classificar(&e("x", "Jogo Indie", r"C:\J"), |_| true).unwrap();
        assert_eq!(g.launcher, Launcher::Generico);
    }

    #[test]
    fn marcador_em_pasta_real() {
        let d = std::env::temp_dir().join(format!("nvt-detect-gen-{}", std::process::id()));
        std::fs::create_dir_all(d.join("Jogo").join("Binaries").join("Win64")).unwrap();
        assert!(tem_marcador(&d, 2));
        std::fs::remove_dir_all(&d).ok();
    }
}
