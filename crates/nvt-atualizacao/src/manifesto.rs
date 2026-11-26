//! Manifesto `atualizacao.json` e sua assinatura Minisign (§6.1, §6.2, U-10..U-13).

use std::fmt;

use minisign_verify::{Error as ErroMinisign, Signature};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::chave::Chave;
use crate::estado::UltimoResultado;
use crate::versao::Versao;
use crate::{LIMITE_ASSINATURA, LIMITE_MANIFESTO, LIMITE_MSI, PRODUTO, URL_REPOSITORIO};

/// Único `formato` aceito.
pub const FORMATO: u64 = 1;

/// Manifesto já verificado e validado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifesto {
    pub versao: Versao,
    /// Sempre `nv-tuner-<versao>-x64.msi`.
    pub msi: String,
    pub tamanho: u64,
    /// 64 dígitos hexadecimais minúsculos.
    pub sha256: String,
}

impl Manifesto {
    /// URL do MSI, derivada da versão (nunca lida do manifesto).
    pub fn url_msi(&self) -> String {
        url_msi(self.versao)
    }
}

/// Motivo de recusa do manifesto ou da assinatura.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroVerificacao {
    /// A chave oficial ainda é o marcador provisório: recusa tudo.
    ChaveProvisoria,
    ChaveInvalida,
    AssinaturaGrande,
    AssinaturaMalformada,
    /// Assinatura `Ed` (sem pré-hash): recusada (`allow_legacy=false`).
    AssinaturaLegada,
    /// Assinada por outra chave (id diferente).
    ChaveDiferente,
    /// Ed25519 não confere (dados, assinatura ou comentário confiável adulterados).
    AssinaturaInvalida,
    /// Comentário confiável diferente do esperado para o manifesto.
    ComentarioDiferente {
        esperado: String,
        recebido: String,
    },
    ManifestoGrande,
    /// JSON inválido, BOM, campo ausente/desconhecido ou tipo errado.
    ManifestoMalformado(String),
    FormatoDesconhecido(u64),
    ProdutoDiferente,
    VersaoInvalida,
    NomeMsiInvalido,
    TamanhoInvalido,
    Sha256Invalido,
}

impl ErroVerificacao {
    /// Classificação persistida em `ultimo_resultado`.
    pub fn resultado(&self) -> UltimoResultado {
        use ErroVerificacao::*;
        match self {
            ChaveProvisoria
            | ChaveInvalida
            | AssinaturaGrande
            | AssinaturaMalformada
            | AssinaturaLegada
            | ChaveDiferente
            | AssinaturaInvalida
            | ComentarioDiferente { .. } => UltimoResultado::ErroAssinatura,
            _ => UltimoResultado::ErroManifesto,
        }
    }
}

impl fmt::Display for ErroVerificacao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ErroVerificacao::*;
        match self {
            ChaveProvisoria => f.write_str("chave oficial provisória: atualização desativada"),
            ChaveInvalida => f.write_str("chave pública inválida"),
            AssinaturaGrande => f.write_str("assinatura maior que o limite"),
            AssinaturaMalformada => f.write_str("assinatura malformada"),
            AssinaturaLegada => f.write_str("assinatura legada (sem pré-hash) recusada"),
            ChaveDiferente => f.write_str("assinatura de outra chave"),
            AssinaturaInvalida => f.write_str("assinatura inválida"),
            ComentarioDiferente { esperado, recebido } => {
                write!(f, "comentário confiável {recebido:?} ≠ {esperado:?}")
            }
            ManifestoGrande => f.write_str("manifesto maior que o limite"),
            ManifestoMalformado(m) => write!(f, "manifesto malformado: {m}"),
            FormatoDesconhecido(n) => write!(f, "formato desconhecido ({n})"),
            ProdutoDiferente => f.write_str("produto diferente"),
            VersaoInvalida => f.write_str("versão inválida"),
            NomeMsiInvalido => f.write_str("nome do MSI inválido"),
            TamanhoInvalido => f.write_str("tamanho inválido"),
            Sha256Invalido => f.write_str("SHA-256 inválido"),
        }
    }
}

impl std::error::Error for ErroVerificacao {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bruto {
    formato: u64,
    produto: String,
    versao: String,
    msi: String,
    tamanho: u64,
    sha256: String,
}

/// Comentário confiável exato da assinatura do manifesto.
pub fn comentario_manifesto(versao: Versao) -> String {
    format!("nv-tuner atualizacao formato={FORMATO} versao={versao}")
}

/// Comentário confiável da assinatura do MSI.
pub fn comentario_msi(versao: Versao, sha256: &str) -> String {
    format!("nv-tuner msi versao={versao} sha256={sha256}")
}

/// `nv-tuner-<versao>-x64.msi`.
pub fn nome_msi(versao: Versao) -> String {
    format!("nv-tuner-{versao}-x64.msi")
}

/// `instalacao-<versao>.log`.
pub fn nome_log(versao: Versao) -> String {
    format!("instalacao-{versao}.log")
}

/// URL do MSI derivada da versão.
pub fn url_msi(versao: Versao) -> String {
    format!(
        "{URL_REPOSITORIO}/releases/download/v{versao}/{}",
        nome_msi(versao)
    )
}

/// SHA-256 em hexadecimal minúsculo.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub(crate) fn hex(b: &[u8]) -> String {
    use fmt::Write;
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        let _ = write!(s, "{x:02x}");
    }
    s
}

/// JSON canônico do manifesto (para `nvt-assinar manifesto`).
pub fn gerar_manifesto_json(versao: Versao, tamanho: u64, sha256: &str) -> String {
    format!(
        "{{\n  \"formato\": {FORMATO},\n  \"produto\": \"{PRODUTO}\",\n  \"versao\": \"{versao}\",\n  \"msi\": \"{}\",\n  \"tamanho\": {tamanho},\n  \"sha256\": \"{sha256}\"\n}}\n",
        nome_msi(versao)
    )
}

fn sha_valido(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Verifica com a chave oficial (falha fechada com [`ErroVerificacao::ChaveProvisoria`]).
pub fn verificar_manifesto_oficial(
    manifesto: &[u8],
    assinatura: &[u8],
) -> Result<Manifesto, ErroVerificacao> {
    let chave = Chave::oficial()?;
    verificar_manifesto(manifesto, assinatura, &chave)
}

/// Verifica a assinatura sobre os bytes recebidos e só então decodifica e valida (U-10).
pub fn verificar_manifesto(
    manifesto: &[u8],
    assinatura: &[u8],
    chave: &Chave,
) -> Result<Manifesto, ErroVerificacao> {
    if assinatura.len() > LIMITE_ASSINATURA {
        return Err(ErroVerificacao::AssinaturaGrande);
    }
    if manifesto.len() > LIMITE_MANIFESTO {
        return Err(ErroVerificacao::ManifestoGrande);
    }
    let texto_sig =
        std::str::from_utf8(assinatura).map_err(|_| ErroVerificacao::AssinaturaMalformada)?;
    let sig = Signature::decode(texto_sig).map_err(|_| ErroVerificacao::AssinaturaMalformada)?;
    chave
        .pk
        .verify(manifesto, &sig, false)
        .map_err(|e| match e {
            ErroMinisign::UnexpectedAlgorithm | ErroMinisign::UnsupportedLegacyMode => {
                ErroVerificacao::AssinaturaLegada
            }
            ErroMinisign::UnexpectedKeyId => ErroVerificacao::ChaveDiferente,
            _ => ErroVerificacao::AssinaturaInvalida,
        })?;

    if manifesto.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Err(ErroVerificacao::ManifestoMalformado("BOM".into()));
    }
    let b: Bruto = serde_json::from_slice(manifesto)
        .map_err(|e| ErroVerificacao::ManifestoMalformado(e.to_string()))?;
    if b.formato != FORMATO {
        return Err(ErroVerificacao::FormatoDesconhecido(b.formato));
    }
    if b.produto != PRODUTO {
        return Err(ErroVerificacao::ProdutoDiferente);
    }
    let versao: Versao = b
        .versao
        .parse()
        .map_err(|_| ErroVerificacao::VersaoInvalida)?;
    if b.msi != nome_msi(versao) {
        return Err(ErroVerificacao::NomeMsiInvalido);
    }
    if !(1..=LIMITE_MSI).contains(&b.tamanho) {
        return Err(ErroVerificacao::TamanhoInvalido);
    }
    if !sha_valido(&b.sha256) {
        return Err(ErroVerificacao::Sha256Invalido);
    }
    let esperado = comentario_manifesto(versao);
    if sig.trusted_comment() != esperado {
        return Err(ErroVerificacao::ComentarioDiferente {
            esperado,
            recebido: sig.trusted_comment().to_string(),
        });
    }
    Ok(Manifesto {
        versao,
        msi: b.msi,
        tamanho: b.tamanho,
        sha256: b.sha256,
    })
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::teste::{assinar, par, para_legada};

    const SHA: &str = "3f2a9c0000000000000000000000000000000000000000000000000000000001";

    fn json(v: &str) -> String {
        gerar_manifesto_json(v.parse().unwrap(), 4_718_592, SHA)
    }

    fn assinado(corpo: &str, comentario: &str) -> (Vec<u8>, Vec<u8>) {
        (
            corpo.as_bytes().to_vec(),
            assinar(par(), corpo.as_bytes(), comentario).into_bytes(),
        )
    }

    #[test]
    fn manifesto_valido() {
        let p = par();
        let (m, s) = assinado(
            &json("0.1.0"),
            "nv-tuner atualizacao formato=1 versao=0.1.0",
        );
        let r = verificar_manifesto(&m, &s, &p.chave()).unwrap();
        assert_eq!(r.versao, Versao::nova(0, 1, 0));
        assert_eq!(r.msi, "nv-tuner-0.1.0-x64.msi");
        assert_eq!(
            r.url_msi(),
            "https://github.com/luansilvadb/nv-tuner/releases/download/v0.1.0/nv-tuner-0.1.0-x64.msi"
        );
    }

    #[test]
    fn dados_trocados() {
        let p = par();
        let (_, s) = assinado(&json("0.1.0"), &comentario_manifesto(Versao::nova(0, 1, 0)));
        let outro = json("0.1.0").replace("4718592", "4718593");
        assert_eq!(
            verificar_manifesto(outro.as_bytes(), &s, &p.chave()),
            Err(ErroVerificacao::AssinaturaInvalida)
        );
    }

    #[test]
    fn comentario_de_outra_versao() {
        let p = par();
        let (m, s) = assinado(
            &json("0.2.0"),
            "nv-tuner atualizacao formato=1 versao=0.1.0",
        );
        assert!(matches!(
            verificar_manifesto(&m, &s, &p.chave()),
            Err(ErroVerificacao::ComentarioDiferente { .. })
        ));
    }

    #[test]
    fn comentario_confiavel_adulterado() {
        let p = par();
        let (m, s) = assinado(
            &json("0.1.0"),
            "nv-tuner atualizacao formato=1 versao=0.1.0",
        );
        let s = String::from_utf8(s)
            .unwrap()
            .replace("versao=0.1.0", "versao=0.1.1");
        assert_eq!(
            verificar_manifesto(&m, s.as_bytes(), &p.chave()),
            Err(ErroVerificacao::AssinaturaInvalida)
        );
    }

    #[test]
    fn assinatura_legada_recusada() {
        let p = par();
        let (m, s) = assinado(&json("0.1.0"), &comentario_manifesto(Versao::nova(0, 1, 0)));
        let leg = para_legada(std::str::from_utf8(&s).unwrap());
        assert_eq!(
            verificar_manifesto(&m, leg.as_bytes(), &p.chave()),
            Err(ErroVerificacao::AssinaturaLegada)
        );
    }

    #[test]
    fn chave_errada() {
        let (m, s) = assinado(&json("0.1.0"), &comentario_manifesto(Versao::nova(0, 1, 0)));
        let outra = crate::teste::novo_par();
        assert_eq!(
            verificar_manifesto(&m, &s, &outra.chave()),
            Err(ErroVerificacao::ChaveDiferente)
        );
    }

    #[test]
    fn assinatura_truncada_ausente_ou_grande() {
        let p = par();
        let (m, s) = assinado(&json("0.1.0"), &comentario_manifesto(Versao::nova(0, 1, 0)));
        for sig in [&s[..s.len() / 2], &b""[..], &vec![b'a'; 5000][..]] {
            let e = verificar_manifesto(&m, sig, &p.chave()).unwrap_err();
            assert_eq!(e.resultado(), UltimoResultado::ErroAssinatura, "{e}");
        }
    }

    #[test]
    fn chave_provisoria_recusa_tudo() {
        let (m, s) = assinado(&json("0.1.0"), &comentario_manifesto(Versao::nova(0, 1, 0)));
        if crate::chave_oficial_provisoria() {
            assert_eq!(
                verificar_manifesto_oficial(&m, &s),
                Err(ErroVerificacao::ChaveProvisoria)
            );
        } else {
            assert!(verificar_manifesto_oficial(&m, &s).is_err());
        }
    }

    #[test]
    fn campos_fora_do_formato() {
        let p = par();
        let c = comentario_manifesto(Versao::nova(0, 1, 0));
        let base = json("0.1.0");
        type Caso = (String, fn(&ErroVerificacao) -> bool);
        let casos: Vec<Caso> = vec![
            (base.replace("\"formato\": 1", "\"formato\": 2"), |e| {
                matches!(e, ErroVerificacao::FormatoDesconhecido(2))
            }),
            (base.replace("\"nv-tuner\"", "\"outro\""), |e| {
                *e == ErroVerificacao::ProdutoDiferente
            }),
            (base.replace("x64.msi", "x86.msi"), |e| {
                *e == ErroVerificacao::NomeMsiInvalido
            }),
            (base.replace("4718592", "0"), |e| {
                *e == ErroVerificacao::TamanhoInvalido
            }),
            (base.replace("4718592", "67108865"), |e| {
                *e == ErroVerificacao::TamanhoInvalido
            }),
            (base.replace(SHA, &SHA.to_uppercase()), |e| {
                *e == ErroVerificacao::Sha256Invalido
            }),
            (base.replace("}", ", \"url\": \"https://x\"}"), |e| {
                matches!(e, ErroVerificacao::ManifestoMalformado(_))
            }),
            (base.replace("  \"tamanho\": 4718592,\n", ""), |e| {
                matches!(e, ErroVerificacao::ManifestoMalformado(_))
            }),
            (format!("\u{feff}{base}"), |e| {
                matches!(e, ErroVerificacao::ManifestoMalformado(_))
            }),
        ];
        for (corpo, ok) in casos {
            let (m, s) = assinado(&corpo, &c);
            let e = verificar_manifesto(&m, &s, &p.chave()).unwrap_err();
            assert!(ok(&e), "{corpo}: {e:?}");
            assert_eq!(e.resultado(), UltimoResultado::ErroManifesto);
        }
        let (m, s) = assinado(&json("0.1.0").replace("0.1.0", "0.1"), &c);
        assert_eq!(
            verificar_manifesto(&m, &s, &p.chave()),
            Err(ErroVerificacao::VersaoInvalida)
        );
    }

    #[test]
    fn hash_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
