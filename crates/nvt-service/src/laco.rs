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
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use nvt_drs::NvApi;
use nvt_engine::{DriverStore, DrsReal, jogos_da_maquina};
use nvt_ipc::pipe::{ConfigServidor, IdentidadeCliente, Parada, Servidor};
use nvt_ipc::{CodigoErro, Requisicao, Resposta};
use tracing::{error, info, warn};

use crate::diretorio::{self, Situacao};
use crate::nucleo::{Config, EstadoServico, Nucleo, agora_unix, sugestao_drs};
use crate::usuario::{ArquivoUsuarios, Coordenador};
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
    identidade: IdentidadeCliente,
    req: Requisicao,
    estado: Arc<AtomicU8>,
    resposta: SyncSender<Resposta>,
}

impl Chamado {
    /// Executa `f` se o pedido não foi cancelado e envia a resposta.
    /// `false` se foi descartado.
    pub fn atender(self, f: impl FnOnce(IdentidadeCliente, Requisicao) -> Resposta) -> bool {
        if self
            .estado
            .compare_exchange(PENDENTE, INICIADO, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            info!("pedido IPC cancelado por tempo de espera; descartado sem executar");
            return false;
        }
        let r = f(self.identidade, self.req);
        let _ = self.resposta.send(r);
        true
    }
}

/// Evento para a thread principal.
pub enum Evento {
    Ipc(Box<Chamado>),
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
    identidade: IdentidadeCliente,
    req: Requisicao,
    espera_inicio: Duration,
    espera_total: Duration,
) -> Resposta {
    let (rtx, rrx) = mpsc::sync_channel(1);
    let estado = Arc::new(AtomicU8::new(PENDENTE));
    let chamado = Chamado {
        identidade,
        req,
        estado: estado.clone(),
        resposta: rtx,
    };
    if tx.send(Evento::Ipc(Box::new(chamado))).is_err() {
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
            let mut tratador =
                |identidade, req| encaminhar(&tx, identidade, req, ESPERA_INICIO, ESPERA_TOTAL);
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
    fn tratar(
        &mut self,
        _identidade: IdentidadeCliente,
        req: Requisicao,
        agora: Instant,
    ) -> Resposta;
    fn notificar_mudanca_drs(&mut self, agora: Instant);
    fn tick(&mut self, agora: Instant);
}

pub(crate) struct Atendimento<'a, 'b, S: DriverStore> {
    pub nucleo: &'a mut Nucleo<'b, S>,
    pub integracao: &'a mut Integracao,
}

impl<S: DriverStore> Atendente for Atendimento<'_, '_, S> {
    fn tratar(
        &mut self,
        identidade: IdentidadeCliente,
        req: Requisicao,
        agora: Instant,
    ) -> Resposta {
        if let Some(usuario) = &mut self.integracao.usuario
            && matches!(req, Requisicao::AgenteConsultar { .. })
        {
            usuario.atualizar_processos(self.nucleo.processos_vigiados());
        }
        let estado = self.nucleo.servico().clone();
        self.integracao
            .tratar(identidade, req, agora, &estado, |req| {
                let plano = matches!(req, Requisicao::ObterPlano);
                let mut resposta = self.nucleo.tratar(req, agora);
                if plano
                    && matches!(
                        resposta,
                        Resposta::Erro {
                            codigo: CodigoErro::DriverIndisponivel,
                            ..
                        }
                    )
                {
                    resposta = plano_sem_driver();
                }
                (resposta, self.nucleo.servico().clone())
            })
    }

    fn notificar_mudanca_drs(&mut self, agora: Instant) {
        self.nucleo.notificar_mudanca_drs(agora);
    }

    fn tick(&mut self, agora: Instant) {
        if self.integracao.atualizar_console(false) {
            self.nucleo
                .definir_aceites_drs(self.integracao.aceites_drs.clone());
            self.nucleo
                .ciclo(agora, "sessão de console ou aceite experimental");
        }
        self.nucleo.tick(agora);
    }
}

/// Processa eventos e temporizadores até [`Evento::Parar`] (ou o canal
/// fechar), ou até um pânico ser capturado.
pub fn laco_principal(nucleo: &mut impl Atendente, rx: &Receiver<Evento>) -> Fim {
    loop {
        match rx.recv_timeout(PASSO) {
            Ok(Evento::Ipc(chamado)) => {
                let mut panico = false;
                chamado.atender(|identidade, req| {
                    protegido("requisição IPC", || {
                        nucleo.tratar(identidade, req, Instant::now())
                    })
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
                chamado.atender(|_, _| Resposta::erro(codigo, motivo));
            }
            Ok(Evento::MudancaDrs) | Err(RecvTimeoutError::Timeout) => {}
            Ok(Evento::Parar) | Err(RecvTimeoutError::Disconnected) => return false,
        }
    }
    true
}

fn esperar_sem_driver(
    integracao: &mut Integracao,
    op: &Opcoes,
    rx: &Receiver<Evento>,
    d: Duration,
    motivo: &str,
) -> bool {
    let fim = Instant::now() + d;
    let mut estado = EstadoServico::carregar(Some(&op.estado_dir));
    while Instant::now() < fim {
        match rx.recv_timeout(PASSO) {
            Ok(Evento::Ipc(chamado)) => {
                chamado.atender(|identidade, req| {
                    protegido("requisição sem driver", || {
                        let anterior = estado.clone();
                        integracao.tratar(identidade, req, Instant::now(), &anterior, |req| {
                            let resposta = if op.config.somente_leitura
                                && matches!(
                                    req,
                                    Requisicao::AplicarAgora | Requisicao::RestaurarOriginal
                                ) {
                                Resposta::erro(
                                    CodigoErro::Interno,
                                    "serviço em modo somente leitura",
                                )
                            } else {
                                tratar_sem_driver(req, &mut estado, &op.estado_dir, motivo)
                            };
                            (resposta, estado.clone())
                        })
                    })
                    .unwrap_or_else(|| {
                        Resposta::erro(CodigoErro::Interno, "falha interna do serviço")
                    })
                });
            }
            Ok(Evento::MudancaDrs) | Err(RecvTimeoutError::Timeout) => {}
            Ok(Evento::Parar) | Err(RecvTimeoutError::Disconnected) => return false,
        }
    }
    true
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
        let mut integracao = Integracao::abrir(op);
        let diagnosticos = iniciar_diagnosticos(integracao.diagnosticos.clone());
        executar_com_driver(op, &rx, &mut integracao);
        drop(diagnosticos);
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

fn executar_com_driver(op: &Opcoes, rx: &Receiver<Evento>, integracao: &mut Integracao) {
    let api = loop {
        match NvApi::load() {
            Ok(a) => break a,
            Err(e) => {
                warn!("NVAPI indisponível: {e}; nova tentativa em 60 s");
                if !esperar_sem_driver(integracao, op, rx, RETENTATIVA_DRIVER, &e.to_string()) {
                    return;
                }
            }
        }
    };
    loop {
        let mut nucleo = loop {
            let drs_dir = op.vigiar.clone();
            let r = protegido("início do motor", || {
                let nucleo = Nucleo::novo(
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
                )?;
                nucleo.engine().capacidades()?;
                Ok::<_, nvt_engine::ErroEngine>(nucleo)
            });
            match r {
                Some(Ok(n)) => break n,
                Some(Err(e)) => {
                    error!("falha ao iniciar o motor: {e}; nova tentativa em 60 s");
                    if !esperar_sem_driver(integracao, op, rx, RETENTATIVA_DRIVER, &e.to_string()) {
                        return;
                    }
                }
                None => {
                    if !esperar_sem_driver(
                        integracao,
                        op,
                        rx,
                        PAUSA_APOS_PANICO,
                        "falha interna do serviço; reiniciando o motor",
                    ) {
                        return;
                    }
                }
            }
        };
        integracao.atualizar_console(true);
        nucleo.definir_aceites_drs(integracao.aceites_drs.clone());
        let fim = match protegido("início do ciclo", || nucleo.iniciar(Instant::now())) {
            Some(()) => laco_principal(
                &mut Atendimento {
                    nucleo: &mut nucleo,
                    integracao,
                },
                rx,
            ),
            None => Fim::Panico,
        };
        drop(nucleo);
        match fim {
            Fim::Parar => return,
            Fim::Panico => {
                error!("núcleo descartado após pânico; recriando a partir do disco");
                if !esperar_sem_driver(
                    integracao,
                    op,
                    rx,
                    PAUSA_APOS_PANICO,
                    "falha interna do serviço; reiniciando o motor",
                ) {
                    return;
                }
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct Integracao {
    pub(crate) usuario: Option<Coordenador>,
    diagnosticos: Arc<Mutex<Vec<nvt_ipc::Diagnostico>>>,
    regras_drs: Option<nvt_core::ConjuntoRegras>,
    console: Option<IdentidadeCliente>,
    aceites_drs: std::collections::BTreeSet<String>,
}

impl Integracao {
    fn abrir(op: &Opcoes) -> Self {
        let mut integracao = Self::default();
        if op.config.somente_leitura || !diretorio::verificar(&op.estado_dir).is_empty() {
            return integracao;
        }
        let estado = EstadoServico::carregar(Some(&op.estado_dir));
        let abrir = || {
            let drs = nvt_core::ConjuntoRegras::embutido()?;
            let base = nvt_core::BaseJogos::embutida()?;
            let mut usuario = Coordenador::abrir(
                ArquivoUsuarios(op.estado_dir.clone()),
                &drs,
                build_windows(),
                estado.geracao_restauracao,
            )?;
            usuario.atualizar_restauracao(estado.geracao_restauracao, estado.autonomia_pausada);
            usuario.atualizar_processos(
                nvt_engine::Portao::do_sistema()
                    .vigiados(&base, &[])
                    .into_iter()
                    .collect(),
            );
            Ok::<_, Box<dyn std::error::Error>>((drs, usuario))
        };
        match abrir() {
            Ok((drs, usuario)) => {
                integracao.regras_drs = Some(drs);
                integracao.usuario = Some(usuario);
            }
            Err(e) => error!("coordenador de usuário indisponível: {e}"),
        }
        integracao
    }

    fn atualizar_console(&mut self, forcar: bool) -> bool {
        let Some(usuario) = &self.usuario else {
            return false;
        };
        let console = identidade_console();
        let aceites = usuario.aceites_drs(console.as_ref().map(|c| c.sid.as_str()));
        let mudou = self.console != console || self.aceites_drs != aceites;
        self.console = console;
        self.aceites_drs = aceites;
        forcar || mudou
    }

    fn tratar(
        &mut self,
        identidade: IdentidadeCliente,
        req: Requisicao,
        agora: Instant,
        estado: &EstadoServico,
        driver: impl FnOnce(Requisicao) -> (Resposta, EstadoServico),
    ) -> Resposta {
        let limite = match req {
            Requisicao::ListarHistorico { limite } => limite as usize,
            _ => 0,
        };
        if let Some(usuario) = &mut self.usuario {
            usuario.atualizar_restauracao(estado.geracao_restauracao, estado.autonomia_pausada);
            if let Some(resposta) = usuario.tratar(&identidade, &req, agora, agora_unix()) {
                return resposta;
            }
        }
        let (mut resposta, estado) = driver(req);
        if let Resposta::Estado(estado) = &mut resposta {
            estado.diagnosticos = self
                .diagnosticos
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
        }
        if let Some(usuario) = &mut self.usuario {
            usuario.atualizar_restauracao(estado.geracao_restauracao, estado.autonomia_pausada);
            usuario.projetar(&identidade, &mut resposta, limite, agora);
            if let Resposta::Plano(plano) = &mut resposta
                && let Some(regras) = &self.regras_drs
            {
                let aceites = usuario.aceites_drs(Some(&identidade.sid));
                let console = self.console.as_ref() == Some(&identidade);
                plano.sugestoes_driver.retain(|s| {
                    s.estado == nvt_ipc::EstadoExperimental::Aceitavel || aceites.contains(&s.regra)
                });
                for regra in regras
                    .regras
                    .iter()
                    .filter(|r| r.status == nvt_core::Status::Experimental)
                {
                    let aceita = aceites.contains(&regra.id);
                    let existente = plano
                        .sugestoes_driver
                        .iter_mut()
                        .find(|s| s.regra == regra.id);
                    let sugestao = match existente {
                        Some(sugestao) => sugestao,
                        None if aceita => {
                            plano.sugestoes_driver.push(sugestao_drs(regra));
                            plano
                                .sugestoes_driver
                                .last_mut()
                                .expect("sugestão inserida")
                        }
                        None => continue,
                    };
                    if aceita {
                        sugestao.estado = nvt_ipc::EstadoExperimental::Aceita;
                        if !console || !self.aceites_drs.contains(&regra.id) {
                            sugestao
                                .descricao
                                .push_str(" (aceite inativo nesta sessão)");
                        }
                    }
                    sugestao
                        .descricao
                        .push_str(" DRS compartilhado entre usuários.");
                }
            }
        }
        resposta
    }
}

fn iniciar_diagnosticos(cache: Arc<Mutex<Vec<nvt_ipc::Diagnostico>>>) -> Option<Sender<()>> {
    use nvt_windows::diagnosticos::{DiagnosticosWindows, diagnosticos_maquina};

    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("nvt-diagnosticos".into())
        .spawn(move || {
            loop {
                let itens = diagnosticos_maquina(&DiagnosticosWindows);
                *cache
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = itens;
                if rx.recv_timeout(Duration::from_secs(600)) != Err(RecvTimeoutError::Timeout) {
                    break;
                }
            }
        })
        .map_err(|e| error!("diagnósticos de máquina indisponíveis: {e}"))
        .ok()?;
    Some(tx)
}

pub(crate) fn identidade_console() -> Option<IdentidadeCliente> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_USER, TokenUser};
    use windows_sys::Win32::System::RemoteDesktop::{
        WTSGetActiveConsoleSessionId, WTSQueryUserToken,
    };

    // SAFETY: as APIs consultam a sessão do Windows e recebem saídas graváveis.
    let sessao = unsafe { WTSGetActiveConsoleSessionId() };
    if sessao == u32::MAX || sessao == 0 {
        return None;
    }
    let mut token = std::ptr::null_mut();
    if unsafe { WTSQueryUserToken(sessao, &mut token) } == 0 {
        return None;
    }
    let mut buffer = [0usize; 32];
    let mut tamanho = 0;
    // SAFETY: buffer alinhado e suficiente para TOKEN_USER e o SID anexado.
    let lido = unsafe {
        let lido = GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size_of_val(&buffer) as u32,
            &mut tamanho,
        );
        CloseHandle(token);
        lido
    };
    if lido == 0 || unsafe { WTSGetActiveConsoleSessionId() } != sessao {
        return None;
    }
    // SAFETY: TokenUser preencheu o buffer e o ponteiro referencia o SID nele.
    let sid =
        crate::diretorio::sid_texto(unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid });
    sid.starts_with("S-")
        .then_some(IdentidadeCliente { sid, sessao })
}

pub(crate) fn tratar_sem_driver(
    req: Requisicao,
    estado: &mut EstadoServico,
    dir: &std::path::Path,
    motivo: &str,
) -> Resposta {
    let sem_driver = || {
        Resposta::erro(
            CodigoErro::DriverIndisponivel,
            format!("driver NVIDIA indisponível: {motivo}"),
        )
    };
    let falha = |e: std::io::Error| Resposta::erro(CodigoErro::Interno, e.to_string());
    match req {
        Requisicao::ObterEstado => Resposta::Estado(nvt_ipc::Estado {
            agente: Default::default(),
            diagnosticos: Vec::new(),
            versao_servico: env!("CARGO_PKG_VERSION").into(),
            driver: None,
            gpu: None,
            preferencias: crate::nucleo::prefs_ipc(&nvt_core::Preferences::default()),
            jogos: 0,
            ultima_aplicacao: estado.ultima_aplicacao.map(|u| nvt_ipc::Aplicacao {
                quando_unix: u.quando_unix,
                mudancas: u.mudancas,
                falhas: u.falhas,
            }),
            alertas_deriva: Vec::new(),
            avisos: Vec::new(),
            adiamento: None,
            atualizacao: Default::default(),
        }),
        Requisicao::ObterPlano => plano_sem_driver(),
        Requisicao::ListarHistorico { .. } => Resposta::Historico {
            entradas: Vec::new(),
        },
        Requisicao::RestaurarOriginal => estado.pedir_restauracao(Some(dir)).map_or_else(falha, |()| {
            Resposta::erro(CodigoErro::DriverIndisponivel, format!(
                "Restauração solicitada. O driver fica pendente enquanto a NVIDIA estiver indisponível: {motivo}"
            ))
        }),
        Requisicao::AplicarAgora => estado.retomar_autonomia(Some(dir)).map_or_else(falha, |()| {
            Resposta::erro(CodigoErro::DriverIndisponivel, format!(
                "Ajustes de usuário reativados. Driver NVIDIA indisponível: {motivo}"
            ))
        }),
        _ => sem_driver(),
    }
}

fn plano_sem_driver() -> Resposta {
    Resposta::Plano(nvt_ipc::Plano {
        sistema: Default::default(),
        sugestoes_driver: Vec::new(),
        gerado_unix: agora_unix(),
        perfis: Vec::new(),
        reversoes: Vec::new(),
        ignorados: Vec::new(),
        jogos: Vec::new(),
        adiada: None,
    })
}

fn build_windows() -> Option<u32> {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;

    let mut versao = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: estrutura gravável com tamanho indicado no primeiro campo.
    (unsafe { RtlGetVersion(&mut versao) } == 0).then_some(versao.dwBuildNumber)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn pipe_sem_driver_vincula_agente_e_ui_e_retorna_diagnosticos() {
        use nvt_ipc::pipe::{Cliente, ConfigCliente};

        #[derive(Default)]
        struct Memoria(std::cell::RefCell<crate::usuario::DadosUsuarios>);

        impl crate::usuario::Armazenamento for Memoria {
            fn carregar(&self) -> std::io::Result<crate::usuario::DadosUsuarios> {
                Ok(self.0.borrow().clone())
            }

            fn salvar(&self, dados: &crate::usuario::DadosUsuarios) -> std::io::Result<()> {
                self.0.replace(dados.clone());
                Ok(())
            }
        }

        let pipe = format!(r"\\.\pipe\nvt-servico-sem-driver-{}", std::process::id());
        let (tx, rx) = mpsc::channel();
        let (parada, ipc) = iniciar_ipc(&pipe, tx.clone()).unwrap();
        let op = Opcoes {
            pipe: pipe.clone(),
            estado_dir: std::env::temp_dir().join(format!("nvt-sem-driver-{}", std::process::id())),
            vigiar: None,
            config: Config::default(),
        };
        let motor = thread::spawn(move || {
            let regras = nvt_core::ConjuntoRegras::embutido().unwrap();
            let mut integracao = Integracao {
                usuario: Some(
                    Coordenador::abrir(Memoria::default(), &regras, Some(22631), 0).unwrap(),
                ),
                regras_drs: Some(regras),
                ..Integracao::default()
            };
            integracao
                .diagnosticos
                .lock()
                .unwrap()
                .push(nvt_ipc::Diagnostico {
                    codigo: "hags".into(),
                    estado: nvt_ipc::EstadoDiagnostico::Indisponivel,
                });
            esperar_sem_driver(&mut integracao, &op, &rx, Duration::from_secs(15), "teste")
        });
        let cliente = |req: &Requisicao| {
            Cliente::conectar(
                &pipe,
                &ConfigCliente {
                    exigir_dono_privilegiado: false,
                    timeout_conexao: Duration::from_secs(2),
                    timeout_leitura: Duration::from_secs(2),
                    ..ConfigCliente::default()
                },
            )
            .unwrap()
            .requisitar(req)
            .unwrap()
        };
        let consulta = Requisicao::AgenteConsultar {
            fatos: nvt_ipc::FatosUsuario {
                tela: nvt_ipc::Fato::Desconhecido,
                mouse: nvt_ipc::Fato::Desconhecido,
                swap_effect_upgrade: nvt_ipc::Fato::Desconhecido,
                captura_historica: nvt_ipc::Fato::Conhecido(1),
                diagnosticos: vec![nvt_ipc::Diagnostico {
                    codigo: "drr".into(),
                    estado: nvt_ipc::EstadoDiagnostico::Indisponivel,
                }],
                gestao: vec![],
            },
            ultima_tarefa: None,
            geracao_restaurada: 0,
        };
        assert_eq!(
            cliente(&Requisicao::DefinirAceiteExperimental {
                regra: "windows-captura-historica-off".into(),
                aceito: true,
            }),
            Resposta::Ok
        );
        let Resposta::PlanoAgente(primeiro) = cliente(&consulta) else {
            panic!("plano do agente esperado");
        };
        assert!(primeiro.tarefa.is_some());
        // A segunda sessão entra pela fila com a identidade já autenticada,
        // pois o teste roda dentro de uma única sessão Windows.
        thread::scope(|escopo| {
            let consulta = consulta.clone();
            let outro = escopo.spawn(|| {
                encaminhar(
                    &tx,
                    identidade_teste(),
                    consulta,
                    Duration::from_secs(2),
                    Duration::from_secs(3),
                )
            });
            let Resposta::Estado(estado) = cliente(&Requisicao::ObterEstado) else {
                panic!("estado esperado");
            };
            assert!(estado.gpu.is_none());
            assert!(estado.agente.presente);
            assert_eq!(
                estado
                    .diagnosticos
                    .iter()
                    .map(|d| d.codigo.as_str())
                    .collect::<Vec<_>>(),
                ["hags", "drr"]
            );
            let Resposta::PlanoAgente(outro) = outro.join().unwrap() else {
                panic!("plano da outra sessão esperado");
            };
            assert!(outro.tarefa.is_none());
        });
        let Resposta::Plano(plano) = cliente(&Requisicao::ObterPlano) else {
            panic!("plano da UI esperado");
        };
        assert_eq!(plano.sistema.mudancas.len(), 1);
        assert_eq!(
            cliente(&Requisicao::DefinirAceiteExperimental {
                regra: "windows-captura-historica-off".into(),
                aceito: false,
            }),
            Resposta::Ok
        );
        let Resposta::PlanoAgente(retirado) = cliente(&consulta) else {
            panic!("plano do agente esperado");
        };
        assert!(retirado.tarefa.is_none());
        tx.send(Evento::Parar).unwrap();
        assert!(!motor.join().unwrap());
        parada.parar();
        ipc.join().unwrap();
    }

    fn identidade_teste() -> IdentidadeCliente {
        IdentidadeCliente {
            sid: "S-1-5-21-1".into(),
            sessao: 42,
        }
    }

    #[test]
    fn ocupado_cancela_o_pedido() {
        let (tx, rx) = mpsc::channel();
        let r = encaminhar(
            &tx,
            identidade_teste(),
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
        assert!(!ch.atender(|_, _| {
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
            ch.atender(|_, _| {
                thread::sleep(Duration::from_millis(150));
                Resposta::Ok
            })
        });
        let r = encaminhar(
            &tx,
            identidade_teste(),
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
            ch.atender(|identidade, _| {
                assert_eq!(identidade, identidade_teste());
                Resposta::Ok
            })
        });
        let r = encaminhar(
            &tx,
            identidade_teste(),
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
                identidade_teste(),
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
        fn tratar(
            &mut self,
            _identidade: IdentidadeCliente,
            req: Requisicao,
            _agora: Instant,
        ) -> Resposta {
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
            encaminhar(
                &tx,
                identidade_teste(),
                Requisicao::ObterPlano,
                espera,
                espera
            ),
            Resposta::Ok
        );
        assert!(matches!(
            encaminhar(
                &tx,
                identidade_teste(),
                Requisicao::ReescanearJogos,
                espera,
                espera
            ),
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
            encaminhar(
                &tx,
                identidade_teste(),
                Requisicao::ObterEstado,
                espera,
                espera
            ),
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
