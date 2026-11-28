//! Seção Jogos (spec `interface-e-saude.md` §5.4, R-UI-12): lista limpa com
//! nome, selo competitivo e um status; detalhe ao clicar com o que o app fez,
//! "Faça no jogo", "Arquivo do jogo" e detalhes técnicos recolhidos.

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

use super::estrutura::{
    Janela, chip_status, estado_vazio, explicacao, legenda, legenda_cor, ponto_status, rotulo,
    superficie, titulo_cartao,
};
use super::movimento::{recolhivel, ripple, ripple_com_forma};
use crate::arquivo_jogo as aj;
use crate::modelo::{Comando, StatusJogo, status_jogo};
use crate::tema::{self, EstadoBotao as _};
use crate::textos as t;
use crate::textos::jogos as tj;
use gpui_kit::assets::IconName;

mod detalhe;
mod preferencias;
mod segmentado;
use detalhe::*;
use preferencias::*;
use segmentado::*;

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
                cx.new(|cx| InputState::new(window, cx).placeholder(tj::BUSCAR_PLACEHOLDER));
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
    let pode = janela.ui.pode_executar(&Comando::Reescanear);

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
        this.enviar(Comando::Reescanear, cx);
    }));

    let campo_busca = Styled::h(
        Input::new(&busca)
            .appearance(false)
            .aria_label(tj::BUSCAR_ROTULO)
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
                    .label(tj::BOTAO_OTIMIZAR)
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
            tj::jogos_detectados(jogos.len())
        } else {
            tj::jogos_encontrados(filtrados.len(), jogos.len())
        }));
    let v = v.child(v_flex().child(cabecalho).children(recomendacoes));
    if filtrados.is_empty() {
        return v
            .child(superficie(&tema).child(estado_vazio(
                IconName::Gamepad2,
                if jogos.is_empty() {
                    tj::NENHUM_JOGO
                } else {
                    tj::NENHUM_NA_BUSCA
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
                            .child(ponto_status(tema::TAM_PONTO_STATUS, cor_status).flex_shrink_0())
                            .child(legenda(
                                if j.api.is_empty() {
                                    l.texto_status
                                } else {
                                    tj::status_com_api(&l.texto_status, t::api_grafica(&j.api))
                                },
                                &tema,
                            )),
                    ),
            )
            .child(chip_status(l.status == StatusJogo::Otimizado, &tema))
            .child(
                legenda(
                    Icon::new(IconName::ChevronDown)
                        .size(px(tema::TAM_CORPO))
                        .rotate(radians(std::f32::consts::PI * expansao)),
                    &tema,
                )
                .w(px(tema::ESP_4)),
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

#[cfg(test)]
mod testes;
