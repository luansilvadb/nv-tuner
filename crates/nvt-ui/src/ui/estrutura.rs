//! Estrutura comum da janela (spec `interface-e-saude.md` §5.1): navegação
//! lateral mínima, linha de estado única no topo e título da página.
//!
//! Também guarda a [`Janela`] (estado, envio de comandos e desenho comum) e
//! as peças visuais compartilhadas pelas seções.

use std::collections::BTreeSet;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::base::motion::{Presence, Spring, Transition, spring, transition};

use super::movimento::{ripple_com_cor, seta_lateral};
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::component::{Theme, TitleBar};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::modelo::{
    Comando, EstadoUi, Evento, Fatos, Tela, TipoMensagem, linha_estado_visivel, veredito,
};
use crate::servico::Canal;
use crate::tema;
use crate::textos as t;

const NAV_INICIO: &str = "Início";
const NAV_JOGOS: &str = "Jogos & Otimização";
const NAV_AVANCADO: &str = "Avançado";
#[cfg(feature = "medicao-partidas")]
const NAV_PARTIDAS: &str = "Partidas";
#[cfg(feature = "medicao-partidas")]
const BUILD_DE_TESTE: &str = "Build de teste";

/// Item da navegação lateral.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Secao {
    Inicio,
    Jogos,
    // Implementação legada mantida fora das rotas públicas.
    #[allow(dead_code)]
    Avancado,
    #[cfg(feature = "medicao-partidas")]
    Partidas,
}

impl Secao {
    /// Itens visíveis na lateral, na ordem (R-UI-01).
    pub fn visiveis() -> &'static [Secao] {
        #[cfg(not(feature = "medicao-partidas"))]
        {
            &[Secao::Inicio, Secao::Jogos]
        }
        #[cfg(feature = "medicao-partidas")]
        {
            &[Secao::Inicio, Secao::Jogos, Secao::Partidas]
        }
    }

    pub fn titulo(self) -> &'static str {
        match self {
            Secao::Inicio => NAV_INICIO,
            Secao::Jogos => NAV_JOGOS,
            Secao::Avancado => NAV_AVANCADO,
            #[cfg(feature = "medicao-partidas")]
            Secao::Partidas => NAV_PARTIDAS,
        }
    }

    pub fn icone(self) -> IconName {
        match self {
            Secao::Inicio => IconName::House,
            Secao::Jogos => IconName::Gamepad2,
            Secao::Avancado => IconName::SlidersHorizontal,
            #[cfg(feature = "medicao-partidas")]
            Secao::Partidas => IconName::ChartLine,
        }
    }

    /// Chave de `--tela`.
    pub fn chave(self) -> &'static str {
        match self {
            Secao::Inicio => "inicio",
            Secao::Jogos => "jogos",
            Secao::Avancado => "avancado",
            #[cfg(feature = "medicao-partidas")]
            Secao::Partidas => "partidas",
        }
    }

    /// Tela legada mostrada ao entrar na seção.
    pub fn tela_inicial(self) -> Tela {
        match self {
            Secao::Jogos => Tela::Jogos,
            _ => Tela::VisaoGeral,
        }
    }
}

/// Onde a janela abre: seção da lateral e tela legada dentro dela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Destino {
    pub secao: Secao,
    pub tela: Tela,
}

impl Default for Destino {
    fn default() -> Self {
        Destino::de_secao(Secao::Inicio)
    }
}

impl Destino {
    pub fn de_secao(secao: Secao) -> Self {
        Destino {
            secao,
            tela: secao.tela_inicial(),
        }
    }

    /// Interpreta `--tela` usando as seções disponíveis na navegação.
    pub fn de_chave(s: &str) -> Option<Destino> {
        Secao::visiveis()
            .iter()
            .copied()
            .find(|secao| secao.chave() == s)
            .map(Destino::de_secao)
    }
}

impl Janela {
    pub(super) fn ir_secao(&mut self, secao: Secao, cx: &mut Context<Self>) {
        self.secao = secao;
        self.ui.tela = secao.tela_inicial();
        cx.notify();
    }

    fn largura_lateral_alvo(&self) -> f32 {
        if self.lateral_recolhida {
            tema::LARGURA_LATERAL_RECOLHIDA
        } else {
            tema::LARGURA_LATERAL
        }
    }

    fn controle_lateral(
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

    fn barra_lateral(
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
                    .hover(|v| {
                        v.bg(if ativo {
                            tema::cor(tema::FUNDO_ITEM_MENU_ATIVO_HOVER)
                        } else {
                            tema.list_hover
                        })
                    })
                    .active(|v| v.bg(tema::cor(tema::FUNDO_ITEM_MENU_PRESSIONADO)))
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
                                    .child(relogio_lateral(&tema)),
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
                            .child(BUILD_DE_TESTE)
                    })
                }
                #[cfg(not(feature = "medicao-partidas"))]
                {
                    None::<Div>
                }
            })
    }

    /// Fatos do estado atual (veredito e linha de estado).
    pub(super) fn fatos(&self) -> Fatos {
        Fatos::de_ui(&self.ui, self.ui.aviso_atualizado().is_some())
    }

    /// Linha de estado única no topo do conteúdo (R-UI-05), só quando
    /// acrescenta algo ao que a tela já diz (no Início, ao veredito).
    pub(super) fn linha_estado(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let f = self.fatos();
        let v = (self.secao == Secao::Inicio).then(|| veredito(&f, self.ui.saude().as_ref()));
        let texto = linha_estado_visivel(&f, v.as_ref()).map(|l| t::estado::texto(&l));
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

    fn barra_superior(&self, tema: &Theme) -> Div {
        let descricao = match self.secao {
            Secao::Inicio => Some("Painel de desempenho e drivers"),
            Secao::Jogos => Some("Biblioteca e preferências rápidas"),
            _ => None,
        };
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
            .when(crate::demonstracao::ativo().is_some(), |v| {
                v.child(
                    div()
                        .text_color(tema.muted_foreground)
                        .child("Demonstração"),
                )
            })
    }

    /// Título da página (28 px) e, no Avançado, a frase que o apresenta.
    pub(super) fn titulo_pagina(&self, tema: &Theme) -> impl IntoElement {
        let titulo = self.secao.titulo();
        v_flex()
            .gap(px(tema::ESP_1))
            .child(
                div()
                    .text_size(px(tema::TAM_TITULO_PAGINA))
                    .font_semibold()
                    .child(titulo),
            )
            .when(self.secao == Secao::Avancado, |v| {
                v.child(
                    div()
                        .text_size(px(tema::TAM_CORPO))
                        .text_color(tema.muted_foreground)
                        .child(t::avancado::INTRODUCAO),
                )
            })
    }

    /// Conteúdo da seção ativa.
    pub(super) fn conteudo_secao(
        &self,
        largura: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.secao {
            Secao::Inicio => super::inicio::render(self, largura, cx),
            Secao::Jogos => super::jogos::render(self, largura, window, cx),
            Secao::Avancado => super::avancado::render(self, window, cx),
            #[cfg(feature = "medicao-partidas")]
            Secao::Partidas => super::partidas::render(self, cx),
        }
    }
}

// ---------------------------------------------------------------------------
// Janela
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq)]
struct MensagemFlutuante {
    tipo: Option<TipoMensagem>,
    texto: SharedString,
}

/// Janela principal. Só lê [`EstadoUi`] e envia [`Comando`]s para a thread
/// de comunicação; a thread da janela nunca bloqueia em E/S.
pub struct Janela {
    pub(super) ui: EstadoUi,
    /// Item ativo da navegação lateral.
    pub(super) secao: Secao,
    comandos: Sender<Comando>,
    lateral_recolhida: bool,
    fechar_mensagem: Option<Task<()>>,
    /// Perfis dos jogos com a linha expandida na tela Jogos.
    pub(super) expandidos: BTreeSet<String>,
}

impl Janela {
    pub fn new(
        destino: Destino,
        expandidos: Vec<String>,
        canal: Canal,
        cx: &mut Context<Self>,
    ) -> Self {
        let eventos = canal.eventos;
        super::jogos::instalar_envio(cx, canal.comandos.clone());
        super::jogos::abrir_inicial(cx, expandidos.first().cloned());
        // Consome os eventos da thread de comunicação sem bloquear a janela.
        cx.spawn(async move |this, cx| {
            let mut minuto = agora_unix() / 60;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let lote: Vec<Evento> = eventos.try_iter().collect();
                let minuto_atual = agora_unix() / 60;
                if lote.is_empty() && minuto == minuto_atual {
                    continue;
                }
                minuto = minuto_atual;
                let vivo = this.update(cx, |j, cx| {
                    let houve_resultado =
                        lote.iter().any(|e| matches!(e, Evento::Concluido { .. }));
                    for e in lote {
                        j.ui.aplicar(e);
                    }
                    if houve_resultado {
                        j.agendar_fechamento_mensagem(cx);
                    }
                    cx.notify();
                });
                if vivo.is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            ui: EstadoUi::new(destino.tela),
            secao: destino.secao,
            comandos: canal.comandos,
            lateral_recolhida: false,
            fechar_mensagem: None,
            expandidos: expandidos.into_iter().collect(),
        }
    }

    pub(super) fn enviar(&mut self, c: Comando, cx: &mut Context<Self>) {
        if !self.ui.pode_executar(&c) {
            return;
        }
        self.ui.aplicar(Evento::Ocupado(c.clone()));
        let _ = self.comandos.send(c);
        cx.notify();
    }

    fn agendar_fechamento_mensagem(&mut self, cx: &mut Context<Self>) {
        self.fechar_mensagem = self.ui.mensagem_visivel().map(|(tipo, _)| {
            let duracao = match tipo {
                TipoMensagem::Sucesso => Duration::from_secs(3),
                TipoMensagem::Adiada | TipoMensagem::Erro => Duration::from_secs(6),
            };
            cx.spawn(async move |janela, cx| {
                cx.background_executor().timer(duracao).await;
                let _ = janela.update(cx, |janela, cx| {
                    janela.ui.mensagem = None;
                    cx.notify();
                });
            })
        });
    }

    fn mensagem_flutuante(
        &self,
        tema: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let atual = if let Some(comando) = &self.ui.ocupado {
            Some(MensagemFlutuante {
                tipo: None,
                texto: if *comando == Comando::Reescanear {
                    "Procurando jogos…"
                } else {
                    t::EM_ANDAMENTO
                }
                .into(),
            })
        } else {
            self.ui
                .mensagem_visivel()
                .map(|(tipo, texto)| MensagemFlutuante {
                    tipo: Some(*tipo),
                    texto: texto.clone().into(),
                })
        };
        let presenca = Presence::new("presenca-mensagem", atual.is_some())
            .transition(Transition::new(Duration::from_millis(230)))
            .sample(window, cx);
        let memoria = window.use_keyed_state("ultima-mensagem", cx, |_, _| {
            None::<(MensagemFlutuante, Instant)>
        });
        let agora = cx.background_executor().now();
        if let Some(atual) = atual
            && memoria
                .read(cx)
                .as_ref()
                .is_none_or(|(salva, _)| *salva != atual)
        {
            memoria.update(cx, |salva, _| *salva = Some((atual, agora)));
        }
        if !presenca.should_render() {
            if memoria.read(cx).is_some() {
                memoria.update(cx, |salva, _| *salva = None);
            }
            return None;
        }
        let (mensagem, inicio) = memoria.read(cx).clone()?;
        let duracao_conteudo = Duration::from_millis(180);
        let mistura = if cx.reduce_motion() {
            1.
        } else {
            (agora.saturating_duration_since(inicio).as_secs_f32() / duracao_conteudo.as_secs_f32())
                .min(1.)
        };
        if mistura < 1. {
            window.request_animation_frame();
        }

        let texto = mensagem.texto.lines().collect::<Vec<_>>().join(" ");
        let texto: SharedString = if mensagem.tipo == Some(TipoMensagem::Adiada) {
            format!("{} · {texto}", t::titulo_adiamento(&texto)).into()
        } else {
            texto.into()
        };
        let largura_texto = window
            .text_system()
            .shape_line(
                texto.clone(),
                px(tema::TAM_CORPO),
                &[TextRun {
                    len: texto.len(),
                    font: font(tema.font_family.clone()),
                    ..Default::default()
                }],
                None,
            )
            .width;
        let largura_maxima = (window.viewport_size().width - px(tema::ESP_4) * 2.)
            .max(Pixels::ZERO)
            .min(px(tema::LARGURA_TEXTO));
        let largura = transition(
            "largura-mensagem",
            (largura_texto
                + px(tema::ESP_1
                    + tema::ESP_4
                    + tema::ESP_2
                    + 2. * (tema::ESP_3 + tema::LINHA_FINA)))
            .min(largura_maxima),
            Transition::new(Duration::from_millis(220)),
            window,
            cx,
        )
        .min(largura_maxima);

        let icone = if let Some(tipo) = mensagem.tipo {
            let (icone, cor) = match tipo {
                TipoMensagem::Sucesso => (IconName::CircleCheck, tema.success),
                TipoMensagem::Adiada => (IconName::Info, tema.info),
                TipoMensagem::Erro => (IconName::CircleX, tema.danger),
            };
            Icon::new(icone)
                .size(px(tema::ESP_4))
                .text_color(cor)
                .into_any_element()
        } else {
            Spinner::new()
                .with_size(px(tema::ESP_4))
                .color(tema.info)
                .into_any_element()
        };
        let progresso = presenca.progress;
        Some(
            h_flex()
                .absolute()
                .top(px(tema::ESP_2) * progresso)
                .left_0()
                .right_0()
                .justify_center()
                .px(px(tema::ESP_4))
                .opacity(progresso)
                .child(
                    h_flex()
                        .id("mensagem-flutuante")
                        .role(if mensagem.tipo == Some(TipoMensagem::Erro) {
                            accesskit::Role::Alert
                        } else {
                            accesskit::Role::Status
                        })
                        .w(largura)
                        .gap(px(tema::ESP_2))
                        .px(px(tema::ESP_3))
                        .py(px(tema::ESP_2))
                        .rounded(px(tema::RAIO_CARTAO + tema::ESP_1 * (1. - progresso)))
                        .border_1()
                        .border_color(tema::cor(tema::BORDA_SUAVE))
                        .bg(tema.popover)
                        .shadow_md()
                        .child(
                            div()
                                .flex_shrink_0()
                                .opacity(0.35 + 0.65 * mistura)
                                .child(icone),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .opacity(0.35 + 0.65 * mistura)
                                .child(texto),
                        ),
                ),
        )
    }

    /// Avisos que a linha de estado e o veredito não cobrem. "Não está
    /// ativo" e "jogo aberto" ficam só na linha de estado (ou no veredito).
    fn avisos(&self) -> Option<Div> {
        let mut v = v_flex().gap(px(tema::ESP_2));
        let mut algum = false;
        if self.ui.atualizado {
            algum = true;
            v = v.child(
                Alert::info("atualizado-reabra", t::ATUALIZADO_REABRA)
                    .title(t::ATUALIZADO_REABRA_TITULO),
            );
        }
        if let Some(m) = &self.ui.incompativel {
            algum = true;
            v = v.child(
                Alert::error("versao-incompativel", m.clone()).title(t::INCOMPATIVEL_TITULO),
            );
        }
        if let Some(e) = &self.ui.erro_local {
            algum = true;
            v = v.child(Alert::error("erro-local", e.clone()));
        }
        algum.then_some(v)
    }
}

impl Render for Janela {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = cx.theme().clone();
        let base = || {
            div()
                .size_full()
                .bg(tema.background)
                .text_color(tema.foreground)
                .font_family(tema.font_family.clone())
                .text_size(px(tema::TAM_CORPO))
        };
        // Primeiro uso: as boas-vindas ocupam a janela inteira (R-UI-09).
        if self.secao == Secao::Inicio && super::inicio::boas_vindas_pendente(cx) {
            return base()
                .flex()
                .flex_col()
                .child(TitleBar::new().h(px(tema::ALTURA_BARRA)))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(super::boas_vindas::render(self, cx)),
                )
                .into_any_element();
        }
        let largura_lateral = spring(
            "largura-lateral",
            px(self.largura_lateral_alvo()),
            Spring::new(Duration::from_millis(470))
                .with_damping(0.7)
                .with_epsilon(0.35),
            window,
            cx,
        );
        let raio_painel = px(tema::RAIO_AREA_CONTEUDO);
        let largura_conteudo = window.viewport_size().width
            - largura_lateral
            - px(tema::ESP_8 + tema::LINHA_FINA) * 2.;
        base()
            .bg(tema.sidebar)
            .relative()
            .flex()
            .flex_row()
            .items_stretch()
            .child(self.barra_lateral(largura_lateral, window, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(
                        TitleBar::new()
                            .h(px(tema::ALTURA_BARRA))
                            .rounded_tl(Pixels::ZERO)
                            .rounded_tr(Pixels::ZERO)
                            .border_color(transparent_black())
                            .bg(tema.sidebar)
                            .child(self.barra_superior(&tema)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .rounded_tl(raio_painel)
                            .border_1()
                            .border_color(tema::cor(tema::BORDA_SUAVE).opacity(0.75))
                            .bg(tema::cor(tema::FUNDO_AREA_CONTEUDO))
                            .child(self.linha_estado(cx))
                            .child(
                                div()
                                    .id("conteudo")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .child(
                                        v_flex()
                                            .px(px(tema::ESP_8))
                                            .py(px(tema::ESP_6))
                                            .gap(px(tema::ESP_6))
                                            .when(
                                                !matches!(self.secao, Secao::Inicio | Secao::Jogos),
                                                |v| v.child(self.titulo_pagina(&tema)),
                                            )
                                            .when(self.ui.restauracao_usuario_pendente(), |v| {
                                                v.child(explicacao(
                                                    t::RESTAURACAO_USUARIO_PENDENTE,
                                                    &tema,
                                                ))
                                            })
                                            .children(self.avisos())
                                            .child(self.conteudo_secao(
                                                largura_conteudo,
                                                window,
                                                cx,
                                            )),
                                    ),
                            ),
                    ),
            )
            .children(self.mensagem_flutuante(&tema, window, cx))
            .into_any_element()
    }
}

fn abertura_lateral(largura: Pixels) -> f32 {
    ((f32::from(largura) - tema::LARGURA_LATERAL_RECOLHIDA)
        / (tema::LARGURA_LATERAL - tema::LARGURA_LATERAL_RECOLHIDA))
        .clamp(0., 1.)
}

// ---------------------------------------------------------------------------
// Peças
// ---------------------------------------------------------------------------

/// Superfície comum a todo cartão e lista: mesmo canto, borda e fundo.
pub(super) fn superficie(tema: &Theme) -> Div {
    v_flex()
        .w_full()
        .rounded(px(tema::RAIO_CARTAO))
        .border_1()
        .border_color(tema::cor(tema::BORDA_SUAVE))
        .bg(tema.group_box)
}

pub(super) fn cartao(titulo: &'static str, corpo: impl IntoElement, tema: &Theme) -> Div {
    superficie(tema)
        .gap(px(tema::ESP_3))
        .p(px(tema::ESP_4))
        .child(
            div()
                .text_size(px(tema::TAM_CORPO))
                .font_semibold()
                .child(titulo),
        )
        .child(corpo)
}

/// Ícone dentro de um círculo tingido com a cor do estado.
pub(super) fn icone_destaque(icone: IconName, cor: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .size(px(tema::ICONE_DESTAQUE))
        .rounded_full()
        .bg(cor.opacity(0.14))
        .flex()
        .items_center()
        .justify_center()
        .child(
            Icon::new(icone)
                .size(px(tema::ICONE_DESTAQUE_INTERNO))
                .text_color(cor),
        )
}

/// Estado vazio: ícone, frase e (opcional) ação, centralizados.
pub(super) fn estado_vazio(
    icone: IconName,
    frase: &'static str,
    acao: Option<AnyElement>,
    tema: &Theme,
) -> Div {
    v_flex()
        .w_full()
        .items_center()
        .gap(px(tema::ESP_4))
        .py(px(tema::ESP_8))
        .child(icone_destaque(icone, tema.muted_foreground))
        .child(
            div()
                .text_size(px(tema::TAM_CORPO))
                .text_color(tema.muted_foreground)
                .text_center()
                .child(frase),
        )
        .children(acao)
}

/// Logo do projeto (`assets/png/nv-tuner-256.png`, gerado de `logo.svg`).
pub(super) fn logo(tamanho: f32) -> Img {
    static LOGO: std::sync::OnceLock<std::sync::Arc<Image>> = std::sync::OnceLock::new();
    let imagem = LOGO.get_or_init(|| {
        std::sync::Arc::new(Image::from_bytes(
            ImageFormat::Png,
            include_bytes!("../../../../assets/png/nv-tuner-256.png").to_vec(),
        ))
    });
    img(imagem.clone()).size(px(tamanho)).flex_shrink_0()
}

pub(super) fn linha(rotulo: &'static str, valor: String, tema: &Theme) -> Div {
    h_flex()
        .gap(px(tema::ESP_4))
        .justify_between()
        .text_size(px(tema::TAM_CORPO))
        .child(div().text_color(tema.muted_foreground).child(rotulo))
        .child(div().text_right().child(valor))
}

pub(super) fn explicacao(texto: &'static str, tema: &Theme) -> Div {
    div()
        .text_size(px(tema::TAM_CORPO))
        .text_color(tema.muted_foreground)
        .child(texto)
}

fn relogio_lateral(tema: &Theme) -> Div {
    let utc = agora_unix() as i64;
    let local = utc + crate::fuso::deslocamento_local(utc);
    let (ano, mes, dia, hora, minuto, _) = crate::modelo::civil(local);
    let dias = [
        "domingo",
        "segunda-feira",
        "terça-feira",
        "quarta-feira",
        "quinta-feira",
        "sexta-feira",
        "sábado",
    ];
    let meses = [
        "janeiro",
        "fevereiro",
        "março",
        "abril",
        "maio",
        "junho",
        "julho",
        "agosto",
        "setembro",
        "outubro",
        "novembro",
        "dezembro",
    ];
    let dia_semana = dias[(local.div_euclid(86_400) + 4).rem_euclid(7) as usize];
    let mes = meses[(mes - 1) as usize];

    v_flex()
        .items_center()
        .text_center()
        .pt(px(tema::ESP_4))
        .pb(px(tema::ESP_APOS_RELOGIO))
        .child(
            div()
                .font_bold()
                .text_size(px(tema::TAM_RELOGIO_LATERAL))
                .child(format!("{hora:02}:{minuto:02}")),
        )
        .child(
            div()
                .font_semibold()
                .text_size(px(tema::TAM_SUBTITULO))
                .text_color(tema.muted_foreground)
                .child(dia_semana),
        )
        .child(
            div()
                .mt(px(tema::ESP_1))
                .text_color(tema.muted_foreground)
                .child(format!("{dia} de {mes} de {ano}")),
        )
}

/// Instante atual (Unix, segundos) para os tempos relativos da atualização.
pub(super) fn agora_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod testes {
    use std::collections::HashSet;

    use super::{Destino, Secao, Tela};

    #[cfg(not(feature = "medicao-partidas"))]
    #[test]
    fn dois_itens_no_build_normal() {
        assert_eq!(Secao::visiveis(), [Secao::Inicio, Secao::Jogos]);
        let titulos: Vec<_> = Secao::visiveis().iter().map(|s| s.titulo()).collect();
        assert_eq!(titulos, ["Início", "Jogos & Otimização"]);
    }

    #[cfg(feature = "medicao-partidas")]
    #[test]
    fn partidas_no_build_de_teste() {
        assert_eq!(Secao::visiveis().len(), 3);
        assert!(Secao::visiveis().contains(&Secao::Partidas));
        assert_eq!(Secao::Partidas.titulo(), "Partidas");
    }

    #[test]
    fn icones_distintos() {
        let icones: HashSet<_> = Secao::visiveis()
            .iter()
            .map(|s| format!("{:?}", s.icone()))
            .collect();
        assert_eq!(icones.len(), Secao::visiveis().len());
    }

    #[test]
    fn rotas_do_avancado_ficam_indisponiveis() {
        assert_eq!(Destino::de_chave("avancado"), None);
        for tela in Tela::TODAS.into_iter().filter(|tela| *tela != Tela::Jogos) {
            assert_eq!(Destino::de_chave(tela.chave()), None);
        }
    }

    #[test]
    fn chaves_das_secoes() {
        for &s in Secao::visiveis() {
            assert_eq!(Destino::de_chave(s.chave()), Some(Destino::de_secao(s)));
        }
        assert_eq!(Destino::de_chave("nada"), None);
        assert_eq!(Destino::default().secao, Secao::Inicio);
    }
}

/// R-UI-11: espaçamento, tipografia e cantos só pelos tokens do tema.
#[cfg(test)]
mod tokens {
    const TELAS: [(&str, &str); 5] = [
        ("estrutura", include_str!("estrutura.rs")),
        ("inicio", include_str!("inicio.rs")),
        ("boas_vindas", include_str!("boas_vindas.rs")),
        ("jogos", include_str!("jogos.rs")),
        ("avancado", include_str!("avancado.rs")),
    ];

    #[test]
    fn telas_usam_so_tokens() {
        let proibidos = [
            ".gap_",
            ".p_",
            ".px_",
            ".py_",
            ".pt_",
            ".pb_",
            ".pl_",
            ".pr_",
            ".mt_",
            ".mb_",
            ".ml_",
            ".mr_",
            ".text_xs(",
            ".text_sm(",
            ".text_base(",
            ".text_lg(",
            ".text_xl(",
            ".text_2xl(",
            ".rounded_sm(",
            ".rounded_md(",
            ".rounded_lg(",
            ".rounded_xl(",
        ];
        for (nome, fonte) in TELAS {
            let producao = fonte.split("#[cfg(test)]").next().unwrap();
            for p in proibidos {
                assert!(!producao.contains(p), "{nome}: {p}");
            }
            // Números soltos em px(): só tokens (`px(tema::...)`).
            for (i, l) in producao.lines().enumerate() {
                let mut resto = l;
                while let Some(k) = resto.find("px(") {
                    let depois = &resto[k + 3..];
                    assert!(
                        !depois.starts_with(|c: char| c.is_ascii_digit()),
                        "{nome}:{}: {l}",
                        i + 1
                    );
                    resto = depois;
                }
            }
        }
    }
}
