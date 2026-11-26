//! Tela Partidas e grupo Métricas do Avançado (só `medicao-partidas`; spec
//! `interface-e-saude.md` §5.6 e §7.4).
//!
//! As partidas são lidas pelo IPC numa conexão própria, fora da thread da
//! janela, e guardadas num `Global` do aplicativo. Na demonstração, os dados
//! vêm de `demonstracao::partidas` e nada fala com o serviço.

use std::collections::BTreeMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::Theme;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants};
use gpui_kit::component::chart::LineChart;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable, StyledExt, WindowExt};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use nvt_ipc::{AmostraSegundo, PARTIDAS_MAX, Partida, Requisicao, Resposta, ResumoPartida};

use super::estrutura::{Janela, agora_unix, estado_vazio, explicacao, linha, superficie};
use crate::demonstracao;
use crate::servico::{self, FalhaInicio};
use crate::tema;
use crate::textos::partidas as tp;

/// Altura do gráfico pequeno de FPS.
const ALTURA_GRAFICO: f32 = 120.;
/// Pontos desenhados no gráfico (as amostras são agrupadas em faixas).
const PONTOS_GRAFICO: usize = 90;

/// O que se pede ao serviço (ou à demonstração).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pedido {
    Listar,
    Obter(u64),
    Apagar,
}

impl Pedido {
    fn requisicao(self) -> Requisicao {
        match self {
            Pedido::Listar => Requisicao::ListarPartidas {
                desde_unix: 0,
                limite: PARTIDAS_MAX,
            },
            Pedido::Obter(id) => Requisicao::ObterPartida { id },
            Pedido::Apagar => Requisicao::ApagarPartidas,
        }
    }
}

/// Resultado de um pedido, já interpretado.
#[derive(Debug, Clone, PartialEq)]
enum Retorno {
    Lista(Vec<ResumoPartida>),
    Detalhe(Box<Partida>),
    Apagado,
}

/// Situação da lista de partidas.
#[derive(Debug, Clone, Default, PartialEq)]
enum Carga {
    #[default]
    NaoLida,
    Lendo,
    Pronta(Vec<ResumoPartida>),
    Falha(FalhaInicio),
}

#[derive(Default)]
struct EstadoPartidas {
    carga: Carga,
    detalhes: BTreeMap<u64, Partida>,
    /// Partida com a linha aberta.
    aberta: Option<u64>,
    apagando: bool,
    /// Detalhes já pedidos (evita pedir duas vezes pelo mesmo id).
    pedidos: std::collections::BTreeSet<u64>,
    mensagem: Option<String>,
}

impl Global for EstadoPartidas {}

/// Resposta do serviço para o pedido; inválida ou de outro tipo = falha.
fn interpretar(p: Pedido, r: Resposta) -> Result<Retorno, FalhaInicio> {
    match (p, r) {
        (Pedido::Listar, Resposta::Partidas { mut resumos }) => {
            resumos.retain(|r| r.validar().is_ok());
            resumos.sort_by_key(|r| std::cmp::Reverse(r.inicio_unix));
            Ok(Retorno::Lista(resumos))
        }
        (Pedido::Obter(_), Resposta::Partida(d)) if d.validar().is_ok() => {
            Ok(Retorno::Detalhe(Box::new(d)))
        }
        (Pedido::Apagar, Resposta::Ok) => Ok(Retorno::Apagado),
        (
            _,
            Resposta::Erro {
                codigo: nvt_ipc::CodigoErro::RecursoAusente,
                ..
            },
        ) => Err(FalhaInicio::RecursoAusente),
        (_, Resposta::Erro { codigo, mensagem }) => Err(FalhaInicio::Outra(
            crate::textos::erro_servico(codigo, &mensagem),
        )),
        _ => Err(FalhaInicio::Outra(tp::FALHA.into())),
    }
}

/// Pedido bloqueante (chamar fora da thread da janela).
fn executar(p: Pedido) -> Result<Retorno, FalhaInicio> {
    match demonstracao::ativo() {
        Some(c) => {
            let todas = demonstracao::partidas(c);
            Ok(match p {
                Pedido::Listar => Retorno::Lista(todas.into_iter().map(|p| p.resumo).collect()),
                Pedido::Obter(id) => match todas.into_iter().find(|p| p.resumo.id == id) {
                    Some(d) => Retorno::Detalhe(Box::new(d)),
                    None => return Err(FalhaInicio::Outra(tp::FALHA.into())),
                },
                Pedido::Apagar => Retorno::Apagado,
            })
        }
        None => servico::requisitar(&p.requisicao()).and_then(|r| interpretar(p, r)),
    }
}

fn pedir(p: Pedido, cx: &mut Context<Janela>) {
    let tarefa = cx.background_executor().spawn(async move { executar(p) });
    cx.spawn(async move |this, cx| {
        let r = tarefa.await;
        let _ = this.update(cx, |_, cx| {
            concluir(cx.default_global::<EstadoPartidas>(), p, r);
            cx.notify();
        });
    })
    .detach();
}

fn concluir(e: &mut EstadoPartidas, p: Pedido, r: Result<Retorno, FalhaInicio>) {
    match (p, r) {
        (_, Ok(Retorno::Lista(l))) => {
            if e.aberta.is_none() {
                e.aberta = l.first().map(|r| r.id);
            }
            e.carga = Carga::Pronta(l);
        }
        (_, Ok(Retorno::Detalhe(d))) => {
            e.detalhes.insert(d.resumo.id, *d);
        }
        (_, Ok(Retorno::Apagado)) => {
            e.apagando = false;
            e.carga = Carga::Pronta(Vec::new());
            e.detalhes.clear();
            e.aberta = None;
            e.mensagem = Some(tp::APAGADO.into());
        }
        (Pedido::Apagar, Err(f)) => {
            e.apagando = false;
            e.mensagem = Some(mensagem_falha(&f).into());
        }
        (Pedido::Obter(_), Err(_)) => {}
        (Pedido::Listar, Err(f)) => e.carga = Carga::Falha(f),
    }
}

fn mensagem_falha(f: &FalhaInicio) -> &str {
    match f {
        FalhaInicio::RecursoAusente => tp::RECURSO_AUSENTE,
        FalhaInicio::SemServico => tp::SEM_SERVICO,
        FalhaInicio::Outra(m) => m,
    }
}

/// Lê a lista na primeira vez e o detalhe da partida aberta.
fn garantir(cx: &mut Context<Janela>) {
    let e = cx.default_global::<EstadoPartidas>();
    if e.carga == Carga::NaoLida {
        e.carga = Carga::Lendo;
        pedir(Pedido::Listar, cx);
        return;
    }
    if let Some(id) = e.aberta.filter(|id| !e.detalhes.contains_key(id))
        && e.pedidos.insert(id)
    {
        pedir(Pedido::Obter(id), cx);
    }
}

// ---------------------------------------------------------------------------
// Cálculos (puros, testados)
// ---------------------------------------------------------------------------

/// Instante de referência: relógio real, ou o fixo da demonstração.
fn agora() -> u64 {
    match demonstracao::ativo() {
        Some(_) => demonstracao::AGORA_PARTIDAS,
        None => agora_unix(),
    }
}

fn local(unix: u64) -> i64 {
    let u = i64::try_from(unix).unwrap_or(i64::MAX / 2);
    u + crate::fuso::deslocamento_local(u)
}

/// Semana (segunda a domingo, como a ISO) do instante local.
fn semana(local: i64) -> i64 {
    // 01/01/1970 foi quinta-feira: +3 dias alinha o início na segunda.
    (local.div_euclid(86_400) + 3).div_euclid(7)
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Semana {
    partidas: u32,
    com_engasgo: u32,
    soma_fps: f32,
}

impl Semana {
    fn media(&self) -> Option<f32> {
        (self.partidas > 0).then(|| self.soma_fps / self.partidas as f32)
    }
}

/// Esta semana e a passada, das partidas que passam no filtro.
fn semanas(
    lista: &[ResumoPartida],
    agora_local: i64,
    local_de: impl Fn(u64) -> i64,
    filtro: impl Fn(&ResumoPartida) -> bool,
) -> (Semana, Semana) {
    let atual = semana(agora_local);
    let (mut esta, mut passada) = (Semana::default(), Semana::default());
    for r in lista.iter().filter(|r| filtro(r)) {
        let s = match atual - semana(local_de(r.inicio_unix)) {
            0 => &mut esta,
            1 => &mut passada,
            _ => continue,
        };
        s.partidas += 1;
        s.com_engasgo += u32::from(r.engasgos > 0);
        s.soma_fps += r.fps_medio;
    }
    (esta, passada)
}

/// FPS por faixa de tempo (média das amostras), no máximo `pontos` faixas.
fn serie(amostras: &[AmostraSegundo], pontos: usize) -> Vec<(u32, f64)> {
    if amostras.is_empty() || pontos == 0 {
        return Vec::new();
    }
    let passo = amostras.len().div_ceil(pontos);
    amostras
        .chunks(passo)
        .map(|c| {
            let soma: f64 = c.iter().map(|a| f64::from(a.quadros)).sum();
            (c[0].t_rel_s, soma / c.len() as f64)
        })
        .collect()
}

/// Nome do jogo: o da tela Jogos (pelo ID da base), senão o executável.
fn nome_jogo(janela: &Janela, r: &ResumoPartida) -> String {
    janela
        .ui
        .dados
        .jogos
        .iter()
        .find(|j| j.competitivo.as_deref() == Some(r.jogo_id.as_str()))
        .map(|j| j.nome.clone())
        .unwrap_or_else(|| {
            r.executavel
                .strip_suffix(".exe")
                .unwrap_or(&r.executavel)
                .to_owned()
        })
}

// ---------------------------------------------------------------------------
// Desenho
// ---------------------------------------------------------------------------

/// Conteúdo da seção Partidas.
pub(super) fn render(janela: &Janela, cx: &mut Context<Janela>) -> AnyElement {
    garantir(cx);
    let tema = cx.theme().clone();
    let e = cx.global::<EstadoPartidas>();
    let lista = match &e.carga {
        Carga::NaoLida | Carga::Lendo => {
            return explicacao(crate::textos::CARREGANDO, &tema).into_any_element();
        }
        Carga::Falha(f) => {
            return estado_vazio(IconName::ChartLine, falha_estatica(f), None, &tema)
                .into_any_element();
        }
        Carga::Pronta(l) if l.is_empty() => {
            return estado_vazio(IconName::ChartLine, tp::VAZIO, None, &tema).into_any_element();
        }
        Carga::Pronta(l) => l.clone(),
    };
    let aberta = e.aberta;
    let detalhe = aberta.and_then(|id| e.detalhes.get(&id).cloned());
    let agora_l = local(agora());
    let (esta, passada) = semanas(&lista, agora_l, local, |_| true);
    let mut linhas = superficie(&tema);
    for (i, r) in lista.iter().enumerate() {
        let abrir = aberta == Some(r.id);
        let id = r.id;
        linhas = linhas.child(
            h_flex()
                .id(("partida", i))
                .w_full()
                .gap(px(tema::ESP_4))
                .px(px(tema::ESP_4))
                .py(px(tema::ESP_3))
                .items_center()
                .cursor_pointer()
                .when(i > 0, |d| d.border_t_1().border_color(tema.border))
                .hover(|d| d.bg(tema::cor(tema::SUPERFICIE_HOVER)))
                .on_click(cx.listener(move |_, _, _, cx| {
                    let e = cx.default_global::<EstadoPartidas>();
                    e.aberta = if e.aberta == Some(id) { None } else { Some(id) };
                    cx.notify();
                }))
                .child(
                    Icon::new(if abrir {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .size(px(tema::ESP_4))
                    .text_color(tema.muted_foreground),
                )
                .child(div().flex_1().font_semibold().child(nome_jogo(janela, r)))
                .child(
                    div()
                        .w(px(tema::COLUNA_VALOR))
                        .text_color(tema.muted_foreground)
                        .child(tp::quando(local(r.inicio_unix), agora_l)),
                )
                .child(
                    div()
                        .w(px(tema::COLUNA_CURTA))
                        .text_color(tema.muted_foreground)
                        .child(tp::duracao(r.duracao_s)),
                )
                .child(
                    div()
                        .w(px(tema::COLUNA_VALOR))
                        .text_right()
                        .font_semibold()
                        .text_color(if r.engasgos == 0 {
                            tema.success
                        } else {
                            tema.warning
                        })
                        .child(tp::veredito(r.engasgos)),
                ),
        );
        if abrir {
            linhas = linhas.child(detalhe_partida(&lista, r, detalhe.as_ref(), agora_l, &tema));
        }
    }
    v_flex()
        .gap(px(tema::ESP_4))
        .max_w(px(tema::LARGURA_CONTEUDO))
        .child(explicacao_dinamica(
            tp::comparacao_geral(
                (esta.partidas, esta.com_engasgo),
                (passada.partidas, passada.com_engasgo),
            ),
            &tema,
        ))
        .child(linhas)
        .into_any_element()
}

fn falha_estatica(f: &FalhaInicio) -> &'static str {
    match f {
        FalhaInicio::RecursoAusente => tp::RECURSO_AUSENTE,
        FalhaInicio::SemServico => tp::SEM_SERVICO,
        FalhaInicio::Outra(_) => tp::FALHA,
    }
}

fn explicacao_dinamica(texto: String, tema: &Theme) -> Div {
    div()
        .text_size(px(tema::TAM_CORPO))
        .text_color(tema.muted_foreground)
        .child(texto)
}

fn detalhe_partida(
    lista: &[ResumoPartida],
    r: &ResumoPartida,
    detalhe: Option<&Partida>,
    agora_l: i64,
    tema: &Theme,
) -> Div {
    let grafico: AnyElement = match detalhe {
        None => explicacao(tp::CARREGANDO_DETALHE, tema).into_any_element(),
        Some(d) if d.amostras.is_empty() => explicacao(tp::SEM_AMOSTRAS, tema).into_any_element(),
        Some(d) => {
            let pontos = serie(&d.amostras, PONTOS_GRAFICO);
            let marcas = (pontos.len() / 6).max(1);
            v_flex()
                .gap(px(tema::ESP_1))
                .child(
                    div()
                        .text_size(px(tema::TAM_LEGENDA))
                        .text_color(tema.muted_foreground)
                        .child(tp::LEGENDA_GRAFICO),
                )
                .child(
                    div().h(px(ALTURA_GRAFICO)).w_full().child(
                        LineChart::new(pontos)
                            .id(("grafico-partida", r.id))
                            .x(|p: &(u32, f64)| tp::minuto(p.0))
                            .y(|p: &(u32, f64)| p.1)
                            .stroke(tema::cor(tema::DESTAQUE))
                            .tick_margin(marcas)
                            .appear(false)
                            .linear(),
                    ),
                )
                .into_any_element()
        }
    };
    let mut numeros = h_flex()
        .gap(px(tema::ESP_6))
        .flex_wrap()
        .child(tp::media(r.fps_medio))
        .child(tp::piores_momentos(r.fps_1pct_low))
        .child(tp::veredito(r.engasgos));
    if let Some(c) = r.gpu_temp_max_c {
        numeros = numeros.child(tp::temperatura(c));
    }
    let id = r.jogo_id.clone();
    let (esta, passada) = semanas(lista, agora_l, local, |x| x.jogo_id == id);
    v_flex()
        .gap(px(tema::ESP_3))
        .px(px(tema::ESP_8))
        .pb(px(tema::ESP_4))
        .child(grafico)
        .child(numeros.text_size(px(tema::TAM_CORPO)))
        .child(explicacao_dinamica(
            tp::comparacao_jogo(
                (esta.partidas, esta.media()),
                (passada.partidas, passada.media()),
            ),
            tema,
        ))
}

/// Grupo Métricas no fim do Avançado: números técnicos da última partida,
/// definições e "Apagar histórico de partidas".
pub(super) fn grupo_metricas(_janela: &Janela, cx: &mut Context<Janela>) -> AnyElement {
    garantir(cx);
    let tema = cx.theme().clone();
    let e = cx.global::<EstadoPartidas>();
    let ultima = match &e.carga {
        Carga::Pronta(l) => l.first().cloned(),
        _ => None,
    };
    let ausente = matches!(
        e.carga,
        Carga::Falha(FalhaInicio::RecursoAusente | FalhaInicio::SemServico)
    );
    let apagando = e.apagando;
    let mensagem = e.mensagem.clone();
    let mut corpo = v_flex().gap(px(tema::ESP_3));
    match (&e.carga, &ultima) {
        (Carga::Falha(f), _) => corpo = corpo.child(explicacao(falha_estatica(f), &tema)),
        (_, None) => corpo = corpo.child(explicacao(tp::SEM_PARTIDA, &tema)),
        (_, Some(r)) => {
            let frametime = if r.fps_medio > 0.0 {
                tp::ms(1000.0 / r.fps_medio)
            } else {
                "-".into()
            };
            corpo = corpo
                .child(linha(
                    tp::ROTULO_JOGO,
                    format!(
                        "{} · {}",
                        r.executavel,
                        crate::modelo::formatar_data(r.inicio_unix)
                    ),
                    &tema,
                ))
                .child(linha(tp::ROTULO_FRAMETIME, frametime, &tema))
                .child(linha(tp::ROTULO_MEDIANA, tp::fps(r.fps_mediana), &tema))
                .child(linha(tp::ROTULO_1PCT, tp::fps(r.fps_1pct_low), &tema))
                .child(linha(tp::ROTULO_01PCT, tp::fps(r.fps_01pct_low), &tema))
                .child(linha(
                    tp::ROTULO_ENGASGADO,
                    tp::tempo_engasgado(r.tempo_engasgado_ms),
                    &tema,
                ))
                .when_some(r.gpu_uso_medio_pct, |c, u| {
                    c.child(linha(tp::ROTULO_USO_GPU, format!("{u}%"), &tema))
                })
                .child(linha(
                    tp::ROTULO_PERDA,
                    tp::percentual(r.perda_eventos_pct),
                    &tema,
                ));
        }
    }
    v_flex()
        .gap(px(tema::ESP_3))
        .max_w(px(tema::LARGURA_CONTEUDO))
        .child(
            v_flex()
                .gap(px(tema::ESP_1))
                .child(
                    div()
                        .text_size(px(tema::TAM_SUBTITULO))
                        .font_semibold()
                        .child(tp::GRUPO_METRICAS),
                )
                .child(explicacao(tp::GRUPO_METRICAS_DESC, &tema)),
        )
        .child(superficie(&tema).p(px(tema::ESP_4)).child(corpo))
        .child(explicacao(tp::EXPLICA_FRAMETIME, &tema))
        .child(explicacao(tp::EXPLICA_1PCT, &tema))
        .child(explicacao(tp::EXPLICA_ENGASGO, &tema))
        .when_some(mensagem, |v, m| v.child(explicacao_dinamica(m, &tema)))
        .child(
            h_flex().child(
                Button::new("apagar-partidas")
                    .danger()
                    .small()
                    .label(tp::BOTAO_APAGAR)
                    .disabled(ausente || apagando)
                    .loading(apagando)
                    .on_click(cx.listener(|_, _, window, cx| confirmar_apagar(window, cx))),
            ),
        )
        .into_any_element()
}

fn confirmar_apagar(window: &mut Window, cx: &mut Context<Janela>) {
    let janela = cx.entity().downgrade();
    window.open_alert_dialog(cx, move |alerta, _, _| {
        let janela = janela.clone();
        alerta
            .title(tp::CONFIRMAR_TITULO)
            .description(tp::CONFIRMAR_TEXTO)
            .show_cancel(true)
            .ok_text(tp::CONFIRMAR_OK)
            .ok_variant(ButtonVariant::Danger)
            .cancel_text(tp::CANCELAR)
            .on_ok(move |_, _, cx| {
                let _ = janela.update(cx, |_, cx| {
                    let e = cx.default_global::<EstadoPartidas>();
                    if !e.apagando {
                        e.apagando = true;
                        e.mensagem = None;
                        e.pedidos.clear();
                        pedir(Pedido::Apagar, cx);
                        cx.notify();
                    }
                });
                true
            })
    });
}

#[cfg(test)]
mod testes {
    use super::*;
    use nvt_ipc::CodigoErro;
    // `gpui_kit::*` também exporta um `test`; este é o da biblioteca padrão.
    use std::prelude::rust_2024::test;

    fn resumo(id: u64, jogo: &str, inicio: u64, engasgos: u32, fps: f32) -> ResumoPartida {
        let mut r = demonstracao::partidas(demonstracao::Cenario::Partidas)[0]
            .resumo
            .clone();
        r.id = id;
        r.jogo_id = jogo.into();
        r.inicio_unix = inicio;
        r.engasgos = engasgos;
        r.fps_medio = fps;
        r
    }

    #[test]
    fn requisicoes() {
        assert_eq!(
            Pedido::Listar.requisicao(),
            Requisicao::ListarPartidas {
                desde_unix: 0,
                limite: PARTIDAS_MAX
            }
        );
        assert!(Pedido::Listar.requisicao().validar().is_ok());
        assert_eq!(
            Pedido::Obter(3).requisicao(),
            Requisicao::ObterPartida { id: 3 }
        );
        assert_eq!(Pedido::Apagar.requisicao(), Requisicao::ApagarPartidas);
    }

    #[test]
    fn recurso_ausente_vira_mensagem_simples() {
        let r = Resposta::erro(CodigoErro::RecursoAusente, "x");
        let f = interpretar(Pedido::Listar, r).unwrap_err();
        assert_eq!(f, FalhaInicio::RecursoAusente);
        assert_eq!(falha_estatica(&f), tp::RECURSO_AUSENTE);
        let mut e = EstadoPartidas::default();
        concluir(&mut e, Pedido::Listar, Err(f));
        assert_eq!(e.carga, Carga::Falha(FalhaInicio::RecursoAusente));
    }

    #[test]
    fn lista_ordenada_e_abre_a_mais_recente() {
        let r = Resposta::Partidas {
            resumos: vec![resumo(1, "cs2", 100, 0, 1.0), resumo(2, "cs2", 200, 0, 1.0)],
        };
        let mut e = EstadoPartidas::default();
        concluir(&mut e, Pedido::Listar, interpretar(Pedido::Listar, r));
        let Carga::Pronta(l) = &e.carga else { panic!() };
        assert_eq!(l[0].id, 2);
        assert_eq!(e.aberta, Some(2));
        concluir(&mut e, Pedido::Apagar, Ok(Retorno::Apagado));
        assert_eq!(e.carga, Carga::Pronta(vec![]));
        assert_eq!(e.aberta, None);
    }

    #[test]
    fn semanas_comparadas() {
        // Quarta-feira, 25/11/2026, meio-dia (UTC, sem fuso).
        let agora = crate::modelo::unix_de_civil(2026, 11, 25, 12, 0, 0);
        let dia = 86_400;
        let segunda = (agora - 2 * dia - 12 * 3600) as u64;
        let lista = [
            resumo(1, "cs2", segunda, 0, 300.0),
            resumo(2, "cs2", segunda + 3600, 4, 200.0),
            resumo(3, "valorant", segunda - 1, 1, 100.0),
            resumo(4, "cs2", segunda - 21 * 86_400, 0, 50.0),
        ];
        let id = |u: u64| u as i64;
        let (e, p) = semanas(&lista, agora, id, |_| true);
        assert_eq!((e.partidas, e.com_engasgo), (2, 1));
        assert_eq!((p.partidas, p.com_engasgo), (1, 1));
        assert_eq!(e.media(), Some(250.0));
        let (e, p) = semanas(&lista, agora, id, |r| r.jogo_id == "valorant");
        assert_eq!((e.partidas, p.partidas), (0, 1));
        assert_eq!(e.media(), None);
    }

    #[test]
    fn serie_agrupada() {
        let a = |t, q| AmostraSegundo {
            t_rel_s: t,
            quadros: q,
            frametime_medio_us: 0,
            frametime_max_us: 0,
            engasgos: 0,
            gpu_uso_pct: None,
            gpu_temp_c: None,
        };
        let amostras: Vec<_> = (0..10).map(|t| a(t, (t * 10) as u16)).collect();
        let s = serie(&amostras, 5);
        assert_eq!(s.len(), 5);
        assert_eq!(s[0], (0, 5.0));
        assert!(serie(&[], 5).is_empty());
        assert!(serie(&amostras, 100).len() == 10);
    }

    #[test]
    fn demonstracao_valida() {
        let ps = demonstracao::partidas(demonstracao::Cenario::Partidas);
        assert!(ps.len() >= 4);
        for p in &ps {
            assert_eq!(p.validar(), Ok(()));
        }
        assert!(ps.iter().any(|p| p.resumo.engasgos == 0));
        assert!(ps.iter().any(|p| p.resumo.engasgos > 0));
        assert!(demonstracao::partidas(demonstracao::Cenario::Otimizado).is_empty());
    }
}
