//! Modelo e avaliação da camada interativa do Windows, sem E/S.

use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::config_jogo::{FatosConfigJogo, PlanoJogo};
use crate::evaluator::Chave;
use crate::plan::{CodigoExplicacao, EstadoExperimental};
use crate::rules::id_valido;
use crate::{ConjuntoRegras, ErroRegra, ErrosRegras, ImpactoVisual, Qualidade, Status};

pub const WINDOWS_TOML: &str = include_str!("../../../regras/windows.toml");
pub const VERSAO_SCHEMA_WINDOWS: u32 = 2;

/// Lista fechada de destinos. A fachada resolve o destino físico pelo enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingUsuario {
    TelaRefresh,
    MouseAceleracao,
    DxSwapEffectUpgrade,
    GamedvrCapturaHistorica,
}

use SettingUsuario::{DxSwapEffectUpgrade, GamedvrCapturaHistorica, MouseAceleracao, TelaRefresh};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshRacional {
    pub numerador: u32,
    pub denominador: u32,
}

impl RefreshRacional {
    pub fn valido(self) -> bool {
        self.numerador != 0 && self.denominador != 0
    }

    /// Compara em Hz sem arredondar nem mudar a representação a ser gravada.
    pub fn comparar(self, outro: Self) -> Option<Ordering> {
        (self.valido() && outro.valido()).then(|| {
            (u64::from(self.numerador) * u64::from(outro.denominador))
                .cmp(&(u64::from(outro.numerador) * u64::from(self.denominador)))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValorUsuario {
    Ausente,
    Dword(u32),
    Token(String),
    Spi([i32; 3]),
    Refresh(RefreshRacional),
}

impl SettingUsuario {
    /// Valida leituras e valores usados em restauração, além dos alvos do plano.
    pub fn aceita(self, valor: &ValorUsuario) -> bool {
        match (self, valor) {
            (TelaRefresh, ValorUsuario::Refresh(taxa)) => taxa.valido(),
            (MouseAceleracao, ValorUsuario::Spi([_, _, aceleracao])) => {
                (0..=2).contains(aceleracao)
            }
            (DxSwapEffectUpgrade, ValorUsuario::Token(token)) => {
                matches!(token.as_str(), "0" | "1")
            }
            (GamedvrCapturaHistorica, ValorUsuario::Dword(0 | 1))
            | (DxSwapEffectUpgrade | GamedvrCapturaHistorica, ValorUsuario::Ausente) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelaUsuario {
    /// Caminho estável do dispositivo de exibição da sessão.
    pub alvo: String,
    pub atual: RefreshRacional,
    /// Melhor modo compatível com a topologia, resolução e profundidade atuais.
    pub maximo_nativo: Option<RefreshRacional>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GestaoUsuario {
    pub antes: ValorUsuario,
    pub esperado: ValorUsuario,
    /// Caminho estável tomado junto com o snapshot da tela.
    pub alvo_tela: Option<String>,
}

/// `None` significa leitura desconhecida. `Some(ValorUsuario::Ausente)` é
/// ausência confirmada. O coordenador fornece as travas e a gestão da sessão.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FatosUsuario {
    pub windows_build: Option<u32>,
    pub tela: Option<TelaUsuario>,
    pub mouse_aceleracao: Option<[i32; 3]>,
    pub dx_swap_effect_upgrade: Option<ValorUsuario>,
    pub gamedvr_captura_historica: Option<ValorUsuario>,
    pub gestao: BTreeMap<SettingUsuario, GestaoUsuario>,
    pub travadas: BTreeSet<SettingUsuario>,
    /// Família jogo (fase 3, D10); ausente num estado v6.
    pub jogo: FatosConfigJogo,
}

impl FatosUsuario {
    pub fn valor_atual(&self, setting: SettingUsuario) -> Option<ValorUsuario> {
        let valor = match setting {
            TelaRefresh => self
                .tela
                .as_ref()
                .filter(|t| !t.alvo.trim().is_empty())
                .map(|t| ValorUsuario::Refresh(t.atual)),
            MouseAceleracao => self.mouse_aceleracao.map(ValorUsuario::Spi),
            DxSwapEffectUpgrade => self.dx_swap_effect_upgrade.clone(),
            GamedvrCapturaHistorica => self.gamedvr_captura_historica.clone(),
        };
        valor.filter(|v| setting.aceita(v))
    }

    fn gestao_do_alvo(&self, setting: SettingUsuario) -> Option<&GestaoUsuario> {
        let gestao = self.gestao.get(&setting)?;
        if setting == TelaRefresh
            && (gestao.alvo_tela.is_none()
                || gestao.alvo_tela.as_deref() != self.tela.as_ref().map(|t| t.alvo.as_str()))
        {
            return None;
        }
        Some(gestao)
    }
}

fn corresponde(atual: &ValorUsuario, esperado: &ValorUsuario) -> bool {
    match (atual, esperado) {
        (ValorUsuario::Refresh(a), ValorUsuario::Refresh(b)) => {
            a.comparar(*b) == Some(Ordering::Equal)
        }
        _ => atual == esperado,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EstadoToken {
    On,
    Off,
    Ausente,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct CondicoesUsuario {
    windows_build_min: Option<u32>,
    refresh_abaixo_do_maximo: Option<bool>,
    mouse_aceleracao_ligada: Option<bool>,
    swap_effect_upgrade: Option<Vec<EstadoToken>>,
    captura_historica_ligada: Option<bool>,
}

impl CondicoesUsuario {
    fn presencas(&self) -> [bool; 5] {
        [
            self.windows_build_min.is_some(),
            self.refresh_abaixo_do_maximo.is_some(),
            self.mouse_aceleracao_ligada.is_some(),
            self.swap_effect_upgrade.is_some(),
            self.captura_historica_ligada.is_some(),
        ]
    }

    fn casa(&self, fatos: &FatosUsuario, valores: &BTreeMap<SettingUsuario, ValorUsuario>) -> bool {
        self.windows_build_min
            .is_none_or(|min| fatos.windows_build.is_some_and(|b| b >= min))
            && self.refresh_abaixo_do_maximo.is_none_or(|esperado| {
                let atual = match valores.get(&TelaRefresh) {
                    Some(ValorUsuario::Refresh(taxa)) => *taxa,
                    _ => return false,
                };
                let maximo = fatos.tela.as_ref().and_then(|t| t.maximo_nativo);
                maximo
                    .and_then(|max| atual.comparar(max))
                    .is_some_and(|ordem| (ordem == Ordering::Less) == esperado)
            })
            && self.mouse_aceleracao_ligada.is_none_or(|esperado| {
                matches!(valores.get(&MouseAceleracao),
                    Some(ValorUsuario::Spi([_, _, aceleracao])) if (*aceleracao != 0) == esperado)
            })
            && self.swap_effect_upgrade.as_ref().is_none_or(|aceitos| {
                let estado = match valores.get(&DxSwapEffectUpgrade) {
                    Some(ValorUsuario::Token(token)) if token == "1" => EstadoToken::On,
                    Some(ValorUsuario::Token(token)) if token == "0" => EstadoToken::Off,
                    Some(ValorUsuario::Ausente) => EstadoToken::Ausente,
                    _ => return false,
                };
                aceitos.contains(&estado)
            })
            && self.captura_historica_ligada.is_none_or(|esperado| {
                match valores.get(&GamedvrCapturaHistorica) {
                    Some(ValorUsuario::Dword(1)) => esperado,
                    Some(ValorUsuario::Dword(0) | ValorUsuario::Ausente) => !esperado,
                    _ => false,
                }
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ValorCatalogoUsuario {
    MaximoNativo,
    Original,
    Fixo(ValorUsuario),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
enum NomeValor {
    MaximoNativo,
    Original,
    Off,
    On,
    Ausente,
}

impl NomeValor {
    fn esperado(self, setting: SettingUsuario) -> Option<ValorCatalogoUsuario> {
        use ValorCatalogoUsuario::{Fixo, MaximoNativo, Original};
        Some(match (setting, self) {
            (TelaRefresh, Self::MaximoNativo) => MaximoNativo,
            (TelaRefresh | MouseAceleracao, Self::Original) => Original,
            (MouseAceleracao, Self::Off) => Fixo(ValorUsuario::Spi([0, 0, 0])),
            (DxSwapEffectUpgrade, Self::On) => Fixo(ValorUsuario::Token("1".into())),
            (DxSwapEffectUpgrade, Self::Off) => Fixo(ValorUsuario::Token("0".into())),
            (GamedvrCapturaHistorica, Self::On) => Fixo(ValorUsuario::Dword(1)),
            (GamedvrCapturaHistorica, Self::Off) => Fixo(ValorUsuario::Dword(0)),
            (DxSwapEffectUpgrade | GamedvrCapturaHistorica, Self::Ausente) => {
                Fixo(ValorUsuario::Ausente)
            }
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
enum ValorToml {
    Dword(u32),
    Spi([i32; 3]),
    Texto(String),
}

impl From<ValorCatalogoUsuario> for ValorToml {
    fn from(valor: ValorCatalogoUsuario) -> Self {
        match valor {
            ValorCatalogoUsuario::MaximoNativo => Self::Texto("maximo_nativo".into()),
            ValorCatalogoUsuario::Original => Self::Texto("original".into()),
            ValorCatalogoUsuario::Fixo(ValorUsuario::Ausente) => Self::Texto("ausente".into()),
            ValorCatalogoUsuario::Fixo(ValorUsuario::Dword(dword)) => Self::Dword(dword),
            ValorCatalogoUsuario::Fixo(ValorUsuario::Spi(spi)) => Self::Spi(spi),
            ValorCatalogoUsuario::Fixo(ValorUsuario::Token(token)) => Self::Texto(token),
            ValorCatalogoUsuario::Fixo(ValorUsuario::Refresh(_)) => {
                unreachable!("catálogo usa refresh dinâmico")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EscopoUsuario {
    Usuario,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegraUsuario {
    id: String,
    descricao: String,
    status: Status,
    escopo: EscopoUsuario,
    #[serde(default)]
    quando: CondicoesUsuario,
    aplicar: BTreeMap<SettingUsuario, NomeValor>,
    impacto_visual: ImpactoVisual,
    #[serde(default)]
    prioridade: i32,
    motivo: String,
    evidencia: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArquivoWindows {
    versao_schema: u32,
    catalogo: BTreeMap<SettingUsuario, BTreeMap<NomeValor, ValorToml>>,
    regra: Vec<RegraUsuario>,
}

const SETTINGS: [SettingUsuario; 4] = [
    TelaRefresh,
    MouseAceleracao,
    DxSwapEffectUpgrade,
    GamedvrCapturaHistorica,
];

/// Catálogo e regras validados junto com os IDs DRS que serão usados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConjuntoRegrasUsuario {
    catalogo: BTreeMap<SettingUsuario, BTreeMap<NomeValor, ValorCatalogoUsuario>>,
    regras: Vec<RegraUsuario>,
}

impl ConjuntoRegrasUsuario {
    pub fn embutido(drs: &ConjuntoRegras) -> Result<Self, ErrosRegras> {
        Self::carregar("windows.toml", WINDOWS_TOML, drs)
    }

    pub fn carregar(
        arquivo: &str,
        conteudo: &str,
        drs: &ConjuntoRegras,
    ) -> Result<Self, ErrosRegras> {
        let erro = |mensagem: String| {
            ErrosRegras(vec![ErroRegra::Toml {
                arquivo: arquivo.to_owned(),
                mensagem,
            }])
        };
        let bruto: ArquivoWindows = toml::from_str(conteudo).map_err(|e| erro(e.to_string()))?;
        if bruto.versao_schema != VERSAO_SCHEMA_WINDOWS {
            return Err(erro(format!(
                "versao_schema {} não suportada (esperado {VERSAO_SCHEMA_WINDOWS})",
                bruto.versao_schema
            )));
        }
        let mut catalogo = BTreeMap::new();
        for setting in SETTINGS {
            let nomes = [
                NomeValor::MaximoNativo,
                NomeValor::Original,
                NomeValor::Off,
                NomeValor::On,
                NomeValor::Ausente,
            ];
            let valores: BTreeMap<_, _> = nomes
                .into_iter()
                .filter_map(|nome| nome.esperado(setting).map(|valor| (nome, valor)))
                .collect();
            let esperado = valores
                .iter()
                .map(|(&nome, valor)| (nome, ValorToml::from(valor.clone())))
                .collect();
            if bruto.catalogo.get(&setting) != Some(&esperado) {
                return Err(ErrosRegras(vec![ErroRegra::Catalogo {
                    arquivo: arquivo.into(),
                    setting: format!("{setting:?}"),
                    mensagem: format!("valores esperados: {esperado:?}"),
                }]));
            }
            catalogo.insert(setting, valores);
        }

        let mut ids: BTreeSet<_> = drs.regras.iter().map(|r| r.id.as_str()).collect();
        for regra in &bruto.regra {
            let id = &regra.id;
            if !id_valido(id) {
                return Err(ErrosRegras(vec![ErroRegra::IdInvalido {
                    arquivo: arquivo.into(),
                    id: id.clone(),
                }]));
            }
            if !ids.insert(id) {
                return Err(ErrosRegras(vec![ErroRegra::RegraDuplicada {
                    arquivo: arquivo.into(),
                    id: id.clone(),
                }]));
            }
            for (campo, vazio) in [
                ("descricao", regra.descricao.trim().is_empty()),
                ("motivo", regra.motivo.trim().is_empty()),
                ("evidencia", regra.evidencia.trim().is_empty()),
                ("aplicar", regra.aplicar.is_empty()),
            ] {
                if vazio {
                    return Err(ErrosRegras(vec![ErroRegra::CampoVazio {
                        regra: id.clone(),
                        campo,
                    }]));
                }
            }
            let q = &regra.quando;
            if q.windows_build_min == Some(0)
                || q.swap_effect_upgrade.as_ref().is_some_and(|estados| {
                    estados.is_empty()
                        || estados
                            .iter()
                            .enumerate()
                            .any(|(i, estado)| estados[..i].contains(estado))
                })
            {
                return Err(ErrosRegras(vec![ErroRegra::CondicaoInvalida {
                    regra: id.clone(),
                    mensagem: "build mínimo deve ser positivo e a lista swap_effect_upgrade deve ter estados distintos".into(),
                }]));
            }
            for (setting, nome) in &regra.aplicar {
                let valores = &catalogo[setting];
                if !valores.contains_key(nome) {
                    return Err(ErrosRegras(vec![ErroRegra::ValorInvalido {
                        regra: id.clone(),
                        setting: format!("{setting:?}"),
                        valor: format!("{nome:?}"),
                        aceitos: format!("{:?}", valores.keys().collect::<Vec<_>>()),
                    }]));
                }
            }
        }
        Ok(Self {
            catalogo,
            regras: bruto.regra,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MudancaUsuario {
    pub setting: SettingUsuario,
    pub atual: ValorUsuario,
    pub valor: ValorUsuario,
    pub alvo_tela: Option<String>,
    pub regra: String,
    pub descricao: String,
    pub motivo: String,
    pub evidencia: String,
    pub codigo: Option<CodigoExplicacao>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SugestaoUsuario {
    pub regra: String,
    pub mudancas: Vec<MudancaUsuario>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotivoRespeitadaUsuario {
    Travada,
    AlteradaForaDaGestao,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumoSistema {
    /// Vencedoras do avaliador. A execução efetiva é confirmada pelo agente.
    pub mudancas: BTreeMap<SettingUsuario, MudancaUsuario>,
    pub respeitadas: BTreeMap<SettingUsuario, MotivoRespeitadaUsuario>,
    pub estado_experimental: BTreeMap<String, EstadoExperimental>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanoUsuario {
    pub resumo: ResumoSistema,
    pub experimentais: Vec<SugestaoUsuario>,
    /// Família jogo (fase 3, D10), preenchida por [`crate::config_jogo::avaliar_config_jogo`].
    #[serde(default)]
    pub jogo: PlanoJogo,
}

/// Avalia o Windows para uma sessão. Para settings geridas, as condições
/// corretivas usam o original enquanto a releitura confere com o esperado.
pub fn avaliar(
    fatos: &FatosUsuario,
    qualidade: Qualidade,
    conjunto: &ConjuntoRegrasUsuario,
    aceites: &BTreeSet<String>,
) -> PlanoUsuario {
    let mut plano = PlanoUsuario::default();
    let atuais: BTreeMap<_, _> = SETTINGS
        .into_iter()
        .filter_map(|setting| fatos.valor_atual(setting).map(|valor| (setting, valor)))
        .collect();
    plano.resumo.respeitadas.extend(
        fatos
            .travadas
            .iter()
            .map(|&setting| (setting, MotivoRespeitadaUsuario::Travada)),
    );
    let mut valores = atuais.clone();
    for (&setting, atual) in &atuais {
        if let Some(gestao) = fatos.gestao_do_alvo(setting) {
            if corresponde(atual, &gestao.esperado) && setting.aceita(&gestao.antes) {
                valores.insert(setting, gestao.antes.clone());
            } else {
                plano
                    .resumo
                    .respeitadas
                    .insert(setting, MotivoRespeitadaUsuario::AlteradaForaDaGestao);
            }
        }
    }

    let mut vencedoras: BTreeMap<_, (Chave, _)> = BTreeMap::new();
    for regra in &conjunto.regras {
        let aceita = regra.status == Status::Experimental && aceites.contains(&regra.id);
        if aceita {
            plano
                .resumo
                .estado_experimental
                .insert(regra.id.clone(), EstadoExperimental::Aceita);
        }
        if regra.impacto_visual > qualidade.impacto_maximo() || !regra.quando.casa(fatos, &valores)
        {
            continue;
        }

        let mut sugestoes = Vec::new();
        for (&setting, &nome) in &regra.aplicar {
            if plano.resumo.respeitadas.contains_key(&setting) {
                continue;
            }
            let Some(atual) = atuais.get(&setting) else {
                continue;
            };
            let Some(valor) = resolver(fatos, setting, &conjunto.catalogo[&setting][&nome]) else {
                continue;
            };
            let mudanca = MudancaUsuario {
                setting,
                atual: atual.clone(),
                valor,
                alvo_tela: fatos
                    .tela
                    .as_ref()
                    .filter(|_| setting == TelaRefresh)
                    .map(|t| t.alvo.clone()),
                regra: regra.id.clone(),
                descricao: regra.descricao.clone(),
                motivo: regra.motivo.clone(),
                evidencia: regra.evidencia.clone(),
                codigo: aceita.then_some(CodigoExplicacao::ExperimentalAceitaPorVoce),
            };
            if regra.status == Status::Experimental && !aceita {
                sugestoes.push(mudanca);
                continue;
            }

            let chave = (
                0,
                0,
                regra.quando.presencas().into_iter().filter(|&p| p).count(),
                regra.prioridade,
                Reverse(regra.id.clone()),
            );
            if vencedoras
                .get(&setting)
                .is_none_or(|(anterior, _)| chave > *anterior)
            {
                vencedoras.insert(setting, (chave, mudanca));
            }
        }
        if !sugestoes.is_empty() {
            plano
                .resumo
                .estado_experimental
                .insert(regra.id.clone(), EstadoExperimental::Aceitavel);
            plano.experimentais.push(SugestaoUsuario {
                regra: regra.id.clone(),
                mudancas: sugestoes,
            });
        }
    }
    plano.resumo.mudancas = vencedoras.into_iter().map(|(s, (_, m))| (s, m)).collect();
    plano
}

fn resolver(
    fatos: &FatosUsuario,
    setting: SettingUsuario,
    valor: &ValorCatalogoUsuario,
) -> Option<ValorUsuario> {
    match valor {
        ValorCatalogoUsuario::MaximoNativo => fatos
            .tela
            .as_ref()?
            .maximo_nativo
            .filter(|taxa| taxa.valido())
            .map(ValorUsuario::Refresh),
        ValorCatalogoUsuario::Original => {
            Some(fatos.gestao_do_alvo(setting)?.antes.clone()).filter(|valor| setting.aceita(valor))
        }
        ValorCatalogoUsuario::Fixo(valor) => Some(valor.clone()),
    }
}
