//! Avaliador: cruza capacidades, fatos e preferências com as regras e
//! resolve conflitos de forma determinística. Semântica completa em
//! `docs/regras.md`.
//!
//! Ordem de decisão para cada par (alvo, setting):
//! 1. preferência direta do usuário (ex.: `limite_fps`);
//! 2. regra que traduz uma preferência (condicionada a ela, ex.: sincronismo);
//! 3. regra comum.
//!
//! Empate dentro da mesma classe: escopo por jogo > global; mais condições >
//! menos; maior prioridade; menor id (ordem lexicográfica). Por fim, settings
//! travadas pelo usuário nunca entram no plano.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use crate::capabilities::Capabilities;
use crate::catalogo::{Catalogo, PreferenciaGovernante};
use crate::explicacao::{MotivoNaoAplicada, RegraAplicada, RegraNaoAplicada, ResumoJogo};
use crate::facts::GameFacts;
use crate::jogos::filtrar_orientacoes;
use crate::plan::{
    AvisoPlano, Descarte, EstadoExperimental, Origem, Plan, Respeitada, SettingChange, Sugestao,
};
use crate::preferences::{Alvo, Preferences, Sincronismo};
use crate::rules::{ConjuntoRegras, Escopo, Rule, Status, ValorAcao};

/// Setting do catálogo usada pela preferência `limite_fps`.
pub const SETTING_LIMITE_FPS: &str = "limite_fps";

/// Limite de FPS para VRR (`limite_fps = { limite_vrr = true }`):
/// `min(Hz − 3, floor(Hz − Hz²/3600))`. Coincide com o limite automático do
/// driver no modo G-SYNC (≈224 a 240 Hz) e nunca fica a menos de 3 FPS do
/// refresh (spec competitivo §7.4). `None` se o resultado não for positivo.
pub fn limite_vrr(hz: u32) -> Option<u32> {
    let hz = u64::from(hz);
    // floor(Hz − Hz²/3600) = Hz − ceil(Hz²/3600), em inteiros.
    let heuristica = hz.checked_sub((hz * hz).div_ceil(3600))?;
    let margem = hz.checked_sub(3)?;
    u32::try_from(heuristica.min(margem))
        .ok()
        .filter(|&v| v > 0)
}

/// Avisos do plano sobre o sincronismo (F1-15): só com `sincronismo = gsync`.
fn avisos(caps: &Capabilities, prefs: &Preferences) -> Vec<AvisoPlano> {
    if prefs.sincronismo != Sincronismo::GSync {
        return Vec::new();
    }
    match (caps.vrr.possivel, caps.vrr.ativo) {
        (Some(false), _) => vec![AvisoPlano::GsyncSemVrr],
        (Some(true), Some(false)) => vec![AvisoPlano::VrrDesligadoNoPainel],
        _ => Vec::new(),
    }
}

/// Chave de precedência; maior vence.
pub(crate) type Chave = (u8, u8, usize, i32, Reverse<String>);

struct Candidato {
    chave: Chave,
    mudanca: SettingChange,
}

/// Gera o plano de configuração.
pub fn avaliar(
    caps: &Capabilities,
    jogos: &[GameFacts],
    prefs: &Preferences,
    conjunto: &ConjuntoRegras,
) -> Plan {
    avaliar_com_aceites(caps, jogos, prefs, conjunto, &BTreeSet::new())
}

/// Gera o plano DRS com os aceites da sessão de console elegível.
pub fn avaliar_com_aceites(
    caps: &Capabilities,
    jogos: &[GameFacts],
    prefs: &Preferences,
    conjunto: &ConjuntoRegras,
    aceites: &BTreeSet<String>,
) -> Plan {
    let catalogo = &conjunto.catalogo;
    let teto = prefs.qualidade.impacto_maximo();
    let mut plan = Plan {
        avisos: avisos(caps, prefs),
        ..Plan::default()
    };
    let mut candidatos: BTreeMap<(Alvo, String), Vec<Candidato>> = BTreeMap::new();
    // Explicação por jogo: motivos de regras por jogo que ficaram de fora.
    let mut nao_aplicadas: BTreeMap<String, Vec<RegraNaoAplicada>> = BTreeMap::new();
    let mut nao = |alvo: &Alvo, regra: &str, motivo: MotivoNaoAplicada| {
        if let Alvo::Jogo(id) = alvo {
            let lista = nao_aplicadas.entry(id.clone()).or_default();
            let item = RegraNaoAplicada {
                regra: regra.to_owned(),
                motivo,
            };
            if !lista.contains(&item) {
                lista.push(item);
            }
        }
    };

    for regra in &conjunto.regras {
        let aceita = regra.status == Status::Experimental && aceites.contains(&regra.id);
        if aceita {
            plan.estado_experimental
                .insert(regra.id.clone(), EstadoExperimental::Aceita);
        }
        let alvos: Vec<(Alvo, Option<&GameFacts>)> = match regra.escopo {
            Escopo::Global => vec![(Alvo::Global, None)],
            Escopo::PorJogo => jogos
                .iter()
                .map(|j| (Alvo::Jogo(j.id.clone()), Some(j)))
                .collect(),
        };
        for (alvo, jogo) in alvos {
            if !regra.quando.casa(caps, jogo, prefs) {
                if jogo.is_some() {
                    let falhas = regra.quando.falhas(caps, jogo, prefs);
                    nao(
                        &alvo,
                        &regra.id,
                        regra.motivo_nao_casou(&falhas, caps.arquitetura),
                    );
                }
                continue;
            }
            if regra.impacto_visual > teto {
                nao(
                    &alvo,
                    &regra.id,
                    MotivoNaoAplicada::BloqueadaPeloTetoDeQualidade {
                        teto: prefs.qualidade,
                        impacto: regra.impacto_visual,
                    },
                );
                plan.descartes.push(Descarte {
                    regra: Some(regra.id.clone()),
                    alvo,
                    setting: None,
                    motivo: format!(
                        "impacto visual {:?} acima do teto de qualidade {:?}",
                        regra.impacto_visual, prefs.qualidade
                    ),
                });
                continue;
            }
            let mudancas = resolver(regra, caps, catalogo, &alvo, &mut plan.descartes);
            if mudancas.is_empty() {
                // Todas as ações descartadas (ex.: refresh − margem fora da faixa).
                nao(
                    &alvo,
                    &regra.id,
                    MotivoNaoAplicada::CondicaoNaoAtendida {
                        condicao: "aplicar".into(),
                        esperado: "valor aceito pelo catálogo".into(),
                        encontrado: "fora da faixa".into(),
                    },
                );
            }
            match regra.status {
                Status::Experimental if !aceita => {
                    if !mudancas.is_empty() {
                        if mudancas
                            .iter()
                            .any(|(_, m)| !prefs.travada(&alvo, &m.setting))
                        {
                            plan.estado_experimental
                                .insert(regra.id.clone(), EstadoExperimental::Aceitavel);
                        }
                        nao(&alvo, &regra.id, MotivoNaoAplicada::Experimental);
                        plan.experimentais.push(Sugestao {
                            regra: regra.id.clone(),
                            alvo,
                            mudancas: mudancas.into_iter().map(|(_, m)| m).collect(),
                        });
                    }
                }
                Status::Estavel | Status::Experimental => {
                    for (classe, mudanca) in mudancas {
                        let chave = (
                            classe,
                            u8::from(regra.escopo == Escopo::PorJogo),
                            regra.quando.quantidade(),
                            regra.prioridade,
                            Reverse(regra.id.clone()),
                        );
                        candidatos
                            .entry((alvo.clone(), mudanca.setting.clone()))
                            .or_default()
                            .push(Candidato { chave, mudanca });
                    }
                }
            }
        }
    }

    if let Some(limite) = prefs.limite_fps {
        match catalogo.get(SETTING_LIMITE_FPS) {
            Some(def) if def.aceita(limite) => {
                let mudanca = SettingChange {
                    setting: def.nome.clone(),
                    id: def.id,
                    valor: limite,
                    valor_nome: def.nome_do_valor(limite).map(str::to_owned),
                    valor_anterior: None,
                    origem: Origem::Preferencia {
                        preferencia: PreferenciaGovernante::LimiteFps.to_string(),
                        regra: None,
                    },
                    explicacao: format!("Limite de FPS escolhido pelo usuário: {limite}."),
                };
                candidatos
                    .entry((Alvo::Global, def.nome.clone()))
                    .or_default()
                    .push(Candidato {
                        chave: (2, 0, 0, 0, Reverse(String::new())),
                        mudanca,
                    });
            }
            def => plan.descartes.push(Descarte {
                regra: None,
                alvo: Alvo::Global,
                setting: Some(SETTING_LIMITE_FPS.to_owned()),
                motivo: match def {
                    Some(d) => format!(
                        "limite de FPS {limite} fora dos valores aceitos ({})",
                        d.valores_aceitos()
                    ),
                    None => format!("catálogo sem a setting \"{SETTING_LIMITE_FPS}\""),
                },
            }),
        }
    }

    // Settings aplicadas por (jogo, regra).
    let mut aplicadas: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for ((alvo, setting), mut cands) in candidatos {
        cands.sort_by(|a, b| b.chave.cmp(&a.chave));
        let mut iter = cands.into_iter();
        let Some(vencedor) = iter.next() else {
            continue;
        };
        let nome_vencedor = descrever(&vencedor.mudanca.origem);
        let id_vencedor = id_origem(&vencedor.mudanca.origem);
        for perdedor in iter {
            if let Some(r) = perdedor.mudanca.origem.regra() {
                nao(
                    &alvo,
                    r,
                    MotivoNaoAplicada::ConflitoComRegraMaisEspecifica {
                        regra: id_vencedor.clone(),
                    },
                );
            }
            plan.descartes.push(Descarte {
                regra: perdedor.mudanca.origem.regra().map(str::to_owned),
                alvo: alvo.clone(),
                setting: Some(setting.clone()),
                motivo: format!("conflito: suplantada por {nome_vencedor}"),
            });
        }
        if prefs.travada(&alvo, &setting) {
            nao(
                &alvo,
                &id_vencedor,
                MotivoNaoAplicada::EscolhaDoUsuario {
                    setting: setting.clone(),
                },
            );
            plan.respeitadas.push(Respeitada {
                alvo,
                mudanca: vencedor.mudanca,
            });
            continue;
        }
        match alvo {
            Alvo::Global => plan.global.push(vencedor.mudanca),
            Alvo::Jogo(id) => {
                aplicadas
                    .entry(id.clone())
                    .or_default()
                    .entry(id_vencedor)
                    .or_default()
                    .push(setting);
                plan.por_jogo.entry(id).or_default().push(vencedor.mudanca);
            }
        }
    }

    for j in jogos {
        let proprias: Vec<&str> = plan
            .mudancas(&Alvo::Jogo(j.id.clone()))
            .iter()
            .map(|m| m.setting.as_str())
            .collect();
        let herda_do_global = plan
            .global
            .iter()
            .filter(|m| !proprias.contains(&m.setting.as_str()))
            .map(|m| m.setting.clone())
            .collect();
        let mut nao_aplicadas = nao_aplicadas.remove(&j.id).unwrap_or_default();
        nao_aplicadas.sort_by(|a, b| a.regra.cmp(&b.regra));
        let aplicadas = aplicadas
            .remove(&j.id)
            .unwrap_or_default()
            .into_iter()
            .map(|(regra, settings)| RegraAplicada { regra, settings })
            .collect();
        plan.resumos.insert(
            j.id.clone(),
            ResumoJogo {
                aplicadas,
                nao_aplicadas,
                herda_do_global,
                orientacoes: filtrar_orientacoes(&j.orientacoes, prefs.sincronismo),
            },
        );
    }

    plan
}

/// Id estável da origem: o id da regra ou `preferencia:<nome>`.
fn id_origem(origem: &Origem) -> String {
    match origem.regra() {
        Some(r) => r.to_owned(),
        None => match origem {
            Origem::Preferencia { preferencia, .. } => format!("preferencia:{preferencia}"),
            Origem::Regra { id } => id.clone(),
        },
    }
}

fn descrever(origem: &Origem) -> String {
    match origem {
        Origem::Regra { id } => format!("regra \"{id}\""),
        Origem::Preferencia {
            preferencia,
            regra: Some(id),
        } => format!("preferência {preferencia} (regra \"{id}\")"),
        Origem::Preferencia {
            preferencia,
            regra: None,
        } => format!("preferência {preferencia} do usuário"),
    }
}

/// Converte as ações de uma regra em mudanças concretas, com a classe de
/// precedência (1 = traduz preferência, 0 = regra comum).
fn resolver(
    regra: &Rule,
    caps: &Capabilities,
    catalogo: &Catalogo,
    alvo: &Alvo,
    descartes: &mut Vec<Descarte>,
) -> Vec<(u8, SettingChange)> {
    let base = format!(
        "{}. {}",
        regra.descricao.trim_end_matches('.'),
        regra.motivo
    );
    let mut saida = Vec::new();
    for (setting, acao) in &regra.aplicar {
        // A validação garante que a setting existe no catálogo.
        let Some(def) = catalogo.get(setting) else {
            continue;
        };
        let (valor, extra) = match acao {
            ValorAcao::Nome(n) => (def.valores[n], String::new()),
            ValorAcao::Numero(v) => (*v, String::new()),
            // O cálculo usa o piso do refresh (59,94 Hz → 59): arredondar para
            // cima deixaria o limite a menos de 3 FPS do refresh real. O
            // arredondamento fica só na exibição.
            ValorAcao::LimiteVrr => match caps
                .refresh_hz_piso()
                .and_then(|hz| limite_vrr(hz).map(|v| (hz, v)))
            {
                Some((hz, v)) if def.aceita(v) => {
                    let formula = format!("mínimo entre {hz} Hz − 3 e ⌊{hz} − {hz}²/3600⌋ = {v}");
                    let texto = match caps.refresh_hz() {
                        Some(exib) if exib != hz => format!(
                            " Valor: refresh de ≈{exib} Hz, contado como {hz} Hz (para baixo); {formula}."
                        ),
                        _ => format!(" Valor: {formula}."),
                    };
                    (v, texto)
                }
                _ => {
                    descartes.push(Descarte {
                        regra: Some(regra.id.clone()),
                        alvo: alvo.clone(),
                        setting: Some(setting.clone()),
                        motivo: format!(
                            "limite para VRR com refresh {:?} Hz não gera valor aceito ({})",
                            caps.refresh_hz(),
                            def.valores_aceitos()
                        ),
                    });
                    continue;
                }
            },
            // Mesmo critério do limite para VRR: a conta usa o piso do refresh
            // (59,94 Hz → 59), senão `Hz − N` ficaria a menos de N FPS do
            // refresh real; o arredondamento fica só na exibição.
            ValorAcao::RefreshMenos(margem) => {
                let calculado = caps
                    .refresh_hz_piso()
                    .and_then(|hz| hz.checked_sub(*margem).map(|v| (hz, v)));
                match calculado {
                    Some((hz, v)) if def.aceita(v) => {
                        let texto = match caps.refresh_hz() {
                            Some(exib) if exib != hz => format!(
                                " Valor: refresh de ≈{exib} Hz, contado como {hz} Hz (para baixo); {hz} Hz − {margem} = {v}."
                            ),
                            _ => format!(" Valor: {hz} Hz − {margem} = {v}."),
                        };
                        (v, texto)
                    }
                    _ => {
                        descartes.push(Descarte {
                            regra: Some(regra.id.clone()),
                            alvo: alvo.clone(),
                            setting: Some(setting.clone()),
                            motivo: format!(
                                "refresh {:?} Hz menos {margem} não gera valor aceito ({})",
                                caps.refresh_hz(),
                                def.valores_aceitos()
                            ),
                        });
                        continue;
                    }
                }
            }
        };
        let governante = def
            .governada_por
            .iter()
            .find(|p| regra.quando.condiciona(**p));
        let (classe, origem) = match governante {
            Some(p) => (
                1,
                Origem::Preferencia {
                    preferencia: p.to_string(),
                    regra: Some(regra.id.clone()),
                },
            ),
            None => (
                0,
                Origem::Regra {
                    id: regra.id.clone(),
                },
            ),
        };
        saida.push((
            classe,
            SettingChange {
                setting: setting.clone(),
                id: def.id,
                valor,
                valor_nome: def.nome_do_valor(valor).map(str::to_owned),
                valor_anterior: None,
                origem,
                explicacao: format!("{base}{extra}"),
            },
        ));
    }
    saida
}
