//! Sinais de saúde: composição testável sobre leitores somente leitura.

use std::time::Duration;

use nvt_ipc::saude::{Gravidade, Sinal, TipoSinal, ValorSinal};

/// Alerta de espaço: abaixo de 10% ou de 20 GB livres (R-S-05).
pub const ESPACO_MIN_PCT: u64 = 10;
pub const ESPACO_MIN_BYTES: u64 = 20 * 1024 * 1024 * 1024;
/// Memória comprometida em repouso a partir da qual o sinal pede atenção.
pub const MEMORIA_ATENCAO_PCT: u8 = 85;
/// CPU média em repouso a partir da qual o sinal pede atenção.
pub const CPU_ATENCAO_PCT: u8 = 15;
/// Plano "Equilibrado", padrão do Windows.
pub const PLANO_EQUILIBRADO: u128 = 0x381b4222_f694_41f0_9685_ff5bb260df2e;

/// Tempos acumulados de `GetSystemTimes` (unidades de 100 ns).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemposCpu {
    pub ocioso: u64,
    /// Inclui o ocioso, como em `GetSystemTimes`.
    pub kernel: u64,
    pub usuario: u64,
}

/// Leitura de um DWORD do registro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeituraDword {
    Ausente,
    Valor(u32),
    /// Existe, mas não é DWORD.
    Outro,
}

/// Fachada somente leitura do serviço. `None` = indisponível.
pub trait LeitorSinaisMaquina {
    /// `(livres, total)` do volume do sistema.
    fn disco_sistema(&self) -> Option<(u64, u64)>;
    /// `(comprometida, limite)` em bytes.
    fn memoria_comprometida(&self) -> Option<(u64, u64)>;
    fn tempos_cpu(&self) -> Option<TemposCpu>;
    /// GUID do plano de energia ativo.
    fn plano_ativo(&self) -> Option<u128>;
    /// Versão do driver NVIDIA no formato `56603` (566.03).
    fn driver_nvidia(&self) -> Option<u32>;
}

/// Fachada somente leitura da sessão (HKCU).
pub trait LeitorSinaisSessao {
    /// `HKCU\...\StorageSense\Parameters\StoragePolicy`, valor `01`.
    fn storage_policy_01(&self) -> Option<LeituraDword>;
}

/// Parâmetros que vêm de fora desta camada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParametrosSinais {
    /// Janela de amostragem da CPU; `None` = não amostrar agora (jogo aberto, logo após
    /// o logon), e o sinal sai indisponível.
    pub janela_cpu: Option<Duration>,
    /// Última versão conhecida do catálogo; `None` = desconhecida, sinal indisponível.
    pub driver_ultimo: Option<u32>,
    /// Planos considerados esperados (padrão: Equilibrado e o da camada Windows).
    pub planos_esperados: Vec<u128>,
}

impl Default for ParametrosSinais {
    fn default() -> Self {
        Self {
            janela_cpu: None,
            driver_ultimo: None,
            planos_esperados: vec![PLANO_EQUILIBRADO],
        }
    }
}

/// Resultado da avaliação de um sinal: `None` = indisponível.
type Avaliacao = Option<(Gravidade, ValorSinal)>;

/// Espera entre amostras (CPU). Chamada no máximo uma vez por leitura.
type Esperar<'a> = &'a mut dyn FnMut(Duration);

/// Entrada do registro de sinais: o tipo e a função que o avalia.
struct Descritor<F> {
    tipo: TipoSinal,
    avaliar: F,
}

type AvaliarMaquina = fn(&dyn LeitorSinaisMaquina, &ParametrosSinais, Esperar<'_>) -> Avaliacao;
type AvaliarSessao = fn(&dyn LeitorSinaisSessao) -> Avaliacao;

/// Registro dos sinais do serviço. A ordem aqui é a ordem da lista publicada; a de leitura
/// põe a CPU primeiro (ver [`ler_sinais_maquina`]).
/// Sinal novo: escrever a função `avaliar_*` e acrescentar uma entrada.
const SINAIS_MAQUINA: &[Descritor<AvaliarMaquina>] = &[
    Descritor {
        tipo: TipoSinal::EspacoLivre,
        avaliar: |l, _, _| avaliar_espaco(l),
    },
    Descritor {
        tipo: TipoSinal::MemoriaRepouso,
        avaliar: |l, _, _| avaliar_memoria(l),
    },
    Descritor {
        tipo: TipoSinal::CpuRepouso,
        // Sem janela (jogo aberto, logo após o logon) a CPU não é amostrada.
        avaliar: |l, p, esperar| avaliar_cpu(l, p.janela_cpu?, esperar),
    },
    Descritor {
        tipo: TipoSinal::DriverDesatualizado,
        avaliar: |l, p, _| avaliar_driver(l, p.driver_ultimo),
    },
    Descritor {
        tipo: TipoSinal::PlanoEnergia,
        avaliar: |l, p, _| avaliar_plano(l, &p.planos_esperados),
    },
];

/// Registro dos sinais da sessão (agente), na ordem publicada.
const SINAIS_SESSAO: &[Descritor<AvaliarSessao>] = &[Descritor {
    tipo: TipoSinal::SensorArmazenamento,
    avaliar: avaliar_storage_sense,
}];

/// Único ponto que monta o [`Sinal`], inclusive o caso indisponível.
fn montar(tipo: TipoSinal, avaliacao: Avaliacao, agora: u64) -> Sinal {
    let (gravidade, valor) = avaliacao.unwrap_or((Gravidade::Indisponivel, ValorSinal::Nenhum));
    Sinal {
        tipo,
        gravidade,
        valor,
        lido_unix: agora,
    }
}

/// Adapta um `FnOnce` à espera do registro: chamadas extras são ignoradas.
fn uma_vez(esperar: impl FnOnce(Duration)) -> impl FnMut(Duration) {
    let mut esperar = Some(esperar);
    move |d| {
        if let Some(e) = esperar.take() {
            e(d);
        }
    }
}

fn pct(parte: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    (u128::from(parte.min(total)) * 100 / u128::from(total)) as u8
}

fn atencao_se(cond: bool) -> Gravidade {
    if cond {
        Gravidade::Atencao
    } else {
        Gravidade::Ok
    }
}

fn informativo_se(cond: bool) -> Gravidade {
    if cond {
        Gravidade::Informativo
    } else {
        Gravidade::Ok
    }
}

fn avaliar_espaco(l: &dyn LeitorSinaisMaquina) -> Avaliacao {
    let (livres, total) = l.disco_sistema().filter(|&(l, t)| t > 0 && l <= t)?;
    let baixo = u128::from(livres) * 100 < u128::from(total) * u128::from(ESPACO_MIN_PCT)
        || livres < ESPACO_MIN_BYTES;
    Some((atencao_se(baixo), ValorSinal::Bytes { livres, total }))
}

fn avaliar_memoria(l: &dyn LeitorSinaisMaquina) -> Avaliacao {
    let (uso, limite) = l.memoria_comprometida().filter(|&(_, lim)| lim > 0)?;
    let p = pct(uso, limite);
    Some((
        atencao_se(p >= MEMORIA_ATENCAO_PCT),
        ValorSinal::Percentual(p),
    ))
}

/// Duas leituras separadas por `esperar(janela)`.
fn avaliar_cpu(l: &dyn LeitorSinaisMaquina, janela: Duration, esperar: Esperar<'_>) -> Avaliacao {
    let a = l.tempos_cpu()?;
    esperar(janela);
    let b = l.tempos_cpu()?;
    let ocioso = b.ocioso.checked_sub(a.ocioso)?;
    let kernel = b.kernel.checked_sub(a.kernel)?;
    let usuario = b.usuario.checked_sub(a.usuario)?;
    let total = kernel.saturating_add(usuario);
    if total == 0 || ocioso > total {
        return None;
    }
    let p = pct(total - ocioso, total);
    Some((atencao_se(p >= CPU_ATENCAO_PCT), ValorSinal::Percentual(p)))
}

fn avaliar_driver(l: &dyn LeitorSinaisMaquina, ultimo: Option<u32>) -> Avaliacao {
    let (atual, ultima) = (l.driver_nvidia()?, ultimo?);
    Some((
        informativo_se(atual < ultima),
        ValorSinal::Versao { atual, ultima },
    ))
}

fn avaliar_plano(l: &dyn LeitorSinaisMaquina, esperados: &[u128]) -> Avaliacao {
    let trocado = !esperados.contains(&l.plano_ativo()?);
    Some((informativo_se(trocado), ValorSinal::Booleano(trocado)))
}

fn avaliar_storage_sense(l: &dyn LeitorSinaisSessao) -> Avaliacao {
    match l.storage_policy_01()? {
        LeituraDword::Valor(1) => Some((Gravidade::Ok, ValorSinal::Booleano(true))),
        LeituraDword::Valor(0) => Some((Gravidade::Informativo, ValorSinal::Booleano(false))),
        _ => None,
    }
}

// Avaliações avulsas mantidas como API pública (reexportadas e usadas nos testes).
// Sinais novos não precisam de função equivalente: o registro basta.

pub fn sinal_espaco(l: &impl LeitorSinaisMaquina, agora: u64) -> Sinal {
    montar(TipoSinal::EspacoLivre, avaliar_espaco(l), agora)
}

pub fn sinal_memoria(l: &impl LeitorSinaisMaquina, agora: u64) -> Sinal {
    montar(TipoSinal::MemoriaRepouso, avaliar_memoria(l), agora)
}

/// Duas leituras separadas por `esperar(janela)`.
pub fn sinal_cpu(
    l: &impl LeitorSinaisMaquina,
    janela: Duration,
    esperar: impl FnOnce(Duration),
    agora: u64,
) -> Sinal {
    let avaliacao = avaliar_cpu(l, janela, &mut uma_vez(esperar));
    montar(TipoSinal::CpuRepouso, avaliacao, agora)
}

pub fn sinal_driver(l: &impl LeitorSinaisMaquina, ultimo: Option<u32>, agora: u64) -> Sinal {
    montar(
        TipoSinal::DriverDesatualizado,
        avaliar_driver(l, ultimo),
        agora,
    )
}

/// `Booleano(true)` = plano trocado em relação aos esperados.
pub fn sinal_plano(l: &impl LeitorSinaisMaquina, esperados: &[u128], agora: u64) -> Sinal {
    montar(TipoSinal::PlanoEnergia, avaliar_plano(l, esperados), agora)
}

/// `Booleano(ligado)`. Ausente ou valor fora de {0, 1} = indisponível.
pub fn sinal_storage_sense(l: &impl LeitorSinaisSessao, agora: u64) -> Sinal {
    montar(
        TipoSinal::SensorArmazenamento,
        avaliar_storage_sense(l),
        agora,
    )
}

/// Serviço: sinais de [`SINAIS_MAQUINA`], na ordem do registro.
pub fn ler_sinais_maquina(
    l: &impl LeitorSinaisMaquina,
    p: &ParametrosSinais,
    esperar: impl FnOnce(Duration),
    agora: u64,
) -> Vec<Sinal> {
    let mut esperar = uma_vez(esperar);
    // Ordem de leitura: CPU primeiro (com a espera da janela), depois os demais na ordem
    // do registro. A lista publicada segue a ordem do registro.
    let mut ordem: Vec<usize> = (0..SINAIS_MAQUINA.len()).collect();
    ordem.sort_by_key(|&i| SINAIS_MAQUINA[i].tipo != TipoSinal::CpuRepouso);
    let mut saida: Vec<Option<Sinal>> = SINAIS_MAQUINA.iter().map(|_| None).collect();
    for i in ordem {
        let d = &SINAIS_MAQUINA[i];
        saida[i] = Some(montar(d.tipo, (d.avaliar)(l, p, &mut esperar), agora));
    }
    saida.into_iter().flatten().collect()
}

/// Agente: sinais de [`SINAIS_SESSAO`], na ordem do registro.
pub fn ler_sinais_sessao(l: &impl LeitorSinaisSessao, agora: u64) -> Vec<Sinal> {
    SINAIS_SESSAO
        .iter()
        .map(|d| montar(d.tipo, (d.avaliar)(l), agora))
        .collect()
}

#[cfg(test)]
mod testes {
    use std::cell::{Cell, RefCell};

    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    #[derive(Default)]
    struct Falso {
        disco: Option<(u64, u64)>,
        memoria: Option<(u64, u64)>,
        cpu: Vec<TemposCpu>,
        lidas: Cell<usize>,
        plano: Option<u128>,
        driver: Option<u32>,
        storage: Option<LeituraDword>,
        /// Sequência de chamadas ao leitor, para travar a ordem de leitura.
        chamadas: RefCell<Vec<&'static str>>,
    }

    impl Falso {
        fn anotar(&self, c: &'static str) {
            self.chamadas.borrow_mut().push(c);
        }
    }

    impl LeitorSinaisMaquina for Falso {
        fn disco_sistema(&self) -> Option<(u64, u64)> {
            self.anotar("disco");
            self.disco
        }
        fn memoria_comprometida(&self) -> Option<(u64, u64)> {
            self.anotar("memoria");
            self.memoria
        }
        fn tempos_cpu(&self) -> Option<TemposCpu> {
            self.anotar("cpu");
            let i = self.lidas.get();
            self.lidas.set(i + 1);
            self.cpu.get(i).copied()
        }
        fn plano_ativo(&self) -> Option<u128> {
            self.anotar("plano");
            self.plano
        }
        fn driver_nvidia(&self) -> Option<u32> {
            self.anotar("driver");
            self.driver
        }
    }

    impl LeitorSinaisSessao for Falso {
        fn storage_policy_01(&self) -> Option<LeituraDword> {
            self.anotar("storage");
            self.storage
        }
    }

    fn g(s: Sinal) -> Gravidade {
        s.validar().unwrap();
        s.gravidade
    }

    #[test]
    fn espaco() {
        let com = |d| Falso {
            disco: d,
            ..Falso::default()
        };
        assert_eq!(
            g(sinal_espaco(&com(Some((100 * GB, 500 * GB))), 0)),
            Gravidade::Ok
        );
        assert_eq!(
            g(sinal_espaco(&com(Some((40 * GB, 500 * GB))), 0)),
            Gravidade::Atencao
        );
        assert_eq!(
            g(sinal_espaco(&com(Some((19 * GB, 100 * GB))), 0)),
            Gravidade::Atencao
        );
        assert_eq!(
            g(sinal_espaco(&com(Some((2, 1))), 0)),
            Gravidade::Indisponivel
        );
        assert_eq!(g(sinal_espaco(&com(None), 0)), Gravidade::Indisponivel);
    }

    #[test]
    fn memoria() {
        let com = |m| Falso {
            memoria: m,
            ..Falso::default()
        };
        let s = sinal_memoria(&com(Some((50, 100))), 7);
        assert_eq!(s.valor, ValorSinal::Percentual(50));
        assert_eq!(s.lido_unix, 7);
        assert_eq!(
            g(sinal_memoria(&com(Some((90, 100))), 0)),
            Gravidade::Atencao
        );
        assert_eq!(
            g(sinal_memoria(&com(Some((1, 0))), 0)),
            Gravidade::Indisponivel
        );
    }

    #[test]
    fn cpu_amostra_e_espera() {
        let t = |o, k, u| TemposCpu {
            ocioso: o,
            kernel: k,
            usuario: u,
        };
        let f = Falso {
            cpu: vec![t(0, 0, 0), t(900, 950, 50)],
            ..Falso::default()
        };
        let esperou = Cell::new(None);
        let s = sinal_cpu(&f, Duration::from_secs(3), |d| esperou.set(Some(d)), 0);
        assert_eq!(esperou.get(), Some(Duration::from_secs(3)));
        assert_eq!(s.valor, ValorSinal::Percentual(10));
        assert_eq!(s.gravidade, Gravidade::Ok);

        let f = Falso {
            cpu: vec![t(0, 0, 0), t(100, 500, 500)],
            ..Falso::default()
        };
        assert_eq!(
            g(sinal_cpu(&f, Duration::ZERO, |_| {}, 0)),
            Gravidade::Atencao
        );

        let f = Falso {
            cpu: vec![t(10, 10, 10), t(0, 0, 0)],
            ..Falso::default()
        };
        assert_eq!(
            g(sinal_cpu(&f, Duration::ZERO, |_| {}, 0)),
            Gravidade::Indisponivel
        );
        let f = Falso::default();
        assert_eq!(
            g(sinal_cpu(&f, Duration::ZERO, |_| {}, 0)),
            Gravidade::Indisponivel
        );
    }

    #[test]
    fn driver_e_plano() {
        let f = Falso {
            driver: Some(56601),
            plano: Some(1),
            ..Falso::default()
        };
        assert_eq!(g(sinal_driver(&f, Some(57102), 0)), Gravidade::Informativo);
        assert_eq!(g(sinal_driver(&f, Some(56601), 0)), Gravidade::Ok);
        assert_eq!(g(sinal_driver(&f, None, 0)), Gravidade::Indisponivel);
        assert_eq!(
            g(sinal_driver(&Falso::default(), Some(1), 0)),
            Gravidade::Indisponivel
        );

        let s = sinal_plano(&f, &[PLANO_EQUILIBRADO], 0);
        assert_eq!(
            (s.gravidade, s.valor),
            (Gravidade::Informativo, ValorSinal::Booleano(true))
        );
        assert_eq!(g(sinal_plano(&f, &[1], 0)), Gravidade::Ok);
        assert_eq!(
            g(sinal_plano(&Falso::default(), &[1], 0)),
            Gravidade::Indisponivel
        );
    }

    #[test]
    fn storage_sense_inesperado_e_indisponivel() {
        let com = |s| Falso {
            storage: s,
            ..Falso::default()
        };
        let s = sinal_storage_sense(&com(Some(LeituraDword::Valor(1))), 0);
        assert_eq!(s.valor, ValorSinal::Booleano(true));
        let s = sinal_storage_sense(&com(Some(LeituraDword::Valor(0))), 0);
        assert_eq!(s.valor, ValorSinal::Booleano(false));
        for v in [
            Some(LeituraDword::Valor(7)),
            Some(LeituraDword::Outro),
            Some(LeituraDword::Ausente),
            None,
        ] {
            assert_eq!(g(sinal_storage_sense(&com(v), 0)), Gravidade::Indisponivel);
        }
    }

    #[test]
    fn agregados_sem_repeticao() {
        let f = Falso::default();
        let m = ler_sinais_maquina(&f, &ParametrosSinais::default(), |_| panic!(), 0);
        let s = ler_sinais_sessao(&f, 0);
        let sessao = nvt_ipc::saude::SaudeSessao {
            sinais: s,
            ..Default::default()
        };
        sessao.validar().unwrap();
        let mut tipos: Vec<_> = m.iter().map(|s| s.tipo).collect();
        tipos.dedup();
        assert_eq!(tipos.len(), 5);
        assert!(m.iter().all(|s| s.gravidade == Gravidade::Indisponivel));
        assert_eq!(f.lidas.get(), 0, "sem janela, CPU não é amostrada");
    }

    #[test]
    fn ordem_de_leitura_cpu_primeiro() {
        let t = TemposCpu {
            ocioso: 0,
            kernel: 0,
            usuario: 0,
        };
        let f = Falso {
            cpu: vec![t, t],
            ..Falso::default()
        };
        let p = ParametrosSinais {
            janela_cpu: Some(Duration::ZERO),
            ..ParametrosSinais::default()
        };
        let m = ler_sinais_maquina(&f, &p, |_| f.anotar("esperar"), 0);
        assert_eq!(
            *f.chamadas.borrow(),
            [
                "cpu", "esperar", "cpu", "disco", "memoria", "driver", "plano"
            ]
        );
        let tipos: Vec<_> = m.iter().map(|s| s.tipo).collect();
        assert_eq!(
            tipos,
            [
                TipoSinal::EspacoLivre,
                TipoSinal::MemoriaRepouso,
                TipoSinal::CpuRepouso,
                TipoSinal::DriverDesatualizado,
                TipoSinal::PlanoEnergia,
            ]
        );

        let f = Falso::default();
        let s = ler_sinais_sessao(&f, 0);
        assert_eq!(*f.chamadas.borrow(), ["storage"]);
        let tipos: Vec<_> = s.iter().map(|s| s.tipo).collect();
        assert_eq!(tipos, [TipoSinal::SensorArmazenamento]);
    }
}
