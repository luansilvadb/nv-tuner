//! Conversões entre `String` e os tipos de texto da NVAPI.

use crate::error::Error;
use crate::ffi::{NVAPI_UNICODE_STRING_MAX, NvShortString, NvUnicodeString};

/// UTF-16 terminado em zero (ou até o fim do buffer) para `String`, trocando sequências
/// inválidas por U+FFFD.
pub fn from_wide(w: &[u16]) -> String {
    let n = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..n])
}

/// `NvAPI_ShortString` (ANSI, terminado em zero) para `String`.
pub fn from_short(s: &NvShortString) -> String {
    let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..n]).into_owned()
}

/// `&str` para `NvAPI_UnicodeString` (2048 `u16`, terminado em zero), no heap.
///
/// Recusa textos com NUL interno ou que não caibam (máximo 2047 unidades UTF-16), em vez
/// de truncar em silêncio: um nome de perfil ou executável truncado apontaria para outro
/// objeto do DRS.
pub fn to_unicode_string(s: &str) -> Result<Box<NvUnicodeString>, Error> {
    let mut b = Box::new([0u16; NVAPI_UNICODE_STRING_MAX]);
    for (n, c) in s.encode_utf16().enumerate() {
        if c == 0 {
            return Err(Error::InvalidString {
                reason: "contém NUL",
            });
        }
        if n >= NVAPI_UNICODE_STRING_MAX - 1 {
            return Err(Error::InvalidString {
                reason: "mais de 2047 unidades UTF-16",
            });
        }
        b[n] = c;
    }
    Ok(b)
}
