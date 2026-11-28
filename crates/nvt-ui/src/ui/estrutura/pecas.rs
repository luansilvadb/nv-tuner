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

/// Texto explicativo: corpo em tom apagado.
pub(crate) fn explicacao(texto: impl IntoElement, tema: &Theme) -> Div {
    div()
        .text_size(px(tema::TAM_CORPO))
        .text_color(tema.muted_foreground)
        .child(texto)
}

/// Legenda: texto pequeno em tom apagado.
pub(crate) fn legenda(conteudo: impl IntoElement, tema: &Theme) -> Div {
    legenda_cor(conteudo, tema.muted_foreground)
}

/// Legenda com cor própria (ex.: avisos em `tema.warning`).
pub(crate) fn legenda_cor(conteudo: impl IntoElement, cor: Hsla) -> Div {
    div()
        .text_size(px(tema::TAM_LEGENDA))
        .text_color(cor)
        .child(conteudo)
}

/// Título de cartão: corpo em semibold.
pub(crate) fn titulo_cartao(texto: impl IntoElement) -> Div {
    div()
        .text_size(px(tema::TAM_CORPO))
        .font_semibold()
        .child(texto)
}

/// Ponto de status: círculo cheio do tamanho e da cor dados.
pub(crate) fn ponto_status(tamanho: f32, cor: Hsla) -> Div {
    div().size(px(tamanho)).rounded_full().bg(cor)
}

/// Chip de status (Material 3): tonal quando ativo, contornado quando
/// pendente; ícone em vez de glifos de texto.
pub(crate) fn chip_status(ativo: bool, tema: &Theme) -> Div {
    let (icone, texto, cor, fundo, borda) = if ativo {
        let cor = tema::cor(tema::TEXTO_ICONE);
        (
            IconName::Check,
            crate::textos::jogos::CHIP_ATIVO,
            cor,
            cor.opacity(0.14),
            tema.transparent,
        )
    } else {
        (
            IconName::Clock,
            crate::textos::jogos::CHIP_PENDENTE,
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
