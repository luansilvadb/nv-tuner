//! Tema visual: escuro fixo, destaque ciano do logo e Segoe UI Variable
//! (R-UI-10), com tokens de espaçamento e de tipografia (R-UI-11).
//!
//! As cores ficam aqui como `u32` (0xRRGGBB) para os testes calcularem o
//! contraste sem depender do GPUI; [`aplicar`] as copia para o tema global
//! do gpui-component.

use gpui_kit::component::{Theme, ThemeMode};
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
pub const FUNDO: u32 = 0x14171C;
/// Fundo da navegação lateral (o quadrado escuro do logo).
pub const LATERAL: u32 = 0x161A21;
/// Cartões e grupos.
pub const SUPERFICIE: u32 = 0x1C2129;
/// Linha sob o cursor na lateral e em listas.
pub const SUPERFICIE_HOVER: u32 = 0x242A34;
/// Borda (a moldura do logo).
pub const BORDA: u32 = 0x2C3440;

pub const TEXTO: u32 = 0xE8ECF1;
pub const TEXTO_SECUNDARIO: u32 = 0x9AA4B2;

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
pub const TAM_TITULO_CARTAO: f32 = 15.;
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

pub const RAIO: f32 = 6.;
pub const LARGURA_LATERAL: f32 = 240.;
pub const LARGURA_JANELA: f32 = 1200.;
pub const ALTURA_JANELA: f32 = 800.;

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
        t.ring = cor(DESTAQUE);
        t.link = cor(DESTAQUE);
        t.sidebar = cor(LATERAL);
        t.sidebar_foreground = cor(TEXTO);
        t.sidebar_border = cor(BORDA);
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
        DESTAQUE, DESTAQUE_ATIVO, DESTAQUE_HOVER, ESP_1, ESP_2, ESP_3, ESP_4, ESP_6, ESP_8, FONTE,
        FONTE_QUEDA, FUNDO, LATERAL, SUPERFICIE, SUPERFICIE_HOVER, TAM_CORPO, TAM_LEGENDA,
        TAM_TITULO_CARTAO, TAM_TITULO_PAGINA, TEXTO, TEXTO_SECUNDARIO, TEXTO_SOBRE_DESTAQUE,
        contraste, escolher_fonte, luminancia,
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
        let escala = [TAM_TITULO_PAGINA, TAM_TITULO_CARTAO, TAM_CORPO, TAM_LEGENDA];
        assert_eq!(escala, [28., 15., 14., 12.]);
        assert!(escala.windows(2).all(|p| p[0] > p[1]));
        let esp = [ESP_1, ESP_2, ESP_3, ESP_4, ESP_6, ESP_8];
        assert!(esp.iter().all(|e| e % 4. == 0.));
        assert!(esp.windows(2).all(|p| p[0] < p[1]));
    }
}
