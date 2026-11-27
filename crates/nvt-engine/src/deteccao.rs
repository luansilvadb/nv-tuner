//! Conversão da detecção (`nvt-drs` + `nvt-detect`) para o domínio do
//! `nvt-core`: [`Capabilities`] da GPU e do monitor, [`JogoEntrada`] de cada
//! jogo e os fatos de cada jogo cruzados com a base curada
//! (`regras/jogos.toml`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Mutex;

use nvt_core::{
    Api, Arquitetura, BaseJogos, Capabilities, EstadoVrr, GameFacts, IdJogo, JogoCurado, Versao,
};
use nvt_detect::facts::AnaliseAdiada;
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
    /// Jogos adiados apenas por esta ser a primeira passada com lista legível
    /// e nenhum processo vigiado aberto.
    pub adiadas_primeira_passada: Vec<String>,
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
    avisos_adiamento: BTreeSet<(String, MotivoAdiamento)>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MotivoAdiamento {
    ExeDesconhecido,
    ListagemIncompleta,
    ProcessosIndisponiveis,
}

fn aviso_exe_desconhecido(j: &DetectedGame) -> String {
    format!(
        "{}: análise adiada (exe desconhecido com jogo possivelmente aberto)",
        j.nome
    )
}

impl DetectorJogos {
    /// Detector sobre a base dada, sem memória.
    pub fn new(base: BaseJogos) -> Self {
        Self {
            base,
            anteriores: BTreeMap::new(),
            avisos_adiamento: BTreeSet::new(),
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
        self.detectar_passada(jogos, falhas, servicos, processos, false)
    }

    /// Detecção local com uma nova passada quando o único motivo do
    /// adiamento foi a primeira leitura da lista de processos.
    pub fn detectar_jogos_local<P: FonteProcessos + ?Sized>(
        &mut self,
        jogos: &[DetectedGame],
        falhas: &[FalhaScanner],
        servicos: &ServicosSistema,
        processos: &P,
    ) -> ResultadoDeteccao {
        self.detectar_passada(jogos, falhas, servicos, processos, true)
    }

    fn detectar_passada<P: FonteProcessos + ?Sized>(
        &mut self,
        jogos: &[DetectedGame],
        falhas: &[FalhaScanner],
        servicos: &ServicosSistema,
        processos: &P,
        local: bool,
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
        let mut avisos_adiamento = BTreeSet::new();
        let mut avisar_adiamento = |avisos: &mut Vec<String>, chave: &str, motivo, mensagem| {
            let aviso = (chave.to_owned(), motivo);
            let anterior = self.avisos_adiamento.contains(&aviso);
            if avisos_adiamento.insert(aviso) && !anterior {
                avisos.push(mensagem);
            }
        };
        let mut saida = Vec::new();
        let mut novos = BTreeMap::new();
        let mut adiadas = Vec::new();
        let mut encontrados = Vec::new();
        let mut lista_processos_indisponivel = false;
        let mut pendentes: Vec<_> = (0..jogos.len()).collect();

        loop {
            // Falha fechada: sem a lista não se sabe o que está aberto.
            let lista = match processos.ler() {
                Ok(ps) => Some(ps),
                Err(e) => {
                    lista_processos_indisponivel = true;
                    avisos.push(format!(
                        "{e}: pastas de jogos não lidas nesta varredura (fatos anteriores)"
                    ));
                    None
                }
            };
            let rodando = lista
                .as_ref()
                .map_or_else(Vec::new, |ps| nvt_detect::em_execucao(ps, &vigiados));
            for exe in &rodando {
                adicionar_sem_repetir(&mut encontrados, exe);
            }
            // Só enumera jogos de exe desconhecido após uma lista legível
            // anterior e quando nenhum processo vigiado está em execução.
            let le_desconhecidos = rodando.is_empty() && self.ja_varreu;
            for indice in pendentes {
                let j = &jogos[indice];
                let (chave, exes) = &conhecidos[indice];
                let exe_desconhecido = exes.is_empty() && exe_indicado(j).is_none();
                let analise = match &lista {
                    Some(ps)
                        if j.launcher == nvt_detect::Launcher::Xbox
                            || !exe_desconhecido
                            || le_desconhecidos =>
                    {
                        Some(nvt_detect::facts::analisar_com_lista_de_processos(
                            j, servicos, exes, ps, processos,
                        ))
                    }
                    Some(_) if rodando.is_empty() && !self.ja_varreu => {
                        adiadas.push(indice);
                        continue;
                    }
                    _ => None,
                };
                let entrada = match analise {
                    Some(Ok((AnaliseJogo::Analisado(t), metadados))) => {
                        let Some(e) = jogo_de_traits(metadados.as_ref().unwrap_or(j), &t) else {
                            avisos.push(format!("{}: exe principal não encontrado", j.nome));
                            continue;
                        };
                        e
                    }
                    Some(Ok((AnaliseJogo::EmExecucao { exes: abertos }, _))) => {
                        for exe in &abertos {
                            adicionar_sem_repetir(&mut encontrados, exe);
                        }
                        let confirmados = nvt_detect::facts::jogo_em_execucao(j, exes, &abertos);
                        if confirmados.is_empty() {
                            // Um nome descoberto no disco basta para bloquear a leitura.
                            // O snapshot não informa o caminho do processo.
                            avisar_adiamento(
                                &mut avisos,
                                chave,
                                MotivoAdiamento::ExeDesconhecido,
                                aviso_exe_desconhecido(j),
                            );
                            let Some(entrada) = self.entrada_sem_leitura(j, chave, exes) else {
                                continue;
                            };
                            entrada
                        } else {
                            self.entrada_em_execucao(j, chave, exes, &confirmados)
                        }
                    }
                    Some(Err(AnaliseAdiada::ListagemIncompleta)) => {
                        avisar_adiamento(
                            &mut avisos,
                            chave,
                            MotivoAdiamento::ListagemIncompleta,
                            format!(
                                "{}: análise adiada (listagem de arquivos incompleta)",
                                j.nome
                            ),
                        );
                        let Some(mut entrada) = self.anteriores.get(chave).cloned() else {
                            continue;
                        };
                        entrada.em_execucao = false;
                        entrada
                    }
                    Some(Err(AnaliseAdiada::ProcessosIndisponiveis(e))) => {
                        lista_processos_indisponivel = true;
                        avisar_adiamento(
                            &mut avisos,
                            chave,
                            MotivoAdiamento::ProcessosIndisponiveis,
                            format!("{}: análise adiada ({e})", j.nome),
                        );
                        let Some(entrada) = self.entrada_sem_leitura(j, chave, exes) else {
                            continue;
                        };
                        entrada
                    }
                    None => {
                        let Some(entrada) = self.entrada_sem_leitura(j, chave, exes) else {
                            avisar_adiamento(
                                &mut avisos,
                                chave,
                                MotivoAdiamento::ExeDesconhecido,
                                aviso_exe_desconhecido(j),
                            );
                            continue;
                        };
                        entrada
                    }
                };
                let mut entrada = entrada;
                entrada.id_jogo = casar_na_base(&self.base, &entrada).map(|c| c.id.clone());
                novos.insert(chave.clone(), entrada.clone());
                saida.push((indice, entrada));
            }
            self.ja_varreu |= lista.is_some();
            if !local
                || adiadas.is_empty()
                || !encontrados.is_empty()
                || lista_processos_indisponivel
            {
                break;
            }
            for (_, entrada) in &saida {
                if !entrada.exe.is_empty() {
                    adicionar_sem_repetir(&mut vigiados, &entrada.exe);
                }
            }
            pendentes = std::mem::take(&mut adiadas);
        }
        for &indice in &adiadas {
            avisar_adiamento(
                &mut avisos,
                &conhecidos[indice].0,
                MotivoAdiamento::ExeDesconhecido,
                aviso_exe_desconhecido(&jogos[indice]),
            );
        }
        self.avisos_adiamento = avisos_adiamento;
        self.anteriores = novos;
        // O Xbox só descobre a pasta efetiva ao ler o manifesto protegido.
        saida.sort_by_key(|(indice, _)| jogos[*indice].launcher.prioridade());
        let mut pastas = BTreeSet::new();
        saida.retain(|(_, entrada)| {
            entrada
                .pasta
                .as_deref()
                .is_none_or(|pasta| pastas.insert(nvt_detect::pasta_canonica(pasta)))
        });
        saida.sort_by_key(|(indice, _)| *indice);
        ResultadoDeteccao {
            jogos: saida.into_iter().map(|(_, entrada)| entrada).collect(),
            avisos,
            adiadas_primeira_passada: if encontrados.is_empty() {
                adiadas.into_iter().map(|i| jogos[i].nome.clone()).collect()
            } else {
                Vec::new()
            },
            processos_em_execucao: encontrados,
            lista_processos_indisponivel,
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
            Some(a) if j.launcher == nvt_detect::Launcher::Xbox => {
                let mut anterior = a.clone();
                anterior.em_execucao = false;
                return Some(anterior);
            }
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
        if let Some(a) = self
            .anteriores
            .get(&chave_cache(j))
            .filter(|a| !a.exe.is_empty())
        {
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
            Some(a) if j.launcher == nvt_detect::Launcher::Xbox => {
                let mut anterior = a.clone();
                anterior.em_execucao = true;
                return anterior;
            }
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

/// Varre as bibliotecas no modo local, com nova passada quando o detector
/// ainda não tinha uma lista legível de processos.
pub fn detectar_jogos_da_maquina_local() -> ResultadoDeteccao {
    let mut guarda = DETECTOR_LOCAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rel = nvt_detect::varrer_bibliotecas();
    guarda
        .get_or_insert_with(DetectorJogos::embutido)
        .detectar_jogos_local(
            &rel.jogos,
            &rel.falhas,
            &ServicosSistema::ler(),
            &ProcessosDoSistema,
        )
}

static DETECTOR_LOCAL: Mutex<Option<DetectorJogos>> = Mutex::new(None);

/// Varre as bibliotecas desta máquina (somente leitura). Devolve os jogos com
/// exe principal e avisos (scanners que falharam, jogos sem exe). Jogos em
/// execução não têm a pasta lida ([`detectar_jogos_da_maquina`]).
pub fn jogos_da_maquina() -> (Vec<JogoEntrada>, Vec<String>) {
    let r = detectar_jogos_da_maquina();
    (r.jogos, r.avisos)
}
