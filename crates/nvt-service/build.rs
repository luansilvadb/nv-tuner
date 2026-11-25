//! Extrai de `docs/pesquisa/settings-referencia.json` (NVIDIA Profile
//! Inspector, MIT) só os nomes legíveis das settings do catálogo
//! (`regras/settings.toml`), para não embutir a referência inteira (~1 MB).
//! Também embute o ícone e o VERSIONINFO do nvt-service.exe
//! (`assets/recurso-win.rs`).

include!("../../assets/recurso-win.rs");

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::{env, fs};

use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Referencia {
    settings: Vec<SettingRef>,
}

#[derive(Deserialize)]
struct SettingRef {
    id: String,
    nome: String,
    #[serde(default)]
    valores: Vec<ValorRef>,
}

#[derive(Deserialize)]
struct ValorRef {
    valor: String,
    rotulo: String,
}

#[derive(Serialize)]
struct NomeSaida {
    id: u32,
    nome: String,
    valores: Vec<(u32, String)>,
}

fn hex(s: &str) -> Option<u32> {
    u32::from_str_radix(
        s.trim().trim_start_matches("0x").trim_start_matches("0X"),
        16,
    )
    .ok()
}

fn main() {
    embutir_recurso_win("nv-tuner (serviço)", "nvt-service");

    let raiz = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let toml_path = raiz.join("regras/settings.toml");
    let ref_path = raiz.join("docs/pesquisa/settings-referencia.json");
    println!("cargo:rerun-if-changed={}", toml_path.display());
    println!("cargo:rerun-if-changed={}", ref_path.display());

    let catalogo: toml::Value =
        toml::from_str(&fs::read_to_string(&toml_path).expect("regras/settings.toml"))
            .expect("settings.toml válido");
    let ids: BTreeSet<u32> = catalogo
        .get("setting")
        .and_then(|s| s.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.get("id")?.as_integer())
                .filter_map(|i| u32::try_from(i).ok())
                .collect()
        })
        .unwrap_or_default();

    // A referência é opcional: sem ela, os nomes vêm do catálogo.
    let saida: Vec<NomeSaida> = match fs::read_to_string(&ref_path) {
        Ok(texto) => {
            let r: Referencia = serde_json::from_str(&texto).expect("settings-referencia.json");
            r.settings
                .into_iter()
                .filter_map(|s| {
                    let id = hex(&s.id)?;
                    ids.contains(&id).then(|| NomeSaida {
                        id,
                        nome: s.nome,
                        valores: s
                            .valores
                            .into_iter()
                            .filter_map(|v| Some((hex(&v.valor)?, v.rotulo)))
                            .collect(),
                    })
                })
                .collect()
        }
        Err(_) => Vec::new(),
    };
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("nomes.json");
    fs::write(out, serde_json::to_string(&saida).unwrap()).unwrap();
}
