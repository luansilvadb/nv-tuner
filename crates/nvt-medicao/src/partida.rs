//! Definição de partida e associação PID -> jogo (R-M-02, R-M-04).
//!
//! A partida começa quando um executável da base curada aparece no snapshot de processos
//! e termina quando o PID some (polling de [`POLLING_S`]). A associação de um Present ao
//! jogo é **só** pelo PID do evento cruzado com o snapshot: nenhum handle para o processo.

use std::collections::{HashMap, HashSet};

use nvt_ipc::{CodigoErroMedicao, Partida};

use crate::fachada::{EventoPresent, FonteProcessos, LeituraGpu, ProcessoVisto};
use crate::nucleo::{Agregador, Descarte, MetaPartida};

/// Intervalo de renovação do snapshot pelo serviço.
pub const POLLING_S: u64 = 2;

/// Base curada: executável (nome do arquivo) -> ID do jogo.
pub trait BaseJogos {
    fn jogo_de(&self, executavel: &str) -> Option<String>;
}

impl<F: Fn(&str) -> Option<String>> BaseJogos for F {
    fn jogo_de(&self, executavel: &str) -> Option<String> {
        self(executavel)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventoPartida {
    Inicio {
        pid: u32,
        executavel: String,
        jogo_id: String,
    },
    Fim {
        pid: u32,
    },
}

/// Compara snapshots sucessivos e emite início e fim de partida.
#[derive(Debug, Default)]
pub struct MonitorPartidas {
    ativos: HashMap<u32, String>,
}

impl MonitorPartidas {
    pub fn observar(
        &mut self,
        snapshot: &[ProcessoVisto],
        base: &dyn BaseJogos,
    ) -> Vec<EventoPartida> {
        let mut eventos = Vec::new();
        let atuais: HashMap<u32, &str> = snapshot
            .iter()
            .map(|p| (p.pid, p.executavel.as_str()))
            .collect();
        // Fim: PID sumiu ou foi reusado por outro executável.
        let mut fins: Vec<u32> = self
            .ativos
            .iter()
            .filter(|(pid, exe)| atuais.get(pid).is_none_or(|a| !a.eq_ignore_ascii_case(exe)))
            .map(|(pid, _)| *pid)
            .collect();
        fins.sort_unstable();
        for pid in fins {
            self.ativos.remove(&pid);
            eventos.push(EventoPartida::Fim { pid });
        }
        for p in snapshot {
            if self.ativos.contains_key(&p.pid) {
                continue;
            }
            if let Some(jogo_id) = base.jogo_de(&p.executavel) {
                self.ativos.insert(p.pid, p.executavel.clone());
                eventos.push(EventoPartida::Inicio {
                    pid: p.pid,
                    executavel: p.executavel.clone(),
                    jogo_id,
                });
            }
        }
        eventos
    }

    pub fn ativos(&self) -> impl Iterator<Item = u32> + '_ {
        self.ativos.keys().copied()
    }
}

/// Resultado de uma partida encerrada.
pub type Encerrada = Result<Partida, (MetaPartida, Descarte)>;

/// Orquestra monitor, associação e agregadores. Puro: o relógio (`t_us`, monotônico no
/// mesmo domínio dos Presents) e o `agora_unix` vêm do chamador.
#[derive(Debug, Default)]
pub struct Medidor {
    monitor: MonitorPartidas,
    /// PIDs do último snapshot (qualquer executável), para saber se um PID é conhecido.
    conhecidos: HashSet<u32>,
    /// PIDs que já motivaram renovação desde o último snapshot (evita renovar a cada evento).
    renovados: HashSet<u32>,
    ativas: HashMap<u32, Agregador>,
    sequencia: u32,
    perdidos_vistos: u64,
}

impl Medidor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Aplica um snapshot novo: abre e fecha partidas.
    pub fn atualizar(
        &mut self,
        snapshot: &[ProcessoVisto],
        base: &dyn BaseJogos,
        t_us: u64,
        agora_unix: u64,
    ) -> Vec<Encerrada> {
        self.conhecidos = snapshot.iter().map(|p| p.pid).collect();
        self.renovados.clear();
        let mut encerradas = Vec::new();
        for ev in self.monitor.observar(snapshot, base) {
            match ev {
                EventoPartida::Inicio {
                    pid,
                    executavel,
                    jogo_id,
                } => {
                    self.sequencia = self.sequencia.wrapping_add(1);
                    let meta = MetaPartida {
                        id: (agora_unix << 20) | u64::from(self.sequencia & 0xF_FFFF),
                        jogo_id,
                        executavel,
                        inicio_unix: agora_unix,
                    };
                    self.ativas.insert(pid, Agregador::new(meta, t_us));
                }
                EventoPartida::Fim { pid } => {
                    if let Some(ag) = self.ativas.remove(&pid) {
                        encerradas.push(fechar(ag, t_us));
                    }
                }
            }
        }
        encerradas
    }

    /// Entrega um Present. PID desconhecido renova o snapshot uma vez (R-M-02); PID que
    /// não é de partida ativa é ignorado.
    pub fn present(
        &mut self,
        ev: EventoPresent,
        fonte: &mut dyn FonteProcessos,
        base: &dyn BaseJogos,
        agora_unix: u64,
    ) -> Vec<Encerrada> {
        let mut encerradas = Vec::new();
        if !self.conhecidos.contains(&ev.pid) && self.renovados.insert(ev.pid) {
            if let Ok(s) = fonte.snapshot() {
                let renovados = std::mem::take(&mut self.renovados);
                encerradas = self.atualizar(&s, base, ev.t_us, agora_unix);
                self.renovados = renovados;
            }
        }
        if let Some(ag) = self.ativas.get_mut(&ev.pid) {
            ag.registrar_present(ev.t_us);
        }
        encerradas
    }

    pub fn gpu(&mut self, t_us: u64, leitura: LeituraGpu) {
        for ag in self.ativas.values_mut() {
            ag.registrar_gpu(t_us, leitura);
        }
    }

    /// Recebe o total acumulado de perdas da sessão e atribui o incremento às partidas
    /// ativas (a perda não tem PID).
    pub fn perdidos_acumulados(&mut self, total: u64) {
        let delta = total.saturating_sub(self.perdidos_vistos);
        self.perdidos_vistos = self.perdidos_vistos.max(total);
        if delta > 0 {
            for ag in self.ativas.values_mut() {
                ag.registrar_perdidos(delta);
            }
        }
    }

    /// Erro da fonte: marca todas as partidas ativas; a sessão não é retomada nelas.
    pub fn erro(&mut self, codigo: CodigoErroMedicao) {
        for ag in self.ativas.values_mut() {
            ag.marcar_erro(codigo);
        }
    }

    /// Encerra tudo (parada do serviço).
    pub fn encerrar(&mut self, t_us: u64) -> Vec<Encerrada> {
        let mut pids: Vec<u32> = self.ativas.keys().copied().collect();
        pids.sort_unstable();
        pids.into_iter()
            .filter_map(|pid| self.ativas.remove(&pid))
            .map(|ag| fechar(ag, t_us))
            .collect()
    }

    pub fn partidas_ativas(&self) -> usize {
        self.ativas.len()
    }
}

fn fechar(ag: Agregador, t_us: u64) -> Encerrada {
    let meta = ag.meta().clone();
    ag.finalizar(t_us).map_err(|d| (meta, d))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fachada::falsos::ProcessosFalsos;

    fn base(exe: &str) -> Option<String> {
        exe.eq_ignore_ascii_case("cs2.exe")
            .then(|| "cs2".to_string())
    }

    fn proc(pid: u32, exe: &str) -> ProcessoVisto {
        ProcessoVisto {
            pid,
            executavel: exe.into(),
        }
    }

    #[test]
    fn monitor_abre_fecha_e_trata_reuso_de_pid() {
        let mut m = MonitorPartidas::default();
        let ev = m.observar(&[proc(4, "explorer.exe"), proc(10, "CS2.exe")], &base);
        assert_eq!(
            ev,
            vec![EventoPartida::Inicio {
                pid: 10,
                executavel: "CS2.exe".into(),
                jogo_id: "cs2".into()
            }]
        );
        assert!(m.observar(&[proc(10, "CS2.exe")], &base).is_empty());
        // PID reusado por outro executável: fim.
        assert_eq!(
            m.observar(&[proc(10, "notepad.exe")], &base),
            vec![EventoPartida::Fim { pid: 10 }]
        );
        assert!(m.observar(&[], &base).is_empty());
    }

    #[test]
    fn partida_curta_e_descartada_e_longa_e_guardada() {
        let mut med = Medidor::new();
        let mut fonte = ProcessosFalsos::default();
        assert!(
            med.atualizar(&[proc(10, "cs2.exe")], &base, 0, 1_000)
                .is_empty()
        );
        for i in 0..3_000u64 {
            med.present(
                EventoPresent {
                    pid: 10,
                    t_us: i * 10_000,
                },
                &mut fonte,
                &base,
                1_000,
            );
        }
        let enc = med.atualizar(&[], &base, 30_000_000, 1_030);
        assert!(matches!(enc.as_slice(), [Err((_, Descarte::Curta))]));

        med.atualizar(&[proc(11, "cs2.exe")], &base, 100_000_000, 2_000);
        for i in 0..7_000u64 {
            let t = 100_000_000 + i * 10_000;
            med.present(EventoPresent { pid: 11, t_us: t }, &mut fonte, &base, 2_000);
        }
        let enc = med.encerrar(170_000_000);
        let p = enc.into_iter().next().unwrap().unwrap();
        assert_eq!(p.resumo.duracao_s, 70);
        assert_eq!(p.resumo.jogo_id, "cs2");
        assert_eq!(p.resumo.inicio_unix, 2_000);
        assert_eq!(fonte.chamadas, 0);
    }

    #[test]
    fn pid_desconhecido_renova_snapshot_uma_vez() {
        let mut med = Medidor::new();
        let mut fonte = ProcessosFalsos::default();
        fonte
            .sequencia
            .push_back(vec![proc(4, "dwm.exe"), proc(20, "cs2.exe")]);
        med.atualizar(&[proc(4, "dwm.exe")], &base, 0, 1);
        // Present do DWM: PID conhecido, ignorado sem renovar.
        med.present(EventoPresent { pid: 4, t_us: 1 }, &mut fonte, &base, 1);
        assert_eq!(fonte.chamadas, 0);
        // PID novo: renova e já associa ao jogo.
        med.present(EventoPresent { pid: 20, t_us: 2 }, &mut fonte, &base, 1);
        assert_eq!(fonte.chamadas, 1);
        assert_eq!(med.partidas_ativas(), 1);
        // PID que não está em lugar nenhum: renova uma vez só.
        med.present(EventoPresent { pid: 99, t_us: 3 }, &mut fonte, &base, 1);
        med.present(EventoPresent { pid: 99, t_us: 4 }, &mut fonte, &base, 1);
        assert_eq!(fonte.chamadas, 2);
    }

    #[test]
    fn perdas_e_erro_chegam_as_partidas_ativas() {
        let mut med = Medidor::new();
        let mut fonte = ProcessosFalsos::default();
        med.atualizar(&[proc(10, "cs2.exe")], &base, 0, 1);
        for i in 0..6_200u64 {
            med.present(
                EventoPresent {
                    pid: 10,
                    t_us: i * 10_000,
                },
                &mut fonte,
                &base,
                1,
            );
        }
        med.perdidos_acumulados(10);
        med.perdidos_acumulados(10);
        med.erro(CodigoErroMedicao::SessaoPerdida);
        let p = med.encerrar(62_000_000).remove(0).unwrap();
        assert_eq!(
            p.resumo.encerrada_por_erro,
            Some(CodigoErroMedicao::SessaoPerdida)
        );
        assert!(p.resumo.perda_eventos_pct > 0.0 && p.resumo.perda_eventos_pct < 1.0);
    }
}
