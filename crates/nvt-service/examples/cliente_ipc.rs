//! Cliente de teste do IPC: conecta num pipe (padrão `\\.\pipe\nv-tuner`) e
//! imprime a resposta do serviço em JSON.
//!
//! ```text
//! cargo run -p nvt-service --example cliente_ipc -- nvt-teste estado
//! ```
//!
//! Requisições: `estado` (padrão), `plano`, `historico`, `aceitar <regra>`,
//! `retirar <regra>`. Só aceita servidor
//! cujo dono seja SYSTEM ou Administradores (como a UI), exceto com
//! `--sem-verificar-dono`.

use std::process::ExitCode;

use nvt_ipc::pipe::{Cliente, ConfigCliente};
use nvt_ipc::{NOME_PIPE, Requisicao};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let sem_dono = args.iter().any(|a| a == "--sem-verificar-dono");
    let pos: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    // Aceita também só o nome curto (ex.: `nvt-teste`): alguns shells comem
    // as barras iniciais de `\\.\pipe\...`.
    const PREFIXO: &str = r"\\.\pipe\";
    let pipe = match pos.first() {
        None => NOME_PIPE.to_owned(),
        Some(s) if s.starts_with(PREFIXO) => s.to_string(),
        Some(s) => {
            let t = s.trim_start_matches(['\\', '/', '.']);
            let t = t.strip_prefix(r"pipe\").unwrap_or(t);
            format!("{PREFIXO}{t}")
        }
    };
    let pipe = pipe.as_str();
    let req = match pos.get(1).map(|s| s.as_str()).unwrap_or("estado") {
        "estado" => Requisicao::ObterEstado,
        "plano" => Requisicao::ObterPlano,
        "historico" => Requisicao::ListarHistorico { limite: 20 },
        acao @ ("aceitar" | "retirar") => {
            let Some(regra) = pos.get(2) else {
                eprintln!("Informe o ID da regra.");
                return ExitCode::from(2);
            };
            Requisicao::DefinirAceiteExperimental {
                regra: regra.to_string(),
                aceito: acao == "aceitar",
            }
        }
        outra => {
            eprintln!(
                "requisição desconhecida: {outra} (use estado, plano, historico, aceitar ou retirar)"
            );
            return ExitCode::from(2);
        }
    };
    let cfg = ConfigCliente {
        timeout_conexao: std::time::Duration::from_secs(10),
        exigir_dono_privilegiado: !sem_dono,
        ..ConfigCliente::default()
    };
    let r = Cliente::conectar(pipe, &cfg).and_then(|mut c| c.requisitar(&req));
    match r {
        Ok(resp) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&resp).unwrap_or_else(|e| e.to_string())
            );
            if matches!(resp, nvt_ipc::Resposta::Erro { .. }) {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("Erro: {e}");
            ExitCode::FAILURE
        }
    }
}
