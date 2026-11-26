//! Código de saída a partir do log `/l*v` do msiexec (U-35).

const MARCA: &str = "MainEngineThread is returning ";

/// Decodifica o log (UTF-16LE com ou sem BOM, ou bytes de 8 bits) e devolve o código da
/// **última** linha `MainEngineThread is returning N`. O chamador lê no máximo
/// [`crate::LIMITE_LOG_MSI`] bytes do fim.
pub fn extrair_codigo(log: &[u8]) -> Option<i64> {
    let texto = decodificar(log);
    let pos = texto.rfind(MARCA)?;
    let resto = &texto[pos + MARCA.len()..];
    let fim = resto
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && c == '-')))
        .map_or(resto.len(), |(i, _)| i);
    resto[..fim].parse().ok()
}

fn decodificar(b: &[u8]) -> String {
    let amostra = &b[..b.len().min(128)];
    let pares = amostra.len() / 2;
    let zeros = amostra
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|&&x| x == 0)
        .count();
    let utf16 = b.starts_with(&[0xFF, 0xFE]) || (pares >= 2 && zeros * 2 > pares);
    if utf16 {
        let corpo = b.strip_prefix(&[0xFF, 0xFE]).unwrap_or(b);
        // Leitura pelo fim pode cair no meio de uma unidade: alinha pelo início.
        let u: Vec<u16> = corpo
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(b).into_owned()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const EXEMPLO: &str = "MSI (s) (A4:B8) [10:00:01:123]: Product: nv-tuner -- Installation completed successfully.\r\n\
MSI (s) (A4:B8) [10:00:01:124]: MainEngineThread is returning 0\r\n\
MSI (c) (10:14) [10:00:01:200]: MainEngineThread is returning 3010\r\n\
=== Logging stopped: 26/11/2026  10:00:01 ===\r\n";

    fn utf16(s: &str, bom: bool) -> Vec<u8> {
        let mut v = if bom { vec![0xFF, 0xFE] } else { vec![] };
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    }

    #[test]
    fn ultima_ocorrencia_em_ansi_e_utf16() {
        assert_eq!(extrair_codigo(EXEMPLO.as_bytes()), Some(3010));
        assert_eq!(extrair_codigo(&utf16(EXEMPLO, true)), Some(3010));
        assert_eq!(extrair_codigo(&utf16(EXEMPLO, false)), Some(3010));
    }

    #[test]
    fn falha_e_ausente() {
        assert_eq!(
            extrair_codigo(b"x MainEngineThread is returning 1603\r\n"),
            Some(1603)
        );
        assert_eq!(extrair_codigo(b"instalando...\r\n"), None);
        assert_eq!(extrair_codigo(b"MainEngineThread is returning \r\n"), None);
        assert_eq!(extrair_codigo(b""), None);
    }
}
