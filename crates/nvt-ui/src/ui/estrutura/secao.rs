//! Seções da navegação lateral.
//!
//! Para adicionar uma seção: uma variante em [`Secao`], uma const
//! [`DefSecao`], o braço em [`Secao::def`] (o compilador cobra), a entrada em
//! [`SECOES`] na posição da lateral (com o mesmo `cfg` da variante, se houver)
//! e o módulo da tela.

use super::*;

/// Desenha o conteúdo de uma seção.
pub(super) type RenderSecao = fn(&Janela, Pixels, &mut Window, &mut Context<Janela>) -> AnyElement;

/// Definição de uma seção: tudo o que a estrutura comum precisa saber dela.
pub(super) struct DefSecao {
    pub(super) secao: Secao,
    /// Chave de `--tela`.
    pub(super) chave: &'static str,
    pub(super) titulo: &'static str,
    /// Texto ao lado do título na barra superior.
    pub(super) descricao: Option<&'static str>,
    pub(super) icone: IconName,
    /// Repete o título no topo da área de conteúdo.
    pub(super) titulo_na_pagina: bool,
    pub(super) render: RenderSecao,
}

const INICIO: DefSecao = DefSecao {
    secao: Secao::Inicio,
    chave: "inicio",
    titulo: t::nav::INICIO,
    descricao: Some(t::nav::DESCRICAO_INICIO),
    icone: IconName::House,
    titulo_na_pagina: false,
    render: |janela, largura, _, cx| inicio::render(janela, largura, cx),
};

const JOGOS: DefSecao = DefSecao {
    secao: Secao::Jogos,
    chave: "jogos",
    titulo: t::nav::JOGOS,
    descricao: Some(t::nav::DESCRICAO_JOGOS),
    icone: IconName::Gamepad2,
    titulo_na_pagina: false,
    render: jogos::render,
};

#[cfg(feature = "medicao-partidas")]
const PARTIDAS: DefSecao = DefSecao {
    secao: Secao::Partidas,
    chave: "partidas",
    titulo: t::nav::PARTIDAS,
    descricao: None,
    icone: IconName::ChartLine,
    titulo_na_pagina: true,
    render: |janela, _, _, cx| partidas::render(janela, cx),
};

/// Seções na ordem da lateral (R-UI-01); a primeira é a inicial.
pub(super) const SECOES: &[DefSecao] = &[
    INICIO,
    JOGOS,
    #[cfg(feature = "medicao-partidas")]
    PARTIDAS,
];

/// As seções de [`SECOES`], na mesma ordem.
const VISIVEIS: [Secao; SECOES.len()] = {
    let mut v = [Secao::Inicio; SECOES.len()];
    let mut i = 0;
    while i < SECOES.len() {
        v[i] = SECOES[i].secao;
        i += 1;
    }
    v
};

/// Item da navegação lateral.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Secao {
    #[default]
    Inicio,
    Jogos,
    #[cfg(feature = "medicao-partidas")]
    Partidas,
}

impl Secao {
    /// Itens visíveis na lateral, na ordem (R-UI-01).
    pub fn visiveis() -> &'static [Secao] {
        &VISIVEIS
    }

    /// Definição desta seção. O `match` é exaustivo: uma variante nova sem
    /// definição não compila.
    pub(super) fn def(self) -> &'static DefSecao {
        match self {
            Secao::Inicio => &INICIO,
            Secao::Jogos => &JOGOS,
            #[cfg(feature = "medicao-partidas")]
            Secao::Partidas => &PARTIDAS,
        }
    }

    pub fn titulo(self) -> &'static str {
        self.def().titulo
    }

    pub fn icone(self) -> IconName {
        self.def().icone
    }

    /// Chave de `--tela`.
    pub fn chave(self) -> &'static str {
        self.def().chave
    }

    /// Interpreta `--tela` usando as seções disponíveis na navegação.
    pub fn de_chave(s: &str) -> Option<Secao> {
        SECOES.iter().find(|d| d.chave == s).map(|d| d.secao)
    }
}
