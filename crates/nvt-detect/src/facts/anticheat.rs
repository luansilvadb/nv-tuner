//! Anticheat a partir de `data/anticheats.toml` (dados, não código).

use std::sync::OnceLock;

use serde::Deserialize;

use super::arquivos::Listagem;
use super::{Confianca, pe};
use crate::registro::ServicosSistema;

pub const ANTICHEATS_TOML: &str = include_str!("../../data/anticheats.toml");

#[derive(Debug, Deserialize)]
struct Tabela {
    #[allow(dead_code)]
    versao_schema: u32,
    anticheat: Vec<Entrada>,
}

#[derive(Debug, Deserialize)]
struct DescricaoDll {
    arquivo: String,
    contem: String,
}

#[derive(Debug, Deserialize)]
struct Entrada {
    nome: String,
    kernel: bool,
    #[serde(default)]
    pastas: Vec<String>,
    #[serde(default)]
    arquivos: Vec<String>,
    #[serde(default)]
    prefixos_arquivo: Vec<String>,
    #[serde(default)]
    sufixos_arquivo: Vec<String>,
    descricao_dll: Option<DescricaoDll>,
    #[serde(default)]
    metadata_riot: bool,
    #[serde(default)]
    servicos: Vec<String>,
    #[serde(default)]
    prefixos_servico: Vec<String>,
}

fn tabela() -> &'static Tabela {
    static T: OnceLock<Tabela> = OnceLock::new();
    T.get_or_init(|| toml::from_str(ANTICHEATS_TOML).expect("data/anticheats.toml inválido"))
}

/// Anticheat detectado para um jogo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FatoAnticheat {
    pub nome: String,
    /// Anticheat de kernel (tabela) ou confirmado por driver registrado (Type 1).
    pub kernel: bool,
    pub confianca: Confianca,
    pub evidencias: Vec<String>,
}

/// Procura anticheats na instalação, no metadado do launcher e nos serviços do sistema.
///
/// Serviço sozinho não é atribuído ao jogo (não diz qual jogo o instalou); ele só
/// reforça um indício encontrado na pasta ou no metadado.
pub fn detectar(
    listagem: &Listagem,
    exige_vanguard: bool,
    servicos: &ServicosSistema,
) -> Vec<FatoAnticheat> {
    let mut out = Vec::new();
    for e in &tabela().anticheat {
        let mut fortes = Vec::new();
        let mut medias = Vec::new();

        if e.metadata_riot && exige_vanguard {
            fortes.push("metadata Riot: dependencies.vanguard = true".to_string());
        }
        for p in &e.pastas {
            let p = p.to_lowercase();
            if let Some(d) = listagem
                .pastas
                .iter()
                .find(|d| d.rsplit('\\').next() == Some(p.as_str()))
            {
                medias.push(format!("pasta {d}"));
            }
        }
        for a in &listagem.arquivos {
            let hit = e.arquivos.iter().any(|x| x.eq_ignore_ascii_case(&a.nome))
                || e.prefixos_arquivo
                    .iter()
                    .any(|x| a.nome.starts_with(&x.to_lowercase()))
                || e.sufixos_arquivo
                    .iter()
                    .any(|x| a.nome.ends_with(&x.to_lowercase()));
            if hit {
                medias.push(format!("arquivo {}", a.rel));
            }
        }
        if let Some(dd) = &e.descricao_dll {
            for a in listagem
                .arquivos
                .iter()
                .filter(|a| a.nome.eq_ignore_ascii_case(&dd.arquivo))
            {
                if let Ok(v) = pe::versao(&a.caminho) {
                    let texto = format!(
                        "{} {}",
                        v.descricao.unwrap_or_default(),
                        v.produto.unwrap_or_default()
                    );
                    if texto.to_lowercase().contains(&dd.contem.to_lowercase()) {
                        fortes.push(format!("{} (VERSIONINFO: {})", a.rel, texto.trim()));
                    }
                }
            }
        }
        if fortes.is_empty() && medias.is_empty() {
            continue;
        }

        let mut sistema = Vec::new();
        let mut kernel_sistema = false;
        for s in &e.servicos {
            if let Some(sv) = servicos.exato(s) {
                kernel_sistema |= sv.e_kernel();
                sistema.push(format!("serviço {}", sv.nome));
            }
        }
        for p in &e.prefixos_servico {
            let drivers: Vec<_> = servicos.com_prefixo(p).collect();
            if !drivers.is_empty() {
                kernel_sistema |= drivers.iter().any(|d| d.e_kernel());
                sistema.push(format!("{} drivers {p}*", drivers.len()));
            }
        }

        let confianca = if !fortes.is_empty() || !sistema.is_empty() {
            Confianca::Alta
        } else {
            Confianca::Media
        };
        let mut evidencias = fortes;
        evidencias.extend(medias.into_iter().take(5));
        evidencias.extend(sistema);
        out.push(FatoAnticheat {
            nome: e.nome.clone(),
            kernel: e.kernel || kernel_sistema,
            confianca,
            evidencias,
        });
    }
    out
}

#[cfg(test)]
mod testes {
    use super::super::arquivos::Arquivo;
    use super::*;
    use crate::registro::Servico;

    #[test]
    fn tabela_carrega() {
        assert!(tabela().anticheat.len() >= 4);
    }

    fn lst(pastas: &[&str], arquivos: &[&str]) -> Listagem {
        Listagem {
            pastas: pastas.iter().map(|s| s.to_lowercase()).collect(),
            arquivos: arquivos
                .iter()
                .map(|r| {
                    let rel = r.to_lowercase();
                    Arquivo {
                        caminho: format!(r"X:\inexistente\{rel}").into(),
                        nome: rel.rsplit('\\').next().unwrap().to_string(),
                        rel,
                        tamanho: 1,
                    }
                })
                .collect(),
            truncada: false,
        }
    }

    #[test]
    fn vanguard_so_por_metadata() {
        let sv = ServicosSistema {
            servicos: vec![Servico {
                nome: "vgk".into(),
                tipo: Some(1),
            }],
        };
        // Sem metadata: serviço sozinho não atribui Vanguard ao jogo.
        assert!(detectar(&lst(&[], &[]), false, &sv).is_empty());
        let r = detectar(&lst(&[], &[]), true, &sv);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].nome, "Riot Vanguard");
        assert_eq!(r[0].confianca, Confianca::Alta);
    }

    #[test]
    fn eac_e_battleye_por_pasta() {
        let sv = ServicosSistema::default();
        let r = detectar(
            &lst(
                &["EasyAntiCheat"],
                &[r"EasyAntiCheat\EasyAntiCheat_EOS_Setup.exe"],
            ),
            false,
            &sv,
        );
        assert_eq!(r[0].nome, "Easy Anti-Cheat");
        assert_eq!(r[0].confianca, Confianca::Media);
        let r = detectar(&lst(&[], &["Jogo_BE.exe"]), false, &sv);
        assert_eq!(r[0].nome, "BattlEye");
    }

    #[test]
    fn elytra_com_drivers() {
        let sv = ServicosSistema {
            servicos: vec![
                Servico {
                    nome: "elytra_ab12".into(),
                    tipo: Some(1),
                },
                Servico {
                    nome: "Elytra.Service".into(),
                    tipo: Some(16),
                },
            ],
        };
        let r = detectar(&lst(&["Elytra"], &[r"Elytra\Elytra-Setup.exe"]), false, &sv);
        assert_eq!(r[0].nome, "Elytra");
        assert!(r[0].kernel);
        assert_eq!(r[0].confianca, Confianca::Alta);
    }
}
