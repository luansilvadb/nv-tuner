//! Entrada pelo Service Control Manager (crate `windows-service`).
//!
//! Nome do serviço: [`NOME_SERVICO`] (contrato com `instalador/nv-tuner.wxs`).
//! Responde a Stop, Shutdown e Interrogate; reporta StartPending → Running →
//! StopPending → Stopped.

use std::ffi::OsString;
use std::sync::OnceLock;
use std::sync::mpsc;
use std::time::Duration;

use tracing::error;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::{define_windows_service, service_dispatcher};

use crate::laco::{self, Evento, Opcoes};

/// Nome interno do serviço no SCM.
pub const NOME_SERVICO: &str = "nvt-service";

static OPCOES: OnceLock<Opcoes> = OnceLock::new();

define_windows_service!(ffi_main_servico, main_servico);

/// Entrega a thread ao SCM. Retorna quando o serviço para. Falha com
/// `ERROR_FAILED_SERVICE_CONTROLLER_CONNECT` (1063) se o processo não foi
/// iniciado pelo SCM.
pub fn iniciar(op: Opcoes) -> windows_service::Result<()> {
    let _ = OPCOES.set(op);
    service_dispatcher::start(NOME_SERVICO, ffi_main_servico)
}

fn main_servico(_args: Vec<OsString>) {
    if let Err(e) = executar() {
        error!("serviço: {e}");
    }
}

fn status(estado: ServiceState, codigo: u32, espera: Duration) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: estado,
        controls_accepted: if estado == ServiceState::Running {
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
        } else {
            ServiceControlAccept::empty()
        },
        exit_code: if codigo == 0 {
            ServiceExitCode::Win32(0)
        } else {
            ServiceExitCode::ServiceSpecific(codigo)
        },
        checkpoint: 0,
        wait_hint: espera,
        process_id: None,
    }
}

fn executar() -> windows_service::Result<()> {
    let op = OPCOES
        .get()
        .cloned()
        .expect("opções definidas antes do dispatcher");
    let (tx, rx) = mpsc::channel::<Evento>();
    let tx_ctl = tx.clone();
    let tratador = move |c| match c {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            let _ = tx_ctl.send(Evento::Parar);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let h = service_control_handler::register(NOME_SERVICO, tratador)?;
    h.set_service_status(status(
        ServiceState::StartPending,
        0,
        Duration::from_secs(10),
    ))?;
    let h2 = h;
    let r = laco::executar(&op, tx, rx, || {
        if let Err(e) = h2.set_service_status(status(ServiceState::Running, 0, Duration::ZERO)) {
            error!("SCM: falha ao reportar Running: {e}");
        }
    });
    h.set_service_status(status(
        ServiceState::StopPending,
        0,
        Duration::from_secs(30),
    ))?;
    let codigo = match &r {
        Ok(()) => 0,
        Err(e) => {
            error!("serviço terminou com erro: {e}");
            1
        }
    };
    h.set_service_status(status(ServiceState::Stopped, codigo, Duration::ZERO))
}
