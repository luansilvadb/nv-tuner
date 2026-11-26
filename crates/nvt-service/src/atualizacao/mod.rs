//! Atualização automática no serviço (spec `docs/spec/atualizacao.md` §9.2).

// Dependência já declarada para o lote 2; evita o aviso de dependência sem uso.
use nvt_atualizacao as _;

/// Cliente WinHTTP: sessão, proxy automático, redirecionamento manual, limites e cancelamento.
pub(crate) mod http;

/// Pasta protegida, arquivo `.parcial`, hash e renomeação pelo handle, limpeza.
pub(crate) mod arquivo;

/// Único ponto de criação de processos (msiexec e agente na sessão do usuário).
pub(crate) mod lancador;
