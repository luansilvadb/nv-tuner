//! Leituras de saúde do PC, somente leitura (`docs/spec/interface-e-saude.md` §4.3, §7.2, §7.3).
//!
//! Os traits [`LeitorInicio`], [`LeitorSinaisMaquina`] e [`LeitorSinaisSessao`] só têm
//! métodos de leitura: não existe caminho de escrita nesta camada. A fachada real
//! ([`SaudeWindows`]) lê registro, pastas Inicializar, Agendador de Tarefas (COM em
//! processo, sem executar nada) e contadores do sistema. Nenhum processo é criado.
//!
//! Divisão: o agente (sessão) usa [`ler_inicio_sessao`] e [`ler_sinais_sessao`]; o
//! serviço (máquina) usa [`ler_inicio_maquina`] e [`ler_sinais_maquina`].

mod inicio;
mod real;
mod sinais;

pub use inicio::{
    Aprovacao, ArquivoPasta, ChaveRun, EntradaInicio, LeitorInicio, LeituraInicio, PastaInicio,
    TarefaBruta, estado_aprovacao, executavel_de, ler_inicio_maquina, ler_inicio_sessao,
};
pub use real::SaudeWindows;
pub use sinais::{
    CPU_ATENCAO_PCT, ESPACO_MIN_BYTES, ESPACO_MIN_PCT, LeitorSinaisMaquina, LeitorSinaisSessao,
    LeituraDword, MEMORIA_ATENCAO_PCT, PLANO_EQUILIBRADO, ParametrosSinais, TemposCpu,
    ler_sinais_maquina, ler_sinais_sessao, sinal_cpu, sinal_driver, sinal_espaco, sinal_memoria,
    sinal_plano, sinal_storage_sense,
};
