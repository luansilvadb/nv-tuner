//! Exibição: refresh atual e VRR do monitor principal (primário GDI) pela NVAPI.
//!
//! - `NvAPI_DISP_GetDisplayConfig` (três passadas: contagem de caminhos, contagem de alvos
//!   e, com `sourceModeInfo`/`targetInfo`/`details` alocados, os dados) dá o caminho cuja
//!   origem tem `bGDIPrimary`, e dele o `displayId` e o `refreshRate1K` do alvo.
//! - `NvAPI_Disp_GetVRRInfo(displayId)` dá `bIsVRRPossible` e `bIsVRREnabled`.
//!
//! Não usa APIs Win32 de modo de vídeo: o serviço roda na sessão 0, sem área de trabalho
//! interativa (F1-01, F1-02).
//!
//! [`ler_exibicao_principal`] nunca falha nem entra em pânico: qualquer erro vira `None`.
//! A fonte é a trait [`FonteExibicao`], implementada por [`NvApi`] e por [`FonteFalsa`]
//! (testes sem driver).

use crate::api::NvApi;
use crate::error::{Error, Result};
use crate::ffi::{
    FnDispGetDisplayConfig, FnDispGetVrrInfo, NvDisplayConfigPathAdvancedTargetInfoV1,
    NvDisplayConfigPathInfoV2, NvDisplayConfigPathTargetInfoV2, NvDisplayConfigSourceModeInfoV1,
    NvGetVrrInfoV1, id,
};

/// Limites de sanidade contra contagens absurdas devolvidas pelo driver.
const MAX_CAMINHOS: u32 = 64;
const MAX_ALVOS_POR_CAMINHO: u32 = 16;

/// Refresh e VRR do monitor principal. `None` = desconhecido (falha da NVAPI, função
/// ausente ou monitor principal fora da GPU NVIDIA).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InfoExibicao {
    /// `displayId` NVAPI do alvo do primário GDI.
    pub display_id: Option<u32>,
    /// Refresh atual em milésimos de Hz (`refreshRate1K`; 239_760 = 239,76 Hz).
    pub refresh_mhz: Option<u32>,
    /// `bIsVRRPossible`.
    pub vrr_possivel: Option<bool>,
    /// `bIsVRREnabled`.
    pub vrr_ativo: Option<bool>,
}

/// Um caminho de `NvAPI_DISP_GetDisplayConfig` (uma origem e seus alvos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaminhoExibicao {
    /// A origem tem `bGDIPrimary` (é o monitor principal do Windows).
    pub gdi_primario: bool,
    /// `false` se o caminho tem `IsNonNVIDIAAdapter`.
    pub adaptador_nvidia: bool,
    pub alvos: Vec<AlvoExibicao>,
}

/// Alvo (monitor) de um caminho.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlvoExibicao {
    pub display_id: u32,
    /// `refreshRate1K`; `None` se 0 ou sem `details`.
    pub refresh_mhz: Option<u32>,
    /// Bit `primary` (primário do clone, não o primário GDI).
    pub primario_clone: bool,
}

/// Resultado bruto de `NvAPI_Disp_GetVRRInfo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VrrBruto {
    pub possivel: bool,
    pub ativo: bool,
}

/// Fonte das informações de exibição. Implementada por [`NvApi`] e por [`FonteFalsa`].
pub trait FonteExibicao {
    /// Caminhos da configuração de exibição atual.
    fn caminhos(&self) -> Result<Vec<CaminhoExibicao>>;
    /// VRR do `display_id`.
    fn vrr(&self, display_id: u32) -> Result<VrrBruto>;
}

/// Refresh e VRR do monitor principal (primário GDI). Nunca falha: erro vira `None`.
pub fn ler_exibicao_principal<F: FonteExibicao + ?Sized>(fonte: &F) -> InfoExibicao {
    let Ok(caminhos) = fonte.caminhos() else {
        return InfoExibicao::default();
    };
    let Some(caminho) = caminhos.iter().find(|c| c.gdi_primario) else {
        return InfoExibicao::default();
    };
    if !caminho.adaptador_nvidia {
        return InfoExibicao::default();
    }
    let alvo = caminho
        .alvos
        .iter()
        .find(|a| a.primario_clone)
        .or_else(|| caminho.alvos.first());
    let Some(alvo) = alvo.filter(|a| a.display_id != 0) else {
        return InfoExibicao::default();
    };
    let vrr = fonte.vrr(alvo.display_id).ok();
    InfoExibicao {
        display_id: Some(alvo.display_id),
        refresh_mhz: alvo.refresh_mhz.filter(|&r| r > 0),
        vrr_possivel: vrr.map(|v| v.possivel),
        vrr_ativo: vrr.map(|v| v.ativo),
    }
}

impl FonteExibicao for NvApi {
    fn caminhos(&self) -> Result<Vec<CaminhoExibicao>> {
        const F: &str = "NvAPI_DISP_GetDisplayConfig";
        let get: FnDispGetDisplayConfig = self.resolve(id::DISP_GET_DISPLAY_CONFIG, F)?;

        // 1ª passada: só a contagem de caminhos.
        let mut n = 0u32;
        // SAFETY: contador de saída válido; `pathInfo` nulo com `*count == 0` é o uso
        // documentado da primeira passada.
        self.check(F, unsafe { get(&mut n, std::ptr::null_mut()) })?;
        let n = n.min(MAX_CAMINHOS);
        if n == 0 {
            return Ok(Vec::new());
        }

        // 2ª passada: caminhos versionados, sem alvos nem modos, para obter `targetInfoCount`.
        let mut caminhos = vec![NvDisplayConfigPathInfoV2::zeroed(); n as usize];
        let mut n2 = n;
        // SAFETY: vetor de `n2` estruturas V2 com `version` preenchido e ponteiros nulos
        // (permitido: "sourceModeInfo may be NULL"; `targetInfo` nulo pede só a contagem).
        self.check(F, unsafe { get(&mut n2, caminhos.as_mut_ptr()) })?;
        caminhos.truncate(n2.min(n) as usize);

        // 3ª passada: aloca origem, alvos e `details` de cada caminho. Os buffers ficam
        // nestes vetores, que não são redimensionados enquanto o driver tem os ponteiros.
        let mut modos = vec![NvDisplayConfigSourceModeInfoV1::default(); caminhos.len()];
        let mut alvos: Vec<Vec<NvDisplayConfigPathTargetInfoV2>> = Vec::new();
        let mut detalhes: Vec<Vec<NvDisplayConfigPathAdvancedTargetInfoV1>> = Vec::new();
        for c in &caminhos {
            let k = c.target_info_count.min(MAX_ALVOS_POR_CAMINHO) as usize;
            alvos.push(vec![
                NvDisplayConfigPathTargetInfoV2 {
                    display_id: 0,
                    details: std::ptr::null_mut(),
                    target_id: 0,
                };
                k
            ]);
            detalhes.push(vec![NvDisplayConfigPathAdvancedTargetInfoV1::zeroed(); k]);
        }
        for (i, c) in caminhos.iter_mut().enumerate() {
            let det = detalhes[i].as_mut_ptr();
            for (j, a) in alvos[i].iter_mut().enumerate() {
                // SAFETY: `j < detalhes[i].len()` (mesmo `k` dos dois vetores).
                a.details = unsafe { det.add(j) };
            }
            c.target_info_count = alvos[i].len() as u32;
            c.target_info = if alvos[i].is_empty() {
                std::ptr::null_mut()
            } else {
                alvos[i].as_mut_ptr()
            };
            c.source_mode_info = &mut modos[i];
        }
        let mut n3 = caminhos.len() as u32;
        // SAFETY: cada caminho aponta para `target_info_count` alvos, um modo de origem e
        // um `details` V1 por alvo, todos vivos até o fim desta função; `n3` é o tamanho de
        // `caminhos`.
        self.check(F, unsafe { get(&mut n3, caminhos.as_mut_ptr()) })?;
        let validos = (n3 as usize).min(caminhos.len());

        // Leitura pelos nossos próprios buffers (nada de seguir ponteiros do driver).
        let saida = caminhos[..validos]
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let k = (c.target_info_count as usize).min(alvos[i].len());
                let lista = alvos[i][..k]
                    .iter()
                    .zip(&detalhes[i])
                    .map(|(a, d)| {
                        let tem_details = !a.details.is_null();
                        AlvoExibicao {
                            display_id: a.display_id,
                            refresh_mhz: (tem_details && d.refresh_rate_1k > 0)
                                .then_some(d.refresh_rate_1k),
                            primario_clone: tem_details
                                && d.flags & NvDisplayConfigPathAdvancedTargetInfoV1::FLAG_PRIMARY
                                    != 0,
                        }
                    })
                    .collect();
                CaminhoExibicao {
                    gdi_primario: modos[i].flags
                        & NvDisplayConfigSourceModeInfoV1::FLAG_GDI_PRIMARY
                        != 0,
                    adaptador_nvidia: c.flags & NvDisplayConfigPathInfoV2::FLAG_NON_NVIDIA_ADAPTER
                        == 0,
                    alvos: lista,
                }
            })
            .collect();
        Ok(saida)
    }

    fn vrr(&self, display_id: u32) -> Result<VrrBruto> {
        const F: &str = "NvAPI_Disp_GetVRRInfo";
        let get: FnDispGetVrrInfo = self.resolve(id::DISP_GET_VRR_INFO, F)?;
        let mut info = NvGetVrrInfoV1 {
            version: NvGetVrrInfoV1::VER,
            ..Default::default()
        };
        // SAFETY: estrutura V1 com `version` preenchido; `display_id` é só um número.
        self.check(F, unsafe { get(display_id, &mut info) })?;
        Ok(VrrBruto {
            possivel: info.flags & NvGetVrrInfoV1::FLAG_POSSIBLE != 0,
            ativo: info.flags & NvGetVrrInfoV1::FLAG_ENABLED != 0,
        })
    }
}

/// Fonte falsa para testes sem NVAPI. `None` em um campo simula falha da chamada.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FonteFalsa {
    pub caminhos: Option<Vec<CaminhoExibicao>>,
    /// VRR por `displayId`; ausente = falha de `NvAPI_Disp_GetVRRInfo`.
    pub vrr: Vec<(u32, VrrBruto)>,
}

impl FonteFalsa {
    /// NVAPI indisponível: todas as chamadas falham.
    pub fn sem_nvapi() -> Self {
        Self::default()
    }

    /// Um monitor principal NVIDIA com `display_id`, refresh em mHz e VRR opcional.
    pub fn monitor(display_id: u32, refresh_mhz: u32, vrr: Option<VrrBruto>) -> Self {
        Self {
            caminhos: Some(vec![CaminhoExibicao {
                gdi_primario: true,
                adaptador_nvidia: true,
                alvos: vec![AlvoExibicao {
                    display_id,
                    refresh_mhz: (refresh_mhz > 0).then_some(refresh_mhz),
                    primario_clone: true,
                }],
            }]),
            vrr: vrr.map(|v| vec![(display_id, v)]).unwrap_or_default(),
        }
    }
}

impl FonteExibicao for FonteFalsa {
    fn caminhos(&self) -> Result<Vec<CaminhoExibicao>> {
        self.caminhos.clone().ok_or(Error::FunctionUnavailable {
            function: "NvAPI_DISP_GetDisplayConfig",
        })
    }

    fn vrr(&self, display_id: u32) -> Result<VrrBruto> {
        self.vrr
            .iter()
            .find(|(d, _)| *d == display_id)
            .map(|&(_, v)| v)
            .ok_or(Error::FunctionUnavailable {
                function: "NvAPI_Disp_GetVRRInfo",
            })
    }
}

#[cfg(test)]
mod tests {
    use std::mem::{offset_of, size_of};

    use super::*;
    use crate::ffi;

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn layouts_conferidos_com_o_sdk() {
        assert_eq!(size_of::<ffi::NvTimingExt>(), 64);
        assert_eq!(size_of::<ffi::NvTiming>(), 96);
        assert_eq!(offset_of!(ffi::NvTiming, etc), 32);
        assert_eq!(size_of::<NvDisplayConfigPathAdvancedTargetInfoV1>(), 128);
        assert_eq!(NvDisplayConfigPathAdvancedTargetInfoV1::VER, 0x0001_0080);
        assert_eq!(
            offset_of!(NvDisplayConfigPathAdvancedTargetInfoV1, timing),
            32
        );
        assert_eq!(size_of::<NvDisplayConfigPathTargetInfoV2>(), 24);
        assert_eq!(offset_of!(NvDisplayConfigPathTargetInfoV2, details), 8);
        assert_eq!(offset_of!(NvDisplayConfigPathTargetInfoV2, target_id), 16);
        assert_eq!(size_of::<NvDisplayConfigSourceModeInfoV1>(), 32);
        assert_eq!(offset_of!(NvDisplayConfigSourceModeInfoV1, position), 16);
        assert_eq!(size_of::<NvDisplayConfigPathInfoV2>(), 48);
        assert_eq!(offset_of!(NvDisplayConfigPathInfoV2, source_mode_info), 24);
        assert_eq!(offset_of!(NvDisplayConfigPathInfoV2, os_adapter_id), 40);
        assert_eq!(NvDisplayConfigPathInfoV2::VER, 0x0002_0030);
        assert_eq!(size_of::<NvGetVrrInfoV1>(), 24);
        assert_eq!(NvGetVrrInfoV1::VER, 0x0001_0018);
    }

    const VRR_LIGADO: VrrBruto = VrrBruto {
        possivel: true,
        ativo: true,
    };

    #[test]
    fn monitor_principal_nvidia() {
        let info =
            ler_exibicao_principal(&FonteFalsa::monitor(0x8000_1000, 239_760, Some(VRR_LIGADO)));
        assert_eq!(
            info,
            InfoExibicao {
                display_id: Some(0x8000_1000),
                refresh_mhz: Some(239_760),
                vrr_possivel: Some(true),
                vrr_ativo: Some(true),
            }
        );
    }

    #[test]
    fn sem_nvapi_tudo_none() {
        assert_eq!(
            ler_exibicao_principal(&FonteFalsa::sem_nvapi()),
            InfoExibicao::default()
        );
    }

    #[test]
    fn falha_de_vrr_mantem_refresh() {
        let info = ler_exibicao_principal(&FonteFalsa::monitor(7, 144_000, None));
        assert_eq!(info.refresh_mhz, Some(144_000));
        assert_eq!(info.vrr_possivel, None);
        assert_eq!(info.vrr_ativo, None);
    }

    #[test]
    fn refresh_zero_vira_none() {
        let info = ler_exibicao_principal(&FonteFalsa::monitor(7, 0, Some(VRR_LIGADO)));
        assert_eq!(info.refresh_mhz, None);
        assert_eq!(info.vrr_possivel, Some(true));
    }

    #[test]
    fn principal_fora_da_nvidia() {
        let mut f = FonteFalsa::monitor(7, 60_000, Some(VRR_LIGADO));
        f.caminhos.as_mut().unwrap()[0].adaptador_nvidia = false;
        assert_eq!(ler_exibicao_principal(&f), InfoExibicao::default());
    }

    #[test]
    fn escolhe_o_caminho_gdi_primario_e_o_alvo_primario_do_clone() {
        let alvo = |display_id, refresh, primario| AlvoExibicao {
            display_id,
            refresh_mhz: Some(refresh),
            primario_clone: primario,
        };
        let f = FonteFalsa {
            caminhos: Some(vec![
                CaminhoExibicao {
                    gdi_primario: false,
                    adaptador_nvidia: true,
                    alvos: vec![alvo(1, 60_000, true)],
                },
                CaminhoExibicao {
                    gdi_primario: true,
                    adaptador_nvidia: true,
                    alvos: vec![alvo(2, 120_000, false), alvo(3, 240_000, true)],
                },
            ]),
            vrr: vec![(
                3,
                VrrBruto {
                    possivel: true,
                    ativo: false,
                },
            )],
        };
        let info = ler_exibicao_principal(&f);
        assert_eq!(info.display_id, Some(3));
        assert_eq!(info.refresh_mhz, Some(240_000));
        assert_eq!(info.vrr_possivel, Some(true));
        assert_eq!(info.vrr_ativo, Some(false));
    }

    #[test]
    fn sem_primario_ou_sem_alvos() {
        let mut f = FonteFalsa::monitor(7, 60_000, None);
        f.caminhos.as_mut().unwrap()[0].gdi_primario = false;
        assert_eq!(ler_exibicao_principal(&f), InfoExibicao::default());

        let mut f = FonteFalsa::monitor(7, 60_000, None);
        f.caminhos.as_mut().unwrap()[0].alvos.clear();
        assert_eq!(ler_exibicao_principal(&f), InfoExibicao::default());

        assert_eq!(
            ler_exibicao_principal(&FonteFalsa::monitor(0, 60_000, None)),
            InfoExibicao::default()
        );
    }
}
