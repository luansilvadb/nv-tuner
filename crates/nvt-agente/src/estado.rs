use std::{collections::BTreeMap, io};

use nvt_ipc::{ReferenciaTarefa, ResultadoUsuario, SettingUsuario};
use serde::{Deserialize, Serialize};

use crate::ValorLido;

/// A implementação deve manter a trava exclusiva desde a carga até o fim do ciclo.
/// Cada gravação substitui atomicamente o estado anterior.
pub trait Persistencia {
    fn carregar(&self) -> io::Result<Estado>;
    fn salvar(&self, estado: &Estado) -> io::Result<()>;
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Estado {
    pub(crate) geracao_vista: u64,
    pub(crate) sessoes: BTreeMap<u32, Sessao>,
    pub(crate) destinos: Vec<Destino>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) backups_msi: Vec<Backup>,
    /// Arquivos de jogo geridos (Fase 3); estado v6 carrega sem eles.
    #[serde(default, skip_serializing_if = "crate::config_jogo::EstadoJogo::vazio")]
    pub(crate) jogo: crate::config_jogo::EstadoJogo,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Backup {
    pub destino: SettingUsuario,
    pub sessao: Option<u32>,
    pub valor: ValorLido,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct Sessao {
    pub geracao: u64,
    pub revisao: u64,
    pub trabalho: Option<Trabalho>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Trabalho {
    pub tarefa: ReferenciaTarefa,
    pub resultados: Vec<ResultadoUsuario>,
    pub pronto: bool,
    pub confirmado: bool,
    #[serde(default)]
    pub geracao_concluida: Option<u64>,
    /// Resultados das ações de arquivo de jogo (v7).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resultados_jogo: Vec<nvt_ipc::ResultadoJogo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Destino {
    pub destino: SettingUsuario,
    pub sessao: Option<u32>,
    pub snapshot: ValorLido,
    pub geracao_snapshot: u64,
    pub antes_gestao: Option<ValorLido>,
    pub esperado: Option<ValorLido>,
    pub travado: bool,
    #[serde(default)]
    pub bloquear_snapshot_ate: Option<u64>,
    pub intencao: Option<Intencao>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Operacao {
    Aplicar,
    Retirar,
    Reverter,
    Snapshot,
    SnapshotManual,
}

impl Operacao {
    pub fn snapshot(self) -> bool {
        matches!(self, Self::Snapshot | Self::SnapshotManual)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Intencao {
    pub anterior: ValorLido,
    pub pedido: ValorLido,
    pub operacao: Operacao,
    pub geracao: u64,
    pub rollback: Option<ValorLido>,
    pub sessao: u32,
    pub tarefa: Option<ReferenciaTarefa>,
}

impl Estado {
    pub(crate) fn validar(&self) -> io::Result<()> {
        use crate::valores::valido;
        let mut chaves = std::collections::BTreeSet::new();
        for entrada in &self.destinos {
            let destino = &entrada.destino;
            let tela = matches!(destino, SettingUsuario::Refresh { .. });
            let intent = entrada.intencao.iter().flat_map(|i| {
                [&i.anterior, &i.pedido]
                    .into_iter()
                    .chain(i.rollback.iter())
            });
            if tela != entrada.sessao.is_some()
                || !chaves.insert((destino, entrada.sessao))
                || std::iter::once(&entrada.snapshot)
                    .chain(entrada.antes_gestao.iter())
                    .chain(entrada.esperado.iter())
                    .chain(intent)
                    .any(|valor| !valido(destino, valor))
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "estado dos destinos inválido",
                ));
            }
        }
        Ok(())
    }
}
