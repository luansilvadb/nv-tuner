//! Modo de visualização: pipeline local **somente leitura**.
//!
//! Usado quando a interface não consegue falar com o serviço. Três travas
//! garantem que nada seja gravado:
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

mod conversao;
mod montagem;
mod preferencias_ui;
mod salvo;
mod somente_leitura;
pub use conversao::*;
pub use montagem::*;
pub use preferencias_ui::*;
pub use salvo::*;
pub use somente_leitura::*;

/// Limite de entradas do histórico exibidas.
pub const HISTORICO_EXIBIDO: usize = 200;

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
    let agora = crate::fuso::agora_unix();
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
    let entradas = nvt_engine::detectar_jogos_da_maquina_local().jogos;
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

#[cfg(test)]
mod testes;
