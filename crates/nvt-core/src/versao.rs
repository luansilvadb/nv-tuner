//! Versões numéricas com até quatro partes (driver `621.40`, DLL do DLSS `310.6.0.0`).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Versão com até quatro partes numéricas; partes ausentes valem 0.
///
/// Aceita ponto ou vírgula como separador, porque o `FileVersion` do
/// VERSIONINFO costuma vir com vírgulas (`310,6,0,0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Versao(pub [u32; 4]);

impl Versao {
    /// Cria uma versão a partir das quatro partes.
    pub const fn new(a: u32, b: u32, c: u32, d: u32) -> Self {
        Self([a, b, c, d])
    }
}

/// Erro ao interpretar uma versão.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErroVersao(pub String);

impl fmt::Display for ErroVersao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "versão inválida \"{}\": esperado de 1 a 4 números separados por ponto",
            self.0
        )
    }
}

impl std::error::Error for ErroVersao {}

impl FromStr for Versao {
    type Err = ErroVersao;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let erro = || ErroVersao(s.to_owned());
        let texto = s.trim();
        if texto.is_empty() {
            return Err(erro());
        }
        let mut partes = [0u32; 4];
        for (n, parte) in texto.split(['.', ',']).enumerate() {
            let destino = partes.get_mut(n).ok_or_else(erro)?;
            *destino = parte.trim().parse().map_err(|_| erro())?;
        }
        Ok(Self(partes))
    }
}

impl fmt::Display for Versao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d] = self.0;
        if c == 0 && d == 0 {
            write!(f, "{a}.{b}")
        } else {
            write!(f, "{a}.{b}.{c}.{d}")
        }
    }
}

impl Serialize for Versao {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Versao {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let texto = String::deserialize(d)?;
        texto.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn interpreta_formatos_comuns() {
        assert_eq!(
            "621.40".parse::<Versao>().unwrap(),
            Versao::new(621, 40, 0, 0)
        );
        assert_eq!(
            "310,6,0,0".parse::<Versao>().unwrap(),
            Versao::new(310, 6, 0, 0)
        );
        assert!("".parse::<Versao>().is_err());
        assert!("1.2.3.4.5".parse::<Versao>().is_err());
        assert!("abc".parse::<Versao>().is_err());
    }

    #[test]
    fn ordena_numericamente() {
        let a: Versao = "310.10".parse().unwrap();
        let b: Versao = "310.9".parse().unwrap();
        assert!(a > b);
    }
}
