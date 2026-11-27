//! Boas-vindas do primeiro uso (spec `interface-e-saude.md` §5.2, R-UI-09).
//!
//! Uma tela, um botão "Começar". A marca fica em
//! `%LOCALAPPDATA%\nv-tuner\ui.json` ([`crate::local`]); quem decide exibir
//! é o Início.
//!
//! Hierarquia: logo, título (28), frase principal (20), duas garantias com
//! ícone (14) e o botão primário, tudo centrado numa coluna estreita.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::*;

use super::estrutura::{Janela, logo};
use crate::tema;
use crate::textos::boas_vindas as t;

/// Conteúdo das boas-vindas.
pub(super) fn render(_janela: &Janela, cx: &mut Context<Janela>) -> AnyElement {
    let tema = cx.theme();
    let garantia = |texto: &'static str| {
        h_flex()
            .gap(px(tema::ESP_3))
            .items_center()
            .child(
                Icon::new(IconName::CircleCheck)
                    .size(px(tema::ESP_4))
                    .text_color(tema::cor(tema::DESTAQUE)),
            )
            .child(
                div()
                    .text_size(px(tema::TAM_CORPO))
                    .text_color(tema.muted_foreground)
                    .child(texto),
            )
    };
    v_flex()
        .id("boas-vindas")
        .w(px(tema::LARGURA_TEXTO))
        .items_center()
        .child(logo(tema::LOGO_BOAS_VINDAS))
        .child(
            div()
                .mt(px(tema::ESP_8))
                .text_size(px(tema::TAM_TITULO_PAGINA))
                .font_semibold()
                .text_center()
                .child(t::TITULO),
        )
        .child(
            div()
                .mt(px(tema::ESP_3))
                .text_size(px(tema::TAM_SUBTITULO))
                .text_center()
                .child(t::PARAGRAFO_AJUSTA),
        )
        .child(
            v_flex()
                .mt(px(tema::ESP_8))
                .gap(px(tema::ESP_3))
                .child(garantia(t::PARAGRAFO_JOGO_ABERTO))
                .child(garantia(t::PARAGRAFO_DESFAZER)),
        )
        .child(
            div().mt(px(tema::ESP_12)).child(
                tema::botao_base("comecar")
                    .primary()
                    .large()
                    .h(px(tema::ALTURA_BOTAO_DESTAQUE))
                    .min_w(px(tema::LARGURA_BOTAO_DESTAQUE))
                    .label(t::BOTAO_COMECAR)
                    .on_click(cx.listener(|_, _, _, cx| super::inicio::concluir_boas_vindas(cx))),
            ),
        )
        .into_any_element()
}
