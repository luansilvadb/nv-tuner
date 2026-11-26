#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::process::ExitCode;

#[cfg(windows)]
fn main() -> ExitCode {
    use std::io::{self, Write};

    use nvt_agente::{Controle, windows::executar};
    let restaurar = match std::env::args_os().skip(1).collect::<Vec<_>>().as_slice() {
        [] => false,
        [arg] if arg == "--restaurar" => true,
        _ => return ExitCode::from(2),
    };
    let motivo = match executar(restaurar) {
        Ok(Controle::Encerrar) if restaurar => {
            let _ = writeln!(
                io::stdout().lock(),
                "Restauração ignorada: sem perfil da sessão interativa."
            );
            return ExitCode::SUCCESS;
        }
        Ok(Controle::Bloqueado(motivo)) => motivo,
        Ok(Controle::Encerrar | Controle::Restaurado) => return ExitCode::SUCCESS,
        Ok(Controle::Continuar) => "encerramento inesperado do ciclo".into(),
        Err(erro) => erro.to_string(),
    };
    let operacao = if restaurar {
        "Restauração do usuário bloqueada"
    } else {
        "Falha no agente"
    };
    let _ = writeln!(io::stderr().lock(), "{operacao}: {motivo}");
    ExitCode::FAILURE
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    ExitCode::FAILURE
}
