//! Chave pública oficial (§6.2, U-15, U-15a).
//!
//! Vem de um único arquivo, `instalador/nv-tuner-minisign.pub`, embutido por
//! `include_str!`. Enquanto o arquivo tiver o marcador provisório, toda verificação
//! com a chave oficial falha fechada ([`ErroVerificacao::ChaveProvisoria`]).

use minisign_verify::PublicKey;

use crate::manifesto::ErroVerificacao;

/// Conteúdo completo do `.pub` oficial.
pub const PUB_OFICIAL: &str = include_str!("../../../instalador/nv-tuner-minisign.pub");

/// Marcador da chave provisória (Lote 0 adiado).
pub const MARCADOR_PROVISORIO: &str = "NVT-CHAVE-PROVISORIA";

/// Linha base64 da chave oficial (segunda linha do `.pub`).
pub const CHAVE_OFICIAL: &str = segunda_linha(PUB_OFICIAL);

const fn segunda_linha(s: &'static str) -> &'static str {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i] != b'\n' {
        i += 1;
    }
    if i >= b.len() {
        return "";
    }
    let (_, r) = b.split_at(i + 1);
    let mut fim = 0;
    while fim < r.len() && r[fim] != b'\n' && r[fim] != b'\r' {
        fim += 1;
    }
    match std::str::from_utf8(r.split_at(fim).0) {
        Ok(linha) => linha,
        Err(_) => "",
    }
}

/// Verdadeiro enquanto a chave oficial é o marcador provisório (ou não decodifica).
pub fn chave_oficial_provisoria() -> bool {
    CHAVE_OFICIAL == MARCADOR_PROVISORIO || PublicKey::from_base64(CHAVE_OFICIAL).is_err()
}

/// Chave pública Minisign pronta para verificar.
#[derive(Clone)]
pub struct Chave {
    pub(crate) pk: PublicKey,
}

impl std::fmt::Debug for Chave {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Chave(..)")
    }
}

impl Chave {
    /// A chave oficial; recusa se provisória.
    pub fn oficial() -> Result<Chave, ErroVerificacao> {
        if chave_oficial_provisoria() {
            return Err(ErroVerificacao::ChaveProvisoria);
        }
        Chave::de_base64(CHAVE_OFICIAL)
    }

    /// Chave a partir da linha base64 (testes, `nvt-assinar` com chave injetada).
    pub fn de_base64(linha: &str) -> Result<Chave, ErroVerificacao> {
        PublicKey::from_base64(linha.trim())
            .map(|pk| Chave { pk })
            .map_err(|_| ErroVerificacao::ChaveInvalida)
    }

    /// Chave de ensaio do laboratório (só com a feature `laboratorio`).
    #[cfg(feature = "laboratorio")]
    pub fn ensaio() -> Result<Chave, ErroVerificacao> {
        match crate::laboratorio::CHAVE_ENSAIO {
            Some(c) if c.trim() != CHAVE_OFICIAL => Chave::de_base64(c),
            _ => Err(ErroVerificacao::ChaveInvalida),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn chave_oficial_vem_do_arquivo_pub() {
        let arquivo = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../instalador/nv-tuner-minisign.pub"
        ))
        .unwrap();
        let linha = arquivo.lines().nth(1).unwrap();
        assert_eq!(CHAVE_OFICIAL, linha);
        assert!(
            arquivo
                .lines()
                .next()
                .unwrap()
                .starts_with("untrusted comment: ")
        );
    }

    #[test]
    fn chave_oficial_valida_ou_provisoria_falha_fechada() {
        if chave_oficial_provisoria() {
            assert!(matches!(
                Chave::oficial(),
                Err(ErroVerificacao::ChaveProvisoria)
            ));
        } else {
            assert_eq!(CHAVE_OFICIAL.len(), 56);
            assert!(CHAVE_OFICIAL.starts_with("RW"));
            Chave::oficial().unwrap();
        }
    }

    #[test]
    fn chave_de_teste_nunca_e_a_oficial() {
        let par = crate::teste::par();
        assert_ne!(par.publica_b64, CHAVE_OFICIAL);
    }
}
