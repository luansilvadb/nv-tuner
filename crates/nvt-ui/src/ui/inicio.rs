//! Seção Início (spec `interface-e-saude.md` §5.3, lote L2): veredito único,
//! botões de ação e saúde do PC.
//!
//! O estado próprio da tela (boas-vindas vista, ajustes desligados, ação em
//! andamento e o último resultado) fica num `Global` do aplicativo, para não
//! mexer na janela legada.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{ButtonVariant, ButtonVariants};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    ActiveTheme, Disableable, FocusableExt, Icon, Sizable, StyledExt, WindowExt, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::estrutura::Janela;
use super::estrutura::{
    explicacao, icone_destaque, legenda, legenda_cor, ponto_status, superficie, titulo_cartao,
};
use super::movimento::recolhivel;
use crate::abrir::{self, Bloqueio, FalhaAbrir};
use crate::backend::{Abertura, Backend};
use crate::modelo::{Conexao, Fatos, MotivoIndisponivel, Veredito};
use crate::servico::{AcaoInicio, FalhaInicio, RespostaInicio};
use crate::tema::{self, EstadoBotao as _};
use crate::textos::inicio as t;
use crate::textos::jogos as tj;
use crate::textos::saude::{self as ts, SecaoSaude};

mod recomendacoes;
mod saude;
pub(super) use recomendacoes::*;
use saude::*;

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
    saude_aberta: bool,
    /// `None` = ainda não lido do disco.
    pub boas_vindas_vista: Option<bool>,
    pub ocupado: Option<AcaoInicio>,
    pub resultado: Option<(Tom, String)>,
    /// Resposta ao clique em "Abrir inicialização" (só quando não abriu).
    pub aviso_abrir: Option<&'static str>,
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
        desfazer: f.plano_com_mudancas
            && matches!(
                v,
                Veredito::Otimizado
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
pub(super) fn mostra_saude(saude: Option<&nvt_ipc::Saude>) -> bool {
    saude.is_some()
}

// ---------------------------------------------------------------------------
// Abrir inicialização (D3, R-S-04)
// ---------------------------------------------------------------------------

/// Último estado **do serviço** (no modo de visualização, desconhecido).
fn estado_servico(janela: &Janela) -> Option<&nvt_ipc::Estado> {
    janela
        .ui
        .dados
        .estado
        .as_ref()
        .filter(|_| janela.ui.conexao == Conexao::Servico)
}

/// Bloqueio do botão "Abrir inicialização" no estado atual.
pub(super) fn bloqueio_abrir(janela: &Janela) -> Option<Bloqueio> {
    abrir::bloqueio(estado_servico(janela), &janela.ui.dados.jogos)
}

pub(super) fn texto_bloqueio(b: Bloqueio) -> &'static str {
    match b {
        Bloqueio::JogoAberto => ts::ABRIR_BLOQUEADO_JOGO,
        Bloqueio::EstadoDesconhecido => ts::ABRIR_BLOQUEADO_DESCONHECIDO,
    }
}

/// Tratador de clique: única chamada de [`Backend::abrir_inicializacao`].
/// Na demonstração nada abre.
pub(super) fn clicar_abrir(janela: &Janela, cx: &mut Context<Janela>) {
    let aviso = match Backend::atual()
        .abrir_inicializacao(estado_servico(janela), &janela.ui.dados.jogos)
    {
        Abertura::Aberta => None,
        Abertura::Simulada => Some(ts::ABRIR_DEMONSTRACAO),
        Abertura::Falhou(FalhaAbrir::Bloqueado(b)) => Some(texto_bloqueio(b)),
        Abertura::Falhou(FalhaAbrir::Windows) => Some(ts::ABRIR_FALHOU),
    };
    cx.default_global::<EstadoInicio>().aviso_abrir = aviso;
    cx.notify();
}

/// Botão "Abrir inicialização" (desabilitado quando bloqueado) e a legenda
/// com o motivo do bloqueio ou o aviso do último clique.
pub(super) fn botao_abrir(
    id: &'static str,
    rotulo: &'static str,
    janela: &Janela,
    cx: &mut Context<Janela>,
) -> Div {
    let tema = cx.theme().clone();
    let bloqueio = bloqueio_abrir(janela);
    let aviso = bloqueio
        .map(texto_bloqueio)
        .or(cx.default_global::<EstadoInicio>().aviso_abrir);
    v_flex()
        .gap(px(tema::ESP_1))
        .items_start()
        .child(
            tema::botao_base(id)
                .outline()
                .small()
                .label(rotulo)
                .indisponivel(bloqueio.is_some())
                .on_click(cx.listener(|this, _, _, cx| clicar_abrir(this, cx))),
        )
        .when_some(aviso, |c, a| c.child(legenda(a, &tema)))
}

fn icone(v: &Veredito) -> IconName {
    match v {
        Veredito::Otimizado | Veredito::NadaAFazer => IconName::CircleCheck,
        Veredito::Melhorias { .. } | Veredito::Verificando | Veredito::Iniciando => IconName::Info,
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
    let backend = Backend::atual();
    let tarefa = cx
        .background_executor()
        .spawn(async move { backend.executar_inicio(acao) });
    cx.spawn(async move |this, cx| {
        let r = tarefa.await;
        let _ = this.update(cx, |janela, cx| {
            let estado = cx.default_global::<EstadoInicio>();
            let pausa = estado.concluir(acao, &r);
            if acao != AcaoInicio::AplicarRecomendadas
                && let Some((tom, texto)) = estado.resultado.take()
            {
                let tipo = match tom {
                    Tom::Sucesso => crate::modelo::TipoMensagem::Sucesso,
                    Tom::Informacao => crate::modelo::TipoMensagem::Adiada,
                    Tom::Erro => crate::modelo::TipoMensagem::Erro,
                };
                janela.ui.mensagem = Some((tipo, texto));
                janela.agendar_fechamento_mensagem(cx);
            }
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
    // Na demonstração nada é gravado.
    Backend::atual().marcar_boas_vindas();
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
                let _ = janela.update(cx, |_, cx| {
                    executar(c.acao, cx);
                });
                true
            })
    });
}

/// Liga/desliga da otimização (Material 3: estado binário é um switch, não
/// um botão que muda de rótulo). Desligar pede a confirmação de "Desfazer
/// tudo" (R-UI-07); ligar é imediato. `None` quando não há o que ligar nem
/// desligar (ex.: nada aplicado ainda).
pub(super) fn controle_otimizacao(
    janela: &Janela,
    window: &mut Window,
    cx: &mut Context<Janela>,
) -> Option<AnyElement> {
    let botoes = botoes(janela.veredito(), janela.fatos());
    let ocupado = cx.default_global::<EstadoInicio>().ocupado;
    let ligando = ocupado == Some(AcaoInicio::VoltarAAjustar);
    let desligando = ocupado == Some(AcaoInicio::DesfazerTudo);
    if !(botoes.voltar || botoes.desfazer || ligando || desligando) {
        return None;
    }
    // Durante a operação o switch já mostra o estado pedido.
    let ligada = ligando || (botoes.desfazer && !botoes.voltar && !desligando);
    let pendente = janela.ui.restauracao_usuario_pendente();
    // Só as duas cores usadas: evita clonar o tema inteiro a cada frame.
    let (aviso, transparente) = {
        let tema = cx.theme();
        (tema.warning, tema.transparent)
    };
    // Foco como no switch de sincronismo: sem o anel padrão (que aparece
    // até após clique com o mouse), só um contorno discreto via teclado.
    let foco = window
        .use_keyed_state("foco-otimizacao", cx, |_, cx| cx.focus_handle())
        .read(cx)
        .clone();
    let foco_visivel = window.last_input_was_keyboard() && foco.contains_focused(window, cx);
    let switch = Switch::new("otimizacao-ativa")
        .focus_ring(false)
        .cursor_pointer()
        .color(tema::cor(tema::ACAO_VIOLETA))
        .accessibility_label(t::OTIMIZACAO_AUTOMATICA)
        .checked(ligada)
        .disabled(ocupado.is_some())
        .on_click(cx.listener(|_, ligar: &bool, window, cx| {
            if *ligar {
                executar(AcaoInicio::VoltarAAjustar, cx);
            } else {
                abrir_confirmacao(window, cx);
            }
        }));
    Some(
        h_flex()
            .id("controle-otimizacao")
            .h(px(tema::ALTURA_BUSCA))
            .flex_shrink_0()
            .gap(px(tema::ESP_3))
            .pl(px(tema::ESP_4))
            .pr(px(tema::ESP_3))
            .rounded_full()
            .bg(tema::cor(tema::FUNDO_CONTROLE))
            .when(pendente, |v| {
                v.tooltip(|window, cx| {
                    Tooltip::new(crate::textos::RESTAURACAO_USUARIO_PENDENTE).build(window, cx)
                })
            })
            .child(
                h_flex()
                    .gap(px(tema::ESP_2))
                    .font_medium()
                    .child(t::OTIMIZACAO_AUTOMATICA)
                    .when(pendente, |v| {
                        v.child(ponto_status(tema::TAM_PONTO_STATUS + 1., aviso))
                    }),
            )
            .child(
                div()
                    .track_focus(&foco)
                    .rounded_full()
                    .border_1()
                    .border_color(if foco_visivel {
                        tema::cor(tema::BORDA_FOCO)
                    } else {
                        transparente
                    })
                    .on_mouse_down_out(move |_, window, cx| {
                        if foco.contains_focused(window, cx) {
                            window.blur(cx);
                        }
                    })
                    .child(switch),
            )
            .into_any_element(),
    )
}

// ---------------------------------------------------------------------------
// Desenho
// ---------------------------------------------------------------------------

/// As boas-vindas ainda não foram vistas (lê a marca do disco uma vez; na
/// demonstração, decide o cenário e nada é lido).
pub(super) fn boas_vindas_pendente(cx: &mut App) -> bool {
    let e = cx.default_global::<EstadoInicio>();
    !*e.boas_vindas_vista
        .get_or_insert_with(|| Backend::atual().boas_vindas_vista())
}

/// Tom visual do veredito: cor do ícone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TomVeredito {
    /// Tudo certo (verde).
    Bom,
    /// Informação ou ação sugerida (ciano).
    Destaque,
    /// Pede atenção (âmbar).
    Atencao,
    /// Nada a fazer aqui (cinza).
    Neutro,
}

pub(super) fn tom(v: &Veredito) -> TomVeredito {
    match v {
        Veredito::Otimizado | Veredito::NadaAFazer => TomVeredito::Bom,
        Veredito::Melhorias { .. }
        | Veredito::Verificando
        | Veredito::Iniciando
        | Veredito::AjustesDesligados => TomVeredito::Destaque,
        Veredito::Piorou(_) | Veredito::EsperandoJogo | Veredito::Indisponivel(_) => {
            TomVeredito::Atencao
        }
        Veredito::SemPlacaNvidia => TomVeredito::Neutro,
    }
}

pub(super) fn cor_tom(t: TomVeredito, tema: &component::Theme) -> Hsla {
    match t {
        TomVeredito::Bom => tema.success,
        TomVeredito::Destaque => tema::cor(tema::DESTAQUE),
        TomVeredito::Atencao => tema.warning,
        TomVeredito::Neutro => tema.muted_foreground,
    }
}

/// Um cartão do painel do Início: textos, ícone e cor do destaque.
pub(super) struct CartaoPainel {
    pub textos: t::TextoCartaoPainel,
    pub icone: IconName,
    /// Destaque verde (senão, a cor dos ícones).
    pub verde: bool,
}

/// Cartões do painel, na ordem de exibição.
pub(super) const CARTOES_PAINEL: [CartaoPainel; 5] = [
    CartaoPainel {
        textos: t::PAINEL_LATENCIA_SISTEMA,
        icone: IconName::Gauge,
        verde: true,
    },
    CartaoPainel {
        textos: t::PAINEL_FRAMETIMES,
        icone: IconName::ChartLine,
        verde: false,
    },
    CartaoPainel {
        textos: t::PAINEL_PERIFERICO,
        icone: IconName::Mouse,
        verde: false,
    },
    CartaoPainel {
        textos: t::PAINEL_FILA,
        icone: IconName::Layers,
        verde: true,
    },
    CartaoPainel {
        textos: t::PAINEL_DWM,
        icone: IconName::Zap,
        verde: true,
    },
];

pub(super) fn render(janela: &Janela, largura: Pixels, cx: &mut Context<Janela>) -> AnyElement {
    let tema = cx.theme();
    let gpu = janela
        .ui
        .dados
        .estado
        .as_ref()
        .and_then(|estado| estado.gpu.as_ref());
    let backend = Backend::atual();
    let simulacao = backend.eh_demonstracao() && gpu.is_some();
    let texto = t::veredito(janela.veredito());
    let descricao = if backend.demonstracao_otimizada() {
        t::DESCRICAO_DEMONSTRACAO_OTIMIZADO.into()
    } else {
        texto.frase
    };
    let nome = gpu.map_or(texto.titulo.as_str(), |gpu| gpu.nome.as_str());
    let colunas = if largura >= px(tema::LARGURA_GRADE_TRES) {
        3
    } else if largura >= px(tema::LARGURA_GRADE_DOIS) {
        2
    } else {
        1
    };

    v_flex()
        .w_full()
        .gap(px(tema::ESP_6))
        .child(
            superficie(tema)
                .rounded(px(tema::RAIO_PAINEL))
                .bg(linear_gradient(
                    110.,
                    linear_color_stop(tema.group_box, 0.),
                    linear_color_stop(tema::cor(tema::FUNDO_PAINEL), 1.),
                ))
                .min_h(px(tema::ALTURA_PAINEL))
                .justify_center()
                .p(px(tema::ESP_6))
                .gap(px(tema::ESP_1))
                .child(
                    div()
                        .font_semibold()
                        .text_size(px(tema::TAM_VALOR_PAINEL))
                        .child(nome.to_owned()),
                )
                .child(div().text_color(tema.muted_foreground).child(descricao)),
        )
        .child(
            div()
                .grid()
                .grid_cols(colunas)
                .gap(px(tema::ESP_GRADE))
                .children(CARTOES_PAINEL.iter().map(|cartao| {
                    let t::TextoCartaoPainel {
                        titulo,
                        valor,
                        detalhe,
                        rotulo,
                        rodape,
                    } = cartao.textos;
                    let icone = cartao.icone;
                    let destaque = if cartao.verde {
                        tema::cor(tema::VERDE_PAINEL)
                    } else {
                        tema::cor(tema::TEXTO_ICONE)
                    };
                    superficie(tema)
                        .rounded(px(tema::RAIO_CARTAO_PAINEL))
                        .min_h(px(tema::ALTURA_CARTAO_PAINEL))
                        .min_w_0()
                        .p(px(tema::ESP_4))
                        .gap(px(tema::ESP_2))
                        .justify_between()
                        .child(
                            h_flex()
                                .justify_between()
                                .gap(px(tema::ESP_2))
                                .child(super::estrutura::rotulo(titulo, tema))
                                .child(
                                    h_flex()
                                        .size(px(tema::ESP_6))
                                        .flex_shrink_0()
                                        .justify_center()
                                        .rounded(px(tema::RAIO))
                                        .bg(tema::cor(tema::FUNDO_ICONE))
                                        .child(
                                            Icon::new(icone)
                                                .size(px(tema::TAM_CORPO))
                                                .text_color(destaque),
                                        ),
                                ),
                        )
                        .child(
                            h_flex()
                                .items_baseline()
                                .flex_wrap()
                                .gap(px(tema::ESP_2))
                                .child(
                                    div()
                                        .font_semibold()
                                        .text_size(px(tema::TAM_VALOR_PAINEL))
                                        .child(if simulacao { valor } else { t::SEM_VALOR }),
                                )
                                .when(simulacao && !detalhe.is_empty(), |v| {
                                    v.child(legenda_cor(
                                        detalhe,
                                        if icone == IconName::ChartLine {
                                            tema.muted_foreground
                                        } else {
                                            tema::cor(tema::VERDE_PAINEL)
                                        },
                                    ))
                                }),
                        )
                        .child(
                            h_flex()
                                .justify_between()
                                .flex_wrap()
                                .gap(px(tema::ESP_1))
                                .border_t_1()
                                .border_color(tema::cor(tema::BORDA_SUAVE).opacity(0.65))
                                .pt(px(tema::ESP_2))
                                .text_size(px(tema::TAM_LEGENDA))
                                .child(legenda(rotulo, tema))
                                .child(
                                    div()
                                        .text_color(if simulacao {
                                            tema.foreground
                                        } else {
                                            tema.muted_foreground
                                        })
                                        .child(if simulacao { rodape } else { t::SEM_MEDICAO }),
                                ),
                        )
                })),
        )
        .into_any_element()
}

#[cfg(test)]
mod testes;
