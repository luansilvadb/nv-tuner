//! Cartão de saúde do PC no Início.

use super::*;

/// Seção Saúde do PC do Início (só com dados de `ObterSaude`).
pub(super) fn secao_saude(s: &nvt_ipc::Saude, janela: &Janela, cx: &mut Context<Janela>) -> Div {
    let tema = cx.theme().clone();
    let titulo = titulo_cartao(t::SECAO_SAUDE);
    let caixa = superficie(&tema)
        .id("saude")
        .p(px(tema::ESP_4))
        .gap(px(tema::ESP_2));
    let linhas = match ts::secao_inicio(s) {
        SecaoSaude::TudoCerto => {
            return div().child(
                caixa.child(
                    h_flex()
                        .gap(px(tema::ESP_3))
                        .items_center()
                        .child(
                            component::Icon::new(IconName::CircleCheck)
                                .size(px(tema::ESP_4))
                                .text_color(tema.success),
                        )
                        .child(div().text_size(px(tema::TAM_CORPO)).child(ts::TUDO_CERTO)),
                ),
            );
        }
        SecaoSaude::Linhas(l) => l,
    };
    let mut lista = v_flex();
    for (i, l) in linhas.into_iter().enumerate() {
        let botao = l.botao_abrir.then(|| {
            botao_abrir(
                "abrir-inicializacao-inicio",
                ts::BOTAO_ABRIR_INICIO,
                janela,
                cx,
            )
        });
        let cor = if l.atencao {
            tema.warning
        } else {
            tema.foreground
        };
        lista = lista.child(
            h_flex()
                .gap(px(tema::ESP_4))
                .items_start()
                .py(px(tema::ESP_3))
                .when(i > 0, |d| d.border_t_1().border_color(tema.border))
                .text_size(px(tema::TAM_CORPO))
                .child(
                    div()
                        .w(px(tema::COLUNA_ROTULO))
                        .flex_none()
                        .text_color(tema.muted_foreground)
                        .child(l.rotulo),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(tema::ESP_1))
                        .child(
                            h_flex()
                                .gap(px(tema::ESP_2))
                                .items_center()
                                .when(l.atencao, |d| {
                                    d.child(
                                        component::Icon::new(IconName::TriangleAlert)
                                            .size(px(tema::ESP_4))
                                            .text_color(tema.warning),
                                    )
                                })
                                .child(div().text_color(cor).child(l.frase)),
                        )
                        .when_some(l.caminho, |d, c| d.child(legenda(c, &tema)))
                        .children(botao),
                ),
        );
    }
    div().child(caixa.child(titulo).child(lista))
}
