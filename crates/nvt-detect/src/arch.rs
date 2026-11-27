//! Capacidades da GPU a partir da arquitetura informada pela NVAPI.
//!
//! A tabela vem de `data/capacidades-gpu.toml`.
//! A função [`arch_facts`] é pura: recebe os números de
//! `NvAPI_GPU_GetArchInfo` e o nome de `NvAPI_GPU_GetFullName`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// Conteúdo embutido da tabela de arquiteturas.
pub const CAPACIDADES_TOML: &str = include_str!("../data/capacidades-gpu.toml");

/// `NV_GPU_ARCHITECTURE_TU100`.
pub const ARCH_TURING: u32 = 0x160;
/// `NV_GPU_ARCHITECTURE_GB200` (GeForce RTX 50), a mais nova conhecida.
pub const ARCH_BLACKWELL: u32 = 0x1B0;
/// Implementações Turing sem Tensor Cores (GTX 16xx).
pub const IMPL_TU116: u32 = 0x08;
/// Implementação TU117 (GTX 1650), também sem Tensor Cores.
pub const IMPL_TU117: u32 = 0x07;

/// Família de arquitetura reconhecida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FamiliaGpu {
    Turing,
    Ampere,
    AdaLovelace,
    Blackwell,
    /// Arquitetura mais nova que a Blackwell: herda dela de forma conservadora.
    Futura,
    /// Pré-RTX, datacenter ou ID desconhecido entre os conhecidos: "sem DLSS".
    ForaDeEscopo,
}

/// Custo relativo do modelo transformer do DLSS (presets J/K).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustoTransformer {
    Alto,
    Baixo,
    Desconhecido,
}

/// Fatos de hardware derivados da arquitetura.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuArchFacts {
    pub familia: FamiliaGpu,
    /// Nome legível da família ("Turing", "Ada Lovelace"...).
    pub nome_familia: String,
    pub arch_id: u32,
    pub impl_id: u32,
    /// Nome do chip quando a implementação consta da tabela (ex.: "TU106").
    pub chip: Option<String>,
    pub tensor_cores: bool,
    pub tensor_geracao: Option<u8>,
    pub tensor_fp8: bool,
    pub dlss_sr: bool,
    pub dlss_rr: bool,
    /// DLSS Frame Generation (2x).
    pub frame_gen: bool,
    /// DLSS Multi Frame Generation (3x/4x).
    pub mfg: bool,
    /// Resizable BAR suportado oficialmente (estar ATIVO é outro fato, de runtime).
    pub rebar_suportado: bool,
    pub transformer_custo: CustoTransformer,
    /// Verdadeiro quando os fatos foram inferidos (arquitetura futura).
    pub avisar_usuario: bool,
    pub observacao: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TabelaToml {
    // Obrigatório no TOML; lido só pelos testes.
    #[cfg_attr(not(test), allow(dead_code))]
    pub versao_schema: u32,
    pub arquitetura: Vec<ArquiteturaToml>,
    pub fora_de_escopo: ForaDeEscopoToml,
    pub desconhecida: DesconhecidaToml,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ArquiteturaToml {
    pub nome: String,
    pub nv_gpu_architecture_id: u32,
    pub implementacoes: BTreeMap<String, u32>,
    pub tensor_geracao: u8,
    pub tensor_fp8: bool,
    pub dlss_sr: bool,
    pub dlss_rr: bool,
    pub dlss_fg: bool,
    pub dlss_mfg: bool,
    pub rebar_oficial: bool,
    pub dlss_transformer_custo: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ForaDeEscopoToml {
    pub ids: Vec<u32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DesconhecidaToml {
    pub herdar_de: String,
    pub sobrescrever: BTreeMap<String, bool>,
    pub avisar_usuario: bool,
}

pub(crate) fn tabela() -> &'static TabelaToml {
    static TABELA: OnceLock<TabelaToml> = OnceLock::new();
    TABELA.get_or_init(|| {
        // O arquivo é embutido no binário e validado pelos testes; falhar aqui
        // é erro de build, não de runtime.
        toml::from_str(CAPACIDADES_TOML).expect("data/capacidades-gpu.toml inválido")
    })
}

fn familia_por_nome(nome: &str) -> FamiliaGpu {
    match nome {
        "Turing" => FamiliaGpu::Turing,
        "Ampere" => FamiliaGpu::Ampere,
        "Ada Lovelace" => FamiliaGpu::AdaLovelace,
        "Blackwell" => FamiliaGpu::Blackwell,
        _ => FamiliaGpu::ForaDeEscopo,
    }
}

fn custo(s: &str) -> CustoTransformer {
    match s {
        "alto" => CustoTransformer::Alto,
        "baixo" => CustoTransformer::Baixo,
        _ => CustoTransformer::Desconhecido,
    }
}

fn de_entrada(a: &ArquiteturaToml, arch_id: u32, impl_id: u32) -> GpuArchFacts {
    let chip = a
        .implementacoes
        .iter()
        .find(|(_, v)| **v == impl_id)
        .map(|(k, _)| k.clone());
    GpuArchFacts {
        familia: familia_por_nome(&a.nome),
        nome_familia: a.nome.clone(),
        arch_id,
        impl_id,
        chip,
        tensor_cores: a.tensor_geracao > 0,
        tensor_geracao: Some(a.tensor_geracao),
        tensor_fp8: a.tensor_fp8,
        dlss_sr: a.dlss_sr,
        dlss_rr: a.dlss_rr,
        frame_gen: a.dlss_fg,
        mfg: a.dlss_mfg,
        rebar_suportado: a.rebar_oficial,
        transformer_custo: custo(&a.dlss_transformer_custo),
        avisar_usuario: false,
        observacao: None,
    }
}

fn fora_de_escopo(arch_id: u32, impl_id: u32, motivo: &str) -> GpuArchFacts {
    GpuArchFacts {
        familia: FamiliaGpu::ForaDeEscopo,
        nome_familia: "Fora de escopo".to_string(),
        arch_id,
        impl_id,
        chip: None,
        tensor_cores: false,
        tensor_geracao: None,
        tensor_fp8: false,
        dlss_sr: false,
        dlss_rr: false,
        frame_gen: false,
        mfg: false,
        rebar_suportado: false,
        transformer_custo: CustoTransformer::Desconhecido,
        avisar_usuario: false,
        observacao: Some(motivo.to_string()),
    }
}

/// Indica se o nome comercial é de uma GTX (sem Tensor Cores).
fn nome_indica_gtx(gpu_name: &str) -> bool {
    gpu_name.to_ascii_uppercase().contains("GTX")
}

/// Converte arquitetura/implementação/nome em fatos de hardware.
///
/// Regras:
/// - `0x160` com TU116/TU117 ou nome "GTX": Turing sem Tensor Cores, sem DLSS;
/// - `> 0x1B0`: herda da Blackwell, mas `mfg = false` e `avisar_usuario = true`;
/// - IDs fora da tabela (pré-RTX, datacenter, buracos): "sem DLSS".
pub fn arch_facts(arch_id: u32, impl_id: u32, gpu_name: &str) -> GpuArchFacts {
    let t = tabela();

    if let Some(a) = t
        .arquitetura
        .iter()
        .find(|a| a.nv_gpu_architecture_id == arch_id)
    {
        let mut f = de_entrada(a, arch_id, impl_id);
        let sem_tensor_por_impl = impl_id == IMPL_TU116 || impl_id == IMPL_TU117;
        if arch_id == ARCH_TURING && (sem_tensor_por_impl || nome_indica_gtx(gpu_name)) {
            f.tensor_cores = false;
            f.tensor_geracao = None;
            f.tensor_fp8 = false;
            f.dlss_sr = false;
            f.dlss_rr = false;
            f.frame_gen = false;
            f.mfg = false;
            f.transformer_custo = CustoTransformer::Desconhecido;
            f.observacao = Some(
                "GTX 16 (Turing TU116/TU117): sem Tensor Cores, DLSS indisponível".to_string(),
            );
        }
        return f;
    }

    if t.fora_de_escopo.ids.contains(&arch_id) {
        return fora_de_escopo(arch_id, impl_id, "arquitetura pré-RTX ou de datacenter");
    }

    if arch_id > ARCH_BLACKWELL {
        let base = t
            .arquitetura
            .iter()
            .find(|a| a.nome == t.desconhecida.herdar_de)
            .expect("herdar_de aponta para arquitetura da tabela");
        let mut f = de_entrada(base, arch_id, impl_id);
        f.familia = FamiliaGpu::Futura;
        f.nome_familia = format!("Futura (herdada de {})", base.nome);
        f.chip = None;
        f.tensor_geracao = None;
        if let Some(mfg) = t.desconhecida.sobrescrever.get("dlss_mfg") {
            f.mfg = *mfg;
        }
        f.avisar_usuario = t.desconhecida.avisar_usuario;
        f.observacao = Some(
            "arquitetura mais nova que a Blackwell: fatos herdados; MFG só após confirmação em runtime"
                .to_string(),
        );
        return f;
    }

    fora_de_escopo(arch_id, impl_id, "ID de arquitetura desconhecido")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn tabela_carrega() {
        let t = tabela();
        assert_eq!(t.versao_schema, 1);
        assert_eq!(t.arquitetura.len(), 4);
    }

    #[test]
    fn rtx_2060_super() {
        let f = arch_facts(0x160, 0x06, "NVIDIA GeForce RTX 2060 SUPER");
        assert_eq!(f.familia, FamiliaGpu::Turing);
        assert_eq!(f.chip.as_deref(), Some("TU106"));
        assert!(f.tensor_cores);
        assert_eq!(f.tensor_geracao, Some(2));
        assert!(!f.tensor_fp8 && !f.frame_gen && !f.mfg && !f.rebar_suportado);
        assert!(f.dlss_sr && f.dlss_rr);
        assert_eq!(f.transformer_custo, CustoTransformer::Alto);
    }

    #[test]
    fn gtx_16_por_implementacao_e_por_nome() {
        for (imp, nome) in [
            (IMPL_TU116, "NVIDIA GeForce GTX 1660"),
            (IMPL_TU117, "NVIDIA GeForce GTX 1650"),
            (0x06, "NVIDIA GeForce GTX 1650"), // TU106 em GTX 1650 (variante rara)
            (IMPL_TU116, "?"),
        ] {
            let f = arch_facts(0x160, imp, nome);
            assert_eq!(f.familia, FamiliaGpu::Turing);
            assert!(!f.tensor_cores, "{nome}");
            assert!(!f.dlss_sr && !f.dlss_rr && !f.frame_gen && !f.mfg);
        }
    }

    #[test]
    fn ampere_ada_blackwell() {
        let a = arch_facts(0x170, 0x04, "NVIDIA GeForce RTX 3070");
        assert_eq!(a.familia, FamiliaGpu::Ampere);
        assert!(a.rebar_suportado && !a.frame_gen && !a.tensor_fp8);
        // GA107 = 0x07 não pode ser confundido com TU117.
        let a7 = arch_facts(0x170, 0x07, "NVIDIA GeForce RTX 3050");
        assert!(a7.tensor_cores && a7.dlss_sr);

        let d = arch_facts(0x190, 0x04, "NVIDIA GeForce RTX 4070");
        assert_eq!(d.familia, FamiliaGpu::AdaLovelace);
        assert!(d.tensor_fp8 && d.frame_gen && !d.mfg);

        let b = arch_facts(0x1B0, 0x02, "NVIDIA GeForce RTX 5090");
        assert_eq!(b.familia, FamiliaGpu::Blackwell);
        assert!(b.tensor_fp8 && b.frame_gen && b.mfg && b.rebar_suportado);
        assert_eq!(b.transformer_custo, CustoTransformer::Baixo);
    }

    #[test]
    fn futura_herda_sem_mfg() {
        let f = arch_facts(0x1C0, 0x02, "NVIDIA GeForce RTX 6090");
        assert_eq!(f.familia, FamiliaGpu::Futura);
        assert!(f.tensor_fp8 && f.dlss_sr && f.dlss_rr && f.frame_gen && f.rebar_suportado);
        assert!(!f.mfg);
        assert!(f.avisar_usuario);
    }

    #[test]
    fn fora_de_escopo_e_buracos() {
        for id in [0x130, 0x140, 0x150, 0x180, 0x1A0, 0x175, 0x0, 0x100] {
            let f = arch_facts(id, 0, "NVIDIA");
            assert_eq!(f.familia, FamiliaGpu::ForaDeEscopo, "{id:#x}");
            assert!(!f.dlss_sr && !f.tensor_cores && !f.frame_gen);
        }
    }
}
