//! Portão duplo antes de cada escrita de arquivo de jogo (D8, F3-03).
//!
//! 1. Portão de processos da Fase 2 (S1): base curada, conjunto vigiado do serviço e
//!    anticheats; lista ilegível bloqueia.
//! 2. Janela fechada do destino: executáveis e `processos_protegidos` do jogo,
//!    `processos_fechados` do arquivo e, para arquivos da Steam, `steam.exe`.
//!
//! Só lê a lista de nomes (`TH32CS_SNAPPROCESS`, no `nvt-detect`); nunca fecha processo.

use std::collections::BTreeSet;

use nvt_core::config_jogo::{DefArquivoJogo, RaizJogo};
use nvt_core::jogos::BaseJogos;
use nvt_engine::Portao;
use nvt_ipc::{MotivoNaoAplicada, PROCESSOS_MAX};

/// Processo da Steam exigido fechado para arquivos da Steam.
pub(crate) const STEAM: &str = "steam.exe";

/// Escrita adiada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Bloqueio {
    /// Portão S1 (outro jogo, anticheat, lista ilegível ou sem lista do serviço).
    Portao,
    /// Processos do destino abertos (`JanelaFechadaExigida`).
    Janela(Vec<String>),
}

impl Bloqueio {
    pub(crate) fn motivo(&self) -> Option<MotivoNaoAplicada> {
        match self {
            Self::Portao => None,
            Self::Janela(processos) => Some(MotivoNaoAplicada::JanelaFechadaExigida {
                processos: processos.clone(),
            }),
        }
    }
}

/// Processos que precisam estar fechados para escrever em `def`.
pub(crate) fn processos(base: &BaseJogos, def: &DefArquivoJogo) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    let mut somar = |n: &str| {
        if !n.is_empty() && !v.iter().any(|x| x.eq_ignore_ascii_case(n)) {
            v.push(n.to_owned());
        }
    };
    if let Some(j) = base.jogo(def.jogo.as_str()) {
        j.executaveis.iter().for_each(|p| somar(p));
        j.processos_protegidos.iter().for_each(|p| somar(p));
    }
    def.processos_fechados.iter().for_each(|p| somar(p));
    if def.exige_steam_fechada || def.raiz == RaizJogo::SteamUserdata {
        somar(STEAM);
    }
    v
}

fn abertos(rodando: &[String], janela: &[String]) -> Vec<String> {
    let mut v: Vec<String> = janela
        .iter()
        .filter(|j| rodando.iter().any(|r| r.eq_ignore_ascii_case(j)))
        .filter(|j| !j.contains(['\\', '/', ':']) && j.len() <= 128)
        .cloned()
        .collect();
    v.truncate(PROCESSOS_MAX);
    v
}

/// Portão duplo. `vigiados = None`: lista do serviço indisponível (bloqueia).
pub(crate) fn verificar(
    portao: &Portao,
    base: &BaseJogos,
    vigiados: Option<&BTreeSet<String>>,
    janela: &[String],
) -> Result<(), Bloqueio> {
    let vigiados = vigiados.ok_or(Bloqueio::Portao)?;
    let extras: Vec<&str> = vigiados.iter().map(String::as_str).collect();
    let classificar = |a: nvt_engine::Adiamento| {
        if a.lista_indisponivel {
            return Bloqueio::Portao;
        }
        let v = abertos(&a.processos, janela);
        if v.is_empty() {
            Bloqueio::Portao
        } else {
            Bloqueio::Janela(v)
        }
    };
    portao.verificar(base, &extras).map_err(classificar)?;
    fechada(portao, base, janela)
}

/// Só a janela fechada (deriva): `Ok` se nenhum processo do destino está aberto e a
/// lista foi lida.
pub(crate) fn fechada(
    portao: &Portao,
    base: &BaseJogos,
    janela: &[String],
) -> Result<(), Bloqueio> {
    let extras: Vec<&str> = janela.iter().map(String::as_str).collect();
    match portao.verificar(base, &extras) {
        Ok(_) => Ok(()),
        Err(a) if a.lista_indisponivel => Err(Bloqueio::Portao),
        Err(a) => {
            let v = abertos(&a.processos, janela);
            if v.is_empty() {
                // Outro jogo aberto: a janela deste destino está fechada.
                Ok(())
            } else {
                Err(Bloqueio::Janela(v))
            }
        }
    }
}
