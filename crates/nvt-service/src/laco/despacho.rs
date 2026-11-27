//! Encaminhamento das requisições IPC ao laço principal, atendimento pelo
//! núcleo e o próprio laço de eventos.

use super::*;

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
pub(super) fn protegido<R>(onde: &str, f: impl FnOnce() -> R) -> Option<R> {
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
        if matches!(
            req,
            Requisicao::ObterSaude | Requisicao::MarcarInicioVisto { .. }
        ) {
            // Só lê o cache da thread de saúde; não entra no ciclo DRS.
            let maquina = self.integracao.saude_maquina();
            return crate::saude::atender(
                self.integracao.usuario.as_mut(),
                &identidade,
                &req,
                &maquina,
                agora,
                agora_unix(),
            );
        }
        let exclusao = self.integracao.exclusao.clone();
        let _drs = exclusao.ciclo_drs();
        if matches!(req, Requisicao::AplicarRecomendadas)
            && let Some(usuario) = &mut self.integracao.usuario
        {
            // R-AR: depende da identidade e da sessão de console.
            let resposta = crate::recomendadas::aplicar(
                self.nucleo,
                usuario,
                &identidade,
                self.integracao.console.as_ref(),
                agora,
            );
            self.integracao
                .definir_preferencias(&self.nucleo.preferencias());
            return resposta;
        }
        if let Some(usuario) = &mut self.integracao.usuario
            && matches!(req, Requisicao::AgenteConsultar { .. })
        {
            usuario.atualizar_processos(self.nucleo.processos_vigiados());
        }
        let antes = restauracao(self.nucleo.servico());
        let resposta = self
            .integracao
            .tratar(identidade, req, agora, antes, |req| {
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
                (resposta, restauracao(self.nucleo.servico()))
            });
        self.integracao
            .definir_preferencias(&self.nucleo.preferencias());
        resposta
    }

    fn notificar_mudanca_drs(&mut self, agora: Instant) {
        let _drs = self.integracao.exclusao.ciclo_drs();
        self.nucleo.notificar_mudanca_drs(agora);
    }

    fn tick(&mut self, agora: Instant) {
        let exclusao = self.integracao.exclusao.clone();
        let _drs = exclusao.ciclo_drs();
        let extras: Vec<String> = self.nucleo.portao_extras();
        {
            let mut atual = self
                .integracao
                .extras_portao
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if *atual != extras {
                *atual = extras;
            }
        }
        self.integracao.repassar_sessoes(agora);
        self.integracao
            .definir_preferencias(&self.nucleo.preferencias());
        if self.integracao.atualizar_console(false) {
            self.nucleo
                .definir_aceites_drs(self.integracao.aceites_drs.clone());
            self.nucleo
                .definir_limites_jogo(self.integracao.limites_jogo.clone());
            self.nucleo.ciclo(
                agora,
                "sessão de console, aceite experimental ou limite no jogo",
            );
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
