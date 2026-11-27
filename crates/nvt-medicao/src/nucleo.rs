//! Núcleo puro da medição (R-M-05, R-M-06): recebe timestamps de Present de **um** PID e
//! produz amostras por segundo e o resumo da partida. Nenhuma API do sistema.
//!
//! Definições:
//! - frametime: diferença entre timestamps de `Present_Start` consecutivos do mesmo PID
//!   (equivale ao `MsBetweenPresents` do PresentMon);
//! - engasgo: frametime > [`FATOR_ENGASGO`] x mediana móvel dos últimos
//!   [`JANELA_MEDIANA_US`] **e** > [`PISO_ENGASGO_US`];
//! - 1% low: percentil 99 do frametime convertido em FPS (0,1% low: percentil 99,9).
//!
//! Os frametimes ficam só em memória durante a partida; em disco vão apenas agregados.

use std::collections::VecDeque;

use nvt_ipc::{
    AMOSTRAS_MAX, AmostraSegundo, CodigoErroMedicao, FAIXAS_HISTOGRAMA, Partida, ResumoPartida,
};

use crate::fachada::LeituraGpu;

/// Versão do formato do resumo e do arquivo `<id>.bin`.
pub const VERSAO_FORMATO: u16 = 1;
/// Fator sobre a mediana móvel acima do qual o quadro é engasgo.
pub const FATOR_ENGASGO: f64 = 2.5;
/// Piso absoluto do engasgo (25 ms): oscilações a FPS alto não contam.
pub const PISO_ENGASGO_US: u32 = 25_000;
/// Janela da mediana móvel (2 s).
pub const JANELA_MEDIANA_US: u64 = 2_000_000;
/// Partidas mais curtas que isso são descartadas (R-M-04).
pub const DURACAO_MINIMA_S: u32 = 60;
/// Perda de eventos acima disso encerra a medição da partida (R-M-07).
pub const PERDA_MAXIMA_PCT: f32 = 5.0;
/// Limites do histograma logarítmico (0,5 ms a 500 ms).
pub const HISTOGRAMA_MIN_US: f64 = 500.0;
pub const HISTOGRAMA_MAX_US: f64 = 500_000.0;

/// Faixa (0..64) do histograma logarítmico para um frametime em microssegundos.
pub fn faixa_histograma(frametime_us: u32) -> usize {
    let ft = f64::from(frametime_us.max(1));
    if ft <= HISTOGRAMA_MIN_US {
        return 0;
    }
    let posicao = (ft / HISTOGRAMA_MIN_US).ln() / (HISTOGRAMA_MAX_US / HISTOGRAMA_MIN_US).ln();
    ((posicao * FAIXAS_HISTOGRAMA as f64) as usize).min(FAIXAS_HISTOGRAMA - 1)
}

/// Regra de engasgo; sem mediana (início da partida) nada é engasgo.
pub fn eh_engasgo(frametime_us: u32, mediana_us: Option<u32>) -> bool {
    match mediana_us {
        Some(m) => {
            frametime_us > PISO_ENGASGO_US && f64::from(frametime_us) > FATOR_ENGASGO * f64::from(m)
        }
        None => false,
    }
}

/// FPS do percentil: `fracao_pior` = 0,01 dá o 1% low (percentil 99 do frametime).
/// Método do percentil "nearest rank". Lista vazia dá 0.
pub fn fps_low_percentil(frametimes_us: &[u32], fracao_pior: f64) -> f32 {
    if frametimes_us.is_empty() {
        return 0.0;
    }
    let mut ordenados = frametimes_us.to_vec();
    ordenados.sort_unstable();
    fps_low_ordenados(&ordenados, fracao_pior)
}

fn fps_low_ordenados(ordenados: &[u32], fracao_pior: f64) -> f32 {
    if ordenados.is_empty() {
        return 0.0;
    }
    let n = ordenados.len();
    let rank = ((1.0 - fracao_pior) * n as f64).ceil() as usize;
    let ft = ordenados[rank.clamp(1, n) - 1];
    fps_de(ft)
}

fn fps_de(frametime_us: u32) -> f32 {
    if frametime_us == 0 {
        0.0
    } else {
        (1_000_000.0 / f64::from(frametime_us)) as f32
    }
}

fn mediana_ordenados(ordenados: &[u32]) -> Option<u32> {
    let n = ordenados.len();
    match n {
        0 => None,
        _ if n % 2 == 1 => Some(ordenados[n / 2]),
        _ => Some(((u64::from(ordenados[n / 2 - 1]) + u64::from(ordenados[n / 2])) / 2) as u32),
    }
}

/// Mediana dos frametimes dos últimos `largura_us`, mantida em vetor ordenado.
#[derive(Debug, Clone)]
pub struct MedianaMovel {
    largura_us: u64,
    janela: VecDeque<(u64, u32)>,
    ordenados: Vec<u32>,
}

impl MedianaMovel {
    pub fn new(largura_us: u64) -> Self {
        Self {
            largura_us,
            janela: VecDeque::new(),
            ordenados: Vec::new(),
        }
    }

    /// Mediana da janela que termina em `t_us` (exclusive o quadro corrente).
    pub fn mediana(&mut self, t_us: u64) -> Option<u32> {
        while let Some(&(t, ft)) = self.janela.front() {
            if t + self.largura_us >= t_us {
                break;
            }
            self.janela.pop_front();
            if let Ok(i) = self.ordenados.binary_search(&ft) {
                self.ordenados.remove(i);
            }
        }
        mediana_ordenados(&self.ordenados)
    }

    pub fn inserir(&mut self, t_us: u64, frametime_us: u32) {
        self.janela.push_back((t_us, frametime_us));
        let i = self.ordenados.partition_point(|&v| v < frametime_us);
        self.ordenados.insert(i, frametime_us);
    }
}

/// Identificação da partida, vinda do monitor de processos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaPartida {
    pub id: u64,
    pub jogo_id: String,
    pub executavel: String,
    pub inicio_unix: u64,
}

/// Por que uma partida não foi guardada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Descarte {
    /// Menos de [`DURACAO_MINIMA_S`].
    Curta,
    /// Nenhum Present associado ao PID.
    SemPresents,
}

#[derive(Debug, Clone, Copy, Default)]
struct Acumulado {
    quadros: u32,
    soma_ft_us: u64,
    max_ft_us: u32,
    engasgos: u32,
    gpu: Option<LeituraGpu>,
}

/// Agregador de uma partida (um PID). `t0_us` é o instante, no mesmo relógio dos
/// Presents, em que a partida começou.
#[derive(Debug, Clone)]
pub struct Agregador {
    meta: MetaPartida,
    t0_us: u64,
    ultimo_present_us: Option<u64>,
    movel: MedianaMovel,
    frametimes: Vec<u32>,
    histograma: [u32; FAIXAS_HISTOGRAMA],
    engasgos: u32,
    tempo_engasgado_us: u64,
    segundos: Vec<Acumulado>,
    presents: u64,
    perdidos: u64,
    erro: Option<CodigoErroMedicao>,
}

impl Agregador {
    pub fn new(meta: MetaPartida, t0_us: u64) -> Self {
        Self {
            meta,
            t0_us,
            ultimo_present_us: None,
            movel: MedianaMovel::new(JANELA_MEDIANA_US),
            frametimes: Vec::new(),
            histograma: [0; FAIXAS_HISTOGRAMA],
            engasgos: 0,
            tempo_engasgado_us: 0,
            segundos: Vec::new(),
            presents: 0,
            perdidos: 0,
            erro: None,
        }
    }

    pub fn meta(&self) -> &MetaPartida {
        &self.meta
    }

    /// Erro já registrado (a medição daquela partida parou de receber dados).
    pub fn erro(&self) -> Option<CodigoErroMedicao> {
        self.erro
    }

    fn segundo(&mut self, t_us: u64) -> &mut Acumulado {
        let s = ((t_us - self.t0_us) / 1_000_000) as usize;
        if self.segundos.len() <= s {
            self.segundos.resize(s + 1, Acumulado::default());
        }
        &mut self.segundos[s]
    }

    /// Registra um Present. Timestamps fora de ordem ou anteriores a `t0_us` são
    /// ignorados; após erro, nada mais entra (R-M-07).
    pub fn registrar_present(&mut self, t_us: u64) {
        if self.erro.is_some() || t_us < self.t0_us {
            return;
        }
        self.presents += 1;
        let Some(anterior) = self.ultimo_present_us else {
            self.ultimo_present_us = Some(t_us);
            return;
        };
        if t_us <= anterior {
            return;
        }
        self.ultimo_present_us = Some(t_us);
        let ft = u32::try_from(t_us - anterior).unwrap_or(u32::MAX);
        let mediana = self.movel.mediana(t_us);
        let engasgo = eh_engasgo(ft, mediana);
        self.movel.inserir(t_us, ft);
        self.frametimes.push(ft);
        self.histograma[faixa_histograma(ft)] += 1;
        if engasgo {
            self.engasgos += 1;
            self.tempo_engasgado_us += u64::from(ft);
        }
        let seg = self.segundo(t_us);
        seg.quadros += 1;
        seg.soma_ft_us += u64::from(ft);
        seg.max_ft_us = seg.max_ft_us.max(ft);
        if engasgo {
            seg.engasgos += 1;
        }
    }

    /// Leitura da GPU (1 Hz); a última do segundo vale.
    pub fn registrar_gpu(&mut self, t_us: u64, leitura: LeituraGpu) {
        if t_us < self.t0_us {
            return;
        }
        self.segundo(t_us).gpu = Some(leitura);
    }

    /// Soma eventos perdidos atribuídos a esta partida.
    pub fn registrar_perdidos(&mut self, perdidos: u64) {
        self.perdidos += perdidos;
        if self.erro.is_none() && self.perda_pct() > PERDA_MAXIMA_PCT {
            self.erro = Some(CodigoErroMedicao::EventosPerdidos);
        }
    }

    /// Encerra a medição da partida por erro; não há nova tentativa (R-M-07).
    pub fn marcar_erro(&mut self, codigo: CodigoErroMedicao) {
        self.erro.get_or_insert(codigo);
    }

    pub fn perda_pct(&self) -> f32 {
        let total = self.presents + self.perdidos;
        if total == 0 {
            0.0
        } else {
            (self.perdidos as f64 * 100.0 / total as f64) as f32
        }
    }

    /// Fecha a partida em `t_fim_us`. Descarta < 60 s ou sem Presents (R-M-04).
    pub fn finalizar(self, t_fim_us: u64) -> Result<Partida, Descarte> {
        let duracao_s =
            u32::try_from(t_fim_us.saturating_sub(self.t0_us) / 1_000_000).unwrap_or(u32::MAX);
        if self.presents == 0 {
            return Err(Descarte::SemPresents);
        }
        if duracao_s < DURACAO_MINIMA_S {
            return Err(Descarte::Curta);
        }
        let mut ordenados = self.frametimes.clone();
        ordenados.sort_unstable();
        let soma_ft: u64 = ordenados.iter().map(|&f| u64::from(f)).sum();
        let quadros_total = ordenados.len() as u64;
        let fps_medio = if soma_ft == 0 {
            0.0
        } else {
            (quadros_total as f64 * 1_000_000.0 / soma_ft as f64) as f32
        };

        let n_seg = (duracao_s as usize).min(AMOSTRAS_MAX);
        let mut amostras = Vec::with_capacity(n_seg);
        let mut temp_max: Option<u8> = None;
        let (mut soma_uso, mut n_uso) = (0u32, 0u32);
        for s in 0..n_seg {
            let a = self.segundos.get(s).copied().unwrap_or_default();
            let gpu = a.gpu.unwrap_or_default();
            if let Some(t) = gpu.temp_c {
                temp_max = Some(temp_max.map_or(t, |m| m.max(t)));
            }
            if let Some(u) = gpu.uso_pct {
                soma_uso += u32::from(u);
                n_uso += 1;
            }
            amostras.push(AmostraSegundo {
                t_rel_s: s as u32,
                quadros: u16::try_from(a.quadros).unwrap_or(u16::MAX),
                frametime_medio_us: if a.quadros == 0 {
                    0
                } else {
                    (a.soma_ft_us / u64::from(a.quadros)) as u32
                },
                frametime_max_us: a.max_ft_us,
                engasgos: u8::try_from(a.engasgos).unwrap_or(u8::MAX),
                gpu_uso_pct: gpu.uso_pct,
                gpu_temp_c: gpu.temp_c,
            });
        }

        let resumo = ResumoPartida {
            id: self.meta.id,
            jogo_id: self.meta.jogo_id.clone(),
            executavel: self.meta.executavel.clone(),
            inicio_unix: self.meta.inicio_unix,
            duracao_s,
            quadros_total,
            fps_medio,
            fps_mediana: mediana_ordenados(&ordenados).map_or(0.0, fps_de),
            fps_1pct_low: fps_low_ordenados(&ordenados, 0.01),
            fps_01pct_low: fps_low_ordenados(&ordenados, 0.001),
            engasgos: self.engasgos,
            tempo_engasgado_ms: u32::try_from(self.tempo_engasgado_us / 1000).unwrap_or(u32::MAX),
            histograma: self.histograma.to_vec(),
            gpu_temp_max_c: temp_max,
            gpu_uso_medio_pct: (n_uso > 0).then(|| (soma_uso / n_uso) as u8),
            perda_eventos_pct: self.perda_pct().clamp(0.0, 100.0),
            encerrada_por_erro: self.erro,
            versao_formato: VERSAO_FORMATO,
        };
        Ok(Partida { resumo, amostras })
    }
}

/// Veredito exibido (§7.4): "Estável" com zero engasgos; senão "N engasgos".
pub fn veredito(resumo: &ResumoPartida) -> String {
    match resumo.engasgos {
        0 => "Estável".into(),
        1 => "1 engasgo".into(),
        n => format!("{n} engasgos"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> MetaPartida {
        MetaPartida {
            id: 7,
            jogo_id: "cs2".into(),
            executavel: "cs2.exe".into(),
            inicio_unix: 1_700_000_000,
        }
    }

    /// Presents a intervalo fixo de `ft_us` por `dur_s` segundos.
    fn constante(ag: &mut Agregador, t0: u64, ft_us: u64, dur_s: u64) -> u64 {
        let mut t = t0;
        while t < t0 + dur_s * 1_000_000 {
            ag.registrar_present(t);
            t += ft_us;
        }
        t
    }

    #[test]
    fn regra_de_engasgo_exige_fator_e_piso() {
        // 2,5x a mediana mas abaixo do piso (FPS alto): não é engasgo.
        assert!(!eh_engasgo(10_000, Some(2_000)));
        // Acima do piso mas abaixo de 2,5x: não é.
        assert!(!eh_engasgo(30_000, Some(16_000)));
        // Acima dos dois: é.
        assert!(eh_engasgo(50_000, Some(16_667)));
        assert!(!eh_engasgo(50_000, None));
    }

    #[test]
    fn engasgo_em_sequencia_sintetica() {
        let mut ag = Agregador::new(meta(), 0);
        let mut t = constante(&mut ag, 0, 16_667, 30) - 16_667;
        // Um pico de 80 ms e outro de 20 ms (abaixo do piso).
        t += 80_000;
        ag.registrar_present(t);
        t += 20_000;
        ag.registrar_present(t);
        constante(&mut ag, t + 16_667, 16_667, 40);
        let p = ag.finalizar(71_000_000).unwrap();
        assert_eq!(p.resumo.engasgos, 1);
        assert_eq!(p.resumo.tempo_engasgado_ms, 80);
        assert_eq!(
            p.amostras
                .iter()
                .map(|a| u32::from(a.engasgos))
                .sum::<u32>(),
            1
        );
        assert_eq!(veredito(&p.resumo), "1 engasgo");
    }

    #[test]
    fn um_por_cento_low_pelo_percentil() {
        // 990 quadros de 10 ms e 10 de 50 ms: o percentil 99 é 10 ms (rank 990).
        let mut v = vec![10_000u32; 990];
        v.extend([50_000; 10]);
        assert_eq!(fps_low_percentil(&v, 0.01), 100.0);
        // Com 11 ruins, o rank 990 cai em 50 ms.
        v[0] = 50_000;
        assert_eq!(fps_low_percentil(&v, 0.01), 20.0);
        assert_eq!(fps_low_percentil(&[], 0.01), 0.0);
    }

    #[test]
    fn agregacao_por_segundo_e_resumo() {
        let mut ag = Agregador::new(meta(), 1_000);
        ag.registrar_gpu(
            1_000 + 500_000,
            LeituraGpu {
                uso_pct: Some(90),
                temp_c: Some(70),
            },
        );
        ag.registrar_gpu(
            1_000 + 1_500_000,
            LeituraGpu {
                uso_pct: Some(80),
                temp_c: Some(75),
            },
        );
        constante(&mut ag, 1_000, 10_000, 61);
        let p = ag.finalizar(1_000 + 61_000_000).unwrap();
        let r = &p.resumo;
        assert_eq!(r.duracao_s, 61);
        assert_eq!(p.amostras.len(), 61);
        assert_eq!(r.quadros_total, 6_099);
        assert!((r.fps_medio - 100.0).abs() < 0.01);
        assert_eq!(r.fps_mediana, 100.0);
        assert_eq!(r.fps_1pct_low, 100.0);
        assert_eq!(r.engasgos, 0);
        assert_eq!(r.gpu_temp_max_c, Some(75));
        assert_eq!(r.gpu_uso_medio_pct, Some(85));
        assert_eq!(r.histograma.iter().sum::<u32>(), 6_099);
        assert_eq!(r.histograma[faixa_histograma(10_000)], 6_099);
        // O primeiro segundo tem 99 frametimes (o primeiro Present não tem anterior).
        assert_eq!(p.amostras[0].quadros, 99);
        assert_eq!(p.amostras[1].quadros, 100);
        assert_eq!(p.amostras[1].frametime_medio_us, 10_000);
        assert_eq!(p.amostras[1].gpu_temp_c, Some(75));
        assert_eq!(p.amostras[2].gpu_temp_c, None);
        assert!(r.validar().is_ok());
        assert!(p.validar().is_ok());
        assert_eq!(veredito(r), "Estável");
    }

    #[test]
    fn partida_curta_ou_sem_presents_e_descartada() {
        let mut ag = Agregador::new(meta(), 0);
        constante(&mut ag, 0, 10_000, 59);
        assert_eq!(ag.finalizar(59_999_999).unwrap_err(), Descarte::Curta);
        let ag = Agregador::new(meta(), 0);
        assert_eq!(
            ag.finalizar(600_000_000).unwrap_err(),
            Descarte::SemPresents
        );
    }

    #[test]
    fn perda_acima_do_limite_encerra_por_erro() {
        let mut ag = Agregador::new(meta(), 0);
        let t = constante(&mut ag, 0, 10_000, 61);
        ag.registrar_perdidos(1_000);
        assert_eq!(ag.erro(), Some(CodigoErroMedicao::EventosPerdidos));
        ag.registrar_present(t + 10_000);
        let p = ag.finalizar(62_000_000).unwrap();
        assert_eq!(p.resumo.quadros_total, 6_099);
        assert_eq!(
            p.resumo.encerrada_por_erro,
            Some(CodigoErroMedicao::EventosPerdidos)
        );
        assert!(p.resumo.perda_eventos_pct > PERDA_MAXIMA_PCT);
    }

    #[test]
    fn histograma_logaritmico_nos_limites() {
        assert_eq!(faixa_histograma(0), 0);
        assert_eq!(faixa_histograma(500), 0);
        assert_eq!(faixa_histograma(499_999), FAIXAS_HISTOGRAMA - 1);
        assert_eq!(faixa_histograma(u32::MAX), FAIXAS_HISTOGRAMA - 1);
        assert!(faixa_histograma(16_667) < faixa_histograma(33_333));
    }

    #[test]
    fn mediana_movel_expira_a_janela() {
        let mut m = MedianaMovel::new(2_000_000);
        m.inserir(0, 100);
        m.inserir(1_000_000, 300);
        assert_eq!(m.mediana(1_500_000), Some(200));
        assert_eq!(m.mediana(2_500_000), Some(300));
        assert_eq!(m.mediana(10_000_000), None);
    }
}
