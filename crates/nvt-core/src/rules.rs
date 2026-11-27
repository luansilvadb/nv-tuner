//! Regras declarativas carregadas de arquivos TOML (pasta `regras/` na raiz
//! do repositório). Formato e semântica: `docs/regras.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::capabilities::{Arquitetura, Capabilities};
use crate::catalogo::{Catalogo, PreferenciaGovernante, VERSAO_SCHEMA};
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
            self.vrr_possivel.is_some(),
            self.vrr_ativo.is_some(),
            self.jogo_api.is_some(),
            self.jogo_dlss.is_some(),
            self.jogo_dlss_min.is_some(),
            self.jogo_reflex.is_some(),
            self.jogo_anticheat_kernel.is_some(),
            self.jogo_competitivo.is_some(),
            self.jogo_limitador_nativo.is_some(),
            self.sincronismo.is_some(),
            self.qualidade.is_some(),
            self.limite_fps_definido.is_some(),
        ]
        .into_iter()
        .filter(|b| *b)
        .count()
    }

    pub(crate) fn usa_fatos_de_jogo(&self) -> bool {
        self.jogo_api.is_some()
            || self.jogo_dlss.is_some()
            || self.jogo_dlss_min.is_some()
            || self.jogo_reflex.is_some()
            || self.jogo_anticheat_kernel.is_some()
            || self.jogo_competitivo.is_some()
            || self.jogo_limitador_nativo.is_some()
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
        fn add(
            f: &mut Vec<Falha>,
            condicao: &'static str,
            esperado: String,
            achado: Option<String>,
        ) {
            f.push(Falha {
                condicao,
                esperado,
                encontrado: achado,
            });
        }
        fn booleano(f: &mut Vec<Falha>, condicao: &'static str, c: Option<bool>, v: Option<bool>) {
            if let Some(c) = c
                && v != Some(c)
            {
                add(f, condicao, c.to_string(), v.map(|v| v.to_string()));
            }
        }
        fn lista(
            f: &mut Vec<Falha>,
            condicao: &'static str,
            l: &Option<Vec<String>>,
            nome: &str,
            conhecido: bool,
        ) {
            if let Some(l) = l
                && !l.iter().any(|n| n == nome)
            {
                add(
                    f,
                    condicao,
                    l.join(", "),
                    conhecido.then(|| nome.to_owned()),
                );
            }
        }

        let mut f = Vec::new();
        if let Some(j) = jogo {
            booleano(&mut f, "jogo_dlss", self.jogo_dlss, Some(j.dlss.is_some()));
            if let Some(m) = self.jogo_dlss_min
                && !j.dlss.is_some_and(|v| v >= m)
            {
                let achado = j
                    .dlss
                    .map_or_else(|| DLSS_AUSENTE.to_owned(), |v| v.to_string());
                add(&mut f, "jogo_dlss_min", m.to_string(), Some(achado));
            }
            lista(
                &mut f,
                "jogo_api",
                &self.jogo_api,
                j.api.nome_regra(),
                j.api != Api::Desconhecida,
            );
            booleano(&mut f, "jogo_reflex", self.jogo_reflex, j.reflex);
            booleano(
                &mut f,
                "jogo_anticheat_kernel",
                self.jogo_anticheat_kernel,
                j.anticheat_kernel,
            );
            booleano(
                &mut f,
                "jogo_competitivo",
                self.jogo_competitivo,
                Some(j.competitivo),
            );
            booleano(
                &mut f,
                "jogo_limitador_nativo",
                self.jogo_limitador_nativo,
                j.limitador_nativo,
            );
        }

        booleano(
            &mut f,
            "tem_tensor_cores",
            self.tem_tensor_cores,
            Some(caps.tem_tensor_cores),
        );
        booleano(&mut f, "tensor_fp8", self.tensor_fp8, Some(caps.tensor_fp8));
        lista(
            &mut f,
            "arquitetura",
            &self.arquitetura,
            caps.arquitetura.nome_regra(),
            true,
        );
        booleano(&mut f, "frame_gen", self.frame_gen, Some(caps.frame_gen));
        booleano(&mut f, "mfg", self.mfg, Some(caps.mfg));
        booleano(
            &mut f,
            "rebar_suportado",
            self.rebar_suportado,
            Some(caps.rebar_suportado),
        );
        booleano(&mut f, "rebar_ativo", self.rebar_ativo, caps.rebar_ativo);
        if let Some(m) = self.vram_mb_min
            && caps.vram_mb < m
        {
            add(
                &mut f,
                "vram_mb_min",
                m.to_string(),
                Some(caps.vram_mb.to_string()),
            );
        }
        if let Some(m) = self.driver_min
            && caps.driver_version < m
        {
            add(
                &mut f,
                "driver_min",
                m.to_string(),
                Some(caps.driver_version.to_string()),
            );
        }
        booleano(
            &mut f,
            "refresh_conhecido",
            self.refresh_conhecido,
            Some(caps.refresh_hz().is_some()),
        );
        booleano(&mut f, "vrr_possivel", self.vrr_possivel, caps.vrr.possivel);
        booleano(&mut f, "vrr_ativo", self.vrr_ativo, caps.vrr.ativo);

        lista(
            &mut f,
            "sincronismo",
            &self.sincronismo,
            prefs.sincronismo.nome_regra(),
            true,
        );
        lista(
            &mut f,
            "qualidade",
            &self.qualidade,
            prefs.qualidade.nome_regra(),
            true,
        );
        booleano(
            &mut f,
            "limite_fps_definido",
            self.limite_fps_definido,
            Some(prefs.limite_fps.is_some()),
        );
        f
    }

    /// Verdadeiro se a condição `nome` está presente.
    pub fn presente(&self, nome: &str) -> bool {
        match nome {
            "arquitetura" => self.arquitetura.is_some(),
            "tem_tensor_cores" => self.tem_tensor_cores.is_some(),
            "tensor_fp8" => self.tensor_fp8.is_some(),
            "frame_gen" => self.frame_gen.is_some(),
            "mfg" => self.mfg.is_some(),
            "rebar_suportado" => self.rebar_suportado.is_some(),
            "rebar_ativo" => self.rebar_ativo.is_some(),
            "vram_mb_min" => self.vram_mb_min.is_some(),
            "driver_min" => self.driver_min.is_some(),
            "refresh_conhecido" => self.refresh_conhecido.is_some(),
            "vrr_possivel" => self.vrr_possivel.is_some(),
            "vrr_ativo" => self.vrr_ativo.is_some(),
            "jogo_api" => self.jogo_api.is_some(),
            "jogo_dlss" => self.jogo_dlss.is_some(),
            "jogo_dlss_min" => self.jogo_dlss_min.is_some(),
            "jogo_reflex" => self.jogo_reflex.is_some(),
            "jogo_anticheat_kernel" => self.jogo_anticheat_kernel.is_some(),
            "jogo_competitivo" => self.jogo_competitivo.is_some(),
            "jogo_limitador_nativo" => self.jogo_limitador_nativo.is_some(),
            "sincronismo" => self.sincronismo.is_some(),
            "qualidade" => self.qualidade.is_some(),
            "limite_fps_definido" => self.limite_fps_definido.is_some(),
            _ => false,
        }
    }

    /// Itens da condição de lista `nome` e os valores que ela aceita
    /// (`None` se `nome` não é condição de lista).
    fn lista_de(&self, nome: &str) -> Option<(&[String], &'static [&'static str])> {
        let (l, aceitos): (&Option<Vec<String>>, &'static [&'static str]) = match nome {
            "arquitetura" => (&self.arquitetura, &Arquitetura::NOMES_REGRA),
            "jogo_api" => (&self.jogo_api, &Api::NOMES_REGRA),
            "sincronismo" => (&self.sincronismo, &Sincronismo::NOMES_REGRA),
            "qualidade" => (&self.qualidade, &Qualidade::NOMES_REGRA),
            _ => return None,
        };
        Some((l.as_deref().unwrap_or(&[]), aceitos))
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
        let valor = match &bruto {
            toml::Value::String(nome) if def.valores.contains_key(nome) => {
                Some(ValorAcao::Nome(nome.clone()))
            }
            toml::Value::Integer(n) => match u32::try_from(*n) {
                Ok(v) if def.aceita(v) => Some(ValorAcao::Numero(v)),
                _ => None,
            },
            toml::Value::Table(t)
                if t.len() == 1 && t.get("limite_vrr") == Some(&toml::Value::Boolean(true)) =>
            {
                if def.faixa.is_some() {
                    if q.refresh_conhecido != Some(true) || q.vrr_ativo != Some(true) {
                        erros.push(cond(
                            "`limite_vrr` exige `quando.refresh_conhecido = true` e `quando.vrr_ativo = true`",
                        ));
                    }
                    Some(ValorAcao::LimiteVrr)
                } else {
                    None
                }
            }
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
        motivos,
        arquivo: arquivo.to_owned(),
    })
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
