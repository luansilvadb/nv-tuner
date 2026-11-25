//! Capacidades da GPU (arquitetura, Tensor Cores, Frame Generation, ReBAR,
//! VRAM, driver) normalizadas para uso pelas regras.
//!
//! O preenchimento vem do `nvt-detect`; aqui só há o modelo, sem E/S.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::versao::Versao;

/// Arquitetura da GPU, a partir do `NV_GPU_ARCHITECTURE_ID` da NVAPI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Arquitetura {
    /// RTX 20 e GTX 16 (`0x160`). GTX 16 não tem Tensor Cores.
    Turing,
    /// RTX 30 (`0x170`).
    Ampere,
    /// RTX 40 (`0x190`).
    Ada,
    /// RTX 50 (`0x1B0`).
    Blackwell,
    /// Qualquer outro ID (pré-RTX, datacenter ou futura).
    Unknown(u32),
}

impl Arquitetura {
    /// Converte o `NV_GPU_ARCHITECTURE_ID` lido pela NVAPI.
    pub const fn from_nvapi_id(id: u32) -> Self {
        match id {
            0x160 => Self::Turing,
            0x170 => Self::Ampere,
            0x190 => Self::Ada,
            0x1B0 => Self::Blackwell,
            outro => Self::Unknown(outro),
        }
    }

    /// Nome usado nas condições das regras (`quando.arquitetura`).
    pub const fn nome_regra(self) -> &'static str {
        match self {
            Self::Turing => "turing",
            Self::Ampere => "ampere",
            Self::Ada => "ada",
            Self::Blackwell => "blackwell",
            Self::Unknown(_) => "desconhecida",
        }
    }

    /// Nomes aceitos em `quando.arquitetura`.
    pub const NOMES_REGRA: [&'static str; 5] =
        ["turing", "ampere", "ada", "blackwell", "desconhecida"];
}

impl fmt::Display for Arquitetura {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(id) => write!(f, "desconhecida (0x{id:X})"),
            outra => f.write_str(outra.nome_regra()),
        }
    }
}

/// O que a GPU (e o monitor principal) sabem fazer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub arquitetura: Arquitetura,
    /// Falso em GTX 16 (TU116/TU117), mesmo com arquitetura Turing.
    pub tem_tensor_cores: bool,
    /// Tensor Cores com FP8 (Ada em diante).
    pub tensor_fp8: bool,
    /// DLSS Frame Generation 2x (Ada em diante).
    pub frame_gen: bool,
    /// DLSS Multi Frame Generation (Blackwell).
    pub mfg: bool,
    /// Resizable BAR suportado oficialmente na arquitetura.
    pub rebar_suportado: bool,
    /// Resizable BAR ativo de fato; `None` quando não foi possível ler.
    pub rebar_ativo: Option<bool>,
    pub vram_mb: u32,
    pub driver_version: Versao,
    /// Taxa de atualização do monitor principal, quando conhecida.
    pub refresh_hz: Option<u32>,
}

impl Capabilities {
    /// Capacidades típicas de uma arquitetura, para testes e valores iniciais.
    /// A detecção real sobrescreve tudo com o que foi lido na máquina.
    pub fn tipicas(arquitetura: Arquitetura) -> Self {
        let (tensor, fp8, fg, mfg, rebar) = match arquitetura {
            Arquitetura::Turing => (true, false, false, false, false),
            Arquitetura::Ampere => (true, false, false, false, true),
            Arquitetura::Ada => (true, true, true, false, true),
            Arquitetura::Blackwell => (true, true, true, true, true),
            Arquitetura::Unknown(_) => (false, false, false, false, false),
        };
        Self {
            arquitetura,
            tem_tensor_cores: tensor,
            tensor_fp8: fp8,
            frame_gen: fg,
            mfg,
            rebar_suportado: rebar,
            rebar_ativo: None,
            vram_mb: 8192,
            driver_version: Versao::default(),
            refresh_hz: None,
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn converte_ids_da_nvapi() {
        assert_eq!(Arquitetura::from_nvapi_id(0x160), Arquitetura::Turing);
        assert_eq!(Arquitetura::from_nvapi_id(0x1B0), Arquitetura::Blackwell);
        assert_eq!(
            Arquitetura::from_nvapi_id(0x140),
            Arquitetura::Unknown(0x140)
        );
        assert_eq!(Arquitetura::Unknown(0x1C0).nome_regra(), "desconhecida");
    }
}
