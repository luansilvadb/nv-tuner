//! Cartão de recomendações do Início.

use super::*;

pub(crate) fn recomendacoes(
    janela: &Janela,
    window: &mut Window,
    cx: &mut Context<Janela>,
) -> AnyElement {
    // Só os campos usados: evita clonar o estado inteiro a cada frame.
    let (ocupado, saude_aberta, resultado) = {
        let estado = cx.default_global::<EstadoInicio>();
        (
            estado.ocupado,
            estado.saude_aberta,
            estado.resultado.clone(),
        )
    };
    let fatos = janela.fatos();
    // Saúde do PC: cache da thread de comunicação (`ObterSaude`); sem
    // resposta, a seção não aparece e "piorou" nunca vence.
    let saude = janela
        .ui
        .dados
        .saude
        .as_ref()
        .filter(|s| mostra_saude(Some(s)));
    let v = janela.veredito();
    let texto = t::veredito(v);
    let b = botoes(v, fatos);
    let tema = cx.theme().clone();
    let otimizado_com_pausa = fatos.agente_ausente && *v == Veredito::Otimizado;
    let (icone, cor) = if otimizado_com_pausa {
        (IconName::SlidersHorizontal, tema.info)
    } else {
        (icone(v), cor_tom(tom(v), &tema))
    };

    // A aplicação de recomendações fica junto do resumo dos ajustes.
    let mut acoes = h_flex().gap(px(tema::ESP_3)).items_center().flex_wrap();
    let mut motivo = None;
    if let Some(a) = b.aplicar {
        let desabilitado = match a {
            BotaoAplicar::Habilitado => false,
            BotaoAplicar::Desabilitado(m) => {
                motivo = Some(m);
                true
            }
        };
        acoes = acoes.child(
            tema::botao_base("aplicar-recomendadas")
                .primary()
                .large()
                .h(px(tema::ALTURA_BOTAO))
                .label(t::BOTAO_APLICAR_RECOMENDADAS)
                .indisponivel(desabilitado || ocupado.is_some())
                .loading(ocupado == Some(AcaoInicio::AplicarRecomendadas))
                .on_click(cx.listener(|_, _, _, cx| executar(AcaoInicio::AplicarRecomendadas, cx))),
        );
    }
    let tem_acoes = b.aplicar.is_some();

    let cor_resultado = |tom: Tom| match tom {
        Tom::Sucesso => tema.success,
        Tom::Informacao => tema.foreground,
        Tom::Erro => tema.danger,
    };
    let saude_disponivel = saude.is_some() && !matches!(v, Veredito::Verificando);
    let saude_el = recolhivel(
        "detalhes-saude-otimizacao",
        saude_disponivel && saude_aberta,
        window,
        cx,
        |_, cx| {
            div()
                .pt(px(tema::ESP_3))
                .children(saude.map(|s| secao_saude(s, janela, cx)))
                .into_any_element()
        },
    );
    let corpo = v_flex()
        .flex_1()
        .min_w_0()
        .gap(px(tema::ESP_2))
        .child(
            v_flex()
                .gap(px(tema::ESP_1))
                .child(
                    div()
                        .text_size(px(tema::TAM_SUBTITULO))
                        .font_semibold()
                        .child(if otimizado_com_pausa {
                            t::OTIMIZACAO_DO_SISTEMA.into()
                        } else {
                            texto.titulo
                        }),
                )
                .when(!otimizado_com_pausa, |d| {
                    d.child(explicacao(texto.frase, &tema))
                }),
        )
        .when(tem_acoes, |c| c.child(acoes))
        .when(fatos.agente_ausente && !b.voltar, |c| {
            c.child(
                h_flex()
                    .items_start()
                    .gap(px(tema::ESP_2))
                    .child(
                        Icon::new(IconName::Info)
                            .size(px(tema::ESP_4))
                            .flex_shrink_0()
                            .text_color(tema::cor(tema::DESTAQUE)),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .text_size(px(tema::TAM_LEGENDA))
                            .child(div().font_semibold().child(tj::AJUSTES_WINDOWS_PAUSADOS))
                            .child(legenda(tj::RETOMAR_AJUSTES_WINDOWS, &tema)),
                    ),
            )
        })
        .when_some(motivo, |c, m| c.child(legenda(m, &tema)))
        .when(ocupado.is_some(), |c| c.child(legenda(t::AGUARDE, &tema)))
        .when_some(resultado, |c, (tom, m)| {
            c.child(
                div()
                    .text_size(px(tema::TAM_CORPO))
                    .text_color(cor_resultado(tom))
                    .child(m),
            )
        });
    superficie(&tema)
        .id("veredito")
        .p(px(tema::ESP_4))
        .child(
            h_flex()
                .gap(px(tema::ESP_3))
                .items_start()
                .child(icone_destaque(icone, cor))
                .child(corpo),
        )
        .when(saude_disponivel, |c| {
            c.child(
                h_flex()
                    .justify_between()
                    .flex_wrap()
                    .gap(px(tema::ESP_2))
                    .mt(px(tema::ESP_3))
                    .pt(px(tema::ESP_2))
                    .border_t_1()
                    .border_color(tema.border)
                    .child(
                        tema::botao_base("detalhes-saude")
                            .ghost()
                            .small()
                            .label(t::SECAO_SAUDE)
                            .icon(if saude_aberta {
                                IconName::ChevronUp
                            } else {
                                IconName::ChevronDown
                            })
                            .on_click(cx.listener(|_, _, _, cx| {
                                let estado = cx.global_mut::<EstadoInicio>();
                                estado.saude_aberta = !estado.saude_aberta;
                                cx.notify();
                            })),
                    ),
            )
        })
        .children(saude_el)
        .into_any_element()
}
