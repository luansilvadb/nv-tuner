//! Seção Avançado (spec `interface-e-saude.md` §9, lote L2).
//!
//! Por enquanto agrupa as telas legadas sob a subnavegação.

use gpui_kit::component::v_flex;
use gpui_kit::*;

use super::legado::Janela;
use crate::tema;

/// Conteúdo da seção Avançado. Assinatura estável para o L2: recebe a
/// janela e o contexto dela e devolve o elemento já apagado de tipo.
pub(super) fn render(janela: &Janela, cx: &mut Context<Janela>) -> AnyElement {
    v_flex()
        .gap(px(tema::ESP_4))
        .child(janela.subnavegacao_avancado(cx))
        .child(janela.conteudo(cx))
        .into_any_element()
}
