//! Seção Jogos (spec `interface-e-saude.md` §5.4, R-UI-12): lista limpa com
//! nome, selo competitivo e um status; detalhe ao clicar com o que o app fez,
//! "Faça no jogo", "Arquivo do jogo" e detalhes técnicos recolhidos.

use std::collections::BTreeSet;
use std::sync::mpsc::Sender;
use std::time::Duration;

use gpui_kit::base::motion::{Presence, Transition, transition};
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme, FocusableExt, Icon, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use nvt_ipc::{Jogo, LIMITE_FPS_MAX, LIMITE_FPS_MIN, Qualidade, Sincronismo, StatusMudanca};

use super::estrutura::{Janela, estado_vazio, superficie};
use super::movimento::ripple;
use crate::arquivo_jogo as aj;
use crate::modelo::{Comando, EstadoUi, Evento, Fatos, StatusJogo, status_jogo};
use crate::tema::{self, EstadoBotao as _};
use crate::textos as t;
use crate::textos::jogos as tj;
use gpui_kit::assets::IconName;

/// Seleção da tela (jogo aberto e detalhes técnicos visíveis). Fica num
/// global porque os campos da [`Janela`] pertencem à estrutura.
#[derive(Default)]
struct Selecao {
    jogo: Option<String>,
    tecnicos: bool,
    busca: Option<Entity<InputState>>,
    filtros_excluidos: BTreeSet<FiltroJogo>,
    filtros_abertos: bool,
    mostrar_recomendacoes: bool,
}
impl Global for Selecao {}

/// Canal de comandos usado por "Reverter" e "Procurar jogos de novo". Sem
/// ele instalado os botões ficam desabilitados; quem cria a janela instala
/// com [`instalar_envio`].
struct Envio(Sender<Comando>);
impl Global for Envio {}

/// Liga os botões da tela Jogos ao canal de comandos da janela.
pub(crate) fn instalar_envio(cx: &mut App, comandos: Sender<Comando>) {
    cx.set_global(Envio(comandos));
}

/// Abre já expandido o jogo com esse nome ou perfil (`--expandir`).
pub(crate) fn abrir_inicial(cx: &mut App, jogo: Option<String>) {
    cx.default_global::<Selecao>().jogo = jogo;
}

/// Linha da lista: só o que o leigo precisa (R-UI-12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinhaJogo {
    pub chave: String,
    pub nome: String,
    pub competitivo: bool,
    pub status: StatusJogo,
    pub texto_status: String,
}

fn esperando(ui: &EstadoUi) -> bool {
    Fatos::de_ui(ui, false).esperando_jogo
}

pub(crate) fn linha(j: &Jogo, esperando_jogo: bool) -> LinhaJogo {
    let status = status_jogo(j, esperando_jogo);
    LinhaJogo {
        chave: j.perfil.clone(),
        nome: j.nome.clone(),
        competitivo: j.competitivo.is_some(),
        status,
        texto_status: tj::status(status),
    }
}

/// "O que o nv-tuner fez" em frases simples.
pub(crate) fn feito(j: &Jogo) -> Vec<String> {
    let mut v: Vec<String> = j
        .resumo
        .aplicadas
        .iter()
        .map(|a| t::nome_regra(&a.regra, &a.descricao))
        .collect();
    let arquivo = j.config_jogo.as_ref().is_some_and(|c| {
        c.arquivos.iter().any(|a| {
            a.mudancas
                .iter()
                .any(|m| m.status == StatusMudanca::Aplicada)
        })
    });
    if arquivo {
        v.push(tj::AJUSTOU_ARQUIVO.to_owned());
    }
    v
}

/// Passos numerados de "Faça no jogo".
pub(crate) fn passos(j: &Jogo) -> Vec<String> {
    j.orientacoes
        .iter()
        .map(|o| t::orientacao_ou_codigo(o))
        .chain(aj::orientacoes(j))
        .enumerate()
        .map(|(i, s)| tj::passo(i + 1, &s))
        .collect()
}

/// Detalhes técnicos (recolhidos por padrão): API, DLSS, perfil, anticheat.
pub(crate) fn tecnicos(j: &Jogo) -> Vec<(&'static str, String)> {
    let launcher = if j.launcher.is_empty() {
        "?".to_owned()
    } else {
        j.launcher.clone()
    };
    let dlss = match &j.dlss {
        None => t::NAO.to_owned(),
        Some(v) if v.is_empty() => t::SIM.to_owned(),
        Some(v) => v.clone(),
    };
    let anticheat = match j.anticheat_kernel {
        Some(true) => t::ANTICHEAT_KERNEL,
        _ => "?",
    };
    vec![
        (t::COL_LAUNCHER, launcher),
        (t::COL_API, t::api_grafica(&j.api).to_owned()),
        (t::COL_DLSS, dlss),
        (t::COL_REFLEX, t::talvez(j.reflex).to_owned()),
        (t::COL_ANTICHEAT, anticheat.to_owned()),
        (
            tj::ROTULO_PERFIL,
            format!("“{}” · {}", j.perfil, t::origem_perfil(j.origem_perfil)),
        ),
    ]
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum FiltroJogo {
    Plataforma(String),
    Status(bool),
    Api(String),
}

impl FiltroJogo {
    fn rotulo(&self) -> &str {
        match self {
            Self::Plataforma(nome) => nome,
            Self::Status(true) => "Ativo",
            Self::Status(false) => "Inativo",
            Self::Api(api) => match t::api_grafica(api) {
                "?" if api.is_empty() => "API desconhecida",
                "?" => api,
                nome => nome,
            },
        }
    }
}

fn filtros_do_jogo(jogo: &Jogo, esperando: bool) -> [FiltroJogo; 3] {
    [
        FiltroJogo::Plataforma(if jogo.launcher.is_empty() {
            "Plataforma desconhecida".into()
        } else {
            jogo.launcher.clone()
        }),
        FiltroJogo::Status(status_jogo(jogo, esperando) == StatusJogo::Otimizado),
        FiltroJogo::Api(jogo.api.clone()),
    ]
}

fn menu_filtros(
    jogos: &[Jogo],
    excluidos: &BTreeSet<FiltroJogo>,
    window: &mut Window,
    cx: &mut Context<Janela>,
) -> Popover {
    let janela = cx.entity().downgrade();
    let alternar = janela.clone();
    let aberto = cx.global::<Selecao>().filtros_abertos;
    let presenca = Presence::new("presenca-menu-filtros", aberto)
        .transition(Transition::new(Duration::from_millis(if aberto {
            220
        } else {
            160
        })))
        .sample(window, cx);
    let progresso = presenca.progress;
    let opcoes: BTreeSet<_> = jogos
        .iter()
        .flat_map(|jogo| filtros_do_jogo(jogo, false))
        .collect();
    let plataformas: Vec<_> = opcoes
        .iter()
        .filter(|f| matches!(f, FiltroJogo::Plataforma(_)))
        .cloned()
        .collect();
    let apis: Vec<_> = opcoes
        .into_iter()
        .filter(|f| matches!(f, FiltroJogo::Api(_)))
        .collect();
    let grupos = [
        ("Plataforma", plataformas),
        (
            "Status",
            vec![FiltroJogo::Status(true), FiltroJogo::Status(false)],
        ),
        ("API gráfica", apis),
    ];
    Popover::new("menu-filtros-jogos")
        .anchor(Anchor::TopRight)
        .offset(px(tema::ESP_2) * progresso)
        .open(presenca.should_render())
        .on_open_change(move |aberto, _, cx| {
            let _ = alternar.update(cx, |_, cx| {
                cx.default_global::<Selecao>().filtros_abertos = *aberto;
                cx.notify();
            });
        })
        .opacity(progresso)
        .rounded(px(tema::RAIO_MENU_FILTROS - tema::ESP_1 * progresso))
        .bg(tema::cor(tema::FUNDO_MENU_FILTROS))
        .trigger(
            tema::botao("filtros")
                .outline()
                .icon(IconName::SlidersHorizontal)
                .label(if excluidos.is_empty() {
                    "Filtros".into()
                } else {
                    format!("Filtros · {}", excluidos.len())
                }),
        )
        .content(move |_, window, cx| {
            let tema = cx.theme().clone();
            let excluidos = cx.global::<Selecao>().filtros_excluidos.clone();
            let limpar = janela.clone();
            v_flex()
                .w(px(tema::LARGURA_MENU_FILTROS))
                .gap(px(tema::ESP_3))
                .child(
                    h_flex()
                        .justify_between()
                        .child(
                            h_flex()
                                .gap(px(tema::ESP_2))
                                .child(
                                    Icon::new(IconName::SlidersHorizontal)
                                        .size(px(tema::ESP_4))
                                        .text_color(tema::cor(tema::TEXTO_ICONE)),
                                )
                                .child(div().font_semibold().child("Filtros avançados")),
                        )
                        .child(
                            tema::botao_compacto("limpar-filtros")
                                .ghost()
                                .label("Limpar")
                                .text_color(tema::cor(tema::TEXTO_ICONE))
                                .indisponivel(excluidos.is_empty())
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    let _ = limpar.update(cx, |_, cx| {
                                        cx.default_global::<Selecao>().filtros_excluidos.clear();
                                        cx.notify();
                                    });
                                    cx.notify();
                                })),
                        ),
                )
                .children(grupos.iter().enumerate().map(|(i, (titulo, opcoes))| {
                    let itens = opcoes
                        .iter()
                        .enumerate()
                        .map(|(j, opcao)| {
                            let marcada = !excluidos.contains(opcao);
                            let chave: SharedString = format!("opcao-filtro-{i}-{j}").into();
                            let selecao = transition(
                                chave.clone(),
                                if marcada { 1_f32 } else { 0_f32 },
                                Transition::new(Duration::from_millis(180)),
                                window,
                                cx,
                            );
                            let rotulo = opcao.rotulo().to_owned();
                            let opcao = opcao.clone();
                            let janela = janela.clone();

                            tema::botao_compacto(chave)
                                .ghost()
                                .bg(tema::cor(tema::ACAO_VIOLETA).opacity(0.10 + 0.34 * selecao))
                                .text_color(if marcada {
                                    tema.foreground
                                } else {
                                    tema.muted_foreground
                                })
                                .accessibility_label(rotulo.clone())
                                .toggled(marcada)
                                .child(
                                    h_flex()
                                        .child(
                                            div()
                                                .overflow_hidden()
                                                .opacity(selecao)
                                                .w(px(tema::ESP_4 + tema::ESP_1) * selecao)
                                                .child(
                                                    Icon::new(IconName::Check)
                                                        .size(px(tema::TAM_CORPO))
                                                        .text_color(tema::cor(tema::TEXTO_ICONE)),
                                                ),
                                        )
                                        .child(rotulo),
                                )
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    let _ = janela.update(cx, |_, cx| {
                                        let excluidos =
                                            &mut cx.default_global::<Selecao>().filtros_excluidos;
                                        if !excluidos.remove(&opcao) {
                                            excluidos.insert(opcao.clone());
                                        }
                                        cx.notify();
                                    });
                                    cx.notify();
                                }))
                        })
                        .collect::<Vec<_>>();
                    v_flex()
                        .gap(px(tema::ESP_2))
                        .border_t_1()
                        .border_color(tema::cor(tema::BORDA_SUAVE))
                        .pt(px(tema::ESP_3))
                        .child(
                            div()
                                .font_semibold()
                                .text_size(px(tema::TAM_LEGENDA))
                                .text_color(tema.muted_foreground)
                                .child(*titulo),
                        )
                        .child(h_flex().flex_wrap().gap(px(tema::ESP_2)).children(itens))
                }))
        })
}

/// Conteúdo da seção Jogos.
pub(super) fn render(
    janela: &Janela,
    largura: Pixels,
    window: &mut Window,
    cx: &mut Context<Janela>,
) -> AnyElement {
    let tema = cx.theme().clone();
    let jogos = janela.ui.jogos();
    let busca = match cx.default_global::<Selecao>().busca.clone() {
        Some(busca) => busca,
        None => {
            let busca =
                cx.new(|cx| InputState::new(window, cx).placeholder("Buscar jogo instalado…"));
            cx.subscribe(&busca, |_, _, evento: &InputEvent, cx| {
                if matches!(
                    evento,
                    InputEvent::Change | InputEvent::Focus | InputEvent::Blur
                ) {
                    cx.notify();
                }
            })
            .detach();
            cx.default_global::<Selecao>().busca = Some(busca.clone());
            busca
        }
    };
    let campo = busca.read(cx);
    let termo = campo.value().trim().to_lowercase();
    let busca_focada = campo.has_selection_focus(window, cx);
    let foco_busca = campo.focus_handle(cx);
    let selecao = cx.default_global::<Selecao>();
    let excluidos = selecao.filtros_excluidos.clone();
    let esp = esperando(&janela.ui);
    let mostrar_recomendacoes = selecao.mostrar_recomendacoes;
    let filtrados: Vec<_> = jogos
        .iter()
        .enumerate()
        .filter(|(_, j)| {
            filtros_do_jogo(j, esp)
                .iter()
                .all(|filtro| !excluidos.contains(filtro))
                && (termo.is_empty()
                    || j.nome.to_lowercase().contains(&termo)
                    || j.launcher.to_lowercase().contains(&termo))
        })
        .collect();
    let (aberto, tec) = cx
        .try_global::<Selecao>()
        .map(|s| (s.jogo.clone(), s.tecnicos))
        .unwrap_or_default();
    let pode = cx.has_global::<Envio>() && janela.ui.pode_executar(&Comando::Reescanear);

    let procurar = tema::botao_icone(
        "procurar-jogos",
        Icon::new(IconName::RefreshCw).size(px(tema::ESP_6)),
    )
    .ghost()
    .text_color(tema.muted_foreground)
    .accessibility_label(tj::BOTAO_PROCURAR)
    .tooltip(tj::BOTAO_PROCURAR)
    .indisponivel(!pode)
    .when(!pode, |b| b.tooltip(t::SO_COM_SERVICO))
    .on_click(cx.listener(|this, _, _, cx| {
        enviar(this, Comando::Reescanear, cx);
    }));

    let campo_busca = Styled::h(
        Input::new(&busca)
            .appearance(false)
            .aria_label("Buscar jogo instalado")
            .cleanable(true)
            .prefix(
                Icon::new(IconName::Search)
                    .size(px(tema::TAM_CORPO))
                    .text_color(if busca_focada {
                        tema::cor(tema::TEXTO_ICONE)
                    } else {
                        tema.muted_foreground
                    }),
            )
            .w_full()
            .px(px(tema::ESP_3))
            .rounded(px(tema::RAIO))
            .bg(tema::cor(tema::FUNDO_CONTROLE))
            .border_1()
            .border_color(tema::cor(if busca_focada {
                tema::BORDA_FOCO
            } else {
                tema::BORDA_BUSCA
            })),
        px(tema::ALTURA_BUSCA),
    );
    let barra = superficie(&tema).p(px(tema::ESP_2)).child(
        h_flex()
            .w_full()
            .flex_wrap()
            .gap(px(tema::ESP_2))
            .child(
                div()
                    .flex_1()
                    .min_w(px(tema::COLUNA_ROTULO))
                    .on_mouse_down_out(move |_, window, cx| {
                        if foco_busca.is_focused(window) {
                            window.blur(cx);
                        }
                    })
                    .child(campo_busca),
            )
            .when(!jogos.is_empty(), |v| {
                let filtros = menu_filtros(&jogos, &excluidos, window, cx);
                v.child(ripple("onda-filtros", true, filtros, window, cx))
            })
            .child(ripple(
                "onda-otimizar",
                true,
                tema::botao("otimizar-sistema")
                    .primary()
                    .icon(IconName::Zap)
                    .label(if mostrar_recomendacoes {
                        "Ocultar recomendações"
                    } else {
                        "Otimizar sistema"
                    })
                    .on_click(cx.listener(|_, _, _, cx| {
                        let selecao = cx.default_global::<Selecao>();
                        selecao.mostrar_recomendacoes = !selecao.mostrar_recomendacoes;
                        cx.notify();
                    })),
                window,
                cx,
            )),
    );
    let preferencias = preferencias_globais(janela, largura, window, cx);
    let v = v_flex()
        .gap(px(tema::ESP_4))
        .w_full()
        .child(barra)
        .when(mostrar_recomendacoes, |v| {
            v.child(super::inicio::recomendacoes(janela, cx))
        });
    let cabecalho = h_flex()
        .gap(px(tema::ESP_3))
        .child(ripple("onda-procurar-jogos", pode, procurar, window, cx))
        .child(
            div()
                .font_semibold()
                .child(if termo.is_empty() && excluidos.is_empty() {
                    format!("Jogos detectados ({})", jogos.len())
                } else {
                    format!("Jogos encontrados ({} de {})", filtrados.len(), jogos.len())
                }),
        );
    let v = v.child(cabecalho);
    if filtrados.is_empty() {
        return v
            .child(superficie(&tema).child(estado_vazio(
                IconName::Gamepad2,
                if jogos.is_empty() {
                    tj::NENHUM_JOGO
                } else {
                    "Nenhum jogo corresponde aos filtros."
                },
                None,
                &tema,
            )))
            .child(preferencias)
            .into_any_element();
    }
    let mut lista = v_flex().gap(px(tema::ESP_2));
    for (i, j) in filtrados {
        let l = linha(j, esp);
        let sel = aberto.as_deref() == Some(l.chave.as_str())
            || aberto.as_deref() == Some(l.nome.as_str());
        let chave = l.chave.clone();
        let cor_status = match l.status {
            StatusJogo::Otimizado => tema.success,
            StatusJogo::FacaAjustes { .. } => tema.info,
            StatusJogo::AguardandoFechar => tema.warning,
            StatusJogo::NadaAAjustar => tema.muted_foreground,
        };
        let item = h_flex()
            .id(("jogo", i))
            .rounded(px(tema::RAIO_CARTAO - tema::LINHA_FINA))
            .gap(px(tema::ESP_3))
            .px(px(tema::ESP_4))
            .py(px(tema::ESP_3))
            .cursor_pointer()
            .hover(|d| d.bg(tema.list_hover))
            .on_click(cx.listener(move |_, _, _, cx| {
                let s = cx.default_global::<Selecao>();
                s.jogo = if sel { None } else { Some(chave.clone()) };
                s.tecnicos = false;
                cx.notify();
            }))
            .child(
                h_flex()
                    .size(px(tema::TAM_CAPA_JOGO))
                    .flex_shrink_0()
                    .justify_center()
                    .rounded(px(tema::RAIO))
                    .bg(tema::cor(tema::FUNDO_ICONE))
                    .text_color(tema::cor(tema::TEXTO_ICONE))
                    .child(Icon::new(IconName::Gamepad2)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(tema::ESP_1))
                    .child(
                        h_flex()
                            .gap(px(tema::ESP_2))
                            .flex_wrap()
                            .child(div().font_semibold().child(l.nome))
                            .when(!j.launcher.is_empty(), |v| {
                                v.child(
                                    Tag::secondary().small().outline().child(j.launcher.clone()),
                                )
                            })
                            .when(l.competitivo, |v| {
                                v.child(
                                    Tag::secondary()
                                        .small()
                                        .outline()
                                        .child(t::SELO_COMPETITIVO),
                                )
                            }),
                    )
                    .child(
                        h_flex()
                            .gap(px(tema::ESP_2))
                            .child(
                                div()
                                    .size(px(tema::TAM_PONTO_STATUS))
                                    .flex_shrink_0()
                                    .rounded_full()
                                    .bg(cor_status),
                            )
                            .child(
                                div()
                                    .text_size(px(tema::TAM_LEGENDA))
                                    .text_color(tema.muted_foreground)
                                    .child(if j.api.is_empty() {
                                        l.texto_status
                                    } else {
                                        format!("{} · {}", l.texto_status, t::api_grafica(&j.api))
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .rounded_full()
                    .px(px(tema::ESP_2))
                    .bg(tema::cor(tema::FUNDO_ICONE))
                    .font_family("Consolas")
                    .text_size(px(tema::TAM_LEGENDA))
                    .text_color(if l.status == StatusJogo::Otimizado {
                        tema::cor(tema::TEXTO_ICONE)
                    } else {
                        tema.muted_foreground
                    })
                    .child(if l.status == StatusJogo::Otimizado {
                        "● Ativo"
                    } else {
                        "○ Pendente"
                    }),
            )
            .child(
                div()
                    .w(px(tema::ESP_4))
                    .text_size(px(tema::TAM_LEGENDA))
                    .text_color(tema.muted_foreground)
                    .child(
                        Icon::new(if sel {
                            IconName::ChevronUp
                        } else {
                            IconName::ChevronDown
                        })
                        .size(px(tema::TAM_CORPO)),
                    ),
            );
        lista = lista.child(
            superficie(&tema)
                .overflow_hidden()
                .child(item)
                .when(sel, |v| v.child(detalhe(janela, i, j, tec, cx))),
        );
    }
    v.child(lista).child(preferencias).into_any_element()
}

fn preferencias_globais(
    janela: &Janela,
    largura: Pixels,
    window: &mut Window,
    cx: &mut Context<Janela>,
) -> Div {
    let tema = cx.theme().clone();
    let r = &janela.ui.rascunho;
    let alterado = janela.ui.rascunho_alterado();
    let pode_salvar = janela
        .ui
        .pode_executar(&Comando::DefinirPreferencias(r.clone()));
    let titulo = |texto: &'static str| {
        div()
            .font_family("Consolas")
            .text_size(px(tema::TAM_LEGENDA))
            .text_color(tema.muted_foreground)
            .child(texto)
    };
    let cartao = || {
        superficie(&tema)
            .h_full()
            .min_w_0()
            .gap(px(tema::ESP_2))
            .p(px(tema::ESP_3))
            .bg(tema::cor(tema::FUNDO_CONTROLE))
    };

    let perfil = cartao().child(titulo("PERFIL DE DESEMPENHO")).children(
        [
            (Qualidade::Desempenho, "Desempenho máximo"),
            (Qualidade::Equilibrado, "Equilibrado"),
            (Qualidade::Qualidade, "Alta fidelidade"),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (qualidade, nome))| {
            tema::botao_compacto(("qualidade-global", i))
                .w_full()
                .ghost()
                .toggled(r.qualidade == qualidade)
                .child(h_flex().w_full().justify_between().child(nome).when(
                    r.qualidade == qualidade,
                    |v| {
                        v.child(
                            Icon::new(IconName::CircleCheck)
                                .size(px(tema::TAM_CORPO))
                                .text_color(tema::cor(tema::TEXTO_ICONE)),
                        )
                    },
                ))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.ui.rascunho.qualidade = qualidade;
                    cx.notify();
                }))
        }),
    );

    let reflex = janela
        .ui
        .dados
        .estado
        .as_ref()
        .and_then(|estado| estado.gpu.as_ref())
        .map(|gpu| gpu.capacidades.reflex);
    let sync_ativo = r.sincronismo != Sincronismo::Desligado;
    let foco_sync = window
        .use_keyed_state("foco-sincronismo", cx, |_, cx| cx.focus_handle())
        .read(cx)
        .clone();
    let foco_visivel = window.last_input_was_keyboard() && foco_sync.contains_focused(window, cx);
    let sincronismo = cartao()
        .child(titulo("SINCRONIZAÇÃO & LATÊNCIA"))
        .child(
            h_flex()
                .justify_between()
                .gap(px(tema::ESP_2))
                .child(
                    v_flex()
                        .child(div().font_semibold().child("NVIDIA Reflex"))
                        .child(
                            div()
                                .text_size(px(tema::TAM_LEGENDA))
                                .text_color(tema.muted_foreground)
                                .child("Suporte da GPU"),
                        ),
                )
                .child(titulo(match reflex {
                    Some(true) => "Disponível",
                    Some(false) => "Indisponível",
                    None => "Desconhecido",
                })),
        )
        .child(
            h_flex()
                .justify_between()
                .gap(px(tema::ESP_2))
                .border_t_1()
                .border_color(tema.border)
                .pt(px(tema::ESP_2))
                .child(div().font_semibold().child("Sincronismo vertical"))
                .child(
                    div()
                        .track_focus(&foco_sync)
                        .rounded_full()
                        .border_1()
                        .border_color(if foco_visivel {
                            tema::cor(tema::BORDA_FOCO)
                        } else {
                            tema.transparent
                        })
                        .on_mouse_down_out(move |_, window, cx| {
                            if foco_sync.contains_focused(window, cx) {
                                window.blur(cx);
                            }
                        })
                        .child(
                            Switch::new("sincronismo-global")
                                .cursor_pointer()
                                .focus_ring(false)
                                .color(tema::cor(tema::ACAO_VIOLETA))
                                .accessibility_label("Sincronismo vertical")
                                .checked(sync_ativo)
                                .on_click(cx.listener(|this, ligado: &bool, _, cx| {
                                    this.ui.rascunho.sincronismo = if *ligado {
                                        let atual = this.ui.preferencias_atuais().sincronismo;
                                        if atual == Sincronismo::GSync {
                                            atual
                                        } else {
                                            Sincronismo::VSync
                                        }
                                    } else {
                                        Sincronismo::Desligado
                                    };
                                    cx.notify();
                                })),
                        ),
                ),
        )
        .when(sync_ativo, |v| {
            v.child(
                h_flex().gap(px(tema::ESP_1)).children(
                    [
                        (Sincronismo::VSync, "V-Sync"),
                        (Sincronismo::GSync, "G-SYNC"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(i, (modo, nome))| {
                        tema::botao_compacto(("modo-sync", i))
                            .ghost()
                            .label(nome)
                            .toggled(r.sincronismo == modo)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ui.rascunho.sincronismo = modo;
                                cx.notify();
                            }))
                    }),
                ),
            )
        })
        .when_some(janela.ui.aviso_sincronismo(), |v, aviso| {
            v.child(
                div()
                    .text_size(px(tema::TAM_LEGENDA))
                    .text_color(tema.warning)
                    .child(aviso),
            )
        });

    let passo = |id, rotulo, delta: i32| {
        tema::botao_compacto(id)
            .ghost()
            .label(rotulo)
            .indisponivel(r.limite_fps.is_none())
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(fps) = this.ui.rascunho.limite_fps {
                    this.ui.rascunho.limite_fps = Some(
                        fps.saturating_add_signed(delta)
                            .clamp(LIMITE_FPS_MIN, LIMITE_FPS_MAX),
                    );
                    cx.notify();
                }
            }))
    };
    let limite = cartao()
        .child(titulo("LIMITE DE FPS"))
        .child(
            h_flex()
                .justify_between()
                .rounded(px(tema::RAIO))
                .border_1()
                .border_color(tema.border)
                .bg(tema.group_box)
                .child(passo("fps-global-menos", "−", -1))
                .child(
                    div().font_family("Consolas").font_semibold().child(
                        r.limite_fps
                            .map_or_else(|| "Desligado".into(), |fps| format!("{fps} FPS")),
                    ),
                )
                .child(passo("fps-global-mais", "+", 1)),
        )
        .child(
            h_flex().flex_wrap().gap(px(tema::ESP_1)).children(
                [Some(60), Some(144), Some(240), None]
                    .into_iter()
                    .enumerate()
                    .map(|(i, fps)| {
                        tema::botao_compacto(("atalho-fps-global", i))
                            .flex_grow(1.)
                            .outline()
                            .label(fps.map_or_else(|| "OFF".into(), |fps| fps.to_string()))
                            .toggled(r.limite_fps == fps)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ui.rascunho.limite_fps = fps;
                                cx.notify();
                            }))
                    }),
            ),
        );

    let colunas = if largura >= px(tema::LARGURA_GRADE_TRES) {
        3
    } else {
        1
    };
    superficie(&tema)
        .p(px(tema::ESP_4))
        .gap(px(tema::ESP_4))
        .child(
            h_flex()
                .justify_between()
                .border_b_1()
                .border_color(tema.border)
                .pb(px(tema::ESP_3))
                .child(
                    v_flex()
                        .child(div().font_semibold().child("Preferências globais"))
                        .child(
                            div()
                                .text_size(px(tema::TAM_LEGENDA))
                                .text_color(tema.muted_foreground)
                                .child("Configurações padrão para os jogos detectados"),
                        ),
                )
                .child(Icon::new(IconName::SlidersHorizontal).text_color(tema.muted_foreground)),
        )
        .child(
            div()
                .grid()
                .grid_cols(colunas)
                .gap(px(tema::ESP_4))
                .children([perfil, sincronismo, limite]),
        )
        .child(
            h_flex()
                .justify_end()
                .flex_wrap()
                .gap(px(tema::ESP_2))
                .border_t_1()
                .border_color(tema.border)
                .pt(px(tema::ESP_2))
                .when(alterado, |v| {
                    v.child(
                        div()
                            .flex_1()
                            .text_color(tema.warning)
                            .text_size(px(tema::TAM_LEGENDA))
                            .child(t::PREFS_NAO_SALVAS),
                    )
                })
                .child(ripple(
                    "onda-restaurar",
                    true,
                    tema::botao("restaurar-prefs-globais")
                        .ghost()
                        .label("Restaurar padrões")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.ui.rascunho = crate::modelo::preferencias_padrao();
                            cx.notify();
                        })),
                    window,
                    cx,
                ))
                .child(ripple(
                    "onda-salvar",
                    alterado && pode_salvar,
                    tema::botao("salvar-prefs-globais")
                        .primary()
                        .icon(IconName::Check)
                        .label("Salvar alterações")
                        .indisponivel(!(alterado && pode_salvar))
                        .when(!pode_salvar, |b| b.tooltip(t::SO_COM_SERVICO))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.enviar(Comando::DefinirPreferencias(this.ui.rascunho.clone()), cx);
                        })),
                    window,
                    cx,
                )),
        )
}

fn enviar(this: &mut Janela, c: Comando, cx: &mut Context<Janela>) {
    let Some(Envio(s)) = cx.try_global::<Envio>() else {
        return;
    };
    if !this.ui.pode_executar(&c) {
        return;
    }
    let s = s.clone();
    this.ui.aplicar(Evento::Ocupado(c.clone()));
    let _ = s.send(c);
    cx.notify();
}

fn detalhe(janela: &Janela, i: usize, j: &Jogo, tec: bool, cx: &mut Context<Janela>) -> Div {
    let tema = cx.theme().clone();
    let titulo = |s: &'static str| {
        div()
            .w(px(tema::COLUNA_ROTULO))
            .flex_shrink_0()
            .text_size(px(tema::TAM_CORPO))
            .font_semibold()
            .text_color(tema.muted_foreground)
            .child(s)
    };
    let bloco = |rotulo: &'static str, linhas: Vec<String>, vazio: &'static str| {
        let corpo = if linhas.is_empty() {
            v_flex().child(
                div()
                    .text_size(px(tema::TAM_CORPO))
                    .text_color(tema.muted_foreground)
                    .child(vazio),
            )
        } else {
            v_flex().gap(px(tema::ESP_1)).children(
                linhas
                    .into_iter()
                    .map(|s| div().text_size(px(tema::TAM_CORPO)).child(s)),
            )
        };
        h_flex()
            .items_start()
            .gap(px(tema::ESP_3))
            .child(titulo(rotulo))
            .child(corpo.flex_1())
    };
    let mut d = v_flex()
        .gap(px(tema::ESP_3))
        .p(px(tema::ESP_4))
        .border_t_1()
        .border_color(tema.border)
        .bg(tema.background)
        .child(div().font_semibold().child(j.nome.clone()))
        .child(bloco(tj::DETALHE_FEITO, feito(j), tj::DETALHE_NADA_FEITO))
        .child(bloco(tj::DETALHE_FACA, passos(j), tj::DETALHE_NADA_A_FAZER));
    if let Some(a) = arquivo(janela, i, j, cx) {
        d = d.child(
            h_flex()
                .items_start()
                .gap(px(tema::ESP_3))
                .child(titulo(aj::SECAO_ARQUIVO_JOGO))
                .child(a.flex_1()),
        );
    }
    d = d.child(
        h_flex().child(
            tema::botao_compacto(("tecnicos", i))
                .ghost()
                .label(if tec {
                    tj::BOTAO_OCULTAR_TECNICOS
                } else {
                    tj::BOTAO_DETALHES_TECNICOS
                })
                .on_click(cx.listener(|_, _, _, cx| {
                    let s = cx.default_global::<Selecao>();
                    s.tecnicos = !s.tecnicos;
                    cx.notify();
                })),
        ),
    );
    if tec {
        d = d.child(
            v_flex()
                .gap(px(tema::ESP_1))
                .children(tecnicos(j).into_iter().map(|(r, v)| {
                    h_flex()
                        .gap(px(tema::ESP_3))
                        .text_size(px(tema::TAM_LEGENDA))
                        .child(
                            div()
                                .w(px(tema::COLUNA_ROTULO))
                                .text_color(tema.muted_foreground)
                                .child(r),
                        )
                        .child(div().child(v))
                })),
        );
    }
    d
}

/// "Arquivo do jogo": arquivo, antes/depois, backup e Reverter por chave.
fn arquivo(janela: &Janela, i: usize, j: &Jogo, cx: &mut Context<Janela>) -> Option<Div> {
    let c = j.config_jogo.as_ref()?;
    let tema = cx.theme().clone();
    let pode = janela.ui.pode_escrever() && cx.has_global::<Envio>();
    let sub = |s: String| {
        div()
            .text_size(px(tema::TAM_LEGENDA))
            .text_color(tema.muted_foreground)
            .child(s)
    };
    let mut v = v_flex().gap(px(tema::ESP_2));
    for (k, a) in c.arquivos.iter().enumerate() {
        let mut b = v_flex()
            .gap(px(tema::ESP_1))
            .child(
                div()
                    .text_size(px(tema::TAM_CORPO))
                    .font_semibold()
                    .child(aj::nome_arquivo(&a.rotulo.0).to_owned()),
            )
            .children(
                a.conta
                    .as_ref()
                    .map(|c| sub(format!("{}: {c}", aj::ROTULO_CONTA))),
            )
            .child(sub(format!(
                "{}: {}",
                aj::ROTULO_BACKUP,
                aj::backup(a.backup)
            )))
            .when(a.mudancas.is_empty(), |d| {
                d.child(sub(aj::SEM_MUDANCAS_ARQUIVO.into()))
            });
        for (n, m) in a.mudancas.iter().enumerate() {
            let destino = (m.status == StatusMudanca::Aplicada)
                .then(|| aj::destino_reverter(a, &m.setting))
                .flatten();
            let mut l = h_flex().gap(px(tema::ESP_3)).justify_between().child(
                div()
                    .text_size(px(tema::TAM_CORPO))
                    .flex_1()
                    .min_w_0()
                    .child(aj::linha_mudanca(m)),
            );
            if let Some(destino) = destino {
                l = l.child(
                    tema::botao_compacto(("reverter-jogo", i * 10_000 + k * 100 + n))
                        .outline()
                        .label(t::BOTAO_REVERTER)
                        .indisponivel(!pode)
                        .when(!pode, |b| b.tooltip(t::SO_COM_SERVICO))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            enviar(
                                this,
                                Comando::ReverterConfigJogo {
                                    destino: destino.clone(),
                                },
                                cx,
                            )
                        })),
                );
            }
            b = b.child(l);
        }
        v = v.child(b);
    }
    Some(v)
}

#[cfg(test)]
mod testes {
    use super::{feito, linha, passos, t, tecnicos, tj};
    use nvt_ipc::Jogo;
    use nvt_ipc::{OrigemPerfil, ResumoJogo};

    fn jogo(nome: &str) -> Jogo {
        Jogo {
            nome: nome.into(),
            perfil: format!("perfil-{nome}"),
            origem_perfil: OrigemPerfil::Driver,
            launcher: "Steam".into(),
            api: "dx12".into(),
            dlss: Some("3.7.10".into()),
            reflex: Some(true),
            anticheat_kernel: Some(true),
            competitivo: Some("cs2".into()),
            em_execucao: false,
            orientacoes: vec![],
            resumo: ResumoJogo::default(),
            config_jogo: None,
        }
    }

    #[test]
    fn lista_mostra_so_nome_selo_e_status() {
        let mut j = jogo("Cyberpunk 2077");
        j.orientacoes = vec!["reflex_on_boost".into(), "x".into()];
        let l = linha(&j, false);
        assert_eq!(l.texto_status, "Faça 2 ajustes no jogo");
        assert!(l.competitivo);
        let visivel = format!("{} {}", l.nome, l.texto_status);
        for proibido in ["dx12", "DirectX", "3.7.10", "perfil-", "kernel", "Steam"] {
            assert!(!visivel.contains(proibido), "{proibido} na lista");
        }
    }

    #[test]
    fn status_de_cada_caso() {
        let mut j = jogo("Valorant");
        assert_eq!(linha(&j, false).texto_status, tj::STATUS_NADA);
        j.em_execucao = true;
        assert_eq!(linha(&j, true).texto_status, tj::STATUS_AGUARDANDO);
        j.orientacoes = vec!["a".into()];
        assert_eq!(linha(&j, false).texto_status, "Faça 1 ajuste no jogo");
    }

    #[test]
    fn detalhe_numera_passos_e_tecnicos_ficam_a_parte() {
        let mut j = jogo("CS2");
        j.orientacoes = vec!["a".into(), "b".into()];
        let p = passos(&j);
        assert_eq!(p.len(), 2);
        assert!(p[0].starts_with("1. ") && p[1].starts_with("2. "));
        assert!(feito(&j).is_empty());
        let tec = tecnicos(&j);
        assert!(tec.iter().any(|(_, v)| v == "3.7.10"));
        assert!(tec.iter().any(|(r, _)| *r == t::COL_ANTICHEAT));
    }
}
