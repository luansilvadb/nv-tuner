use std::collections::BTreeSet;

use crate::Erro;

const SWAP: &str = "SwapEffectUpgradeEnable";

struct Token<'a> {
    chave: &'a str,
    valor: &'a str,
    bruto: &'a str,
}

fn analisar(texto: &str) -> Result<Vec<Token<'_>>, Erro> {
    let mut chaves = BTreeSet::new();
    if !texto.is_empty() && !texto.ends_with(';') {
        return Err(Erro::FormatoAmbiguo);
    }
    texto
        .split_inclusive(';')
        .map(|bruto| {
            let (chave, valor) = bruto[..bruto.len() - 1]
                .split_once('=')
                .ok_or(Erro::FormatoAmbiguo)?;
            if chave.is_empty()
                || chave.chars().any(|c| c.is_control() || c.is_whitespace())
                || valor.contains('=')
                || valor.chars().any(char::is_control)
                || !chaves.insert(chave.to_ascii_lowercase())
            {
                return Err(Erro::FormatoAmbiguo);
            }
            Ok(Token {
                chave,
                valor,
                bruto,
            })
        })
        .collect()
}

pub(crate) fn ler<'a>(texto: &'a str, chave: &str) -> Result<Option<&'a str>, Erro> {
    let valor = analisar(texto)?
        .into_iter()
        .find(|token| token.chave.eq_ignore_ascii_case(chave))
        .map(|token| token.valor);
    if valor.is_some_and(|v| !matches!(v, "0" | "1")) {
        return Err(Erro::ValorInvalido);
    }
    Ok(valor)
}

pub(crate) fn swap(texto: &str) -> Result<Option<&str>, Erro> {
    ler(texto, SWAP)
}

pub(crate) fn substituir(texto: &str, valor: Option<&str>) -> Result<String, Erro> {
    let mut resultado = String::with_capacity(texto.len());
    let mut encontrado = false;
    for token in analisar(texto)? {
        if token.chave.eq_ignore_ascii_case(SWAP) {
            encontrado = true;
            if let Some(valor) = valor {
                anexar(&mut resultado, token.chave, valor);
            }
        } else {
            resultado.push_str(token.bruto);
        }
    }
    if !encontrado {
        if let Some(valor) = valor {
            anexar(&mut resultado, SWAP, valor);
        }
    }
    Ok(resultado)
}

fn anexar(texto: &mut String, chave: &str, valor: &str) {
    texto.push_str(chave);
    texto.push('=');
    texto.push_str(valor);
    texto.push(';');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabela_de_tokens() {
        for (origem, valor, esperado) in [
            ("", Some("1"), "SwapEffectUpgradeEnable=1;"),
            (
                "HDR=1;VRR=0;",
                Some("1"),
                "HDR=1;VRR=0;SwapEffectUpgradeEnable=1;",
            ),
            (
                "Primeiro= á🦀 ;swapeffectupgradeenable=1;Ultimo=;",
                Some("0"),
                "Primeiro= á🦀 ;swapeffectupgradeenable=0;Ultimo=;",
            ),
            ("A=1;SwapEffectUpgradeEnable=0;B=2;", None, "A=1;B=2;"),
            ("SwapEffectUpgradeEnable=1;", None, ""),
            ("AutoHDR=1;", None, "AutoHDR=1;"),
        ] {
            assert_eq!(substituir(origem, valor).unwrap(), esperado);
        }
        for invalido in [
            "X=1",
            ";",
            "=1;",
            " X=1;",
            "A=B=C;",
            "A=1;;",
            "X=1;x=2;",
            "SwapEffectUpgradeEnable=0;swapeffectupgradeenable=1;",
            "X=\0;",
        ] {
            assert_eq!(swap(invalido), Err(Erro::FormatoAmbiguo), "{invalido:?}");
        }
        assert_eq!(swap("SwapEffectUpgradeEnable=2;"), Err(Erro::ValorInvalido));
    }
}
