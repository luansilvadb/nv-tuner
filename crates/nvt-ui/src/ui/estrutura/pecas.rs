//! Peças visuais compartilhadas pelas seções.

use super::*;

// ---------------------------------------------------------------------------
// Peças
// ---------------------------------------------------------------------------

/// Superfície comum a todo cartão e lista: mesmo canto e fundo tonal, sem
/// borda (Material 3: o nível vem do tom).
pub(crate) fn superficie(tema: &Theme) -> Div {
    v_flex()
        .w_full()
        .rounded(px(tema::RAIO_CARTAO))
        .bg(tema.group_box)
}

/// Rótulo de grupo (Material 3 *Label medium*): 12 px, peso médio, só a
/// primeira letra maiúscula.
pub(crate) fn rotulo(texto: impl Into<SharedString>, tema: &Theme) -> Div {
    div()
        .text_size(px(tema::TAM_LEGENDA))
        .font_medium()
        .text_color(tema.muted_foreground)
        .child(texto.into())
}

/// Ícone dentro de um círculo tingido com a cor do estado.
pub(crate) fn icone_destaque(icone: IconName, cor: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .size(px(tema::ICONE_DESTAQUE))
        .rounded_full()
        .bg(cor.opacity(0.14))
        .flex()
        .items_center()
        .justify_center()
        .child(
            Icon::new(icone)
                .size(px(tema::ICONE_DESTAQUE_INTERNO))
                .text_color(cor),
        )
}

/// Estado vazio: ícone, frase e (opcional) ação, centralizados.
pub(crate) fn estado_vazio(
    icone: IconName,
    frase: &'static str,
    acao: Option<AnyElement>,
    tema: &Theme,
) -> Div {
    v_flex()
        .w_full()
        .items_center()
        .gap(px(tema::ESP_4))
        .py(px(tema::ESP_8))
        .child(icone_destaque(icone, tema.muted_foreground))
        .child(
            div()
                .text_size(px(tema::TAM_CORPO))
                .text_color(tema.muted_foreground)
                .text_center()
                .child(frase),
        )
        .children(acao)
}

#[cfg(feature = "medicao-partidas")]
pub(crate) fn explicacao(texto: &'static str, tema: &Theme) -> Div {
    div()
        .text_size(px(tema::TAM_CORPO))
        .text_color(tema.muted_foreground)
        .child(texto)
}
