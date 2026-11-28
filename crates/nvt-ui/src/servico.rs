//! Thread de fundo que conversa com o serviço (cliente IPC bloqueante) e,
//! quando ele não responde, monta o modo de visualização local.
//!
//! A UI envia [`Comando`]s e recebe [`Evento`]s por canais; nada aqui roda
//! na thread da janela.

use std::io::{self, ErrorKind};
use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use nvt_ipc::{ErroIpc, Requisicao, Resposta};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SERVICE_DOES_NOT_EXIST};
use windows_sys::Win32::System::Services::*;
use windows_sys::Win32::System::SystemInformation::GetTickCount64;

use crate::local;
use crate::modelo::{
    Comando, Conexao, Dados, Desfecho, Evento, grupos_do_plano, upgrade_detectado,
};
use crate::textos;

/// Intervalo entre tentativas de conexão no modo de visualização.
const RECONEXAO: Duration = Duration::from_secs(3);
const JANELA_BOOT: Duration = Duration::from_secs(300);
const PRAZO_INICIANDO: Duration = Duration::from_secs(60);
/// Intervalo de atualização automática com o serviço conectado.
const ATUALIZACAO: Duration = Duration::from_secs(15);
/// Entradas do histórico pedidas ao serviço.
const HISTORICO: u32 = 200;
/// Validade da última resposta de `ObterSaude` (o serviço já guarda cache
/// próprio; aqui só se evita uma conexão a mais a cada atualização).
const CACHE_SAUDE: Duration = Duration::from_secs(60);

/// Lado da UI dos canais.
pub struct Canal {
    pub comandos: Sender<Comando>,
    pub eventos: Receiver<Evento>,
}

/// Inicia a thread de comunicação com o pipe `pipe`.
pub fn iniciar(pipe: String) -> Canal {
    let (tx_cmd, rx_cmd) = mpsc::channel();
    let (tx_ev, rx_ev) = mpsc::channel();
    registrar(&pipe, &tx_cmd);
    thread::Builder::new()
        .name("nvt-ui-ipc".into())
        .spawn(move || Trabalhador::new(pipe, tx_ev).executar(&rx_cmd))
        .expect("falha ao criar a thread de comunicação");
    Canal {
        comandos: tx_cmd,
        eventos: rx_ev,
    }
}

/// Falha ao falar com o serviço.
enum Falha {
    Ipc(ErroIpc),
    /// Erro de negócio (o serviço recusou).
    Recusa(String),
}

/// Saúde de uma resposta do serviço; inválida ou de outro tipo = sem dados.
fn saude_da_resposta(r: Resposta) -> Option<nvt_ipc::Saude> {
    match r {
        Resposta::Saude(s) if s.validar().is_ok() => Some(s),
        _ => None,
    }
}

fn mensagem_ipc(e: &ErroIpc) -> String {
    match e {
        ErroIpc::VersaoIncompativel { recebida, esperada } => {
            textos::versao_incompativel(*recebida, *esperada)
        }
        ErroIpc::ServidorNaoConfiavel => {
            textos::erro_comunicacao(&ErroIpc::ServicoIndisponivel.to_string())
        }
        outro => textos::erro_comunicacao(&outro.to_string()),
    }
}

enum ConsultaScm {
    Ausente,
    Falhou,
    Instalado {
        estado: u32,
        inicio: u32,
        saida: u32,
    },
}

/// Mantém a primeira observação elegível até o IPC voltar a responder.
fn classificar(
    falha: &Falha,
    scm: ConsultaScm,
    uptime: Duration,
    primeira: Option<Instant>,
    agora: Instant,
) -> (Conexao, Option<Instant>) {
    let ConsultaScm::Instalado {
        estado,
        inicio,
        saida,
    } = scm
    else {
        return (
            if matches!(scm, ConsultaScm::Ausente) {
                Conexao::Visualizacao
            } else {
                Conexao::SemResposta
            },
            primeira,
        );
    };
    let recuperavel = match falha {
        Falha::Ipc(ErroIpc::ServicoIndisponivel) => true,
        Falha::Ipc(ErroIpc::Io(e)) => e.kind() == ErrorKind::TimedOut,
        _ => false,
    };
    let subindo = matches!(estado, SERVICE_START_PENDING | SERVICE_RUNNING)
        || (estado == SERVICE_STOPPED
            && inicio == SERVICE_AUTO_START
            && saida == 0
            && uptime < JANELA_BOOT);
    if recuperavel && subindo {
        let primeira = primeira.unwrap_or(agora);
        if agora.duration_since(primeira) < PRAZO_INICIANDO {
            return (Conexao::Iniciando, Some(primeira));
        }
    }
    (Conexao::SemResposta, primeira)
}

struct HandleScm(SC_HANDLE);

impl Drop for HandleScm {
    fn drop(&mut self) {
        // SAFETY: o handle válido pertence a este objeto.
        unsafe { CloseServiceHandle(self.0) };
    }
}

fn consultar_scm() -> ConsultaScm {
    // SAFETY: ponteiros nulos selecionam o SCM local e o banco padrão.
    let scm = unsafe { OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT) };
    if scm.is_null() {
        return ConsultaScm::Falhou;
    }
    let scm = HandleScm(scm);
    let nome: Vec<u16> = nvt_core::produto::NOME_SERVICO
        .encode_utf16()
        .chain([0])
        .collect();
    // SAFETY: handle válido e nome terminado em zero.
    let servico = unsafe {
        OpenServiceW(
            scm.0,
            nome.as_ptr(),
            SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG,
        )
    };
    if servico.is_null() {
        return if io::Error::last_os_error().raw_os_error()
            == Some(ERROR_SERVICE_DOES_NOT_EXIST as i32)
        {
            ConsultaScm::Ausente
        } else {
            ConsultaScm::Falhou
        };
    }
    let servico = HandleScm(servico);
    let mut status = SERVICE_STATUS_PROCESS::default();
    let mut tamanho = 0;
    // SAFETY: buffer gravável do tamanho e alinhamento da estrutura pedida.
    if unsafe {
        QueryServiceStatusEx(
            servico.0,
            SC_STATUS_PROCESS_INFO,
            (&mut status as *mut SERVICE_STATUS_PROCESS).cast(),
            size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut tamanho,
        )
    } == 0
    {
        return ConsultaScm::Falhou;
    }
    tamanho = 0;
    // SAFETY: a chamada com buffer nulo obtém o tamanho necessário.
    unsafe { QueryServiceConfigW(servico.0, null_mut(), 0, &mut tamanho) };
    if io::Error::last_os_error().raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
        || (tamanho as usize) < size_of::<QUERY_SERVICE_CONFIGW>()
    {
        return ConsultaScm::Falhou;
    }
    // A estrutura contém ponteiros, então o buffer precisa de alinhamento de palavra.
    let mut buffer = vec![0usize; (tamanho as usize).div_ceil(size_of::<usize>())];
    let config = buffer.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
    // SAFETY: buffer alinhado e gravável com pelo menos o tamanho informado.
    if unsafe { QueryServiceConfigW(servico.0, config, tamanho, &mut tamanho) } == 0 {
        return ConsultaScm::Falhou;
    }
    ConsultaScm::Instalado {
        estado: status.dwCurrentState,
        // SAFETY: a chamada bem-sucedida inicializou a estrutura no buffer.
        inicio: unsafe { (*config).dwStartType },
        saida: status.dwWin32ExitCode,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Espera {
    Bloquear,
    Aguardar(Duration),
    Atualizar,
}

fn proxima_espera(
    encerrado: bool,
    servico: bool,
    ultima_tentativa: Option<Instant>,
    prazo_iniciando: Option<Instant>,
    agora: Instant,
) -> Espera {
    if encerrado {
        return Espera::Bloquear;
    }
    if servico {
        return Espera::Aguardar(ATUALIZACAO);
    }
    let reconexao = ultima_tentativa.map_or(RECONEXAO, |t| {
        (t + RECONEXAO).saturating_duration_since(agora)
    });
    let espera = prazo_iniciando.map_or(reconexao, |prazo| {
        reconexao.min(prazo.saturating_duration_since(agora))
    });
    if espera.is_zero() {
        Espera::Atualizar
    } else {
        Espera::Aguardar(espera)
    }
}

struct Trabalhador {
    pipe: String,
    eventos: Sender<Evento>,
    servico: bool,
    conexao_local: Conexao,
    primeira_elegivel: Option<Instant>,
    ultima_tentativa: Option<Instant>,
    /// Último estado lido do serviço (para reconhecer a queda num upgrade).
    ultimo_estado: Option<nvt_ipc::Estado>,
    /// O nv-tuner foi atualizado (U-42): não reconecta mais.
    encerrado: bool,
    /// Cache de `ObterSaude` e quando foi lido (`None` = ler de novo).
    saude: Option<nvt_ipc::Saude>,
    saude_lida: Option<Instant>,
}

impl Trabalhador {
    fn new(pipe: String, eventos: Sender<Evento>) -> Self {
        Self {
            pipe,
            eventos,
            servico: false,
            conexao_local: Conexao::Conectando,
            primeira_elegivel: None,
            ultima_tentativa: None,
            ultimo_estado: None,
            encerrado: false,
            saude: None,
            saude_lida: None,
        }
    }

    fn enviar(&self, ev: Evento) {
        // A janela fechou: nada a fazer.
        let _ = self.eventos.send(ev);
    }

    fn executar(mut self, comandos: &Receiver<Comando>) {
        self.atualizar(true);
        loop {
            let espera = match proxima_espera(
                self.encerrado,
                self.servico,
                self.ultima_tentativa,
                self.prazo_iniciando(),
                Instant::now(),
            ) {
                Espera::Bloquear => {
                    while let Ok(c) = comandos.recv() {
                        self.tratar(c);
                    }
                    break;
                }
                Espera::Aguardar(espera) => espera,
                Espera::Atualizar => {
                    self.atualizar(false);
                    continue;
                }
            };
            match comandos.recv_timeout(espera) {
                Ok(c) => self.tratar(c),
                Err(RecvTimeoutError::Timeout) => self.atualizar(false),
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    /// Uma requisição numa conexão nova (o servidor atende em sequência).
    fn requisitar(&self, req: &Requisicao) -> Result<Resposta, Falha> {
        let mut cfg = nvt_ipc::pipe::ConfigCliente::default();
        if let Some(restante) = self.tempo_iniciando() {
            // Conexão, envio e leitura dividem o tempo que resta.
            let limite = restante / 3;
            if limite.is_zero() {
                return Err(Falha::Ipc(io::Error::from(ErrorKind::TimedOut).into()));
            }
            cfg.timeout_conexao = cfg.timeout_conexao.min(limite);
            cfg.timeout_escrita = cfg.timeout_escrita.min(limite);
            cfg.timeout_leitura = cfg.timeout_leitura.min(limite);
        }
        let mut c = nvt_ipc::pipe::Cliente::conectar(&self.pipe, &cfg).map_err(Falha::Ipc)?;
        match c.requisitar(req).map_err(Falha::Ipc)? {
            Resposta::Erro { codigo, mensagem } => {
                Err(Falha::Recusa(textos::erro_servico(codigo, &mensagem)))
            }
            r => Ok(r),
        }
    }

    fn buscar(&self) -> Result<Dados, Falha> {
        let estado = match self.requisitar(&Requisicao::ObterEstado)? {
            Resposta::Estado(e) => Some(e),
            _ => None,
        };
        let (grupos, respeitadas, reversoes, ignorados, jogos, sistema, sugestoes_driver) =
            match self.requisitar(&Requisicao::ObterPlano)? {
                Resposta::Plano(p) => {
                    let (g, r) = grupos_do_plano(&p);
                    (
                        g,
                        r,
                        p.reversoes,
                        p.ignorados,
                        p.jogos,
                        p.sistema,
                        p.sugestoes_driver,
                    )
                }
                _ => Default::default(),
            };
        let historico = match self.requisitar(&Requisicao::ListarHistorico { limite: HISTORICO })? {
            Resposta::Historico { entradas } => entradas,
            _ => Vec::new(),
        };
        Ok(Dados {
            estado,
            sistema,
            sugestoes_driver,
            grupos,
            respeitadas,
            historico,
            reversoes,
            ignorados,
            jogos,
            saude: None,
        })
    }

    fn prazo_iniciando(&self) -> Option<Instant> {
        self.primeira_elegivel
            .filter(|_| self.conexao_local == Conexao::Iniciando)
            .map(|t| t + PRAZO_INICIANDO)
    }

    fn tempo_iniciando(&self) -> Option<Duration> {
        self.prazo_iniciando()
            .map(|prazo| prazo.saturating_duration_since(Instant::now()))
    }

    fn diagnostico_local(&self, agora: Instant) -> Conexao {
        if self.prazo_iniciando().is_some_and(|prazo| prazo <= agora) {
            Conexao::SemResposta
        } else {
            self.conexao_local
        }
    }

    fn mudar_conexao_local(&mut self, conexao: Conexao) {
        if self.conexao_local != conexao {
            self.conexao_local = conexao;
            self.enviar(Evento::Conexao(conexao));
        }
    }

    /// Atualiza os dados. No modo de visualização, só tenta reconectar a
    /// cada [`RECONEXAO`]; o cálculo local é refeito apenas quando `forcar`.
    fn atualizar(&mut self, forcar: bool) {
        if self.encerrado {
            return;
        }
        self.mudar_conexao_local(self.diagnostico_local(Instant::now()));
        if !self.servico
            && !forcar
            && self
                .ultima_tentativa
                .is_some_and(|t| t.elapsed() < RECONEXAO)
        {
            return;
        }
        let resultado = self.buscar();
        self.ultima_tentativa = Some(Instant::now());
        if self.upgrade(&resultado) {
            self.encerrado = true;
            self.servico = false;
            self.enviar(Evento::Atualizado);
            return;
        }
        match resultado {
            Ok(mut d) => {
                d.saude = self.saude_atual(forcar);
                self.primeira_elegivel = None;
                self.conexao_local = Conexao::Conectando;
                self.servico = true;
                self.ultimo_estado = d.estado.clone();
                self.enviar(Evento::Servico(d));
            }
            Err(falha) => {
                let scm = match &falha {
                    Falha::Ipc(e @ ErroIpc::VersaoIncompativel { .. }) => {
                        self.enviar(Evento::Incompativel(Some(mensagem_ipc(e))));
                        ConsultaScm::Falhou
                    }
                    Falha::Ipc(ErroIpc::ServicoIndisponivel | ErroIpc::ServidorNaoConfiavel) => {
                        self.enviar(Evento::Incompativel(None));
                        consultar_scm()
                    }
                    _ => consultar_scm(),
                };
                // SAFETY: a função não recebe argumentos nem acessa buffers.
                let uptime = Duration::from_millis(unsafe { GetTickCount64() });
                let (conexao, primeira) =
                    classificar(&falha, scm, uptime, self.primeira_elegivel, Instant::now());
                self.primeira_elegivel = primeira;
                if self.servico || forcar {
                    self.modo_visualizacao(conexao, local::calcular);
                } else {
                    self.mudar_conexao_local(conexao);
                }
            }
        }
    }

    /// Saúde do PC com cache de [`CACHE_SAUDE`]. Serviço sem o recurso
    /// (`RecursoAusente`) ou resposta inválida: sem dados, e a seção some.
    /// Falha de transporte mantém a última resposta.
    fn saude_atual(&mut self, forcar: bool) -> Option<nvt_ipc::Saude> {
        if !forcar && self.saude_lida.is_some_and(|t| t.elapsed() < CACHE_SAUDE) {
            return self.saude.clone();
        }
        match self.requisitar(&Requisicao::ObterSaude) {
            Ok(r) => self.saude = saude_da_resposta(r),
            Err(Falha::Recusa(_)) => self.saude = None,
            Err(Falha::Ipc(_)) => {}
        }
        self.saude_lida = Some(Instant::now());
        self.saude.clone()
    }

    /// O resultado indica que o serviço foi trocado por uma atualização?
    /// Versão diferente da interface, ou queda (inclusive protocolo novo)
    /// depois de conectar enquanto o serviço baixava ou instalava.
    fn upgrade(&self, r: &Result<Dados, Falha>) -> bool {
        let versao = env!("CARGO_PKG_VERSION");
        match r {
            Ok(d) => d
                .estado
                .as_ref()
                .is_some_and(|e| upgrade_detectado(None, Some(e), versao)),
            Err(Falha::Ipc(ErroIpc::VersaoIncompativel { .. })) if self.servico => true,
            Err(Falha::Ipc(_)) if self.servico => {
                upgrade_detectado(self.ultimo_estado.as_ref(), None, versao)
            }
            Err(_) => false,
        }
    }

    /// Calcula os dados localmente, somente leitura.
    fn modo_visualizacao(
        &mut self,
        conexao: Conexao,
        calcular: impl FnOnce() -> Result<Dados, String>,
    ) {
        self.servico = false;
        self.mudar_conexao_local(conexao);
        self.mudar_conexao_local(self.diagnostico_local(Instant::now()));
        let dados = if let Some(prazo) = self.prazo_iniciando() {
            // O diagnóstico precisa vencer mesmo com a leitura local em andamento.
            thread::scope(|escopo| {
                let (terminou, aguardar) = mpsc::channel::<()>();
                let eventos = self.eventos.clone();
                escopo.spawn(move || {
                    let restante = prazo.saturating_duration_since(Instant::now());
                    if aguardar.recv_timeout(restante) == Err(RecvTimeoutError::Timeout) {
                        let _ = eventos.send(Evento::Conexao(Conexao::SemResposta));
                    }
                });
                let dados = calcular();
                drop(terminou);
                dados
            })
        } else {
            calcular()
        };
        self.conexao_local = self.diagnostico_local(Instant::now());
        self.enviar(Evento::Visualizacao(self.conexao_local, dados));
    }

    fn tratar(&mut self, c: Comando) {
        if self.encerrado {
            self.concluir(c, Err(textos::ATUALIZADO_REABRA.into()));
            return;
        }
        match &c {
            Comando::Atualizar => {
                self.enviar(Evento::Ocupado(c.clone()));
                self.atualizar(true);
                self.concluir(c, Ok(Desfecho::Feito));
                return;
            }
            Comando::Reescanear if !self.servico => {
                self.enviar(Evento::Ocupado(c.clone()));
                self.modo_visualizacao(self.conexao_local, local::calcular);
                self.concluir(c, Ok(Desfecho::Feito));
                return;
            }
            _ => {}
        }
        // Escrita (ou reescanear no serviço): só pelo serviço. O modo de
        // visualização nunca grava nada.
        if !self.servico {
            self.concluir(c, Err(textos::SO_COM_SERVICO.into()));
            return;
        }
        let Some(req) = c.requisicao() else {
            unreachable!("`Atualizar` é tratado acima e é o único comando sem requisição");
        };
        if let Err(e) = req.validar() {
            self.concluir(c, Err(e));
            return;
        }
        self.enviar(Evento::Ocupado(c.clone()));
        let resultado = match self.requisitar(&req) {
            Ok(r) => Ok(Desfecho::da_resposta(&r)),
            Err(Falha::Recusa(m)) => Err(m),
            Err(Falha::Ipc(e)) => Err(mensagem_ipc(&e)),
        };
        let reescaneou = matches!(c, Comando::Reescanear);
        self.atualizar(reescaneou);
        self.concluir(c, resultado);
    }

    fn concluir(&self, comando: Comando, resultado: Result<Desfecho, String>) {
        self.enviar(Evento::Concluido { comando, resultado });
    }
}

// ---------------------------------------------------------------------------
// Ações do Início (spec interface-e-saude.md §5.3): chamadas diretas
// ---------------------------------------------------------------------------

/// Pipe e fila de comandos da thread de comunicação, para as ações do Início
/// pedirem uma atualização dos dados depois de concluídas.
static PIPE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static COMANDOS: std::sync::OnceLock<std::sync::Mutex<Sender<Comando>>> =
    std::sync::OnceLock::new();

fn registrar(pipe: &str, comandos: &Sender<Comando>) {
    let _ = PIPE.set(pipe.to_owned());
    let _ = COMANDOS.set(std::sync::Mutex::new(comandos.clone()));
}

/// Ação do Início.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcaoInicio {
    /// `AplicarRecomendadas` (R-UI-06).
    AplicarRecomendadas,
    /// `RestaurarOriginal`, depois da confirmação (R-UI-07).
    DesfazerTudo,
    /// "Voltar a ajustar": `AplicarAgora` (R-UI-02).
    VoltarAAjustar,
}

impl AcaoInicio {
    pub fn requisicao(self) -> Requisicao {
        match self {
            AcaoInicio::AplicarRecomendadas => Requisicao::AplicarRecomendadas,
            AcaoInicio::DesfazerTudo => Requisicao::RestaurarOriginal,
            AcaoInicio::VoltarAAjustar => Requisicao::AplicarAgora,
        }
    }
}

/// Resultado de uma ação do Início, já resumido para a tela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RespostaInicio {
    /// `adiada` = há jogo aberto; acontece quando ele fechar.
    Feito { aceitas: usize, adiada: bool },
}

/// Por que a ação não aconteceu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalhaInicio {
    /// Serviço ausente: modo de visualização, nada é gravado.
    SemServico,
    /// O serviço não conhece a requisição (`RecursoAusente`).
    RecursoAusente,
    /// Outra falha, já em texto para o usuário.
    Outra(String),
}

/// Traduz um `Resposta::Erro` para a tela: `RecursoAusente` vira a variante
/// própria (serviço antigo); o resto vira o texto de recusa do serviço.
pub fn falha_do_erro(codigo: nvt_ipc::CodigoErro, mensagem: &str) -> FalhaInicio {
    match codigo {
        nvt_ipc::CodigoErro::RecursoAusente => FalhaInicio::RecursoAusente,
        _ => FalhaInicio::Outra(textos::erro_servico(codigo, mensagem)),
    }
}

/// Resume a resposta do serviço para a ação.
pub fn resposta_inicio(r: &Resposta) -> Result<RespostaInicio, FalhaInicio> {
    match r {
        Resposta::Erro { codigo, mensagem } => Err(falha_do_erro(*codigo, mensagem)),
        Resposta::Recomendadas { aceitas, adiada } => Ok(RespostaInicio::Feito {
            aceitas: aceitas.len(),
            adiada: *adiada,
        }),
        outra => Ok(RespostaInicio::Feito {
            aceitas: 0,
            adiada: matches!(Desfecho::da_resposta(outra), Desfecho::Adiado(_)),
        }),
    }
}

/// Executa a ação numa conexão própria (bloqueante; chamar fora da thread
/// da janela) e pede à thread de comunicação que atualize os dados.
pub fn executar_inicio(acao: AcaoInicio) -> Result<RespostaInicio, FalhaInicio> {
    let pipe = PIPE.get().ok_or(FalhaInicio::SemServico)?;
    let cfg = nvt_ipc::pipe::ConfigCliente::default();
    let req = acao.requisicao();
    let resultado = match nvt_ipc::pipe::Cliente::conectar(pipe, &cfg) {
        Ok(mut c) => match c.requisitar(&req) {
            Ok(r) => resposta_inicio(&r),
            Err(e) => Err(falha_inicio(e)),
        },
        Err(e) => Err(falha_inicio(e)),
    };
    if let Some(Ok(tx)) = COMANDOS.get().map(|m| m.lock()) {
        let _ = tx.send(Comando::Atualizar);
    }
    resultado
}

fn falha_inicio(e: ErroIpc) -> FalhaInicio {
    match e {
        ErroIpc::ServicoIndisponivel | ErroIpc::ServidorNaoConfiavel => FalhaInicio::SemServico,
        outro => FalhaInicio::Outra(mensagem_ipc(&outro)),
    }
}

/// Pedido de partidas numa conexão própria (bloqueante; chamar fora da
/// thread da janela). `RecursoAusente` = serviço sem a gravação.
#[cfg(feature = "medicao-partidas")]
pub fn requisitar(req: &Requisicao) -> Result<Resposta, FalhaInicio> {
    let pipe = PIPE.get().ok_or(FalhaInicio::SemServico)?;
    let cfg = nvt_ipc::pipe::ConfigCliente::default();
    let mut c = nvt_ipc::pipe::Cliente::conectar(pipe, &cfg).map_err(falha_inicio)?;
    c.requisitar(req).map_err(falha_inicio)
}

#[cfg(test)]
mod testes_inicio {
    use super::*;
    use nvt_ipc::CodigoErro;

    #[test]
    fn estado_terminal_bloqueia_sem_sondar() {
        let agora = Instant::now();
        assert_eq!(
            proxima_espera(true, false, Some(agora - RECONEXAO), Some(agora), agora),
            Espera::Bloquear
        );
    }

    #[test]
    fn prazo_vencido_e_processado_uma_vez() {
        let agora = Instant::now();
        let (tx, rx) = mpsc::channel();
        let mut trabalhador = Trabalhador::new(String::new(), tx);
        trabalhador.conexao_local = Conexao::Iniciando;
        trabalhador.primeira_elegivel = Some(agora - PRAZO_INICIANDO);
        trabalhador.ultima_tentativa = Some(agora);
        let espera = |t: &Trabalhador| {
            proxima_espera(false, false, t.ultima_tentativa, t.prazo_iniciando(), agora)
        };

        assert_eq!(espera(&trabalhador), Espera::Atualizar);
        trabalhador.atualizar(false);
        assert!(matches!(
            rx.try_recv(),
            Ok(Evento::Conexao(Conexao::SemResposta))
        ));
        assert_eq!(espera(&trabalhador), Espera::Aguardar(RECONEXAO));
    }

    #[test]
    fn publica_queda_antes_de_calcular_dados_locais() {
        let (tx, rx) = mpsc::channel();
        let mut trabalhador = Trabalhador::new(String::new(), tx);
        trabalhador.servico = true;
        let mut ui = crate::modelo::EstadoUi::new();
        ui.aplicar(Evento::Servico(Dados::default()));
        assert!(ui.pode_escrever());

        trabalhador.modo_visualizacao(Conexao::SemResposta, || {
            let evento = rx.try_recv().expect("diagnóstico antes do cálculo");
            assert!(matches!(evento, Evento::Conexao(Conexao::SemResposta)));
            ui.aplicar(evento);
            assert!(!ui.pode_escrever());
            Ok(Dados::default())
        });

        assert!(matches!(
            rx.try_recv(),
            Ok(Evento::Visualizacao(Conexao::SemResposta, Ok(_)))
        ));
    }

    #[test]
    fn calculo_local_longo_reavalia_prazo_e_reconecta_sem_espera() {
        let inicio = Instant::now();
        let fim = inicio + PRAZO_INICIANDO + RECONEXAO;
        let (tx, _) = mpsc::channel();
        let mut trabalhador = Trabalhador::new(String::new(), tx);
        trabalhador.conexao_local = Conexao::Iniciando;
        trabalhador.primeira_elegivel = Some(inicio);
        trabalhador.ultima_tentativa = Some(inicio);

        assert_eq!(trabalhador.diagnostico_local(inicio), Conexao::Iniciando);
        trabalhador.conexao_local = trabalhador.diagnostico_local(fim);
        assert_eq!(trabalhador.conexao_local, Conexao::SemResposta);
        assert_eq!(
            proxima_espera(
                false,
                false,
                trabalhador.ultima_tentativa,
                trabalhador.prazo_iniciando(),
                fim,
            ),
            Espera::Atualizar
        );
    }

    #[test]
    fn diagnostico_do_scm_por_falha_e_prazo() {
        let ausente = Falha::Ipc(ErroIpc::ServicoIndisponivel);
        let timeout = Falha::Ipc(io::Error::from(ErrorKind::TimedOut).into());
        let io = Falha::Ipc(io::Error::from(ErrorKind::PermissionDenied).into());
        let inseguro = Falha::Ipc(ErroIpc::ServidorNaoConfiavel);
        let recusado = Falha::Recusa(String::new());
        let servico = |estado, inicio, saida| ConsultaScm::Instalado {
            estado,
            inicio,
            saida,
        };
        let automatico = |estado| servico(estado, SERVICE_AUTO_START, 0);
        let agora = Instant::now();
        let antes_boot = JANELA_BOOT - Duration::from_millis(1);
        let antes_prazo = PRAZO_INICIANDO - Duration::from_millis(1);
        use Conexao::{Iniciando, SemResposta, Visualizacao};

        for (nome, falha, scm, uptime, decorrido, esperado) in [
            (
                "ausência confirmada",
                &ausente,
                ConsultaScm::Ausente,
                JANELA_BOOT,
                None,
                Visualizacao,
            ),
            (
                "ausência com outro erro IPC",
                &io,
                ConsultaScm::Ausente,
                JANELA_BOOT,
                None,
                Visualizacao,
            ),
            (
                "consulta falhou",
                &ausente,
                ConsultaScm::Falhou,
                antes_boot,
                None,
                SemResposta,
            ),
            (
                "start pending",
                &ausente,
                automatico(SERVICE_START_PENDING),
                JANELA_BOOT,
                None,
                Iniciando,
            ),
            (
                "running com timeout",
                &timeout,
                automatico(SERVICE_RUNNING),
                JANELA_BOOT,
                None,
                Iniciando,
            ),
            (
                "parado automático antes de 300 s",
                &ausente,
                automatico(SERVICE_STOPPED),
                antes_boot,
                None,
                Iniciando,
            ),
            (
                "parado automático aos 300 s",
                &ausente,
                automatico(SERVICE_STOPPED),
                JANELA_BOOT,
                Some(antes_prazo),
                SemResposta,
            ),
            (
                "parado com erro",
                &ausente,
                servico(SERVICE_STOPPED, SERVICE_AUTO_START, 1),
                antes_boot,
                None,
                SemResposta,
            ),
            (
                "parado manual",
                &ausente,
                servico(SERVICE_STOPPED, SERVICE_DEMAND_START, 0),
                antes_boot,
                None,
                SemResposta,
            ),
            (
                "parado desabilitado",
                &ausente,
                servico(SERVICE_STOPPED, SERVICE_DISABLED, 0),
                antes_boot,
                None,
                SemResposta,
            ),
            (
                "stop pending",
                &ausente,
                automatico(SERVICE_STOP_PENDING),
                antes_boot,
                None,
                SemResposta,
            ),
            (
                "erro de E/S",
                &io,
                automatico(SERVICE_RUNNING),
                antes_boot,
                None,
                SemResposta,
            ),
            (
                "pipe não confiável",
                &inseguro,
                automatico(SERVICE_RUNNING),
                antes_boot,
                None,
                SemResposta,
            ),
            (
                "recusa do serviço",
                &recusado,
                automatico(SERVICE_RUNNING),
                antes_boot,
                None,
                SemResposta,
            ),
            (
                "antes de 60 s",
                &ausente,
                automatico(SERVICE_RUNNING),
                JANELA_BOOT,
                Some(antes_prazo),
                Iniciando,
            ),
            (
                "aos 60 s",
                &ausente,
                automatico(SERVICE_RUNNING),
                JANELA_BOOT,
                Some(PRAZO_INICIANDO),
                SemResposta,
            ),
            (
                "depois de 60 s",
                &ausente,
                automatico(SERVICE_RUNNING),
                JANELA_BOOT,
                Some(PRAZO_INICIANDO + RECONEXAO),
                SemResposta,
            ),
        ] {
            let primeira = decorrido.map(|d| agora - d);
            let (conexao, observacao) = classificar(falha, scm, uptime, primeira, agora);
            assert_eq!(conexao, esperado, "{nome}");
            let observacao_esperada = primeira.or((esperado == Iniciando).then_some(agora));
            assert_eq!(observacao, observacao_esperada, "{nome}");
        }
    }

    #[test]
    fn saude_so_de_resposta_valida() {
        let s = crate::demonstracao::Cenario::Saude.saude().unwrap();
        assert_eq!(
            saude_da_resposta(Resposta::Saude(s.clone())),
            Some(s.clone())
        );
        let mut invalida = s;
        invalida.sinais.push(invalida.sinais[0]);
        assert_eq!(saude_da_resposta(Resposta::Saude(invalida)), None);
        assert_eq!(saude_da_resposta(Resposta::Ok), None);
    }

    #[test]
    fn requisicoes_das_acoes() {
        assert_eq!(
            AcaoInicio::AplicarRecomendadas.requisicao(),
            Requisicao::AplicarRecomendadas
        );
        assert_eq!(
            AcaoInicio::DesfazerTudo.requisicao(),
            Requisicao::RestaurarOriginal
        );
        assert_eq!(
            AcaoInicio::VoltarAAjustar.requisicao(),
            Requisicao::AplicarAgora
        );
    }

    #[test]
    fn recomendadas_e_recurso_ausente() {
        let r = Resposta::Recomendadas {
            aceitas: vec!["a".into(), "b".into()],
            adiada: true,
        };
        assert_eq!(
            resposta_inicio(&r),
            Ok(RespostaInicio::Feito {
                aceitas: 2,
                adiada: true
            })
        );
        let e = Resposta::erro(CodigoErro::RecursoAusente, "ainda não implementado");
        assert_eq!(resposta_inicio(&e), Err(FalhaInicio::RecursoAusente));
        assert!(matches!(
            resposta_inicio(&Resposta::erro(CodigoErro::Interno, "x")),
            Err(FalhaInicio::Outra(_))
        ));
        assert_eq!(
            resposta_inicio(&Resposta::Restauracao { adiada: None }),
            Ok(RespostaInicio::Feito {
                aceitas: 0,
                adiada: false
            })
        );
    }
}
