//! Fachadas das fontes de dados e falsos para teste.
//!
//! As implementações reais (`etw`, `nvml`, `processos`) só existem com a feature
//! `medicao-partidas`; o núcleo e o [`crate::partida::Medidor`] dependem só destas traits.

use std::collections::VecDeque;

use nvt_ipc::CodigoErroMedicao;

/// Um `Present_Start` (DXGI ou D3D9): PID do `EVENT_HEADER` e instante em microssegundos
/// num relógio monotônico (QPC convertido).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventoPresent {
    pub pid: u32,
    pub t_us: u64,
}

/// Leitura da GPU; campo ausente quando a leitura falha (R-M-03).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LeituraGpu {
    pub uso_pct: Option<u8>,
    pub temp_c: Option<u8>,
}

/// Processo do snapshot `TH32CS_SNAPPROCESS`: só PID e nome do arquivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessoVisto {
    pub pid: u32,
    pub executavel: String,
}

/// Fonte de Presents (sessão ETW em tempo real).
pub trait FontePresents {
    /// Move para `destino` os Presents recebidos desde a última chamada. `Ok` traz o
    /// total acumulado de eventos perdidos pela sessão; `Err` encerra a medição (R-M-07).
    fn coletar(&mut self, destino: &mut Vec<EventoPresent>) -> Result<u64, CodigoErroMedicao>;
    /// Para a sessão e libera recursos; idempotente.
    fn parar(&mut self);
}

/// Fonte de leituras da GPU (NVML), consultada a 1 Hz.
pub trait FonteGpu {
    fn ler(&mut self) -> LeituraGpu;
}

/// Fonte de snapshots da lista de processos.
pub trait FonteProcessos {
    fn snapshot(&mut self) -> Result<Vec<ProcessoVisto>, String>;
}

/// Falsos determinísticos para testes do serviço e deste crate.
pub mod falsos {
    use super::*;

    /// Presents enfileirados; `erro` é devolvido na próxima coleta.
    #[derive(Debug, Default)]
    pub struct PresentsFalsos {
        pub fila: Vec<EventoPresent>,
        pub perdidos: u64,
        pub erro: Option<CodigoErroMedicao>,
        pub parado: bool,
    }

    impl FontePresents for PresentsFalsos {
        fn coletar(&mut self, destino: &mut Vec<EventoPresent>) -> Result<u64, CodigoErroMedicao> {
            if let Some(e) = self.erro.take() {
                return Err(e);
            }
            destino.append(&mut self.fila);
            Ok(self.perdidos)
        }

        fn parar(&mut self) {
            self.parado = true;
        }
    }

    /// Devolve as leituras em ordem; esgotadas, campos ausentes.
    #[derive(Debug, Default)]
    pub struct GpuFalsa {
        pub leituras: VecDeque<LeituraGpu>,
    }

    impl FonteGpu for GpuFalsa {
        fn ler(&mut self) -> LeituraGpu {
            self.leituras.pop_front().unwrap_or_default()
        }
    }

    /// Devolve os snapshots em ordem; esgotados, repete o último. Conta as chamadas.
    #[derive(Debug, Default)]
    pub struct ProcessosFalsos {
        pub sequencia: VecDeque<Vec<ProcessoVisto>>,
        pub ultimo: Vec<ProcessoVisto>,
        pub chamadas: usize,
    }

    impl FonteProcessos for ProcessosFalsos {
        fn snapshot(&mut self) -> Result<Vec<ProcessoVisto>, String> {
            self.chamadas += 1;
            if let Some(s) = self.sequencia.pop_front() {
                self.ultimo = s;
            }
            Ok(self.ultimo.clone())
        }
    }
}
