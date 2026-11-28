//! Lógica do serviço, independente do SCM, das threads e do driver real.
//!
//! [`Nucleo`] guarda o motor, os jogos detectados e o estado próprio do
//! serviço (autonomia, última aplicação, alertas) e expõe:
//!
//! - [`Nucleo::iniciar`] / [`Nucleo::ciclo`]: deriva → tratamento → plano →
//!   aplicação, respeitando autonomia e modo somente leitura;
//! - [`Nucleo::notificar_mudanca_drs`] / [`Nucleo::tick`]: debounce das
//!   notificações do diretório do DRS, verificação e varredura periódicas,
//!   ignorando as mudanças que o próprio serviço gravou;
//! - [`Nucleo::tratar`]: tratador das requisições do IPC.
//!
//! Toda gravação no DRS passa pelo portão do motor
//! ([`Engine::pode_gravar`], T-02). Com jogo ou anticheat vigiado em
//! execução, a ação fica pendente ([`AcaoPendente`]) e é reavaliada a cada
//! [`Config::intervalo_adiamento`]; ao fechar o jogo, roda uma vez (F1-07).
//! A pendência só sai depois que a ação dá certo: um erro que não é
//! adiamento é tentado de novo no mesmo intervalo, até
//! [`TENTATIVAS_PENDENTE`] vezes, e então vira aviso e é descartada. A
//! restauração pendente fica também em `servico.json` e é retomada quando o
//! serviço reinicia.
//!
//! O tempo é sempre passado por quem chama (`agora`), para os testes
//! controlarem o relógio.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nvt_core::{
    Alvo, AvisoPlano, BaseJogos, DefSetting, ImpactoVisual as ImpactoRegra, Origem, Preferences,
};
use nvt_drs::ProfileRef;
use nvt_engine::{
    AcaoReversao, Adiamento, Aplicacao, Armazem, ClasseDeriva, CodigoAviso, Deriva, Diff,
    Divergencia, DriverStore, Engine, ErroEngine, Esperado, JogoEntrada, MAX_TRAVADAS,
    OrigemRegistro, Portao, Resultado, casar_na_base, mesmo_alvo, mesmo_nome_perfil,
};
use nvt_ipc::{
    AcaoHistorico, Aviso, Capacidades, CodigoErro, EntradaHistorico, Mudanca, NivelAviso, Plano,
    PlanoPerfil, Requisicao, Resposta, StatusMudanca,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::nomes;

mod adiamento;
mod ciclo;
mod estado;
mod ipc;
mod requisicoes;
mod rota;

use estado::*;
pub use ipc::*;
pub(crate) use rota::*;

/// Abre uma sessão nova do DRS (o driver só é relido ao abrir a sessão).
pub type Fabrica<'a, S> = Box<dyn FnMut() -> Resultado<S> + 'a>;
/// Varre os jogos instalados.
pub type Varredor<'a> = Box<dyn FnMut() -> Vec<JogoEntrada> + 'a>;
/// Assinatura do diretório do DRS (nome, tamanho, data de modificação).
pub type Assinatura = Vec<(String, u64, u128)>;
/// Calcula a [`Assinatura`] atual (`None` se indisponível).
pub type Assinador<'a> = Box<dyn FnMut() -> Option<Assinatura> + 'a>;

/// Nome do perfil global no protocolo IPC.
pub const PERFIL_GLOBAL: &str = "global";
const ARQ_SERVICO: &str = "servico.json";
const ALERTAS_MAX: usize = 100;
/// Execuções seguidas com erro (que não é adiamento) antes de a ação
/// pendente ser descartada.
pub const TENTATIVAS_PENDENTE: u32 = 3;

/// Intervalos do laço.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Impede qualquer gravação (driver e estado).
    pub somente_leitura: bool,
    /// Verificação periódica de deriva.
    pub intervalo_verificacao: Duration,
    /// Nova varredura de jogos.
    pub intervalo_varredura: Duration,
    /// Silêncio exigido após a última notificação do DRS antes de verificar.
    pub debounce: Duration,
    /// Notificações até este tempo depois de uma gravação própria são ignoradas.
    pub janela_propria: Duration,
    /// Reavaliação do portão enquanto houver ação pendente (jogo aberto).
    pub intervalo_adiamento: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            somente_leitura: false,
            intervalo_verificacao: Duration::from_secs(10 * 60),
            intervalo_varredura: Duration::from_secs(6 * 60 * 60),
            debounce: Duration::from_secs(3),
            janela_propria: Duration::from_secs(15),
            intervalo_adiamento: Duration::from_secs(15),
        }
    }
}

/// Gravação que ficou para quando o jogo fechar (F1-07). Só uma por vez: a
/// mais recente substitui as anteriores, exceto [`AcaoPendente::RestaurarOriginal`],
/// que vence qualquer outra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcaoPendente {
    /// Aplicação do plano (automática, preferências, destravar, “Aplicar agora”).
    AplicarPlano,
    /// Tratamento de deriva com reaplicação no driver.
    TratarDeriva,
    /// “Restaurar original”.
    RestaurarOriginal,
}

impl AcaoPendente {
    fn ipc(self) -> nvt_ipc::AcaoAdiada {
        match self {
            Self::AplicarPlano => nvt_ipc::AcaoAdiada::AplicarPlano,
            Self::TratarDeriva => nvt_ipc::AcaoAdiada::TratarDeriva,
            Self::RestaurarOriginal => nvt_ipc::AcaoAdiada::RestaurarOriginal,
        }
    }
}

/// Ação pendente e o último adiamento visto pelo portão.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pendente {
    pub acao: AcaoPendente,
    pub adiamento: Adiamento,
    /// Execuções que já falharam (erro que não é adiamento).
    pub tentativas: u32,
    /// Erro da última execução que falhou (limpo quando o portão adia de
    /// novo): enquanto houver, a pendência não é exibida como jogo aberto.
    pub ultimo_erro: Option<String>,
}

impl Pendente {
    /// O portão de fato adiou (processos vigiados ou lista indisponível) e a
    /// última execução não falhou por outro erro? A pendência carregada do
    /// disco, ainda não reavaliada (processos vazios), não é adiamento real.
    fn adiamento_real(&self) -> bool {
        self.ultimo_erro.is_none()
            && (self.adiamento.lista_indisponivel || !self.adiamento.processos.is_empty())
    }
}

/// Última aplicação de plano (persistida em `servico.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UltimaAplicacao {
    pub quando_unix: u64,
    pub mudancas: u32,
    pub falhas: u32,
}

/// Estado próprio do serviço (`<estado-dir>\servico.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstadoServico {
    /// Pausada por `RestaurarOriginal`; reativada por `AplicarAgora`.
    pub autonomia_pausada: bool,
    pub pausada_em_unix: Option<u64>,
    pub ultima_aplicacao: Option<UltimaAplicacao>,
    /// `RestaurarOriginal` adiada pelo portão (desde quando, Unix): retomada
    /// quando o serviço reinicia. Ausente nos arquivos antigos.
    #[serde(default)]
    pub restauracao_pendente_desde_unix: Option<u64>,
    #[serde(default)]
    pub geracao_restauracao: u64,
}

impl EstadoServico {
    pub(crate) fn pedir_restauracao(&mut self, dir: Option<&Path>) -> std::io::Result<()> {
        if self.restauracao_pendente_desde_unix.is_some() && self.autonomia_pausada {
            return Ok(());
        }
        let mut novo = self.clone();
        if !novo.autonomia_pausada || novo.geracao_restauracao == 0 {
            novo.geracao_restauracao = novo
                .geracao_restauracao
                .checked_add(1)
                .ok_or_else(|| std::io::Error::other("limite de gerações de restauração"))?;
        }
        let agora = agora_unix();
        novo.autonomia_pausada = true;
        novo.pausada_em_unix = Some(agora);
        novo.restauracao_pendente_desde_unix = Some(agora);
        self.persistir(novo, dir)
    }

    pub(crate) fn retomar_autonomia(&mut self, dir: Option<&Path>) -> std::io::Result<()> {
        if !self.autonomia_pausada {
            return Ok(());
        }
        let mut novo = self.clone();
        novo.autonomia_pausada = false;
        novo.pausada_em_unix = None;
        self.persistir(novo, dir)
    }

    fn persistir(&mut self, novo: Self, dir: Option<&Path>) -> std::io::Result<()> {
        if let Some(dir) = dir {
            novo.salvar(dir)?;
        }
        *self = novo;
        Ok(())
    }

    /// Carrega de `dir` (ausente ou inválido = padrão).
    pub fn carregar(dir: Option<&Path>) -> Self {
        let Some(dir) = dir else {
            return Self::default();
        };
        let ler = || -> std::io::Result<String> {
            let mut s = String::new();
            std::io::Read::read_to_string(
                &mut crate::diretorio::abrir_sem_reparse(
                    &dir.join(ARQ_SERVICO),
                    fs::OpenOptions::new().read(true),
                )?,
                &mut s,
            )?;
            Ok(s)
        };
        match ler() {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                warn!("servico.json inválido ({e}); usando o padrão");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Grava em `dir` (temporário novo + renomear). O temporário é sempre
    /// criado do zero (`create_new`, sem seguir reparse points): um link
    /// plantado no lugar dele é removido (o próprio link, não o alvo).
    pub fn salvar(&self, dir: &Path) -> std::io::Result<()> {
        fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("{ARQ_SERVICO}.tmp"));
        let _ = fs::remove_file(&tmp);
        let texto = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let mut f = crate::diretorio::abrir_sem_reparse(
            &tmp,
            fs::OpenOptions::new().write(true).create_new(true),
        )?;
        std::io::Write::write_all(&mut f, texto.as_bytes())?;
        f.sync_all()?;
        drop(f);
        fs::rename(tmp, dir.join(ARQ_SERVICO))
    }
}

/// Segundos Unix agora.
pub fn agora_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// ISO 8601 UTC (`AAAA-MM-DDTHH:MM:SSZ`, formato do `nvt-engine`) para Unix.
pub fn iso_para_unix(s: &str) -> u64 {
    let n = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<i64>().ok());
    let (Some(y), Some(m), Some(d), Some(hh), Some(mm), Some(ss)) =
        (n(0, 4), n(5, 7), n(8, 10), n(11, 13), n(14, 16), n(17, 19))
    else {
        return 0;
    };
    // days_from_civil (Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let dias = era * 146_097 + doe - 719_468;
    u64::try_from(dias * 86_400 + hh * 3600 + mm * 60 + ss).unwrap_or(0)
}

/// Lógica do serviço sobre um [`DriverStore`].
pub struct Nucleo<'a, S: DriverStore> {
    engine: Engine<S>,
    regras_drs: nvt_core::ConjuntoRegras,
    aceites_drs: BTreeSet<String>,
    /// F3-13: jogos com limite de FPS efetivo no próprio jogo (SID da sessão
    /// de console) → regra. O perfil DRS deles recebe `limite_fps = off`.
    limites_jogo: BTreeMap<nvt_core::IdJogo, String>,
    fabrica: Fabrica<'a, S>,
    varredor: Varredor<'a>,
    assinador: Assinador<'a>,
    cfg: Config,
    dir: Option<PathBuf>,
    servico: EstadoServico,
    jogos: Vec<JogoEntrada>,
    alertas: Vec<nvt_ipc::AlertaDeriva>,
    /// Avisos da abertura do estado (acumulados: o arquivo corrompido só é
    /// visto na primeira abertura, depois de renomeado).
    avisos_estado: Vec<Aviso>,
    /// Erros isolados por perfil da última aplicação.
    erros_aplicacao: Vec<Aviso>,
    /// Erros isolados por perfil do último tratamento de deriva.
    erros_deriva: Vec<Aviso>,
    /// Ação pendente descartada após [`TENTATIVAS_PENDENTE`] falhas.
    erros_pendente: Vec<Aviso>,
    /// Jogos ignorados no último plano calculado e quando.
    ignorados: Vec<nvt_ipc::JogoIgnorado>,
    ignorados_unix: u64,
    /// Avisos do último plano calculado (G-SYNC sem VRR etc.) e quando mudaram.
    avisos_plano: Vec<AvisoPlano>,
    avisos_plano_unix: u64,
    ultima_gravacao: Option<Instant>,
    assinatura_propria: Option<Assinatura>,
    mudanca_pendente: Option<Instant>,
    proxima_verificacao: Instant,
    proxima_varredura: Instant,
    /// Portão de gravação, o mesmo em cada motor recriado por `renovar`.
    portao: Portao,
    /// Gravação adiada por jogo aberto (F1-07).
    pendente: Option<Pendente>,
    /// Próxima consulta ao portão (só com `pendente`).
    proxima_reavaliacao: Option<Instant>,
}

#[cfg(test)]
mod testes {
    use super::iso_para_unix;

    #[test]
    fn iso_ida_e_volta() {
        assert_eq!(iso_para_unix("1970-01-01T00:00:00Z"), 0);
        assert_eq!(iso_para_unix("2025-11-25T13:45:00Z"), 1_764_078_300);
        assert_eq!(iso_para_unix("lixo"), 0);
    }
}
