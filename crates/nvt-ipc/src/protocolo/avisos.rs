//! Avisos estruturados para o usuário: gravidade, códigos estáveis e ordenação.

use super::*;

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
