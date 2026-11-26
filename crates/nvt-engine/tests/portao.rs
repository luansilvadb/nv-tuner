//! Portão único de gravação (T-02): com processo vigiado em execução, nenhuma
//! operação do motor chega ao driver.

use std::sync::{Arc, Mutex};

use nvt_detect::{FonteProcessos, Processo};
use nvt_drs::ProfileRef;
use nvt_engine::{Armazem, DriverFalso, Engine, ErroEngine, JogoEntrada, Portao};

const EXE_WARDOGS: &str = "WardogsClient-Win64-Shipping.exe";

/// Fonte de processos que o teste abre e fecha.
#[derive(Clone, Default)]
struct Processos(Arc<Mutex<Vec<String>>>);

impl Processos {
    fn abrir(&self, nome: &str) {
        self.0.lock().unwrap().push(nome.to_owned());
    }
    fn fechar_todos(&self) {
        self.0.lock().unwrap().clear();
    }
}

impl FonteProcessos for Processos {
    fn processos(&self) -> Vec<Processo> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, n)| Processo::new(n.clone(), i as u32 + 10, 0))
            .collect()
    }
}

fn driver() -> DriverFalso {
    let d = DriverFalso::turing();
    d.definir_padrao(0x00A8_79CF, 0x6092_5292);
    d.adicionar_perfil_predefinido("Wardogs", &[EXE_WARDOGS], &[]);
    d
}

fn jogos() -> Vec<JogoEntrada> {
    let mut w = JogoEntrada::new("WARDOGS", EXE_WARDOGS);
    w.dlss = Some(nvt_core::Versao::new(310, 6, 0, 0));
    vec![w]
}

fn motor(d: &DriverFalso, p: &Processos) -> Engine<DriverFalso> {
    Engine::new(d.clone(), Armazem::em_memoria())
        .unwrap()
        .com_portao(Portao::new(p.clone()))
}

fn adiada<T: std::fmt::Debug>(r: Result<T, ErroEngine>) -> nvt_engine::Adiamento {
    match r {
        Err(ErroEngine::Adiada(a)) => a,
        outro => panic!("esperava Adiada, veio {outro:?}"),
    }
}

#[test]
fn jogo_do_diff_aberto_adia_a_aplicacao_sem_tocar_no_driver() {
    let d = driver();
    let p = Processos::default();
    p.abrir("wardogsclient-win64-shipping.exe");
    let mut e = motor(&d, &p);
    let diff = e.planejar(&jogos()).unwrap();
    assert!(!diff.vazio());
    let a = adiada(e.aplicar(&diff, true));
    assert_eq!(a.processos, [EXE_WARDOGS]);
    assert!(a.desde_unix > 0);
    assert_eq!(d.gravacoes(), 0);
    assert_eq!(d.salvamentos(), 0);
    assert!(e.armazem().snapshot_original().is_none(), "nada mudou");
    assert!(e.estado().esperado.is_empty());
    assert!(
        e.gravar_permitido(&ProfileRef::Base, 0x00A8_79CF, 0x0812_4C0B)
            .is_err()
    );
    assert_eq!(d.gravacoes(), 0);

    p.fechar_todos();
    assert!(e.pode_gravar().is_ok());
    assert!(e.aplicar(&diff, true).unwrap().aplicada);
    assert!(d.gravacoes() > 0);
}

#[test]
fn anticheat_aberto_adia_deriva_e_restauracao() {
    let d = driver();
    let p = Processos::default();
    let mut e = motor(&d, &p);
    let diff = e.planejar(&jogos()).unwrap();
    e.aplicar(&diff, true).unwrap();
    let (g, s) = (d.gravacoes(), d.salvamentos());

    // Restaurar com anticheat aberto.
    p.abrir("EasyAntiCheat_EOS.exe");
    let a = adiada(e.restaurar(true));
    assert_eq!(a.processos, ["EasyAntiCheat_EOS.exe"]);
    assert_eq!((d.gravacoes(), d.salvamentos()), (g, s));

    // Jogo vigiado pela lista do portão (sem estar no diff).
    p.fechar_todos();
    e.portao().vigiar_jogos(["outro.exe"]);
    p.abrir("OUTRO.EXE");
    adiada(e.restaurar(true));
    assert_eq!((d.gravacoes(), d.salvamentos()), (g, s));

    // Reset em massa (o driver voltou tudo ao padrão) com o jogo aberto.
    d.resetar_settings();
    let deriva = e.verificar_deriva().unwrap();
    assert!(!deriva.divergencias.is_empty(), "a leitura da deriva segue");
    adiada(e.tratar_deriva(&deriva, true));
    assert_eq!((d.gravacoes(), d.salvamentos()), (g, s));

    p.fechar_todos();
    assert!(e.tratar_deriva(&deriva, true).unwrap().aplicado);
    assert!(d.gravacoes() > g);
}
