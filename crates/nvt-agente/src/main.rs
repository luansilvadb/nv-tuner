#![cfg_attr(windows, windows_subsystem = "windows")]

use std::process::ExitCode;

#[cfg(windows)]
fn main() -> ExitCode {
    use nvt_agente::{
        Controle,
        windows::{SomenteTela, executar},
    };
    let restaurar = match std::env::args_os().skip(1).collect::<Vec<_>>().as_slice() {
        [] => false,
        [arg] if arg == "--restaurar" => true,
        _ => return ExitCode::from(2),
    };
    match executar(SomenteTela, restaurar) {
        Ok(Controle::Encerrar | Controle::Restaurado) => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    ExitCode::FAILURE
}
