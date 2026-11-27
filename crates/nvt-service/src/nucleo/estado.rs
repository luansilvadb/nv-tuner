//! Construção do Nucleo, abertura do armazém e acessores de estado.

use super::*;

impl<'a, S: DriverStore> Nucleo<'a, S> {
    /// Abre o estado de `dir` (`None` = só em memória) e a primeira sessão do
    /// DRS, com o portão sobre os processos do sistema.
    pub fn novo(
        fabrica: Fabrica<'a, S>,
        varredor: Varredor<'a>,
        assinador: Assinador<'a>,
        dir: Option<PathBuf>,
        cfg: Config,
        agora: Instant,
    ) -> Resultado<Self> {
        Self::novo_com_portao(
            fabrica,
            varredor,
            assinador,
            dir,
            cfg,
            agora,
            Portao::do_sistema(),
        )
    }

    /// Como [`Nucleo::novo`], com o portão dado (fonte de processos injetável).
    pub fn novo_com_portao(
        mut fabrica: Fabrica<'a, S>,
        varredor: Varredor<'a>,
        assinador: Assinador<'a>,
        dir: Option<PathBuf>,
        cfg: Config,
        agora: Instant,
        portao: Portao,
    ) -> Resultado<Self> {
        let mut avisos_estado = Vec::new();
        let armazem = match &dir {
            Some(d) => abrir_armazem(d, &cfg, &mut avisos_estado)?,
            None => Armazem::em_memoria(),
        };
        let engine = Engine::new(fabrica()?, armazem)?.com_portao(portao.clone());
        let servico = EstadoServico::carregar(dir.as_deref());
        // Restauração adiada antes de o serviço parar: volta a ser a ação
        // pendente, reavaliada já (nunca em somente leitura).
        let pendente = servico
            .restauracao_pendente_desde_unix
            .filter(|_| !cfg.somente_leitura)
            .map(|desde_unix| Pendente {
                acao: AcaoPendente::RestaurarOriginal,
                adiamento: Adiamento {
                    processos: Vec::new(),
                    desde_unix,
                    lista_indisponivel: false,
                },
                tentativas: 0,
                ultimo_erro: None,
            });
        let proxima_reavaliacao = pendente.as_ref().map(|_| agora);
        Ok(Self {
            regras_drs: engine.regras().clone(),
            aceites_drs: BTreeSet::new(),
            limites_jogo: BTreeMap::new(),
            engine,
            fabrica,
            varredor,
            assinador,
            cfg,
            servico,
            dir,
            jogos: Vec::new(),
            alertas: Vec::new(),
            avisos_estado,
            erros_aplicacao: Vec::new(),
            erros_deriva: Vec::new(),
            erros_pendente: Vec::new(),
            ignorados: Vec::new(),
            ignorados_unix: 0,
            avisos_plano: Vec::new(),
            avisos_plano_unix: 0,
            ultima_gravacao: None,
            assinatura_propria: None,
            mudanca_pendente: None,
            proxima_verificacao: agora + cfg.intervalo_verificacao,
            proxima_varredura: agora + cfg.intervalo_varredura,
            portao,
            pendente,
            proxima_reavaliacao,
        })
    }

    pub fn engine(&self) -> &Engine<S> {
        &self.engine
    }

    pub fn servico(&self) -> &EstadoServico {
        &self.servico
    }

    pub fn jogos(&self) -> &[JogoEntrada] {
        &self.jogos
    }

    #[cfg(test)]
    pub fn alertas(&self) -> &[nvt_ipc::AlertaDeriva] {
        &self.alertas
    }

    /// O serviço pode gravar (não está em modo somente leitura)?
    pub fn pode_gravar(&self) -> bool {
        !self.cfg.somente_leitura
    }

    /// Aplicação automática ativa?
    pub fn autonomia_ativa(&self) -> bool {
        self.pode_gravar() && !self.servico.autonomia_pausada
    }

    /// Relê o driver (sessão nova) e o estado em disco, que pode ter sido
    /// alterado por `nvt-service apply`/`restore-original` em outro processo.
    pub(crate) fn definir_aceites_drs(&mut self, aceites: BTreeSet<String>) {
        self.aceites_drs = aceites;
    }

    /// F3-13: limites no jogo efetivos (de [`crate::usuario::Coordenador::limites_no_jogo_efetivos`]).
    pub(crate) fn definir_limites_jogo(&mut self, limites: BTreeMap<nvt_core::IdJogo, String>) {
        self.limites_jogo = limites;
    }

    /// Preferências persistidas no estado do motor (repassadas ao coordenador).
    pub(crate) fn preferencias(&self) -> Preferences {
        self.engine.estado().preferencias.clone()
    }
}

/// Abre o armazém e acumula os avisos novos em `acumulados` (registrando no
/// log). O estado recuperado ou reiniciado já sai gravado de
/// [`Armazem::abrir`] (que falha se não conseguir gravá-lo); e, se uma
/// gravação se perder, a abertura seguinte recupera de novo de
/// `estado.anterior.json` em vez de começar vazia.
///
/// Em modo somente leitura usa [`Armazem::abrir_somente_leitura`]: nada é
/// renomeado, criado nem gravado no diretório de estado.
pub(super) fn abrir_armazem(
    dir: &Path,
    cfg: &Config,
    acumulados: &mut Vec<Aviso>,
) -> Resultado<Armazem> {
    let armazem = if cfg.somente_leitura {
        Armazem::abrir_somente_leitura(dir)?
    } else {
        Armazem::abrir(dir)?
    };
    let novos = avisos_do_armazem(
        armazem.avisos(),
        armazem.snapshot_indisponivel(),
        agora_unix(),
    );
    let estado_recuperado = novos.iter().any(|a| {
        a.codigo == nvt_ipc::codigo_aviso::ESTADO_RECUPERADO
            || a.codigo == nvt_ipc::codigo_aviso::ESTADO_REINICIADO
    });
    for a in novos {
        let repetido = acumulados
            .iter()
            .any(|x| x.codigo == a.codigo && x.detalhe == a.detalhe);
        if repetido {
            continue;
        }
        let detalhe = a.detalhe.as_deref().unwrap_or("");
        match a.nivel {
            NivelAviso::Erro => error!("estado: {} {detalhe}", a.mensagem),
            NivelAviso::Atencao => warn!("estado: {} {detalhe}", a.mensagem),
            NivelAviso::Info => info!("estado: {} {detalhe}", a.mensagem),
        }
        acumulados.push(a);
    }
    if estado_recuperado && !armazem.somente_leitura() {
        info!("estado recuperado gravado em estado.json");
    }
    Ok(armazem)
}
