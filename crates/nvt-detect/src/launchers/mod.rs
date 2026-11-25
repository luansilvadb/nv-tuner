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

fn executar(
    launcher: Launcher,
    f: impl FnOnce() -> Result<Vec<DetectedGame>, String>,
    jogos: &mut Vec<DetectedGame>,
    falhas: &mut Vec<FalhaScanner>,
) {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(v)) => jogos.extend(v),
        Ok(Err(erro)) => falhas.push(FalhaScanner { launcher, erro }),
        Err(_) => falhas.push(FalhaScanner {
            launcher,
            erro: "pânico no scanner".into(),
        }),
    }
}

/// Executa todos os scanners (ordem = prioridade na deduplicação).
pub fn varrer_tudo() -> RelatorioVarredura {
    let mut jogos = Vec::new();
    let mut falhas = Vec::new();
    let entradas = catch_unwind(registro::entradas_desinstalacao).unwrap_or_default();

    executar(Launcher::Steam, steam::varrer, &mut jogos, &mut falhas);
    executar(Launcher::Riot, riot::varrer, &mut jogos, &mut falhas);
    executar(Launcher::Epic, epic::varrer, &mut jogos, &mut falhas);
    executar(Launcher::Gog, por_registro::gog, &mut jogos, &mut falhas);
    executar(
        Launcher::Ubisoft,
        por_registro::ubisoft,
        &mut jogos,
        &mut falhas,
    );
    executar(
        Launcher::Ea,
        || por_registro::ea(&entradas),
        &mut jogos,
        &mut falhas,
    );
    executar(
        Launcher::BattleNet,
        || por_registro::battlenet(&entradas),
        &mut jogos,
        &mut falhas,
    );
    executar(Launcher::Xbox, xbox::varrer, &mut jogos, &mut falhas);
    executar(
        Launcher::Generico,
        || generico::varrer(&entradas),
        &mut jogos,
        &mut falhas,
    );

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
