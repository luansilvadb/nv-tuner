//! Scanners de bibliotecas de jogos.
//!
//! Cada scanner é independente: erro ou pânico em um vira [`FalhaScanner`] no
//! relatório e não impede os demais.

pub mod epic;
pub mod generico;
pub mod por_registro;
pub mod riot;
pub mod steam;
pub mod xbox;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use crate::model::{DetectedGame, Launcher, deduplicar};
use crate::registro;

/// `%ProgramData%` (padrão `C:\ProgramData`).
pub fn program_data() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
}

/// Falha de um scanner específico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FalhaScanner {
    pub launcher: Launcher,
    pub erro: String,
}

/// Resultado da varredura de todas as bibliotecas.
#[derive(Debug, Clone, Default)]
pub struct RelatorioVarredura {
    /// Jogos deduplicados por pasta canônica.
    pub jogos: Vec<DetectedGame>,
    pub falhas: Vec<FalhaScanner>,
}

type Scanner<'a> = Box<dyn FnOnce() -> Result<Vec<DetectedGame>, String> + Send + 'a>;

/// Roda um scanner isolando pânico; o erro vira texto.
fn rodar(
    f: impl FnOnce() -> Result<Vec<DetectedGame>, String>,
) -> Result<Vec<DetectedGame>, String> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| Err("pânico no scanner".into()))
}

fn acumular(
    launcher: Launcher,
    r: Result<Vec<DetectedGame>, String>,
    jogos: &mut Vec<DetectedGame>,
    falhas: &mut Vec<FalhaScanner>,
) {
    match r {
        Ok(v) => jogos.extend(v),
        Err(erro) => falhas.push(FalhaScanner { launcher, erro }),
    }
}

#[cfg(test)]
fn executar(
    launcher: Launcher,
    f: impl FnOnce() -> Result<Vec<DetectedGame>, String>,
    jogos: &mut Vec<DetectedGame>,
    falhas: &mut Vec<FalhaScanner>,
) {
    acumular(launcher, rodar(f), jogos, falhas);
}

/// Executa todos os scanners em paralelo (ordem da lista = prioridade na
/// deduplicação; os resultados são juntados nessa ordem).
pub fn varrer_tudo() -> RelatorioVarredura {
    let mut jogos = Vec::new();
    let mut falhas = Vec::new();
    let entradas = catch_unwind(registro::entradas_desinstalacao).unwrap_or_default();
    let entradas = &entradas;

    let scanners: Vec<(Launcher, Scanner)> = vec![
        (Launcher::Steam, Box::new(steam::varrer)),
        (Launcher::Riot, Box::new(riot::varrer)),
        (Launcher::Epic, Box::new(epic::varrer)),
        (Launcher::Gog, Box::new(por_registro::gog)),
        (Launcher::Ubisoft, Box::new(por_registro::ubisoft)),
        (Launcher::Ea, Box::new(move || por_registro::ea(entradas))),
        (
            Launcher::BattleNet,
            Box::new(move || por_registro::battlenet(entradas)),
        ),
        (Launcher::Xbox, Box::new(xbox::varrer)),
        (
            Launcher::Generico,
            Box::new(move || generico::varrer(entradas)),
        ),
    ];

    std::thread::scope(|s| {
        let tarefas: Vec<_> = scanners
            .into_iter()
            .map(|(l, f)| (l, s.spawn(move || rodar(f))))
            .collect();
        for (l, t) in tarefas {
            let r = t.join().unwrap_or_else(|_| Err("pânico no scanner".into()));
            acumular(l, r, &mut jogos, &mut falhas);
        }
    });

    // Pastas que não existem mais (desinstalação incompleta) são descartadas.
    jogos.retain(|j| j.pasta.is_dir());
    RelatorioVarredura {
        jogos: deduplicar(jogos),
        falhas,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn falha_e_panico_nao_derrubam_os_outros() {
        let mut jogos = Vec::new();
        let mut falhas = Vec::new();
        executar(Launcher::Epic, || Err("x".into()), &mut jogos, &mut falhas);
        executar(Launcher::Gog, || panic!("teste"), &mut jogos, &mut falhas);
        executar(
            Launcher::Steam,
            || Ok(vec![DetectedGame::new(Launcher::Steam, "A", "X:/A")]),
            &mut jogos,
            &mut falhas,
        );
        assert_eq!(jogos.len(), 1);
        assert_eq!(falhas.len(), 2);
    }
}
