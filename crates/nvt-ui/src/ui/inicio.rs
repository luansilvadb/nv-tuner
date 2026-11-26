//! Seção Início (spec `interface-e-saude.md` §9, lote L2).
//!
//! Por enquanto delega à Visão geral legada.

use gpui_kit::*;

use super::legado::Janela;

/// Conteúdo da seção Início. Assinatura estável para o L2: recebe a janela
/// e o contexto dela e devolve o elemento já apagado de tipo.
pub(super) fn render(janela: &Janela, cx: &mut Context<Janela>) -> AnyElement {
    janela.conteudo(cx)
}
