//! Imprime o refresh e o VRR do monitor principal lidos pela NVAPI (somente leitura).
//!
//! ```text
//! cargo run -p nvt-drs --example exibicao
//! ```

use std::process::ExitCode;

use nvt_drs::{FonteExibicao, NvApi, ler_exibicao_principal};

fn main() -> ExitCode {
    let api = match NvApi::load() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("NVAPI indisponível: {e}");
            return ExitCode::FAILURE;
        }
    };
    match api.caminhos() {
        Ok(c) => println!("caminhos: {c:#?}"),
        Err(e) => println!("caminhos: erro {e}"),
    }
    let info = ler_exibicao_principal(&api);
    println!("{info:#?}");
    if let Some(r) = info.refresh_mhz {
        println!("refresh: {:.3} Hz", f64::from(r) / 1000.0);
    }
    ExitCode::SUCCESS
}
