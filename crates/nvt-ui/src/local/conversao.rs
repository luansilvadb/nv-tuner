//! Conversões do motor para o protocolo (nomes, valores, impacto, preferências e itens).

use super::*;

pub(super) fn alertas_deriva<S: DriverStore>(engine: &Engine<S>) -> Vec<AlertaDeriva> {
    let Ok(deriva) = engine.verificar_deriva() else {
        return Vec::new();
    };
    let agora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    deriva
        .divergencias
        .iter()
        .map(|d| AlertaDeriva {
            perfil: perfil_de_alvo(&d.esperado.alvo),
            setting: d.esperado.setting.clone(),
            nome_legivel: nome_legivel(engine, &d.esperado.setting),
            esperado: valor(engine, &d.esperado.setting, Some(d.esperado.valor))
                .unwrap_or_default(),
            encontrado: valor(engine, &d.esperado.setting, d.atual),
            detectado_unix: agora,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Conversões
// ---------------------------------------------------------------------------

pub(super) fn perfil_de_alvo(a: &Alvo) -> String {
    match a {
        Alvo::Global => "global".into(),
        Alvo::Jogo(id) => id.clone(),
    }
}

pub(super) fn nome_do_alvo(a: &Alvo) -> String {
    match a {
        Alvo::Global => textos::PERFIL_GLOBAL.into(),
        Alvo::Jogo(id) => id.clone(),
    }
}

pub(super) fn nome_legivel<S: DriverStore>(e: &Engine<S>, setting: &str) -> String {
    if let Some(n) = textos::nome_setting(setting) {
        return n.into();
    }
    e.regras()
        .catalogo
        .get(setting)
        .map(|d| {
            d.descricao
                .split(" (")
                .next()
                .unwrap_or(&d.descricao)
                .to_string()
        })
        .unwrap_or_else(|| setting.to_string())
}

/// Valor legível; `None` = não definido.
pub(super) fn valor<S: DriverStore>(
    e: &Engine<S>,
    setting: &str,
    v: Option<u32>,
) -> Option<String> {
    let v = v?;
    let def = e.regras().catalogo.get(setting);
    if setting == "limite_fps" {
        return Some(if v == 0 {
            textos::nome_valor("off")
        } else {
            format!("{v} FPS")
        });
    }
    Some(match def.and_then(|d| d.nome_do_valor(v)) {
        Some(n) => textos::nome_valor(n),
        None => format!("{v} (0x{v:X})"),
    })
}

pub fn impacto_ipc(i: ImpactoRegra) -> ImpactoVisual {
    match i {
        ImpactoRegra::Nenhum => ImpactoVisual::Nenhum,
        ImpactoRegra::Leve => ImpactoVisual::Baixo,
        ImpactoRegra::Forte => ImpactoVisual::Alto,
    }
}

pub fn preferencias_ipc(p: &nvt_core::Preferences) -> Preferencias {
    Preferencias {
        sincronismo: match p.sincronismo {
            nvt_core::Sincronismo::SemSync => Sincronismo::Desligado,
            nvt_core::Sincronismo::GSync => Sincronismo::GSync,
            nvt_core::Sincronismo::VSync => Sincronismo::VSync,
        },
        qualidade: match p.qualidade {
            nvt_core::Qualidade::Desempenho => Qualidade::Desempenho,
            nvt_core::Qualidade::Equilibrado => Qualidade::Equilibrado,
            nvt_core::Qualidade::Fidelidade => Qualidade::Qualidade,
        },
        limite_fps: p.limite_fps,
    }
}

pub fn acao_de_origem(o: &OrigemRegistro) -> AcaoHistorico {
    match o {
        OrigemRegistro::Plano { .. } | OrigemRegistro::CriacaoPerfil => AcaoHistorico::Aplicacao,
        OrigemRegistro::Reaplicacao { .. } => AcaoHistorico::Rollback,
        OrigemRegistro::Usuario => AcaoHistorico::DerivaDetectada,
        OrigemRegistro::Restauracao => AcaoHistorico::Restauracao,
    }
}

pub(super) fn item<S: DriverStore>(
    e: &Engine<S>,
    m: &nvt_core::SettingChange,
    status: StatusMudanca,
    experimental: bool,
) -> ItemMudanca {
    let regra_id = m.origem.regra().unwrap_or("");
    let regra = e.regras().regras.iter().find(|r| r.id == regra_id);
    ItemMudanca {
        setting: m.setting.clone(),
        nome_legivel: nome_legivel(e, &m.setting),
        antes: valor(e, &m.setting, m.valor_anterior),
        depois: valor(e, &m.setting, Some(m.valor)),
        motivo: m.explicacao.clone(),
        regra: match &m.origem {
            Origem::Regra { id } => id.clone(),
            Origem::Preferencia {
                preferencia,
                regra: None,
            } => format!("preferência: {preferencia}"),
            Origem::Preferencia { regra: Some(r), .. } => r.clone(),
        },
        impacto: regra.map_or(ImpactoVisual::Nenhum, |r| impacto_ipc(r.impacto_visual)),
        status,
        camada: nvt_ipc::Camada::Driver,
        experimental: if experimental {
            nvt_ipc::EstadoExperimental::Aceitavel
        } else {
            nvt_ipc::EstadoExperimental::Estavel
        },
    }
}
