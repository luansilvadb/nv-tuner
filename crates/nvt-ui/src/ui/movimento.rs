use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::base::motion::{Presence, Transition};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::tema;

/// Revela o conteúdo pela altura medida, incluindo quebras de linha.
pub(super) fn recolhivel<T: 'static>(
    id: impl Into<SharedString>,
    aberto: bool,
    window: &mut Window,
    cx: &mut Context<T>,
    conteudo: impl FnOnce(&mut Window, &mut Context<T>) -> AnyElement,
) -> Option<AnyElement> {
    let id: SharedString = id.into();
    let presenca = Presence::new(id.clone(), aberto)
        .transition(Transition::new(Duration::from_millis(240)))
        .sample(window, cx);
    if !presenca.should_render() {
        return None;
    }
    let altura = window
        .use_keyed_state(SharedString::from(format!("altura-{id}")), cx, |_, _| {
            Rc::new(Cell::new(Pixels::ZERO))
        })
        .read(cx)
        .clone();
    let progresso = presenca.progress;
    let limite = altura.get() * progresso;
    let conteudo = conteudo(window, cx);
    Some(
        v_flex()
            .min_h_0()
            .overflow_hidden()
            .when(progresso < 1., |d| d.max_h(limite).opacity(progresso))
            .child(
                div().relative().flex_shrink_0().child(conteudo).child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, (), window, _| {
                            if altura.replace(bounds.size.height) != bounds.size.height
                                && progresso < 1.
                            {
                                window.request_animation_frame();
                            }
                        },
                    )
                    .absolute()
                    .inset_0(),
                ),
            )
            .into_any_element(),
    )
}

#[derive(Clone, Copy)]
struct Toque {
    origem: Point<Pixels>,
    inicio: Instant,
    soltou: Option<Instant>,
}

const EXPANSAO: Duration = Duration::from_millis(480);
const DISSIPACAO: Duration = Duration::from_millis(420);

/// Mostra a confirmação do clique sem interceptar a ação do controle.
pub(super) fn ripple(
    id: &'static str,
    habilitado: bool,
    controle: impl IntoElement,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    ripple_com_cor(id, habilitado, controle, hsla(0., 0., 1., 0.24), window, cx)
}

pub(super) fn ripple_com_cor(
    id: &'static str,
    habilitado: bool,
    controle: impl IntoElement,
    cor: Hsla,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    ripple_com_forma(id, habilitado, controle, cor, (None, None), window, cx)
}

/// Ripple com cantos escolhidos `(esquerda, direita)`: `None` é meia cápsula
/// (botões); `Some(r)` recorta a onda com canto `r` naquele lado (linhas de
/// lista, segmentos de um segmented, que só arredondam a ponta de fora).
pub(super) fn ripple_com_forma(
    id: impl Into<SharedString>,
    habilitado: bool,
    controle: impl IntoElement,
    cor: Hsla,
    cantos: (Option<f32>, Option<f32>),
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let id: SharedString = id.into();
    let area = div().id(id.clone()).relative().child(controle);
    if cx.reduce_motion() {
        return area;
    }

    let estado = window
        .use_keyed_state(SharedString::from(format!("ripple-{id}")), cx, |_, _| {
            Rc::new(Cell::<Option<Toque>>::new(None))
        })
        .read(cx)
        .clone();
    area.child(
        canvas(
            |_, _, _| (),
            move |limites, (), window, _| {
                let area_visivel = limites.intersect(&window.content_mask().bounds);
                let clique = estado.clone();
                window.on_mouse_event(move |evento: &MouseDownEvent, fase, window, _| {
                    if fase == DispatchPhase::Capture
                        && habilitado
                        && evento.button == MouseButton::Left
                        && area_visivel.contains(&evento.position)
                    {
                        clique.set(Some(Toque {
                            origem: evento.position - limites.origin,
                            inicio: Instant::now(),
                            soltou: None,
                        }));
                        window.refresh();
                    }
                });

                let liberar = estado.clone();
                window.on_mouse_event(move |evento: &MouseUpEvent, fase, window, _| {
                    if fase == DispatchPhase::Capture
                        && evento.button == MouseButton::Left
                        && let Some(mut toque) = liberar.get()
                    {
                        toque.soltou = Some(Instant::now());
                        liberar.set(Some(toque));
                        window.refresh();
                    }
                });

                let Some(toque) = estado.get() else { return };
                let t = (toque.inicio.elapsed().as_secs_f32() / EXPANSAO.as_secs_f32()).min(1.);
                let dissipacao = toque.soltou.map_or(0., |soltou| {
                    soltou.elapsed().as_secs_f32() / DISSIPACAO.as_secs_f32()
                });
                if dissipacao >= 1. {
                    estado.set(None);
                    return;
                }
                if let Some(onda) = contorno(
                    limites,
                    limites.origin + toque.origem,
                    1. - (1. - t).powi(2),
                    cantos,
                ) {
                    window.paint_path(onda, cor.opacity(1. - dissipacao));
                }
                if t < 1. || toque.soltou.is_some() {
                    window.request_animation_frame();
                }
            },
        )
        .absolute()
        .size_full()
        .top_0()
        .left_0(),
    )
}

/// Interpola os vértices da seta à medida que o menu muda de largura.
pub(super) fn seta_lateral(abertura: f32, cor: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |area, (), window, _| {
            let centro = area.center();
            let largura = area.size.width * (0.125 * (1. - 2. * abertura));
            let altura = area.size.height * 0.25;
            let mut seta = PathBuilder::stroke(px(2.));
            seta.move_to(centro + point(-largura, -altura));
            seta.line_to(centro + point(largura, px(0.)));
            seta.line_to(centro + point(-largura, altura));
            if let Ok(seta) = seta.build() {
                window.paint_path(seta, cor);
            }
        },
    )
    .size(px(tema::ESP_6))
}

/// Interseção entre a onda circular e o contorno do controle (cápsula, ou
/// retângulo com canto próprio em cada lado).
fn contorno(
    area: Bounds<Pixels>,
    origem: Point<Pixels>,
    progresso: f32,
    cantos: (Option<f32>, Option<f32>),
) -> Option<Path> {
    let largura = f32::from(area.size.width);
    let altura = f32::from(area.size.height);
    if largura <= 0. || altura <= 0. {
        return None;
    }
    let x = f32::from(origem.x - area.origin.x).clamp(0., largura);
    let y = f32::from(origem.y - area.origin.y).clamp(0., altura);
    let alcance = x.max(largura - x).hypot(y.max(altura - y));
    let onda = 4. + progresso * alcance;
    let raio = |c: Option<f32>| c.unwrap_or(f32::INFINITY).min(largura.min(altura) / 2.);
    let (canto_esq, canto_dir) = (raio(cantos.0), raio(cantos.1));
    // Quanto a borda recua, nesta altura, por causa do canto daquele lado.
    let recuo = |canto: f32, altura_linha: f32| {
        let dy = altura_linha.clamp(canto, altura - canto) - altura_linha;
        canto - (canto * canto - dy * dy).max(0.).sqrt()
    };
    let topo = (y - onda).max(0.);
    let fundo = (y + onda).min(altura);
    let bordas: Vec<_> = (0..=32)
        .filter_map(|linha| {
            let altura_linha = topo + (fundo - topo) * linha as f32 / 32.;
            let dy = altura_linha - y;
            let semilargura = (onda * onda - dy * dy).max(0.).sqrt();
            let inicio = (x - semilargura).max(recuo(canto_esq, altura_linha));
            let fim = (x + semilargura).min(largura - recuo(canto_dir, altura_linha));
            let posicao = |x| area.origin + point(px(x), px(altura_linha));
            (inicio <= fim).then(|| (posicao(inicio), posicao(fim)))
        })
        .collect();

    let mut caminho = PathBuilder::fill();
    caminho.move_to(bordas.first()?.0);
    for (inicio, _) in bordas.iter().skip(1) {
        caminho.line_to(*inicio);
    }
    for (_, fim) in bordas.iter().rev() {
        caminho.line_to(*fim);
    }
    caminho.close();
    caminho.build().ok()
}
