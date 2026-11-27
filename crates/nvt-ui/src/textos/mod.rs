//! Todos os textos exibidos pela interface, em português do Brasil.
//!
//! Centralizados aqui para facilitar uma tradução futura: o resto do crate
//! não contém frases de interface, só referências a este módulo.
//!
//! As telas novas têm um submódulo cada; as constantes da raiz atendem às
//! telas legadas, que o Avançado reaproveita.

// As telas novas usam estes módulos a partir do lote L2; até lá, parte deles
// (e constantes legadas que a estrutura nova deixou de usar) fica sem uso.
#![allow(dead_code)]

pub mod avancado;
pub mod boas_vindas;
pub mod estado;
pub mod inicio;
pub mod jogos;
#[cfg(feature = "medicao-partidas")]
pub mod partidas;
pub mod saude;
#[cfg(test)]
mod testes;

use nvt_ipc::{
    AcaoAdiada, AcaoHistorico, AcaoReversao, Adiamento, Aviso, CodigoErro, ImpactoVisual,
    MotivoNaoAplicada, NivelAviso, OrigemPerfil, Qualidade, ResumoJogo, Sincronismo, StatusMudanca,
    codigo_aviso,
};

pub const TITULO_JANELA: &str = "nv-tuner";
pub const TITULO_JANELA_DEMONSTRACAO: &str = "nv-tuner (demonstração)";
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
pub const CARREGANDO: &str = "Carregando…";
pub const SO_COM_SERVICO: &str = "Disponível só com o nv-tuner ativo.";

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
pub const CAP_REFRESH: &str = "Taxa de atualização";
pub const CAP_VRR_POSSIVEL: &str = "VRR (G-SYNC) suportado";
pub const CAP_VRR_ATIVO: &str = "VRR (G-SYNC) ativo";
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
pub const COL_APLICADO: &str = "O que o app fez";
pub const ANTICHEAT_KERNEL: &str = "kernel";
pub const NENHUM_AJUSTE: &str = "Nenhum ajuste específico";
pub const EXPERIMENTAL_CURTO: &str = "Experimental: aguardando confirmação";
pub const DETALHE_APLICADAS: &str = "Regras aplicadas no perfil do jogo";
pub const DETALHE_NAO_APLICADAS: &str = "Regras não aplicadas";
pub const DETALHE_HERDA: &str = "Herdado do perfil global";
pub const DETALHE_NENHUMA: &str = "nenhuma";
pub const DETALHE_EXPANDIR: &str = "Clique na linha para ver as regras e os motivos.";
pub const SEM_JOGOS: &str = "Nenhum jogo detectado.";
pub const PERFIL_DRIVER: &str = "perfil da NVIDIA";
pub const PERFIL_USUARIO: &str = "perfil existente";
pub const PERFIL_APP: &str = "perfil criado pelo app";
pub const PERFIL_A_CRIAR: &str = "sem perfil (será criado se preciso)";
pub const SELO_COMPETITIVO: &str = "competitivo";
pub const SELO_ABERTO: &str = "aberto";
pub const DETALHE_FACA_NO_JOGO: &str = "Faça no jogo";
pub const FACA_NO_JOGO_EXPLICACAO: &str =
    "O app não mexe nas opções do jogo: ajuste estes itens no menu do próprio jogo.";

// Mudanças ----------------------------------------------------------------
pub const SEM_MUDANCAS: &str = "Nenhuma mudança no plano.";
pub const SECAO_RESPEITADAS: &str = "Respeitadas por escolha sua";
pub const SEM_RESPEITADAS: &str = "Nenhuma setting travada.";
pub const BOTAO_DESTRAVAR: &str = "Destravar";
pub const ESTAVEL: &str = "estável";
pub const TELA_WINDOWS: &str = "Windows";
pub const CAMADA_DRIVER: &str = "Driver";
pub const SECAO_WINDOWS_FEITO: &str = "O que o app fez";
pub const SECAO_EXPERIMENTAIS: &str = "Experimentais";
pub const SECAO_DIAGNOSTICO: &str = "Diagnóstico";
pub const SEM_AJUSTES_USUARIO: &str = "Nenhum ajuste do Windows para mostrar.";
pub const SEM_EXPERIMENTAIS: &str = "Nenhuma regra experimental disponível.";
pub const SEM_DIAGNOSTICOS: &str = "Nenhum diagnóstico disponível.";
pub const BOTAO_ACEITAR: &str = "Aceitar";
pub const BOTAO_RETIRAR: &str = "Retirar";
pub const BOTAO_REVERTER: &str = "Reverter";
pub const LINK_OFICIAL: &str = "Documentação oficial";
pub const EXPERIMENTAL_ACEITA: &str = "experimental aceita por você";
pub const EXPERIMENTAIS_DRIVER: &str = "O efeito no driver vale para todos os usuários da máquina. Só valem os aceites de quem está na sessão de console ativa.";
pub const ACEITE_DRIVER_INATIVO: &str = "Seu aceite fica inativo fora da sessão de console ativa.";
pub const AGENTE_ATIVO: &str = "Ajustes do seu usuário: ativos";
pub const AGENTE_AUSENTE: &str =
    "Ajustes do seu usuário: pausados até você entrar de novo no Windows";
pub const AGENTE_INDISPONIVEL: &str = "Ajustes do seu usuário: indisponíveis sem o nv-tuner ativo";
pub const RESTAURACAO_USUARIO_PENDENTE: &str = "Restauração pendente · conclui automaticamente";
pub const DRIVER_RESTAURADO: &str = "Restauração do driver concluída.";
pub const OK_ACEITE: &str = "Preferência experimental atualizada.";
pub const OK_REVERSAO_USUARIO: &str = "Reversão do ajuste do Windows solicitada.";
pub const DESTINO_REVERTER_INDISPONIVEL: &str = "O destino deste ajuste está indisponível.";
pub const RESTAURACAO_SOLICITADA: &str = "Restauração solicitada ao serviço.";

pub fn camada(camada: nvt_ipc::Camada) -> &'static str {
    match camada {
        nvt_ipc::Camada::Driver => CAMADA_DRIVER,
        nvt_ipc::Camada::Usuario => TELA_WINDOWS,
    }
}

pub fn experimental(estado: nvt_ipc::EstadoExperimental) -> &'static str {
    match estado {
        nvt_ipc::EstadoExperimental::Estavel => ESTAVEL,
        nvt_ipc::EstadoExperimental::Aceitavel => EXPERIMENTAL,
        nvt_ipc::EstadoExperimental::Aceita => EXPERIMENTAL_ACEITA,
    }
}

pub fn estado_diagnostico(d: &nvt_ipc::Diagnostico) -> String {
    use nvt_ipc::EstadoDiagnostico::*;
    let valor = match &d.estado {
        Conhecido { valor } => valor.as_str(),
        Indisponivel => return "Leitura indisponível".into(),
        Orientacao => return "Orientação".into(),
    };
    match (d.codigo.as_str(), valor) {
        ("game_mode_desligado" | "vrr_windows", "0") => "Desligado".into(),
        ("game_mode_desligado" | "vrr_windows", "1") => "Ligado".into(),
        ("hags", "1") => "Desligado".into(),
        ("hags", "2") => "Ligado".into(),
        ("secure_boot" | "iommu" | "msi_gpu", "0") => "Desativado".into(),
        ("secure_boot" | "iommu" | "msi_gpu", "1") => "Ativado".into(),
        ("fso_por_jogo", _) => match valor.split_once(',') {
            Some(("0", exe)) => format!("Padrão do Windows · {exe}"),
            Some(("1", exe)) => format!("Desativadas · {exe}"),
            _ => valor.into(),
        },
        _ => valor.into(),
    }
}

pub fn mostrar_orientacao_diagnostico(d: &nvt_ipc::Diagnostico) -> bool {
    d.codigo != "game_mode_desligado"
        || matches!(&d.estado, nvt_ipc::EstadoDiagnostico::Conhecido { valor } if valor == "0")
}

pub fn diagnostico(codigo: &str) -> Option<(&'static str, &'static str, &'static str)> {
    const SEGURANCA_FACEIT: &str = "https://support.faceit.com/hc/en-us/articles/23117181142556-Windows-Security-Requirements-FAQ";
    Some(match codigo {
        "game_mode_desligado" => (
            "Modo de Jogo",
            "Considere religá-lo. O Modo de Jogo é necessário para o gerenciamento dos Ryzen X3D com dois CCDs.",
            "https://support.xbox.com/pt-BR/help/games-apps/game-setup-and-play/use-game-mode-gaming-on-pc",
        ),
        "hags" => (
            "Agendamento de GPU com aceleração de hardware",
            "Mantenha o padrão do Windows.",
            "https://devblogs.microsoft.com/directx/hardware-accelerated-gpu-scheduling/",
        ),
        "vrr_windows" => (
            "Taxa de atualização variável (VRR) do Windows",
            "Se o VRR estiver ativo, a opção em Configurações > Sistema > Tela > Gráficos pode ajudar jogos DirectX 11 em tela cheia sem suporte nativo.",
            "https://devblogs.microsoft.com/directx/os-variable-refresh-rate/",
        ),
        "drr" => (
            "Taxa de atualização dinâmica (DRR)",
            "Considere desligá-la se ela limitar a taxa de atualização de jogos sem VRR.",
            "https://support.microsoft.com/en-us/windows/hardware/display-graphics/change-the-refresh-rate-on-your-monitor-in-windows",
        ),
        "mpo" => (
            "Sobreposição multiplano (MPO)",
            "Consulte o suporte da NVIDIA se houver cintilação ou engasgos em aplicativos de desktop.",
            "https://nvidia.custhelp.com/app/answers/detail/a_id/5157",
        ),
        "fso_por_jogo" => (
            "Otimizações de tela inteira por jogo",
            "A leitura mostra os ajustes de compatibilidade dos jogos detectados.",
            "https://devblogs.microsoft.com/directx/demystifying-full-screen-optimizations/",
        ),
        "plano_energia" => (
            "Plano e modo de energia",
            "O estado mostrado é o que está em uso no Windows.",
            "https://support.microsoft.com/en-us/windows/experience/change-the-power-mode-for-your-windows-pc",
        ),
        "cpu_x3d_dual_ccd" => (
            "Ryzen X3D com dois CCDs",
            "O Windows usa o Modo de Jogo e o driver de chipset da AMD para direcionar os jogos ao CCD adequado.",
            "https://www.amd.com/en/resources/support-articles/release-notes/RN-RYZEN-CHIPSET-5-08-02-027.html",
        ),
        "msi_gpu" => (
            "Interrupções sinalizadas por mensagem (MSI) da GPU",
            "O valor indica a configuração do dispositivo.",
            "https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/enabling-message-signaled-interrupts-in-the-registry",
        ),
        "vbs_hvci" => (
            "Segurança baseada em virtualização e integridade de memória",
            "Vanguard ou FACEIT podem exigir a integridade de memória.",
            SEGURANCA_FACEIT,
        ),
        "tpm" => (
            "Módulo de Plataforma Confiável (TPM)",
            "Vanguard ou FACEIT podem exigir TPM 2.0.",
            SEGURANCA_FACEIT,
        ),
        "secure_boot" => (
            "Inicialização Segura",
            "Vanguard ou FACEIT podem exigir a Inicialização Segura.",
            SEGURANCA_FACEIT,
        ),
        "iommu" => (
            "IOMMU e proteção contra acesso direto à memória",
            "Verifique os requisitos do FACEIT e do Vanguard On-Demand.",
            SEGURANCA_FACEIT,
        ),
        "windows_11" => (
            "Versão do Windows",
            "Confira os requisitos atuais do FACEIT para a versão do Windows.",
            SEGURANCA_FACEIT,
        ),
        "useplatformclock" => (
            "Relógio de plataforma (useplatformclock)",
            "A Microsoft reserva essa opção do BCD para depuração.",
            "https://learn.microsoft.com/en-us/windows-hardware/drivers/devtest/bcdedit--set",
        ),
        "polling_mouse" => (
            "Taxa de consulta do mouse",
            "Considere 1.000 Hz ou mais no software do mouse, se o dispositivo oferecer essa opção.",
            "https://dl.razer.com/master-guides/Mamba/MambaOMG-ENG.pdf",
        ),
        _ => return None,
    })
}

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
pub const RESTAURAR_EXPLICACAO: &str = "Devolve os valores originais dos ajustes gerenciados no driver \
     e no Windows. Se os ajustes do seu usuário ficarem pendentes, o agente retoma a restauração \
     no próximo contato com o serviço. A aplicação automática fica desligada até você pedir \
     “Aplicar agora”.";
pub const BOTAO_RESTAURAR: &str = "Restaurar original…";
pub const CONFIRMAR_TITULO: &str = "Restaurar os valores originais?";
pub const CONFIRMAR_TEXTO: &str = "Os ajustes gerenciados no driver e no seu usuário do Windows serão restaurados. Deseja continuar?";
pub const CONFIRMAR_OK: &str = "Restaurar";
pub const CONFIRMAR_CANCELAR: &str = "Cancelar";

// Resultados das ações ----------------------------------------------------
pub const OK_APLICADO: &str = "Plano aplicado e conferido no driver.";
pub const OK_RESTAURADO: &str = "Valores originais restaurados.";
pub const OK_PREFS: &str = "Preferências salvas.";
pub const OK_DESTRAVADO: &str = "Setting devolvida ao automático.";
pub const OK_REESCANEADO: &str = "Jogos reescaneados.";
pub const ADIAMENTO_TITULO: &str = "Jogo aberto";
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
    match a.codigo.as_str() {
        codigo_aviso::SNAPSHOT_ORIGINAL_PERDIDO => SNAPSHOT_ORIGINAL_PERDIDO,
        codigo_aviso::GSYNC_SEM_VRR => GSYNC_SEM_VRR,
        codigo_aviso::VRR_DESLIGADO_NO_PAINEL => VRR_DESLIGADO_NO_PAINEL,
        _ => &a.mensagem,
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

/// Valor de condição legível (`"true"` → “sim”, `"ampere"` → “Ampere”…).
fn valor_condicao(v: &str) -> String {
    v.split(", ")
        .map(|x| match x {
            "true" => "sim".to_owned(),
            "false" => "não".to_owned(),
            "turing" | "ampere" | "ada" | "blackwell" => arquitetura(x),
            "desconhecida" => DESCONHECIDO.to_owned(),
            "dx9" | "dx11" | "dx12" | "vulkan" | "opengl" => api_grafica(x).to_owned(),
            outro => outro.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ou ")
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

/// Motivo completo (lista expandida de um jogo).
pub fn motivo_longo(m: &MotivoNaoAplicada) -> String {
    match m {
        MotivoNaoAplicada::JogoSemDlss => "O jogo não tem DLSS (nenhum nvngx_dlss.dll \
            encontrado), então não há preset do DLSS para ajustar."
            .into(),
        MotivoNaoAplicada::ArquiteturaNaoPrecisa { arquitetura: a } => match a.as_str() {
            "ada" | "blackwell" => format!(
                "GPU {}: o modelo padrão do DLSS já roda com FP8, forçar outro preset não \
                 ganharia desempenho.",
                arquitetura(a)
            ),
            _ => format!(
                "GPU {}: esta arquitetura não precisa deste ajuste.",
                arquitetura(a)
            ),
        },
        MotivoNaoAplicada::SemTensorCores => "A GPU não tem Tensor Cores (GTX 16), então não \
            roda DLSS."
            .into(),
        MotivoNaoAplicada::Experimental => "Experimental: aguardando confirmação por medição \
            nesta arquitetura. Aparece só como sugestão em Mudanças; não é aplicada."
            .into(),
        MotivoNaoAplicada::BloqueadaPeloTetoDeQualidade { teto } => format!(
            "Bloqueada pelo teto de qualidade “{}”: a regra muda a imagem além do que esse \
             teto permite (ajuste em Preferências).",
            nome_qualidade(*teto)
        ),
        MotivoNaoAplicada::EscolhaDoUsuario { setting } => format!(
            "Você mudou “{}” manualmente; o app respeita a sua escolha (destrave em Mudanças \
             para voltar ao automático).",
            setting_legivel(setting)
        ),
        MotivoNaoAplicada::ConflitoComRegraMaisEspecifica { regra } => {
            match regra.strip_prefix("preferencia:") {
                Some(p) => format!("A sua preferência “{p}” prevalece nesta setting."),
                None => format!(
                    "Outra regra mais específica (“{regra}”) define a mesma setting e \
                     prevaleceu."
                ),
            }
        }
        MotivoNaoAplicada::FatoDesconhecido { fato } => format!(
            "Depende de “{}”, que a detecção não conseguiu determinar neste jogo.",
            nome_condicao(fato)
        ),
        MotivoNaoAplicada::CondicaoNaoAtendida {
            condicao,
            esperado,
            encontrado,
        } => format!(
            "Condição “{}” não atendida: a regra pede {}; aqui é {}.",
            nome_condicao(condicao),
            valor_condicao(esperado),
            valor_condicao(encontrado)
        ),
        MotivoNaoAplicada::VrrInativo => "Só vale com o G-SYNC (VRR) ativo, e ele está \
            desligado ou o monitor não tem VRR. Ative o G-SYNC em Painel de Controle NVIDIA > \
            Configurar G-SYNC ou escolha outro sincronismo em Preferências."
            .into(),
        MotivoNaoAplicada::JogoComReflex => "Esta regra é para jogos sem NVIDIA Reflex. Este \
            jogo tem Reflex: ligue-o nas opções do jogo, que ele já controla a fila de quadros \
            e a latência."
            .into(),
        MotivoNaoAplicada::ConfigPolitica { .. }
        | MotivoNaoAplicada::JanelaFechadaExigida { .. }
        | MotivoNaoAplicada::ArquivoNaNuvem
        | MotivoNaoAplicada::ArquivoSomenteLeitura
        | MotivoNaoAplicada::FormatoInesperado
        | MotivoNaoAplicada::ArquivoAlteradoPeloJogo
        | MotivoNaoAplicada::ContaAmbigua
        | MotivoNaoAplicada::ArquivoAusente
        | MotivoNaoAplicada::ReparseRecusado => {
            crate::arquivo_jogo::motivo_longo_jogo(m).unwrap_or_default()
        }
    }
}

/// Peso de um motivo na escolha do texto curto (menor = mais relevante).
fn relevancia(m: &MotivoNaoAplicada) -> u8 {
    match m {
        MotivoNaoAplicada::Experimental => 0,
        MotivoNaoAplicada::EscolhaDoUsuario { .. } => 1,
        MotivoNaoAplicada::BloqueadaPeloTetoDeQualidade { .. } => 2,
        MotivoNaoAplicada::SemTensorCores => 3,
        MotivoNaoAplicada::ArquiteturaNaoPrecisa { .. } => 4,
        MotivoNaoAplicada::JogoSemDlss => 5,
        MotivoNaoAplicada::FatoDesconhecido { .. } => 6,
        MotivoNaoAplicada::ConflitoComRegraMaisEspecifica { .. } => 7,
        MotivoNaoAplicada::VrrInativo | MotivoNaoAplicada::JogoComReflex => 7,
        MotivoNaoAplicada::CondicaoNaoAtendida { .. } => 8,
        _ => 8,
    }
}

/// Texto curto da coluna “O que o app fez”: as regras aplicadas, ou por que
/// nada específico foi feito.
pub fn resumo_curto(r: &ResumoJogo) -> String {
    if !r.aplicadas.is_empty() {
        return r
            .aplicadas
            .iter()
            .map(|a| nome_regra(&a.regra, &a.descricao))
            .collect::<Vec<_>>()
            .join(" + ");
    }
    match r
        .nao_aplicadas
        .iter()
        .map(|n| &n.motivo)
        .min_by_key(|m| relevancia(m))
    {
        None => NENHUM_AJUSTE.into(),
        Some(MotivoNaoAplicada::Experimental) => EXPERIMENTAL_CURTO.into(),
        Some(m) => format!("{NENHUM_AJUSTE} — {}", motivo_curto(m)),
    }
}

/// “herda N ajustes globais” (vazio se não herda nada).
pub fn herda_globais(n: usize) -> String {
    match n {
        0 => String::new(),
        1 => "herda 1 ajuste global".into(),
        n => format!("herda {n} ajustes globais"),
    }
}

/// Início da lista de herdados na linha expandida (“Herda N ajustes globais”).
pub fn herda_globais_lista(n: usize) -> String {
    let s = herda_globais(n);
    match s.strip_prefix('h') {
        Some(resto) => format!("H{resto}"),
        None => s,
    }
}

// Fase 1: refresh, VRR, adiamento e orientações -----------------------------

/// Taxa de atualização em Hz arredondados (`239_757` mHz → “240 Hz”).
pub fn refresh(mhz: Option<u32>) -> String {
    match mhz {
        Some(m) if m > 0 => format!("{} Hz", (u64::from(m) + 500) / 1000),
        _ => DESCONHECIDO.into(),
    }
}

/// Valor de uma capacidade que pode ser desconhecida.
pub fn sim_nao_desconhecido(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => SIM,
        Some(false) => NAO,
        None => DESCONHECIDO,
    }
}

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

/// Linha curta na coluna “O que o app fez” (ex.: “Faça no jogo: 3 itens”).
pub fn faca_no_jogo_resumo(n: usize) -> String {
    match n {
        0 => String::new(),
        1 => format!("{DETALHE_FACA_NO_JOGO}: 1 item"),
        n => format!("{DETALHE_FACA_NO_JOGO}: {n} itens"),
    }
}

// ---------------------------------------------------------------------------
// Atualização automática (docs/spec/atualizacao.md U-42, U-47, U-48)
// ---------------------------------------------------------------------------

pub const SECAO_ATUALIZACAO: &str = "Atualização do nv-tuner";
pub const BOTAO_OK: &str = "OK";
pub const REINICIO_PENDENTE: &str = "Reinicie o Windows quando puder para concluir a atualização.";
pub const ATUALIZADO_REABRA_TITULO: &str = "O nv-tuner foi atualizado";
pub const ATUALIZADO_REABRA: &str =
    "O nv-tuner foi atualizado. Feche esta janela e abra de novo para usar a versão nova.";

/// Banner exibido uma vez depois de uma atualização (U-47).
pub fn atualizado_para(versao: &str) -> String {
    format!("Atualizado para a v{versao}.")
}

/// Tempo decorrido em linguagem simples: “agora há pouco”, “há 5 min”…
pub fn ha(quando_unix: u64, agora_unix: u64) -> String {
    let s = agora_unix.saturating_sub(quando_unix);
    match s {
        0..60 => "agora há pouco".into(),
        60..3600 => format!("há {} min", s / 60),
        3600..86_400 => format!("há {} h", s / 3600),
        _ => match s / 86_400 {
            1 => "há 1 dia".into(),
            d => format!("há {d} dias"),
        },
    }
}

/// Tempo até um instante futuro: “em instantes”, “em 5 min”…
pub fn em(quando_unix: u64, agora_unix: u64) -> String {
    let s = quando_unix.saturating_sub(agora_unix);
    match s {
        0..60 => "em instantes".into(),
        60..3600 => format!("em {} min", s.div_ceil(60)),
        3600..86_400 => format!("em {} h", s / 3600),
        _ => match s / 86_400 {
            1 => "em 1 dia".into(),
            d => format!("em {d} dias"),
        },
    }
}

/// “; nova tentativa em …” quando há próxima verificação agendada.
fn nova_tentativa(proxima: Option<u64>, agora: u64) -> String {
    proxima.map_or_else(String::new, |p| {
        format!("; nova tentativa {}", em(p, agora))
    })
}

/// Primeira linha de estado: versão e verificações (U-48).
pub fn linha_versao_atualizacao(e: &nvt_ipc::EstadoAtualizacao, agora: u64) -> String {
    let versao = if e.versao_instalada.is_empty() {
        DESCONHECIDO.to_owned()
    } else {
        e.versao_instalada.clone()
    };
    let mut s = format!("Versão {versao}");
    match e.ultima_verificacao_unix {
        Some(u) => s.push_str(&format!(" · última verificação {}", ha(u, agora))),
        None => s.push_str(" · ainda não verificou atualizações"),
    }
    if let Some(p) = e.proxima_verificacao_unix {
        s.push_str(&format!(" · próxima {}", em(p, agora)));
    }
    s
}

/// Por que a atualização encontrada ainda não foi instalada.
pub fn pendencia_atualizacao(p: &nvt_ipc::AtualizacaoPendente) -> String {
    use nvt_ipc::MotivoPendente as M;
    let v = &p.versao;
    match p.motivo {
        M::JogoAberto => format!("Atualização v{v} pendente: jogo aberto."),
        M::ListaIndisponivel => {
            format!("Atualização v{v} pendente: não foi possível conferir se há jogo aberto.")
        }
        M::OutraInstalacao => {
            format!("Atualização v{v} pendente: aguardando outra instalação do Windows.")
        }
        M::Baixando => format!("Baixando a atualização v{v}…"),
        M::Instalando => format!("Instalando a atualização v{v}…"),
    }
}

/// Falha de instalação, com o código do instalador.
pub fn falha_atualizacao(
    f: &nvt_ipc::FalhaAtualizacao,
    proxima: Option<u64>,
    agora: u64,
) -> String {
    if f.abandonada {
        format!(
            "Falha ao instalar v{} (código {}) após {} tentativas; o nv-tuner não tentará \
             de novo esta versão e aguarda a próxima.",
            f.versao, f.codigo, f.tentativas
        )
    } else {
        format!(
            "Falha ao instalar v{} (código {}){}.",
            f.versao,
            f.codigo,
            nova_tentativa(proxima, agora)
        )
    }
}

/// Erro da última verificação, em linguagem simples.
pub fn erro_verificacao(e: nvt_ipc::ErroVerificacao, proxima: Option<u64>, agora: u64) -> String {
    use nvt_ipc::ErroVerificacao as E;
    let base = match e {
        E::SemConexao => "Sem conexão com o GitHub".to_owned(),
        E::RecusadoHttp(s) => format!("O GitHub recusou a verificação (HTTP {s})"),
        E::Assinatura => {
            "A atualização encontrada não tem assinatura válida e foi descartada".to_owned()
        }
        E::Manifesto => "O aviso de nova versão publicado está inválido e foi ignorado".to_owned(),
        E::Disco => "Não foi possível gravar a atualização no disco".to_owned(),
    };
    format!("{base}{}.", nova_tentativa(proxima, agora))
}

/// Todas as linhas de estado da atualização para a Visão geral (U-48).
/// Nenhuma oferece desligar, adiar ou voltar de versão (U-49).
pub fn linhas_atualizacao(e: &nvt_ipc::EstadoAtualizacao, agora: u64) -> Vec<String> {
    let mut v = vec![linha_versao_atualizacao(e, agora)];
    if e.ultima_atualizacao
        .as_ref()
        .is_some_and(|u| u.reinicio_pendente)
    {
        v.push(REINICIO_PENDENTE.to_owned());
    }
    if let Some(p) = &e.pendente {
        v.push(pendencia_atualizacao(p));
    }
    if let Some(f) = &e.falha {
        v.push(falha_atualizacao(f, e.proxima_verificacao_unix, agora));
    }
    if let Some(er) = e.erro_verificacao {
        v.push(erro_verificacao(er, e.proxima_verificacao_unix, agora));
    }
    v
}
