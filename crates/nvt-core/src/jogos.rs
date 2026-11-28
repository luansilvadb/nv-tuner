//! Base curada de jogos competitivos (`regras/jogos.toml`, spec competitivo
//! §7.3).
//!
//! A base casa um jogo detectado por nome de executável e, quando houver,
//! AppID da Steam; fornece fatos curados (Reflex, limitador nativo,
//! anticheat), a política de configuração da fase 3 e códigos de orientação.
//! O núcleo não gera texto: as orientações são só códigos, traduzidos na UI.

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::config_jogo::{self, CatalogoConfigJogo, DefArquivoJogo, ErroConfigJogo, RegraJogo};
use crate::preferences::{Sincronismo, enum_nomeado};

/// Conteúdo de `regras/jogos.toml`, embutido no binário.
pub const JOGOS_TOML: &str = include_str!("../../../regras/jogos.toml");

/// Versão de schema aceita em `regras/jogos.toml`.
pub const VERSAO_SCHEMA_JOGOS: u32 = 2;

/// Jogos em que editar arquivos ou opções de inicialização é proibido
/// (trava do escopo, §7.3).
pub const JOGOS_SEM_EDICAO: [&str; 4] = ["valorant", "lol", "marvel-rivals", "cod"];

/// Identificador kebab-case de um jogo da base.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdJogo(String);

impl IdJogo {
    pub(crate) fn interno(id: &str) -> Self {
        Self(id.to_owned())
    }

    /// O id como texto.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdJogo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Anticheat declarado na base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anticheat {
    Vac,
    Vanguard,
    Eac,
    Battleye,
    EacBattleye,
    Ricochet,
    Blizzard,
    Netease,
    Proprio,
}

/// O que o app pode fazer com a configuração do jogo (fase 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoliticaConfig {
    /// Editar o arquivo de configuração do usuário.
    Edicao,
    /// Gravar opções de inicialização do launcher.
    OpcoesInicializacao,
    /// Só orientar o usuário.
    Orientar,
    /// Nem editar nem gravar opções: a publisher/anticheat proíbe.
    Proibido,
}

impl PoliticaConfig {
    /// `true` quando a política leva o app a gravar algo do jogo.
    pub const fn grava(self) -> bool {
        matches!(self, Self::Edicao | Self::OpcoesInicializacao)
    }
}

enum_nomeado! {
    /// Código de orientação por jogo (texto em `textos.rs`, fora do núcleo).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Orientacao {
        ReflexOn => "reflex_on",
        ReflexOnBoost => "reflex_on_boost",
        ReduceBuffering => "reduce_buffering",
        LimitadorDoJogo => "limitador_do_jogo",
        SemLimiteComReflex => "sem_limite_com_reflex",
        TelaCheia => "tela_cheia",
        VsyncJogoOff => "vsync_jogo_off",
        VsyncDriverComGsync => "vsync_driver_com_gsync",
        FpsMaxLaunchEa => "fps_max_launch_ea",
        FpsMaxPadraoBaixo => "fps_max_padrao_baixo",
        OneFrameThreadLagOff => "one_frame_thread_lag_off",
        ResolucaoAdaptativaOff => "resolucao_adaptativa_off",
        ModoPerformanceOpcional => "modo_performance_opcional",
        #[serde(rename = "fps_limit_cpu_100")]
        FpsLimitCpu100 => "fps_limit_cpu_100",
    }
}

impl Orientacao {
    /// Todos os códigos conhecidos.
    pub const TODAS: [Self; 14] = Self::VARIANTES;

    /// Código usado no TOML e no IPC.
    pub const fn codigo(self) -> &'static str {
        self.nome_variante()
    }
}

/// Um jogo da base curada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JogoCurado {
    pub id: IdJogo,
    pub nome: String,
    pub executaveis: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam_appid: Option<u32>,
    pub reflex: bool,
    pub limitador_nativo: bool,
    pub anticheat: Anticheat,
    pub anticheat_kernel: bool,
    #[serde(default)]
    pub processos_protegidos: Vec<String>,
    pub config: PoliticaConfig,
    #[serde(default)]
    pub orientacoes: Vec<Orientacao>,
    pub evidencia: String,
}

impl JogoCurado {
    /// `true` se `exe` (nome ou caminho) é um dos executáveis do jogo, sem
    /// diferenciar maiúsculas.
    pub fn tem_executavel(&self, exe: &str) -> bool {
        let nome = nome_arquivo(exe);
        let chave = chave_exe(nome);
        self.executaveis
            .iter()
            .any(|e| e.eq_ignore_ascii_case(nome) || chave_exe(e) == chave)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArquivoJogos {
    versao_schema: u32,
    #[serde(default)]
    jogo: Vec<JogoCurado>,
    #[serde(default)]
    arquivo: Vec<DefArquivoJogo>,
    #[serde(default)]
    regra: Vec<RegraJogo>,
}

/// Um erro de carga ou validação da base de jogos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroJogo {
    /// TOML malformado, campo desconhecido/ausente ou código desconhecido.
    Toml {
        arquivo: String,
        mensagem: String,
    },
    VersaoSchema {
        arquivo: String,
        encontrada: u32,
    },
    IdInvalido {
        id: String,
    },
    IdDuplicado {
        id: String,
    },
    CampoVazio {
        jogo: String,
        campo: &'static str,
    },
    /// O mesmo executável aparece duas vezes (no jogo ou na base).
    ExecutavelDuplicado {
        jogo: String,
        exe: String,
    },
    OrientacaoRepetida {
        jogo: String,
        orientacao: Orientacao,
    },
    /// `config` que grava algo num jogo em que isso é proibido.
    ConfigProibida {
        jogo: String,
        config: PoliticaConfig,
    },
    /// Erro em `[[arquivo]]`/`[[regra]]` (configuração de jogo, fase 3).
    ConfigJogo(ErroConfigJogo),
}

impl fmt::Display for ErroJogo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml { arquivo, mensagem } => write!(f, "{arquivo}: TOML inválido: {mensagem}"),
            Self::VersaoSchema {
                arquivo,
                encontrada,
            } => write!(
                f,
                "{arquivo}: versao_schema {encontrada} não suportada (esperado 1 a {VERSAO_SCHEMA_JOGOS})"
            ),
            Self::IdInvalido { id } => write!(
                f,
                "id de jogo \"{id}\" inválido (use kebab-case: a-z, 0-9, -)"
            ),
            Self::IdDuplicado { id } => write!(f, "id de jogo \"{id}\" duplicado"),
            Self::CampoVazio { jogo, campo } => {
                write!(f, "jogo \"{jogo}\": campo `{campo}` não pode ser vazio")
            }
            Self::ExecutavelDuplicado { jogo, exe } => {
                write!(f, "jogo \"{jogo}\": executável \"{exe}\" repetido na base")
            }
            Self::OrientacaoRepetida { jogo, orientacao } => write!(
                f,
                "jogo \"{jogo}\": orientação \"{}\" repetida",
                orientacao.codigo()
            ),
            Self::ConfigProibida { jogo, config } => write!(
                f,
                "jogo \"{jogo}\": config {config:?} proibida (o jogo só aceita orientar ou proibido)"
            ),
            Self::ConfigJogo(e) => write!(f, "{e}"),
        }
    }
}

/// Lista de erros (a carga reporta todos de uma vez).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrosJogos(pub Vec<ErroJogo>);

impl fmt::Display for ErrosJogos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, e) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{e}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ErrosJogos {}

/// Base curada validada.
#[derive(Clone)]
pub struct BaseJogos {
    jogos: Vec<JogoCurado>,
    config: CatalogoConfigJogo,
    /// Derivado de `jogos`; fora de `Debug` e `PartialEq`.
    indices: Indices,
}

/// Índices montados uma vez na construção da base (ela é imutável).
#[derive(Clone, Default)]
struct Indices {
    /// Exe em [`chave_exe`] -> posições em `jogos`, em ordem crescente.
    por_exe: HashMap<String, Vec<usize>>,
    /// Id -> primeira posição com esse id.
    por_id: HashMap<String, usize>,
    /// AppID da Steam -> posições em `jogos`, em ordem crescente.
    por_appid: HashMap<u32, Vec<usize>>,
    /// Resultado de [`BaseJogos::processos_vigiados`].
    vigiados: BTreeSet<String>,
}

impl Indices {
    fn montar(jogos: &[JogoCurado]) -> Self {
        let mut ind = Self::default();
        for (i, j) in jogos.iter().enumerate() {
            ind.por_id.entry(j.id.as_str().to_owned()).or_insert(i);
            for exe in &j.executaveis {
                let lista = ind.por_exe.entry(chave_exe(exe)).or_default();
                if lista.last() != Some(&i) {
                    lista.push(i);
                }
            }
            if let Some(appid) = j.steam_appid {
                ind.por_appid.entry(appid).or_default().push(i);
            }
            ind.vigiados.extend(
                j.executaveis
                    .iter()
                    .chain(&j.processos_protegidos)
                    .map(|p| p.to_lowercase()),
            );
        }
        ind
    }
}

impl fmt::Debug for BaseJogos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BaseJogos")
            .field("jogos", &self.jogos)
            .field("config", &self.config)
            .finish()
    }
}

impl PartialEq for BaseJogos {
    fn eq(&self, outra: &Self) -> bool {
        self.jogos == outra.jogos && self.config == outra.config
    }
}

impl Eq for BaseJogos {}

impl BaseJogos {
    fn nova(jogos: Vec<JogoCurado>, config: CatalogoConfigJogo) -> Self {
        let indices = Indices::montar(&jogos);
        Self {
            jogos,
            config,
            indices,
        }
    }

    /// Base embutida (`regras/jogos.toml`).
    /// O parse e a validação rodam uma vez por processo; cada chamada devolve um clone.
    pub fn embutida() -> Result<Self, ErrosJogos> {
        static CACHE: std::sync::OnceLock<Result<BaseJogos, ErrosJogos>> =
            std::sync::OnceLock::new();
        CACHE
            .get_or_init(|| Self::from_toml("jogos.toml", JOGOS_TOML))
            .clone()
    }

    /// Carrega e valida a base a partir do texto. Retorna todos os erros.
    pub fn from_toml(arquivo: &str, conteudo: &str) -> Result<Self, ErrosJogos> {
        let bruto: ArquivoJogos = toml::from_str(conteudo).map_err(|e| {
            ErrosJogos(vec![ErroJogo::Toml {
                arquivo: arquivo.to_owned(),
                mensagem: e.to_string(),
            }])
        })?;
        let mut erros = Vec::new();
        if !(1..=VERSAO_SCHEMA_JOGOS).contains(&bruto.versao_schema) {
            erros.push(ErroJogo::VersaoSchema {
                arquivo: arquivo.to_owned(),
                encontrada: bruto.versao_schema,
            });
        }
        let mut ids = BTreeSet::new();
        let mut exes = BTreeSet::new();
        for j in &bruto.jogo {
            validar(j, &mut ids, &mut exes, &mut erros);
        }
        if bruto.versao_schema < 2 && !(bruto.arquivo.is_empty() && bruto.regra.is_empty()) {
            erros.push(ErroJogo::VersaoSchema {
                arquivo: arquivo.to_owned(),
                encontrada: bruto.versao_schema,
            });
        }
        let mut erros_config = Vec::new();
        let config = config_jogo::carregar(
            &bruto.jogo,
            bruto.arquivo,
            bruto.regra,
            bruto.versao_schema == VERSAO_SCHEMA_JOGOS,
            &mut erros_config,
        );
        erros.extend(erros_config.into_iter().map(ErroJogo::ConfigJogo));
        if erros.is_empty() {
            Ok(Self::nova(bruto.jogo, config))
        } else {
            Err(ErrosJogos(erros))
        }
    }

    /// Todos os jogos, na ordem do arquivo.
    pub fn jogos(&self) -> &[JogoCurado] {
        &self.jogos
    }

    /// Catálogo de configuração de jogo (`[[arquivo]]` e `[[regra]]`).
    pub fn config_jogo(&self) -> &CatalogoConfigJogo {
        &self.config
    }

    /// Jogo pelo id.
    pub fn jogo(&self, id: &str) -> Option<&JogoCurado> {
        self.indices.por_id.get(id).map(|&i| &self.jogos[i])
    }

    /// Jogos com o AppID da Steam informado, na ordem da base.
    pub fn jogos_por_appid(&self, steam_appid: u32) -> impl Iterator<Item = &JogoCurado> {
        self.indices
            .por_appid
            .get(&steam_appid)
            .into_iter()
            .flatten()
            .map(|&i| &self.jogos[i])
    }

    /// Jogo da base para um exe detectado (nome ou caminho; sem diferenciar
    /// maiúsculas). Se o jogo curado tem `steam_appid` e o chamador informou
    /// um AppID diferente, não casa.
    pub fn casar(&self, exe: &str, steam_appid: Option<u32>) -> Option<&JogoCurado> {
        let candidatos = self.indices.por_exe.get(&chave_exe(nome_arquivo(exe)))?;
        candidatos
            .iter()
            .map(|&i| &self.jogos[i])
            .find(|j| match (j.steam_appid, steam_appid) {
                (Some(curado), Some(informado)) => curado == informado,
                _ => true,
            })
    }

    /// Executáveis e `processos_protegidos` de todos os jogos, em minúsculas,
    /// para vigiar jogos em execução.
    pub fn processos_vigiados(&self) -> BTreeSet<String> {
        self.indices.vigiados.clone()
    }

    /// Como [`BaseJogos::processos_vigiados`], sem copiar o conjunto.
    pub fn processos_vigiados_ref(&self) -> &BTreeSet<String> {
        &self.indices.vigiados
    }

    /// Base sem nenhum jogo (todo jogo fica fora da base).
    pub fn vazia() -> Self {
        Self::nova(Vec::new(), CatalogoConfigJogo::default())
    }
}

/// Orientações de um jogo filtradas pelas preferências (F1-12), sem
/// repetição e na ordem da base:
///
/// - com `sincronismo = gsync`, `vsync_jogo_off` vira `vsync_driver_com_gsync`
///   (V-Sync desligado no jogo e ligado no driver);
/// - sem G-SYNC, `vsync_driver_com_gsync` sai.
pub fn filtrar_orientacoes(
    orientacoes: &[Orientacao],
    sincronismo: Sincronismo,
) -> Vec<Orientacao> {
    let gsync = sincronismo == Sincronismo::GSync;
    let mut saida = Vec::with_capacity(orientacoes.len());
    for &o in orientacoes {
        let o = match o {
            Orientacao::VsyncJogoOff if gsync => Orientacao::VsyncDriverComGsync,
            Orientacao::VsyncDriverComGsync if !gsync => continue,
            outra => outra,
        };
        if !saida.contains(&o) {
            saida.push(o);
        }
    }
    saida
}

fn validar(
    j: &JogoCurado,
    ids: &mut BTreeSet<String>,
    exes: &mut BTreeSet<String>,
    erros: &mut Vec<ErroJogo>,
) {
    let id = j.id.as_str();
    if !id_valido(id) {
        erros.push(ErroJogo::IdInvalido { id: id.to_owned() });
    }
    if !ids.insert(id.to_owned()) {
        erros.push(ErroJogo::IdDuplicado { id: id.to_owned() });
    }
    let vazio = |campo| ErroJogo::CampoVazio {
        jogo: id.to_owned(),
        campo,
    };
    if j.nome.trim().is_empty() {
        erros.push(vazio("nome"));
    }
    if j.evidencia.trim().is_empty() {
        erros.push(vazio("evidencia"));
    }
    if j.executaveis.is_empty() {
        erros.push(vazio("executaveis"));
    }
    for exe in &j.executaveis {
        if exe.trim().is_empty() {
            erros.push(vazio("executaveis"));
        } else if !exes.insert(exe.to_lowercase()) {
            erros.push(ErroJogo::ExecutavelDuplicado {
                jogo: id.to_owned(),
                exe: exe.clone(),
            });
        }
    }
    if j.processos_protegidos.iter().any(|p| p.trim().is_empty()) {
        erros.push(vazio("processos_protegidos"));
    }
    let mut vistas = BTreeSet::new();
    for &o in &j.orientacoes {
        if !vistas.insert(o) {
            erros.push(ErroJogo::OrientacaoRepetida {
                jogo: id.to_owned(),
                orientacao: o,
            });
        }
    }
    if j.config.grava() && JOGOS_SEM_EDICAO.contains(&id) {
        erros.push(ErroJogo::ConfigProibida {
            jogo: id.to_owned(),
            config: j.config,
        });
    }
}

fn id_valido(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-'))
        && !id.ends_with('-')
        && !id.contains("--")
}

fn nome_arquivo(exe: &str) -> &str {
    exe.rsplit(['\\', '/']).next().unwrap_or(exe)
}

/// Chave de comparação de executáveis sem diferenciar maiúsculas. A regra
/// antiga, `eq_ignore_ascii_case || to_lowercase ==`, equivale a comparar
/// esta chave: se dois nomes diferem só na caixa ASCII, os demais caracteres
/// são idênticos e `to_lowercase` dá o mesmo resultado nos dois.
fn chave_exe(exe: &str) -> String {
    exe.to_lowercase()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn base() -> BaseJogos {
        BaseJogos::embutida().unwrap_or_else(|e| panic!("{e}"))
    }

    fn jogo_toml(id: &str, extra: &str) -> String {
        format!(
            r#"versao_schema = 1

[[jogo]]
id = "{id}"
nome = "Teste"
executaveis = ["teste.exe"]
reflex = true
limitador_nativo = true
anticheat = "vac"
anticheat_kernel = false
config = "orientar"
evidencia = "teste"
{extra}
"#
        )
    }

    fn erros(toml: &str) -> Vec<ErroJogo> {
        BaseJogos::from_toml("teste.toml", toml).unwrap_err().0
    }

    #[test]
    fn carrega_base_real() {
        let b = base();
        let ids: Vec<_> = b.jogos().iter().map(|j| j.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "cs2",
                "valorant",
                "lol",
                "fortnite",
                "apex",
                "overwatch2",
                "r6-siege",
                "cod",
                "rocket-league",
                "pubg",
                "dota2",
                "marvel-rivals",
                "deadlock",
                "the-finals",
            ]
        );
        let cs2 = b.jogo("cs2").unwrap();
        assert_eq!(cs2.steam_appid, Some(730));
        assert_eq!(cs2.anticheat, Anticheat::Vac);
        assert!(!cs2.anticheat_kernel);
        assert_eq!(cs2.config, PoliticaConfig::Edicao);
        assert!(!b.jogo("lol").unwrap().reflex);
        assert!(b.jogos().iter().all(|j| !j.orientacoes.is_empty()));
    }

    #[test]
    fn trava_de_politica_na_base_real() {
        let b = base();
        for id in JOGOS_SEM_EDICAO {
            let j = b.jogo(id).unwrap_or_else(|| panic!("{id} fora da base"));
            assert_eq!(j.config, PoliticaConfig::Proibido, "{id}");
        }
    }

    #[test]
    fn trava_de_politica_rejeita_edicao() {
        for id in JOGOS_SEM_EDICAO {
            for config in ["edicao", "opcoes_inicializacao"] {
                let toml = jogo_toml(id, "")
                    .replace("config = \"orientar\"", &format!("config = \"{config}\""));
                let e = erros(&toml);
                assert!(
                    matches!(&e[..], [ErroJogo::ConfigProibida { jogo, .. }] if jogo == id),
                    "{id}/{config}: {e:?}"
                );
            }
            for config in ["orientar", "proibido"] {
                let toml = jogo_toml(id, "")
                    .replace("config = \"orientar\"", &format!("config = \"{config}\""));
                assert!(BaseJogos::from_toml("t", &toml).is_ok(), "{id}/{config}");
            }
        }
    }

    #[test]
    fn rejeita_id_invalido_e_duplicado() {
        assert!(matches!(
            &erros(&jogo_toml("Jogo_A", ""))[..],
            [ErroJogo::IdInvalido { .. }]
        ));
        let mut dup = jogo_toml("a", "");
        dup.push_str(
            &jogo_toml("a", "")
                .replace("versao_schema = 1", "")
                .replace("teste.exe", "outro.exe"),
        );
        assert!(matches!(&erros(&dup)[..], [ErroJogo::IdDuplicado { .. }]));
    }

    #[test]
    fn rejeita_sem_executavel_e_exe_repetido() {
        let vazio = jogo_toml("a", "").replace(r#"["teste.exe"]"#, "[]");
        assert!(matches!(
            &erros(&vazio)[..],
            [ErroJogo::CampoVazio {
                campo: "executaveis",
                ..
            }]
        ));
        let mut dup = jogo_toml("a", "");
        dup.push_str(
            &jogo_toml("b", "")
                .replace("versao_schema = 1", "")
                .replace("teste.exe", "TESTE.EXE"),
        );
        assert!(matches!(
            &erros(&dup)[..],
            [ErroJogo::ExecutavelDuplicado { .. }]
        ));
    }

    #[test]
    fn rejeita_orientacao_desconhecida_e_repetida() {
        let e = erros(&jogo_toml("a", r#"orientacoes = ["reflex_turbo"]"#));
        assert!(matches!(&e[..], [ErroJogo::Toml { .. }]), "{e:?}");
        let e = erros(&jogo_toml(
            "a",
            r#"orientacoes = ["tela_cheia", "tela_cheia"]"#,
        ));
        assert!(matches!(&e[..], [ErroJogo::OrientacaoRepetida { .. }]));
    }

    #[test]
    fn rejeita_campo_desconhecido_anticheat_e_versao() {
        assert!(matches!(
            &erros(&jogo_toml("a", "campo_novo = 1"))[..],
            [ErroJogo::Toml { .. }]
        ));
        let e = erros(&jogo_toml("a", "").replace("\"vac\"", "\"xigncode\""));
        assert!(matches!(&e[..], [ErroJogo::Toml { .. }]));
        let e = erros(&jogo_toml("a", "").replace("versao_schema = 1", "versao_schema = 3"));
        assert!(matches!(
            &e[..],
            [ErroJogo::VersaoSchema { encontrada: 3, .. }]
        ));
    }

    #[test]
    fn codigos_de_orientacao_batem_com_serde() {
        for o in Orientacao::TODAS {
            let v: Orientacao =
                serde::Deserialize::deserialize(toml::Value::String(o.codigo().into())).unwrap();
            assert_eq!(v, o);
        }
    }

    #[test]
    fn casa_por_exe_sem_diferenciar_maiusculas() {
        let b = base();
        assert_eq!(b.casar("cs2.exe", None).unwrap().id.as_str(), "cs2");
        assert_eq!(b.casar("CS2.EXE", None).unwrap().id.as_str(), "cs2");
        assert_eq!(
            b.casar(r"C:\Riot Games\VALORANT\valorant-win64-shipping.exe", None)
                .unwrap()
                .id
                .as_str(),
            "valorant"
        );
        assert_eq!(
            b.casar("r5apex_dx12.exe", None).unwrap().id.as_str(),
            "apex"
        );
        assert!(b.casar("desconhecido.exe", None).is_none());
    }

    #[test]
    fn appid_divergente_nao_casa() {
        let b = base();
        assert_eq!(b.casar("cs2.exe", Some(730)).unwrap().id.as_str(), "cs2");
        assert!(b.casar("cs2.exe", Some(570)).is_none());
        // Jogo curado sem AppID: o AppID informado não impede o casamento.
        assert_eq!(
            b.casar("Overwatch.exe", Some(12345)).unwrap().id.as_str(),
            "overwatch2"
        );
    }

    #[test]
    fn processos_vigiados_juntam_exes_e_protegidos() {
        let toml = jogo_toml("a", r#"processos_protegidos = ["Protegido.exe"]"#);
        let b = BaseJogos::from_toml("t", &toml).unwrap();
        let v = b.processos_vigiados();
        assert_eq!(
            v.into_iter().collect::<Vec<_>>(),
            ["protegido.exe", "teste.exe"]
        );
        let real = base().processos_vigiados();
        assert!(real.contains("cs2.exe"));
        assert!(real.contains("league of legends.exe"));
        assert!(!real.contains("vgc.exe"));
    }
}
