//! Cartão das preferências globais da tela Jogos.

use super::*;

/// Segmentos de uma opção exclusiva das preferências: o clique grava o valor
/// no rascunho com `definir`, agenda o salvamento e redesenha.
fn segmentos_opcao<T, R>(
    opcoes: impl IntoIterator<Item = (T, R)>,
    atual: T,
    definir: fn(&mut Janela, T),
    cx: &Context<Janela>,
) -> Vec<Segmento>
where
    T: Copy + PartialEq + 'static,
    R: Into<SharedString>,
{
    opcoes
        .into_iter()
        .map(|(valor, rotulo)| {
            Segmento::new(
                rotulo,
                atual == valor,
                cx.listener(move |this, _, _, cx| {
                    definir(this, valor);
                    this.agendar_salvar_preferencias(cx);
                    cx.notify();
                }),
            )
        })
        .collect()
}

pub(super) fn preferencias_globais(
    janela: &Janela,
    largura: Pixels,
    window: &mut Window,
    cx: &mut Context<Janela>,
) -> Div {
    let tema = cx.theme().clone();
    let r = &janela.ui.rascunho;
    let alterado = janela.ui.rascunho_alterado();
    let pode_salvar = janela
        .ui
        .pode_executar(&Comando::DefinirPreferencias(r.clone()));
    let titulo = |texto: &'static str| rotulo(texto, &tema);
    // Cartão aninhado: um tom acima do cartão externo, sem borda.
    let cartao = || {
        superficie(&tema)
            .h_full()
            .min_w_0()
            .gap(px(tema::ESP_2))
            .p(px(tema::ESP_4))
            .rounded(px(tema::RAIO_CARTAO_INTERNO))
            .bg(tema::cor(tema::SUPERFICIE_ALTA))
    };

    let perfil = v_flex()
        .gap(px(tema::ESP_2))
        .child(titulo(tj::ROTULO_PERFIL_DESEMPENHO))
        .child(segmentado(
            "qualidade-global",
            tema::ALTURA_SEGMENTADO,
            segmentos_opcao(
                [
                    (Qualidade::Desempenho, tj::QUALIDADE_DESEMPENHO),
                    (Qualidade::Equilibrado, tj::QUALIDADE_EQUILIBRADO),
                    (Qualidade::Qualidade, tj::QUALIDADE_FIDELIDADE),
                ],
                r.qualidade,
                |this, qualidade| this.ui.rascunho.qualidade = qualidade,
                cx,
            ),
            &tema,
            window,
            cx,
        ));

    let reflex = janela
        .ui
        .dados
        .estado
        .as_ref()
        .and_then(|estado| estado.gpu.as_ref())
        .map(|gpu| gpu.capacidades.reflex);
    let sync_ativo = r.sincronismo != Sincronismo::Desligado;
    let foco_sync = window
        .use_keyed_state("foco-sincronismo", cx, |_, cx| cx.focus_handle())
        .read(cx)
        .clone();
    let foco_visivel = window.last_input_was_keyboard() && foco_sync.contains_focused(window, cx);
    let sincronismo = cartao()
        .child(titulo(tj::ROTULO_SINCRONIZACAO))
        .child(
            h_flex()
                .justify_between()
                .gap(px(tema::ESP_2))
                .child(
                    v_flex()
                        .child(div().font_semibold().child(tj::NOME_REFLEX))
                        .child(legenda(tj::SUPORTE_GPU, &tema)),
                )
                .child(titulo(match reflex {
                    Some(true) => tj::REFLEX_DISPONIVEL,
                    Some(false) => tj::REFLEX_INDISPONIVEL,
                    None => tj::REFLEX_DESCONHECIDO,
                })),
        )
        .child(
            h_flex()
                .justify_between()
                .gap(px(tema::ESP_2))
                .pt(px(tema::ESP_2))
                .child(div().font_semibold().child(tj::SINCRONISMO_VERTICAL))
                .child(
                    div()
                        .track_focus(&foco_sync)
                        .rounded_full()
                        .border_1()
                        .border_color(if foco_visivel {
                            tema::cor(tema::BORDA_FOCO)
                        } else {
                            tema.transparent
                        })
                        .on_mouse_down_out(move |_, window, cx| {
                            if foco_sync.contains_focused(window, cx) {
                                window.blur(cx);
                            }
                        })
                        .child(
                            Switch::new("sincronismo-global")
                                .cursor_pointer()
                                .focus_ring(false)
                                .color(tema::cor(tema::ACAO_VIOLETA))
                                .accessibility_label(tj::SINCRONISMO_VERTICAL)
                                .checked(sync_ativo)
                                .on_click(cx.listener(|this, ligado: &bool, _, cx| {
                                    this.ui.rascunho.sincronismo = if *ligado {
                                        let atual = this.ui.preferencias_atuais().sincronismo;
                                        if atual == Sincronismo::GSync {
                                            atual
                                        } else {
                                            Sincronismo::VSync
                                        }
                                    } else {
                                        Sincronismo::Desligado
                                    };
                                    this.agendar_salvar_preferencias(cx);
                                    cx.notify();
                                })),
                        ),
                ),
        )
        .when(sync_ativo, |v| {
            v.child(segmentado(
                "modo-sync",
                tema::ALTURA_SEGMENTADO_COMPACTO,
                segmentos_opcao(
                    [
                        (Sincronismo::VSync, tj::MODO_VSYNC),
                        (Sincronismo::GSync, tj::MODO_GSYNC),
                    ],
                    r.sincronismo,
                    |this, modo| this.ui.rascunho.sincronismo = modo,
                    cx,
                ),
                &tema,
                window,
                cx,
            ))
        })
        .when_some(janela.ui.aviso_sincronismo(), |v, aviso| {
            v.child(legenda_cor(aviso, tema.warning))
        });

    // Atalhos num segmented; "Outro" abre o ajuste fino para valores fora
    // dos atalhos (ex.: 165), que antes ficava sempre visível.
    const ATALHOS_FPS: [Option<u32>; 4] = [None, Some(60), Some(144), Some(240)];
    const FPS_OUTRO_INICIAL: u32 = 120;
    let personalizado = !ATALHOS_FPS.contains(&r.limite_fps);
    let mut segmentos_fps = segmentos_opcao(
        ATALHOS_FPS.map(|fps| {
            (
                fps,
                fps.map_or_else(|| tj::FPS_SEM_LIMITE.to_owned(), |fps| fps.to_string()),
            )
        }),
        r.limite_fps,
        |this, fps| this.ui.rascunho.limite_fps = fps,
        cx,
    );
    segmentos_fps.push(Segmento::new(
        tj::FPS_OUTRO,
        personalizado,
        cx.listener(|this, _, _, cx| {
            let atual = this.ui.rascunho.limite_fps;
            if ATALHOS_FPS.contains(&atual) {
                this.ui.rascunho.limite_fps = Some(FPS_OUTRO_INICIAL);
                this.agendar_salvar_preferencias(cx);
                cx.notify();
            }
        }),
    ));
    let seletor_fps = segmentado(
        "atalho-fps-global",
        tema::ALTURA_SEGMENTADO_COMPACTO,
        segmentos_fps,
        &tema,
        window,
        cx,
    );
    let passo = |id, rotulo, delta: i32| {
        tema::botao_compacto(id)
            .ghost()
            .label(rotulo)
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(fps) = this.ui.rascunho.limite_fps {
                    this.ui.rascunho.limite_fps = Some(
                        fps.saturating_add_signed(delta)
                            .clamp(LIMITE_FPS_MIN, LIMITE_FPS_MAX),
                    );
                    this.agendar_salvar_preferencias(cx);
                    cx.notify();
                }
            }))
    };
    let limite = cartao()
        .child(titulo(tj::ROTULO_LIMITE_FPS))
        .child(seletor_fps)
        .when(personalizado, |v| {
            v.child(
                h_flex()
                    .justify_between()
                    .rounded(px(tema::RAIO_CHIP))
                    .bg(tema::cor(tema::SUPERFICIE_MAIS_ALTA))
                    .child(passo("fps-global-menos-5", "−5", -5))
                    .child(passo("fps-global-menos", "−", -1))
                    .child(
                        div()
                            .flex_1()
                            .text_center()
                            .font_semibold()
                            .child(r.limite_fps.map_or_else(String::new, tj::fps)),
                    )
                    .child(passo("fps-global-mais", "+", 1))
                    .child(passo("fps-global-mais-5", "+5", 5)),
            )
        });

    let colunas = if largura >= px(tema::LARGURA_GRADE_DOIS) {
        2
    } else {
        1
    };
    superficie(&tema)
        .rounded(px(tema::RAIO_CARTAO_PAINEL))
        .p(px(tema::ESP_6))
        .gap(px(tema::ESP_4))
        .child(
            h_flex()
                .justify_between()
                .child(
                    v_flex()
                        .child(div().font_semibold().child(tj::PREFS_TITULO))
                        .child(legenda(tj::PREFS_SUBTITULO, &tema)),
                )
                // Ação rara e secundária: link discreto no cabeçalho, no lugar
                // do antigo rodapé de botões (as escolhas já se aplicam sozinhas).
                .child(ripple(
                    "onda-restaurar",
                    true,
                    tema::botao_compacto("restaurar-prefs-globais")
                        .ghost()
                        .icon(IconName::RotateCcw)
                        .label(tj::BOTAO_RESTAURAR_PADROES)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.ui.rascunho = crate::modelo::preferencias_padrao();
                            this.agendar_salvar_preferencias(cx);
                            cx.notify();
                        })),
                    window,
                    cx,
                )),
        )
        .child(perfil)
        .child(
            div()
                .grid()
                .grid_cols(colunas)
                .gap(px(tema::ESP_4))
                .children([sincronismo, limite]),
        )
        // Sem o serviço a escolha não tem como ser aplicada: fica guardada e
        // a tela diz por quê (com o serviço, salva sozinha em ~0,5 s).
        .when(
            alterado && !pode_salvar && janela.ui.ocupado.is_none(),
            |v| v.child(legenda_cor(t::SO_COM_SERVICO, tema.warning)),
        )
}
