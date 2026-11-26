//! Edição por token de `LaunchOptions` da Steam (F3-11, N10).
//!
//! O app só acrescenta, troca ou remove o token `+fps_max <n>`; o resto da string do
//! usuário fica intacto. Qualquer entrada com `-allow_third_party_software` é recusada
//! ([`Erro::TokenProibido`]) e nenhuma saída o contém.

use crate::{Erro, Resultado};
use std::ops::Range;

/// Token que o app gerencia.
pub const TOKEN: &str = "+fps_max";
/// Token que o app nunca emite nem aceita na entrada.
pub const PROIBIDO: &str = "-allow_third_party_software";

fn palavras(s: &str) -> Vec<Range<usize>> {
    let mut v = Vec::new();
    let mut ini = None;
    for (i, c) in s.char_indices() {
        match (c.is_whitespace(), ini) {
            (true, Some(a)) => {
                v.push(a..i);
                ini = None;
            }
            (false, None) => ini = Some(i),
            _ => {}
        }
    }
    if let Some(a) = ini {
        v.push(a..s.len());
    }
    v
}

fn travar(s: &str) -> Resultado<()> {
    if s.to_ascii_lowercase().contains(PROIBIDO) {
        return Err(Erro::TokenProibido);
    }
    Ok(())
}

/// Faixa do token, faixa do número e o número.
type Achado = (Range<usize>, Range<usize>, u32);

/// Posição de `+fps_max` e do número que o segue (ambos como faixas de bytes).
fn localizar(s: &str) -> Resultado<Option<Achado>> {
    let p = palavras(s);
    let mut achado = None;
    for (i, r) in p.iter().enumerate() {
        if !s[r.clone()].eq_ignore_ascii_case(TOKEN) {
            continue;
        }
        if achado.is_some() {
            return Err(Erro::formato("+fps_max repetido"));
        }
        let Some(num) = p.get(i + 1) else {
            return Err(Erro::formato("+fps_max sem número"));
        };
        let n = s[num.clone()]
            .parse::<u32>()
            .map_err(|_| Erro::formato("+fps_max com valor não numérico"))?;
        achado = Some((r.clone(), num.clone(), n));
    }
    Ok(achado)
}

/// Valor atual de `+fps_max`, se houver.
pub fn ler_fps_max(s: &str) -> Resultado<Option<u32>> {
    travar(s)?;
    Ok(localizar(s)?.map(|(_, _, n)| n))
}

/// Acrescenta `+fps_max n` ou troca só o número do token existente.
pub fn definir_fps_max(s: &str, n: u32) -> Resultado<String> {
    travar(s)?;
    let saida = match localizar(s)? {
        Some((_, num, _)) => format!("{}{n}{}", &s[..num.start], &s[num.end..]),
        None if s.is_empty() || s.ends_with(char::is_whitespace) => format!("{s}{TOKEN} {n}"),
        None => format!("{s} {TOKEN} {n}"),
    };
    travar(&saida)?;
    Ok(saida)
}

/// Desfaz a gravação do app.
///
/// - `atual == gravado`: devolve `original` (a string não mudou desde a gravação);
/// - senão, se `atual` ainda tem `+fps_max` com o número que o app gravou: devolve o
///   número de `original` (se ele já tinha o token) ou remove só o token e o espaço que
///   o separava;
/// - senão: [`Erro::ArquivoAlteradoPeloJogo`] (o token não é mais do app).
pub fn restaurar(atual: &str, original: &str, gravado: &str) -> Resultado<String> {
    travar(atual)?;
    if atual == gravado {
        return Ok(original.to_owned());
    }
    let nosso = ler_fps_max(gravado)?.ok_or_else(|| Erro::formato("gravado sem +fps_max"))?;
    let Some((tok, num, n)) = localizar(atual)? else {
        return Err(Erro::ArquivoAlteradoPeloJogo);
    };
    if n != nosso {
        return Err(Erro::ArquivoAlteradoPeloJogo);
    }
    if let Some(n0) = ler_fps_max(original)? {
        return Ok(format!("{}{n0}{}", &atual[..num.start], &atual[num.end..]));
    }
    let antes = &atual[..tok.start];
    let depois = &atual[num.end..];
    let sem_antes = antes.trim_end_matches(char::is_whitespace);
    let saida = if sem_antes.len() < antes.len() {
        format!("{sem_antes}{depois}")
    } else {
        format!("{antes}{}", depois.trim_start_matches(char::is_whitespace))
    };
    Ok(saida)
}
