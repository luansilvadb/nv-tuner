//! Guarda de COM (MTA) da thread atual, comum ao diagnóstico e à saúde.

use windows::Win32::{
    Foundation::RPC_E_CHANGED_MODE,
    System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize},
};

/// COM ativo na thread enquanto viver. Thread já em outro modo (`RPC_E_CHANGED_MODE`)
/// é aceita sem desfazer nada.
pub(crate) struct Com {
    desfazer: bool,
}

impl Com {
    pub(crate) fn iniciar() -> Option<Self> {
        // SAFETY: inicialização pareada pela guarda; modo já definido é aceito sem desfazer.
        let r = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if r.is_ok() {
            Some(Self { desfazer: true })
        } else if r == RPC_E_CHANGED_MODE {
            Some(Self { desfazer: false })
        } else {
            None
        }
    }

    /// Como [`Com::iniciar`], mas recusa thread já em outro modo (`RPC_E_CHANGED_MODE`).
    pub(crate) fn iniciar_estrito() -> Option<Self> {
        // SAFETY: inicialização pareada pela guarda; só devolve guarda após sucesso.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .ok()?;
        Some(Self { desfazer: true })
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.desfazer {
            // SAFETY: pareado com o CoInitializeEx bem-sucedido.
            unsafe { CoUninitialize() };
        }
    }
}
