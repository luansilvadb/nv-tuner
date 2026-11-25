//! Todos os textos exibidos pela interface, em português do Brasil.
//!
//! Centralizados aqui para facilitar uma tradução futura: o resto do crate
//! não contém frases de interface, só referências a este módulo.

use nvt_ipc::{
    AcaoHistorico, AcaoReversao, Aviso, CodigoErro, ImpactoVisual, NivelAviso, Qualidade,
    Sincronismo, StatusMudanca, codigo_aviso,
};

pub const TITULO_JANELA: &str = "nv-tuner";
pub const SUBTITULO_APP: &str = "Ajuste automático do driver NVIDIA";

// Navegação --------------------------------------------------------------
pub const NAV_GRUPO: &str = "Navegação";
pub const TELA_VISAO_GERAL: &str = "Visão geral";
pub const TELA_JOGOS: &str = "Jogos";
pub const TELA_MUDANCAS: &str = "Mudanças";
pub const TELA_PREFERENCIAS: &str = "Preferências";
pub const TELA_HISTORICO: &str = "Histórico";
pub const TELA_RESTAURAR: &str = "Restaurar original";

// Conexão ----------------------------------------------------------------
pub const CONEXAO_CONECTANDO: &str = "Conectando ao serviço…";
pub const CONEXAO_SERVICO: &str = "Serviço conectado";
pub const CONEXAO_VISUALIZACAO: &str = "Modo de visualização";
pub const AVISO_VISUALIZACAO_TITULO: &str = "Serviço do nv-tuner não está em execução";
pub const AVISO_VISUALIZACAO: &str = "Mostrando o que seria feito, calculado localmente e somente \
     leitura: nada é gravado no driver. Para aplicar, instale o nv-tuner pelo instalador ou \
     inicie o serviço “nv-tuner (serviço)” (nvt-service) em services.msc. A interface reconecta sozinha quando o \
     serviço subir.";
pub const CARREGANDO: &str = "Carregando…";
pub const SO_COM_SERVICO: &str = "Disponível apenas com o serviço em execução.";

// Visão geral -------------------------------------------------------------
pub const SECAO_SISTEMA: &str = "Sistema";
pub const SECAO_CAPACIDADES: &str = "Capacidades relevantes";
pub const SECAO_SERVICO: &str = "Serviço";
pub const SECAO_CONFORMIDADE: &str = "Conformidade";
pub const SECAO_DERIVA: &str = "Alertas de deriva";
pub const ROTULO_GPU: &str = "GPU";
pub const ROTULO_ARQUITETURA: &str = "Arquitetura";
pub const ROTULO_VRAM: &str = "VRAM";
pub const ROTULO_DRIVER: &str = "Driver";
pub const ROTULO_JOGOS: &str = "Jogos detectados";
pub const ROTULO_STATUS: &str = "Status";
pub const ROTULO_VERSAO: &str = "Versão do serviço";
pub const ROTULO_ULTIMA_APLICACAO: &str = "Última aplicação";
pub const CAP_DLSS: &str = "DLSS";
pub const CAP_FG: &str = "Geração de quadros";
pub const CAP_REFLEX: &str = "Reflex";
pub const CAP_REBAR: &str = "Resizable BAR";
pub const CAP_VRR: &str = "VRR (G-SYNC)";
pub const CONF_PENDENTES: &str = "pendentes";
pub const CONF_CONFORMES: &str = "conformes";
pub const CONF_TRAVADAS: &str = "escolhas suas";
pub const CONF_FALHAS: &str = "com falha";
pub const CONF_TUDO_OK: &str = "O driver está conforme o plano.";
pub const SEM_DERIVA: &str = "Nenhuma divergência entre o driver e o que foi aplicado.";
pub const NUNCA: &str = "nunca";
pub const DESCONHECIDO: &str = "desconhecido";
pub const SIM: &str = "Sim";
pub const NAO: &str = "Não";
pub const BOTAO_APLICAR: &str = "Aplicar agora";
pub const BOTAO_REESCANEAR: &str = "Reescanear jogos";
pub const BOTAO_ATUALIZAR: &str = "Atualizar";

// Jogos -------------------------------------------------------------------
pub const COL_JOGO: &str = "Jogo";
pub const COL_LAUNCHER: &str = "Launcher";
pub const COL_API: &str = "API";
pub const COL_DLSS: &str = "DLSS";
pub const COL_REFLEX: &str = "Reflex";
pub const COL_ANTICHEAT: &str = "Anticheat";
pub const COL_APLICADO: &str = "Aplicado pelo app";
pub const ANTICHEAT_KERNEL: &str = "kernel";
pub const NENHUM_APLICADO: &str = "nada aplicado";
pub const SEM_JOGOS: &str = "Nenhum jogo detectado.";
pub const PERFIL_DRIVER: &str = "perfil da NVIDIA";
pub const PERFIL_USUARIO: &str = "perfil existente";
pub const PERFIL_APP: &str = "perfil criado pelo app";
pub const PERFIL_A_CRIAR: &str = "sem perfil (será criado se preciso)";

// Mudanças ----------------------------------------------------------------
pub const SEM_MUDANCAS: &str = "Nenhuma mudança no plano.";
pub const SECAO_RESPEITADAS: &str = "Respeitadas por escolha sua";
pub const SEM_RESPEITADAS: &str = "Nenhuma setting travada.";
pub const BOTAO_DESTRAVAR: &str = "Destravar";
pub const ESTAVEL: &str = "estável";
pub const EXPERIMENTAL: &str = "experimental (não aplicada)";
pub const ROTULO_REGRA: &str = "Regra";
pub const ROTULO_IMPACTO: &str = "Impacto visual";
pub const NAO_DEFINIDO: &str = "não definido";
pub const PERFIL_GLOBAL: &str = "Perfil global";

// Preferências ------------------------------------------------------------
pub const SECAO_SINCRONISMO: &str = "Sincronismo";
pub const SECAO_QUALIDADE: &str = "Qualidade visual";
pub const SECAO_LIMITE_FPS: &str = "Limite de FPS";
pub const LIMITE_FPS_LIGADO: &str = "Limitar FPS no driver";
pub const BOTAO_SALVAR_PREFS: &str = "Salvar preferências";
pub const BOTAO_DESCARTAR: &str = "Descartar alterações";
pub const PREFS_NAO_SALVAS: &str = "Há alterações não salvas.";

// Histórico ---------------------------------------------------------------
pub const SEM_HISTORICO: &str = "Nenhum registro ainda.";

// Restaurar ---------------------------------------------------------------
pub const RESTAURAR_EXPLICACAO: &str = "Devolve ao driver os valores originais de tudo o que o \
     nv-tuner alterou (snapshot feito antes da primeira mudança). Settings que o app nunca \
     tocou não são alteradas. Depois de restaurar, a aplicação automática fica desligada até \
     você pedir “Aplicar agora”.";
pub const BOTAO_RESTAURAR: &str = "Restaurar original…";
pub const CONFIRMAR_TITULO: &str = "Restaurar os valores originais?";
pub const CONFIRMAR_TEXTO: &str =
    "Todas as mudanças feitas pelo nv-tuner serão desfeitas no driver. Deseja continuar?";
pub const CONFIRMAR_OK: &str = "Restaurar";
pub const CONFIRMAR_CANCELAR: &str = "Cancelar";

// Resultados das ações ----------------------------------------------------
pub const OK_APLICADO: &str = "Plano aplicado e conferido no driver.";
pub const OK_RESTAURADO: &str = "Valores originais restaurados.";
pub const OK_PREFS: &str = "Preferências salvas.";
pub const OK_DESTRAVADO: &str = "Setting devolvida ao automático.";
pub const OK_REESCANEADO: &str = "Jogos reescaneados.";
pub const EM_ANDAMENTO: &str = "Operação em andamento…";

/// Nome de uma opção de sincronismo.
pub fn nome_sincronismo(s: Sincronismo) -> &'static str {
    match s {
        Sincronismo::Desligado => "Sem sincronização",
        Sincronismo::GSync => "G-SYNC",
        Sincronismo::VSync => "V-Sync",
    }
}

pub fn explicacao_sincronismo(s: Sincronismo) -> &'static str {
    match s {
        Sincronismo::Desligado => {
            "Menor latência possível; pode haver tearing (rasgo de imagem). Recomendado para \
             jogos competitivos."
        }
        Sincronismo::GSync => {
            "Usa a taxa variável do monitor (VRR) com V-Sync no driver e limite de FPS logo \
             abaixo da taxa do monitor: sem tearing e com latência baixa."
        }
        Sincronismo::VSync => {
            "Sincronização vertical tradicional, sem VRR: sem tearing, mas com mais latência."
        }
    }
}

pub fn nome_qualidade(q: Qualidade) -> &'static str {
    match q {
        Qualidade::Desempenho => "Desempenho",
        Qualidade::Equilibrado => "Equilibrado",
        Qualidade::Qualidade => "Fidelidade",
    }
}

pub fn explicacao_qualidade(q: Qualidade) -> &'static str {
    match q {
        Qualidade::Desempenho => {
            "Aceita ajustes com impacto visual forte quando trazem FPS ou latência melhores."
        }
        Qualidade::Equilibrado => {
            "Aceita só ajustes de impacto visual leve. É o padrão recomendado."
        }
        Qualidade::Qualidade => "Só aplica ajustes sem nenhum impacto visual.",
    }
}

pub fn explicacao_limite_fps() -> &'static str {
    "Opcional. Com G-SYNC, o app já calcula um limite abaixo da taxa do monitor; defina aqui \
     só se quiser um teto fixo (ex.: para reduzir consumo e calor)."
}

pub fn impacto(i: ImpactoVisual) -> &'static str {
    match i {
        ImpactoVisual::Nenhum => "nenhum",
        ImpactoVisual::Baixo => "baixo",
        ImpactoVisual::Medio => "médio",
        ImpactoVisual::Alto => "alto",
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

pub fn acao_historico(a: AcaoHistorico) -> &'static str {
    match a {
        AcaoHistorico::Aplicacao => "Aplicação",
        AcaoHistorico::Restauracao => "Restauração",
        AcaoHistorico::Rollback => "Rollback",
        AcaoHistorico::Travamento => "Travamento",
        AcaoHistorico::Destravamento => "Destravamento",
        AcaoHistorico::Preferencias => "Preferências",
        AcaoHistorico::DerivaDetectada => "Deriva detectada",
    }
}

pub fn codigo_erro(c: CodigoErro) -> &'static str {
    match c {
        CodigoErro::VersaoIncompativel => "a versão do serviço é incompatível com esta interface",
        CodigoErro::MensagemGrande => "mensagem grande demais",
        CodigoErro::MensagemInvalida => "mensagem inválida",
        CodigoErro::RequisicaoInvalida => "pedido inválido",
        CodigoErro::SettingNaoPermitida => "setting fora da lista permitida",
        CodigoErro::PerfilNaoEncontrado => "perfil não encontrado no driver",
        CodigoErro::DriverIndisponivel => "driver NVIDIA indisponível",
        CodigoErro::Ocupado => "o serviço está ocupado com outra operação",
        CodigoErro::Interno => "falha interna do serviço",
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

/// Resumo de um perfil na tela de mudanças (ex.: “2 pendentes · 5 já no valor”).
pub fn resumo_grupo(pendentes: usize, no_valor: usize, outras: usize, sugestoes: usize) -> String {
    let mut partes = Vec::new();
    match pendentes {
        0 => partes.push("nada pendente".to_string()),
        1 => partes.push("1 pendente".into()),
        n => partes.push(format!("{n} pendentes")),
    }
    if no_valor > 0 {
        partes.push(format!("{no_valor} já no valor"));
    }
    if outras > 0 {
        partes.push(format!("{outras} com falha"));
    }
    match sugestoes {
        0 => {}
        1 => partes.push("1 sugestão experimental".into()),
        n => partes.push(format!("{n} sugestões experimentais")),
    }
    partes.join(" · ")
}

pub fn ultima_aplicacao(quando: &str, mudancas: u32, falhas: u32) -> String {
    if falhas == 0 {
        format!("{quando} · {mudancas} mudança(s)")
    } else {
        format!("{quando} · {mudancas} mudança(s), {falhas} falha(s)")
    }
}

pub fn deriva(nome: &str, perfil: &str, esperado: &str, encontrado: Option<&str>) -> String {
    format!(
        "{nome} em “{perfil}”: esperado {esperado}, encontrado {}",
        encontrado.unwrap_or(NAO_DEFINIDO)
    )
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
pub const SECAO_AVISOS: &str = "Avisos";
pub const SEM_AVISOS: &str = "Nenhum aviso.";

pub fn nivel_aviso(n: NivelAviso) -> &'static str {
    match n {
        NivelAviso::Info => "Informação",
        NivelAviso::Atencao => "Atenção",
        NivelAviso::Erro => "Erro",
    }
}

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

/// Frase exibida para um aviso: o texto próprio da UI quando ela tem um que
/// substitui a mensagem recebida; senão, a mensagem do serviço.
pub fn mensagem_aviso(a: &Aviso) -> &str {
    if a.codigo == codigo_aviso::SNAPSHOT_ORIGINAL_PERDIDO {
        SNAPSHOT_ORIGINAL_PERDIDO
    } else {
        &a.mensagem
    }
}

pub const RESTAURAR_BLOQUEADO: &str = "Restauração desabilitada";

// Reversões e jogos ignorados ----------------------------------------------
pub const SECAO_REVERSOES: &str = "Saíram do plano (voltam ao original)";
pub const SEM_REVERSOES: &str = "Nenhuma setting saiu do plano.";
pub const SECAO_IGNORADOS: &str = "Ignorados pelo plano";
pub const IGNORADOS_EXPLICACAO: &str = "Estes jogos ficaram de fora por um problema só deles \
     (nome ou executável inválido, erro no perfil do driver); os demais seguiram normalmente.";

pub fn acao_reversao(a: AcaoReversao, original: Option<&str>) -> String {
    match a {
        AcaoReversao::GravarOriginal => {
            format!("volta a {}", original.unwrap_or(NAO_DEFINIDO))
        }
        AcaoReversao::Remover => "sai do perfil (volta ao predefinido da NVIDIA)".into(),
        AcaoReversao::SoDeixarDeVigiar => {
            "já está como no original; só deixa de ser vigiada".into()
        }
    }
}

pub fn status_reversao(s: StatusMudanca) -> &'static str {
    match s {
        StatusMudanca::Aplicada => "revertida",
        StatusMudanca::Falhou | StatusMudanca::Recusada => "falhou",
        _ => "a reverter",
    }
}

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
