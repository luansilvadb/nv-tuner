//! Editor KeyValues texto (Valve) que troca só os bytes do valor (F3-01).
//!
//! Aceita cadeias entre aspas (escapes `\\`, `\"`, `\n`, `\t`), tokens sem aspas,
//! comentários `//` e blocos aninhados. Condicionais (`[$WIN32]`), chaves sem valor,
//! blocos desbalanceados ou aspas abertas fazem o arquivo inteiro ser recusado.
//! Chaves são comparadas sem caixa ASCII; folha duplicada no caminho alvo é recusada.

use crate::{Erro, Resultado};
use std::ops::Range;

/// Uma folha `"chave" "valor"` do documento.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folha {
    /// Caminho de chaves (já sem escapes), incluindo a chave da folha.
    pub caminho: Vec<String>,
    /// Valor sem escapes.
    pub valor: String,
    faixa: Range<usize>,
    aspas: bool,
}

enum Token {
    Texto { faixa: Range<usize>, aspas: bool },
    Abre,
    Fecha,
}

fn tokens(t: &str) -> Resultado<Vec<Token>> {
    let b = t.as_bytes();
    let mut v = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => {
                v.push(Token::Abre);
                i += 1;
            }
            b'}' => {
                v.push(Token::Fecha);
                i += 1;
            }
            b'"' => {
                let ini = i + 1;
                i = ini;
                loop {
                    match b.get(i) {
                        None => return Err(Erro::formato("aspas sem fechamento")),
                        Some(b'\\') => i += 2,
                        Some(b'"') => break,
                        Some(_) => i += 1,
                    }
                }
                if i > b.len() {
                    return Err(Erro::formato("escape no fim do arquivo"));
                }
                v.push(Token::Texto {
                    faixa: ini..i,
                    aspas: true,
                });
                i += 1;
            }
            b'[' => return Err(Erro::formato("condicional KeyValues não suportada")),
            _ => {
                let ini = i;
                while i < b.len()
                    && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'"' | b'{' | b'}')
                {
                    i += 1;
                }
                v.push(Token::Texto {
                    faixa: ini..i,
                    aspas: false,
                });
            }
        }
    }
    Ok(v)
}

fn sem_escape(s: &str) -> String {
    let mut r = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            r.push(c);
            continue;
        }
        match it.next() {
            Some('\\') => r.push('\\'),
            Some('"') => r.push('"'),
            Some('n') => r.push('\n'),
            Some('t') => r.push('\t'),
            Some(o) => {
                r.push('\\');
                r.push(o);
            }
            None => r.push('\\'),
        }
    }
    r
}

fn com_escape(s: &str) -> String {
    let mut r = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => r.push_str("\\\\"),
            '"' => r.push_str("\\\""),
            '\n' => r.push_str("\\n"),
            '\t' => r.push_str("\\t"),
            o => r.push(o),
        }
    }
    r
}

/// Todas as folhas do documento, em ordem. Falha se o documento não for reconhecido por inteiro.
pub fn folhas(texto: &str) -> Resultado<Vec<Folha>> {
    let toks = tokens(texto)?;
    let mut pilha: Vec<String> = Vec::new();
    let mut saida = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Token::Fecha => {
                if pilha.pop().is_none() {
                    return Err(Erro::formato("'}' sem bloco aberto"));
                }
                i += 1;
            }
            Token::Abre => return Err(Erro::formato("'{' sem chave")),
            Token::Texto { faixa, .. } => {
                let chave = sem_escape(&texto[faixa.clone()]);
                match toks.get(i + 1) {
                    Some(Token::Abre) => {
                        pilha.push(chave);
                        i += 2;
                    }
                    Some(Token::Texto { faixa: fv, aspas }) => {
                        let mut caminho = pilha.clone();
                        caminho.push(chave);
                        saida.push(Folha {
                            caminho,
                            valor: sem_escape(&texto[fv.clone()]),
                            faixa: fv.clone(),
                            aspas: *aspas,
                        });
                        i += 2;
                    }
                    _ => return Err(Erro::formato("chave sem valor")),
                }
            }
        }
    }
    if !pilha.is_empty() {
        return Err(Erro::formato("bloco sem fechamento"));
    }
    Ok(saida)
}

fn igual(a: &[String], b: &[&str]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.eq_ignore_ascii_case(y))
}

fn localizar(texto: &str, caminho: &[&str]) -> Resultado<Folha> {
    let todas = folhas(texto)?;
    let mut achadas = todas.into_iter().filter(|f| igual(&f.caminho, caminho));
    let Some(f) = achadas.next() else {
        return Err(Erro::ChaveAusente(caminho.join("/")));
    };
    if achadas.next().is_some() {
        return Err(Erro::formato(format!(
            "chave duplicada: {}",
            caminho.join("/")
        )));
    }
    Ok(f)
}

/// Valor (sem escapes) da folha em `caminho`.
pub fn ler(texto: &str, caminho: &[&str]) -> Resultado<String> {
    Ok(localizar(texto, caminho)?.valor)
}

/// `texto` com só o valor da folha em `caminho` trocado por `novo`. Não cria chave.
pub fn definir(texto: &str, caminho: &[&str], novo: &str) -> Resultado<String> {
    let f = localizar(texto, caminho)?;
    let bruto = if f.aspas {
        // O escape é local a cada caractere: se o valor atual reescapado reproduz o bruto
        // byte a byte, só os caracteres alterados mudam na regravação. Escape fora de
        // `\\ \" \n \t` (ex.: `C:\q`) ou tab/quebra crus não reproduzem: recusa.
        if com_escape(&f.valor) != texto[f.faixa.clone()] {
            return Err(Erro::formato(
                "cadeia com escape não suportado; regravar mudaria o resto",
            ));
        }
        com_escape(novo)
    } else {
        let simples = !novo.is_empty()
            && !novo.contains([' ', '\t', '\r', '\n', '"', '{', '}', '\\', '['])
            && !novo.starts_with("//");
        if !simples {
            return Err(Erro::formato("valor sem aspas não comporta o valor novo"));
        }
        novo.to_owned()
    };
    let saida = [&texto[..f.faixa.start], &bruto, &texto[f.faixa.end..]].concat();
    if ler(&saida, caminho)? != novo {
        return Err(Erro::formato("releitura não confere"));
    }
    Ok(saida)
}
