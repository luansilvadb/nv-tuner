//! Tipos de saída da detecção de jogos.

use std::fmt;
use std::path::{Path, PathBuf};

/// Origem de um jogo detectado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Launcher {
    Steam,
    Riot,
    Epic,
    Gog,
    Ubisoft,
    Ea,
    BattleNet,
    Xbox,
    /// Chaves de desinstalação do Windows (rede de segurança).
    Generico,
}

impl Launcher {
    /// Prioridade na deduplicação: menor vence (scanners específicos antes do genérico).
    pub fn prioridade(self) -> u8 {
        self as u8
    }
}

impl fmt::Display for Launcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Launcher::Steam => "Steam",
            Launcher::Riot => "Riot",
            Launcher::Epic => "Epic",
            Launcher::Gog => "GOG",
            Launcher::Ubisoft => "Ubisoft Connect",
            Launcher::Ea => "EA app",
            Launcher::BattleNet => "Battle.net",
            Launcher::Xbox => "Xbox",
            Launcher::Generico => "Genérico (desinstalação)",
        };
        f.write_str(s)
    }
}

/// Jogo instalado encontrado por um scanner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedGame {
    pub launcher: Launcher,
    pub nome: String,
    pub pasta: PathBuf,
    /// Identificador no launcher (appid da Steam, AppName da Epic, produto Riot...).
    pub app_id: Option<String>,
    /// Exe principal informado pelo próprio launcher (Epic, GOG, Xbox), quando houver.
    pub exe_indicado: Option<PathBuf>,
    /// Metadado do launcher Riot: `dependencies.vanguard: true`.
    pub exige_vanguard: bool,
}

impl DetectedGame {
    pub fn new(launcher: Launcher, nome: impl Into<String>, pasta: impl Into<PathBuf>) -> Self {
        Self {
            launcher,
            nome: nome.into(),
            pasta: pasta.into(),
            app_id: None,
            exe_indicado: None,
            exige_vanguard: false,
        }
    }

    pub fn with_app_id(mut self, id: impl Into<String>) -> Self {
        self.app_id = Some(id.into());
        self
    }

    /// Pasta de instalação.
    pub fn pasta_instalacao(&self) -> &Path {
        &self.pasta
    }

    /// AppID da Steam, quando o jogo veio da Steam e o identificador é numérico.
    pub fn steam_appid(&self) -> Option<u32> {
        if self.launcher != Launcher::Steam {
            return None;
        }
        self.app_id.as_deref()?.parse().ok()
    }

    /// Identificador do jogo no launcher (appid, AppName, produto Riot...).
    pub fn id_launcher(&self) -> Option<&str> {
        self.app_id.as_deref()
    }

    /// Chave de deduplicação (pasta canônica).
    pub fn chave(&self) -> String {
        pasta_canonica(&self.pasta)
    }
}

/// Normaliza uma pasta para comparação: resolve o caminho quando existe, troca
/// `/` por `\`, remove o prefixo `\\?\`, a barra final e diferenças de caixa.
pub fn pasta_canonica(p: &Path) -> String {
    let base = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    chave_caminho(&base)
}

/// Troca `/` por `\` (forma de caminho do Windows), sem outras mudanças.
pub(crate) fn caminho_windows(s: &str) -> PathBuf {
    PathBuf::from(s.replace('/', "\\"))
}

pub(crate) fn chave_caminho(base: &Path) -> String {
    let mut s = base.to_string_lossy().replace('/', "\\");
    if let Some(r) = s.strip_prefix(r"\\?\UNC\") {
        s = format!(r"\\{r}");
    } else if let Some(r) = s.strip_prefix(r"\\?\") {
        s = r.to_string();
    }
    while s.len() > 3 && s.ends_with('\\') {
        s.pop();
    }
    s.to_lowercase()
}

/// Remove duplicatas pela pasta canônica. Mantém a entrada do launcher de maior
/// prioridade e completa campos ausentes com as demais.
pub fn deduplicar(jogos: Vec<DetectedGame>) -> Vec<DetectedGame> {
    let mut saida: Vec<(String, DetectedGame)> = Vec::new();
    for j in jogos {
        let chave = j.chave();
        match saida.iter_mut().find(|(c, _)| *c == chave) {
            Some((_, atual)) => {
                let (mut vence, perde) = if j.launcher.prioridade() < atual.launcher.prioridade() {
                    (j, atual.clone())
                } else {
                    (atual.clone(), j)
                };
                if vence.app_id.is_none() {
                    vence.app_id = perde.app_id;
                }
                if vence.exe_indicado.is_none() {
                    vence.exe_indicado = perde.exe_indicado;
                }
                vence.exige_vanguard |= perde.exige_vanguard;
                *atual = vence;
            }
            None => saida.push((chave, j)),
        }
    }
    saida.into_iter().map(|(_, j)| j).collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn canonica_ignora_barras_e_caixa() {
        let a = pasta_canonica(Path::new("C:/Riot Games/League of Legends/"));
        let b = pasta_canonica(Path::new(r"c:\riot games\league of legends"));
        assert_eq!(a, b);
    }

    #[test]
    fn dedup_prefere_launcher_especifico() {
        let generico =
            DetectedGame::new(Launcher::Generico, "Steam App 1", "X:/Jogo/").with_app_id("1");
        let mut steam = DetectedGame::new(Launcher::Steam, "Jogo", r"x:\jogo");
        steam.exe_indicado = None;
        let r = deduplicar(vec![generico, steam]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].launcher, Launcher::Steam);
        assert_eq!(r[0].nome, "Jogo");
        assert_eq!(r[0].app_id.as_deref(), Some("1"));
    }

    #[test]
    fn dedup_mantem_pastas_diferentes() {
        let r = deduplicar(vec![
            DetectedGame::new(Launcher::Epic, "A", "X:/A"),
            DetectedGame::new(Launcher::Epic, "B", "X:/B"),
        ]);
        assert_eq!(r.len(), 2);
    }
}
