//! Plano de configuração (jogos, regras, reversões, mudanças) e histórico.

use super::*;

/// Plano de configuração.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plano {
    pub sistema: ResumoSistema,
    pub sugestoes_driver: Vec<SugestaoExperimental>,
    pub gerado_unix: u64,
    pub perfis: Vec<PlanoPerfil>,
    /// Settings que o app gravou e que saíram do plano: voltam ao original.
    pub reversoes: Vec<Reversao>,
    /// Jogos deixados de fora do plano, com o motivo.
    pub ignorados: Vec<JogoIgnorado>,
    /// Jogos do plano com os fatos detectados e o que o app faz em cada um.
    pub jogos: Vec<Jogo>,
    /// Só na resposta de `AplicarAgora` (v4): `Some` = jogo aberto, a
    /// aplicação ficou pendente (não é erro) e as mudanças seguem `pendente`.
    pub adiada: Option<Adiamento>,
}

/// Jogo do plano: fatos detectados, perfil do driver e explicação.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Jogo {
    /// Nome no launcher.
    pub nome: String,
    /// Nome do perfil no driver (existente ou a criar).
    pub perfil: String,
    pub origem_perfil: OrigemPerfil,
    /// Launcher (ex.: `"Steam"`, `"Riot"`); vazio se desconhecido.
    pub launcher: String,
    /// API gráfica (`dx9`, `dx11`, `dx12`, `vulkan`, `opengl`, `desconhecida`).
    pub api: String,
    /// Versão do DLSS; `None` = sem DLSS; `Some("")` = DLSS sem versão legível.
    pub dlss: Option<String>,
    /// `None` = não determinado. Jogo da base curada: o valor da base.
    pub reflex: Option<bool>,
    /// `None` = não determinado. Jogo da base curada: o valor da base.
    pub anticheat_kernel: Option<bool>,
    /// Id do jogo na base curada de competitivos (`regras/jogos.toml`);
    /// `None` = fora da base (v4).
    pub competitivo: Option<String>,
    /// Algum exe do jogo estava em execução na última varredura (v4).
    pub em_execucao: bool,
    /// Códigos de orientação “faça no jogo”, já filtrados pelas preferências
    /// (ex.: `"reflex_on_boost"`); a UI traduz (v4).
    pub orientacoes: Vec<String>,
    pub resumo: ResumoJogo,
    /// Arquivo do jogo: o que o app fez (v7); `None` = jogo sem `[[arquivo]]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_jogo: Option<ConfigJogoUi>,
}

/// De onde vem o perfil do driver de um jogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrigemPerfil {
    /// Perfil predefinido pela NVIDIA.
    Driver,
    /// Perfil que já existia (criado por outra ferramenta ou pelo usuário).
    Usuario,
    /// Perfil criado pelo nv-tuner.
    CriadoPeloApp,
    /// Sem perfil: será criado se houver mudança.
    ACriar,
}

/// O que o plano faz por um jogo.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumoJogo {
    /// Regras aplicadas no perfil do jogo.
    pub aplicadas: Vec<RegraAplicada>,
    /// Regras por jogo não aplicadas (ou com setting suprimida), com motivo.
    pub nao_aplicadas: Vec<RegraNaoAplicada>,
    /// Settings do perfil global que o jogo herda (nomes simbólicos).
    pub herda_do_global: Vec<String>,
}

/// Regra aplicada no perfil do jogo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegraAplicada {
    pub regra: String,
    /// Descrição da regra (pt-BR, do TOML).
    pub descricao: String,
    /// Settings gravadas (nomes simbólicos).
    pub settings: Vec<String>,
}

/// Regra por jogo que não foi aplicada, com o motivo estruturado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegraNaoAplicada {
    pub regra: String,
    /// Descrição da regra (pt-BR, do TOML).
    pub descricao: String,
    pub motivo: MotivoNaoAplicada,
}

/// Por que uma regra não foi aplicada. O texto é gerado pela UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum MotivoNaoAplicada {
    /// O jogo não tem DLSS.
    JogoSemDlss,
    /// A arquitetura da GPU não precisa do ajuste (ex.: `"ada"`).
    ArquiteturaNaoPrecisa { arquitetura: String },
    /// A GPU não tem Tensor Cores.
    SemTensorCores,
    /// Regra experimental: só sugestão, aguardando confirmação por medição.
    Experimental,
    /// Impacto visual acima do teto de qualidade.
    BloqueadaPeloTetoDeQualidade { teto: Qualidade },
    /// O usuário travou a setting.
    EscolhaDoUsuario { setting: String },
    /// Outra regra (ou `preferencia:<nome>`) venceu o conflito.
    ConflitoComRegraMaisEspecifica { regra: String },
    /// Fato que a detecção não determinou (nome da condição, ex.: `"jogo_reflex"`).
    FatoDesconhecido { fato: String },
    /// Condição não atendida, sem motivo específico.
    CondicaoNaoAtendida {
        condicao: String,
        esperado: String,
        encontrado: String,
    },
    /// A regra exige VRR ativo e ele está desligado (`vrr_ativo`; v4).
    VrrInativo,
    /// A regra é para jogo sem Reflex e o jogo tem Reflex (`jogo_reflex`; v4).
    JogoComReflex,
    /// Política do jogo na base curada impede a escrita (`orientar`/`proibido`; v7).
    ConfigPolitica { politica: String },
    /// Processos que precisam estar fechados para a escrita (ex.: `steam.exe`; v7).
    JanelaFechadaExigida { processos: Vec<String> },
    /// Arquivo efetivo sincronizado na nuvem (F3-09; v7).
    ArquivoNaNuvem,
    /// Arquivo marcado como somente leitura (F3-06; v7).
    ArquivoSomenteLeitura,
    /// Codificação ou sintaxe não reconhecida, ou chave ausente (v7).
    FormatoInesperado,
    /// O jogo mudou a chave depois da escrita do app (F3-07; v7).
    ArquivoAlteradoPeloJogo,
    /// Várias contas empatadas (F3-12; v7).
    ContaAmbigua,
    /// Arquivo não encontrado na instância resolvida (v7).
    ArquivoAusente,
    /// Link ou junção no caminho: recusado (v7).
    ReparseRecusado,
}

/// Setting gerida pelo app que o plano atual não quer mais (preferência
/// trocada, teto de qualidade, regra que deixou de casar): volta ao original.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reversao {
    /// Nome do perfil no driver (`"global"` para o perfil base).
    pub perfil: String,
    /// Nome para exibição (ex.: nome do jogo).
    pub nome_perfil: String,
    pub setting: String,
    pub nome_legivel: String,
    /// Valor atual no driver (`None` = não definido).
    pub atual: Option<String>,
    /// Valor que volta (`None` = sai do perfil / predefinido da NVIDIA).
    pub original: Option<String>,
    pub acao: AcaoReversao,
    /// Por que saiu do plano.
    pub motivo: String,
    /// `pendente`, `aplicada` (revertida e relida) ou `falhou`.
    pub status: StatusMudanca,
}

/// O que acontece com uma setting que saiu do plano.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcaoReversao {
    /// Grava o valor original do snapshot.
    GravarOriginal,
    /// Remove a setting do perfil (não existia no original).
    Remover,
    /// O driver já está como no original (ou o original é desconhecido): só
    /// deixa de vigiar.
    SoDeixarDeVigiar,
}

/// Jogo deixado de fora do plano por um erro só dele.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JogoIgnorado {
    pub nome: String,
    /// Nome do arquivo do executável principal.
    pub exe: String,
    pub motivo: String,
}

/// Mudanças de um perfil do driver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanoPerfil {
    /// Nome do perfil no driver (`"global"` para o perfil base).
    pub perfil: String,
    /// Nome para exibição (ex.: nome do jogo).
    pub nome_legivel: String,
    pub mudancas: Vec<Mudanca>,
}

/// Uma alteração de setting, com valores já formatados para exibição.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mudanca {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alvo_tela: Option<String>,
    pub camada: Camada,
    pub experimental: EstadoExperimental,
    pub setting: String,
    pub nome_legivel: String,
    /// `None` = não definida no perfil.
    pub antes: Option<String>,
    pub depois: Option<String>,
    pub motivo: String,
    /// Identificador da regra de origem.
    pub regra: String,
    pub impacto_visual: ImpactoVisual,
    pub status: StatusMudanca,
}

/// Impacto visual estimado de uma mudança.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactoVisual {
    Nenhum,
    Baixo,
    Medio,
    Alto,
}

/// Situação de uma mudança do plano.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusMudanca {
    /// Ainda não aplicada.
    Pendente,
    /// Aplicada e confirmada por releitura do driver.
    Aplicada,
    /// O driver já tinha o valor desejado.
    JaNoValor,
    /// Travada pelo usuário; não será alterada.
    Travada,
    /// Recusada pela validação do serviço.
    Recusada,
    /// Falhou ao gravar ou a releitura divergiu.
    Falhou,
}

/// Entrada do histórico.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntradaHistorico {
    pub camada: Camada,
    pub id: u64,
    pub quando_unix: u64,
    pub acao: AcaoHistorico,
    pub perfil: Option<String>,
    pub setting: Option<String>,
    pub antes: Option<String>,
    pub depois: Option<String>,
    pub descricao: String,
}

/// Tipo de evento registrado no histórico.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcaoHistorico {
    Aplicacao,
    Restauracao,
    Rollback,
    Travamento,
    Destravamento,
    Preferencias,
    DerivaDetectada,
}
