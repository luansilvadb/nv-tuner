//! Ponto único de classificação das requisições IPC.
//!
//! Cada [`Requisicao`] é classificada aqui, em `match` exaustivos e sem
//! curinga: uma variante nova no protocolo só compila depois de decidir quem
//! a atende ([`rota`]) e se ela grava ([`grava_com_driver`] e
//! [`grava_sem_driver`]). Os tratadores (núcleo, coordenador, saúde,
//! partidas) consultam estas funções em vez de repetir listas próprias.

use nvt_ipc::Requisicao;

/// Quem atende a requisição.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rota {
    /// Cache da thread de saúde; não entra no ciclo DRS nem pega a trava.
    Saude,
    /// Partidas (só com `medicao-partidas`; sem a feature, `RecursoAusente`).
    Partidas,
    /// Coordenador de usuário (por identidade do pipe).
    Usuario,
    /// R-AR: depende da identidade e da sessão de console
    /// (`crate::recomendadas::aplicar`); sem coordenador cai no núcleo, que
    /// recusa.
    Recomendadas,
    /// Núcleo com driver ([`super::Nucleo::tratar`]) ou o atendimento sem
    /// driver (`crate::laco::tratar_sem_driver`).
    Nucleo,
}

/// Classifica `req` pelo tratador que a atende.
pub(crate) fn rota(req: &Requisicao) -> Rota {
    match req {
        Requisicao::ObterSaude | Requisicao::MarcarInicioVisto { .. } => Rota::Saude,
        Requisicao::ListarPartidas { .. }
        | Requisicao::ObterPartida { .. }
        | Requisicao::ApagarPartidas => Rota::Partidas,
        Requisicao::AgenteConsultar { .. }
        | Requisicao::AgenteRelatar { .. }
        | Requisicao::DefinirAceiteExperimental { .. }
        | Requisicao::ReverterAjusteUsuario { .. }
        | Requisicao::ReverterConfigJogo { .. }
        | Requisicao::MarcarAtualizacaoVista { .. } => Rota::Usuario,
        Requisicao::AplicarRecomendadas => Rota::Recomendadas,
        Requisicao::ObterEstado
        | Requisicao::ObterPlano
        | Requisicao::AplicarAgora
        | Requisicao::RestaurarOriginal
        | Requisicao::DefinirPreferencias { .. }
        | Requisicao::ListarHistorico { .. }
        | Requisicao::TravarSetting { .. }
        | Requisicao::DestravarSetting { .. }
        | Requisicao::ReescanearJogos => Rota::Nucleo,
    }
}

/// A requisição pega a trava do ciclo DRS (`Exclusao::ciclo_drs`)? Só a saúde
/// fica de fora: ela lê o cache da thread de saúde.
pub(crate) fn usa_trava_drs(req: &Requisicao) -> bool {
    rota(req) != Rota::Saude
}

/// Grava no caminho **com driver**: em modo somente leitura o núcleo recusa
/// com "serviço em modo somente leitura: nenhuma gravação é feita".
///
/// Difere de [`grava_sem_driver`] de propósito: sem driver, preferências e
/// travas já respondem `DriverIndisponivel` (nada é gravado), então só
/// `AplicarAgora` e `RestaurarOriginal`, que gravam `servico.json`, precisam
/// da recusa, com o texto curto "serviço em modo somente leitura".
pub(crate) fn grava_com_driver(req: &Requisicao) -> bool {
    match req {
        Requisicao::AplicarAgora
        | Requisicao::RestaurarOriginal
        | Requisicao::DefinirPreferencias { .. }
        | Requisicao::TravarSetting { .. }
        | Requisicao::DestravarSetting { .. } => true,
        Requisicao::ObterEstado
        | Requisicao::ObterPlano
        | Requisicao::ListarHistorico { .. }
        | Requisicao::ReescanearJogos
        | Requisicao::AgenteConsultar { .. }
        | Requisicao::AgenteRelatar { .. }
        | Requisicao::DefinirAceiteExperimental { .. }
        | Requisicao::ReverterAjusteUsuario { .. }
        | Requisicao::ReverterConfigJogo { .. }
        | Requisicao::MarcarAtualizacaoVista { .. }
        | Requisicao::AplicarRecomendadas
        | Requisicao::ObterSaude
        | Requisicao::MarcarInicioVisto { .. }
        | Requisicao::ListarPartidas { .. }
        | Requisicao::ObterPartida { .. }
        | Requisicao::ApagarPartidas => false,
    }
}

/// Grava no caminho **sem driver** (só `servico.json`); ver a diferença em
/// [`grava_com_driver`].
pub(crate) fn grava_sem_driver(req: &Requisicao) -> bool {
    match req {
        Requisicao::AplicarAgora | Requisicao::RestaurarOriginal => true,
        Requisicao::DefinirPreferencias { .. }
        | Requisicao::TravarSetting { .. }
        | Requisicao::DestravarSetting { .. }
        | Requisicao::ObterEstado
        | Requisicao::ObterPlano
        | Requisicao::ListarHistorico { .. }
        | Requisicao::ReescanearJogos
        | Requisicao::AgenteConsultar { .. }
        | Requisicao::AgenteRelatar { .. }
        | Requisicao::DefinirAceiteExperimental { .. }
        | Requisicao::ReverterAjusteUsuario { .. }
        | Requisicao::ReverterConfigJogo { .. }
        | Requisicao::MarcarAtualizacaoVista { .. }
        | Requisicao::AplicarRecomendadas
        | Requisicao::ObterSaude
        | Requisicao::MarcarInicioVisto { .. }
        | Requisicao::ListarPartidas { .. }
        | Requisicao::ObterPartida { .. }
        | Requisicao::ApagarPartidas => false,
    }
}
