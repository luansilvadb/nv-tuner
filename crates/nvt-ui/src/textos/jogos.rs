//! Tela Jogos (§5.4, R-UI-12): nome, um status e detalhe ao clicar.

use crate::modelo::StatusJogo;

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
#[cfg(test)]
pub const TODOS: &[&str] = &[
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
pub const ROTULO_PERFIL: &str = "Perfil do driver";

// Lista e busca -------------------------------------------------------------
pub const BUSCAR_PLACEHOLDER: &str = "Buscar jogo instalado…";
pub const BUSCAR_ROTULO: &str = "Buscar jogo instalado";
pub const BOTAO_OTIMIZAR: &str = "Otimizar sistema";
pub const NENHUM_NA_BUSCA: &str = "Nenhum jogo corresponde à busca.";
pub const CHIP_ATIVO: &str = "Ativo";
pub const CHIP_PENDENTE: &str = "Pendente";

/// "Jogos detectados (12)".
pub fn jogos_detectados(total: usize) -> String {
    format!("Jogos detectados ({total})")
}

/// "Jogos encontrados (3 de 12)".
pub fn jogos_encontrados(achados: usize, total: usize) -> String {
    format!("Jogos encontrados ({achados} de {total})")
}

// Preferências globais ------------------------------------------------------
pub const PREFS_TITULO: &str = "Preferências globais";
pub const PREFS_SUBTITULO: &str = "Configurações padrão para os jogos detectados";
pub const BOTAO_RESTAURAR_PADROES: &str = "Restaurar padrões";
pub const ROTULO_PERFIL_DESEMPENHO: &str = "Perfil de desempenho";
pub const QUALIDADE_DESEMPENHO: &str = "Desempenho máximo";
pub const QUALIDADE_EQUILIBRADO: &str = "Equilibrado";
pub const QUALIDADE_FIDELIDADE: &str = "Alta fidelidade";
pub const ROTULO_SINCRONIZACAO: &str = "Sincronização e latência";
pub const NOME_REFLEX: &str = "NVIDIA Reflex";
pub const SUPORTE_GPU: &str = "Suporte da GPU";
pub const REFLEX_DISPONIVEL: &str = "Disponível";
pub const REFLEX_INDISPONIVEL: &str = "Indisponível";
pub const REFLEX_DESCONHECIDO: &str = "Desconhecido";
pub const SINCRONISMO_VERTICAL: &str = "Sincronismo vertical";
pub const MODO_VSYNC: &str = "V-Sync";
pub const MODO_GSYNC: &str = "G-SYNC";
pub const ROTULO_LIMITE_FPS: &str = "Limite de FPS";
pub const FPS_SEM_LIMITE: &str = "Sem limite";
pub const FPS_OUTRO: &str = "Outro";

/// "165 FPS".
pub fn fps(valor: u32) -> String {
    format!("{valor} FPS")
}

/// Status da linha com a API gráfica: "Otimizado · DirectX 12".
pub fn status_com_api(status: &str, api: &str) -> String {
    format!("{status} · {api}")
}
