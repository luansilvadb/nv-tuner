//! Detalhe expandido de um jogo e cartão do arquivo do jogo.

use super::*;

pub(super) fn detalhe(
    janela: &Janela,
    i: usize,
    j: &Jogo,
    tec: bool,
    cx: &mut Context<Janela>,
) -> Div {
    let tema = cx.theme().clone();
    let titulo = |s: &'static str| {
        div()
            .w(px(tema::COLUNA_ROTULO))
            .flex_shrink_0()
            .text_size(px(tema::TAM_CORPO))
            .font_semibold()
            .text_color(tema.muted_foreground)
            .child(s)
    };
    let bloco = |rotulo: &'static str, linhas: Vec<String>, vazio: &'static str| {
        let corpo = if linhas.is_empty() {
            v_flex().child(
                div()
                    .text_size(px(tema::TAM_CORPO))
                    .text_color(tema.muted_foreground)
                    .child(vazio),
            )
        } else {
            v_flex().gap(px(tema::ESP_1)).children(
                linhas
                    .into_iter()
                    .map(|s| div().text_size(px(tema::TAM_CORPO)).child(s)),
            )
        };
        h_flex()
            .items_start()
            .gap(px(tema::ESP_3))
            .child(titulo(rotulo))
            .child(corpo.flex_1())
    };
    let mut d = v_flex()
        .gap(px(tema::ESP_3))
        // Sem fundo nem divisória próprios: herda o tom do cartão expandido,
        // e o nome já está no cabeçalho logo acima.
        .px(px(tema::ESP_4))
        .pt(px(tema::ESP_1))
        .pb(px(tema::ESP_4))
        .child(bloco(tj::DETALHE_FEITO, feito(j), tj::DETALHE_NADA_FEITO))
        .child(bloco(tj::DETALHE_FACA, passos(j), tj::DETALHE_NADA_A_FAZER));
    if let Some(a) = arquivo(janela, i, j, cx) {
        d = d.child(
            h_flex()
                .items_start()
                .gap(px(tema::ESP_3))
                .child(titulo(aj::SECAO_ARQUIVO_JOGO))
                .child(a.flex_1()),
        );
    }
    d = d.child(
        h_flex().child(
            tema::botao_compacto(("tecnicos", i))
                .ghost()
                .label(if tec {
                    tj::BOTAO_OCULTAR_TECNICOS
                } else {
                    tj::BOTAO_DETALHES_TECNICOS
                })
                .on_click(cx.listener(|_, _, _, cx| {
                    let s = cx.default_global::<Selecao>();
                    s.tecnicos = !s.tecnicos;
                    cx.notify();
                })),
        ),
    );
    if tec {
        d = d.child(
            v_flex()
                .gap(px(tema::ESP_1))
                .children(tecnicos(j).into_iter().map(|(r, v)| {
                    h_flex()
                        .gap(px(tema::ESP_3))
                        .text_size(px(tema::TAM_LEGENDA))
                        .child(
                            div()
                                .w(px(tema::COLUNA_ROTULO))
                                .text_color(tema.muted_foreground)
                                .child(r),
                        )
                        .child(div().child(v))
                })),
        );
    }
    d
}

/// "Arquivo do jogo": arquivo, antes/depois, backup e Reverter por chave.
pub(super) fn arquivo(
    janela: &Janela,
    i: usize,
    j: &Jogo,
    cx: &mut Context<Janela>,
) -> Option<Div> {
    let c = j.config_jogo.as_ref()?;
    let tema = cx.theme();
    let pode = janela.ui.pode_escrever() && cx.has_global::<Envio>();
    let sub = |s: String| {
        div()
            .text_size(px(tema::TAM_LEGENDA))
            .text_color(tema.muted_foreground)
            .child(s)
    };
    let mut v = v_flex().gap(px(tema::ESP_2));
    for (k, a) in c.arquivos.iter().enumerate() {
        let mut b = v_flex()
            .gap(px(tema::ESP_1))
            .child(
                div()
                    .text_size(px(tema::TAM_CORPO))
                    .font_semibold()
                    .child(aj::nome_arquivo(&a.rotulo.0).to_owned()),
            )
            .children(
                a.conta
                    .as_ref()
                    .map(|c| sub(format!("{}: {c}", aj::ROTULO_CONTA))),
            )
            .child(sub(format!(
                "{}: {}",
                aj::ROTULO_BACKUP,
                aj::backup(a.backup)
            )))
            .when(a.mudancas.is_empty(), |d| {
                d.child(sub(aj::SEM_MUDANCAS_ARQUIVO.into()))
            });
        for (n, m) in a.mudancas.iter().enumerate() {
            let destino = (m.status == StatusMudanca::Aplicada)
                .then(|| aj::destino_reverter(a, &m.setting))
                .flatten();
            let mut l = h_flex().gap(px(tema::ESP_3)).justify_between().child(
                div()
                    .text_size(px(tema::TAM_CORPO))
                    .flex_1()
                    .min_w_0()
                    .child(aj::linha_mudanca(m)),
            );
            if let Some(destino) = destino {
                l = l.child(
                    tema::botao_compacto(("reverter-jogo", i * 10_000 + k * 100 + n))
                        .outline()
                        .label(t::BOTAO_REVERTER)
                        .indisponivel(!pode)
                        .when(!pode, |b| b.tooltip(t::SO_COM_SERVICO))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            enviar(
                                this,
                                Comando::ReverterConfigJogo {
                                    destino: destino.clone(),
                                },
                                cx,
                            )
                        })),
                );
            }
            b = b.child(l);
        }
        v = v.child(b);
    }
    Some(v)
}
