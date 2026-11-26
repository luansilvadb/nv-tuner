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
    /// Linha de base da inicialização e política da limpeza (L3); estado v7 carrega sem eles.
    #[serde(default, skip_serializing_if = "EstadoSaude::vazio")]
    pub(crate) saude: EstadoSaude,
}

/// Saúde persistida no perfil local. Só nomes e hashes: nenhum comando ou caminho.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EstadoSaude {
    /// `None` = primeira leitura ainda não feita.
    #[serde(default)]
    pub base: Option<LinhaBase>,
    /// Itens fora da base, com a data da primeira detecção (mantém a data estável).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fora_da_base: Vec<ItemBase>,
    /// Início (Unix) da última passada de limpeza por alvo (política diária, R-L-05).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ultimas_limpezas: BTreeMap<nvt_ipc::AlvoLimpeza, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limpeza: Option<nvt_ipc::ResultadoLimpeza>,
}

impl EstadoSaude {
    pub fn vazio(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LinhaBase {
    pub criada_unix: u64,
    pub itens: Vec<ItemBase>,
}

/// `hash` = sha256 hexadecimal do comando completo, que nunca é gravado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ItemBase {
    pub fonte: nvt_ipc::FonteInicio,
    pub nome: String,
    pub hash: String,
    pub detectado_unix: u64,
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
