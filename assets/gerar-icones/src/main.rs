//! Gera `assets/png/nv-tuner-<N>.png` e `assets/nv-tuner.ico` a partir de
//! `assets/logo.svg`.
//!
//! O .ico guarda 16–48 px como DIB 32 bits (BGRA + máscara AND, compatível
//! com qualquer leitor) e 64–256 px como PNG (formato aceito desde o Vista).

use std::path::{Path, PathBuf};
use std::{env, fs};

use resvg::{tiny_skia, usvg};

const TAMANHOS: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];
/// Até este tamanho a imagem entra no .ico como DIB; acima, como PNG.
const MAX_DIB: u32 = 48;

fn renderizar(arvore: &usvg::Tree, lado: u32) -> tiny_skia::Pixmap {
    let mut pixmap = tiny_skia::Pixmap::new(lado, lado).expect("tamanho válido");
    let tam = arvore.size();
    let escala = lado as f32 / tam.width().max(tam.height());
    resvg::render(
        arvore,
        tiny_skia::Transform::from_scale(escala, escala),
        &mut pixmap.as_mut(),
    );
    pixmap
}

/// DIB 32 bits de baixo para cima (altura dobrada: cor + máscara AND).
fn dib(pixmap: &tiny_skia::Pixmap) -> Vec<u8> {
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut out = Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes()); // biSize
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&((h * 2) as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // planos
    out.extend_from_slice(&32u16.to_le_bytes()); // bits por pixel
    out.extend_from_slice(&[0u8; 24]); // BI_RGB, tamanho, resolução, paleta
    let pixels = pixmap.pixels();
    for y in (0..h).rev() {
        for x in 0..w {
            let c = pixels[(y * w + x) as usize].demultiply();
            out.extend_from_slice(&[c.blue(), c.green(), c.red(), c.alpha()]);
        }
    }
    // Máscara AND toda zero (a transparência vem do canal alfa).
    let linha = w.div_ceil(32) * 4;
    out.extend(std::iter::repeat_n(0u8, (linha * h) as usize));
    out
}

fn main() {
    let raiz: PathBuf = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join(".."));
    let svg = fs::read(raiz.join("logo.svg")).expect("assets/logo.svg");
    let arvore = usvg::Tree::from_data(&svg, &usvg::Options::default()).expect("SVG válido");

    let pasta_png = raiz.join("png");
    fs::create_dir_all(&pasta_png).unwrap();

    let mut imagens: Vec<(u32, Vec<u8>)> = Vec::new();
    for lado in TAMANHOS {
        let pixmap = renderizar(&arvore, lado);
        let png = pixmap.encode_png().expect("PNG");
        fs::write(pasta_png.join(format!("nv-tuner-{lado}.png")), &png).unwrap();
        imagens.push((lado, if lado <= MAX_DIB { dib(&pixmap) } else { png }));
    }

    // ICONDIR + ICONDIRENTRY[n] + dados.
    let mut ico = Vec::new();
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes()); // tipo: ícone
    ico.extend_from_slice(&(imagens.len() as u16).to_le_bytes());
    let mut deslocamento = 6 + 16 * imagens.len() as u32;
    for (lado, dados) in &imagens {
        let b = if *lado >= 256 { 0 } else { *lado as u8 }; // 0 = 256
        ico.extend_from_slice(&[b, b, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(dados.len() as u32).to_le_bytes());
        ico.extend_from_slice(&deslocamento.to_le_bytes());
        deslocamento += dados.len() as u32;
    }
    for (_, dados) in &imagens {
        ico.extend_from_slice(dados);
    }
    fs::write(raiz.join("nv-tuner.ico"), &ico).unwrap();
    println!(
        "{} PNGs em {} e nv-tuner.ico ({} bytes)",
        imagens.len(),
        pasta_png.display(),
        ico.len()
    );
}
