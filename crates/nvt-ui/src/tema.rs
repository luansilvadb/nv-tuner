//! Tema visual: escuro fixo, destaque ciano do logo e Segoe UI Variable
//! (R-UI-10), com tokens de espaçamento e de tipografia (R-UI-11).
//!
//! As cores ficam aqui como `u32` (0xRRGGBB) para os testes calcularem o
//! contraste sem depender do GPUI; [`aplicar`] as copia para o tema global
//! do gpui-component.

use gpui_kit::component::button::Button;
use gpui_kit::component::{Disableable, Sizable, Theme, ThemeMode};
use gpui_kit::*;

// ---------------------------------------------------------------------------
// Cores
// ---------------------------------------------------------------------------

/// Ciano do logo (`assets/logo.svg`).
pub const DESTAQUE: u32 = 0x38C8F0;
pub const DESTAQUE_HOVER: u32 = 0x5CD3F3;
pub const DESTAQUE_ATIVO: u32 = 0x22AFD8;
/// Texto sobre o destaque (botão primário, item ativo).
pub const TEXTO_SOBRE_DESTAQUE: u32 = 0x08161D;

/// Fundo da área de conteúdo.
pub const FUNDO: u32 = 0x0B0B0E;
/// Fundo da navegação lateral (o quadrado escuro do logo).
pub const LATERAL: u32 = 0x15171B;
/// Cartões e grupos (Material 3: *surface container*). A hierarquia entre
/// níveis vem do tom de cada superfície, não de bordas.
pub const SUPERFICIE: u32 = 0x16171E;
/// Controles e cartões aninhados (*surface container high*).
pub const SUPERFICIE_ALTA: u32 = 0x1D1F28;
/// Campos dentro de um cartão aninhado (*surface container highest*).
pub const SUPERFICIE_MAIS_ALTA: u32 = 0x262833;
/// Linha sob o cursor na lateral e em listas.
pub const SUPERFICIE_HOVER: u32 = 0x202127;
/// Borda (a moldura do logo).
pub const BORDA: u32 = 0x282A34;
pub const BORDA_SUAVE: u32 = 0x22242C;
pub const BORDA_LATERAL: u32 = 0x1C1F26;

pub const TEXTO: u32 = 0xE8ECF1;
pub const TEXTO_SECUNDARIO: u32 = 0xA0A2AE;

// ---------------------------------------------------------------------------
// Fonte
// ---------------------------------------------------------------------------

/// Fonte preferida (Windows 11).
pub const FONTE: &str = "Segoe UI Variable";
/// Nomes da família variável como o DirectWrite pode listá-la.
const FONTE_VARIANTES: [&str; 2] = [FONTE, "Segoe UI Variable Text"];
/// Queda (Windows 10).
pub const FONTE_QUEDA: &str = "Segoe UI";

/// Escolhe a família entre as instaladas: Segoe UI Variable, senão Segoe UI.
pub fn escolher_fonte<S: AsRef<str>>(instaladas: &[S]) -> &'static str {
    FONTE_VARIANTES
        .into_iter()
        .find(|f| {
            instaladas
                .iter()
                .any(|i| i.as_ref().eq_ignore_ascii_case(f))
        })
        .unwrap_or(FONTE_QUEDA)
}

// ---------------------------------------------------------------------------
// Tipografia (px) e espaçamento (px)
// ---------------------------------------------------------------------------

pub const TAM_TITULO_PAGINA: f32 = 28.;
/// Subtítulo (veredito, seções do Avançado, marca). Escala do Windows 11:
/// 28 / 20 / 14 / 12, nada fora dela.
pub const TAM_SUBTITULO: f32 = 20.;
pub const TAM_CORPO: f32 = 14.;
pub const TAM_LEGENDA: f32 = 12.;

/// Espaço mínimo entre elementos de uma mesma linha.
pub const ESP_1: f32 = 4.;
/// Espaço entre linhas de um grupo.
pub const ESP_2: f32 = 8.;
/// Espaço interno de linha de estado e de cartão.
pub const ESP_3: f32 = 12.;
/// Espaço entre grupos.
pub const ESP_4: f32 = 16.;
/// Margem da página.
pub const ESP_6: f32 = 24.;
/// Margem lateral do conteúdo.
pub const ESP_8: f32 = 32.;
/// Respiro grande (boas-vindas, estados vazios).
pub const ESP_12: f32 = 48.;

pub const RAIO: f32 = 6.;
/// Canto de todo cartão e lista (um só valor em todas as telas).
pub const RAIO_CARTAO: f32 = 12.;
pub const RAIO_AREA_CONTEUDO: f32 = 18.;
pub const FUNDO_AREA_CONTEUDO: u32 = 0x0E0F13;
/// Logo nas boas-vindas.
pub const LOGO_BOAS_VINDAS: f32 = 96.;
/// Círculo do ícone do veredito e dos estados vazios.
pub const ICONE_DESTAQUE: f32 = 56.;
pub const ICONE_DESTAQUE_INTERNO: f32 = 28.;
/// Largura máxima do conteúdo e dos parágrafos centrados.
pub const LARGURA_CONTEUDO: f32 = 880.;
pub const LARGURA_TEXTO: f32 = 520.;
/// Largura mínima do botão principal das boas-vindas.
pub const LARGURA_BOTAO_DESTAQUE: f32 = 220.;
/// Altura dos botões de ação principal (alvo confortável) e do "Começar".
pub const ALTURA_BOTAO: f32 = 36.;
pub const ALTURA_BOTAO_DESTAQUE: f32 = 44.;
/// Colunas das tabelas técnicas (rótulo, valor curto, valor, texto longo).
pub const COLUNA_ROTULO: f32 = 160.;
pub const COLUNA_CURTA: f32 = 96.;
pub const COLUNA_VALOR: f32 = 124.;
pub const COLUNA_LARGA: f32 = 300.;
pub const ALTURA_BARRA: f32 = 36.;
/// Barra de busca do Material 3 (pílula).
pub const ALTURA_BUSCA: f32 = 48.;
/// Segmented button do Material 3.
pub const ALTURA_SEGMENTADO: f32 = 40.;
pub const ALTURA_SEGMENTADO_COMPACTO: f32 = 32.;
const ALTURA_BOTAO_MATERIAL: f32 = 40.;
pub const TAM_BOTAO_ICONE: f32 = ALTURA_BOTAO_MATERIAL;
const ALTURA_BOTAO_COMPACTO: f32 = 32.;
const ESP_BOTAO_MATERIAL: f32 = 24.;
pub const TAM_VALOR_PAINEL: f32 = 24.;
pub const TAM_PONTO_STATUS: f32 = 6.;
pub const TAM_CAPA_JOGO: f32 = 44.;
pub const ALTURA_CARTAO_PAINEL: f32 = 136.;
pub const ALTURA_PAINEL: f32 = 106.;
pub const RAIO_PAINEL: f32 = 12.;
pub const RAIO_CARTAO_PAINEL: f32 = 20.;
pub const ESP_GRADE: f32 = 14.;
pub const LINHA_FINA: f32 = 1.;
pub const ALTURA_ITEM_MENU: f32 = 56.;
pub const FUNDO_ITEM_MENU_ATIVO: u32 = 0x364553;
pub const FUNDO_ITEM_MENU_ATIVO_HOVER: u32 = 0x415261;
pub const FUNDO_ITEM_MENU_PRESSIONADO: u32 = 0x3A4C5E;
pub const BORDA_FOCO_MENU: u32 = 0x7893AA;
pub const COR_RIPPLE_MENU: u32 = 0x7998B5;
pub const TEXTO_ITEM_MENU_ATIVO: u32 = 0xE4EFFA;
pub const LARGURA_GRADE_TRES: f32 = 830.;
pub const LARGURA_GRADE_DOIS: f32 = 540.;
pub const VERDE_PAINEL: u32 = 0x54D5AA;
pub const FUNDO_PAINEL: u32 = 0x161825;
pub const FUNDO_CONTROLE: u32 = SUPERFICIE_ALTA;
/// Chips (Material 3): canto de 8 px, altura compacta.
pub const RAIO_CHIP: f32 = 8.;
pub const ALTURA_CHIP: f32 = 24.;
/// Cartão dentro de outro cartão.
pub const RAIO_CARTAO_INTERNO: f32 = 16.;
pub const BORDA_FOCO: u32 = 0x817FD1;
pub const FUNDO_ICONE: u32 = 0x1D2031;
pub const TEXTO_ICONE: u32 = 0xA9ABFF;
pub const ACAO_VIOLETA: u32 = 0x595BC5;

pub const LARGURA_LATERAL: f32 = 268.;
pub const LARGURA_LATERAL_RECOLHIDA: f32 = 80.;
pub const TAM_RELOGIO_LATERAL: f32 = 46.;
pub const ALTURA_RELOGIO_LATERAL: f32 = 168.;
pub const ESP_APOS_RELOGIO: f32 = 48.;
pub const LARGURA_JANELA: f32 = 1280.;
pub const ALTURA_JANELA: f32 = 900.;

pub fn botao_base(id: impl Into<ElementId>) -> Button {
    Button::new(id).cursor_pointer()
}

pub trait EstadoBotao {
    fn indisponivel(self, indisponivel: bool) -> Self;
}

impl EstadoBotao for Button {
    fn indisponivel(self, indisponivel: bool) -> Self {
        let botao = self.disabled(indisponivel);
        if indisponivel {
            botao.cursor_default()
        } else {
            botao
        }
    }
}

/// Botão de ação com a geometria do Material Design 3.
pub fn botao(id: impl Into<ElementId>) -> Button {
    botao_base(id)
        .h(px(ALTURA_BOTAO_MATERIAL))
        .px(px(ESP_BOTAO_MATERIAL))
        .rounded(px(ALTURA_BOTAO_MATERIAL / 2.))
}

/// Botão de ícone do Material Design 3 com área circular e glifo central.
pub fn botao_icone(id: impl Into<ElementId>, icone: impl IntoElement) -> Button {
    botao_base(id)
        .size(px(TAM_BOTAO_ICONE))
        .px(px(0.))
        .rounded(px(ALTURA_BOTAO_MATERIAL / 2.))
        .child(icone)
}

/// Variante para ações auxiliares em grupos compactos.
pub fn botao_compacto(id: impl Into<ElementId>) -> Button {
    botao_base(id)
        .small()
        .h(px(ALTURA_BOTAO_COMPACTO))
        .rounded(px(ALTURA_BOTAO_COMPACTO / 2.))
}

/// Cor GPUI a partir de 0xRRGGBB.
pub fn cor(c: u32) -> Hsla {
    rgb(c).into()
}

/// Liga o tema escuro e copia os tokens para o tema global.
pub fn aplicar(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    let fonte = escolher_fonte(&cx.text_system().all_font_names());
    Theme::update(cx, |t| {
        t.font_family = fonte.into();
        t.font_size = px(TAM_CORPO);
        t.radius = px(RAIO);
        t.background = cor(FUNDO);
        t.foreground = cor(TEXTO);
        t.muted_foreground = cor(TEXTO_SECUNDARIO);
        t.border = cor(BORDA);
        t.group_box = cor(SUPERFICIE);
        t.popover = cor(SUPERFICIE);
        t.list_hover = cor(SUPERFICIE_HOVER);
        t.primary = cor(DESTAQUE);
        t.primary_hover = cor(DESTAQUE_HOVER);
        t.primary_active = cor(DESTAQUE_ATIVO);
        t.primary_foreground = cor(TEXTO_SOBRE_DESTAQUE);
        // O botão primário do gpui-component lê os campos próprios.
        t.button_primary = cor(ACAO_VIOLETA);
        t.button_primary_hover = cor(0x6264CD);
        t.button_primary_active = cor(0x5052B6);
        t.button_primary_foreground = cor(0xFFFFFF);
        t.ring = cor(DESTAQUE);
        t.switch_thumb = cor(TEXTO);
        t.link = cor(DESTAQUE);
        t.sidebar = cor(LATERAL);
        t.title_bar = cor(FUNDO);
        t.title_bar_border = cor(BORDA_SUAVE);
        t.sidebar_foreground = cor(TEXTO);
        t.sidebar_border = cor(BORDA_LATERAL);
        t.sidebar_accent = cor(SUPERFICIE_HOVER);
        t.sidebar_accent_foreground = cor(TEXTO);
        t.sidebar_primary = cor(DESTAQUE);
        t.sidebar_primary_foreground = cor(TEXTO_SOBRE_DESTAQUE);
    });
}

// ---------------------------------------------------------------------------
// Contraste (WCAG 2.x): verificação dos tokens pelos testes
// ---------------------------------------------------------------------------

#[cfg(test)]
fn canal_linear(c: u8) -> f64 {
    let c = f64::from(c) / 255.;
    if c <= 0.039_28 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
/// Luminância relativa de 0xRRGGBB.
pub fn luminancia(c: u32) -> f64 {
    let [_, r, g, b] = c.to_be_bytes();
    0.2126 * canal_linear(r) + 0.7152 * canal_linear(g) + 0.0722 * canal_linear(b)
}

#[cfg(test)]
/// Razão de contraste entre duas cores (1 a 21).
pub fn contraste(a: u32, b: u32) -> f64 {
    let (la, lb) = (luminancia(a), luminancia(b));
    let (claro, escuro) = if la > lb { (la, lb) } else { (lb, la) };
    (claro + 0.05) / (escuro + 0.05)
}

#[cfg(test)]
mod testes {
    use super::{
        DESTAQUE, DESTAQUE_ATIVO, DESTAQUE_HOVER, ESP_1, ESP_2, ESP_3, ESP_4, ESP_6, ESP_8, ESP_12,
        FONTE, FONTE_QUEDA, FUNDO, LATERAL, SUPERFICIE, SUPERFICIE_HOVER, TAM_CORPO, TAM_LEGENDA,
        TAM_SUBTITULO, TAM_TITULO_PAGINA, TEXTO, TEXTO_SECUNDARIO, TEXTO_SOBRE_DESTAQUE, contraste,
        escolher_fonte, luminancia,
    };

    const MINIMO: f64 = 4.5;

    #[test]
    fn destaque_e_o_ciano_do_logo() {
        assert_eq!(DESTAQUE, 0x38C8F0);
        let logo = include_str!("../../../assets/logo.svg");
        assert!(logo.to_ascii_uppercase().contains("#38C8F0"));
    }

    #[test]
    fn tema_e_escuro() {
        assert!(luminancia(FUNDO) < 0.05);
        assert!(luminancia(LATERAL) < 0.05);
        assert!(luminancia(SUPERFICIE) < 0.05);
        assert!(luminancia(TEXTO) > luminancia(FUNDO));
    }

    #[test]
    fn contraste_do_texto_sobre_os_fundos() {
        for fundo in [FUNDO, LATERAL, SUPERFICIE, SUPERFICIE_HOVER] {
            for texto in [TEXTO, TEXTO_SECUNDARIO, DESTAQUE] {
                let c = contraste(texto, fundo);
                assert!(c >= MINIMO, "{texto:06X} sobre {fundo:06X}: {c:.2}");
            }
        }
    }

    #[test]
    fn contraste_sobre_o_destaque() {
        for d in [DESTAQUE, DESTAQUE_HOVER, DESTAQUE_ATIVO] {
            let c = contraste(TEXTO_SOBRE_DESTAQUE, d);
            assert!(c >= MINIMO, "texto sobre {d:06X}: {c:.2}");
        }
    }

    #[test]
    fn contraste_de_referencia() {
        assert!((contraste(0x000000, 0xFFFFFF) - 21.).abs() < 1e-9);
        assert!((contraste(0x777777, 0x777777) - 1.).abs() < 1e-9);
    }

    #[test]
    fn fonte_variavel_com_queda() {
        assert_eq!(FONTE, "Segoe UI Variable");
        assert_eq!(escolher_fonte(&["Arial", "Segoe UI Variable"]), FONTE);
        assert_eq!(
            escolher_fonte(&["segoe ui variable text"]),
            "Segoe UI Variable Text"
        );
        assert_eq!(escolher_fonte(&["Arial", "Segoe UI"]), FONTE_QUEDA);
        assert_eq!(escolher_fonte::<&str>(&[]), FONTE_QUEDA);
    }

    #[test]
    fn escala_tipografica() {
        let escala = [TAM_TITULO_PAGINA, TAM_SUBTITULO, TAM_CORPO, TAM_LEGENDA];
        assert_eq!(escala, [28., 20., 14., 12.]);
        assert!(escala.windows(2).all(|p| p[0] > p[1]));
        let esp = [ESP_1, ESP_2, ESP_3, ESP_4, ESP_6, ESP_8, ESP_12];
        assert!(esp.iter().all(|e| e % 4. == 0.));
        assert!(esp.windows(2).all(|p| p[0] < p[1]));
    }
}
