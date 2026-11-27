//! Configuração de jogo (fase 3, `docs/spec/fase3-plano.md` D10): tipos dos
//! destinos de jogo, catálogo `[[arquivo]]`/`[[regra]]` de `regras/jogos.toml`
//! (schema 2) e avaliação das regras `config_jogo`, sem E/S.
//!
//! O serviço avalia; o agente resolve caminhos, lê e grava (D7). Aqui só há o
//! modelo, a validação do catálogo (F3-02, trava de 9.5.4, N8) e o plano.
//! F3-13 (DRS `limite_fps = off` no perfil do jogo) fica em
//! [`desligar_limite_drs`].

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::catalogo::{Catalogo, PreferenciaGovernante};
use crate::evaluator::SETTING_LIMITE_FPS;
use crate::explicacao::RegraAplicada;
use crate::facts::GameFacts;
use crate::jogos::{IdJogo, JOGOS_SEM_EDICAO, JogoCurado, PoliticaConfig};
use crate::plan::{CodigoExplicacao, EstadoExperimental, Origem, Plan, SettingChange};
use crate::preferences::{Alvo, Preferences};
use crate::rules::{ImpactoVisual, Status, id_valido};
use crate::usuario::MotivoRespeitadaUsuario;

/// Padrão de `fps_max` do Dota 2: `jogo-dota2-fps-max-padrao` só sobe acima dele.
pub const FPS_MAX_PADRAO_DOTA2: i64 = 120;

/// Token de limite nas opções de inicialização (único que o app grava, F3-11).
pub const TOKEN_FPS_MAX: &str = "+fps_max";

/// Nomes de arquivo da pasta de instalação que o app nunca toca (§9.1).
pub const ARQUIVOS_PROIBIDOS: [&str; 4] = ["autoexec.cfg", "video.txt", "boot.vcfg", "gameinfo.gi"];

/// Componentes de caminho que indicam pasta de instalação (N8).
const PASTAS_DE_INSTALACAO: [&str; 4] = [
    "steamapps",
    "common",
    "program files",
    "program files (x86)",
];

/// Marcadores aceitos em `caminho`.
const MARCADORES: [&str; 3] = ["<conta>", "<steamid3>", "<appid>"];

/// Chaves do catálogo de jogo: lista fechada. Cada uma pertence a um só
/// arquivo lógico de um só jogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingJogo {
    R6FpsLimit,
    R6Reflex,
    FortniteFrameRateLimit,
    FortniteReflex,
    RlOneFrameThreadLag,
    RlMaxFps,
    Cs2FpsMax,
    Cs2Reflex,
    Dota2FpsMax,
    DeadlockFpsMax,
}

impl SettingJogo {
    pub const TODAS: [Self; 10] = [
        Self::R6FpsLimit,
        Self::R6Reflex,
        Self::FortniteFrameRateLimit,
        Self::FortniteReflex,
        Self::RlOneFrameThreadLag,
        Self::RlMaxFps,
        Self::Cs2FpsMax,
        Self::Cs2Reflex,
        Self::Dota2FpsMax,
        Self::DeadlockFpsMax,
    ];

    /// Código usado no TOML, no IPC e na forma textual do destino.
    pub const fn codigo(self) -> &'static str {
        match self {
            Self::R6FpsLimit => "r6_fps_limit",
            Self::R6Reflex => "r6_reflex",
            Self::FortniteFrameRateLimit => "fortnite_frame_rate_limit",
            Self::FortniteReflex => "fortnite_reflex",
            Self::RlOneFrameThreadLag => "rl_one_frame_thread_lag",
            Self::RlMaxFps => "rl_max_fps",
            Self::Cs2FpsMax => "cs2_fps_max",
            Self::Cs2Reflex => "cs2_reflex",
            Self::Dota2FpsMax => "dota2_fps_max",
            Self::DeadlockFpsMax => "deadlock_fps_max",
        }
    }

    /// Id do jogo na base curada.
    pub const fn jogo(self) -> &'static str {
        match self {
            Self::R6FpsLimit | Self::R6Reflex => "r6-siege",
            Self::FortniteFrameRateLimit | Self::FortniteReflex => "fortnite",
            Self::RlOneFrameThreadLag | Self::RlMaxFps => "rocket-league",
            Self::Cs2FpsMax | Self::Cs2Reflex => "cs2",
            Self::Dota2FpsMax => "dota2",
            Self::DeadlockFpsMax => "deadlock",
        }
    }

    /// Limite padrão do próprio jogo, abaixo do qual `refresh_limitado_ao_menu`
    /// não sobe; `None` = a ação não vale para esta setting.
    pub const fn limite_padrao(self) -> Option<i64> {
        match self {
            Self::Dota2FpsMax => Some(FPS_MAX_PADRAO_DOTA2),
            _ => None,
        }
    }

    /// Id do arquivo lógico (`[[arquivo]].id`).
    pub const fn arquivo(self) -> &'static str {
        match self {
            Self::R6FpsLimit | Self::R6Reflex => "r6-game-settings",
            Self::FortniteFrameRateLimit | Self::FortniteReflex => "fortnite-game-user-settings",
            Self::RlOneFrameThreadLag | Self::RlMaxFps => "rl-ta-system-settings",
            Self::Cs2FpsMax => "cs2-launch",
            Self::Cs2Reflex => "cs2-video",
            Self::Dota2FpsMax => "dota2-launch",
            Self::DeadlockFpsMax => "deadlock-launch",
        }
    }

    pub fn de_codigo(codigo: &str) -> Option<Self> {
        Self::TODAS.into_iter().find(|s| s.codigo() == codigo)
    }
}

impl fmt::Display for SettingJogo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

/// Onde está o arquivo: jogo, arquivo lógico e conta resolvida pelo agente
/// (hash, nunca o identificador em claro).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct InstanciaJogo {
    pub jogo: IdJogo,
    pub arquivo: String,
    pub conta: Option<String>,
}

/// Destino de uma escrita de jogo. Forma textual canônica estável
/// (`"r6_fps_limit@<hash-da-conta>"` ou `"dota2_fps_max"` sem conta), usada
/// como chave de mapa na serialização.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DestinoJogo {
    pub setting: SettingJogo,
    pub instancia: InstanciaJogo,
}

/// Forma textual de destino inválida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinoInvalido(pub String);

impl fmt::Display for DestinoInvalido {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "destino de jogo inválido: \"{}\"", self.0)
    }
}

impl std::error::Error for DestinoInvalido {}

fn conta_valida(conta: &str) -> bool {
    (1..=64).contains(&conta.len())
        && conta
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl DestinoJogo {
    /// Destino de `setting` na conta `conta` (hash). `None` se a conta não
    /// for um hash aceito (1 a 64 de `[A-Za-z0-9_-]`).
    pub fn novo(setting: SettingJogo, conta: Option<&str>) -> Option<Self> {
        if conta.is_some_and(|c| !conta_valida(c)) {
            return None;
        }
        Some(Self {
            setting,
            instancia: InstanciaJogo {
                jogo: IdJogo::interno(setting.jogo()),
                arquivo: setting.arquivo().to_owned(),
                conta: conta.map(str::to_owned),
            },
        })
    }

    /// `true` se jogo e arquivo da instância são os da setting e a conta é válida.
    pub fn coerente(&self) -> bool {
        self.instancia.jogo.as_str() == self.setting.jogo()
            && self.instancia.arquivo == self.setting.arquivo()
            && self.instancia.conta.as_deref().is_none_or(conta_valida)
    }
}

impl fmt::Display for DestinoJogo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.instancia.conta {
            Some(conta) => write!(f, "{}@{conta}", self.setting.codigo()),
            None => f.write_str(self.setting.codigo()),
        }
    }
}

impl FromStr for DestinoJogo {
    type Err = DestinoInvalido;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (codigo, conta) = match s.split_once('@') {
            Some((c, conta)) => (c, Some(conta)),
            None => (s, None),
        };
        SettingJogo::de_codigo(codigo)
            .and_then(|setting| Self::novo(setting, conta))
            .ok_or_else(|| DestinoInvalido(s.to_owned()))
    }
}

impl Serialize for DestinoJogo {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for DestinoJogo {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let texto = String::deserialize(d)?;
        texto.parse().map_err(serde::de::Error::custom)
    }
}

/// Valor de uma chave de jogo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValorJogo {
    /// Chave ausente (só como leitura/original; o app não cria chave).
    Ausente,
    Inteiro(i64),
    Booleano(bool),
    /// Texto curto, ASCII imprimível (até 64 bytes).
    Texto(String),
    /// Token `+fps_max <n>` nas opções de inicialização; `None` = sem o token.
    TokenLancamento(Option<u32>),
}

impl ValorJogo {
    fn texto_valido(t: &str) -> bool {
        t.len() <= 64 && t.bytes().all(|b| (0x20..0x7f).contains(&b))
    }
}

/// Gestão de um destino de jogo pelo app (snapshot e backup, F3-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GestaoJogo {
    pub antes: ValorJogo,
    pub esperado: ValorJogo,
    pub hash_antes: Option<String>,
    pub hash_depois: Option<String>,
    /// Id do backup `.orig` (nunca um caminho completo).
    pub backup: Option<String>,
}

/// Leitura de um destino resolvido pelo agente.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EstadoDestinoJogo {
    /// `None` = leitura desconhecida.
    pub atual: Option<ValorJogo>,
    pub somente_leitura: bool,
    pub na_nuvem: bool,
    pub conta_ambigua: bool,
    pub hash: Option<String>,
}

/// Fatos de configuração de jogo de uma sessão (agente + coordenador).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FatosConfigJogo {
    pub destinos: BTreeMap<DestinoJogo, EstadoDestinoJogo>,
    pub gestao: BTreeMap<DestinoJogo, GestaoJogo>,
    pub travadas: BTreeSet<DestinoJogo>,
}

// ── Catálogo ────────────────────────────────────────────────────────────────

/// Raiz conhecida resolvida pelo agente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaizJogo {
    Documentos,
    LocalAppData,
    SteamUserdata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatoArquivo {
    Ini,
    Vdf,
    Lancamento,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoValorJogo {
    Inteiro,
    /// Número gravado como decimal (`240.000000`, Unreal); o valor é inteiro.
    Decimal,
    /// `True`/`False`.
    Booleano,
    Texto,
    TokenLancamento,
}

/// Como um número pedido vira valor do menu.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arredondamento {
    /// Só valores exatos do menu.
    #[default]
    Exato,
    /// Maior valor do menu que não passa do pedido (Fortnite).
    Baixo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaixaJogo {
    pub min: i64,
    pub max: i64,
}

/// Uma chave de um arquivo do catálogo (`[[arquivo.chave]]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefChaveJogo {
    pub setting: SettingJogo,
    /// Seção INI.
    #[serde(default)]
    pub secao: Option<String>,
    /// Chave INI, ou caminho de chaves VDF (`A/B/C`).
    #[serde(default)]
    pub chave: String,
    /// Token das opções de inicialização (só `+fps_max`).
    #[serde(default)]
    pub token: Option<String>,
    pub tipo: TipoValorJogo,
    /// Valores do menu (F3-02).
    #[serde(default)]
    pub valores: Vec<i64>,
    /// Faixa do menu (F3-02).
    #[serde(default)]
    pub faixa: Option<FaixaJogo>,
    #[serde(default)]
    pub arredondar: Arredondamento,
    /// Chave ou valores por confirmar em máquina real: nenhuma regra grava.
    #[serde(default)]
    pub a_confirmar: bool,
}

impl DefChaveJogo {
    /// Número pedido → valor do menu, ou `None` (descartado, F3-02).
    pub fn ajustar(&self, pedido: i64) -> Option<i64> {
        if self.a_confirmar {
            return None;
        }
        let n = if self.valores.is_empty() {
            pedido
        } else {
            match self.arredondar {
                Arredondamento::Exato => self.valores.contains(&pedido).then_some(pedido)?,
                Arredondamento::Baixo => self
                    .valores
                    .iter()
                    .copied()
                    .filter(|&v| v <= pedido)
                    .max()?,
            }
        };
        self.faixa
            .is_none_or(|f| (f.min..=f.max).contains(&n))
            .then_some(n)
    }

    /// Valor aceito como alvo de gravação (tipo e menu).
    pub fn aceita(&self, valor: &ValorJogo) -> bool {
        if self.a_confirmar {
            return false;
        }
        match (self.tipo, valor) {
            (TipoValorJogo::Inteiro | TipoValorJogo::Decimal, ValorJogo::Inteiro(n)) => {
                self.ajustar(*n) == Some(*n)
            }
            (TipoValorJogo::TokenLancamento, ValorJogo::TokenLancamento(Some(n))) => {
                self.ajustar(i64::from(*n)) == Some(i64::from(*n))
            }
            (TipoValorJogo::TokenLancamento, ValorJogo::TokenLancamento(None))
            | (TipoValorJogo::Booleano, ValorJogo::Booleano(_)) => true,
            _ => false,
        }
    }

    /// Valor plausível como leitura ou original (só o tipo).
    pub fn aceita_leitura(&self, valor: &ValorJogo) -> bool {
        match (self.tipo, valor) {
            (TipoValorJogo::Inteiro | TipoValorJogo::Decimal, ValorJogo::Inteiro(_))
            | (TipoValorJogo::Booleano, ValorJogo::Booleano(_))
            | (TipoValorJogo::TokenLancamento, ValorJogo::TokenLancamento(_)) => true,
            (TipoValorJogo::Texto, ValorJogo::Texto(t)) => ValorJogo::texto_valido(t),
            (t, ValorJogo::Ausente) => t != TipoValorJogo::TokenLancamento,
            _ => false,
        }
    }

    fn valor_de_numero(&self, n: i64) -> Option<ValorJogo> {
        match self.tipo {
            TipoValorJogo::Inteiro | TipoValorJogo::Decimal => Some(ValorJogo::Inteiro(n)),
            TipoValorJogo::TokenLancamento => u32::try_from(n)
                .ok()
                .map(|n| ValorJogo::TokenLancamento(Some(n))),
            _ => None,
        }
    }
}

/// Um arquivo de configuração do usuário (`[[arquivo]]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefArquivoJogo {
    pub id: String,
    pub jogo: IdJogo,
    pub raiz: RaizJogo,
    /// Relativo à raiz, com `\`, podendo usar `<conta>`, `<steamid3>`, `<appid>`.
    pub caminho: String,
    pub formato: FormatoArquivo,
    /// Processos que precisam estar fechados (além dos protegidos do jogo).
    #[serde(default)]
    pub processos_fechados: Vec<String>,
    #[serde(default)]
    pub exige_steam_fechada: bool,
    pub evidencia: String,
    pub chave: Vec<DefChaveJogo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscopoJogo {
    ConfigJogo,
}

/// Condições de uma regra `config_jogo`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CondicoesJogo {
    pub jogo: Vec<IdJogo>,
    #[serde(default)]
    pub limite_fps_definido: Option<bool>,
    #[serde(default)]
    pub refresh_conhecido: Option<bool>,
    #[serde(default)]
    pub arquivo_local: Option<bool>,
}

impl CondicoesJogo {
    fn quantidade(&self) -> usize {
        1 + [
            self.limite_fps_definido.is_some(),
            self.refresh_conhecido.is_some(),
            self.arquivo_local.is_some(),
        ]
        .into_iter()
        .filter(|&p| p)
        .count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreferenciaJogo {
    LimiteFps,
}

/// Valor de uma ação `aplicar` de regra de jogo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcaoJogo {
    /// `{ preferencia = "limite_fps" }`: o limite do usuário, ajustado ao menu.
    Preferencia(PreferenciaJogo),
    /// `"refresh_limitado_ao_menu"`: `min(refresh, teto do menu)`; só em
    /// `dota2_fps_max` e só sobe acima do padrão 120.
    RefreshLimitadoAoMenu,
    Booleano(bool),
    Inteiro(i64),
}

/// Regra `config_jogo` (`[[regra]]` de `jogos.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegraJogo {
    pub id: String,
    pub descricao: String,
    pub status: Status,
    pub escopo: EscopoJogo,
    pub quando: CondicoesJogo,
    pub aplicar: BTreeMap<SettingJogo, AcaoJogo>,
    pub impacto_visual: ImpactoVisual,
    #[serde(default)]
    pub prioridade: i32,
    /// Limite de FPS no jogo: aplicado, o perfil DRS recebe `limite_fps = off` (F3-13).
    #[serde(default)]
    pub limite_no_jogo: bool,
    pub motivo: String,
    pub evidencia: String,
}

/// Seções `[[arquivo]]` e `[[regra]]` validadas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogoConfigJogo {
    arquivos: Vec<DefArquivoJogo>,
    regras: Vec<RegraJogo>,
}

impl CatalogoConfigJogo {
    pub fn arquivos(&self) -> &[DefArquivoJogo] {
        &self.arquivos
    }

    pub fn regras(&self) -> &[RegraJogo] {
        &self.regras
    }

    pub fn arquivo(&self, id: &str) -> Option<&DefArquivoJogo> {
        self.arquivos.iter().find(|a| a.id == id)
    }

    /// Definição da chave de uma setting.
    pub fn chave(&self, setting: SettingJogo) -> Option<&DefChaveJogo> {
        self.arquivo(setting.arquivo())?
            .chave
            .iter()
            .find(|c| c.setting == setting)
    }

    /// Settings que alguma regra pode gravar.
    pub fn gravaveis(&self) -> BTreeSet<SettingJogo> {
        self.regras
            .iter()
            .flat_map(|r| r.aplicar.keys().copied())
            .collect()
    }

    /// Valida um relato do agente: destino coerente e valor aceito.
    pub fn aceita(&self, destino: &DestinoJogo, valor: &ValorJogo) -> bool {
        destino.coerente() && self.chave(destino.setting).is_some_and(|c| c.aceita(valor))
    }
}

/// Erro de validação de `[[arquivo]]`/`[[regra]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroConfigJogo {
    /// Jogo fora da base.
    JogoDesconhecido { item: String, jogo: String },
    /// `[[arquivo]]` ou regra em jogo com `orientar`/`proibido` (9.5.4).
    JogoSemEdicao { item: String, jogo: String },
    /// Caminho absoluto, com `..`, na pasta de instalação (N8) ou com nome proibido (§9.1).
    CaminhoProibido { arquivo: String, motivo: String },
    /// Setting em arquivo/jogo diferente do da lista fechada, repetida ou ausente.
    ChaveForaDoCatalogo { item: String, setting: String },
    /// Definição de chave/arquivo incoerente (formato, tipo, faixa, processos).
    DefinicaoInvalida { item: String, mensagem: String },
    /// Regra que grava chave `a_confirmar`.
    ChaveAConfirmar { regra: String, setting: SettingJogo },
    /// Valor fixo fora do menu (F3-02).
    ValorForaDoMenu {
        regra: String,
        setting: SettingJogo,
        valor: i64,
    },
    /// Regra incoerente (status, impacto, condições, ação).
    RegraInvalida { regra: String, mensagem: String },
}

impl fmt::Display for ErroConfigJogo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JogoDesconhecido { item, jogo } => {
                write!(f, "{item}: jogo \"{jogo}\" fora da base")
            }
            Self::JogoSemEdicao { item, jogo } => write!(
                f,
                "{item}: jogo \"{jogo}\" só aceita orientar ou proibido; nada gravável"
            ),
            Self::CaminhoProibido { arquivo, motivo } => {
                write!(f, "arquivo \"{arquivo}\": caminho proibido ({motivo})")
            }
            Self::ChaveForaDoCatalogo { item, setting } => {
                write!(f, "{item}: chave \"{setting}\" fora do catálogo")
            }
            Self::DefinicaoInvalida { item, mensagem } => write!(f, "{item}: {mensagem}"),
            Self::ChaveAConfirmar { regra, setting } => write!(
                f,
                "regra \"{regra}\": chave \"{setting}\" a confirmar não pode ser gravada"
            ),
            Self::ValorForaDoMenu {
                regra,
                setting,
                valor,
            } => write!(
                f,
                "regra \"{regra}\": valor {valor} fora do menu de \"{setting}\""
            ),
            Self::RegraInvalida { regra, mensagem } => write!(f, "regra \"{regra}\": {mensagem}"),
        }
    }
}

fn validar_caminho(caminho: &str) -> Result<(), String> {
    if caminho.trim().is_empty() {
        return Err("vazio".into());
    }
    if caminho.contains('/') {
        return Err("use \\ como separador".into());
    }
    if caminho.starts_with('\\') || caminho.contains(':') {
        return Err("caminho absoluto".into());
    }
    let partes: Vec<&str> = caminho.split('\\').collect();
    for parte in &partes {
        let minuscula = parte.to_lowercase();
        if parte.is_empty() || *parte == "." || *parte == ".." {
            return Err(format!("componente \"{parte}\" inválido"));
        }
        if PASTAS_DE_INSTALACAO.contains(&minuscula.as_str()) {
            return Err(format!("pasta de instalação (\"{parte}\")"));
        }
        let mut resto = minuscula.as_str();
        while let Some(i) = resto.find('<') {
            let fim = resto[i..].find('>').ok_or("marcador sem \">\"")? + i;
            if !MARCADORES.contains(&&resto[i..=fim]) {
                return Err(format!("marcador \"{}\" desconhecido", &resto[i..=fim]));
            }
            resto = &resto[fim + 1..];
        }
        if resto.contains('>') {
            return Err("marcador sem \"<\"".into());
        }
    }
    let nome = partes.last().map(|p| p.to_lowercase()).unwrap_or_default();
    if ARQUIVOS_PROIBIDOS.contains(&nome.as_str()) {
        return Err(format!("arquivo \"{nome}\" fora do escopo"));
    }
    Ok(())
}

fn validar_chave(arq: &DefArquivoJogo, c: &DefChaveJogo, erros: &mut Vec<ErroConfigJogo>) {
    let item = format!("arquivo \"{}\"", arq.id);
    let mut def = |m: &str| {
        erros.push(ErroConfigJogo::DefinicaoInvalida {
            item: format!("{item}, chave \"{}\"", c.setting),
            mensagem: m.to_owned(),
        });
    };
    match arq.formato {
        FormatoArquivo::Ini => {
            if c.secao.as_deref().is_none_or(|s| s.trim().is_empty()) {
                def("INI exige `secao`");
            }
            if c.token.is_some() {
                def("INI não usa `token`");
            }
        }
        FormatoArquivo::Vdf | FormatoArquivo::Lancamento => {
            if c.secao.is_some() {
                def("VDF não usa `secao`");
            }
        }
    }
    let lancamento = arq.formato == FormatoArquivo::Lancamento;
    if lancamento != (c.tipo == TipoValorJogo::TokenLancamento) {
        def("`token_lancamento` é o tipo exclusivo do formato `lancamento`");
    }
    if lancamento != (c.token.as_deref() == Some(TOKEN_FPS_MAX)) {
        def("formato `lancamento` exige token = \"+fps_max\" (único token gravável)");
    }
    if let Some(f) = c.faixa
        && f.min > f.max
    {
        def("faixa com min > max");
    }
    if c.valores.windows(2).any(|w| w[0] >= w[1]) {
        def("`valores` deve estar em ordem crescente e sem repetição");
    }
    if c.arredondar == Arredondamento::Baixo && c.valores.is_empty() && !c.a_confirmar {
        def("arredondar = \"baixo\" exige `valores`");
    }
    let numerico = matches!(
        c.tipo,
        TipoValorJogo::Inteiro | TipoValorJogo::Decimal | TipoValorJogo::TokenLancamento
    );
    if !numerico && (!c.valores.is_empty() || c.faixa.is_some()) {
        def("`valores`/`faixa` só para tipos numéricos");
    }
    if !c.a_confirmar {
        if c.chave.trim().is_empty() {
            def("chave vazia (só permitido com a_confirmar)");
        }
        if numerico && c.valores.is_empty() && c.faixa.is_none() {
            def("chave numérica exige `valores` ou `faixa` iguais ao menu (F3-02)");
        }
        if c.tipo == TipoValorJogo::Texto {
            def("tipo texto só com a_confirmar nesta fase");
        }
    }
}

fn checar_jogo<'a>(
    jogos: &'a [JogoCurado],
    item: String,
    id: &IdJogo,
    erros: &mut Vec<ErroConfigJogo>,
) -> Option<&'a JogoCurado> {
    match jogos.iter().find(|j| j.id == *id) {
        None => {
            erros.push(ErroConfigJogo::JogoDesconhecido {
                item,
                jogo: id.to_string(),
            });
            None
        }
        Some(j) if !j.config.grava() || JOGOS_SEM_EDICAO.contains(&id.as_str()) => {
            erros.push(ErroConfigJogo::JogoSemEdicao {
                item,
                jogo: id.to_string(),
            });
            None
        }
        Some(j) => Some(j),
    }
}

/// Valida `[[arquivo]]` e `[[regra]]` contra a base de jogos. `completo`
/// exige todas as chaves de [`SettingJogo`] (schema 2).
pub(crate) fn carregar(
    jogos: &[JogoCurado],
    arquivos: Vec<DefArquivoJogo>,
    regras: Vec<RegraJogo>,
    completo: bool,
    erros: &mut Vec<ErroConfigJogo>,
) -> CatalogoConfigJogo {
    let mut ids = BTreeSet::new();
    let mut vistas = BTreeSet::new();
    for arq in &arquivos {
        let item = format!("arquivo \"{}\"", arq.id);
        if !id_valido(&arq.id) || !ids.insert(arq.id.clone()) {
            erros.push(ErroConfigJogo::DefinicaoInvalida {
                item: item.clone(),
                mensagem: "id inválido ou repetido".into(),
            });
        }
        if let Err(motivo) = validar_caminho(&arq.caminho) {
            erros.push(ErroConfigJogo::CaminhoProibido {
                arquivo: arq.id.clone(),
                motivo,
            });
        }
        let mut def = |m: &str| {
            erros.push(ErroConfigJogo::DefinicaoInvalida {
                item: item.clone(),
                mensagem: m.to_owned(),
            });
        };
        if arq.evidencia.trim().is_empty() {
            def("evidencia vazia");
        }
        if arq.chave.is_empty() {
            def("sem chaves");
        }
        if arq.raiz == RaizJogo::SteamUserdata && !arq.exige_steam_fechada {
            def("arquivo da Steam exige exige_steam_fechada = true (F3-03)");
        }
        if arq.formato == FormatoArquivo::Lancamento && arq.raiz != RaizJogo::SteamUserdata {
            def("opções de inicialização só em localconfig.vdf da Steam");
        }
        if let Some(j) = checar_jogo(jogos, item.clone(), &arq.jogo, erros) {
            if j.config == PoliticaConfig::OpcoesInicializacao
                && arq.formato != FormatoArquivo::Lancamento
            {
                erros.push(ErroConfigJogo::DefinicaoInvalida {
                    item: item.clone(),
                    mensagem: "jogo com opcoes_inicializacao só grava formato lancamento".into(),
                });
            }
            let fechados: BTreeSet<_> = arq
                .processos_fechados
                .iter()
                .map(|p| p.to_lowercase())
                .collect();
            if j.executaveis
                .iter()
                .any(|e| !fechados.contains(&e.to_lowercase()))
            {
                erros.push(ErroConfigJogo::DefinicaoInvalida {
                    item: item.clone(),
                    mensagem: "processos_fechados deve conter os executáveis do jogo".into(),
                });
            }
        }
        for c in &arq.chave {
            if c.setting.arquivo() != arq.id
                || c.setting.jogo() != arq.jogo.as_str()
                || !vistas.insert(c.setting)
            {
                erros.push(ErroConfigJogo::ChaveForaDoCatalogo {
                    item: item.clone(),
                    setting: c.setting.to_string(),
                });
            }
            validar_chave(arq, c, erros);
        }
    }
    if completo {
        for s in SettingJogo::TODAS {
            if !vistas.contains(&s) {
                erros.push(ErroConfigJogo::ChaveForaDoCatalogo {
                    item: "catálogo".into(),
                    setting: format!("{s} (ausente)"),
                });
            }
        }
    }

    let catalogo = CatalogoConfigJogo {
        arquivos,
        regras: Vec::new(),
    };
    let mut ids_regra = BTreeSet::new();
    for r in &regras {
        validar_regra(r, jogos, &catalogo, &mut ids_regra, erros);
    }
    CatalogoConfigJogo { regras, ..catalogo }
}

fn validar_regra(
    r: &RegraJogo,
    jogos_base: &[JogoCurado],
    catalogo: &CatalogoConfigJogo,
    ids: &mut BTreeSet<String>,
    erros: &mut Vec<ErroConfigJogo>,
) {
    let inv = |m: &str, erros: &mut Vec<ErroConfigJogo>| {
        erros.push(ErroConfigJogo::RegraInvalida {
            regra: r.id.clone(),
            mensagem: m.to_owned(),
        });
    };
    if !id_valido(&r.id) || !r.id.starts_with("jogo-") || !ids.insert(r.id.clone()) {
        inv("id inválido, repetido ou sem prefixo \"jogo-\"", erros);
    }
    if r.status != Status::Experimental {
        inv(
            "toda regra de config de jogo é experimental nesta fase (D9)",
            erros,
        );
    }
    if r.impacto_visual != ImpactoVisual::Nenhum {
        inv("impacto_visual deve ser \"nenhum\"", erros);
    }
    for (campo, vazio) in [
        ("descricao", r.descricao.trim().is_empty()),
        ("motivo", r.motivo.trim().is_empty()),
        ("evidencia", r.evidencia.trim().is_empty()),
        ("aplicar", r.aplicar.is_empty()),
        ("quando.jogo", r.quando.jogo.is_empty()),
    ] {
        if vazio {
            inv(&format!("campo `{campo}` vazio"), erros);
        }
    }
    let mut jogos = BTreeSet::new();
    for j in &r.quando.jogo {
        if !jogos.insert(j.as_str()) {
            inv("jogo repetido em quando.jogo", erros);
        }
        checar_jogo(jogos_base, format!("regra \"{}\"", r.id), j, erros);
    }
    let q = &r.quando;
    for (&setting, acao) in &r.aplicar {
        if !jogos.contains(setting.jogo()) {
            inv(
                &format!("\"{setting}\" é de jogo fora de quando.jogo"),
                erros,
            );
        }
        let Some(def) = catalogo.chave(setting) else {
            erros.push(ErroConfigJogo::ChaveForaDoCatalogo {
                item: format!("regra \"{}\"", r.id),
                setting: setting.to_string(),
            });
            continue;
        };
        if def.a_confirmar {
            erros.push(ErroConfigJogo::ChaveAConfirmar {
                regra: r.id.clone(),
                setting,
            });
            continue;
        }
        let numerico = matches!(
            def.tipo,
            TipoValorJogo::Inteiro | TipoValorJogo::Decimal | TipoValorJogo::TokenLancamento
        );
        match acao {
            AcaoJogo::Preferencia(PreferenciaJogo::LimiteFps) => {
                if !numerico || q.limite_fps_definido != Some(true) {
                    inv(
                        "preferencia limite_fps exige chave numérica e limite_fps_definido = true",
                        erros,
                    );
                }
            }
            AcaoJogo::RefreshLimitadoAoMenu => {
                if setting.limite_padrao().is_none()
                    || q.limite_fps_definido != Some(false)
                    || q.refresh_conhecido != Some(true)
                    || def.faixa.is_none()
                {
                    inv(
                        "refresh_limitado_ao_menu só em dota2_fps_max, com limite_fps_definido = false e refresh_conhecido = true (F3-13)",
                        erros,
                    );
                }
            }
            AcaoJogo::Booleano(_) => {
                if def.tipo != TipoValorJogo::Booleano {
                    inv(&format!("\"{setting}\" não é booleana"), erros);
                }
            }
            AcaoJogo::Inteiro(n) => {
                if !numerico || def.ajustar(*n) != Some(*n) {
                    erros.push(ErroConfigJogo::ValorForaDoMenu {
                        regra: r.id.clone(),
                        setting,
                        valor: *n,
                    });
                }
            }
        }
        if r.limite_no_jogo && !matches!(acao, AcaoJogo::Preferencia(_)) {
            inv(
                "limite_no_jogo exige ações { preferencia = \"limite_fps\" }",
                erros,
            );
        }
    }
}

// ── Avaliação ───────────────────────────────────────────────────────────────

/// Mudança planejada num destino de jogo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MudancaJogo {
    pub destino: DestinoJogo,
    pub atual: ValorJogo,
    pub valor: ValorJogo,
    pub regra: String,
    pub descricao: String,
    pub motivo: String,
    pub evidencia: String,
    pub codigo: Option<CodigoExplicacao>,
    /// Limite de FPS no jogo (F3-13).
    pub limite_no_jogo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SugestaoJogo {
    pub regra: String,
    pub mudancas: Vec<MudancaJogo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotivoDescarteJogo {
    /// Valor pedido fora dos valores/faixa do menu (F3-02).
    ForaDoMenu {
        pedido: i64,
    },
    /// Não sobe acima do padrão nem do valor existente (Dota 2, F3-13).
    NaoSobeOLimite,
    LeituraDesconhecida,
    SomenteLeitura,
    NaNuvem,
    ContaAmbigua,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescarteJogo {
    pub regra: String,
    pub destino: DestinoJogo,
    pub motivo: MotivoDescarteJogo,
}

/// Plano da família jogo de uma sessão.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanoJogo {
    /// Vencedoras (só regras aceitas). A execução é confirmada pelo agente.
    pub mudancas: BTreeMap<DestinoJogo, MudancaJogo>,
    pub respeitadas: BTreeMap<DestinoJogo, MotivoRespeitadaUsuario>,
    pub experimentais: Vec<SugestaoJogo>,
    pub estado_experimental: BTreeMap<String, EstadoExperimental>,
    pub descartes: Vec<DescarteJogo>,
}

/// Precedência entre regras aceitas no mesmo destino; maior vence.
type ChaveJogo = (usize, i32, Reverse<String>);

/// Avalia as regras `config_jogo` sobre os destinos resolvidos pelo agente.
/// `refresh_hz` é o refresh inteiro (piso) do monitor, quando conhecido.
/// Toda regra é experimental: sem aceite, vira sugestão (D9).
pub fn avaliar_config_jogo(
    fatos: &FatosConfigJogo,
    prefs: &Preferences,
    refresh_hz: Option<u32>,
    catalogo: &CatalogoConfigJogo,
    aceites: &BTreeSet<String>,
) -> PlanoJogo {
    let mut plano = PlanoJogo::default();
    plano.respeitadas.extend(
        fatos
            .travadas
            .iter()
            .map(|d| (d.clone(), MotivoRespeitadaUsuario::Travada)),
    );
    // Valor-base: o original enquanto a gestão confere; deriva vira escolha (F3-08).
    let mut bases = BTreeMap::new();
    for (destino, estado) in &fatos.destinos {
        let Some(atual) = estado.atual.as_ref() else {
            continue;
        };
        match fatos.gestao.get(destino) {
            Some(g) if *atual == g.esperado => {
                bases.insert(destino, g.antes.clone());
            }
            Some(_) => {
                plano
                    .respeitadas
                    .entry(destino.clone())
                    .or_insert(MotivoRespeitadaUsuario::AlteradaForaDaGestao);
            }
            None => {
                bases.insert(destino, atual.clone());
            }
        }
    }

    let mut vencedoras: BTreeMap<DestinoJogo, (ChaveJogo, MudancaJogo)> = BTreeMap::new();
    for regra in &catalogo.regras {
        let aceita = regra.status == Status::Experimental && aceites.contains(&regra.id);
        if aceita {
            plano
                .estado_experimental
                .insert(regra.id.clone(), EstadoExperimental::Aceita);
        }
        let q = &regra.quando;
        if q.limite_fps_definido
            .is_some_and(|v| v != prefs.limite_fps.is_some())
            || q.refresh_conhecido
                .is_some_and(|v| v != refresh_hz.is_some())
        {
            continue;
        }
        let mut sugestoes = Vec::new();
        for (destino, estado) in &fatos.destinos {
            let setting = destino.setting;
            let Some(acao) = regra.aplicar.get(&setting) else {
                continue;
            };
            if !destino.coerente()
                || !q.jogo.iter().any(|j| j.as_str() == setting.jogo())
                || q.arquivo_local.is_some_and(|v| v == estado.na_nuvem)
                || plano.respeitadas.contains_key(destino)
            {
                continue;
            }
            let Some(def) = catalogo.chave(setting).filter(|d| !d.a_confirmar) else {
                continue;
            };
            let mut descartar = |motivo| {
                plano.descartes.push(DescarteJogo {
                    regra: regra.id.clone(),
                    destino: destino.clone(),
                    motivo,
                });
            };
            let motivo_bloqueio = if estado.conta_ambigua {
                Some(MotivoDescarteJogo::ContaAmbigua)
            } else if estado.na_nuvem {
                Some(MotivoDescarteJogo::NaNuvem)
            } else if estado.somente_leitura {
                Some(MotivoDescarteJogo::SomenteLeitura)
            } else {
                None
            };
            if let Some(m) = motivo_bloqueio {
                descartar(m);
                continue;
            }
            let (Some(atual), Some(base)) = (estado.atual.as_ref(), bases.get(destino)) else {
                descartar(MotivoDescarteJogo::LeituraDesconhecida);
                continue;
            };
            if !def.aceita_leitura(atual) {
                descartar(MotivoDescarteJogo::LeituraDesconhecida);
                continue;
            }
            let valor = match acao {
                AcaoJogo::Booleano(b) => Some(ValorJogo::Booleano(*b)),
                AcaoJogo::Inteiro(n) => def.ajustar(*n).and_then(|n| def.valor_de_numero(n)),
                AcaoJogo::Preferencia(PreferenciaJogo::LimiteFps) => {
                    let Some(pedido) = prefs.limite_fps.map(i64::from) else {
                        continue;
                    };
                    match def.ajustar(pedido) {
                        Some(n) => def.valor_de_numero(n),
                        None => {
                            descartar(MotivoDescarteJogo::ForaDoMenu { pedido });
                            continue;
                        }
                    }
                }
                AcaoJogo::RefreshLimitadoAoMenu => {
                    let (Some(hz), Some(f), Some(padrao)) =
                        (refresh_hz, def.faixa, setting.limite_padrao())
                    else {
                        continue;
                    };
                    let n = i64::from(hz).min(f.max);
                    let existente = match base {
                        ValorJogo::TokenLancamento(Some(0)) => Some(i64::MAX),
                        ValorJogo::TokenLancamento(Some(m)) => Some(i64::from(*m)),
                        _ => None,
                    };
                    if n <= padrao || existente.is_some_and(|m| m >= n) {
                        descartar(MotivoDescarteJogo::NaoSobeOLimite);
                        continue;
                    }
                    match def.ajustar(n) {
                        Some(n) => def.valor_de_numero(n),
                        None => {
                            descartar(MotivoDescarteJogo::ForaDoMenu { pedido: n });
                            continue;
                        }
                    }
                }
            };
            let Some(valor) = valor.filter(|v| def.aceita(v)) else {
                continue;
            };
            let mudanca = MudancaJogo {
                destino: destino.clone(),
                atual: atual.clone(),
                valor,
                regra: regra.id.clone(),
                descricao: regra.descricao.clone(),
                motivo: regra.motivo.clone(),
                evidencia: regra.evidencia.clone(),
                codigo: aceita.then_some(CodigoExplicacao::ExperimentalAceitaPorVoce),
                limite_no_jogo: regra.limite_no_jogo,
            };
            if !aceita {
                sugestoes.push(mudanca);
                continue;
            }
            let chave = (q.quantidade(), regra.prioridade, Reverse(regra.id.clone()));
            if vencedoras
                .get(destino)
                .is_none_or(|(anterior, _)| chave > *anterior)
            {
                vencedoras.insert(destino.clone(), (chave, mudanca));
            }
        }
        if !sugestoes.is_empty() {
            plano
                .estado_experimental
                .insert(regra.id.clone(), EstadoExperimental::Aceitavel);
            plano.experimentais.push(SugestaoJogo {
                regra: regra.id.clone(),
                mudancas: sugestoes,
            });
        }
    }
    plano.mudancas = vencedoras.into_iter().map(|(d, (_, m))| (d, m)).collect();
    plano
}

/// Jogos com limite de FPS no jogo no plano (regra aceita, F3-13) → id da regra.
/// O serviço passa a [`desligar_limite_drs`] só os que o agente não relatou
/// como falha ou adiamento (assim o driver nunca fica sem limitador).
pub fn limites_no_jogo(plano: &PlanoJogo) -> BTreeMap<IdJogo, String> {
    plano
        .mudancas
        .values()
        .filter(|m| m.limite_no_jogo)
        .map(|m| (m.destino.instancia.jogo.clone(), m.regra.clone()))
        .collect()
}

/// F3-13: o perfil DRS de cada jogo com limite no jogo recebe
/// `limite_fps = off` (sem empilhar limitadores). Escolha travada do usuário
/// no perfil do jogo é respeitada. Sem entrada em `limites`, nada muda.
pub fn desligar_limite_drs(
    plan: &mut Plan,
    jogos: &[GameFacts],
    limites: &BTreeMap<IdJogo, String>,
    catalogo: &Catalogo,
    prefs: &Preferences,
) {
    let Some(def) = catalogo.get(SETTING_LIMITE_FPS) else {
        return;
    };
    let Some(&off) = def.valores.get("off") else {
        return;
    };
    for j in jogos {
        let Some(regra) = j.jogo_base.as_ref().and_then(|b| limites.get(b)) else {
            continue;
        };
        let alvo = Alvo::Jogo(j.id.clone());
        if prefs.travada(&alvo, SETTING_LIMITE_FPS) {
            continue;
        }
        let mudanca = SettingChange {
            setting: def.nome.clone(),
            id: def.id,
            valor: off,
            valor_nome: Some("off".into()),
            valor_anterior: None,
            origem: Origem::Preferencia {
                preferencia: PreferenciaGovernante::LimiteFps.to_string(),
                regra: Some(regra.clone()),
            },
            explicacao: "Limite de FPS aplicado no próprio jogo; o limitador do driver fica desligado para não empilhar limitadores.".into(),
        };
        let lista = plan.por_jogo.entry(j.id.clone()).or_default();
        lista.retain(|m| m.setting != SETTING_LIMITE_FPS);
        lista.push(mudanca);
        lista.sort_by(|a, b| a.setting.cmp(&b.setting));
        if let Some(resumo) = plan.resumos.get_mut(&j.id) {
            resumo.herda_do_global.retain(|s| s != SETTING_LIMITE_FPS);
            for a in &mut resumo.aplicadas {
                a.settings.retain(|s| s != SETTING_LIMITE_FPS);
            }
            resumo.aplicadas.retain(|a| !a.settings.is_empty());
            match resumo.aplicadas.iter_mut().find(|a| a.regra == *regra) {
                Some(a) => {
                    a.settings.push(SETTING_LIMITE_FPS.into());
                    a.settings.sort();
                }
                None => resumo.aplicadas.push(RegraAplicada {
                    regra: regra.clone(),
                    settings: vec![SETTING_LIMITE_FPS.into()],
                }),
            }
            resumo.aplicadas.sort_by(|a, b| a.regra.cmp(&b.regra));
        }
    }
}
