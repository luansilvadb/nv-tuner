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
        tela: Fato::Desconhecido,
        mouse: Fato::Desconhecido,
        swap_effect_upgrade: Fato::Desconhecido,
        captura_historica: Fato::Conhecido(captura),
        diagnosticos: vec![],
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
