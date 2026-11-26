//! Editor INI (Unreal, Ubisoft) que troca só os bytes do valor (F3-01).
//!
//! Linhas reconhecidas: vazia, comentário (`;` ou `#`), seção `[nome]` e `chave=valor`.
//! Qualquer outra linha faz o arquivo inteiro ser recusado. Seções e chaves são
//! comparadas sem caixa ASCII; o valor é o texto após `=` sem os espaços das pontas.

use crate::{Erro, Resultado};
use std::ops::Range;

struct Entrada {
    secao: Range<usize>,
    chave: Range<usize>,
    valor: Range<usize>,
}

fn analisar(texto: &str) -> Resultado<Vec<Entrada>> {
    let mut entradas = Vec::new();
    let mut secao = 0..0;
    let mut pos = 0;
    for (n, linha) in texto.split_inclusive('\n').enumerate() {
        let inicio = pos;
        pos += linha.len();
        let conteudo = linha.strip_suffix('\n').unwrap_or(linha);
        let conteudo = conteudo.strip_suffix('\r').unwrap_or(conteudo);
        if conteudo.contains('\r') {
            return Err(Erro::formato(format!("CR isolado na linha {}", n + 1)));
        }
        let aparado = conteudo.trim_matches(ESPACO);
        if aparado.is_empty() || aparado.starts_with(';') || aparado.starts_with('#') {
            continue;
        }
        let desloc = inicio + (conteudo.len() - conteudo.trim_start_matches(ESPACO).len());
        if aparado.starts_with('[') {
            if !aparado.ends_with(']') || aparado.len() < 2 {
                return Err(Erro::formato(format!(
                    "seção malformada na linha {}",
                    n + 1
                )));
            }
            secao = desloc + 1..desloc + aparado.len() - 1;
            continue;
        }
        let Some(igual) = conteudo.find('=') else {
            return Err(Erro::formato(format!("linha {} não reconhecida", n + 1)));
        };
        let nome = &conteudo[..igual];
        let nome_ini = nome.len() - nome.trim_start_matches(ESPACO).len();
        let nome_fim = nome.trim_end_matches(ESPACO).len();
        if nome_fim <= nome_ini {
            return Err(Erro::formato(format!("chave vazia na linha {}", n + 1)));
        }
        let bruto = &conteudo[igual + 1..];
        let v_ini = igual + 1 + (bruto.len() - bruto.trim_start_matches(ESPACO).len());
        let v_fim = (igual + 1 + bruto.trim_end_matches(ESPACO).len()).max(v_ini);
        entradas.push(Entrada {
            secao: secao.clone(),
            chave: inicio + nome_ini..inicio + nome_fim,
            valor: inicio + v_ini..inicio + v_fim,
        });
    }
    Ok(entradas)
}

const ESPACO: &[char] = &[' ', '\t'];

fn localizar(texto: &str, secao: &str, chave: &str) -> Resultado<Range<usize>> {
    let entradas = analisar(texto)?;
    let mut achadas = entradas.iter().filter(|e| {
        texto[e.secao.clone()].eq_ignore_ascii_case(secao)
            && texto[e.chave.clone()].eq_ignore_ascii_case(chave)
    });
    let Some(e) = achadas.next() else {
        return Err(Erro::ChaveAusente(format!("[{secao}] {chave}")));
    };
    if achadas.next().is_some() {
        return Err(Erro::formato(format!("chave duplicada: [{secao}] {chave}")));
    }
    Ok(e.valor.clone())
}

/// Valor de `chave` em `[secao]` (`""` = antes da primeira seção).
pub fn ler(texto: &str, secao: &str, chave: &str) -> Resultado<String> {
    Ok(texto[localizar(texto, secao, chave)?].to_owned())
}

/// `texto` com só o valor de `chave` trocado por `novo`. Não cria chave.
pub fn definir(texto: &str, secao: &str, chave: &str, novo: &str) -> Resultado<String> {
    if novo.contains(['\r', '\n']) || novo.trim_matches(ESPACO) != novo {
        return Err(Erro::formato(
            "valor novo com quebra de linha ou espaço nas pontas",
        ));
    }
    let r = localizar(texto, secao, chave)?;
    let saida = [&texto[..r.start], novo, &texto[r.end..]].concat();
    if ler(&saida, secao, chave)? != novo {
        return Err(Erro::formato("releitura não confere"));
    }
    Ok(saida)
}
