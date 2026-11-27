//! Coordenação da camada interativa. A identidade vem exclusivamente do pipe.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nvt_core::config_jogo::{
    CatalogoConfigJogo, DestinoJogo, EstadoDestinoJogo, FatosConfigJogo, GestaoJogo, PlanoJogo,
    SettingJogo, ValorJogo, avaliar_config_jogo,
};
use nvt_core::usuario::{
    self, ConjuntoRegrasUsuario, FatosUsuario, GestaoUsuario, PlanoUsuario,
    SettingUsuario as Setting, ValorUsuario,
};
use nvt_core::{BaseJogos, ConjuntoRegras, IdJogo, Preferences, Qualidade, Status};
use nvt_ipc::pipe::IdentidadeCliente;
use nvt_ipc::{
    AcaoHistorico, AcaoUsuario, AjusteUsuario, Camada, CodigoErro, EntradaHistorico, EstadoAgente,
    EstadoExperimental, EstadoTarefa, Fato, ImpactoVisual, Mudanca, OperacaoUsuario, PlanoAgente,
    ProgressoTarefa, ReferenciaTarefa, Requisicao, Resposta, ResultadoUsuario, ResumoSistema,
    SettingUsuario, StatusMudanca, SugestaoExperimental, TarefaUsuario,
};
use nvt_ipc::{
    AcaoJogo, AlvoIpc, ArquivoJogoUi, ConfigJogoUi, DestinoJogoIpc, FatoConfigJogo,
    MotivoNaoAplicada, MudancaArquivoJogo, OrientacaoConfigJogo, ResultadoJogo, TarefaJogo,
    ValorJogoIpc,
};
use nvt_windows::conversao::{spi_core, spi_ipc, taxa_core, taxa_ipc};
use serde::{Deserialize, Serialize};

mod aceites;
mod ajustes;
mod consulta;
mod estado;
mod jogo;
mod relato;

use ajustes::*;
use jogo::*;

const PRESENCA: Duration = Duration::from_secs(180);
const SETTINGS: [Setting; 4] = [
    Setting::TelaRefresh,
    Setting::MouseAceleracao,
    Setting::DxSwapEffectUpgrade,
    Setting::GamedvrCapturaHistorica,
];
const RECIBOS_POR_SESSAO: usize = 32;
const HISTORICO_POR_SID: usize = nvt_ipc::HISTORICO_MAX as usize;
type Erro = (CodigoErro, String);

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct DadosUsuarios {
    por_sid: BTreeMap<String, DadosSid>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct DadosSid {
    aceites: BTreeSet<String>,
    historico: VecDeque<EntradaHistorico>,
    recibos: BTreeMap<u32, VecDeque<Recibo>>,
    /// Última atualização do nv-tuner já vista por este SID (U-47, U-52).
    atualizacao_vista: Option<String>,
    /// Programas de inicialização marcados como vistos por este SID (v8).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    inicio_vistos: Vec<MarcaInicio>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MarcaInicio {
    fonte: nvt_ipc::FonteInicio,
    nome: String,
    marcado_unix: u64,
}

#[derive(Clone, Deserialize, Serialize)]
struct Recibo {
    tarefa: ReferenciaTarefa,
    resultados: Vec<ResultadoUsuario>,
    /// Resultados de arquivo de jogo (v7); recibos antigos não têm o campo.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    resultados_jogo: Vec<ResultadoJogo>,
}

pub(crate) trait Armazenamento {
    fn carregar(&self) -> io::Result<DadosUsuarios>;
    fn salvar(&self, dados: &DadosUsuarios) -> io::Result<()>;
}

/// Deve ser construído sobre o diretório de estado privilegiado já verificado.
pub(crate) struct ArquivoUsuarios(pub PathBuf);

impl Armazenamento for ArquivoUsuarios {
    fn carregar(&self) -> io::Result<DadosUsuarios> {
        crate::diretorio::ler_json_privado(&self.0.join("usuarios.json"))
    }

    fn salvar(&self, dados: &DadosUsuarios) -> io::Result<()> {
        let bytes = serde_json::to_vec(dados)?;
        crate::diretorio::gravar_atomico_privado(&self.0, "usuarios.json", &bytes)
    }
}

struct MetaRegra {
    id: String,
    descricao: String,
    status: Status,
    impacto_visual: nvt_core::ImpactoVisual,
}

#[derive(Default)]
struct Sessao {
    contato: Option<Instant>,
    restaurada: u64,
    snapshot_ofertado: Option<u64>,
    snapshot_confirmado: Option<u64>,
    revisao: u64,
    fatos: Option<FatosUsuario>,
    diagnosticos: Vec<nvt_ipc::Diagnostico>,
    em_voo: Option<TarefaUsuario>,
    gestao: BTreeMap<SettingUsuario, GestaoUsuario>,
    travas: BTreeSet<SettingUsuario>,
    reversoes: BTreeSet<SettingUsuario>,
    /// Família jogo (Fase 3): fatos brutos validados, destinos convertidos,
    /// gestão reidratada do agente, travas, reversões pedidas e tarefa.
    fatos_jogo: Vec<FatoConfigJogo>,
    destinos_jogo: BTreeMap<DestinoJogo, EstadoDestinoJogo>,
    refresh_hz: Option<u32>,
    gestao_jogo: BTreeMap<DestinoJogo, GestaoJogo>,
    travas_jogo: BTreeSet<DestinoJogo>,
    reversoes_jogo: BTreeSet<DestinoJogo>,
    /// Último relato sem sucesso por destino (falha ou adiamento).
    falhas_jogo: BTreeMap<DestinoJogo, FalhaJogo>,
    em_voo_jogo: Option<TarefaJogo>,
    /// Último relato de saúde do agente desta sessão (só memória).
    saude: Option<nvt_ipc::SaudeSessao>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FalhaJogo {
    status: StatusMudanca,
    motivo: Option<MotivoNaoAplicada>,
    /// Hash do arquivo quando a ação foi planejada: com o mesmo arquivo, a
    /// falha não se repete (nunca regrava em loop).
    hash: String,
}

impl FalhaJogo {
    fn adiamento(&self) -> bool {
        self.status == StatusMudanca::Pendente
            || matches!(
                self.motivo,
                Some(MotivoNaoAplicada::JanelaFechadaExigida { .. })
            )
    }
}

impl Sessao {
    fn invalidar(&mut self) {
        self.revisao += 1;
        self.em_voo = None;
        self.em_voo_jogo = None;
    }

    fn fatos_jogo_core(&self) -> FatosConfigJogo {
        FatosConfigJogo {
            destinos: self.destinos_jogo.clone(),
            gestao: self.gestao_jogo.clone(),
            travadas: self.travas_jogo.clone(),
        }
    }

    fn arquivo_jogo(&self, destino: &DestinoJogo) -> Option<&FatoConfigJogo> {
        self.fatos_jogo.iter().find(|f| {
            let AlvoIpc::ConfigJogo { jogo, arquivo } = &f.alvo;
            jogo == destino.instancia.jogo.as_str()
                && *arquivo == destino.instancia.arquivo
                && destino.instancia.conta.as_deref() == Some(f.instancia.as_str())
        })
    }

    fn presente(&self, agora: Instant) -> bool {
        self.contato
            .is_some_and(|contato| agora.saturating_duration_since(contato) < PRESENCA)
    }
}

pub(crate) struct Coordenador {
    armazenamento: Box<dyn Armazenamento>,
    dados: DadosUsuarios,
    sessoes: HashMap<IdentidadeCliente, Sessao>,
    regras: ConjuntoRegrasUsuario,
    metadados: BTreeMap<String, MetaRegra>,
    drs: BTreeSet<String>,
    /// Catálogo `[[arquivo]]`/`[[regra]]` de config de jogo (Fase 3).
    catalogo_jogo: CatalogoConfigJogo,
    /// Preferências do usuário (só `limite_fps` interessa à família jogo).
    prefs: Preferences,
    geracao: u64,
    pausado: bool,
    serial: u64,
    windows_build: Option<u32>,
    processos: BTreeSet<String>,
    /// Projeção atual da atualização automática (vem da thread do atualizador).
    atualizacao: nvt_ipc::EstadoAtualizacao,
}

fn invalida(mensagem: impl Into<String>) -> Erro {
    (CodigoErro::RequisicaoInvalida, mensagem.into())
}

/// Aplica `f` a uma cópia de `dados`, grava a cópia e só então a efetiva: se a
/// gravação falhar, nada muda em memória.
fn transacao<R>(
    dados: &mut DadosUsuarios,
    armazenamento: &dyn Armazenamento,
    f: impl FnOnce(&mut DadosUsuarios) -> R,
) -> Result<R, Erro> {
    let mut copia = dados.clone();
    let r = f(&mut copia);
    armazenamento.salvar(&copia).map_err(erro_armazenamento)?;
    *dados = copia;
    Ok(r)
}

fn erro_armazenamento(erro: io::Error) -> Erro {
    tracing::error!("falha ao gravar estado de usuário: {erro}");
    (
        CodigoErro::Interno,
        "falha ao gravar estado de usuário".into(),
    )
}

fn impacto(impacto: nvt_core::ImpactoVisual) -> ImpactoVisual {
    match impacto {
        nvt_core::ImpactoVisual::Nenhum => ImpactoVisual::Nenhum,
        nvt_core::ImpactoVisual::Leve => ImpactoVisual::Baixo,
        nvt_core::ImpactoVisual::Forte => ImpactoVisual::Alto,
    }
}

#[cfg(test)]
#[path = "usuario_testes.rs"]
mod testes;
