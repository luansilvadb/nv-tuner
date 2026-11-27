//! Mensagens do protocolo (DTOs).
//!
//! São tipos próprios do IPC, independentes do motor: o serviço converte
//! entre eles e o seu modelo interno. A UI só envia comandos de alto nível;
//! configurações do driver aparecem apenas por nome simbólico (ex.:
//! `"vsync"`, `"dlss_preset"`), nunca por ID DRS ou valor bruto.

use serde::{Deserialize, Serialize};

use crate::config_jogo::{ConfigJogoUi, DestinoJogoIpc, ResultadoJogo};
use crate::saude::{FonteInicio, PARTIDAS_MAX, Partida, ResumoPartida, Saude, SaudeSessao};

use crate::usuario::{
    Camada, Diagnostico, EstadoAgente, EstadoExperimental, EstadoTarefa, FatosUsuario,
    OperacaoUsuario, PlanoAgente, ResultadoUsuario, ResumoSistema, SettingUsuario,
    SugestaoExperimental, ascii_restrito, validar_texto,
};

mod avisos;
mod plano;
mod resposta;
#[cfg(test)]
mod testes;

pub use avisos::*;
pub use plano::*;
pub use resposta::*;

/// Limite inferior aceito para o limitador de FPS.
pub const LIMITE_FPS_MIN: u32 = 20;
/// Limite superior aceito para o limitador de FPS.
pub const LIMITE_FPS_MAX: u32 = 1000;
/// Máximo de entradas pedidas em [`Requisicao::ListarHistorico`].
pub const HISTORICO_MAX: u32 = 1000;
/// Comprimento máximo do nome de um perfil (em caracteres).
pub const PERFIL_MAX: usize = 256;
/// Comprimento máximo de um nome simbólico de setting.
pub const SETTING_MAX: usize = 64;
/// Comprimento máximo do identificador de regra experimental.
pub const REGRA_MAX: usize = 128;
/// Máximo de resultados em [`Requisicao::AgenteRelatar`].
pub const RESULTADOS_MAX: usize = 256;
/// Comprimento máximo do detalhe de um resultado.
pub const DETALHE_MAX: usize = 1024;
/// Comprimento máximo de uma versão.
const VERSAO_MAX: usize = 64;

// ---------------------------------------------------------------------------
// Requisições
// ---------------------------------------------------------------------------

/// Requisição enviada pela UI ao serviço.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum Requisicao {
    /// Resumo do estado atual.
    ObterEstado,
    /// Plano que seria aplicado agora (sem aplicar).
    ObterPlano,
    /// Calcula e aplica o plano imediatamente.
    AplicarAgora,
    /// Restaura os valores originais de tudo que o nv-tuner alterou.
    RestaurarOriginal,
    /// Atualiza as preferências do usuário.
    DefinirPreferencias {
        sincronismo: Sincronismo,
        qualidade: Qualidade,
        /// `None` = sem limitador.
        limite_fps: Option<u32>,
    },
    /// Últimas `limite` entradas do histórico (mais recentes primeiro).
    ListarHistorico {
        limite: u32,
    },
    /// Marca uma setting como escolha do usuário: o avaliador não a altera.
    TravarSetting {
        perfil: String,
        setting: String,
    },
    /// Devolve uma setting travada ao controle do avaliador.
    DestravarSetting {
        perfil: String,
        setting: String,
    },
    /// Refaz a varredura de jogos instalados.
    ReescanearJogos,
    AgenteConsultar {
        fatos: FatosUsuario,
        ultima_tarefa: Option<EstadoTarefa>,
        geracao_restaurada: u64,
    },
    AgenteRelatar {
        id_tarefa: u64,
        operacao: OperacaoUsuario,
        revisao: u64,
        resultados: Vec<ResultadoUsuario>,
        /// Resultados das ações de arquivo de jogo (v7).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        resultados_jogo: Vec<ResultadoJogo>,
        /// Saúde lida na sessão do usuário (v8); ausente = sem leitura nova.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        saude: Option<SaudeSessao>,
    },
    DefinirAceiteExperimental {
        regra: String,
        aceito: bool,
    },
    ReverterAjusteUsuario {
        destino: SettingUsuario,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alvo_tela: Option<String>,
    },
    /// Reversão individual de uma chave de arquivo de jogo (v7).
    ReverterConfigJogo {
        destino: DestinoJogoIpc,
    },
    /// O usuário viu o aviso "Atualizado para a vX.Y.Z" (v6, U-47/U-52).
    /// A identidade vem só do token do chamador; `versao` diferente de
    /// `ultima_atualizacao.para` é recusada com `entrada_invalida`.
    MarcarAtualizacaoVista {
        versao: String,
    },
    /// Aceita, para o SID chamador, todas as experimentais aplicáveis numa
    /// única gravação (v8, R-AR). Resposta [`Resposta::Recomendadas`].
    AplicarRecomendadas,
    /// Sinais, inicialização e limpeza do SID chamador e da máquina (v8).
    /// Resposta [`Resposta::Saude`].
    ObterSaude,
    /// Incorpora o programa `(fonte, nome)` à linha de base (v8); o agente
    /// busca o pedido na próxima consulta.
    MarcarInicioVisto {
        fonte: FonteInicio,
        nome: String,
    },
    /// Resumos de partidas do SID chamador (v8, só `medicao-partidas`).
    ListarPartidas {
        desde_unix: u64,
        limite: u32,
    },
    /// Resumo e amostras de uma partida do SID chamador (v8).
    ObterPartida {
        id: u64,
    },
    /// Apaga as partidas do SID chamador (v8).
    ApagarPartidas,
}

impl Requisicao {
    /// Validação de forma, feita pelo servidor antes de chamar o tratador.
    ///
    /// Não substitui a validação semântica do serviço (lista permitida de
    /// settings, existência do perfil); apenas rejeita entradas absurdas.
    pub fn validar(&self) -> Result<(), String> {
        match self {
            Self::DefinirPreferencias {
                limite_fps: Some(fps),
                ..
            } if !(LIMITE_FPS_MIN..=LIMITE_FPS_MAX).contains(fps) => Err(format!(
                "limite_fps deve estar entre {LIMITE_FPS_MIN} e {LIMITE_FPS_MAX}"
            )),
            Self::ListarHistorico { limite } if !(1..=HISTORICO_MAX).contains(limite) => {
                Err(format!("limite deve estar entre 1 e {HISTORICO_MAX}"))
            }
            Self::TravarSetting { perfil, setting }
            | Self::DestravarSetting { perfil, setting } => {
                validar_perfil(perfil)?;
                validar_setting(setting)
            }
            Self::DefinirAceiteExperimental { regra, .. } => {
                let classe = |b: u8| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')
                };
                if !ascii_restrito(regra, REGRA_MAX, classe, classe) {
                    return Err("identificador de regra inválido".into());
                }
                Ok(())
            }
            Self::AgenteConsultar { fatos, .. } => fatos.validar(),
            Self::AgenteRelatar {
                resultados,
                resultados_jogo,
                saude,
                ..
            } => {
                crate::config_jogo::validar_resultados_jogo(resultados_jogo)?;
                if let Some(saude) = saude {
                    saude.validar()?;
                }
                if resultados.len() > RESULTADOS_MAX {
                    return Err("muitos resultados da tarefa".into());
                }
                let mut destinos = std::collections::BTreeSet::new();
                for resultado in resultados {
                    resultado.destino.validar()?;
                    if !destinos.insert(&resultado.destino) {
                        return Err("destino repetido no relato".into());
                    }
                    if let Some(mudanca) = &resultado.mudanca {
                        mudanca.validar()?;
                        if mudanca.destino() != resultado.destino {
                            return Err("mudança de destino incompatível com o relato".into());
                        }
                    }
                    if let Some(detalhe) = &resultado.detalhe {
                        validar_texto(detalhe, DETALHE_MAX)?;
                    }
                }
                Ok(())
            }
            Self::ReverterAjusteUsuario { destino, alvo_tela } => {
                destino.validar()?;
                if let Some(alvo_tela) = alvo_tela
                    && !matches!(destino, SettingUsuario::Refresh { alvo } if alvo == alvo_tela)
                {
                    return Err("alvo_tela incompatível com o destino".into());
                }
                Ok(())
            }
            Self::ReverterConfigJogo { destino } => destino.validar(),
            Self::MarcarAtualizacaoVista { versao } => validar_versao(versao),
            Self::MarcarInicioVisto { nome, .. } => crate::saude::validar_nome_inicio(nome),
            Self::ListarPartidas { limite, .. } if !(1..=PARTIDAS_MAX).contains(limite) => {
                Err(format!("limite deve estar entre 1 e {PARTIDAS_MAX}"))
            }
            _ => Ok(()),
        }
    }
}

/// Versão (`X.Y.Z`, opcionalmente com sufixo): 1 a 64 caracteres ASCII
/// alfanuméricos, `.`, `-` ou `+`.
pub fn validar_versao(versao: &str) -> Result<(), String> {
    let classe = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+');
    if !ascii_restrito(versao, VERSAO_MAX, classe, classe) {
        return Err("versão inválida".into());
    }
    Ok(())
}

/// Nome de perfil: 1 a [`PERFIL_MAX`] caracteres, sem caracteres de controle.
pub fn validar_perfil(perfil: &str) -> Result<(), String> {
    let n = perfil.chars().count();
    if n == 0 || n > PERFIL_MAX || perfil.chars().any(char::is_control) {
        return Err(format!(
            "nome de perfil inválido (1 a {PERFIL_MAX} caracteres, sem controle)"
        ));
    }
    Ok(())
}

/// Nome simbólico de setting: 1 a [`SETTING_MAX`] caracteres em
/// `[a-z0-9_.]`, começando por letra.
pub fn validar_setting(setting: &str) -> Result<(), String> {
    let ok = ascii_restrito(
        setting,
        SETTING_MAX,
        |b| b.is_ascii_lowercase(),
        |b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.',
    );
    if ok {
        Ok(())
    } else {
        Err(format!(
            "nome simbólico de setting inválido (até {SETTING_MAX} caracteres em [a-z0-9_.])"
        ))
    }
}

// ---------------------------------------------------------------------------
// Preferências
// ---------------------------------------------------------------------------

/// Política de sincronismo de quadros.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sincronismo {
    /// Sem V-Sync nem G-Sync (menor latência).
    Desligado,
    /// V-Sync forçado no driver.
    VSync,
    /// G-Sync/VRR com V-Sync no driver e limitador abaixo da taxa do monitor.
    GSync,
}

/// Teto de qualidade visual escolhido pelo usuário.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Qualidade {
    Desempenho,
    Equilibrado,
    Qualidade,
}

/// Preferências atuais (eco do que foi definido).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferencias {
    pub sincronismo: Sincronismo,
    pub qualidade: Qualidade,
    pub limite_fps: Option<u32>,
}
