//! Avaliação das regras `config_jogo` em [`PlanoJogo`] e o limite DRS do jogo (F3-13).

use super::*;

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
    let travas = prefs.travas();
    for j in jogos {
        let Some(regra) = j.jogo_base.as_ref().and_then(|b| limites.get(b)) else {
            continue;
        };
        let alvo = Alvo::Jogo(j.id.clone());
        if travas.contains(&(&alvo, SETTING_LIMITE_FPS)) {
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
