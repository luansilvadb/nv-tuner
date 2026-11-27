//! Funções livres dos ajustes de settings do usuário: fatos, planejamento,
//! validação e entradas de histórico.

use super::*;

pub(super) fn fatos_core(
    fatos: &nvt_ipc::FatosUsuario,
    windows_build: Option<u32>,
) -> FatosUsuario {
    FatosUsuario {
        windows_build,
        tela: match &fatos.tela {
            Fato::Conhecido(t) => Some(usuario::TelaUsuario {
                alvo: t.alvo.clone(),
                atual: taxa_core(t.atual),
                maximo_nativo: Some(taxa_core(t.maximo)),
            }),
            _ => None,
        },
        mouse_aceleracao: match fatos.mouse {
            Fato::Conhecido(m) => Some([m.limiar1, m.limiar2, m.aceleracao]),
            _ => None,
        },
        dx_swap_effect_upgrade: valor_fato(&fatos.swap_effect_upgrade, ValorUsuario::Token),
        gamedvr_captura_historica: valor_fato(&fatos.captura_historica, ValorUsuario::Dword),
        ..Default::default()
    }
}

pub(super) fn valor_fato<T: Clone>(
    fato: &Fato<T>,
    converter: impl FnOnce(T) -> ValorUsuario,
) -> Option<ValorUsuario> {
    match fato {
        Fato::Conhecido(v) => Some(converter(v.clone())),
        Fato::Ausente => Some(ValorUsuario::Ausente),
        Fato::Desconhecido => None,
    }
}

/// O agente persiste a gestão da camada usuário; a sessão em memória do
/// serviço só a reconstrói, sem nunca apagar o que já conhece.
pub(super) fn reidratar(s: &mut Sessao, gestao: &[nvt_ipc::GestaoAgente]) -> bool {
    let mut mudou = false;
    for g in gestao {
        if let Some(ajuste) = g.valores.as_ref().filter(|a| a.destino() == g.destino) {
            let (antes, esperado) = valores(ajuste);
            let tipo = setting(&g.destino);
            if tipo.aceita(&antes) && tipo.aceita(&esperado) {
                let nova = GestaoUsuario {
                    antes,
                    esperado,
                    alvo_tela: alvo(&g.destino).map(str::to_owned),
                };
                if s.gestao.get(&g.destino) != Some(&nova) {
                    s.gestao.insert(g.destino.clone(), nova);
                    mudou = true;
                }
            }
        }
        if g.travado {
            mudou |= s.travas.insert(g.destino.clone());
        }
    }
    mudou
}

pub(super) fn preencher_gestao(fatos: &mut FatosUsuario, s: &Sessao) {
    let tela = fatos.tela.as_ref().map(|t| t.alvo.as_str());
    for (destino, gestao) in &s.gestao {
        if alvo(destino).is_none() || alvo(destino) == tela {
            fatos.gestao.insert(setting(destino), gestao.clone());
        }
    }
    fatos.travadas.extend(
        s.travas
            .iter()
            .filter(|d| alvo(d).is_none() || alvo(d) == tela)
            .map(setting),
    );
}

pub(super) fn alvo(destino: &SettingUsuario) -> Option<&str> {
    match destino {
        SettingUsuario::Refresh { alvo } => Some(alvo),
        _ => None,
    }
}

pub(super) fn setting(destino: &SettingUsuario) -> Setting {
    match destino {
        SettingUsuario::Refresh { .. } => Setting::TelaRefresh,
        SettingUsuario::MouseAceleracao => Setting::MouseAceleracao,
        SettingUsuario::DxSwapEffectUpgrade => Setting::DxSwapEffectUpgrade,
        SettingUsuario::GameDvrCapturaHistorica => Setting::GamedvrCapturaHistorica,
    }
}

pub(super) fn destino(setting: Setting, tela: Option<&str>) -> Option<SettingUsuario> {
    Some(match setting {
        Setting::TelaRefresh => SettingUsuario::Refresh { alvo: tela?.into() },
        Setting::MouseAceleracao => SettingUsuario::MouseAceleracao,
        Setting::DxSwapEffectUpgrade => SettingUsuario::DxSwapEffectUpgrade,
        Setting::GamedvrCapturaHistorica => SettingUsuario::GameDvrCapturaHistorica,
    })
}

pub(super) fn corresponde(a: &ValorUsuario, b: &ValorUsuario) -> bool {
    match (a, b) {
        (ValorUsuario::Refresh(a), ValorUsuario::Refresh(b)) => {
            a.comparar(*b) == Some(std::cmp::Ordering::Equal)
        }
        _ => a == b,
    }
}

pub(super) fn planejar(
    s: &Sessao,
    fatos: &FatosUsuario,
    plano: &PlanoUsuario,
    pausado: bool,
) -> Option<(OperacaoUsuario, Vec<AcaoUsuario>)> {
    let tela = fatos.tela.as_ref().map(|t| t.alvo.as_str());
    let legivel = |d: &SettingUsuario| {
        (alvo(d).is_none() || alvo(d) == tela) && fatos.valor_atual(setting(d)).is_some()
    };
    let reverter: Vec<_> = s
        .reversoes
        .iter()
        .filter(|d| legivel(d))
        .map(|d| AcaoUsuario::Reverter { destino: d.clone() })
        .collect();
    if !reverter.is_empty() {
        return Some((OperacaoUsuario::Reverter, reverter));
    }
    if pausado {
        return None;
    }
    let restaurar: Vec<_> = s
        .gestao
        .iter()
        .filter(|(d, g)| {
            legivel(d)
                && !plano.resumo.mudancas.contains_key(&setting(d))
                && fatos
                    .valor_atual(setting(d))
                    .is_some_and(|v| corresponde(&v, &g.esperado))
        })
        .map(|(d, _)| AcaoUsuario::Restaurar { destino: d.clone() })
        .collect();
    if !restaurar.is_empty() {
        return Some((OperacaoUsuario::Restaurar, restaurar));
    }
    let aplicar: Vec<_> = plano
        .resumo
        .mudancas
        .values()
        .filter(|m| !corresponde(&m.atual, &m.valor))
        .filter_map(|m| {
            ajuste(m.alvo_tela.as_deref(), &m.atual, &m.valor)
                .map(|ajuste| AcaoUsuario::Aplicar { ajuste })
        })
        .collect();
    (!aplicar.is_empty()).then_some((OperacaoUsuario::Aplicar, aplicar))
}

pub(super) fn ajuste(
    alvo: Option<&str>,
    antes: &ValorUsuario,
    depois: &ValorUsuario,
) -> Option<AjusteUsuario> {
    let token = |v: &ValorUsuario| match v {
        ValorUsuario::Token(s) => Some(Some(s.clone())),
        ValorUsuario::Ausente => Some(None),
        _ => None,
    };
    let dword = |v: &ValorUsuario| match v {
        ValorUsuario::Dword(v) => Some(Some(*v)),
        ValorUsuario::Ausente => Some(None),
        _ => None,
    };
    Some(match (antes, depois) {
        (ValorUsuario::Refresh(a), ValorUsuario::Refresh(b)) => AjusteUsuario::Refresh {
            alvo: alvo?.into(),
            antes: taxa_ipc(*a),
            depois: taxa_ipc(*b),
        },
        (ValorUsuario::Spi(a), ValorUsuario::Spi(b)) => AjusteUsuario::MouseAceleracao {
            antes: spi_ipc(*a),
            depois: spi_ipc(*b),
        },
        _ if matches!(antes, ValorUsuario::Token(_))
            || matches!(depois, ValorUsuario::Token(_)) =>
        {
            AjusteUsuario::DxSwapEffectUpgrade {
                antes: token(antes)?,
                depois: token(depois)?,
            }
        }
        _ => AjusteUsuario::GameDvrCapturaHistorica {
            antes: dword(antes)?,
            depois: dword(depois)?,
        },
    })
}

pub(super) fn valores(ajuste: &AjusteUsuario) -> (ValorUsuario, ValorUsuario) {
    let spi = |m: &nvt_ipc::MouseSpi| ValorUsuario::Spi(spi_core(*m));
    let token = |v: &Option<String>| v.clone().map_or(ValorUsuario::Ausente, ValorUsuario::Token);
    let dword = |v: &Option<u32>| v.map_or(ValorUsuario::Ausente, ValorUsuario::Dword);
    match ajuste {
        AjusteUsuario::Refresh { antes, depois, .. } => (
            ValorUsuario::Refresh(taxa_core(*antes)),
            ValorUsuario::Refresh(taxa_core(*depois)),
        ),
        AjusteUsuario::MouseAceleracao { antes, depois } => (spi(antes), spi(depois)),
        AjusteUsuario::DxSwapEffectUpgrade { antes, depois } => (token(antes), token(depois)),
        AjusteUsuario::GameDvrCapturaHistorica { antes, depois } => (dword(antes), dword(depois)),
    }
}

pub(super) fn validar_resultados(
    s: &Sessao,
    tarefa: &TarefaUsuario,
    resultados: &[ResultadoUsuario],
) -> Result<(), &'static str> {
    let snapshot = tarefa.acoes == [AcaoUsuario::RestaurarSnapshot];
    if !snapshot && resultados.len() != tarefa.acoes.len() {
        return Err("relato não cobre todos os destinos da tarefa");
    }
    for r in resultados {
        let acao = tarefa
            .acoes
            .iter()
            .find(|a| match a {
                AcaoUsuario::Aplicar { ajuste } => ajuste.destino() == r.destino,
                AcaoUsuario::Restaurar { destino } | AcaoUsuario::Reverter { destino } => {
                    *destino == r.destino
                }
                AcaoUsuario::RestaurarSnapshot => true,
            })
            .ok_or("destino não faz parte da tarefa")?;
        if (r.status == StatusMudanca::Pendente && r.mudanca.is_some())
            || (r.status == StatusMudanca::Aplicada && r.mudanca.is_none())
        {
            return Err("resultado sem confirmação da mudança");
        }
        if let Some(mudanca) = &r.mudanca {
            let (antes, depois) = valores(mudanca);
            let tipo = setting(&r.destino);
            if !tipo.aceita(&antes) || !tipo.aceita(&depois) {
                return Err("valor inválido para o destino");
            }
            if matches!(r.status, StatusMudanca::Aplicada | StatusMudanca::JaNoValor) {
                match acao {
                    AcaoUsuario::Aplicar { ajuste } => {
                        let (previsto, alvo) = valores(ajuste);
                        if !corresponde(&depois, &alvo)
                            || (!corresponde(&antes, &previsto)
                                && !(r.status == StatusMudanca::JaNoValor
                                    && corresponde(&antes, &alvo)))
                        {
                            return Err("mudança não corresponde ao plano");
                        }
                    }
                    AcaoUsuario::Restaurar { .. } | AcaoUsuario::Reverter { .. } => {
                        let g = &s.gestao[&r.destino];
                        if !corresponde(&depois, &g.antes)
                            || (!corresponde(&antes, &g.esperado)
                                && !(r.status == StatusMudanca::JaNoValor
                                    && corresponde(&antes, &g.antes)))
                        {
                            return Err("restauração não corresponde ao valor anterior");
                        }
                    }
                    AcaoUsuario::RestaurarSnapshot => {}
                }
            }
        }
    }
    Ok(())
}

pub(super) fn nome(destino: &SettingUsuario) -> &'static str {
    match destino {
        SettingUsuario::Refresh { .. } => "tela_refresh",
        SettingUsuario::MouseAceleracao => "mouse_aceleracao",
        SettingUsuario::DxSwapEffectUpgrade => "dx_swap_effect_upgrade",
        SettingUsuario::GameDvrCapturaHistorica => "gamedvr_captura_historica",
    }
}

pub(super) fn valor_legivel(valor: &ValorUsuario) -> Option<String> {
    match valor {
        ValorUsuario::Ausente => None,
        ValorUsuario::Dword(v) => Some(v.to_string()),
        ValorUsuario::Token(v) => Some(v.clone()),
        ValorUsuario::Spi([a, b, c]) => Some(format!("{a}, {b}, {c}")),
        ValorUsuario::Refresh(t) => Some(format!("{}/{} Hz", t.numerador, t.denominador)),
    }
}

pub(super) fn entrada(
    id: u64,
    unix: u64,
    operacao: OperacaoUsuario,
    resultado: &ResultadoUsuario,
) -> EntradaHistorico {
    let (antes, depois) = resultado.mudanca.as_ref().map_or((None, None), |m| {
        let (a, d) = valores(m);
        (valor_legivel(&a), valor_legivel(&d))
    });
    let descricao = resultado.detalhe.clone().unwrap_or_else(|| {
        match resultado.status {
            StatusMudanca::Aplicada => "Mudança confirmada pelo agente",
            StatusMudanca::JaNoValor => "Já no valor solicitado",
            StatusMudanca::Travada => "Ajuste travado",
            StatusMudanca::Recusada => "Mudança recusada",
            StatusMudanca::Falhou => "Falha na mudança",
            StatusMudanca::Pendente => "Mudança pendente",
        }
        .into()
    });
    EntradaHistorico {
        camada: Camada::Usuario,
        id,
        quando_unix: unix,
        acao: match operacao {
            OperacaoUsuario::Aplicar => AcaoHistorico::Aplicacao,
            OperacaoUsuario::Restaurar => AcaoHistorico::Restauracao,
            OperacaoUsuario::Reverter => AcaoHistorico::Rollback,
        },
        perfil: alvo(&resultado.destino).map(str::to_owned),
        setting: Some(nome(&resultado.destino).into()),
        antes,
        depois,
        descricao,
    }
}

// ── Família jogo (Fase 3) ───────────────────────────────────────────────────
