//! Tela Jogos (§5.4, R-UI-12): nome, um status e detalhe ao clicar.

use crate::modelo::StatusJogo;

pub const TITULO: &str = "Jogos";
pub const BOTAO_PROCURAR: &str = "Procurar jogos de novo";
pub const NENHUM_JOGO: &str = "Não encontramos jogos neste PC.";
pub const STATUS_OTIMIZADO: &str = "Otimizado";
pub const AJUSTES_WINDOWS_PAUSADOS: &str = "Ajustes do Windows pausados.";
pub const RETOMAR_AJUSTES_WINDOWS: &str = "Reinicie o PC para retomar os ajustes.";
pub const STATUS_AGUARDANDO: &str = "Aguardando o jogo fechar";
pub const STATUS_NADA: &str = "Nada a ajustar";
pub const DETALHE_FEITO: &str = "O que o nv-tuner fez";
pub const DETALHE_FACA: &str = "Faça no jogo";
pub const DETALHE_NADA_FEITO: &str = "Nada foi mudado neste jogo.";
pub const DETALHE_NADA_A_FAZER: &str = "Não há nada para você fazer no jogo.";
pub const BOTAO_DETALHES_TECNICOS: &str = "Ver detalhes técnicos";

/// Todas as frases fixas da tela, para os testes de linguagem.
pub const TODOS: &[&str] = &[
    TITULO,
    BOTAO_PROCURAR,
    NENHUM_JOGO,
    STATUS_OTIMIZADO,
    STATUS_AGUARDANDO,
    STATUS_NADA,
    DETALHE_FEITO,
    DETALHE_FACA,
    DETALHE_NADA_FEITO,
    DETALHE_NADA_A_FAZER,
    BOTAO_DETALHES_TECNICOS,
];

/// "Encontramos 4 jogos neste PC."
pub fn encontrados(n: usize) -> String {
    match n {
        0 => NENHUM_JOGO.to_owned(),
        1 => "Encontramos 1 jogo neste PC.".to_owned(),
        n => format!("Encontramos {n} jogos neste PC."),
    }
}

/// "Faça 2 ajustes no jogo".
pub fn faca_ajustes(n: usize) -> String {
    match n {
        1 => "Faça 1 ajuste no jogo".to_owned(),
        n => format!("Faça {n} ajustes no jogo"),
    }
}

/// Texto do status do modelo: mapeamento único (R-UI-12).
pub fn status(s: StatusJogo) -> String {
    match s {
        StatusJogo::Otimizado => STATUS_OTIMIZADO.to_owned(),
        StatusJogo::FacaAjustes { quantidade } => faca_ajustes(quantidade.max(1)),
        StatusJogo::AguardandoFechar => STATUS_AGUARDANDO.to_owned(),
        StatusJogo::NadaAAjustar => STATUS_NADA.to_owned(),
    }
}

/// Passo numerado de "Faça no jogo": "1. No menu Vídeo, ligue ..."
pub fn passo(numero: usize, texto: &str) -> String {
    format!("{numero}. {}", texto.trim())
}

pub const BOTAO_OCULTAR_TECNICOS: &str = "Ocultar detalhes técnicos";
pub const AJUSTOU_ARQUIVO: &str = "Ajustou o arquivo de configuração do jogo.";
pub const DICA_CLIQUE: &str = "Clique em um jogo para ver o que foi feito.";
pub const ROTULO_PERFIL: &str = "Perfil do driver";
