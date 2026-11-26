//! Conversão da detecção (`nvt-drs` + `nvt-detect`) para o domínio do
//! `nvt-core`: [`Capabilities`] da GPU e do monitor, [`JogoEntrada`] de cada
//! jogo e os fatos de cada jogo cruzados com a base curada
//! (`regras/jogos.toml`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use nvt_core::{
    Api, Arquitetura, BaseJogos, Capabilities, EstadoVrr, GameFacts, IdJogo, JogoCurado, Versao,
};
use nvt_detect::{
    ANTICHEAT_SO_DURANTE_JOGO, AnaliseJogo, ApiGrafica, Confianca, DetectedGame, FalhaScanner,
    FonteProcessos, GameTraits, ProcessosDoSistema, ServicosSistema,
};
use nvt_drs::InfoExibicao;

use crate::erro::{ErroEngine, Resultado};
use crate::store::DriverStore;

/// Jogo detectado, pronto para ser cruzado com o DRS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JogoEntrada {
    /// Nome no launcher (vira nome do perfil se o app precisar criá-lo).
    pub nome: String,
    /// Launcher onde o jogo foi encontrado (ex.: `Steam`); vazio se desconhecido.
    pub launcher: String,
    /// Nome do arquivo do exe principal (ex.: `League of Legends.exe`).
    pub exe: String,
    pub api: Api,
    /// Versão do `nvngx_dlss.dll`; `None` = sem DLSS.
    pub dlss: Option<Versao>,
    /// Reflex detectado em disco (a base curada vence; ver [`fatos_do_jogo`]).
    pub reflex: Option<bool>,
    /// Anticheat de kernel detectado (a base curada vence).
    pub anticheat_kernel: Option<bool>,
    /// Pasta de instalação.
    pub pasta: Option<PathBuf>,
    /// AppID da Steam (jogo da Steam com identificador numérico).
    pub steam_appid: Option<u32>,
    /// Identificador no launcher (appid, AppName, produto Riot...).
    pub id_launcher: Option<String>,
    /// Jogo da base curada casado pela detecção (exe + AppID). `None` = fora
    /// da base ou ainda não casado (o motor casa de novo ao montar os fatos).
    pub id_jogo: Option<IdJogo>,
    /// Algum exe do jogo estava em execução na varredura: a pasta não foi
    /// lida e os fatos são os da varredura anterior (ou desconhecidos) (F1-09).
    pub em_execucao: bool,
}

impl JogoEntrada {
    /// Jogo mínimo (testes): API desconhecida, sem DLSS, resto desconhecido.
    pub fn new(nome: impl Into<String>, exe: impl Into<String>) -> Self {
        Self {
            nome: nome.into(),
            launcher: String::new(),
            exe: exe.into(),
            api: Api::Desconhecida,
            dlss: None,
            reflex: None,
            anticheat_kernel: None,
            pasta: None,
            steam_appid: None,
            id_launcher: None,
            id_jogo: None,
            em_execucao: false,
        }
    }

    /// Copia os identificadores do jogo detectado (pasta, AppID, id no launcher).
    fn com_ids(mut self, j: &DetectedGame) -> Self {
        self.pasta = Some(j.pasta_instalacao().to_path_buf());
        self.steam_appid = j.steam_appid();
        self.id_launcher = j.id_launcher().map(str::to_owned);
        self
    }
}

/// Capacidades da GPU principal (a primeira da enumeração da NVAPI) e do
/// monitor principal.
///
/// Arquitetura e recursos vêm de `nvt_detect::arch_facts`; VRAM e driver da
/// NVAPI; refresh (milésimos de Hz) e VRR de [`DriverStore::exibicao`]
/// (`nvt_drs::ler_exibicao_principal`). `rebar_ativo` fica desconhecido.
pub fn capacidades<S: DriverStore>(store: &S) -> Resultado<Capabilities> {
    capacidades_com_exibicao(store, store.exibicao())
}

/// Como [`capacidades`], com a exibição já lida (fonte injetável).
pub fn capacidades_com_exibicao<S: DriverStore>(
    store: &S,
    exibicao: InfoExibicao,
) -> Resultado<Capabilities> {
    let gpus = store.gpus()?;
    let gpu = gpus.first().ok_or(ErroEngine::SemGpu)?;
    let (arch_id, impl_id) = gpu
        .arch
        .map(|a| (a.architecture, a.implementation))
        .unwrap_or((0, 0));
    let f = nvt_detect::arch_facts(arch_id, impl_id, &gpu.name);
    let drv = store.versao_driver()?;
    Ok(Capabilities {
        arquitetura: Arquitetura::from_nvapi_id(arch_id),
        tem_tensor_cores: f.tensor_cores,
        tensor_fp8: f.tensor_fp8,
        frame_gen: f.frame_gen,
        mfg: f.mfg,
        rebar_suportado: f.rebar_suportado,
        rebar_ativo: None,
        vram_mb: gpu.vram_kib.map_or(0, |k| k / 1024),
        driver_version: Versao::new(drv.major(), drv.minor(), 0, 0),
        refresh_mhz: exibicao.refresh_mhz,
        vrr: EstadoVrr {
            possivel: exibicao.vrr_possivel,
            ativo: exibicao.vrr_ativo,
        },
    })
}

/// Jogo da base curada para a entrada: o `id_jogo` já casado ou, sem ele, o
/// casamento por exe + AppID ([`BaseJogos::casar`]).
pub fn casar_na_base<'a>(base: &'a BaseJogos, j: &JogoEntrada) -> Option<&'a JogoCurado> {
    j.id_jogo
        .as_ref()
        .and_then(|id| base.jogo(id.as_str()))
        .or_else(|| base.casar(&j.exe, j.steam_appid))
}

/// Fatos do jogo para o avaliador (F1-04). Jogo casado na base: competitivo,
/// e Reflex, limitador nativo e anticheat de kernel vêm da base (ela vence a
/// detecção em disco; anticheat pode ser `false`). Fora da base: os fatos da
/// detecção, `competitivo = false` e limitador desconhecido.
pub fn fatos_do_jogo(
    id: impl Into<String>,
    j: &JogoEntrada,
    curado: Option<&JogoCurado>,
) -> GameFacts {
    GameFacts {
        id: id.into(),
        api: j.api,
        dlss: j.dlss,
        reflex: curado.map_or(j.reflex, |c| Some(c.reflex)),
        anticheat_kernel: curado.map_or(j.anticheat_kernel, |c| Some(c.anticheat_kernel)),
        competitivo: curado.is_some(),
        limitador_nativo: curado.map(|c| c.limitador_nativo),
        jogo_base: curado.map(|c| c.id.clone()),
        orientacoes: curado.map(|c| c.orientacoes.clone()).unwrap_or_default(),
    }
}

/// Converte o resultado do `nvt-detect`. `None` se não houver exe principal.
///
/// - DLSS presente sem versão legível vira `Some(0.0)` (tem DLSS, versão
///   desconhecida).
/// - Reflex: `Some` só com confiança média ou alta (ausência de rastro em disco
///   não prova ausência de Reflex via NVAPI).
/// - Anticheat de kernel: `Some(true)` se detectado; senão desconhecido.
/// - `id_jogo` fica `None` (quem tem a base casa; ver [`DetectorJogos`]).
pub fn jogo_de_traits(j: &DetectedGame, t: &GameTraits) -> Option<JogoEntrada> {
    let exe = t.exe_principal.as_ref()?.file_name()?.to_string_lossy();
    let api = match t.api.valor {
        Some(ApiGrafica::Dx9) => Api::Dx9,
        Some(ApiGrafica::Dx11) => Api::Dx11,
        Some(ApiGrafica::Dx12) => Api::Dx12,
        Some(ApiGrafica::Vulkan) => Api::Vulkan,
        Some(ApiGrafica::OpenGl) => Api::OpenGL,
        None => Api::Desconhecida,
    };
    let dlss = t.dlss.valor.super_resolution.as_ref().map(|d| {
        d.versao
            .as_deref()
            .and_then(|v| v.parse().ok())
            .unwrap_or_default()
    });
    let reflex = (t.reflex.confianca >= Confianca::Media).then_some(t.reflex.valor);
    let anticheat_kernel = t.anticheat_kernel().then_some(true);
    let mut e = JogoEntrada::new(j.nome.clone(), exe.into_owned()).com_ids(j);
    e.launcher = j.launcher.to_string();
    e.api = api;
    e.dlss = dlss;
    e.reflex = reflex;
    e.anticheat_kernel = anticheat_kernel;
    Some(e)
}

/// Resultado de uma detecção ([`DetectorJogos::detectar`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResultadoDeteccao {
    /// Jogos com exe principal. `JogoEntrada::em_execucao` marca os que
    /// estavam abertos (pasta não lida).
    pub jogos: Vec<JogoEntrada>,
    /// Scanners que falharam e jogos sem exe principal.
    pub avisos: Vec<String>,
    /// Processos vigiados em execução (exes dos jogos, processos protegidos
    /// da base e anticheats que só existem durante o jogo), na grafia vigiada.
    pub processos_em_execucao: Vec<String>,
    /// A lista de processos não pôde ser lida: nenhuma pasta de jogo foi
    /// lida nesta varredura (fatos anteriores ou desconhecidos).
    pub lista_processos_indisponivel: bool,
}

impl ResultadoDeteccao {
    /// Jogos em execução.
    pub fn em_execucao(&self) -> impl Iterator<Item = &JogoEntrada> {
        self.jogos.iter().filter(|j| j.em_execucao)
    }
}

/// Detecção de jogos com memória da varredura anterior (F1-09): um jogo em
/// execução não tem a pasta lida; os fatos dele vêm da última varredura em
/// que estava fechado ou, sem ela, ficam desconhecidos.
#[derive(Debug, Clone)]
pub struct DetectorJogos {
    base: BaseJogos,
    /// Última entrada de cada jogo, pela chave de [`chave_cache`].
    anteriores: BTreeMap<String, JogoEntrada>,
    /// Já houve varredura com a lista de processos lida. Antes dela (ex.:
    /// serviço recém-iniciado) a pasta de jogo de exe desconhecido não é lida.
    ja_varreu: bool,
}

/// Chave do cache: launcher + pasta como veio do launcher, sem resolver o
/// caminho no disco (resolver abriria a pasta de um jogo em execução).
fn chave_cache(j: &DetectedGame) -> String {
    format!(
        "{}|{}",
        j.launcher,
        j.pasta_instalacao().to_string_lossy().to_lowercase()
    )
}

/// Nome do arquivo do exe indicado pelo launcher (sem tocar no disco).
fn exe_indicado(j: &DetectedGame) -> Option<String> {
    j.exe_indicado
        .as_deref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
}

fn adicionar_sem_repetir(lista: &mut Vec<String>, nome: &str) {
    if !nome.is_empty() && !lista.iter().any(|x| x.eq_ignore_ascii_case(nome)) {
        lista.push(nome.to_owned());
    }
}

impl DetectorJogos {
    /// Detector sobre a base dada, sem memória.
    pub fn new(base: BaseJogos) -> Self {
        Self {
            base,
            anteriores: BTreeMap::new(),
            ja_varreu: false,
        }
    }

    /// Detector sobre a base embutida (vazia se a base embutida for inválida,
    /// o que o teste de carga das regras impede).
    pub fn embutido() -> Self {
        Self::new(BaseJogos::embutida().unwrap_or_else(|_| BaseJogos::vazia()))
    }

    /// A base curada usada para casar os jogos.
    pub fn base(&self) -> &BaseJogos {
        &self.base
    }

    /// Varre as bibliotecas desta máquina e analisa cada jogo, sem ler a pasta
    /// dos que estão em execução segundo `processos`.
    pub fn detectar<P: FonteProcessos + ?Sized>(&mut self, processos: &P) -> ResultadoDeteccao {
        let rel = nvt_detect::varrer_bibliotecas();
        let servicos = ServicosSistema::ler();
        self.detectar_jogos(&rel.jogos, &rel.falhas, &servicos, processos)
    }

    /// Como [`DetectorJogos::detectar`], sobre uma varredura já feita.
    pub fn detectar_jogos<P: FonteProcessos + ?Sized>(
        &mut self,
        jogos: &[DetectedGame],
        falhas: &[FalhaScanner],
        servicos: &ServicosSistema,
        processos: &P,
    ) -> ResultadoDeteccao {
        let mut avisos: Vec<String> = falhas
            .iter()
            .map(|f| format!("scanner {} falhou: {}", f.launcher, f.erro))
            .collect();
        let conhecidos: Vec<(String, Vec<String>)> = jogos
            .iter()
            .map(|j| (chave_cache(j), self.exes_conhecidos(j)))
            .collect();
        let mut vigiados: Vec<String> = Vec::new();
        for (_, exes) in &conhecidos {
            for e in exes {
                adicionar_sem_repetir(&mut vigiados, e);
            }
        }
        for p in self.base.processos_vigiados() {
            adicionar_sem_repetir(&mut vigiados, &p);
        }
        for p in ANTICHEAT_SO_DURANTE_JOGO {
            adicionar_sem_repetir(&mut vigiados, p);
        }
        for j in jogos {
            if let Some(n) = j.exe_indicado.as_deref().and_then(|p| p.file_name()) {
                adicionar_sem_repetir(&mut vigiados, &n.to_string_lossy());
            }
        }
        // Falha fechada: sem a lista não se sabe o que está aberto, então
        // nenhuma pasta é lida.
        let (rodando, lista_ok) = match processos.ler() {
            Ok(ps) => (nvt_detect::em_execucao(&ps, &vigiados), true),
            Err(e) => {
                avisos.push(format!(
                    "{e}: pastas de jogos não lidas nesta varredura (fatos anteriores)"
                ));
                (Vec::new(), false)
            }
        };
        // Jogo de exe desconhecido (sem exe anterior, sem exe da base pelo
        // AppID, sem exe indicado) não é reconhecível entre os processos: só
        // tem a pasta lida numa varredura com a lista lida, sem nenhum
        // processo vigiado aberto e depois de uma varredura anterior.
        let le_desconhecidos = lista_ok && rodando.is_empty() && self.ja_varreu;

        let mut saida = Vec::new();
        let mut novos = BTreeMap::new();
        for (j, (chave, exes)) in jogos.iter().zip(conhecidos) {
            let exe_desconhecido = exes.is_empty() && exe_indicado(j).is_none();
            if !lista_ok || (exe_desconhecido && !le_desconhecidos) {
                match self.entrada_sem_leitura(j, &chave, &exes) {
                    Some(mut e) => {
                        e.id_jogo = casar_na_base(&self.base, &e).map(|c| c.id.clone());
                        novos.insert(chave, e.clone());
                        saida.push(e);
                    }
                    None => avisos.push(format!(
                        "{}: análise adiada (exe desconhecido com jogo possivelmente aberto)",
                        j.nome
                    )),
                }
                continue;
            }
            let entrada = match nvt_detect::analisar_jogo_se_fechado(j, servicos, &exes, &rodando) {
                AnaliseJogo::Analisado(t) => match jogo_de_traits(j, &t) {
                    Some(e) => e,
                    None => {
                        avisos.push(format!("{}: exe principal não encontrado", j.nome));
                        continue;
                    }
                },
                AnaliseJogo::EmExecucao { exes: abertos } => {
                    self.entrada_em_execucao(j, &chave, &exes, &abertos)
                }
            };
            let mut entrada = entrada;
            entrada.id_jogo = casar_na_base(&self.base, &entrada).map(|c| c.id.clone());
            novos.insert(chave, entrada.clone());
            saida.push(entrada);
        }
        self.anteriores = novos;
        self.ja_varreu |= lista_ok;
        ResultadoDeteccao {
            jogos: saida,
            avisos,
            processos_em_execucao: rodando,
            lista_processos_indisponivel: !lista_ok,
        }
    }

    /// Entrada de um jogo cuja pasta não foi lida sem se saber se está
    /// aberto: os fatos anteriores ou, sem eles, fatos desconhecidos com o exe
    /// indicado ou conhecido. `None` sem nenhum exe sabido.
    fn entrada_sem_leitura(
        &self,
        j: &DetectedGame,
        chave: &str,
        conhecidos: &[String],
    ) -> Option<JogoEntrada> {
        let mut e = match self.anteriores.get(chave) {
            Some(a) => a.clone(),
            None => {
                let exe = exe_indicado(j).or_else(|| conhecidos.first().cloned())?;
                let mut e = JogoEntrada::new(j.nome.clone(), exe);
                e.launcher = j.launcher.to_string();
                e
            }
        };
        e.nome = j.nome.clone();
        e.em_execucao = false;
        Some(e.com_ids(j))
    }

    /// Exes conhecidos de um jogo sem tocar na pasta dele: o exe da varredura
    /// anterior e, para jogo da Steam, os executáveis e processos protegidos
    /// do jogo da base com o mesmo AppID.
    fn exes_conhecidos(&self, j: &DetectedGame) -> Vec<String> {
        let mut exes = Vec::new();
        if let Some(a) = self.anteriores.get(&chave_cache(j)) {
            adicionar_sem_repetir(&mut exes, &a.exe);
        }
        if let Some(appid) = j.steam_appid() {
            for c in self
                .base
                .jogos()
                .iter()
                .filter(|c| c.steam_appid == Some(appid))
            {
                for e in c.executaveis.iter().chain(&c.processos_protegidos) {
                    adicionar_sem_repetir(&mut exes, e);
                }
            }
        }
        exes
    }

    /// Entrada de um jogo em execução: os fatos anteriores ou, sem eles,
    /// fatos desconhecidos com o exe principal que está aberto.
    fn entrada_em_execucao(
        &self,
        j: &DetectedGame,
        chave: &str,
        conhecidos: &[String],
        abertos: &[String],
    ) -> JogoEntrada {
        let mut e = match self.anteriores.get(chave) {
            Some(a) => a.clone(),
            None => {
                // Prefere o exe indicado pelo launcher ou um executável da
                // base (os conhecidos vêm nessa ordem) a um processo protegido.
                let indicado = j
                    .exe_indicado
                    .as_deref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned());
                let exe = indicado
                    .iter()
                    .chain(conhecidos)
                    .find(|c| abertos.iter().any(|a| a.eq_ignore_ascii_case(c)))
                    .or_else(|| abertos.first())
                    .cloned()
                    .unwrap_or_default();
                let mut e = JogoEntrada::new(j.nome.clone(), exe);
                e.launcher = j.launcher.to_string();
                e
            }
        };
        e.nome = j.nome.clone();
        e.em_execucao = true;
        e.com_ids(j)
    }
}

/// Detector do processo: guarda os fatos da varredura anterior entre chamadas
/// de [`jogos_da_maquina`] e [`detectar_jogos_da_maquina`].
static DETECTOR: Mutex<Option<DetectorJogos>> = Mutex::new(None);

/// Varre as bibliotecas desta máquina com os processos do sistema, usando o
/// detector do processo (memória da varredura anterior, F1-09).
pub fn detectar_jogos_da_maquina() -> ResultadoDeteccao {
    let mut guarda = DETECTOR
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guarda
        .get_or_insert_with(DetectorJogos::embutido)
        .detectar(&ProcessosDoSistema)
}

/// Varre as bibliotecas desta máquina (somente leitura). Devolve os jogos com
/// exe principal e avisos (scanners que falharam, jogos sem exe). Jogos em
/// execução não têm a pasta lida ([`detectar_jogos_da_maquina`]).
pub fn jogos_da_maquina() -> (Vec<JogoEntrada>, Vec<String>) {
    let r = detectar_jogos_da_maquina();
    (r.jogos, r.avisos)
}
