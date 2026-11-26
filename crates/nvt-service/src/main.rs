//! Serviço do Windows do nv-tuner.
//!
//! Único componente que grava no driver. Aplica o plano (só regras estáveis)
//! com snapshot original antes da primeira mudança, vigia o DRS para tratar
//! deriva, e atende a UI pelo named pipe. Detalhes: `docs/servico.md`.

#[cfg(not(windows))]
compile_error!("nvt-service só existe para Windows");

mod atualizacao;
mod diretorio;
mod laco;
mod logs;
mod nomes;
mod nucleo;
mod recomendadas;
mod relatorio;
mod scm;
mod usuario;
mod vigia;

#[cfg(test)]
mod testes;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use clap::{Parser, Subcommand};
use nvt_engine::Armazem;

use crate::laco::{Evento, Opcoes};
use crate::nucleo::Config;

/// Serviço privilegiado do nv-tuner (aplica configurações no driver NVIDIA).
#[derive(Parser, Debug)]
#[command(name = "nvt-service", version)]
struct Cli {
    /// Diretório de estado (padrão: %ProgramData%\nv-tuner).
    #[arg(long, global = true, value_name = "DIR")]
    estado_dir: Option<PathBuf>,
    /// Impede qualquer gravação no driver e no estado, mesmo em `run`/`service`.
    #[arg(long, global = true)]
    somente_leitura: bool,
    /// Nome do named pipe do IPC (para testes sem conflitar com o serviço).
    #[arg(long, global = true, value_name = "NOME", default_value = nvt_ipc::NOME_PIPE)]
    pipe: String,
    /// Logs em nível de depuração.
    #[arg(long, global = true)]
    detalhado: bool,
    #[command(subcommand)]
    comando: Option<Comando>,
}

#[derive(Subcommand, Debug)]
enum Comando {
    /// Entrada do Service Control Manager (padrão sem subcomando).
    Service,
    /// Mesma lógica do serviço, no console (desenvolvimento). Ctrl+C (ou a linha \"sair\") encerra.
    Run {
        /// Não vigia o diretório do DRS.
        #[arg(long)]
        sem_vigia: bool,
    },
    /// Somente leitura: mostra o plano e o diff contra o driver.
    DryRun,
    /// Somente leitura: driver, GPU, capacidades, jogos e conformidade.
    Status,
    /// Aplica o plano agora (exige administrador) e reativa a autonomia.
    Apply {
        /// Confirma a gravação no driver.
        #[arg(long)]
        yes: bool,
    },
    /// Restaura o estado original do driver e pausa a autonomia (exige administrador).
    RestoreOriginal {
        /// Confirma a gravação no driver.
        #[arg(long)]
        yes: bool,
    },
    /// Cria ou corrige o diretório de estado (dono e ACL seguros; conteúdo
    /// inseguro vai para quarentena). Usado pelo instalador; exige administrador.
    PrepareStateDir,
}

/// Confere o diretório de estado antes de abrir os logs. Devolve se o log em
/// arquivo é permitido e o resultado (relatado depois que os logs sobem).
fn preparar_estado(
    dir: &std::path::Path,
    somente_leitura: bool,
) -> (bool, Result<diretorio::Situacao, String>) {
    let modo = if somente_leitura {
        diretorio::Modo::Avisar
    } else {
        diretorio::modo_do_processo()
    };
    let r = diretorio::preparar(dir, modo);
    let arquivo = match &r {
        // Privilegiado num diretório inseguro (só em --somente-leitura): sem
        // log em arquivo, que seria escrito onde o dono do diretório quisesse.
        Ok(diretorio::Situacao::Inseguro(_)) => !diretorio::processo_privilegiado(),
        Ok(_) => true,
        Err(_) => false,
    };
    (arquivo, r)
}

fn opcoes(cli: &Cli, vigiar: bool) -> Opcoes {
    Opcoes {
        pipe: normalizar_pipe(&cli.pipe),
        estado_dir: cli
            .estado_dir
            .clone()
            .unwrap_or_else(Armazem::diretorio_padrao),
        vigiar: vigiar.then(vigia::diretorio_drs),
        config: Config {
            somente_leitura: cli.somente_leitura,
            ..Config::default()
        },
    }
}

/// Aceita `nome`, `pipe\nome` ou `\\.\pipe\nome` (alguns shells comem as
/// barras iniciais) e devolve sempre o caminho completo do pipe.
pub fn normalizar_pipe(s: &str) -> String {
    const PREFIXO: &str = r"\\.\pipe\";
    if s.starts_with(PREFIXO) {
        return s.to_owned();
    }
    let t = s.trim_start_matches(['\\', '/', '.']);
    let t = t
        .strip_prefix("pipe\\")
        .or_else(|| t.strip_prefix("pipe/"))
        .unwrap_or(t);
    format!("{PREFIXO}{t}")
}

static CTRL_C: OnceLock<Sender<Evento>> = OnceLock::new();

unsafe extern "system" fn ao_ctrl_c(_tipo: u32) -> windows_sys::core::BOOL {
    if let Some(tx) = CTRL_C.get() {
        let _ = tx.send(Evento::Parar);
    }
    1
}

fn rodar_console(cli: &Cli, sem_vigia: bool) -> ExitCode {
    let op = opcoes(cli, !sem_vigia);
    let (arquivo, r) = preparar_estado(&op.estado_dir, cli.somente_leitura);
    let _guarda = logs::iniciar(&op.estado_dir, true, arquivo, cli.detalhado);
    if r.is_err() {
        laco::relatar_diretorio(&op.estado_dir, &r);
    }
    let (tx, rx) = mpsc::channel();
    let _ = CTRL_C.set(tx.clone());
    // SAFETY: rotina estática com a assinatura de PHANDLER_ROUTINE.
    unsafe { windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(ao_ctrl_c), 1) };
    // Uma linha "sair" no stdin também encerra (útil em scripts); EOF é ignorado.
    let tx_stdin = tx.clone();
    let _ = std::thread::Builder::new()
        .name("nvt-stdin".into())
        .spawn(move || {
            for linha in std::io::stdin().lines().map_while(Result::ok) {
                if linha.trim().eq_ignore_ascii_case("sair") {
                    let _ = tx_stdin.send(Evento::Parar);
                    return;
                }
            }
        });
    match laco::executar(&op, tx, rx, || {
        eprintln!(
            "nvt-service em execução no console (pipe {}); Ctrl+C ou a linha \"sair\" encerra.",
            op.pipe
        );
    }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Erro: {e}");
            ExitCode::FAILURE
        }
    }
}

fn rodar_servico(cli: &Cli) -> ExitCode {
    let op = opcoes(cli, true);
    let (arquivo, r) = preparar_estado(&op.estado_dir, cli.somente_leitura);
    let guarda = logs::iniciar(&op.estado_dir, false, arquivo, cli.detalhado);
    if r.is_err() {
        laco::relatar_diretorio(&op.estado_dir, &r);
    }
    let r = scm::iniciar(op);
    drop(guarda);
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(1063) => {
            eprintln!(
                "nvt-service não foi iniciado pelo Service Control Manager.\n\
                 Use `nvt-service run` no console ou `nvt-service --help`."
            );
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("Erro do SCM: {e}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    logs::registrar_panicos();
    if diretorio::processo_privilegiado()
        && let Err(e) = diretorio::definir_dono_padrao()
    {
        eprintln!("aviso: não foi possível definir Administradores como dono padrão: {e}");
    }
    let estado_explicito = cli.estado_dir.is_some();
    let dir = cli
        .estado_dir
        .clone()
        .unwrap_or_else(Armazem::diretorio_padrao);
    match &cli.comando {
        None | Some(Comando::Service) => rodar_servico(&cli),
        Some(Comando::Run { sem_vigia }) => rodar_console(&cli, *sem_vigia),
        Some(Comando::Status) => relatorio::status(&dir, &normalizar_pipe(&cli.pipe)),
        Some(Comando::DryRun) => relatorio::dry_run(&dir),
        Some(Comando::PrepareStateDir) => {
            if !diretorio::processo_privilegiado() {
                eprintln!("Recusado: preparar o diretório de estado exige administrador.");
                return ExitCode::from(2);
            }
            match diretorio::preparar(&dir, diretorio::Modo::Exigir) {
                Ok(s) => {
                    match s {
                        diretorio::Situacao::Corrigido {
                            quarentena,
                            problemas,
                        } => println!(
                            "Diretório de estado inseguro ({}); movido para {} e recriado.",
                            diretorio::resumo(&problemas),
                            quarentena.display()
                        ),
                        diretorio::Situacao::PosseTomada { itens } => println!(
                            "Diretório de estado: Administradores tomou posse de {itens} item(ns)."
                        ),
                        diretorio::Situacao::Criado => {
                            println!("Diretório de estado criado: {}", dir.display());
                        }
                        _ => println!("Diretório de estado seguro: {}", dir.display()),
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("Erro: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(Comando::Apply { yes }) | Some(Comando::RestoreOriginal { yes }) => {
            let restaurar = matches!(cli.comando, Some(Comando::RestoreOriginal { .. }));
            if !yes {
                eprintln!(
                    "Recusado: esta ação grava no driver. Confirme com --yes \
                     (use `dry-run` para ver o plano sem gravar)."
                );
                return ExitCode::from(2);
            }
            if cli.somente_leitura {
                eprintln!("Recusado: --somente-leitura impede gravações.");
                return ExitCode::from(2);
            }
            if !diretorio::processo_privilegiado() {
                eprintln!("Recusado: gravar no driver exige administrador.");
                return ExitCode::FAILURE;
            }
            let r = diretorio::preparar(&dir, diretorio::Modo::Exigir);
            let _guarda = logs::iniciar(&dir, true, r.is_ok(), cli.detalhado);
            laco::relatar_diretorio(&dir, &r);
            if let Err(e) = r {
                eprintln!("Recusado: {e}");
                return ExitCode::FAILURE;
            }
            relatorio::gravar(
                &dir,
                &normalizar_pipe(&cli.pipe),
                !estado_explicito,
                restaurar,
            )
        }
    }
}

/// Tempo de espera do cliente IPC usado pelos comandos `apply`/`restore-original`.
pub const ESPERA_CLIENTE: Duration = Duration::from_secs(120);
