//! Motor do nv-tuner.
//!
//! Liga a detecção (`nvt-drs` + `nvt-detect`) ao avaliador (`nvt-core`) e ao
//! driver, com rede de segurança:
//!
//! 1. [`Engine::planejar`] — capacidades da GPU, jogos cruzados com o DRS pelo
//!    exe principal, plano do avaliador e [`Diff`] contra o estado atual
//!    (somente leitura).
//! 2. [`Engine::aplicar`] — com confirmação explícita: snapshot original antes
//!    da primeira mudança, criação de perfis, gravação via lista permitida,
//!    releitura e histórico.
//! 3. [`Engine::verificar_deriva`] / [`Engine::tratar_deriva`] — reset em
//!    massa reaplica; mudança pontual vira escolha travada.
//! 4. [`Engine::plano_restauracao`] / [`Engine::restaurar`] — volta ao
//!    snapshot original.
//!
//! O acesso ao driver passa pelo trait [`DriverStore`]: [`DrsReal`] (NVAPI) e
//! [`DriverFalso`] (memória, para testes). Detalhes em `docs/engine.md`.

mod aviso;
mod deteccao;
mod engine;
mod erro;
mod estado;
mod falso;
mod store;

pub use aviso::{Aviso, CodigoAviso, NivelAviso};
pub use deteccao::{JogoEntrada, capacidades, jogo_de_traits, jogos_da_maquina};
pub use engine::{
    AcaoReversao, Aplicacao, ClasseDeriva, Deriva, Diff, Divergencia, Engine, JogoIgnorado,
    JogoResolvido, LIMIAR_RESET_MIN, LIMIAR_RESET_PCT, MAX_EXE, MAX_NOME_PERFIL, MotivoReset,
    MudancaPlanejada, NOME_PERFIL_BASE, OrigemPerfil, Restauracao, Reversao, TratamentoDeriva,
    classificar_deriva, nome_perfil,
};
pub use erro::{ErroEngine, Resultado};
pub use estado::{
    Armazem, Esperado, Estado, LIMITE_HISTORICO_PADRAO, MAX_TRAVADAS, OrigemRegistro, PerfilCriado,
    RegistroHistorico, Travada, VERSAO_ESTADO, ValorAntes, agora_iso, mesmo_alvo,
    mesmo_nome_perfil,
};
pub use falso::DriverFalso;
pub use store::{DriverStore, DrsReal, PerfilEncontrado};
