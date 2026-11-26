//! Veredito do Início, linha de estado e status por jogo (R-UI-02, R-UI-05,
//! R-UI-12). Funções puras: devolvem códigos; os textos ficam em `textos`.
//!
//! A decisão lê [`Fatos`], um resumo neutro do protocolo. Hoje ele é montado
//! do IPC v7 por [`Fatos::de_ui`]; na v8 só esse construtor muda.

use nvt_ipc::{EstadoExperimental, Jogo};

use super::{Conexao, EstadoUi};

/// Sinais de saúde do PC (espelho de `TipoSinal`, spec §7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoSinal {
    EspacoLivre,
    MemoriaRepouso,
    CpuRepouso,
    DriverDesatualizado,
    PlanoEnergia,
    SensorArmazenamento,
}

/// Resumo da resposta de `ObterSaude`: sinais em atenção, mais grave primeiro.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Saude {
    pub em_atencao: Vec<TipoSinal>,
}

/// Fatos que decidem o veredito e a linha de estado.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fatos {
    /// Serviço de outra versão do protocolo.
    pub protocolo_incompativel: bool,
    /// Serviço ausente (modo de visualização, somente leitura).
    pub servico_ausente: bool,
    /// Primeira resposta ainda não chegou.
    pub conectando: bool,
    /// Há placa NVIDIA (`None` = ainda não se sabe).
    pub placa_nvidia: Option<bool>,
    /// O usuário usou "Desfazer tudo" e não pediu para voltar a ajustar.
    pub ajustes_desligados: bool,
    /// Há gravação esperando o jogo fechar.
    pub esperando_jogo: bool,
    /// Nome do jogo aberto, se identificado.
    pub jogo_aberto: Option<String>,
    /// Melhorias recomendadas (experimentais aplicáveis) ainda não aceitas.
    pub melhorias: usize,
    /// O plano tem alguma mudança (aplicada, pendente ou experimental).
    pub plano_com_mudancas: bool,
    /// Versão mudou desde a última abertura e o aviso ainda vale.
    pub atualizacao_concluida: bool,
    /// Atualização encontrada, baixando ou instalando (falha não conta).
    pub atualizacao_pendente: bool,
    /// Agente da sessão ausente (serviço conectado).
    pub agente_ausente: bool,
}

impl Fatos {
    /// Monta os fatos do estado atual da UI (IPC v8). `ajustes_desligados`
    /// vem do serviço (`Estado::autonomia_pausada`); `atualizacao_concluida`
    /// vem da UI, porque o aviso some ao trocar de tela.
    pub fn de_ui(ui: &EstadoUi, atualizacao_concluida: bool) -> Self {
        let estado = ui.dados.estado.as_ref();
        let servico = ui.conexao == Conexao::Servico;
        let adiamento = estado.and_then(|e| e.adiamento.as_ref());
        let itens = || ui.dados.grupos.iter().flat_map(|g| &g.itens);
        let jogo_aberto = adiamento.and_then(|a| {
            ui.dados
                .jogos
                .iter()
                .find(|j| j.em_execucao)
                .map(|j| j.nome.clone())
                .or_else(|| a.processos.first().cloned())
        });
        Fatos {
            protocolo_incompativel: ui.incompativel.is_some(),
            servico_ausente: ui.conexao == Conexao::Visualizacao,
            conectando: ui.conexao == Conexao::Conectando,
            placa_nvidia: estado.map(|e| e.gpu.is_some()),
            ajustes_desligados: servico && estado.is_some_and(|e| e.autonomia_pausada),
            esperando_jogo: adiamento.is_some(),
            jogo_aberto,
            melhorias: itens()
                .filter(|i| i.experimental == EstadoExperimental::Aceitavel)
                .count(),
            plano_com_mudancas: itens().next().is_some(),
            atualizacao_concluida: servico && atualizacao_concluida,
            atualizacao_pendente: servico
                && estado.is_some_and(|e| e.atualizacao.pendente.is_some()),
            agente_ausente: servico && estado.is_some_and(|e| !e.agente.presente),
        }
    }
}

/// Por que o nv-tuner não pode agir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotivoIndisponivel {
    ProtocoloIncompativel,
    ServicoAusente,
}

/// O único veredito do Início (R-UI-02).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Veredito {
    /// Primeira resposta ainda não chegou.
    Verificando,
    Indisponivel(MotivoIndisponivel),
    SemPlacaNvidia,
    /// Botão "Voltar a ajustar" (envia `AplicarAgora`).
    AjustesDesligados,
    EsperandoJogo,
    Piorou(TipoSinal),
    Melhorias {
        quantidade: usize,
    },
    Otimizado,
    /// Variante de "otimizado": o plano não tem nada para este PC.
    NadaAFazer,
}

impl Veredito {
    /// O veredito oferece "Voltar a ajustar"?
    pub fn oferece_voltar_a_ajustar(&self) -> bool {
        matches!(self, Veredito::AjustesDesligados)
    }

    /// O veredito oferece "Aplicar melhorias recomendadas"?
    pub fn oferece_aplicar_recomendadas(&self) -> bool {
        matches!(self, Veredito::Melhorias { .. })
    }
}

/// Veredito por prioridade: indisponível > sem placa > desligado > esperando
/// o jogo > piorou > melhorias > otimizado. Sem saúde, "piorou" nunca vence.
pub fn veredito(f: &Fatos, saude: Option<&Saude>) -> Veredito {
    if f.protocolo_incompativel {
        return Veredito::Indisponivel(MotivoIndisponivel::ProtocoloIncompativel);
    }
    if f.servico_ausente {
        return Veredito::Indisponivel(MotivoIndisponivel::ServicoAusente);
    }
    if f.conectando {
        return Veredito::Verificando;
    }
    match f.placa_nvidia {
        Some(false) => return Veredito::SemPlacaNvidia,
        None => return Veredito::Verificando,
        Some(true) => {}
    }
    if f.ajustes_desligados {
        return Veredito::AjustesDesligados;
    }
    if f.esperando_jogo {
        return Veredito::EsperandoJogo;
    }
    if let Some(s) = saude.and_then(|s| s.em_atencao.first()) {
        return Veredito::Piorou(*s);
    }
    if f.melhorias > 0 {
        return Veredito::Melhorias {
            quantidade: f.melhorias,
        };
    }
    if f.plano_com_mudancas {
        Veredito::Otimizado
    } else {
        Veredito::NadaAFazer
    }
}

/// A linha de estado no topo do conteúdo (R-UI-05, spec §5.1). Nenhuma tem ação.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinhaEstado {
    ProtocoloIncompativel,
    ServicoAusente,
    AtualizacaoConcluida,
    /// `None` = não se sabe qual jogo (lista de processos indisponível).
    JogoAberto {
        nome: Option<String>,
    },
    AtualizacaoPendente,
    AgenteAusente,
}

/// No máximo uma linha, a de maior prioridade.
pub fn linha_estado(f: &Fatos) -> Option<LinhaEstado> {
    if f.protocolo_incompativel {
        Some(LinhaEstado::ProtocoloIncompativel)
    } else if f.servico_ausente {
        Some(LinhaEstado::ServicoAusente)
    } else if f.atualizacao_concluida {
        Some(LinhaEstado::AtualizacaoConcluida)
    } else if f.esperando_jogo {
        Some(LinhaEstado::JogoAberto {
            nome: f.jogo_aberto.clone(),
        })
    } else if f.atualizacao_pendente {
        Some(LinhaEstado::AtualizacaoPendente)
    } else if f.agente_ausente {
        Some(LinhaEstado::AgenteAusente)
    } else {
        None
    }
}

/// Status de um jogo na lista (R-UI-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusJogo {
    AguardandoFechar,
    FacaAjustes { quantidade: usize },
    Otimizado,
    NadaAAjustar,
}

/// Status do jogo: aguardando (aberto com gravação adiada) > ajustes no jogo
/// > otimizado (o app fez algo) > nada a ajustar.
pub fn status_jogo(jogo: &Jogo, esperando_jogo: bool) -> StatusJogo {
    if jogo.em_execucao && esperando_jogo {
        StatusJogo::AguardandoFechar
    } else if !jogo.orientacoes.is_empty() {
        StatusJogo::FacaAjustes {
            quantidade: jogo.orientacoes.len(),
        }
    } else if !jogo.resumo.aplicadas.is_empty() || jogo.config_jogo.is_some() {
        StatusJogo::Otimizado
    } else {
        StatusJogo::NadaAAjustar
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::modelo::Tela;

    fn pronto() -> Fatos {
        Fatos {
            placa_nvidia: Some(true),
            plano_com_mudancas: true,
            ..Default::default()
        }
    }

    #[test]
    fn tabela_do_veredito() {
        use MotivoIndisponivel as M;
        let s = Saude {
            em_atencao: vec![TipoSinal::EspacoLivre, TipoSinal::PlanoEnergia],
        };
        let vazia = Saude::default();
        let casos: Vec<(&str, Fatos, Option<&Saude>, Veredito)> = vec![
            ("otimizado", pronto(), None, Veredito::Otimizado),
            ("saúde ok", pronto(), Some(&vazia), Veredito::Otimizado),
            (
                "nada a fazer",
                Fatos {
                    plano_com_mudancas: false,
                    ..pronto()
                },
                None,
                Veredito::NadaAFazer,
            ),
            (
                "melhorias",
                Fatos {
                    melhorias: 3,
                    ..pronto()
                },
                None,
                Veredito::Melhorias { quantidade: 3 },
            ),
            (
                "piorou vence melhorias",
                Fatos {
                    melhorias: 3,
                    ..pronto()
                },
                Some(&s),
                Veredito::Piorou(TipoSinal::EspacoLivre),
            ),
            (
                "esperando vence piorou",
                Fatos {
                    esperando_jogo: true,
                    ..pronto()
                },
                Some(&s),
                Veredito::EsperandoJogo,
            ),
            (
                "desligado vence esperando",
                Fatos {
                    ajustes_desligados: true,
                    esperando_jogo: true,
                    ..pronto()
                },
                Some(&s),
                Veredito::AjustesDesligados,
            ),
            (
                "sem placa vence desligado",
                Fatos {
                    placa_nvidia: Some(false),
                    ajustes_desligados: true,
                    ..pronto()
                },
                None,
                Veredito::SemPlacaNvidia,
            ),
            (
                "serviço ausente vence sem placa",
                Fatos {
                    servico_ausente: true,
                    placa_nvidia: Some(false),
                    ..pronto()
                },
                None,
                Veredito::Indisponivel(M::ServicoAusente),
            ),
            (
                "protocolo incompatível vence tudo",
                Fatos {
                    protocolo_incompativel: true,
                    servico_ausente: true,
                    ..pronto()
                },
                Some(&s),
                Veredito::Indisponivel(M::ProtocoloIncompativel),
            ),
            (
                "atualização e agente só na linha",
                Fatos {
                    atualizacao_concluida: true,
                    atualizacao_pendente: true,
                    agente_ausente: true,
                    ..pronto()
                },
                None,
                Veredito::Otimizado,
            ),
            (
                "conectando",
                Fatos {
                    conectando: true,
                    ..Default::default()
                },
                Some(&s),
                Veredito::Verificando,
            ),
            (
                "placa desconhecida",
                Fatos::default(),
                None,
                Veredito::Verificando,
            ),
        ];
        for (nome, f, saude, esperado) in casos {
            assert_eq!(veredito(&f, saude), esperado, "{nome}");
        }
    }

    #[test]
    fn sem_saude_piorou_nunca_vence() {
        for melhorias in [0, 2] {
            for plano_com_mudancas in [false, true] {
                for esperando_jogo in [false, true] {
                    let f = Fatos {
                        melhorias,
                        plano_com_mudancas,
                        esperando_jogo,
                        ..pronto()
                    };
                    assert!(!matches!(veredito(&f, None), Veredito::Piorou(_)));
                }
            }
        }
    }

    #[test]
    fn acoes_do_veredito() {
        assert!(Veredito::AjustesDesligados.oferece_voltar_a_ajustar());
        assert!(!Veredito::Otimizado.oferece_voltar_a_ajustar());
        assert!(Veredito::Melhorias { quantidade: 1 }.oferece_aplicar_recomendadas());
        assert!(!Veredito::NadaAFazer.oferece_aplicar_recomendadas());
    }

    #[test]
    fn tabela_da_linha_de_estado() {
        let tudo = Fatos {
            protocolo_incompativel: true,
            servico_ausente: true,
            atualizacao_concluida: true,
            esperando_jogo: true,
            jogo_aberto: Some("Valorant".into()),
            atualizacao_pendente: true,
            agente_ausente: true,
            ..pronto()
        };
        let casos: Vec<(&str, Fatos, Option<LinhaEstado>)> = vec![
            ("nada", pronto(), None),
            (
                "incompatível",
                tudo.clone(),
                Some(LinhaEstado::ProtocoloIncompativel),
            ),
            (
                "ausente",
                Fatos {
                    protocolo_incompativel: false,
                    ..tudo.clone()
                },
                Some(LinhaEstado::ServicoAusente),
            ),
            (
                "atualizado",
                Fatos {
                    protocolo_incompativel: false,
                    servico_ausente: false,
                    ..tudo.clone()
                },
                Some(LinhaEstado::AtualizacaoConcluida),
            ),
            (
                "jogo aberto",
                Fatos {
                    protocolo_incompativel: false,
                    servico_ausente: false,
                    atualizacao_concluida: false,
                    ..tudo.clone()
                },
                Some(LinhaEstado::JogoAberto {
                    nome: Some("Valorant".into()),
                }),
            ),
            (
                "jogo desconhecido",
                Fatos {
                    esperando_jogo: true,
                    ..pronto()
                },
                Some(LinhaEstado::JogoAberto { nome: None }),
            ),
            (
                "atualização pendente",
                Fatos {
                    atualizacao_pendente: true,
                    agente_ausente: true,
                    ..pronto()
                },
                Some(LinhaEstado::AtualizacaoPendente),
            ),
            (
                "agente",
                Fatos {
                    agente_ausente: true,
                    ..pronto()
                },
                Some(LinhaEstado::AgenteAusente),
            ),
        ];
        for (nome, f, esperado) in casos {
            assert_eq!(linha_estado(&f), esperado, "{nome}");
        }
    }

    fn jogo(em_execucao: bool, orientacoes: &[&str], aplicadas: usize) -> Jogo {
        Jogo {
            nome: "Jogo".into(),
            perfil: "Jogo".into(),
            origem_perfil: nvt_ipc::OrigemPerfil::Usuario,
            launcher: String::new(),
            api: "dx11".into(),
            dlss: None,
            reflex: None,
            anticheat_kernel: None,
            competitivo: None,
            em_execucao,
            orientacoes: orientacoes.iter().map(|s| s.to_string()).collect(),
            resumo: nvt_ipc::ResumoJogo {
                aplicadas: (0..aplicadas)
                    .map(|i| nvt_ipc::RegraAplicada {
                        regra: format!("r{i}"),
                        descricao: String::new(),
                        settings: vec![],
                    })
                    .collect(),
                nao_aplicadas: vec![],
                herda_do_global: vec![],
            },
            config_jogo: None,
        }
    }

    #[test]
    fn tabela_do_status_do_jogo() {
        let casos = [
            (
                "otimizado",
                jogo(false, &[], 1),
                false,
                StatusJogo::Otimizado,
            ),
            ("nada", jogo(false, &[], 0), false, StatusJogo::NadaAAjustar),
            (
                "ajustes",
                jogo(false, &["a", "b"], 1),
                false,
                StatusJogo::FacaAjustes { quantidade: 2 },
            ),
            (
                "aguardando",
                jogo(true, &["a"], 1),
                true,
                StatusJogo::AguardandoFechar,
            ),
            (
                "aberto sem adiamento",
                jogo(true, &[], 1),
                false,
                StatusJogo::Otimizado,
            ),
            (
                "adiado por outro jogo",
                jogo(false, &[], 0),
                true,
                StatusJogo::NadaAAjustar,
            ),
        ];
        for (nome, j, esperando, esperado) in casos {
            assert_eq!(status_jogo(&j, esperando), esperado, "{nome}");
        }
    }

    #[test]
    fn fatos_do_estado_v7() {
        let mut ui = EstadoUi::new(Tela::VisaoGeral);
        ui.conexao = Conexao::Conectando;
        assert_eq!(
            veredito(&Fatos::de_ui(&ui, false), None),
            Veredito::Verificando
        );
        ui.conexao = Conexao::Visualizacao;
        let f = Fatos::de_ui(&ui, true);
        assert!(f.servico_ausente && !f.atualizacao_concluida && !f.agente_ausente);
        ui.incompativel = Some("x".into());
        assert_eq!(
            linha_estado(&Fatos::de_ui(&ui, false)),
            Some(LinhaEstado::ProtocoloIncompativel)
        );
        // A pausa vem do serviço e sobrevive a reabrir a UI.
        ui.incompativel = None;
        ui.conexao = Conexao::Servico;
        ui.dados.estado = Some(nvt_ipc::Estado {
            agente: Default::default(),
            diagnosticos: Vec::new(),
            versao_servico: String::new(),
            driver: None,
            gpu: None,
            preferencias: nvt_ipc::Preferencias {
                sincronismo: nvt_ipc::Sincronismo::Desligado,
                qualidade: nvt_ipc::Qualidade::Equilibrado,
                limite_fps: None,
            },
            jogos: 0,
            ultima_aplicacao: None,
            alertas_deriva: Vec::new(),
            avisos: Vec::new(),
            adiamento: None,
            atualizacao: Default::default(),
            autonomia_pausada: true,
        });
        assert!(Fatos::de_ui(&ui, false).ajustes_desligados);
        ui.conexao = Conexao::Visualizacao;
        assert!(!Fatos::de_ui(&ui, false).ajustes_desligados);
    }
}
