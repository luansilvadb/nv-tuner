//! Tela Partidas (só `medicao-partidas`; spec
//! `interface-e-saude.md` §5.6 e §7.4).
//!
//! As partidas são lidas pelo IPC numa conexão própria, fora da thread da
//! janela, e guardadas num `Global` do aplicativo. Na demonstração, os dados
//! vêm do backend de demonstração e nada fala com o serviço.

use std::collections::{BTreeMap, HashMap};

use gpui_kit::assets::IconName;
use gpui_kit::component::Theme;
use gpui_kit::component::chart::LineChart;
use gpui_kit::component::{ActiveTheme, Icon, StyledExt};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use nvt_ipc::{AmostraSegundo, Partida, ResumoPartida};

use super::estrutura::{Janela, estado_vazio, explicacao, legenda, superficie};
use crate::backend::{Backend, PedidoPartidas as Pedido, RetornoPartidas as Retorno};
use crate::servico::FalhaInicio;
use crate::tema;
use crate::textos::partidas as tp;

/// Altura do gráfico pequeno de FPS.
const ALTURA_GRAFICO: f32 = 120.;
/// Pontos desenhados no gráfico (as amostras são agrupadas em faixas).
const PONTOS_GRAFICO: usize = 90;

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
    /// Detalhes já pedidos (evita pedir duas vezes pelo mesmo id).
    pedidos: std::collections::BTreeSet<u64>,
}

impl Global for EstadoPartidas {}

fn pedir(p: Pedido, cx: &mut Context<Janela>) {
    let backend = Backend::atual();
    let tarefa = cx
        .background_executor()
        .spawn(async move { backend.partidas(p) });
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
        (Pedido::Obter(_), Err(_)) => {}
        (Pedido::Listar, Err(f)) => e.carga = Carga::Falha(f),
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
    Backend::atual().agora_partidas()
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

/// Nomes da tela Jogos pelo ID da base; montado uma vez por desenho. Com IDs
/// repetidos vale o primeiro jogo, como na busca linear de antes.
fn nomes_jogos(janela: &Janela) -> HashMap<&str, &str> {
    let mut nomes = HashMap::new();
    for j in &janela.ui.dados.jogos {
        if let Some(id) = j.competitivo.as_deref() {
            nomes.entry(id).or_insert(j.nome.as_str());
        }
    }
    nomes
}

/// Nome do jogo: o da tela Jogos (pelo ID da base), senão o executável.
fn nome_jogo(nomes: &HashMap<&str, &str>, r: &ResumoPartida) -> String {
    nomes
        .get(r.jogo_id.as_str())
        .map(|nome| (*nome).to_owned())
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
    let tema = cx.theme();
    let e = cx.global::<EstadoPartidas>();
    let lista = match &e.carga {
        Carga::NaoLida | Carga::Lendo => {
            return explicacao(crate::textos::CARREGANDO, tema).into_any_element();
        }
        Carga::Falha(f) => {
            return estado_vazio(IconName::ChartLine, falha_estatica(f), None, tema)
                .into_any_element();
        }
        Carga::Pronta(l) if l.is_empty() => {
            return estado_vazio(IconName::ChartLine, tp::VAZIO, None, tema).into_any_element();
        }
        Carga::Pronta(l) => l,
    };
    let aberta = e.aberta;
    let detalhe = aberta.and_then(|id| e.detalhes.get(&id));
    let nomes = nomes_jogos(janela);
    let agora_l = local(agora());
    let (esta, passada) = semanas(lista, agora_l, local, |_| true);
    let mut linhas = superficie(tema);
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
                .child(div().flex_1().font_semibold().child(nome_jogo(&nomes, r)))
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
            linhas = linhas.child(detalhe_partida(lista, r, detalhe, agora_l, tema));
        }
    }
    v_flex()
        .gap(px(tema::ESP_4))
        .max_w(px(tema::LARGURA_CONTEUDO))
        .child(explicacao(
            tp::comparacao_geral(
                (esta.partidas, esta.com_engasgo),
                (passada.partidas, passada.com_engasgo),
            ),
            tema,
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
                .child(legenda(tp::LEGENDA_GRAFICO, tema))
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
        .child(explicacao(
            tp::comparacao_jogo(
                (esta.partidas, esta.media()),
                (passada.partidas, passada.media()),
            ),
            tema,
        ))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::backend::interpretar_partidas as interpretar;
    use crate::demonstracao;
    use nvt_ipc::CodigoErro;
    use nvt_ipc::{PARTIDAS_MAX, Requisicao, Resposta};
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
