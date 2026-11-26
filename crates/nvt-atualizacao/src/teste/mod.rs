//! Par de chaves Minisign descartável, gerado em memória a cada execução de teste
//! (crate `minisign` como dev-dependency). Nunca é a chave oficial.

use std::sync::OnceLock;

use minisign::KeyPair;

use crate::chave::Chave;

pub struct Par {
    pub par: KeyPair,
    pub publica_b64: String,
}

impl Par {
    pub fn chave(&self) -> Chave {
        Chave::de_base64(&self.publica_b64).unwrap()
    }
}

pub fn novo_par() -> Par {
    let par = KeyPair::generate_unencrypted_keypair().unwrap();
    let publica_b64 = par.pk.to_base64();
    Par { par, publica_b64 }
}

pub fn par() -> &'static Par {
    static PAR: OnceLock<Par> = OnceLock::new();
    PAR.get_or_init(novo_par)
}

/// Assinatura `.minisig` (pré-hash `ED`) com o comentário confiável dado.
pub fn assinar(p: &Par, dados: &[u8], comentario: &str) -> String {
    minisign::sign(Some(&p.par.pk), &p.par.sk, dados, Some(comentario), None)
        .unwrap()
        .into_string()
}

/// Troca o algoritmo `ED` por `Ed` (legado) na linha da assinatura.
pub fn para_legada(sig: &str) -> String {
    let mut linhas: Vec<String> = sig.lines().map(str::to_string).collect();
    let mut bin = b64_dec(&linhas[1]);
    assert_eq!(&bin[..2], b"ED");
    bin[1] = b'd';
    linhas[1] = b64_enc(&bin);
    linhas.join("\n") + "\n"
}

const ALF: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_enc(b: &[u8]) -> String {
    let mut s = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(ALF[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

fn b64_dec(s: &str) -> Vec<u8> {
    let mut bits = 0u32;
    let mut n = 0;
    let mut out = Vec::new();
    for ch in s.bytes().filter(|&c| c != b'=') {
        let v = ALF.iter().position(|&a| a == ch).unwrap() as u32;
        bits = bits << 6 | v;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
        }
    }
    out
}
