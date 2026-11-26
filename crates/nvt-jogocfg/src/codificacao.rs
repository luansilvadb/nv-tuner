//! Detecção e regravação da codificação (F3-01).

use crate::{Erro, Resultado};

/// Codificações aceitas. Outras (UTF-16BE, UTF-16 sem BOM, bytes inválidos) são
/// [`Erro::FormatoInesperado`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codificacao {
    Utf8,
    Utf8Bom,
    Utf16LeBom,
}

/// Texto decodificado (sem o BOM) e a codificação de origem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Texto {
    pub codificacao: Codificacao,
    pub texto: String,
}

const BOM8: &[u8] = &[0xEF, 0xBB, 0xBF];

pub fn decodificar(bytes: &[u8]) -> Resultado<Texto> {
    let (codificacao, texto) = if let Some(resto) = bytes.strip_prefix(BOM8) {
        (Codificacao::Utf8Bom, utf8(resto)?)
    } else if let Some(resto) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        (Codificacao::Utf16LeBom, utf16le(resto)?)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        return Err(Erro::formato("UTF-16BE não suportado"));
    } else {
        (Codificacao::Utf8, utf8(bytes)?)
    };
    if texto.contains('\0') {
        return Err(Erro::formato("caractere NUL (codificação mista?)"));
    }
    if texto.starts_with('\u{FEFF}') {
        return Err(Erro::formato("BOM repetido"));
    }
    Ok(Texto { codificacao, texto })
}

pub fn codificar(codificacao: Codificacao, texto: &str) -> Vec<u8> {
    match codificacao {
        Codificacao::Utf8 => texto.as_bytes().to_vec(),
        Codificacao::Utf8Bom => [BOM8, texto.as_bytes()].concat(),
        Codificacao::Utf16LeBom => {
            let mut v = vec![0xFF, 0xFE];
            for u in texto.encode_utf16() {
                v.extend(u.to_le_bytes());
            }
            v
        }
    }
}

fn utf8(b: &[u8]) -> Resultado<String> {
    String::from_utf8(b.to_vec()).map_err(|_| Erro::formato("UTF-8 inválido"))
}

fn utf16le(b: &[u8]) -> Resultado<String> {
    if b.len() % 2 != 0 {
        return Err(Erro::formato("UTF-16LE com número ímpar de bytes"));
    }
    let unidades = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]));
    char::decode_utf16(unidades)
        .collect::<Result<String, _>>()
        .map_err(|_| Erro::formato("UTF-16LE inválido"))
}
