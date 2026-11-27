//! Portão único de gravação no DRS (T-02; F1-06 e F1-07).
//!
//! Toda chamada que altera o driver (`criar_perfil`, `criar_aplicacao`,
//! `gravar_setting`, `remover_setting`, `salvar`) exige uma [`Licenca`], e a
//! única forma de obter uma é [`Portao::verificar`], que lista os processos
//! (só por nome, `TH32CS_SNAPPROCESS`, sem abrir handle) **no momento** da
//! verificação. O motor guarda o [`DriverStore`](crate::DriverStore) dentro
//! de [`Guardado`], que só dá acesso mutável mediante a licença: gravar sem
//! passar pelo portão não compila.
//!
//! Conjunto vigiado = exes dos jogos detectados ([`Portao::vigiar_jogos`]) +
//! exes e processos protegidos da base curada
//! ([`nvt_core::BaseJogos::processos_vigiados`]) +
//! [`nvt_detect::ANTICHEAT_SO_DURANTE_JOGO`]. Comparação sem diferenciar
//! maiúsculas. Falso positivo por nome só adia.

use std::fmt;
use std::ops::Deref;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use nvt_core::BaseJogos;
use nvt_detect::{ANTICHEAT_SO_DURANTE_JOGO, FonteProcessos, ProcessosDoSistema};

/// Marcador posto em [`Adiamento::processos`] quando a lista de processos não
/// pôde ser lida (falha fechada; nenhum nome de processo é exposto).
pub const LISTA_INDISPONIVEL: &str = "lista de processos indisponível";

/// Gravação recusada pelo portão: há processo vigiado em execução ou a lista
/// de processos não pôde ser lida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adiamento {
    /// Processos vigiados em execução (grafia da lista vigiada, sem
    /// repetição) ou, com `lista_indisponivel`, só [`LISTA_INDISPONIVEL`].
    pub processos: Vec<String>,
    /// Desde quando (segundos Unix) o portão adia sem interrupção.
    pub desde_unix: u64,
    /// A lista de processos não pôde ser lida: não se sabe o que está aberto.
    pub lista_indisponivel: bool,
}

impl fmt::Display for Adiamento {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.lista_indisponivel {
            return write!(f, "gravação adiada: {LISTA_INDISPONIVEL}");
        }
        write!(
            f,
            "gravação adiada: processo(s) de jogo em execução ({})",
            self.processos.join(", ")
        )
    }
}

/// Autorização de uma operação de gravação. Só [`Portao::verificar`] a cria;
/// não é `Clone` nem `Copy`.
#[derive(Debug)]
pub struct Licenca {
    _privado: (),
}

/// Valor que só dá acesso mutável com uma [`Licenca`] (leitura livre por
/// `Deref`).
#[derive(Debug)]
pub(crate) struct Guardado<S>(S);

impl<S> Guardado<S> {
    pub(crate) fn new(s: S) -> Self {
        Self(s)
    }

    /// Acesso para gravar: exige a licença do portão.
    pub(crate) fn escrita(&mut self, _licenca: &Licenca) -> &mut S {
        &mut self.0
    }
}

impl<S> Deref for Guardado<S> {
    type Target = S;

    fn deref(&self) -> &S {
        &self.0
    }
}

struct Interno {
    fonte: Box<dyn FonteProcessos + Send + Sync>,
    exes_jogos: Mutex<Vec<String>>,
    /// Início do adiamento atual (segundos Unix); 0 = nenhum.
    desde: AtomicU64,
}

/// Portão de gravação, com a fonte de processos injetável. Clonar
/// compartilha a fonte, a lista de jogos e o início do adiamento (o serviço
/// recria o motor a cada ciclo e passa o mesmo portão).
#[derive(Clone)]
pub struct Portao(Arc<Interno>);

impl fmt::Debug for Portao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Portao")
            .field("exes_jogos", &self.exes_jogos())
            .field("desde", &self.0.desde.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl Default for Portao {
    /// Portão sobre os processos do sistema.
    fn default() -> Self {
        Self::do_sistema()
    }
}

fn agora_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |d| d.as_secs().max(1))
}

impl Portao {
    /// Portão sobre a fonte de processos dada (testes: `ProcessosFalsos` ou
    /// uma fonte compartilhada que muda durante o teste).
    pub fn new(fonte: impl FonteProcessos + Send + Sync + 'static) -> Self {
        Self(Arc::new(Interno {
            fonte: Box::new(fonte),
            exes_jogos: Mutex::new(Vec::new()),
            desde: AtomicU64::new(0),
        }))
    }

    /// Portão sobre os processos do sistema (`TH32CS_SNAPPROCESS`).
    pub fn do_sistema() -> Self {
        Self::new(ProcessosDoSistema)
    }

    /// Troca a lista de exes dos jogos detectados (vazios são ignorados).
    pub fn vigiar_jogos<'a>(&self, exes: impl IntoIterator<Item = &'a str>) {
        let mut v: Vec<String> = Vec::new();
        for e in exes {
            if !e.is_empty() && !v.iter().any(|x| x.eq_ignore_ascii_case(e)) {
                v.push(e.to_owned());
            }
        }
        *self
            .0
            .exes_jogos
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = v;
    }

    /// Exes dos jogos detectados vigiados.
    pub fn exes_jogos(&self) -> Vec<String> {
        self.0
            .exes_jogos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Conjunto vigiado: jogos detectados, `extras`, base curada e
    /// anticheats que só existem durante o jogo.
    pub fn vigiados(&self, base: &BaseJogos, extras: &[&str]) -> Vec<String> {
        let mut v = self.exes_jogos();
        let mut somar = |n: &str| {
            if !n.is_empty() && !v.iter().any(|x| x.eq_ignore_ascii_case(n)) {
                v.push(n.to_owned());
            }
        };
        for e in extras {
            somar(e);
        }
        for p in base.processos_vigiados() {
            somar(&p);
        }
        for p in ANTICHEAT_SO_DURANTE_JOGO {
            somar(p);
        }
        v
    }

    /// Consulta a lista de processos agora. Livre: devolve a [`Licenca`].
    /// Com processo vigiado em execução ou lista ilegível (falha fechada):
    /// [`Adiamento`].
    pub fn verificar(&self, base: &BaseJogos, extras: &[&str]) -> Result<Licenca, Adiamento> {
        let vigiados = self.vigiados(base, extras);
        let (rodando, lista_indisponivel) = match self.0.fonte.ler() {
            Ok(ps) => (nvt_detect::em_execucao(&ps, &vigiados), false),
            Err(_) => (vec![LISTA_INDISPONIVEL.to_owned()], true),
        };
        if rodando.is_empty() {
            self.0.desde.store(0, Ordering::Relaxed);
            return Ok(Licenca { _privado: () });
        }
        let agora = agora_unix();
        let desde =
            match self
                .0
                .desde
                .compare_exchange(0, agora, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => agora,
                Err(anterior) => anterior,
            };
        Err(Adiamento {
            processos: rodando,
            desde_unix: desde,
            lista_indisponivel,
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use nvt_detect::ProcessosFalsos;

    #[test]
    fn conjunto_vigiado_e_adiamento() {
        let base = BaseJogos::embutida().unwrap();
        let p = Portao::new(ProcessosFalsos::com_nomes(&["explorer.exe", "JOGO.EXE"]));
        assert!(p.verificar(&base, &[]).is_ok(), "jogo ainda não vigiado");
        p.vigiar_jogos(["jogo.exe", "", "Jogo.exe"]);
        assert_eq!(p.exes_jogos(), ["jogo.exe"]);
        let a = p.verificar(&base, &[]).unwrap_err();
        assert_eq!(a.processos, ["jogo.exe"]);
        let b = p.verificar(&base, &[]).unwrap_err();
        assert_eq!(
            a.desde_unix, b.desde_unix,
            "o início do adiamento se mantém"
        );
        let v = p.vigiados(&base, &["extra.exe"]);
        assert!(v.iter().any(|x| x == "extra.exe"));
        assert!(v.iter().any(|x| x == "cs2.exe"), "base curada vigiada");
        assert!(v.iter().any(|x| x == "EasyAntiCheat_EOS.exe"));
        assert!(!v.iter().any(|x| x.eq_ignore_ascii_case("vgc.exe")));
    }

    #[test]
    fn lista_ilegivel_adia_sem_nomes() {
        let base = BaseJogos::embutida().unwrap();
        let p = Portao::new(ProcessosFalsos::falhando());
        let a = p.verificar(&base, &[]).unwrap_err();
        assert!(a.lista_indisponivel);
        assert_eq!(a.processos, [LISTA_INDISPONIVEL]);
        assert!(a.desde_unix > 0);
        assert!(a.to_string().contains(LISTA_INDISPONIVEL));
    }

    #[test]
    fn anticheat_e_base_adiam_sem_jogo_detectado() {
        let base = BaseJogos::embutida().unwrap();
        for nome in ["beservice_x64.exe", "CS2.exe"] {
            let p = Portao::new(ProcessosFalsos::com_nomes(&[nome]));
            assert!(p.verificar(&base, &[]).is_err(), "{nome}");
        }
        let p = Portao::new(ProcessosFalsos::com_nomes(&["vgc.exe"]));
        assert!(
            p.verificar(&base, &[]).is_ok(),
            "Vanguard roda desde o boot"
        );
    }
}
