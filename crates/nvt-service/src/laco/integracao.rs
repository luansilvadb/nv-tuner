//! Integração com saúde, atualização, diagnósticos e console, incluindo o
//! atendimento sem driver.

use super::*;

impl Integracao {
    pub(super) fn abrir(op: &Opcoes) -> Self {
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

    /// Sobe a thread do atualizador com as fachadas reais (fora de somente leitura).
    pub(super) fn iniciar_atualizacao(&mut self) {
        let cancelar = Arc::new(AtomicBool::new(false));
        let portao = portao_atualizacao(
            self.portao.clone(),
            self.extras_portao.clone(),
            self.exclusao.clone(),
            cancelar.clone(),
        );
        let r =
            crate::atualizacao::fachadas::Fachadas::reais(portao, cancelar.clone()).and_then(|f| {
                let a = crate::atualizacao::Atualizador::novo(
                    f,
                    crate::atualizacao::fachadas::versao_instalada(),
                );
                ThreadAtualizacao::iniciar(a, self.exclusao.clone(), cancelar)
            });
        match r {
            Ok(t) => self.atualizacao = Some(t),
            Err(e) => error!("atualização automática indisponível: {e}"),
        }
    }

    /// Sobe a thread de saúde (leituras de máquina e limpeza de `C:\Windows\Temp`).
    pub(super) fn iniciar_saude(&mut self, estado_dir: PathBuf) {
        match crate::saude::ThreadSaude::iniciar(
            estado_dir,
            self.portao.clone(),
            self.extras_portao.clone(),
            self.exclusao.clone(),
            self.saude_cache.clone(),
        ) {
            Ok(t) => self.saude = Some(t),
            Err(e) => error!("saúde do PC indisponível: {e}"),
        }
    }

    /// Cópia do cache de máquina (vazio sem a thread de saúde).
    pub(crate) fn saude_maquina(&self) -> crate::saude::SaudeMaquina {
        self.saude_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Projeção atual da atualização (sem `nao_vista`, que é por SID).
    pub(crate) fn estado_atualizacao(&self) -> nvt_ipc::EstadoAtualizacao {
        self.atualizacao
            .as_ref()
            .map_or_else(estado_atualizacao_padrao, ThreadAtualizacao::estado)
    }

    /// Repassa ao atualizador as sessões com agente, quando mudam.
    pub(crate) fn repassar_sessoes(&mut self, agora: Instant) {
        let (Some(usuario), Some(t)) = (&self.usuario, &self.atualizacao) else {
            return;
        };
        let sessoes = usuario.sessoes_agente(agora);
        if self.sessoes_agente.as_ref() != Some(&sessoes) {
            t.definir_sessoes(sessoes.clone());
            self.sessoes_agente = Some(sessoes);
        }
    }

    /// Repassa ao coordenador as preferências vigentes do motor (só invalida
    /// tarefas de jogo quando `limite_fps` muda).
    pub(crate) fn definir_preferencias(&mut self, prefs: &nvt_core::Preferences) {
        if let Some(usuario) = &mut self.usuario {
            usuario.definir_preferencias(prefs);
        }
    }

    pub(super) fn atualizar_console(&mut self, forcar: bool) -> bool {
        let Some(usuario) = &self.usuario else {
            return false;
        };
        let console = identidade_console();
        let sid = console.as_ref().map(|c| c.sid.as_str());
        let aceites = usuario.aceites_drs(sid);
        let limites = usuario.limites_no_jogo_efetivos(sid);
        let mudou =
            self.console != console || self.aceites_drs != aceites || self.limites_jogo != limites;
        self.console = console;
        self.aceites_drs = aceites;
        self.limites_jogo = limites;
        forcar || mudou
    }

    pub(super) fn tratar(
        &mut self,
        identidade: IdentidadeCliente,
        req: Requisicao,
        agora: Instant,
        restauracao: (u64, bool),
        driver: impl FnOnce(Requisicao) -> (Resposta, (u64, bool)),
    ) -> Resposta {
        if let Some(r) = self
            .partidas
            .atender(&identidade, self.console.as_ref(), &req)
        {
            return r;
        }
        let limite = match req {
            Requisicao::ListarHistorico { limite } => limite as usize,
            _ => 0,
        };
        let atualizacao = self.estado_atualizacao();
        if let Some(usuario) = &mut self.usuario {
            usuario.atualizar_restauracao(restauracao.0, restauracao.1);
            usuario.definir_atualizacao(atualizacao.clone());
            if let Some(resposta) = usuario.tratar(&identidade, &req, agora, agora_unix()) {
                self.repassar_sessoes(agora);
                return resposta;
            }
        }
        let (mut resposta, restauracao) = driver(req);
        if let Resposta::Estado(estado) = &mut resposta {
            estado.atualizacao = atualizacao;
            estado.diagnosticos = self
                .diagnosticos
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
        }
        if let Some(usuario) = &mut self.usuario {
            usuario.atualizar_restauracao(restauracao.0, restauracao.1);
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
        self.repassar_sessoes(agora);
        resposta
    }
}

pub(super) fn iniciar_diagnosticos(
    cache: Arc<Mutex<Vec<nvt_ipc::Diagnostico>>>,
) -> Option<Sender<()>> {
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
            autonomia_pausada: false,
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

pub(super) fn plano_sem_driver() -> Resposta {
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

// ── Atualização automática ─────────────────────────────────────────────────
//
// O [`Atualizador`](crate::atualizacao::Atualizador) roda numa thread própria
// (`tick` é síncrono e pode baixar o MSI por minutos). A exclusão com o ciclo
// DRS é feita por [`Exclusao`]: a thread principal marca `drs` durante cada
// requisição, notificação e temporizador do núcleo; o portão do atualizador,
// consultado imediatamente antes de lançar o msiexec, primeiro espera o fim do
// ciclo DRS em curso e **reserva** a instalação, que só é liberada quando o
// `tick` termina. Enquanto reservada, nenhum ciclo DRS começa.
