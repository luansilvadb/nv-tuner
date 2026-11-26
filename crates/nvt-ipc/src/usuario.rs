use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::config_jogo::{FatoConfigJogo, TarefaJogo};
use crate::{ImpactoVisual, Mudanca, StatusMudanca};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Camada {
    #[default]
    Driver,
    Usuario,
}

/// Identificador estável do alvo de exibição, sem índices ou LUIDs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum SettingUsuario {
    Refresh { alvo: String },
    MouseAceleracao,
    DxSwapEffectUpgrade,
    GameDvrCapturaHistorica,
}

impl SettingUsuario {
    pub(crate) fn validar(&self) -> Result<(), String> {
        if let Self::Refresh { alvo } = self {
            validar_texto(alvo, 1024)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshRacional {
    pub numerador: u32,
    pub denominador: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MouseSpi {
    pub limiar1: i32,
    pub limiar2: i32,
    pub aceleracao: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "estado",
    content = "valor",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Fato<T> {
    Desconhecido,
    Ausente,
    Conhecido(T),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FatoTela {
    pub alvo: String,
    pub atual: RefreshRacional,
    pub maximo: RefreshRacional,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FatosUsuario {
    pub tela: Fato<FatoTela>,
    pub mouse: Fato<MouseSpi>,
    pub swap_effect_upgrade: Fato<String>,
    pub captura_historica: Fato<u32>,
    pub diagnosticos: Vec<Diagnostico>,
    /// Gestão persistida pelo agente, fonte de verdade da camada usuário.
    /// Agentes anteriores omitem o campo.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gestao: Vec<GestaoAgente>,
    /// Arquivos de jogo lidos pelo agente (v7).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_jogo: Vec<FatoConfigJogo>,
}

/// Estado persistido pelo agente para um destino acessível na sessão.
/// `valores.antes` é o valor de antes da gestão e `valores.depois` o esperado;
/// no Refresh, `alvo` é o alvo da tela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GestaoAgente {
    pub destino: SettingUsuario,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valores: Option<AjusteUsuario>,
    #[serde(default)]
    pub travado: bool,
}

impl FatosUsuario {
    pub(crate) fn validar(&self) -> Result<(), String> {
        if let Fato::Conhecido(tela) = &self.tela {
            validar_texto(&tela.alvo, 1024)?;
            validar_taxas([tela.atual, tela.maximo])?;
        }
        if let Fato::Conhecido(token) = &self.swap_effect_upgrade {
            validar_token(token)?;
        }
        if self.gestao.len() > 64 {
            return Err("muitos destinos geridos pelo agente".into());
        }
        let mut vistos = BTreeSet::new();
        for g in &self.gestao {
            g.destino.validar()?;
            if !vistos.insert(&g.destino) {
                return Err("destino gerido repetido".into());
            }
            if let Some(valores) = &g.valores {
                valores.validar()?;
                if valores.destino() != g.destino {
                    return Err("gestão não corresponde ao destino".into());
                }
            }
        }
        crate::config_jogo::validar_fatos_jogo(&self.config_jogo)?;
        if self.diagnosticos.len() > 256 {
            return Err("muitos diagnósticos do agente".into());
        }
        for diagnostico in &self.diagnosticos {
            validar_texto(&diagnostico.codigo, 128)?;
            if let EstadoDiagnostico::Conhecido { valor } = &diagnostico.estado {
                validar_texto(valor, 1024)?;
            }
        }
        Ok(())
    }
}

/// Transição tipada por destino. Nos valores de registro, None representa ausência.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum AjusteUsuario {
    Refresh {
        alvo: String,
        antes: RefreshRacional,
        depois: RefreshRacional,
    },
    MouseAceleracao {
        antes: MouseSpi,
        depois: MouseSpi,
    },
    DxSwapEffectUpgrade {
        antes: Option<String>,
        depois: Option<String>,
    },
    GameDvrCapturaHistorica {
        antes: Option<u32>,
        depois: Option<u32>,
    },
}

impl AjusteUsuario {
    pub fn destino(&self) -> SettingUsuario {
        match self {
            Self::Refresh { alvo, .. } => SettingUsuario::Refresh { alvo: alvo.clone() },
            Self::MouseAceleracao { .. } => SettingUsuario::MouseAceleracao,
            Self::DxSwapEffectUpgrade { .. } => SettingUsuario::DxSwapEffectUpgrade,
            Self::GameDvrCapturaHistorica { .. } => SettingUsuario::GameDvrCapturaHistorica,
        }
    }

    pub(crate) fn validar(&self) -> Result<(), String> {
        match self {
            Self::Refresh {
                alvo,
                antes,
                depois,
            } => {
                validar_texto(alvo, 1024)?;
                validar_taxas([*antes, *depois])
            }
            Self::DxSwapEffectUpgrade { antes, depois } => {
                for token in [antes, depois].into_iter().flatten() {
                    validar_token(token)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

fn validar_taxas(taxas: [RefreshRacional; 2]) -> Result<(), String> {
    if taxas
        .iter()
        .any(|taxa| taxa.numerador == 0 || taxa.denominador == 0)
    {
        return Err("taxa de atualização inválida".into());
    }
    Ok(())
}

fn validar_token(token: &str) -> Result<(), String> {
    if token.len() > 256 || token.contains([';', '=']) || token.chars().any(char::is_control) {
        return Err("token SwapEffectUpgradeEnable inválido".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperacaoUsuario {
    Aplicar,
    Restaurar,
    Reverter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenciaTarefa {
    pub id_tarefa: u64,
    pub operacao: OperacaoUsuario,
    pub revisao: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressoTarefa {
    Pendente,
    EmExecucao,
    RelatoPendente,
    Confirmada,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstadoTarefa {
    pub tarefa: ReferenciaTarefa,
    pub progresso: ProgressoTarefa,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum AcaoUsuario {
    Aplicar {
        ajuste: AjusteUsuario,
    },
    Restaurar {
        destino: SettingUsuario,
    },
    /// Restaura todos os destinos do snapshot local ainda pendentes nesta geração.
    RestaurarSnapshot,
    Reverter {
        destino: SettingUsuario,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TarefaUsuario {
    pub tarefa: ReferenciaTarefa,
    pub acoes: Vec<AcaoUsuario>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanoAgente {
    pub revisao: u64,
    pub geracao_restauracao: u64,
    pub tarefa: Option<TarefaUsuario>,
    pub processos_vigiados: BTreeSet<String>,
    /// Tarefa de arquivo de jogo (v7), independente da tarefa Windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tarefa_jogo: Option<TarefaJogo>,
}

impl PlanoAgente {
    /// Validação de forma feita pelo agente antes de executar.
    pub fn validar(&self) -> Result<(), String> {
        if let Some(t) = &self.tarefa_jogo {
            t.validar()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultadoUsuario {
    pub destino: SettingUsuario,
    pub status: StatusMudanca,
    pub detalhe: Option<String>,
    pub mudanca: Option<AjusteUsuario>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstadoAgente {
    pub presente: bool,
    pub restauracao_pendente: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostico {
    pub codigo: String,
    pub estado: EstadoDiagnostico,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum EstadoDiagnostico {
    Conhecido { valor: String },
    Indisponivel,
    Orientacao,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoExperimental {
    #[default]
    Estavel,
    Aceitavel,
    Aceita,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SugestaoExperimental {
    pub regra: String,
    pub descricao: String,
    pub camada: Camada,
    pub impacto_visual: ImpactoVisual,
    pub estado: EstadoExperimental,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumoSistema {
    pub mudancas: Vec<Mudanca>,
    pub sugestoes: Vec<SugestaoExperimental>,
}

pub(crate) fn validar_texto(texto: &str, limite: usize) -> Result<(), String> {
    if texto.is_empty() || texto.len() > limite || texto.chars().any(char::is_control) {
        return Err("campo textual inválido".into());
    }
    Ok(())
}
