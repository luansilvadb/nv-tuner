//! Saúde do PC no serviço (`docs/spec/interface-e-saude.md` §4.3, §4.4, §7, §8.1).
//!
//! - Uma thread própria ([`ThreadSaude`]) lê os sinais de máquina (a cada 10 min) e os
//!   programas de inicialização da máquina (a cada 6 h), fora do atendimento do pipe, e
//!   guarda o resultado num cache ([`SaudeMaquina`]). O pipe só lê o cache: nada aqui
//!   bloqueia o motor.
//! - Nada roda com jogo ou anticheat aberto: o mesmo `Portao` do serviço é consultado
//!   antes de cada passo (e a amostra de CPU feita durante um jogo é descartada).
//! - A limpeza de `C:\Windows\Temp` usa `nvt_manutencao::limpar` no modo fixo
//!   [`MODO_LIMPEZA`]: `Desligada` (build normal) não resolve a raiz nem percorre nada.
//!   Portões: jogo (antes e durante), mutex MSI, política diária persistida em
//!   `manutencao.json` no diretório de estado, e exclusão com o ciclo DRS e com a
//!   instalação de atualização (`Exclusao::tentar_limpeza`).
//! - [`projetar`] monta a resposta `ObterSaude` para a identidade chamadora: máquina +
//!   o que o agente daquela sessão relatou, com as marcas "visto" do próprio SID.
//!
//! O log registra só contagens e bytes (R-L-09). Nenhum processo é criado.

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use nvt_ipc::{
    AlvoLimpeza, FonteInicio, Gravidade, ModoLimpeza, PROGRAMAS_INICIO_MAX, ProgramaInicio,
    ResultadoLimpeza, SINAIS_MAX, Saude, SaudeSessao, Sinal, SituacaoInicio, TipoSinal, ValorSinal,
};
use nvt_manutencao::{
    Contexto, ErroRaiz, Excluidor, Instalacao, MODO_LIMPEZA, PoliticaDiaria, Portao, Raiz, Relogio,
    SistemaArquivos,
};
use nvt_windows::saude::{
    LeitorInicio, LeitorSinaisMaquina, LeituraInicio, ParametrosSinais, ler_inicio_maquina,
    ler_sinais_maquina,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Sinais de máquina: a cada 10 min (R-S-07).
pub(crate) const INTERVALO_SINAIS_S: u64 = 600;
/// Inicialização da máquina: a cada 6 h (R-S-07).
pub(crate) const INTERVALO_INICIO_S: u64 = 6 * 3600;
/// Tentativa de limpeza (a política diária decide se a passada acontece).
pub(crate) const INTERVALO_LIMPEZA_S: u64 = 3600;
/// Média da CPU em repouso (R-S-05).
pub(crate) const JANELA_CPU: Duration = Duration::from_secs(300);
/// CPU só é amostrada 10 min depois do início do Windows (aproximação do logon).
pub(crate) const REPOUSO_APOS_INICIO_S: u64 = 600;
/// Granularidade da thread.
const PASSO_THREAD: Duration = Duration::from_secs(60);
/// Passadas guardadas no histórico técnico (§7.1).
const HISTORICO_LIMPEZA: usize = 30;
/// Itens da linha de base da inicialização da máquina.
const BASE_MAX: usize = 1024;
/// Marcas "visto" por SID.
pub(crate) const VISTOS_MAX: usize = 512;
const ARQUIVO: &str = "manutencao.json";

// ── Cache e estado persistido ─────────────────────────────────────────────

/// O que a thread leu da máquina (só memória).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SaudeMaquina {
    pub(crate) sinais: Vec<Sinal>,
    pub(crate) inicializacao: Vec<ProgramaInicio>,
    pub(crate) limpeza: Option<ResultadoLimpeza>,
}

/// Item da linha de base da inicialização da máquina (sem comando: só o hash).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ItemBase {
    fonte: FonteInicio,
    nome: String,
    /// `None` = fora da linha de base (programa novo).
    hash_base: Option<u64>,
    hash_atual: u64,
    /// Quando o estado atual (novo ou mudado) foi detectado.
    desde_unix: u64,
}

/// `%ProgramData%\nv-tuner\manutencao.json`: política diária, histórico técnico das
/// passadas (só contagens) e linha de base da inicialização da máquina.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct EstadoManutencao {
    pub(crate) ultimas: BTreeMap<AlvoLimpeza, u64>,
    pub(crate) historico: VecDeque<ResultadoLimpeza>,
    pub(crate) base_inicio: Option<Vec<ItemBase>>,
}

pub(crate) trait ArmazemManutencao: Send {
    fn carregar(&self) -> io::Result<EstadoManutencao>;
    fn salvar(&self, estado: &EstadoManutencao) -> io::Result<()>;
}

/// Deve ser construído sobre o diretório de estado privilegiado já verificado.
pub(crate) struct ArquivoManutencao(pub PathBuf);

impl ArmazemManutencao for ArquivoManutencao {
    fn carregar(&self) -> io::Result<EstadoManutencao> {
        match crate::diretorio::abrir_arquivo_privado(&self.0.join(ARQUIVO)) {
            Ok(f) => Ok(serde_json::from_reader(io::BufReader::new(f))?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(EstadoManutencao::default()),
            Err(e) => Err(e),
        }
    }

    fn salvar(&self, estado: &EstadoManutencao) -> io::Result<()> {
        let (tmp, mut f) = (0..100)
            .find_map(|tentativa| {
                let tmp = self
                    .0
                    .join(format!("{ARQUIVO}.{}-{tentativa}.tmp", std::process::id()));
                match crate::diretorio::criar_arquivo_privado(&tmp) {
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => None,
                    r => Some(r.map(|f| (tmp, f))),
                }
            })
            .ok_or_else(|| io::Error::other("sem nome temporário livre para manutencao.json"))??;
        let gravado = (|| {
            serde_json::to_writer(&mut f, estado)?;
            f.flush()?;
            f.sync_all()
        })();
        drop(f);
        let r = gravado.and_then(|()| fs::rename(&tmp, self.0.join(ARQUIVO)));
        if r.is_err() {
            let _ = fs::remove_file(tmp);
        }
        r
    }
}

// ── Ambiente ───────────────────────────────────────────────────────────────

/// Portões e tempo vistos pela thread de saúde.
pub(crate) trait Ambiente {
    /// Jogo ou anticheat aberto (ou lista de processos indisponível).
    fn jogo_aberto(&self) -> bool;
    /// Instalação MSI em andamento (`Global\_MSIExecute`).
    fn instalacao_msi(&self) -> bool;
    /// Reserva a limpeza contra o ciclo DRS e a instalação de atualização;
    /// `false` = algum deles está em curso agora.
    fn reservar_limpeza(&self) -> bool;
    fn liberar_limpeza(&self);
    /// Segundos desde o início do Windows.
    fn desde_inicio_s(&self) -> u64;
    fn agora_ms(&self) -> u64;
    /// Dorme até `d`; `false` = pediram para parar.
    fn esperar(&self, d: Duration) -> bool;
}

struct Adaptador<'a, A: Ambiente>(&'a A);

impl<A: Ambiente> Portao for Adaptador<'_, A> {
    fn jogo_aberto(&self) -> bool {
        self.0.jogo_aberto()
    }
}

impl<A: Ambiente> Instalacao for Adaptador<'_, A> {
    fn em_andamento(&self) -> bool {
        self.0.instalacao_msi()
    }
}

impl<A: Ambiente> Relogio for Adaptador<'_, A> {
    fn agora_ms(&self) -> u64 {
        self.0.agora_ms()
    }
}

// ── Leitor ─────────────────────────────────────────────────────────────────

type ResolverRaiz = Box<dyn Fn() -> Result<Raiz, ErroRaiz> + Send>;

/// Agendador das leituras e da limpeza da máquina.
pub(crate) struct LeitorSaude<L, A, S, E> {
    leitor: L,
    ambiente: A,
    fs: S,
    excluidor: E,
    modo: ModoLimpeza,
    raiz: ResolverRaiz,
    armazem: Box<dyn ArmazemManutencao>,
    estado: EstadoManutencao,
    cache: Arc<Mutex<SaudeMaquina>>,
    proximo_sinais: u64,
    proximo_inicio: u64,
    proximo_limpeza: u64,
}

impl<L, A, S, E> LeitorSaude<L, A, S, E>
where
    L: LeitorSinaisMaquina + LeitorInicio,
    A: Ambiente,
    S: SistemaArquivos,
    E: Excluidor,
{
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn novo(
        leitor: L,
        ambiente: A,
        fs: S,
        excluidor: E,
        modo: ModoLimpeza,
        raiz: ResolverRaiz,
        armazem: Box<dyn ArmazemManutencao>,
        cache: Arc<Mutex<SaudeMaquina>>,
    ) -> Self {
        let estado = armazem.carregar().unwrap_or_else(|e| {
            warn!("saúde: estado de manutenção ilegível ({e}); começando do zero");
            EstadoManutencao::default()
        });
        let limpeza = estado.historico.front().cloned();
        if limpeza.is_some() {
            travar(&cache).limpeza = limpeza;
        }
        Self {
            leitor,
            ambiente,
            fs,
            excluidor,
            modo,
            raiz,
            armazem,
            estado,
            cache,
            proximo_sinais: 0,
            proximo_inicio: 0,
            proximo_limpeza: 0,
        }
    }

    pub(crate) fn ambiente(&self) -> &A {
        &self.ambiente
    }

    #[cfg(test)]
    pub(crate) fn estado(&self) -> &EstadoManutencao {
        &self.estado
    }

    /// Um passo do agendador. Com jogo aberto, não faz nada.
    pub(crate) fn passo(&mut self) {
        if self.ambiente.jogo_aberto() {
            return;
        }
        let agora = self.ambiente.agora_ms() / 1000;
        if agora >= self.proximo_sinais {
            self.proximo_sinais = agora + INTERVALO_SINAIS_S;
            self.ler_sinais(agora);
        }
        if self.ambiente.jogo_aberto() {
            return;
        }
        if agora >= self.proximo_inicio {
            self.proximo_inicio = agora + INTERVALO_INICIO_S;
            self.ler_inicio(agora);
        }
        if self.modo != ModoLimpeza::Desligada && agora >= self.proximo_limpeza {
            self.proximo_limpeza = agora + INTERVALO_LIMPEZA_S;
            self.limpar();
        }
    }

    fn ler_sinais(&mut self, agora: u64) {
        let parametros = ParametrosSinais {
            janela_cpu: (self.ambiente.desde_inicio_s() >= REPOUSO_APOS_INICIO_S)
                .then_some(JANELA_CPU),
            ..ParametrosSinais::default()
        };
        let ambiente = &self.ambiente;
        let mut sinais = ler_sinais_maquina(
            &self.leitor,
            &parametros,
            |d| {
                ambiente.esperar(d);
            },
            agora,
        );
        if self.ambiente.jogo_aberto() {
            // A média cobriu um jogo: não é repouso.
            for s in sinais
                .iter_mut()
                .filter(|s| s.tipo == TipoSinal::CpuRepouso)
            {
                *s = Sinal {
                    tipo: TipoSinal::CpuRepouso,
                    gravidade: Gravidade::Indisponivel,
                    valor: ValorSinal::Nenhum,
                    lido_unix: agora,
                };
            }
        }
        travar(&self.cache).sinais = sinais;
    }

    fn ler_inicio(&mut self, agora: u64) {
        let leitura = ler_inicio_maquina(&self.leitor);
        let programas = diferenca(&mut self.estado.base_inicio, &leitura, agora);
        self.persistir();
        travar(&self.cache).inicializacao = programas;
    }

    fn limpar(&mut self) {
        if self.ambiente.instalacao_msi() || !self.ambiente.reservar_limpeza() {
            return;
        }
        let resultado = match (self.raiz)() {
            Ok(raiz) => {
                let mut politica = PoliticaDiaria {
                    ultimas: self.estado.ultimas.clone(),
                };
                let adaptador = Adaptador(&self.ambiente);
                let mut ctx = Contexto {
                    relogio: &adaptador,
                    portao: &adaptador,
                    instalacao: &adaptador,
                    politica: &mut politica,
                };
                let r = nvt_manutencao::limpar(
                    self.modo,
                    &raiz,
                    &self.fs,
                    &mut self.excluidor,
                    &mut ctx,
                );
                if r.is_some() {
                    self.estado.ultimas = politica.ultimas;
                }
                r
            }
            Err(e) => {
                warn!("limpeza: raiz de C:\\Windows\\Temp indisponível ({e:?})");
                None
            }
        };
        self.ambiente.liberar_limpeza();
        let Some(r) = resultado else { return };
        info!(
            "limpeza ({:?}): {} arquivo(s), {} byte(s), {} pulado(s), interrompida: {:?}",
            r.modo,
            r.arquivos,
            r.bytes,
            r.pulados.values().sum::<u32>(),
            r.interrompida
        );
        self.estado.historico.push_front(r.clone());
        self.estado.historico.truncate(HISTORICO_LIMPEZA);
        self.persistir();
        travar(&self.cache).limpeza = Some(r);
    }

    fn persistir(&self) {
        if let Err(e) = self.armazem.salvar(&self.estado) {
            warn!("saúde: falha ao gravar manutencao.json: {e}");
        }
    }
}

fn travar(cache: &Mutex<SaudeMaquina>) -> std::sync::MutexGuard<'_, SaudeMaquina> {
    cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// FNV-1a de 64 bits: estável entre versões (só detecta mudança de comando).
fn hash(texto: &str) -> u64 {
    texto.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// R-S-02 na máquina: a primeira leitura vira linha de base (nada novo); depois,
/// ausente da base = novo, hash diferente = mudou.
pub(crate) fn diferenca(
    base: &mut Option<Vec<ItemBase>>,
    leitura: &LeituraInicio,
    agora: u64,
) -> Vec<ProgramaInicio> {
    let primeira = base.is_none();
    let itens = base.get_or_insert_with(Vec::new);
    let mut programas = Vec::new();
    for e in leitura.entradas.iter().take(PROGRAMAS_INICIO_MAX) {
        let h = hash(&e.comando);
        let pos = itens
            .iter()
            .position(|i| i.fonte == e.fonte && i.nome == e.nome);
        let item = match pos {
            Some(p) => {
                let item = &mut itens[p];
                if item.hash_atual != h {
                    item.hash_atual = h;
                    item.desde_unix = agora;
                }
                item
            }
            None => {
                itens.push(ItemBase {
                    fonte: e.fonte,
                    nome: e.nome.clone(),
                    hash_base: primeira.then_some(h),
                    hash_atual: h,
                    desde_unix: agora,
                });
                itens.last_mut().expect("item inserido")
            }
        };
        let situacao = match item.hash_base {
            None => SituacaoInicio::Novo,
            Some(b) if b != item.hash_atual => SituacaoInicio::Mudou,
            Some(_) => SituacaoInicio::NaBase,
        };
        let programa = e.programa(situacao, item.desde_unix);
        if programa.validar().is_ok() {
            programas.push(programa);
        }
    }
    // Programas novos que sumiram deixam de existir; os da base ficam.
    itens.retain(|i| {
        i.hash_base.is_some()
            || leitura.fontes_indisponiveis.contains(&i.fonte)
            || leitura
                .entradas
                .iter()
                .any(|e| e.fonte == i.fonte && e.nome == i.nome)
    });
    if itens.len() > BASE_MAX {
        let excesso = itens.len() - BASE_MAX;
        itens.drain(..excesso);
    }
    programas
}

// ── Projeção para a identidade ────────────────────────────────────────────

/// Fontes que o agente pode relatar (sessão do usuário).
pub(crate) fn fonte_da_sessao(fonte: FonteInicio) -> bool {
    matches!(
        fonte,
        FonteInicio::RunUsuario
            | FonteInicio::RunOnceUsuario
            | FonteInicio::PastaUsuario
            | FonteInicio::Tarefa
    )
}

/// Filtra o relato do agente: só fontes da sessão e só o sinal de sessão.
pub(crate) fn filtrar_relato(saude: &SaudeSessao) -> SaudeSessao {
    SaudeSessao {
        sinais: saude
            .sinais
            .iter()
            .filter(|s| s.tipo == TipoSinal::SensorArmazenamento)
            .copied()
            .collect(),
        inicializacao: saude
            .inicializacao
            .iter()
            .filter(|p| fonte_da_sessao(p.fonte))
            .cloned()
            .collect(),
        limpeza: saude
            .limpeza
            .clone()
            .filter(|l| l.alvo == AlvoLimpeza::TempUsuario),
    }
}

/// Marcas "visto" de um SID: `(fonte, nome)` → quando foi marcado.
pub(crate) type Vistos = BTreeMap<(FonteInicio, String), u64>;

/// Resposta `ObterSaude`: máquina + a sessão chamadora (se o agente relatou), com
/// "novo"/"mudou" virando "na base" quando o SID marcou depois da detecção.
pub(crate) fn projetar(
    maquina: &SaudeMaquina,
    sessao: Option<&SaudeSessao>,
    vistos: &Vistos,
) -> Saude {
    let mut sinais: Vec<Sinal> = maquina.sinais.clone();
    for s in sessao.into_iter().flat_map(|s| &s.sinais) {
        if !sinais.iter().any(|m| m.tipo == s.tipo) {
            sinais.push(*s);
        }
    }
    sinais.truncate(SINAIS_MAX);
    let mut inicializacao: Vec<ProgramaInicio> = Vec::new();
    for p in maquina
        .inicializacao
        .iter()
        .chain(sessao.into_iter().flat_map(|s| &s.inicializacao))
    {
        if inicializacao.len() >= PROGRAMAS_INICIO_MAX
            || inicializacao
                .iter()
                .any(|q| q.fonte == p.fonte && q.nome == p.nome)
        {
            continue;
        }
        let mut p = p.clone();
        if p.situacao != SituacaoInicio::NaBase
            && vistos
                .get(&(p.fonte, p.nome.clone()))
                .is_some_and(|&marca| marca >= p.detectado_unix)
        {
            p.situacao = SituacaoInicio::NaBase;
        }
        inicializacao.push(p);
    }
    let limpeza = maquina
        .limpeza
        .iter()
        .chain(sessao.and_then(|s| s.limpeza.as_ref()))
        .cloned()
        .collect();
    Saude {
        sinais,
        inicializacao,
        modo_limpeza: MODO_LIMPEZA,
        limpeza,
    }
}

/// `ObterSaude` e `MarcarInicioVisto` para a identidade do pipe. Sem coordenador,
/// `ObterSaude` responde só com a máquina e `MarcarInicioVisto` falha.
pub(crate) fn atender(
    usuario: Option<&mut crate::usuario::Coordenador>,
    identidade: &nvt_ipc::pipe::IdentidadeCliente,
    req: &nvt_ipc::Requisicao,
    maquina: &SaudeMaquina,
    agora: std::time::Instant,
    unix: u64,
) -> nvt_ipc::Resposta {
    use nvt_ipc::{CodigoErro, Requisicao, Resposta};
    match (req, usuario) {
        (Requisicao::ObterSaude, Some(u)) => u.obter_saude(identidade, maquina, agora),
        (Requisicao::ObterSaude, None) => Resposta::Saude(projetar(maquina, None, &Vistos::new())),
        (Requisicao::MarcarInicioVisto { fonte, nome }, Some(u)) => {
            u.marcar_inicio_visto(identidade, *fonte, nome, maquina, agora, unix)
        }
        (Requisicao::MarcarInicioVisto { .. }, None) => Resposta::erro(
            CodigoErro::Interno,
            "estado de usuário indisponível neste serviço",
        ),
        _ => Resposta::erro(CodigoErro::RequisicaoInvalida, "pedido não é de saúde"),
    }
}

// ── Thread ─────────────────────────────────────────────────────────────────

/// Ambiente real: portão do serviço, mutex MSI, exclusão com DRS/atualização.
pub(crate) struct AmbienteReal {
    portao: nvt_engine::Portao,
    extras: Arc<Mutex<Vec<String>>>,
    base: Option<nvt_core::BaseJogos>,
    exclusao: Arc<crate::laco::Exclusao>,
    msi: nvt_manutencao::windows::MutexMsi,
    parar: Receiver<()>,
}

impl Ambiente for AmbienteReal {
    fn jogo_aberto(&self) -> bool {
        let Some(base) = &self.base else {
            return true;
        };
        let extras = self
            .extras
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let refs: Vec<&str> = extras.iter().map(String::as_str).collect();
        self.portao.verificar(base, &refs).is_err()
    }

    fn instalacao_msi(&self) -> bool {
        self.msi.em_andamento()
    }

    fn reservar_limpeza(&self) -> bool {
        self.exclusao.tentar_limpeza()
    }

    fn liberar_limpeza(&self) {
        self.exclusao.liberar_limpeza();
    }

    fn desde_inicio_s(&self) -> u64 {
        // SAFETY: sem pré-condições.
        unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() / 1000 }
    }

    fn agora_ms(&self) -> u64 {
        nvt_manutencao::RelogioSistema.agora_ms()
    }

    fn esperar(&self, d: Duration) -> bool {
        self.parar.recv_timeout(d) == Err(RecvTimeoutError::Timeout)
    }
}

/// Thread de saúde; parada ao soltar.
pub(crate) struct ThreadSaude {
    parar: Option<Sender<()>>,
    h: Option<JoinHandle<()>>,
}

impl ThreadSaude {
    pub(crate) fn iniciar(
        estado_dir: PathBuf,
        portao: nvt_engine::Portao,
        extras: Arc<Mutex<Vec<String>>>,
        exclusao: Arc<crate::laco::Exclusao>,
        cache: Arc<Mutex<SaudeMaquina>>,
    ) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let base = nvt_core::BaseJogos::embutida()
            .map_err(|e| warn!("saúde: base de jogos indisponível ({e}); portão fechado"))
            .ok();
        let h = thread::Builder::new()
            .name("nvt-saude".into())
            .spawn(move || {
                let ambiente = AmbienteReal {
                    portao,
                    extras,
                    base,
                    exclusao,
                    msi: nvt_manutencao::windows::MutexMsi::padrao(),
                    parar: rx,
                };
                let mut leitor = LeitorSaude::novo(
                    nvt_windows::saude::SaudeWindows,
                    ambiente,
                    nvt_manutencao::windows::SistemaArquivosWindows,
                    nvt_manutencao::windows::ExcluidorReal,
                    MODO_LIMPEZA,
                    Box::new(|| nvt_manutencao::windows::raiz_do_alvo(AlvoLimpeza::TempWindows)),
                    Box::new(ArquivoManutencao(estado_dir)),
                    cache,
                );
                loop {
                    leitor.passo();
                    if !leitor.ambiente().esperar(PASSO_THREAD) {
                        break;
                    }
                }
            })?;
        Ok(Self {
            parar: Some(tx),
            h: Some(h),
        })
    }
}

impl Drop for ThreadSaude {
    fn drop(&mut self) {
        drop(self.parar.take());
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
#[path = "saude_testes.rs"]
mod testes;
