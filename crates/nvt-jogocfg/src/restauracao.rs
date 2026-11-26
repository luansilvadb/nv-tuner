//! Restauração por hash (F3-07).
//!
//! - hash atual = `hash_depois` → o original inteiro volta;
//! - senão (o jogo regravou o arquivo), só as chaves que ainda têm o valor gravado pelo
//!   app voltam ao valor anterior; chave que o jogo mudou não é tocada e é informada;
//! - nenhuma chave reversível → [`Erro::ArquivoAlteradoPeloJogo`], nada a gravar.

use crate::{Chave, Erro, Hash, Resultado, definir_valor, lancamento, ler_valor, sha256};

/// Como o app mudou o valor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modo {
    /// O valor inteiro foi trocado.
    Valor,
    /// Só o token `+fps_max` de `LaunchOptions` foi acrescentado/trocado.
    TokenLancamento,
}

/// Registro de uma chave alterada pelo app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChaveGravada {
    pub chave: Chave,
    pub modo: Modo,
    pub valor_antes: String,
    pub valor_gravado: String,
}

/// Resultado do planejamento (nada é gravado aqui).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Restauracao {
    /// O arquivo não mudou desde a gravação: devolver o original inteiro.
    Original(Vec<u8>),
    /// O jogo regravou o arquivo: só `revertidas` voltaram.
    Parcial {
        bytes: Vec<u8>,
        revertidas: Vec<Chave>,
        alteradas_pelo_jogo: Vec<Chave>,
    },
}

pub fn planejar(
    atual: &[u8],
    original: &[u8],
    hash_depois: &Hash,
    chaves: &[ChaveGravada],
) -> Resultado<Restauracao> {
    if sha256(atual) == *hash_depois {
        return Ok(Restauracao::Original(original.to_vec()));
    }
    let mut bytes = atual.to_vec();
    let mut revertidas = Vec::new();
    let mut alteradas_pelo_jogo = Vec::new();
    for c in chaves {
        let valor = match ler_valor(&bytes, &c.chave) {
            Ok(v) => v,
            Err(Erro::ChaveAusente(_)) => {
                alteradas_pelo_jogo.push(c.chave.clone());
                continue;
            }
            Err(e) => return Err(e),
        };
        let novo = match c.modo {
            Modo::Valor if valor == c.valor_gravado => Some(c.valor_antes.clone()),
            Modo::Valor => None,
            Modo::TokenLancamento => {
                match lancamento::restaurar(&valor, &c.valor_antes, &c.valor_gravado) {
                    Ok(s) => Some(s),
                    Err(Erro::ArquivoAlteradoPeloJogo) => None,
                    Err(e) => return Err(e),
                }
            }
        };
        match novo {
            Some(v) => {
                bytes = definir_valor(&bytes, &c.chave, &v)?;
                revertidas.push(c.chave.clone());
            }
            None => alteradas_pelo_jogo.push(c.chave.clone()),
        }
    }
    if revertidas.is_empty() {
        return Err(Erro::ArquivoAlteradoPeloJogo);
    }
    Ok(Restauracao::Parcial {
        bytes,
        revertidas,
        alteradas_pelo_jogo,
    })
}
