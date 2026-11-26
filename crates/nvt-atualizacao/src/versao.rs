//! Versão `X.Y.Z` com os limites do MSI (255.255.65535, §6.1).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Versao {
    pub maior: u8,
    pub menor: u8,
    pub correcao: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersaoInvalida;

impl fmt::Display for VersaoInvalida {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("versão inválida (esperado X.Y.Z dentro de 255.255.65535)")
    }
}

impl std::error::Error for VersaoInvalida {}

impl Versao {
    pub const fn nova(maior: u8, menor: u8, correcao: u16) -> Versao {
        Versao {
            maior,
            menor,
            correcao,
        }
    }
}

/// Campo numérico sem zero à esquerda, com no máximo `digitos` dígitos.
fn campo(s: &str, digitos: usize) -> Result<u32, VersaoInvalida> {
    let ok = !s.is_empty()
        && s.len() <= digitos
        && s.bytes().all(|b| b.is_ascii_digit())
        && (s == "0" || !s.starts_with('0'));
    if !ok {
        return Err(VersaoInvalida);
    }
    s.parse().map_err(|_| VersaoInvalida)
}

impl FromStr for Versao {
    type Err = VersaoInvalida;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut p = s.split('.');
        let (a, b, c) = match (p.next(), p.next(), p.next(), p.next()) {
            (Some(a), Some(b), Some(c), None) => (a, b, c),
            _ => return Err(VersaoInvalida),
        };
        let maior = u8::try_from(campo(a, 3)?).map_err(|_| VersaoInvalida)?;
        let menor = u8::try_from(campo(b, 3)?).map_err(|_| VersaoInvalida)?;
        let correcao = u16::try_from(campo(c, 5)?).map_err(|_| VersaoInvalida)?;
        Ok(Versao::nova(maior, menor, correcao))
    }
}

impl fmt::Display for Versao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.maior, self.menor, self.correcao)
    }
}

impl Serialize for Versao {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Versao {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn aceita_e_compara() {
        let v: Versao = "0.10.2".parse().unwrap();
        assert_eq!(v, Versao::nova(0, 10, 2));
        assert!(v > "0.9.99".parse().unwrap());
        assert!("255.255.65535".parse::<Versao>().is_ok());
        assert_eq!(v.to_string(), "0.10.2");
    }

    #[test]
    fn recusa_fora_do_formato() {
        for s in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "01.2.3",
            "1.02.3",
            "1.2.03",
            "256.0.0",
            "0.256.0",
            "0.0.65536",
            "0.0.100000",
            "1.2.3-beta",
            "v1.2.3",
            " 1.2.3",
            "1.2.+3",
            "1..3",
        ] {
            assert!(s.parse::<Versao>().is_err(), "{s}");
        }
    }

    #[test]
    fn serde_como_texto() {
        let v = Versao::nova(1, 2, 3);
        assert_eq!(serde_json::to_string(&v).unwrap(), "\"1.2.3\"");
        assert_eq!(serde_json::from_str::<Versao>("\"1.2.3\"").unwrap(), v);
        assert!(serde_json::from_str::<Versao>("\"1.2\"").is_err());
    }
}
