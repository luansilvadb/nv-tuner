//! Trava da feature `build-de-teste` (docs/spec/interface-e-saude.md §8.4, D9).

fn main() {
    println!("cargo:rustc-check-cfg=cfg(nvt_build_de_teste)");
    for v in ["NVT_BUILD_DE_TESTE", "CI", "GITHUB_ACTIONS"] {
        println!("cargo:rerun-if-env-changed={v}");
    }
    if std::env::var_os("CARGO_FEATURE_BUILD_DE_TESTE").is_none() {
        return;
    }
    let liberado = std::env::var("NVT_BUILD_DE_TESTE").as_deref() == Ok("1");
    let ci = std::env::var_os("CI").is_some() || std::env::var_os("GITHUB_ACTIONS").is_some();
    if !liberado || ci {
        panic!("build-de-teste exige NVT_BUILD_DE_TESTE=1 e não roda em CI");
    }
    println!("cargo:rustc-cfg=nvt_build_de_teste");
}
