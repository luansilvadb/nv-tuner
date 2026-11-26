//! Boas-vindas do primeiro uso (spec `interface-e-saude.md` §5.2, R-UI-09).
//!
//! Uma tela, um botão "Começar". A marca fica em
//! `%LOCALAPPDATA%\nv-tuner\ui.json` ([`crate::local`]); quem decide exibir
//! é o Início.

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Sizable, StyledExt, v_flex};
use gpui_kit::*;

use super::estrutura::Janela;
use crate::tema;
use crate::textos::boas_vindas as t;

/// Conteúdo das boas-vindas.
pub(super) fn render(_janela: &Janela, cx: &mut Context<Janela>) -> AnyElement {
    let tema = cx.theme().clone();
    v_flex()
        .id("boas-vindas")
        .w_full()
        .items_center()
        .gap(px(tema::ESP_6))
        .py(px(tema::ESP_8 * 2.))
        .child(
            div()
                .size(px(72.))
                .rounded(px(tema::ESP_4))
                .bg(tema::cor(tema::DESTAQUE)),
        )
        .child(
            div()
                .text_size(px(tema::TAM_TITULO_PAGINA))
                .font_semibold()
                .child(t::TITULO),
        )
        .child(
            v_flex()
                .items_center()
                .gap(px(tema::ESP_1))
                .text_size(px(tema::TAM_CORPO))
                .text_color(tema.muted_foreground)
                .child(t::PARAGRAFO_AJUSTA)
                .child(t::PARAGRAFO_JOGO_ABERTO)
                .child(t::PARAGRAFO_DESFAZER),
        )
        .child(
            Button::new("comecar")
                .primary()
                .large()
                .label(t::BOTAO_COMECAR)
                .on_click(cx.listener(|_, _, _, cx| super::inicio::concluir_boas_vindas(cx))),
        )
        .into_any_element()
}
