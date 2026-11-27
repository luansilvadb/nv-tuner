//! Linha de estado única no topo do conteúdo (§5.1, R-UI-05).
//!
//! Uma frase por estado, sem ação. Códigos de falha ficam no Avançado.

use crate::modelo::LinhaEstado;

pub const PROTOCOLO_INCOMPATIVEL: &str =
    "O nv-tuner foi atualizado. Feche esta janela e abra de novo.";
pub const SERVICO_INICIANDO: &str = "Iniciando o nv-tuner… Mostrando só informações.";
pub const SERVICO_SEM_RESPOSTA: &str = "O nv-tuner não está respondendo. Mostrando só informações.";
pub const SERVICO_AUSENTE: &str = "O nv-tuner não está ativo neste PC. Mostrando só informações.";
pub const ATUALIZACAO_CONCLUIDA: &str = "O nv-tuner foi atualizado.";
pub const ATUALIZACAO_PENDENTE: &str =
    "Uma atualização do nv-tuner será instalada quando nenhum jogo estiver aberto.";
pub const AGENTE_AUSENTE: &str =
    "Parte dos ajustes do Windows está pausada até você entrar de novo no Windows.";
/// Usado quando o nome do jogo aberto não é conhecido.
pub const JOGO_ABERTO_SEM_NOME: &str = "Um jogo está aberto. As mudanças esperam o jogo fechar.";

/// "<Nome do jogo> está aberto. As mudanças esperam o jogo fechar."
pub fn jogo_aberto(nome: &str) -> String {
    let nome = nome.trim();
    if nome.is_empty() {
        JOGO_ABERTO_SEM_NOME.to_owned()
    } else {
        format!("{nome} está aberto. As mudanças esperam o jogo fechar.")
    }
}

/// Texto da linha de estado do modelo: mapeamento único (R-UI-05).
pub fn texto(linha: &LinhaEstado) -> String {
    match linha {
        LinhaEstado::ProtocoloIncompativel => PROTOCOLO_INCOMPATIVEL.to_owned(),
        LinhaEstado::ServicoIniciando => SERVICO_INICIANDO.to_owned(),
        LinhaEstado::ServicoSemResposta => SERVICO_SEM_RESPOSTA.to_owned(),
        LinhaEstado::ServicoAusente => SERVICO_AUSENTE.to_owned(),
        LinhaEstado::AtualizacaoConcluida => ATUALIZACAO_CONCLUIDA.to_owned(),
        LinhaEstado::JogoAberto { nome } => jogo_aberto(nome.as_deref().unwrap_or("")),
        LinhaEstado::AtualizacaoPendente => ATUALIZACAO_PENDENTE.to_owned(),
        LinhaEstado::AgenteAusente => AGENTE_AUSENTE.to_owned(),
    }
}
