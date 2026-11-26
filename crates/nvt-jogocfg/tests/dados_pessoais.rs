//! D12: as amostras de teste são sintéticas e não contêm dados pessoais.
//!
//! Procura em `tests/amostras/` (texto decodificado de UTF-8 ou UTF-16LE):
//! - SteamID64 (`7656119` seguido de dígitos, 17 no total) fora da faixa fictícia
//!   reservada (contas 1 a 999: `BASE + 1 ..= BASE + 999`);
//! - endereços de e-mail;
//! - perfis `C:\Users\<nome>` (também com `/` ou `\\`) com nome diferente de `Usuario`.

mod comum;

use comum::amostras;
use nvt_jogocfg::steam::BASE_STEAMID64;
use std::fs;

const FAIXA_FICTICIA: std::ops::RangeInclusive<u64> = BASE_STEAMID64 + 1..=BASE_STEAMID64 + 999;

fn steamids_reais(t: &str) -> Vec<String> {
    let b = t.as_bytes();
    let mut v = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_digit()) {
            let ini = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            let num = &t[ini..i];
            if num.starts_with("7656119") && num.len() >= 17 {
                let ok = num.len() == 17
                    && num
                        .parse::<u64>()
                        .is_ok_and(|n| FAIXA_FICTICIA.contains(&n));
                if !ok {
                    v.push(num.to_owned());
                }
            }
        } else {
            i += 1;
        }
    }
    v
}

fn emails(t: &str) -> Vec<String> {
    let parte = |c: char| c.is_alphanumeric() || "._%+-".contains(c);
    let mut v = Vec::new();
    for (i, _) in t.match_indices('@') {
        let antes = t[..i].chars().rev().take_while(|&c| parte(c)).count();
        let depois: String = t[i + 1..].chars().take_while(|&c| parte(c)).collect();
        if antes > 0 && depois.contains('.') && !depois.ends_with('.') {
            v.push(format!("…@{depois}"));
        }
    }
    v
}

fn perfis(t: &str) -> Vec<String> {
    let baixo = t.to_lowercase();
    let mut v = Vec::new();
    for sep in ["\\\\", "\\", "/"] {
        let agulha = format!("c:{sep}users{sep}");
        for (i, _) in baixo.match_indices(&agulha) {
            let resto = &t[i + agulha.len()..];
            let nome: String = resto
                .chars()
                .take_while(|c| !matches!(c, '\\' | '/' | '"' | '\r' | '\n'))
                .collect();
            if nome != "Usuario" {
                v.push(nome);
            }
        }
    }
    v
}

fn decodificar(b: &[u8]) -> String {
    match nvt_jogocfg::codificacao::decodificar(b) {
        Ok(t) => t.texto,
        Err(_) => String::from_utf8_lossy(b).into_owned(),
    }
}

#[test]
fn detectores_funcionam() {
    let id_real = format!("{}", BASE_STEAMID64 + 123_456_789);
    assert_eq!(steamids_reais(&format!("\"{id_real}\"")).len(), 1);
    assert!(steamids_reais("\"76561197960265729\"").is_empty());
    assert_eq!(emails("x fulano@exemplo.com y").len(), 1);
    assert!(emails("@ a@b").is_empty());
    let perfil = ["C:", "Users", "Fulano", "AppData"].join("\\");
    assert_eq!(perfis(&perfil), vec!["Fulano".to_owned()]);
    assert!(perfis(&["C:", "Users", "Usuario", "x"].join("/")).is_empty());
}

#[test]
fn amostras_nao_contem_dados_pessoais() {
    let mut achados = Vec::new();
    let mut n = 0;
    for e in fs::read_dir(amostras()).unwrap() {
        let p = e.unwrap().path();
        let t = decodificar(&fs::read(&p).unwrap());
        n += 1;
        let nome = p.file_name().unwrap().to_string_lossy().into_owned();
        for x in steamids_reais(&t) {
            achados.push(format!("{nome}: SteamID64 {x}"));
        }
        for x in emails(&t) {
            achados.push(format!("{nome}: e-mail {x}"));
        }
        for x in perfis(&t) {
            achados.push(format!("{nome}: perfil {x}"));
        }
    }
    assert!(n >= 10, "amostras não encontradas");
    assert!(achados.is_empty(), "{achados:#?}");
}
