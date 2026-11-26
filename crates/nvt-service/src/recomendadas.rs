//! "Aplicar melhorias recomendadas" no serviço (spec `interface-e-saude.md`
//! §4.2, R-AR-01 a R-AR-06; lote L2).
//!
//! [`aplicar`] calcula, para a identidade chamadora, todas as regras
//! experimentais aplicáveis agora e ainda não aceitas:
//!
//! - DRS: só se o SID chamador for o da sessão de console (o DRS é
//!   compartilhado; aceite de outro SID não vale para o driver);
//! - Windows e arquivo de jogo: do próprio SID, pelo plano da sua sessão.
//!
//! "Aplicável" é o estado `Aceitavel` dos avaliadores: regra experimental de
//! condição casada, camada disponível e ao menos um destino fora de trava.
//! Todos os aceites vão numa única gravação de `usuarios.json` (tudo ou nada);
//! se ela falha, nada é aplicado. Depois, a aplicação passa pelo mesmo caminho
//! de `AplicarAgora` (portão do motor): com jogo aberto, os aceites ficam
//! gravados e a resposta sai com `adiada: true`.
//!
//! "Desfazer tudo" (`RestaurarOriginal`) não mexe nos aceites (Q9/D7); a UI
//! pede "Voltar a ajustar" com `AplicarAgora`, que retoma a autonomia e
//! reaplica os aceites.

use std::time::Instant;

use nvt_engine::DriverStore;
use nvt_ipc::pipe::IdentidadeCliente;
use nvt_ipc::{CodigoErro, Requisicao, Resposta};
use tracing::{info, warn};

use crate::nucleo::Nucleo;
use crate::usuario::Coordenador;

/// Trata `AplicarRecomendadas` para `identidade`; `console` é a identidade
/// da sessão de console atual (se houver).
pub(crate) fn aplicar<S: DriverStore>(
    nucleo: &mut Nucleo<'_, S>,
    usuario: &mut Coordenador,
    identidade: &IdentidadeCliente,
    console: Option<&IdentidadeCliente>,
    agora: Instant,
) -> Resposta {
    if !nucleo.pode_gravar() {
        return Resposta::erro(
            CodigoErro::Interno,
            "serviço em modo somente leitura: nenhuma gravação é feita",
        );
    }
    let sid_console = console.map(|c| c.sid.as_str());
    let mut regras = usuario.experimentais_aplicaveis(identidade, agora);
    if sid_console == Some(identidade.sid.as_str()) {
        match nucleo.experimentais_drs_aplicaveis() {
            Ok(drs) => regras.extend(drs),
            Err(e) => warn!("recomendadas sem a camada do driver: {e}"),
        }
    }
    let aceitas = match usuario.aceitar_todas(identidade, &regras) {
        Ok(aceitas) => aceitas,
        Err((codigo, mensagem)) => return Resposta::erro(codigo, mensagem),
    };
    if !aceitas.is_empty() {
        info!("recomendadas aceitas: {}", aceitas.join(", "));
    }
    nucleo.definir_aceites_drs(usuario.aceites_drs(sid_console));
    let resposta = nucleo.tratar(Requisicao::AplicarAgora, agora);
    let servico = nucleo.servico();
    usuario.atualizar_restauracao(servico.geracao_restauracao, servico.autonomia_pausada);
    match resposta {
        Resposta::Plano(p) => Resposta::Recomendadas {
            aceitas,
            adiada: p.adiada.is_some(),
        },
        erro @ Resposta::Erro { .. } => erro,
        outra => {
            warn!("resposta inesperada de aplicar agora: {outra:?}");
            Resposta::Recomendadas {
                aceitas,
                adiada: false,
            }
        }
    }
}
