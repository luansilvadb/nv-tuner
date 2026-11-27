//! Configuração de arquivo de jogo no agente (Fase 3, `docs/spec/fase3-plano.md`
//! L2-b e L2-c; `docs/spec/competitivo.md` §9.2 e §3).
//!
//! O agente resolve os caminhos do catálogo (`[[arquivo]]` de `regras/jogos.toml`)
//! com os direitos do próprio usuário, lê os valores, faz o backup imutável antes da
//! primeira gravação, grava pelo `nvt-jogocfg` (só os bytes do valor, `ReplaceFileW`)
//! e restaura por hash. Antes de **cada** escrita passa pelo portão duplo
//! ([`janela`]): o portão de processos da Fase 2 e a janela fechada do destino (jogo,
//! processos protegidos e, para arquivos da Steam, `steam.exe`). O agente nunca fecha,
//! abre nem inspeciona processo: só lê a lista de nomes.
//!
//! Ao serviço só vão rótulos anonimizados (`%USERPROFILE%\…`, `%STEAM%\…`) e
//! instâncias opacas (hash da conta), nunca caminhos completos nem identificadores.

use nvt_core::config_jogo::{SettingJogo, ValorJogo};
use nvt_ipc::{DestinoJogoIpc, ReferenciaTarefa, ResultadoJogo};
use serde::{Deserialize, Serialize};

#[cfg(windows)]
mod armazem;
#[cfg(windows)]
mod execucao;
#[cfg(windows)]
mod janela;

#[cfg(windows)]
pub use armazem::{AmbienteJogo, GravadorJogo, GravadorReal};

#[cfg(all(test, windows))]
#[path = "config_jogo_testes.rs"]
mod testes;

/// Estado persistido da família jogo (dentro de `estado.json` do agente).
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct EstadoJogo {
    #[serde(default)]
    pub arquivos: Vec<ArquivoGerido>,
    /// Segredo do perfil (64 hex) das instâncias de conta. Fica só aqui, no estado do
    /// agente em `%LOCALAPPDATA%\nv-tuner\usuario` (aberto sem seguir reparse points),
    /// junto da gestão que o usa: apagar o estado apaga os dois, então nenhuma gestão
    /// fica com instância órfã; um segredo novo só muda as instâncias relatadas daí em
    /// diante. Gestão gravada antes do segredo (instância sem chave) é migrada na carga.
    /// Nunca vai ao IPC: o serviço só recebe as instâncias derivadas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segredo_conta: Option<String>,
}

impl EstadoJogo {
    pub(crate) fn vazio(&self) -> bool {
        self.arquivos.is_empty() && self.segredo_conta.is_none()
    }

    /// Segredo decodificado; `None` se ausente ou malformado.
    pub(crate) fn segredo(&self) -> Option<[u8; 32]> {
        let t = self.segredo_conta.as_deref()?;
        if t.len() != 64 {
            return None;
        }
        let mut s = [0u8; 32];
        for (i, b) in s.iter_mut().enumerate() {
            *b = u8::from_str_radix(t.get(2 * i..2 * i + 2)?, 16).ok()?;
        }
        Some(s)
    }

    /// Há algo que uma restauração precisa devolver.
    pub(crate) fn gerido(&self) -> bool {
        self.arquivos.iter().any(ArquivoGerido::gerido)
    }
}

impl ArquivoGerido {
    /// Intenção pendente ou chave alterada pelo app.
    pub(crate) fn gerido(&self) -> bool {
        self.intencao.is_some() || self.chaves.iter().any(ChaveGerida::alterada)
    }
}

/// Uma instância (arquivo lógico + conta) já tocada pelo app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ArquivoGerido {
    /// Id lógico do `[[arquivo]]`.
    pub arquivo: String,
    /// Instância opaca (hash da conta ou `local`).
    pub instancia: String,
    /// Nome do backup em `backups\<jogo>\` (sem caminho).
    pub backup: String,
    /// SHA-256 do original (backup).
    pub hash_original: String,
    /// SHA-256 depois da última gravação do app.
    pub hash_depois: Option<String>,
    /// Geração de restauração em que o backup vale como snapshot.
    pub geracao: u64,
    /// O backup é o arquivo de antes da gestão atual: restauração inteira permitida.
    pub restaurar_inteiro: bool,
    pub chaves: Vec<ChaveGerida>,
    pub intencao: Option<IntencaoJogo>,
}

/// Chave gerida pelo app dentro do arquivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChaveGerida {
    pub setting: SettingJogo,
    pub antes: ValorJogo,
    pub esperado: ValorJogo,
    /// Texto bruto do valor antes da gestão.
    pub texto_antes: String,
    /// Texto bruto gravado pelo app.
    pub texto_gravado: String,
    /// Deriva, reversão do usuário ou mudança do jogo: o app não regrava.
    pub travado: bool,
}

impl ChaveGerida {
    /// O app deixou no arquivo um valor diferente do original.
    pub(crate) fn alterada(&self) -> bool {
        self.texto_gravado != self.texto_antes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OperacaoJogo {
    Aplicar,
    Restaurar,
    Reverter,
    Snapshot,
    SnapshotManual,
}

/// Intenção persistida antes da gravação; resolvida pelo hash na recuperação.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct IntencaoJogo {
    pub operacao: OperacaoJogo,
    pub hash_anterior: String,
    pub hash_novo: String,
    /// Gestão resultante se a gravação se confirmar.
    pub chaves_novas: Vec<ChaveGerida>,
    pub restaurar_inteiro: bool,
    /// Resultados a registrar (um por destino afetado), sem o status final.
    pub resultados: Vec<ResultadoJogo>,
    pub sessao: u32,
    pub tarefa: Option<ReferenciaTarefa>,
    pub geracao: u64,
    /// Caminho local (só no estado do perfil; nunca vai ao IPC).
    pub raiz: std::path::PathBuf,
    pub caminho: std::path::PathBuf,
}

/// Resultado que encerra a ação (não precisa de nova tentativa).
pub(crate) fn final_jogo(r: &ResultadoJogo) -> bool {
    use nvt_ipc::StatusMudanca::*;
    !matches!(r.status, Pendente | Falhou)
}

pub(crate) fn destino_ipc(
    arquivo: &str,
    jogo: &str,
    setting: SettingJogo,
    instancia: &str,
) -> DestinoJogoIpc {
    DestinoJogoIpc {
        alvo: nvt_ipc::AlvoIpc::ConfigJogo {
            jogo: jogo.to_owned(),
            arquivo: arquivo.to_owned(),
        },
        setting: setting.codigo().to_owned(),
        instancia: instancia.to_owned(),
    }
}

/// Fora do Windows a família jogo fica inerte (o `nvt-jogocfg` é só Windows).
#[cfg(not(windows))]
mod inerte {
    use std::io;

    use nvt_ipc::{AcaoJogo, FatoConfigJogo, ReferenciaTarefa, ResultadoJogo};

    use crate::ciclo::{Execucao, Transporte};
    use crate::{EscritaUsuario, LeituraUsuario, Persistencia};

    impl<A: LeituraUsuario + EscritaUsuario, T: Transporte, P: Persistencia> Execucao<'_, A, T, P> {
        pub(crate) fn fatos_jogo(&self) -> Vec<FatoConfigJogo> {
            Vec::new()
        }
        pub(crate) fn recuperar_jogo(&mut self) -> io::Result<()> {
            Ok(())
        }
        pub(crate) fn deriva_jogo(&mut self) -> io::Result<()> {
            Ok(())
        }
        pub(crate) fn geracao_jogo_pendente(&self, _: u64) -> bool {
            false
        }
        pub(crate) fn tem_intencao_jogo(&self, _: &ReferenciaTarefa) -> bool {
            false
        }
        pub(crate) fn executar_jogo(
            &mut self,
            _: &AcaoJogo,
            _: &ReferenciaTarefa,
            _: u64,
        ) -> io::Result<()> {
            Ok(())
        }
        pub(crate) fn restaurar_jogo(
            &mut self,
            _: Option<u64>,
            _: Option<&ReferenciaTarefa>,
        ) -> io::Result<Vec<ResultadoJogo>> {
            Ok(Vec::new())
        }
    }
}
