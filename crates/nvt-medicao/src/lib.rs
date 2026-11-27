//! Medição de partidas (R-M), só em build de teste.
//!
//! - [`nucleo`]: agregação pura de Presents (frametime, FPS, engasgo, 1% low, histograma,
//!   amostras por segundo e resumo da partida), testável sem ETW.
//! - [`partida`]: início e fim de partida pela lista de processos e associação PID ->
//!   executável só pelo snapshot de processos (nenhum handle para o jogo).
//! - [`persistencia`]: formato `<id>.bin`, `indice.json`, gravação atômica e retenção.
//! - [`fachada`]: traits das fontes (Presents, GPU, processos) e falsos para teste.
//! - `etw`, `nvml` e `processos` (só com `medicao-partidas`, no Windows): implementações
//!   reais das fachadas.
//!
//! Sem a feature o crate traz só o núcleo puro; nenhuma API do sistema é ligada.

// §8.4, item 2: a feature só compila no build de teste autorizado pelo build.rs.
#[cfg(all(feature = "medicao-partidas", not(nvt_build_de_teste)))]
compile_error!("medicao-partidas só compila em build de teste (NVT_BUILD_DE_TESTE=1, fora de CI)");

pub mod fachada;
pub mod nucleo;
pub mod partida;
pub mod persistencia;

#[cfg(all(feature = "medicao-partidas", windows))]
pub mod etw;
#[cfg(all(feature = "medicao-partidas", windows))]
pub mod nvml;
#[cfg(all(feature = "medicao-partidas", windows))]
pub mod processos;
