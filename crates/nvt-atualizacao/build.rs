//! Primeira camada de U-15a: a feature `laboratorio` (URL e chave de ensaio) nunca
//! entra em build otimizado, mesmo com `-C debug-assertions=on`.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var_os("CARGO_FEATURE_LABORATORIO").is_none() {
        return;
    }
    let perfil = std::env::var("PROFILE").unwrap_or_default();
    let opt = std::env::var("OPT_LEVEL").unwrap_or_default();
    if perfil == "release" || opt != "0" {
        panic!(
            "a feature `laboratorio` (URL e chave de ensaio) não pode entrar em build \
             otimizado (PROFILE={perfil}, OPT_LEVEL={opt}; U-15a)"
        );
    }
}
