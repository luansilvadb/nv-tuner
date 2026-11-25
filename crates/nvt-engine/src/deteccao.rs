//! Conversão da detecção (`nvt-drs` + `nvt-detect`) para o domínio do
//! `nvt-core`: [`Capabilities`] da GPU e [`JogoEntrada`] de cada jogo.

use nvt_core::{Api, Arquitetura, Capabilities, Versao};
use nvt_detect::{ApiGrafica, Confianca, DetectedGame, GameTraits, ServicosSistema};

use crate::erro::{ErroEngine, Resultado};
use crate::store::DriverStore;

/// Jogo detectado, pronto para ser cruzado com o DRS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JogoEntrada {
    /// Nome no launcher (vira nome do perfil se o app precisar criá-lo).
    pub nome: String,
    /// Nome do arquivo do exe principal (ex.: `League of Legends.exe`).
    pub exe: String,
    pub api: Api,
    /// Versão do `nvngx_dlss.dll`; `None` = sem DLSS.
    pub dlss: Option<Versao>,
    pub reflex: Option<bool>,
    pub anticheat_kernel: Option<bool>,
}

impl JogoEntrada {
    /// Jogo mínimo (testes): API desconhecida, sem DLSS, resto desconhecido.
    pub fn new(nome: impl Into<String>, exe: impl Into<String>) -> Self {
        Self {
            nome: nome.into(),
            exe: exe.into(),
            api: Api::Desconhecida,
            dlss: None,
            reflex: None,
            anticheat_kernel: None,
        }
    }
}

/// Capacidades da GPU principal (a primeira da enumeração da NVAPI).
///
/// Arquitetura e recursos vêm de `nvt_detect::arch_facts`; VRAM e driver da
/// NVAPI. `rebar_ativo` e `refresh_hz` ficam desconhecidos (ainda não lidos).
pub fn capacidades<S: DriverStore>(store: &S) -> Resultado<Capabilities> {
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
        refresh_hz: None,
    })
}

/// Converte o resultado do `nvt-detect`. `None` se não houver exe principal.
///
/// - DLSS presente sem versão legível vira `Some(0.0)` (tem DLSS, versão
///   desconhecida).
/// - Reflex: `Some` só com confiança média ou alta (ausência de rastro em disco
///   não prova ausência de Reflex via NVAPI).
/// - Anticheat de kernel: `Some(true)` se detectado; senão desconhecido.
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
    Some(JogoEntrada {
        nome: j.nome.clone(),
        exe: exe.into_owned(),
        api,
        dlss,
        reflex,
        anticheat_kernel,
    })
}

/// Varre as bibliotecas desta máquina (somente leitura). Devolve os jogos com
/// exe principal e avisos (scanners que falharam, jogos sem exe).
pub fn jogos_da_maquina() -> (Vec<JogoEntrada>, Vec<String>) {
    let rel = nvt_detect::varrer_bibliotecas();
    let mut avisos: Vec<String> = rel
        .falhas
        .iter()
        .map(|f| format!("scanner {} falhou: {}", f.launcher, f.erro))
        .collect();
    let servicos = ServicosSistema::ler();
    let mut jogos = Vec::new();
    for j in &rel.jogos {
        let t = nvt_detect::analisar_jogo(j, &servicos);
        match jogo_de_traits(j, &t) {
            Some(e) => jogos.push(e),
            None => avisos.push(format!("{}: exe principal não encontrado", j.nome)),
        }
    }
    (jogos, avisos)
}
