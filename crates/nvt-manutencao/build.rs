//! Trava da feature `build-de-teste` (D9).

include!("../../assets/trava-build-de-teste.rs");

fn main() {
    println!("cargo:rerun-if-changed=../../assets/trava-build-de-teste.rs");
    trava_build_de_teste("build-de-teste");
}
