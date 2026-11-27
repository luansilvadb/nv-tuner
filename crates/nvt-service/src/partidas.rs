//! Partidas, só com `medicao-partidas`.
//!
//! Com a feature, uma thread própria ([`ThreadPartidas`]) observa a lista de processos
//! (`TH32CS_SNAPPROCESS`, nenhum handle para o jogo), detecta início e fim de partida dos
//! jogos da base curada e dos detectados pelo portão, abre a sessão ETW **só** enquanto
//! há partida ativa, lê a GPU a 1 Hz, agrega e grava pelo [`Armazem`] em
//! `<estado>\partidas` (DACL protegida SYSTEM/Administradores, preparada por
//! [`crate::diretorio::preparar_subpasta_privada`]). A sessão é parada ao fim da
//! partida, no stop do serviço e no desempilhamento de um pânico ([`Drop`] do
//! [`Gravador`]); uma sessão órfã com o nosso nome é parada ao iniciar.
//!
//! Visibilidade (decisão desta tarefa): as partidas são da máquina (um armazém só, sem
//! atribuição por `ProcessIdToSessionId`), e só o SID da sessão de console as vê e
//! apaga. Outro SID recebe lista vazia, "partida não encontrada" e recusa ao apagar.
//!
//! Sem a feature, `ListarPartidas`, `ObterPartida` e `ApagarPartidas` respondem
//! `Erro { RecursoAusente }` e nada de ETW, NVML ou do crate de medição é compilado.

use nvt_ipc::pipe::IdentidadeCliente;
use nvt_ipc::{CodigoErro, Requisicao, Resposta};

/// É uma requisição de partidas?
pub(crate) fn eh_de_partidas(req: &Requisicao) -> bool {
    matches!(
        req,
        Requisicao::ListarPartidas { .. }
            | Requisicao::ObterPartida { .. }
            | Requisicao::ApagarPartidas
    )
}

#[cfg(not(feature = "medicao-partidas"))]
mod imp {
    use super::*;

    /// Sem a feature: nada a guardar.
    #[derive(Debug, Default)]
    pub(crate) struct Partidas;

    impl Partidas {
        pub(crate) fn atender(
            &self,
            _identidade: &IdentidadeCliente,
            _console: Option<&IdentidadeCliente>,
            req: &Requisicao,
        ) -> Option<Resposta> {
            eh_de_partidas(req).then(|| {
                Resposta::erro(
                    CodigoErro::RecursoAusente,
                    "gravação de partidas indisponível neste build",
                )
            })
        }
    }
}

#[cfg(feature = "medicao-partidas")]
mod imp {
    use std::io;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::path::Path;
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
    use std::sync::{Arc, Mutex, PoisonError};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use nvt_ipc::CodigoErroMedicao;
    use nvt_medicao::fachada::{
        EventoPresent, FonteGpu, FontePresents, FonteProcessos, LeituraGpu,
    };
    use nvt_medicao::partida::{BaseJogos, Encerrada, Medidor, POLLING_S};
    use nvt_medicao::persistencia::Armazem;
    use tracing::{error, info, warn};

    use super::*;

    /// Passo da thread (coleta de Presents).
    const PASSO: Duration = Duration::from_millis(100);
    const SEGUNDO_US: u64 = 1_000_000;

    pub(crate) type Compartilhado = Arc<Mutex<Option<Armazem>>>;

    /// Variável de ambiente do serviço que desliga a gravação (Lab-M, condição A).
    pub(crate) const VAR_DESLIGADA: &str = "NVT_MEDICAO_DESLIGADA";

    /// Só o valor exato `1` desliga.
    pub(crate) fn medicao_desligada(valor: Option<&str>) -> bool {
        valor.map(str::trim) == Some("1")
    }

    /// Armazém compartilhado entre a thread e o IPC (`None` = indisponível).
    #[derive(Debug, Default)]
    pub(crate) struct Partidas {
        pub(crate) armazem: Compartilhado,
        pub(crate) thread: Option<ThreadPartidas>,
    }

    impl Partidas {
        /// Prepara a pasta e sobe a thread com as fachadas reais.
        pub(crate) fn iniciar(&mut self, estado_dir: &Path, portao: nvt_engine::Portao) {
            let desligada = medicao_desligada(std::env::var(VAR_DESLIGADA).ok().as_deref());
            self.iniciar_com(estado_dir, desligada, move |a| {
                ThreadPartidas::iniciar(a, portao)
            });
        }

        /// `desligada` (Lab-M, condição A): a pasta continua listável, mas a
        /// thread não sobe; nenhuma sessão ETW nem leitura da GPU.
        pub(crate) fn iniciar_com(
            &mut self,
            estado_dir: &Path,
            desligada: bool,
            subir: impl FnOnce(Compartilhado) -> io::Result<ThreadPartidas>,
        ) {
            let pasta = match abrir_armazem(&estado_dir.join("partidas")) {
                Ok(a) => {
                    *self.armazem.lock().unwrap_or_else(PoisonError::into_inner) = Some(a);
                    true
                }
                Err(e) => {
                    error!("partidas: pasta indisponível ({e}); gravação desligada");
                    false
                }
            };
            if desligada {
                info!(
                    "partidas: gravação DESLIGADA pela variável {VAR_DESLIGADA}=1 \
                     (sem sessão ETW nem leitura da GPU)"
                );
                return;
            }
            if !pasta {
                return;
            }
            match subir(self.armazem.clone()) {
                Ok(t) => self.thread = Some(t),
                Err(e) => error!("partidas: thread indisponível: {e}"),
            }
        }

        /// Para a thread (grava as partidas abertas e para a sessão ETW).
        pub(crate) fn parar(&mut self) {
            drop(self.thread.take());
        }

        pub(crate) fn atender(
            &self,
            identidade: &IdentidadeCliente,
            console: Option<&IdentidadeCliente>,
            req: &Requisicao,
        ) -> Option<Resposta> {
            if !eh_de_partidas(req) {
                return None;
            }
            let guarda = self.armazem.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(armazem) = guarda.as_ref() else {
                return Some(Resposta::erro(
                    CodigoErro::Interno,
                    "armazém de partidas indisponível",
                ));
            };
            let do_console = console.is_some_and(|c| c.sid == identidade.sid);
            Some(atender_armazem(armazem, do_console, req))
        }
    }

    fn abrir_armazem(dir: &Path) -> Result<Armazem, String> {
        crate::diretorio::preparar_subpasta_privada(dir)
            .map_err(|p| crate::diretorio::resumo(&p))?;
        Armazem::abrir(dir).map_err(|e| e.to_string())
    }

    /// Responde a partir do armazém. Só a sessão de console vê e apaga.
    pub(crate) fn atender_armazem(
        armazem: &Armazem,
        do_console: bool,
        req: &Requisicao,
    ) -> Resposta {
        let nao_encontrada =
            || Resposta::erro(CodigoErro::EntradaInvalida, "partida não encontrada");
        match req {
            Requisicao::ListarPartidas { desde_unix, limite } => {
                if !do_console {
                    return Resposta::Partidas {
                        resumos: Vec::new(),
                    };
                }
                match armazem.listar() {
                    Ok(mut v) => {
                        v.retain(|r| r.inicio_unix >= *desde_unix);
                        v.sort_by(|a, b| b.inicio_unix.cmp(&a.inicio_unix).then(b.id.cmp(&a.id)));
                        v.truncate(*limite as usize);
                        Resposta::Partidas { resumos: v }
                    }
                    Err(e) => Resposta::erro(CodigoErro::Interno, format!("partidas: {e}")),
                }
            }
            Requisicao::ObterPartida { id } => {
                if !do_console {
                    return nao_encontrada();
                }
                match armazem.ler(*id) {
                    Ok(p) => Resposta::Partida(p),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => nao_encontrada(),
                    Err(e) => Resposta::erro(CodigoErro::Interno, format!("partidas: {e}")),
                }
            }
            Requisicao::ApagarPartidas => {
                if !do_console {
                    return Resposta::erro(
                        CodigoErro::EntradaInvalida,
                        "só a sessão de console apaga as partidas",
                    );
                }
                match armazem.apagar_todas() {
                    Ok(()) => Resposta::Ok,
                    Err(e) => Resposta::erro(CodigoErro::Interno, format!("partidas: {e}")),
                }
            }
            _ => Resposta::erro(CodigoErro::RequisicaoInvalida, "não é de partidas"),
        }
    }

    /// Abre a fonte de Presents (sessão ETW real; nos testes, um falso).
    pub(crate) type AbrirPresents =
        Box<dyn FnMut() -> Result<Box<dyn FontePresents + Send>, CodigoErroMedicao> + Send>;

    /// Fontes do [`Gravador`].
    pub(crate) struct Fontes {
        pub processos: Box<dyn FonteProcessos + Send>,
        pub gpu: Box<dyn FonteGpu + Send>,
        pub abrir_presents: AbrirPresents,
        pub base: Box<dyn BaseJogos + Send>,
        /// Relógio monotônico em microssegundos, no domínio dos Presents (QPC).
        pub relogio_us: Box<dyn FnMut() -> u64 + Send>,
        pub agora_unix: Box<dyn FnMut() -> u64 + Send>,
    }

    /// Orquestra snapshot, sessão, GPU e gravação. A sessão é parada ao soltar.
    pub(crate) struct Gravador {
        fontes: Fontes,
        medidor: Medidor,
        armazem: Compartilhado,
        sessao: Option<Box<dyn FontePresents + Send>>,
        /// Sessão falhou durante as partidas atuais: não tenta de novo (R-M-07).
        falhou: bool,
        ultimo_snapshot: Option<u64>,
        ultima_gpu: Option<u64>,
        buf: Vec<EventoPresent>,
    }

    impl Gravador {
        pub(crate) fn novo(fontes: Fontes, armazem: Compartilhado) -> Self {
            Self {
                fontes,
                medidor: Medidor::new(),
                armazem,
                sessao: None,
                falhou: false,
                ultimo_snapshot: None,
                ultima_gpu: None,
                buf: Vec::new(),
            }
        }

        #[cfg(test)]
        pub(crate) fn sessao_aberta(&self) -> bool {
            self.sessao.is_some()
        }

        #[cfg(test)]
        pub(crate) fn partidas_ativas(&self) -> usize {
            self.medidor.partidas_ativas()
        }

        /// Poda a retenção (na partida do serviço).
        pub(crate) fn podar(&mut self) {
            let agora = (self.fontes.agora_unix)();
            if let Some(a) = self
                .armazem
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
                && let Err(e) = a.podar(agora)
            {
                warn!("partidas: poda falhou: {e}");
            }
        }

        pub(crate) fn passo(&mut self) {
            let t = (self.fontes.relogio_us)();
            let agora = (self.fontes.agora_unix)();
            let renovar = self
                .ultimo_snapshot
                .is_none_or(|u| t.saturating_sub(u) >= POLLING_S * SEGUNDO_US);
            if renovar {
                self.ultimo_snapshot = Some(t);
                match self.fontes.processos.snapshot() {
                    Ok(s) => {
                        let enc = self.medidor.atualizar(&s, &*self.fontes.base, t, agora);
                        self.gravar(enc, agora);
                    }
                    Err(e) => warn!("partidas: snapshot de processos falhou: {e}"),
                }
            }
            let ativas = self.medidor.partidas_ativas() > 0;
            if !ativas {
                self.parar_sessao();
                self.falhou = false;
                return;
            }
            if self.sessao.is_none() && !self.falhou {
                match (self.fontes.abrir_presents)() {
                    Ok(s) => {
                        info!("partidas: sessão de Presents aberta");
                        self.sessao = Some(s);
                    }
                    Err(c) => {
                        warn!("partidas: sessão de Presents negada ({c:?})");
                        self.medidor.erro(c);
                        self.falhou = true;
                    }
                }
            }
            if let Some(s) = self.sessao.as_mut() {
                match s.coletar(&mut self.buf) {
                    Ok(perdidos) => {
                        let eventos = std::mem::take(&mut self.buf);
                        for ev in &eventos {
                            let enc = self.medidor.present(
                                *ev,
                                &mut *self.fontes.processos,
                                &*self.fontes.base,
                                agora,
                            );
                            self.gravar(enc, agora);
                        }
                        self.buf = eventos;
                        self.buf.clear();
                        self.medidor.perdidos_acumulados(perdidos);
                    }
                    Err(c) => {
                        warn!("partidas: consumidor de Presents falhou ({c:?}); sessão encerrada");
                        self.medidor.erro(c);
                        self.parar_sessao();
                        self.falhou = true;
                    }
                }
            }
            if self
                .ultima_gpu
                .is_none_or(|u| t.saturating_sub(u) >= SEGUNDO_US)
            {
                self.ultima_gpu = Some(t);
                let l: LeituraGpu = self.fontes.gpu.ler();
                self.medidor.gpu(t, l);
            }
        }

        /// Stop do serviço: fecha as partidas abertas, grava e para a sessão.
        pub(crate) fn encerrar(&mut self) {
            let t = (self.fontes.relogio_us)();
            let agora = (self.fontes.agora_unix)();
            let enc = self.medidor.encerrar(t);
            self.gravar(enc, agora);
            self.parar_sessao();
        }

        fn parar_sessao(&mut self) {
            if let Some(mut s) = self.sessao.take() {
                s.parar();
                info!("partidas: sessão de Presents parada");
            }
        }

        fn gravar(&self, encerradas: Vec<Encerrada>, agora: u64) {
            for e in encerradas {
                match e {
                    Ok(p) => {
                        let guarda = self.armazem.lock().unwrap_or_else(PoisonError::into_inner);
                        match guarda.as_ref() {
                            Some(a) => match a.gravar(&p, agora) {
                                Ok(()) => info!(
                                    "partidas: partida de {} gravada ({} s)",
                                    p.resumo.jogo_id, p.resumo.duracao_s
                                ),
                                Err(e) => warn!("partidas: gravação falhou: {e}"),
                            },
                            None => warn!("partidas: armazém indisponível; partida perdida"),
                        }
                    }
                    Err((meta, d)) => {
                        info!("partidas: partida de {} descartada ({d:?})", meta.jogo_id)
                    }
                }
            }
        }
    }

    impl Drop for Gravador {
        fn drop(&mut self) {
            // Também no desempilhamento de um pânico: a sessão ETW nunca fica para trás.
            self.parar_sessao();
        }
    }

    /// Base do serviço: base curada e jogos detectados pelo portão.
    struct BaseServico {
        curada: Option<nvt_core::BaseJogos>,
        portao: nvt_engine::Portao,
    }

    impl BaseJogos for BaseServico {
        fn jogo_de(&self, executavel: &str) -> Option<String> {
            if let Some(j) = self.curada.as_ref().and_then(|b| b.casar(executavel, None)) {
                return Some(j.id.as_str().to_owned());
            }
            self.portao
                .exes_jogos()
                .iter()
                .any(|e| nome_arquivo(e).eq_ignore_ascii_case(executavel))
                .then(|| id_detectado(executavel))
        }
    }

    fn nome_arquivo(e: &str) -> &str {
        e.rsplit(['\\', '/']).next().unwrap_or(e)
    }

    /// ID para jogo detectado fora da base: nome do arquivo sem extensão, `[a-z0-9_-]`.
    pub(crate) fn id_detectado(exe: &str) -> String {
        let base = nome_arquivo(exe);
        let base = base
            .rsplit_once('.')
            .filter(|(_, ext)| ext.eq_ignore_ascii_case("exe"))
            .map_or(base, |(n, _)| n);
        let id: String = base
            .chars()
            .filter_map(|c| {
                let c = c.to_ascii_lowercase();
                (c.is_ascii_alphanumeric() || c == '-' || c == '_').then_some(c)
            })
            .take(64)
            .collect();
        if id.is_empty() { "jogo".into() } else { id }
    }

    /// GPU sem NVML: campos ausentes (R-M-03).
    struct SemGpu;

    impl FonteGpu for SemGpu {
        fn ler(&mut self) -> LeituraGpu {
            LeituraGpu::default()
        }
    }

    fn relogio_qpc_us() -> u64 {
        use windows_sys::Win32::System::Performance::{
            QueryPerformanceCounter, QueryPerformanceFrequency,
        };
        let (mut c, mut f) = (0i64, 0i64);
        // SAFETY: ponteiros locais válidos.
        unsafe {
            QueryPerformanceCounter(&mut c);
            QueryPerformanceFrequency(&mut f);
        }
        (i128::from(c) * 1_000_000 / i128::from(f.max(1))) as u64
    }

    fn fontes_reais(portao: nvt_engine::Portao) -> Fontes {
        let curada = nvt_core::BaseJogos::embutida()
            .map_err(|e| warn!("partidas: base de jogos indisponível ({e})"))
            .ok();
        let gpu: Box<dyn FonteGpu + Send> = match nvt_medicao::nvml::GpuNvml::abrir() {
            Ok(g) => Box::new(g),
            Err(e) => {
                warn!("partidas: NVML indisponível ({e}); sem leituras da GPU");
                Box::new(SemGpu)
            }
        };
        Fontes {
            processos: Box::new(nvt_medicao::processos::SnapshotProcessos),
            gpu,
            abrir_presents: Box::new(|| {
                nvt_medicao::etw::SessaoEtw::iniciar()
                    .map(|s| Box::new(s) as Box<dyn FontePresents + Send>)
                    .map_err(|c| {
                        warn!("partidas: StartTrace falhou (Win32 {c})");
                        CodigoErroMedicao::SessaoPerdida
                    })
            }),
            base: Box::new(BaseServico { curada, portao }),
            relogio_us: Box::new(relogio_qpc_us),
            agora_unix: Box::new(crate::nucleo::agora_unix),
        }
    }

    /// Thread de partidas; parada (e sessão ETW parada) ao soltar.
    #[derive(Debug)]
    pub(crate) struct ThreadPartidas {
        parar: Option<Sender<()>>,
        h: Option<JoinHandle<()>>,
    }

    impl ThreadPartidas {
        fn iniciar(armazem: Compartilhado, portao: nvt_engine::Portao) -> io::Result<Self> {
            Self::com_fontes(armazem, move || fontes_reais(portao), true)
        }

        /// `limpar_orfa`: para a sessão ETW órfã com o nosso nome antes de começar.
        pub(crate) fn com_fontes(
            armazem: Compartilhado,
            fontes: impl FnOnce() -> Fontes + Send + 'static,
            limpar_orfa: bool,
        ) -> io::Result<Self> {
            let (tx, rx) = mpsc::channel();
            let h = thread::Builder::new()
                .name("nvt-partidas".into())
                .spawn(move || {
                    if limpar_orfa && let Err(c) = nvt_medicao::etw::parar_orfa() {
                        warn!("partidas: sessão órfã não pôde ser parada (Win32 {c})");
                    }
                    let r = catch_unwind(AssertUnwindSafe(|| rodar(fontes(), armazem, &rx)));
                    if r.is_err() {
                        error!("partidas: pânico na thread; gravação encerrada");
                    }
                    if limpar_orfa {
                        let _ = nvt_medicao::etw::parar_orfa();
                    }
                })?;
            Ok(Self {
                parar: Some(tx),
                h: Some(h),
            })
        }
    }

    fn rodar(fontes: Fontes, armazem: Compartilhado, parar: &Receiver<()>) {
        let mut g = Gravador::novo(fontes, armazem);
        g.podar();
        loop {
            g.passo();
            match parar.recv_timeout(PASSO) {
                Err(RecvTimeoutError::Timeout) => {}
                _ => break,
            }
        }
        g.encerrar();
    }

    impl Drop for ThreadPartidas {
        fn drop(&mut self) {
            drop(self.parar.take());
            if let Some(h) = self.h.take() {
                let _ = h.join();
            }
        }
    }
}

pub(crate) use imp::*;

#[cfg(test)]
#[path = "partidas_testes.rs"]
mod testes;
