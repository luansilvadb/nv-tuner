//! Seção Início (spec `interface-e-saude.md` §5.3, lote L2): veredito único,
//! botões de ação, resumo dos jogos e saúde do PC (só com dados).
//!
//! O estado próprio da tela (boas-vindas vista, ajustes desligados, ação em
//! andamento e o último resultado) fica num `Global` do aplicativo, para não
//! mexer na janela legada.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, StyledExt, WindowExt, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::estrutura::Janela;
use super::estrutura::{Secao, estado_vazio, icone_destaque, superficie};
use crate::abrir::{self, Bloqueio, FalhaAbrir};
use crate::demonstracao;
use crate::local;
use crate::modelo::{
    Conexao, Fatos, MotivoIndisponivel, StatusJogo, Veredito, status_jogo, veredito,
};
use crate::servico::{self, AcaoInicio, FalhaInicio, RespostaInicio};
use crate::tema;
use crate::textos::inicio as t;
use crate::textos::jogos as tj;
use crate::textos::saude::{self as ts, SecaoSaude};

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
// Abrir inicialização (D3, R-S-04), usado também pelo Avançado
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

/// Tratador de clique: única chamada de [`abrir::abrir_inicializacao`].
/// Na demonstração nada abre.
pub(super) fn clicar_abrir(janela: &Janela, cx: &mut Context<Janela>) {
    let aviso = if demonstracao::ativo().is_some() {
        Some(ts::ABRIR_DEMONSTRACAO)
    } else {
        match abrir::abrir_inicializacao(estado_servico(janela), &janela.ui.dados.jogos) {
            Ok(()) => None,
            Err(FalhaAbrir::Bloqueado(b)) => Some(texto_bloqueio(b)),
            Err(FalhaAbrir::Windows) => Some(ts::ABRIR_FALHOU),
        }
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
            Button::new(id)
                .outline()
                .small()
                .label(rotulo)
                .disabled(bloqueio.is_some())
                .on_click(cx.listener(|this, _, _, cx| clicar_abrir(this, cx))),
        )
        .when_some(aviso, |c, a| {
            c.child(
                div()
                    .text_size(px(tema::TAM_LEGENDA))
                    .text_color(tema.muted_foreground)
                    .child(a),
            )
        })
}

/// Seção Saúde do PC do Início (só com dados de `ObterSaude`).
fn secao_saude(s: &nvt_ipc::Saude, janela: &Janela, cx: &mut Context<Janela>) -> Div {
    let tema = cx.theme().clone();
    let titulo = div()
        .text_size(px(tema::TAM_CORPO))
        .font_semibold()
        .child(t::SECAO_SAUDE);
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
                        .when_some(l.caminho, |d, c| {
                            d.child(
                                div()
                                    .text_size(px(tema::TAM_LEGENDA))
                                    .text_color(tema.muted_foreground)
                                    .child(c),
                            )
                        })
                        .children(botao),
                ),
        );
    }
    div().child(caixa.child(titulo).child(lista))
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
    let tarefa = cx.background_executor().spawn(async move {
        match demonstracao::ativo() {
            Some(c) => demonstracao::executar_inicio(c, acao),
            None => servico::executar_inicio(acao),
        }
    });
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
    // Na demonstração nada é gravado.
    if demonstracao::ativo().is_none() {
        let _ = local::marcar_boas_vindas();
    }
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

/// As boas-vindas ainda não foram vistas (lê a marca do disco uma vez; na
/// demonstração, decide o cenário e nada é lido).
pub(super) fn boas_vindas_pendente(cx: &mut App) -> bool {
    let e = cx.default_global::<EstadoInicio>();
    !*e.boas_vindas_vista
        .get_or_insert_with(|| match demonstracao::ativo() {
            Some(c) => !c.boas_vindas(),
            None => local::boas_vindas_vista(),
        })
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

/// Conteúdo da seção Início (as boas-vindas são desenhadas pela estrutura).
pub(super) fn render(janela: &Janela, cx: &mut Context<Janela>) -> AnyElement {
    let estado = cx.global::<EstadoInicio>().clone();
    let fatos = janela.fatos();
    // Saúde do PC: cache da thread de comunicação (`ObterSaude`); sem
    // resposta, a seção não aparece e "piorou" nunca vence.
    let saude = janela.ui.dados.saude.clone();
    let v = veredito(&fatos, janela.ui.saude().as_ref());
    let texto = t::veredito(&v);
    let b = botoes(&v, &fatos);
    let tema = cx.theme().clone();
    let ocupado = estado.ocupado;
    let cor = cor_tom(tom(&v), &tema);

    // Um só botão primário (aplicar ou voltar a ajustar); "Desfazer tudo"
    // é a ação destrutiva, com estilo próprio e confirmação.
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
                .large()
                .h(px(tema::ALTURA_BOTAO))
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
                .large()
                .h(px(tema::ALTURA_BOTAO))
                .label(t::BOTAO_VOLTAR_A_AJUSTAR)
                .disabled(ocupado.is_some())
                .loading(ocupado == Some(AcaoInicio::VoltarAAjustar))
                .on_click(cx.listener(|_, _, _, cx| executar(AcaoInicio::VoltarAAjustar, cx))),
        );
    }
    // "Desfazer tudo" fica fora do cartão: link neutro no rodapé; o estilo
    // de atenção aparece só na confirmação.
    let desfazer_el = b.desfazer.then(|| {
        h_flex().child(
            Button::new("desfazer-tudo")
                .ghost()
                .small()
                .text_color(tema.muted_foreground)
                .label(t::LINK_DESFAZER_TUDO)
                .disabled(ocupado.is_some())
                .loading(ocupado == Some(AcaoInicio::DesfazerTudo))
                .on_click(cx.listener(|_, _, window, cx| abrir_confirmacao(window, cx))),
        )
    });
    let tem_acoes = b.aplicar.is_some() || b.voltar;

    let cor_resultado = |tom: Tom| match tom {
        Tom::Sucesso => tema.success,
        Tom::Informacao => tema.foreground,
        Tom::Erro => tema.danger,
    };
    let legenda = |s: &'static str| {
        div()
            .text_size(px(tema::TAM_LEGENDA))
            .text_color(tema.muted_foreground)
            .child(s)
    };
    let corpo = v_flex()
        .flex_1()
        .min_w_0()
        .gap(px(tema::ESP_4))
        .child(
            v_flex()
                .gap(px(tema::ESP_1))
                .child(
                    div()
                        .text_size(px(tema::TAM_SUBTITULO))
                        .font_semibold()
                        .child(texto.titulo),
                )
                .child(
                    div()
                        .text_size(px(tema::TAM_CORPO))
                        .text_color(tema.muted_foreground)
                        .child(texto.frase),
                ),
        )
        .when(tem_acoes, |c| c.child(acoes))
        .when_some(motivo, |c, m| c.child(legenda(m)))
        .when(ocupado.is_some(), |c| c.child(legenda(t::AGUARDE)))
        .when_some(estado.resultado.clone(), |c, (tom, m)| {
            c.child(
                div()
                    .text_size(px(tema::TAM_CORPO))
                    .text_color(cor_resultado(tom))
                    .child(m),
            )
        });
    let veredito_el = superficie(&tema).id("veredito").p(px(tema::ESP_6)).child(
        h_flex()
            .gap(px(tema::ESP_6))
            .items_start()
            .child(icone_destaque(icone(&v), cor))
            .child(corpo),
    );

    let esperando = fatos.esperando_jogo;
    let (linhas, resto) = resumo_jogos(
        janela
            .ui
            .jogos()
            .iter()
            .map(|j| (j.nome.clone(), status_jogo(j, esperando))),
        JOGOS_NO_RESUMO,
    );
    let vazio = linhas.is_empty();
    let lista = v_flex().children(linhas.into_iter().enumerate().map(|(i, (nome, status))| {
        h_flex()
            .justify_between()
            .gap(px(tema::ESP_4))
            .py(px(tema::ESP_3))
            .when(i > 0, |d| d.border_t_1().border_color(tema.border))
            .text_size(px(tema::TAM_CORPO))
            .child(nome)
            .child(div().text_color(tema.muted_foreground).child(status))
    }));
    let jogos_el = superficie(&tema)
        .p(px(tema::ESP_4))
        .gap(px(tema::ESP_2))
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_size(px(tema::TAM_CORPO))
                        .font_semibold()
                        .child(t::SECAO_JOGOS),
                )
                .child(
                    Button::new("ver-todos-jogos")
                        .ghost()
                        .label(t::VER_TODOS)
                        .on_click(cx.listener(|this, _, _, cx| this.ir_secao(Secao::Jogos, cx))),
                ),
        )
        .when(vazio, |c| {
            c.child(estado_vazio(
                IconName::Gamepad2,
                tj::NENHUM_JOGO,
                None,
                &tema,
            ))
        })
        .when(!vazio, |c| c.child(lista))
        .when(resto > 0, |c| {
            c.child(legenda_dinamica(t::mais_jogos(resto), &tema))
        });

    v_flex()
        .gap(px(tema::ESP_6))
        .max_w(px(tema::LARGURA_CONTEUDO))
        .child(veredito_el)
        .when(!matches!(v, Veredito::Verificando), |c| c.child(jogos_el))
        .when(!matches!(v, Veredito::Verificando), |c| {
            c.children(
                saude
                    .as_ref()
                    .filter(|s| mostra_saude(Some(*s)))
                    .map(|s| secao_saude(s, janela, cx)),
            )
        })
        .children(desfazer_el)
        .into_any_element()
}

fn legenda_dinamica(s: String, tema: &component::Theme) -> Div {
    div()
        .text_size(px(tema::TAM_LEGENDA))
        .text_color(tema.muted_foreground)
        .child(s)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::modelo::{Saude, TipoSinal};
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
                false,
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
    fn servico_iniciando_ou_sem_resposta_mostra_diagnostico_sem_acoes() {
        for (f, titulo) in [
            (
                Fatos {
                    servico_iniciando: true,
                    ..pronto()
                },
                "Iniciando o nv-tuner…",
            ),
            (
                Fatos {
                    servico_sem_resposta: true,
                    ..pronto()
                },
                "O nv-tuner não está respondendo",
            ),
        ] {
            let v = veredito(&f, None);
            assert_eq!(t::veredito(&v).titulo, titulo);
            assert_eq!(
                botoes(&v, &f),
                Botoes {
                    aplicar: None,
                    desfazer: false,
                    voltar: false
                }
            );
        }
    }

    #[test]
    fn desfazer_so_aparece_com_algo_a_desfazer() {
        let com = pronto();
        assert!(botoes(&veredito(&com, None), &com).desfazer);
        let sem = Fatos {
            plano_com_mudancas: false,
            ..pronto()
        };
        assert!(!botoes(&veredito(&sem, None), &sem).desfazer);
        let desligado = Fatos {
            ajustes_desligados: true,
            ..pronto()
        };
        let b = botoes(&veredito(&desligado, None), &desligado);
        assert!(!b.desfazer && b.voltar);
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
        let s = crate::demonstracao::Cenario::Piorou.saude().unwrap();
        assert!(mostra_saude(Some(&s)));
        // "Algo piorou" vem dos sinais em atenção da resposta (R-S-06).
        let resumo = Saude::de_ipc(&s);
        assert_eq!(
            resumo.em_atencao,
            vec![TipoSinal::EspacoLivre, TipoSinal::PlanoEnergia]
        );
        let f = pronto();
        assert_eq!(
            t::veredito(&veredito(&f, Some(&resumo))).titulo,
            "Algo piorou: pouco espaço livre"
        );
        // Programas novos não pioram o veredito; só sinais em `Atencao`.
        let novos = crate::demonstracao::Cenario::Saude.saude().unwrap();
        assert!(Saude::de_ipc(&novos).em_atencao.is_empty());
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
