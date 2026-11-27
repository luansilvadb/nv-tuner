//! Atualização automática no serviço.
//!
//! [`Atualizador`] executa a máquina de estados pura de `nvt_atualizacao::estado`
//! (§7) e aplica os [`Efeito`]s na ordem devolvida; se `Persistir` falhar, os
//! seguintes não rodam e o estado em memória volta ao anterior. Todas as
//! dependências externas ficam em [`fachadas::Fachadas`].
//!
//! `tick` é síncrono (rede e hash rodam dentro dele): o laço deve chamá-lo fora de
//! um ciclo DRS, ou numa thread própria (o atualizador é `Send`).
// Integrado ao laço na tarefa 3.2.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use nvt_atualizacao::estado::FORMATO_ESTADO;
use nvt_atualizacao::politica::{
    FALHAS_PARA_ABANDONAR, RELER_LOG, interpretar_ratelimit_reset, interpretar_retry_after,
};
use nvt_atualizacao::{
    Contexto, Efeito, ErroVerificacao, EstadoPersistido, Evento, FalhaDownload, Fase,
    LIMITE_ASSINATURA, LIMITE_LOG_MSI, LIMITE_MANIFESTO, MotivoAdiamento, PedidoDownload,
    PedidoLancamento, SessaoAgente, UltimoResultado, Versao, log_msi, politica_url, transicao,
};
use nvt_ipc::{
    AtualizacaoConcluida as IpcConcluida, AtualizacaoPendente, ErroVerificacao as IpcErro,
    EstadoAtualizacao, FalhaAtualizacao, MotivoPendente,
};

use fachadas::{Fachadas, MsiPronto};
use http::ErroRede;

/// Cliente WinHTTP: sessão, proxy automático, redirecionamento manual, limites e cancelamento.
pub(crate) mod http;

/// Pasta protegida, arquivo `.parcial`, hash e renomeação pelo handle, limpeza.
pub(crate) mod arquivo;

/// Único ponto de criação de processos (msiexec e agente na sessão do usuário).
pub(crate) mod lancador;

/// Traits das dependências externas, com implementações reais e falsas.
pub(crate) mod fachadas;

#[cfg(test)]
mod testes;

/// Arquivo de estado e sua cópia (§6.3).
pub(crate) const ESTADO: &str = "estado.json";
pub(crate) const ESTADO_BAK: &str = "estado.json.bak";
const LIMITE_ESTADO: u64 = 64 * 1024;
/// O handle do MSI fica aberto até 30 min após o lançamento (U-22).
const SEGURAR_MSI: Duration = Duration::from_secs(30 * 60);
/// Nova tentativa quando a própria gravação do estado falha.
const ESPERA_ERRO_ESTADO: Duration = Duration::from_secs(30 * 60);

/// Executor da atualização automática.
pub(crate) struct Atualizador {
    f: Fachadas,
    instalada: Versao,
    estado: EstadoPersistido,
    iniciado: bool,
    prazo_verificacao: Option<Instant>,
    prazo_reavaliar: Option<Instant>,
    /// MSI verificado, com o handle protegido vivo.
    msi: Option<Box<dyn MsiPronto>>,
    soltar_msi_em: Option<Instant>,
    /// Sessões com agente conectado agora (informadas pelo coordenador).
    agentes: Vec<SessaoAgente>,
    /// Só em memória, para a projeção do IPC.
    motivo_adiamento: Option<MotivoAdiamento>,
    status_http: u16,
    erro_estado: bool,
    /// Texto do último registro de lançamento (U-34), o mesmo enviado ao log.
    #[cfg(test)]
    ultimo_lancamento: Option<String>,
}

impl Atualizador {
    /// Carrega `estado.json`/`.bak` (U-18) e faz a limpeza do início (U-25). A
    /// transição `Iniciado` (leitura do log, conclusão, relançamento) roda no
    /// primeiro [`tick`](Self::tick).
    pub(crate) fn novo(f: Fachadas, versao_instalada: Versao) -> Atualizador {
        let ler = |nome| match f.disco.ler(nome, LIMITE_ESTADO) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("atualização: {nome} ilegível: {e}");
                None
            }
        };
        let (principal, bak) = (ler(ESTADO), ler(ESTADO_BAK));
        let estado =
            EstadoPersistido::carregar(principal.as_deref(), bak.as_deref(), versao_instalada);
        let manter = match &estado.fase {
            Fase::Instalando { para, .. } => Some(*para),
            _ => estado
                .ultima_atualizacao
                .as_ref()
                .map(|u| u.para)
                .max(estado.ultima_falha.as_ref().map(|u| u.versao)),
        };
        f.disco.limpar(manter);
        Atualizador {
            f,
            instalada: versao_instalada,
            estado,
            iniciado: false,
            prazo_verificacao: None,
            prazo_reavaliar: None,
            msi: None,
            soltar_msi_em: None,
            agentes: Vec::new(),
            motivo_adiamento: None,
            status_http: 0,
            erro_estado: false,
            #[cfg(test)]
            ultimo_lancamento: None,
        }
    }

    /// Sessões com agente conectado agora (coordenador). São gravadas em
    /// `sessoes_agente` antes de lançar o msiexec (U-28) e evitam relançar onde já
    /// há agente (U-43).
    pub(crate) fn definir_sessoes_agente(&mut self, sessoes: Vec<SessaoAgente>) {
        self.agentes = sessoes;
    }

    /// Último registro de lançamento do msiexec (U-34).
    #[cfg(test)]
    pub(crate) fn ultimo_lancamento(&self) -> Option<&str> {
        self.ultimo_lancamento.as_deref()
    }

    /// Estado persistido atual (testes).
    #[cfg(test)]
    pub(crate) fn estado(&self) -> &EstadoPersistido {
        &self.estado
    }

    /// Executa o que venceu até `agora` (relógio monotônico) e devolve quanto falta
    /// para o próximo prazo (`None`: nada agendado).
    pub(crate) fn tick(&mut self, agora: Instant) -> Option<Duration> {
        if !self.iniciado {
            self.iniciado = true;
            let codigo_log = self.codigo_log();
            self.aplicar(Evento::Iniciado { codigo_log }, agora);
        }
        if self.soltar_msi_em.is_some_and(|t| agora >= t) {
            self.soltar_msi_em = None;
            self.msi = None;
        }
        if self.prazo_reavaliar.is_some_and(|t| agora >= t) {
            self.prazo_reavaliar = None;
            match self.estado.fase {
                Fase::Pronta { .. } => self.executar(vec![Efeito::ConsultarPortao], agora),
                Fase::Instalando { .. } => {
                    let codigo_log = self.codigo_log();
                    self.aplicar(Evento::LogRelido { codigo_log }, agora);
                }
                _ => {}
            }
        }
        if self.prazo_verificacao.is_some_and(|t| agora >= t) {
            self.prazo_verificacao = None;
            if self.estado.fase == Fase::Ocioso {
                self.aplicar(Evento::PrazoVerificacao, agora);
            } else {
                // Outra fase em curso: a própria fase agenda a próxima ao terminar.
                tracing::debug!("atualização: prazo de verificação fora de `ocioso`");
            }
        }
        if !matches!(
            self.estado.fase,
            Fase::Pronta { .. } | Fase::Instalando { .. }
        ) {
            // Instalação resolvida (ou descartada): o handle do MSI não serve mais.
            self.msi = None;
            self.soltar_msi_em = None;
        }
        [
            self.prazo_verificacao,
            self.prazo_reavaliar,
            self.soltar_msi_em,
        ]
        .into_iter()
        .flatten()
        .min()
        .map(|t| t.saturating_duration_since(agora))
    }

    /// Projeção para o IPC v6 (`nao_vista` fica com o coordenador, por SID).
    pub(crate) fn estado_ipc(&self) -> EstadoAtualizacao {
        let e = &self.estado;
        let ultima_atualizacao = e.ultima_atualizacao.as_ref().map(|u| IpcConcluida {
            de: u.de.to_string(),
            para: u.para.to_string(),
            quando_unix: u.quando_unix,
            reinicio_pendente: u.reinicio_pendente(),
        });
        let pendente = match &e.fase {
            Fase::Baixando { versao, .. } => Some(AtualizacaoPendente {
                versao: versao.to_string(),
                motivo: MotivoPendente::Baixando,
                desde_unix: e.ultima_verificacao_unix.unwrap_or(0),
            }),
            Fase::Pronta {
                versao,
                desde_unix,
                adiada_desde_unix,
                ..
            } => {
                let motivo = match (e.ultimo_resultado, self.motivo_adiamento) {
                    (Some(UltimoResultado::AdiadaMsiOcupado), _) => MotivoPendente::OutraInstalacao,
                    (_, Some(MotivoAdiamento::ListaIndisponivel)) => {
                        MotivoPendente::ListaIndisponivel
                    }
                    (Some(UltimoResultado::AdiadaPortao), _) => MotivoPendente::JogoAberto,
                    _ => MotivoPendente::Instalando,
                };
                Some(AtualizacaoPendente {
                    versao: versao.to_string(),
                    motivo,
                    desde_unix: adiada_desde_unix.unwrap_or(*desde_unix),
                })
            }
            Fase::Instalando {
                para, inicio_unix, ..
            } => Some(AtualizacaoPendente {
                versao: para.to_string(),
                motivo: MotivoPendente::Instalando,
                desde_unix: *inicio_unix,
            }),
            Fase::Ocioso if e.ultimo_resultado == Some(UltimoResultado::AdiadaMsiOcupado) => e
                .ultima_falha
                .as_ref()
                .map(|f| f.versao)
                .or(e.maior_vista)
                .filter(|v| *v > self.instalada)
                .map(|v| AtualizacaoPendente {
                    versao: v.to_string(),
                    motivo: MotivoPendente::OutraInstalacao,
                    desde_unix: e.ultima_verificacao_unix.unwrap_or(0),
                }),
            _ => None,
        };
        let falha = e
            .ultima_falha
            .as_ref()
            .filter(|f| f.versao > self.instalada)
            .map(|f| {
                let abandonada = e.abandonadas.contains(&f.versao);
                FalhaAtualizacao {
                    versao: f.versao.to_string(),
                    codigo: f.codigo,
                    tentativas: e.falhas.get(&f.versao).copied().unwrap_or(if abandonada {
                        FALHAS_PARA_ABANDONAR
                    } else {
                        0
                    }),
                    abandonada,
                    quando_unix: f.quando_unix,
                }
            });
        let erro_verificacao = if self.erro_estado {
            Some(IpcErro::Disco)
        } else {
            match e.ultimo_resultado {
                Some(UltimoResultado::ErroRede) => Some(IpcErro::SemConexao),
                Some(UltimoResultado::ErroLimite) => {
                    Some(IpcErro::RecusadoHttp(if self.status_http == 0 {
                        429
                    } else {
                        self.status_http
                    }))
                }
                Some(UltimoResultado::ErroAssinatura) => Some(IpcErro::Assinatura),
                Some(UltimoResultado::ErroManifesto) => Some(IpcErro::Manifesto),
                Some(UltimoResultado::ErroDisco) => Some(IpcErro::Disco),
                _ => None,
            }
        };
        EstadoAtualizacao {
            versao_instalada: self.instalada.to_string(),
            ultima_verificacao_unix: e.ultima_verificacao_unix,
            proxima_verificacao_unix: e.proxima_verificacao_unix,
            ultima_atualizacao,
            nao_vista: false,
            pendente,
            falha,
            erro_verificacao,
        }
    }

    // ------------------------------------------------------------ execução

    fn contexto(&self) -> Contexto {
        Contexto {
            agora_unix: self.f.relogio.agora_unix(),
            instalada: self.instalada,
            aleatorio: self.f.aleatorio.sortear(),
        }
    }

    /// Código do log `/l*v` da fase `instalando` (U-35), lido do fim com limite.
    fn codigo_log(&self) -> Option<i64> {
        let Fase::Instalando { log, .. } = &self.estado.fase else {
            return None;
        };
        match self.f.disco.ler(log, LIMITE_LOG_MSI) {
            Ok(Some(b)) => log_msi::extrair_codigo(&b),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!("atualização: log {log} ilegível: {e}");
                None
            }
        }
    }

    /// Aplica um evento e, em cadeia, os eventos que seus efeitos produzirem.
    fn aplicar(&mut self, evento: Evento, agora: Instant) {
        let mut fila = VecDeque::from([evento]);
        while let Some(ev) = fila.pop_front() {
            let ctx = self.contexto();
            let (novo, efeitos) = transicao(&self.estado, ev, &ctx);
            let anterior = std::mem::replace(&mut self.estado, novo);
            if let Some(ev) = self.efeitos(efeitos, anterior, agora) {
                fila.push_back(ev);
            }
        }
    }

    /// Efeitos sem transição prévia (reavaliação do portão).
    fn executar(&mut self, efeitos: Vec<Efeito>, agora: Instant) {
        let anterior = self.estado.clone();
        if let Some(ev) = self.efeitos(efeitos, anterior, agora) {
            self.aplicar(ev, agora);
        }
    }

    /// Aplica `efeitos` na ordem; devolve o evento resultante (no máximo um efeito
    /// por lista gera evento). `anterior` é restaurado se `Persistir` falhar.
    fn efeitos(
        &mut self,
        efeitos: Vec<Efeito>,
        anterior: EstadoPersistido,
        agora: Instant,
    ) -> Option<Evento> {
        let mut resultado = None;
        for ef in efeitos {
            match ef {
                Efeito::Persistir => {
                    if let Err(e) = self.persistir() {
                        tracing::error!(
                            "atualização: falha ao gravar o estado ({e}); efeitos seguintes cancelados"
                        );
                        self.recuperar_persistencia(anterior, agora);
                        return None;
                    }
                }
                Efeito::BuscarManifesto => resultado = Some(self.buscar_manifesto()),
                Efeito::Baixar(p) => resultado = Some(self.baixar(&p)),
                Efeito::ConsultarPortao => resultado = Some(self.consultar_portao()),
                Efeito::Lancar(p) => resultado = Some(self.lancar(&p, agora)),
                Efeito::DescartarMsi => {
                    self.msi = None;
                    self.soltar_msi_em = None;
                }
                Efeito::RelancarAgentes(s) => self.relancar_agentes(&s),
                Efeito::AgendarVerificacao(d) => self.prazo_verificacao = Some(agora + d),
                Efeito::Reavaliar(d) => self.prazo_reavaliar = Some(agora + d),
                Efeito::Registrar(m) => tracing::info!("atualização: {m}"),
            }
        }
        if matches!(resultado, Some(Evento::Lancado { .. })) {
            // Sem handle do processo (o lançador o fecha): o fim do msiexec com o
            // serviço antigo vivo (ex.: 1618) é visto relendo o log (U-33).
            self.prazo_reavaliar = Some(agora + RELER_LOG);
        }
        resultado
    }

    fn persistir(&mut self) -> std::io::Result<()> {
        debug_assert_eq!(self.estado.formato, FORMATO_ESTADO);
        let b = self.estado.serializar();
        self.f.disco.gravar(ESTADO, &b)?;
        self.f.disco.gravar(ESTADO_BAK, &b)?;
        self.erro_estado = false;
        Ok(())
    }

    /// Persistência falhou: nada do que dependia dela roda (U-28). Volta ao estado
    /// anterior e agenda nova tentativa sem laço agressivo.
    fn recuperar_persistencia(&mut self, anterior: EstadoPersistido, agora: Instant) {
        self.erro_estado = true;
        self.estado = anterior;
        match self.estado.fase {
            Fase::Pronta { .. } => self.prazo_reavaliar = Some(agora + ESPERA_ERRO_ESTADO),
            Fase::Instalando { .. } => self.prazo_reavaliar = Some(agora + RELER_LOG),
            Fase::Verificando | Fase::Baixando { .. } => {
                self.estado.fase = Fase::Ocioso;
                self.msi = None;
                self.prazo_verificacao = Some(agora + ESPERA_ERRO_ESTADO);
            }
            Fase::Ocioso => {
                if self.prazo_verificacao.is_none_or(|t| t <= agora) {
                    self.prazo_verificacao = Some(agora + ESPERA_ERRO_ESTADO);
                }
            }
        }
    }

    fn falha_rede(&mut self, e: &ErroRede) -> Evento {
        tracing::info!("atualização: rede indisponível: {e}");
        match e {
            ErroRede::Limitado {
                status,
                retry_after,
                ratelimit_reset,
            } => {
                self.status_http = *status;
                let agora = self.f.relogio.agora_unix();
                Evento::FalhaRede {
                    limite: true,
                    retry_after_s: retry_after
                        .as_deref()
                        .and_then(|v| interpretar_retry_after(v, agora)),
                    reset_s: ratelimit_reset
                        .as_deref()
                        .and_then(|v| interpretar_ratelimit_reset(v, agora)),
                }
            }
            _ => Evento::FalhaRede {
                limite: false,
                retry_after_s: None,
                reset_s: None,
            },
        }
    }

    fn buscar_manifesto(&mut self) -> Evento {
        if self.f.verificador.chave_provisoria() {
            // Nada é verificável: nenhum contato com a rede; próxima em 6 h.
            return Evento::ManifestoRecusado(ErroVerificacao::ChaveProvisoria);
        }
        let obter = |url: &str, limite: usize| {
            let u = politica_url(url).map_err(ErroRede::Url)?;
            self.f.rede.obter(&u, limite as u64).map(|r| r.corpo)
        };
        let manifesto = match obter(&nvt_atualizacao::url_manifesto(), LIMITE_MANIFESTO) {
            Ok(b) => b,
            Err(ErroRede::NaoEncontrado) => return Evento::ManifestoAusente,
            Err(ErroRede::TamanhoExcedido { .. }) => {
                return Evento::ManifestoRecusado(ErroVerificacao::ManifestoGrande);
            }
            Err(e) => return self.falha_rede(&e),
        };
        let assinatura = match obter(&nvt_atualizacao::url_assinatura(), LIMITE_ASSINATURA) {
            Ok(b) => b,
            Err(ErroRede::NaoEncontrado) => {
                return Evento::ManifestoRecusado(ErroVerificacao::AssinaturaMalformada);
            }
            Err(ErroRede::TamanhoExcedido { .. }) => {
                return Evento::ManifestoRecusado(ErroVerificacao::AssinaturaGrande);
            }
            Err(e) => return self.falha_rede(&e),
        };
        self.status_http = 0;
        match self.f.verificador.verificar(&manifesto, &assinatura) {
            Ok(m) => Evento::ManifestoValido(m),
            Err(e) => Evento::ManifestoRecusado(e),
        }
    }

    /// Download para o `.parcial` e verificação pelo handle (U-14, U-24).
    fn baixar(&mut self, p: &PedidoDownload) -> Evento {
        self.msi = None;
        let url = match politica_url(&p.url) {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("atualização: URL do MSI recusada: {e:?}");
                return Evento::FalhaDownload(FalhaDownload::Verificacao);
            }
        };
        // MSI/log de tentativa anterior impediriam a renomeação sem substituir.
        self.f.disco.limpar(None);
        let mut d = match self.f.disco.novo_download(p.versao) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("atualização: não foi possível criar o download: {e}");
                return Evento::FalhaDownload(FalhaDownload::Disco);
            }
        };
        match self.f.rede.baixar(&url, p.tamanho, d.escritor()) {
            Ok(_) => {}
            Err(ErroRede::Escrita(k)) => {
                tracing::warn!("atualização: escrita do MSI falhou: {k:?}");
                return Evento::FalhaDownload(FalhaDownload::Disco);
            }
            Err(ErroRede::TamanhoExcedido { .. }) => {
                tracing::warn!("atualização: MSI maior que o tamanho assinado");
                return Evento::FalhaDownload(FalhaDownload::Verificacao);
            }
            Err(e) => {
                tracing::info!("atualização: download do MSI falhou: {e}");
                return Evento::FalhaDownload(FalhaDownload::Rede);
            }
        }
        let baixado = match d.concluir() {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("atualização: leitura do MSI falhou: {e}");
                return Evento::FalhaDownload(FalhaDownload::Disco);
            }
        };
        if baixado.tamanho() != p.tamanho || baixado.sha256() != p.sha256 {
            tracing::warn!(
                "atualização: MSI {} divergente (tamanho {} ≠ {} ou SHA-256 {} ≠ {}); descartado",
                p.msi,
                baixado.tamanho(),
                p.tamanho,
                baixado.sha256(),
                p.sha256
            );
            return Evento::FalhaDownload(FalhaDownload::Verificacao);
        }
        match baixado.finalizar() {
            Ok(m) if m.sha256() == p.sha256 && m.tamanho() == p.tamanho => {
                self.msi = Some(m);
                Evento::MsiVerificado
            }
            Ok(_) => Evento::FalhaDownload(FalhaDownload::Verificacao),
            Err(e) => {
                tracing::warn!("atualização: finalização do MSI falhou: {e}");
                Evento::FalhaDownload(FalhaDownload::Disco)
            }
        }
    }

    /// Portão e `_MSIExecute` agora, imediatamente antes de lançar (U-26, U-27).
    fn consultar_portao(&mut self) -> Evento {
        let Fase::Pronta { sha256, .. } = &self.estado.fase else {
            return Evento::ArquivoPerdido;
        };
        let ok = self
            .msi
            .as_ref()
            .is_some_and(|m| m.intacto() && m.sha256() == sha256);
        if !ok {
            tracing::warn!("atualização: MSI verificado perdido ou alterado; descartado");
            return Evento::ArquivoPerdido;
        }
        if self.f.portao.msi_ocupado() {
            self.motivo_adiamento = None;
            return Evento::MsiOcupado;
        }
        match self.f.portao.consultar() {
            Ok(()) => {
                self.motivo_adiamento = None;
                Evento::PortaoLiberou {
                    sessoes_agente: self.agentes.clone(),
                }
            }
            Err(m) => {
                self.motivo_adiamento = Some(m);
                Evento::PortaoAdiou(m)
            }
        }
    }

    /// `fase = instalando` já está persistida (efeito anterior); lança sem esperar.
    fn lancar(&mut self, p: &PedidoLancamento, agora: Instant) -> Evento {
        let Some(msi) = self.msi.as_ref().filter(|m| m.sha256() == p.sha256) else {
            tracing::error!("atualização: MSI ausente no lançamento");
            return Evento::LancamentoFalhou { codigo_win32: -1 };
        };
        let log = self.f.disco.pasta().join(&p.log);
        match self.f.lancador.lancar_msiexec(msi.caminho(), &log) {
            Ok(l) => {
                let registro = format!(
                    "msiexec lançado: de={} para={} sha256={} msi={} log={} pid={} flags={:#x} breakaway_recusado={} quando_unix={}",
                    p.de,
                    p.para,
                    p.sha256,
                    msi.caminho().display(),
                    log.display(),
                    l.pid,
                    l.flags,
                    l.breakaway_recusado,
                    self.f.relogio.agora_unix()
                );
                tracing::info!("atualização: {registro}");
                #[cfg(test)]
                {
                    self.ultimo_lancamento = Some(registro);
                }
                self.soltar_msi_em = Some(agora + SEGURAR_MSI);
                Evento::Lancado { pid: l.pid }
            }
            Err(e) => {
                tracing::error!("atualização: lançamento do msiexec falhou: {e}");
                let codigo = match e {
                    lancador::ErroLancador::Win32 { codigo, .. } => i64::from(codigo),
                    _ => -1,
                };
                Evento::LancamentoFalhou {
                    codigo_win32: codigo,
                }
            }
        }
    }

    /// Uma vez, só nas sessões gravadas antes do upgrade que estão ativas e sem
    /// agente; o SID é conferido pelo lançador (U-43). Falha só é registrada (U-46).
    fn relancar_agentes(&mut self, gravadas: &[SessaoAgente]) {
        let ativas = self.f.sessoes.ativas();
        let alvo: Vec<(u32, String)> = gravadas
            .iter()
            .filter(|s| ativas.contains(&s.sessao))
            .filter(|s| !self.agentes.iter().any(|a| a.sessao == s.sessao))
            .filter(|s| !self.f.sessoes.agente_presente(s.sessao))
            .map(|s| (s.sessao, s.sid.clone()))
            .collect();
        if alvo.is_empty() {
            return;
        }
        for (sessao, r) in self.f.lancador.relancar_agente(&alvo) {
            match r {
                Ok(pid) => {
                    tracing::info!("atualização: agente relançado na sessão {sessao} (PID {pid})")
                }
                Err(e) => tracing::warn!(
                    "atualização: agente não relançado na sessão {sessao}: {e}; volta no próximo logon"
                ),
            }
        }
    }
}
