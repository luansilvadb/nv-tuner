//! Estrutura comum da janela (spec `interface-e-saude.md` §5.1): navegação
//! lateral mínima, linha de estado única no topo e título da página.
//!
//! Também guarda a [`Janela`] (estado, envio de comandos e desenho comum) e
//! as peças visuais compartilhadas pelas seções.

use std::cell::RefCell;
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
    Comando, EstadoUi, Evento, Fatos, TipoMensagem, Veredito, linha_estado_visivel, veredito,
};
use crate::servico::Canal;
use crate::tema;
use crate::textos as t;

mod lateral;
mod pecas;
mod secao;
use lateral::*;
pub(super) use pecas::*;
pub use secao::*;

#[cfg(feature = "medicao-partidas")]
use super::partidas;
use super::{inicio, jogos};

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
    /// Momento da última troca de seção (transição de entrada do conteúdo).
    troca_secao: Option<Instant>,
    /// Salvamento automático das preferências globais, adiado até o usuário
    /// parar de escolher (cada nova escolha reinicia a espera).
    salvar_preferencias: Option<Task<()>>,
    /// Fatos e veredito do estado atual, refeitos no início de cada desenho.
    painel: (Fatos, Veredito),
    /// Texto do relógio lateral do último minuto desenhado.
    relogio: RefCell<Option<(u64, Relogio)>>,
}

impl Janela {
    pub fn new(
        secao: Secao,
        expandidos: Vec<String>,
        canal: Canal,
        cx: &mut Context<Self>,
    ) -> Self {
        let eventos = canal.eventos;
        super::jogos::instalar_envio(cx, canal.comandos.clone());
        super::jogos::abrir_inicial(cx, expandidos.first().cloned());
        // Consome os eventos da thread de comunicação sem bloquear a janela.
        cx.spawn(async move |this, cx| {
            let mut minuto = crate::fuso::agora_unix() / 60;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let lote: Vec<Evento> = eventos.try_iter().collect();
                let minuto_atual = crate::fuso::agora_unix() / 60;
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
        let mut janela = Self {
            ui: EstadoUi::new(),
            secao,
            relogio: RefCell::new(None),
            painel: (Fatos::default(), Veredito::Verificando),
            comandos: canal.comandos,
            lateral_recolhida: false,
            fechar_mensagem: None,
            troca_secao: None,
            salvar_preferencias: None,
        };
        janela.atualizar_painel();
        janela
    }

    pub(super) fn enviar(&mut self, c: Comando, cx: &mut Context<Self>) {
        if !self.ui.pode_executar(&c) {
            return;
        }
        self.ui.aplicar(Evento::Ocupado(c.clone()));
        let _ = self.comandos.send(c);
        cx.notify();
    }

    /// Aplica as preferências globais sozinho, 500 ms depois da última
    /// escolha. Se outra operação estiver em andamento, tenta de novo depois;
    /// sem o serviço, a escolha fica no rascunho e a tela avisa.
    pub(super) fn agendar_salvar_preferencias(&mut self, cx: &mut Context<Self>) {
        self.salvar_preferencias = Some(cx.spawn(async move |janela, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let tentar_de_novo = janela
                    .update(cx, |j, cx| {
                        if !j.ui.rascunho_alterado() {
                            return false;
                        }
                        let comando = Comando::DefinirPreferencias(j.ui.rascunho.clone());
                        if j.ui.pode_executar(&comando) {
                            j.enviar(comando, cx);
                            false
                        } else {
                            // Ocupado com outra operação: espera ela terminar.
                            j.ui.ocupado.is_some()
                        }
                    })
                    .unwrap_or(false);
                if !tentar_de_novo {
                    break;
                }
            }
        }));
    }

    pub(super) fn agendar_fechamento_mensagem(&mut self, cx: &mut Context<Self>) {
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
        self.atualizar_painel();
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
        // Troca de seção (Material 3, eixo compartilhado Y): o conteúdo novo
        // entra subindo 16 px enquanto aparece, com desaceleração.
        let entrada = match self.troca_secao {
            Some(inicio) if !cx.reduce_motion() => {
                let t =
                    (inicio.elapsed().as_secs_f32() / DURACAO_TROCA_SECAO.as_secs_f32()).min(1.);
                if t < 1. {
                    window.request_animation_frame();
                }
                1. - (1. - t).powi(3)
            }
            _ => 1.,
        };
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
                                            .relative()
                                            .top(px(tema::ESP_4 * (1. - entrada)))
                                            .opacity(entrada)
                                            .px(px(tema::ESP_8))
                                            .py(px(tema::ESP_6))
                                            .gap(px(tema::ESP_6))
                                            .when(
                                                !matches!(self.secao, Secao::Inicio | Secao::Jogos),
                                                |v| v.child(self.titulo_pagina()),
                                            )
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

const DURACAO_TROCA_SECAO: Duration = Duration::from_millis(280);

fn abertura_lateral(largura: Pixels) -> f32 {
    ((f32::from(largura) - tema::LARGURA_LATERAL_RECOLHIDA)
        / (tema::LARGURA_LATERAL - tema::LARGURA_LATERAL_RECOLHIDA))
        .clamp(0., 1.)
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

#[cfg(test)]
mod testes {
    use std::collections::HashSet;

    use super::Secao;

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
    fn rotas_fora_da_navegacao_ficam_indisponiveis() {
        for chave in [
            "avancado",
            "visao-geral",
            "mudancas",
            "windows",
            "historico",
        ] {
            assert_eq!(Secao::de_chave(chave), None);
        }
    }

    #[test]
    fn chaves_das_secoes() {
        for &s in Secao::visiveis() {
            assert_eq!(Secao::de_chave(s.chave()), Some(s));
        }
        assert_eq!(Secao::de_chave("nada"), None);
        assert_eq!(Secao::default(), Secao::Inicio);
    }
}

/// R-UI-11: espaçamento, tipografia e cantos só pelos tokens do tema.
#[cfg(test)]
mod tokens {
    const TELAS: [(&str, &str); 12] = [
        ("estrutura", include_str!("estrutura.rs")),
        ("inicio", include_str!("inicio.rs")),
        ("boas_vindas", include_str!("boas_vindas.rs")),
        ("jogos", include_str!("jogos.rs")),
        ("estrutura/secao", include_str!("estrutura/secao.rs")),
        ("estrutura/lateral", include_str!("estrutura/lateral.rs")),
        ("estrutura/pecas", include_str!("estrutura/pecas.rs")),
        ("inicio/saude", include_str!("inicio/saude.rs")),
        (
            "inicio/recomendacoes",
            include_str!("inicio/recomendacoes.rs"),
        ),
        ("jogos/segmentado", include_str!("jogos/segmentado.rs")),
        ("jogos/preferencias", include_str!("jogos/preferencias.rs")),
        ("jogos/detalhe", include_str!("jogos/detalhe.rs")),
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
