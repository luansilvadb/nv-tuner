//! Laço de eventos do serviço (modos `service` e `run`).
//!
//! Threads:
//!
//! - **principal** (quem chama [`executar`]): dono da NVAPI, do motor e do
//!   [`Nucleo`]; processa os [`Evento`]s em ordem e os temporizadores;
//! - **IPC**: servidor do named pipe; cada requisição vira
//!   [`Evento::Ipc`] e a thread espera a resposta da principal;
//! - **vigia**: `ReadDirectoryChangesW` no diretório do DRS →
//!   [`Evento::MudancaDrs`].
//!
//! A sessão DRS e a NVAPI ficam só na thread principal (não são `Send`).
//!
//! Robustez:
//!
//! - um pânico ao tratar uma requisição, num temporizador ou ao iniciar o
//!   núcleo é capturado e registrado; a requisição recebe `erro`/`interno`,
//!   o núcleo é descartado (o estado em memória pode ter ficado pela metade;
//!   a sessão DRS sem `SaveSettings` não grava nada) e recriado do disco;
//! - um pedido que esperou demais na fila recebe `ocupado` **e é cancelado**:
//!   a principal o descarta sem executar ([`Chamado`]);
//! - com o diretório de estado inseguro (`diretorio.rs`), o serviço não usa
//!   o driver nem o estado: responde `interno` com o motivo e tenta de novo
//!   a cada minuto.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use nvt_drs::NvApi;
use nvt_engine::{DriverStore, DrsReal, jogos_da_maquina};
use nvt_ipc::pipe::{ConfigServidor, Parada, Servidor};
use nvt_ipc::{CodigoErro, Requisicao, Resposta};
use tracing::{error, info, warn};

use crate::diretorio::{self, Situacao};
use crate::nucleo::{Config, Nucleo};
use crate::vigia;

/// Espera máxima de um pedido na fila antes de ser cancelado com `ocupado`.
const ESPERA_INICIO: Duration = Duration::from_secs(45);
/// Espera máxima total da thread do IPC (abaixo dos 60 s do cliente da UI).
const ESPERA_TOTAL: Duration = Duration::from_secs(57);
/// Granularidade dos temporizadores.
const PASSO: Duration = Duration::from_millis(500);
/// Intervalo entre tentativas de carregar a NVAPI.
const RETENTATIVA_DRIVER: Duration = Duration::from_secs(60);
/// Pausa antes de recriar o núcleo depois de um pânico.
const PAUSA_APOS_PANICO: Duration = Duration::from_secs(5);

const PENDENTE: u8 = 0;
const INICIADO: u8 = 1;
const CANCELADO: u8 = 2;

/// Requisição do IPC à espera da thread principal.
///
/// O estado (`pendente` → `iniciado` ou `cancelado`) é decidido por uma só
/// troca atômica: ou a principal começa a executar, ou a thread do IPC
/// cancela e responde `ocupado` — nunca os dois.
pub struct Chamado {
    req: Requisicao,
    estado: Arc<AtomicU8>,
    resposta: SyncSender<Resposta>,
}

impl Chamado {
    /// Executa `f` se o pedido não foi cancelado e envia a resposta.
    /// `false` se foi descartado.
    pub fn atender(self, f: impl FnOnce(Requisicao) -> Resposta) -> bool {
        if self
            .estado
            .compare_exchange(PENDENTE, INICIADO, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            info!("pedido IPC cancelado por tempo de espera; descartado sem executar");
            return false;
        }
        let r = f(self.req);
        let _ = self.resposta.send(r);
        true
    }
}

/// Evento para a thread principal.
pub enum Evento {
    Ipc(Chamado),
    MudancaDrs,
    Parar,
}

/// Opções do laço.
#[derive(Debug, Clone)]
pub struct Opcoes {
    pub pipe: String,
    pub estado_dir: PathBuf,
    /// Diretório do DRS a vigiar (`None` = não vigia).
    pub vigiar: Option<PathBuf>,
    pub config: Config,
}

/// Envia `req` à thread principal e espera a resposta (thread do IPC).
///
/// Se a principal não começar em `espera_inicio`, cancela o pedido e
/// responde `ocupado`; se já começou, espera até `espera_total`.
pub fn encaminhar(
    tx: &Sender<Evento>,
    req: Requisicao,
    espera_inicio: Duration,
    espera_total: Duration,
) -> Resposta {
    let (rtx, rrx) = mpsc::sync_channel(1);
    let estado = Arc::new(AtomicU8::new(PENDENTE));
    let chamado = Chamado {
        req,
        estado: estado.clone(),
        resposta: rtx,
    };
    if tx.send(Evento::Ipc(chamado)).is_err() {
        return Resposta::erro(CodigoErro::Interno, "serviço encerrando");
    }
    let sem_resposta = || Resposta::erro(CodigoErro::Interno, "falha interna do serviço");
    match rrx.recv_timeout(espera_inicio) {
        Ok(r) => return r,
        Err(RecvTimeoutError::Disconnected) => return sem_resposta(),
        Err(RecvTimeoutError::Timeout) => {}
    }
    if estado
        .compare_exchange(PENDENTE, CANCELADO, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        return Resposta::erro(
            CodigoErro::Ocupado,
            "o serviço está ocupado; o pedido foi cancelado e não será executado. Tente de novo",
        );
    }
    match rrx.recv_timeout(espera_total.saturating_sub(espera_inicio)) {
        Ok(r) => r,
        Err(RecvTimeoutError::Disconnected) => sem_resposta(),
        Err(RecvTimeoutError::Timeout) => Resposta::erro(
            CodigoErro::Interno,
            "a operação começou e ainda não terminou; confira o estado em instantes",
        ),
    }
}

/// Sobe o servidor IPC numa thread. Falha se o pipe não puder ser criado
/// (por exemplo, já existe um com o mesmo nome).
pub fn iniciar_ipc(nome: &str, tx: Sender<Evento>) -> std::io::Result<(Parada, JoinHandle<()>)> {
    let parada = Parada::nova()?;
    let mut servidor = Servidor::criar(nome, ConfigServidor::default(), parada.clone())
        .map_err(|e| std::io::Error::new(e.kind(), format!("pipe {nome}: {e}")))?;
    let nome = nome.to_owned();
    let h = thread::Builder::new()
        .name("nvt-ipc".into())
        .spawn(move || {
            info!("IPC ouvindo em {nome}");
            let mut tratador = |req: Requisicao| encaminhar(&tx, req, ESPERA_INICIO, ESPERA_TOTAL);
            if let Err(e) = servidor.executar(&mut tratador) {
                error!("servidor IPC parou: {e}");
            }
        })?;
    Ok((parada, h))
}

/// Sobe a vigia do DRS numa thread.
pub fn iniciar_vigia(dir: PathBuf, tx: Sender<Evento>, parar: Arc<AtomicBool>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("nvt-vigia".into())
        .spawn(move || {
            vigia::vigiar(&dir, &parar, || {
                let _ = tx.send(Evento::MudancaDrs);
            });
        })
        .expect("thread da vigia")
}

/// Como terminou o [`laco_principal`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fim {
    /// [`Evento::Parar`] ou canal fechado.
    Parar,
    /// Um pânico foi capturado; o núcleo deve ser descartado e recriado.
    Panico,
}

/// Roda `f` capturando pânicos (já registrados no log pelo gancho de
/// `logs::registrar_panicos`).
fn protegido<R>(onde: &str, f: impl FnOnce() -> R) -> Option<R> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => Some(r),
        Err(p) => {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "sem mensagem".into());
            error!("pânico capturado em {onde}: {msg}");
            None
        }
    }
}

/// O que o [`laco_principal`] aciona (o [`Nucleo`]; nos testes, um falso).
pub trait Atendente {
    fn tratar(&mut self, req: Requisicao, agora: Instant) -> Resposta;
    fn notificar_mudanca_drs(&mut self, agora: Instant);
    fn tick(&mut self, agora: Instant);
}

impl<S: DriverStore> Atendente for Nucleo<'_, S> {
    fn tratar(&mut self, req: Requisicao, agora: Instant) -> Resposta {
        Nucleo::tratar(self, req, agora)
    }

    fn notificar_mudanca_drs(&mut self, agora: Instant) {
        Nucleo::notificar_mudanca_drs(self, agora);
    }

    fn tick(&mut self, agora: Instant) {
        Nucleo::tick(self, agora);
    }
}

/// Processa eventos e temporizadores até [`Evento::Parar`] (ou o canal
/// fechar), ou até um pânico ser capturado.
pub fn laco_principal(nucleo: &mut impl Atendente, rx: &Receiver<Evento>) -> Fim {
    loop {
        match rx.recv_timeout(PASSO) {
            Ok(Evento::Ipc(chamado)) => {
                let mut panico = false;
                chamado.atender(|req| {
                    protegido("requisição IPC", || nucleo.tratar(req, Instant::now()))
                        .unwrap_or_else(|| {
                            panico = true;
                            Resposta::erro(CodigoErro::Interno, "falha interna do serviço")
                        })
                });
                if panico {
                    return Fim::Panico;
                }
            }
            Ok(Evento::MudancaDrs) => {
                if protegido("notificação do DRS", || {
                    nucleo.notificar_mudanca_drs(Instant::now())
                })
                .is_none()
                {
                    return Fim::Panico;
                }
            }
            Ok(Evento::Parar) | Err(RecvTimeoutError::Disconnected) => return Fim::Parar,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if protegido("ciclo do serviço", || nucleo.tick(Instant::now())).is_none() {
            return Fim::Panico;
        }
    }
}

/// Espera `d` atendendo o IPC com o erro `codigo`/`motivo`. `false` se
/// pediram para parar.
fn esperar_com_erro(rx: &Receiver<Evento>, d: Duration, codigo: CodigoErro, motivo: &str) -> bool {
    let fim = Instant::now() + d;
    while Instant::now() < fim {
        match rx.recv_timeout(PASSO) {
            Ok(Evento::Ipc(chamado)) => {
                chamado.atender(|_| Resposta::erro(codigo, motivo));
            }
            Ok(Evento::MudancaDrs) | Err(RecvTimeoutError::Timeout) => {}
            Ok(Evento::Parar) | Err(RecvTimeoutError::Disconnected) => return false,
        }
    }
    true
}

fn esperar_sem_driver(rx: &Receiver<Evento>, d: Duration, motivo: &str) -> bool {
    esperar_com_erro(
        rx,
        d,
        CodigoErro::DriverIndisponivel,
        &format!("driver NVIDIA indisponível: {motivo}"),
    )
}

/// Executa o serviço contra o driver real até [`Evento::Parar`].
/// `ao_iniciar` é chamado assim que o IPC está no ar (o SCM recebe `Running`).
pub fn executar(
    op: &Opcoes,
    tx: Sender<Evento>,
    rx: Receiver<Evento>,
    ao_iniciar: impl FnOnce(),
) -> std::io::Result<()> {
    let (parada, h_ipc) = iniciar_ipc(&op.pipe, tx.clone())?;
    let parar_vigia = Arc::new(AtomicBool::new(false));
    let h_vigia = op
        .vigiar
        .clone()
        .map(|d| iniciar_vigia(d, tx.clone(), parar_vigia.clone()));
    drop(tx);
    ao_iniciar();
    info!(
        "serviço iniciado (estado em {}, somente leitura: {})",
        op.estado_dir.display(),
        op.config.somente_leitura
    );

    if esperar_diretorio_seguro(op, &rx) {
        executar_com_driver(op, &rx);
    }

    parada.parar();
    parar_vigia.store(true, Ordering::Relaxed);
    let _ = h_ipc.join();
    if let Some(h) = h_vigia {
        let _ = h.join();
    }
    info!("serviço encerrado");
    Ok(())
}

/// Registra o resultado de [`diretorio::preparar`]. `Err` = recusar operar.
pub fn relatar_diretorio(dir: &std::path::Path, r: &Result<Situacao, String>) {
    match r {
        Ok(Situacao::Seguro | Situacao::Ausente) => {}
        Ok(Situacao::Criado) => info!("diretório de estado criado: {}", dir.display()),
        Ok(Situacao::PosseTomada { itens }) => warn!(
            "diretório de estado: Administradores tomou posse de {itens} item(ns) com dono errado"
        ),
        Ok(Situacao::Corrigido {
            quarentena,
            problemas,
        }) => warn!(
            "diretório de estado inseguro ({}); movido para {} e recriado",
            diretorio::resumo(problemas),
            quarentena.display()
        ),
        Ok(Situacao::Inseguro(p)) => warn!(
            "diretório de estado inseguro ({}); processo sem privilégio, seguindo assim mesmo",
            diretorio::resumo(p)
        ),
        Err(e) => error!("{e}; o serviço não vai operar até isso ser corrigido"),
    }
}

/// Garante o diretório de estado antes de usar driver e estado. Enquanto
/// estiver inseguro, responde `interno` com o motivo e tenta de novo a cada
/// minuto. `false` se pediram para parar.
fn esperar_diretorio_seguro(op: &Opcoes, rx: &Receiver<Evento>) -> bool {
    if op.config.somente_leitura {
        return true;
    }
    loop {
        let r = diretorio::preparar(&op.estado_dir, diretorio::modo_do_processo());
        relatar_diretorio(&op.estado_dir, &r);
        let Err(e) = r else { return true };
        let motivo = format!(
            "diretório de estado inseguro, o serviço não está operando: {e}. \
             Reinstale o nv-tuner ou corrija as permissões de {}",
            op.estado_dir.display()
        );
        if !esperar_com_erro(rx, RETENTATIVA_DRIVER, CodigoErro::Interno, &motivo) {
            return false;
        }
    }
}

fn executar_com_driver(op: &Opcoes, rx: &Receiver<Evento>) {
    let api = loop {
        match NvApi::load() {
            Ok(a) => break a,
            Err(e) => {
                warn!("NVAPI indisponível: {e}; nova tentativa em 60 s");
                if !esperar_sem_driver(rx, RETENTATIVA_DRIVER, &e.to_string()) {
                    return;
                }
            }
        }
    };
    loop {
        let mut nucleo = loop {
            let drs_dir = op.vigiar.clone();
            let r = protegido("início do motor", || {
                Nucleo::novo(
                    Box::new(|| Ok(DrsReal::abrir(&api)?)),
                    Box::new(|| {
                        let (jogos, avisos) = jogos_da_maquina();
                        for a in avisos {
                            warn!("varredura: {a}");
                        }
                        jogos
                    }),
                    Box::new(move || drs_dir.as_deref().and_then(vigia::assinatura)),
                    Some(op.estado_dir.clone()),
                    op.config,
                    Instant::now(),
                )
            });
            match r {
                Some(Ok(n)) => break n,
                Some(Err(e)) => {
                    error!("falha ao iniciar o motor: {e}; nova tentativa em 60 s");
                    if !esperar_sem_driver(rx, RETENTATIVA_DRIVER, &e.to_string()) {
                        return;
                    }
                }
                None => {
                    if !esperar_com_erro(
                        rx,
                        PAUSA_APOS_PANICO,
                        CodigoErro::Interno,
                        "falha interna do serviço; reiniciando o motor",
                    ) {
                        return;
                    }
                }
            }
        };
        let fim = match protegido("início do ciclo", || nucleo.iniciar(Instant::now())) {
            Some(()) => laco_principal(&mut nucleo, rx),
            None => Fim::Panico,
        };
        drop(nucleo);
        match fim {
            Fim::Parar => return,
            Fim::Panico => {
                error!("núcleo descartado após pânico; recriando a partir do disco");
                if !esperar_com_erro(
                    rx,
                    PAUSA_APOS_PANICO,
                    CodigoErro::Interno,
                    "falha interna do serviço; reiniciando o motor",
                ) {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ocupado_cancela_o_pedido() {
        let (tx, rx) = mpsc::channel();
        let r = encaminhar(
            &tx,
            Requisicao::RestaurarOriginal,
            Duration::from_millis(50),
            Duration::from_millis(100),
        );
        assert!(matches!(
            r,
            Resposta::Erro {
                codigo: CodigoErro::Ocupado,
                ..
            }
        ));
        // A principal, ao chegar no pedido, não o executa.
        let Ok(Evento::Ipc(ch)) = rx.try_recv() else {
            panic!("esperava o pedido na fila");
        };
        let mut executou = false;
        assert!(!ch.atender(|_| {
            executou = true;
            Resposta::Ok
        }));
        assert!(!executou, "pedido cancelado não pode ser executado");
    }

    #[test]
    fn pedido_ja_iniciado_nao_vira_ocupado() {
        let (tx, rx) = mpsc::channel::<Evento>();
        let principal = thread::spawn(move || {
            let Ok(Evento::Ipc(ch)) = rx.recv() else {
                panic!("esperava pedido");
            };
            // Começa logo, mas demora mais que a espera de início.
            ch.atender(|_| {
                thread::sleep(Duration::from_millis(150));
                Resposta::Ok
            })
        });
        let r = encaminhar(
            &tx,
            Requisicao::AplicarAgora,
            Duration::from_millis(50),
            Duration::from_secs(5),
        );
        assert_eq!(r, Resposta::Ok);
        assert!(principal.join().unwrap());
    }

    #[test]
    fn pedido_atendido_a_tempo() {
        let (tx, rx) = mpsc::channel::<Evento>();
        let principal = thread::spawn(move || {
            let Ok(Evento::Ipc(ch)) = rx.recv() else {
                panic!("esperava pedido");
            };
            ch.atender(|_| Resposta::Ok)
        });
        let r = encaminhar(
            &tx,
            Requisicao::ObterPlano,
            Duration::from_secs(5),
            Duration::from_secs(5),
        );
        assert_eq!(r, Resposta::Ok);
        assert!(principal.join().unwrap());
    }

    #[test]
    fn principal_encerrada_responde_interno() {
        let (tx, rx) = mpsc::channel::<Evento>();
        drop(rx);
        assert!(matches!(
            encaminhar(
                &tx,
                Requisicao::ObterEstado,
                Duration::from_millis(10),
                Duration::from_millis(10)
            ),
            Resposta::Erro {
                codigo: CodigoErro::Interno,
                ..
            }
        ));
    }

    /// Atendente que entra em pânico em `ReescanearJogos` ou no tick pedido.
    struct Instavel {
        tratadas: u32,
        panico_no_tick: bool,
    }

    impl Atendente for Instavel {
        fn tratar(&mut self, req: Requisicao, _agora: Instant) -> Resposta {
            self.tratadas += 1;
            match req {
                Requisicao::ReescanearJogos => panic!("pânico proposital do tratador"),
                _ => Resposta::Ok,
            }
        }

        fn notificar_mudanca_drs(&mut self, _agora: Instant) {}

        fn tick(&mut self, _agora: Instant) {
            if self.panico_no_tick {
                panic!("pânico proposital no ciclo");
            }
        }
    }

    #[test]
    fn panico_no_tratador_responde_interno_e_encerra_o_nucleo() {
        let (tx, rx) = mpsc::channel::<Evento>();
        let principal = thread::spawn(move || {
            let mut a = Instavel {
                tratadas: 0,
                panico_no_tick: false,
            };
            let fim = laco_principal(&mut a, &rx);
            (fim, a.tratadas, rx)
        });
        let espera = Duration::from_secs(5);
        assert_eq!(
            encaminhar(&tx, Requisicao::ObterPlano, espera, espera),
            Resposta::Ok
        );
        assert!(matches!(
            encaminhar(&tx, Requisicao::ReescanearJogos, espera, espera),
            Resposta::Erro {
                codigo: CodigoErro::Interno,
                ..
            }
        ));
        // A thread principal não morreu: o laço devolveu `Panico` para o
        // núcleo ser recriado.
        let (fim, tratadas, rx) = principal.join().expect("thread principal viva");
        assert_eq!(fim, Fim::Panico);
        assert_eq!(tratadas, 2);
        // Um núcleo novo continua atendendo pelo mesmo canal.
        let principal = thread::spawn(move || {
            let mut a = Instavel {
                tratadas: 0,
                panico_no_tick: false,
            };
            laco_principal(&mut a, &rx)
        });
        assert_eq!(
            encaminhar(&tx, Requisicao::ObterEstado, espera, espera),
            Resposta::Ok
        );
        tx.send(Evento::Parar).unwrap();
        assert_eq!(principal.join().unwrap(), Fim::Parar);
    }

    #[test]
    fn panico_no_ciclo_e_capturado() {
        let (_tx, rx) = mpsc::channel::<Evento>();
        let mut a = Instavel {
            tratadas: 0,
            panico_no_tick: true,
        };
        assert_eq!(laco_principal(&mut a, &rx), Fim::Panico);
    }

    #[test]
    fn protegido_captura_panico() {
        assert_eq!(protegido("teste", || 7), Some(7));
        assert_eq!(protegido("teste", || -> i32 { panic!("proposital") }), None);
    }
}
