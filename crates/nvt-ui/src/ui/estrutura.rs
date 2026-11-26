//! Estrutura comum da janela (spec `interface-e-saude.md` §5.1): navegação
//! lateral mínima, linha de estado única no topo e título da página.
//!
//! Também guarda a [`Janela`] (estado, envio de comandos e desenho comum) e
//! as peças visuais compartilhadas pelas seções.

use std::collections::BTreeSet;
use std::sync::mpsc::Sender;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::Theme;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::sidebar::{
    Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem,
};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::modelo::{Comando, Conexao, EstadoUi, Evento, Tela, TipoMensagem};
use crate::servico::Canal;
use crate::tema;
use crate::textos as t;

const NAV_INICIO: &str = "Início";
const NAV_JOGOS: &str = "Jogos";
const NAV_AVANCADO: &str = "Avançado";
#[cfg(feature = "medicao-partidas")]
const NAV_PARTIDAS: &str = "Partidas";
#[cfg(feature = "medicao-partidas")]
const PARTIDAS_VAZIO: &str = "Nenhuma partida gravada ainda.";
#[cfg(feature = "medicao-partidas")]
const BUILD_DE_TESTE: &str = "Build de teste";
const VERSAO: &str = concat!("versão ", env!("CARGO_PKG_VERSION"));

/// Item da navegação lateral.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Secao {
    Inicio,
    Jogos,
    Avancado,
    #[cfg(feature = "medicao-partidas")]
    Partidas,
}

impl Secao {
    /// Itens visíveis na lateral, na ordem (R-UI-01).
    pub fn visiveis() -> &'static [Secao] {
        #[cfg(not(feature = "medicao-partidas"))]
        {
            &[Secao::Inicio, Secao::Jogos, Secao::Avancado]
        }
        #[cfg(feature = "medicao-partidas")]
        {
            &[
                Secao::Inicio,
                Secao::Jogos,
                Secao::Partidas,
                Secao::Avancado,
            ]
        }
    }

    pub fn titulo(self) -> &'static str {
        match self {
            Secao::Inicio => NAV_INICIO,
            Secao::Jogos => NAV_JOGOS,
            Secao::Avancado => NAV_AVANCADO,
            #[cfg(feature = "medicao-partidas")]
            Secao::Partidas => NAV_PARTIDAS,
        }
    }

    pub fn icone(self) -> IconName {
        match self {
            Secao::Inicio => IconName::House,
            Secao::Jogos => IconName::Gamepad2,
            Secao::Avancado => IconName::SlidersHorizontal,
            #[cfg(feature = "medicao-partidas")]
            Secao::Partidas => IconName::ChartLine,
        }
    }

    /// Chave de `--tela`.
    pub fn chave(self) -> &'static str {
        match self {
            Secao::Inicio => "inicio",
            Secao::Jogos => "jogos",
            Secao::Avancado => "avancado",
            #[cfg(feature = "medicao-partidas")]
            Secao::Partidas => "partidas",
        }
    }

    /// Tela legada mostrada ao entrar na seção.
    pub fn tela_inicial(self) -> Tela {
        match self {
            Secao::Jogos => Tela::Jogos,
            _ => Tela::VisaoGeral,
        }
    }
}

/// Telas legadas agrupadas no Avançado (as demais têm seção própria).
pub const TELAS_AVANCADO: [Tela; 6] = [
    Tela::VisaoGeral,
    Tela::Mudancas,
    Tela::Windows,
    Tela::Preferencias,
    Tela::Historico,
    Tela::Restaurar,
];

/// Onde a janela abre: seção da lateral e tela legada dentro dela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Destino {
    pub secao: Secao,
    pub tela: Tela,
}

impl Default for Destino {
    fn default() -> Self {
        Destino::de_secao(Secao::Inicio)
    }
}

impl Destino {
    pub fn de_secao(secao: Secao) -> Self {
        Destino {
            secao,
            tela: secao.tela_inicial(),
        }
    }

    /// Interpreta `--tela`: chaves das seções novas ou das telas legadas
    /// (estas abrem na seção que as contém; `jogos` é a seção Jogos).
    pub fn de_chave(s: &str) -> Option<Destino> {
        if let Some(secao) = Secao::visiveis().iter().copied().find(|x| x.chave() == s) {
            return Some(Destino::de_secao(secao));
        }
        let tela = Tela::de_chave(s)?;
        let secao = if TELAS_AVANCADO.contains(&tela) {
            Secao::Avancado
        } else {
            Secao::Jogos
        };
        Some(Destino { secao, tela })
    }
}

impl Janela {
    pub(super) fn ir_secao(&mut self, secao: Secao, cx: &mut Context<Self>) {
        self.secao = secao;
        self.ui.tela = secao.tela_inicial();
        cx.notify();
    }

    pub(super) fn barra_lateral(
        &self,
        rodape: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let itens = Secao::visiveis().iter().map(|&secao| {
            SidebarMenuItem::new(secao.titulo())
                .icon(secao.icone())
                .active(self.secao == secao)
                .on_click(cx.listener(move |this, _, _, cx| this.ir_secao(secao, cx)))
        });
        let tema = cx.theme();
        let marca = h_flex()
            .gap(px(tema::ESP_2))
            .items_center()
            .child(
                div()
                    .size(px(tema::ESP_3))
                    .rounded(px(tema::ESP_1))
                    .bg(tema::cor(tema::DESTAQUE)),
            )
            .child(
                div()
                    .text_size(px(tema::TAM_TITULO_CARTAO + 3.))
                    .font_semibold()
                    .child(crate::textos::TITULO_JANELA),
            );
        let versao = div()
            .text_size(px(tema::TAM_LEGENDA))
            .text_color(tema.muted_foreground)
            .child(VERSAO);
        #[cfg(feature = "medicao-partidas")]
        let versao = versao.child(format!(" · {BUILD_DE_TESTE}"));
        Sidebar::new("navegacao")
            .w(px(tema::LARGURA_LATERAL))
            .header(SidebarHeader::new().child(marca))
            .child(SidebarGroup::new("").child(SidebarMenu::new().children(itens)))
            .footer(
                SidebarFooter::new()
                    .child(v_flex().gap(px(tema::ESP_1)).child(rodape).child(versao)),
            )
    }

    /// Linha de estado única no topo do conteúdo (R-UI-05). O texto vem do
    /// modelo; sem texto, não ocupa espaço.
    pub(super) fn linha_estado(
        &self,
        texto: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tema = cx.theme();
        div().when_some(texto, |d, texto| {
            d.child(
                h_flex()
                    .id("linha-estado")
                    .gap(px(tema::ESP_2))
                    .items_center()
                    .px(px(tema::ESP_8))
                    .py(px(tema::ESP_2))
                    .border_b_1()
                    .border_color(tema.border)
                    .bg(tema.group_box)
                    .text_size(px(tema::TAM_CORPO))
                    .child(
                        div()
                            .w(px(3.))
                            .h(px(tema::TAM_CORPO + tema::ESP_1))
                            .rounded(px(2.))
                            .bg(tema::cor(tema::DESTAQUE)),
                    )
                    .child(texto),
            )
        })
    }

    /// Título da página (28 px).
    pub(super) fn titulo_pagina(&self) -> impl IntoElement {
        let titulo = self.secao.titulo();
        div()
            .text_size(px(tema::TAM_TITULO_PAGINA))
            .font_semibold()
            .child(titulo)
    }

    /// Conteúdo da seção ativa.
    pub(super) fn conteudo_secao(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.secao {
            Secao::Inicio => super::inicio::render(self, cx),
            Secao::Jogos => super::jogos::render(self, cx),
            Secao::Avancado => super::avancado::render(self, cx),
            #[cfg(feature = "medicao-partidas")]
            Secao::Partidas => div()
                .text_color(cx.theme().muted_foreground)
                .child(PARTIDAS_VAZIO)
                .into_any_element(),
        }
    }
}

// ---------------------------------------------------------------------------
// Janela
// ---------------------------------------------------------------------------

/// Janela principal. Só lê [`EstadoUi`] e envia [`Comando`]s para a thread
/// de comunicação; a thread da janela nunca bloqueia em E/S.
pub struct Janela {
    pub(super) ui: EstadoUi,
    /// Item ativo da navegação lateral.
    pub(super) secao: Secao,
    comandos: Sender<Comando>,
    /// Perfis dos jogos com a linha expandida na tela Jogos.
    pub(super) expandidos: BTreeSet<String>,
}

impl Janela {
    pub fn new(
        destino: Destino,
        expandidos: Vec<String>,
        canal: Canal,
        cx: &mut Context<Self>,
    ) -> Self {
        let eventos = canal.eventos;
        super::jogos::instalar_envio(cx, canal.comandos.clone());
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
            ui: EstadoUi::new(destino.tela),
            secao: destino.secao,
            comandos: canal.comandos,
            expandidos: expandidos.into_iter().collect(),
        }
    }

    pub(super) fn enviar(&mut self, c: Comando, cx: &mut Context<Self>) {
        if !self.ui.pode_executar(&c) {
            return;
        }
        self.ui.aplicar(Evento::Ocupado(c.clone()));
        let _ = self.comandos.send(c);
        cx.notify();
    }

    pub(super) fn selo_conexao(&self) -> impl IntoElement {
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
}

impl Render for Janela {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tema = cx.theme().clone();
        let agente = self
            .ui
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
            });
        let rodape = v_flex().gap_1().child(self.selo_conexao()).child(
            div()
                .text_xs()
                .text_color(tema.muted_foreground)
                .child(agente),
        );
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
                .items_center()
                .justify_center()
                .child(super::boas_vindas::render(self, cx))
                .into_any_element();
        }
        base()
            .flex()
            .flex_row()
            .child(self.barra_lateral(rodape, cx))
            .child(
                v_flex()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    // O texto da linha de estado virá do modelo (`linha_estado`).
                    .child(self.linha_estado(None, cx))
                    .child(
                        div()
                            .id("conteudo")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(
                                v_flex()
                                    .px(px(tema::ESP_8))
                                    .py(px(tema::ESP_6))
                                    .gap(px(tema::ESP_4))
                                    .child(self.titulo_pagina())
                                    .when(self.ui.restauracao_usuario_pendente(), |v| {
                                        v.child(explicacao(t::RESTAURACAO_USUARIO_PENDENTE, &tema))
                                    })
                                    .child(self.avisos(cx))
                                    .child(self.conteudo_secao(cx)),
                            ),
                    ),
            )
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// Peças
// ---------------------------------------------------------------------------

pub(super) fn cartao(titulo: &'static str, corpo: impl IntoElement, tema: &Theme) -> Div {
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

pub(super) fn linha(rotulo: &'static str, valor: String, tema: &Theme) -> Div {
    h_flex()
        .gap_4()
        .justify_between()
        .text_sm()
        .child(div().text_color(tema.muted_foreground).child(rotulo))
        .child(div().text_right().child(valor))
}

pub(super) fn explicacao(texto: &'static str, tema: &Theme) -> Div {
    div()
        .text_sm()
        .text_color(tema.muted_foreground)
        .child(texto)
}

/// Instante atual (Unix, segundos) para os tempos relativos da atualização.
pub(super) fn agora_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod testes {
    use std::collections::HashSet;

    use super::{Destino, Secao, TELAS_AVANCADO, Tela};

    #[cfg(not(feature = "medicao-partidas"))]
    #[test]
    fn tres_itens_no_build_normal() {
        assert_eq!(
            Secao::visiveis(),
            [Secao::Inicio, Secao::Jogos, Secao::Avancado]
        );
        let titulos: Vec<_> = Secao::visiveis().iter().map(|s| s.titulo()).collect();
        assert_eq!(titulos, ["Início", "Jogos", "Avançado"]);
    }

    #[cfg(feature = "medicao-partidas")]
    #[test]
    fn partidas_no_build_de_teste() {
        assert_eq!(Secao::visiveis().len(), 4);
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
    fn toda_tela_legada_alcancavel() {
        for tela in Tela::TODAS {
            let d = Destino::de_chave(tela.chave()).expect("chave legada");
            assert_eq!(d.tela, tela);
            assert!(Secao::visiveis().contains(&d.secao));
            assert!(
                TELAS_AVANCADO.contains(&tela) || d.secao.tela_inicial() == tela,
                "{tela:?} sem caminho pela navegação"
            );
        }
    }

    #[test]
    fn chaves_das_secoes() {
        for &s in Secao::visiveis() {
            assert_eq!(Destino::de_chave(s.chave()), Some(Destino::de_secao(s)));
        }
        assert_eq!(
            Destino::de_chave("mudancas"),
            Some(Destino {
                secao: Secao::Avancado,
                tela: Tela::Mudancas
            })
        );
        assert_eq!(Destino::de_chave("nada"), None);
        assert_eq!(Destino::default().secao, Secao::Inicio);
    }
}
