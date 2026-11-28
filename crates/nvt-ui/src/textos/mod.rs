//! Todos os textos exibidos pela interface, em português do Brasil.
//!
//! Centralizados aqui para facilitar uma tradução futura: o resto do crate
//! não contém frases de interface, só referências a este módulo.
//!
//! As telas têm um submódulo cada; a raiz guarda os textos compartilhados.

pub mod boas_vindas;
pub mod estado;
pub mod inicio;
pub mod jogos;
pub mod nav;
#[cfg(feature = "medicao-partidas")]
pub mod partidas;
pub mod saude;
#[cfg(test)]
mod testes;

use nvt_ipc::{
    AcaoAdiada, AcaoReversao, Adiamento, CodigoErro, MotivoNaoAplicada, OrigemPerfil, Qualidade,
    StatusMudanca,
};

pub const DIAS_SEMANA: [&str; 7] = [
    "domingo",
    "segunda-feira",
    "terça-feira",
    "quarta-feira",
    "quinta-feira",
    "sexta-feira",
    "sábado",
];
pub const MESES: [&str; 12] = [
    "janeiro",
    "fevereiro",
    "março",
    "abril",
    "maio",
    "junho",
    "julho",
    "agosto",
    "setembro",
    "outubro",
    "novembro",
    "dezembro",
];

/// "1 item" ou "N itens".
pub(crate) fn plural(n: u64, um: &str, varios: &str) -> String {
    if n == 1 {
        format!("1 {um}")
    } else {
        format!("{n} {varios}")
    }
}

pub const TITULO_JANELA: &str = "nv-tuner";
pub const TITULO_JANELA_DEMONSTRACAO: &str = "nv-tuner (demonstração)";

// Navegação --------------------------------------------------------------

// Conexão ----------------------------------------------------------------
#[cfg(feature = "medicao-partidas")]
pub const CARREGANDO: &str = "Carregando…";
pub const SO_COM_SERVICO: &str = "Disponível só com o nv-tuner ativo.";

// Gerais ------------------------------------------------------------------
pub const DESCONHECIDO: &str = "desconhecido";
pub const SIM: &str = "Sim";
pub const NAO: &str = "Não";

// Jogos -------------------------------------------------------------------
pub const COL_LAUNCHER: &str = "Launcher";
pub const COL_API: &str = "API";
pub const COL_DLSS: &str = "DLSS";
pub const COL_REFLEX: &str = "Reflex";
pub const COL_ANTICHEAT: &str = "Anticheat";
pub const ANTICHEAT_KERNEL: &str = "kernel";
pub const EXPERIMENTAL_CURTO: &str = "Experimental: aguardando confirmação";
pub const PERFIL_DRIVER: &str = "perfil da NVIDIA";
pub const PERFIL_USUARIO: &str = "perfil existente";
pub const PERFIL_APP: &str = "perfil criado pelo app";
pub const PERFIL_A_CRIAR: &str = "sem perfil (será criado se preciso)";
pub const SELO_COMPETITIVO: &str = "competitivo";

// Mudanças ----------------------------------------------------------------
pub const BOTAO_REVERTER: &str = "Reverter";
pub const RESTAURACAO_USUARIO_PENDENTE: &str = "Restauração pendente · conclui automaticamente";
pub const DRIVER_RESTAURADO: &str = "Restauração do driver concluída.";
pub const RESTAURACAO_SOLICITADA: &str = "Restauração solicitada ao serviço.";

pub const PERFIL_GLOBAL: &str = "Perfil global";

// Preferências ------------------------------------------------------------

// Histórico ---------------------------------------------------------------

// Restaurar ---------------------------------------------------------------

// Resultados das ações ----------------------------------------------------
pub const OK_RESTAURADO: &str = "Valores originais restaurados.";
pub const OK_PREFS: &str = "Preferências salvas.";
pub const OK_REESCANEADO: &str = "Jogos reescaneados.";
pub const ADIAMENTO_TITULO: &str = "Jogo aberto";
pub const EM_ANDAMENTO: &str = "Operação em andamento…";

pub fn nome_qualidade(q: Qualidade) -> &'static str {
    match q {
        Qualidade::Desempenho => "Desempenho",
        Qualidade::Equilibrado => "Equilibrado",
        Qualidade::Qualidade => "Fidelidade",
    }
}

pub fn status_mudanca(s: StatusMudanca) -> &'static str {
    match s {
        StatusMudanca::Pendente => "pendente",
        StatusMudanca::Aplicada => "aplicada",
        StatusMudanca::JaNoValor => "já no valor",
        StatusMudanca::Travada => "escolha sua",
        StatusMudanca::Recusada => "recusada",
        StatusMudanca::Falhou => "falhou",
    }
}

pub fn codigo_erro(c: CodigoErro) -> &'static str {
    match c {
        CodigoErro::VersaoIncompativel => "a versão do serviço é incompatível com esta interface",
        CodigoErro::MensagemGrande => "mensagem grande demais",
        CodigoErro::MensagemInvalida => "mensagem inválida",
        CodigoErro::IdentidadeIndisponivel => "não foi possível identificar seu usuário do Windows",
        CodigoErro::RequisicaoInvalida => "pedido inválido",
        CodigoErro::SettingNaoPermitida => "setting fora da lista permitida",
        CodigoErro::PerfilNaoEncontrado => "perfil não encontrado no driver",
        CodigoErro::DriverIndisponivel => "driver NVIDIA indisponível",
        CodigoErro::Ocupado => "o serviço está ocupado com outra operação",
        CodigoErro::EntradaInvalida => "pedido incoerente com o estado do serviço",
        CodigoErro::Interno => "falha interna do serviço",
        CodigoErro::RecursoAusente => "este recurso ainda não existe nesta versão do serviço",
    }
}

pub fn arquitetura(nome_regra: &str) -> String {
    match nome_regra {
        "turing" => "Turing".into(),
        "ampere" => "Ampere".into(),
        "ada" => "Ada Lovelace".into(),
        "blackwell" => "Blackwell".into(),
        "" => DESCONHECIDO.into(),
        outra => outra.into(),
    }
}

pub fn api_grafica(nome_regra: &str) -> &'static str {
    match nome_regra {
        "dx9" => "DirectX 9",
        "dx11" => "DirectX 11",
        "dx12" => "DirectX 12",
        "vulkan" => "Vulkan",
        "opengl" => "OpenGL",
        _ => "?",
    }
}

pub fn talvez(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => SIM,
        Some(false) => NAO,
        None => "?",
    }
}

pub fn erro_comunicacao(detalhe: &str) -> String {
    format!("Falha ao falar com o serviço: {detalhe}")
}

pub fn erro_servico(codigo: CodigoErro, mensagem: &str) -> String {
    format!(
        "O serviço recusou o pedido ({}): {mensagem}",
        codigo_erro(codigo)
    )
}

pub fn erro_local(detalhe: &str) -> String {
    format!("Não foi possível ler o driver localmente: {detalhe}")
}

pub const RECUSA_VISUALIZACAO: &str =
    "modo de visualização: gravação no driver bloqueada (somente leitura)";

// Versão do protocolo ------------------------------------------------------
pub const INCOMPATIVEL_TITULO: &str = "Interface e serviço de versões diferentes";

/// Serviço com outra versão do protocolo: o que fazer.
pub fn versao_incompativel(recebida: u64, esperada: u32) -> String {
    let quem = if recebida > u64::from(esperada) {
        "o serviço é mais novo que esta interface"
    } else {
        "o serviço é mais antigo que esta interface"
    };
    format!(
        "O serviço em execução fala o protocolo v{recebida} e esta interface, o v{esperada} \
         ({quem}). Atualize o nv-tuner instalando a versão mais recente pelo instalador, que \
         atualiza a interface e o serviço juntos, e reinicie o serviço “nv-tuner (serviço)” \
         se ele não reiniciar sozinho. Até lá, a interface fica no modo de visualização."
    )
}

// Avisos -------------------------------------------------------------------

/// Avisos calculados localmente (modo de visualização).
pub const LOCAL_ESTADO_CORROMPIDO: &str = "O arquivo de estado (estado.json) está corrompido. \
     Ao iniciar, o serviço o guarda à parte e recupera a cópia de segurança.";
pub const LOCAL_ESTADO_RECUPERAVEL: &str =
    "Mostrando a cópia de segurança do estado (estado.anterior.json).";
pub const LOCAL_ESTADO_PERDIDO: &str = "Não há cópia de segurança válida do estado: ao iniciar, \
     o serviço recomeça do zero (preferências, escolhas travadas e histórico anteriores se \
     perdem; o snapshot original é preservado).";
pub const LOCAL_ESTADO_ILEGIVEL: &str = "Não foi possível ler o estado do nv-tuner.";
pub const LOCAL_ESTADO_AUSENTE_RECUPERAVEL: &str = "O arquivo de estado (estado.json) não \
     existe; mostrando a cópia de segurança (estado.anterior.json). Ao iniciar, o serviço \
     recupera o estado a partir dela.";

/// Texto próprio da UI para `snapshot_original_perdido` (substitui a
/// mensagem recebida).
pub const SNAPSHOT_ORIGINAL_PERDIDO: &str = "O snapshot original do driver foi perdido: o \
     diretório de estado do nv-tuner foi posto em quarentena e o snapshot ficou lá. A partir \
     de agora, “Restaurar original” volta à base pós-quarentena — o driver como estava na \
     primeira aplicação depois da quarentena, que pode já conter mudanças do nv-tuner — e não \
     ao estado anterior ao app. Para o original de fábrica, confira a pasta da quarentena ou \
     reinstale o driver.";

/// Texto da UI para cada aviso do motor (correspondência exaustiva: um
/// código novo no motor não compila sem o seu texto aqui).
pub fn texto_aviso_motor(c: nvt_engine::CodigoAviso) -> &'static str {
    use nvt_engine::CodigoAviso as C;
    match c {
        C::EstadoCorrompido => LOCAL_ESTADO_CORROMPIDO,
        C::EstadoRecuperado => LOCAL_ESTADO_RECUPERAVEL,
        C::EstadoReiniciado => LOCAL_ESTADO_PERDIDO,
        C::SnapshotIndisponivel => RESTAURAR_BLOQUEADO,
        C::SnapshotOriginalPerdido => SNAPSHOT_ORIGINAL_PERDIDO,
    }
}

/// Texto próprio da UI para `gsync_sem_vrr` (também ao lado da escolha de
/// sincronismo nas Preferências).
pub const GSYNC_SEM_VRR: &str = "Seu monitor não tem G-SYNC/VRR: escolha “sem sincronismo”. \
     Sem VRR, o G-SYNC vira V-Sync comum, com fila de quadros e mais latência.";
/// Texto próprio da UI para `vrr_desligado_no_painel`.
pub const VRR_DESLIGADO_NO_PAINEL: &str = "Ative o G-SYNC em Painel de Controle NVIDIA > \
     Configurar G-SYNC: o monitor aceita VRR, mas ele está desligado (o app não liga o VRR \
     do monitor).";

pub const RESTAURAR_BLOQUEADO: &str = "Restauração desabilitada";

// Reversões e jogos ignorados ----------------------------------------------

/// Por que uma setting saiu do plano (modo de visualização; o serviço manda
/// a mesma frase).
pub fn motivo_reversao(causa: &str, a: AcaoReversao) -> String {
    let efeito = match a {
        AcaoReversao::GravarOriginal => "volta ao valor original",
        AcaoReversao::Remover => "sai do perfil (volta ao predefinido da NVIDIA, se houver)",
        AcaoReversao::SoDeixarDeVigiar => {
            "o driver já está como no original; só deixa de ser vigiada"
        }
    };
    format!("Saiu do plano porque {causa}: {efeito}.")
}

pub fn causa_regra(id: &str) -> String {
    format!("a regra “{id}” não se aplica mais")
}

pub fn causa_preferencia(preferencia: &str, regra: Option<&str>) -> String {
    match regra {
        None => format!("a preferência “{preferencia}” mudou"),
        Some(r) => format!("a preferência “{preferencia}” (regra “{r}”) mudou"),
    }
}

/// Nome legível de uma setting do catálogo (`regras/settings.toml`).
pub fn nome_setting(setting: &str) -> Option<&'static str> {
    Some(match setting {
        "vsync" => "Sincronização vertical (V-Sync)",
        "vsync_tear_control" => "Controle de tearing do V-Sync",
        "gsync_global" => "G-SYNC: recurso global",
        "gsync_app_mode" => "G-SYNC: modo (tela cheia/janela)",
        "gsync_app_state" => "G-SYNC: estado no aplicativo",
        "low_latency_cpl" => "Modo de baixa latência (painel)",
        "low_latency_enabled" => "Modo de baixa latência",
        "limite_fps" => "Limitador de FPS",
        "platform_boost" => "Platform Boost",
        "energia" => "Gerenciamento de energia",
        "shader_cache" => "Cache de shaders",
        "af_modo" => "Filtragem anisotrópica: modo",
        "af_otimizacao" => "Filtragem anisotrópica: otimização",
        "af_otimizacao_amostras" => "Filtragem anisotrópica: otimização de amostras",
        "aa_modo" => "Antisserrilhamento (MSAA): modo",
        "indicador_vrr" => "Indicador de VRR na tela",
        "indicador_mfaa" => "Indicador de MFAA",
        "indicador_triple_buffering" => "Indicador de buffer triplo",
        "dlss_override" => "DLSS: substituição da DLL",
        "dlss_perfil_preset" => "DLSS: perfil de preset",
        "dlss_preset" => "DLSS: preset forçado",
        _ => return None,
    })
}

/// Nome legível de um valor nomeado do catálogo.
pub fn nome_valor(valor: &str) -> String {
    let fixo = match valor {
        "off" | "force_off" | "forcar_off" => "Desligado",
        "on" | "force_on" => "Ligado",
        "ultra" => "Ultra",
        "fast_sync" => "Fast Sync",
        "aplicacao" => "Controlado pelo aplicativo",
        "meio_refresh" => "Metade da taxa de atualização",
        "terco_refresh" => "Um terço da taxa de atualização",
        "quarto_refresh" => "Um quarto da taxa de atualização",
        "standard" => "Padrão",
        "adaptive" => "Adaptativo",
        "tela_cheia" => "Só tela cheia",
        "tela_cheia_e_janela" => "Tela cheia e janela",
        "permitir" => "Permitir",
        "desabilitar" => "Desabilitado",
        "ulmb" => "ULMB",
        "refresh_fixo" => "Taxa fixa",
        "adaptativo" => "Adaptativo",
        "desempenho_maximo" => "Preferir desempenho máximo",
        "otimizado" => "Otimizado",
        "usuario" => "Definido pelo usuário",
        "sobrescrever" => "Substituir o do aplicativo",
        "melhorar" => "Melhorar o do aplicativo",
        "na" => "Não definido",
        "recomendado" => "Recomendado",
        "personalizado" => "Personalizado",
        _ => "",
    };
    if !fixo.is_empty() {
        return fixo.into();
    }
    // Letras de preset do DLSS (a, b, …, m) e outros nomes curtos.
    if valor.len() == 1 {
        return valor.to_uppercase();
    }
    let mut s = valor.replace('_', " ");
    if let Some(c) = s.get(0..1) {
        let maiuscula = c.to_uppercase();
        s.replace_range(0..1, &maiuscula);
    }
    s
}

// Explicação por jogo ------------------------------------------------------

/// Origem do perfil do driver de um jogo.
pub fn origem_perfil(o: OrigemPerfil) -> &'static str {
    match o {
        OrigemPerfil::Driver => PERFIL_DRIVER,
        OrigemPerfil::Usuario => PERFIL_USUARIO,
        OrigemPerfil::CriadoPeloApp => PERFIL_APP,
        OrigemPerfil::ACriar => PERFIL_A_CRIAR,
    }
}

/// Nome curto de uma regra (para a coluna “O que o app fez”); sem nome
/// próprio, a descrição da regra (ou o id).
pub fn nome_regra(id: &str, descricao: &str) -> String {
    let fixo = match id {
        "dlss-preset-e-turing" => "Preset E do DLSS (Turing)",
        "dlss-preset-e-ampere" => "Preset E do DLSS (Ampere)",
        _ if descricao.is_empty() => id,
        _ => descricao,
    };
    fixo.to_owned()
}

/// Arquitetura em forma curta (“Ada”, não “Ada Lovelace”).
fn arquitetura_curta(nome_regra: &str) -> String {
    match nome_regra {
        "ada" => "Ada".into(),
        outra => arquitetura(outra),
    }
}

/// Nome legível de uma setting (nome simbólico se não houver).
pub fn setting_legivel(setting: &str) -> String {
    nome_setting(setting).map_or_else(|| setting.to_owned(), str::to_owned)
}

/// Nome legível de uma condição das regras (`quando`).
pub fn nome_condicao(condicao: &str) -> String {
    let fixo = match condicao {
        "arquitetura" => "arquitetura da GPU",
        "tem_tensor_cores" => "Tensor Cores",
        "tensor_fp8" => "FP8 nos Tensor Cores",
        "frame_gen" => "geração de quadros",
        "mfg" => "geração múltipla de quadros",
        "rebar_suportado" => "Resizable BAR suportado",
        "rebar_ativo" => "Resizable BAR ativo",
        "vram_mb_min" => "VRAM mínima (MB)",
        "driver_min" => "versão mínima do driver",
        "refresh_conhecido" => "taxa de atualização do monitor",
        "jogo_api" => "API gráfica do jogo",
        "jogo_dlss" => "DLSS no jogo",
        "jogo_dlss_min" => "versão mínima do DLSS",
        "jogo_reflex" => "Reflex no jogo",
        "jogo_anticheat_kernel" => "anticheat de kernel",
        "sincronismo" => "sincronismo",
        "qualidade" => "teto de qualidade",
        "limite_fps_definido" => "limite de FPS",
        "aplicar" => "valor calculado",
        outro => outro,
    };
    fixo.to_owned()
}

/// Motivo em poucas palavras (completa “Nenhum ajuste específico — …”).
pub fn motivo_curto(m: &MotivoNaoAplicada) -> String {
    match m {
        MotivoNaoAplicada::JogoSemDlss => "jogo sem DLSS".into(),
        MotivoNaoAplicada::ArquiteturaNaoPrecisa { arquitetura } => format!(
            "GPU {} roda o DLSS padrão sem custo extra",
            arquitetura_curta(arquitetura)
        ),
        MotivoNaoAplicada::SemTensorCores => "GPU sem Tensor Cores (sem DLSS)".into(),
        MotivoNaoAplicada::Experimental => EXPERIMENTAL_CURTO.into(),
        MotivoNaoAplicada::BloqueadaPeloTetoDeQualidade { teto } => {
            format!(
                "bloqueado pelo teto de qualidade “{}”",
                nome_qualidade(*teto)
            )
        }
        MotivoNaoAplicada::EscolhaDoUsuario { setting } => {
            format!("você travou “{}”", setting_legivel(setting))
        }
        MotivoNaoAplicada::ConflitoComRegraMaisEspecifica { .. } => {
            "outra regra mais específica prevaleceu".into()
        }
        MotivoNaoAplicada::FatoDesconhecido { fato } => {
            format!("não foi possível detectar: {}", nome_condicao(fato))
        }
        MotivoNaoAplicada::CondicaoNaoAtendida { .. } => "condições da regra não atendidas".into(),
        MotivoNaoAplicada::VrrInativo => "G-SYNC (VRR) não está ativo".into(),
        MotivoNaoAplicada::JogoComReflex => "o jogo tem Reflex, que já cuida disso".into(),
        // Motivos de arquivo de jogo (v7, Fase 3).
        MotivoNaoAplicada::ConfigPolitica { .. }
        | MotivoNaoAplicada::JanelaFechadaExigida { .. }
        | MotivoNaoAplicada::ArquivoNaNuvem
        | MotivoNaoAplicada::ArquivoSomenteLeitura
        | MotivoNaoAplicada::FormatoInesperado
        | MotivoNaoAplicada::ArquivoAlteradoPeloJogo
        | MotivoNaoAplicada::ContaAmbigua
        | MotivoNaoAplicada::ArquivoAusente
        | MotivoNaoAplicada::ReparseRecusado => {
            crate::arquivo_jogo::motivo_curto_jogo(m).unwrap_or_default()
        }
    }
}

// Fase 1: refresh, VRR, adiamento e orientações -----------------------------

/// O que acontece quando o jogo fechar, conforme a ação pendente.
pub fn efeito_adiado(a: AcaoAdiada) -> &'static str {
    match a {
        AcaoAdiada::AplicarPlano => "as alterações serão aplicadas quando você fechar o jogo",
        AcaoAdiada::TratarDeriva => {
            "a mudança detectada no driver será tratada quando você fechar o jogo"
        }
        AcaoAdiada::RestaurarOriginal => {
            "os valores originais serão restaurados quando você fechar o jogo"
        }
    }
}

/// Faixa e mensagem quando o serviço não conseguiu ler a lista de processos
/// (`Adiamento::lista_indisponivel`).
pub const ADIAMENTO_SEM_LISTA: &str =
    "Não foi possível verificar se há jogo aberto; as alterações ficam pendentes.";
/// Título da faixa/mensagem de [`ADIAMENTO_SEM_LISTA`].
pub const ADIAMENTO_SEM_LISTA_TITULO: &str = "Alterações pendentes";

/// Título da faixa ou da mensagem de adiamento com o texto `texto`.
pub fn titulo_adiamento(texto: &str) -> &'static str {
    if texto == ADIAMENTO_SEM_LISTA {
        ADIAMENTO_SEM_LISTA_TITULO
    } else {
        ADIAMENTO_TITULO
    }
}

/// Faixa de estado com jogo aberto (ex.: “Jogo aberto (cs2.exe): as
/// alterações serão aplicadas quando você fechar o jogo.”). Com a lista de
/// processos indisponível, [`ADIAMENTO_SEM_LISTA`].
pub fn adiamento(a: &Adiamento) -> String {
    if a.lista_indisponivel {
        return ADIAMENTO_SEM_LISTA.to_owned();
    }
    let efeito = efeito_adiado(a.acao);
    if a.processos.is_empty() {
        format!("Jogo aberto: {efeito}.")
    } else {
        format!("Jogo aberto ({}): {efeito}.", a.processos.join(", "))
    }
}

/// Orientação “faça no jogo” de um código do núcleo
/// (`nvt_core::Orientacao::codigo`); `None` se o código for desconhecido.
pub fn orientacao(codigo: &str) -> Option<&'static str> {
    Some(match codigo {
        "reflex_on" => "Ligue o NVIDIA Reflex em On (Ativado) nas opções de vídeo.",
        "reflex_on_boost" => "Ligue o NVIDIA Reflex em On + Boost (Ativado + Turbo).",
        "reduce_buffering" => {
            "Se preferir não usar o Reflex, ative “Reduzir buffer” (Reduce Buffering)."
        }
        "limitador_do_jogo" => {
            "Limite o FPS no próprio jogo, num valor que a máquina mantenha estável."
        }
        "sem_limite_com_reflex" => {
            "Com o Reflex ligado, deixe o FPS sem limite no jogo: o Reflex já segura a fila."
        }
        "tela_cheia" => "Use tela cheia (não janela nem janela sem bordas).",
        "vsync_jogo_off" => "Desligue o V-Sync nas opções do jogo.",
        "vsync_driver_com_gsync" => {
            "Desligue o V-Sync no jogo: com G-SYNC, o app já liga o V-Sync no driver."
        }
        "fps_max_launch_ea" => {
            "No EA app, em Propriedades > Opções de inicialização avançadas, use +fps_max 0 \
             com o Reflex ligado (ou um teto estável, ex.: +fps_max 200)."
        }
        "fps_max_padrao_baixo" => {
            "O limite padrão é de 120 FPS: aumente-o no menu de vídeo ou use +fps_max nas \
             opções de inicialização da Steam."
        }
        "one_frame_thread_lag_off" => {
            "Desligue “One Frame Thread Lag” no menu de vídeo (menos latência, um pouco menos \
             de FPS)."
        }
        "resolucao_adaptativa_off" => {
            "Desligue a resolução adaptativa (Adaptive Resolution FPS Target = 0)."
        }
        "modo_performance_opcional" => {
            "Opcional: o modo de renderização Performance tem menos latência, mas reduz a \
             qualidade visual."
        }
        "fps_limit_cpu_100" => {
            "Se a CPU fica em 100%, ative o limite de FPS do jogo (FPSLimit), como orienta a \
             Ubisoft."
        }
        _ => return None,
    })
}

/// Texto de um código de orientação; o próprio código se for desconhecido
/// (versão mais nova do serviço), sem quebrar a tela.
pub fn orientacao_ou_codigo(codigo: &str) -> String {
    orientacao(codigo).map_or_else(|| codigo.to_owned(), str::to_owned)
}

// ---------------------------------------------------------------------------
// Atualização automática (U-42, U-47, U-48)
// ---------------------------------------------------------------------------

pub const ATUALIZADO_REABRA_TITULO: &str = "O nv-tuner foi atualizado";
pub const ATUALIZADO_REABRA: &str =
    "O nv-tuner foi atualizado. Feche esta janela e abra de novo para usar a versão nova.";
