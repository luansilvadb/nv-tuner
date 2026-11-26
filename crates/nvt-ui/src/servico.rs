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
use crate::modelo::{Comando, Dados, Desfecho, Evento, grupos_do_plano, upgrade_detectado};
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
    /// Último estado lido do serviço (para reconhecer a queda num upgrade).
    ultimo_estado: Option<nvt_ipc::Estado>,
    /// O nv-tuner foi atualizado (U-42): não reconecta mais.
    encerrado: bool,
}

impl Trabalhador {
    fn new(pipe: String, eventos: Sender<Evento>) -> Self {
        Self {
            pipe,
            eventos,
            servico: false,
            ultima_tentativa: None,
            ultimo_estado: None,
            encerrado: false,
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
        })
    }

    /// Atualiza os dados. No modo de visualização, só tenta reconectar a
    /// cada [`RECONEXAO`]; o cálculo local é refeito apenas quando `forcar`.
    fn atualizar(&mut self, forcar: bool) {
        if self.encerrado {
            return;
        }
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
        if self.upgrade(&resultado) {
            self.encerrado = true;
            self.servico = false;
            self.enviar(Evento::Atualizado);
            return;
        }
        if let Err(Falha::Ausente(m)) = &resultado {
            self.enviar(Evento::Incompativel(m.clone()));
        }
        match resultado {
            Ok(d) => {
                self.servico = true;
                self.ultimo_estado = d.estado.clone();
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
            Err(Falha::Ausente(Some(_))) if self.servico => true,
            Err(Falha::Ausente(_) | Falha::Comunicacao(_)) if self.servico => {
                upgrade_detectado(self.ultimo_estado.as_ref(), None, versao)
            }
            Err(_) => false,
        }
    }

    /// Calcula os dados localmente, somente leitura.
    fn modo_visualizacao(&mut self) {
        self.servico = false;
        self.enviar(Evento::Visualizacao(local::calcular()));
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
            Comando::DefinirAceiteExperimental { regra, aceito } => {
                Requisicao::DefinirAceiteExperimental {
                    regra: regra.clone(),
                    aceito: *aceito,
                }
            }
            Comando::ReverterAjusteUsuario { destino } => Requisicao::ReverterAjusteUsuario {
                alvo_tela: match destino {
                    nvt_ipc::SettingUsuario::Refresh { alvo } => Some(alvo.clone()),
                    _ => None,
                },
                destino: destino.clone(),
            },
            Comando::ReverterConfigJogo { destino } => Requisicao::ReverterConfigJogo {
                destino: destino.clone(),
            },
            Comando::MarcarAtualizacaoVista { versao } => Requisicao::MarcarAtualizacaoVista {
                versao: versao.clone(),
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

/// Resume a resposta do serviço para a ação.
pub fn resposta_inicio(r: &Resposta) -> Result<RespostaInicio, FalhaInicio> {
    match r {
        Resposta::Erro {
            codigo: nvt_ipc::CodigoErro::RecursoAusente,
            ..
        } => Err(FalhaInicio::RecursoAusente),
        Resposta::Erro { codigo, mensagem } => {
            Err(FalhaInicio::Outra(textos::erro_servico(*codigo, mensagem)))
        }
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
    match falha_de(e) {
        Falha::Ausente(None) => FalhaInicio::SemServico,
        Falha::Ausente(Some(m)) | Falha::Comunicacao(m) | Falha::Recusa(m) => FalhaInicio::Outra(m),
    }
}

#[cfg(test)]
mod testes_inicio {
    use super::*;
    use nvt_ipc::CodigoErro;

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
