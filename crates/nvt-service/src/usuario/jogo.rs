//! Configuração de jogo: visão para a UI, planejamento, validação e registro
//! das ações sobre arquivos de configuração dos jogos.

use super::*;

impl Coordenador {
    /// Projeção “Arquivo do jogo” por id da base curada, só da própria
    /// identidade (SID + sessão) e com agente presente.
    pub(crate) fn config_jogo_ui(
        &self,
        identidade: &IdentidadeCliente,
        agora: Instant,
    ) -> BTreeMap<String, ConfigJogoUi> {
        let Some(s) = self.sessoes.get(identidade).filter(|s| s.presente(agora)) else {
            return BTreeMap::new();
        };
        let plano = self.plano_jogo(&identidade.sid, s);
        let mut mapa: BTreeMap<String, ConfigJogoUi> = BTreeMap::new();
        for f in &s.fatos_jogo {
            let AlvoIpc::ConfigJogo { jogo, arquivo } = &f.alvo;
            let Some(def) = self
                .catalogo_jogo
                .arquivo(arquivo)
                .filter(|d| d.jogo.as_str() == jogo)
            else {
                continue;
            };
            let mut arquivo_ui = ArquivoJogoUi {
                alvo: f.alvo.clone(),
                rotulo: f.rotulo.clone(),
                instancia: f.instancia.clone(),
                conta: (!f.conta_ambigua).then(|| f.instancia.clone()),
                backup: f.backup,
                mudancas: Vec::new(),
                reversivel: false,
            };
            let mut orientacoes = Vec::new();
            if f.somente_leitura {
                orientacoes.push(OrientacaoConfigJogo::DesmarcarSomenteLeitura);
            }
            if f.na_nuvem {
                orientacoes.push(OrientacaoConfigJogo::AjustarNoJogo);
            }
            if f.conta_ambigua {
                orientacoes.push(OrientacaoConfigJogo::EscolherConta);
            }
            for chave in &def.chave {
                let Some(d) = DestinoJogo::novo(chave.setting, Some(&f.instancia)) else {
                    continue;
                };
                let falha = s.falhas_jogo.get(&d);
                if let Some(MotivoNaoAplicada::JanelaFechadaExigida { processos }) =
                    falha.and_then(|f| f.motivo.as_ref())
                {
                    orientacoes.extend(orientacoes_janela(processos));
                }
                let gestao = s.gestao_jogo.get(&d);
                arquivo_ui.reversivel |= gestao.is_some();
                let (antes, depois, status) = if let Some(m) = plano.mudancas.get(&d) {
                    let status = if m.atual == m.valor {
                        StatusMudanca::JaNoValor
                    } else {
                        StatusMudanca::Pendente
                    };
                    (m.atual.clone(), m.valor.clone(), status)
                } else if let Some(g) = gestao {
                    let status = if s.travas_jogo.contains(&d) {
                        StatusMudanca::Travada
                    } else {
                        StatusMudanca::Aplicada
                    };
                    (g.antes.clone(), g.esperado.clone(), status)
                } else {
                    continue;
                };
                if arquivo_ui.mudancas.len() < nvt_ipc::CHAVES_ARQUIVO_MAX {
                    arquivo_ui.mudancas.push(MudancaArquivoJogo {
                        setting: chave.setting.codigo().into(),
                        antes: valor_ipc(&antes),
                        depois: valor_ipc(&depois),
                        status: falha.map_or(status, |f| f.status),
                        motivo: falha.and_then(|f| f.motivo.clone()),
                    });
                }
            }
            let ui = mapa.entry(jogo.clone()).or_default();
            for o in orientacoes {
                if !ui.orientacoes.contains(&o) && ui.orientacoes.len() < 16 {
                    ui.orientacoes.push(o);
                }
            }
            if ui.arquivos.len() < nvt_ipc::ARQUIVOS_JOGO_MAX {
                ui.arquivos.push(arquivo_ui);
            }
        }
        let vazio = BTreeSet::new();
        let aceites = self
            .dados
            .por_sid
            .get(&identidade.sid)
            .map_or(&vazio, |d| &d.aceites);
        for regra in self
            .catalogo_jogo
            .regras()
            .iter()
            .filter(|r| r.status == Status::Experimental)
        {
            let estado = if aceites.contains(&regra.id) {
                EstadoExperimental::Aceita
            } else if plano.estado_experimental.contains_key(&regra.id) {
                EstadoExperimental::Aceitavel
            } else {
                continue;
            };
            let Some(jogo) = regra.aplicar.keys().next().map(|s| s.jogo()) else {
                continue;
            };
            let destinos: Vec<DestinoJogoIpc> = plano
                .experimentais
                .iter()
                .filter(|e| e.regra == regra.id)
                .flat_map(|e| e.mudancas.iter())
                .chain(plano.mudancas.values().filter(|m| m.regra == regra.id))
                .map(|m| destino_ipc(&m.destino))
                .filter(|d| d.validar().is_ok())
                .take(nvt_ipc::CHAVES_ARQUIVO_MAX)
                .collect();
            let ui = mapa.entry(jogo.to_owned()).or_default();
            if ui.experimentais.len() < nvt_ipc::ACOES_JOGO_MAX {
                ui.experimentais.push(nvt_ipc::SugestaoConfigJogo {
                    regra: regra.id.clone(),
                    descricao: regra.descricao.clone(),
                    impacto_visual: impacto(regra.impacto_visual),
                    destinos,
                    estado,
                });
            }
        }
        mapa
    }

    /// Plano da família jogo de uma sessão com os aceites do SID.
    pub(super) fn plano_jogo(&self, sid: &str, s: &Sessao) -> PlanoJogo {
        let vazio = BTreeSet::new();
        let aceites = self.dados.por_sid.get(sid).map_or(&vazio, |d| &d.aceites);
        avaliar_config_jogo(
            &s.fatos_jogo_core(),
            &self.prefs,
            s.refresh_hz,
            &self.catalogo_jogo,
            aceites,
        )
    }
}

/// Destino do catálogo: setting conhecida, do jogo e do arquivo do alvo, com
/// a instância opaca como conta.
pub(super) fn destino_de(
    jogo: &str,
    arquivo: &str,
    setting: &str,
    instancia: &str,
    catalogo: &CatalogoConfigJogo,
) -> Option<DestinoJogo> {
    let setting = SettingJogo::de_codigo(setting)?;
    let def = catalogo.arquivo(arquivo)?;
    if setting.jogo() != jogo || setting.arquivo() != arquivo || def.jogo.as_str() != jogo {
        return None;
    }
    catalogo.chave(setting)?;
    DestinoJogo::novo(setting, Some(instancia)).filter(DestinoJogo::coerente)
}

pub(super) fn destino_core(
    destino: &DestinoJogoIpc,
    catalogo: &CatalogoConfigJogo,
) -> Option<DestinoJogo> {
    let AlvoIpc::ConfigJogo { jogo, arquivo } = &destino.alvo;
    destino_de(
        jogo,
        arquivo,
        &destino.setting,
        &destino.instancia,
        catalogo,
    )
}

pub(super) fn destino_ipc(destino: &DestinoJogo) -> DestinoJogoIpc {
    DestinoJogoIpc {
        alvo: AlvoIpc::ConfigJogo {
            jogo: destino.instancia.jogo.as_str().into(),
            arquivo: destino.instancia.arquivo.clone(),
        },
        setting: destino.setting.codigo().into(),
        instancia: destino.instancia.conta.clone().unwrap_or_default(),
    }
}

pub(super) fn valor_core(valor: &ValorJogoIpc) -> ValorJogo {
    match valor {
        ValorJogoIpc::Ausente => ValorJogo::Ausente,
        ValorJogoIpc::Inteiro(n) => ValorJogo::Inteiro(*n),
        ValorJogoIpc::Booleano(b) => ValorJogo::Booleano(*b),
        ValorJogoIpc::Texto(t) => ValorJogo::Texto(t.clone()),
        ValorJogoIpc::TokenLancamento(n) => ValorJogo::TokenLancamento(*n),
    }
}

pub(super) fn valor_ipc(valor: &ValorJogo) -> ValorJogoIpc {
    match valor {
        ValorJogo::Ausente => ValorJogoIpc::Ausente,
        ValorJogo::Inteiro(n) => ValorJogoIpc::Inteiro(*n),
        ValorJogo::Booleano(b) => ValorJogoIpc::Booleano(*b),
        ValorJogo::Texto(t) => ValorJogoIpc::Texto(t.clone()),
        ValorJogo::TokenLancamento(n) => ValorJogoIpc::TokenLancamento(*n),
    }
}

/// Leituras do agente convertidas; entradas fora do catálogo ou de tipo
/// inválido são ignoradas (leitura desconhecida).
pub(super) fn destinos_jogo(
    fatos: &[FatoConfigJogo],
    catalogo: &CatalogoConfigJogo,
) -> BTreeMap<DestinoJogo, EstadoDestinoJogo> {
    let mut destinos = BTreeMap::new();
    for f in fatos {
        let AlvoIpc::ConfigJogo { jogo, arquivo } = &f.alvo;
        for v in &f.valores {
            let Some(d) = destino_de(jogo, arquivo, &v.setting, &f.instancia, catalogo) else {
                tracing::warn!("leitura de jogo fora do catálogo ignorada");
                continue;
            };
            let def = catalogo.chave(d.setting).expect("chave conferida");
            let atual = match &v.valor {
                _ if !f.presente => None,
                Fato::Conhecido(valor) => Some(valor_core(valor)),
                Fato::Ausente => Some(ValorJogo::Ausente),
                Fato::Desconhecido => None,
            }
            .filter(|a| def.aceita_leitura(a));
            destinos.insert(
                d,
                EstadoDestinoJogo {
                    atual,
                    somente_leitura: f.somente_leitura,
                    na_nuvem: f.na_nuvem,
                    conta_ambigua: f.conta_ambigua,
                    hash: f.hash.clone(),
                },
            );
        }
    }
    destinos
}

/// Reconstrói a gestão de jogo persistida pelo agente (só o que vem desta
/// identidade, validado contra o catálogo e os tipos), sem apagar nada.
pub(super) fn reidratar_jogo(
    s: &mut Sessao,
    fatos: &[FatoConfigJogo],
    catalogo: &CatalogoConfigJogo,
) -> bool {
    let mut mudou = false;
    for f in fatos {
        let AlvoIpc::ConfigJogo { jogo, arquivo } = &f.alvo;
        for g in &f.gestao {
            let Some(d) = destino_de(jogo, arquivo, &g.setting, &f.instancia, catalogo) else {
                tracing::warn!("gestão de jogo fora do catálogo ignorada");
                continue;
            };
            let def = catalogo.chave(d.setting).expect("chave conferida");
            let (antes, esperado) = (valor_core(&g.antes), valor_core(&g.esperado));
            if def.aceita_leitura(&antes) && def.aceita_leitura(&esperado) {
                let backup = s.gestao_jogo.get(&d).and_then(|a| a.backup.clone());
                let nova = GestaoJogo {
                    antes,
                    esperado,
                    hash_antes: Some(g.hash_antes.clone()),
                    hash_depois: Some(g.hash_depois.clone()),
                    backup,
                };
                if s.gestao_jogo.get(&d) != Some(&nova) {
                    s.gestao_jogo.insert(d.clone(), nova);
                    mudou = true;
                }
            }
            if g.travado {
                mudou |= s.travas_jogo.insert(d);
            }
        }
    }
    mudou
}

/// Leitura atual e hash do arquivo, exigidos para qualquer ação.
pub(super) fn leitura_jogo<'a>(
    s: &'a Sessao,
    d: &DestinoJogo,
) -> Option<(&'a ValorJogo, &'a String)> {
    let e = s.destinos_jogo.get(d)?;
    Some((e.atual.as_ref()?, e.hash.as_ref()?))
}

/// Restauração por geração: tudo o que o app gere volta ao original.
pub(super) fn restaurar_jogo(s: &Sessao) -> Option<(OperacaoUsuario, Vec<AcaoJogo>)> {
    let acoes: Vec<_> = s
        .gestao_jogo
        .keys()
        .filter_map(|d| {
            let (_, hash) = leitura_jogo(s, d)?;
            Some(AcaoJogo::Restaurar {
                destino: destino_ipc(d),
                hash_esperado: hash.clone(),
            })
        })
        .take(nvt_ipc::ACOES_JOGO_MAX)
        .collect();
    (!acoes.is_empty()).then_some((OperacaoUsuario::Restaurar, acoes))
}

pub(super) fn planejar_jogo(
    s: &Sessao,
    plano: &PlanoJogo,
    pausado: bool,
) -> Option<(OperacaoUsuario, Vec<AcaoJogo>)> {
    let limitar = |acoes: Vec<AcaoJogo>| {
        acoes
            .into_iter()
            .take(nvt_ipc::ACOES_JOGO_MAX)
            .collect::<Vec<_>>()
    };
    let reverter: Vec<_> = s
        .reversoes_jogo
        .iter()
        .filter(|d| s.gestao_jogo.contains_key(*d))
        .filter_map(|d| {
            let (_, hash) = leitura_jogo(s, d)?;
            Some(AcaoJogo::Reverter {
                destino: destino_ipc(d),
                hash_esperado: hash.clone(),
            })
        })
        .collect();
    if !reverter.is_empty() {
        return Some((OperacaoUsuario::Reverter, limitar(reverter)));
    }
    if pausado {
        return None;
    }
    let restaurar: Vec<_> = s
        .gestao_jogo
        .iter()
        .filter(|(d, _)| !plano.mudancas.contains_key(*d))
        .filter_map(|(d, g)| {
            let (atual, hash) = leitura_jogo(s, d)?;
            (*atual == g.esperado).then(|| AcaoJogo::Restaurar {
                destino: destino_ipc(d),
                hash_esperado: hash.clone(),
            })
        })
        .collect();
    if !restaurar.is_empty() {
        return Some((OperacaoUsuario::Restaurar, limitar(restaurar)));
    }
    let aplicar: Vec<_> = plano
        .mudancas
        .values()
        .filter(|m| m.atual != m.valor)
        .filter_map(|m| {
            let (_, hash) = leitura_jogo(s, &m.destino)?;
            let arquivo = s.arquivo_jogo(&m.destino)?;
            let repetiria = s
                .falhas_jogo
                .get(&m.destino)
                .is_some_and(|f| !f.adiamento() && f.hash == *hash);
            (arquivo.presente
                && arquivo.backup != nvt_ipc::EstadoBackup::Comprometido
                && !repetiria)
                .then(|| AcaoJogo::Aplicar {
                    destino: destino_ipc(&m.destino),
                    valor: valor_ipc(&m.valor),
                    hash_esperado: hash.clone(),
                })
        })
        .collect();
    (!aplicar.is_empty()).then(|| (OperacaoUsuario::Aplicar, limitar(aplicar)))
}

/// Confere o relato com a tarefa de jogo em voo: cobre todas as ações, cada
/// destino no catálogo, tipos aceitos e hash/valores coerentes com o plano.
pub(super) fn validar_resultados_jogo(
    s: &Sessao,
    resultados: &[ResultadoJogo],
    catalogo: &CatalogoConfigJogo,
) -> Result<Vec<DestinoJogo>, &'static str> {
    let tarefa = s
        .em_voo_jogo
        .as_ref()
        .ok_or("tarefa de jogo desconhecida")?;
    if resultados.len() != tarefa.acoes.len() {
        return Err("relato não cobre todos os destinos da tarefa de jogo");
    }
    let mut destinos = Vec::with_capacity(resultados.len());
    for r in resultados {
        let d = destino_core(&r.destino, catalogo).ok_or("destino de jogo fora do catálogo")?;
        let acao = tarefa
            .acoes
            .iter()
            .find(|a| *a.destino() == r.destino)
            .ok_or("destino não faz parte da tarefa de jogo")?;
        let def = catalogo.chave(d.setting).expect("chave conferida");
        let antes = r.antes.as_ref().map(valor_core);
        let depois = r.depois.as_ref().map(valor_core);
        if [&antes, &depois]
            .into_iter()
            .flatten()
            .any(|v| !def.aceita_leitura(v))
        {
            return Err("valor inválido para o destino de jogo");
        }
        let sucesso = matches!(r.status, StatusMudanca::Aplicada | StatusMudanca::JaNoValor);
        if sucesso && r.motivo.is_some() {
            return Err("resultado de sucesso com motivo de falha");
        }
        if r.status == StatusMudanca::Aplicada
            && (antes.is_none() || depois.is_none() || r.hash_depois.is_none())
        {
            return Err("resultado sem confirmação da mudança de jogo");
        }
        if sucesso {
            let (alvo, hash_esperado) = match acao {
                AcaoJogo::Aplicar {
                    valor,
                    hash_esperado,
                    ..
                } => (valor_core(valor), hash_esperado),
                AcaoJogo::Restaurar { hash_esperado, .. }
                | AcaoJogo::Reverter { hash_esperado, .. } => (
                    s.gestao_jogo
                        .get(&d)
                        .ok_or("restauração de jogo sem valor anterior")?
                        .antes
                        .clone(),
                    hash_esperado,
                ),
            };
            if depois.as_ref().is_some_and(|v| *v != alvo) {
                return Err("mudança de jogo não corresponde ao plano");
            }
            if r.status == StatusMudanca::Aplicada && r.hash_antes.as_ref() != Some(hash_esperado) {
                return Err("hash do arquivo de jogo diferente do esperado");
            }
        }
        destinos.push(d);
    }
    Ok(destinos)
}

pub(super) fn hash_da_acao(acao: &AcaoJogo) -> &String {
    match acao {
        AcaoJogo::Aplicar { hash_esperado, .. }
        | AcaoJogo::Restaurar { hash_esperado, .. }
        | AcaoJogo::Reverter { hash_esperado, .. } => hash_esperado,
    }
}

pub(super) fn registrar_jogo(
    s: &mut Sessao,
    tarefa: &TarefaJogo,
    r: &ResultadoJogo,
    d: DestinoJogo,
) {
    let operacao = tarefa.tarefa.operacao;
    // A releitura do agente após a escrita vale até a próxima consulta.
    if matches!(r.status, StatusMudanca::Aplicada | StatusMudanca::JaNoValor) {
        if let Some(e) = s.destinos_jogo.get_mut(&d)
            && let Some(depois) = &r.depois
        {
            e.atual = Some(valor_core(depois));
        }
        if let Some(hash) = &r.hash_depois {
            for (outro, e) in &mut s.destinos_jogo {
                if outro.instancia == d.instancia {
                    e.hash = Some(hash.clone());
                }
            }
        }
    }
    match r.status {
        StatusMudanca::Aplicada | StatusMudanca::JaNoValor => {
            s.falhas_jogo.remove(&d);
        }
        StatusMudanca::Travada => {
            s.falhas_jogo.remove(&d);
            s.travas_jogo.insert(d.clone());
        }
        status => {
            if let Some(acao) = tarefa.acoes.iter().find(|a| *a.destino() == r.destino) {
                s.falhas_jogo.insert(
                    d.clone(),
                    FalhaJogo {
                        status,
                        motivo: r.motivo.clone(),
                        hash: hash_da_acao(acao).clone(),
                    },
                );
            }
        }
    }
    if operacao == OperacaoUsuario::Aplicar
        && r.status == StatusMudanca::Aplicada
        && let (Some(antes), Some(depois)) = (&r.antes, &r.depois)
    {
        let gestao = s
            .gestao_jogo
            .entry(d.clone())
            .or_insert_with(|| GestaoJogo {
                antes: valor_core(antes),
                esperado: valor_core(depois),
                hash_antes: r.hash_antes.clone(),
                hash_depois: None,
                backup: None,
            });
        gestao.esperado = valor_core(depois);
        gestao.hash_depois.clone_from(&r.hash_depois);
    }
    if operacao != OperacaoUsuario::Aplicar
        && matches!(
            r.status,
            StatusMudanca::Aplicada | StatusMudanca::JaNoValor | StatusMudanca::Travada
        )
    {
        s.gestao_jogo.remove(&d);
        s.reversoes_jogo.remove(&d);
    }
}

pub(super) fn orientacoes_janela(processos: &[String]) -> Vec<OrientacaoConfigJogo> {
    let (steam, jogo): (Vec<_>, Vec<_>) = processos
        .iter()
        .cloned()
        .partition(|p| p.eq_ignore_ascii_case("steam.exe"));
    let mut v = Vec::new();
    if !steam.is_empty() {
        v.push(OrientacaoConfigJogo::FecharSteam);
    }
    if !jogo.is_empty() {
        v.push(OrientacaoConfigJogo::FecharJogo { processos: jogo });
    }
    v
}

pub(super) fn valor_legivel_jogo(valor: &ValorJogoIpc) -> Option<String> {
    match valor {
        ValorJogoIpc::Ausente | ValorJogoIpc::TokenLancamento(None) => None,
        ValorJogoIpc::Inteiro(n) => Some(n.to_string()),
        ValorJogoIpc::Booleano(b) => Some(if *b { "True" } else { "False" }.into()),
        ValorJogoIpc::Texto(t) => Some(t.clone()),
        ValorJogoIpc::TokenLancamento(Some(n)) => {
            Some(format!("{} {n}", nvt_core::config_jogo::TOKEN_FPS_MAX))
        }
    }
}

pub(super) fn entrada_jogo(
    id: u64,
    unix: u64,
    operacao: OperacaoUsuario,
    r: &ResultadoJogo,
) -> EntradaHistorico {
    let AlvoIpc::ConfigJogo { jogo, .. } = &r.destino.alvo;
    EntradaHistorico {
        camada: Camada::Usuario,
        id,
        quando_unix: unix,
        acao: match operacao {
            OperacaoUsuario::Aplicar => AcaoHistorico::Aplicacao,
            OperacaoUsuario::Restaurar => AcaoHistorico::Restauracao,
            OperacaoUsuario::Reverter => AcaoHistorico::Rollback,
        },
        perfil: Some(jogo.clone()),
        setting: Some(r.destino.setting.clone()),
        antes: r.antes.as_ref().and_then(valor_legivel_jogo),
        depois: r.depois.as_ref().and_then(valor_legivel_jogo),
        descricao: match r.status {
            StatusMudanca::Aplicada => "Arquivo do jogo alterado e confirmado pelo agente",
            StatusMudanca::JaNoValor => "Arquivo do jogo já no valor solicitado",
            StatusMudanca::Travada => "Ajuste do jogo travado",
            StatusMudanca::Recusada => "Mudança no arquivo do jogo recusada",
            StatusMudanca::Falhou => "Falha na mudança do arquivo do jogo",
            StatusMudanca::Pendente => "Mudança no arquivo do jogo adiada",
        }
        .into(),
    }
}
