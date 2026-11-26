//! Modo de demonstração (`--demonstracao <cenario>`): a janela recebe dados
//! FICTÍCIOS fixos, sem serviço e sem ler a máquina.
//!
//! Garantias (testadas abaixo):
//!
//! - nada aqui abre o named pipe nem chama o cálculo local;
//! - quando o modo está ligado, `main` não inicia a thread do serviço, e as
//!   ações do Início e da tela Jogos recebem um resultado simulado;
//! - os dados não têm nome de usuário, caminho real nem conta: só jogos
//!   conhecidos, uma placa genérica e caminhos anonimizados.

use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use nvt_ipc::{
    AcaoAdiada, Adiamento, Aplicacao, AtualizacaoConcluida, Camada, Capacidades, Driver,
    EstadoAgente, EstadoAtualizacao, EstadoExperimental, ImpactoVisual, Jogo, Mudanca,
    OrigemPerfil, Plano, PlanoPerfil, Preferencias, Qualidade, RegraAplicada, ResumoJogo,
    ResumoSistema, Sincronismo, StatusMudanca, Vrr,
};
use nvt_ipc::{
    AlvoIpc, ArquivoJogoUi, ConfigJogoUi, EstadoBackup, MudancaArquivoJogo, RotuloCaminho,
    ValorJogoIpc,
};

use crate::modelo::{Comando, Dados, Desfecho, Evento, Saude, TipoSinal, grupos_do_plano};
use crate::servico::{AcaoInicio, Canal, FalhaInicio, RespostaInicio};

/// Cenários de demonstração.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cenario {
    Otimizado,
    Melhorias,
    Piorou,
    JogoAberto,
    Desligado,
    Atualizado,
    SemServico,
    PrimeiraVez,
}

impl Cenario {
    pub const TODOS: [Cenario; 8] = [
        Cenario::Otimizado,
        Cenario::Melhorias,
        Cenario::Piorou,
        Cenario::JogoAberto,
        Cenario::Desligado,
        Cenario::Atualizado,
        Cenario::SemServico,
        Cenario::PrimeiraVez,
    ];

    /// Chave de `--demonstracao`.
    pub fn chave(self) -> &'static str {
        match self {
            Cenario::Otimizado => "otimizado",
            Cenario::Melhorias => "melhorias",
            Cenario::Piorou => "piorou",
            Cenario::JogoAberto => "jogo-aberto",
            Cenario::Desligado => "desligado",
            Cenario::Atualizado => "atualizado",
            Cenario::SemServico => "sem-servico",
            Cenario::PrimeiraVez => "primeira-vez",
        }
    }

    pub fn de_chave(s: &str) -> Option<Cenario> {
        Cenario::TODOS.into_iter().find(|c| c.chave() == s)
    }

    /// Lista para a mensagem de erro da linha de comando.
    pub fn chaves() -> String {
        Cenario::TODOS.map(Cenario::chave).join(", ")
    }

    /// Sinais de saúde fictícios (só `piorou`).
    pub fn saude(self) -> Option<Saude> {
        (self == Cenario::Piorou).then(|| Saude {
            em_atencao: vec![TipoSinal::EspacoLivre],
        })
    }

    /// As boas-vindas aparecem?
    pub fn boas_vindas(self) -> bool {
        self == Cenario::PrimeiraVez
    }
}

static ATIVO: OnceLock<Cenario> = OnceLock::new();

/// Liga o modo de demonstração para o processo inteiro (uma vez).
pub fn ativar(c: Cenario) {
    let _ = ATIVO.set(c);
}

/// Cenário ativo, se o modo de demonstração estiver ligado.
pub fn ativo() -> Option<Cenario> {
    ATIVO.get().copied()
}

// ---------------------------------------------------------------------------
// Canal simulado
// ---------------------------------------------------------------------------

/// Canal no lugar do serviço: a thread responde a cada comando com um
/// resultado simulado sobre os dados fictícios. Nunca abre o pipe.
pub fn iniciar(c: Cenario) -> Canal {
    let (tx_cmd, rx_cmd) = mpsc::channel();
    let (tx_ev, rx_ev) = mpsc::channel();
    thread::Builder::new()
        .name("nvt-ui-demonstracao".into())
        .spawn(move || executar(c, &rx_cmd, &tx_ev))
        .expect("falha ao criar a thread de demonstração");
    Canal {
        comandos: tx_cmd,
        eventos: rx_ev,
    }
}

fn evento_dados(c: Cenario, d: Dados) -> Evento {
    if c == Cenario::SemServico {
        Evento::Visualizacao(Ok(d))
    } else {
        Evento::Servico(d)
    }
}

fn executar(c: Cenario, comandos: &Receiver<Comando>, eventos: &Sender<Evento>) {
    let mut d = dados(c);
    let _ = eventos.send(evento_dados(c, d.clone()));
    while let Ok(cmd) = comandos.recv() {
        for ev in responder(c, &mut d, cmd) {
            if eventos.send(ev).is_err() {
                return;
            }
        }
    }
}

/// Resposta simulada a um comando (pura; testada).
pub fn responder(c: Cenario, d: &mut Dados, cmd: Comando) -> Vec<Evento> {
    let resultado = if c == Cenario::SemServico && cmd.escreve() {
        Err(crate::textos::SO_COM_SERVICO.to_owned())
    } else {
        if let Some(e) = d.estado.as_mut() {
            match &cmd {
                Comando::RestaurarOriginal => e.autonomia_pausada = true,
                Comando::AplicarAgora => e.autonomia_pausada = false,
                Comando::MarcarAtualizacaoVista { .. } => e.atualizacao.nao_vista = false,
                _ => {}
            }
        }
        Ok(
            match (&cmd, d.estado.as_ref().and_then(|e| e.adiamento.clone())) {
                (Comando::AplicarAgora | Comando::RestaurarOriginal, Some(a)) => {
                    Desfecho::Adiado(a)
                }
                _ => Desfecho::Feito,
            },
        )
    };
    vec![
        Evento::Ocupado(cmd.clone()),
        evento_dados(c, d.clone()),
        Evento::Concluido {
            comando: cmd,
            resultado,
        },
    ]
}

/// Resultado simulado das ações do Início.
pub fn executar_inicio(c: Cenario, acao: AcaoInicio) -> Result<RespostaInicio, FalhaInicio> {
    if c == Cenario::SemServico {
        return Err(FalhaInicio::SemServico);
    }
    let adiada = c == Cenario::JogoAberto;
    let aceitas = match acao {
        AcaoInicio::AplicarRecomendadas => MELHORIAS.len(),
        _ => 0,
    };
    Ok(RespostaInicio::Feito { aceitas, adiada })
}

// ---------------------------------------------------------------------------
// Dados fictícios
// ---------------------------------------------------------------------------

/// Instante fixo dos dados (não depende do relógio).
const QUANDO: u64 = 1_795_000_000;
const VERSAO: &str = env!("CARGO_PKG_VERSION");

/// Melhorias recomendadas do cenário `melhorias`: (setting, nome, depois).
const MELHORIAS: [(&str, &str, &str); 3] = [
    (
        "shader_cache_size",
        "Tamanho do cache de sombreadores",
        "10 GB",
    ),
    (
        "texture_filter_quality",
        "Qualidade da filtragem de texturas",
        "Alto desempenho",
    ),
    (
        "power_mode",
        "Modo de gerenciamento de energia",
        "Desempenho máximo",
    ),
];

fn mudanca(setting: &str, nome: &str, antes: &str, depois: &str, status: StatusMudanca) -> Mudanca {
    Mudanca {
        alvo_tela: None,
        camada: Camada::Driver,
        experimental: EstadoExperimental::Estavel,
        setting: setting.into(),
        nome_legivel: nome.into(),
        antes: Some(antes.into()),
        depois: Some(depois.into()),
        motivo: "Menos atraso entre o clique e a tela.".into(),
        regra: "demonstracao".into(),
        impacto_visual: ImpactoVisual::Nenhum,
        status,
    }
}

fn aplicada(regra: &str, descricao: &str) -> RegraAplicada {
    RegraAplicada {
        regra: regra.into(),
        descricao: descricao.into(),
        settings: vec![],
    }
}

#[allow(clippy::too_many_arguments)]
fn jogo(
    nome: &str,
    perfil: &str,
    launcher: &str,
    api: &str,
    competitivo: Option<&str>,
    anticheat: bool,
    orientacoes: &[&str],
    aplicadas: Vec<RegraAplicada>,
) -> Jogo {
    Jogo {
        nome: nome.into(),
        perfil: perfil.into(),
        origem_perfil: OrigemPerfil::Driver,
        launcher: launcher.into(),
        api: api.into(),
        dlss: None,
        reflex: Some(true),
        anticheat_kernel: Some(anticheat),
        competitivo: competitivo.map(str::to_owned),
        em_execucao: false,
        orientacoes: orientacoes.iter().map(|s| (*s).to_owned()).collect(),
        resumo: ResumoJogo {
            aplicadas,
            ..Default::default()
        },
        config_jogo: None,
    }
}

fn jogos(c: Cenario) -> Vec<Jogo> {
    let mut cs2 = jogo(
        "Counter-Strike 2",
        "Counter-Strike 2",
        "Steam",
        "dx11",
        Some("cs2"),
        false,
        &["reflex_on_boost", "tela_cheia"],
        vec![
            aplicada("demo-latencia", "Menos atraso: fila de quadros curta"),
            aplicada(
                "demo-energia",
                "Placa de vídeo em desempenho máximo durante o jogo",
            ),
        ],
    );
    cs2.config_jogo = Some(ConfigJogoUi {
        arquivos: vec![ArquivoJogoUi {
            alvo: AlvoIpc::ConfigJogo {
                jogo: "cs2".into(),
                arquivo: "video".into(),
            },
            rotulo: RotuloCaminho("%STEAM%\\steamapps\\common\\cs2\\cfg\\cs2_video.txt".into()),
            instancia: "padrao".into(),
            conta: None,
            backup: EstadoBackup::Integro,
            mudancas: vec![MudancaArquivoJogo {
                setting: "low_latency_enabled".into(),
                antes: ValorJogoIpc::Texto("desligada".into()),
                depois: ValorJogoIpc::Texto("ligada".into()),
                status: StatusMudanca::Aplicada,
                motivo: None,
            }],
            reversivel: true,
        }],
        ..Default::default()
    });
    let mut valorant = jogo(
        "VALORANT",
        "VALORANT",
        "Riot",
        "dx11",
        Some("valorant"),
        true,
        &[],
        vec![aplicada(
            "demo-latencia",
            "Menos atraso: fila de quadros curta",
        )],
    );
    valorant.em_execucao = c == Cenario::JogoAberto;
    vec![
        cs2,
        valorant,
        jogo(
            "Rocket League",
            "Rocket League",
            "Epic",
            "dx11",
            Some("rocket-league"),
            false,
            &[],
            vec![aplicada(
                "demo-latencia",
                "Menos atraso: fila de quadros curta",
            )],
        ),
        jogo(
            "Rainbow Six Siege",
            "Rainbow Six Siege",
            "Ubisoft",
            "vulkan",
            Some("r6"),
            true,
            &["reflex_on"],
            vec![aplicada(
                "demo-latencia",
                "Menos atraso: fila de quadros curta",
            )],
        ),
        jogo(
            "Dota 2",
            "Dota 2",
            "Steam",
            "vulkan",
            None,
            false,
            &[],
            vec![],
        ),
    ]
}

fn plano(c: Cenario, jogos: Vec<Jogo>) -> Plano {
    let status = if c == Cenario::JogoAberto {
        StatusMudanca::Pendente
    } else {
        StatusMudanca::Aplicada
    };
    let mut global = vec![
        mudanca(
            "low_latency",
            "Modo de baixa latência",
            "Desligado",
            "Ativado",
            status,
        ),
        mudanca(
            "max_prerendered",
            "Quadros pré-renderizados",
            "Automático",
            "1",
            StatusMudanca::Aplicada,
        ),
        mudanca(
            "vsync",
            "Sincronização vertical",
            "Usar a do jogo",
            "Ativada",
            StatusMudanca::JaNoValor,
        ),
    ];
    if c == Cenario::Melhorias {
        global.extend(MELHORIAS.iter().map(|(s, n, d)| Mudanca {
            experimental: EstadoExperimental::Aceitavel,
            ..mudanca(s, n, "Padrão do driver", d, StatusMudanca::Pendente)
        }));
    }
    let perfis = vec![
        PlanoPerfil {
            perfil: "global".into(),
            nome_legivel: "Todos os jogos".into(),
            mudancas: global,
        },
        PlanoPerfil {
            perfil: "Counter-Strike 2".into(),
            nome_legivel: "Counter-Strike 2".into(),
            mudancas: vec![mudanca(
                "frame_limiter",
                "Limite de quadros",
                "Desligado",
                "162",
                StatusMudanca::Aplicada,
            )],
        },
    ];
    Plano {
        sistema: ResumoSistema::default(),
        sugestoes_driver: vec![],
        gerado_unix: QUANDO,
        perfis,
        reversoes: vec![],
        ignorados: vec![],
        jogos,
        adiada: None,
    }
}

fn estado(c: Cenario, jogos: u32) -> nvt_ipc::Estado {
    nvt_ipc::Estado {
        agente: EstadoAgente {
            presente: true,
            restauracao_pendente: false,
        },
        diagnosticos: vec![],
        versao_servico: VERSAO.into(),
        driver: Some(Driver {
            versao: "581.57".into(),
            ramo: Some("r580_00".into()),
        }),
        gpu: Some(nvt_ipc::Gpu {
            nome: "NVIDIA GeForce RTX 4070".into(),
            arquitetura: "ada".into(),
            vram_mb: 12_282,
            capacidades: Capacidades {
                dlss: true,
                geracao_quadros: true,
                reflex: true,
                rebar: true,
                refresh_mhz: Some(165_000),
                vrr: Vrr {
                    possivel: Some(true),
                    ativo: Some(true),
                },
            },
        }),
        preferencias: Preferencias {
            sincronismo: Sincronismo::GSync,
            qualidade: Qualidade::Equilibrado,
            limite_fps: Some(162),
        },
        jogos,
        ultima_aplicacao: Some(Aplicacao {
            quando_unix: QUANDO,
            mudancas: 6,
            falhas: 0,
        }),
        alertas_deriva: vec![],
        avisos: vec![],
        adiamento: (c == Cenario::JogoAberto).then(|| Adiamento {
            processos: vec!["VALORANT-Win64-Shipping.exe".into()],
            desde_unix: QUANDO,
            acao: AcaoAdiada::AplicarPlano,
            lista_indisponivel: false,
        }),
        atualizacao: EstadoAtualizacao {
            versao_instalada: VERSAO.into(),
            ultima_verificacao_unix: Some(QUANDO),
            proxima_verificacao_unix: Some(QUANDO + 86_400),
            ultima_atualizacao: (c == Cenario::Atualizado).then(|| AtualizacaoConcluida {
                de: "0.1.9".into(),
                para: VERSAO.into(),
                quando_unix: QUANDO,
                reinicio_pendente: false,
            }),
            nao_vista: c == Cenario::Atualizado,
            pendente: None,
            falha: None,
            erro_verificacao: None,
        },
        autonomia_pausada: c == Cenario::Desligado,
    }
}

/// Dados fictícios completos do cenário.
pub fn dados(c: Cenario) -> Dados {
    let js = jogos(c);
    let p = plano(c, js);
    let (grupos, respeitadas) = grupos_do_plano(&p);
    Dados {
        estado: Some(estado(c, p.jogos.len() as u32)),
        sistema: p.sistema,
        sugestoes_driver: p.sugestoes_driver,
        grupos,
        respeitadas,
        historico: vec![],
        reversoes: p.reversoes,
        ignorados: p.ignorados,
        jogos: p.jogos,
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::modelo::{Conexao, EstadoUi, Fatos, Tela, Veredito, veredito};

    fn ui(c: Cenario) -> EstadoUi {
        let mut ui = EstadoUi::new(Tela::VisaoGeral);
        let canal = iniciar(c);
        ui.aplicar(canal.eventos.recv().expect("primeiro evento"));
        ui
    }

    #[test]
    fn chaves_dos_cenarios() {
        for c in Cenario::TODOS {
            assert_eq!(Cenario::de_chave(c.chave()), Some(c));
        }
        assert_eq!(Cenario::de_chave("nada"), None);
    }

    #[test]
    fn cada_cenario_tem_o_veredito_esperado() {
        for c in Cenario::TODOS {
            let ui = ui(c);
            let v = veredito(&Fatos::de_ui(&ui, false), c.saude().as_ref());
            let ok = match c {
                Cenario::Otimizado | Cenario::Atualizado | Cenario::PrimeiraVez => {
                    v == Veredito::Otimizado
                }
                Cenario::Melhorias => v == Veredito::Melhorias { quantidade: 3 },
                Cenario::Piorou => matches!(v, Veredito::Piorou(TipoSinal::EspacoLivre)),
                Cenario::JogoAberto => v == Veredito::EsperandoJogo,
                Cenario::Desligado => v == Veredito::AjustesDesligados,
                Cenario::SemServico => matches!(v, Veredito::Indisponivel(_)),
            };
            assert!(ok, "{c:?}: {v:?}");
            assert_eq!(ui.dados.jogos.len(), 5);
            assert_eq!(
                ui.conexao == Conexao::Visualizacao,
                c == Cenario::SemServico
            );
        }
        assert!(ui(Cenario::Atualizado).aviso_atualizado().is_some());
    }

    /// Todo comando recebe resposta simulada sem passar pelo serviço: a
    /// resposta não depende de pipe (não há pipe nenhum no módulo) e chega
    /// mesmo com o pipe padrão sem servidor.
    #[test]
    fn todos_os_comandos_respondem_sem_servico() {
        let canal = iniciar(Cenario::Otimizado);
        let _ = canal.eventos.recv().unwrap();
        let comandos = [
            Comando::Atualizar,
            Comando::AplicarAgora,
            Comando::RestaurarOriginal,
            Comando::Reescanear,
            Comando::MarcarAtualizacaoVista {
                versao: VERSAO.into(),
            },
        ];
        for cmd in comandos {
            canal.comandos.send(cmd.clone()).unwrap();
            let evs: Vec<_> = (0..3).map(|_| canal.eventos.recv().unwrap()).collect();
            assert!(
                matches!(&evs[2], Evento::Concluido { comando, resultado: Ok(_) } if *comando == cmd)
            );
        }
        for acao in [
            AcaoInicio::AplicarRecomendadas,
            AcaoInicio::DesfazerTudo,
            AcaoInicio::VoltarAAjustar,
        ] {
            assert!(executar_inicio(Cenario::Melhorias, acao).is_ok());
            assert_eq!(
                executar_inicio(Cenario::SemServico, acao),
                Err(FalhaInicio::SemServico)
            );
        }
    }

    #[test]
    fn desfazer_e_voltar_simulados() {
        let mut d = dados(Cenario::Otimizado);
        responder(Cenario::Otimizado, &mut d, Comando::RestaurarOriginal);
        assert!(d.estado.as_ref().unwrap().autonomia_pausada);
        responder(Cenario::Otimizado, &mut d, Comando::AplicarAgora);
        assert!(!d.estado.as_ref().unwrap().autonomia_pausada);
    }

    /// O módulo não tem caminho de código até o serviço nem até a máquina.
    #[test]
    fn modulo_nao_fala_com_o_servico_nem_le_a_maquina() {
        let fonte = include_str!("demonstracao.rs");
        let producao = fonte.split("#[cfg(test)]").next().unwrap();
        for proibido in [
            "nvt_ipc::pipe",
            "Cliente",
            "requisitar",
            "Requisicao",
            "servico::iniciar",
            "servico::executar_inicio",
            "crate::local",
            "local::",
            "std::fs",
            "std::env",
        ] {
            assert!(!producao.contains(proibido), "{proibido}");
        }
    }

    /// `main` escolhe o canal simulado e nunca inicia o do serviço junto.
    #[test]
    fn main_nao_inicia_o_servico_na_demonstracao() {
        let main = include_str!("main.rs");
        let i = main.find("fn main()").unwrap();
        let corpo = &main[i..main[i..].find("\n}\n").unwrap() + i];
        let escolha = corpo
            .find("match opcoes.demonstracao")
            .expect("main escolhe o canal pela demonstração");
        let demo = corpo[escolha..].find("demonstracao::iniciar").unwrap();
        let servico = corpo[escolha..].find("servico::iniciar").unwrap();
        assert!(demo < servico);
        assert_eq!(corpo.matches("servico::iniciar").count(), 1);
    }

    /// Ações do Início e da tela Jogos passam pelo desvio da demonstração.
    #[test]
    fn acoes_desviam_para_a_simulacao() {
        let inicio = include_str!("ui/inicio.rs");
        assert!(inicio.contains("demonstracao::ativo()"));
        assert!(inicio.contains("demonstracao::executar_inicio"));
    }

    #[test]
    fn dados_sem_informacao_pessoal() {
        let texto = format!("{:?}", dados(Cenario::Otimizado));
        for proibido in ["Users\\", "C:\\", "@"] {
            assert!(!texto.contains(proibido), "{proibido}");
        }
    }
}
