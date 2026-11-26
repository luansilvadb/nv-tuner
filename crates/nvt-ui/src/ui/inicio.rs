//! Seção Início (spec `interface-e-saude.md` §5.3, lote L2): veredito único,
//! botões de ação, resumo dos jogos e saúde do PC (só com dados).
//!
//! O estado próprio da tela (boas-vindas vista, ajustes desligados, ação em
//! andamento e o último resultado) fica num `Global` do aplicativo, para não
//! mexer na janela legada.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants};
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable, StyledExt, WindowExt, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::estrutura::Janela;
use super::estrutura::Secao;
use crate::local;
use crate::modelo::{
    Fatos, MotivoIndisponivel, Saude, StatusJogo, Veredito, status_jogo, veredito,
};
use crate::servico::{self, AcaoInicio, FalhaInicio, RespostaInicio};
use crate::tema;
use crate::textos::inicio as t;
use crate::textos::jogos as tj;

/// Jogos mostrados no resumo; o resto fica em "Ver todos".
const JOGOS_NO_RESUMO: usize = 5;

// ---------------------------------------------------------------------------
// Modelo da tela (puro)
// ---------------------------------------------------------------------------

/// Tom do aviso de resultado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tom {
    Sucesso,
    Informacao,
    Erro,
}

/// Estado próprio do Início.
#[derive(Debug, Clone, Default)]
pub(super) struct EstadoInicio {
    /// `None` = ainda não lido do disco.
    pub boas_vindas_vista: Option<bool>,
    pub ocupado: Option<AcaoInicio>,
    pub resultado: Option<(Tom, String)>,
}

impl Global for EstadoInicio {}

impl EstadoInicio {
    /// Registra o fim de uma ação e devolve o novo valor da pausa dos
    /// ajustes, se a ação o mudou (o serviço confirma no próximo estado).
    pub fn concluir(
        &mut self,
        acao: AcaoInicio,
        r: &Result<RespostaInicio, FalhaInicio>,
    ) -> Option<bool> {
        self.ocupado = None;
        self.resultado = Some(texto_resultado(acao, r));
        match (acao, r) {
            (AcaoInicio::DesfazerTudo, Ok(_)) => Some(true),
            (AcaoInicio::VoltarAAjustar, Ok(_)) => Some(false),
            _ => None,
        }
    }
}

/// Resultado de uma ação em linguagem simples (R-UI-06, R-UI-07).
pub(super) fn texto_resultado(
    acao: AcaoInicio,
    r: &Result<RespostaInicio, FalhaInicio>,
) -> (Tom, String) {
    match (acao, r) {
        (AcaoInicio::AplicarRecomendadas, Ok(RespostaInicio::Feito { aceitas: 0, .. })) => {
            (Tom::Informacao, t::NENHUMA_APLICADA.to_owned())
        }
        (AcaoInicio::AplicarRecomendadas, Ok(RespostaInicio::Feito { aceitas, adiada })) => {
            if *adiada {
                (
                    Tom::Informacao,
                    format!("{} {}", t::APLICAR_ADIADO, t::PARA_VOLTAR_ATRAS),
                )
            } else {
                (Tom::Sucesso, t::aplicadas(*aceitas))
            }
        }
        (AcaoInicio::DesfazerTudo, Ok(RespostaInicio::Feito { adiada, .. })) => {
            if *adiada {
                (Tom::Informacao, t::DESFAZER_ADIADO.to_owned())
            } else {
                (Tom::Sucesso, t::DESFEITO_OK.to_owned())
            }
        }
        (AcaoInicio::VoltarAAjustar, Ok(RespostaInicio::Feito { adiada, .. })) => {
            if *adiada {
                (Tom::Informacao, t::VOLTAR_ADIADO.to_owned())
            } else {
                (Tom::Sucesso, t::VOLTAR_A_AJUSTAR_OK.to_owned())
            }
        }
        (_, Err(FalhaInicio::SemServico)) => (Tom::Informacao, t::APLICAR_INDISPONIVEL.to_owned()),
        (AcaoInicio::AplicarRecomendadas, Err(FalhaInicio::RecursoAusente)) => {
            (Tom::Informacao, t::RECOMENDADAS_AUSENTE.to_owned())
        }
        (_, Err(FalhaInicio::RecursoAusente)) => (Tom::Informacao, t::RECURSO_AUSENTE.to_owned()),
        (_, Err(FalhaInicio::Outra(m))) => (Tom::Erro, m.clone()),
    }
}

/// Botão "Aplicar melhorias recomendadas": habilitado ou com o motivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BotaoAplicar {
    Habilitado,
    Desabilitado(&'static str),
}

/// Botões do cartão do veredito.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Botoes {
    pub aplicar: Option<BotaoAplicar>,
    pub desfazer: bool,
    pub voltar: bool,
}

/// Quais botões o veredito oferece (R-UI-02, R-UI-06, R-UI-14).
pub(super) fn botoes(v: &Veredito, f: &Fatos) -> Botoes {
    let aplicavel = f.melhorias > 0
        && matches!(
            v,
            Veredito::Melhorias { .. }
                | Veredito::EsperandoJogo
                | Veredito::Piorou(_)
                | Veredito::Indisponivel(MotivoIndisponivel::ServicoAusente)
        );
    let aplicar = aplicavel.then_some({
        if f.servico_ausente || f.protocolo_incompativel {
            BotaoAplicar::Desabilitado(t::APLICAR_INDISPONIVEL)
        } else {
            BotaoAplicar::Habilitado
        }
    });
    Botoes {
        aplicar,
        desfazer: matches!(
            v,
            Veredito::Otimizado
                | Veredito::NadaAFazer
                | Veredito::Melhorias { .. }
                | Veredito::EsperandoJogo
                | Veredito::Piorou(_)
        ),
        voltar: v.oferece_voltar_a_ajustar(),
    }
}

/// Confirmação de "Desfazer tudo" (R-UI-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Confirmacao {
    pub titulo: &'static str,
    pub linhas: [&'static str; 3],
    pub cancelar: &'static str,
    pub confirmar: &'static str,
    /// Ação enviada ao confirmar.
    pub acao: AcaoInicio,
}

pub(super) const CONFIRMAR_DESFAZER: Confirmacao = Confirmacao {
    titulo: t::CONFIRMAR_DESFAZER_TITULO,
    linhas: [
        t::CONFIRMAR_DESFAZER_VOLTA,
        t::CONFIRMAR_DESFAZER_JOGO,
        t::CONFIRMAR_DESFAZER_PARA,
    ],
    cancelar: t::BOTAO_CANCELAR,
    confirmar: t::BOTAO_DESFAZER_TUDO,
    acao: AcaoInicio::DesfazerTudo,
};

/// A seção Saúde do PC aparece? Só com resposta de `ObterSaude`.
pub(super) fn mostra_saude(saude: Option<&Saude>) -> bool {
    saude.is_some()
}

/// Resumo de Seus jogos: até `max` linhas (nome, status) e quantos sobram.
pub(super) fn resumo_jogos(
    jogos: impl IntoIterator<Item = (String, StatusJogo)>,
    max: usize,
) -> (Vec<(String, String)>, usize) {
    let todos: Vec<_> = jogos.into_iter().collect();
    let resto = todos.len().saturating_sub(max);
    let linhas = todos
        .into_iter()
        .take(max)
        .map(|(nome, s)| (nome, tj::status(s)))
        .collect();
    (linhas, resto)
}

fn icone(v: &Veredito) -> IconName {
    match v {
        Veredito::Otimizado | Veredito::NadaAFazer => IconName::CircleCheck,
        Veredito::Melhorias { .. } | Veredito::Verificando => IconName::Info,
        Veredito::Piorou(_) => IconName::TriangleAlert,
        Veredito::Indisponivel(_) | Veredito::SemPlacaNvidia => IconName::CircleX,
        Veredito::AjustesDesligados => IconName::Undo2,
        Veredito::EsperandoJogo => IconName::Gamepad2,
    }
}

// ---------------------------------------------------------------------------
// Ações
// ---------------------------------------------------------------------------

/// Executa a ação fora da thread da janela e mostra o resultado.
fn executar(acao: AcaoInicio, cx: &mut Context<Janela>) {
    let estado = cx.default_global::<EstadoInicio>();
    if estado.ocupado.is_some() {
        return;
    }
    estado.ocupado = Some(acao);
    estado.resultado = None;
    cx.notify();
    let tarefa = cx
        .background_executor()
        .spawn(async move { servico::executar_inicio(acao) });
    cx.spawn(async move |this, cx| {
        let r = tarefa.await;
        let _ = this.update(cx, |janela, cx| {
            let pausa = cx.default_global::<EstadoInicio>().concluir(acao, &r);
            if let (Some(p), Some(e)) = (pausa, janela.ui.dados.estado.as_mut()) {
                e.autonomia_pausada = p;
            }
            cx.notify();
        });
    })
    .detach();
}

/// "Começar" nas boas-vindas: grava a marca (falha não bloqueia) e segue.
pub(super) fn concluir_boas_vindas(cx: &mut Context<Janela>) {
    let _ = local::marcar_boas_vindas();
    cx.default_global::<EstadoInicio>().boas_vindas_vista = Some(true);
    cx.notify();
}

fn abrir_confirmacao(window: &mut Window, cx: &mut Context<Janela>) {
    let janela = cx.entity().downgrade();
    window.open_alert_dialog(cx, move |alerta, _, _| {
        let janela = janela.clone();
        let c = CONFIRMAR_DESFAZER;
        alerta
            .title(c.titulo)
            .description(v_flex().gap(px(tema::ESP_1)).children(c.linhas))
            .show_cancel(true)
            .ok_text(c.confirmar)
            .ok_variant(ButtonVariant::Danger)
            .cancel_text(c.cancelar)
            .on_ok(move |_, _, cx| {
                let _ = janela.update(cx, |_, cx| executar(c.acao, cx));
                true
            })
    });
}

// ---------------------------------------------------------------------------
// Desenho
// ---------------------------------------------------------------------------

fn cartao(tema: &component::Theme) -> Div {
    v_flex()
        .w_full()
        .gap(px(tema::ESP_3))
        .p(px(tema::ESP_6))
        .rounded(px(tema::RAIO))
        .border_1()
        .border_color(tema.border)
        .bg(tema.group_box)
}

fn titulo_secao(texto: &'static str) -> impl IntoElement {
    div()
        .text_size(px(tema::TAM_TITULO_CARTAO))
        .font_semibold()
        .child(texto)
}

/// As boas-vindas ainda não foram vistas (lê a marca do disco uma vez).
pub(super) fn boas_vindas_pendente(cx: &mut App) -> bool {
    let e = cx.default_global::<EstadoInicio>();
    !*e.boas_vindas_vista
        .get_or_insert_with(local::boas_vindas_vista)
}

/// Conteúdo da seção Início (as boas-vindas são desenhadas pela estrutura).
pub(super) fn render(janela: &Janela, cx: &mut Context<Janela>) -> AnyElement {
    let estado = cx.global::<EstadoInicio>().clone();
    let fatos = Fatos::de_ui(&janela.ui, false);
    // A UI ainda não consulta `ObterSaude`: sem dados, a seção não aparece.
    let saude: Option<&Saude> = None;
    let v = veredito(&fatos, saude);
    let texto = t::veredito(&v);
    let b = botoes(&v, &fatos);
    let tema = cx.theme().clone();
    let ocupado = estado.ocupado;

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
            Button::new("aplicar-recomendadas")
                .primary()
                .label(t::BOTAO_APLICAR_RECOMENDADAS)
                .disabled(desabilitado || ocupado.is_some())
                .loading(ocupado == Some(AcaoInicio::AplicarRecomendadas))
                .on_click(cx.listener(|_, _, _, cx| executar(AcaoInicio::AplicarRecomendadas, cx))),
        );
    }
    if b.voltar {
        acoes = acoes.child(
            Button::new("voltar-a-ajustar")
                .primary()
                .label(t::BOTAO_VOLTAR_A_AJUSTAR)
                .disabled(ocupado.is_some())
                .loading(ocupado == Some(AcaoInicio::VoltarAAjustar))
                .on_click(cx.listener(|_, _, _, cx| executar(AcaoInicio::VoltarAAjustar, cx))),
        );
    }
    if b.desfazer {
        acoes = acoes.child(
            Button::new("desfazer-tudo")
                .outline()
                .label(t::BOTAO_DESFAZER_TUDO)
                .disabled(ocupado.is_some())
                .loading(ocupado == Some(AcaoInicio::DesfazerTudo))
                .on_click(cx.listener(|_, _, window, cx| abrir_confirmacao(window, cx))),
        );
    }
    let tem_acoes = b.aplicar.is_some() || b.voltar || b.desfazer;

    let cor_resultado = |tom: Tom| match tom {
        Tom::Sucesso => tema.success,
        Tom::Informacao => tema.foreground,
        Tom::Erro => tema.danger,
    };
    let veredito_el = cartao(&tema)
        .id("veredito")
        .child(
            h_flex()
                .gap(px(tema::ESP_4))
                .items_center()
                .child(
                    Icon::new(icone(&v))
                        .size(px(40.))
                        .text_color(tema::cor(tema::DESTAQUE)),
                )
                .child(
                    v_flex()
                        .gap(px(tema::ESP_1))
                        .child(div().text_size(px(20.)).font_semibold().child(texto.titulo))
                        .child(
                            div()
                                .text_size(px(tema::TAM_CORPO))
                                .text_color(tema.muted_foreground)
                                .child(texto.frase),
                        ),
                ),
        )
        .when(tem_acoes, |c| c.child(acoes))
        .when_some(motivo, |c, m| {
            c.child(
                div()
                    .text_size(px(tema::TAM_LEGENDA))
                    .text_color(tema.muted_foreground)
                    .child(m),
            )
        })
        .when(ocupado.is_some(), |c| {
            c.child(
                div()
                    .text_size(px(tema::TAM_LEGENDA))
                    .text_color(tema.muted_foreground)
                    .child(t::AGUARDE),
            )
        })
        .when_some(estado.resultado.clone(), |c, (tom, m)| {
            c.child(
                div()
                    .text_size(px(tema::TAM_CORPO))
                    .text_color(cor_resultado(tom))
                    .child(m),
            )
        });

    let esperando = fatos.esperando_jogo;
    let (linhas, resto) = resumo_jogos(
        janela
            .ui
            .dados
            .jogos
            .iter()
            .map(|j| (j.nome.clone(), status_jogo(j, esperando))),
        JOGOS_NO_RESUMO,
    );
    let jogos_el = cartao(&tema)
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .child(titulo_secao(t::SECAO_JOGOS))
                .child(
                    Button::new("ver-todos-jogos")
                        .ghost()
                        .small()
                        .label(t::VER_TODOS)
                        .on_click(cx.listener(|this, _, _, cx| this.ir_secao(Secao::Jogos, cx))),
                ),
        )
        .when(linhas.is_empty(), |c| {
            c.child(
                div()
                    .text_size(px(tema::TAM_CORPO))
                    .text_color(tema.muted_foreground)
                    .child(tj::NENHUM_JOGO),
            )
        })
        .children(linhas.into_iter().map(|(nome, status)| {
            h_flex()
                .justify_between()
                .text_size(px(tema::TAM_CORPO))
                .child(nome)
                .child(div().text_color(tema.muted_foreground).child(status))
        }))
        .when(resto > 0, |c| {
            c.child(
                div()
                    .text_size(px(tema::TAM_LEGENDA))
                    .text_color(tema.muted_foreground)
                    .child(t::mais_jogos(resto)),
            )
        });

    v_flex()
        .gap(px(tema::ESP_6))
        .max_w(px(880.))
        .child(veredito_el)
        .when(!matches!(v, Veredito::Verificando), |c| c.child(jogos_el))
        .when(mostra_saude(saude), |c| {
            c.child(cartao(&tema).child(titulo_secao(t::SECAO_SAUDE)))
        })
        .into_any_element()
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::modelo::TipoSinal;
    use core::prelude::v1::test;

    fn pronto() -> Fatos {
        Fatos {
            placa_nvidia: Some(true),
            plano_com_mudancas: true,
            ..Default::default()
        }
    }

    /// (caso, fatos, saúde, título esperado, aplicar, desfazer, voltar)
    type Caso<'a> = (
        &'a str,
        Fatos,
        Option<&'a Saude>,
        String,
        Option<BotaoAplicar>,
        bool,
        bool,
    );

    fn feito(aceitas: usize, adiada: bool) -> Result<RespostaInicio, FalhaInicio> {
        Ok(RespostaInicio::Feito { aceitas, adiada })
    }

    #[test]
    fn variacoes_do_veredito_e_botoes() {
        let s = Saude {
            em_atencao: vec![TipoSinal::EspacoLivre],
        };
        let hab = Some(BotaoAplicar::Habilitado);
        let des = Some(BotaoAplicar::Desabilitado(t::APLICAR_INDISPONIVEL));
        // (caso, fatos, saúde, título esperado, aplicar, desfazer, voltar)
        let casos: Vec<Caso> = Vec::from([
            (
                "otimizado",
                pronto(),
                None,
                t::OTIMIZADO_TITULO.into(),
                None,
                true,
                false,
            ),
            (
                "nada a fazer",
                Fatos {
                    plano_com_mudancas: false,
                    ..pronto()
                },
                None,
                t::NADA_A_FAZER_TITULO.into(),
                None,
                true,
                false,
            ),
            (
                "melhorias",
                Fatos {
                    melhorias: 3,
                    ..pronto()
                },
                None,
                t::MELHORIAS_TITULO.into(),
                hab,
                true,
                false,
            ),
            (
                "piorou",
                Fatos {
                    melhorias: 3,
                    ..pronto()
                },
                Some(&s),
                "Algo piorou: pouco espaço livre".into(),
                hab,
                true,
                false,
            ),
            (
                "esperando o jogo: aplicar continua habilitado",
                Fatos {
                    melhorias: 2,
                    esperando_jogo: true,
                    ..pronto()
                },
                None,
                t::ESPERANDO_TITULO.into(),
                hab,
                true,
                false,
            ),
            (
                "desligado: só voltar a ajustar",
                Fatos {
                    melhorias: 2,
                    ajustes_desligados: true,
                    ..pronto()
                },
                None,
                t::PAUSADO_TITULO.into(),
                None,
                false,
                true,
            ),
            (
                "sem placa",
                Fatos {
                    placa_nvidia: Some(false),
                    ..pronto()
                },
                None,
                t::SEM_PLACA_TITULO.into(),
                None,
                false,
                false,
            ),
            (
                "serviço ausente: aplicar desabilitado com motivo",
                Fatos {
                    servico_ausente: true,
                    melhorias: 2,
                    ..pronto()
                },
                None,
                t::INDISPONIVEL_TITULO.into(),
                des,
                false,
                false,
            ),
            (
                "serviço ausente sem melhorias",
                Fatos {
                    servico_ausente: true,
                    ..pronto()
                },
                None,
                t::INDISPONIVEL_TITULO.into(),
                None,
                false,
                false,
            ),
            (
                "protocolo incompatível",
                Fatos {
                    protocolo_incompativel: true,
                    melhorias: 2,
                    ..pronto()
                },
                None,
                t::INDISPONIVEL_TITULO.into(),
                None,
                false,
                false,
            ),
            (
                "verificando",
                Fatos {
                    conectando: true,
                    ..Default::default()
                },
                None,
                t::VERIFICANDO_TITULO.into(),
                None,
                false,
                false,
            ),
        ]);
        for (nome, f, saude, titulo, aplicar, desfazer, voltar) in casos {
            let v = veredito(&f, saude);
            assert_eq!(t::veredito(&v).titulo, titulo, "{nome}");
            let b = botoes(&v, &f);
            assert_eq!(b.aplicar, aplicar, "{nome}: aplicar");
            assert_eq!(b.desfazer, desfazer, "{nome}: desfazer");
            assert_eq!(b.voltar, voltar, "{nome}: voltar");
        }
    }

    #[test]
    fn modo_visualizacao_diz_que_nao_esta_ativo() {
        // R-UI-14: somente leitura e frase de reinstalação, sem services.msc.
        let f = Fatos {
            servico_ausente: true,
            ..pronto()
        };
        let texto = t::veredito(&veredito(&f, None));
        assert_eq!(texto.titulo, "O nv-tuner não está ativo neste PC");
        assert_eq!(
            texto.frase,
            "Reinstale o nv-tuner para que ele volte a cuidar do PC."
        );
        assert!(!texto.frase.contains("services.msc"));
        let b = botoes(&veredito(&f, None), &f);
        assert!(!b.desfazer && !b.voltar);
    }

    #[test]
    fn resultado_de_aplicar_recomendadas() {
        let a = AcaoInicio::AplicarRecomendadas;
        assert_eq!(
            texto_resultado(a, &feito(3, false)),
            (
                Tom::Sucesso,
                "Aplicamos 3 melhorias. Para voltar atrás, use Desfazer tudo.".into()
            )
        );
        let (tom, adiada) = texto_resultado(a, &feito(3, true));
        assert_eq!(tom, Tom::Informacao);
        assert!(adiada.starts_with("As melhorias serão aplicadas quando o jogo fechar."));
        assert!(adiada.contains("Desfazer tudo"));
        assert_eq!(texto_resultado(a, &feito(0, false)).1, t::NENHUMA_APLICADA);
        assert_eq!(
            texto_resultado(a, &Err(FalhaInicio::RecursoAusente)),
            (Tom::Informacao, t::RECOMENDADAS_AUSENTE.into())
        );
        assert_eq!(
            texto_resultado(a, &Err(FalhaInicio::SemServico)).0,
            Tom::Informacao
        );
        assert_eq!(
            texto_resultado(a, &Err(FalhaInicio::Outra("x".into()))),
            (Tom::Erro, "x".into())
        );
    }

    #[test]
    fn desfazer_e_voltar_mudam_a_pausa() {
        let mut e = EstadoInicio::default();
        assert_eq!(
            e.concluir(AcaoInicio::DesfazerTudo, &feito(0, false)),
            Some(true)
        );
        assert_eq!(e.resultado, Some((Tom::Sucesso, t::DESFEITO_OK.into())));
        assert_eq!(
            e.concluir(AcaoInicio::VoltarAAjustar, &Err(FalhaInicio::SemServico)),
            None,
            "falha não religa"
        );
        assert_eq!(
            e.concluir(AcaoInicio::VoltarAAjustar, &feito(0, true)),
            Some(false)
        );
        assert_eq!(
            e.resultado,
            Some((Tom::Informacao, t::VOLTAR_ADIADO.into()))
        );
        e.concluir(AcaoInicio::DesfazerTudo, &feito(0, true));
        assert_eq!(
            e.resultado,
            Some((Tom::Informacao, t::DESFAZER_ADIADO.into()))
        );
        assert_eq!(e.ocupado, None);
    }

    #[test]
    fn confirmacao_de_desfazer_tudo() {
        let c = CONFIRMAR_DESFAZER;
        assert_eq!(c.titulo, "Desfazer tudo?");
        assert_eq!(
            c.linhas,
            [
                "Todos os ajustes que o nv-tuner fez voltam a ser como eram antes.",
                "Se um jogo estiver aberto, isso acontece quando ele fechar.",
                "O nv-tuner para de ajustar até você pedir de novo.",
            ]
        );
        assert_eq!((c.cancelar, c.confirmar), ("Cancelar", "Desfazer tudo"));
        assert_eq!(c.acao, AcaoInicio::DesfazerTudo);
        assert_eq!(c.acao.requisicao(), nvt_ipc::Requisicao::RestaurarOriginal);
    }

    #[test]
    fn saude_so_com_dados() {
        assert!(!mostra_saude(None));
        assert!(mostra_saude(Some(&Saude::default())));
    }

    #[test]
    fn resumo_dos_jogos() {
        let jogos = (0..7).map(|i| (format!("Jogo {i}"), StatusJogo::Otimizado));
        let (linhas, resto) = resumo_jogos(jogos, 5);
        assert_eq!(linhas.len(), 5);
        assert_eq!(resto, 2);
        assert_eq!(linhas[0], ("Jogo 0".into(), "Otimizado".into()));
        assert_eq!(t::mais_jogos(resto), "e mais 2 jogos");
        let (linhas, resto) = resumo_jogos(
            [("A".to_owned(), StatusJogo::FacaAjustes { quantidade: 2 })],
            5,
        );
        assert_eq!(linhas[0].1, "Faça 2 ajustes no jogo");
        assert_eq!(resto, 0);
    }

    #[test]
    fn textos_novos_sem_sigla_e_curtos() {
        const SIGLAS: &[&str] = &[
            "GPU", "VRAM", "DLSS", "VRR", "G-SYNC", "FPS", "BAR", "DRS", "HAGS", "MPO", "SID",
            "IPC", "ETW", "MSI", "CPU", "RAM", "SSD", "API",
        ];
        let mut todos: Vec<String> = t::TODOS_L2.iter().map(|s| (*s).to_owned()).collect();
        todos.push(t::mais_jogos(1));
        todos.push(format!("{} {}", t::APLICAR_ADIADO, t::PARA_VOLTAR_ATRAS));
        for s in &todos {
            for sigla in SIGLAS {
                assert!(
                    !s.split(|c: char| !c.is_alphanumeric() && c != '-')
                        .any(|p| p == *sigla),
                    "{s:?} tem {sigla}"
                );
            }
            assert!(s.chars().all(|c| (c as u32) < 0x2600), "{s:?}");
            assert!(!s.contains("serviço"), "{s:?}");
        }
    }
}
