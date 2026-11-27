//! Controle segmentado (grupo de botões exclusivos) e mistura de cores.

use super::*;

/// Interpolação linear entre duas cores (para morphs de tom).
pub(super) fn misturar(de: Hsla, para: Hsla, t: f32) -> Hsla {
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

pub(super) type AoClicar = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Um segmento de [`segmentado`].
pub(super) struct Segmento {
    rotulo: SharedString,
    escolhido: bool,
    ao_clicar: AoClicar,
}

impl Segmento {
    pub(super) fn new(
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
pub(super) fn segmentado(
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
