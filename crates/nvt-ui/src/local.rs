//! Modo de visualização: pipeline local **somente leitura**.
//!
//! Usado quando o serviço não está em execução (e, nos dois modos, para
//! listar os jogos, que o protocolo não transporta). Três travas garantem que
//! nada seja gravado:
//!
//! 1. só [`Engine::planejar`] e [`Engine::verificar_deriva`] são chamados —
//!    nunca `aplicar`, `tratar_deriva`, `restaurar` nem `definir_preferencias`;
//! 2. o motor recebe um [`SomenteLeitura`], que recusa toda escrita no DRS
//!    (`criar_*`, `gravar_setting`, `remover_setting`, `salvar`);
//! 3. o estado salvo é lido como texto (sem `Armazem::abrir`, que renomeia um
//!    `estado.json` corrompido) e copiado para um [`Armazem::em_memoria`], que
//!    não grava nada em disco.

use std::path::Path;

use nvt_core::{Alvo, ImpactoVisual as ImpactoRegra, Origem};
use nvt_drs::{
    DriverVersion, Error as ErroDrs, GpuInfo, NvApi, ProfileRef, Result as ResultadoDrs, Setting,
    SettingValue, Snapshot, Status as StatusNvapi,
};
use nvt_engine::{
    AcaoReversao as AcaoMotor, Armazem, CodigoAviso, Diff, DriverStore, DrsReal, Engine,
    MudancaPlanejada, OrigemPerfil, OrigemRegistro, PerfilEncontrado, VERSAO_ESTADO,
};
use nvt_ipc::{
    AcaoHistorico, AcaoReversao, AlertaDeriva, Aplicacao, Aviso, Capacidades, Driver,
    EntradaHistorico, Estado, Gpu, ImpactoVisual, Jogo, JogoIgnorado, MotivoNaoAplicada,
    NivelAviso, OrigemPerfil as OrigemPerfilIpc, Preferencias, Qualidade, RegraAplicada,
    RegraNaoAplicada, ResumoJogo, Reversao, Sincronismo, StatusMudanca, codigo_aviso,
};

use crate::modelo::{Dados, GrupoMudancas, ItemMudanca, Respeitada, ler_iso};
use crate::textos;

/// Limite de entradas do histórico exibidas.
pub const HISTORICO_EXIBIDO: usize = 200;

// ---------------------------------------------------------------------------
// DriverStore que recusa escrita
// ---------------------------------------------------------------------------

/// Envolve um [`DriverStore`] e recusa toda operação de escrita.
pub struct SomenteLeitura<S>(pub S);

fn recusa() -> ErroDrs {
    ErroDrs::Nvapi {
        function: "nv-tuner (modo de visualização)",
        status: StatusNvapi::InvalidUserPrivilege,
        message: textos::RECUSA_VISUALIZACAO.into(),
    }
}

impl<S: DriverStore> DriverStore for SomenteLeitura<S> {
    fn versao_driver(&self) -> ResultadoDrs<DriverVersion> {
        self.0.versao_driver()
    }
    fn gpus(&self) -> ResultadoDrs<Vec<GpuInfo>> {
        let gpus = self.0.gpus()?;
        #[cfg(debug_assertions)]
        if let Some(g) = gpu_simulada() {
            return Ok(vec![g]);
        }
        Ok(gpus)
    }
    fn perfil_do_exe(&self, exe: &str) -> ResultadoDrs<Option<PerfilEncontrado>> {
        self.0.perfil_do_exe(exe)
    }
    fn perfil_existe(&self, nome: &str) -> ResultadoDrs<bool> {
        self.0.perfil_existe(nome)
    }
    fn criar_perfil(&mut self, _: &str) -> ResultadoDrs<()> {
        Err(recusa())
    }
    fn criar_aplicacao(&mut self, _: &str, _: &str) -> ResultadoDrs<()> {
        Err(recusa())
    }
    fn ler_setting(&self, perfil: &ProfileRef, id: u32) -> ResultadoDrs<Option<Setting>> {
        self.0.ler_setting(perfil, id)
    }
    fn gravar_setting(&mut self, _: &ProfileRef, _: u32, _: &SettingValue) -> ResultadoDrs<()> {
        Err(recusa())
    }
    fn remover_setting(&mut self, _: &ProfileRef, _: u32, _: bool) -> ResultadoDrs<()> {
        Err(recusa())
    }
    fn settings_do_perfil(&self, perfil: &ProfileRef) -> ResultadoDrs<Vec<Setting>> {
        self.0.settings_do_perfil(perfil)
    }
    fn capturar(&self, exes: &[&str]) -> ResultadoDrs<Snapshot> {
        self.0.capturar(exes)
    }
    fn salvar(&mut self) -> ResultadoDrs<()> {
        Err(recusa())
    }
}

/// Variável de ambiente que simula outra GPU no modo de visualização (só em
/// build de depuração, para testes e capturas): `ada`, `ampere`,
/// `blackwell`, `turing` ou `gtx16`. O driver real continua sendo lido; só a
/// GPU informada ao motor muda. Nada é gravado (o modo é somente leitura).
#[cfg(debug_assertions)]
pub const VAR_SIMULAR_GPU: &str = "NVT_UI_SIMULAR_GPU";

/// GPU simulada por [`VAR_SIMULAR_GPU`], se definida e conhecida.
#[cfg(debug_assertions)]
fn gpu_simulada() -> Option<GpuInfo> {
    gpu_simulada_de(&std::env::var(VAR_SIMULAR_GPU).ok()?)
}

#[cfg(debug_assertions)]
pub fn gpu_simulada_de(nome: &str) -> Option<GpuInfo> {
    let (architecture, implementation, nome, vram_mb) = match nome {
        "turing" => (
            0x160,
            0x06,
            "NVIDIA GeForce RTX 2060 SUPER (simulada)",
            8192,
        ),
        "gtx16" => (
            0x160,
            0x08,
            "NVIDIA GeForce GTX 1660 SUPER (simulada)",
            6144,
        ),
        "ampere" => (0x170, 0x04, "NVIDIA GeForce RTX 3060 Ti (simulada)", 8192),
        "ada" => (0x190, 0x06, "NVIDIA GeForce RTX 4060 Ti (simulada)", 8192),
        "blackwell" => (0x1B0, 0x06, "NVIDIA GeForce RTX 5070 (simulada)", 12288),
        _ => return None,
    };
    Some(GpuInfo {
        index: 0,
        name: nome.into(),
        arch: Some(nvt_drs::ArchInfo {
            architecture,
            implementation,
            revision: 0,
        }),
        vram_kib: Some(vram_mb * 1024),
    })
}

/// Roda o pipeline local somente leitura.
pub fn calcular() -> Result<Dados, String> {
    let api = NvApi::load().map_err(|e| textos::erro_local(&e.to_string()))?;
    let store = DrsReal::abrir(&api).map_err(|e| textos::erro_local(&e.to_string()))?;
    let agora = agora_unix();
    let salvo = ler_salvo(&Armazem::diretorio_padrao(), agora);
    // Só a leitura: o motor recebe um armazém em memória.
    let mut armazem = Armazem::em_memoria();
    if let Some(e) = salvo.estado {
        armazem.estado = e;
    }
    if let Some(s) = salvo.snapshot {
        // Em memória: nada vai para o disco; serve para as reversões saírem
        // com o valor original certo.
        let _ = armazem.guardar_snapshot_original(s);
    }
    let engine = Engine::new(SomenteLeitura(store), armazem)
        .map_err(|e| textos::erro_local(&e.to_string()))?;
    let (entradas, _) = nvt_engine::jogos_da_maquina();
    let diff = engine
        .planejar(&entradas)
        .map_err(|e| textos::erro_local(&e.to_string()))?;
    let alertas = alertas_deriva(&engine);
    let gpu = engine
        .store()
        .gpus()
        .ok()
        .and_then(|g| g.into_iter().next());
    let ramo = engine.store().versao_driver().ok().map(|v| v.branch);
    let mut dados = montar_dados(&engine, &diff, gpu.as_ref(), ramo, alertas);
    let mut avisos = salvo.avisos;
    if !dados.ignorados.is_empty() {
        avisos.push(Aviso::jogos_ignorados(&dados.ignorados, agora));
    }
    avisos.extend(diff.plano.avisos.iter().map(|a| aviso_do_plano(*a, agora)));
    nvt_ipc::ordenar_avisos(&mut avisos);
    if let Some(e) = &mut dados.estado {
        e.avisos = avisos;
    }
    dados.jogos = jogos_ipc(&engine, &diff);
    Ok(dados)
}

/// Aviso do plano (F1-15) no formato do protocolo, como no serviço.
pub fn aviso_do_plano(a: nvt_core::AvisoPlano, quando: u64) -> Aviso {
    match a {
        nvt_core::AvisoPlano::GsyncSemVrr => Aviso::gsync_sem_vrr(quando),
        nvt_core::AvisoPlano::VrrDesligadoNoPainel => Aviso::vrr_desligado_no_painel(quando),
    }
}

fn agora_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

// ---------------------------------------------------------------------------
// Estado salvo (somente leitura)
// ---------------------------------------------------------------------------

/// Estado e snapshot lidos do disco, sem alterar nada.
#[derive(Debug, Default)]
pub struct Salvo {
    pub estado: Option<nvt_engine::Estado>,
    pub snapshot: Option<Snapshot>,
    pub avisos: Vec<Aviso>,
}

/// Conteúdo de um arquivo: `Ok(None)` = não existe.
pub type Conteudo = Result<Option<String>, String>;

fn ler_arquivo(p: &Path) -> Conteudo {
    match std::fs::read_to_string(p) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

/// Lê o estado do serviço em `dir` sem nunca escrever (ao contrário de
/// `Armazem::abrir`, que renomeia um arquivo corrompido e grava o estado
/// recuperado).
pub fn ler_salvo(dir: &Path, agora: u64) -> Salvo {
    let mut salvo = interpretar_salvo(
        ler_arquivo(&dir.join("estado.json")),
        ler_arquivo(&dir.join("estado.anterior.json")),
        ler_arquivo(&dir.join("snapshot-original.json")),
        agora,
    );
    if let Some(a) =
        interpretar_marcador_quarentena(ler_arquivo(&dir.join("snapshot-perdido.json")), agora)
    {
        salvo.avisos.push(a);
    }
    salvo
}

/// Código do protocolo para um código do motor (correspondência exaustiva:
/// um código novo no motor não compila sem o seu par aqui).
pub fn codigo_ipc(c: CodigoAviso) -> &'static str {
    match c {
        CodigoAviso::EstadoCorrompido => codigo_aviso::ESTADO_CORROMPIDO,
        CodigoAviso::EstadoRecuperado => codigo_aviso::ESTADO_RECUPERADO,
        CodigoAviso::EstadoReiniciado => codigo_aviso::ESTADO_REINICIADO,
        CodigoAviso::SnapshotIndisponivel => codigo_aviso::SNAPSHOT_INDISPONIVEL,
        CodigoAviso::SnapshotOriginalPerdido => codigo_aviso::SNAPSHOT_ORIGINAL_PERDIDO,
    }
}

/// Aviso local com o código e o texto da UI para um código do motor.
fn aviso_motor(nivel: NivelAviso, c: CodigoAviso, detalhe: Option<String>, agora: u64) -> Aviso {
    Aviso::novo(
        nivel,
        codigo_ipc(c),
        textos::texto_aviso_motor(c),
        detalhe,
        agora,
    )
}

/// Marcador `snapshot-perdido.json` (gravado pelo serviço depois de uma
/// quarentena): mesmo aviso do motor, `snapshot_original_perdido`. Erro de
/// leitura vira `estado_aviso`.
pub fn interpretar_marcador_quarentena(marcador: Conteudo, agora: u64) -> Option<Aviso> {
    match marcador {
        Ok(None) => None,
        Ok(Some(s)) => {
            let detalhe = serde_json::from_str::<serde_json::Value>(&s)
                .ok()
                .and_then(|v| v.get("detalhe").and_then(|d| d.as_str()).map(str::to_owned))
                .unwrap_or_else(|| s.trim().to_owned());
            Some(aviso_motor(
                NivelAviso::Atencao,
                CodigoAviso::SnapshotOriginalPerdido,
                Some(format!("snapshot-perdido.json: {detalhe}")),
                agora,
            ))
        }
        Err(e) => Some(Aviso::novo(
            NivelAviso::Atencao,
            codigo_aviso::ESTADO_AVISO,
            textos::LOCAL_ESTADO_ILEGIVEL,
            Some(e),
            agora,
        )),
    }
}

fn estado_valido(s: &str) -> Result<nvt_engine::Estado, String> {
    let e: nvt_engine::Estado = serde_json::from_str(s).map_err(|e| e.to_string())?;
    if e.versao != VERSAO_ESTADO {
        return Err(format!(
            "versão {} não suportada (esperada {VERSAO_ESTADO})",
            e.versao
        ));
    }
    Ok(e)
}

/// Mesma decisão do `Armazem::abrir` (estado inválido → cópia de segurança →
/// estado limpo; snapshot ilegível → restauração bloqueada), só que em
/// avisos e sem tocar nos arquivos.
pub fn interpretar_salvo(
    estado: Conteudo,
    anterior: Conteudo,
    snapshot: Conteudo,
    agora: u64,
) -> Salvo {
    let mut r = Salvo::default();
    match estado {
        // Ausente: como o motor, usa a cópia de segurança se for válida
        // (só leitura; nada é renomeado nem gravado).
        Ok(None) => {
            if let Some(Ok(e)) = anterior.ok().flatten().map(|s| estado_valido(&s)) {
                r.estado = Some(e);
                r.avisos.push(Aviso::novo(
                    NivelAviso::Atencao,
                    codigo_ipc(CodigoAviso::EstadoRecuperado),
                    textos::LOCAL_ESTADO_AUSENTE_RECUPERAVEL,
                    Some("estado.json ausente; lido de estado.anterior.json".into()),
                    agora,
                ));
            }
        }
        Ok(Some(s)) => match estado_valido(&s) {
            Ok(e) => r.estado = Some(e),
            Err(motivo) => {
                r.avisos.push(aviso_motor(
                    NivelAviso::Atencao,
                    CodigoAviso::EstadoCorrompido,
                    Some(format!("estado.json: {motivo}")),
                    agora,
                ));
                match anterior.ok().flatten().map(|s| estado_valido(&s)) {
                    Some(Ok(e)) => {
                        r.estado = Some(e);
                        r.avisos.push(aviso_motor(
                            NivelAviso::Info,
                            CodigoAviso::EstadoRecuperado,
                            None,
                            agora,
                        ));
                    }
                    _ => r.avisos.push(aviso_motor(
                        NivelAviso::Atencao,
                        CodigoAviso::EstadoReiniciado,
                        None,
                        agora,
                    )),
                }
            }
        },
        Err(e) => r.avisos.push(Aviso::novo(
            NivelAviso::Atencao,
            codigo_aviso::ESTADO_AVISO,
            textos::LOCAL_ESTADO_ILEGIVEL,
            Some(e),
            agora,
        )),
    }
    match snapshot {
        Ok(None) => {}
        Ok(Some(s)) => match Snapshot::from_json(&s) {
            Ok(snap) => r.snapshot = Some(snap),
            Err(e) => r.avisos.push(Aviso::snapshot_indisponivel(
                format!("snapshot-original.json ilegível: {e}"),
                agora,
            )),
        },
        Err(e) => r.avisos.push(Aviso::novo(
            NivelAviso::Atencao,
            codigo_aviso::ESTADO_AVISO,
            textos::LOCAL_ESTADO_ILEGIVEL,
            Some(e),
            agora,
        )),
    }
    r
}

fn alertas_deriva<S: DriverStore>(engine: &Engine<S>) -> Vec<AlertaDeriva> {
    let Ok(deriva) = engine.verificar_deriva() else {
        return Vec::new();
    };
    let agora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    deriva
        .divergencias
        .iter()
        .map(|d| AlertaDeriva {
            perfil: perfil_de_alvo(&d.esperado.alvo),
            setting: d.esperado.setting.clone(),
            nome_legivel: nome_legivel(engine, &d.esperado.setting),
            esperado: valor(engine, &d.esperado.setting, Some(d.esperado.valor))
                .unwrap_or_default(),
            encontrado: valor(engine, &d.esperado.setting, d.atual),
            detectado_unix: agora,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Conversões
// ---------------------------------------------------------------------------

fn perfil_de_alvo(a: &Alvo) -> String {
    match a {
        Alvo::Global => "global".into(),
        Alvo::Jogo(id) => id.clone(),
    }
}

fn nome_do_alvo(a: &Alvo) -> String {
    match a {
        Alvo::Global => textos::PERFIL_GLOBAL.into(),
        Alvo::Jogo(id) => id.clone(),
    }
}

fn nome_legivel<S: DriverStore>(e: &Engine<S>, setting: &str) -> String {
    if let Some(n) = textos::nome_setting(setting) {
        return n.into();
    }
    e.regras()
        .catalogo
        .get(setting)
        .map(|d| {
            d.descricao
                .split(" (")
                .next()
                .unwrap_or(&d.descricao)
                .to_string()
        })
        .unwrap_or_else(|| setting.to_string())
}

/// Valor legível; `None` = não definido.
fn valor<S: DriverStore>(e: &Engine<S>, setting: &str, v: Option<u32>) -> Option<String> {
    let v = v?;
    let def = e.regras().catalogo.get(setting);
    if setting == "limite_fps" {
        return Some(if v == 0 {
            textos::nome_valor("off")
        } else {
            format!("{v} FPS")
        });
    }
    Some(match def.and_then(|d| d.nome_do_valor(v)) {
        Some(n) => textos::nome_valor(n),
        None => format!("{v} (0x{v:X})"),
    })
}

pub fn impacto_ipc(i: ImpactoRegra) -> ImpactoVisual {
    match i {
        ImpactoRegra::Nenhum => ImpactoVisual::Nenhum,
        ImpactoRegra::Leve => ImpactoVisual::Baixo,
        ImpactoRegra::Forte => ImpactoVisual::Alto,
    }
}

pub fn preferencias_ipc(p: &nvt_core::Preferences) -> Preferencias {
    Preferencias {
        sincronismo: match p.sincronismo {
            nvt_core::Sincronismo::SemSync => Sincronismo::Desligado,
            nvt_core::Sincronismo::GSync => Sincronismo::GSync,
            nvt_core::Sincronismo::VSync => Sincronismo::VSync,
        },
        qualidade: match p.qualidade {
            nvt_core::Qualidade::Desempenho => Qualidade::Desempenho,
            nvt_core::Qualidade::Equilibrado => Qualidade::Equilibrado,
            nvt_core::Qualidade::Fidelidade => Qualidade::Qualidade,
        },
        limite_fps: p.limite_fps,
    }
}

pub fn acao_de_origem(o: &OrigemRegistro) -> AcaoHistorico {
    match o {
        OrigemRegistro::Plano { .. } | OrigemRegistro::CriacaoPerfil => AcaoHistorico::Aplicacao,
        OrigemRegistro::Reaplicacao { .. } => AcaoHistorico::Rollback,
        OrigemRegistro::Usuario => AcaoHistorico::DerivaDetectada,
        OrigemRegistro::Restauracao => AcaoHistorico::Restauracao,
    }
}

fn item<S: DriverStore>(
    e: &Engine<S>,
    m: &nvt_core::SettingChange,
    status: StatusMudanca,
    experimental: bool,
) -> ItemMudanca {
    let regra_id = m.origem.regra().unwrap_or("");
    let regra = e.regras().regras.iter().find(|r| r.id == regra_id);
    ItemMudanca {
        setting: m.setting.clone(),
        nome_legivel: nome_legivel(e, &m.setting),
        antes: valor(e, &m.setting, m.valor_anterior),
        depois: valor(e, &m.setting, Some(m.valor)),
        motivo: m.explicacao.clone(),
        regra: match &m.origem {
            Origem::Regra { id } => id.clone(),
            Origem::Preferencia {
                preferencia,
                regra: None,
            } => format!("preferência: {preferencia}"),
            Origem::Preferencia { regra: Some(r), .. } => r.clone(),
        },
        impacto: regra.map_or(ImpactoVisual::Nenhum, |r| impacto_ipc(r.impacto_visual)),
        status,
        camada: nvt_ipc::Camada::Driver,
        experimental: if experimental {
            nvt_ipc::EstadoExperimental::Aceitavel
        } else {
            nvt_ipc::EstadoExperimental::Estavel
        },
    }
}

fn montar_dados<S: DriverStore>(
    e: &Engine<S>,
    diff: &Diff,
    gpu: Option<&GpuInfo>,
    ramo: Option<String>,
    alertas_deriva: Vec<AlertaDeriva>,
) -> Dados {
    let c = &diff.capacidades;
    let est = e.estado();

    // Alvos na ordem: global, depois jogos.
    let mut alvos: Vec<Alvo> = vec![Alvo::Global];
    alvos.extend(diff.jogos.iter().map(|j| Alvo::Jogo(j.perfil.clone())));
    let mut grupos = Vec::new();
    for a in &alvos {
        let mut itens: Vec<ItemMudanca> = diff
            .mudancas_de(a)
            .map(|m: &MudancaPlanejada| item(e, &m.mudanca, StatusMudanca::Pendente, false))
            .collect();
        itens.extend(
            diff.conformes_de(a)
                .map(|m| item(e, &m.mudanca, StatusMudanca::JaNoValor, false)),
        );
        for s in diff.plano.experimentais.iter().filter(|s| &s.alvo == a) {
            itens.extend(
                s.mudancas
                    .iter()
                    .map(|m| item(e, m, StatusMudanca::Pendente, true)),
            );
        }
        if !itens.is_empty() {
            grupos.push(GrupoMudancas {
                perfil: perfil_de_alvo(a),
                nome_legivel: nome_do_alvo(a),
                itens,
            });
        }
    }

    let respeitadas = diff
        .plano
        .respeitadas
        .iter()
        .map(|r| Respeitada {
            perfil: perfil_de_alvo(&r.alvo),
            nome_perfil: nome_do_alvo(&r.alvo),
            setting: r.mudanca.setting.clone(),
            nome_legivel: nome_legivel(e, &r.mudanca.setting),
            motivo: est
                .travadas
                .iter()
                .find(|t| t.alvo == r.alvo && t.setting == r.mudanca.setting)
                .map_or_else(|| r.mudanca.explicacao.clone(), |t| t.motivo.clone()),
        })
        .collect();

    let historico: Vec<EntradaHistorico> = est
        .historico
        .iter()
        .enumerate()
        .rev()
        .take(HISTORICO_EXIBIDO)
        .map(|(i, h)| EntradaHistorico {
            camada: nvt_ipc::Camada::Driver,
            id: i as u64 + 1,
            quando_unix: ler_iso(&h.data).unwrap_or(0),
            acao: acao_de_origem(&h.origem),
            perfil: Some(h.perfil.clone()),
            setting: (!h.setting.is_empty()).then(|| nome_legivel(e, &h.setting)),
            antes: valor(e, &h.setting, h.antes),
            depois: valor(e, &h.setting, h.depois),
            descricao: h.motivo.clone(),
        })
        .collect();

    let ultima_aplicacao = est
        .historico
        .iter()
        .rev()
        .find(|h| matches!(h.origem, OrigemRegistro::Plano { .. }))
        .and_then(|h| ler_iso(&h.data))
        .map(|quando| Aplicacao {
            quando_unix: quando,
            mudancas: est.esperado.len() as u32,
            falhas: 0,
        });

    let estado = Estado {
        agente: Default::default(),
        diagnosticos: Vec::new(),
        versao_servico: String::new(),
        driver: Some(Driver {
            versao: c.driver_version.to_string(),
            ramo,
        }),
        gpu: gpu.map(|g| Gpu {
            nome: g.name.clone(),
            arquitetura: c.arquitetura.nome_regra().into(),
            vram_mb: c.vram_mb,
            capacidades: Capacidades {
                dlss: c.tem_tensor_cores,
                geracao_quadros: c.frame_gen,
                // Reflex vale para toda GeForce suportada pelo driver atual.
                reflex: c.arquitetura.nome_regra() != "desconhecida",
                rebar: c.rebar_suportado,
                refresh_mhz: c.refresh_mhz,
                vrr: nvt_ipc::Vrr {
                    possivel: c.vrr.possivel,
                    ativo: c.vrr.ativo,
                },
            },
        }),
        preferencias: preferencias_ipc(&est.preferencias),
        jogos: diff.jogos.len() as u32,
        ultima_aplicacao,
        alertas_deriva,
        avisos: Vec::new(),
        adiamento: None,
        atualizacao: Default::default(),
        autonomia_pausada: false,
    };

    // O modo local só lê o estado compartilhado do driver.
    let sugestoes_driver = diff
        .plano
        .experimentais
        .iter()
        .map(|s| &s.regra)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter_map(|id| {
            let regra = e.regras().regras.iter().find(|r| &r.id == id)?;
            Some(nvt_ipc::SugestaoExperimental {
                regra: id.clone(),
                descricao: regra.descricao.clone(),
                camada: nvt_ipc::Camada::Driver,
                impacto_visual: impacto_ipc(regra.impacto_visual),
                estado: nvt_ipc::EstadoExperimental::Aceitavel,
            })
        })
        .collect();

    Dados {
        estado: Some(estado),
        sistema: Default::default(),
        sugestoes_driver,
        grupos,
        respeitadas,
        historico,
        reversoes: reversoes(e, diff),
        ignorados: ignorados(diff),
        jogos: Vec::new(),
        saude: None,
    }
}

/// Settings que saíram do plano, como o serviço as mostraria (pendentes).
pub fn reversoes<S: DriverStore>(e: &Engine<S>, diff: &Diff) -> Vec<Reversao> {
    diff.reverter
        .iter()
        .map(|r| {
            let x = &r.esperado;
            let (acao, original) = match &r.acao {
                AcaoMotor::Gravar(v) => (
                    AcaoReversao::GravarOriginal,
                    valor(e, &x.setting, v.as_u32()),
                ),
                AcaoMotor::Remover { .. } => (AcaoReversao::Remover, None),
                AcaoMotor::SoDeixarDeVigiar => (AcaoReversao::SoDeixarDeVigiar, None),
            };
            let causa = match &x.origem {
                Origem::Regra { id } => textos::causa_regra(id),
                Origem::Preferencia { preferencia, regra } => {
                    textos::causa_preferencia(preferencia, regra.as_deref())
                }
            };
            Reversao {
                perfil: perfil_de_alvo(&x.alvo),
                nome_perfil: nome_do_alvo(&x.alvo),
                setting: x.setting.clone(),
                nome_legivel: nome_legivel(e, &x.setting),
                atual: valor(e, &x.setting, r.atual),
                original,
                acao,
                motivo: textos::motivo_reversao(&causa, acao),
                status: StatusMudanca::Pendente,
            }
        })
        .collect()
}

pub fn ignorados(diff: &Diff) -> Vec<JogoIgnorado> {
    diff.ignorados
        .iter()
        .map(|j| JogoIgnorado {
            nome: j.nome.clone(),
            exe: j.exe.clone(),
            motivo: j.motivo.clone(),
        })
        .collect()
}

/// Jogos do diff (fatos, perfil e explicação) no formato do protocolo, como
/// o serviço os envia (`nvt-service`, `nucleo::jogos_ipc`).
pub fn jogos_ipc<S: DriverStore>(e: &Engine<S>, diff: &Diff) -> Vec<Jogo> {
    use nvt_core::MotivoNaoAplicada as M;
    let regras = &e.regras().regras;
    let descricao = |id: &str| {
        regras
            .iter()
            .find(|r| r.id == id)
            .map_or_else(String::new, |r| r.descricao.clone())
    };
    let motivo = |m: &M| match m {
        M::JogoSemDlss => MotivoNaoAplicada::JogoSemDlss,
        M::ArquiteturaNaoPrecisa { arquitetura } => MotivoNaoAplicada::ArquiteturaNaoPrecisa {
            arquitetura: arquitetura.nome_regra().to_owned(),
        },
        M::SemTensorCores => MotivoNaoAplicada::SemTensorCores,
        M::Experimental => MotivoNaoAplicada::Experimental,
        M::BloqueadaPeloTetoDeQualidade { teto, .. } => {
            MotivoNaoAplicada::BloqueadaPeloTetoDeQualidade {
                teto: preferencias_ipc(&nvt_core::Preferences {
                    qualidade: *teto,
                    ..Default::default()
                })
                .qualidade,
            }
        }
        M::EscolhaDoUsuario { setting } => MotivoNaoAplicada::EscolhaDoUsuario {
            setting: setting.clone(),
        },
        M::ConflitoComRegraMaisEspecifica { regra } => {
            MotivoNaoAplicada::ConflitoComRegraMaisEspecifica {
                regra: regra.clone(),
            }
        }
        M::FatoDesconhecido { fato } => MotivoNaoAplicada::FatoDesconhecido { fato: fato.clone() },
        // Mesmos motivos específicos do serviço (`nvt-service::nucleo::jogos_ipc`).
        M::CondicaoNaoAtendida {
            condicao,
            esperado,
            encontrado,
        } if condicao == "vrr_ativo" && esperado == "true" && encontrado == "false" => {
            MotivoNaoAplicada::VrrInativo
        }
        M::CondicaoNaoAtendida {
            condicao,
            esperado,
            encontrado,
        } if condicao == "jogo_reflex" && esperado == "false" && encontrado == "true" => {
            MotivoNaoAplicada::JogoComReflex
        }
        M::CondicaoNaoAtendida {
            condicao,
            esperado,
            encontrado,
        } => MotivoNaoAplicada::CondicaoNaoAtendida {
            condicao: condicao.clone(),
            esperado: esperado.clone(),
            encontrado: encontrado.clone(),
        },
    };
    let base = e.base_jogos();
    diff.jogos
        .iter()
        .map(|j| {
            let r = diff
                .plano
                .resumos
                .get(&j.perfil)
                .cloned()
                .unwrap_or_default();
            let curado = nvt_engine::casar_na_base(base, &j.entrada);
            Jogo {
                nome: j.entrada.nome.clone(),
                perfil: j.perfil.clone(),
                origem_perfil: match j.origem {
                    OrigemPerfil::Driver => OrigemPerfilIpc::Driver,
                    OrigemPerfil::Usuario => OrigemPerfilIpc::Usuario,
                    OrigemPerfil::CriadoPeloApp => OrigemPerfilIpc::CriadoPeloApp,
                    OrigemPerfil::ACriar => OrigemPerfilIpc::ACriar,
                },
                launcher: j.entrada.launcher.clone(),
                api: j.entrada.api.nome_regra().to_owned(),
                dlss: j.entrada.dlss.map(|v| {
                    if v == nvt_core::Versao::default() {
                        String::new()
                    } else {
                        v.to_string()
                    }
                }),
                // Jogo da base curada: Reflex e anticheat vêm da base (como no serviço).
                reflex: curado.map_or(j.entrada.reflex, |c| Some(c.reflex)),
                anticheat_kernel: curado
                    .map_or(j.entrada.anticheat_kernel, |c| Some(c.anticheat_kernel)),
                competitivo: curado.map(|c| c.id.as_str().to_owned()),
                em_execucao: j.entrada.em_execucao,
                orientacoes: r
                    .orientacoes
                    .iter()
                    .map(|o| o.codigo().to_owned())
                    .collect(),
                config_jogo: None,
                resumo: ResumoJogo {
                    aplicadas: r
                        .aplicadas
                        .iter()
                        .map(|a| RegraAplicada {
                            regra: a.regra.clone(),
                            descricao: descricao(&a.regra),
                            settings: a.settings.clone(),
                        })
                        .collect(),
                    nao_aplicadas: r
                        .nao_aplicadas
                        .iter()
                        .map(|n| RegraNaoAplicada {
                            regra: n.regra.clone(),
                            descricao: descricao(&n.regra),
                            motivo: motivo(&n.motivo),
                        })
                        .collect(),
                    herda_do_global: r.herda_do_global,
                },
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Preferências da UI (spec interface-e-saude.md §7.5, R-UI-09)
// ---------------------------------------------------------------------------

/// Arquivo de preferências da UI, dentro da pasta local do usuário.
pub const ARQUIVO_UI: &str = "ui.json";
/// Chave da marca de boas-vindas vista.
const CHAVE_BOAS_VINDAS: &str = "boas_vindas_vista";

/// `%LOCALAPPDATA%\nv-tuner`, se a variável existir.
pub fn pasta_ui() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(std::path::PathBuf::from(base).join("nv-tuner"))
}

/// As boas-vindas já foram vistas? Qualquer falha de leitura conta como
/// "não vistas": no máximo a tela aparece de novo.
pub fn boas_vindas_vista_em(pasta: &Path) -> bool {
    std::fs::read_to_string(pasta.join(ARQUIVO_UI))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| {
            v.get(CHAVE_BOAS_VINDAS)
                .and_then(serde_json::Value::as_bool)
        })
        .unwrap_or(false)
}

/// Grava a marca de boas-vindas vista, preservando outras chaves. Grava num
/// arquivo temporário e renomeia, para nunca deixar um `ui.json` pela metade.
pub fn marcar_boas_vindas_em(pasta: &Path) -> Result<(), String> {
    std::fs::create_dir_all(pasta).map_err(|e| e.to_string())?;
    let caminho = pasta.join(ARQUIVO_UI);
    let mut valor = std::fs::read_to_string(&caminho)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    valor[CHAVE_BOAS_VINDAS] = serde_json::Value::Bool(true);
    let texto = serde_json::to_string_pretty(&valor).map_err(|e| e.to_string())?;
    let temporario = pasta.join(format!("{ARQUIVO_UI}.tmp"));
    std::fs::write(&temporario, texto).map_err(|e| e.to_string())?;
    std::fs::rename(&temporario, &caminho).map_err(|e| e.to_string())
}

/// [`boas_vindas_vista_em`] na pasta padrão; sem pasta, "não vistas".
pub fn boas_vindas_vista() -> bool {
    pasta_ui().is_some_and(|p| boas_vindas_vista_em(&p))
}

/// [`marcar_boas_vindas_em`] na pasta padrão.
pub fn marcar_boas_vindas() -> Result<(), String> {
    let pasta = pasta_ui().ok_or_else(|| "LOCALAPPDATA ausente".to_owned())?;
    marcar_boas_vindas_em(&pasta)
}

#[cfg(test)]
mod testes_preferencias_ui {
    use super::*;

    fn pasta_temporaria(nome: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "nvt-ui-teste-{nome}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn sem_arquivo_nao_vista_e_marca_persiste() {
        let p = pasta_temporaria("marca");
        assert!(!boas_vindas_vista_em(&p));
        marcar_boas_vindas_em(&p).unwrap();
        assert!(boas_vindas_vista_em(&p));
        let texto = std::fs::read_to_string(p.join(ARQUIVO_UI)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&texto).unwrap();
        assert_eq!(v, serde_json::json!({ "boas_vindas_vista": true }));
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn arquivo_corrompido_nao_bloqueia() {
        let p = pasta_temporaria("corrompido");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join(ARQUIVO_UI), "{ isto não é json").unwrap();
        assert!(!boas_vindas_vista_em(&p));
        marcar_boas_vindas_em(&p).unwrap();
        assert!(boas_vindas_vista_em(&p));
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn preserva_outras_chaves_e_tipo_errado_conta_como_nao_vista() {
        let p = pasta_temporaria("chaves");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(
            p.join(ARQUIVO_UI),
            r#"{ "outra": 1, "boas_vindas_vista": "sim" }"#,
        )
        .unwrap();
        assert!(!boas_vindas_vista_em(&p));
        marcar_boas_vindas_em(&p).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(p.join(ARQUIVO_UI)).unwrap()).unwrap();
        assert_eq!(v["outra"], 1);
        assert_eq!(v["boas_vindas_vista"], true);
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn falha_ao_gravar_devolve_erro_sem_panico() {
        let p = pasta_temporaria("arquivo-no-lugar");
        // Um arquivo onde deveria estar a pasta: a gravação falha.
        std::fs::write(&p, "x").unwrap();
        assert!(marcar_boas_vindas_em(&p).is_err());
        assert!(!boas_vindas_vista_em(&p));
        let _ = std::fs::remove_file(&p);
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use nvt_engine::{DriverFalso, JogoEntrada};

    #[test]
    fn somente_leitura_recusa_toda_escrita() {
        let falso = DriverFalso::turing();
        let mut s = SomenteLeitura(falso.clone());
        assert!(s.criar_perfil("x").is_err());
        assert!(s.criar_aplicacao("x", "x.exe").is_err());
        assert!(
            s.gravar_setting(&ProfileRef::Base, 1, &SettingValue::Dword(1))
                .is_err()
        );
        assert!(s.remover_setting(&ProfileRef::Base, 1, false).is_err());
        assert!(s.salvar().is_err());
        // Leitura continua funcionando.
        assert!(s.gpus().is_ok());
        assert!(s.perfil_existe("qualquer").is_ok());
        assert_eq!((falso.gravacoes(), falso.salvamentos()), (0, 0));
    }

    #[test]
    fn aplicar_pelo_motor_falha_em_somente_leitura() {
        let falso = DriverFalso::turing();
        let mut e = Engine::new(SomenteLeitura(falso.clone()), Armazem::em_memoria()).unwrap();
        let diff = e.planejar(&[]).unwrap();
        assert!(
            !diff.vazio(),
            "o perfil global de uma Turing tem o que mudar"
        );
        assert!(e.aplicar(&diff, true).is_err());
        assert_eq!((falso.gravacoes(), falso.salvamentos()), (0, 0));
    }

    fn estado_json() -> String {
        serde_json::to_string(&nvt_engine::Estado::default()).unwrap()
    }

    fn codigos(s: &Salvo) -> Vec<&str> {
        s.avisos.iter().map(|a| a.codigo.as_str()).collect()
    }

    #[test]
    fn estado_salvo_lido_sem_avisos() {
        let s = interpretar_salvo(Ok(Some(estado_json())), Ok(None), Ok(None), 1);
        assert!(s.estado.is_some());
        assert!(s.snapshot.is_none());
        assert!(s.avisos.is_empty());
        let s = interpretar_salvo(Ok(None), Ok(None), Ok(None), 1);
        assert!(s.estado.is_none() && s.avisos.is_empty());
    }

    #[test]
    fn estado_corrompido_usa_a_copia_de_seguranca() {
        let s = interpretar_salvo(
            Ok(Some("{ truncado".into())),
            Ok(Some(estado_json())),
            Ok(None),
            7,
        );
        assert!(s.estado.is_some(), "mostra a cópia de segurança");
        assert_eq!(
            codigos(&s),
            [
                codigo_aviso::ESTADO_CORROMPIDO,
                codigo_aviso::ESTADO_RECUPERADO
            ]
        );
        assert_eq!(s.avisos[0].nivel, NivelAviso::Atencao);
        assert!(s.avisos.iter().all(|a| a.quando_unix == 7));
        // Versão não suportada também é inválida; sem cópia, recomeça do zero.
        let v2 = estado_json().replace("\"versao\":1", "\"versao\":99");
        let s = interpretar_salvo(Ok(Some(v2)), Ok(Some("".into())), Ok(None), 7);
        assert!(s.estado.is_none());
        assert_eq!(
            codigos(&s),
            [
                codigo_aviso::ESTADO_CORROMPIDO,
                codigo_aviso::ESTADO_REINICIADO
            ]
        );
        assert!(
            s.avisos[0]
                .detalhe
                .as_deref()
                .unwrap()
                .contains("versão 99")
        );
    }

    /// Todos os códigos do motor. A `match` abaixo não compila se o motor
    /// ganhar um código novo: aí ele precisa entrar nesta lista.
    fn todos_os_codigos_do_motor() -> [CodigoAviso; 5] {
        fn _exaustivo(c: CodigoAviso) {
            match c {
                CodigoAviso::EstadoCorrompido
                | CodigoAviso::EstadoRecuperado
                | CodigoAviso::EstadoReiniciado
                | CodigoAviso::SnapshotIndisponivel
                | CodigoAviso::SnapshotOriginalPerdido => {}
            }
        }
        [
            CodigoAviso::EstadoCorrompido,
            CodigoAviso::EstadoRecuperado,
            CodigoAviso::EstadoReiniciado,
            CodigoAviso::SnapshotIndisponivel,
            CodigoAviso::SnapshotOriginalPerdido,
        ]
    }

    #[test]
    fn todo_codigo_do_motor_tem_constante_no_protocolo_e_texto_na_ui() {
        let constantes = [
            codigo_aviso::ESTADO_CORROMPIDO,
            codigo_aviso::ESTADO_RECUPERADO,
            codigo_aviso::ESTADO_REINICIADO,
            codigo_aviso::ESTADO_AVISO,
            codigo_aviso::SNAPSHOT_INDISPONIVEL,
            codigo_aviso::SNAPSHOT_ORIGINAL_PERDIDO,
            codigo_aviso::ERRO_PERFIL_APLICACAO,
            codigo_aviso::ERRO_PERFIL_DERIVA,
            codigo_aviso::JOGOS_IGNORADOS,
        ];
        for c in todos_os_codigos_do_motor() {
            assert_eq!(codigo_ipc(c), c.como_str(), "{c:?}");
            assert!(constantes.contains(&c.como_str()), "{c:?} sem constante");
            assert!(!textos::texto_aviso_motor(c).trim().is_empty(), "{c:?}");
        }
        // O texto próprio de `snapshot_original_perdido` substitui a mensagem
        // recebida e explica que “voltar ao original” volta à base.
        let a = Aviso::novo(
            NivelAviso::Atencao,
            codigo_aviso::SNAPSHOT_ORIGINAL_PERDIDO,
            "mensagem do serviço",
            None,
            1,
        );
        assert_eq!(
            textos::mensagem_aviso(&a),
            textos::SNAPSHOT_ORIGINAL_PERDIDO
        );
        assert!(textos::SNAPSHOT_ORIGINAL_PERDIDO.contains("base pós-quarentena"));
        let b = Aviso::snapshot_indisponivel("x", 1);
        assert_eq!(textos::mensagem_aviso(&b), b.mensagem);
    }

    #[test]
    fn estado_ausente_usa_a_copia_de_seguranca_como_o_motor() {
        let s = interpretar_salvo(Ok(None), Ok(Some(estado_json())), Ok(None), 4);
        assert!(s.estado.is_some(), "mostra a cópia de segurança");
        assert_eq!(codigos(&s), [codigo_aviso::ESTADO_RECUPERADO]);
        assert_eq!(s.avisos[0].nivel, NivelAviso::Atencao);
        // Cópia inválida ou ilegível: estado vazio, sem aviso (como o motor).
        for anterior in [Ok(Some("{".to_owned())), Err("acesso negado".to_owned())] {
            let s = interpretar_salvo(Ok(None), anterior, Ok(None), 4);
            assert!(s.estado.is_none() && s.avisos.is_empty());
        }
    }

    #[test]
    fn ler_salvo_nao_toca_nos_arquivos() {
        let dir = std::env::temp_dir().join(format!(
            "nvt-ui-ler-salvo-{}-{}",
            std::process::id(),
            agora_unix()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let anterior = estado_json();
        std::fs::write(dir.join("estado.anterior.json"), &anterior).unwrap();
        std::fs::write(
            dir.join("snapshot-perdido.json"),
            r#"{"detalhe":"quarentena em X"}"#,
        )
        .unwrap();
        let s = ler_salvo(&dir, 2);
        assert!(s.estado.is_some());
        assert_eq!(
            codigos(&s),
            [
                codigo_aviso::ESTADO_RECUPERADO,
                codigo_aviso::SNAPSHOT_ORIGINAL_PERDIDO
            ]
        );
        assert_eq!(
            s.avisos[1].detalhe.as_deref(),
            Some("snapshot-perdido.json: quarentena em X")
        );
        // Nada foi renomeado nem criado.
        let mut nomes: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        nomes.sort();
        assert_eq!(nomes, ["estado.anterior.json", "snapshot-perdido.json"]);
        assert_eq!(
            std::fs::read_to_string(dir.join("estado.anterior.json")).unwrap(),
            anterior
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn marcador_de_quarentena_vira_snapshot_original_perdido() {
        assert!(interpretar_marcador_quarentena(Ok(None), 1).is_none());
        let a = interpretar_marcador_quarentena(Ok(Some("texto solto\n".into())), 1).unwrap();
        assert_eq!(a.codigo, codigo_aviso::SNAPSHOT_ORIGINAL_PERDIDO);
        assert_eq!(a.nivel, NivelAviso::Atencao);
        assert_eq!(
            a.detalhe.as_deref(),
            Some("snapshot-perdido.json: texto solto")
        );
        let a = interpretar_marcador_quarentena(Err("acesso negado".into()), 1).unwrap();
        assert_eq!(a.codigo, codigo_aviso::ESTADO_AVISO);
    }

    #[test]
    fn snapshot_ilegivel_bloqueia_restauracao_no_modo_local() {
        let s = interpretar_salvo(Ok(None), Ok(None), Ok(Some(String::new())), 3);
        assert!(s.snapshot.is_none());
        assert_eq!(codigos(&s), [codigo_aviso::SNAPSHOT_INDISPONIVEL]);
        assert_eq!(s.avisos[0].nivel, NivelAviso::Erro);
        // Erro de leitura não é tratado como snapshot corrompido.
        let s = interpretar_salvo(Ok(None), Ok(None), Err("acesso negado".into()), 3);
        assert_eq!(codigos(&s), [codigo_aviso::ESTADO_AVISO]);
    }

    #[test]
    fn reversoes_e_ignorados_calculados_localmente() {
        // Aplica com um motor gravável (só no DriverFalso) e calcula a visão
        // local com o estado resultante, como o modo de visualização faz.
        let falso = DriverFalso::turing();
        let mut e = Engine::new(falso.clone(), Armazem::em_memoria()).unwrap();
        e.definir_preferencias(nvt_core::Preferences {
            limite_fps: Some(141),
            ..Default::default()
        })
        .unwrap();
        let diff = e.planejar(&[]).unwrap();
        e.aplicar(&diff, true).unwrap();
        let mut armazem = Armazem::em_memoria();
        armazem.estado = e.estado().clone();
        armazem.estado.preferencias.limite_fps = None;
        let gravacoes = falso.gravacoes();
        let local = Engine::new(SomenteLeitura(falso.clone()), armazem).unwrap();
        let ruim = JogoEntrada::new("Jogo Ruim", "ruim\u{0}.exe");
        let diff = local.planejar(&[ruim]).unwrap();
        let r = reversoes(&local, &diff);
        let lim = r
            .iter()
            .find(|r| r.setting == "limite_fps")
            .expect("limite de FPS sai do plano");
        assert_eq!(lim.status, StatusMudanca::Pendente);
        assert_eq!(lim.perfil, "global");
        assert_eq!(lim.atual.as_deref(), Some("141 FPS"));
        assert!(lim.motivo.starts_with("Saiu do plano porque a preferência"));
        let ig = ignorados(&diff);
        assert_eq!(ig.len(), 1);
        assert_eq!(ig[0].nome, "Jogo Ruim");
        assert_eq!(falso.gravacoes(), gravacoes, "nada gravado no modo local");
    }

    fn jogos_de_teste() -> Vec<JogoEntrada> {
        let mut w = JogoEntrada::new("WARDOGS", "WardogsClient-Win64-Shipping.exe");
        w.launcher = "Steam".into();
        w.api = nvt_core::Api::Dx12;
        w.dlss = Some(nvt_core::Versao::new(310, 6, 0, 0));
        let mut l = JogoEntrada::new("League of Legends", "League of Legends.exe");
        l.launcher = "Riot".into();
        l.api = nvt_core::Api::Dx11;
        vec![w, l]
    }

    fn resumo<'a>(jogos: &'a [Jogo], nome: &str) -> &'a ResumoJogo {
        &jogos.iter().find(|j| j.nome == nome).unwrap().resumo
    }

    /// Descrição da regra `energia-desempenho-maximo-por-jogo` (regras/base.toml).
    const ENERGIA_TXT: &str =
        "Gerenciamento de energia em desempenho máximo no perfil do jogo competitivo";

    #[test]
    fn jogos_locais_tem_o_mesmo_formato_do_servico() {
        let falso = DriverFalso::turing();
        let e = Engine::new(SomenteLeitura(falso.clone()), Armazem::em_memoria()).unwrap();
        let diff = e.planejar(&jogos_de_teste()).unwrap();
        let jogos = jogos_ipc(&e, &diff);
        assert_eq!(jogos.len(), 2);
        let w = jogos.iter().find(|j| j.nome == "WARDOGS").unwrap();
        assert_eq!((w.launcher.as_str(), w.api.as_str()), ("Steam", "dx12"));
        assert_eq!(w.origem_perfil, OrigemPerfilIpc::ACriar);
        // Spec competitivo F1-11 e §11 item 19: a energia por jogo só vale para
        // jogo competitivo. WARDOGS está fora da base: só o DLSS; LoL é
        // competitivo (base curada): recebe a energia.
        assert_eq!(textos::resumo_curto(&w.resumo), "Preset E do DLSS (Turing)");
        assert_eq!(
            textos::resumo_curto(resumo(&jogos, "League of Legends")),
            ENERGIA_TXT
        );
        assert!(!w.resumo.herda_do_global.is_empty());
        // Fase 1: jogo da base curada sai com o id, os fatos curados e as
        // orientações “faça no jogo”, todas com texto na UI.
        assert_eq!(w.competitivo, None);
        assert!(w.orientacoes.is_empty());
        let l = jogos
            .iter()
            .find(|j| j.nome == "League of Legends")
            .unwrap();
        assert_eq!(l.competitivo.as_deref(), Some("lol"));
        assert_eq!(l.reflex, Some(false), "fato da base curada");
        assert!(!l.em_execucao);
        assert!(!l.orientacoes.is_empty());
        assert!(
            l.orientacoes
                .iter()
                .all(|o| textos::orientacao(o).is_some())
        );
        assert_eq!((falso.gravacoes(), falso.salvamentos()), (0, 0));
    }

    #[test]
    fn avisos_do_plano_iguais_aos_do_servico() {
        let a = aviso_do_plano(nvt_core::AvisoPlano::GsyncSemVrr, 9);
        assert_eq!(a.codigo, codigo_aviso::GSYNC_SEM_VRR);
        assert_eq!(a.quando_unix, 9);
        let b = aviso_do_plano(nvt_core::AvisoPlano::VrrDesligadoNoPainel, 9);
        assert_eq!(b.codigo, codigo_aviso::VRR_DESLIGADO_NO_PAINEL);
        assert_eq!(textos::mensagem_aviso(&a), textos::GSYNC_SEM_VRR);
        assert_eq!(textos::mensagem_aviso(&b), textos::VRR_DESLIGADO_NO_PAINEL);
    }

    #[test]
    fn jogo_sem_dlss_fora_da_base_explica_que_nada_foi_aplicado() {
        // Spec competitivo §11 item 19: sem a energia por jogo, o jogo não
        // competitivo e sem DLSS volta a dizer "nenhum ajuste específico".
        let falso = DriverFalso::turing();
        let e = Engine::new(SomenteLeitura(falso), Armazem::em_memoria()).unwrap();
        let mut entrada = jogos_de_teste();
        entrada.push(JogoEntrada::new("Jogo Qualquer", "qualquer.exe"));
        let diff = e.planejar(&entrada).unwrap();
        let jogos = jogos_ipc(&e, &diff);
        assert_eq!(
            textos::resumo_curto(resumo(&jogos, "Jogo Qualquer")),
            "Nenhum ajuste específico — jogo sem DLSS"
        );
        assert_eq!(
            textos::resumo_curto(resumo(&jogos, "League of Legends")),
            ENERGIA_TXT
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn gpu_ada_simulada_explica_por_que_nada_foi_aplicado() {
        let ada = gpu_simulada_de("ada").unwrap();
        assert!(ada.name.contains("simulada"));
        assert!(gpu_simulada_de("rtx9090").is_none());
        // Mesma decisão do motor com a GPU simulada: a arquitetura vem dela.
        let falso = DriverFalso::turing();
        let e = Engine::new(SomenteLeitura(falso.clone()), Armazem::em_memoria()).unwrap();
        let mut diff = e.planejar(&jogos_de_teste()).unwrap();
        let caps = nvt_engine::capacidades(&GpuFixa(ada)).unwrap();
        assert_eq!(caps.arquitetura, nvt_core::Arquitetura::Ada);
        assert!(caps.tensor_fp8);
        let fatos: Vec<_> = diff.jogos.iter().map(|j| j.fatos.clone()).collect();
        diff.plano = nvt_core::avaliar(&caps, &fatos, &Default::default(), e.regras());
        let jogos = jogos_ipc(&e, &diff);
        // WARDOGS está fora da base competitiva: sem energia por jogo, o resumo
        // curto volta a explicar o motivo do DLSS (spec competitivo §11 item
        // 19). LoL é competitivo e recebe a energia.
        assert_eq!(
            textos::resumo_curto(resumo(&jogos, "WARDOGS")),
            "Nenhum ajuste específico — GPU Ada roda o DLSS padrão sem custo extra"
        );
        assert_eq!(
            textos::resumo_curto(resumo(&jogos, "League of Legends")),
            ENERGIA_TXT
        );
    }

    /// Driver mínimo que só informa uma GPU (para `nvt_engine::capacidades`).
    struct GpuFixa(GpuInfo);

    impl DriverStore for GpuFixa {
        fn versao_driver(&self) -> ResultadoDrs<DriverVersion> {
            DriverFalso::turing().versao_driver()
        }
        fn gpus(&self) -> ResultadoDrs<Vec<GpuInfo>> {
            Ok(vec![self.0.clone()])
        }
        fn perfil_do_exe(&self, _: &str) -> ResultadoDrs<Option<PerfilEncontrado>> {
            Ok(None)
        }
        fn perfil_existe(&self, _: &str) -> ResultadoDrs<bool> {
            Ok(false)
        }
        fn criar_perfil(&mut self, _: &str) -> ResultadoDrs<()> {
            Err(recusa())
        }
        fn criar_aplicacao(&mut self, _: &str, _: &str) -> ResultadoDrs<()> {
            Err(recusa())
        }
        fn ler_setting(&self, _: &ProfileRef, _: u32) -> ResultadoDrs<Option<Setting>> {
            Ok(None)
        }
        fn gravar_setting(&mut self, _: &ProfileRef, _: u32, _: &SettingValue) -> ResultadoDrs<()> {
            Err(recusa())
        }
        fn remover_setting(&mut self, _: &ProfileRef, _: u32, _: bool) -> ResultadoDrs<()> {
            Err(recusa())
        }
        fn settings_do_perfil(&self, _: &ProfileRef) -> ResultadoDrs<Vec<Setting>> {
            Ok(Vec::new())
        }
        fn capturar(&self, exes: &[&str]) -> ResultadoDrs<Snapshot> {
            DriverFalso::turing().capturar(exes)
        }
        fn salvar(&mut self) -> ResultadoDrs<()> {
            Err(recusa())
        }
    }

    #[test]
    fn mapeamentos() {
        assert_eq!(impacto_ipc(ImpactoRegra::Leve), ImpactoVisual::Baixo);
        assert_eq!(impacto_ipc(ImpactoRegra::Forte), ImpactoVisual::Alto);
        let p = nvt_core::Preferences {
            qualidade: nvt_core::Qualidade::Fidelidade,
            limite_fps: Some(141),
            ..Default::default()
        };
        assert_eq!(
            preferencias_ipc(&p),
            Preferencias {
                sincronismo: Sincronismo::Desligado,
                qualidade: Qualidade::Qualidade,
                limite_fps: Some(141),
            }
        );
        assert_eq!(
            acao_de_origem(&OrigemRegistro::Restauracao),
            AcaoHistorico::Restauracao
        );
        assert_eq!(
            acao_de_origem(&OrigemRegistro::Usuario),
            AcaoHistorico::DerivaDetectada
        );
    }
}
