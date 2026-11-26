//! Thread de fundo que conversa com o serviço (cliente IPC bloqueante) e,
//! quando ele está ausente, monta o modo de visualização local.
//!
//! A UI envia [`Comando`]s e recebe [`Evento`]s por canais; nada aqui roda
//! na thread da janela.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use nvt_ipc::{ErroIpc, Requisicao, Resposta};

use crate::local;
use crate::modelo::{Comando, Dados, Desfecho, Evento, grupos_do_plano};
use crate::textos;

/// Intervalo entre tentativas de conexão no modo de visualização.
const RECONEXAO: Duration = Duration::from_secs(3);
/// Intervalo de atualização automática com o serviço conectado.
const ATUALIZACAO: Duration = Duration::from_secs(15);
/// Entradas do histórico pedidas ao serviço.
const HISTORICO: u32 = 200;

/// Lado da UI dos canais.
pub struct Canal {
    pub comandos: Sender<Comando>,
    pub eventos: Receiver<Evento>,
}

/// Inicia a thread de comunicação com o pipe `pipe`.
pub fn iniciar(pipe: String) -> Canal {
    let (tx_cmd, rx_cmd) = mpsc::channel();
    let (tx_ev, rx_ev) = mpsc::channel();
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
    /// Serviço ausente (ou não confiável/incompatível): modo de visualização.
    /// Com versão de protocolo diferente, traz a instrução de atualizar.
    Ausente(Option<String>),
    /// Erro de transporte com o serviço presente.
    Comunicacao(String),
    /// Erro de negócio (o serviço recusou).
    Recusa(String),
}

fn falha_de(e: ErroIpc) -> Falha {
    match e {
        ErroIpc::ServicoIndisponivel | ErroIpc::ServidorNaoConfiavel => Falha::Ausente(None),
        ErroIpc::VersaoIncompativel { recebida, esperada } => {
            Falha::Ausente(Some(textos::versao_incompativel(recebida, esperada)))
        }
        outro => Falha::Comunicacao(textos::erro_comunicacao(&outro.to_string())),
    }
}

struct Trabalhador {
    pipe: String,
    eventos: Sender<Evento>,
    servico: bool,
    ultima_tentativa: Option<Instant>,
}

impl Trabalhador {
    fn new(pipe: String, eventos: Sender<Evento>) -> Self {
        Self {
            pipe,
            eventos,
            servico: false,
            ultima_tentativa: None,
        }
    }

    fn enviar(&self, ev: Evento) {
        // A janela fechou: nada a fazer.
        let _ = self.eventos.send(ev);
    }

    fn executar(mut self, comandos: &Receiver<Comando>) {
        self.atualizar(true);
        loop {
            let espera = if self.servico { ATUALIZACAO } else { RECONEXAO };
            match comandos.recv_timeout(espera) {
                Ok(c) => self.tratar(c),
                Err(RecvTimeoutError::Timeout) => self.atualizar(false),
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    /// Uma requisição numa conexão nova (o servidor atende em sequência).
    fn requisitar(&self, req: &Requisicao) -> Result<Resposta, Falha> {
        let cfg = nvt_ipc::pipe::ConfigCliente::default();
        let mut c = nvt_ipc::pipe::Cliente::conectar(&self.pipe, &cfg).map_err(falha_de)?;
        match c.requisitar(req).map_err(falha_de)? {
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
        let (grupos, respeitadas, reversoes, ignorados, jogos) =
            match self.requisitar(&Requisicao::ObterPlano)? {
                Resposta::Plano(p) => {
                    let (g, r) = grupos_do_plano(&p);
                    (g, r, p.reversoes, p.ignorados, p.jogos)
                }
                _ => Default::default(),
            };
        let historico = match self.requisitar(&Requisicao::ListarHistorico { limite: HISTORICO })? {
            Resposta::Historico { entradas } => entradas,
            _ => Vec::new(),
        };
        Ok(Dados {
            estado,
            grupos,
            respeitadas,
            historico,
            reversoes,
            ignorados,
            jogos,
        })
    }

    /// Atualiza os dados. No modo de visualização, só tenta reconectar a
    /// cada [`RECONEXAO`]; o cálculo local é refeito apenas quando `forcar`.
    fn atualizar(&mut self, forcar: bool) {
        if !self.servico
            && !forcar
            && self
                .ultima_tentativa
                .is_some_and(|t| t.elapsed() < RECONEXAO)
        {
            return;
        }
        self.ultima_tentativa = Some(Instant::now());
        let resultado = self.buscar();
        if let Err(Falha::Ausente(m)) = &resultado {
            self.enviar(Evento::Incompativel(m.clone()));
        }
        match resultado {
            Ok(d) => {
                self.servico = true;
                self.enviar(Evento::Servico(d));
            }
            Err(Falha::Ausente(_)) | Err(Falha::Comunicacao(_)) | Err(Falha::Recusa(_))
                if !self.servico && !forcar =>
            {
                // Continua no modo de visualização; tenta de novo depois.
            }
            Err(_) => self.modo_visualizacao(),
        }
    }

    /// Calcula os dados localmente, somente leitura.
    fn modo_visualizacao(&mut self) {
        self.servico = false;
        self.enviar(Evento::Visualizacao(local::calcular()));
    }

    fn tratar(&mut self, c: Comando) {
        match &c {
            Comando::Atualizar => {
                self.enviar(Evento::Ocupado(c.clone()));
                self.atualizar(true);
                self.concluir(c, Ok(Desfecho::Feito));
                return;
            }
            Comando::Reescanear if !self.servico => {
                self.enviar(Evento::Ocupado(c.clone()));
                self.modo_visualizacao();
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
        let req = match &c {
            Comando::AplicarAgora => Requisicao::AplicarAgora,
            Comando::RestaurarOriginal => Requisicao::RestaurarOriginal,
            Comando::Reescanear => Requisicao::ReescanearJogos,
            Comando::DefinirPreferencias(p) => Requisicao::DefinirPreferencias {
                sincronismo: p.sincronismo,
                qualidade: p.qualidade,
                limite_fps: p.limite_fps,
            },
            Comando::Destravar { perfil, setting } => Requisicao::DestravarSetting {
                perfil: perfil.clone(),
                setting: setting.clone(),
            },
            Comando::Atualizar => unreachable!("tratado acima"),
        };
        if let Err(e) = req.validar() {
            self.concluir(c, Err(e));
            return;
        }
        self.enviar(Evento::Ocupado(c.clone()));
        let resultado = match self.requisitar(&req) {
            Ok(r) => Ok(Desfecho::da_resposta(&r)),
            Err(Falha::Recusa(m)) | Err(Falha::Comunicacao(m)) => Err(m),
            Err(Falha::Ausente(m)) => Err(m.unwrap_or_else(|| {
                textos::erro_comunicacao(&ErroIpc::ServicoIndisponivel.to_string())
            })),
        };
        let reescaneou = matches!(c, Comando::Reescanear);
        self.atualizar(reescaneou);
        self.concluir(c, resultado);
    }

    fn concluir(&self, comando: Comando, resultado: Result<Desfecho, String>) {
        self.enviar(Evento::Concluido { comando, resultado });
    }
}
