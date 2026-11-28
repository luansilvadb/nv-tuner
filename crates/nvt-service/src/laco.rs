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

#[derive(Default)]
pub(crate) struct Integracao {
    pub(crate) usuario: Option<Coordenador>,
    /// Portão de processos único do serviço (núcleo e atualizador).
    pub(crate) portao: nvt_engine::Portao,
    /// Extras atuais do portão, vistos também pelo atualizador.
    pub(crate) extras_portao: Arc<Mutex<Vec<String>>>,
    pub(crate) exclusao: Arc<Exclusao>,
    pub(crate) atualizacao: Option<ThreadAtualizacao>,
    sessoes_agente: Option<Vec<nvt_atualizacao::SessaoAgente>>,
    diagnosticos: Arc<Mutex<Vec<nvt_ipc::Diagnostico>>>,
    regras_drs: Option<nvt_core::ConjuntoRegras>,
    pub(crate) console: Option<IdentidadeCliente>,
    aceites_drs: std::collections::BTreeSet<String>,
    /// F3-13 do SID da sessão de console (ver `Coordenador::limites_no_jogo_efetivos`).
    limites_jogo: std::collections::BTreeMap<nvt_core::IdJogo, String>,
    /// Cache das leituras de máquina (escrito só pela thread de saúde).
    pub(crate) saude_cache: Arc<Mutex<crate::saude::SaudeMaquina>>,
    saude: Option<crate::saude::ThreadSaude>,
    /// Partidas (`medicao-partidas`; sem a feature, só responde `RecursoAusente`).
    pub(crate) partidas: crate::partidas::Partidas,
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn pipe_sem_driver_vincula_agente_e_ui_e_retorna_diagnosticos() {
        use nvt_ipc::pipe::{Cliente, ConfigCliente};

        let estado = crate::teste_util::DirTemp::novo("sem-driver");
        let pipe = format!(r"\\.\pipe\nvt-servico-sem-driver-{}", std::process::id());
        let (tx, rx) = mpsc::channel();
        let (parada, ipc) = iniciar_ipc(&pipe, tx.clone()).unwrap();
        let op = Opcoes {
            pipe: pipe.clone(),
            estado_dir: estado.0.clone(),
            vigiar: None,
            config: Config::default(),
        };
        let motor = thread::spawn(move || {
            let regras = nvt_core::ConjuntoRegras::embutido().unwrap();
            let mut integracao = Integracao {
                usuario: Some(crate::teste_util::coordenador(
                    &crate::teste_util::Memoria::default(),
                )),
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
                config_jogo: vec![],
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
        crate::teste_util::cliente("S-1-5-21-1", 42)
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

mod despacho;
mod exclusao;
mod execucao;
mod integracao;
mod thread_atualizacao;

pub(crate) use despacho::*;
pub(crate) use exclusao::*;
pub(crate) use execucao::*;
pub(crate) use integracao::*;
pub(crate) use thread_atualizacao::*;
