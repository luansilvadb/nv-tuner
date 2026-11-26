//! Consumidor ETW em tempo real dos eventos de Present (R-M-01, §8.2).
//!
//! Sessão própria com nome fixo [`NOME_SESSAO`], consumidor no mesmo processo, só os
//! provedores `Microsoft-Windows-DXGI` e `Microsoft-Windows-D3D9`, nível mínimo e só a
//! palavra-chave de eventos. Nenhum logger de kernel, stack walk ou sessão alheia; uma
//! sessão órfã é parada só se tiver exatamente o nosso nome.
//!
//! GUIDs, IDs e palavras-chave conferidos no PresentMon (GameTechDev/PresentMon, branch
//! main): `PresentData/ETW/Microsoft_Windows_DXGI.h` (GUID
//! `CA11C036-0102-4A2D-A6AD-F03CFED5D3C9`, `Present_Start` id 0x002a, task 9, keyword
//! `0x8000000000000002`) e `PresentData/ETW/Microsoft_Windows_D3D9.h` (GUID
//! `783ACA0A-790E-4D7F-8451-AA850511C6B9`, `Present_Start` id 0x0001, task 1, mesma
//! keyword); a ativação por provedor está em `PresentData/PresentMonTraceSession.cpp`.
//! Só usamos `ProcessId` e `TimeStamp` do `EVENT_HEADER`; o payload não é lido.
//!
//! Termos de controle de sessão aparecem cada um numa única linha (contagem fixada em
//! `crates/nvt-core/tests/proibicoes.rs`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use nvt_ipc::CodigoErroMedicao;
use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_SUCCESS, ERROR_WMI_INSTANCE_NOT_FOUND, WIN32_ERROR,
};
use windows_sys::Win32::System::Diagnostics::Etw as e;
use windows_sys::Win32::System::Performance::QueryPerformanceFrequency;
use windows_sys::core::GUID;

use crate::fachada::{EventoPresent, FontePresents};

pub const NOME_SESSAO: &str = "nv-tuner-partidas";
/// `Microsoft-Windows-DXGI` (PresentMon `Microsoft_Windows_DXGI.h`).
pub const GUID_DXGI: GUID = GUID::from_u128(0xCA11C036_0102_4A2D_A6AD_F03CFED5D3C9);
/// `Microsoft-Windows-D3D9` (PresentMon `Microsoft_Windows_D3D9.h`).
pub const GUID_D3D9: GUID = GUID::from_u128(0x783ACA0A_790E_4D7F_8451_AA850511C6B9);
pub const ID_DXGI_PRESENT_START: u16 = 0x002a;
pub const ID_D3D9_PRESENT_START: u16 = 0x0001;
/// `Events` (0x2) mais o bit do canal Analytic, como no PresentMon.
pub const KEYWORD_PRESENT: u64 = 0x8000_0000_0000_0002;
/// `TRACE_LEVEL_CRITICAL`: os Presents são nível 0 e passam em qualquer nível.
pub const NIVEL: u8 = 1;
/// GUID da nossa sessão (identifica a sessão, não um provedor).
const GUID_SESSAO: GUID = GUID::from_u128(0x6e1d3c2a_8b0f_4c55_9a3e_5d1f0b7a2c41);
const INVALIDO: u64 = u64::MAX;

/// Diz se o evento é um `Present_Start` de um dos dois provedores.
pub fn eh_present_start(provedor: &GUID, id: u16) -> bool {
    let igual = |a: &GUID, b: &GUID| {
        a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
    };
    (igual(provedor, &GUID_DXGI) && id == ID_DXGI_PRESENT_START)
        || (igual(provedor, &GUID_D3D9) && id == ID_D3D9_PRESENT_START)
}

fn largo(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// `EVENT_TRACE_PROPERTIES` seguida do nome, em buffer alinhado.
struct Propriedades {
    buf: Vec<u64>,
}

impl Propriedades {
    fn new() -> Self {
        let base = size_of::<e::EVENT_TRACE_PROPERTIES>();
        let nome = largo(NOME_SESSAO);
        let total = base + nome.len() * 2;
        let mut buf = vec![0u64; total.div_ceil(8)];
        // SAFETY: o buffer tem pelo menos `total` bytes, alinhado a 8, e a estrutura é
        // POD; o nome é copiado logo após a estrutura, no offset informado.
        unsafe {
            let p = buf.as_mut_ptr().cast::<e::EVENT_TRACE_PROPERTIES>();
            (*p).Wnode.BufferSize = total as u32;
            (*p).Wnode.Flags = e::WNODE_FLAG_TRACED_GUID;
            (*p).Wnode.ClientContext = 1; // QPC
            (*p).Wnode.Guid = GUID_SESSAO;
            (*p).LogFileMode = e::EVENT_TRACE_REAL_TIME_MODE;
            (*p).FlushTimer = 1;
            (*p).LoggerNameOffset = base as u32;
            let destino = buf.as_mut_ptr().cast::<u8>().add(base).cast::<u16>();
            std::ptr::copy_nonoverlapping(nome.as_ptr(), destino, nome.len());
        }
        Self { buf }
    }

    fn ptr(&mut self) -> *mut e::EVENT_TRACE_PROPERTIES {
        self.buf.as_mut_ptr().cast()
    }

    fn eventos_perdidos(&self) -> u64 {
        // SAFETY: o buffer começa com a estrutura inicializada em `new`.
        let p = unsafe { &*self.buf.as_ptr().cast::<e::EVENT_TRACE_PROPERTIES>() };
        u64::from(p.EventsLost) + u64::from(p.RealTimeBuffersLost)
    }
}

/// Consulta ou para a sessão com o nosso nome (única chamada de controle).
fn controlar(codigo: e::EVENT_TRACE_CONTROL) -> (WIN32_ERROR, u64) {
    let mut props = Propriedades::new();
    let nome = largo(NOME_SESSAO);
    let semalca = e::CONTROLTRACE_HANDLE { Value: 0 };
    // SAFETY: nome terminado em zero e propriedades com tamanho e offset válidos.
    let r = unsafe { e::ControlTraceW(semalca, nome.as_ptr(), props.ptr(), codigo) };
    (r, props.eventos_perdidos())
}

/// Para uma sessão órfã com exatamente o nosso nome (crash anterior).
pub fn parar_orfa() -> Result<(), u32> {
    match controlar(e::EVENT_TRACE_CONTROL_STOP).0 {
        ERROR_SUCCESS | ERROR_WMI_INSTANCE_NOT_FOUND => Ok(()),
        c => Err(c),
    }
}

struct Contexto {
    tx: Sender<EventoPresent>,
    freq: i64,
    recebidos: AtomicU64,
}

unsafe extern "system" fn ao_evento(registro: *mut e::EVENT_RECORD) {
    // SAFETY: o ETW entrega um registro válido durante a chamada; `UserContext` é o
    // `Contexto` que vive até o fim do consumo (liberado só depois do join).
    unsafe {
        let r = &*registro;
        let h = &r.EventHeader;
        if !eh_present_start(&h.ProviderId, h.EventDescriptor.Id) {
            return;
        }
        let ctx = &*(r.UserContext as *const Contexto);
        ctx.recebidos.fetch_add(1, Ordering::Relaxed);
        let t_us = (i128::from(h.TimeStamp) * 1_000_000 / i128::from(ctx.freq.max(1))) as u64;
        let _ = ctx.tx.send(EventoPresent {
            pid: h.ProcessId,
            t_us,
        });
    }
}

/// Sessão ativa. `Drop` para e limpa a sessão.
pub struct SessaoEtw {
    consumo: u64,
    thread: Option<JoinHandle<WIN32_ERROR>>,
    terminou: std::sync::Arc<AtomicBool>,
    contexto: *mut Contexto,
    rx: Receiver<EventoPresent>,
    ativa: bool,
}

// SAFETY: o ponteiro de contexto só é lido pelo callback e liberado em `parar` após o join.
unsafe impl Send for SessaoEtw {}

impl SessaoEtw {
    /// Abre a sessão. Erros devolvem o código Win32 (acesso negado sem privilégio).
    pub fn iniciar() -> Result<Self, u32> {
        parar_orfa()?;
        let mut controle = e::CONTROLTRACE_HANDLE { Value: 0 };
        let nome = largo(NOME_SESSAO);
        let mut r = ERROR_ALREADY_EXISTS;
        for _ in 0..2 {
            let mut props = Propriedades::new();
            // SAFETY: argumentos válidos; `controle` recebe o handle da sessão.
            r = unsafe { e::StartTraceW(&mut controle, nome.as_ptr(), props.ptr()) };
            if r != ERROR_ALREADY_EXISTS {
                break;
            }
            parar_orfa()?;
        }
        if r != ERROR_SUCCESS {
            return Err(r);
        }
        for provedor in [GUID_DXGI, GUID_D3D9] {
            // SAFETY: handle de sessão válido, GUID em memória local, sem parâmetros extras.
            let r = unsafe {
                e::EnableTraceEx2(
                    controle,
                    &provedor,
                    e::EVENT_CONTROL_CODE_ENABLE_PROVIDER,
                    NIVEL,
                    KEYWORD_PRESENT,
                    0,
                    0,
                    std::ptr::null(),
                )
            };
            if r != ERROR_SUCCESS {
                let _ = parar_orfa();
                return Err(r);
            }
        }

        let mut freq = 0i64;
        // SAFETY: ponteiro local válido.
        unsafe { QueryPerformanceFrequency(&mut freq) };
        let (tx, rx) = mpsc::channel();
        let contexto = Box::into_raw(Box::new(Contexto {
            tx,
            freq,
            recebidos: AtomicU64::new(0),
        }));
        let mut nome_consumo = largo(NOME_SESSAO);
        let mut log = e::EVENT_TRACE_LOGFILEW {
            LoggerName: nome_consumo.as_mut_ptr(),
            Anonymous1: e::EVENT_TRACE_LOGFILEW_0 {
                ProcessTraceMode: e::PROCESS_TRACE_MODE_REAL_TIME
                    | e::PROCESS_TRACE_MODE_EVENT_RECORD,
            },
            Anonymous2: e::EVENT_TRACE_LOGFILEW_1 {
                EventRecordCallback: Some(ao_evento),
            },
            Context: contexto.cast(),
            ..Default::default()
        };
        // SAFETY: estrutura inicializada; nome e contexto vivem além da chamada.
        let consumo = unsafe { e::OpenTraceW(&mut log) }.Value;
        if consumo == INVALIDO {
            let _ = parar_orfa();
            // SAFETY: nenhum callback roda sem consumo aberto.
            drop(unsafe { Box::from_raw(contexto) });
            return Err(windows_sys::Win32::Foundation::ERROR_INVALID_HANDLE);
        }
        let terminou = std::sync::Arc::new(AtomicBool::new(false));
        let sinal = terminou.clone();
        let thread = std::thread::Builder::new()
            .name("nvt-etw".into())
            .spawn(move || {
                let h = e::PROCESSTRACE_HANDLE { Value: consumo };
                // SAFETY: handle de consumo válido até `CloseTrace` em `parar`.
                let r = unsafe { e::ProcessTrace(&h, 1, std::ptr::null(), std::ptr::null()) };
                sinal.store(true, Ordering::Release);
                r
            })
            .map_err(|_| windows_sys::Win32::Foundation::ERROR_NOT_ENOUGH_MEMORY);
        let thread = match thread {
            Ok(t) => t,
            Err(c) => {
                // SAFETY: consumo aberto e nenhuma thread o processando.
                unsafe { e::CloseTrace(e::PROCESSTRACE_HANDLE { Value: consumo }) };
                let _ = parar_orfa();
                drop(unsafe { Box::from_raw(contexto) });
                return Err(c);
            }
        };
        Ok(Self {
            consumo,
            thread: Some(thread),
            terminou,
            contexto,
            rx,
            ativa: true,
        })
    }

    /// Presents recebidos desde o início (para o teste de fumaça).
    pub fn recebidos(&self) -> u64 {
        if self.contexto.is_null() {
            return 0;
        }
        // SAFETY: contexto vivo enquanto `ativa`.
        unsafe { (*self.contexto).recebidos.load(Ordering::Relaxed) }
    }
}

impl FontePresents for SessaoEtw {
    fn coletar(&mut self, destino: &mut Vec<EventoPresent>) -> Result<u64, CodigoErroMedicao> {
        if !self.ativa {
            return Err(CodigoErroMedicao::SessaoPerdida);
        }
        destino.extend(self.rx.try_iter());
        if self.terminou.load(Ordering::Acquire) {
            self.parar();
            return Err(CodigoErroMedicao::SessaoPerdida);
        }
        match controlar(e::EVENT_TRACE_CONTROL_QUERY) {
            (ERROR_SUCCESS, perdidos) => Ok(perdidos),
            _ => {
                self.parar();
                Err(CodigoErroMedicao::SessaoPerdida)
            }
        }
    }

    fn parar(&mut self) {
        if !self.ativa {
            return;
        }
        self.ativa = false;
        // SAFETY: handle de consumo aberto por `OpenTraceW`; fechar faz o consumo retornar.
        unsafe {
            e::CloseTrace(e::PROCESSTRACE_HANDLE {
                Value: self.consumo,
            })
        };
        let _ = parar_orfa();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        // SAFETY: a thread de consumo terminou; nenhum callback usa mais o contexto.
        drop(unsafe { Box::from_raw(self.contexto) });
        self.contexto = std::ptr::null_mut();
    }
}

impl Drop for SessaoEtw {
    fn drop(&mut self) {
        self.parar();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtro_de_eventos() {
        assert!(eh_present_start(&GUID_DXGI, 0x2a));
        assert!(eh_present_start(&GUID_D3D9, 0x01));
        assert!(!eh_present_start(&GUID_DXGI, 0x2b));
        assert!(!eh_present_start(&GUID_D3D9, 0x2a));
        assert!(!eh_present_start(&GUID_SESSAO, 0x2a));
    }

    #[test]
    fn propriedades_trazem_o_nome_exato() {
        let p = Propriedades::new();
        let base = size_of::<e::EVENT_TRACE_PROPERTIES>();
        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(p.buf.as_ptr().cast(), p.buf.len() * 8) };
        let nome: Vec<u16> = bytes[base..]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        assert_eq!(String::from_utf16(&nome).unwrap(), NOME_SESSAO);
    }

    /// Fumaça com a sessão real por 5 s (exige administrador ou Performance Log Users).
    #[test]
    #[ignore]
    fn sessao_real_por_5_segundos() {
        let mut s = SessaoEtw::iniciar().expect("abrir sessão ETW (privilégio?)");
        let mut buf = Vec::new();
        let mut perdidos = 0;
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            perdidos = s.coletar(&mut buf).expect("coletar");
        }
        let recebidos = s.recebidos();
        s.parar();
        eprintln!(
            "presents={} coletados={} perdidos={perdidos}",
            recebidos,
            buf.len()
        );
        // Depois de parar, não sobra sessão com o nosso nome.
        assert_eq!(
            controlar(e::EVENT_TRACE_CONTROL_QUERY).0,
            ERROR_WMI_INSTANCE_NOT_FOUND
        );
    }
}
