//! Boas-vindas: tela única da primeira abertura (§5.2).

pub const TITULO: &str = "Bem-vindo ao nv-tuner";
pub const PARAGRAFO_AJUSTA: &str = "O nv-tuner ajusta o seu PC para jogos sozinho.";
pub const PARAGRAFO_JOGO_ABERTO: &str = "Ele não mexe em nada enquanto um jogo está aberto.";
pub const PARAGRAFO_DESFAZER: &str = "Você pode desfazer tudo a qualquer momento.";
pub const BOTAO_COMECAR: &str = "Começar";

/// Todas as frases da tela, para os testes de linguagem.
pub const TODOS: &[&str] = &[
    TITULO,
    PARAGRAFO_AJUSTA,
    PARAGRAFO_JOGO_ABERTO,
    PARAGRAFO_DESFAZER,
    BOTAO_COMECAR,
];
