use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    io,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    time::{Duration, Instant},
};

use super::*;
use nvt_core::usuario::ValorUsuario;
use nvt_detect::ProcessosFalsos;
use nvt_engine::Portao;
use nvt_ipc::{AjusteUsuario, FatoTela, MouseSpi, RefreshRacional};

type Diario = Rc<RefCell<Vec<u8>>>;

struct Memoria(Diario);

impl Persistencia for Memoria {
    fn carregar(&self) -> io::Result<Estado> {
        let dados = self.0.borrow();
        if dados.is_empty() {
            return Ok(Estado::default());
        }
        Ok(serde_json::from_slice(&dados)?)
    }

    fn salvar(&self, estado: &Estado) -> io::Result<()> {
        *self.0.borrow_mut() = serde_json::to_vec(estado)?;
        Ok(())
    }
}

const DVR: SettingUsuario = SettingUsuario::GameDvrCapturaHistorica;
const MOUSE: SettingUsuario = SettingUsuario::MouseAceleracao;

fn valor(valor: ValorUsuario) -> ValorLido {
    ValorLido {
        valor,
        registro_dx_presente: false,
        outros_tokens_dx: false,
    }
}

struct Ajustes {
    persistencia: Memoria,
    valores: BTreeMap<SettingUsuario, ValorLido>,
    gravacoes: Vec<SettingUsuario>,
    releituras: Cell<usize>,
    crash: Option<usize>,
    divergencia: Option<ValorLido>,
    #[cfg(windows)]
    compensacao_interna: bool,
}

impl LeituraUsuario for Ajustes {
    fn ler(&self, destino: &SettingUsuario) -> io::Result<ValorLido> {
        self.releituras.set(self.releituras.get() + 1);
        self.valores
            .get(destino)
            .cloned()
            .ok_or_else(|| io::Error::other("indisponível"))
    }

    fn tela(&self) -> Fato<FatoTela> {
        let destino = SettingUsuario::Refresh {
            alvo: "monitor".into(),
        };
        match self.valores.get(&destino).map(|v| &v.valor) {
            Some(ValorUsuario::Refresh(taxa)) => Fato::Conhecido(FatoTela {
                alvo: "monitor".into(),
                atual: crate::valores::taxa_ipc(*taxa),
                maximo: RefreshRacional {
                    numerador: 144,
                    denominador: 1,
                },
            }),
            _ => Fato::Desconhecido,
        }
    }
}

impl EscritaUsuario for Ajustes {
    fn escrever(
        &mut self,
        destino: &SettingUsuario,
        antes: &ValorLido,
        pedido: &ValorLido,
        _restauracao: bool,
        autorizar: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        assert_eq!(self.valores.get(destino), Some(antes));
        let estado = self.persistencia.carregar().unwrap();
        let entrada = estado
            .destinos
            .iter()
            .find(|d| &d.destino == destino)
            .unwrap();
        assert!(entrada.intencao.is_some());
        autorizar()?;
        let lido = self.divergencia.take().unwrap_or_else(|| pedido.clone());
        self.valores.insert(destino.clone(), lido.clone());
        self.gravacoes.push(destino.clone());
        assert_ne!(
            self.crash,
            Some(self.gravacoes.len()),
            "crash depois de gravar"
        );
        #[cfg(windows)]
        if self.compensacao_interna && !iguais(&lido, pedido) {
            use nvt_windows::Erro;
            let compensacao = autorizar().map_err(|_| Box::new(Erro::Bloqueado));
            if compensacao.is_ok() {
                self.valores.insert(destino.clone(), antes.clone());
                self.gravacoes.push(destino.clone());
            }
            return Err(io::Error::other(Erro::RefreshNaoConfirmado {
                falha: Box::new(Erro::Releitura),
                compensacao,
            }));
        }
        Ok(())
    }
}

struct Servico {
    persistencia: Memoria,
    respostas: VecDeque<io::Result<Resposta>>,
    chamadas: Vec<Requisicao>,
}

impl Transporte for Servico {
    fn trocar(&mut self, req: &Requisicao) -> io::Result<Resposta> {
        if let Requisicao::AgenteRelatar { resultados, .. } = req {
            let estado = self.persistencia.carregar().unwrap();
            assert!(estado.sessoes.values().any(|s| {
                s.trabalho
                    .as_ref()
                    .is_some_and(|t| t.pronto && &t.resultados == resultados)
            }));
        }
        self.chamadas.push(req.clone());
        self.respostas.pop_front().expect("requisição inesperada")
    }
}

fn plano(
    revisao: u64,
    geracao: u64,
    operacao: OperacaoUsuario,
    acoes: Vec<AcaoUsuario>,
) -> Resposta {
    Resposta::PlanoAgente(PlanoAgente {
        tarefa_jogo: None,
        revisao,
        geracao_restauracao: geracao,
        tarefa: (!acoes.is_empty()).then_some(TarefaUsuario {
            tarefa: ReferenciaTarefa {
                id_tarefa: revisao,
                revisao,
                operacao,
            },
            acoes,
        }),
        processos_vigiados: ["servico-extra.exe".into()].into(),
    })
}

fn aplicar(revisao: u64) -> Resposta {
    plano(
        revisao,
        0,
        OperacaoUsuario::Aplicar,
        vec![AcaoUsuario::Aplicar {
            ajuste: AjusteUsuario::GameDvrCapturaHistorica {
                antes: Some(1),
                depois: Some(0),
            },
        }],
    )
}

fn preparar(fonte: ProcessosFalsos) -> (Memoria, Agente<Ajustes, Servico>, Instant) {
    let dados = Diario::default();
    let armazem = Ajustes {
        persistencia: Memoria(dados.clone()),
        valores: [
            (DVR, valor(ValorUsuario::Dword(1))),
            (MOUSE, valor(ValorUsuario::Spi([6, 10, 1]))),
        ]
        .into(),
        gravacoes: Vec::new(),
        releituras: Cell::new(0),
        crash: None,
        divergencia: None,
        #[cfg(windows)]
        compensacao_interna: false,
    };
    let servico = Servico {
        persistencia: Memoria(dados.clone()),
        respostas: VecDeque::new(),
        chamadas: Vec::new(),
    };
    let agente = Agente::novo(armazem, servico, Portao::new(fonte), 1).unwrap();
    (Memoria(dados), agente, Instant::now())
}

fn responder(agente: &mut Agente<Ajustes, Servico>, respostas: impl IntoIterator<Item = Resposta>) {
    agente
        .transporte
        .respostas
        .extend(respostas.into_iter().map(Ok));
}

fn ultimo_status(agente: &Agente<Ajustes, Servico>) -> StatusMudanca {
    let Some(Requisicao::AgenteRelatar { resultados, .. }) = agente.transporte.chamadas.last()
    else {
        panic!("relato ausente")
    };
    resultados[0].status
}

#[test]
fn consulta_escrita_releitura_relato_e_retirada() {
    let (p, mut a, agora) = preparar(ProcessosFalsos::default());
    responder(&mut a, [aplicar(1), Resposta::Ok]);
    a.passo(&p, agora, false).unwrap();
    assert_eq!(ultimo_status(&a), StatusMudanca::Aplicada);
    assert_eq!(a.armazem.gravacoes, [DVR]);
    let estado = p.carregar().unwrap();
    let d = &estado.destinos[0];
    assert_eq!(d.snapshot.valor, ValorUsuario::Dword(1));
    assert_eq!(d.esperado.as_ref().unwrap().valor, ValorUsuario::Dword(0));
    assert!(a.armazem.releituras.get() >= 4);
    assert!(matches!(
        a.transporte.chamadas[..],
        [
            Requisicao::AgenteConsultar { .. },
            Requisicao::AgenteRelatar { .. }
        ]
    ));

    responder(
        &mut a,
        [
            plano(
                2,
                0,
                OperacaoUsuario::Restaurar,
                vec![AcaoUsuario::Restaurar { destino: DVR }],
            ),
            Resposta::Ok,
        ],
    );
    a.passo(&p, agora + Duration::from_secs(2), false).unwrap();
    assert_eq!(ultimo_status(&a), StatusMudanca::Aplicada);
    assert_eq!(a.armazem.valores[&DVR].valor, ValorUsuario::Dword(1));
    assert!(p.carregar().unwrap().destinos[0].esperado.is_none());
}

#[test]
fn portao_une_base_anticheat_extras_e_falha_fechada() {
    for fonte in [
        ProcessosFalsos::falhando(),
        ProcessosFalsos::com_nomes(&["CS2.EXE"]),
        ProcessosFalsos::com_nomes(&["EasyAntiCheat_EOS.exe"]),
        ProcessosFalsos::com_nomes(&["SERVICO-EXTRA.EXE"]),
    ] {
        let (p, mut a, agora) = preparar(fonte);
        responder(&mut a, [aplicar(1), Resposta::Ok]);
        a.passo(&p, agora, false).unwrap();
        assert_eq!(ultimo_status(&a), StatusMudanca::Pendente);
        assert!(a.armazem.gravacoes.is_empty());
        assert!(p.carregar().unwrap().destinos.is_empty());
    }
    let (p, mut a, agora) = preparar(ProcessosFalsos::default());
    a.transporte
        .respostas
        .push_back(Err(io::Error::other("pipe indisponível")));
    assert!(a.passo(&p, agora, false).is_err());
    a.passo(&p, agora + Duration::from_millis(500), false)
        .unwrap();
    assert_eq!(a.transporte.chamadas.len(), 1);
    assert!(a.armazem.gravacoes.is_empty());
}

#[test]
fn deriva_no_logon_e_periodica_preserva_edicao_manual() {
    let (p, mut a, agora) = preparar(ProcessosFalsos::default());
    responder(&mut a, [aplicar(1), Resposta::Ok]);
    a.passo(&p, agora, false).unwrap();
    a.armazem.valores.insert(DVR, valor(ValorUsuario::Dword(1)));
    responder(&mut a, [plano(2, 0, OperacaoUsuario::Aplicar, Vec::new())]);
    a.passo(&p, agora + Duration::from_secs(600), false)
        .unwrap();
    assert!(p.carregar().unwrap().destinos[0].travado);
    a = Agente::novo(
        a.armazem,
        a.transporte,
        Portao::new(ProcessosFalsos::default()),
        1,
    )
    .unwrap();
    responder(&mut a, [aplicar(3), Resposta::Ok]);
    a.passo(&p, Instant::now(), false).unwrap();
    assert_eq!(ultimo_status(&a), StatusMudanca::Travada);
    assert_eq!(a.armazem.gravacoes, [DVR]);
}

#[test]
fn recupera_crash_parcial_sem_repetir_e_trava_divergencia() {
    let (p, mut a, agora) = preparar(ProcessosFalsos::default());
    let tarefa = plano(
        1,
        0,
        OperacaoUsuario::Aplicar,
        vec![
            AcaoUsuario::Aplicar {
                ajuste: AjusteUsuario::GameDvrCapturaHistorica {
                    antes: Some(1),
                    depois: Some(0),
                },
            },
            AcaoUsuario::Aplicar {
                ajuste: AjusteUsuario::MouseAceleracao {
                    antes: MouseSpi {
                        limiar1: 6,
                        limiar2: 10,
                        aceleracao: 1,
                    },
                    depois: MouseSpi {
                        limiar1: 0,
                        limiar2: 0,
                        aceleracao: 0,
                    },
                },
            },
        ],
    );
    a.armazem.crash = Some(2);
    responder(&mut a, [tarefa.clone()]);
    assert!(catch_unwind(AssertUnwindSafe(|| a.passo(&p, agora, false))).is_err());
    a.armazem
        .valores
        .insert(MOUSE, valor(ValorUsuario::Spi([4, 8, 2])));
    assert!(p.carregar().unwrap().destinos[1].intencao.is_some());
    a = Agente::novo(
        a.armazem,
        a.transporte,
        Portao::new(ProcessosFalsos::default()),
        1,
    )
    .unwrap();
    responder(&mut a, [tarefa, Resposta::Ok]);
    a.passo(&p, Instant::now(), false).unwrap();
    assert_eq!(a.armazem.gravacoes, [DVR, MOUSE]);
    let estado = p.carregar().unwrap();
    assert!(estado.destinos[1].travado);
    let trabalho = estado.sessoes[&1].trabalho.as_ref().unwrap();
    assert_eq!(trabalho.resultados[0].status, StatusMudanca::Aplicada);
    assert_eq!(trabalho.resultados[1].status, StatusMudanca::Travada);
}

#[test]
fn restauracao_no_logon_bloqueada_e_depois_confirmada() {
    let (p, mut a, agora) = preparar(ProcessosFalsos::default());
    responder(&mut a, [aplicar(1), Resposta::Ok]);
    a.passo(&p, agora, false).unwrap();
    let snapshot = |revisao| {
        plano(
            revisao,
            1,
            OperacaoUsuario::Restaurar,
            vec![AcaoUsuario::RestaurarSnapshot],
        )
    };
    a = Agente::novo(
        a.armazem,
        a.transporte,
        Portao::new(ProcessosFalsos::falhando()),
        2,
    )
    .unwrap();
    responder(
        &mut a,
        [snapshot(2), Resposta::Ok, snapshot(3), Resposta::Ok],
    );
    let agora = Instant::now();
    a.passo(&p, agora, false).unwrap();
    assert_eq!(p.carregar().unwrap().sessoes[&2].geracao, 0);
    assert_eq!(ultimo_status(&a), StatusMudanca::Pendente);
    a.portao = Portao::new(ProcessosFalsos::default());
    a.passo(&p, agora + Duration::from_secs(2), false).unwrap();
    assert_eq!(p.carregar().unwrap().sessoes[&2].geracao, 1);
    assert_eq!(a.armazem.valores[&DVR].valor, ValorUsuario::Dword(1));
    assert_eq!(ultimo_status(&a), StatusMudanca::Aplicada);
    responder(&mut a, [Resposta::Encerrar]);
    assert_eq!(
        a.passo(&p, agora + Duration::from_secs(4), false).unwrap(),
        Controle::Encerrar
    );
    assert!(matches!(
        a.transporte.chamadas.last(),
        Some(Requisicao::AgenteConsultar {
            geracao_restaurada: 1,
            ..
        })
    ));
}

#[test]
fn restauracao_msi_exige_lista_atual_e_portao() {
    let (p, mut a, agora) = preparar(ProcessosFalsos::default());
    responder(&mut a, [aplicar(1), Resposta::Ok]);
    a.passo(&p, agora, false).unwrap();
    a.portao = Portao::new(ProcessosFalsos::com_nomes(&["servico-extra.exe"]));
    responder(&mut a, [plano(2, 0, OperacaoUsuario::Aplicar, Vec::new())]);
    assert!(matches!(
        a.passo(&p, agora, true).unwrap(),
        Controle::Bloqueado(motivo) if motivo.contains("jogo ou anticheat")
    ));
    assert_eq!(a.armazem.gravacoes, [DVR]);
    a.portao = Portao::new(ProcessosFalsos::default());
    responder(&mut a, [plano(2, 0, OperacaoUsuario::Aplicar, Vec::new())]);
    assert_eq!(a.passo(&p, agora, true).unwrap(), Controle::Restaurado);
    assert_eq!(a.armazem.gravacoes, [DVR, DVR]);
    let estado = p.carregar().unwrap();
    assert!(estado.destinos.is_empty());
    assert_eq!(estado.backups_msi[0].valor.valor, ValorUsuario::Dword(1));
}

fn taxa(hz: u32) -> RefreshRacional {
    RefreshRacional {
        numerador: hz,
        denominador: 1,
    }
}

fn refresh(hz: u32) -> ValorLido {
    valor(ValorUsuario::Refresh(crate::valores::taxa(taxa(hz))))
}

fn aplicar_refresh(revisao: u64) -> Resposta {
    plano(
        revisao,
        0,
        OperacaoUsuario::Aplicar,
        vec![AcaoUsuario::Aplicar {
            ajuste: AjusteUsuario::Refresh {
                alvo: "monitor".into(),
                antes: taxa(60),
                depois: taxa(144),
            },
        }],
    )
}

#[test]
fn compensa_refresh_so_com_novo_portao_e_intencao_persistida() {
    use nvt_detect::{FonteProcessos, ListaIndisponivel, Processo};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Sequencia(AtomicUsize);
    impl FonteProcessos for Sequencia {
        fn processos(&self) -> Vec<Processo> {
            Vec::new()
        }
        fn ler(&self) -> Result<Vec<Processo>, ListaIndisponivel> {
            if self.0.fetch_add(1, Ordering::Relaxed) == 1 {
                return Err(ListaIndisponivel { codigo: 5 });
            }
            Ok(Vec::new())
        }
    }
    #[cfg(windows)]
    for compensacao_interna in [false, true] {
        conferir(compensacao_interna);
    }
    #[cfg(not(windows))]
    conferir(false);

    fn conferir(_compensacao_interna: bool) {
        let (p, mut a, agora) = preparar(ProcessosFalsos::default());
        #[cfg(windows)]
        {
            a.armazem.compensacao_interna = _compensacao_interna;
        }
        a.portao = Portao::new(Sequencia(AtomicUsize::new(0)));
        let destino = SettingUsuario::Refresh {
            alvo: "monitor".into(),
        };
        a.armazem.valores.insert(destino.clone(), refresh(60));
        a.armazem.divergencia = Some(refresh(120));
        responder(&mut a, [aplicar_refresh(1)]);
        a.passo(&p, agora, false).unwrap();
        assert_eq!(a.armazem.valores[&destino], refresh(120));
        assert_eq!(a.armazem.gravacoes.len(), 1);
        assert!(
            p.carregar().unwrap().destinos[0]
                .intencao
                .as_ref()
                .unwrap()
                .rollback
                .is_some()
        );
        responder(&mut a, [aplicar_refresh(1), Resposta::Ok]);
        a.passo(&p, agora + Duration::from_secs(5), false).unwrap();
        assert_eq!(a.armazem.valores[&destino], refresh(60));
        assert_eq!(a.armazem.gravacoes.len(), 2);
        assert_eq!(ultimo_status(&a), StatusMudanca::Falhou);
    }
}

#[test]
fn snapshot_de_refresh_retorna_no_logon_com_outro_id_de_sessao() {
    let (p, mut a, agora) = preparar(ProcessosFalsos::default());
    let destino = SettingUsuario::Refresh {
        alvo: "monitor".into(),
    };
    a.armazem.valores.insert(destino.clone(), refresh(60));
    responder(&mut a, [aplicar_refresh(1), Resposta::Ok]);
    a.passo(&p, agora, false).unwrap();
    assert_eq!(p.carregar().unwrap().destinos[0].sessao, Some(1));

    a = Agente::novo(
        a.armazem,
        a.transporte,
        Portao::new(ProcessosFalsos::default()),
        2,
    )
    .unwrap();
    a.definir_sessoes_existentes(Some([2].into()));
    responder(
        &mut a,
        [
            plano(
                2,
                1,
                OperacaoUsuario::Restaurar,
                vec![AcaoUsuario::RestaurarSnapshot],
            ),
            Resposta::Ok,
        ],
    );
    a.passo(&p, Instant::now(), false).unwrap();
    assert_eq!(a.armazem.valores[&destino], refresh(60));
    assert!(p.carregar().unwrap().destinos.is_empty());
}

#[test]
fn repete_relato_de_geracao_sem_repetir_escrita() {
    let (p, mut a, agora) = preparar(ProcessosFalsos::default());
    responder(
        &mut a,
        [
            aplicar(1),
            Resposta::Ok,
            plano(
                2,
                1,
                OperacaoUsuario::Restaurar,
                vec![AcaoUsuario::RestaurarSnapshot],
            ),
        ],
    );
    a.transporte
        .respostas
        .push_back(Err(io::Error::other("resposta perdida")));
    a.passo(&p, agora, false).unwrap();
    assert!(a.passo(&p, agora + Duration::from_secs(2), false).is_err());
    let relato = a.transporte.chamadas.last().cloned().unwrap();
    assert_eq!(p.carregar().unwrap().sessoes[&1].geracao, 0);
    responder(
        &mut a,
        [
            plano(2, 1, OperacaoUsuario::Restaurar, Vec::new()),
            Resposta::Ok,
        ],
    );
    a.passo(&p, agora + Duration::from_secs(10), false).unwrap();
    assert_eq!(a.transporte.chamadas.last(), Some(&relato));
    assert_eq!(p.carregar().unwrap().sessoes[&1].geracao, 1);
    assert_eq!(a.armazem.gravacoes, [DVR, DVR]);
}

#[test]
fn geracao_nova_exige_consulta_depois_da_restauracao() {
    let (p, mut a, agora) = preparar(ProcessosFalsos::default());
    responder(&mut a, [aplicar(1), Resposta::Ok]);
    a.passo(&p, agora, false).unwrap();
    let Resposta::PlanoAgente(mut novo) = aplicar(2) else {
        unreachable!()
    };
    novo.geracao_restauracao = 1;
    responder(&mut a, [Resposta::PlanoAgente(novo)]);
    a.passo(&p, agora + Duration::from_secs(2), false).unwrap();
    assert_eq!(a.armazem.gravacoes, [DVR, DVR]);
    assert_eq!(a.armazem.valores[&DVR].valor, ValorUsuario::Dword(1));
    assert!(p.carregar().unwrap().destinos.is_empty());
    assert_eq!(p.carregar().unwrap().sessoes[&1].geracao, 1);
}

#[test]
fn releitura_dx_so_aceita_registro_restante_se_houver_outros_tokens() {
    let ausente = valor(ValorUsuario::Ausente);
    let vazio = ValorLido {
        registro_dx_presente: true,
        ..ausente.clone()
    };
    let terceiros = ValorLido {
        outros_tokens_dx: true,
        ..vazio.clone()
    };
    assert!(!atingiu(&vazio, &ausente));
    assert!(atingiu(&terceiros, &ausente));
    assert!(atingiu(&terceiros, &vazio));
}

fn gestao_consultada(a: &Agente<Ajustes, Servico>) -> Vec<nvt_ipc::GestaoAgente> {
    let Some(Requisicao::AgenteConsultar { fatos, .. }) = a
        .transporte
        .chamadas
        .iter()
        .rev()
        .find(|r| matches!(r, Requisicao::AgenteConsultar { .. }))
    else {
        panic!("consulta ausente")
    };
    fatos.gestao.clone()
}

#[test]
fn consulta_envia_gestao_persistida_para_reidratar_o_servico() {
    let (p, mut a, agora) = preparar(ProcessosFalsos::default());
    responder(&mut a, [aplicar(1), Resposta::Ok]);
    a.passo(&p, agora, false).unwrap();
    assert!(gestao_consultada(&a).is_empty());

    // Um agente novo (nova sessão, reboot) lê a gestão do perfil persistido.
    let mut a = Agente::novo(
        a.armazem,
        a.transporte,
        Portao::new(ProcessosFalsos::default()),
        2,
    )
    .unwrap();
    responder(&mut a, [plano(2, 0, OperacaoUsuario::Aplicar, Vec::new())]);
    a.passo(&p, agora + Duration::from_secs(5), false).unwrap();
    assert_eq!(
        gestao_consultada(&a),
        [nvt_ipc::GestaoAgente {
            destino: DVR,
            valores: Some(AjusteUsuario::GameDvrCapturaHistorica {
                antes: Some(1),
                depois: Some(0),
            }),
            travado: false,
        }]
    );
}

struct InstalacaoFalsa(Cell<bool>);

impl Instalacao for InstalacaoFalsa {
    fn presente(&self) -> bool {
        self.0.get()
    }
}

#[test]
fn sai_quando_servico_some_e_instalacao_foi_removida() {
    let (p, mut a, agora) = preparar(ProcessosFalsos::default());
    let removida = InstalacaoFalsa(Cell::new(false));
    let presente = InstalacaoFalsa(Cell::new(true));
    assert!(!a.desinstalado(agora, &removida));
    assert!(!a.desinstalado(agora, &presente));

    a.transporte
        .respostas
        .push_back(Err(io::Error::other("pipe indisponível")));
    assert!(a.passo(&p, agora, false).is_err());
    let depois = agora + Duration::from_secs(121);
    assert!(!a.desinstalado(agora + Duration::from_secs(1), &removida));
    assert!(!a.desinstalado(agora + Duration::from_secs(60), &removida));
    assert!(!a.desinstalado(depois, &presente));
    // Presença zera a ausência: precisa de nova ausência contínua.
    assert!(!a.desinstalado(depois, &removida));
    assert!(a.desinstalado(depois + Duration::from_secs(120), &removida));
    let depois = depois + Duration::from_secs(120);

    // A falha persiste desde a primeira; o serviço voltar zera a contagem.
    a.transporte
        .respostas
        .push_back(Err(io::Error::other("pipe indisponível")));
    assert!(
        a.passo(&p, agora + Duration::from_secs(100), false)
            .is_err()
    );
    assert!(a.desinstalado(depois, &removida));
    responder(&mut a, [plano(1, 0, OperacaoUsuario::Aplicar, Vec::new())]);
    a.passo(&p, agora + Duration::from_secs(200), false)
        .unwrap();
    assert!(!a.desinstalado(agora + Duration::from_secs(900), &removida));
}

#[test]
fn versao_incompativel_no_upgrade_e_indisponivel_temporario() {
    let (p, mut a, agora) = preparar(ProcessosFalsos::default());
    let removida = InstalacaoFalsa(Cell::new(false));
    responder(&mut a, [plano(1, 0, OperacaoUsuario::Aplicar, Vec::new())]);
    a.passo(&p, agora, false).unwrap();
    let antes = p.0.borrow().clone();

    // Serviço novo (v7) responde erro estável; serviço antigo manda envelope
    // de outra versão: os dois são "indisponível temporário", sem sair.
    a.transporte.respostas.push_back(Ok(Resposta::erro(
        nvt_ipc::CodigoErro::VersaoIncompativel,
        "v7",
    )));
    a.transporte.respostas.push_back(Err(io::Error::other(
        nvt_ipc::ErroIpc::VersaoIncompativel {
            recebida: 5,
            esperada: nvt_ipc::PROTOCOL_VERSION,
        },
    )));
    let mut t = agora + Duration::from_secs(61);
    for _ in 0..2 {
        assert!(a.passo(&p, t, false).is_err());
        assert_eq!(*p.0.borrow(), antes, "estado local intocado");
        assert!(a.armazem.gravacoes.is_empty());
        assert!(!a.desinstalado(t, &removida));
        t += Duration::from_secs(300);
        assert!(!a.desinstalado(t, &removida));
    }

    // Serviço compatível de volta: retoma normalmente.
    responder(&mut a, [plano(1, 0, OperacaoUsuario::Aplicar, Vec::new())]);
    assert_eq!(a.passo(&p, t, false).unwrap(), Controle::Continuar);
    assert!(!a.desinstalado(t + Duration::from_secs(900), &removida));
}

#[test]
fn ausencia_momentanea_da_instalacao_no_upgrade_nao_e_desinstalacao() {
    let (p, mut a, agora) = preparar(ProcessosFalsos::default());
    let removida = InstalacaoFalsa(Cell::new(false));
    let presente = InstalacaoFalsa(Cell::new(true));
    a.transporte
        .respostas
        .push_back(Err(io::Error::other("pipe indisponível")));
    assert!(a.passo(&p, agora, false).is_err());
    // Serviço fora há muito, mas o MSI regrava exe e Run entre as checagens.
    let mut t = agora + Duration::from_secs(300);
    for _ in 0..5 {
        assert!(!a.desinstalado(t, &removida));
        t += Duration::from_secs(60);
        assert!(!a.desinstalado(t, &presente));
        t += Duration::from_secs(1);
    }
}
