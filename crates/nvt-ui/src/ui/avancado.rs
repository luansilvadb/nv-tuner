//! Seção Avançado (spec `interface-e-saude.md` §5.5, lote L2).
//!
//! Uma [`SettingPage`] com os grupos técnicos: sistema e recursos, mudanças
//! por perfil, sugestões em teste, Windows, jogos (técnico), preferências,
//! atualização, histórico técnico e desfazer tudo. Inicialização, Manutenção
//! e Métricas entram no L4. Detalhe técnico (IDs de regra, motivo, impacto)
//! fica recolhido em acordeões.

use gpui_kit::component::Theme;
use gpui_kit::component::accordion::Accordion;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants};
use gpui_kit::component::link::Link;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, StyledExt, WindowExt, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use nvt_ipc::{
    Aviso, EstadoExperimental, ImpactoVisual, Jogo, JogoIgnorado, LIMITE_FPS_MAX, LIMITE_FPS_MIN,
    ModoLimpeza, NivelAviso, Qualidade, Reversao, Sincronismo, SituacaoInicio, StatusMudanca,
    SugestaoExperimental,
};

use super::estrutura::{Janela, agora_unix, cartao, explicacao, linha};
use crate::arquivo_jogo as aj;
use crate::modelo::{
    Comando, Conexao, GrupoMudancas, ItemMudanca, destino_usuario, formatar_data, linha_setting,
};
use crate::tema;
use crate::textos as t;
use crate::textos::avancado as ta;
use crate::textos::saude as ts;

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

/// Desenha o conteúdo de uma seção do Avançado.
type Desenho = fn(&Janela, &mut Window, &mut Context<Janela>) -> AnyElement;

/// Conteúdo da seção Avançado: seções empilhadas, cada uma com título e
/// frase (sem navegação própria; o título da página já diz "Avançado").
pub(super) fn render(janela: &Janela, window: &mut Window, cx: &mut Context<Janela>) -> AnyElement {
    if janela.ui.conexao == Conexao::Conectando && janela.ui.dados.estado.is_none() {
        return div()
            .text_color(cx.theme().muted_foreground)
            .child(t::CARREGANDO)
            .into_any_element();
    }
    let secoes: [(&'static str, &'static str, Desenho); 11] = [
        (
            ta::GRUPO_SISTEMA,
            ta::GRUPO_SISTEMA_DESC,
            Janela::grupo_sistema,
        ),
        (
            ta::GRUPO_MUDANCAS,
            ta::GRUPO_MUDANCAS_DESC,
            Janela::mudancas,
        ),
        (
            ta::GRUPO_SUGESTOES,
            ta::GRUPO_SUGESTOES_DESC,
            Janela::grupo_sugestoes,
        ),
        (ta::GRUPO_WINDOWS, ta::GRUPO_WINDOWS_DESC, Janela::windows),
        (ta::GRUPO_JOGOS, ta::GRUPO_JOGOS_DESC, Janela::grupo_jogos),
        (
            ta::GRUPO_PREFERENCIAS,
            ta::GRUPO_PREFERENCIAS_DESC,
            |j, w, cx| j.preferencias(w, cx).into_any_element(),
        ),
        (
            ta::GRUPO_INICIALIZACAO,
            ta::GRUPO_INICIALIZACAO_DESC,
            Janela::grupo_inicializacao,
        ),
        (
            ta::GRUPO_MANUTENCAO,
            ta::GRUPO_MANUTENCAO_DESC,
            Janela::grupo_manutencao,
        ),
        (
            ta::GRUPO_ATUALIZACAO,
            ta::GRUPO_ATUALIZACAO_DESC,
            Janela::grupo_atualizacao,
        ),
        (ta::GRUPO_HISTORICO, ta::GRUPO_HISTORICO_DESC, |j, w, cx| {
            j.historico(w, cx).into_any_element()
        }),
        (ta::GRUPO_DESFAZER, ta::GRUPO_DESFAZER_DESC, |j, w, cx| {
            j.restaurar(w, cx).into_any_element()
        }),
    ];
    let tema = cx.theme().clone();
    let mut v = v_flex()
        .gap(px(tema::ESP_8))
        .max_w(px(tema::LARGURA_CONTEUDO));
    for (titulo, frase, desenhar) in secoes {
        // Métricas (build de teste) entra logo antes de "Desfazer tudo", que fecha a página.
        #[cfg(feature = "medicao-partidas")]
        if titulo == ta::GRUPO_DESFAZER {
            v = v.child(super::partidas::grupo_metricas(janela, cx));
        }
        let corpo = desenhar(janela, window, cx);
        v = v.child(
            v_flex()
                .gap(px(tema::ESP_3))
                .child(
                    v_flex()
                        .gap(px(tema::ESP_1))
                        .child(
                            div()
                                .text_size(px(tema::TAM_SUBTITULO))
                                .font_semibold()
                                .child(titulo),
                        )
                        .child(
                            div()
                                .text_size(px(tema::TAM_CORPO))
                                .text_color(tema.muted_foreground)
                                .child(frase),
                        ),
                )
                .child(corpo),
        );
    }
    v.into_any_element()
}
/// Cartões técnicos do sistema, também usados pela Visão geral legada.
pub(super) struct CartoesTecnicos {
    pub sistema: Div,
    pub capacidades: Div,
    pub servico: Div,
    pub atualizacao: Option<Div>,
    pub aviso_atualizado: Option<Div>,
    pub conformidade: Div,
    pub deriva: Div,
}

impl Janela {
    pub(super) fn cartoes_tecnicos(&self, cx: &mut Context<Self>) -> CartoesTecnicos {
        let tema = cx.theme().clone();
        let estado = self.ui.dados.estado.as_ref();
        let gpu = estado.and_then(|e| e.gpu.as_ref());
        let driver = estado.and_then(|e| e.driver.as_ref());

        let sistema = cartao(
            t::SECAO_SISTEMA,
            v_flex()
                .gap(px(tema::ESP_1))
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
                .text_size(px(tema::TAM_CORPO))
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
                .gap(px(tema::ESP_1))
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
            _ if self.ui.incompativel.is_some() => t::CONEXAO_VISUALIZACAO,
            Conexao::Conectando => t::CONEXAO_CONECTANDO,
            Conexao::Servico => t::CONEXAO_SERVICO,
            Conexao::Visualizacao => t::CONEXAO_VISUALIZACAO,
            Conexao::Iniciando => t::estado::SERVICO_INICIANDO,
            Conexao::SemResposta => t::estado::SERVICO_SEM_RESPOSTA,
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
                .gap(px(tema::ESP_1))
                .child(linha(t::ROTULO_STATUS, status.into(), &tema))
                .child(linha(t::ROTULO_VERSAO, versao, &tema))
                .child(linha(t::ROTULO_ULTIMA_APLICACAO, ultima, &tema)),
            &tema,
        );

        let linhas_atualizacao = self.ui.linhas_atualizacao(agora_unix());
        let atualizacao = (!linhas_atualizacao.is_empty()).then(|| {
            cartao(
                t::SECAO_ATUALIZACAO,
                v_flex().gap(px(tema::ESP_1)).children(
                    linhas_atualizacao
                        .into_iter()
                        .map(|l| div().text_size(px(tema::TAM_CORPO)).child(l)),
                ),
                &tema,
            )
        });
        let aviso_atualizado = self.ui.aviso_atualizado().map(|versao| {
            h_flex()
                .gap(px(tema::ESP_2))
                .items_center()
                .child(div().flex_1().child(Alert::success(
                    "atualizado-para",
                    t::atualizado_para(versao),
                )))
                .child(
                    Button::new("atualizado-ok")
                        .outline()
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
                .min_w(px(tema::COLUNA_CURTA))
                .child(
                    div()
                        .text_size(px(tema::TAM_SUBTITULO))
                        .font_semibold()
                        .text_color(cor)
                        .child(n.to_string()),
                )
                .child(
                    div()
                        .text_size(px(tema::TAM_LEGENDA))
                        .text_color(tema.muted_foreground)
                        .child(rotulo),
                )
        };
        let conformidade = cartao(
            t::SECAO_CONFORMIDADE,
            v_flex()
                .gap(px(tema::ESP_3))
                .child(
                    h_flex()
                        .gap(px(tema::ESP_4))
                        .child(numero(c.pendentes, t::CONF_PENDENTES, tema.warning))
                        .child(numero(c.conformes, t::CONF_CONFORMES, tema.success))
                        .child(numero(c.travadas, t::CONF_TRAVADAS, tema.info))
                        .child(numero(c.falhas, t::CONF_FALHAS, tema.danger)),
                )
                .when(c.conforme(), |d| {
                    d.child(
                        div()
                            .text_size(px(tema::TAM_CORPO))
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
                        .text_size(px(tema::TAM_CORPO))
                        .text_color(tema.muted_foreground)
                        .child(t::SEM_DERIVA),
                )
            } else {
                v_flex()
                    .gap(px(tema::ESP_1))
                    .children(alertas.iter().map(|a| {
                        div()
                            .text_size(px(tema::TAM_CORPO))
                            .text_color(tema.warning)
                            .child(t::deriva(
                                &a.nome_legivel,
                                &a.perfil,
                                &a.esperado,
                                a.encontrado.as_deref(),
                            ))
                    }))
            },
            &tema,
        );

        CartoesTecnicos {
            sistema,
            capacidades,
            servico,
            atualizacao,
            aviso_atualizado,
            conformidade,
            deriva,
        }
    }

    // -----------------------------------------------------------------------
    // Grupos
    // -----------------------------------------------------------------------

    fn botoes_sistema(&self, cx: &mut Context<Self>) -> Div {
        let pode_escrever = self.ui.pode_escrever();
        let ocioso = self.ui.ocupado.is_none();
        h_flex()
            .gap(px(tema::ESP_2))
            .child(
                Button::new("avancado-aplicar")
                    .outline()
                    .label(t::BOTAO_APLICAR)
                    .disabled(!pode_escrever)
                    .when(!pode_escrever, |b| b.tooltip(t::SO_COM_SERVICO))
                    .on_click(cx.listener(|this, _, _, cx| this.enviar(Comando::AplicarAgora, cx))),
            )
            .child(
                Button::new("avancado-reescanear")
                    .outline()
                    .label(t::BOTAO_REESCANEAR)
                    .disabled(!ocioso)
                    .on_click(cx.listener(|this, _, _, cx| this.enviar(Comando::Reescanear, cx))),
            )
    }

    fn grupo_sistema(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tema = cx.theme().clone();
        let c = self.cartoes_tecnicos(cx);
        let termos = detalhe(
            "termos-sistema",
            ta::TERMOS,
            lista_termos(
                &[
                    ta::EXPLICA_NVAPI,
                    ta::EXPLICA_VRAM,
                    ta::EXPLICA_DLSS,
                    ta::EXPLICA_REFLEX,
                    ta::EXPLICA_REBAR,
                    ta::EXPLICA_VRR,
                    ta::EXPLICA_DERIVA,
                ],
                &tema,
            ),
            window,
            cx,
        );
        v_flex()
            .gap(px(tema::ESP_4))
            .child(self.botoes_sistema(cx))
            .child(self.cartao_avisos(&tema))
            .child(
                h_flex()
                    .gap(px(tema::ESP_4))
                    .items_start()
                    .child(div().flex_1().min_w_0().child(c.sistema))
                    .child(div().flex_1().min_w_0().child(c.capacidades)),
            )
            .child(c.conformidade)
            .child(c.deriva)
            .child(termos)
            .into_any_element()
    }

    fn grupo_atualizacao(&self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tema = cx.theme().clone();
        let c = self.cartoes_tecnicos(cx);
        let ocioso = self.ui.ocupado.is_none();
        v_flex()
            .gap(px(tema::ESP_4))
            .children(c.aviso_atualizado)
            .child(c.servico)
            .child(linha(ta::ROTULO_AGENTE, self.estado_agente().into(), &tema))
            .child(explicacao(ta::EXPLICA_AGENTE, &tema))
            .children(c.atualizacao)
            .child(
                h_flex().child(
                    Button::new("verificar-atualizacao")
                        .outline()
                        .label(ta::BOTAO_VERIFICAR_AGORA)
                        .disabled(!ocioso)
                        .on_click(
                            cx.listener(|this, _, _, cx| this.enviar(Comando::Atualizar, cx)),
                        ),
                ),
            )
            .into_any_element()
    }

    fn estado_agente(&self) -> &'static str {
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
            })
    }

    /// Inicialização (§5.5): lista por fonte, novo/mudou/visto, data de
    /// detecção; "Marcar como visto" e "Abrir configurações de inicialização".
    fn grupo_inicializacao(&self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tema = cx.theme().clone();
        let Some(saude) = self.ui.dados.saude.as_ref() else {
            return explicacao(ts::SEM_DADOS, &tema).into_any_element();
        };
        let ocioso = self.ui.ocupado.is_none();
        let mut programas = saude.inicializacao.clone();
        // Novidades primeiro; depois por origem e nome.
        programas.sort_by_key(|p| {
            (
                p.situacao == SituacaoInicio::NaBase,
                p.fonte,
                p.nome.to_lowercase(),
            )
        });
        let mut lista = v_flex();
        for (i, p) in programas.iter().enumerate() {
            let tag = match p.situacao {
                SituacaoInicio::Novo => Tag::warning(),
                SituacaoInicio::Mudou => Tag::info(),
                SituacaoInicio::NaBase => Tag::secondary(),
            }
            .small()
            .child(ts::situacao(p.situacao));
            let marcar = (p.situacao != SituacaoInicio::NaBase).then(|| {
                let cmd = Comando::MarcarInicioVisto {
                    fonte: p.fonte,
                    nome: p.nome.clone(),
                };
                Button::new(("marcar-visto", i))
                    .ghost()
                    .small()
                    .label(ts::BOTAO_MARCAR_VISTO)
                    .disabled(!ocioso)
                    .on_click(cx.listener(move |this, _, _, cx| this.enviar(cmd.clone(), cx)))
            });
            lista = lista.child(
                h_flex()
                    .gap(px(tema::ESP_4))
                    .items_center()
                    .py(px(tema::ESP_3))
                    .when(i > 0, |d| d.border_t_1().border_color(tema.border))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(tema::ESP_1))
                            .child(
                                h_flex()
                                    .gap(px(tema::ESP_2))
                                    .items_center()
                                    .text_size(px(tema::TAM_CORPO))
                                    .child(p.nome.clone())
                                    .child(tag),
                            )
                            .child(
                                div()
                                    .text_size(px(tema::TAM_LEGENDA))
                                    .text_color(tema.muted_foreground)
                                    .child(ts::detalhe_programa(
                                        p.fonte,
                                        &p.executavel,
                                        &formatar_data(p.detectado_unix),
                                        p.habilitado,
                                    )),
                            ),
                    )
                    .children(marcar),
            );
        }
        let corpo = if programas.is_empty() {
            explicacao(ts::SEM_PROGRAMAS, &tema)
        } else {
            lista
        };
        v_flex()
            .gap(px(tema::ESP_4))
            .child(
                super::estrutura::superficie(&tema)
                    .p(px(tema::ESP_4))
                    .child(corpo),
            )
            .child(explicacao(ts::EXPLICA_INICIALIZACAO, &tema))
            .child(super::inicio::botao_abrir(
                "abrir-inicializacao-avancado",
                ts::BOTAO_ABRIR_CONFIGURACOES,
                self,
                cx,
            ))
            .into_any_element()
    }

    /// Manutenção (§5.5, R-L-08): modo da limpeza e últimas passadas.
    fn grupo_manutencao(&self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tema = cx.theme().clone();
        let Some(saude) = self.ui.dados.saude.as_ref() else {
            return explicacao(ts::SEM_DADOS, &tema).into_any_element();
        };
        let mut passadas = saude.limpeza.clone();
        passadas.sort_by_key(|p| p.alvo);
        let resumo = ts::resumo_limpeza(saude.modo_limpeza, &passadas);
        let mut corpo = v_flex()
            .gap(px(tema::ESP_3))
            .child(linha(
                ta::ROTULO_MODO_LIMPEZA,
                ts::modo(saude.modo_limpeza).into(),
                &tema,
            ))
            .when_some(resumo, |c, r| {
                c.child(
                    div()
                        .text_size(px(tema::TAM_CORPO))
                        .font_semibold()
                        .child(r),
                )
            });
        if saude.modo_limpeza != ModoLimpeza::Desligada || !passadas.is_empty() {
            corpo = corpo.child(
                div()
                    .text_size(px(tema::TAM_CORPO))
                    .text_color(tema.muted_foreground)
                    .child(ts::ROTULO_ULTIMAS),
            );
            if passadas.is_empty() {
                corpo = corpo.child(explicacao(ts::SEM_PASSADAS, &tema));
            }
            for p in &passadas {
                corpo = corpo.child(linha(
                    ts::alvo(p.alvo),
                    ts::passada(p, &formatar_data(p.inicio_unix)),
                    &tema,
                ));
            }
        }
        v_flex()
            .gap(px(tema::ESP_4))
            .child(
                super::estrutura::superficie(&tema)
                    .p(px(tema::ESP_4))
                    .child(corpo),
            )
            .child(explicacao(ts::EXPLICA_LIMPEZA, &tema))
            .into_any_element()
    }

    fn grupo_jogos(&self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.jogos(cx).into_any_element()
    }

    fn grupo_sugestoes(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tema = cx.theme().clone();
        let sugestoes = &self.ui.dados.sugestoes_driver;
        v_flex()
            .gap(px(tema::ESP_3))
            .child(explicacao(ta::EXPLICA_EXPERIMENTAL, &tema))
            .when(!sugestoes.is_empty(), |v| {
                v.child(explicacao(t::EXPERIMENTAIS_DRIVER, &tema))
            })
            .when(
                !sugestoes.is_empty()
                    && self.ui.conexao == Conexao::Servico
                    && !sessao_console_ativa(),
                |v| v.child(explicacao(t::ACEITE_DRIVER_INATIVO, &tema)),
            )
            .child(self.experimentais("driver", sugestoes, &tema, window, cx))
            .into_any_element()
    }

    // -----------------------------------------------------------------------
    // Mudanças por perfil
    // -----------------------------------------------------------------------

    pub(super) fn mudancas(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tema = cx.theme().clone();
        let mut v = v_flex()
            .gap(px(tema::ESP_4))
            .child(explicacao(ta::EXPLICA_PERFIL, &tema));
        if self.ui.dados.grupos.is_empty() {
            v = v.child(explicacao(t::SEM_MUDANCAS, &tema));
        }
        for (i, g) in self.ui.dados.grupos.iter().enumerate() {
            v = v.child(grupo(i, g, &tema, window, cx));
        }
        let pode = self.ui.pode_escrever();
        let respeitadas = &self.ui.dados.respeitadas;
        let corpo = if respeitadas.is_empty() {
            v_flex().child(explicacao(t::SEM_RESPEITADAS, &tema))
        } else {
            v_flex()
                .gap(px(tema::ESP_2))
                .child(explicacao(ta::EXPLICA_TRAVADA, &tema))
                .child(explicacao(ta::EXPLICA_DESTRAVAR, &tema))
                .children(respeitadas.iter().enumerate().map(|(i, r)| {
                    let (perfil, setting) = (r.perfil.clone(), r.setting.clone());
                    h_flex()
                        .gap(px(tema::ESP_3))
                        .justify_between()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_size(px(tema::TAM_CORPO))
                                        .font_semibold()
                                        .child(format!("{} · {}", r.nome_perfil, r.nome_legivel)),
                                )
                                .child(
                                    div()
                                        .text_size(px(tema::TAM_LEGENDA))
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
            v_flex().child(explicacao(t::SEM_REVERSOES, &tema))
        } else {
            v_flex()
                .gap(px(tema::ESP_2))
                .children(reversoes.iter().map(|r| reversao(r, &tema)))
        };
        v.child(cartao(t::SECAO_RESPEITADAS, corpo, &tema))
            .child(cartao(t::SECAO_REVERSOES, corpo_rev, &tema))
            .into_any_element()
    }

    // -----------------------------------------------------------------------
    // Windows
    // -----------------------------------------------------------------------

    pub(super) fn windows(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tema = cx.theme().clone();
        let mudancas = &self.ui.dados.sistema.mudancas;
        let mut ajustes = v_flex().gap(px(tema::ESP_2));
        for (i, mudanca) in mudancas.iter().enumerate() {
            let destino = destino_usuario(&mudanca.setting, mudanca.alvo_tela.as_deref());
            let pode = self.ui.pode_escrever() && destino.is_some();
            let m = ItemMudanca::from(mudanca);
            ajustes = ajustes.child(
                h_flex()
                    .gap(px(tema::ESP_3))
                    .justify_between()
                    .child(linha_mudanca(&m, &tema).flex_1().min_w_0())
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
        if mudancas.is_empty() {
            ajustes = ajustes.child(explicacao(t::SEM_AJUSTES_USUARIO, &tema));
        } else {
            let itens: Vec<ItemMudanca> = mudancas.iter().map(ItemMudanca::from).collect();
            ajustes = ajustes.child(detalhe(
                "detalhe-windows",
                ta::DETALHES_TECNICOS,
                detalhes_itens(&itens, &tema),
                window,
                cx,
            ));
        }

        let diagnosticos = self
            .ui
            .dados
            .estado
            .as_ref()
            .map_or(&[][..], |e| e.diagnosticos.as_slice());
        let diagnostico = v_flex()
            .gap(px(tema::ESP_3))
            .child(explicacao(ta::EXPLICA_DIAGNOSTICO, &tema))
            .when(diagnosticos.is_empty(), |v| {
                v.child(explicacao(t::SEM_DIAGNOSTICOS, &tema))
            })
            .children(diagnosticos.iter().enumerate().map(|(i, d)| {
                let info = t::diagnostico(&d.codigo);
                v_flex()
                    .gap(px(tema::ESP_1))
                    .child(
                        div().text_size(px(tema::TAM_CORPO)).font_semibold().child(
                            info.map_or(d.codigo.as_str(), |(titulo, _, _)| titulo)
                                .to_owned(),
                        ),
                    )
                    .child(
                        div()
                            .text_size(px(tema::TAM_CORPO))
                            .child(t::estado_diagnostico(d)),
                    )
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
        let experimentais = self.experimentais(
            "windows",
            &self.ui.dados.sistema.sugestoes,
            &tema,
            window,
            cx,
        );

        v_flex()
            .gap(px(tema::ESP_4))
            .child(linha(ta::ROTULO_AGENTE, self.estado_agente().into(), &tema))
            .child(cartao(t::SECAO_WINDOWS_FEITO, ajustes, &tema))
            .child(cartao(
                t::SECAO_EXPERIMENTAIS,
                v_flex()
                    .gap(px(tema::ESP_2))
                    .child(explicacao(ta::EXPLICA_EXPERIMENTAL, &tema))
                    .child(experimentais),
                &tema,
            ))
            .child(cartao(t::SECAO_DIAGNOSTICO, diagnostico, &tema))
            .into_any_element()
    }

    fn experimentais(
        &self,
        chave: &'static str,
        sugestoes: &[SugestaoExperimental],
        tema: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let regras: Vec<(String, String)> = sugestoes
            .iter()
            .filter(|s| s.estado != EstadoExperimental::Estavel)
            .map(|s| (s.descricao.clone(), s.regra.clone()))
            .collect();
        let detalhe_regras = (!regras.is_empty()).then(|| {
            detalhe(
                SharedString::from(format!("regras-{chave}")),
                ta::DETALHES_TECNICOS,
                v_flex()
                    .gap(px(tema::ESP_1))
                    .children(regras.into_iter().map(|(d, r)| {
                        div()
                            .text_size(px(tema::TAM_LEGENDA))
                            .text_color(tema.muted_foreground)
                            .child(format!("{d}: {} {r}", t::ROTULO_REGRA))
                    })),
                window,
                cx,
            )
        });
        self.linhas_experimentais(chave, sugestoes, tema, cx)
            .children(detalhe_regras)
    }

    /// Linhas com Aceitar/Retirar, sem o detalhe recolhido das regras.
    pub(super) fn linhas_experimentais(
        &self,
        chave: &'static str,
        sugestoes: &[SugestaoExperimental],
        tema: &Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let pode = self.ui.pode_escrever();
        v_flex()
            .gap(px(tema::ESP_2))
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
                        .gap(px(tema::ESP_3))
                        .justify_between()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap(px(tema::ESP_1))
                                .child(
                                    div()
                                        .text_size(px(tema::TAM_CORPO))
                                        .font_semibold()
                                        .child(s.descricao.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(tema::TAM_LEGENDA))
                                        .text_color(tema.muted_foreground)
                                        .child(format!(
                                            "{} · {}",
                                            t::camada(s.camada),
                                            t::experimental(s.estado)
                                        )),
                                ),
                        )
                        .child(
                            Button::new(SharedString::from(format!("aceite-{chave}-{i}")))
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

    fn preferencias(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = cx.theme().clone();
        let r = &self.ui.rascunho;
        let i_sync = SINCRONISMOS.iter().position(|s| *s == r.sincronismo);
        let i_qual = QUALIDADES.iter().position(|q| *q == r.qualidade);

        let sincronismo = cartao(
            t::SECAO_SINCRONISMO,
            v_flex()
                .gap(px(tema::ESP_3))
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
                    .gap(px(tema::ESP_3))
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
                            .gap(px(tema::ESP_2))
                            .text_size(px(tema::TAM_CORPO))
                            .child(
                                div()
                                    .w(px(tema::COLUNA_CURTA))
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
                .gap(px(tema::ESP_3))
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
                        .gap(px(tema::ESP_2))
                        .child(passo("fps-10", "−10", -10))
                        .child(passo("fps-1", "−1", -1))
                        .child(
                            div()
                                .w(px(tema::COLUNA_CURTA))
                                .text_center()
                                .font_semibold()
                                .child(limite.map_or("—".into(), |v| format!("{v} FPS"))),
                        )
                        .child(passo("fps+1", "+1", 1))
                        .child(passo("fps+10", "+10", 10))
                        .child(div().w(px(tema::ESP_4)))
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
            .gap(px(tema::ESP_4))
            .child(sincronismo)
            .child(qualidade)
            .child(limite_fps)
            .child(
                h_flex()
                    .gap(px(tema::ESP_2))
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
                                .text_size(px(tema::TAM_CORPO))
                                .text_color(tema.warning)
                                .child(t::PREFS_NAO_SALVAS),
                        )
                    })
                    .when(!pode, |d| {
                        d.child(
                            div()
                                .text_size(px(tema::TAM_CORPO))
                                .text_color(tema.muted_foreground)
                                .child(t::SO_COM_SERVICO),
                        )
                    }),
            )
    }

    // -----------------------------------------------------------------------
    // Histórico
    // -----------------------------------------------------------------------

    fn historico(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .rounded(px(tema::RAIO_CARTAO))
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
                    .gap(px(tema::ESP_3))
                    .px(px(tema::ESP_3))
                    .py(px(tema::ESP_2))
                    .items_start()
                    .when(i % 2 == 1, |d| d.bg(tema.table_even))
                    .child(
                        div()
                            .w(px(tema::COLUNA_VALOR))
                            .flex_shrink_0()
                            .text_size(px(tema::TAM_CORPO))
                            .text_color(tema.muted_foreground)
                            .child(if e.quando_unix == 0 {
                                "—".to_string()
                            } else {
                                formatar_data(e.quando_unix)
                            }),
                    )
                    .child(
                        div().w(px(tema::COLUNA_VALOR)).flex_shrink_0().child(
                            v_flex()
                                .gap(px(tema::ESP_1))
                                .items_start()
                                .child(Tag::secondary().small().child(t::camada(e.camada)))
                                .child(Tag::secondary().small().child(t::acao_historico(e.acao))),
                        ),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(tema::TAM_CORPO))
                                    .font_semibold()
                                    .child(alvo),
                            )
                            .when(!valores.is_empty(), |d| {
                                d.child(div().text_size(px(tema::TAM_CORPO)).child(valores))
                            })
                            .child(
                                div()
                                    .text_size(px(tema::TAM_LEGENDA))
                                    .text_color(tema.muted_foreground)
                                    .child(e.descricao.clone()),
                            ),
                    )
            }))
    }

    // -----------------------------------------------------------------------
    // Restaurar
    // -----------------------------------------------------------------------

    fn restaurar(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = cx.theme().clone();
        let pode = self.ui.pode_restaurar();
        let bloqueio = self.ui.restauracao_bloqueada().cloned();
        cartao(
            t::TELA_RESTAURAR,
            v_flex()
                .gap(px(tema::ESP_4))
                .child(
                    div()
                        .text_size(px(tema::TAM_CORPO))
                        .child(t::RESTAURAR_EXPLICACAO),
                )
                .when_some(bloqueio.clone(), |v, a| {
                    v.child(
                        Alert::error("restauracao-bloqueada", a.mensagem.clone())
                            .title(t::RESTAURAR_BLOQUEADO),
                    )
                    .when_some(a.detalhe, |v, d| {
                        v.child(
                            div()
                                .text_size(px(tema::TAM_LEGENDA))
                                .text_color(tema.muted_foreground)
                                .child(d),
                        )
                    })
                })
                .child(
                    h_flex()
                        .gap(px(tema::ESP_3))
                        .items_center()
                        .child(
                            Button::new("restaurar")
                                .danger()
                                .outline()
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
                                    .text_size(px(tema::TAM_CORPO))
                                    .text_color(tema.muted_foreground)
                                    .child(texto),
                            )
                        }),
                ),
            &tema,
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

fn reversao(r: &Reversao, tema: &Theme) -> Div {
    let tag = match r.status {
        StatusMudanca::Aplicada => Tag::success(),
        StatusMudanca::Falhou | StatusMudanca::Recusada => Tag::danger(),
        _ => Tag::warning(),
    };
    v_flex()
        .gap(px(tema::ESP_1))
        .py(px(tema::ESP_2))
        .border_t_1()
        .border_color(tema.border)
        .child(
            h_flex()
                .gap(px(tema::ESP_2))
                .child(
                    div()
                        .text_size(px(tema::TAM_CORPO))
                        .font_semibold()
                        .child(format!(
                            "{} · {} · {}",
                            t::CAMADA_DRIVER,
                            r.nome_perfil,
                            r.nome_legivel
                        )),
                )
                .child(tag.small().child(t::status_reversao(r.status))),
        )
        .child(div().text_size(px(tema::TAM_CORPO)).child(format!(
            "{} → {}",
            r.atual.as_deref().unwrap_or(t::NAO_DEFINIDO),
            t::acao_reversao(r.acao, r.original.as_deref())
        )))
        .child(
            div()
                .text_size(px(tema::TAM_CORPO))
                .text_color(tema.muted_foreground)
                .child(r.motivo.clone()),
        )
}
/// Acordeão de uma seção só, recolhido por padrão; o estado aberto fica na
/// janela, pela chave `id`.
fn detalhe(
    id: impl Into<SharedString>,
    titulo: &'static str,
    conteudo: impl IntoElement,
    window: &mut Window,
    cx: &mut App,
) -> Accordion {
    let id: SharedString = id.into();
    let aberto =
        window.use_keyed_state(SharedString::from(format!("{id}-aberto")), cx, |_, _| false);
    let esta_aberto = *aberto.read(cx);
    Accordion::new(ElementId::Name(id))
        .small()
        .item(|i| {
            i.title(div().text_size(px(tema::TAM_CORPO)).child(titulo))
                .open(esta_aberto)
                .child(conteudo)
        })
        .on_toggle_click(move |abertos, window, cx| {
            let novo = !abertos.is_empty();
            aberto.update(cx, |a, _| *a = novo);
            window.refresh();
        })
}

fn lista_termos(termos: &[&'static str], tema: &Theme) -> Div {
    v_flex()
        .gap(px(tema::ESP_1))
        .children(termos.iter().map(|&texto| explicacao(texto, tema)))
}

/// Uma linha por mudança: nome, de → para e situação. Sem motivo nem regra
/// (ficam no detalhe recolhido).
fn linha_mudanca(m: &ItemMudanca, tema: &Theme) -> Div {
    h_flex()
        .gap(px(tema::ESP_2))
        .py(px(tema::ESP_1))
        .items_center()
        .child(
            div()
                .text_size(px(tema::TAM_CORPO))
                .font_semibold()
                .min_w(px(tema::COLUNA_ROTULO))
                .child(m.nome_legivel.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(tema::TAM_CORPO))
                .text_color(tema.muted_foreground)
                .child(format!(
                    "{} → {}",
                    m.antes.as_deref().unwrap_or(t::NAO_DEFINIDO),
                    m.depois.as_deref().unwrap_or(t::NAO_DEFINIDO)
                )),
        )
        .child(tag_status(m.status))
        .when(m.experimental != EstadoExperimental::Estavel, |d| {
            d.child(
                Tag::secondary()
                    .small()
                    .child(t::experimental(m.experimental)),
            )
        })
}

/// Detalhe técnico das mudanças: regra, impacto visual e motivo. O motivo
/// aparece uma vez para cada sequência de itens com o mesmo texto.
fn detalhes_itens(itens: &[ItemMudanca], tema: &Theme) -> Div {
    let mut v = v_flex().gap(px(tema::ESP_1));
    let mut ultimo_motivo: Option<&str> = None;
    for m in itens {
        v = v.child(
            h_flex()
                .gap(px(tema::ESP_1))
                .text_size(px(tema::TAM_LEGENDA))
                .text_color(tema.muted_foreground)
                .child(format!(
                    "{} · {} {} · {}:",
                    m.nome_legivel,
                    t::ROTULO_REGRA,
                    m.regra,
                    t::ROTULO_IMPACTO
                ))
                .child(
                    div()
                        .text_color(cor_impacto(m.impacto, tema))
                        .child(t::impacto(m.impacto)),
                ),
        );
        if ultimo_motivo != Some(m.motivo.as_str()) {
            v = v.child(
                div()
                    .pl(px(tema::ESP_3))
                    .text_size(px(tema::TAM_LEGENDA))
                    .text_color(tema.muted_foreground)
                    .child(m.motivo.clone()),
            );
            ultimo_motivo = Some(m.motivo.as_str());
        }
    }
    v.child(explicacao(ta::EXPLICA_IMPACTO, tema))
}

/// Perfil do driver: só o que muda; o que já está no valor vira uma contagem.
fn grupo(i: usize, g: &GrupoMudancas, tema: &Theme, window: &mut Window, cx: &mut App) -> Div {
    let (pendentes, no_valor, outras, sugestoes) = g.resumo();
    let mudam: Vec<&ItemMudanca> = g
        .itens
        .iter()
        .filter(|m| m.status != StatusMudanca::JaNoValor)
        .collect();
    let itens: Vec<ItemMudanca> = g.itens.clone();
    v_flex()
        .p(px(tema::ESP_4))
        .gap(px(tema::ESP_1))
        .rounded(px(tema::RAIO_CARTAO))
        .border_1()
        .border_color(tema.border)
        .bg(tema.group_box)
        .child(
            h_flex()
                .gap(px(tema::ESP_2))
                .pb(px(tema::ESP_2))
                .child(div().font_semibold().child(g.nome_legivel.clone()))
                .when(g.nome_legivel != g.perfil && g.perfil != "global", |d| {
                    d.child(
                        div()
                            .text_size(px(tema::TAM_LEGENDA))
                            .text_color(tema.muted_foreground)
                            .child(format!("“{}”", g.perfil)),
                    )
                })
                .child(
                    div()
                        .text_size(px(tema::TAM_LEGENDA))
                        .text_color(tema.muted_foreground)
                        .child(t::resumo_grupo(pendentes, no_valor, outras, sugestoes)),
                ),
        )
        .children(mudam.into_iter().map(|m| linha_mudanca(m, tema)))
        .when(no_valor > 0, |v| {
            v.child(explicacao_dinamica(ta::ja_no_valor(no_valor), tema))
        })
        .child(div().pt(px(tema::ESP_2)).child(detalhe(
            SharedString::from(format!("detalhe-perfil-{i}")),
            ta::DETALHES_TECNICOS,
            detalhes_itens(&itens, tema),
            window,
            cx,
        )))
}

fn explicacao_dinamica(texto: String, tema: &Theme) -> Div {
    div()
        .text_size(px(tema::TAM_CORPO))
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

// ---------------------------------------------------------------------------
// Jogos (técnico) e avisos do serviço
// ---------------------------------------------------------------------------

impl Janela {
    /// Avisos do serviço (estado recuperado, snapshot, erros por perfil...).
    fn cartao_avisos(&self, tema: &Theme) -> Div {
        let lista_avisos = self.ui.avisos();
        cartao(
            t::SECAO_AVISOS,
            if lista_avisos.is_empty() {
                v_flex().child(
                    div()
                        .text_size(px(tema::TAM_CORPO))
                        .text_color(tema.muted_foreground)
                        .child(t::SEM_AVISOS),
                )
            } else {
                v_flex()
                    .gap(px(tema::ESP_2))
                    .children(lista_avisos.iter().map(|a| aviso(a, tema)))
            },
            tema,
        )
    }

    pub(super) fn jogos(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = cx.theme().clone();
        let ignorados = &self.ui.dados.ignorados;
        v_flex()
            .gap(px(tema::ESP_4))
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
                .text_size(px(tema::TAM_LEGENDA))
                .font_semibold()
                .text_color(tema.muted_foreground)
                .child(s)
        };
        let cabecalho = h_flex()
            .gap(px(tema::ESP_3))
            .px(px(tema::ESP_3))
            .py(px(tema::ESP_2))
            .border_b_1()
            .border_color(tema.border)
            .child(div().w(px(tema::ESP_4)).flex_shrink_0())
            .child(
                div()
                    .flex_1()
                    .min_w(px(tema::COLUNA_ROTULO))
                    .text_size(px(tema::TAM_LEGENDA))
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
            let cel = |w: f32| {
                div()
                    .w(px(w))
                    .flex_shrink_0()
                    .text_size(px(tema::TAM_CORPO))
            };
            let aberto = self.expandidos.contains(&j.perfil) || self.expandidos.contains(&j.nome);
            let (perfil, nome) = (j.perfil.clone(), j.nome.clone());
            let herda = t::herda_globais(j.resumo.herda_do_global.len());
            let faca = t::faca_no_jogo_resumo(j.orientacoes.len());
            let linha = h_flex()
                .id(("jogo", i))
                .gap(px(tema::ESP_3))
                .px(px(tema::ESP_3))
                .py(px(tema::ESP_2))
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
                        .w(px(tema::ESP_4))
                        .flex_shrink_0()
                        .text_size(px(tema::TAM_LEGENDA))
                        .text_color(tema.muted_foreground)
                        .child(if aberto { "▾" } else { "▸" }),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(tema::COLUNA_ROTULO))
                        .child(
                            h_flex()
                                .gap(px(tema::ESP_2))
                                .flex_wrap()
                                .child(
                                    div()
                                        .text_size(px(tema::TAM_CORPO))
                                        .font_semibold()
                                        .child(j.nome.clone()),
                                )
                                .when(j.competitivo.is_some(), |d| {
                                    d.child(Tag::info().small().child(t::SELO_COMPETITIVO))
                                })
                                .when(j.em_execucao, |d| {
                                    d.child(
                                        Tag::secondary().small().outline().child(t::SELO_ABERTO),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(tema::TAM_LEGENDA))
                                .text_color(tema.muted_foreground)
                                .child(format!(
                                    "“{}” · {}",
                                    j.perfil,
                                    t::origem_perfil(j.origem_perfil)
                                )),
                        ),
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
                        .w(px(tema::COLUNA_LARGA))
                        .flex_shrink_0()
                        .text_size(px(tema::TAM_LEGENDA))
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
                .when(aberto, |d| d.child(self.detalhe_jogo(i, &j, &tema, cx)))
        });
        v_flex()
            .gap(px(tema::ESP_2))
            .child(
                div()
                    .text_size(px(tema::TAM_LEGENDA))
                    .text_color(tema.muted_foreground)
                    .child(t::DETALHE_EXPANDIR),
            )
            .child(
                v_flex()
                    .rounded(px(tema::RAIO_CARTAO))
                    .border_1()
                    .border_color(tema.border)
                    .child(cabecalho)
                    .children(linhas),
            )
    }

    /// Lista completa de um jogo: regras aplicadas (com as settings),
    /// não aplicadas (com o motivo) e o que herda do global.
    fn detalhe_jogo(&self, i: usize, j: &Jogo, tema: &Theme, cx: &mut Context<Self>) -> Div {
        let titulo = |s: &'static str| {
            div()
                .text_size(px(tema::TAM_LEGENDA))
                .font_semibold()
                .text_color(tema.muted_foreground)
                .child(s)
        };
        let nenhuma = || {
            div()
                .text_size(px(tema::TAM_CORPO))
                .text_color(tema.muted_foreground)
                .child(t::DETALHE_NENHUMA)
        };
        let r = &j.resumo;
        let aplicadas = v_flex()
            .gap(px(tema::ESP_1))
            .child(titulo(t::DETALHE_APLICADAS))
            .when(r.aplicadas.is_empty(), |d| d.child(nenhuma()))
            .children(r.aplicadas.iter().map(|a| {
                v_flex()
                    .child(
                        h_flex()
                            .gap(px(tema::ESP_2))
                            .child(
                                div()
                                    .text_size(px(tema::TAM_CORPO))
                                    .font_semibold()
                                    .text_color(tema.success)
                                    .child(t::nome_regra(&a.regra, &a.descricao)),
                            )
                            .child(
                                div()
                                    .text_size(px(tema::TAM_LEGENDA))
                                    .text_color(tema.muted_foreground)
                                    .child(a.regra.clone()),
                            ),
                    )
                    .children(a.settings.iter().map(|s| {
                        div()
                            .text_size(px(tema::TAM_LEGENDA))
                            .pl(px(tema::ESP_3))
                            .child(format!(
                                "• {}",
                                linha_setting(&self.ui.dados.grupos, &j.perfil, s)
                            ))
                    }))
            }));
        let nao_aplicadas = v_flex()
            .gap(px(tema::ESP_1))
            .child(titulo(t::DETALHE_NAO_APLICADAS))
            .when(r.nao_aplicadas.is_empty(), |d| d.child(nenhuma()))
            .children(r.nao_aplicadas.iter().map(|n| {
                v_flex()
                    .child(
                        h_flex()
                            .gap(px(tema::ESP_2))
                            .child(
                                div()
                                    .text_size(px(tema::TAM_CORPO))
                                    .font_semibold()
                                    .child(t::nome_regra(&n.regra, &n.descricao)),
                            )
                            .child(
                                div()
                                    .text_size(px(tema::TAM_LEGENDA))
                                    .text_color(tema.muted_foreground)
                                    .child(n.regra.clone()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(tema::TAM_LEGENDA))
                            .pl(px(tema::ESP_3))
                            .text_color(tema.muted_foreground)
                            .child(t::motivo_longo(&n.motivo)),
                    )
            }));
        let herda = v_flex()
            .gap(px(tema::ESP_1))
            .child(titulo(t::DETALHE_HERDA))
            .child(if r.herda_do_global.is_empty() {
                nenhuma()
            } else {
                div().text_size(px(tema::TAM_LEGENDA)).child(format!(
                    "{}: {}",
                    t::herda_globais_lista(r.herda_do_global.len()),
                    r.herda_do_global
                        .iter()
                        .map(|s| t::setting_legivel(s))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            });
        let faca_no_jogo = (!j.orientacoes.is_empty()).then(|| {
            v_flex()
                .gap(px(tema::ESP_1))
                .child(titulo(t::DETALHE_FACA_NO_JOGO))
                .child(
                    div()
                        .text_size(px(tema::TAM_LEGENDA))
                        .text_color(tema.muted_foreground)
                        .child(t::FACA_NO_JOGO_EXPLICACAO),
                )
                .children(j.orientacoes.iter().map(|o| {
                    div()
                        .text_size(px(tema::TAM_CORPO))
                        .pl(px(tema::ESP_3))
                        .child(format!("• {}", t::orientacao_ou_codigo(o)))
                }))
        });
        v_flex()
            .gap(px(tema::ESP_3))
            .ml(px(tema::ESP_6))
            .mr(px(tema::ESP_3))
            .mb(px(tema::ESP_3))
            .p(px(tema::ESP_3))
            .rounded(px(tema::RAIO_CARTAO))
            .border_1()
            .border_color(tema.border)
            .bg(tema.background)
            .child(aplicadas)
            .child(nao_aplicadas)
            .child(herda)
            .children(faca_no_jogo)
            .children(self.arquivo_do_jogo(i, j, tema, cx))
    }

    /// “Arquivo do jogo” (Fase 3): caminho anonimizado, antes/depois, backup,
    /// Reverter, orientações e sugestões experimentais do jogo.
    fn arquivo_do_jogo(
        &self,
        i: usize,
        j: &Jogo,
        tema: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let sugestoes = aj::sugestoes_do_jogo(j);
        if j.config_jogo.is_none() && sugestoes.is_empty() {
            return None;
        }
        let pode = self.ui.pode_escrever();
        let sub = |s: String| {
            div()
                .text_size(px(tema::TAM_LEGENDA))
                .text_color(tema.muted_foreground)
                .child(s)
        };
        let mut v = v_flex().gap(px(tema::ESP_2)).child(
            div()
                .text_size(px(tema::TAM_LEGENDA))
                .font_semibold()
                .text_color(tema.muted_foreground)
                .child(aj::SECAO_ARQUIVO_JOGO),
        );
        if let Some(c) = &j.config_jogo {
            for (k, a) in c.arquivos.iter().enumerate() {
                let mut bloco = v_flex()
                    .gap(px(tema::ESP_1))
                    .child(
                        div()
                            .text_size(px(tema::TAM_CORPO))
                            .font_semibold()
                            .child(aj::nome_arquivo(&a.rotulo.0).to_owned()),
                    )
                    .child(sub(a.rotulo.0.clone()))
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
                    let destino = (m.status == nvt_ipc::StatusMudanca::Aplicada)
                        .then(|| aj::destino_reverter(a, &m.setting))
                        .flatten();
                    let mut linha = h_flex().gap(px(tema::ESP_3)).justify_between().child(
                        div()
                            .text_size(px(tema::TAM_CORPO))
                            .pl(px(tema::ESP_3))
                            .flex_1()
                            .min_w_0()
                            .child(format!("• {}", aj::linha_mudanca(m))),
                    );
                    if let Some(destino) = destino {
                        let ok = pode;
                        linha = linha.child(
                            Button::new(("reverter-jogo", i * 10_000 + k * 100 + n))
                                .small()
                                .outline()
                                .label(t::BOTAO_REVERTER)
                                .disabled(!ok)
                                .when(!ok, |b| b.tooltip(t::SO_COM_SERVICO))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.enviar(
                                        Comando::ReverterConfigJogo {
                                            destino: destino.clone(),
                                        },
                                        cx,
                                    )
                                })),
                        );
                    }
                    bloco = bloco.child(linha);
                }
                v = v.child(bloco);
            }
            v = v.children(aj::orientacoes(j).into_iter().map(|o| {
                div()
                    .text_size(px(tema::TAM_CORPO))
                    .font_semibold()
                    .child(o)
            }));
        }
        if !sugestoes.is_empty() {
            v = v
                .child(sub(aj::SECAO_SUGESTOES_JOGO.into()))
                .child(self.linhas_experimentais("jogo", &sugestoes, tema, cx));
        }
        Some(v)
    }
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
        .gap(px(tema::ESP_1))
        .pl(px(tema::ESP_3))
        .border_l_2()
        .border_color(cor)
        .child(
            h_flex()
                .gap(px(tema::ESP_2))
                .child(tag.small().child(t::nivel_aviso(a.nivel)))
                .child(
                    div()
                        .text_size(px(tema::TAM_LEGENDA))
                        .text_color(tema.muted_foreground)
                        .child(formatar_data(a.quando_unix)),
                ),
        )
        .child(
            div()
                .text_size(px(tema::TAM_CORPO))
                .text_color(cor)
                .child(t::mensagem_aviso(a).to_owned()),
        )
        .when_some(a.detalhe.clone(), |d, detalhe| {
            d.child(
                div()
                    .text_size(px(tema::TAM_LEGENDA))
                    .text_color(tema.muted_foreground)
                    .child(detalhe),
            )
        })
}

fn cartao_ignorados(ignorados: &[JogoIgnorado], tema: &Theme) -> Div {
    cartao(
        t::SECAO_IGNORADOS,
        v_flex()
            .gap(px(tema::ESP_2))
            .child(explicacao(t::IGNORADOS_EXPLICACAO, tema))
            .children(ignorados.iter().map(|j| {
                v_flex()
                    .child(
                        div()
                            .text_size(px(tema::TAM_CORPO))
                            .font_semibold()
                            .text_color(tema.warning)
                            .child(j.nome.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(tema::TAM_LEGENDA))
                            .text_color(tema.muted_foreground)
                            .child(if j.exe.is_empty() {
                                j.motivo.clone()
                            } else {
                                format!("{} · {}", j.exe, j.motivo)
                            }),
                    )
            })),
        tema,
    )
}
