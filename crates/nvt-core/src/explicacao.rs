//! Explicação por jogo: o que o plano aplica no perfil de cada jogo, o que
//! deixou de aplicar e por quê (motivos estruturados), e o que o jogo herda
//! do perfil global. O texto em pt-BR é gerado pela UI a partir dos motivos.
//! Semântica em `docs/regras.md`, “Explicação por jogo”.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::capabilities::Arquitetura;
use crate::jogos::Orientacao;
use crate::preferences::Qualidade;
use crate::rules::ImpactoVisual;

/// Motivos que uma regra pode declarar em `motivo_quando_nao_aplica`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoMotivo {
    /// O jogo não tem DLSS.
    JogoSemDlss,
    /// A arquitetura da GPU não precisa do ajuste (ex.: Ada com FP8).
    ArquiteturaNaoPrecisa,
    /// A GPU não tem Tensor Cores (GTX 16).
    SemTensorCores,
}

impl TipoMotivo {
    /// Nomes aceitos em `motivo_quando_nao_aplica`.
    pub const NOMES: [&'static str; 3] = [
        "jogo_sem_dlss",
        "arquitetura_nao_precisa",
        "sem_tensor_cores",
    ];

    pub fn de_nome(nome: &str) -> Option<Self> {
        match nome {
            "jogo_sem_dlss" => Some(Self::JogoSemDlss),
            "arquitetura_nao_precisa" => Some(Self::ArquiteturaNaoPrecisa),
            "sem_tensor_cores" => Some(Self::SemTensorCores),
            _ => None,
        }
    }
}

/// Motivo declarado para uma condição que falhou.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotivoDeclarado {
    /// Vale sempre que a condição falha (`arquitetura = "arquitetura_nao_precisa"`).
    Sempre(TipoMotivo),
    /// Só para condições de lista: vale quando o valor encontrado é uma das
    /// chaves (`arquitetura = { ada = "arquitetura_nao_precisa" }`); outros
    /// valores caem no motivo genérico.
    PorValor(BTreeMap<String, TipoMotivo>),
}

impl MotivoDeclarado {
    /// Motivo para o valor encontrado (`None` = fato desconhecido).
    pub fn para(&self, encontrado: Option<&str>) -> Option<TipoMotivo> {
        match self {
            Self::Sempre(t) => Some(*t),
            Self::PorValor(m) => encontrado.and_then(|v| m.get(v).copied()),
        }
    }
}

/// Uma condição de `quando` que não casou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Falha {
    /// Nome da condição (`"arquitetura"`, `"jogo_dlss"`…).
    pub condicao: &'static str,
    /// O que a regra pede (forma do TOML: `"turing"`, `"true"`, `"310.0"`).
    pub esperado: String,
    /// O que foi encontrado; `None` = fato desconhecido.
    pub encontrado: Option<String>,
}

/// Por que uma regra por jogo não foi aplicada (no todo ou numa setting).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotivoNaoAplicada {
    /// O jogo não tem DLSS (nenhum `nvngx_dlss.dll` encontrado).
    JogoSemDlss,
    /// A arquitetura da GPU não precisa do ajuste.
    ArquiteturaNaoPrecisa { arquitetura: Arquitetura },
    /// A GPU não tem Tensor Cores.
    SemTensorCores,
    /// Regra experimental: só sugestão, aguardando confirmação por medição.
    Experimental,
    /// Impacto visual acima do teto de qualidade escolhido.
    BloqueadaPeloTetoDeQualidade {
        teto: Qualidade,
        impacto: ImpactoVisual,
    },
    /// O usuário travou a setting (mudança manual).
    EscolhaDoUsuario { setting: String },
    /// Outra regra (ou preferência) venceu o conflito na mesma setting.
    /// `regra` é o id da vencedora ou `preferencia:<nome>`.
    ConflitoComRegraMaisEspecifica { regra: String },
    /// A condição depende de um fato que a detecção não determinou.
    FatoDesconhecido { fato: String },
    /// Condição não atendida, sem motivo específico.
    CondicaoNaoAtendida {
        condicao: String,
        esperado: String,
        encontrado: String,
    },
}

impl MotivoNaoAplicada {
    /// Motivo específico a partir de um tipo declarado (ou padrão).
    pub fn de_tipo(t: TipoMotivo, arquitetura: Arquitetura) -> Self {
        match t {
            TipoMotivo::JogoSemDlss => Self::JogoSemDlss,
            TipoMotivo::ArquiteturaNaoPrecisa => Self::ArquiteturaNaoPrecisa { arquitetura },
            TipoMotivo::SemTensorCores => Self::SemTensorCores,
        }
    }

    /// Motivo genérico de uma falha.
    pub fn generico(f: &Falha) -> Self {
        match &f.encontrado {
            None => Self::FatoDesconhecido {
                fato: f.condicao.to_owned(),
            },
            Some(e) => Self::CondicaoNaoAtendida {
                condicao: f.condicao.to_owned(),
                esperado: f.esperado.clone(),
                encontrado: e.clone(),
            },
        }
    }
}

/// Motivo padrão (sem declaração no TOML) de algumas condições.
///
/// - `jogo_dlss = true` (ou `jogo_dlss_min`) num jogo sem DLSS → `JogoSemDlss`;
/// - `tem_tensor_cores = true` numa GPU sem Tensor Cores → `SemTensorCores`.
pub fn motivo_padrao(f: &Falha) -> Option<TipoMotivo> {
    match f.condicao {
        "jogo_dlss" if f.esperado == "true" => Some(TipoMotivo::JogoSemDlss),
        "jogo_dlss_min" if f.encontrado.as_deref() == Some(DLSS_AUSENTE) => {
            Some(TipoMotivo::JogoSemDlss)
        }
        "tem_tensor_cores" if f.esperado == "true" => Some(TipoMotivo::SemTensorCores),
        _ => None,
    }
}

/// Valor de [`Falha::encontrado`] para `jogo_dlss_min` num jogo sem DLSS.
pub const DLSS_AUSENTE: &str = "ausente";

/// Regra aplicada no perfil do jogo e as settings que ela grava.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegraAplicada {
    pub regra: String,
    /// Nomes simbólicos (catálogo), ordenados.
    pub settings: Vec<String>,
}

/// Regra por jogo que não foi aplicada (ou teve uma setting suprimida).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegraNaoAplicada {
    pub regra: String,
    pub motivo: MotivoNaoAplicada,
}

/// Resumo do que o plano faz por um jogo.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ResumoJogo {
    /// Regras aplicadas no perfil do jogo, ordenadas por id.
    pub aplicadas: Vec<RegraAplicada>,
    /// Regras por jogo não aplicadas, com o motivo, ordenadas por id. Uma
    /// regra aplicada pode aparecer aqui também, se uma setting dela foi
    /// travada pelo usuário ou perdeu um conflito.
    pub nao_aplicadas: Vec<RegraNaoAplicada>,
    /// Settings do perfil global que o jogo herda (as que o perfil do jogo
    /// não sobrescreve no plano), ordenadas.
    pub herda_do_global: Vec<String>,
    /// Códigos de orientação da base curada para o jogo, filtrados pelas
    /// preferências (F1-12; ver [`crate::jogos::filtrar_orientacoes`]). Vazio
    /// para jogo fora da base. A UI traduz cada código.
    #[serde(default)]
    pub orientacoes: Vec<Orientacao>,
}
