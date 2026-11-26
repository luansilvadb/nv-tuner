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

fn sinal(tipo: TipoSinal, gravidade: Gravidade, valor: ValorSinal, agora: u64) -> Sinal {
    Sinal {
        tipo,
        gravidade,
        valor,
        lido_unix: agora,
    }
}

fn indisponivel(tipo: TipoSinal, agora: u64) -> Sinal {
    sinal(tipo, Gravidade::Indisponivel, ValorSinal::Nenhum, agora)
}

fn pct(parte: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    (u128::from(parte.min(total)) * 100 / u128::from(total)) as u8
}

pub fn sinal_espaco(l: &impl LeitorSinaisMaquina, agora: u64) -> Sinal {
    let tipo = TipoSinal::EspacoLivre;
    match l.disco_sistema() {
        Some((livres, total)) if total > 0 && livres <= total => {
            let baixo = u128::from(livres) * 100 < u128::from(total) * u128::from(ESPACO_MIN_PCT)
                || livres < ESPACO_MIN_BYTES;
            let g = if baixo {
                Gravidade::Atencao
            } else {
                Gravidade::Ok
            };
            sinal(tipo, g, ValorSinal::Bytes { livres, total }, agora)
        }
        _ => indisponivel(tipo, agora),
    }
}

pub fn sinal_memoria(l: &impl LeitorSinaisMaquina, agora: u64) -> Sinal {
    let tipo = TipoSinal::MemoriaRepouso;
    match l.memoria_comprometida() {
        Some((uso, limite)) if limite > 0 => {
            let p = pct(uso, limite);
            let g = if p >= MEMORIA_ATENCAO_PCT {
                Gravidade::Atencao
            } else {
                Gravidade::Ok
            };
            sinal(tipo, g, ValorSinal::Percentual(p), agora)
        }
        _ => indisponivel(tipo, agora),
    }
}

/// Duas leituras separadas por `esperar(janela)`.
pub fn sinal_cpu(
    l: &impl LeitorSinaisMaquina,
    janela: Duration,
    esperar: impl FnOnce(Duration),
    agora: u64,
) -> Sinal {
    let tipo = TipoSinal::CpuRepouso;
    let Some(a) = l.tempos_cpu() else {
        return indisponivel(tipo, agora);
    };
    esperar(janela);
    let Some(b) = l.tempos_cpu() else {
        return indisponivel(tipo, agora);
    };
    let delta = |x: u64, y: u64| y.checked_sub(x);
    let (Some(ocioso), Some(kernel), Some(usuario)) = (
        delta(a.ocioso, b.ocioso),
        delta(a.kernel, b.kernel),
        delta(a.usuario, b.usuario),
    ) else {
        return indisponivel(tipo, agora);
    };
    let total = kernel.saturating_add(usuario);
    if total == 0 || ocioso > total {
        return indisponivel(tipo, agora);
    }
    let p = pct(total - ocioso, total);
    let g = if p >= CPU_ATENCAO_PCT {
        Gravidade::Atencao
    } else {
        Gravidade::Ok
    };
    sinal(tipo, g, ValorSinal::Percentual(p), agora)
}

pub fn sinal_driver(l: &impl LeitorSinaisMaquina, ultimo: Option<u32>, agora: u64) -> Sinal {
    let tipo = TipoSinal::DriverDesatualizado;
    match (l.driver_nvidia(), ultimo) {
        (Some(atual), Some(ultima)) => {
            let g = if atual < ultima {
                Gravidade::Informativo
            } else {
                Gravidade::Ok
            };
            sinal(tipo, g, ValorSinal::Versao { atual, ultima }, agora)
        }
        _ => indisponivel(tipo, agora),
    }
}

/// `Booleano(true)` = plano trocado em relação aos esperados.
pub fn sinal_plano(l: &impl LeitorSinaisMaquina, esperados: &[u128], agora: u64) -> Sinal {
    let tipo = TipoSinal::PlanoEnergia;
    match l.plano_ativo() {
        Some(guid) => {
            let trocado = !esperados.contains(&guid);
            let g = if trocado {
                Gravidade::Informativo
            } else {
                Gravidade::Ok
            };
            sinal(tipo, g, ValorSinal::Booleano(trocado), agora)
        }
        None => indisponivel(tipo, agora),
    }
}

/// `Booleano(ligado)`. Ausente ou valor fora de {0, 1} = indisponível.
pub fn sinal_storage_sense(l: &impl LeitorSinaisSessao, agora: u64) -> Sinal {
    let tipo = TipoSinal::SensorArmazenamento;
    match l.storage_policy_01() {
        Some(LeituraDword::Valor(1)) => {
            sinal(tipo, Gravidade::Ok, ValorSinal::Booleano(true), agora)
        }
        Some(LeituraDword::Valor(0)) => sinal(
            tipo,
            Gravidade::Informativo,
            ValorSinal::Booleano(false),
            agora,
        ),
        _ => indisponivel(tipo, agora),
    }
}

/// Serviço: espaço, memória, CPU, driver e plano.
pub fn ler_sinais_maquina(
    l: &impl LeitorSinaisMaquina,
    p: &ParametrosSinais,
    esperar: impl FnOnce(Duration),
    agora: u64,
) -> Vec<Sinal> {
    let cpu = match p.janela_cpu {
        Some(j) => sinal_cpu(l, j, esperar, agora),
        None => indisponivel(TipoSinal::CpuRepouso, agora),
    };
    vec![
        sinal_espaco(l, agora),
        sinal_memoria(l, agora),
        cpu,
        sinal_driver(l, p.driver_ultimo, agora),
        sinal_plano(l, &p.planos_esperados, agora),
    ]
}

/// Agente: Storage Sense.
pub fn ler_sinais_sessao(l: &impl LeitorSinaisSessao, agora: u64) -> Vec<Sinal> {
    vec![sinal_storage_sense(l, agora)]
}

#[cfg(test)]
mod testes {
    use std::cell::Cell;

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
    }

    impl LeitorSinaisMaquina for Falso {
        fn disco_sistema(&self) -> Option<(u64, u64)> {
            self.disco
        }
        fn memoria_comprometida(&self) -> Option<(u64, u64)> {
            self.memoria
        }
        fn tempos_cpu(&self) -> Option<TemposCpu> {
            let i = self.lidas.get();
            self.lidas.set(i + 1);
            self.cpu.get(i).copied()
        }
        fn plano_ativo(&self) -> Option<u128> {
            self.plano
        }
        fn driver_nvidia(&self) -> Option<u32> {
            self.driver
        }
    }

    impl LeitorSinaisSessao for Falso {
        fn storage_policy_01(&self) -> Option<LeituraDword> {
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
}
