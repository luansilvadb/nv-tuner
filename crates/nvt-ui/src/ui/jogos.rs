//! Seção Jogos (spec `interface-e-saude.md` §5.4, R-UI-12): lista limpa com
//! nome, selo competitivo e um status; detalhe ao clicar com o que o app fez,
//! "Faça no jogo", "Arquivo do jogo" e detalhes técnicos recolhidos.

use std::sync::mpsc::Sender;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme, Disableable, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use nvt_ipc::{Jogo, StatusMudanca};

use super::estrutura::Janela;
use crate::arquivo_jogo as aj;
use crate::modelo::{Comando, EstadoUi, Evento, Fatos, StatusJogo, status_jogo};
use crate::textos as t;
use crate::textos::jogos as tj;

/// Seleção da tela (jogo aberto e detalhes técnicos visíveis). Fica num
/// global porque os campos da [`Janela`] pertencem à estrutura.
#[derive(Default)]
struct Selecao {
    jogo: Option<String>,
    tecnicos: bool,
}
impl Global for Selecao {}

/// Canal de comandos usado por "Reverter" e "Procurar jogos de novo". Sem
/// ele instalado os botões ficam desabilitados; quem cria a janela instala
/// com [`instalar_envio`].
struct Envio(Sender<Comando>);
impl Global for Envio {}

/// Liga os botões da tela Jogos ao canal de comandos da janela.
pub(crate) fn instalar_envio(cx: &mut App, comandos: Sender<Comando>) {
    cx.set_global(Envio(comandos));
}

/// Linha da lista: só o que o leigo precisa (R-UI-12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinhaJogo {
    pub chave: String,
    pub nome: String,
    pub competitivo: bool,
    pub status: StatusJogo,
    pub texto_status: String,
}

fn esperando(ui: &EstadoUi) -> bool {
    Fatos::de_ui(ui, false).esperando_jogo
}

pub(crate) fn linha(j: &Jogo, esperando_jogo: bool) -> LinhaJogo {
    let status = status_jogo(j, esperando_jogo);
    LinhaJogo {
        chave: j.perfil.clone(),
        nome: j.nome.clone(),
        competitivo: j.competitivo.is_some(),
        status,
        texto_status: tj::status(status),
    }
}

/// "O que o nv-tuner fez" em frases simples.
pub(crate) fn feito(j: &Jogo) -> Vec<String> {
    let mut v: Vec<String> = j
        .resumo
        .aplicadas
        .iter()
        .map(|a| t::nome_regra(&a.regra, &a.descricao))
        .collect();
    let arquivo = j.config_jogo.as_ref().is_some_and(|c| {
        c.arquivos.iter().any(|a| {
            a.mudancas
                .iter()
                .any(|m| m.status == StatusMudanca::Aplicada)
        })
    });
    if arquivo {
        v.push(tj::AJUSTOU_ARQUIVO.to_owned());
    }
    v
}

/// Passos numerados de "Faça no jogo".
pub(crate) fn passos(j: &Jogo) -> Vec<String> {
    j.orientacoes
        .iter()
        .map(|o| t::orientacao_ou_codigo(o))
        .chain(aj::orientacoes(j))
        .enumerate()
        .map(|(i, s)| tj::passo(i + 1, &s))
        .collect()
}

/// Detalhes técnicos (recolhidos por padrão): API, DLSS, perfil, anticheat.
pub(crate) fn tecnicos(j: &Jogo) -> Vec<(&'static str, String)> {
    let launcher = if j.launcher.is_empty() {
        "?".to_owned()
    } else {
        j.launcher.clone()
    };
    let dlss = match &j.dlss {
        None => t::NAO.to_owned(),
        Some(v) if v.is_empty() => t::SIM.to_owned(),
        Some(v) => v.clone(),
    };
    let anticheat = match j.anticheat_kernel {
        Some(true) => t::ANTICHEAT_KERNEL,
        _ => "?",
    };
    vec![
        (t::COL_LAUNCHER, launcher),
        (t::COL_API, t::api_grafica(&j.api).to_owned()),
        (t::COL_DLSS, dlss),
        (t::COL_REFLEX, t::talvez(j.reflex).to_owned()),
        (t::COL_ANTICHEAT, anticheat.to_owned()),
        (
            tj::ROTULO_PERFIL,
            format!("“{}” · {}", j.perfil, t::origem_perfil(j.origem_perfil)),
        ),
    ]
}

/// Conteúdo da seção Jogos.
pub(super) fn render(janela: &Janela, cx: &mut Context<Janela>) -> AnyElement {
    let tema = cx.theme().clone();
    let jogos = janela.ui.jogos();
    let esp = esperando(&janela.ui);
    let (aberto, tec) = cx
        .try_global::<Selecao>()
        .map(|s| (s.jogo.clone(), s.tecnicos))
        .unwrap_or_default();
    let pode = cx.has_global::<Envio>() && janela.ui.pode_executar(&Comando::Reescanear);

    let cabecalho = h_flex()
        .justify_between()
        .gap_3()
        .child(
            div()
                .text_color(tema.muted_foreground)
                .child(tj::encontrados(jogos.len())),
        )
        .child(
            Button::new("procurar-jogos")
                .small()
                .outline()
                .label(tj::BOTAO_PROCURAR)
                .disabled(!pode)
                .when(!pode, |b| b.tooltip(t::SO_COM_SERVICO))
                .on_click(cx.listener(|this, _, _, cx| {
                    enviar(this, Comando::Reescanear, cx);
                })),
        );

    let mut v = v_flex().gap_4().child(cabecalho);
    if jogos.is_empty() {
        return v.into_any_element();
    }
    v = v.child(
        div()
            .text_xs()
            .text_color(tema.muted_foreground)
            .child(tj::DICA_CLIQUE),
    );
    let mut lista = v_flex().rounded_lg().border_1().border_color(tema.border);
    for (i, j) in jogos.iter().enumerate() {
        let l = linha(j, esp);
        let sel = aberto.as_deref() == Some(l.chave.as_str());
        let chave = l.chave.clone();
        let cor_status = match l.status {
            StatusJogo::Otimizado => tema.success,
            StatusJogo::FacaAjustes { .. } => tema.info,
            StatusJogo::AguardandoFechar => tema.warning,
            StatusJogo::NadaAAjustar => tema.muted_foreground,
        };
        let item = h_flex()
            .id(("jogo", i))
            .gap_3()
            .px_3()
            .py_2()
            .cursor_pointer()
            .when(i > 0, |d| d.border_t_1().border_color(tema.border))
            .hover(|d| d.bg(tema.list_hover))
            .on_click(cx.listener(move |_, _, _, cx| {
                let s = cx.default_global::<Selecao>();
                s.jogo = if s.jogo.as_deref() == Some(chave.as_str()) {
                    None
                } else {
                    Some(chave.clone())
                };
                s.tecnicos = false;
                cx.notify();
            }))
            .child(
                div()
                    .w(px(14.))
                    .text_xs()
                    .text_color(tema.muted_foreground)
                    .child(if sel { "▾" } else { "▸" }),
            )
            .child(
                h_flex()
                    .flex_1()
                    .gap_2()
                    .child(div().text_sm().font_semibold().child(l.nome.clone()))
                    .when(l.competitivo, |d| {
                        d.child(
                            Tag::secondary()
                                .small()
                                .outline()
                                .child(t::SELO_COMPETITIVO),
                        )
                    }),
            )
            .child(div().text_sm().text_color(cor_status).child(l.texto_status));
        lista = lista.child(item);
        if sel {
            lista = lista.child(detalhe(janela, i, j, tec, cx));
        }
    }
    v.child(lista).into_any_element()
}

fn enviar(this: &mut Janela, c: Comando, cx: &mut Context<Janela>) {
    let Some(Envio(s)) = cx.try_global::<Envio>() else {
        return;
    };
    if !this.ui.pode_executar(&c) {
        return;
    }
    let s = s.clone();
    this.ui.aplicar(Evento::Ocupado(c.clone()));
    let _ = s.send(c);
    cx.notify();
}

fn detalhe(janela: &Janela, i: usize, j: &Jogo, tec: bool, cx: &mut Context<Janela>) -> Div {
    let tema = cx.theme().clone();
    let titulo = |s: &'static str| {
        div()
            .w(px(180.))
            .flex_shrink_0()
            .text_sm()
            .font_semibold()
            .text_color(tema.muted_foreground)
            .child(s)
    };
    let bloco = |rotulo: &'static str, linhas: Vec<String>, vazio: &'static str| {
        let corpo = if linhas.is_empty() {
            v_flex().child(
                div()
                    .text_sm()
                    .text_color(tema.muted_foreground)
                    .child(vazio),
            )
        } else {
            v_flex()
                .gap_1()
                .children(linhas.into_iter().map(|s| div().text_sm().child(s)))
        };
        h_flex()
            .items_start()
            .gap_3()
            .child(titulo(rotulo))
            .child(corpo.flex_1())
    };
    let mut d = v_flex()
        .gap_3()
        .p_4()
        .border_t_1()
        .border_color(tema.border)
        .bg(tema.background)
        .child(div().font_semibold().child(j.nome.clone()))
        .child(bloco(tj::DETALHE_FEITO, feito(j), tj::DETALHE_NADA_FEITO))
        .child(bloco(tj::DETALHE_FACA, passos(j), tj::DETALHE_NADA_A_FAZER));
    if let Some(a) = arquivo(janela, i, j, cx) {
        d = d.child(
            h_flex()
                .items_start()
                .gap_3()
                .child(titulo(aj::SECAO_ARQUIVO_JOGO))
                .child(a.flex_1()),
        );
    }
    d = d.child(
        h_flex().child(
            Button::new(("tecnicos", i))
                .small()
                .ghost()
                .label(if tec {
                    tj::BOTAO_OCULTAR_TECNICOS
                } else {
                    tj::BOTAO_DETALHES_TECNICOS
                })
                .on_click(cx.listener(|_, _, _, cx| {
                    let s = cx.default_global::<Selecao>();
                    s.tecnicos = !s.tecnicos;
                    cx.notify();
                })),
        ),
    );
    if tec {
        d = d.child(
            v_flex()
                .gap_1()
                .children(tecnicos(j).into_iter().map(|(r, v)| {
                    h_flex()
                        .gap_3()
                        .text_xs()
                        .child(div().w(px(140.)).text_color(tema.muted_foreground).child(r))
                        .child(div().child(v))
                })),
        );
    }
    d
}

/// "Arquivo do jogo": arquivo, antes/depois, backup e Reverter por chave.
fn arquivo(janela: &Janela, i: usize, j: &Jogo, cx: &mut Context<Janela>) -> Option<Div> {
    let c = j.config_jogo.as_ref()?;
    let tema = cx.theme().clone();
    let pode = janela.ui.pode_escrever() && cx.has_global::<Envio>();
    let sub = |s: String| div().text_xs().text_color(tema.muted_foreground).child(s);
    let mut v = v_flex().gap_2();
    for (k, a) in c.arquivos.iter().enumerate() {
        let mut b = v_flex()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(aj::nome_arquivo(&a.rotulo.0).to_owned()),
            )
            .children(
                a.conta
                    .as_ref()
                    .map(|c| sub(format!("{}: {c}", aj::ROTULO_CONTA))),
            )
            .child(sub(format!(
                "{}: {}",
                aj::ROTULO_BACKUP,
                aj::backup(a.backup)
            )))
            .when(a.mudancas.is_empty(), |d| {
                d.child(sub(aj::SEM_MUDANCAS_ARQUIVO.into()))
            });
        for (n, m) in a.mudancas.iter().enumerate() {
            let destino = (m.status == StatusMudanca::Aplicada)
                .then(|| aj::destino_reverter(a, &m.setting))
                .flatten();
            let mut l = h_flex().gap_3().justify_between().child(
                div()
                    .text_sm()
                    .flex_1()
                    .min_w_0()
                    .child(aj::linha_mudanca(m)),
            );
            if let Some(destino) = destino {
                l = l.child(
                    Button::new(("reverter-jogo", i * 10_000 + k * 100 + n))
                        .small()
                        .outline()
                        .label(t::BOTAO_REVERTER)
                        .disabled(!pode)
                        .when(!pode, |b| b.tooltip(t::SO_COM_SERVICO))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            enviar(
                                this,
                                Comando::ReverterConfigJogo {
                                    destino: destino.clone(),
                                },
                                cx,
                            )
                        })),
                );
            }
            b = b.child(l);
        }
        v = v.child(b);
    }
    Some(v)
}

#[cfg(test)]
mod testes {
    use super::{feito, linha, passos, t, tecnicos, tj};
    use nvt_ipc::Jogo;
    use nvt_ipc::{OrigemPerfil, ResumoJogo};

    fn jogo(nome: &str) -> Jogo {
        Jogo {
            nome: nome.into(),
            perfil: format!("perfil-{nome}"),
            origem_perfil: OrigemPerfil::Driver,
            launcher: "Steam".into(),
            api: "dx12".into(),
            dlss: Some("3.7.10".into()),
            reflex: Some(true),
            anticheat_kernel: Some(true),
            competitivo: Some("cs2".into()),
            em_execucao: false,
            orientacoes: vec![],
            resumo: ResumoJogo::default(),
            config_jogo: None,
        }
    }

    #[test]
    fn lista_mostra_so_nome_selo_e_status() {
        let mut j = jogo("Cyberpunk 2077");
        j.orientacoes = vec!["reflex_on_boost".into(), "x".into()];
        let l = linha(&j, false);
        assert_eq!(l.texto_status, "Faça 2 ajustes no jogo");
        assert!(l.competitivo);
        let visivel = format!("{} {}", l.nome, l.texto_status);
        for proibido in ["dx12", "DirectX", "3.7.10", "perfil-", "kernel", "Steam"] {
            assert!(!visivel.contains(proibido), "{proibido} na lista");
        }
    }

    #[test]
    fn status_de_cada_caso() {
        let mut j = jogo("Valorant");
        assert_eq!(linha(&j, false).texto_status, tj::STATUS_NADA);
        j.em_execucao = true;
        assert_eq!(linha(&j, true).texto_status, tj::STATUS_AGUARDANDO);
        j.orientacoes = vec!["a".into()];
        assert_eq!(linha(&j, false).texto_status, "Faça 1 ajuste no jogo");
    }

    #[test]
    fn detalhe_numera_passos_e_tecnicos_ficam_a_parte() {
        let mut j = jogo("CS2");
        j.orientacoes = vec!["a".into(), "b".into()];
        let p = passos(&j);
        assert_eq!(p.len(), 2);
        assert!(p[0].starts_with("1. ") && p[1].starts_with("2. "));
        assert!(feito(&j).is_empty());
        let tec = tecnicos(&j);
        assert!(tec.iter().any(|(_, v)| v == "3.7.10"));
        assert!(tec.iter().any(|(r, _)| *r == t::COL_ANTICHEAT));
    }
}
