//! Trava da feature `medicao-partidas` (docs/spec/interface-e-saude.md §8.4, item 1).
//!
//! Com a feature ligada, o build exige `NVT_BUILD_DE_TESTE=1` e recusa CI
//! (`CI` ou `GITHUB_ACTIONS` presentes). Não bloqueia o perfil release: o custo de FPS
//! só vale medido em build otimizado.

use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=NVT_BUILD_DE_TESTE");
    println!("cargo:rerun-if-env-changed=CI");
    println!("cargo:rerun-if-env-changed=GITHUB_ACTIONS");
    println!("cargo:rustc-check-cfg=cfg(nvt_build_de_teste)");

    if env::var_os("CARGO_FEATURE_MEDICAO_PARTIDAS").is_none() {
        return;
    }
    if env::var("NVT_BUILD_DE_TESTE").as_deref() != Ok("1") {
        panic!("a feature medicao-partidas exige NVT_BUILD_DE_TESTE=1 (build de teste, §8.4)");
    }
    if env::var_os("CI").is_some() || env::var_os("GITHUB_ACTIONS").is_some() {
        panic!("a feature medicao-partidas é proibida em CI (§8.4)");
    }
    println!("cargo:rustc-cfg=nvt_build_de_teste");
}
