use std::io;

use nvt_core::usuario::{RefreshRacional as Taxa, SettingUsuario as Destino, ValorUsuario};
use nvt_ipc::{
    AjusteUsuario, Diagnostico, Fato, FatoTela, MouseSpi, RefreshRacional, SettingUsuario,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValorLido {
    pub valor: ValorUsuario,
    /// Para o token DX, distingue a ausência do REG_SZ da ausência só do token.
    pub registro_dx_presente: bool,
    /// Permite confirmar a restauração quando outros tokens impedem remover o REG_SZ.
    #[serde(default)]
    pub outros_tokens_dx: bool,
}

pub trait LeituraUsuario {
    fn ler(&self, destino: &SettingUsuario) -> io::Result<ValorLido>;
    fn tela(&self) -> Fato<FatoTela>;
    fn diagnosticos(&self) -> Vec<Diagnostico> {
        Vec::new()
    }
}

pub trait EscritaUsuario {
    /// Revalida o valor anterior e preserva os demais tokens no DX. A fachada
    /// chama `autorizar` antes de cada escrita física, inclusive compensação.
    fn escrever(
        &mut self,
        destino: &SettingUsuario,
        anterior: &ValorLido,
        pedido: &ValorLido,
        restauracao: bool,
        autorizar: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()>;
}

pub(crate) fn valido(destino: &SettingUsuario, valor: &ValorLido) -> bool {
    let tipo = match destino {
        SettingUsuario::Refresh { .. } => Destino::TelaRefresh,
        SettingUsuario::MouseAceleracao => Destino::MouseAceleracao,
        SettingUsuario::DxSwapEffectUpgrade => Destino::DxSwapEffectUpgrade,
        SettingUsuario::GameDvrCapturaHistorica => Destino::GamedvrCapturaHistorica,
    };
    tipo.aceita(&valor.valor)
        && match tipo {
            Destino::DxSwapEffectUpgrade => {
                valor.registro_dx_presente
                    || (valor.valor == ValorUsuario::Ausente && !valor.outros_tokens_dx)
            }
            _ => !valor.registro_dx_presente && !valor.outros_tokens_dx,
        }
}

pub(crate) fn iguais(a: &ValorLido, b: &ValorLido) -> bool {
    match (&a.valor, &b.valor) {
        (ValorUsuario::Refresh(a), ValorUsuario::Refresh(b)) => {
            a.comparar(*b) == Some(std::cmp::Ordering::Equal)
        }
        _ => a.valor == b.valor && a.registro_dx_presente == b.registro_dx_presente,
    }
}

/// Restaurar ausência de token pode preservar um REG_SZ criado por terceiros.
pub(crate) fn atingiu(atual: &ValorLido, pedido: &ValorLido) -> bool {
    iguais(atual, pedido)
        || (atual.outros_tokens_dx
            && atual.valor == ValorUsuario::Ausente
            && pedido.valor == ValorUsuario::Ausente
            && !pedido.registro_dx_presente)
}

pub(crate) fn taxa(t: RefreshRacional) -> Taxa {
    Taxa {
        numerador: t.numerador,
        denominador: t.denominador,
    }
}

pub(crate) fn taxa_ipc(t: Taxa) -> RefreshRacional {
    RefreshRacional {
        numerador: t.numerador,
        denominador: t.denominador,
    }
}

impl From<MouseSpi> for ValorLido {
    fn from(m: MouseSpi) -> Self {
        Self {
            valor: ValorUsuario::Spi([m.limiar1, m.limiar2, m.aceleracao]),
            registro_dx_presente: false,
            outros_tokens_dx: false,
        }
    }
}

pub(crate) fn mouse([limiar1, limiar2, aceleracao]: [i32; 3]) -> MouseSpi {
    MouseSpi {
        limiar1,
        limiar2,
        aceleracao,
    }
}

pub(crate) fn valores(ajuste: &AjusteUsuario, atual: &ValorLido) -> (ValorLido, ValorLido) {
    use ValorUsuario::{Ausente, Dword, Refresh, Token};
    let (antes, depois) = match ajuste {
        AjusteUsuario::Refresh { antes, depois, .. } => {
            (Refresh(taxa(*antes)), Refresh(taxa(*depois)))
        }
        AjusteUsuario::MouseAceleracao { antes, depois } => {
            return ((*antes).into(), (*depois).into());
        }
        AjusteUsuario::DxSwapEffectUpgrade { antes, depois } => (
            antes.clone().map_or(Ausente, Token),
            depois.clone().map_or(Ausente, Token),
        ),
        AjusteUsuario::GameDvrCapturaHistorica { antes, depois } => {
            (antes.map_or(Ausente, Dword), depois.map_or(Ausente, Dword))
        }
    };
    (
        ValorLido {
            valor: antes,
            registro_dx_presente: atual.registro_dx_presente,
            outros_tokens_dx: atual.outros_tokens_dx,
        },
        ValorLido {
            registro_dx_presente: atual.registro_dx_presente || matches!(depois, Token(_)),
            outros_tokens_dx: atual.outros_tokens_dx,
            valor: depois,
        },
    )
}

pub(crate) fn mudanca(
    destino: &SettingUsuario,
    antes: ValorLido,
    depois: ValorLido,
) -> Option<AjusteUsuario> {
    use ValorUsuario::{Ausente, Dword, Refresh, Spi, Token};
    Some(match (destino, antes.valor, depois.valor) {
        (SettingUsuario::Refresh { alvo }, Refresh(antes), Refresh(depois)) => {
            AjusteUsuario::Refresh {
                alvo: alvo.clone(),
                antes: taxa_ipc(antes),
                depois: taxa_ipc(depois),
            }
        }
        (SettingUsuario::MouseAceleracao, Spi(antes), Spi(depois)) => {
            AjusteUsuario::MouseAceleracao {
                antes: mouse(antes),
                depois: mouse(depois),
            }
        }
        (SettingUsuario::DxSwapEffectUpgrade, antes, depois) => {
            let token = |v| match v {
                Ausente => Some(None),
                Token(s) => Some(Some(s)),
                _ => None,
            };
            AjusteUsuario::DxSwapEffectUpgrade {
                antes: token(antes)?,
                depois: token(depois)?,
            }
        }
        (SettingUsuario::GameDvrCapturaHistorica, antes, depois) => {
            let dword = |v| match v {
                Ausente => Some(None),
                Dword(n) => Some(Some(n)),
                _ => None,
            };
            AjusteUsuario::GameDvrCapturaHistorica {
                antes: dword(antes)?,
                depois: dword(depois)?,
            }
        }
        _ => return None,
    })
}
