//! Trava da feature `medicao-partidas`.
//! O custo de FPS só vale medido em build otimizado, por isso o release não é bloqueado.

include!("../../assets/trava-build-de-teste.rs");

fn main() {
    println!("cargo:rerun-if-changed=../../assets/trava-build-de-teste.rs");
    trava_build_de_teste("medicao-partidas");
}
