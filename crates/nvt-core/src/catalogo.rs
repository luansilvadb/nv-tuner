//! Catálogo de settings simbólicas: nome estável usado pelas regras →
//! ID DRS (`u32`) e valores nomeados. Carregado de `regras/settings.toml`.
//!
//! O catálogo é também a lista permitida: uma regra só pode referenciar
//! settings que estão nele.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::carga;
use crate::rules::{ErroRegra, ErrosRegras};

/// Preferência do usuário que governa uma setting. Regras só podem mexer
/// numa setting governada se forem condicionadas a essa preferência.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreferenciaGovernante {
    /// `Preferences::sincronismo` (condição `quando.sincronismo`).
    Sincronismo,
    /// `Preferences::limite_fps` (condição `quando.limite_fps_definido`).
    LimiteFps,
}

impl fmt::Display for PreferenciaGovernante {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Sincronismo => "sincronismo",
            Self::LimiteFps => "limite_fps",
        })
    }
}

/// Faixa de valores numéricos aceitos (inclusiva), além dos nomeados.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Faixa {
    pub min: u32,
    pub max: u32,
}

impl Faixa {
    pub const fn contem(self, v: u32) -> bool {
        v >= self.min && v <= self.max
    }
}

/// Definição de uma setting do catálogo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefSetting {
    /// Nome simbólico (`snake_case`), chave usada em `aplicar`.
    pub nome: String,
    /// ID DRS.
    pub id: u32,
    /// Nome original (NPI/driver) e observações.
    pub descricao: String,
    /// Valores nomeados (`nome = valor`).
    #[serde(default)]
    pub valores: BTreeMap<String, u32>,
    /// Valores numéricos livres aceitos (ex.: limite de FPS).
    #[serde(default)]
    pub faixa: Option<Faixa>,
    /// Preferências que governam esta setting.
    #[serde(default)]
    pub governada_por: Vec<PreferenciaGovernante>,
}

impl DefSetting {
    /// Nome do valor, se for um dos nomeados.
    pub fn nome_do_valor(&self, valor: u32) -> Option<&str> {
        self.valores
            .iter()
            .find(|(_, v)| **v == valor)
            .map(|(n, _)| n.as_str())
    }

    /// Verdadeiro se o valor numérico é aceito (nomeado ou dentro da faixa).
    pub fn aceita(&self, valor: u32) -> bool {
        self.valores.values().any(|v| *v == valor) || self.faixa.is_some_and(|f| f.contem(valor))
    }

    /// Lista legível dos valores aceitos, para mensagens de erro.
    pub fn valores_aceitos(&self) -> String {
        let mut partes: Vec<String> = self.valores.keys().cloned().collect();
        if let Some(f) = self.faixa {
            partes.push(format!("números de {} a {}", f.min, f.max));
        }
        partes.join(", ")
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArquivoCatalogo {
    versao_schema: u32,
    #[serde(default)]
    setting: Vec<DefSetting>,
}

impl carga::Versionado for ArquivoCatalogo {
    const VERSAO_SCHEMA: u32 = VERSAO_SCHEMA;

    fn versao_schema(&self) -> u32 {
        self.versao_schema
    }
}

/// Versão do formato de `settings.toml` e dos arquivos de regras.
pub const VERSAO_SCHEMA: u32 = 1;

/// Catálogo validado.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Catalogo {
    settings: BTreeMap<String, DefSetting>,
    /// Índice ID DRS → nome simbólico.
    nomes_por_id: BTreeMap<u32, String>,
}

impl Catalogo {
    /// Interpreta e valida o conteúdo de `settings.toml`.
    pub fn from_toml(arquivo: &str, conteudo: &str) -> Result<Self, ErrosRegras> {
        let lido =
            carga::ler::<ArquivoCatalogo>(arquivo, conteudo).map_err(|e| ErrosRegras(vec![e]))?;
        let bruto = lido.dados;
        let mut erros = Vec::new();
        if let Some(encontrada) = lido.versao_errada {
            erros.push(ErroRegra::VersaoSchema {
                arquivo: arquivo.to_owned(),
                encontrada,
            });
        }
        let mut settings = BTreeMap::new();
        let mut ids: BTreeMap<u32, String> = BTreeMap::new();
        for def in bruto.setting {
            let erro = |mensagem: String| ErroRegra::Catalogo {
                arquivo: arquivo.to_owned(),
                setting: def.nome.clone(),
                mensagem,
            };
            if !nome_valido(&def.nome) {
                erros.push(erro("nome deve ser snake_case (a-z, 0-9, _)".into()));
            }
            if def.valores.is_empty() && def.faixa.is_none() {
                erros.push(erro("precisa de `valores` ou `faixa`".into()));
            }
            if let Some(f) = def.faixa
                && f.min > f.max
            {
                erros.push(erro(format!(
                    "faixa inválida: min {} > max {}",
                    f.min, f.max
                )));
            }
            for nome in def.valores.keys() {
                if !nome_valido(nome) {
                    erros.push(erro(format!(
                        "nome de valor \"{nome}\" deve ser snake_case"
                    )));
                }
            }
            if let Some(outra) = ids.insert(def.id, def.nome.clone()) {
                erros.push(erro(format!(
                    "ID 0x{:08X} já usado pela setting \"{outra}\"",
                    def.id
                )));
            }
            if settings.contains_key(&def.nome) {
                erros.push(erro("nome duplicado".into()));
                continue;
            }
            settings.insert(def.nome.clone(), def);
        }
        if erros.is_empty() {
            Ok(Self {
                settings,
                nomes_por_id: ids,
            })
        } else {
            Err(ErrosRegras(erros))
        }
    }

    /// Busca uma setting pelo nome simbólico.
    pub fn get(&self, nome: &str) -> Option<&DefSetting> {
        self.settings.get(nome)
    }

    /// Busca uma setting pelo ID DRS.
    pub fn por_id(&self, id: u32) -> Option<&DefSetting> {
        self.nomes_por_id
            .get(&id)
            .and_then(|n| self.settings.get(n))
    }

    /// Todas as settings, em ordem de nome.
    pub fn iter(&self) -> impl Iterator<Item = &DefSetting> {
        self.settings.values()
    }

    /// IDs DRS permitidos (lista permitida do serviço).
    pub fn ids_permitidos(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.settings.values().map(|d| d.id).collect();
        ids.sort_unstable();
        ids
    }

    pub fn len(&self) -> usize {
        self.settings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.settings.is_empty()
    }
}

/// `snake_case` ASCII: começa com letra, só `a-z`, `0-9` e `_`.
pub(crate) fn nome_valido(nome: &str) -> bool {
    let mut chars = nome.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_'))
}
