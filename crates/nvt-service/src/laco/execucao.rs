//! Execução do serviço: espera do diretório seguro e do driver, e laço com
//! driver presente.

use super::*;

/// Espera `d` atendendo o IPC com o erro `codigo`/`motivo`. `false` se
/// pediram para parar.
pub(super) fn esperar_com_erro(
    rx: &Receiver<Evento>,
    d: Duration,
    codigo: CodigoErro,
    motivo: &str,
) -> bool {
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

/// Campos de `EstadoServico` que o coordenador de usuário acompanha
/// (`geracao_restauracao`, `autonomia_pausada`).
pub(super) fn restauracao(estado: &EstadoServico) -> (u64, bool) {
    (estado.geracao_restauracao, estado.autonomia_pausada)
}

pub(super) fn esperar_sem_driver(
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
                        let anterior = restauracao(&estado);
                        integracao.tratar(identidade, req, Instant::now(), anterior, |req| {
                            let resposta = if op.config.somente_leitura
                                && crate::nucleo::grava_sem_driver(&req)
                            {
                                Resposta::erro(
                                    CodigoErro::Interno,
                                    "serviço em modo somente leitura",
                                )
                            } else {
                                tratar_sem_driver(req, &mut estado, &op.estado_dir, motivo)
                            };
                            (resposta, restauracao(&estado))
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
        "serviço {} iniciado (estado em {}, somente leitura: {})",
        crate::VERSAO,
        op.estado_dir.display(),
        op.config.somente_leitura
    );

    if esperar_diretorio_seguro(op, &rx) {
        let mut integracao = Integracao::abrir(op);
        if !op.config.somente_leitura {
            integracao.iniciar_atualizacao();
            integracao.iniciar_saude(op.estado_dir.clone());
            #[cfg(feature = "medicao-partidas")]
            integracao
                .partidas
                .iniciar(&op.estado_dir, integracao.portao.clone());
        }
        let diagnosticos = iniciar_diagnosticos(integracao.diagnosticos.clone());
        executar_com_driver(op, &rx, &mut integracao);
        // Encerramento: `cancelar` faz a rede desistir; não espera um download inteiro.
        if let Some(mut t) = integracao.atualizacao.take() {
            t.parar();
        }
        drop(integracao.saude.take());
        #[cfg(feature = "medicao-partidas")]
        integracao.partidas.parar();
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
pub(super) fn esperar_diretorio_seguro(op: &Opcoes, rx: &Receiver<Evento>) -> bool {
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

pub(super) fn executar_com_driver(op: &Opcoes, rx: &Receiver<Evento>, integracao: &mut Integracao) {
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
            let portao = integracao.portao.clone();
            let r = protegido("início do motor", || {
                let nucleo = Nucleo::novo_com_portao(
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
                    portao,
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
        integracao.definir_preferencias(&nucleo.preferencias());
        integracao.atualizar_console(true);
        nucleo.definir_aceites_drs(integracao.aceites_drs.clone());
        nucleo.definir_limites_jogo(integracao.limites_jogo.clone());
        let exclusao = integracao.exclusao.clone();
        let fim = match protegido("início do ciclo", || {
            let _drs = exclusao.ciclo_drs();
            nucleo.iniciar(Instant::now());
        }) {
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
