//! Navegação lateral, relógio, linha de estado, topo e título da página.

use super::*;

impl Janela {
    pub(crate) fn ir_secao(&mut self, secao: Secao, cx: &mut Context<Self>) {
        if self.secao != secao {
            self.troca_secao = Some(Instant::now());
        }
        self.secao = secao;
        cx.notify();
    }

    pub(super) fn largura_lateral_alvo(&self) -> f32 {
        if self.lateral_recolhida {
            tema::LARGURA_LATERAL_RECOLHIDA
        } else {
            tema::LARGURA_LATERAL
        }
    }

    pub(super) fn controle_lateral(
        &self,
        largura: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let recolhida = self.lateral_recolhida;
        let abertura = abertura_lateral(largura);
        let abertura_seta = spring(
            "morph-seta-lateral",
            if recolhida { 0_f32 } else { 1_f32 },
            Spring::new(Duration::from_millis(440)).with_damping(0.54),
            window,
            cx,
        );
        let rotulo = if recolhida {
            "Expandir menu"
        } else {
            "Recolher menu"
        };
        let margem_recolhida =
            (tema::LARGURA_LATERAL_RECOLHIDA - tema::TAM_BOTAO_ICONE) / 2. - tema::ESP_3;
        let deslocamento = px(margem_recolhida * (1. - abertura));
        let cor = cx.theme().muted_foreground;
        let controle = tema::botao_icone("recolher-menu", seta_lateral(abertura_seta, cor))
            .ghost()
            .text_color(cor)
            .accessibility_label(rotulo)
            .tooltip(rotulo)
            .on_click(cx.listener(|this, _, _, cx| {
                this.lateral_recolhida = !this.lateral_recolhida;
                cx.notify();
            }));
        let cor_ripple = tema::cor(tema::COR_RIPPLE_MENU).opacity(0.18);
        h_flex()
            .w_full()
            .flex_shrink_0()
            .pl(deslocamento)
            .child(ripple_com_cor(
                "onda-recolher-menu",
                true,
                controle,
                cor_ripple,
                window,
                cx,
            ))
    }

    pub(super) fn barra_lateral(
        &self,
        largura: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tema = cx.theme().clone();
        let recolhida = self.lateral_recolhida;
        let abertura = abertura_lateral(largura);
        let alvo = if recolhida { 0_f32 } else { 1_f32 };
        let abertura_relogio = spring(
            "altura-relogio-lateral",
            alvo,
            Spring::new(Duration::from_millis(510)).with_damping(0.92),
            window,
            cx,
        )
        .clamp(0., 1.);
        let opacidade_texto = ((abertura - 0.2) / 0.8).clamp(0., 1.).powi(2);
        // O recuo central desconta o preenchimento da barra e a borda do item.
        let largura_recolhida = largura.min(px(tema::LARGURA_LATERAL_RECOLHIDA));
        let recuo_central = (largura_recolhida - px(tema::ESP_3) * 2. - px(tema::ESP_6)) / 2.
            - px(tema::LINHA_FINA);
        let recuo_icone = recuo_central * (1. - abertura) + px(tema::ESP_4) * abertura;
        let cor_ripple = tema::cor(tema::COR_RIPPLE_MENU).opacity(0.18);
        let itens: Vec<_> = Secao::visiveis()
            .iter()
            .map(|&secao| {
                let ativo = self.secao == secao;
                let selecao = transition(
                    SharedString::from(format!("selecao-lateral-{}", secao.chave())),
                    if ativo { 1_f32 } else { 0_f32 },
                    Transition::new(Duration::from_millis(160)),
                    window,
                    cx,
                );
                let item = h_flex()
                    .w_full()
                    .id(secao.chave())
                    .aria_label(secao.titulo())
                    .when(recolhida, |v| {
                        v.tooltip(move |window, cx| Tooltip::new(secao.titulo()).build(window, cx))
                    })
                    .h(px(tema::ALTURA_ITEM_MENU))
                    .gap(px(tema::ESP_4))
                    .pl(recuo_icone)
                    .pr(px(tema::ESP_4))
                    .overflow_hidden()
                    .border_1()
                    .border_color(transparent_black())
                    .rounded_full()
                    .focusable()
                    .tab_stop(true)
                    .focus_visible(|v| v.border_color(tema::cor(tema::BORDA_FOCO_MENU)))
                    .cursor_pointer()
                    .font_semibold()
                    .text_color(tema.muted_foreground)
                    .bg(tema::cor(tema::FUNDO_ITEM_MENU_ATIVO).opacity(selecao))
                    .when(ativo, |v| {
                        v.text_color(tema::cor(tema::TEXTO_ITEM_MENU_ATIVO))
                    })
                    // Item inativo (Material 3, camada de estado): sob o cursor
                    // ou pressionado ganha só um véu neutro, e texto e ícone
                    // passam à cor plena. Não herda o azul do item ativo.
                    .hover(|v| {
                        if ativo {
                            v.bg(tema::cor(tema::FUNDO_ITEM_MENU_ATIVO_HOVER))
                        } else {
                            v.bg(tema::cor(tema::TEXTO).opacity(0.08))
                                .text_color(tema::cor(tema::TEXTO))
                        }
                    })
                    .active(|v| {
                        if ativo {
                            v.bg(tema::cor(tema::FUNDO_ITEM_MENU_PRESSIONADO))
                        } else {
                            v.bg(tema::cor(tema::TEXTO).opacity(0.12))
                                .text_color(tema::cor(tema::TEXTO))
                        }
                    })
                    .child(
                        Icon::new(secao.icone())
                            .size(px(tema::ESP_6))
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .opacity(opacidade_texto)
                            .child(secao.titulo()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.ir_secao(secao, cx)));
                ripple_com_cor(secao.chave(), true, item, cor_ripple, window, cx).w_full()
            })
            .collect();
        let controle = self.controle_lateral(largura, window, cx);
        v_flex()
            .w(largura)
            .overflow_hidden()
            .h_full()
            .flex_shrink_0()
            .px(px(tema::ESP_3))
            .pt(px(tema::ESP_3))
            .gap(px(tema::ESP_2))
            .child(controle)
            .child(
                v_flex()
                    .w_full()
                    .child(
                        div()
                            .relative()
                            .overflow_hidden()
                            .h(px(
                                (tema::ALTURA_RELOGIO_LATERAL + tema::ESP_2) * abertura_relogio
                            ))
                            .child(
                                div()
                                    .absolute()
                                    .left((largura - px(tema::LARGURA_LATERAL)) / 2.)
                                    .w(px(tema::LARGURA_LATERAL - 2. * tema::ESP_3))
                                    .opacity(opacidade_texto * abertura_relogio)
                                    .child(self.relogio_lateral(&tema)),
                            ),
                    )
                    .child(v_flex().w_full().gap(px(tema::ESP_2)).children(itens)),
            )
            .children({
                #[cfg(feature = "medicao-partidas")]
                {
                    (abertura > 0.).then(|| {
                        div()
                            .px(px(tema::ESP_2))
                            .text_color(tema.muted_foreground)
                            .opacity(opacidade_texto)
                            .child(t::partidas::BUILD_DE_TESTE)
                    })
                }
                #[cfg(not(feature = "medicao-partidas"))]
                {
                    None::<Div>
                }
            })
    }

    /// Fatos do estado atual (veredito e linha de estado), calculados uma
    /// vez por desenho.
    pub(crate) fn fatos(&self) -> &Fatos {
        &self.painel.0
    }

    /// Veredito do estado atual, calculado uma vez por desenho.
    pub(crate) fn veredito(&self) -> &Veredito {
        &self.painel.1
    }

    pub(super) fn atualizar_painel(&mut self) {
        let fatos = Fatos::de_ui(&self.ui, self.ui.aviso_atualizado().is_some());
        let v = veredito(&fatos, self.ui.saude().as_ref());
        self.painel = (fatos, v);
    }

    /// Linha de estado única no topo do conteúdo (R-UI-05), só quando
    /// acrescenta algo ao que a tela já diz (no Início, ao veredito).
    pub(crate) fn linha_estado(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let v = (self.secao == Secao::Inicio).then(|| self.veredito());
        let texto = linha_estado_visivel(self.fatos(), v)
            .filter(|l| !matches!(l, crate::modelo::LinhaEstado::AgenteAusente))
            .map(|l| t::estado::texto(&l));
        let tema = cx.theme();
        div().when_some(texto, |d, texto| {
            d.child(
                h_flex()
                    .id("linha-estado")
                    .gap(px(tema::ESP_3))
                    .items_center()
                    .px(px(tema::ESP_8))
                    .py(px(tema::ESP_3))
                    .border_b_1()
                    .border_color(tema.border)
                    .bg(tema.group_box)
                    .text_size(px(tema::TAM_CORPO))
                    .child(
                        Icon::new(IconName::Info)
                            .size(px(tema::ESP_4))
                            .text_color(tema::cor(tema::DESTAQUE)),
                    )
                    .child(texto),
            )
        })
    }

    pub(super) fn barra_superior(&self, tema: &Theme) -> Div {
        let descricao = self.secao.def().descricao;
        h_flex()
            .w_full()
            .h(px(tema::ALTURA_BARRA))
            .flex_shrink_0()
            .justify_between()
            .gap(px(tema::ESP_3))
            .px(px(tema::ESP_8))
            .text_size(px(tema::TAM_LEGENDA))
            .child(
                h_flex()
                    .gap(px(tema::ESP_3))
                    .child(div().font_semibold().child(self.secao.titulo()))
                    .when_some(descricao, |v, descricao| {
                        v.child(
                            div()
                                .border_l_1()
                                .border_color(tema.border)
                                .pl(px(tema::ESP_3))
                                .text_color(tema.muted_foreground)
                                .child(descricao),
                        )
                    }),
            )
            .when(crate::backend::Backend::atual().eh_demonstracao(), |v| {
                v.child(
                    div()
                        .text_color(tema.muted_foreground)
                        .child("Demonstração"),
                )
            })
    }

    /// Título da página (28 px).
    pub(crate) fn titulo_pagina(&self) -> impl IntoElement {
        v_flex().gap(px(tema::ESP_1)).child(
            div()
                .text_size(px(tema::TAM_TITULO_PAGINA))
                .font_semibold()
                .child(self.secao.titulo()),
        )
    }

    /// Conteúdo da seção ativa.
    pub(crate) fn conteudo_secao(
        &self,
        largura: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        (self.secao.def().render)(self, largura, window, cx)
    }
}

/// Textos do relógio lateral, refeitos só quando o minuto muda.
#[derive(Clone)]
pub(super) struct Relogio {
    hora: SharedString,
    dia_semana: &'static str,
    data: SharedString,
}

impl Relogio {
    pub(super) fn de_unix(utc: i64) -> Self {
        let local = utc + crate::fuso::deslocamento_local(utc);
        let (ano, mes, dia, hora, minuto, _) = crate::modelo::civil(local);
        let mes = t::MESES[(mes - 1) as usize];
        Relogio {
            hora: format!("{hora:02}:{minuto:02}").into(),
            dia_semana: t::DIAS_SEMANA[(local.div_euclid(86_400) + 4).rem_euclid(7) as usize],
            data: format!("{dia} de {mes} de {ano}").into(),
        }
    }
}

impl Janela {
    pub(super) fn relogio_lateral(&self, tema: &Theme) -> Div {
        let utc = crate::fuso::agora_unix();
        let r = {
            let mut cache = self.relogio.borrow_mut();
            match &*cache {
                Some((minuto, r)) if *minuto == utc / 60 => r.clone(),
                _ => {
                    let r = Relogio::de_unix(utc as i64);
                    *cache = Some((utc / 60, r.clone()));
                    r
                }
            }
        };
        v_flex()
            .items_center()
            .text_center()
            .pt(px(tema::ESP_4))
            .pb(px(tema::ESP_APOS_RELOGIO))
            .child(
                div()
                    .font_bold()
                    .text_size(px(tema::TAM_RELOGIO_LATERAL))
                    .child(r.hora),
            )
            .child(
                div()
                    .font_semibold()
                    .text_size(px(tema::TAM_SUBTITULO))
                    .text_color(tema.muted_foreground)
                    .child(r.dia_semana),
            )
            .child(
                div()
                    .mt(px(tema::ESP_1))
                    .text_color(tema.muted_foreground)
                    .child(r.data),
            )
    }
}
