//! Seção Jogos (spec `interface-e-saude.md` §5.4, R-UI-12): lista limpa com
//! nome, selo competitivo e um status; detalhe ao clicar com o que o app fez,
//! "Faça no jogo", "Arquivo do jogo" e detalhes técnicos recolhidos.

use std::sync::mpsc::Sender;
use std::time::Duration;

use gpui_kit::base::motion::{Transition, transition};
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme, FocusableExt, Icon, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use nvt_ipc::{Jogo, LIMITE_FPS_MAX, LIMITE_FPS_MIN, Qualidade, Sincronismo, StatusMudanca};

use super::estrutura::{Janela, estado_vazio, rotulo, superficie};
use super::movimento::{recolhivel, ripple, ripple_com_forma};
use crate::arquivo_jogo as aj;
use crate::modelo::{Comando, Evento, StatusJogo, status_jogo};
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
pub(crate) fn tecnicos(j: &Jogo) -> Vec<(&'static str, SharedString)> {
    let launcher = if j.launcher.is_empty() {
        "?".into()
    } else {
        j.launcher.clone().into()
    };
    let dlss = match &j.dlss {
        None => t::NAO.into(),
        Some(v) if v.is_empty() => t::SIM.into(),
        Some(v) => v.clone().into(),
    };
    let anticheat = match j.anticheat_kernel {
        Some(true) => t::ANTICHEAT_KERNEL,
        _ => "?",
    };
    vec![
        (t::COL_LAUNCHER, launcher),
        (t::COL_API, t::api_grafica(&j.api).into()),
        (t::COL_DLSS, dlss),
        (t::COL_REFLEX, t::talvez(j.reflex).into()),
        (t::COL_ANTICHEAT, anticheat.into()),
        (
            tj::ROTULO_PERFIL,
            format!("“{}” · {}", j.perfil, t::origem_perfil(j.origem_perfil)).into(),
        ),
    ]
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
    let esp = janela.fatos().esperando_jogo;
    let mostrar_recomendacoes = selecao.mostrar_recomendacoes;
    let filtrados: Vec<_> = jogos
        .iter()
        .enumerate()
        .filter(|(i, _)| janela.ui.jogo_corresponde(*i, &termo))
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
            .px(px(tema::ESP_4))
            .rounded_full()
            .bg(tema::cor(if busca_focada {
                tema::SUPERFICIE_MAIS_ALTA
            } else {
                tema::FUNDO_CONTROLE
            }))
            // Sem moldura em repouso; o anel só aparece com foco.
            .border_1()
            .border_color(if busca_focada {
                tema::cor(tema::BORDA_FOCO)
            } else {
                tema.transparent
            }),
        px(tema::ALTURA_BUSCA),
    );
    let alternar_detalhes = cx.listener(|_, _, _, cx| {
        let selecao = cx.default_global::<Selecao>();
        selecao.mostrar_recomendacoes = !selecao.mostrar_recomendacoes;
        cx.notify();
    });
    // Com ajustes aplicados (ou pausados) o topo mostra o switch da
    // otimização; o painel de recomendações só abre por "Otimizar sistema",
    // então nesse estado fica oculto.
    let (acao, mostrar_recomendacoes) = match super::inicio::controle_otimizacao(janela, window, cx)
    {
        Some(controle) => (controle, false),
        None => (
            // Mesma altura da busca: a barra se lê como uma linha só (Material 3).
            ripple(
                "onda-otimizar",
                true,
                tema::botao("otimizar-sistema")
                    .icon(if mostrar_recomendacoes {
                        IconName::ChevronUp
                    } else {
                        IconName::Zap
                    })
                    .label("Otimizar sistema")
                    .primary()
                    .on_click(alternar_detalhes)
                    .h(px(tema::ALTURA_BUSCA))
                    .rounded(px(tema::ALTURA_BUSCA / 2.)),
                window,
                cx,
            )
            .into_any_element(),
            mostrar_recomendacoes,
        ),
    };
    // Barra de busca do Material 3: pílula solta, sem cartão em volta.
    let barra = div().w_full().child(
        h_flex()
            .w_full()
            .flex_wrap()
            .items_center()
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
            .child(acao),
    );
    let recomendacoes = recolhivel(
        "painel-otimizacao",
        mostrar_recomendacoes,
        window,
        cx,
        |window, cx| {
            div()
                .pt(px(tema::ESP_3))
                .child(super::inicio::recomendacoes(janela, window, cx))
                .into_any_element()
        },
    );
    let preferencias = preferencias_globais(janela, largura, window, cx);
    let v = v_flex().gap(px(tema::ESP_4)).w_full().child(barra);
    let cabecalho = h_flex()
        .gap(px(tema::ESP_3))
        .child(ripple("onda-procurar-jogos", pode, procurar, window, cx))
        .child(div().flex_1().font_semibold().child(if termo.is_empty() {
            format!("Jogos detectados ({})", jogos.len())
        } else {
            format!("Jogos encontrados ({} de {})", filtrados.len(), jogos.len())
        }));
    let v = v.child(v_flex().child(cabecalho).children(recomendacoes));
    if filtrados.is_empty() {
        return v
            .child(superficie(&tema).child(estado_vazio(
                IconName::Gamepad2,
                if jogos.is_empty() {
                    tj::NENHUM_JOGO
                } else {
                    "Nenhum jogo corresponde à busca."
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
        // Morph do cartão ao expandir: cantos crescem, o tom sobe um nível, o
        // cartão se afasta dos vizinhos e a seta gira; o detalhe revela junto.
        let expansao = transition(
            SharedString::from(format!("expansao-jogo-{}", l.chave)),
            if sel { 1_f32 } else { 0_f32 },
            Transition::new(Duration::from_millis(280)),
            window,
            cx,
        );
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
            .child(chip_status(l.status == StatusJogo::Otimizado, &tema))
            .child(
                div()
                    .w(px(tema::ESP_4))
                    .text_size(px(tema::TAM_LEGENDA))
                    .text_color(tema.muted_foreground)
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .size(px(tema::TAM_CORPO))
                            .rotate(radians(std::f32::consts::PI * expansao)),
                    ),
            );
        let detalhe = recolhivel(
            format!("detalhe-jogo-{}", l.chave),
            sel,
            window,
            cx,
            |_, cx| detalhe(janela, i, j, tec, cx).into_any_element(),
        );
        lista = lista.child(
            superficie(&tema)
                .overflow_hidden()
                .rounded(px(
                    tema::RAIO_CARTAO + (tema::RAIO_CARTAO_PAINEL - tema::RAIO_CARTAO) * expansao
                ))
                .bg(misturar(
                    tema.group_box,
                    tema::cor(tema::SUPERFICIE_ALTA),
                    expansao,
                ))
                .my(px(tema::ESP_2 * expansao))
                .child(ripple_com_forma(
                    format!("onda-jogo-{}", l.chave),
                    true,
                    item,
                    tema::cor(tema::TEXTO).opacity(0.12),
                    (Some(tema::RAIO_CARTAO), Some(tema::RAIO_CARTAO)),
                    window,
                    cx,
                ))
                .children(detalhe),
        );
    }
    v.child(lista).child(preferencias).into_any_element()
}

/// Chip de status (Material 3): tonal quando ativo, contornado quando
/// pendente; ícone em vez de glifos de texto.
fn chip_status(ativo: bool, tema: &gpui_kit::component::Theme) -> Div {
    let (icone, texto, cor, fundo, borda) = if ativo {
        let cor = tema::cor(tema::TEXTO_ICONE);
        (
            IconName::Check,
            "Ativo",
            cor,
            cor.opacity(0.14),
            tema.transparent,
        )
    } else {
        (
            IconName::Clock,
            "Pendente",
            tema.muted_foreground,
            tema.transparent,
            tema::cor(tema::BORDA),
        )
    };
    h_flex()
        .flex_shrink_0()
        .h(px(tema::ALTURA_CHIP))
        .gap(px(tema::ESP_1))
        .px(px(tema::ESP_2))
        .rounded(px(tema::RAIO_CHIP))
        .border_1()
        .border_color(borda)
        .bg(fundo)
        .text_size(px(tema::TAM_LEGENDA))
        .font_medium()
        .text_color(cor)
        .child(Icon::new(icone).size(px(tema::TAM_LEGENDA)))
        .child(texto)
}

/// Interpolação linear entre duas cores (para morphs de tom).
fn misturar(de: Hsla, para: Hsla, t: f32) -> Hsla {
    let de = de.to_rgb();
    let para = para.to_rgb();
    let l = |a: f32, b: f32| a + (b - a) * t;
    Rgba {
        r: l(de.r, para.r),
        g: l(de.g, para.g),
        b: l(de.b, para.b),
        a: l(de.a, para.a),
    }
    .into()
}

type AoClicar = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Um segmento de [`segmentado`].
struct Segmento {
    rotulo: SharedString,
    escolhido: bool,
    ao_clicar: AoClicar,
}

impl Segmento {
    fn new(
        rotulo: impl Into<SharedString>,
        escolhido: bool,
        ao_clicar: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            rotulo: rotulo.into(),
            escolhido,
            ao_clicar: Box::new(ao_clicar),
        }
    }
}

/// Segmented button (Material 3): pílula contornada, segmentos iguais, o
/// escolhido preenchido em tom violeta e com check.
fn segmentado(
    id: &'static str,
    altura: f32,
    segmentos: Vec<Segmento>,
    tema: &gpui_kit::component::Theme,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let ultimo = segmentos.len().saturating_sub(1);
    let borda = tema::cor(tema::BORDA);
    h_flex()
        .w_full()
        .h(px(altura))
        .rounded_full()
        .border_1()
        .border_color(borda)
        .children(segmentos.into_iter().enumerate().map(|(i, s)| {
            // Morph da seleção (Material 3): o fundo e a cor entram aos poucos e
            // o check cresce da largura zero, empurrando o rótulo para o lado.
            let selecao = transition(
                SharedString::from(format!("selecao-{id}-{i}")),
                if s.escolhido { 1_f32 } else { 0_f32 },
                Transition::new(Duration::from_millis(220)),
                window,
                cx,
            );
            let suave = 1. - (1. - selecao).powi(3);
            let com_check = altura >= tema::ALTURA_SEGMENTADO;
            let segmento = h_flex()
                .id((id, i))
                .justify_center()
                .px(px(tema::ESP_2))
                .when(i < ultimo, |d| d.border_r_1().border_color(borda))
                // O GPUI não recorta o fundo dos filhos: as pontas arredondam sozinhas.
                .when(i == 0, |d| d.rounded_l_full())
                .when(i == ultimo, |d| d.rounded_r_full())
                .cursor_pointer()
                .text_size(px(if altura < tema::ALTURA_SEGMENTADO {
                    tema::TAM_LEGENDA
                } else {
                    tema::TAM_CORPO
                }))
                .font_medium()
                .bg(tema::cor(tema::ACAO_VIOLETA).opacity(0.28 * suave))
                .text_color(misturar(
                    tema.foreground,
                    tema::cor(tema::TEXTO_ICONE),
                    suave,
                ))
                .when(!s.escolhido, |d| d.hover(|d| d.bg(tema.list_hover)))
                // No compacto o check não cabe ao lado de rótulos como "Sem
                // limite"; o preenchimento já marca a escolha.
                .when(com_check && selecao > 0., |d| {
                    d.child(
                        div()
                            .flex_shrink_0()
                            .overflow_hidden()
                            .w(px((tema::TAM_LEGENDA + tema::ESP_1) * suave))
                            .opacity(suave)
                            .child(Icon::new(IconName::Check).size(px(tema::TAM_LEGENDA))),
                    )
                })
                .child(div().truncate().child(s.rotulo))
                .on_click(s.ao_clicar);
            // Pontas em cápsula, meio retangular: a onda não vaza da pílula.
            let cantos = (
                if i == 0 { None } else { Some(0.) },
                if i == ultimo { None } else { Some(0.) },
            );
            ripple_com_forma(
                format!("onda-{id}-{i}"),
                true,
                segmento.size_full(),
                tema::cor(tema::TEXTO).opacity(0.16),
                cantos,
                window,
                cx,
            )
            .flex_1()
            .min_w_0()
            .h_full()
        }))
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
    let titulo = |texto: &'static str| rotulo(texto, &tema);
    // Cartão aninhado: um tom acima do cartão externo, sem borda.
    let cartao = || {
        superficie(&tema)
            .h_full()
            .min_w_0()
            .gap(px(tema::ESP_2))
            .p(px(tema::ESP_4))
            .rounded(px(tema::RAIO_CARTAO_INTERNO))
            .bg(tema::cor(tema::SUPERFICIE_ALTA))
    };

    let perfil = v_flex()
        .gap(px(tema::ESP_2))
        .child(titulo("Perfil de desempenho"))
        .child(segmentado(
            "qualidade-global",
            tema::ALTURA_SEGMENTADO,
            [
                (Qualidade::Desempenho, "Desempenho máximo"),
                (Qualidade::Equilibrado, "Equilibrado"),
                (Qualidade::Qualidade, "Alta fidelidade"),
            ]
            .into_iter()
            .map(|(qualidade, nome)| {
                Segmento::new(
                    nome,
                    r.qualidade == qualidade,
                    cx.listener(move |this, _, _, cx| {
                        this.ui.rascunho.qualidade = qualidade;
                        this.agendar_salvar_preferencias(cx);
                        cx.notify();
                    }),
                )
            })
            .collect(),
            &tema,
            window,
            cx,
        ));

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
        .child(titulo("Sincronização e latência"))
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
                                    this.agendar_salvar_preferencias(cx);
                                    cx.notify();
                                })),
                        ),
                ),
        )
        .when(sync_ativo, |v| {
            v.child(segmentado(
                "modo-sync",
                tema::ALTURA_SEGMENTADO_COMPACTO,
                [
                    (Sincronismo::VSync, "V-Sync"),
                    (Sincronismo::GSync, "G-SYNC"),
                ]
                .into_iter()
                .map(|(modo, nome)| {
                    Segmento::new(
                        nome,
                        r.sincronismo == modo,
                        cx.listener(move |this, _, _, cx| {
                            this.ui.rascunho.sincronismo = modo;
                            this.agendar_salvar_preferencias(cx);
                            cx.notify();
                        }),
                    )
                })
                .collect(),
                &tema,
                window,
                cx,
            ))
        })
        .when_some(janela.ui.aviso_sincronismo(), |v, aviso| {
            v.child(
                div()
                    .text_size(px(tema::TAM_LEGENDA))
                    .text_color(tema.warning)
                    .child(aviso),
            )
        });

    // Atalhos num segmented; "Outro" abre o ajuste fino para valores fora
    // dos atalhos (ex.: 165), que antes ficava sempre visível.
    const ATALHOS_FPS: [Option<u32>; 4] = [None, Some(60), Some(144), Some(240)];
    const FPS_OUTRO_INICIAL: u32 = 120;
    let personalizado = !ATALHOS_FPS.contains(&r.limite_fps);
    let mut segmentos_fps: Vec<Segmento> = ATALHOS_FPS
        .into_iter()
        .map(|fps| {
            Segmento::new(
                fps.map_or_else(|| "Sem limite".to_owned(), |fps| fps.to_string()),
                r.limite_fps == fps,
                cx.listener(move |this, _, _, cx| {
                    this.ui.rascunho.limite_fps = fps;
                    this.agendar_salvar_preferencias(cx);
                    cx.notify();
                }),
            )
        })
        .collect();
    segmentos_fps.push(Segmento::new(
        "Outro",
        personalizado,
        cx.listener(|this, _, _, cx| {
            let atual = this.ui.rascunho.limite_fps;
            if ATALHOS_FPS.contains(&atual) {
                this.ui.rascunho.limite_fps = Some(FPS_OUTRO_INICIAL);
                this.agendar_salvar_preferencias(cx);
                cx.notify();
            }
        }),
    ));
    let seletor_fps = segmentado(
        "atalho-fps-global",
        tema::ALTURA_SEGMENTADO_COMPACTO,
        segmentos_fps,
        &tema,
        window,
        cx,
    );
    let passo = |id, rotulo, delta: i32| {
        tema::botao_compacto(id)
            .ghost()
            .label(rotulo)
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(fps) = this.ui.rascunho.limite_fps {
                    this.ui.rascunho.limite_fps = Some(
                        fps.saturating_add_signed(delta)
                            .clamp(LIMITE_FPS_MIN, LIMITE_FPS_MAX),
                    );
                    this.agendar_salvar_preferencias(cx);
                    cx.notify();
                }
            }))
    };
    let limite = cartao()
        .child(titulo("Limite de FPS"))
        .child(seletor_fps)
        .when(personalizado, |v| {
            v.child(
                h_flex()
                    .justify_between()
                    .rounded(px(tema::RAIO_CHIP))
                    .bg(tema::cor(tema::SUPERFICIE_MAIS_ALTA))
                    .child(passo("fps-global-menos-5", "−5", -5))
                    .child(passo("fps-global-menos", "−", -1))
                    .child(
                        div().flex_1().text_center().font_semibold().child(
                            r.limite_fps
                                .map_or_else(String::new, |fps| format!("{fps} FPS")),
                        ),
                    )
                    .child(passo("fps-global-mais", "+", 1))
                    .child(passo("fps-global-mais-5", "+5", 5)),
            )
        });

    let colunas = if largura >= px(tema::LARGURA_GRADE_DOIS) {
        2
    } else {
        1
    };
    superficie(&tema)
        .rounded(px(tema::RAIO_CARTAO_PAINEL))
        .p(px(tema::ESP_6))
        .gap(px(tema::ESP_4))
        .child(
            h_flex()
                .justify_between()
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
                // Ação rara e secundária: link discreto no cabeçalho, no lugar
                // do antigo rodapé de botões (as escolhas já se aplicam sozinhas).
                .child(ripple(
                    "onda-restaurar",
                    true,
                    tema::botao_compacto("restaurar-prefs-globais")
                        .ghost()
                        .icon(IconName::RotateCcw)
                        .label("Restaurar padrões")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.ui.rascunho = crate::modelo::preferencias_padrao();
                            this.agendar_salvar_preferencias(cx);
                            cx.notify();
                        })),
                    window,
                    cx,
                )),
        )
        .child(perfil)
        .child(
            div()
                .grid()
                .grid_cols(colunas)
                .gap(px(tema::ESP_4))
                .children([sincronismo, limite]),
        )
        // Sem o serviço a escolha não tem como ser aplicada: fica guardada e
        // a tela diz por quê (com o serviço, salva sozinha em ~0,5 s).
        .when(
            alterado && !pode_salvar && janela.ui.ocupado.is_none(),
            |v| {
                v.child(
                    div()
                        .text_color(tema.warning)
                        .text_size(px(tema::TAM_LEGENDA))
                        .child(t::SO_COM_SERVICO),
                )
            },
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
        // Sem fundo nem divisória próprios: herda o tom do cartão expandido,
        // e o nome já está no cabeçalho logo acima.
        .px(px(tema::ESP_4))
        .pt(px(tema::ESP_1))
        .pb(px(tema::ESP_4))
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
    let tema = cx.theme();
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
