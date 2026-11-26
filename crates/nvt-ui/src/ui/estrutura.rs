//! Estrutura comum da janela (spec `interface-e-saude.md` §5.1): navegação
//! lateral mínima, linha de estado única no topo e título da página.
//!
//! Até o L2, cada seção mostra as telas legadas ([`super::legado`]); o
//! Avançado agrupa as que não têm seção própria.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::sidebar::{
    Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem,
};
use gpui_kit::component::{ActiveTheme, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::legado::Janela;
use crate::modelo::Tela;
use crate::tema;

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

/// Ícone de cada tela legada na subnavegação do Avançado (todos distintos).
pub fn icone_legado(tela: Tela) -> IconName {
    match tela {
        Tela::VisaoGeral => IconName::LayoutDashboard,
        Tela::Jogos => IconName::Gamepad2,
        Tela::Mudancas => IconName::Replace,
        Tela::Windows => IconName::Monitor,
        Tela::Preferencias => IconName::Settings2,
        Tela::Historico => IconName::Calendar,
        Tela::Restaurar => IconName::Undo2,
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
        let titulo = if self.secao == Secao::Avancado {
            format!("{} · {}", NAV_AVANCADO, self.ui.tela.titulo())
        } else {
            self.secao.titulo().to_owned()
        };
        div()
            .text_size(px(tema::TAM_TITULO_PAGINA))
            .font_semibold()
            .child(titulo)
    }

    /// Subnavegação do Avançado entre as telas legadas.
    pub(super) fn subnavegacao_avancado(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_wrap()
            .gap(px(tema::ESP_2))
            .children(TELAS_AVANCADO.map(|tela| {
                let b = Button::new(SharedString::from(format!("avancado-{}", tela.chave())))
                    .small()
                    .icon(icone_legado(tela))
                    .label(tela.titulo())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.ui.tela = tela;
                        cx.notify();
                    }));
                if self.ui.tela == tela {
                    b.primary()
                } else {
                    b.ghost()
                }
            }))
    }

    /// Conteúdo da seção ativa.
    pub(super) fn conteudo_secao(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.secao {
            Secao::Inicio | Secao::Jogos => self.conteudo(cx),
            Secao::Avancado => v_flex()
                .gap(px(tema::ESP_4))
                .child(self.subnavegacao_avancado(cx))
                .child(self.conteudo(cx))
                .into_any_element(),
            #[cfg(feature = "medicao-partidas")]
            Secao::Partidas => div()
                .text_color(cx.theme().muted_foreground)
                .child(PARTIDAS_VAZIO)
                .into_any_element(),
        }
    }
}

#[cfg(test)]
mod testes {
    use std::collections::HashSet;

    use super::{Destino, Secao, TELAS_AVANCADO, Tela, icone_legado};

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
        let legados: HashSet<_> = Tela::TODAS
            .iter()
            .map(|&t| format!("{:?}", icone_legado(t)))
            .collect();
        assert_eq!(legados.len(), Tela::TODAS.len());
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
