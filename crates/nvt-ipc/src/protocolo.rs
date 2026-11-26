//! Mensagens do protocolo (DTOs).
//!
//! São tipos próprios do IPC, independentes do motor: o serviço converte
//! entre eles e o seu modelo interno. A UI só envia comandos de alto nível;
//! configurações do driver aparecem apenas por nome simbólico (ex.:
//! `"vsync"`, `"dlss_preset"`), nunca por ID DRS ou valor bruto.

use serde::{Deserialize, Serialize};

use crate::usuario::{
    Camada, Diagnostico, EstadoAgente, EstadoExperimental, EstadoTarefa, FatosUsuario,
    OperacaoUsuario, PlanoAgente, ResultadoUsuario, ResumoSistema, SettingUsuario,
    SugestaoExperimental, validar_texto,
};

/// Limite inferior aceito para o limitador de FPS.
pub const LIMITE_FPS_MIN: u32 = 20;
/// Limite superior aceito para o limitador de FPS.
pub const LIMITE_FPS_MAX: u32 = 1000;
/// Máximo de entradas pedidas em [`Requisicao::ListarHistorico`].
pub const HISTORICO_MAX: u32 = 1000;
/// Comprimento máximo do nome de um perfil (em caracteres).
pub const PERFIL_MAX: usize = 256;
/// Comprimento máximo de um nome simbólico de setting.
pub const SETTING_MAX: usize = 64;

// ---------------------------------------------------------------------------
// Requisições
// ---------------------------------------------------------------------------

/// Requisição enviada pela UI ao serviço.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum Requisicao {
    /// Resumo do estado atual.
    ObterEstado,
    /// Plano que seria aplicado agora (sem aplicar).
    ObterPlano,
    /// Calcula e aplica o plano imediatamente.
    AplicarAgora,
    /// Restaura os valores originais de tudo que o nv-tuner alterou.
    RestaurarOriginal,
    /// Atualiza as preferências do usuário.
    DefinirPreferencias {
        sincronismo: Sincronismo,
        qualidade: Qualidade,
        /// `None` = sem limitador.
        limite_fps: Option<u32>,
    },
    /// Últimas `limite` entradas do histórico (mais recentes primeiro).
    ListarHistorico {
        limite: u32,
    },
    /// Marca uma setting como escolha do usuário: o avaliador não a altera.
    TravarSetting {
        perfil: String,
        setting: String,
    },
    /// Devolve uma setting travada ao controle do avaliador.
    DestravarSetting {
        perfil: String,
        setting: String,
    },
    /// Refaz a varredura de jogos instalados.
    ReescanearJogos,
    AgenteConsultar {
        fatos: FatosUsuario,
        ultima_tarefa: Option<EstadoTarefa>,
        geracao_restaurada: u64,
    },
    AgenteRelatar {
        id_tarefa: u64,
        operacao: OperacaoUsuario,
        revisao: u64,
        resultados: Vec<ResultadoUsuario>,
    },
    DefinirAceiteExperimental {
        regra: String,
        aceito: bool,
    },
    ReverterAjusteUsuario {
        destino: SettingUsuario,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alvo_tela: Option<String>,
    },
    /// O usuário viu o aviso "Atualizado para a vX.Y.Z" (v6, U-47/U-52).
    /// A identidade vem só do token do chamador; `versao` diferente de
    /// `ultima_atualizacao.para` é recusada com `entrada_invalida`.
    MarcarAtualizacaoVista {
        versao: String,
    },
}

impl Requisicao {
    /// Validação de forma, feita pelo servidor antes de chamar o tratador.
    ///
    /// Não substitui a validação semântica do serviço (lista permitida de
    /// settings, existência do perfil); apenas rejeita entradas absurdas.
    pub fn validar(&self) -> Result<(), String> {
        match self {
            Self::DefinirPreferencias {
                limite_fps: Some(fps),
                ..
            } if !(LIMITE_FPS_MIN..=LIMITE_FPS_MAX).contains(fps) => Err(format!(
                "limite_fps deve estar entre {LIMITE_FPS_MIN} e {LIMITE_FPS_MAX}"
            )),
            Self::ListarHistorico { limite } if !(1..=HISTORICO_MAX).contains(limite) => {
                Err(format!("limite deve estar entre 1 e {HISTORICO_MAX}"))
            }
            Self::TravarSetting { perfil, setting }
            | Self::DestravarSetting { perfil, setting } => {
                validar_perfil(perfil)?;
                validar_setting(setting)
            }
            Self::DefinirAceiteExperimental { regra, .. } => {
                if regra.is_empty()
                    || regra.len() > 128
                    || !regra.bytes().all(|b| {
                        b.is_ascii_lowercase()
                            || b.is_ascii_digit()
                            || matches!(b, b'-' | b'_' | b'.')
                    })
                {
                    return Err("identificador de regra inválido".into());
                }
                Ok(())
            }
            Self::AgenteConsultar { fatos, .. } => fatos.validar(),
            Self::AgenteRelatar { resultados, .. } => {
                if resultados.len() > 256 {
                    return Err("muitos resultados da tarefa".into());
                }
                let mut destinos = std::collections::BTreeSet::new();
                for resultado in resultados {
                    resultado.destino.validar()?;
                    if !destinos.insert(&resultado.destino) {
                        return Err("destino repetido no relato".into());
                    }
                    if let Some(mudanca) = &resultado.mudanca {
                        mudanca.validar()?;
                        if mudanca.destino() != resultado.destino {
                            return Err("mudança de destino incompatível com o relato".into());
                        }
                    }
                    if let Some(detalhe) = &resultado.detalhe {
                        validar_texto(detalhe, 1024)?;
                    }
                }
                Ok(())
            }
            Self::ReverterAjusteUsuario { destino, alvo_tela } => {
                destino.validar()?;
                if let Some(alvo_tela) = alvo_tela
                    && !matches!(destino, SettingUsuario::Refresh { alvo } if alvo == alvo_tela)
                {
                    return Err("alvo_tela incompatível com o destino".into());
                }
                Ok(())
            }
            Self::MarcarAtualizacaoVista { versao } => validar_versao(versao),
            _ => Ok(()),
        }
    }
}

/// Versão (`X.Y.Z`, opcionalmente com sufixo): 1 a 64 caracteres ASCII
/// alfanuméricos, `.`, `-` ou `+`.
pub fn validar_versao(versao: &str) -> Result<(), String> {
    if versao.is_empty()
        || versao.len() > 64
        || !versao
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
    {
        return Err("versão inválida".into());
    }
    Ok(())
}

/// Nome de perfil: 1 a [`PERFIL_MAX`] caracteres, sem caracteres de controle.
pub fn validar_perfil(perfil: &str) -> Result<(), String> {
    let n = perfil.chars().count();
    if n == 0 || n > PERFIL_MAX || perfil.chars().any(char::is_control) {
        return Err(format!(
            "nome de perfil inválido (1 a {PERFIL_MAX} caracteres, sem controle)"
        ));
    }
    Ok(())
}

/// Nome simbólico de setting: 1 a [`SETTING_MAX`] caracteres em
/// `[a-z0-9_.]`, começando por letra.
pub fn validar_setting(setting: &str) -> Result<(), String> {
    let ok = !setting.is_empty()
        && setting.len() <= SETTING_MAX
        && setting.starts_with(|c: char| c.is_ascii_lowercase())
        && setting
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "nome simbólico de setting inválido (até {SETTING_MAX} caracteres em [a-z0-9_.])"
        ))
    }
}

// ---------------------------------------------------------------------------
// Preferências
// ---------------------------------------------------------------------------

/// Política de sincronismo de quadros.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sincronismo {
    /// Sem V-Sync nem G-Sync (menor latência).
    Desligado,
    /// V-Sync forçado no driver.
    VSync,
    /// G-Sync/VRR com V-Sync no driver e limitador abaixo da taxa do monitor.
    GSync,
}

/// Teto de qualidade visual escolhido pelo usuário.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Qualidade {
    Desempenho,
    Equilibrado,
    Qualidade,
}

/// Preferências atuais (eco do que foi definido).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferencias {
    pub sincronismo: Sincronismo,
    pub qualidade: Qualidade,
    pub limite_fps: Option<u32>,
}

// ---------------------------------------------------------------------------
// Respostas
// ---------------------------------------------------------------------------

/// Resposta enviada pelo serviço à UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}

/// Estado da atualização automática (v6, `docs/spec/atualizacao.md` §10).
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

/// Gravidade de um [`Aviso`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NivelAviso {
    /// Informativo: nada a fazer.
    Info,
    /// Merece atenção: algo foi contornado automaticamente.
    Atencao,
    /// Algo deixou de funcionar e precisa de uma pessoa.
    Erro,
}

/// Códigos estáveis de [`Aviso::codigo`]. A UI pode tratar um código
/// específico (ex.: desabilitar a restauração); um código desconhecido é só
/// exibido.
pub mod codigo_aviso {
    /// `estado.json` inválido, guardado à parte como `.corrompido-<data>`.
    pub const ESTADO_CORROMPIDO: &str = "estado_corrompido";
    /// Estado recuperado de `estado.anterior.json`.
    pub const ESTADO_RECUPERADO: &str = "estado_recuperado";
    /// Sem cópia válida: o estado recomeçou limpo.
    pub const ESTADO_REINICIADO: &str = "estado_reiniciado";
    /// Outro aviso da abertura do estado.
    pub const ESTADO_AVISO: &str = "estado_aviso";
    /// `snapshot-original.json` ilegível: restauração desabilitada.
    pub const SNAPSHOT_INDISPONIVEL: &str = "snapshot_indisponivel";
    /// O diretório de estado foi posto em quarentena e o snapshot original
    /// ficou lá: o snapshot atual (ou o próximo) é só uma base pós-quarentena,
    /// e “voltar ao original” volta a essa base.
    pub const SNAPSHOT_ORIGINAL_PERDIDO: &str = "snapshot_original_perdido";
    /// Erro isolado num perfil de jogo ao aplicar o plano.
    pub const ERRO_PERFIL_APLICACAO: &str = "erro_perfil_aplicacao";
    /// Erro isolado num perfil de jogo ao tratar a deriva.
    pub const ERRO_PERFIL_DERIVA: &str = "erro_perfil_deriva";
    /// Jogos deixados de fora do plano (nome/executável inválido, erro no perfil).
    pub const JOGOS_IGNORADOS: &str = "jogos_ignorados";
    /// Sincronismo G-SYNC num monitor sem VRR: vira V-Sync comum (v4).
    pub const GSYNC_SEM_VRR: &str = "gsync_sem_vrr";
    /// G-SYNC escolhido, VRR possível mas desligado no Painel NVIDIA (v4).
    pub const VRR_DESLIGADO_NO_PAINEL: &str = "vrr_desligado_no_painel";
    /// Ação pendente (adiada pelo portão) que falhou em todas as tentativas
    /// depois que o portão liberou e foi descartada (v4).
    pub const ACAO_PENDENTE_DESCARTADA: &str = "acao_pendente_descartada";
}

/// Aviso estruturado para o usuário.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Aviso {
    pub nivel: NivelAviso,
    /// Código estável (ver [`codigo_aviso`]).
    pub codigo: String,
    /// Frase para exibição (pt-BR).
    pub mensagem: String,
    /// Detalhe técnico opcional (texto original do motor).
    pub detalhe: Option<String>,
    /// Segundos desde a época Unix (UTC).
    pub quando_unix: u64,
}

impl Aviso {
    pub fn novo(
        nivel: NivelAviso,
        codigo: &str,
        mensagem: impl Into<String>,
        detalhe: Option<String>,
        quando_unix: u64,
    ) -> Self {
        Self {
            nivel,
            codigo: codigo.to_owned(),
            mensagem: mensagem.into(),
            detalhe,
            quando_unix,
        }
    }

    /// Snapshot original ilegível: restauração desabilitada.
    pub fn snapshot_indisponivel(detalhe: impl Into<String>, quando_unix: u64) -> Self {
        Self::novo(
            NivelAviso::Erro,
            codigo_aviso::SNAPSHOT_INDISPONIVEL,
            "O snapshot original está ilegível, por isso a restauração está desabilitada. \
             O arquivo não é sobrescrito: confira-o ou remova-o manualmente (a próxima \
             aplicação captura um novo original).",
            Some(detalhe.into()),
            quando_unix,
        )
    }

    /// Jogos deixados de fora do plano.
    pub fn jogos_ignorados(ignorados: &[JogoIgnorado], quando_unix: u64) -> Self {
        let n = ignorados.len();
        let mensagem = if n == 1 {
            "1 jogo ficou de fora do plano por um problema só dele; os demais seguiram.".to_owned()
        } else {
            format!(
                "{n} jogos ficaram de fora do plano por problemas só deles; os demais seguiram."
            )
        };
        let detalhe = ignorados
            .iter()
            .map(|j| format!("{}: {}", j.nome, j.motivo))
            .collect::<Vec<_>>()
            .join("; ");
        Self::novo(
            NivelAviso::Atencao,
            codigo_aviso::JOGOS_IGNORADOS,
            mensagem,
            Some(detalhe),
            quando_unix,
        )
    }

    /// G-SYNC escolhido num monitor sem VRR (aviso do plano `GsyncSemVrr`).
    pub fn gsync_sem_vrr(quando_unix: u64) -> Self {
        Self::novo(
            NivelAviso::Atencao,
            codigo_aviso::GSYNC_SEM_VRR,
            "O monitor não tem VRR: com G-SYNC escolhido, o resultado é V-Sync comum, \
             com fila de quadros. Prefira “sem sincronismo”.",
            None,
            quando_unix,
        )
    }

    /// VRR possível mas desligado no painel (aviso do plano
    /// `VrrDesligadoNoPainel`). O app não liga o VRR do monitor.
    pub fn vrr_desligado_no_painel(quando_unix: u64) -> Self {
        Self::novo(
            NivelAviso::Atencao,
            codigo_aviso::VRR_DESLIGADO_NO_PAINEL,
            "O monitor aceita VRR, mas ele está desligado: ative o G-SYNC em \
             Painel de Controle NVIDIA > Configurar G-SYNC.",
            None,
            quando_unix,
        )
    }

    /// Ação pendente que falhou em `tentativas` execuções seguidas e foi
    /// descartada.
    pub fn acao_pendente_descartada(
        acao: AcaoAdiada,
        tentativas: u32,
        detalhe: impl Into<String>,
        quando_unix: u64,
    ) -> Self {
        let (o_que, depois) = match acao {
            AcaoAdiada::AplicarPlano => (
                "A aplicação das alterações",
                "O próximo ciclo tenta de novo; use “Aplicar agora” para forçar.",
            ),
            AcaoAdiada::TratarDeriva => (
                "O tratamento da mudança detectada no driver",
                "O próximo ciclo tenta de novo; use “Aplicar agora” para forçar.",
            ),
            AcaoAdiada::RestaurarOriginal => (
                "A restauração dos valores originais",
                "Use “Restaurar original” de novo.",
            ),
        };
        Self::novo(
            NivelAviso::Erro,
            codigo_aviso::ACAO_PENDENTE_DESCARTADA,
            format!(
                "{o_que}, que esperava o jogo fechar, falhou {tentativas} vez(es) e foi \
                 descartada. {depois}"
            ),
            Some(detalhe.into()),
            quando_unix,
        )
    }

    /// Erro isolado num perfil de jogo (`deriva` = no tratamento da deriva).
    pub fn erro_perfil(detalhe: impl Into<String>, deriva: bool, quando_unix: u64) -> Self {
        let (codigo, mensagem) = if deriva {
            (
                codigo_aviso::ERRO_PERFIL_DERIVA,
                "Falha ao tratar a deriva num perfil de jogo; os demais perfis seguiram \
                 e ele será tentado de novo no próximo ciclo.",
            )
        } else {
            (
                codigo_aviso::ERRO_PERFIL_APLICACAO,
                "Falha ao aplicar o plano num perfil de jogo; os demais perfis seguiram \
                 e ele será tentado de novo no próximo ciclo.",
            )
        };
        Self::novo(
            NivelAviso::Erro,
            codigo,
            mensagem,
            Some(detalhe.into()),
            quando_unix,
        )
    }
}

/// Ordena do mais grave para o menos grave e, no mesmo nível, do mais recente
/// para o mais antigo.
pub fn ordenar_avisos(avisos: &mut [Aviso]) {
    avisos.sort_by(|a, b| {
        b.nivel
            .cmp(&a.nivel)
            .then_with(|| b.quando_unix.cmp(&a.quando_unix))
    });
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

#[cfg(test)]
mod testes {
    use super::*;
    use crate::quadro::{codificar, decodificar};

    fn prefs() -> Preferencias {
        Preferencias {
            sincronismo: Sincronismo::Desligado,
            qualidade: Qualidade::Equilibrado,
            limite_fps: Some(141),
        }
    }

    /// Um exemplar de cada requisição. O `match` garante (em tempo de
    /// compilação) que nenhuma variante nova fique sem teste.
    fn consulta() -> Requisicao {
        use crate::Fato;
        Requisicao::AgenteConsultar {
            fatos: FatosUsuario {
                tela: Fato::Desconhecido,
                mouse: Fato::Desconhecido,
                swap_effect_upgrade: Fato::Ausente,
                captura_historica: Fato::Conhecido(1),
                diagnosticos: vec![Diagnostico {
                    codigo: "drr".into(),
                    estado: crate::EstadoDiagnostico::Indisponivel,
                }],
                gestao: vec![],
            },
            ultima_tarefa: None,
            geracao_restaurada: 0,
        }
    }

    fn todas_requisicoes() -> Vec<Requisicao> {
        let v = vec![
            Requisicao::ObterEstado,
            Requisicao::ObterPlano,
            Requisicao::AplicarAgora,
            Requisicao::RestaurarOriginal,
            Requisicao::DefinirPreferencias {
                sincronismo: Sincronismo::GSync,
                qualidade: Qualidade::Qualidade,
                limite_fps: None,
            },
            Requisicao::DefinirPreferencias {
                sincronismo: Sincronismo::VSync,
                qualidade: Qualidade::Desempenho,
                limite_fps: Some(60),
            },
            Requisicao::ListarHistorico { limite: 50 },
            Requisicao::TravarSetting {
                perfil: "WARDOGS".into(),
                setting: "dlss_preset".into(),
            },
            Requisicao::DestravarSetting {
                perfil: "global".into(),
                setting: "vsync".into(),
            },
            Requisicao::ReescanearJogos,
            consulta(),
            Requisicao::AgenteRelatar {
                id_tarefa: 1,
                operacao: OperacaoUsuario::Aplicar,
                revisao: 7,
                resultados: vec![ResultadoUsuario {
                    destino: SettingUsuario::GameDvrCapturaHistorica,
                    status: StatusMudanca::Aplicada,
                    detalhe: None,
                    mudanca: Some(crate::AjusteUsuario::GameDvrCapturaHistorica {
                        antes: Some(1),
                        depois: Some(0),
                    }),
                }],
            },
            Requisicao::DefinirAceiteExperimental {
                regra: "sincronismo-gsync-limite".into(),
                aceito: true,
            },
            Requisicao::ReverterAjusteUsuario {
                alvo_tela: None,
                destino: SettingUsuario::MouseAceleracao,
            },
            Requisicao::ReverterAjusteUsuario {
                alvo_tela: Some("monitor-estavel".into()),
                destino: SettingUsuario::Refresh {
                    alvo: "monitor-estavel".into(),
                },
            },
            Requisicao::MarcarAtualizacaoVista {
                versao: "1.4.0".into(),
            },
        ];
        for r in &v {
            match r {
                Requisicao::ObterEstado
                | Requisicao::ObterPlano
                | Requisicao::AplicarAgora
                | Requisicao::RestaurarOriginal
                | Requisicao::DefinirPreferencias { .. }
                | Requisicao::ListarHistorico { .. }
                | Requisicao::TravarSetting { .. }
                | Requisicao::DestravarSetting { .. }
                | Requisicao::ReescanearJogos
                | Requisicao::AgenteConsultar { .. }
                | Requisicao::AgenteRelatar { .. }
                | Requisicao::DefinirAceiteExperimental { .. }
                | Requisicao::ReverterAjusteUsuario { .. }
                | Requisicao::MarcarAtualizacaoVista { .. } => {}
            }
        }
        v
    }

    /// Um motivo de cada tipo. O `match` garante que nenhum fique sem teste.
    fn todos_os_motivos() -> Vec<MotivoNaoAplicada> {
        let v = vec![
            MotivoNaoAplicada::JogoSemDlss,
            MotivoNaoAplicada::ArquiteturaNaoPrecisa {
                arquitetura: "ada".into(),
            },
            MotivoNaoAplicada::SemTensorCores,
            MotivoNaoAplicada::Experimental,
            MotivoNaoAplicada::BloqueadaPeloTetoDeQualidade {
                teto: Qualidade::Qualidade,
            },
            MotivoNaoAplicada::EscolhaDoUsuario {
                setting: "dlss_preset".into(),
            },
            MotivoNaoAplicada::ConflitoComRegraMaisEspecifica {
                regra: "outra".into(),
            },
            MotivoNaoAplicada::FatoDesconhecido {
                fato: "jogo_reflex".into(),
            },
            MotivoNaoAplicada::CondicaoNaoAtendida {
                condicao: "arquitetura".into(),
                esperado: "ampere".into(),
                encontrado: "turing".into(),
            },
            MotivoNaoAplicada::VrrInativo,
            MotivoNaoAplicada::JogoComReflex,
        ];
        for m in &v {
            match m {
                MotivoNaoAplicada::JogoSemDlss
                | MotivoNaoAplicada::ArquiteturaNaoPrecisa { .. }
                | MotivoNaoAplicada::SemTensorCores
                | MotivoNaoAplicada::Experimental
                | MotivoNaoAplicada::BloqueadaPeloTetoDeQualidade { .. }
                | MotivoNaoAplicada::EscolhaDoUsuario { .. }
                | MotivoNaoAplicada::ConflitoComRegraMaisEspecifica { .. }
                | MotivoNaoAplicada::FatoDesconhecido { .. }
                | MotivoNaoAplicada::CondicaoNaoAtendida { .. }
                | MotivoNaoAplicada::VrrInativo
                | MotivoNaoAplicada::JogoComReflex => {}
            }
        }
        v
    }

    fn jogo_exemplo() -> Jogo {
        Jogo {
            nome: "WARDOGS".into(),
            perfil: "WARDOGS".into(),
            origem_perfil: OrigemPerfil::Usuario,
            launcher: "Steam".into(),
            api: "dx12".into(),
            dlss: Some("310.6.0.0".into()),
            reflex: Some(true),
            anticheat_kernel: None,
            competitivo: Some("cs2".into()),
            em_execucao: true,
            orientacoes: vec!["reflex_on_boost".into(), "vsync_jogo_off".into()],
            resumo: ResumoJogo {
                aplicadas: vec![RegraAplicada {
                    regra: "dlss-preset-e-turing".into(),
                    descricao: "DLSS-SR com preset E".into(),
                    settings: vec!["dlss_override".into(), "dlss_preset".into()],
                }],
                nao_aplicadas: todos_os_motivos()
                    .into_iter()
                    .map(|motivo| RegraNaoAplicada {
                        regra: "r".into(),
                        descricao: "d".into(),
                        motivo,
                    })
                    .collect(),
                herda_do_global: vec!["energia".into(), "vsync".into()],
            },
        }
    }

    #[test]
    fn formato_json_do_jogo_e_dos_motivos() {
        use serde_json::{from_value, json, to_value};
        for m in todos_os_motivos() {
            assert_eq!(
                from_value::<MotivoNaoAplicada>(to_value(&m).unwrap()).unwrap(),
                m
            );
        }
        assert_eq!(
            to_value(MotivoNaoAplicada::ArquiteturaNaoPrecisa {
                arquitetura: "ada".into()
            })
            .unwrap(),
            json!({"tipo":"arquitetura_nao_precisa","arquitetura":"ada"})
        );
        assert_eq!(
            to_value(MotivoNaoAplicada::JogoSemDlss).unwrap(),
            json!({"tipo":"jogo_sem_dlss"})
        );
        assert_eq!(
            to_value(MotivoNaoAplicada::BloqueadaPeloTetoDeQualidade {
                teto: Qualidade::Qualidade
            })
            .unwrap(),
            json!({"tipo":"bloqueada_pelo_teto_de_qualidade","teto":"qualidade"})
        );
        for (o, s) in [
            (OrigemPerfil::Driver, "driver"),
            (OrigemPerfil::Usuario, "usuario"),
            (OrigemPerfil::CriadoPeloApp, "criado_pelo_app"),
            (OrigemPerfil::ACriar, "a_criar"),
        ] {
            assert_eq!(to_value(o).unwrap(), json!(s));
        }
        let j = to_value(jogo_exemplo()).unwrap();
        assert_eq!(j["origem_perfil"], json!("usuario"));
        assert_eq!(
            j["resumo"]["aplicadas"][0]["regra"],
            json!("dlss-preset-e-turing")
        );
        assert_eq!(j["resumo"]["herda_do_global"], json!(["energia", "vsync"]));
        assert_eq!(from_value::<Jogo>(j.clone()).unwrap(), jogo_exemplo());
        // Campo desconhecido no jogo é recusado.
        let mut extra = j;
        extra["extra"] = json!(1);
        assert!(from_value::<Jogo>(extra).is_err());
        // Plano da versão 2 (sem `jogos`) não é aceito.
        assert!(
            from_value::<Plano>(json!({
                "gerado_unix":0,"perfis":[],"reversoes":[],"ignorados":[]
            }))
            .is_err()
        );
    }

    fn todas_respostas() -> Vec<Resposta> {
        let v = vec![
            Resposta::Estado(Estado {
                agente: EstadoAgente::default(),
                diagnosticos: vec![],
                versao_servico: "0.0.1".into(),
                driver: Some(Driver {
                    versao: "581.57".into(),
                    ramo: Some("r580_00".into()),
                }),
                gpu: Some(Gpu {
                    nome: "NVIDIA GeForce RTX 4070".into(),
                    arquitetura: "ada".into(),
                    vram_mb: 12282,
                    capacidades: Capacidades {
                        dlss: true,
                        geracao_quadros: true,
                        reflex: true,
                        rebar: false,
                        refresh_mhz: Some(239_757),
                        vrr: Vrr {
                            possivel: Some(true),
                            ativo: Some(false),
                        },
                    },
                }),
                preferencias: prefs(),
                jogos: 12,
                ultima_aplicacao: Some(Aplicacao {
                    quando_unix: 1_790_000_000,
                    mudancas: 7,
                    falhas: 0,
                }),
                alertas_deriva: vec![AlertaDeriva {
                    perfil: "WARDOGS".into(),
                    setting: "dlss_preset".into(),
                    nome_legivel: "Preset do DLSS".into(),
                    esperado: "E".into(),
                    encontrado: None,
                    detectado_unix: 1_790_000_100,
                }],
                avisos: vec![
                    Aviso::snapshot_indisponivel("snapshot-original.json ilegível", 1_790_000_000),
                    Aviso::erro_perfil("perfil X: falha", true, 1_790_000_001),
                    Aviso::novo(
                        NivelAviso::Info,
                        codigo_aviso::ESTADO_RECUPERADO,
                        "Estado recuperado",
                        None,
                        1_790_000_002,
                    ),
                    Aviso::gsync_sem_vrr(1_790_000_003),
                    Aviso::vrr_desligado_no_painel(1_790_000_004),
                ],
                adiamento: Some(adiamento(AcaoAdiada::TratarDeriva)),
                atualizacao: atualizacao_exemplo(),
            }),
            Resposta::Estado(Estado {
                agente: EstadoAgente::default(),
                diagnosticos: vec![],
                versao_servico: "0.0.1".into(),
                driver: None,
                gpu: None,
                preferencias: prefs(),
                jogos: 0,
                ultima_aplicacao: None,
                alertas_deriva: vec![],
                avisos: vec![],
                adiamento: None,
                atualizacao: EstadoAtualizacao::default(),
            }),
            Resposta::Plano(Plano {
                sistema: ResumoSistema::default(),
                sugestoes_driver: vec![],
                gerado_unix: 1,
                perfis: vec![],
                reversoes: vec![],
                ignorados: vec![],
                jogos: vec![],
                adiada: Some(adiamento(AcaoAdiada::AplicarPlano)),
            }),
            Resposta::Plano(Plano {
                sistema: ResumoSistema::default(),
                sugestoes_driver: vec![],
                gerado_unix: 1_790_000_000,
                perfis: vec![PlanoPerfil {
                    perfil: "global".into(),
                    nome_legivel: "Perfil global".into(),
                    mudancas: vec![Mudanca {
                        alvo_tela: None,
                        camada: Camada::Driver,
                        experimental: EstadoExperimental::Estavel,
                        setting: "vsync".into(),
                        nome_legivel: "Sincronização vertical".into(),
                        antes: Some("Controlado pelo aplicativo".into()),
                        depois: Some("Forçar desligado".into()),
                        motivo: "Preferência: sincronismo desligado".into(),
                        regra: "sincronismo.desligado".into(),
                        impacto_visual: ImpactoVisual::Baixo,
                        status: StatusMudanca::Pendente,
                    }],
                }],
                reversoes: vec![Reversao {
                    perfil: "global".into(),
                    nome_perfil: "Perfil global".into(),
                    setting: "limite_fps".into(),
                    nome_legivel: "Limitador de FPS".into(),
                    atual: Some("141 FPS".into()),
                    original: None,
                    acao: AcaoReversao::Remover,
                    motivo: "Saiu do plano: preferência limite_fps".into(),
                    status: StatusMudanca::Aplicada,
                }],
                ignorados: vec![JogoIgnorado {
                    nome: "Jogo\u{0}Ruim".into(),
                    exe: String::new(),
                    motivo: "executável vazio".into(),
                }],
                jogos: vec![jogo_exemplo()],
                adiada: None,
            }),
            Resposta::Restauracao { adiada: None },
            Resposta::Restauracao {
                adiada: Some(adiamento(AcaoAdiada::RestaurarOriginal)),
            },
            Resposta::Historico {
                entradas: vec![EntradaHistorico {
                    camada: Camada::Driver,
                    id: 1,
                    quando_unix: 1_790_000_000,
                    acao: AcaoHistorico::Aplicacao,
                    perfil: Some("WARDOGS".into()),
                    setting: Some("dlss_preset".into()),
                    antes: None,
                    depois: Some("E".into()),
                    descricao: "Aplicação automática".into(),
                }],
            },
            Resposta::Historico { entradas: vec![] },
            Resposta::Adiada {
                adiamento: adiamento(AcaoAdiada::AplicarPlano),
            },
            Resposta::Adiada {
                adiamento: Adiamento {
                    processos: vec![],
                    desde_unix: 1_790_000_600,
                    acao: AcaoAdiada::TratarDeriva,
                    lista_indisponivel: true,
                },
            },
            Resposta::PlanoAgente(PlanoAgente {
                revisao: 7,
                geracao_restauracao: 2,
                tarefa: Some(crate::TarefaUsuario {
                    tarefa: crate::ReferenciaTarefa {
                        id_tarefa: 1,
                        operacao: OperacaoUsuario::Aplicar,
                        revisao: 7,
                    },
                    acoes: ajustes_exemplo()
                        .into_iter()
                        .map(|ajuste| crate::AcaoUsuario::Aplicar { ajuste })
                        .collect(),
                }),
                processos_vigiados: ["cs2.exe".into()].into(),
            }),
            Resposta::Encerrar,
            Resposta::Ok,
            Resposta::erro(CodigoErro::SettingNaoPermitida, "setting fora da lista"),
        ];
        for r in &v {
            match r {
                Resposta::Estado(_)
                | Resposta::Plano(_)
                | Resposta::Historico { .. }
                | Resposta::Restauracao { .. }
                | Resposta::Adiada { .. }
                | Resposta::PlanoAgente(_)
                | Resposta::Encerrar
                | Resposta::Ok
                | Resposta::Erro { .. } => {}
            }
        }
        v
    }

    fn ajustes_exemplo() -> [crate::AjusteUsuario; 4] {
        use crate::{AjusteUsuario, MouseSpi, RefreshRacional};
        [
            AjusteUsuario::Refresh {
                alvo: "alvo-estavel".into(),
                antes: RefreshRacional {
                    numerador: 60,
                    denominador: 1,
                },
                depois: RefreshRacional {
                    numerador: 144_000,
                    denominador: 1001,
                },
            },
            AjusteUsuario::MouseAceleracao {
                antes: MouseSpi {
                    limiar1: 6,
                    limiar2: 10,
                    aceleracao: 1,
                },
                depois: MouseSpi {
                    limiar1: 6,
                    limiar2: 10,
                    aceleracao: 0,
                },
            },
            AjusteUsuario::DxSwapEffectUpgrade {
                antes: None,
                depois: Some("1".into()),
            },
            AjusteUsuario::GameDvrCapturaHistorica {
                antes: Some(1),
                depois: Some(0),
            },
        ]
    }

    fn adiamento(acao: AcaoAdiada) -> Adiamento {
        Adiamento {
            processos: vec!["cs2.exe".into(), "EasyAntiCheat_EOS.exe".into()],
            desde_unix: 1_790_000_500,
            acao,
            lista_indisponivel: false,
        }
    }

    #[test]
    fn formato_json_v4() {
        use serde_json::{from_value, json, to_value};
        let a = to_value(adiamento(AcaoAdiada::RestaurarOriginal)).unwrap();
        assert_eq!(
            a,
            json!({
                "processos": ["cs2.exe", "EasyAntiCheat_EOS.exe"],
                "desde_unix": 1_790_000_500,
                "acao": "restaurar_original",
                "lista_indisponivel": false
            })
        );
        for (acao, s) in [
            (AcaoAdiada::AplicarPlano, "aplicar_plano"),
            (AcaoAdiada::TratarDeriva, "tratar_deriva"),
            (AcaoAdiada::RestaurarOriginal, "restaurar_original"),
        ] {
            assert_eq!(to_value(acao).unwrap(), json!(s));
            assert_eq!(from_value::<AcaoAdiada>(json!(s)).unwrap(), acao);
        }
        let mut extra = a;
        extra["extra"] = json!(1);
        assert!(from_value::<Adiamento>(extra).is_err());
        assert_eq!(
            to_value(Resposta::Restauracao { adiada: None }).unwrap(),
            json!({"tipo": "restauracao", "adiada": null})
        );
        let mut sem_lista = adiamento(AcaoAdiada::AplicarPlano);
        sem_lista.processos.clear();
        sem_lista.lista_indisponivel = true;
        assert_eq!(
            to_value(Resposta::Adiada {
                adiamento: sem_lista
            })
            .unwrap(),
            json!({
                "tipo": "adiada",
                "adiamento": {
                    "processos": [],
                    "desde_unix": 1_790_000_500,
                    "acao": "aplicar_plano",
                    "lista_indisponivel": true
                }
            })
        );
        assert_eq!(
            to_value(MotivoNaoAplicada::VrrInativo).unwrap(),
            json!({"tipo": "vrr_inativo"})
        );
        assert_eq!(
            to_value(MotivoNaoAplicada::JogoComReflex).unwrap(),
            json!({"tipo": "jogo_com_reflex"})
        );
        assert_eq!(
            to_value(Vrr {
                possivel: Some(true),
                ativo: None
            })
            .unwrap(),
            json!({"possivel": true, "ativo": null})
        );
        // Capacidades da v3 (`vrr` booleano) não são aceitas pela v4.
        assert!(
            from_value::<Capacidades>(json!({
                "dlss": true, "geracao_quadros": false, "reflex": true,
                "rebar": false, "vrr": false
            }))
            .is_err()
        );
        let j = to_value(jogo_exemplo()).unwrap();
        assert_eq!(j["competitivo"], json!("cs2"));
        assert_eq!(j["em_execucao"], json!(true));
        assert_eq!(
            j["orientacoes"],
            json!(["reflex_on_boost", "vsync_jogo_off"])
        );
        // Jogo da v3 (sem `em_execucao`/`orientacoes`) não é aceito pela v4.
        let mut v3 = j;
        let o = v3.as_object_mut().unwrap();
        o.remove("competitivo");
        o.remove("em_execucao");
        o.remove("orientacoes");
        assert!(from_value::<Jogo>(v3).is_err());
    }

    #[test]
    fn mensagens_da_v4_e_v5_sao_recusadas() {
        for (versao, payload) in [
            (4, &br#"{"versao":4,"corpo":{"tipo":"obter_estado"}}"#[..]),
            (4, br#"{"versao":4,"corpo":{"tipo":"ok"}}"#),
            (5, br#"{"versao":5,"corpo":{"tipo":"obter_estado"}}"#),
            (5, br#"{"versao":5,"corpo":{"tipo":"ok"}}"#),
            // Corpo inválido: a recusa vem do envelope, antes do corpo.
            (5, br#"{"versao":5,"corpo":{"tipo":"formatar_disco"}}"#),
        ] {
            assert!(matches!(
                decodificar::<Requisicao>(payload),
                Err(crate::ErroIpc::VersaoIncompativel {
                    recebida,
                    esperada: 6
                }) if recebida == versao
            ));
            assert!(matches!(
                decodificar::<Resposta>(payload),
                Err(crate::ErroIpc::VersaoIncompativel { recebida, .. }) if recebida == versao
            ));
        }
    }

    fn atualizacao_exemplo() -> EstadoAtualizacao {
        EstadoAtualizacao {
            versao_instalada: "1.4.0".into(),
            ultima_verificacao_unix: Some(1_790_000_000),
            proxima_verificacao_unix: Some(1_790_021_600),
            ultima_atualizacao: Some(AtualizacaoConcluida {
                de: "1.3.2".into(),
                para: "1.4.0".into(),
                quando_unix: 1_789_990_000,
                reinicio_pendente: true,
            }),
            nao_vista: true,
            pendente: Some(AtualizacaoPendente {
                versao: "1.5.0".into(),
                motivo: MotivoPendente::JogoAberto,
                desde_unix: 1_790_000_100,
            }),
            falha: Some(FalhaAtualizacao {
                versao: "1.5.0".into(),
                codigo: 1603,
                tentativas: 2,
                abandonada: false,
                quando_unix: 1_790_000_050,
            }),
            erro_verificacao: Some(ErroVerificacao::RecusadoHttp(403)),
        }
    }

    #[test]
    fn formato_json_v6() {
        use serde_json::{from_value, json, to_value};
        let a = to_value(atualizacao_exemplo()).unwrap();
        assert_eq!(
            a,
            json!({
                "versao_instalada": "1.4.0",
                "ultima_verificacao_unix": 1_790_000_000u64,
                "proxima_verificacao_unix": 1_790_021_600u64,
                "ultima_atualizacao": {
                    "de": "1.3.2", "para": "1.4.0",
                    "quando_unix": 1_789_990_000u64, "reinicio_pendente": true
                },
                "nao_vista": true,
                "pendente": {
                    "versao": "1.5.0", "motivo": "jogo_aberto", "desde_unix": 1_790_000_100u64
                },
                "falha": {
                    "versao": "1.5.0", "codigo": 1603, "tentativas": 2,
                    "abandonada": false, "quando_unix": 1_790_000_050u64
                },
                "erro_verificacao": {"tipo": "recusado_http", "status": 403}
            })
        );
        assert_eq!(
            from_value::<EstadoAtualizacao>(a.clone()).unwrap(),
            atualizacao_exemplo()
        );
        let mut extra = a.clone();
        extra["extra"] = json!(1);
        assert!(from_value::<EstadoAtualizacao>(extra).is_err());
        let mut extra = a;
        extra["falha"]["extra"] = json!(1);
        assert!(from_value::<EstadoAtualizacao>(extra).is_err());
        for (m, s) in [
            (MotivoPendente::JogoAberto, "jogo_aberto"),
            (MotivoPendente::ListaIndisponivel, "lista_indisponivel"),
            (MotivoPendente::OutraInstalacao, "outra_instalacao"),
            (MotivoPendente::Baixando, "baixando"),
            (MotivoPendente::Instalando, "instalando"),
        ] {
            assert_eq!(to_value(m).unwrap(), json!(s));
            assert_eq!(from_value::<MotivoPendente>(json!(s)).unwrap(), m);
        }
        for (e, j) in [
            (ErroVerificacao::SemConexao, json!({"tipo": "sem_conexao"})),
            (
                ErroVerificacao::RecusadoHttp(404),
                json!({"tipo": "recusado_http", "status": 404}),
            ),
            (ErroVerificacao::Assinatura, json!({"tipo": "assinatura"})),
            (ErroVerificacao::Manifesto, json!({"tipo": "manifesto"})),
            (ErroVerificacao::Disco, json!({"tipo": "disco"})),
        ] {
            assert_eq!(to_value(e).unwrap(), j);
            assert_eq!(from_value::<ErroVerificacao>(j).unwrap(), e);
        }
        assert_eq!(
            to_value(EstadoAtualizacao::default()).unwrap(),
            json!({
                "versao_instalada": "", "ultima_verificacao_unix": null,
                "proxima_verificacao_unix": null, "ultima_atualizacao": null,
                "nao_vista": false, "pendente": null, "falha": null,
                "erro_verificacao": null
            })
        );
        assert_eq!(
            to_value(Requisicao::MarcarAtualizacaoVista {
                versao: "1.4.0".into()
            })
            .unwrap(),
            json!({"tipo": "marcar_atualizacao_vista", "versao": "1.4.0"})
        );
        assert!(
            from_value::<Requisicao>(json!({
                "tipo": "marcar_atualizacao_vista", "versao": "1.4.0", "sid": "S-1-5-18"
            }))
            .is_err()
        );
        assert_eq!(
            to_value(CodigoErro::EntradaInvalida).unwrap(),
            json!("entrada_invalida")
        );
    }

    #[test]
    fn ida_e_volta_de_todas_as_requisicoes() {
        for r in todas_requisicoes() {
            let bytes = codificar(&r).unwrap();
            assert_eq!(decodificar::<Requisicao>(&bytes).unwrap(), r);
        }
    }

    #[test]
    fn ida_e_volta_de_todas_as_respostas() {
        for r in todas_respostas() {
            let bytes = codificar(&r).unwrap();
            assert_eq!(decodificar::<Resposta>(&bytes).unwrap(), r);
        }
    }

    #[test]
    fn ida_e_volta_de_todos_os_enums_simples() {
        use serde_json::{from_value, to_value};
        for c in [
            CodigoErro::VersaoIncompativel,
            CodigoErro::MensagemGrande,
            CodigoErro::MensagemInvalida,
            CodigoErro::RequisicaoInvalida,
            CodigoErro::IdentidadeIndisponivel,
            CodigoErro::SettingNaoPermitida,
            CodigoErro::PerfilNaoEncontrado,
            CodigoErro::DriverIndisponivel,
            CodigoErro::Ocupado,
            CodigoErro::Interno,
        ] {
            assert_eq!(from_value::<CodigoErro>(to_value(c).unwrap()).unwrap(), c);
        }
        for s in [
            StatusMudanca::Pendente,
            StatusMudanca::Aplicada,
            StatusMudanca::JaNoValor,
            StatusMudanca::Travada,
            StatusMudanca::Recusada,
            StatusMudanca::Falhou,
        ] {
            assert_eq!(
                from_value::<StatusMudanca>(to_value(s).unwrap()).unwrap(),
                s
            );
        }
        for i in [
            ImpactoVisual::Nenhum,
            ImpactoVisual::Baixo,
            ImpactoVisual::Medio,
            ImpactoVisual::Alto,
        ] {
            assert_eq!(
                from_value::<ImpactoVisual>(to_value(i).unwrap()).unwrap(),
                i
            );
        }
        for a in [
            AcaoHistorico::Aplicacao,
            AcaoHistorico::Restauracao,
            AcaoHistorico::Rollback,
            AcaoHistorico::Travamento,
            AcaoHistorico::Destravamento,
            AcaoHistorico::Preferencias,
            AcaoHistorico::DerivaDetectada,
        ] {
            assert_eq!(
                from_value::<AcaoHistorico>(to_value(a).unwrap()).unwrap(),
                a
            );
        }
    }

    #[test]
    fn formato_json_estavel() {
        let json = String::from_utf8(
            codificar(&Requisicao::TravarSetting {
                perfil: "WARDOGS".into(),
                setting: "dlss_preset".into(),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            json,
            r#"{"versao":6,"corpo":{"tipo":"travar_setting","perfil":"WARDOGS","setting":"dlss_preset"}}"#
        );
        let json = String::from_utf8(codificar(&Resposta::Ok).unwrap()).unwrap();
        assert_eq!(json, r#"{"versao":6,"corpo":{"tipo":"ok"}}"#);
    }

    #[test]
    fn formato_json_de_aviso_reversao_e_ignorado() {
        use serde_json::{json, to_value};
        let a = Aviso::snapshot_indisponivel("x", 7);
        assert_eq!(
            to_value(&a).unwrap()["nivel"],
            json!("erro"),
            "nível em snake_case"
        );
        assert_eq!(
            to_value(&a).unwrap()["codigo"],
            json!("snapshot_indisponivel")
        );
        assert_eq!(to_value(NivelAviso::Atencao).unwrap(), json!("atencao"));
        for n in [NivelAviso::Info, NivelAviso::Atencao, NivelAviso::Erro] {
            assert_eq!(
                serde_json::from_value::<NivelAviso>(to_value(n).unwrap()).unwrap(),
                n
            );
        }
        for (acao, s) in [
            (AcaoReversao::GravarOriginal, "gravar_original"),
            (AcaoReversao::Remover, "remover"),
            (AcaoReversao::SoDeixarDeVigiar, "so_deixar_de_vigiar"),
        ] {
            assert_eq!(to_value(acao).unwrap(), json!(s));
            assert_eq!(
                serde_json::from_value::<AcaoReversao>(json!(s)).unwrap(),
                acao
            );
        }
        let j = to_value(JogoIgnorado {
            nome: "A".into(),
            exe: "a.exe".into(),
            motivo: "m".into(),
        })
        .unwrap();
        assert_eq!(j, json!({"nome":"A","exe":"a.exe","motivo":"m"}));
        // Campo desconhecido continua recusado (mudança incompatível = nova versão).
        assert!(
            serde_json::from_value::<Aviso>(json!({
                "nivel":"info","codigo":"x","mensagem":"m","detalhe":null,"quando_unix":0,"extra":1
            }))
            .is_err()
        );
    }

    #[test]
    fn avisos_ordenados_e_restauracao_bloqueada() {
        let mut v = vec![
            Aviso::novo(NivelAviso::Info, "a", "a", None, 5),
            Aviso::erro_perfil("p", false, 1),
            Aviso::jogos_ignorados(
                &[JogoIgnorado {
                    nome: "J".into(),
                    exe: "j.exe".into(),
                    motivo: "nome inválido".into(),
                }],
                9,
            ),
            Aviso::snapshot_indisponivel("s", 3),
        ];
        ordenar_avisos(&mut v);
        let codigos: Vec<&str> = v.iter().map(|a| a.codigo.as_str()).collect();
        assert_eq!(
            codigos,
            [
                codigo_aviso::SNAPSHOT_INDISPONIVEL,
                codigo_aviso::ERRO_PERFIL_APLICACAO,
                codigo_aviso::JOGOS_IGNORADOS,
                "a"
            ]
        );
        assert_eq!(v[2].detalhe.as_deref(), Some("J: nome inválido"));
        assert!(v[2].mensagem.starts_with("1 jogo ficou"));
        let mut e = match &todas_respostas()[1] {
            Resposta::Estado(e) => e.clone(),
            _ => unreachable!(),
        };
        assert!(e.restauracao_bloqueada().is_none());
        e.avisos = v;
        assert_eq!(
            e.restauracao_bloqueada().map(|a| a.nivel),
            Some(NivelAviso::Erro)
        );
    }

    #[test]
    fn validacao_de_forma() {
        for r in todas_requisicoes() {
            assert_eq!(r.validar(), Ok(()), "{r:?}");
        }
        let invalidas = [
            Requisicao::MarcarAtualizacaoVista {
                versao: String::new(),
            },
            Requisicao::MarcarAtualizacaoVista {
                versao: "1.0/../x".into(),
            },
            Requisicao::DefinirAceiteExperimental {
                regra: "regra/1".into(),
                aceito: true,
            },
            Requisicao::ReverterAjusteUsuario {
                alvo_tela: None,
                destino: SettingUsuario::Refresh {
                    alvo: String::new(),
                },
            },
            Requisicao::ReverterAjusteUsuario {
                alvo_tela: Some("outro-monitor".into()),
                destino: SettingUsuario::Refresh {
                    alvo: "monitor-do-plano".into(),
                },
            },
            Requisicao::AgenteRelatar {
                id_tarefa: 1,
                operacao: OperacaoUsuario::Aplicar,
                revisao: 7,
                resultados: vec![ResultadoUsuario {
                    destino: SettingUsuario::MouseAceleracao,
                    status: StatusMudanca::Aplicada,
                    detalhe: None,
                    mudanca: Some(crate::AjusteUsuario::GameDvrCapturaHistorica {
                        antes: Some(1),
                        depois: Some(0),
                    }),
                }],
            },
            Requisicao::ListarHistorico { limite: 0 },
            Requisicao::ListarHistorico {
                limite: HISTORICO_MAX + 1,
            },
            Requisicao::DefinirPreferencias {
                sincronismo: Sincronismo::Desligado,
                qualidade: Qualidade::Equilibrado,
                limite_fps: Some(5),
            },
            Requisicao::TravarSetting {
                perfil: String::new(),
                setting: "vsync".into(),
            },
            Requisicao::TravarSetting {
                perfil: "a\u{0}b".into(),
                setting: "vsync".into(),
            },
            Requisicao::TravarSetting {
                perfil: "x".repeat(PERFIL_MAX + 1),
                setting: "vsync".into(),
            },
            Requisicao::DestravarSetting {
                perfil: "global".into(),
                setting: "0x10A879CF".into(),
            },
            Requisicao::DestravarSetting {
                perfil: "global".into(),
                setting: "a".repeat(SETTING_MAX + 1),
            },
            Requisicao::DestravarSetting {
                perfil: "global".into(),
                setting: "VSync".into(),
            },
        ];
        for r in invalidas {
            assert!(r.validar().is_err(), "{r:?}");
        }
    }
}
