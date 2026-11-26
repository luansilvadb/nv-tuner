//! Avançado (§5.5): rótulos dos grupos. Siglas podem aparecer aqui, com o
//! nome por extenso na primeira ocorrência.

pub const TITULO: &str = "Avançado";
pub const INTRODUCAO: &str =
    "Detalhes técnicos e controles finos. Nada aqui é preciso no dia a dia.";

pub const GRUPO_SISTEMA: &str = "Sistema e recursos";
pub const GRUPO_SISTEMA_DESC: &str =
    "Placa de vídeo (GPU), memória de vídeo (VRAM), driver e sincronismo adaptável (VRR).";
pub const GRUPO_MUDANCAS: &str = "Mudanças por perfil";
pub const GRUPO_MUDANCAS_DESC: &str = "Cada ajuste no perfil do driver, com a regra que o pediu.";
pub const GRUPO_SUGESTOES: &str = "Sugestões em teste";
pub const GRUPO_SUGESTOES_DESC: &str =
    "Ajustes opcionais (experimentais). Só valem se você aceitar.";
pub const GRUPO_WINDOWS: &str = "Windows";
pub const GRUPO_WINDOWS_DESC: &str =
    "O que o nv-tuner mudou no Windows. O agente é a parte que roda na sua sessão.";
pub const GRUPO_JOGOS: &str = "Jogos (técnico)";
pub const GRUPO_JOGOS_DESC: &str =
    "Interface gráfica (API), DLSS, anticheat e arquivo de configuração de cada jogo.";
pub const GRUPO_PREFERENCIAS: &str = "Preferências";
pub const GRUPO_PREFERENCIAS_DESC: &str =
    "Sincronismo, qualidade visual e limite de quadros por segundo (FPS).";
pub const RECOMENDADO_MONITOR: &str = "Recomendado para seu monitor";
pub const GRUPO_INICIALIZACAO: &str = "Inicialização";
pub const GRUPO_INICIALIZACAO_DESC: &str = "Programas que começam junto com o Windows, por origem.";
pub const BOTAO_MARCAR_VISTO: &str = "Marcar como visto";
pub const GRUPO_MANUTENCAO: &str = "Manutenção";
pub const GRUPO_MANUTENCAO_DESC: &str = "Última limpeza de arquivos temporários.";
pub const GRUPO_ATUALIZACAO: &str = "Atualização";
pub const BOTAO_VERIFICAR_AGORA: &str = "Verificar agora";
pub const GRUPO_HISTORICO: &str = "Histórico técnico";
pub const GRUPO_HISTORICO_DESC: &str = "Registro de cada ação do nv-tuner no driver e no Windows.";
pub const GRUPO_DESFAZER: &str = "Desfazer tudo";
pub const GRUPO_DESFAZER_DESC: &str = "Devolve o driver e o Windows ao que eram antes do nv-tuner.";
pub const GRUPO_METRICAS: &str = "Métricas detalhadas";
pub const GRUPO_METRICAS_DESC: &str =
    "Por partida: piores momentos (1% low), tempo de cada quadro, uso e temperatura.";
pub const BOTAO_APAGAR_PARTIDAS: &str = "Apagar todas as partidas";

// Termos técnicos explicados ----------------------------------------------
pub const EXPLICA_DERIVA: &str = "Deriva: um ajuste foi mudado por fora depois de aplicado.";
pub const EXPLICA_TRAVADA: &str =
    "Escolha sua (travada): você mudou este ajuste e o nv-tuner o mantém.";
pub const EXPLICA_DESTRAVAR: &str = "Destravar devolve o ajuste ao automático.";
pub const EXPLICA_REBAR: &str =
    "Resizable BAR (ReBAR): o processador acessa toda a memória da placa de vídeo.";
pub const EXPLICA_NVAPI: &str = "NVAPI: a interface oficial da NVIDIA para ler e mudar o driver.";

/// Todos os rótulos, para os testes de emoji e tamanho.
pub const TODOS: &[&str] = &[
    TITULO,
    INTRODUCAO,
    GRUPO_SISTEMA,
    GRUPO_SISTEMA_DESC,
    GRUPO_MUDANCAS,
    GRUPO_MUDANCAS_DESC,
    GRUPO_SUGESTOES,
    GRUPO_SUGESTOES_DESC,
    GRUPO_WINDOWS,
    GRUPO_WINDOWS_DESC,
    GRUPO_JOGOS,
    GRUPO_JOGOS_DESC,
    GRUPO_PREFERENCIAS,
    GRUPO_PREFERENCIAS_DESC,
    RECOMENDADO_MONITOR,
    GRUPO_INICIALIZACAO,
    GRUPO_INICIALIZACAO_DESC,
    BOTAO_MARCAR_VISTO,
    GRUPO_MANUTENCAO,
    GRUPO_MANUTENCAO_DESC,
    GRUPO_ATUALIZACAO,
    BOTAO_VERIFICAR_AGORA,
    GRUPO_HISTORICO,
    GRUPO_HISTORICO_DESC,
    GRUPO_DESFAZER,
    GRUPO_DESFAZER_DESC,
    GRUPO_METRICAS,
    GRUPO_METRICAS_DESC,
    BOTAO_APAGAR_PARTIDAS,
    EXPLICA_DERIVA,
    EXPLICA_TRAVADA,
    EXPLICA_DESTRAVAR,
    EXPLICA_REBAR,
    EXPLICA_NVAPI,
];
