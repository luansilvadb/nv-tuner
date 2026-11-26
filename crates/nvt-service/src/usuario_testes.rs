use super::*;
use std::cell::RefCell;
use std::rc::Rc;

const CAPTURA: &str = "windows-captura-historica-off";
const DRS_A: &str = "sincronismo-gsync-limite";
const DRS_B: &str = "dlss-preset-e-ampere";

#[derive(Clone, Default)]
struct Memoria(Rc<RefCell<DadosUsuarios>>);

impl Armazenamento for Memoria {
    fn carregar(&self) -> io::Result<DadosUsuarios> {
        Ok(self.0.borrow().clone())
    }

    fn salvar(&self, dados: &DadosUsuarios) -> io::Result<()> {
        *self.0.borrow_mut() = dados.clone();
        Ok(())
    }
}

fn abrir(memoria: Memoria) -> Coordenador {
    Coordenador::abrir(
        memoria,
        &ConjuntoRegras::embutido().unwrap(),
        Some(22631),
        0,
    )
    .unwrap()
}

fn cliente(sid: &str, sessao: u32) -> IdentidadeCliente {
    IdentidadeCliente {
        sid: sid.into(),
        sessao,
    }
}

fn fatos(captura: u32) -> nvt_ipc::FatosUsuario {
    nvt_ipc::FatosUsuario {
        config_jogo: vec![],
        tela: Fato::Desconhecido,
        mouse: Fato::Desconhecido,
        swap_effect_upgrade: Fato::Desconhecido,
        captura_historica: Fato::Conhecido(captura),
        diagnosticos: vec![],
        gestao: vec![],
    }
}

fn consultar(
    c: &mut Coordenador,
    id: &IdentidadeCliente,
    agora: Instant,
    geracao: u64,
    captura: u32,
) -> PlanoAgente {
    let req = Requisicao::AgenteConsultar {
        fatos: fatos(captura),
        ultima_tarefa: None,
        geracao_restaurada: geracao,
    };
    let Some(Resposta::PlanoAgente(plano)) = c.tratar(id, &req, agora, 12) else {
        panic!("esperado plano do agente");
    };
    plano
}

fn aceite(c: &mut Coordenador, id: &IdentidadeCliente, agora: Instant, regra: &str, aceito: bool) {
    let req = Requisicao::DefinirAceiteExperimental {
        regra: regra.into(),
        aceito,
    };
    assert_eq!(c.tratar(id, &req, agora, 12), Some(Resposta::Ok));
}

fn relato(
    tarefa: &TarefaUsuario,
    status: StatusMudanca,
    mudanca: Option<AjusteUsuario>,
) -> Requisicao {
    let ReferenciaTarefa {
        id_tarefa,
        operacao,
        revisao,
    } = tarefa.tarefa;
    Requisicao::AgenteRelatar {
        resultados_jogo: vec![],
        id_tarefa,
        operacao,
        revisao,
        resultados: vec![ResultadoUsuario {
            destino: SettingUsuario::GameDvrCapturaHistorica,
            status,
            detalhe: None,
            mudanca,
        }],
    }
}

fn captura(antes: u32, depois: u32) -> AjusteUsuario {
    AjusteUsuario::GameDvrCapturaHistorica {
        antes: Some(antes),
        depois: Some(depois),
    }
}

fn recusado(r: Option<Resposta>) -> bool {
    matches!(
        r,
        Some(Resposta::Erro {
            codigo: CodigoErro::RequisicaoInvalida,
            ..
        })
    )
}

fn historico(c: &Coordenador, id: &IdentidadeCliente, agora: Instant) -> Vec<EntradaHistorico> {
    let mut r = Resposta::Historico { entradas: vec![] };
    c.projetar(id, &mut r, 10, agora);
    let Resposta::Historico { entradas } = r else {
        unreachable!()
    };
    entradas
}

fn plano_projetado(c: &Coordenador, id: &IdentidadeCliente, agora: Instant) -> ResumoSistema {
    let mut r = Resposta::Plano(nvt_ipc::Plano {
        sistema: ResumoSistema::default(),
        sugestoes_driver: vec![],
        gerado_unix: 12,
        perfis: vec![],
        reversoes: vec![],
        ignorados: vec![],
        jogos: vec![],
        adiada: None,
    });
    c.projetar(id, &mut r, 0, agora);
    let Resposta::Plano(p) = r else {
        unreachable!()
    };
    p.sistema
}

fn estado(c: &Coordenador, id: &IdentidadeCliente, agora: Instant) -> EstadoAgente {
    let mut r = Resposta::Estado(nvt_ipc::Estado {
        agente: EstadoAgente::default(),
        diagnosticos: vec![],
        versao_servico: String::new(),
        driver: None,
        gpu: None,
        preferencias: crate::nucleo::prefs_ipc(&Default::default()),
        jogos: 0,
        ultima_aplicacao: None,
        alertas_deriva: vec![],
        avisos: vec![],
        adiamento: None,
        atualizacao: Default::default(),
    });
    c.projetar(id, &mut r, 0, agora);
    let Resposta::Estado(e) = r else {
        unreachable!()
    };
    e.agente
}

#[test]
fn isolamento_presenca_e_historico_por_identidade() {
    let mut c = abrir(Memoria::default());
    let agora = Instant::now();
    let a = cliente("A", 1);
    let rdp = cliente("A", 2);
    let b = cliente("B", 1);
    aceite(&mut c, &a, agora, CAPTURA, true);
    let ta = consultar(&mut c, &a, agora, 0, 1).tarefa.unwrap();
    assert_eq!(plano_projetado(&c, &a, agora).mudancas.len(), 1);
    assert!(plano_projetado(&c, &rdp, agora).mudancas.is_empty());
    assert!(plano_projetado(&c, &b, agora).sugestoes.is_empty());
    assert!(estado(&c, &a, agora + Duration::from_secs(179)).presente);
    assert!(!estado(&c, &a, agora + Duration::from_secs(180)).presente);
    assert!(!estado(&c, &rdp, agora).presente);
    assert!(!estado(&c, &b, agora).presente);
    assert!(consultar(&mut c, &b, agora, 0, 1).tarefa.is_none());
    let tr = consultar(&mut c, &rdp, agora, 0, 1).tarefa.unwrap();
    assert_ne!(ta.tarefa.id_tarefa, tr.tarefa.id_tarefa);
    let req = relato(&ta, StatusMudanca::Aplicada, Some(captura(1, 0)));
    assert!(recusado(c.tratar(&rdp, &req, agora, 12)));
    assert!(recusado(c.tratar(&b, &req, agora, 12)));
    assert_eq!(c.tratar(&a, &req, agora, 12), Some(Resposta::Ok));
    assert_eq!(historico(&c, &a, agora), historico(&c, &rdp, agora));
    assert!(historico(&c, &b, agora).is_empty());
}

#[test]
fn mudanca_de_console_e_aceites_persistidos() {
    let memoria = Memoria::default();
    let mut c = abrir(memoria.clone());
    let a = cliente("A", 1);
    let b = cliente("B", 2);
    let agora = Instant::now();
    aceite(&mut c, &a, agora, DRS_A, true);
    aceite(&mut c, &b, agora, DRS_B, true);
    aceite(&mut c, &a, agora, CAPTURA, true);
    let mut c = abrir(memoria);
    assert_eq!(c.aceites_drs(Some("A")), [DRS_A.into()].into());
    assert_eq!(c.aceites_drs(Some("B")), [DRS_B.into()].into());
    assert!(c.aceites_drs(None).is_empty());
    assert!(c.aceites_drs(Some("C")).is_empty());
    assert!(consultar(&mut c, &a, agora, 0, 1).tarefa.is_some());
    assert!(consultar(&mut c, &b, agora, 0, 1).tarefa.is_none());
    assert_eq!(
        c.resumo(&a, None).sugestoes[0].estado,
        EstadoExperimental::Aceita
    );
    assert!(c.resumo(&b, None).sugestoes.is_empty());
}

#[test]
fn replay_idempotente_duravel_e_validacao_de_revisao_operacao_e_valores() {
    let memoria = Memoria::default();
    let mut c = abrir(memoria.clone());
    let a = cliente("A", 1);
    let agora = Instant::now();
    aceite(&mut c, &a, agora, CAPTURA, true);
    let tarefa = consultar(&mut c, &a, agora, 0, 1).tarefa.unwrap();
    c.atualizar_processos(["cs2.exe".into()].into());
    let velha = relato(&tarefa, StatusMudanca::Aplicada, Some(captura(1, 0)));
    assert!(recusado(c.tratar(&a, &velha, agora, 12)));
    let atual = consultar(&mut c, &a, agora, 0, 1);
    assert!(atual.revisao > tarefa.tarefa.revisao);
    assert_eq!(atual.processos_vigiados, ["cs2.exe".into()].into());
    let atual = atual.tarefa.unwrap();
    let mut errada = atual.clone();
    errada.tarefa.operacao = OperacaoUsuario::Reverter;
    let errada = relato(&errada, StatusMudanca::Aplicada, Some(captura(1, 0)));
    let valor_errado = relato(&atual, StatusMudanca::Aplicada, Some(captura(1, 1)));
    assert!(recusado(c.tratar(&a, &errada, agora, 12)));
    assert!(recusado(c.tratar(&a, &valor_errado, agora, 12)));

    let consulta = Requisicao::AgenteConsultar {
        fatos: fatos(0),
        geracao_restaurada: 0,
        ultima_tarefa: Some(EstadoTarefa {
            tarefa: atual.tarefa,
            progresso: ProgressoTarefa::RelatoPendente,
        }),
    };
    let Some(Resposta::PlanoAgente(plano)) = c.tratar(&a, &consulta, agora, 12) else {
        panic!()
    };
    assert_eq!(plano.revisao, atual.tarefa.revisao);
    assert!(plano.tarefa.is_none());
    let req = relato(&atual, StatusMudanca::Aplicada, Some(captura(1, 0)));
    assert_eq!(c.tratar(&a, &req, agora, 12), Some(Resposta::Ok));
    assert_eq!(c.tratar(&a, &req, agora, 13), Some(Resposta::Ok));
    let mut depois = abrir(memoria);
    assert_eq!(depois.tratar(&a, &req, agora, 14), Some(Resposta::Ok));
    assert!(recusado(depois.tratar(&a, &valor_errado, agora, 14)));
    assert_eq!(historico(&depois, &a, agora).len(), 1);
}

#[test]
fn restauracao_precede_aplicacao_ate_confirmacao_da_geracao() {
    let mut c = abrir(Memoria::default());
    let a = cliente("A", 1);
    let agora = Instant::now();
    aceite(&mut c, &a, agora, CAPTURA, true);
    let anterior = consultar(&mut c, &a, agora, 0, 1).tarefa.unwrap();
    c.atualizar_restauracao(1, true);
    c.atualizar_restauracao(1, true);
    assert_eq!(c.geracao(), 1);
    assert!(estado(&c, &a, agora).restauracao_pendente);
    assert!(recusado(c.tratar(
        &a,
        &relato(&anterior, StatusMudanca::Aplicada, Some(captura(1, 0))),
        agora,
        12
    )));
    c.atualizar_restauracao(1, false);
    c.atualizar_restauracao(0, true);
    let mut desconhecidos = fatos(0);
    desconhecidos.captura_historica = Fato::Desconhecido;
    let req = Requisicao::AgenteConsultar {
        fatos: desconhecidos,
        ultima_tarefa: None,
        geracao_restaurada: 0,
    };
    let Some(Resposta::PlanoAgente(sem_fatos)) = c.tratar(&a, &req, agora, 12) else {
        panic!("esperado plano sem fatos");
    };
    assert_eq!(sem_fatos.geracao_restauracao, 1);
    assert!(sem_fatos.tarefa.is_none());
    let plano = consultar(&mut c, &a, agora, 0, 1);
    assert_eq!(plano.geracao_restauracao, 1);
    let tarefa = plano.tarefa.unwrap();
    assert_eq!(tarefa.acoes, vec![AcaoUsuario::RestaurarSnapshot]);
    let adiada = relato(&tarefa, StatusMudanca::Pendente, None);
    assert_eq!(c.tratar(&a, &adiada, agora, 12), Some(Resposta::Ok));
    assert!(estado(&c, &a, agora).restauracao_pendente);
    let falsa_confirmacao = Requisicao::AgenteConsultar {
        fatos: fatos(1),
        ultima_tarefa: None,
        geracao_restaurada: 1,
    };
    assert!(recusado(c.tratar(&a, &falsa_confirmacao, agora, 12)));
    let tarefa = consultar(&mut c, &a, agora, 0, 1).tarefa.unwrap();
    let r = relato(&tarefa, StatusMudanca::JaNoValor, None);
    assert_eq!(c.tratar(&a, &r, agora, 12), Some(Resposta::Ok));
    assert_eq!(
        consultar(&mut c, &a, agora, 1, 1)
            .tarefa
            .unwrap()
            .tarefa
            .operacao,
        OperacaoUsuario::Aplicar
    );
    assert!(!estado(&c, &a, agora).restauracao_pendente);
}

#[test]
fn retirada_e_reversao_usam_original_e_preservam_trava() {
    let mut c = abrir(Memoria::default());
    let a = cliente("A", 1);
    let agora = Instant::now();
    aceite(&mut c, &a, agora, CAPTURA, true);
    let t = consultar(&mut c, &a, agora, 0, 1).tarefa.unwrap();
    assert_eq!(
        c.tratar(
            &a,
            &relato(&t, StatusMudanca::Aplicada, Some(captura(1, 0))),
            agora,
            12
        ),
        Some(Resposta::Ok)
    );
    assert!(consultar(&mut c, &a, agora, 0, 0).tarefa.is_none());
    assert_eq!(
        c.resumo(&a, c.sessoes.get(&a)).mudancas[0].status,
        StatusMudanca::JaNoValor
    );
    aceite(&mut c, &a, agora, CAPTURA, false);
    let retirar = consultar(&mut c, &a, agora, 0, 0).tarefa.unwrap();
    assert_eq!(
        retirar.acoes,
        [AcaoUsuario::Restaurar {
            destino: SettingUsuario::GameDvrCapturaHistorica
        }]
    );
    aceite(&mut c, &a, agora, CAPTURA, true);
    assert_eq!(
        c.tratar(
            &a,
            &Requisicao::ReverterAjusteUsuario {
                alvo_tela: None,
                destino: SettingUsuario::GameDvrCapturaHistorica
            },
            agora,
            12
        ),
        Some(Resposta::Ok)
    );
    let t = consultar(&mut c, &a, agora, 0, 0).tarefa.unwrap();
    assert_eq!(t.tarefa.operacao, OperacaoUsuario::Reverter);
    assert!(recusado(c.tratar(
        &a,
        &relato(&t, StatusMudanca::Aplicada, Some(captura(0, 0))),
        agora,
        12
    )));
    assert_eq!(
        c.tratar(
            &a,
            &relato(&t, StatusMudanca::Aplicada, Some(captura(0, 1))),
            agora,
            12
        ),
        Some(Resposta::Ok)
    );
    assert!(consultar(&mut c, &a, agora, 0, 1).tarefa.is_none());
    assert_eq!(
        c.resumo(&a, c.sessoes.get(&a)).mudancas[0].status,
        StatusMudanca::Travada
    );
}

const REGRAS_USUARIO: [&str; 4] = [
    "windows-refresh-maximo",
    "windows-mouse-sem-aceleracao",
    "windows-otimizacoes-janela",
    CAPTURA,
];

fn fatos_base() -> nvt_ipc::FatosUsuario {
    nvt_ipc::FatosUsuario {
        config_jogo: vec![],
        tela: Fato::Conhecido(nvt_ipc::FatoTela {
            alvo: "monitor".into(),
            atual: nvt_ipc::RefreshRacional {
                numerador: 60,
                denominador: 1,
            },
            maximo: nvt_ipc::RefreshRacional {
                numerador: 144,
                denominador: 1,
            },
        }),
        mouse: Fato::Conhecido(nvt_ipc::MouseSpi {
            limiar1: 6,
            limiar2: 10,
            aceleracao: 1,
        }),
        swap_effect_upgrade: Fato::Ausente,
        captura_historica: Fato::Conhecido(1),
        diagnosticos: vec![],
        gestao: vec![],
    }
}

/// Fatos lidos pelo agente depois de `ajuste` (ou antes, com `depois` falso).
fn lido(
    ajuste: &AjusteUsuario,
    depois: bool,
    gestao: Option<&AjusteUsuario>,
) -> nvt_ipc::FatosUsuario {
    let mut f = fatos_base();
    let opcional = |v: Option<_>| v.map_or(Fato::Ausente, Fato::Conhecido);
    match ajuste.clone() {
        AjusteUsuario::Refresh {
            antes, depois: d, ..
        } => {
            if let Fato::Conhecido(t) = &mut f.tela {
                t.atual = if depois { d } else { antes };
            }
        }
        AjusteUsuario::MouseAceleracao { antes, depois: d } => {
            f.mouse = Fato::Conhecido(if depois { d } else { antes });
        }
        AjusteUsuario::DxSwapEffectUpgrade { antes, depois: d } => {
            f.swap_effect_upgrade = opcional(if depois { d } else { antes });
        }
        AjusteUsuario::GameDvrCapturaHistorica { antes, depois: d } => {
            f.captura_historica =
                (if depois { d } else { antes }).map_or(Fato::Ausente, Fato::Conhecido);
        }
    }
    f.gestao = gestao
        .map(|g| nvt_ipc::GestaoAgente {
            destino: g.destino(),
            valores: Some(g.clone()),
            travado: false,
        })
        .into_iter()
        .collect();
    f
}

fn inverso(ajuste: &AjusteUsuario) -> AjusteUsuario {
    match ajuste.clone() {
        AjusteUsuario::Refresh {
            alvo,
            antes,
            depois,
        } => AjusteUsuario::Refresh {
            alvo,
            antes: depois,
            depois: antes,
        },
        AjusteUsuario::MouseAceleracao { antes, depois } => AjusteUsuario::MouseAceleracao {
            antes: depois,
            depois: antes,
        },
        AjusteUsuario::DxSwapEffectUpgrade { antes, depois } => {
            AjusteUsuario::DxSwapEffectUpgrade {
                antes: depois,
                depois: antes,
            }
        }
        AjusteUsuario::GameDvrCapturaHistorica { antes, depois } => {
            AjusteUsuario::GameDvrCapturaHistorica {
                antes: depois,
                depois: antes,
            }
        }
    }
}

fn consultar_com(
    c: &mut Coordenador,
    id: &IdentidadeCliente,
    agora: Instant,
    fatos: nvt_ipc::FatosUsuario,
) -> Option<TarefaUsuario> {
    let req = Requisicao::AgenteConsultar {
        fatos,
        ultima_tarefa: None,
        geracao_restaurada: 0,
    };
    let Some(Resposta::PlanoAgente(plano)) = c.tratar(id, &req, agora, 12) else {
        panic!("esperado plano do agente");
    };
    plano.tarefa
}

fn relatar(
    c: &mut Coordenador,
    id: &IdentidadeCliente,
    agora: Instant,
    t: &TarefaUsuario,
    ajuste: &AjusteUsuario,
) -> Option<Resposta> {
    let ReferenciaTarefa {
        id_tarefa,
        operacao,
        revisao,
    } = t.tarefa;
    let req = Requisicao::AgenteRelatar {
        resultados_jogo: vec![],
        id_tarefa,
        operacao,
        revisao,
        resultados: vec![ResultadoUsuario {
            destino: ajuste.destino(),
            status: StatusMudanca::Aplicada,
            detalhe: None,
            mudanca: Some(ajuste.clone()),
        }],
    };
    c.tratar(id, &req, agora, 12)
}

/// Aceita, aplica e devolve o ajuste efetivamente gravado pelo agente.
fn aplicar_regra(
    c: &mut Coordenador,
    id: &IdentidadeCliente,
    agora: Instant,
    regra: &str,
) -> AjusteUsuario {
    aceite(c, id, agora, regra, true);
    let t = consultar_com(c, id, agora, fatos_base()).expect("tarefa de aplicação");
    let [AcaoUsuario::Aplicar { ajuste }] = t.acoes.as_slice() else {
        panic!("esperada uma aplicação: {:?}", t.acoes);
    };
    let ajuste = ajuste.clone();
    assert_eq!(relatar(c, id, agora, &t, &ajuste), Some(Resposta::Ok));
    ajuste
}

fn pedido_reverter(destino: &SettingUsuario) -> Requisicao {
    Requisicao::ReverterAjusteUsuario {
        alvo_tela: alvo(destino).map(str::to_owned),
        destino: destino.clone(),
    }
}

#[test]
fn gestao_reidratada_do_agente_restaura_apos_reinicio_e_nova_sessao() {
    let agora = Instant::now();
    for regra in REGRAS_USUARIO {
        let memoria = Memoria::default();
        let a = cliente("A", 1);
        let ajuste = aplicar_regra(&mut abrir(memoria.clone()), &a, agora, regra);
        if regra == "windows-otimizacoes-janela" {
            assert!(
                matches!(
                    ajuste,
                    AjusteUsuario::DxSwapEffectUpgrade { antes: None, .. }
                ),
                "o valor de antes da gestão é a ausência"
            );
        }
        let depois = || lido(&ajuste, true, Some(&ajuste));
        let restaurado = inverso(&ajuste);

        // Sem a gestão do agente, o serviço reiniciado não teria o que restaurar.
        let mut sem = abrir(memoria.clone());
        aceite(&mut sem, &a, agora, regra, false);
        assert!(consultar_com(&mut sem, &a, agora, lido(&ajuste, true, None)).is_none());
        aceite(&mut sem, &a, agora, regra, true);

        // Reinício do serviço (ou reboot): retirar o aceite restaura.
        let mut reiniciado = abrir(memoria.clone());
        assert!(
            consultar_com(&mut reiniciado, &a, agora, depois()).is_none(),
            "{regra}"
        );
        aceite(&mut reiniciado, &a, agora, regra, false);
        let t = consultar_com(&mut reiniciado, &a, agora, depois()).expect(regra);
        assert_eq!(t.tarefa.operacao, OperacaoUsuario::Restaurar, "{regra}");
        assert_eq!(
            t.acoes,
            [AcaoUsuario::Restaurar {
                destino: ajuste.destino()
            }]
        );
        assert!(recusado(relatar(&mut reiniciado, &a, agora, &t, &ajuste)));
        assert_eq!(
            relatar(&mut reiniciado, &a, agora, &t, &restaurado),
            Some(Resposta::Ok),
            "{regra}"
        );
        assert!(consultar_com(&mut reiniciado, &a, agora, lido(&ajuste, false, None)).is_none());
        aceite(&mut reiniciado, &a, agora, regra, true);

        // Nova sessão (logoff/logon) no mesmo serviço.
        let mut c = abrir(memoria.clone());
        let ajuste = aplicar_regra(&mut c, &a, agora, regra);
        let a2 = cliente("A", 2);
        aceite(&mut c, &a2, agora, regra, false);
        let t = consultar_com(&mut c, &a2, agora, lido(&ajuste, true, Some(&ajuste))).expect(regra);
        assert_eq!(t.tarefa.operacao, OperacaoUsuario::Restaurar, "{regra}");
        assert_eq!(
            relatar(&mut c, &a2, agora, &t, &inverso(&ajuste)),
            Some(Resposta::Ok)
        );

        // Outro SID não herda a gestão do agente de A.
        let b = cliente("B", 3);
        assert!(recusado(c.tratar(
            &b,
            &pedido_reverter(&ajuste.destino()),
            agora,
            12
        )));
    }
}

#[test]
fn reverter_individual_apos_reinicio_usa_valor_do_agente() {
    let agora = Instant::now();
    for regra in REGRAS_USUARIO {
        let memoria = Memoria::default();
        let a = cliente("A", 1);
        let ajuste = aplicar_regra(&mut abrir(memoria.clone()), &a, agora, regra);
        let mut c = abrir(memoria);
        let reverter = pedido_reverter(&ajuste.destino());
        // Antes de o agente consultar, o serviço reiniciado não conhece a gestão.
        assert!(recusado(c.tratar(&a, &reverter, agora, 12)));
        assert!(consultar_com(&mut c, &a, agora, lido(&ajuste, true, Some(&ajuste))).is_none());
        assert_eq!(
            c.tratar(&a, &reverter, agora, 12),
            Some(Resposta::Ok),
            "{regra}"
        );
        let t = consultar_com(&mut c, &a, agora, lido(&ajuste, true, Some(&ajuste))).expect(regra);
        assert_eq!(t.tarefa.operacao, OperacaoUsuario::Reverter, "{regra}");
        assert!(recusado(relatar(&mut c, &a, agora, &t, &ajuste)));
        assert_eq!(
            relatar(&mut c, &a, agora, &t, &inverso(&ajuste)),
            Some(Resposta::Ok),
            "{regra}"
        );
    }
}

// ── Atualização automática: projeção por SID e `MarcarAtualizacaoVista` ────

fn estado_atualizado(para: &str) -> nvt_ipc::EstadoAtualizacao {
    nvt_ipc::EstadoAtualizacao {
        versao_instalada: para.into(),
        ultima_atualizacao: Some(nvt_ipc::AtualizacaoConcluida {
            de: "1.0.0".into(),
            para: para.into(),
            quando_unix: 10,
            reinicio_pendente: false,
        }),
        ..Default::default()
    }
}

fn nao_vista_projetada(
    c: &Coordenador,
    id: &IdentidadeCliente,
    e: &nvt_ipc::EstadoAtualizacao,
) -> bool {
    let mut r = Resposta::Estado(nvt_ipc::Estado {
        agente: Default::default(),
        diagnosticos: Vec::new(),
        versao_servico: "x".into(),
        driver: None,
        gpu: None,
        preferencias: crate::nucleo::prefs_ipc(&nvt_core::Preferences::default()),
        jogos: 0,
        ultima_aplicacao: None,
        alertas_deriva: Vec::new(),
        avisos: Vec::new(),
        adiamento: None,
        atualizacao: e.clone(),
    });
    c.projetar(id, &mut r, 0, Instant::now());
    let Resposta::Estado(estado) = r else {
        unreachable!()
    };
    assert_eq!(estado.atualizacao.ultima_atualizacao, e.ultima_atualizacao);
    estado.atualizacao.nao_vista
}

fn marcar(c: &mut Coordenador, id: &IdentidadeCliente, versao: &str) -> Option<Resposta> {
    c.tratar(
        id,
        &Requisicao::MarcarAtualizacaoVista {
            versao: versao.into(),
        },
        Instant::now(),
        12,
    )
}

#[test]
fn nao_vista_e_por_sid_e_persistida() {
    let memoria = Memoria::default();
    let mut c = abrir(memoria.clone());
    let (a, b) = (cliente("S-1-5-21-A", 1), cliente("S-1-5-21-B", 2));
    let sem = nvt_ipc::EstadoAtualizacao::default();
    assert!(
        !nao_vista_projetada(&c, &a, &sem),
        "sem atualização concluída"
    );
    let e = estado_atualizado("1.1.0");
    c.definir_atualizacao(e.clone());
    assert!(nao_vista_projetada(&c, &a, &e));
    assert!(nao_vista_projetada(&c, &b, &e));
    assert_eq!(marcar(&mut c, &a, "1.1.0"), Some(Resposta::Ok));
    assert!(!nao_vista_projetada(&c, &a, &e));
    assert!(nao_vista_projetada(&c, &b, &e), "outro SID ainda não viu");
    // Gravado por SID em usuarios.json: sobrevive ao reinício do coordenador.
    let c2 = abrir(memoria.clone());
    assert!(!nao_vista_projetada(&c2, &cliente("S-1-5-21-A", 7), &e));
    assert!(nao_vista_projetada(&c2, &b, &e));
    // A atualização seguinte volta a ser não vista.
    assert!(nao_vista_projetada(&c2, &a, &estado_atualizado("1.2.0")));
    // Repetir a marcação não regrava.
    let antes = serde_json::to_string(&*memoria.0.borrow()).unwrap();
    assert_eq!(marcar(&mut c, &a, "1.1.0"), Some(Resposta::Ok));
    assert_eq!(serde_json::to_string(&*memoria.0.borrow()).unwrap(), antes);
}

#[test]
fn marcar_atualizacao_vista_invalida() {
    let mut c = abrir(Memoria::default());
    let a = cliente("S-1-5-21-A", 1);
    let codigo = |r: Option<Resposta>| match r {
        Some(Resposta::Erro { codigo, .. }) => codigo,
        outra => panic!("esperado erro: {outra:?}"),
    };
    // Sem atualização concluída.
    assert_eq!(
        codigo(marcar(&mut c, &a, "1.1.0")),
        CodigoErro::EntradaInvalida
    );
    c.definir_atualizacao(estado_atualizado("1.1.0"));
    assert_eq!(
        codigo(marcar(&mut c, &a, "1.0.0")),
        CodigoErro::EntradaInvalida
    );
    assert_eq!(
        codigo(marcar(&mut c, &a, "1.2.0")),
        CodigoErro::EntradaInvalida
    );
    // Forma inválida.
    assert_eq!(
        codigo(marcar(&mut c, &a, "1 1")),
        CodigoErro::RequisicaoInvalida
    );
    assert!(nao_vista_projetada(&c, &a, &estado_atualizado("1.1.0")));
}

#[test]
fn sessoes_agente_so_com_presenca() {
    let mut c = abrir(Memoria::default());
    let agora = Instant::now();
    let (a, b) = (cliente("S-1-5-21-A", 1), cliente("S-1-5-21-B", 3));
    consultar(&mut c, &b, agora, 0, 1);
    consultar(&mut c, &a, agora, 0, 1);
    let esperado = [
        nvt_atualizacao::SessaoAgente {
            sessao: 1,
            sid: a.sid.clone(),
        },
        nvt_atualizacao::SessaoAgente {
            sessao: 3,
            sid: b.sid.clone(),
        },
    ];
    assert_eq!(c.sessoes_agente(agora), esperado);
    assert!(c.sessoes_agente(agora + PRESENCA).is_empty());
}

// ── Família jogo (Fase 3, L2-d) ─────────────────────────────────────────────

const R6: &str = "jogo-limite-no-jogo-r6";
const ARQ_R6: &str = "r6-game-settings";

fn h(c: char) -> String {
    c.to_string().repeat(64)
}

fn destino_r6(instancia: &str) -> DestinoJogoIpc {
    DestinoJogoIpc {
        alvo: AlvoIpc::ConfigJogo {
            jogo: "r6-siege".into(),
            arquivo: ARQ_R6.into(),
        },
        setting: "r6_fps_limit".into(),
        instancia: instancia.into(),
    }
}

fn fato_r6(atual: i64, hash: &str, gestao: Vec<nvt_ipc::GestaoJogoIpc>) -> FatoConfigJogo {
    FatoConfigJogo {
        alvo: destino_r6("conta1").alvo,
        instancia: "conta1".into(),
        rotulo: nvt_ipc::RotuloCaminho(
            "%USERPROFILE%\\Documents\\My Games\\Rainbow Six - Siege\\conta1\\GameSettings.ini"
                .into(),
        ),
        presente: true,
        hash: Some(hash.into()),
        somente_leitura: false,
        na_nuvem: false,
        conta_ambigua: false,
        backup: nvt_ipc::EstadoBackup::Integro,
        valores: vec![nvt_ipc::ValorAtualJogo {
            setting: "r6_fps_limit".into(),
            valor: Fato::Conhecido(ValorJogoIpc::Inteiro(atual)),
        }],
        gestao,
    }
}

fn gestao_r6(antes: i64, esperado: i64) -> nvt_ipc::GestaoJogoIpc {
    nvt_ipc::GestaoJogoIpc {
        setting: "r6_fps_limit".into(),
        antes: ValorJogoIpc::Inteiro(antes),
        esperado: ValorJogoIpc::Inteiro(esperado),
        hash_antes: h('a'),
        hash_depois: h('b'),
        travado: false,
    }
}

fn consultar_jogo(
    c: &mut Coordenador,
    id: &IdentidadeCliente,
    agora: Instant,
    geracao: u64,
    jogo: Vec<FatoConfigJogo>,
) -> PlanoAgente {
    let mut f = fatos(0);
    f.config_jogo = jogo;
    let req = Requisicao::AgenteConsultar {
        fatos: f,
        ultima_tarefa: None,
        geracao_restaurada: geracao,
    };
    let Some(Resposta::PlanoAgente(plano)) = c.tratar(id, &req, agora, 12) else {
        panic!("esperado plano do agente");
    };
    plano
}

fn relato_jogo(tarefa: &TarefaJogo, resultados: Vec<ResultadoJogo>) -> Requisicao {
    let ReferenciaTarefa {
        id_tarefa,
        operacao,
        revisao,
    } = tarefa.tarefa;
    Requisicao::AgenteRelatar {
        id_tarefa,
        operacao,
        revisao,
        resultados: vec![],
        resultados_jogo: resultados,
    }
}

fn resultado(
    status: StatusMudanca,
    antes: i64,
    depois: i64,
    hash_antes: &str,
    motivo: Option<MotivoNaoAplicada>,
) -> ResultadoJogo {
    let sucesso = matches!(status, StatusMudanca::Aplicada | StatusMudanca::JaNoValor);
    ResultadoJogo {
        destino: destino_r6("conta1"),
        status,
        motivo,
        antes: Some(ValorJogoIpc::Inteiro(antes)),
        depois: sucesso.then_some(ValorJogoIpc::Inteiro(depois)),
        hash_antes: Some(hash_antes.into()),
        hash_depois: sucesso.then(|| h('b')),
        chaves_alteradas: vec![],
    }
}

fn com_limite(memoria: Memoria) -> Coordenador {
    let mut c = abrir(memoria);
    c.definir_preferencias(&Preferences {
        limite_fps: Some(141),
        ..Default::default()
    });
    c
}

fn r6_efetivo(c: &Coordenador, sid: Option<&str>) -> bool {
    c.limites_no_jogo_efetivos(sid)
        .keys()
        .any(|j| j.as_str() == "r6-siege")
}

#[test]
fn jogo_exige_aceite_aplica_relato_idempotente_e_f313_so_do_sid_da_console() {
    let agora = Instant::now();
    let mut c = com_limite(Memoria::default());
    let a = cliente("S-1-5-21-1", 1);
    let b = cliente("S-1-5-21-2", 2);

    // Sem aceite: só sugestão, nenhuma tarefa de jogo.
    let p = consultar_jogo(&mut c, &a, agora, 0, vec![fato_r6(0, &h('a'), vec![])]);
    assert!(p.tarefa_jogo.is_none());
    assert!(
        plano_projetado(&c, &a, agora)
            .sugestoes
            .iter()
            .any(|s| s.regra == R6 && s.estado == EstadoExperimental::Aceitavel)
    );

    aceite(&mut c, &a, agora, R6, true);
    let p = consultar_jogo(&mut c, &a, agora, 0, vec![fato_r6(0, &h('a'), vec![])]);
    assert!(p.validar().is_ok());
    let tarefa = p.tarefa_jogo.expect("tarefa de jogo");
    assert_eq!(
        tarefa.acoes,
        vec![AcaoJogo::Aplicar {
            destino: destino_r6("conta1"),
            valor: ValorJogoIpc::Inteiro(141),
            hash_esperado: h('a'),
        }]
    );
    // A mesma consulta reaproveita a tarefa.
    let p2 = consultar_jogo(&mut c, &a, agora, 0, vec![fato_r6(0, &h('a'), vec![])]);
    assert_eq!(p2.tarefa_jogo.as_ref(), Some(&tarefa));
    // Ainda não aplicado: o driver mantém o limite.
    assert!(!r6_efetivo(&c, Some(&a.sid)));

    // Hash diferente do esperado ou destino fora da tarefa: recusado.
    let errado = resultado(StatusMudanca::Aplicada, 0, 141, &h('c'), None);
    assert!(recusado(c.tratar(
        &a,
        &relato_jogo(&tarefa, vec![errado]),
        agora,
        12
    )));
    let mut fora = resultado(StatusMudanca::Aplicada, 0, 141, &h('a'), None);
    fora.destino = destino_r6("conta2");
    assert!(recusado(c.tratar(
        &a,
        &relato_jogo(&tarefa, vec![fora]),
        agora,
        12
    )));
    let mut outro_jogo = resultado(StatusMudanca::Aplicada, 0, 141, &h('a'), None);
    outro_jogo.destino.setting = "cs2_fps_max".into();
    assert!(recusado(c.tratar(
        &a,
        &relato_jogo(&tarefa, vec![outro_jogo]),
        agora,
        12
    )));
    // Outra identidade não relata a tarefa.
    let ok = resultado(StatusMudanca::Aplicada, 0, 141, &h('a'), None);
    assert!(recusado(c.tratar(
        &b,
        &relato_jogo(&tarefa, vec![ok.clone()]),
        agora,
        12
    )));

    let req = relato_jogo(&tarefa, vec![ok.clone()]);
    assert_eq!(c.tratar(&a, &req, agora, 12), Some(Resposta::Ok));
    assert_eq!(c.tratar(&a, &req, agora, 12), Some(Resposta::Ok));
    let diferente = resultado(StatusMudanca::Falhou, 0, 141, &h('a'), None);
    assert!(recusado(c.tratar(
        &a,
        &relato_jogo(&tarefa, vec![diferente]),
        agora,
        12
    )));
    assert!(historico(&c, &a, agora).iter().any(
        |e| e.setting.as_deref() == Some("r6_fps_limit") && e.depois.as_deref() == Some("141")
    ));
    assert!(historico(&c, &b, agora).is_empty());

    // F3-13 só para o SID da console.
    assert!(r6_efetivo(&c, Some(&a.sid)));
    assert!(!r6_efetivo(&c, Some(&b.sid)));
    assert!(!r6_efetivo(&c, None));
    assert!(c.aceites_drs(Some(&a.sid)).is_empty());

    // Releitura no valor: nada a fazer, continua efetivo; projeção só do SID.
    let p = consultar_jogo(
        &mut c,
        &a,
        agora,
        0,
        vec![fato_r6(141, &h('b'), vec![gestao_r6(0, 141)])],
    );
    assert!(p.tarefa_jogo.is_none());
    assert!(r6_efetivo(&c, Some(&a.sid)));
    let ui = c.config_jogo_ui(&a, agora);
    let arquivo = &ui["r6-siege"].arquivos[0];
    assert!(arquivo.reversivel);
    assert_eq!(arquivo.mudancas[0].status, StatusMudanca::JaNoValor);
    assert!(ui["r6-siege"].validar().is_ok());
    assert!(c.config_jogo_ui(&b, agora).is_empty());

    // Retirar o aceite devolve o original.
    aceite(&mut c, &a, agora, R6, false);
    assert!(!r6_efetivo(&c, Some(&a.sid)));
    let p = consultar_jogo(
        &mut c,
        &a,
        agora,
        0,
        vec![fato_r6(141, &h('b'), vec![gestao_r6(0, 141)])],
    );
    let t = p.tarefa_jogo.expect("restauração");
    assert_eq!(t.tarefa.operacao, OperacaoUsuario::Restaurar);
    assert!(
        matches!(&t.acoes[..], [AcaoJogo::Restaurar { hash_esperado, .. }] if *hash_esperado == h('b'))
    );
}

#[test]
fn falha_mantem_limite_do_driver_nao_repete_e_adiamento_orienta() {
    let agora = Instant::now();
    let mut c = com_limite(Memoria::default());
    let a = cliente("S-1-5-21-1", 1);
    aceite(&mut c, &a, agora, R6, true);
    let jogo = || vec![fato_r6(0, &h('a'), vec![])];

    let t = consultar_jogo(&mut c, &a, agora, 0, jogo())
        .tarefa_jogo
        .unwrap();
    let falha = resultado(
        StatusMudanca::Falhou,
        0,
        0,
        &h('a'),
        Some(MotivoNaoAplicada::FormatoInesperado),
    );
    assert_eq!(
        c.tratar(&a, &relato_jogo(&t, vec![falha]), agora, 12),
        Some(Resposta::Ok)
    );
    assert!(!r6_efetivo(&c, Some(&a.sid)));
    // Mesmo arquivo: não regrava em loop.
    assert!(
        consultar_jogo(&mut c, &a, agora, 0, jogo())
            .tarefa_jogo
            .is_none()
    );
    // Arquivo mudou: tenta de novo.
    let t = consultar_jogo(&mut c, &a, agora, 0, vec![fato_r6(0, &h('c'), vec![])])
        .tarefa_jogo
        .unwrap();
    let adiado = resultado(
        StatusMudanca::Pendente,
        0,
        0,
        &h('c'),
        Some(MotivoNaoAplicada::JanelaFechadaExigida {
            processos: vec!["RainbowSix.exe".into(), "steam.exe".into()],
        }),
    );
    assert_eq!(
        c.tratar(&a, &relato_jogo(&t, vec![adiado]), agora, 12),
        Some(Resposta::Ok)
    );
    assert!(!r6_efetivo(&c, Some(&a.sid)));
    let ui = c.config_jogo_ui(&a, agora);
    let ui = &ui["r6-siege"];
    assert!(ui.orientacoes.contains(&OrientacaoConfigJogo::FecharSteam));
    assert!(ui.orientacoes.contains(&OrientacaoConfigJogo::FecharJogo {
        processos: vec!["RainbowSix.exe".into()],
    }));
    assert!(ui.validar().is_ok());
    // Adiamento é tentado de novo na próxima consulta.
    assert!(
        consultar_jogo(&mut c, &a, agora, 0, vec![fato_r6(0, &h('c'), vec![])])
            .tarefa_jogo
            .is_some()
    );

    // Somente leitura e conta ambígua: sem tarefa, com orientação.
    let mut f = fato_r6(0, &h('d'), vec![]);
    f.somente_leitura = true;
    f.conta_ambigua = true;
    assert!(
        consultar_jogo(&mut c, &a, agora, 0, vec![f])
            .tarefa_jogo
            .is_none()
    );
    let ui = c.config_jogo_ui(&a, agora);
    assert!(
        ui["r6-siege"]
            .orientacoes
            .contains(&OrientacaoConfigJogo::DesmarcarSomenteLeitura)
    );
    assert!(
        ui["r6-siege"]
            .orientacoes
            .contains(&OrientacaoConfigJogo::EscolherConta)
    );
}

#[test]
fn reidratacao_valida_catalogo_e_reverter_usa_valor_de_antes() {
    let agora = Instant::now();
    let mut c = com_limite(Memoria::default());
    let a = cliente("S-1-5-21-1", 1);
    let b = cliente("S-1-5-21-2", 2);
    aceite(&mut c, &a, agora, R6, true);

    // Gestão fora do catálogo (setting de outro jogo) é ignorada.
    let mut estranha = gestao_r6(0, 141);
    estranha.setting = "cs2_fps_max".into();
    let p = consultar_jogo(
        &mut c,
        &a,
        agora,
        0,
        vec![fato_r6(141, &h('b'), vec![gestao_r6(60, 141), estranha])],
    );
    assert!(p.tarefa_jogo.is_none());
    let pedido = |destino| Requisicao::ReverterConfigJogo { destino };
    let mut cs2 = destino_r6("conta1");
    cs2.setting = "cs2_fps_max".into();
    assert!(recusado(c.tratar(&a, &pedido(cs2), agora, 12)));
    assert!(recusado(c.tratar(
        &a,
        &pedido(destino_r6("conta2")),
        agora,
        12
    )));
    // Outro SID não reverte a gestão desta sessão.
    assert!(recusado(c.tratar(
        &b,
        &pedido(destino_r6("conta1")),
        agora,
        12
    )));

    assert_eq!(
        c.tratar(&a, &pedido(destino_r6("conta1")), agora, 12),
        Some(Resposta::Ok)
    );
    // Reidratação sem a gestão não apaga o que já se conhece.
    let t = consultar_jogo(&mut c, &a, agora, 0, vec![fato_r6(141, &h('b'), vec![])])
        .tarefa_jogo
        .expect("reversão");
    assert_eq!(t.tarefa.operacao, OperacaoUsuario::Reverter);
    // Reversão volta ao valor de antes da gestão (60), não a outro.
    let errado = resultado(StatusMudanca::Aplicada, 141, 0, &h('b'), None);
    assert!(recusado(c.tratar(
        &a,
        &relato_jogo(&t, vec![errado]),
        agora,
        12
    )));
    let ok = resultado(StatusMudanca::Aplicada, 141, 60, &h('b'), None);
    assert_eq!(
        c.tratar(&a, &relato_jogo(&t, vec![ok]), agora, 12),
        Some(Resposta::Ok)
    );
    // Travado após a reversão: o plano não reaplica.
    assert!(
        consultar_jogo(&mut c, &a, agora, 0, vec![fato_r6(60, &h('c'), vec![])])
            .tarefa_jogo
            .is_none()
    );
    assert!(!r6_efetivo(&c, Some(&a.sid)));
    assert!(recusado(c.tratar(
        &a,
        &pedido(destino_r6("conta1")),
        agora,
        12
    )));
}

#[test]
fn restauracao_por_geracao_cobre_config_de_jogo_e_revisao_obsoleta() {
    let agora = Instant::now();
    let mut c = com_limite(Memoria::default());
    let a = cliente("S-1-5-21-1", 1);
    aceite(&mut c, &a, agora, R6, true);
    let t = consultar_jogo(&mut c, &a, agora, 0, vec![fato_r6(0, &h('a'), vec![])])
        .tarefa_jogo
        .unwrap();
    // Nova geração invalida a tarefa em voo: relato obsoleto descartado.
    c.atualizar_restauracao(1, true);
    let ok = resultado(StatusMudanca::Aplicada, 0, 141, &h('a'), None);
    assert!(recusado(c.tratar(
        &a,
        &relato_jogo(&t, vec![ok]),
        agora,
        12
    )));

    let p = consultar_jogo(
        &mut c,
        &a,
        agora,
        0,
        vec![fato_r6(141, &h('b'), vec![gestao_r6(0, 141)])],
    );
    let t = p.tarefa_jogo.expect("restauração por geração");
    assert_eq!(t.tarefa.operacao, OperacaoUsuario::Restaurar);
    assert_eq!(
        t.acoes,
        vec![AcaoJogo::Restaurar {
            destino: destino_r6("conta1"),
            hash_esperado: h('b'),
        }]
    );
    let ok = resultado(StatusMudanca::Aplicada, 141, 0, &h('b'), None);
    assert_eq!(
        c.tratar(&a, &relato_jogo(&t, vec![ok]), agora, 12),
        Some(Resposta::Ok)
    );
    assert!(!r6_efetivo(&c, Some(&a.sid)));
}

#[test]
fn usuarios_json_antigo_sem_resultados_jogo_carrega() {
    let json = r#"{"por_sid":{"S-1-5-21-1":{"aceites":["windows-captura-historica-off"],
        "historico":[],"recibos":{"1":[{"tarefa":{"id_tarefa":7,"operacao":"aplicar","revisao":3},
        "resultados":[]}]},"atualizacao_vista":null}}}"#;
    let dados: DadosUsuarios = serde_json::from_str(json).unwrap();
    let recibo = &dados.por_sid["S-1-5-21-1"].recibos[&1][0];
    assert!(recibo.resultados_jogo.is_empty());
    let texto = serde_json::to_string(&dados).unwrap();
    assert!(!texto.contains("resultados_jogo"));
}
