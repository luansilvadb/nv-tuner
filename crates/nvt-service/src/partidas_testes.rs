//! Testes de `partidas.rs` com fachadas falsas.

use nvt_ipc::pipe::IdentidadeCliente;
use nvt_ipc::{CodigoErro, Requisicao, Resposta};

use super::*;

fn ident(sid: &str) -> IdentidadeCliente {
    IdentidadeCliente {
        sid: sid.into(),
        sessao: 1,
    }
}

fn requisicoes() -> [Requisicao; 3] {
    [
        Requisicao::ListarPartidas {
            desde_unix: 0,
            limite: 10,
        },
        Requisicao::ObterPartida { id: 1 },
        Requisicao::ApagarPartidas,
    ]
}

#[cfg(not(feature = "medicao-partidas"))]
#[test]
fn sem_feature_responde_recurso_ausente() {
    let p = Partidas;
    let console = ident("S-1-5-21-1");
    for req in requisicoes() {
        let r = p
            .atender(&console, Some(&console), &req)
            .expect("é de partidas");
        assert!(
            matches!(
                r,
                Resposta::Erro {
                    codigo: CodigoErro::RecursoAusente,
                    ..
                }
            ),
            "{r:?}"
        );
    }
    assert!(
        p.atender(&console, None, &Requisicao::ObterEstado)
            .is_none()
    );
}

#[cfg(feature = "medicao-partidas")]
mod com_feature {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    use nvt_ipc::CodigoErroMedicao;
    use nvt_medicao::fachada::falsos::{GpuFalsa, PresentsFalsos, ProcessosFalsos};
    use nvt_medicao::fachada::{EventoPresent, FontePresents, ProcessoVisto};
    use nvt_medicao::persistencia::Armazem;

    use super::*;

    const PID: u32 = 4242;

    struct PresentsComp(Arc<Mutex<PresentsFalsos>>);

    impl FontePresents for PresentsComp {
        fn coletar(&mut self, d: &mut Vec<EventoPresent>) -> Result<u64, CodigoErroMedicao> {
            self.0.lock().unwrap().coletar(d)
        }
        fn parar(&mut self) {
            self.0.lock().unwrap().parar();
        }
    }

    fn base(exe: &str) -> Option<String> {
        exe.eq_ignore_ascii_case("cs2.exe")
            .then(|| "cs2".to_string())
    }

    struct Cenario {
        dir: crate::teste_util::DirTemp,
        armazem: Compartilhado,
        relogio: Arc<AtomicU64>,
        presents: Arc<Mutex<PresentsFalsos>>,
        aberturas: Arc<AtomicU64>,
    }

    fn cenario(nome: &str) -> Cenario {
        let dir = crate::teste_util::DirTemp::novo(&format!("partidas-{nome}"));
        let armazem = Arc::new(Mutex::new(Some(Armazem::abrir(&dir.0).unwrap())));
        Cenario {
            dir,
            armazem,
            relogio: Arc::new(AtomicU64::new(1_000_000)),
            presents: Arc::new(Mutex::new(PresentsFalsos::default())),
            aberturas: Arc::new(AtomicU64::new(0)),
        }
    }

    fn fontes(c: &Cenario, snapshots: Vec<Vec<ProcessoVisto>>, negar: bool) -> Fontes {
        let presents = c.presents.clone();
        let aberturas = c.aberturas.clone();
        let relogio = c.relogio.clone();
        let relogio2 = c.relogio.clone();
        Fontes {
            processos: Box::new(ProcessosFalsos {
                sequencia: VecDeque::from(snapshots),
                ..Default::default()
            }),
            gpu: Box::new(GpuFalsa::default()),
            abrir_presents: Box::new(move || {
                aberturas.fetch_add(1, Ordering::SeqCst);
                if negar {
                    return Err(CodigoErroMedicao::SessaoPerdida);
                }
                presents.lock().unwrap().parado = false;
                Ok(Box::new(PresentsComp(presents.clone())) as Box<dyn FontePresents + Send>)
            }),
            base: Box::new(base),
            relogio_us: Box::new(move || relogio.load(Ordering::SeqCst)),
            agora_unix: Box::new(move || {
                1_700_000_000 + relogio2.load(Ordering::SeqCst) / 1_000_000
            }),
        }
    }

    fn jogo() -> Vec<ProcessoVisto> {
        vec![
            ProcessoVisto {
                pid: 4,
                executavel: "System".into(),
            },
            ProcessoVisto {
                pid: PID,
                executavel: "cs2.exe".into(),
            },
        ]
    }

    /// Avança `s` segundos a 60 quadros por segundo, um passo por segundo.
    fn jogar(c: &Cenario, g: &mut Gravador, s: u64) {
        for _ in 0..s {
            let t0 = c.relogio.load(Ordering::SeqCst);
            {
                let mut p = c.presents.lock().unwrap();
                for i in 0..60u64 {
                    p.fila.push(EventoPresent {
                        pid: PID,
                        t_us: t0 + i * 16_667,
                    });
                }
            }
            c.relogio.store(t0 + 1_000_000, Ordering::SeqCst);
            g.passo();
        }
    }

    fn salvas(c: &Cenario) -> usize {
        c.armazem
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .listar()
            .unwrap()
            .len()
    }

    #[test]
    fn inicio_e_fim_de_partida_gravam_e_param_a_sessao() {
        let c = cenario("inicio-fim");
        // Jogo aparece no 1º snapshot e some depois de ~70 s (snapshot a cada 2 s).
        let mut seq = vec![vec![]];
        seq.extend(std::iter::repeat_n(jogo(), 36));
        seq.push(vec![]);
        let mut g = Gravador::novo(fontes(&c, seq, false), c.armazem.clone());
        g.passo();
        assert_eq!(g.partidas_ativas(), 0);
        assert!(!g.sessao_aberta(), "sem partida, sem sessão ETW");
        c.relogio.fetch_add(2_000_000, Ordering::SeqCst);
        g.passo();
        assert_eq!(g.partidas_ativas(), 1);
        assert!(g.sessao_aberta());
        jogar(&c, &mut g, 76);
        assert_eq!(g.partidas_ativas(), 0, "PID sumiu: partida encerrada");
        assert!(!g.sessao_aberta());
        assert!(
            c.presents.lock().unwrap().parado,
            "sessão parada ao fim da partida"
        );
        assert_eq!(salvas(&c), 1);
        assert_eq!(c.aberturas.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn partida_curta_e_descartada() {
        let c = cenario("curta");
        let mut seq = vec![jogo(); 5];
        seq.push(vec![]);
        let mut g = Gravador::novo(fontes(&c, seq, false), c.armazem.clone());
        g.passo();
        jogar(&c, &mut g, 14);
        assert_eq!(g.partidas_ativas(), 0);
        assert_eq!(salvas(&c), 0, "< 60 s não é guardada");
        assert!(c.presents.lock().unwrap().parado);
    }

    #[test]
    fn encerrar_para_a_sessao_e_grava_a_partida_aberta() {
        let c = cenario("encerrar");
        let mut g = Gravador::novo(fontes(&c, vec![jogo()], false), c.armazem.clone());
        g.passo();
        jogar(&c, &mut g, 65);
        assert!(g.sessao_aberta());
        g.encerrar();
        assert!(!g.sessao_aberta());
        assert!(c.presents.lock().unwrap().parado);
        assert_eq!(salvas(&c), 1);
    }

    #[test]
    fn soltar_o_gravador_para_a_sessao() {
        let c = cenario("soltar");
        let g = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut g = Gravador::novo(fontes(&c, vec![jogo()], false), c.armazem.clone());
            g.passo();
            assert!(g.sessao_aberta());
            panic!("pânico proposital");
        }));
        assert!(g.is_err());
        assert!(
            c.presents.lock().unwrap().parado,
            "pânico não deixa a sessão ETW aberta"
        );
    }

    #[test]
    fn sessao_negada_nao_tenta_de_novo_na_partida() {
        let c = cenario("negada");
        let mut g = Gravador::novo(fontes(&c, vec![jogo()], true), c.armazem.clone());
        g.passo();
        jogar(&c, &mut g, 5);
        assert_eq!(c.aberturas.load(Ordering::SeqCst), 1);
        assert!(!g.sessao_aberta());
    }

    #[test]
    fn erro_do_consumidor_encerra_a_sessao() {
        let c = cenario("erro");
        let mut g = Gravador::novo(fontes(&c, vec![jogo()], false), c.armazem.clone());
        g.passo();
        c.presents.lock().unwrap().erro = Some(CodigoErroMedicao::SessaoPerdida);
        jogar(&c, &mut g, 3);
        assert!(!g.sessao_aberta());
        assert!(c.presents.lock().unwrap().parado);
        assert_eq!(c.aberturas.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn variavel_desligada_nao_abre_sessao_etw() {
        assert!(medicao_desligada(Some("1")));
        assert!(!medicao_desligada(None));
        assert!(!medicao_desligada(Some("0")));
        let c = cenario("desligada");
        let f = fontes(&c, vec![jogo()], false);
        let mut p = Partidas::default();
        let raiz = c.dir.0.join("estado");
        let mut subiu = false;
        p.iniciar_com(&raiz, true, |a| {
            subiu = true;
            ThreadPartidas::com_fontes(a, move || f, false)
        });
        assert!(!subiu, "thread de partidas não sobe");
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(p.thread.is_none());
        assert_eq!(c.aberturas.load(Ordering::SeqCst), 0, "nenhuma sessão ETW");
    }

    #[test]
    fn thread_para_a_sessao_no_stop() {
        let c = cenario("thread");
        let f = fontes(&c, vec![jogo()], false);
        let t = ThreadPartidas::com_fontes(c.armazem.clone(), move || f, false).unwrap();
        for _ in 0..200 {
            if c.aberturas.load(Ordering::SeqCst) > 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(c.aberturas.load(Ordering::SeqCst), 1);
        drop(t);
        assert!(c.presents.lock().unwrap().parado);
    }

    #[test]
    fn so_o_console_ve_e_apaga() {
        let c = cenario("console");
        let mut g = Gravador::novo(fontes(&c, vec![jogo()], false), c.armazem.clone());
        g.passo();
        jogar(&c, &mut g, 65);
        g.encerrar();
        drop(g);
        let p = Partidas {
            armazem: c.armazem.clone(),
            ..Default::default()
        };
        let console = ident("S-1-5-21-1");
        let outro = ident("S-1-5-21-2");
        let listar = Requisicao::ListarPartidas {
            desde_unix: 0,
            limite: 10,
        };
        let Some(Resposta::Partidas { resumos }) = p.atender(&console, Some(&console), &listar)
        else {
            panic!("lista esperada");
        };
        assert_eq!(resumos.len(), 1);
        let id = resumos[0].id;
        assert!(matches!(
            p.atender(&console, Some(&console), &Requisicao::ObterPartida { id }),
            Some(Resposta::Partida(_))
        ));
        assert!(matches!(
            p.atender(&outro, Some(&console), &listar),
            Some(Resposta::Partidas { resumos }) if resumos.is_empty()
        ));
        assert!(matches!(
            p.atender(&outro, Some(&console), &Requisicao::ObterPartida { id }),
            Some(Resposta::Erro {
                codigo: CodigoErro::EntradaInvalida,
                ..
            })
        ));
        assert!(matches!(
            p.atender(&outro, Some(&console), &Requisicao::ApagarPartidas),
            Some(Resposta::Erro { .. })
        ));
        assert!(matches!(
            p.atender(&console, None, &listar),
            Some(Resposta::Partidas { resumos }) if resumos.is_empty()
        ));
        assert!(matches!(
            p.atender(&console, Some(&console), &Requisicao::ApagarPartidas),
            Some(Resposta::Ok)
        ));
        assert_eq!(salvas(&c), 0);
        let _ = requisicoes();
    }

    #[test]
    fn id_de_jogo_detectado() {
        assert_eq!(id_detectado("Meu Jogo.EXE"), "meujogo");
        assert_eq!(id_detectado(r"C:\x\a-b_c.exe"), "a-b_c");
        assert_eq!(id_detectado("???.exe"), "jogo");
    }
}

#[test]
fn versao_exibida_marca_build_de_teste() {
    assert_eq!(
        crate::VERSAO.ends_with("+teste"),
        cfg!(feature = "medicao-partidas")
    );
    assert!(crate::VERSAO.starts_with(env!("CARGO_PKG_VERSION")));
}
