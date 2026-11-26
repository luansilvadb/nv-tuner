//! Apoio comum aos testes do motor.

use nvt_engine::{Armazem, DriverStore, Engine, Portao, ProcessosFalsos};

/// Portão sobre uma lista de processos falsa e vazia: os testes não dependem
/// dos jogos ou anticheats abertos na máquina que os executa.
pub fn sem_processos() -> Portao {
    Portao::new(ProcessosFalsos::default())
}

/// [`Engine::new`] com o portão de [`sem_processos`].
pub fn motor<S: DriverStore>(store: S, armazem: Armazem) -> Engine<S> {
    Engine::new(store, armazem)
        .expect("regras embutidas")
        .com_portao(sem_processos())
}
