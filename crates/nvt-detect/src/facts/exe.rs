//! Candidatos a exe principal (heurísticas).
//!
//! O cruzamento com o DRS (passo 2 do documento) é feito depois, na integração
//! com `nvt-drs`; aqui ficam o metadado do launcher e as heurísticas locais.

use std::path::{Path, PathBuf};

use super::Confianca;
use super::arquivos::{Arquivo, Listagem};

/// Por que um exe virou candidato.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MotivoExe {
    /// Informado pelo launcher (Epic `LaunchExecutable`, GOG `exe`, Xbox).
    Launcher,
    /// Unreal: `*-Win64-Shipping.exe` em `Binaries\Win64`.
    UnrealShipping,
    /// Unity: exe ao lado de `UnityPlayer.dll`.
    Unity,
    /// Maior exe fora das listas de exclusão.
    Tamanho,
    /// Maior exe com nome de cliente/launcher (último recurso).
    TamanhoNomeDeCliente,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidatoExe {
    pub caminho: PathBuf,
    pub tamanho: u64,
    pub motivo: MotivoExe,
    pub confianca: Confianca,
}

/// Trechos de nome que excluem o exe (launchers, crash reporters, instaladores, anticheat).
const NOMES_EXCLUIDOS: &[&str] = &[
    "crashreport",
    "crash",
    "crashpad_handler",
    "unitycrashhandler",
    "helper",
    "setup",
    "unins",
    "launcher",
    "vc_redist",
    "vcredist",
    "dxsetup",
    "dotnet",
    "epicwebhelper",
    "installer",
    "updater",
    "uninstall",
    "redist",
    "easyanticheat",
    "start_protected_game",
    "beservice",
    "battleye",
    "elytra",
    "uxrender",
    "cefprocess",
    "subprocess",
    "renderer",
    "benchmark",
    "touchup",
    "cleanup",
    "report",
];

/// Pastas cujo conteúdo nunca é o exe do jogo.
const PASTAS_EXCLUIDAS: &[&str] = &[
    "engine",
    "_commonredist",
    "commonredist",
    "redist",
    "redistributables",
    "thirdparty",
    "cef3",
    "cef",
    "directx",
    "vcredist",
    "installer",
    "__installer",
    "support",
    "easyanticheat",
    "battleye",
    "elytra",
];

/// Nome "de cliente": rebaixado, porque costuma ser o launcher/CEF do jogo.
fn nome_de_cliente(stem: &str) -> bool {
    stem.contains("client") || stem.ends_with("ux")
}

fn excluido(a: &Arquivo) -> bool {
    let stem = a.nome.trim_end_matches(".exe");
    NOMES_EXCLUIDOS.iter().any(|x| stem.contains(x))
        || a.pastas().any(|p| PASTAS_EXCLUIDAS.contains(&p))
}

fn e_unreal_shipping(a: &Arquivo) -> bool {
    let partes: Vec<&str> = a.rel.split('\\').collect();
    let n = partes.len();
    n >= 3
        && partes[n - 3] == "binaries"
        && partes[n - 2] == "win64"
        && a.nome.ends_with("-win64-shipping.exe")
        && !a.em_pasta("engine")
}

fn dir_rel(a: &Arquivo) -> &str {
    a.rel.rsplit_once('\\').map(|(d, _)| d).unwrap_or("")
}

/// Lista candidatos em ordem de preferência (o primeiro é o exe principal provável).
pub fn candidatos(listagem: &Listagem, exe_indicado: Option<&Path>) -> Vec<CandidatoExe> {
    candidatos_com_chave(listagem, exe_indicado, crate::model::pasta_canonica)
}

pub(super) fn candidatos_com_chave(
    listagem: &Listagem,
    exe_indicado: Option<&Path>,
    chave: impl Fn(&Path) -> String,
) -> Vec<CandidatoExe> {
    let exes: Vec<&Arquivo> = listagem
        .arquivos
        .iter()
        .filter(|a| a.nome.ends_with(".exe"))
        .collect();
    let mut out: Vec<CandidatoExe> = Vec::new();
    let mut push = |a: &Arquivo, motivo, confianca| {
        if !out.iter().any(|c| c.caminho == a.caminho) {
            out.push(CandidatoExe {
                caminho: a.caminho.clone(),
                tamanho: a.tamanho,
                motivo,
                confianca,
            });
        }
    };

    if let Some(ind) = exe_indicado {
        let alvo = chave(ind);
        if let Some(a) = exes.iter().find(|a| chave(&a.caminho) == alvo) {
            push(a, MotivoExe::Launcher, Confianca::Alta);
        }
    }

    let mut ue: Vec<&&Arquivo> = exes.iter().filter(|a| e_unreal_shipping(a)).collect();
    ue.sort_by_key(|a| std::cmp::Reverse(a.tamanho));
    for a in ue {
        push(a, MotivoExe::UnrealShipping, Confianca::Alta);
    }

    let unity_dirs: Vec<&str> = listagem
        .arquivos
        .iter()
        .filter(|a| a.nome == "unityplayer.dll")
        .map(dir_rel)
        .collect();
    for a in &exes {
        if !excluido(a) && unity_dirs.contains(&dir_rel(a)) {
            push(a, MotivoExe::Unity, Confianca::Alta);
        }
    }

    let mut resto: Vec<&&Arquivo> = exes.iter().filter(|a| !excluido(a)).collect();
    resto.sort_by_key(|a| std::cmp::Reverse(a.tamanho));
    let (clientes, normais): (Vec<_>, Vec<_>) = resto
        .into_iter()
        .partition(|a| nome_de_cliente(a.nome.trim_end_matches(".exe")));
    for a in normais {
        push(a, MotivoExe::Tamanho, Confianca::Media);
    }
    for a in clientes {
        push(a, MotivoExe::TamanhoNomeDeCliente, Confianca::Baixa);
    }
    out
}

#[cfg(test)]
mod testes {
    use super::*;

    fn arq(rel: &str, tamanho: u64) -> Arquivo {
        let rel = rel.to_lowercase();
        Arquivo {
            caminho: PathBuf::from(format!(r"X:\J\{rel}")),
            nome: rel.rsplit('\\').next().unwrap().to_string(),
            rel,
            tamanho,
        }
    }

    fn lst(a: Vec<Arquivo>) -> Listagem {
        Listagem {
            arquivos: a,
            ..Default::default()
        }
    }

    #[test]
    fn unreal_como_wardogs() {
        let l = lst(vec![
            arq(
                r"Wardogs\Binaries\Win64\WardogsClient-Win64-Shipping.exe",
                229,
            ),
            arq("WardogsLauncher-Shipping.exe", 50),
            arq(r"Engine\Binaries\Win64\CrashReportClient.exe", 30),
            arq(r"Engine\Binaries\Win64\EpicWebHelper.exe", 5),
            arq(r"Elytra\Elytra-Setup.exe", 10),
            arq(
                r"Engine\Binaries\ThirdParty\CEF3\Win64\crashpad_handler.exe",
                1,
            ),
        ]);
        let c = candidatos(&l, None);
        assert_eq!(c[0].motivo, MotivoExe::UnrealShipping);
        assert!(c[0].caminho.ends_with("wardogsclient-win64-shipping.exe"));
        assert!(
            c.iter()
                .all(|x| !x.caminho.to_string_lossy().contains("crash"))
        );
        assert!(
            c.iter()
                .all(|x| !x.caminho.to_string_lossy().contains("launcher"))
        );
    }

    #[test]
    fn lol_prefere_jogo_ao_cliente() {
        let l = lst(vec![
            arq(r"Game\League of Legends.exe", 44),
            arq(r"Game\LeagueCrashHandler64.exe", 2),
            arq("LeagueClient.exe", 30),
            arq("LeagueClientUx.exe", 9),
            arq("LeagueClientUxRender.exe", 2),
            arq("LeagueCrashHandler64.exe", 2),
        ]);
        let c = candidatos(&l, None);
        assert!(c[0].caminho.ends_with("league of legends.exe"));
        assert_eq!(c[0].motivo, MotivoExe::Tamanho);
        assert!(
            c.iter()
                .all(|x| !x.caminho.to_string_lossy().contains("uxrender"))
        );
    }

    #[test]
    fn unity_e_indicado() {
        let l = lst(vec![
            arq("Jogo.exe", 1),
            arq("UnityPlayer.dll", 30),
            arq("UnityCrashHandler64.exe", 2),
            arq(r"Outro\Grande.exe", 100),
        ]);
        let c = candidatos(&l, None);
        assert_eq!(c[0].motivo, MotivoExe::Unity);
        let c = candidatos(&l, Some(Path::new(r"X:\J\outro\grande.exe")));
        assert_eq!(c[0].motivo, MotivoExe::Launcher);
    }
}
