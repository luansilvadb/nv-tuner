//! Boas-vindas do primeiro uso (spec `interface-e-saude.md` §9, lote L2).
//!
//! Por enquanto não é exibida.

#![allow(dead_code)]

use gpui_kit::*;

use super::legado::Janela;

/// Conteúdo das boas-vindas. Assinatura estável para o L2: recebe a janela
/// e o contexto dela e devolve o elemento já apagado de tipo. Hoje vazio.
pub(super) fn render(_janela: &Janela, _cx: &mut Context<Janela>) -> AnyElement {
    Empty.into_any_element()
}
