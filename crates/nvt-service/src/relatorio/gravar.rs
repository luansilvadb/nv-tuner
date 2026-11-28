//! `apply`/`restore-original`: gravação pelo serviço ou no próprio processo.

use super::*;

pub(super) fn eh_admin() -> bool {
    // SAFETY: sem pré-condições.
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

/// Pedido adiado pelo portão. Nos dois casos nada foi gravado agora e o
/// comando falha: pelo serviço, com [`SAIDA_ADIADA`] (a ação fica pendente no
/// serviço e roda quando o jogo fechar); no próprio processo, com 1.
///
/// A restauração adiada precisa ser explícita: na desinstalação
/// (`msiexec /x … RESTAURAR_PERFIL=1`) o serviço é removido logo depois e a
/// restauração pendente não roda.
pub(super) fn mostrar_adiada(a: &nvt_ipc::Adiamento, pelo_servico: bool) -> Result<(), Erro> {
    // Sem processos nem lista indisponível não houve adiamento real: não
    // dizer "jogo aberto ()".
    if a.processos.is_empty() && !a.lista_indisponivel {
        return Err(format!(
            "a ação ficou pendente ({:?}) sem jogo aberto detectado; nada foi gravado agora. \
             Veja o log do serviço.",
            a.acao
        )
        .into());
    }
    let motivo = motivo_adiamento(&a.processos, a.lista_indisponivel);
    if !pelo_servico {
        return Err(
            format!("{motivo}: nada foi gravado; feche os jogos e repita o comando").into(),
        );
    }
    let texto = match a.acao {
        nvt_ipc::AcaoAdiada::RestaurarOriginal => format!(
            "{motivo}: a restauração NÃO foi feita agora. O serviço a deixou pendente e a \
             executa quando o jogo fechar. Se estiver desinstalando o nv-tuner, a restauração \
             não vai acontecer: feche os jogos e rode a desinstalação de novo, ou use \
             “Restaurar original” antes de desinstalar."
        ),
        nvt_ipc::AcaoAdiada::AplicarPlano | nvt_ipc::AcaoAdiada::TratarDeriva => format!(
            "{motivo}: nada foi gravado agora. O serviço deixou a ação pendente ({:?}) e a \
             executa quando o jogo fechar.",
            a.acao
        ),
    };
    Err(Box::new(PedidoAdiado(texto)))
}

pub(super) fn mostrar(r: &Resposta, restaurar: bool, pelo_servico: bool) -> Result<(), Erro> {
    match r {
        Resposta::Erro { codigo, mensagem } => Err(format!("{codigo:?}: {mensagem}").into()),
        Resposta::Plano(p) if p.adiada.is_some() => {
            imprimir_plano(p);
            mostrar_adiada(p.adiada.as_ref().expect("conferido acima"), pelo_servico)
        }
        Resposta::Restauracao { adiada: Some(a) } => mostrar_adiada(a, pelo_servico),
        Resposta::Adiada { adiamento } => mostrar_adiada(adiamento, pelo_servico),
        Resposta::Plano(p) => {
            imprimir_plano(p);
            let (mut ok, mut falhas) = (0, 0);
            for m in p.perfis.iter().flat_map(|x| &x.mudancas) {
                match m.status {
                    StatusMudanca::Aplicada => ok += 1,
                    StatusMudanca::Falhou => falhas += 1,
                    _ => {}
                }
            }
            println!("\nAplicadas e verificadas: {ok}; falhas: {falhas}. Autonomia ativa.");
            if falhas > 0 {
                return Err("algumas mudanças não conferiram na releitura".into());
            }
            Ok(())
        }
        _ => {
            if restaurar {
                println!(
                    "Restauração do driver concluída (ou nada a restaurar). A camada do usuário depende do agente. Autonomia pausada até `apply --yes` ou “Aplicar agora”."
                );
            }
            Ok(())
        }
    }
}

/// `apply --yes` / `restore-original --yes`. Com `tentar_ipc`, pede ao
/// serviço em execução (evita que o estado em memória dele sobrescreva o
/// resultado); se o serviço não estiver no ar, faz no próprio processo.
pub fn gravar(dir: &Path, pipe: &str, tentar_ipc: bool, restaurar: bool) -> ExitCode {
    if !eh_admin() {
        eprintln!("Recusado: gravar no driver exige administrador.");
        return ExitCode::FAILURE;
    }
    let req = if restaurar {
        Requisicao::RestaurarOriginal
    } else {
        Requisicao::AplicarAgora
    };
    if tentar_ipc {
        let cfg = ConfigCliente {
            timeout_leitura: crate::ESPERA_CLIENTE,
            ..ConfigCliente::default()
        };
        match Cliente::conectar(pipe, &cfg) {
            Ok(mut c) => {
                println!("Serviço em execução: pedido enviado por {pipe}.");
                return sair(
                    c.requisitar(&req)
                        .map_err(Erro::from)
                        .and_then(|r| mostrar(&r, restaurar, true)),
                );
            }
            Err(ErroIpc::ServicoIndisponivel) => {}
            Err(e) => {
                eprintln!("Erro ao falar com o serviço: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    sair((|| {
        let mut estado = crate::nucleo::EstadoServico::carregar(Some(dir));
        let api = match NvApi::load() {
            Ok(api) => api,
            Err(e) => {
                let resposta =
                    crate::laco::tratar_sem_driver(req, &mut estado, dir, &e.to_string());
                return mostrar(&resposta, restaurar, false);
            }
        };
        let mut n = match nucleo(&api, dir, false) {
            Ok(n) => n,
            Err(e)
                if e.downcast_ref::<nvt_engine::ErroEngine>().is_some_and(|e| {
                    matches!(
                        e,
                        nvt_engine::ErroEngine::Drs(_) | nvt_engine::ErroEngine::SemGpu
                    )
                }) =>
            {
                let resposta =
                    crate::laco::tratar_sem_driver(req, &mut estado, dir, &e.to_string());
                return mostrar(&resposta, restaurar, false);
            }
            Err(e) => return Err(e),
        };
        if let Some(console) = crate::laco::identidade_console() {
            let usuario = crate::usuario::Coordenador::abrir(
                crate::usuario::ArquivoUsuarios(dir.to_path_buf()),
                n.engine().regras(),
                None,
                estado.geracao_restauracao,
            )?;
            n.definir_aceites_drs(usuario.aceites_drs(Some(&console.sid)));
        }
        gravar_local(&mut n, restaurar, Instant::now())
    })())
}

/// `apply`/`restore-original` no próprio processo (serviço fora do ar). Toda
/// gravação passa pelo portão do núcleo.
pub(crate) fn gravar_local<S: DriverStore>(
    n: &mut Nucleo<'_, S>,
    restaurar: bool,
    agora: Instant,
) -> Result<(), Erro> {
    // Também na restauração: os exes detectados entram no portão.
    n.varrer(agora);
    // A CLI só substitui a restauração DRS anterior se puder aplicar agora.
    // A geração de usuário continua valendo para os agentes no próximo logon.
    if !restaurar
        && n.servico().restauracao_pendente_desde_unix.is_some()
        && n.engine().pode_gravar().is_ok()
        && n.descartar_restauracao_pendente()
    {
        println!("restauração pendente descartada: você pediu para aplicar");
    }
    let req = if restaurar {
        Requisicao::RestaurarOriginal
    } else {
        Requisicao::AplicarAgora
    };
    let r = n.tratar(req, agora);
    mostrar(&r, restaurar, false)
}
