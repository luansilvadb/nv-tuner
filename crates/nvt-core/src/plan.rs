//! Plano de configuração: conjunto ordenado e auditável de alterações
//! (setting, ID DRS, valor novo, origem e explicação). O valor anterior é
//! preenchido pelo serviço ao ler o estado real do driver, para o rollback.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::explicacao::ResumoJogo;
use crate::preferences::Alvo;

/// De onde veio uma mudança.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origem {
    /// Regra comum.
    Regra { id: String },
    /// Preferência do usuário: aplicada diretamente (`regra = None`, ex.:
    /// `limite_fps`) ou traduzida em settings por uma regra condicionada a
    /// ela (ex.: sincronismo).
    Preferencia {
        preferencia: String,
        regra: Option<String>,
    },
}

impl Origem {
    /// Id da regra, se houver.
    pub fn regra(&self) -> Option<&str> {
        match self {
            Self::Regra { id } => Some(id),
            Self::Preferencia { regra, .. } => regra.as_deref(),
        }
    }
}

/// Uma alteração de setting num alvo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingChange {
    /// Nome simbólico (catálogo).
    pub setting: String,
    /// ID DRS.
    pub id: u32,
    pub valor: u32,
    /// Nome do valor no catálogo, quando for um valor nomeado.
    pub valor_nome: Option<String>,
    /// Preenchido pelo serviço ao ler o driver; o avaliador deixa `None`.
    pub valor_anterior: Option<u32>,
    pub origem: Origem,
    /// Texto em pt-BR para a UI: o porquê da mudança.
    pub explicacao: String,
}

/// Regra experimental que casaria: mostrada como sugestão, não aplicada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sugestao {
    pub regra: String,
    pub alvo: Alvo,
    pub mudancas: Vec<SettingChange>,
}

/// Mudança que o plano faria, mas não faz porque a setting está travada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Respeitada {
    pub alvo: Alvo,
    pub mudanca: SettingChange,
}

/// Algo que ficou fora do plano, com o motivo (auditoria).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Descarte {
    /// Regra descartada (`None` para uma preferência direta).
    pub regra: Option<String>,
    pub alvo: Alvo,
    /// Setting afetada, quando o descarte é por setting.
    pub setting: Option<String>,
    pub motivo: String,
}

/// Aviso do plano para a UI (F1-15). O núcleo não gera texto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvisoPlano {
    /// `sincronismo = gsync` num monitor sem VRR (`vrr.possivel = Some(false)`):
    /// a combinação vira V-Sync comum, com fila; orientar "sem sincronismo".
    GsyncSemVrr,
    /// `sincronismo = gsync`, VRR possível mas desligado no painel da NVIDIA
    /// (`possivel = true`, `ativo = false`). O app não liga o VRR do monitor.
    VrrDesligadoNoPainel,
}

/// Resultado do avaliador.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Plan {
    /// Mudanças no perfil global, ordenadas por setting.
    pub global: Vec<SettingChange>,
    /// Mudanças por jogo (`GameFacts::id`), ordenadas por setting.
    pub por_jogo: BTreeMap<String, Vec<SettingChange>>,
    /// Regras experimentais que casaram (não aplicadas).
    pub experimentais: Vec<Sugestao>,
    /// Mudanças suprimidas por escolhas do usuário.
    pub respeitadas: Vec<Respeitada>,
    /// Regras ou mudanças descartadas (teto de qualidade, conflito etc.).
    pub descartes: Vec<Descarte>,
    /// Explicação por jogo (`GameFacts::id`): regras aplicadas, não
    /// aplicadas com o motivo, e settings herdadas do global. Todo jogo
    /// avaliado tem uma entrada.
    #[serde(default)]
    pub resumos: BTreeMap<String, ResumoJogo>,
    /// Avisos sobre a combinação preferências × monitor (F1-15).
    #[serde(default)]
    pub avisos: Vec<AvisoPlano>,
}

impl Plan {
    /// Mudanças de um alvo.
    pub fn mudancas(&self, alvo: &Alvo) -> &[SettingChange] {
        match alvo {
            Alvo::Global => &self.global,
            Alvo::Jogo(id) => self.por_jogo.get(id).map_or(&[], Vec::as_slice),
        }
    }

    /// Busca a mudança de uma setting num alvo.
    pub fn mudanca(&self, alvo: &Alvo, setting: &str) -> Option<&SettingChange> {
        self.mudancas(alvo).iter().find(|m| m.setting == setting)
    }

    /// Verdadeiro se o plano não altera nada.
    pub fn vazio(&self) -> bool {
        self.global.is_empty() && self.por_jogo.values().all(Vec::is_empty)
    }
}
