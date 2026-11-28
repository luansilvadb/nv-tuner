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

mod dados;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::model::{DetectedGame, Launcher, deduplicar};
use crate::registro::{self, EntradaDesinstalacao};

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

/// Resultado de um scanner: jogos ou erro em texto.
pub(crate) type Resultado = Result<Vec<DetectedGame>, String>;

/// Dados compartilhados por todos os scanners de uma varredura.
pub(crate) struct Contexto {
    /// Chaves de desinstalação do Windows (lidas uma vez).
    pub entradas: Vec<EntradaDesinstalacao>,
    /// `%ProgramData%`.
    pub program_data: PathBuf,
    /// Entradas classificadas por publisher, calculadas na primeira consulta.
    por_publisher: OnceLock<Vec<DetectedGame>>,
}

impl Contexto {
    pub fn new(entradas: Vec<EntradaDesinstalacao>, program_data: PathBuf) -> Self {
        Self {
            entradas,
            program_data,
            por_publisher: OnceLock::new(),
        }
    }

    /// [`por_registro::classificar_publisher`] sobre todas as entradas, uma vez só.
    pub fn por_publisher(&self) -> &[DetectedGame] {
        self.por_publisher.get_or_init(|| {
            self.entradas
                .iter()
                .filter_map(por_registro::classificar_publisher)
                .collect()
        })
    }
}

/// Um scanner de biblioteca de jogos.
pub(crate) trait Scanner: Sync {
    fn launcher(&self) -> Launcher;
    fn varrer(&self, ctx: &Contexto) -> Resultado;
}

/// Scanner definido por uma função.
struct PorFuncao(Launcher, fn(&Contexto) -> Resultado);

impl Scanner for PorFuncao {
    fn launcher(&self) -> Launcher {
        self.0
    }
    fn varrer(&self, ctx: &Contexto) -> Resultado {
        (self.1)(ctx)
    }
}

/// Ponto único de registro dos scanners.
///
/// A ordem da tabela é a ordem em que os resultados são juntados e deve seguir
/// [`Launcher::prioridade`] (verificado em teste).
static SCANNERS: &[&dyn Scanner] = &[
    &PorFuncao(Launcher::Steam, |_| steam::varrer()),
    &PorFuncao(Launcher::Riot, |c| riot::varrer_em(&c.program_data)),
    &PorFuncao(Launcher::Epic, |c| epic::varrer_em(&c.program_data)),
    &PorFuncao(Launcher::Gog, |_| por_registro::gog()),
    &PorFuncao(Launcher::Ubisoft, |_| por_registro::ubisoft()),
    &PorFuncao(Launcher::Ea, por_registro::ea_ctx),
    &PorFuncao(Launcher::BattleNet, por_registro::battlenet_ctx),
    &PorFuncao(Launcher::Xbox, |_| xbox::varrer()),
    &PorFuncao(Launcher::Generico, |c| generico::varrer(&c.entradas)),
];

/// Último componente do caminho (nome da pasta), ou vazio.
pub(crate) fn ultimo_componente(p: &Path) -> String {
    p.components()
        .next_back()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .unwrap_or_default()
}

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
    let ctx = Contexto::new(entradas, program_data());
    let ctx = &ctx;

    std::thread::scope(|s| {
        let tarefas: Vec<_> = SCANNERS
            .iter()
            .map(|sc| (sc.launcher(), s.spawn(move || rodar(|| sc.varrer(ctx)))))
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

    #[test]
    fn tabela_segue_a_prioridade_e_cobre_cada_launcher_uma_vez() {
        let ordem: Vec<_> = SCANNERS.iter().map(|s| s.launcher()).collect();
        assert_eq!(
            ordem,
            [
                Launcher::Steam,
                Launcher::Riot,
                Launcher::Epic,
                Launcher::Gog,
                Launcher::Ubisoft,
                Launcher::Ea,
                Launcher::BattleNet,
                Launcher::Xbox,
                Launcher::Generico,
            ]
        );
        assert!(
            ordem
                .windows(2)
                .all(|w| w[0].prioridade() < w[1].prioridade())
        );
    }

    #[test]
    fn contexto_classifica_publisher_como_antes() {
        let e = |nome: &str, publ: &str| EntradaDesinstalacao {
            chave: nome.into(),
            display_name: Some(nome.into()),
            install_location: Some(format!(r"D:\{nome}")),
            publisher: Some(publ.into()),
        };
        let entradas = vec![
            e("A", "Electronic Arts"),
            e("B", "Blizzard Entertainment"),
            e("C", "Electronic Arts"),
            e("Battle.net", "Blizzard Entertainment"),
        ];
        let ctx = Contexto::new(entradas.clone(), PathBuf::new());
        assert_eq!(
            por_registro::battlenet_ctx(&ctx).unwrap(),
            por_registro::battlenet(&entradas).unwrap()
        );
        let ea: Vec<_> = por_registro::ea_ctx(&ctx)
            .unwrap()
            .into_iter()
            .filter(|j| j.app_id.is_some())
            .collect();
        let ea_antigo: Vec<_> = por_registro::ea(&entradas)
            .unwrap()
            .into_iter()
            .filter(|j| j.app_id.is_some())
            .collect();
        assert_eq!(ea, ea_antigo);
        assert_eq!(ea.len(), 2);
    }
}
