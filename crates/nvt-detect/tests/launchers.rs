//! Scanners de launcher contra fixtures sintéticas (`tests/fixtures`).

use std::path::{Path, PathBuf};

use nvt_detect::launchers::{epic, riot, steam, xbox};
use nvt_detect::{Launcher, pasta_canonica};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(rel)
}

fn ler(rel: &str) -> String {
    std::fs::read_to_string(fixture(rel)).unwrap()
}

#[test]
fn steam_bibliotecas_formato_novo_e_antigo() {
    let libs = steam::bibliotecas(&ler("steam/steamapps/libraryfolders.vdf")).unwrap();
    assert_eq!(
        libs,
        vec![
            PathBuf::from(r"C:\Jogos\Steam"),
            PathBuf::from(r"D:\Biblioteca")
        ]
    );
    let antigo = steam::bibliotecas(&ler("steam/libraryfolders_antigo.vdf")).unwrap();
    assert_eq!(antigo, vec![PathBuf::from(r"E:\SteamLibrary")]);
}

#[test]
fn steam_manifestos() {
    let lib = Path::new(r"C:\Jogos\Steam");
    let j = steam::jogo_de_manifesto(&ler("steam/steamapps/appmanifest_1000.acf"), lib)
        .unwrap()
        .unwrap();
    assert_eq!(j.launcher, Launcher::Steam);
    assert_eq!(j.nome, "Jogo Exemplo");
    assert_eq!(j.app_id.as_deref(), Some("1000"));
    assert_eq!(
        j.pasta,
        PathBuf::from(r"C:\Jogos\Steam\steamapps\common\JogoExemplo")
    );
    // Redistribuível e instalação incompleta são filtrados.
    assert!(
        steam::jogo_de_manifesto(&ler("steam/steamapps/appmanifest_228980.acf"), lib)
            .unwrap()
            .is_none()
    );
    assert!(
        steam::jogo_de_manifesto(&ler("steam/steamapps/appmanifest_2000.acf"), lib)
            .unwrap()
            .is_none()
    );
    assert!(steam::jogo_de_manifesto("\"AppState\" {", lib).is_err());
}

#[test]
fn steam_varre_raiz_de_fixture() {
    let jogos = steam::varrer_raiz(&fixture("steam")).unwrap();
    assert_eq!(jogos.len(), 1);
    assert_eq!(jogos[0].app_id.as_deref(), Some("1000"));
}

#[test]
fn riot_yaml_e_json() {
    let p = riot::produto(&ler(
        "riot/Metadata/league_of_legends.live/league_of_legends.live.product_settings.yaml",
    ));
    assert_eq!(
        p.pasta,
        Some(PathBuf::from(r"X:\Riot Games\League of Legends"))
    );
    assert!(p.vanguard);
    let v = riot::produto(&ler(
        "riot/Metadata/valorant.live/valorant.live.product_settings.yaml",
    ));
    assert!(!v.vanguard);
    assert_eq!(v.pasta, Some(PathBuf::from(r"X:\Riot Games\VALORANT\live")));

    let pastas = riot::pastas_associadas(&ler("riot/RiotClientInstalls.json")).unwrap();
    assert_eq!(pastas.len(), 2);
}

#[test]
fn riot_varre_dir() {
    let jogos = riot::varrer_dir(&fixture("riot")).unwrap();
    let nomes: Vec<&str> = jogos.iter().map(|j| j.nome.as_str()).collect();
    // LoL (metadata), VALORANT (metadata) e "Outro Jogo" (só associated_client);
    // a pasta do LoL em associated_client não duplica.
    assert_eq!(nomes, vec!["League of Legends", "VALORANT", "Outro Jogo"]);
    assert!(jogos[0].exige_vanguard);
    assert!(!jogos[1].exige_vanguard);
    assert!(jogos.iter().all(|j| j.launcher == Launcher::Riot));
    assert_eq!(
        pasta_canonica(&jogos[0].pasta),
        pasta_canonica(Path::new("X:/Riot Games/League of Legends/"))
    );
}

#[test]
fn epic_items() {
    let j = epic::jogo_de_item(&ler("epic/jogo.item")).unwrap().unwrap();
    assert_eq!(j.nome, "Jogo Epic");
    assert_eq!(j.app_id.as_deref(), Some("abc123"));
    assert_eq!(
        j.exe_indicado,
        Some(PathBuf::from(
            r"X:\Epic Games\JogoEpic\Binaries\Win64\JogoEpic.exe"
        ))
    );
    assert!(
        epic::jogo_de_item(&ler("epic/incompleto.item"))
            .unwrap()
            .is_none()
    );
    assert!(
        epic::jogo_de_item(&ler("epic/plugin.item"))
            .unwrap()
            .is_none()
    );
    assert!(epic::jogo_de_item("{").is_err());
    assert_eq!(epic::varrer_dir(&fixture("epic")).unwrap().len(), 1);
}

#[test]
fn xbox_config() {
    let c = xbox::config(&ler("xbox/JogoX/Content/MicrosoftGame.config"));
    assert_eq!(c.nome.as_deref(), Some("Jogo X & Cia"));
    assert_eq!(c.exe.as_deref(), Some("gamelaunchhelper.exe"));
    assert_eq!(c.identidade.as_deref(), Some("Exemplo.JogoX"));
    let jogos = xbox::varrer_dir(&fixture("xbox"));
    assert_eq!(jogos.len(), 1);
    assert_eq!(jogos[0].launcher, Launcher::Xbox);
    assert!(jogos[0].pasta.ends_with("Content"));
    assert!(
        jogos[0]
            .exe_indicado
            .as_ref()
            .unwrap()
            .ends_with("gamelaunchhelper.exe")
    );
}
