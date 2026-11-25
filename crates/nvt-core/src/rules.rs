//! Regras declarativas carregadas de arquivos TOML (pasta `regras/` na raiz
//! do repositório). Formato e semântica: `docs/regras.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::capabilities::{Arquitetura, Capabilities};
use crate::catalogo::{Catalogo, PreferenciaGovernante, VERSAO_SCHEMA};
use crate::facts::{Api, GameFacts};
use crate::preferences::{Preferences, Qualidade, Sincronismo};
use crate::versao::Versao;

/// Maturidade da regra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Entra no plano automático.
    Estavel,
    /// Só aparece como sugestão (`Plan::experimentais`).
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
    // Fatos do jogo (só em escopo `por_jogo`).
    pub jogo_api: Option<Vec<String>>,
    pub jogo_dlss: Option<bool>,
    pub jogo_dlss_min: Option<Versao>,
    pub jogo_reflex: Option<bool>,
    pub jogo_anticheat_kernel: Option<bool>,
    // Preferências.
    pub sincronismo: Option<Vec<String>>,
    pub qualidade: Option<Vec<String>>,
    pub limite_fps_definido: Option<bool>,
}

impl Condicoes {
    /// Número de condições presentes (especificidade da regra).
    pub fn quantidade(&self) -> usize {
        [
            self.arquitetura.is_some(),
            self.tem_tensor_cores.is_some(),
            self.tensor_fp8.is_some(),
            self.frame_gen.is_some(),
            self.mfg.is_some(),
            self.rebar_suportado.is_some(),
            self.rebar_ativo.is_some(),
            self.vram_mb_min.is_some(),
            self.driver_min.is_some(),
            self.refresh_conhecido.is_some(),
            self.jogo_api.is_some(),
            self.jogo_dlss.is_some(),
            self.jogo_dlss_min.is_some(),
            self.jogo_reflex.is_some(),
            self.jogo_anticheat_kernel.is_some(),
            self.sincronismo.is_some(),
            self.qualidade.is_some(),
            self.limite_fps_definido.is_some(),
        ]
        .into_iter()
        .filter(|b| *b)
        .count()
    }

    fn usa_fatos_de_jogo(&self) -> bool {
        self.jogo_api.is_some()
            || self.jogo_dlss.is_some()
            || self.jogo_dlss_min.is_some()
            || self.jogo_reflex.is_some()
            || self.jogo_anticheat_kernel.is_some()
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
        fn lista(l: &Option<Vec<String>>, nome: &str) -> bool {
            l.as_ref().is_none_or(|l| l.iter().any(|n| n == nome))
        }
        fn igual(c: Option<bool>, v: bool) -> bool {
            c.is_none_or(|c| c == v)
        }
        fn igual_opt(c: Option<bool>, v: Option<bool>) -> bool {
            c.is_none_or(|c| v == Some(c))
        }

        let caps_ok = lista(&self.arquitetura, caps.arquitetura.nome_regra())
            && igual(self.tem_tensor_cores, caps.tem_tensor_cores)
            && igual(self.tensor_fp8, caps.tensor_fp8)
            && igual(self.frame_gen, caps.frame_gen)
            && igual(self.mfg, caps.mfg)
            && igual(self.rebar_suportado, caps.rebar_suportado)
            && igual_opt(self.rebar_ativo, caps.rebar_ativo)
            && self.vram_mb_min.is_none_or(|m| caps.vram_mb >= m)
            && self.driver_min.is_none_or(|m| caps.driver_version >= m)
            && igual(self.refresh_conhecido, caps.refresh_hz.is_some());

        let prefs_ok = lista(&self.sincronismo, prefs.sincronismo.nome_regra())
            && lista(&self.qualidade, prefs.qualidade.nome_regra())
            && igual(self.limite_fps_definido, prefs.limite_fps.is_some());

        let jogo_ok = match jogo {
            None => !self.usa_fatos_de_jogo(),
            Some(j) => {
                lista(&self.jogo_api, j.api.nome_regra())
                    && igual(self.jogo_dlss, j.dlss.is_some())
                    && self
                        .jogo_dlss_min
                        .is_none_or(|m| j.dlss.is_some_and(|v| v >= m))
                    && igual_opt(self.jogo_reflex, j.reflex)
                    && igual_opt(self.jogo_anticheat_kernel, j.anticheat_kernel)
            }
        };

        caps_ok && prefs_ok && jogo_ok
    }
}

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
    /// Arquivo de origem (para mensagens).
    pub arquivo: String,
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArquivoRegras {
    versao_schema: u32,
    #[serde(default)]
    regra: Vec<RegraBruta>,
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
            let bruto: ArquivoRegras = match toml::from_str(conteudo) {
                Ok(b) => b,
                Err(e) => {
                    erros.push(ErroRegra::Toml {
                        arquivo: (*arquivo).to_owned(),
                        mensagem: e.to_string(),
                    });
                    continue;
                }
            };
            if bruto.versao_schema != VERSAO_SCHEMA {
                erros.push(ErroRegra::VersaoSchema {
                    arquivo: (*arquivo).to_owned(),
                    encontrada: bruto.versao_schema,
                });
            }
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

fn id_valido(id: &str) -> bool {
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

    // Condições.
    let q = &r.quando;
    validar_lista(
        &id,
        "arquitetura",
        &q.arquitetura,
        &Arquitetura::NOMES_REGRA,
        erros,
    );
    validar_lista(&id, "jogo_api", &q.jogo_api, &Api::NOMES_REGRA, erros);
    validar_lista(
        &id,
        "sincronismo",
        &q.sincronismo,
        &Sincronismo::NOMES_REGRA,
        erros,
    );
    validar_lista(
        &id,
        "qualidade",
        &q.qualidade,
        &Qualidade::NOMES_REGRA,
        erros,
    );
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
        let valor = match &bruto {
            toml::Value::String(nome) if def.valores.contains_key(nome) => {
                Some(ValorAcao::Nome(nome.clone()))
            }
            toml::Value::Integer(n) => match u32::try_from(*n) {
                Ok(v) if def.aceita(v) => Some(ValorAcao::Numero(v)),
                _ => None,
            },
            toml::Value::Table(t) => match (t.len(), t.get("refresh_menos")) {
                (1, Some(toml::Value::Integer(m))) => match u32::try_from(*m) {
                    Ok(m) if def.faixa.is_some() => {
                        if q.refresh_conhecido != Some(true) {
                            erros.push(cond(
                                "`refresh_menos` exige `quando.refresh_conhecido = true`",
                            ));
                        }
                        Some(ValorAcao::RefreshMenos(m))
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        };
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
        arquivo: arquivo.to_owned(),
    })
}
