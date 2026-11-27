//! Exclusão mútua entre o ciclo DRS e a instalação, e o portão do atualizador.

use super::*;

/// Exclusão mútua entre o ciclo DRS (thread principal) e o lançamento da
/// instalação (thread do atualizador).
#[derive(Default)]
pub(crate) struct Exclusao {
    ocupacao: Mutex<Ocupacao>,
    cv: std::sync::Condvar,
}

#[derive(Default)]
pub(super) struct Ocupacao {
    drs: bool,
    instalacao: bool,
    /// Passada de limpeza de `C:\Windows\Temp` (thread de saúde).
    limpeza: bool,
}

/// Ciclo DRS em curso; ao soltar, libera a instalação que estiver esperando.
pub(crate) struct GuardaDrs<'a>(&'a Exclusao);

impl Drop for GuardaDrs<'_> {
    fn drop(&mut self) {
        self.0.ocupar().drs = false;
        self.0.cv.notify_all();
    }
}

impl Exclusao {
    fn ocupar(&self) -> std::sync::MutexGuard<'_, Ocupacao> {
        self.ocupacao
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Entra num ciclo DRS, esperando o fim de uma instalação reservada.
    pub(crate) fn ciclo_drs(&self) -> GuardaDrs<'_> {
        let mut o = self.ocupar();
        while o.instalacao {
            o = self
                .cv
                .wait(o)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        o.drs = true;
        GuardaDrs(self)
    }

    /// Reserva a instalação, esperando o ciclo DRS em curso terminar. `false`
    /// se `cancelar` foi sinalizado durante a espera.
    pub(crate) fn reservar_instalacao(&self, cancelar: &AtomicBool) -> bool {
        let mut o = self.ocupar();
        while o.drs || o.limpeza {
            if cancelar.load(Ordering::Relaxed) {
                return false;
            }
            o = self
                .cv
                .wait_timeout(o, Duration::from_millis(100))
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
        o.instalacao = true;
        true
    }

    /// Reserva a limpeza sem esperar: `false` se um ciclo DRS ou uma instalação
    /// estiver em curso. O ciclo DRS não espera a limpeza (o motor não bloqueia);
    /// a instalação espera a limpeza terminar.
    pub(crate) fn tentar_limpeza(&self) -> bool {
        let mut o = self.ocupar();
        if o.drs || o.instalacao || o.limpeza {
            return false;
        }
        o.limpeza = true;
        true
    }

    /// Libera a reserva da limpeza (idempotente).
    pub(crate) fn liberar_limpeza(&self) {
        self.ocupar().limpeza = false;
        self.cv.notify_all();
    }

    /// Libera a reserva (idempotente).
    pub(crate) fn liberar_instalacao(&self) {
        self.ocupar().instalacao = false;
        self.cv.notify_all();
    }

    #[cfg(test)]
    pub(crate) fn instalacao_reservada(&self) -> bool {
        self.ocupar().instalacao
    }
}

/// Portão do atualizador: o mesmo [`nvt_engine::Portao`] do núcleo, com a
/// base curada e os extras atuais, consultado só depois de reservar a
/// instalação contra o ciclo DRS (U-26).
pub(crate) fn portao_atualizacao(
    portao: nvt_engine::Portao,
    extras: Arc<Mutex<Vec<String>>>,
    exclusao: Arc<Exclusao>,
    cancelar: Arc<AtomicBool>,
) -> impl Fn() -> Result<(), nvt_atualizacao::MotivoAdiamento> + Send + 'static {
    let base = nvt_core::BaseJogos::embutida()
        .map_err(|e| error!("atualização: base de jogos indisponível ({e}); portão fechado"))
        .ok();
    move || {
        use nvt_atualizacao::MotivoAdiamento;
        let Some(base) = &base else {
            return Err(MotivoAdiamento::ListaIndisponivel);
        };
        if !exclusao.reservar_instalacao(&cancelar) {
            return Err(MotivoAdiamento::JogoAberto);
        }
        let extras = extras
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let refs: Vec<&str> = extras.iter().map(String::as_str).collect();
        let r = crate::atualizacao::fachadas::motivo_do_portao(portao.verificar(base, &refs));
        if r.is_err() {
            exclusao.liberar_instalacao();
        }
        r
    }
}
