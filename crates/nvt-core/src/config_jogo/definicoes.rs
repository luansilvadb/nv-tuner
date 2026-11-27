//! Catálogo de jogo: raízes, arquivos, chaves e regras `[[arquivo]]`/`[[regra]]`.

use super::*;

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

    pub(super) fn valor_de_numero(&self, n: i64) -> Option<ValorJogo> {
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
    pub(super) fn quantidade(&self) -> usize {
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
    pub(super) arquivos: Vec<DefArquivoJogo>,
    pub(super) regras: Vec<RegraJogo>,
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
