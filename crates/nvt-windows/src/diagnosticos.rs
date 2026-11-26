//! Coleta sincrona de diagnosticos. Execute consultas de maquina fora do pipe.

mod sistema;

pub use nvt_ipc::{Diagnostico, EstadoDiagnostico};
pub use sistema::DiagnosticosWindows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsultaSessao<'a> {
    GameMode,
    Vrr,
    Fso(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsultaMaquina {
    Hags,
    Mpo,
    PlanoEnergia,
    CpuX3d,
    MsiGpu,

    Tpm,
    SecureBoot,
    WindowsBuild,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeviceGuard {
    pub vbs: Option<u32>,
    pub hvci: Option<bool>,
    pub iommu: Option<bool>,
}

/// Capacidade de leitura, separada do armazem interativo.
///
/// ```compile_fail
/// use nvt_windows::diagnosticos::FonteDiagnosticos;
/// fn tentar_gravar(fonte: &impl FonteDiagnosticos) { fonte.gravar(); }
/// ```
pub trait FonteDiagnosticos {
    fn ler_sessao(&self, consulta: ConsultaSessao<'_>) -> Option<String>;
    fn ler_maquina(&self, consulta: ConsultaMaquina) -> Option<String>;
    fn ler_device_guard(&self) -> DeviceGuard;
}

pub fn diagnosticos_sessao(
    fonte: &impl FonteDiagnosticos,
    executaveis_conhecidos: &[&str],
) -> Vec<Diagnostico> {
    use ConsultaSessao::*;
    let mut itens = vec![
        diagnostico("game_mode_desligado", fonte.ler_sessao(GameMode)),
        diagnostico("vrr_windows", fonte.ler_sessao(Vrr)),
        diagnostico("drr", None),
        orientacao("polling_mouse"),
    ];
    itens.extend(
        executaveis_conhecidos
            .iter()
            .map(|exe| diagnostico("fso_por_jogo", fonte.ler_sessao(Fso(exe)))),
    );
    itens
}

pub fn diagnosticos_maquina(fonte: &impl FonteDiagnosticos) -> Vec<Diagnostico> {
    use ConsultaMaquina::*;
    let mut itens: Vec<_> = [
        ("hags", Hags),
        ("mpo", Mpo),
        ("plano_energia", PlanoEnergia),
        ("cpu_x3d_dual_ccd", CpuX3d),
        ("msi_gpu", MsiGpu),
        ("tpm", Tpm),
        ("secure_boot", SecureBoot),
        ("windows_11", WindowsBuild),
    ]
    .into_iter()
    .map(|(codigo, consulta)| diagnostico(codigo, fonte.ler_maquina(consulta)))
    .collect();
    let DeviceGuard { vbs, hvci, iommu } = fonte.ler_device_guard();
    itens.push(diagnostico(
        "vbs_hvci",
        vbs.zip(hvci)
            .map(|(vbs, hvci)| format!("{vbs},{}", u8::from(hvci))),
    ));
    itens.push(diagnostico("iommu", iommu.map(|v| u8::from(v).to_string())));
    itens.push(orientacao("useplatformclock"));
    itens
}

fn diagnostico(codigo: &str, valor: Option<String>) -> Diagnostico {
    Diagnostico {
        codigo: codigo.into(),
        estado: valor
            .filter(|valor| {
                !valor.is_empty() && valor.len() <= 1024 && !valor.chars().any(char::is_control)
            })
            .map_or(EstadoDiagnostico::Indisponivel, |valor| {
                EstadoDiagnostico::Conhecido { valor }
            }),
    }
}

fn orientacao(codigo: &str) -> Diagnostico {
    Diagnostico {
        codigo: codigo.into(),
        estado: EstadoDiagnostico::Orientacao,
    }
}
