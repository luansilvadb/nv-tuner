//! Regras declarativas carregadas de arquivos TOML (pasta `regras/` na raiz
//! do repositório).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::capabilities::{Arquitetura, Capabilities};
use crate::carga;
use crate::catalogo::{Catalogo, DefSetting, PreferenciaGovernante, VERSAO_SCHEMA};
use crate::explicacao::{
    DLSS_AUSENTE, Falha, MotivoDeclarado, MotivoNaoAplicada, TipoMotivo, motivo_padrao,
};
use crate::facts::{Api, GameFacts};
use crate::preferences::{Preferences, Qualidade, Sincronismo};
use crate::versao::Versao;

/// Maturidade da regra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Entra no plano automático.
    Estavel,
    /// Só concorre no plano após aceite.
    Experimental,
}

/// Onde a regra grava.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Escopo {
    /// Perfil global (base) do driver.
    Global,
    /// Perfil de cada jogo cujos fatos satisfazem as condições.
    PorJogo,
}

/// Impacto visual de uma regra, comparado com o teto de qualidade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactoVisual {
    Nenhum,
    Leve,
    Forte,
}

/// Valor de uma ação em `aplicar`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValorAcao {
    /// Valor nomeado do catálogo (`"force_off"`).
    Nome(String),
    /// Número dentro da faixa (ou igual a um valor nomeado).
    Numero(u32),
    /// Refresh do monitor menos uma margem (`{ refresh_menos = 3 }`);
    /// exige `quando.refresh_conhecido = true`.
    RefreshMenos(u32),
    /// Limite para VRR (`{ limite_vrr = true }`):
    /// `min(Hz − 3, floor(Hz − Hz²/3600))` ([`crate::evaluator::limite_vrr`]);
    /// exige `quando.refresh_conhecido = true` e `quando.vrr_ativo = true`.
    LimiteVrr,
}

/// Condições (`quando`). Todas as presentes precisam valer (E lógico).
/// Condições sobre fatos desconhecidos (`None`) não casam.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condicoes {
    // Capacidades.
    pub arquitetura: Option<Vec<String>>,
    pub tem_tensor_cores: Option<bool>,
    pub tensor_fp8: Option<bool>,
    pub frame_gen: Option<bool>,
    pub mfg: Option<bool>,
    pub rebar_suportado: Option<bool>,
    pub rebar_ativo: Option<bool>,
    pub vram_mb_min: Option<u32>,
    pub driver_min: Option<Versao>,
    pub refresh_conhecido: Option<bool>,
    /// VRR possível no monitor principal; não casa se desconhecido.
    pub vrr_possivel: Option<bool>,
    /// VRR ligado no painel; não casa se desconhecido.
    pub vrr_ativo: Option<bool>,
    // Fatos do jogo (só em escopo `por_jogo`).
    pub jogo_api: Option<Vec<String>>,
    pub jogo_dlss: Option<bool>,
    pub jogo_dlss_min: Option<Versao>,
    pub jogo_reflex: Option<bool>,
    pub jogo_anticheat_kernel: Option<bool>,
    /// Jogo da base curada.
    pub jogo_competitivo: Option<bool>,
    /// Limitador de FPS nativo (base curada); não casa se desconhecido.
    pub jogo_limitador_nativo: Option<bool>,
    // Preferências.
    pub sincronismo: Option<Vec<String>>,
    pub qualidade: Option<Vec<String>>,
    pub limite_fps_definido: Option<bool>,
}

/// Fatos contra os quais as condições são avaliadas. As condições `jogo_*`
/// só são avaliadas com `jogo` presente.
struct Fatos<'a> {
    caps: &'a Capabilities,
    jogo: Option<&'a GameFacts>,
    prefs: &'a Preferences,
}

/// Forma de uma condição e como ler o fato que ela compara.
enum Teste<'a> {
    /// Igualdade com fato booleano; fato desconhecido (`None`) não casa.
    Booleano(Option<bool>, fn(&Fatos<'_>) -> Option<bool>),
    /// O fato precisa estar na lista. O fato devolve `(nome, conhecido)`.
    Lista {
        lista: &'a Option<Vec<String>>,
        aceitos: &'static [&'static str],
        fato: fn(&Fatos<'_>) -> (&'static str, bool),
    },
    /// Mínimo numérico (fato sempre conhecido).
    MinimoU32(Option<u32>, fn(&Fatos<'_>) -> u32),
    /// Versão mínima (fato sempre conhecido).
    MinimoVersao(Option<Versao>, fn(&Fatos<'_>) -> Versao),
    /// Versão mínima do DLSS do jogo (ausência vira [`DLSS_AUSENTE`]).
    DlssMin(Option<Versao>),
}

impl Teste<'_> {
    fn presente(&self) -> bool {
        match self {
            Self::Booleano(c, _) => c.is_some(),
            Self::Lista { lista, .. } => lista.is_some(),
            Self::MinimoU32(c, _) => c.is_some(),
            Self::MinimoVersao(c, _) | Self::DlssMin(c) => c.is_some(),
        }
    }

    /// `(esperado, encontrado)` se a condição está presente e não casa.
    fn falha(&self, f: &Fatos<'_>) -> Option<(String, Option<String>)> {
        match self {
            Self::Booleano(c, fato) => {
                let c = (*c)?;
                let v = fato(f);
                (v != Some(c)).then(|| (c.to_string(), v.map(|v| v.to_string())))
            }
            Self::Lista { lista, fato, .. } => {
                let l = lista.as_ref()?;
                let (nome, conhecido) = fato(f);
                (!l.iter().any(|n| n == nome))
                    .then(|| (l.join(", "), conhecido.then(|| nome.to_owned())))
            }
            Self::MinimoU32(m, fato) => {
                let m = (*m)?;
                let v = fato(f);
                (v < m).then(|| (m.to_string(), Some(v.to_string())))
            }
            Self::MinimoVersao(m, fato) => {
                let m = (*m)?;
                let v = fato(f);
                (v < m).then(|| (m.to_string(), Some(v.to_string())))
            }
            Self::DlssMin(m) => {
                let m = (*m)?;
                let dlss = f.jogo.and_then(|j| j.dlss);
                (!dlss.is_some_and(|v| v >= m)).then(|| {
                    let achado = dlss.map_or_else(|| DLSS_AUSENTE.to_owned(), |v| v.to_string());
                    (m.to_string(), Some(achado))
                })
            }
        }
    }
}

/// Uma entrada do registro de condições ([`Condicoes::registro`]).
struct Condicao<'a> {
    nome: &'static str,
    /// Usa fatos do jogo (só vale em escopo `por_jogo`).
    de_jogo: bool,
    teste: Teste<'a>,
}

impl Condicoes {
    /// Registro único das condições, na ordem usada para explicar as falhas:
    /// fatos do jogo, Tensor Cores, arquitetura, demais capacidades e
    /// preferências. A desestruturação sem `..` faz uma condição nova que
    /// não entrar aqui virar erro de compilação.
    fn registro(&self) -> [Condicao<'_>; 22] {
        use Teste::{Booleano, DlssMin, Lista, MinimoU32, MinimoVersao};
        let Self {
            arquitetura,
            tem_tensor_cores,
            tensor_fp8,
            frame_gen,
            mfg,
            rebar_suportado,
            rebar_ativo,
            vram_mb_min,
            driver_min,
            refresh_conhecido,
            vrr_possivel,
            vrr_ativo,
            jogo_api,
            jogo_dlss,
            jogo_dlss_min,
            jogo_reflex,
            jogo_anticheat_kernel,
            jogo_competitivo,
            jogo_limitador_nativo,
            sincronismo,
            qualidade,
            limite_fps_definido,
        } = self;
        fn jogo<'a>(nome: &'static str, teste: Teste<'a>) -> Condicao<'a> {
            Condicao {
                nome,
                de_jogo: true,
                teste,
            }
        }
        fn geral<'a>(nome: &'static str, teste: Teste<'a>) -> Condicao<'a> {
            Condicao {
                nome,
                de_jogo: false,
                teste,
            }
        }
        [
            jogo(
                "jogo_dlss",
                Booleano(*jogo_dlss, |f| f.jogo.map(|j| j.dlss.is_some())),
            ),
            jogo("jogo_dlss_min", DlssMin(*jogo_dlss_min)),
            jogo(
                "jogo_api",
                Lista {
                    lista: jogo_api,
                    aceitos: &Api::NOMES_REGRA,
                    fato: |f| {
                        let api = f.jogo.map_or(Api::Desconhecida, |j| j.api);
                        (api.nome_regra(), api != Api::Desconhecida)
                    },
                },
            ),
            jogo("jogo_reflex", Booleano(*jogo_reflex, |f| f.jogo?.reflex)),
            jogo(
                "jogo_anticheat_kernel",
                Booleano(*jogo_anticheat_kernel, |f| f.jogo?.anticheat_kernel),
            ),
            jogo(
                "jogo_competitivo",
                Booleano(*jogo_competitivo, |f| f.jogo.map(|j| j.competitivo)),
            ),
            jogo(
                "jogo_limitador_nativo",
                Booleano(*jogo_limitador_nativo, |f| f.jogo?.limitador_nativo),
            ),
            geral(
                "tem_tensor_cores",
                Booleano(*tem_tensor_cores, |f| Some(f.caps.tem_tensor_cores)),
            ),
            geral(
                "tensor_fp8",
                Booleano(*tensor_fp8, |f| Some(f.caps.tensor_fp8)),
            ),
            geral(
                "arquitetura",
                Lista {
                    lista: arquitetura,
                    aceitos: &Arquitetura::NOMES_REGRA,
                    fato: |f| (f.caps.arquitetura.nome_regra(), true),
                },
            ),
            geral(
                "frame_gen",
                Booleano(*frame_gen, |f| Some(f.caps.frame_gen)),
            ),
            geral("mfg", Booleano(*mfg, |f| Some(f.caps.mfg))),
            geral(
                "rebar_suportado",
                Booleano(*rebar_suportado, |f| Some(f.caps.rebar_suportado)),
            ),
            geral(
                "rebar_ativo",
                Booleano(*rebar_ativo, |f| f.caps.rebar_ativo),
            ),
            geral("vram_mb_min", MinimoU32(*vram_mb_min, |f| f.caps.vram_mb)),
            geral(
                "driver_min",
                MinimoVersao(*driver_min, |f| f.caps.driver_version),
            ),
            geral(
                "refresh_conhecido",
                Booleano(*refresh_conhecido, |f| Some(f.caps.refresh_hz().is_some())),
            ),
            geral(
                "vrr_possivel",
                Booleano(*vrr_possivel, |f| f.caps.vrr.possivel),
            ),
            geral("vrr_ativo", Booleano(*vrr_ativo, |f| f.caps.vrr.ativo)),
            geral(
                "sincronismo",
                Lista {
                    lista: sincronismo,
                    aceitos: &Sincronismo::NOMES_REGRA,
                    fato: |f| (f.prefs.sincronismo.nome_regra(), true),
                },
            ),
            geral(
                "qualidade",
                Lista {
                    lista: qualidade,
                    aceitos: &Qualidade::NOMES_REGRA,
                    fato: |f| (f.prefs.qualidade.nome_regra(), true),
                },
            ),
            geral(
                "limite_fps_definido",
                Booleano(*limite_fps_definido, |f| Some(f.prefs.limite_fps.is_some())),
            ),
        ]
    }

    /// Número de condições presentes (especificidade da regra).
    pub fn quantidade(&self) -> usize {
        self.registro()
            .iter()
            .filter(|c| c.teste.presente())
            .count()
    }

    pub(crate) fn usa_fatos_de_jogo(&self) -> bool {
        self.registro()
            .iter()
            .any(|c| c.de_jogo && c.teste.presente())
    }

    /// Verdadeiro se a regra está condicionada à preferência.
    pub fn condiciona(&self, pref: PreferenciaGovernante) -> bool {
        match pref {
            PreferenciaGovernante::Sincronismo => self.sincronismo.is_some(),
            PreferenciaGovernante::LimiteFps => self.limite_fps_definido.is_some(),
        }
    }

    /// Avalia as condições. `jogo` é `None` para regras globais.
    pub fn casa(&self, caps: &Capabilities, jogo: Option<&GameFacts>, prefs: &Preferences) -> bool {
        if jogo.is_none() && self.usa_fatos_de_jogo() {
            return false;
        }
        self.falhas(caps, jogo, prefs).is_empty()
    }

    /// Condições que não casam, na ordem usada para explicar: fatos do jogo,
    /// Tensor Cores, arquitetura, demais capacidades e preferências. Vazia =
    /// a regra casa. Com `jogo = None`, condições `jogo_*` são ignoradas.
    /// Fato desconhecido vira `encontrado = None` (nunca casa).
    pub fn falhas(
        &self,
        caps: &Capabilities,
        jogo: Option<&GameFacts>,
        prefs: &Preferences,
    ) -> Vec<Falha> {
        let fatos = Fatos { caps, jogo, prefs };
        self.registro()
            .iter()
            .filter(|c| !c.de_jogo || jogo.is_some())
            .filter_map(|c| {
                let (esperado, encontrado) = c.teste.falha(&fatos)?;
                Some(Falha {
                    condicao: c.nome,
                    esperado,
                    encontrado,
                })
            })
            .collect()
    }

    /// Verdadeiro se a condição `nome` está presente.
    pub fn presente(&self, nome: &str) -> bool {
        self.registro()
            .iter()
            .any(|c| c.nome == nome && c.teste.presente())
    }

    /// Itens da condição de lista `nome` e os valores que ela aceita
    /// (`None` se `nome` não é condição de lista).
    fn lista_de(&self, nome: &str) -> Option<(&[String], &'static [&'static str])> {
        self.listas()
            .find(|(n, _, _)| *n == nome)
            .map(|(_, l, aceitos)| (l.as_deref().unwrap_or(&[]), aceitos))
    }

    /// Condições de lista do registro, na ordem dele: `(nome, lista, aceitos)`.
    fn listas(&self) -> impl Iterator<Item = ListaCondicao<'_>> {
        self.registro().into_iter().filter_map(|c| match c.teste {
            Teste::Lista { lista, aceitos, .. } => Some((c.nome, lista, aceitos)),
            _ => None,
        })
    }
}

/// `(nome, lista, aceitos)` de uma condição de lista.
type ListaCondicao<'a> = (
    &'static str,
    &'a Option<Vec<String>>,
    &'static [&'static str],
);

/// Regra validada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub descricao: String,
    pub status: Status,
    pub escopo: Escopo,
    pub quando: Condicoes,
    /// Nome simbólico da setting → valor.
    pub aplicar: BTreeMap<String, ValorAcao>,
    pub impacto_visual: ImpactoVisual,
    pub prioridade: i32,
    pub motivo: String,
    pub evidencia: String,
    /// Motivo a mostrar quando uma condição não casa
    /// (`motivo_quando_nao_aplica`), por nome de condição.
    pub motivos: BTreeMap<String, MotivoDeclarado>,
    /// Arquivo de origem (para mensagens).
    pub arquivo: String,
}

impl Rule {
    /// Por que a regra não casou, a partir das falhas de
    /// [`Condicoes::falhas`] (na ordem delas): a primeira falha com motivo
    /// declarado em `motivo_quando_nao_aplica` ou com motivo padrão
    /// ([`motivo_padrao`]) vence; sem nenhuma, a primeira falha vira
    /// `FatoDesconhecido` ou `CondicaoNaoAtendida`.
    pub fn motivo_nao_casou(
        &self,
        falhas: &[Falha],
        arquitetura: Arquitetura,
    ) -> MotivoNaoAplicada {
        let especifico = falhas.iter().find_map(|f| {
            let declarado = self
                .motivos
                .get(f.condicao)
                .and_then(|m| m.para(f.encontrado.as_deref()));
            declarado.or_else(|| {
                // O declarado substitui o padrão da condição.
                (!self.motivos.contains_key(f.condicao))
                    .then(|| motivo_padrao(f))
                    .flatten()
            })
        });
        match (especifico, falhas.first()) {
            (Some(t), _) => MotivoNaoAplicada::de_tipo(t, arquitetura),
            (None, Some(f)) => MotivoNaoAplicada::generico(f),
            (None, None) => MotivoNaoAplicada::CondicaoNaoAtendida {
                condicao: String::new(),
                esperado: String::new(),
                encontrado: String::new(),
            },
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegraBruta {
    id: String,
    descricao: String,
    status: Status,
    escopo: Escopo,
    #[serde(default)]
    quando: Condicoes,
    aplicar: BTreeMap<String, toml::Value>,
    impacto_visual: ImpactoVisual,
    #[serde(default)]
    prioridade: i32,
    motivo: String,
    evidencia: String,
    #[serde(default)]
    motivo_quando_nao_aplica: BTreeMap<String, toml::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArquivoRegras {
    versao_schema: u32,
    #[serde(default)]
    regra: Vec<RegraBruta>,
}

impl carga::Versionado for ArquivoRegras {
    const VERSAO_SCHEMA: u32 = VERSAO_SCHEMA;

    fn versao_schema(&self) -> u32 {
        self.versao_schema
    }
}

/// Um erro de carga ou validação.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroRegra {
    /// TOML malformado ou campo desconhecido/ausente.
    Toml {
        arquivo: String,
        mensagem: String,
    },
    VersaoSchema {
        arquivo: String,
        encontrada: u32,
    },
    /// Problema numa definição do catálogo.
    Catalogo {
        arquivo: String,
        setting: String,
        mensagem: String,
    },
    IdInvalido {
        arquivo: String,
        id: String,
    },
    RegraDuplicada {
        arquivo: String,
        id: String,
    },
    CampoVazio {
        regra: String,
        campo: &'static str,
    },
    SettingDesconhecida {
        regra: String,
        setting: String,
    },
    ValorInvalido {
        regra: String,
        setting: String,
        valor: String,
        aceitos: String,
    },
    CondicaoInvalida {
        regra: String,
        mensagem: String,
    },
    /// Setting governada por preferência em regra não condicionada a ela.
    SettingGovernada {
        regra: String,
        setting: String,
        preferencias: String,
    },
    /// `motivo_quando_nao_aplica` inválido.
    MotivoInvalido {
        regra: String,
        mensagem: String,
    },
}

impl fmt::Display for ErroRegra {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml { arquivo, mensagem } => write!(f, "{arquivo}: TOML inválido: {mensagem}"),
            Self::VersaoSchema {
                arquivo,
                encontrada,
            } => write!(
                f,
                "{arquivo}: versao_schema {encontrada} não suportada (esperado {VERSAO_SCHEMA})"
            ),
            Self::Catalogo {
                arquivo,
                setting,
                mensagem,
            } => write!(f, "{arquivo}: setting \"{setting}\": {mensagem}"),
            Self::IdInvalido { arquivo, id } => write!(
                f,
                "{arquivo}: id de regra \"{id}\" inválido (use kebab-case: a-z, 0-9, -)"
            ),
            Self::RegraDuplicada { arquivo, id } => {
                write!(f, "{arquivo}: id de regra \"{id}\" duplicado")
            }
            Self::CampoVazio { regra, campo } => {
                write!(f, "regra \"{regra}\": campo `{campo}` não pode ser vazio")
            }
            Self::SettingDesconhecida { regra, setting } => write!(
                f,
                "regra \"{regra}\": setting \"{setting}\" não existe no catálogo (regras/settings.toml)"
            ),
            Self::ValorInvalido {
                regra,
                setting,
                valor,
                aceitos,
            } => write!(
                f,
                "regra \"{regra}\": valor {valor} inválido para \"{setting}\"; aceitos: {aceitos}"
            ),
            Self::CondicaoInvalida { regra, mensagem } => {
                write!(f, "regra \"{regra}\": condição inválida: {mensagem}")
            }
            Self::SettingGovernada {
                regra,
                setting,
                preferencias,
            } => write!(
                f,
                "regra \"{regra}\": \"{setting}\" é governada pela preferência {preferencias}; \
                 a regra precisa ser condicionada a ela em `quando`"
            ),
            Self::MotivoInvalido { regra, mensagem } => {
                write!(
                    f,
                    "regra \"{regra}\": motivo_quando_nao_aplica inválido: {mensagem}"
                )
            }
        }
    }
}

/// Lista de erros (a carga reporta todos de uma vez).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrosRegras(pub Vec<ErroRegra>);

impl fmt::Display for ErrosRegras {
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

impl std::error::Error for ErrosRegras {}

/// Catálogo + regras validadas contra ele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConjuntoRegras {
    pub catalogo: Catalogo,
    pub regras: Vec<Rule>,
}

impl ConjuntoRegras {
    /// Carrega e valida arquivos de regras `(nome, conteúdo)` contra o
    /// catálogo. Retorna todos os erros encontrados.
    pub fn carregar(catalogo: Catalogo, arquivos: &[(&str, &str)]) -> Result<Self, ErrosRegras> {
        let mut erros = Vec::new();
        let mut regras = Vec::new();
        let mut ids = BTreeSet::new();
        for (arquivo, conteudo) in arquivos {
            let bruto: ArquivoRegras = match carga::ler(arquivo, conteudo) {
                Ok(lido) => {
                    if let Some(encontrada) = lido.versao_errada {
                        erros.push(ErroRegra::VersaoSchema {
                            arquivo: (*arquivo).to_owned(),
                            encontrada,
                        });
                    }
                    lido.dados
                }
                Err(e) => {
                    erros.push(e);
                    continue;
                }
            };
            for r in bruto.regra {
                if !ids.insert(r.id.clone()) {
                    erros.push(ErroRegra::RegraDuplicada {
                        arquivo: (*arquivo).to_owned(),
                        id: r.id.clone(),
                    });
                    continue;
                }
                if let Some(regra) = validar(&catalogo, arquivo, r, &mut erros) {
                    regras.push(regra);
                }
            }
        }
        if erros.is_empty() {
            Ok(Self { catalogo, regras })
        } else {
            Err(ErrosRegras(erros))
        }
    }

    /// Atalho: catálogo e regras a partir do texto.
    pub fn from_toml(settings_toml: &str, arquivos: &[(&str, &str)]) -> Result<Self, ErrosRegras> {
        let catalogo = Catalogo::from_toml("settings.toml", settings_toml)?;
        Self::carregar(catalogo, arquivos)
    }
}

pub(crate) fn id_valido(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-'))
        && !id.ends_with('-')
}

fn validar_lista(
    regra: &str,
    campo: &str,
    lista: &Option<Vec<String>>,
    aceitos: &[&str],
    erros: &mut Vec<ErroRegra>,
) {
    let Some(lista) = lista else { return };
    let cond = |mensagem: String| ErroRegra::CondicaoInvalida {
        regra: regra.to_owned(),
        mensagem,
    };
    if lista.is_empty() {
        erros.push(cond(format!("`{campo}` não pode ser lista vazia")));
    }
    let mut vistos = BTreeSet::new();
    for nome in lista {
        if !aceitos.contains(&nome.as_str()) {
            erros.push(cond(format!(
                "`{campo}` = \"{nome}\" desconhecido; aceitos: {}",
                aceitos.join(", ")
            )));
        }
        if !vistos.insert(nome) {
            erros.push(cond(format!("`{campo}` repete \"{nome}\"")));
        }
    }
}

fn validar(
    catalogo: &Catalogo,
    arquivo: &str,
    r: RegraBruta,
    erros: &mut Vec<ErroRegra>,
) -> Option<Rule> {
    let inicio = erros.len();
    let id = r.id.clone();
    if !id_valido(&id) {
        erros.push(ErroRegra::IdInvalido {
            arquivo: arquivo.to_owned(),
            id: id.clone(),
        });
    }
    for (campo, valor) in [
        ("descricao", &r.descricao),
        ("motivo", &r.motivo),
        ("evidencia", &r.evidencia),
    ] {
        if valor.trim().is_empty() {
            erros.push(ErroRegra::CampoVazio {
                regra: id.clone(),
                campo,
            });
        }
    }
    if r.aplicar.is_empty() {
        erros.push(ErroRegra::CampoVazio {
            regra: id.clone(),
            campo: "aplicar",
        });
    }

    // Condições. As listas seguem o registro, mas `arquitetura` vem
    // primeiro para manter a ordem histórica das mensagens de erro.
    let q = &r.quando;
    let mut listas: Vec<_> = q.listas().collect();
    listas.sort_by_key(|(nome, _, _)| *nome != "arquitetura");
    for (campo, lista, aceitos) in listas {
        validar_lista(&id, campo, lista, aceitos, erros);
    }
    let cond = |mensagem: &str| ErroRegra::CondicaoInvalida {
        regra: id.clone(),
        mensagem: mensagem.to_owned(),
    };
    if r.escopo == Escopo::Global && q.usa_fatos_de_jogo() {
        erros.push(cond(
            "condições `jogo_*` só valem em regras com escopo = \"por_jogo\"",
        ));
    }
    if q.jogo_dlss == Some(false) && q.jogo_dlss_min.is_some() {
        erros.push(cond(
            "`jogo_dlss = false` contradiz `jogo_dlss_min` (a regra nunca casaria)",
        ));
    }

    let motivos = validar_motivos(&id, q, r.motivo_quando_nao_aplica, erros);

    // Ações.
    let mut aplicar = BTreeMap::new();
    for (setting, bruto) in r.aplicar {
        let Some(def) = catalogo.get(&setting) else {
            erros.push(ErroRegra::SettingDesconhecida {
                regra: id.clone(),
                setting,
            });
            continue;
        };
        if !def.governada_por.is_empty() && !def.governada_por.iter().any(|p| q.condiciona(*p)) {
            let preferencias = def
                .governada_por
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ou ");
            erros.push(ErroRegra::SettingGovernada {
                regra: id.clone(),
                setting: setting.clone(),
                preferencias,
            });
        }
        let invalido = |valor: String| ErroRegra::ValorInvalido {
            regra: id.clone(),
            setting: setting.clone(),
            valor,
            aceitos: def.valores_aceitos(),
        };
        let valor = converter_valor(def, &bruto);
        if let Some((mensagem, atendido)) = valor.as_ref().and_then(ValorAcao::requisito)
            && !atendido(q)
        {
            erros.push(cond(mensagem));
        }
        match valor {
            Some(v) => {
                aplicar.insert(setting, v);
            }
            None => erros.push(invalido(bruto.to_string())),
        }
    }

    (erros.len() == inicio).then(|| Rule {
        id,
        descricao: r.descricao,
        status: r.status,
        escopo: r.escopo,
        quando: r.quando,
        aplicar,
        impacto_visual: r.impacto_visual,
        prioridade: r.prioridade,
        motivo: r.motivo,
        evidencia: r.evidencia,
        motivos,
        arquivo: arquivo.to_owned(),
    })
}

/// Converte o valor bruto de `aplicar` para a setting `def`; `None` se
/// o valor não é aceito. Valores dinâmicos (tabela de uma chave) exigem
/// setting com `faixa`.
fn converter_valor(def: &DefSetting, bruto: &toml::Value) -> Option<ValorAcao> {
    match bruto {
        toml::Value::String(nome) if def.valores.contains_key(nome) => {
            Some(ValorAcao::Nome(nome.clone()))
        }
        toml::Value::Integer(n) => u32::try_from(*n)
            .ok()
            .filter(|v| def.aceita(*v))
            .map(ValorAcao::Numero),
        toml::Value::Table(t) if def.faixa.is_some() && t.len() == 1 => match t.iter().next()? {
            (k, toml::Value::Boolean(true)) if k == "limite_vrr" => Some(ValorAcao::LimiteVrr),
            (k, toml::Value::Integer(m)) if k == "refresh_menos" => {
                u32::try_from(*m).ok().map(ValorAcao::RefreshMenos)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Mensagem de erro e teste de um pré-requisito em `quando`.
type Requisito = (&'static str, fn(&Condicoes) -> bool);

impl ValorAcao {
    /// Pré-requisito em `quando` de um valor dinâmico: mensagem de erro e
    /// teste. `None` para valores fixos.
    fn requisito(&self) -> Option<Requisito> {
        match self {
            Self::Nome(_) | Self::Numero(_) => None,
            Self::RefreshMenos(_) => Some((
                "`refresh_menos` exige `quando.refresh_conhecido = true`",
                |q| q.refresh_conhecido == Some(true),
            )),
            Self::LimiteVrr => Some((
                "`limite_vrr` exige `quando.refresh_conhecido = true` e `quando.vrr_ativo = true`",
                |q| q.refresh_conhecido == Some(true) && q.vrr_ativo == Some(true),
            )),
        }
    }
}

/// Valida `motivo_quando_nao_aplica`: cada chave é uma condição presente em
/// `quando`; o valor é um motivo de [`TipoMotivo::NOMES`] ou, para condições
/// de lista, uma tabela `valor encontrado = motivo` com valores que a lista
/// aceita mas não contém (senão a condição nunca falharia com eles).
fn validar_motivos(
    id: &str,
    q: &Condicoes,
    brutos: BTreeMap<String, toml::Value>,
    erros: &mut Vec<ErroRegra>,
) -> BTreeMap<String, MotivoDeclarado> {
    let erro = |mensagem: String| ErroRegra::MotivoInvalido {
        regra: id.to_owned(),
        mensagem,
    };
    let tipo = |condicao: &str, v: &toml::Value| -> Result<TipoMotivo, String> {
        v.as_str().and_then(TipoMotivo::de_nome).ok_or_else(|| {
            format!(
                "`{condicao}` = {v}: motivo desconhecido; aceitos: {}",
                TipoMotivo::NOMES.join(", ")
            )
        })
    };
    let mut saida = BTreeMap::new();
    for (condicao, valor) in brutos {
        if !q.presente(&condicao) {
            erros.push(erro(format!(
                "`{condicao}` não é uma condição presente em `quando`"
            )));
            continue;
        }
        let declarado = match &valor {
            toml::Value::Table(t) => {
                let Some((lista, aceitos)) = q.lista_de(&condicao) else {
                    erros.push(erro(format!(
                        "`{condicao}` não é condição de lista; use um motivo só (texto)"
                    )));
                    continue;
                };
                if t.is_empty() {
                    erros.push(erro(format!("`{condicao}` não pode ser tabela vazia")));
                    continue;
                }
                let mut m = BTreeMap::new();
                for (v, motivo) in t {
                    if !aceitos.contains(&v.as_str()) {
                        erros.push(erro(format!(
                            "`{condicao}.{v}`: valor desconhecido; aceitos: {}",
                            aceitos.join(", ")
                        )));
                    } else if lista.contains(v) {
                        erros.push(erro(format!(
                            "`{condicao}.{v}`: \"{v}\" está na lista da condição (ela nunca falharia com esse valor)"
                        )));
                    }
                    match tipo(&format!("{condicao}.{v}"), motivo) {
                        Ok(t) => {
                            m.insert(v.clone(), t);
                        }
                        Err(e) => erros.push(erro(e)),
                    }
                }
                MotivoDeclarado::PorValor(m)
            }
            outro => match tipo(&condicao, outro) {
                Ok(t) => MotivoDeclarado::Sempre(t),
                Err(e) => {
                    erros.push(erro(e));
                    continue;
                }
            },
        };
        saida.insert(condicao, declarado);
    }
    saida
}
