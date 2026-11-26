use std::{cell::Cell, io, ptr::null_mut};

use windows_sys::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WAIT_FAILED, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, MSG,
            MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
            RegisterClassW, WM_CLOSE, WM_ENDSESSION, WM_QUERYENDSESSION, WM_QUIT, WNDCLASSW,
            WS_OVERLAPPED,
        },
    },
    w,
};

thread_local! {
    static ENCERRAR: Cell<bool> = const { Cell::new(false) };
}

pub(super) struct Janela(HWND);

impl Janela {
    pub(super) fn criar() -> io::Result<Self> {
        let nome = w!("NvTunerAgente");
        // Janela de nível superior, oculta, para receber as mensagens de sessão.
        // Uma janela HWND_MESSAGE não recebe os broadcasts do Restart Manager.
        let hwnd = unsafe {
            let instancia = GetModuleHandleW(std::ptr::null());
            if instancia.is_null() {
                return Err(io::Error::last_os_error());
            }
            let classe = WNDCLASSW {
                hInstance: instancia,
                lpfnWndProc: Some(procedimento),
                lpszClassName: nome,
                ..Default::default()
            };
            if RegisterClassW(&classe) == 0 {
                return Err(io::Error::last_os_error());
            }
            CreateWindowExW(
                0,
                nome,
                w!("nv-tuner-agente"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                instancia,
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(hwnd))
    }
}

impl Drop for Janela {
    fn drop(&mut self) {
        // A janela pertence à thread do ciclo.
        unsafe { DestroyWindow(self.0) };
    }
}

unsafe extern "system" fn procedimento(janela: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_QUERYENDSESSION => 1,
        WM_CLOSE => {
            ENCERRAR.set(true);
            0
        }
        WM_ENDSESSION => {
            if wp != 0 {
                ENCERRAR.set(true);
            }
            0
        }
        // Encaminha as demais mensagens ao procedimento padrão.
        _ => unsafe { DefWindowProcW(janela, msg, wp, lp) },
    }
}

pub(super) fn encerrando() -> bool {
    let mut msg = MSG::default();
    // A fila só é processada na thread que criou a janela.
    unsafe {
        while !ENCERRAR.get() && PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                ENCERRAR.set(true);
            } else {
                DispatchMessageW(&msg);
            }
        }
    }
    ENCERRAR.get()
}

pub(super) fn esperar() -> io::Result<()> {
    // Sem handles: aguarda a próxima mensagem ou o próximo ciclo.
    let status = unsafe {
        MsgWaitForMultipleObjectsEx(0, std::ptr::null(), 2_000, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
    };
    if status == WAIT_FAILED {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
