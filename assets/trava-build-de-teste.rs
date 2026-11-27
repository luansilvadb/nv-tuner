// Trava das features de build de teste,
// incluída pelos build.rs de nvt-medicao e nvt-manutencao com `include!`.
//
// Com a feature ligada, o build exige `NVT_BUILD_DE_TESTE=1` e recusa CI
// (`CI` ou `GITHUB_ACTIONS` presentes); passando, emite `cfg(nvt_build_de_teste)`.
// Não bloqueia o perfil release.

/// `feature` é o nome da feature no Cargo.toml (ex.: `"build-de-teste"`).
fn trava_build_de_teste(feature: &str) {
    println!("cargo:rustc-check-cfg=cfg(nvt_build_de_teste)");
    for v in ["NVT_BUILD_DE_TESTE", "CI", "GITHUB_ACTIONS"] {
        println!("cargo:rerun-if-env-changed={v}");
    }
    let var = format!(
        "CARGO_FEATURE_{}",
        feature.to_ascii_uppercase().replace('-', "_")
    );
    if std::env::var_os(var).is_none() {
        return;
    }
    if std::env::var("NVT_BUILD_DE_TESTE").as_deref() != Ok("1") {
        panic!("a feature {feature} exige NVT_BUILD_DE_TESTE=1 (build de teste, §8.4)");
    }
    if std::env::var_os("CI").is_some() || std::env::var_os("GITHUB_ACTIONS").is_some() {
        panic!("a feature {feature} é proibida em CI (§8.4)");
    }
    println!("cargo:rustc-cfg=nvt_build_de_teste");
}
