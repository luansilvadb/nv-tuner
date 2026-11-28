//! Logs em arquivo rotativo (`<estado-dir>\logs\nvt-service.AAAA-MM-DD.log`,
//! diário em UTC, últimos 14 arquivos). Os logs registram ações sobre o
//! driver (perfil, setting, valores) e erros; não registram nomes de usuário
//! nem caminhos pessoais além do diretório de estado.
//!
//! O arquivo é aberto pelo próprio [`ArquivoDiario`] (não pelo
//! `tracing-appender`), sem seguir reparse points: se `logs` ou o arquivo do
//! dia for junção, link simbólico ou hard link, o log em arquivo é recusado
//! (ver `diretorio.rs`). A limpeza dos antigos só apaga arquivos comuns.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, fmt};

use crate::diretorio;

const PREFIXO: &str = "nvt-service.";
const SUFIXO: &str = ".log";
const MANTER: usize = 14;

/// Dia civil (UTC) de um número de dias desde 1970-01-01 (Howard Hinnant).
fn data_civil(dias: i64) -> (i64, u32, u32) {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn dia_atual() -> i64 {
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (s / 86_400) as i64
}

fn nome_do_dia(dia: i64) -> String {
    let (y, m, d) = data_civil(dia);
    format!("{PREFIXO}{y:04}-{m:02}-{d:02}{SUFIXO}")
}

/// Escritor diário que abre os arquivos sem seguir reparse points.
pub struct ArquivoDiario {
    dir: PathBuf,
    dia: i64,
    arquivo: Option<File>,
    avisou: bool,
}

impl ArquivoDiario {
    pub fn novo(dir: PathBuf) -> io::Result<Self> {
        let mut a = Self {
            dir,
            dia: i64::MIN,
            arquivo: None,
            avisou: false,
        };
        a.abrir(dia_atual())?;
        Ok(a)
    }

    fn abrir(&mut self, dia: i64) -> io::Result<()> {
        self.arquivo = None;
        if !diretorio::diretorio_comum(&self.dir)? {
            if fs::symlink_metadata(&self.dir).is_ok() {
                return Err(io::Error::other(format!(
                    "{} não é um diretório comum (junção/link?); log em arquivo recusado",
                    self.dir.display()
                )));
            }
            // O diretório de estado já foi conferido por `diretorio::preparar`
            // (processos privilegiados); sem privilégio, cria o que faltar.
            fs::create_dir_all(&self.dir)?;
        }
        let f = diretorio::abrir_sem_reparse(
            &self.dir.join(nome_do_dia(dia)),
            OpenOptions::new().append(true).create(true),
        )?;
        self.arquivo = Some(f);
        self.dia = dia;
        self.limpar_antigos();
        Ok(())
    }

    /// Apaga os mais antigos além de [`MANTER`] (só arquivos comuns).
    fn limpar_antigos(&self) {
        let Ok(it) = fs::read_dir(&self.dir) else {
            return;
        };
        let mut nomes: Vec<(String, PathBuf)> = it
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|e| {
                let n = e.file_name().into_string().ok()?;
                (n.starts_with(PREFIXO) && n.ends_with(SUFIXO)).then(|| (n, e.path()))
            })
            .collect();
        nomes.sort();
        let excesso = nomes.len().saturating_sub(MANTER);
        for (_, p) in nomes.into_iter().take(excesso) {
            let _ = fs::remove_file(p);
        }
    }
}

impl Write for ArquivoDiario {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let dia = dia_atual();
        if (dia != self.dia || self.arquivo.is_none())
            && let Err(e) = self.abrir(dia)
        {
            if !self.avisou {
                eprintln!("aviso: log em arquivo indisponível: {e}");
                self.avisou = true;
            }
            return Err(e);
        }
        match &mut self.arquivo {
            Some(f) => f.write(buf),
            None => Err(io::ErrorKind::NotFound.into()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.arquivo {
            Some(f) => f.flush(),
            None => Ok(()),
        }
    }
}

/// Inicializa os logs. Com `console`, também escreve em stderr. Sem
/// `arquivo` (diretório de estado inseguro), não há log em arquivo. O
/// guarda precisa viver até o fim do processo (descarrega o buffer ao sair).
pub fn iniciar(
    estado_dir: &Path,
    console: bool,
    arquivo: bool,
    detalhado: bool,
) -> Option<WorkerGuard> {
    let nivel = if detalhado {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    let escritor = if arquivo {
        match ArquivoDiario::novo(estado_dir.join("logs")) {
            Ok(a) => Some(a),
            Err(e) => {
                eprintln!("aviso: logs em arquivo indisponíveis: {e}");
                None
            }
        }
    } else {
        None
    };
    let (camada_arquivo, guarda) = match escritor {
        Some(a) => {
            let (w, g) = tracing_appender::non_blocking(a);
            (
                Some(
                    fmt::layer()
                        .with_writer(w)
                        .with_ansi(false)
                        .with_target(false)
                        .with_filter(nivel),
                ),
                Some(g),
            )
        }
        None => (None, None),
    };
    let camada_console = console.then(|| {
        fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
            .with_target(false)
            .with_filter(nivel)
    });
    let _ = tracing_subscriber::registry()
        .with(camada_arquivo)
        .with(camada_console)
        .try_init();
    guarda
}

/// Registra pânicos no log (além da mensagem padrão em stderr), para que um
/// pânico capturado no laço não passe despercebido no serviço.
pub fn registrar_panicos() {
    let anterior = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        tracing::error!(
            "pânico na thread {}: {info}",
            thread.name().unwrap_or("sem nome")
        );
        anterior(info);
    }));
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn datas_civis() {
        assert_eq!(data_civil(0), (1970, 1, 1));
        assert_eq!(data_civil(20_417), (2025, 11, 25));
        assert_eq!(data_civil(19_782), (2024, 2, 29));
        assert_eq!(nome_do_dia(20_417), "nvt-service.2025-11-25.log");
    }

    #[test]
    fn escreve_e_recusa_juncao_em_logs() {
        let temp = crate::teste_util::DirTemp::criado("logs");
        let base = temp.0.clone();
        // Diretório comum: escreve no arquivo do dia e mantém só 14.
        let logs = base.join("logs");
        fs::create_dir_all(&logs).unwrap();
        for d in 0..20 {
            fs::write(logs.join(nome_do_dia(19_000 + d)), "velho").unwrap();
        }
        let mut a = ArquivoDiario::novo(logs.clone()).unwrap();
        a.write_all(b"linha\n").unwrap();
        a.flush().unwrap();
        assert_eq!(
            fs::read_to_string(logs.join(nome_do_dia(dia_atual()))).unwrap(),
            "linha\n"
        );
        assert_eq!(fs::read_dir(&logs).unwrap().count(), MANTER);
        drop(a);
        // `logs` como junção: recusado, nada escrito no alvo.
        let alvo = base.join("alvo");
        fs::create_dir_all(&alvo).unwrap();
        let logs2 = base.join("logs2");
        let s = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&logs2)
            .arg(&alvo)
            .output()
            .unwrap();
        assert!(s.status.success());
        assert!(ArquivoDiario::novo(logs2).is_err());
        assert_eq!(fs::read_dir(&alvo).unwrap().count(), 0);
        let _ = fs::remove_dir_all(&base);
    }
}
