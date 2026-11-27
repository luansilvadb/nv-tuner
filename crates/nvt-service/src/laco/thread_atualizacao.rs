//! Thread própria do atualizador automático e seu executor.

use super::*;

/// O que a thread de atualização executa (o `Atualizador`; nos testes, um falso).
pub(crate) trait Executor: Send + 'static {
    fn tick(&mut self, agora: Instant) -> Option<Duration>;
    fn definir_sessoes_agente(&mut self, sessoes: Vec<nvt_atualizacao::SessaoAgente>);
    fn estado_ipc(&self) -> nvt_ipc::EstadoAtualizacao;
}

impl Executor for crate::atualizacao::Atualizador {
    fn tick(&mut self, agora: Instant) -> Option<Duration> {
        crate::atualizacao::Atualizador::tick(self, agora)
    }

    fn definir_sessoes_agente(&mut self, sessoes: Vec<nvt_atualizacao::SessaoAgente>) {
        crate::atualizacao::Atualizador::definir_sessoes_agente(self, sessoes);
    }

    fn estado_ipc(&self) -> nvt_ipc::EstadoAtualizacao {
        crate::atualizacao::Atualizador::estado_ipc(self)
    }
}

/// Sono máximo da thread sem prazo devolvido.
pub(super) const SONO_MAXIMO: Duration = Duration::from_secs(3600);

#[derive(Default)]
pub(super) struct Caixa {
    acordar: bool,
    sessoes: Option<Vec<nvt_atualizacao::SessaoAgente>>,
    estado: nvt_ipc::EstadoAtualizacao,
}

#[derive(Default)]
pub(super) struct Compartilhado {
    caixa: Mutex<Caixa>,
    cv: std::sync::Condvar,
}

impl Compartilhado {
    fn caixa(&self) -> std::sync::MutexGuard<'_, Caixa> {
        self.caixa
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Thread do atualizador: dorme até o próximo prazo de `tick`, acorda cedo no
/// encerramento (`cancelar`) e quando o laço pede ([`acordar`](Self::acordar)).
pub(crate) struct ThreadAtualizacao {
    comp: Arc<Compartilhado>,
    cancelar: Arc<AtomicBool>,
    h: Option<JoinHandle<()>>,
}

impl ThreadAtualizacao {
    pub(crate) fn iniciar(
        mut exec: impl Executor,
        exclusao: Arc<Exclusao>,
        cancelar: Arc<AtomicBool>,
    ) -> std::io::Result<Self> {
        let comp = Arc::new(Compartilhado::default());
        comp.caixa().estado = exec.estado_ipc();
        let (c, parar) = (comp.clone(), cancelar.clone());
        let h = thread::Builder::new()
            .name("nvt-atualizacao".into())
            .spawn(move || {
                while !parar.load(Ordering::Relaxed) {
                    if let Some(s) = c.caixa().sessoes.take() {
                        exec.definir_sessoes_agente(s);
                    }
                    let r = protegido("atualização", || exec.tick(Instant::now()));
                    // Reserva feita pelo portão neste tick: o lançamento já ocorreu.
                    exclusao.liberar_instalacao();
                    let Some(prazo) = r else {
                        error!("atualização automática desligada até o próximo início do serviço");
                        return;
                    };
                    let estado = exec.estado_ipc();
                    let mut caixa = c.caixa();
                    caixa.estado = estado;
                    let fim = Instant::now() + prazo.unwrap_or(SONO_MAXIMO).min(SONO_MAXIMO);
                    while !caixa.acordar && !parar.load(Ordering::Relaxed) {
                        let resta = fim.saturating_duration_since(Instant::now());
                        if resta.is_zero() {
                            break;
                        }
                        caixa =
                            c.cv.wait_timeout(caixa, resta)
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .0;
                    }
                    caixa.acordar = false;
                }
            })?;
        Ok(Self {
            comp,
            cancelar,
            h: Some(h),
        })
    }

    /// Antecipa o próximo `tick`.
    pub(crate) fn acordar(&self) {
        self.comp.caixa().acordar = true;
        self.comp.cv.notify_all();
    }

    /// Sessões com agente; aplicadas antes do próximo `tick`.
    pub(crate) fn definir_sessoes(&self, sessoes: Vec<nvt_atualizacao::SessaoAgente>) {
        self.comp.caixa().sessoes = Some(sessoes);
    }

    /// Última projeção publicada pela thread.
    pub(crate) fn estado(&self) -> nvt_ipc::EstadoAtualizacao {
        self.comp.caixa().estado.clone()
    }

    /// Sinaliza `cancelar` (a rede desiste) e espera a thread.
    pub(crate) fn parar(&mut self) {
        self.cancelar.store(true, Ordering::Relaxed);
        self.acordar();
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
    }
}

impl Drop for ThreadAtualizacao {
    fn drop(&mut self) {
        self.parar();
    }
}

pub(super) fn estado_atualizacao_padrao() -> nvt_ipc::EstadoAtualizacao {
    nvt_ipc::EstadoAtualizacao {
        versao_instalada: env!("CARGO_PKG_VERSION").into(),
        ..Default::default()
    }
}

pub(super) fn build_windows() -> Option<u32> {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;

    let mut versao = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: estrutura gravável com tamanho indicado no primeiro campo.
    (unsafe { RtlGetVersion(&mut versao) } == 0).then_some(versao.dwBuildNumber)
}
