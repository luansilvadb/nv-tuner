//! Respostas do serviço: envelope, códigos de erro, estado do sistema e da
//! atualização automática, adiamentos e dados do driver/GPU.

use super::*;

// ---------------------------------------------------------------------------
// Respostas
// ---------------------------------------------------------------------------

/// Resposta enviada pelo serviço à UI.
// Sem `Eq`: `Partida` carrega métricas em `f32`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
// `Estado` é a maior variante, mas a resposta é criada uma vez por pedido e
// serializada em seguida; `Box` só espalharia alocações pelo serviço e pela UI.
#[allow(clippy::large_enum_variant)]
pub enum Resposta {
    Estado(Estado),
    /// Resposta de `ObterPlano` e de `AplicarAgora`.
    Plano(Plano),
    Historico {
        entradas: Vec<EntradaHistorico>,
    },
    /// Resposta de `RestaurarOriginal` (v4).
    Restauracao {
        /// `Some` = jogo aberto: a restauração ficou pendente e roda quando
        /// o jogo fechar (não é erro).
        adiada: Option<Adiamento>,
    },
    /// Resposta de `DefinirPreferencias` e de `DestravarSetting` quando o
    /// pedido foi aceito (gravado no estado), mas a aplicação no driver ficou
    /// pendente pelo portão de gravação (v4). Não é erro; sem adiamento, a
    /// resposta é [`Resposta::Ok`].
    Adiada {
        adiamento: Adiamento,
    },
    PlanoAgente(PlanoAgente),
    /// Resposta de `AplicarRecomendadas` (v8).
    Recomendadas {
        aceitas: Vec<String>,
        adiada: bool,
    },
    /// Resposta de `ObterSaude` (v8).
    Saude(Saude),
    /// Resposta de `ListarPartidas` (v8).
    Partidas {
        resumos: Vec<ResumoPartida>,
    },
    /// Resposta de `ObterPartida` (v8).
    Partida(Partida),
    Encerrar,
    Ok,
    Erro {
        codigo: CodigoErro,
        mensagem: String,
    },
}

impl Resposta {
    /// Atalho para montar uma resposta de erro.
    pub fn erro(codigo: CodigoErro, mensagem: impl Into<String>) -> Self {
        Self::Erro {
            codigo,
            mensagem: mensagem.into(),
        }
    }
}

/// Código de erro estável (a mensagem é só para exibição).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodigoErro {
    /// Versão do protocolo diferente da do serviço.
    VersaoIncompativel,
    /// Quadro acima do limite de tamanho.
    MensagemGrande,
    /// JSON ou envelope inválido, ou tipo de mensagem desconhecido.
    MensagemInvalida,
    /// Campos fora de faixa ou com formato inválido.
    RequisicaoInvalida,
    /// Entrada bem formada mas incoerente com o estado do serviço (v6; por
    /// exemplo, `marcar_atualizacao_vista` com versão que não é a da última
    /// atualização concluída).
    EntradaInvalida,
    IdentidadeIndisponivel,
    /// Setting fora da lista permitida do serviço.
    SettingNaoPermitida,
    /// Perfil inexistente no driver.
    PerfilNaoEncontrado,
    /// NVAPI/driver indisponível.
    DriverIndisponivel,
    /// Outra operação em andamento.
    Ocupado,
    /// Falha interna do serviço.
    Interno,
    /// Recurso ausente neste build ou ainda não implementado (v8).
    RecursoAusente,
}

/// Resumo do estado do serviço e do sistema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Estado {
    pub agente: EstadoAgente,
    pub diagnosticos: Vec<Diagnostico>,
    pub versao_servico: String,
    /// `None` se o driver não pôde ser lido.
    pub driver: Option<Driver>,
    pub gpu: Option<Gpu>,
    pub preferencias: Preferencias,
    pub jogos: u32,
    pub ultima_aplicacao: Option<Aplicacao>,
    pub alertas_deriva: Vec<AlertaDeriva>,
    /// Problemas que o usuário precisa ver (estado recuperado, snapshot
    /// indisponível, erros isolados por perfil no último ciclo, jogos
    /// ignorados, G-SYNC sem VRR). Mais graves primeiro.
    pub avisos: Vec<Aviso>,
    /// Gravação pendente porque há jogo ou anticheat em execução (v4).
    pub adiamento: Option<Adiamento>,
    /// Atualização automática do próprio nv-tuner (v6).
    pub atualizacao: EstadoAtualizacao,
    /// Ajustes automáticos desligados pelo usuário ("Desfazer tudo") até
    /// "Voltar a ajustar" (v8). Persistido pelo serviço.
    #[serde(default)]
    pub autonomia_pausada: bool,
}

/// Estado da atualização automática (v6).
/// DTO próprio do protocolo, independente dos tipos do núcleo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstadoAtualizacao {
    /// Versão do serviço instalado.
    pub versao_instalada: String,
    pub ultima_verificacao_unix: Option<u64>,
    pub proxima_verificacao_unix: Option<u64>,
    pub ultima_atualizacao: Option<AtualizacaoConcluida>,
    /// `ultima_atualizacao` ainda não foi vista pelo SID do chamador.
    pub nao_vista: bool,
    pub pendente: Option<AtualizacaoPendente>,
    pub falha: Option<FalhaAtualizacao>,
    pub erro_verificacao: Option<ErroVerificacao>,
}

/// Última atualização instalada com sucesso.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtualizacaoConcluida {
    pub de: String,
    pub para: String,
    pub quando_unix: u64,
    /// O instalador terminou com 3010 (reinício do Windows pendente).
    pub reinicio_pendente: bool,
}

/// Atualização encontrada mas ainda não instalada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtualizacaoPendente {
    pub versao: String,
    pub motivo: MotivoPendente,
    pub desde_unix: u64,
}

/// Por que a atualização pendente ainda não foi instalada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotivoPendente {
    /// Há jogo ou anticheat em execução.
    JogoAberto,
    /// A lista de processos não pôde ser lida (falha fechada).
    ListaIndisponivel,
    /// Outra instalação do Windows em andamento.
    OutraInstalacao,
    Baixando,
    Instalando,
}

/// Falha de instalação de uma versão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FalhaAtualizacao {
    pub versao: String,
    /// Código de saída do instalador.
    pub codigo: i64,
    pub tentativas: u8,
    /// Sem novas tentativas para esta versão.
    pub abandonada: bool,
    pub quando_unix: u64,
}

/// Erro da última verificação de atualização.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", content = "status", rename_all = "snake_case")]
pub enum ErroVerificacao {
    SemConexao,
    /// Resposta HTTP de recusa, com o status.
    RecusadoHttp(u16),
    Assinatura,
    Manifesto,
    Disco,
}

/// Gravação adiada pelo portão do serviço: há processo vigiado (jogo
/// detectado, processo da base curada ou anticheat que só roda durante o
/// jogo) em execução, ou a lista de processos não pôde ser lida (falha
/// fechada). A ação roda uma vez quando o portão liberar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Adiamento {
    /// Nomes dos processos vigiados em execução (ex.: `"cs2.exe"`). Vazio
    /// com `lista_indisponivel`.
    pub processos: Vec<String>,
    /// Desde quando o portão adia sem interrupção (segundos Unix, UTC).
    pub desde_unix: u64,
    /// Ação pendente.
    pub acao: AcaoAdiada,
    /// `true` = a lista de processos do sistema não pôde ser lida: não se
    /// sabe se há jogo aberto, e a gravação fica pendente por segurança.
    pub lista_indisponivel: bool,
}

/// Ação que ficou pendente. Só uma por vez: a mais recente substitui as
/// anteriores, exceto `RestaurarOriginal`, que vence qualquer outra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcaoAdiada {
    /// Aplicação do plano (automática ou “Aplicar agora”).
    AplicarPlano,
    /// Tratamento de deriva (reaplicação após reset do driver).
    TratarDeriva,
    /// “Restaurar original”.
    RestaurarOriginal,
}

impl Estado {
    /// A restauração está bloqueada (snapshot original ilegível)? Devolve o
    /// aviso que explica o motivo.
    pub fn restauracao_bloqueada(&self) -> Option<&Aviso> {
        self.avisos
            .iter()
            .find(|a| a.codigo == codigo_aviso::SNAPSHOT_INDISPONIVEL)
    }
}

/// Driver NVIDIA instalado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Driver {
    /// Ex.: `"581.57"`.
    pub versao: String,
    /// Ramo do driver (ex.: `"r580_00"`), quando conhecido.
    pub ramo: Option<String>,
}

/// GPU principal e capacidades resumidas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gpu {
    pub nome: String,
    /// Ex.: `"ada"`, `"blackwell"`.
    pub arquitetura: String,
    pub vram_mb: u32,
    pub capacidades: Capacidades,
}

/// Capacidades relevantes para as regras.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capacidades {
    pub dlss: bool,
    pub geracao_quadros: bool,
    pub reflex: bool,
    pub rebar: bool,
    /// Refresh atual do monitor principal em milésimos de Hz (ex.: `239_757`);
    /// `None` = desconhecido (v4).
    pub refresh_mhz: Option<u32>,
    /// VRR do monitor principal (v4; antes um `bool` fixo).
    pub vrr: Vrr,
}

/// Estado do VRR do monitor principal. `None` = desconhecido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vrr {
    /// O monitor e a ligação aceitam VRR.
    pub possivel: Option<bool>,
    /// VRR ligado (G-SYNC ativado no painel).
    pub ativo: Option<bool>,
}

/// Última aplicação de plano.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Aplicacao {
    /// Segundos desde a época Unix (UTC).
    pub quando_unix: u64,
    pub mudancas: u32,
    pub falhas: u32,
}

/// Setting cujo valor real no driver diverge do último valor aplicado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertaDeriva {
    pub perfil: String,
    pub setting: String,
    pub nome_legivel: String,
    pub esperado: String,
    pub encontrado: Option<String>,
    pub detectado_unix: u64,
}
