//! Conversões campo a campo entre os tipos da camada usuário de `nvt-core` e
//! os do protocolo `nvt-ipc`. Ficam aqui porque este crate já depende dos
//! dois lados e é usado tanto pelo agente quanto pelo serviço.

use nvt_core::usuario::RefreshRacional as TaxaCore;
use nvt_ipc::{MouseSpi, RefreshRacional as TaxaIpc};

pub fn taxa_core(t: TaxaIpc) -> TaxaCore {
    TaxaCore {
        numerador: t.numerador,
        denominador: t.denominador,
    }
}

pub fn taxa_ipc(t: TaxaCore) -> TaxaIpc {
    TaxaIpc {
        numerador: t.numerador,
        denominador: t.denominador,
    }
}

pub fn spi_ipc([limiar1, limiar2, aceleracao]: [i32; 3]) -> MouseSpi {
    MouseSpi {
        limiar1,
        limiar2,
        aceleracao,
    }
}

pub fn spi_core(m: MouseSpi) -> [i32; 3] {
    [m.limiar1, m.limiar2, m.aceleracao]
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ida_e_volta_preservam_os_campos() {
        let t = TaxaIpc {
            numerador: 143_998,
            denominador: 1000,
        };
        assert_eq!(taxa_ipc(taxa_core(t)), t);
        assert_eq!(spi_core(spi_ipc([6, 10, 1])), [6, 10, 1]);
    }
}
