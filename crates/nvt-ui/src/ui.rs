//! Janela principal: navegação lateral e as telas.
//!
//! Só lê [`EstadoUi`] e envia [`Comando`]s para a thread de comunicação; a
//! thread da janela nunca bloqueia em E/S.

use std::collections::BTreeSet;
use std::sync::mpsc::Sender;
use std::time::Duration;

use gpui_kit::component::Theme;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants};
use gpui_kit::component::link::Link;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::sidebar::{
    Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem,
};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{
    ActiveTheme, Disableable, IconName, Sizable, StyledExt, WindowExt, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use nvt_ipc::{
    Aviso, EstadoExperimental, ImpactoVisual, Jogo, JogoIgnorado, LIMITE_FPS_MAX, LIMITE_FPS_MIN,
    NivelAviso, Qualidade, Reversao, Sincronismo, StatusMudanca, SugestaoExperimental,
};

use crate::modelo::{
    Comando, Conexao, EstadoUi, Evento, GrupoMudancas, ItemMudanca, Tela, TipoMensagem,
    destino_usuario, formatar_data, linha_setting,
};
use crate::servico::Canal;
use crate::textos as t;

const SINCRONISMOS: [Sincronismo; 3] = [
    Sincronismo::Desligado,
    Sincronismo::GSync,
    Sincronismo::VSync,
];
const QUALIDADES: [Qualidade; 3] = [
    Qualidade::Desempenho,
    Qualidade::Equilibrado,
    Qualidade::Qualidade,
];
const FPS_ATALHOS: [u32; 6] = [60, 120, 141, 144, 165, 240];
const FPS_PADRAO: u32 = 141;

pub struct Janela {
    ui: EstadoUi,
    comandos: Sender<Comando>,
    /// Perfis dos jogos com a linha expandida na tela Jogos.
    expandidos: BTreeSet<String>,
}

impl Janela {
    pub fn new(tela: Tela, expandidos: Vec<String>, canal: Canal, cx: &mut Context<Self>) -> Self {
        let eventos = canal.eventos;
        // Consome os eventos da thread de comunicação sem bloquear a janela.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let lote: Vec<Evento> = eventos.try_iter().collect();
                if lote.is_empty() {
                    continue;
                }
                let vivo = this.update(cx, |j, cx| {
                    for e in lote {
                        j.ui.aplicar(e);
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
            ui: EstadoUi::new(tela),
            comandos: canal.comandos,
            expandidos: expandidos.into_iter().collect(),
        }
    }

    fn enviar(&mut self, c: Comando, cx: &mut Context<Self>) {
        if !self.ui.pode_executar(&c) {
            return;
        }
        self.ui.aplicar(Evento::Ocupado(c.clone()));
        let _ = self.comandos.send(c);
        cx.notify();
    }

    fn ir(&mut self, tela: Tela, cx: &mut Context<Self>) {
        self.ui.tela = tela;
        cx.notify();
    }

    // -----------------------------------------------------------------------
    // Estrutura
    // -----------------------------------------------------------------------

    fn barra_lateral(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let itens = Tela::TODAS.map(|tela| {
            SidebarMenuItem::new(tela.titulo())
                .icon(icone(tela))
                .active(self.ui.tela == tela)
                .on_click(cx.listener(move |this, _, _, cx| this.ir(tela, cx)))
        });
        Sidebar::new("navegacao")
            .header(
                SidebarHeader::new().child(
                    v_flex()
                        .child(div().text_lg().font_semibold().child(t::TITULO_JANELA))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(t::SUBTITULO_APP),
                        ),
                ),
            )
            .child(SidebarGroup::new(t::NAV_GRUPO).child(SidebarMenu::new().children(itens)))
            .footer(SidebarFooter::new().child(self.selo_conexao()))
    }

    fn selo_conexao(&self) -> impl IntoElement {
        match self.ui.conexao {
            Conexao::Conectando => Tag::secondary().child(t::CONEXAO_CONECTANDO),
            Conexao::Servico => Tag::success().child(t::CONEXAO_SERVICO),
            Conexao::Visualizacao => Tag::warning().child(t::CONEXAO_VISUALIZACAO),
        }
    }

    fn avisos(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut v = v_flex().gap_2();
        if self.ui.atualizado {
            v = v.child(
                Alert::info("atualizado-reabra", t::ATUALIZADO_REABRA)
                    .title(t::ATUALIZADO_REABRA_TITULO),
            );
        }
        if let Some(m) = &self.ui.incompativel {
            v = v.child(
                Alert::error("versao-incompativel", m.clone()).title(t::INCOMPATIVEL_TITULO),
            );
        }
        if self.ui.conexao == Conexao::Visualizacao && !self.ui.atualizado {
            v = v.child(
                Alert::warning("aviso-visualizacao", t::AVISO_VISUALIZACAO)
                    .title(t::AVISO_VISUALIZACAO_TITULO),
            );
        }
        if let Some(e) = &self.ui.erro_local {
            v = v.child(Alert::error("erro-local", e.clone()));
        }
        if let Some(faixa) = self.ui.faixa_adiamento() {
            let titulo = t::titulo_adiamento(&faixa);
            v = v.child(Alert::info("faixa-adiamento", faixa).title(titulo));
        }
        if let Some((tipo, texto)) = self.ui.mensagem_visivel() {
            let a = match tipo {
                TipoMensagem::Sucesso => Alert::success("mensagem", texto.clone()),
                TipoMensagem::Adiada => {
                    Alert::info("mensagem", texto.clone()).title(t::titulo_adiamento(texto))
                }
                TipoMensagem::Erro => Alert::error("mensagem", texto.clone()),
            };
            v = v.child(a.on_close(cx.listener(|this, _, _, cx| {
                this.ui.mensagem = None;
                cx.notify();
            })));
        }
        if self.ui.ocupado.is_some() {
            v = v.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t::EM_ANDAMENTO),
            );
        }
        v
    }

    fn conteudo(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.ui.conexao == Conexao::Conectando && self.ui.dados.estado.is_none() {
            return div()
                .text_color(cx.theme().muted_foreground)
                .child(t::CARREGANDO)
                .into_any_element();
        }
        match self.ui.tela {
            Tela::VisaoGeral => self.visao_geral(cx).into_any_element(),
            Tela::Jogos => self.jogos(cx).into_any_element(),
            Tela::Mudancas => self.mudancas(cx).into_any_element(),
            Tela::Windows => self.windows(cx).into_any_element(),
            Tela::Preferencias => self.preferencias(cx).into_any_element(),
            Tela::Historico => self.historico(cx).into_any_element(),
            Tela::Restaurar => self.restaurar(cx).into_any_element(),
        }
    }

    // -----------------------------------------------------------------------
    // Visão geral
    // -----------------------------------------------------------------------

    fn visao_geral(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = cx.theme().clone();
        let estado = self.ui.dados.estado.as_ref();
        let gpu = estado.and_then(|e| e.gpu.as_ref());
        let driver = estado.and_then(|e| e.driver.as_ref());

        let sistema = cartao(
            t::SECAO_SISTEMA,
            v_flex()
                .gap_1()
                .child(linha(
                    t::ROTULO_GPU,
                    gpu.map_or(t::DESCONHECIDO.into(), |g| g.nome.clone()),
                    &tema,
                ))
                .child(linha(
                    t::ROTULO_ARQUITETURA,
                    gpu.map_or(t::DESCONHECIDO.into(), |g| t::arquitetura(&g.arquitetura)),
                    &tema,
                ))
                .child(linha(
                    t::ROTULO_VRAM,
                    gpu.map_or(t::DESCONHECIDO.into(), |g| format!("{} MB", g.vram_mb)),
                    &tema,
                ))
                .child(linha(
                    t::ROTULO_DRIVER,
                    driver.map_or(t::DESCONHECIDO.into(), |d| match &d.ramo {
                        Some(r) if !r.is_empty() => format!("{} ({r})", d.versao),
                        _ => d.versao.clone(),
                    }),
                    &tema,
                ))
                .child(linha(
                    t::ROTULO_JOGOS,
                    estado.map_or("0".into(), |e| e.jogos.to_string()),
                    &tema,
                )),
            &tema,
        );

        let caps = gpu.map(|g| g.capacidades);
        let cap = |nome: &'static str, v: Option<bool>| {
            h_flex()
                .justify_between()
                .text_sm()
                .child(div().text_color(tema.muted_foreground).child(nome))
                .child(
                    match v {
                        Some(true) => Tag::success(),
                        Some(false) => Tag::secondary(),
                        None => Tag::secondary().outline(),
                    }
                    .child(t::sim_nao_desconhecido(v)),
                )
        };
        let capacidades = cartao(
            t::SECAO_CAPACIDADES,
            v_flex()
                .gap_1()
                .child(cap(t::CAP_DLSS, caps.map(|c| c.dlss)))
                .child(cap(t::CAP_FG, caps.map(|c| c.geracao_quadros)))
                .child(cap(t::CAP_REFLEX, caps.map(|c| c.reflex)))
                .child(cap(t::CAP_REBAR, caps.map(|c| c.rebar)))
                .child(linha(
                    t::CAP_REFRESH,
                    t::refresh(caps.and_then(|c| c.refresh_mhz)),
                    &tema,
                ))
                .child(cap(t::CAP_VRR_POSSIVEL, caps.and_then(|c| c.vrr.possivel)))
                .child(cap(t::CAP_VRR_ATIVO, caps.and_then(|c| c.vrr.ativo))),
            &tema,
        );

        let status = match self.ui.conexao {
            Conexao::Conectando => t::CONEXAO_CONECTANDO,
            Conexao::Servico => t::CONEXAO_SERVICO,
            Conexao::Visualizacao => t::CONEXAO_VISUALIZACAO,
        };
        let ultima = estado
            .and_then(|e| e.ultima_aplicacao.as_ref())
            .map_or(t::NUNCA.to_string(), |a| {
                t::ultima_aplicacao(&formatar_data(a.quando_unix), a.mudancas, a.falhas)
            });
        let versao = estado
            .map(|e| e.versao_servico.clone())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "—".into());
        let servico = cartao(
            t::SECAO_SERVICO,
            v_flex()
                .gap_1()
                .child(linha(t::ROTULO_STATUS, status.into(), &tema))
                .child(linha(t::ROTULO_VERSAO, versao, &tema))
                .child(linha(t::ROTULO_ULTIMA_APLICACAO, ultima, &tema)),
            &tema,
        );

        let linhas_atualizacao = self.ui.linhas_atualizacao(agora_unix());
        let atualizacao = (!linhas_atualizacao.is_empty()).then(|| {
            cartao(
                t::SECAO_ATUALIZACAO,
                v_flex().gap_1().children(
                    linhas_atualizacao
                        .into_iter()
                        .map(|l| div().text_sm().child(l)),
                ),
                &tema,
            )
        });
        let aviso_atualizado = self.ui.aviso_atualizado().map(|versao| {
            h_flex()
                .gap_2()
                .items_center()
                .child(div().flex_1().child(Alert::success(
                    "atualizado-para",
                    t::atualizado_para(versao),
                )))
                .child(
                    Button::new("atualizado-ok")
                        .primary()
                        .label(t::BOTAO_OK)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(c) = this.ui.dispensar_aviso_atualizado() {
                                this.enviar(c, cx);
                            }
                            cx.notify();
                        })),
                )
        });

        let c = self.ui.conformidade();
        let numero = |n: usize, rotulo: &'static str, cor: Hsla| {
            v_flex()
                .items_center()
                .min_w(px(80.))
                .child(
                    div()
                        .text_2xl()
                        .font_semibold()
                        .text_color(cor)
                        .child(n.to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(tema.muted_foreground)
                        .child(rotulo),
                )
        };
        let conformidade = cartao(
            t::SECAO_CONFORMIDADE,
            v_flex()
                .gap_3()
                .child(
                    h_flex()
                        .gap_4()
                        .child(numero(c.pendentes, t::CONF_PENDENTES, tema.warning))
                        .child(numero(c.conformes, t::CONF_CONFORMES, tema.success))
                        .child(numero(c.travadas, t::CONF_TRAVADAS, tema.info))
                        .child(numero(c.falhas, t::CONF_FALHAS, tema.danger)),
                )
                .when(c.conforme(), |d| {
                    d.child(
                        div()
                            .text_sm()
                            .text_color(tema.success)
                            .child(t::CONF_TUDO_OK),
                    )
                }),
            &tema,
        );

        let alertas = self.ui.alertas();
        let deriva = cartao(
            t::SECAO_DERIVA,
            if alertas.is_empty() {
                v_flex().child(
                    div()
                        .text_sm()
                        .text_color(tema.muted_foreground)
                        .child(t::SEM_DERIVA),
                )
            } else {
                v_flex().gap_1().children(alertas.iter().map(|a| {
                    div().text_sm().text_color(tema.warning).child(t::deriva(
                        &a.nome_legivel,
                        &a.perfil,
                        &a.esperado,
                        a.encontrado.as_deref(),
                    ))
                }))
            },
            &tema,
        );

        let lista_avisos = self.ui.avisos();
        let avisos = cartao(
            t::SECAO_AVISOS,
            if lista_avisos.is_empty() {
                v_flex().child(
                    div()
                        .text_sm()
                        .text_color(tema.muted_foreground)
                        .child(t::SEM_AVISOS),
                )
            } else {
                v_flex()
                    .gap_2()
                    .children(lista_avisos.iter().map(|a| aviso(a, &tema)))
            },
            &tema,
        );

        let pode_escrever = self.ui.pode_escrever();
        let ocioso = self.ui.ocupado.is_none();
        v_flex()
            .gap_4()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("aplicar")
                            .primary()
                            .label(t::BOTAO_APLICAR)
                            .disabled(!pode_escrever)
                            .when(!pode_escrever, |b| b.tooltip(t::SO_COM_SERVICO))
                            .on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.enviar(Comando::AplicarAgora, cx)
                                }),
                            ),
                    )
                    .child(
                        Button::new("reescanear")
                            .outline()
                            .label(t::BOTAO_REESCANEAR)
                            .disabled(!ocioso)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.enviar(Comando::Reescanear, cx)),
                            ),
                    )
                    .child(
                        Button::new("atualizar")
                            .ghost()
                            .label(t::BOTAO_ATUALIZAR)
                            .disabled(!ocioso)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.enviar(Comando::Atualizar, cx)),
                            ),
                    ),
            )
            .child(avisos)
            .children(aviso_atualizado)
            .child(
                h_flex()
                    .gap_4()
                    .items_start()
                    .child(div().flex_1().min_w_0().child(sistema))
                    .child(div().flex_1().min_w_0().child(capacidades)),
            )
            .child(
                h_flex()
                    .gap_4()
                    .items_start()
                    .child(div().flex_1().min_w_0().child(servico))
                    .child(div().flex_1().min_w_0().child(conformidade)),
            )
            .children(atualizacao)
            .child(deriva)
    }

    // -----------------------------------------------------------------------
    // Jogos
    // -----------------------------------------------------------------------

    fn jogos(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = cx.theme().clone();
        let ignorados = &self.ui.dados.ignorados;
        v_flex()
            .gap_4()
            .child(self.tabela_jogos(cx))
            .when(!ignorados.is_empty(), |v| {
                v.child(cartao_ignorados(ignorados, &tema))
            })
    }

    fn tabela_jogos(&self, cx: &mut Context<Self>) -> Div {
        let tema = cx.theme().clone();
        let jogos = self.ui.jogos();
        if jogos.is_empty() {
            return v_flex().child(div().text_color(tema.muted_foreground).child(t::SEM_JOGOS));
        }
        let cab = |s: &'static str, w: f32| {
            div()
                .w(px(w))
                .flex_shrink_0()
                .text_xs()
                .font_semibold()
                .text_color(tema.muted_foreground)
                .child(s)
        };
        let cabecalho = h_flex()
            .gap_3()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(tema.border)
            .child(div().w(px(14.)).flex_shrink_0())
            .child(
                div()
                    .flex_1()
                    .min_w(px(160.))
                    .text_xs()
                    .font_semibold()
                    .text_color(tema.muted_foreground)
                    .child(t::COL_JOGO),
            )
            .child(cab(t::COL_LAUNCHER, 80.))
            .child(cab(t::COL_API, 84.))
            .child(cab(t::COL_DLSS, 56.))
            .child(cab(t::COL_REFLEX, 48.))
            .child(cab(t::COL_ANTICHEAT, 64.))
            .child(cab(t::COL_APLICADO, 300.));
        let linhas = jogos.into_iter().enumerate().map(|(i, j)| {
            let cel = |w: f32| div().w(px(w)).flex_shrink_0().text_sm();
            let aberto = self.expandidos.contains(&j.perfil) || self.expandidos.contains(&j.nome);
            let (perfil, nome) = (j.perfil.clone(), j.nome.clone());
            let herda = t::herda_globais(j.resumo.herda_do_global.len());
            let faca = t::faca_no_jogo_resumo(j.orientacoes.len());
            let linha =
                h_flex()
                    .id(("jogo", i))
                    .gap_3()
                    .px_3()
                    .py_2()
                    .items_start()
                    .cursor_pointer()
                    .hover(|d| d.bg(tema.list_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if aberto {
                            this.expandidos.remove(&perfil);
                            this.expandidos.remove(&nome);
                        } else {
                            this.expandidos.insert(perfil.clone());
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(14.))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(tema.muted_foreground)
                            .child(if aberto { "▾" } else { "▸" }),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(160.))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .flex_wrap()
                                    .child(div().text_sm().font_semibold().child(j.nome.clone()))
                                    .when(j.competitivo.is_some(), |d| {
                                        d.child(Tag::info().small().child(t::SELO_COMPETITIVO))
                                    })
                                    .when(j.em_execucao, |d| {
                                        d.child(
                                            Tag::secondary()
                                                .small()
                                                .outline()
                                                .child(t::SELO_ABERTO),
                                        )
                                    }),
                            )
                            .child(div().text_xs().text_color(tema.muted_foreground).child(
                                format!("“{}” · {}", j.perfil, t::origem_perfil(j.origem_perfil)),
                            )),
                    )
                    .child(cel(80.).child(if j.launcher.is_empty() {
                        "?".to_string()
                    } else {
                        j.launcher.clone()
                    }))
                    .child(cel(84.).child(t::api_grafica(&j.api)))
                    .child(cel(56.).child(match &j.dlss {
                        None => t::NAO.to_string(),
                        Some(v) if v.is_empty() => t::SIM.to_string(),
                        Some(v) => v.clone(),
                    }))
                    .child(cel(48.).child(t::talvez(j.reflex)))
                    .child(cel(64.).child(match j.anticheat_kernel {
                        Some(true) => t::ANTICHEAT_KERNEL,
                        _ => "?",
                    }))
                    .child(
                        v_flex()
                            .w(px(300.))
                            .flex_shrink_0()
                            .text_xs()
                            .child(
                                div()
                                    .when(j.resumo.aplicadas.is_empty(), |d| {
                                        d.text_color(tema.muted_foreground)
                                    })
                                    .child(t::resumo_curto(&j.resumo)),
                            )
                            .when(!herda.is_empty(), |d| {
                                d.child(div().text_color(tema.muted_foreground).child(herda))
                            })
                            .when(!faca.is_empty(), |d| {
                                d.child(div().text_color(tema.info).child(faca))
                            }),
                    );
            v_flex()
                .when(i % 2 == 1, |d| d.bg(tema.table_even))
                .child(linha)
                .when(aberto, |d| d.child(self.detalhe_jogo(&j, &tema)))
        });
        v_flex()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(tema.muted_foreground)
                    .child(t::DETALHE_EXPANDIR),
            )
            .child(
                v_flex()
                    .rounded_lg()
                    .border_1()
                    .border_color(tema.border)
                    .child(cabecalho)
                    .children(linhas),
            )
    }

    /// Lista completa de um jogo: regras aplicadas (com as settings),
    /// não aplicadas (com o motivo) e o que herda do global.
    fn detalhe_jogo(&self, j: &Jogo, tema: &Theme) -> Div {
        let titulo = |s: &'static str| {
            div()
                .text_xs()
                .font_semibold()
                .text_color(tema.muted_foreground)
                .child(s)
        };
        let nenhuma = || {
            div()
                .text_sm()
                .text_color(tema.muted_foreground)
                .child(t::DETALHE_NENHUMA)
        };
        let r = &j.resumo;
        let aplicadas = v_flex()
            .gap_1()
            .child(titulo(t::DETALHE_APLICADAS))
            .when(r.aplicadas.is_empty(), |d| d.child(nenhuma()))
            .children(r.aplicadas.iter().map(|a| {
                v_flex()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(tema.success)
                                    .child(t::nome_regra(&a.regra, &a.descricao)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(tema.muted_foreground)
                                    .child(a.regra.clone()),
                            ),
                    )
                    .children(a.settings.iter().map(|s| {
                        div().text_xs().pl_3().child(format!(
                            "• {}",
                            linha_setting(&self.ui.dados.grupos, &j.perfil, s)
                        ))
                    }))
            }));
        let nao_aplicadas = v_flex()
            .gap_1()
            .child(titulo(t::DETALHE_NAO_APLICADAS))
            .when(r.nao_aplicadas.is_empty(), |d| d.child(nenhuma()))
            .children(r.nao_aplicadas.iter().map(|n| {
                v_flex()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .child(t::nome_regra(&n.regra, &n.descricao)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(tema.muted_foreground)
                                    .child(n.regra.clone()),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .pl_3()
                            .text_color(tema.muted_foreground)
                            .child(t::motivo_longo(&n.motivo)),
                    )
            }));
        let herda = v_flex().gap_1().child(titulo(t::DETALHE_HERDA)).child(
            if r.herda_do_global.is_empty() {
                nenhuma()
            } else {
                div().text_xs().child(format!(
                    "{}: {}",
                    t::herda_globais_lista(r.herda_do_global.len()),
                    r.herda_do_global
                        .iter()
                        .map(|s| t::setting_legivel(s))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            },
        );
        let faca_no_jogo = (!j.orientacoes.is_empty()).then(|| {
            v_flex()
                .gap_1()
                .child(titulo(t::DETALHE_FACA_NO_JOGO))
                .child(
                    div()
                        .text_xs()
                        .text_color(tema.muted_foreground)
                        .child(t::FACA_NO_JOGO_EXPLICACAO),
                )
                .children(j.orientacoes.iter().map(|o| {
                    div()
                        .text_sm()
                        .pl_3()
                        .child(format!("• {}", t::orientacao_ou_codigo(o)))
                }))
        });
        v_flex()
            .gap_3()
            .ml(px(26.))
            .mr_3()
            .mb_3()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(tema.border)
            .bg(tema.background)
            .child(aplicadas)
            .child(nao_aplicadas)
            .child(herda)
            .children(faca_no_jogo)
    }

    // -----------------------------------------------------------------------
    // Mudanças
    // -----------------------------------------------------------------------

    fn mudancas(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = cx.theme().clone();
        let mut v = v_flex().gap_4();
        if self.ui.dados.grupos.is_empty() && self.ui.dados.sistema.mudancas.is_empty() {
            v = v.child(
                div()
                    .text_color(tema.muted_foreground)
                    .child(t::SEM_MUDANCAS),
            );
        }
        for g in &self.ui.dados.grupos {
            v = v.child(grupo(g, &tema));
        }
        if !self.ui.dados.sistema.mudancas.is_empty() {
            let ajustes = self
                .ui
                .dados
                .sistema
                .mudancas
                .iter()
                .map(|m| item(&ItemMudanca::from(m), &tema));
            v = v.child(cartao(t::TELA_WINDOWS, v_flex().children(ajustes), &tema));
        }
        if !self.ui.dados.sugestoes_driver.is_empty() {
            let corpo = v_flex()
                .gap_2()
                .child(explicacao(t::EXPERIMENTAIS_DRIVER, &tema))
                .when(
                    self.ui.conexao == Conexao::Servico && !sessao_console_ativa(),
                    |v| v.child(explicacao(t::ACEITE_DRIVER_INATIVO, &tema)),
                )
                .child(self.experimentais(&self.ui.dados.sugestoes_driver, &tema, cx));
            v = v.child(cartao(t::SECAO_EXPERIMENTAIS, corpo, &tema));
        }
        let pode = self.ui.pode_escrever();
        let respeitadas = &self.ui.dados.respeitadas;
        let corpo = if respeitadas.is_empty() {
            v_flex().child(
                div()
                    .text_sm()
                    .text_color(tema.muted_foreground)
                    .child(t::SEM_RESPEITADAS),
            )
        } else {
            v_flex()
                .gap_2()
                .children(respeitadas.iter().enumerate().map(|(i, r)| {
                    let (perfil, setting) = (r.perfil.clone(), r.setting.clone());
                    h_flex()
                        .gap_3()
                        .justify_between()
                        .child(
                            v_flex()
                                .child(div().text_sm().font_semibold().child(format!(
                                    "{} · {} · {}",
                                    t::CAMADA_DRIVER,
                                    r.nome_perfil,
                                    r.nome_legivel
                                )))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(tema.muted_foreground)
                                        .child(r.motivo.clone()),
                                ),
                        )
                        .child(
                            Button::new(("destravar", i))
                                .small()
                                .outline()
                                .label(t::BOTAO_DESTRAVAR)
                                .disabled(!pode)
                                .when(!pode, |b| b.tooltip(t::SO_COM_SERVICO))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.enviar(
                                        Comando::Destravar {
                                            perfil: perfil.clone(),
                                            setting: setting.clone(),
                                        },
                                        cx,
                                    )
                                })),
                        )
                }))
        };
        let reversoes = &self.ui.dados.reversoes;
        let corpo_rev = if reversoes.is_empty() {
            v_flex().child(
                div()
                    .text_sm()
                    .text_color(tema.muted_foreground)
                    .child(t::SEM_REVERSOES),
            )
        } else {
            v_flex()
                .gap_2()
                .children(reversoes.iter().map(|r| reversao(r, &tema)))
        };
        let ignorados = &self.ui.dados.ignorados;
        v.child(cartao(t::SECAO_REVERSOES, corpo_rev, &tema))
            .child(cartao(t::SECAO_RESPEITADAS, corpo, &tema))
            .when(!ignorados.is_empty(), |v| {
                v.child(cartao_ignorados(ignorados, &tema))
            })
    }

    // -----------------------------------------------------------------------
    // Windows
    // -----------------------------------------------------------------------

    fn windows(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = cx.theme().clone();
        let mut ajustes = v_flex().gap_2();
        for (i, mudanca) in self.ui.dados.sistema.mudancas.iter().enumerate() {
            let destino = destino_usuario(&mudanca.setting, mudanca.alvo_tela.as_deref());
            let pode = self.ui.pode_escrever() && destino.is_some();
            ajustes = ajustes.child(
                h_flex()
                    .gap_3()
                    .justify_between()
                    .child(item(&ItemMudanca::from(mudanca), &tema).flex_1().min_w_0())
                    .child(
                        Button::new(("reverter-usuario", i))
                            .small()
                            .outline()
                            .label(t::BOTAO_REVERTER)
                            .disabled(!pode)
                            .when(destino.is_none(), |b| {
                                b.tooltip(t::DESTINO_REVERTER_INDISPONIVEL)
                            })
                            .when_some(destino, |b, destino| {
                                b.on_click(cx.listener(move |this, _, _, cx| {
                                    this.enviar(
                                        Comando::ReverterAjusteUsuario {
                                            destino: destino.clone(),
                                        },
                                        cx,
                                    )
                                }))
                            }),
                    ),
            );
        }
        if self.ui.dados.sistema.mudancas.is_empty() {
            ajustes = ajustes.child(explicacao(t::SEM_AJUSTES_USUARIO, &tema));
        }

        let diagnosticos = self
            .ui
            .dados
            .estado
            .as_ref()
            .map_or(&[][..], |e| e.diagnosticos.as_slice());
        let diagnostico = v_flex()
            .gap_3()
            .when(diagnosticos.is_empty(), |v| {
                v.child(explicacao(t::SEM_DIAGNOSTICOS, &tema))
            })
            .children(diagnosticos.iter().enumerate().map(|(i, d)| {
                let info = t::diagnostico(&d.codigo);
                v_flex()
                    .gap_1()
                    .child(
                        div().text_sm().font_semibold().child(
                            info.map_or(d.codigo.as_str(), |(titulo, _, _)| titulo)
                                .to_owned(),
                        ),
                    )
                    .child(div().text_sm().child(t::estado_diagnostico(d)))
                    .when_some(info, |v, (_, orientacao, url)| {
                        v.when(t::mostrar_orientacao_diagnostico(d), |v| {
                            v.child(explicacao(orientacao, &tema))
                        })
                        .child(
                            Link::new(("diagnostico-fonte", i))
                                .href(url)
                                .child(t::LINK_OFICIAL),
                        )
                    })
            }));

        v_flex()
            .gap_4()
            .child(cartao(t::SECAO_WINDOWS_FEITO, ajustes, &tema))
            .child(cartao(
                t::SECAO_EXPERIMENTAIS,
                self.experimentais(&self.ui.dados.sistema.sugestoes, &tema, cx),
                &tema,
            ))
            .child(cartao(t::SECAO_DIAGNOSTICO, diagnostico, &tema))
    }

    fn experimentais(
        &self,
        sugestoes: &[SugestaoExperimental],
        tema: &Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let pode = self.ui.pode_escrever();
        v_flex()
            .gap_2()
            .when(sugestoes.is_empty(), |v| {
                v.child(explicacao(t::SEM_EXPERIMENTAIS, tema))
            })
            .children(sugestoes.iter().enumerate().filter_map(|(i, s)| {
                let regra = s.regra.clone();
                let (rotulo, aceito) = match s.estado {
                    EstadoExperimental::Aceitavel => (t::BOTAO_ACEITAR, true),
                    EstadoExperimental::Aceita => (t::BOTAO_RETIRAR, false),
                    EstadoExperimental::Estavel => return None,
                };
                Some(
                    h_flex()
                        .gap_3()
                        .justify_between()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_1()
                                .child(div().text_sm().font_semibold().child(s.descricao.clone()))
                                .child(div().text_xs().text_color(tema.muted_foreground).child(
                                    format!(
                                        "{} · {} · {}",
                                        t::camada(s.camada),
                                        t::experimental(s.estado),
                                        s.regra
                                    ),
                                )),
                        )
                        .child(
                            Button::new(("aceite-experimental", i))
                                .small()
                                .outline()
                                .label(rotulo)
                                .disabled(!pode)
                                .when(!pode, |b| b.tooltip(t::SO_COM_SERVICO))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.enviar(
                                        Comando::DefinirAceiteExperimental {
                                            regra: regra.clone(),
                                            aceito,
                                        },
                                        cx,
                                    )
                                })),
                        ),
                )
            }))
    }

    // -----------------------------------------------------------------------
    // Preferências
    // -----------------------------------------------------------------------

    fn preferencias(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = cx.theme().clone();
        let r = &self.ui.rascunho;
        let i_sync = SINCRONISMOS.iter().position(|s| *s == r.sincronismo);
        let i_qual = QUALIDADES.iter().position(|q| *q == r.qualidade);

        let sincronismo = cartao(
            t::SECAO_SINCRONISMO,
            v_flex()
                .gap_3()
                .child(
                    RadioGroup::horizontal("sincronismo")
                        .selected_index(i_sync)
                        .children(SINCRONISMOS.map(|s| {
                            Radio::new(t::nome_sincronismo(s)).label(t::nome_sincronismo(s))
                        }))
                        .on_click(cx.listener(|this, i: &usize, _, cx| {
                            this.ui.rascunho.sincronismo = SINCRONISMOS[*i];
                            cx.notify();
                        })),
                )
                .child(explicacao(t::explicacao_sincronismo(r.sincronismo), &tema))
                .when_some(self.ui.aviso_sincronismo(), |v, texto| {
                    v.child(Alert::warning("aviso-sincronismo", texto))
                }),
            &tema,
        );
        let qualidade =
            cartao(
                t::SECAO_QUALIDADE,
                v_flex()
                    .gap_3()
                    .child(
                        RadioGroup::horizontal("qualidade")
                            .selected_index(i_qual)
                            .children(QUALIDADES.map(|q| {
                                Radio::new(t::nome_qualidade(q)).label(t::nome_qualidade(q))
                            }))
                            .on_click(cx.listener(|this, i: &usize, _, cx| {
                                this.ui.rascunho.qualidade = QUALIDADES[*i];
                                cx.notify();
                            })),
                    )
                    .children(QUALIDADES.map(|q| {
                        h_flex()
                            .gap_2()
                            .text_sm()
                            .child(
                                div()
                                    .w(px(96.))
                                    .font_semibold()
                                    .when(q == r.qualidade, |d| d.text_color(tema.primary))
                                    .child(t::nome_qualidade(q)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_color(tema.muted_foreground)
                                    .child(t::explicacao_qualidade(q)),
                            )
                    })),
                &tema,
            );

        let limite = r.limite_fps;
        let passo = |id: &'static str, rotulo: &'static str, delta: i32| {
            Button::new(id)
                .small()
                .outline()
                .label(rotulo)
                .disabled(limite.is_none())
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(v) = this.ui.rascunho.limite_fps {
                        let novo =
                            (v as i32 + delta).clamp(LIMITE_FPS_MIN as i32, LIMITE_FPS_MAX as i32);
                        this.ui.rascunho.limite_fps = Some(novo as u32);
                        cx.notify();
                    }
                }))
        };
        let limite_fps = cartao(
            t::SECAO_LIMITE_FPS,
            v_flex()
                .gap_3()
                .child(
                    Switch::new("limitar-fps")
                        .checked(limite.is_some())
                        .label(t::LIMITE_FPS_LIGADO)
                        .on_click(cx.listener(|this, ligado: &bool, _, cx| {
                            this.ui.rascunho.limite_fps = ligado.then_some(FPS_PADRAO);
                            cx.notify();
                        })),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(passo("fps-10", "−10", -10))
                        .child(passo("fps-1", "−1", -1))
                        .child(
                            div()
                                .w(px(88.))
                                .text_center()
                                .font_semibold()
                                .child(limite.map_or("—".into(), |v| format!("{v} FPS"))),
                        )
                        .child(passo("fps+1", "+1", 1))
                        .child(passo("fps+10", "+10", 10))
                        .child(div().w(px(16.)))
                        .children(FPS_ATALHOS.map(|v| {
                            Button::new(("fps-atalho", v as usize))
                                .small()
                                .ghost()
                                .label(v.to_string())
                                .disabled(limite.is_none())
                                .toggled(limite == Some(v))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.ui.rascunho.limite_fps = Some(v);
                                    cx.notify();
                                }))
                        })),
                )
                .child(explicacao(t::explicacao_limite_fps(), &tema)),
            &tema,
        );

        let alterado = self.ui.rascunho_alterado();
        let pode = self.ui.pode_escrever();
        let rascunho = self.ui.rascunho.clone();
        v_flex()
            .gap_4()
            .child(sincronismo)
            .child(qualidade)
            .child(limite_fps)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("salvar-prefs")
                            .primary()
                            .label(t::BOTAO_SALVAR_PREFS)
                            .disabled(!(pode && alterado))
                            .when(!pode, |b| b.tooltip(t::SO_COM_SERVICO))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.enviar(Comando::DefinirPreferencias(rascunho.clone()), cx)
                            })),
                    )
                    .child(
                        Button::new("descartar-prefs")
                            .ghost()
                            .label(t::BOTAO_DESCARTAR)
                            .disabled(!alterado)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.ui.rascunho = this.ui.preferencias_atuais();
                                cx.notify();
                            })),
                    )
                    .when(alterado, |d| {
                        d.child(
                            div()
                                .text_sm()
                                .text_color(tema.warning)
                                .child(t::PREFS_NAO_SALVAS),
                        )
                    })
                    .when(!pode, |d| {
                        d.child(
                            div()
                                .text_sm()
                                .text_color(tema.muted_foreground)
                                .child(t::SO_COM_SERVICO),
                        )
                    }),
            )
    }

    // -----------------------------------------------------------------------
    // Histórico
    // -----------------------------------------------------------------------

    fn historico(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = cx.theme().clone();
        let h = &self.ui.dados.historico;
        if h.is_empty() {
            return v_flex().child(
                div()
                    .text_color(tema.muted_foreground)
                    .child(t::SEM_HISTORICO),
            );
        }
        v_flex()
            .rounded_lg()
            .border_1()
            .border_color(tema.border)
            .children(h.iter().enumerate().map(|(i, e)| {
                let alvo = match (&e.perfil, &e.setting) {
                    (Some(p), Some(s)) => format!("{p} · {s}"),
                    (Some(p), None) => p.clone(),
                    (None, Some(s)) => s.clone(),
                    (None, None) => String::new(),
                };
                let valores = match (&e.antes, &e.depois) {
                    (None, None) => String::new(),
                    (a, d) => format!(
                        "{} → {}",
                        a.as_deref().unwrap_or(t::NAO_DEFINIDO),
                        d.as_deref().unwrap_or(t::NAO_DEFINIDO)
                    ),
                };
                h_flex()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .items_start()
                    .when(i % 2 == 1, |d| d.bg(tema.table_even))
                    .child(
                        div()
                            .w(px(120.))
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(tema.muted_foreground)
                            .child(if e.quando_unix == 0 {
                                "—".to_string()
                            } else {
                                formatar_data(e.quando_unix)
                            }),
                    )
                    .child(
                        div().w(px(124.)).flex_shrink_0().child(
                            v_flex()
                                .gap_1()
                                .items_start()
                                .child(Tag::secondary().small().child(t::camada(e.camada)))
                                .child(Tag::secondary().small().child(t::acao_historico(e.acao))),
                        ),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .child(div().text_sm().font_semibold().child(alvo))
                            .when(!valores.is_empty(), |d| {
                                d.child(div().text_sm().child(valores))
                            })
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(tema.muted_foreground)
                                    .child(e.descricao.clone()),
                            ),
                    )
            }))
    }

    // -----------------------------------------------------------------------
    // Restaurar
    // -----------------------------------------------------------------------

    fn restaurar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = cx.theme().clone();
        let pode = self.ui.pode_restaurar();
        let bloqueio = self.ui.restauracao_bloqueada().cloned();
        cartao(
            t::TELA_RESTAURAR,
            v_flex()
                .gap_4()
                .child(div().text_sm().child(t::RESTAURAR_EXPLICACAO))
                .when_some(bloqueio.clone(), |v, a| {
                    v.child(
                        Alert::error("restauracao-bloqueada", a.mensagem.clone())
                            .title(t::RESTAURAR_BLOQUEADO),
                    )
                    .when_some(a.detalhe, |v, d| {
                        v.child(div().text_xs().text_color(tema.muted_foreground).child(d))
                    })
                })
                .child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .child(
                            Button::new("restaurar")
                                .danger()
                                .label(t::BOTAO_RESTAURAR)
                                .disabled(!pode)
                                .on_click(cx.listener(|_, _, window, cx| {
                                    let janela = cx.entity().downgrade();
                                    window.open_alert_dialog(cx, move |alerta, _, _| {
                                        let janela = janela.clone();
                                        alerta
                                            .title(t::CONFIRMAR_TITULO)
                                            .description(t::CONFIRMAR_TEXTO)
                                            .show_cancel(true)
                                            .ok_text(t::CONFIRMAR_OK)
                                            .ok_variant(ButtonVariant::Danger)
                                            .cancel_text(t::CONFIRMAR_CANCELAR)
                                            .on_ok(move |_, _, cx| {
                                                let _ = janela.update(cx, |j, cx| {
                                                    j.enviar(Comando::RestaurarOriginal, cx)
                                                });
                                                true
                                            })
                                    });
                                })),
                        )
                        .when(!pode, |d| {
                            let texto = if bloqueio.is_some() {
                                t::RESTAURAR_BLOQUEADO
                            } else {
                                t::SO_COM_SERVICO
                            };
                            d.child(
                                div()
                                    .text_sm()
                                    .text_color(tema.muted_foreground)
                                    .child(texto),
                            )
                        }),
                ),
            &tema,
        )
    }
}

impl Render for Janela {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = cx.theme().clone();
        h_flex()
            .size_full()
            .bg(tema.background)
            .text_color(tema.foreground)
            .child(self.barra_lateral(cx))
            .child(
                v_flex()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .child(
                        h_flex()
                            .px_6()
                            .py_4()
                            .border_b_1()
                            .border_color(tema.border)
                            .justify_between()
                            .child(div().text_xl().font_semibold().child(self.ui.tela.titulo()))
                            .child(
                                v_flex()
                                    .items_end()
                                    .gap_1()
                                    .child(self.selo_conexao())
                                    .child(
                                        div().text_xs().text_color(tema.muted_foreground).child(
                                            self.ui
                                                .dados
                                                .estado
                                                .as_ref()
                                                .filter(|_| self.ui.conexao == Conexao::Servico)
                                                .map_or(t::AGENTE_INDISPONIVEL, |e| {
                                                    if e.agente.presente {
                                                        t::AGENTE_ATIVO
                                                    } else {
                                                        t::AGENTE_AUSENTE
                                                    }
                                                }),
                                        ),
                                    )
                                    .when(self.ui.restauracao_usuario_pendente(), |v| {
                                        v.child(explicacao(t::RESTAURACAO_USUARIO_PENDENTE, &tema))
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id("conteudo")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(
                                v_flex()
                                    .p_6()
                                    .gap_4()
                                    .child(self.avisos(cx))
                                    .child(self.conteudo(cx)),
                            ),
                    ),
            )
    }
}

// ---------------------------------------------------------------------------
// Peças
// ---------------------------------------------------------------------------

fn sessao_console_ativa() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::RemoteDesktop::{
            ProcessIdToSessionId, WTSGetActiveConsoleSessionId,
        };
        let mut sessao = 0;
        // SAFETY: a saída aponta para um u32 gravável e o PID é o deste processo.
        unsafe {
            ProcessIdToSessionId(std::process::id(), &mut sessao) != 0
                && sessao == WTSGetActiveConsoleSessionId()
        }
    }
    #[cfg(not(windows))]
    false
}

fn icone(tela: Tela) -> IconName {
    match tela {
        Tela::VisaoGeral => IconName::LayoutDashboard,
        Tela::Jogos => IconName::GalleryVerticalEnd,
        Tela::Mudancas => IconName::Replace,
        Tela::Windows => IconName::Settings2,
        Tela::Preferencias => IconName::Settings2,
        Tela::Historico => IconName::Calendar,
        Tela::Restaurar => IconName::Undo2,
    }
}

fn cartao(titulo: &'static str, corpo: impl IntoElement, tema: &Theme) -> Div {
    v_flex()
        .gap_3()
        .p_4()
        .rounded_lg()
        .border_1()
        .border_color(tema.border)
        .bg(tema.group_box)
        .child(div().text_sm().font_semibold().child(titulo))
        .child(corpo)
}

fn cor_nivel(n: NivelAviso, tema: &Theme) -> Hsla {
    match n {
        NivelAviso::Info => tema.info,
        NivelAviso::Atencao => tema.warning,
        NivelAviso::Erro => tema.danger,
    }
}

fn aviso(a: &Aviso, tema: &Theme) -> Div {
    let cor = cor_nivel(a.nivel, tema);
    let tag = match a.nivel {
        NivelAviso::Info => Tag::info(),
        NivelAviso::Atencao => Tag::warning(),
        NivelAviso::Erro => Tag::danger(),
    };
    v_flex()
        .gap_1()
        .pl_3()
        .border_l_2()
        .border_color(cor)
        .child(
            h_flex()
                .gap_2()
                .child(tag.small().child(t::nivel_aviso(a.nivel)))
                .child(
                    div()
                        .text_xs()
                        .text_color(tema.muted_foreground)
                        .child(formatar_data(a.quando_unix)),
                ),
        )
        .child(
            div()
                .text_sm()
                .text_color(cor)
                .child(t::mensagem_aviso(a).to_owned()),
        )
        .when_some(a.detalhe.clone(), |d, detalhe| {
            d.child(
                div()
                    .text_xs()
                    .text_color(tema.muted_foreground)
                    .child(detalhe),
            )
        })
}

fn reversao(r: &Reversao, tema: &Theme) -> Div {
    let tag = match r.status {
        StatusMudanca::Aplicada => Tag::success(),
        StatusMudanca::Falhou | StatusMudanca::Recusada => Tag::danger(),
        _ => Tag::warning(),
    };
    v_flex()
        .gap_1()
        .py_2()
        .border_t_1()
        .border_color(tema.border)
        .child(
            h_flex()
                .gap_2()
                .child(div().text_sm().font_semibold().child(format!(
                    "{} · {} · {}",
                    t::CAMADA_DRIVER,
                    r.nome_perfil,
                    r.nome_legivel
                )))
                .child(tag.small().child(t::status_reversao(r.status))),
        )
        .child(div().text_sm().child(format!(
            "{} → {}",
            r.atual.as_deref().unwrap_or(t::NAO_DEFINIDO),
            t::acao_reversao(r.acao, r.original.as_deref())
        )))
        .child(
            div()
                .text_sm()
                .text_color(tema.muted_foreground)
                .child(r.motivo.clone()),
        )
}

fn cartao_ignorados(ignorados: &[JogoIgnorado], tema: &Theme) -> Div {
    cartao(
        t::SECAO_IGNORADOS,
        v_flex()
            .gap_2()
            .child(explicacao(t::IGNORADOS_EXPLICACAO, tema))
            .children(ignorados.iter().map(|j| {
                v_flex()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(tema.warning)
                            .child(j.nome.clone()),
                    )
                    .child(div().text_xs().text_color(tema.muted_foreground).child(
                        if j.exe.is_empty() {
                            j.motivo.clone()
                        } else {
                            format!("{} · {}", j.exe, j.motivo)
                        },
                    ))
            })),
        tema,
    )
}

fn linha(rotulo: &'static str, valor: String, tema: &Theme) -> Div {
    h_flex()
        .gap_4()
        .justify_between()
        .text_sm()
        .child(div().text_color(tema.muted_foreground).child(rotulo))
        .child(div().text_right().child(valor))
}

fn explicacao(texto: &'static str, tema: &Theme) -> Div {
    div()
        .text_sm()
        .text_color(tema.muted_foreground)
        .child(texto)
}

fn tag_status(s: StatusMudanca) -> Tag {
    let tag = match s {
        StatusMudanca::Pendente => Tag::warning(),
        StatusMudanca::Aplicada | StatusMudanca::JaNoValor => Tag::success(),
        StatusMudanca::Travada => Tag::info(),
        StatusMudanca::Recusada | StatusMudanca::Falhou => Tag::danger(),
    };
    tag.small().child(t::status_mudanca(s))
}

fn cor_impacto(i: ImpactoVisual, tema: &Theme) -> Hsla {
    match i {
        ImpactoVisual::Nenhum => tema.muted_foreground,
        ImpactoVisual::Baixo => tema.info,
        ImpactoVisual::Medio => tema.warning,
        ImpactoVisual::Alto => tema.danger,
    }
}

fn item(m: &ItemMudanca, tema: &Theme) -> Div {
    v_flex()
        .gap_1()
        .py_2()
        .border_t_1()
        .border_color(tema.border)
        .child(
            h_flex()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(m.nome_legivel.clone()),
                )
                .child(tag_status(m.status))
                .child(Tag::secondary().small().child(t::camada(m.camada)))
                .child(
                    Tag::secondary()
                        .small()
                        .child(t::experimental(m.experimental)),
                ),
        )
        .child(div().text_sm().child(format!(
            "{} → {}",
            m.antes.as_deref().unwrap_or(t::NAO_DEFINIDO),
            m.depois.as_deref().unwrap_or(t::NAO_DEFINIDO)
        )))
        .child(
            div()
                .text_sm()
                .text_color(tema.muted_foreground)
                .child(m.motivo.clone()),
        )
        .child(
            h_flex()
                .gap_1()
                .text_xs()
                .text_color(tema.muted_foreground)
                .child(format!("{}: {} ·", t::ROTULO_REGRA, m.regra))
                .child(format!("{}:", t::ROTULO_IMPACTO))
                .child(
                    div()
                        .text_color(cor_impacto(m.impacto, tema))
                        .child(t::impacto(m.impacto)),
                ),
        )
}

fn grupo(g: &GrupoMudancas, tema: &Theme) -> Div {
    let (pendentes, no_valor, outras, sugestoes) = g.resumo();
    v_flex()
        .p_4()
        .rounded_lg()
        .border_1()
        .border_color(tema.border)
        .bg(tema.group_box)
        .child(
            h_flex()
                .gap_2()
                .pb_2()
                .child(div().font_semibold().child(g.nome_legivel.clone()))
                .when(g.nome_legivel != g.perfil && g.perfil != "global", |d| {
                    d.child(
                        div()
                            .text_xs()
                            .text_color(tema.muted_foreground)
                            .child(format!("“{}”", g.perfil)),
                    )
                })
                .child(
                    div()
                        .text_xs()
                        .text_color(tema.muted_foreground)
                        .child(t::resumo_grupo(pendentes, no_valor, outras, sugestoes)),
                ),
        )
        .children(g.itens.iter().map(|m| item(m, tema)))
}

/// Instante atual (Unix, segundos) para os tempos relativos da atualização.
fn agora_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
