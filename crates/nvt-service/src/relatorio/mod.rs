//! Comandos de console: `status` e `dry-run` (somente leitura) e
//! `apply`/`restore-original` (gravam; pelo serviço, se estiver no ar).

use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use nvt_core::Alvo;
use nvt_drs::NvApi;
use nvt_engine::{DriverStore, DrsReal, OrigemPerfil, jogos_da_maquina};
use nvt_ipc::pipe::{Cliente, ConfigCliente};
use nvt_ipc::{ErroIpc, Plano, Requisicao, Resposta, StatusMudanca};

use crate::nucleo::{Config, Nucleo};

type Erro = Box<dyn std::error::Error>;

mod dry_run;
mod gravar;
mod status;

pub(crate) use dry_run::*;
pub(crate) use gravar::*;
pub(crate) use status::*;
fn varredor<'a>() -> crate::nucleo::Varredor<'a> {
    Box::new(|| {
        let (jogos, avisos) = jogos_da_maquina();
        for a in avisos {
            eprintln!("aviso: {a}");
        }
        jogos
    })
}

fn nucleo<'a>(
    api: &'a NvApi,
    dir: &Path,
    somente_leitura: bool,
) -> Result<Nucleo<'a, DrsReal<'a>>, Erro> {
    Ok(Nucleo::novo(
        Box::new(move || Ok(DrsReal::abrir(api)?)),
        varredor(),
        Box::new(|| None),
        Some(dir.to_path_buf()),
        Config {
            somente_leitura,
            ..Config::default()
        },
        Instant::now(),
    )?)
}

/// Código de saída de `apply`/`restore-original` quando o serviço aceitou o
/// pedido, mas deixou a gravação pendente (jogo aberto ou lista de processos
/// indisponível): nada foi gravado agora.
pub const SAIDA_ADIADA: u8 = 3;

/// O serviço deixou o pedido pendente (ver [`SAIDA_ADIADA`]).
#[derive(Debug)]
struct PedidoAdiado(String);

impl std::fmt::Display for PedidoAdiado {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PedidoAdiado {}

/// Código de saída de um resultado: 0, [`SAIDA_ADIADA`] ou 1.
fn codigo_saida(r: &Result<(), Erro>) -> u8 {
    match r {
        Ok(()) => 0,
        Err(e) if e.is::<PedidoAdiado>() => SAIDA_ADIADA,
        Err(_) => 1,
    }
}

fn sair(r: Result<(), Erro>) -> ExitCode {
    let codigo = codigo_saida(&r);
    if let Err(e) = &r {
        // stderr: o log do MSI (WixQuietExec64, msiexec /l*v) registra o motivo.
        if codigo == SAIDA_ADIADA {
            eprintln!("Pendente (código de saída {SAIDA_ADIADA}): {e}");
        } else {
            eprintln!("Erro: {e}");
        }
    }
    ExitCode::from(codigo)
}

/// Por que o portão adiou: os processos vigiados ou a lista indisponível.
fn motivo_adiamento(processos: &[String], lista_indisponivel: bool) -> String {
    if lista_indisponivel {
        "não foi possível verificar se há jogo aberto (a lista de processos do sistema não pôde \
         ser lida)"
            .to_owned()
    } else {
        format!("jogo aberto ({})", processos.join(", "))
    }
}

fn sim(b: bool) -> &'static str {
    if b { "sim" } else { "não" }
}

fn origem(o: OrigemPerfil) -> &'static str {
    match o {
        OrigemPerfil::Driver => "perfil do driver",
        OrigemPerfil::Usuario => "perfil de usuário",
        OrigemPerfil::CriadoPeloApp => "criado pelo nv-tuner",
        OrigemPerfil::ACriar => "sem perfil: seria criado",
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use nvt_ipc::{AcaoAdiada, Adiamento};

    fn adiamento(acao: AcaoAdiada, lista_indisponivel: bool) -> Adiamento {
        Adiamento {
            processos: if lista_indisponivel {
                vec![]
            } else {
                vec!["cs2.exe".into()]
            },
            desde_unix: 1,
            acao,
            lista_indisponivel,
        }
    }

    /// Desinstalação com `RESTAURAR_PERFIL=1` e jogo aberto: o comando não
    /// pode sair com 0 (a restauração não foi feita) e o motivo vai para o
    /// stderr, que o log do MSI registra.
    #[test]
    fn restauracao_adiada_pelo_servico_sai_com_codigo_proprio() {
        let r = Resposta::Restauracao {
            adiada: Some(adiamento(AcaoAdiada::RestaurarOriginal, false)),
        };
        let res = mostrar(&r, true, true);
        assert_eq!(codigo_saida(&res), SAIDA_ADIADA);
        let texto = res.unwrap_err().to_string();
        assert!(texto.starts_with("jogo aberto (cs2.exe)"), "{texto}");
        assert!(texto.contains("NÃO foi feita"), "{texto}");
        assert!(texto.contains("feche os jogos e rode a desinstalação de novo"));
        assert!(texto.contains("“Restaurar original” antes de desinstalar"));

        let feita = mostrar(&Resposta::Restauracao { adiada: None }, true, true);
        assert_eq!(codigo_saida(&feita), 0);
    }

    /// Pendência sem processos e sem lista indisponível não é jogo aberto:
    /// a mensagem não pode dizer "jogo aberto ()".
    #[test]
    fn pendencia_sem_adiamento_real_nao_diz_jogo_aberto() {
        let mut a = adiamento(AcaoAdiada::RestaurarOriginal, false);
        a.processos.clear();
        for pelo_servico in [false, true] {
            let res = mostrar(
                &Resposta::Restauracao {
                    adiada: Some(a.clone()),
                },
                true,
                pelo_servico,
            );
            assert_eq!(codigo_saida(&res), 1);
            let texto = res.unwrap_err().to_string();
            assert!(!texto.contains("jogo aberto ("), "{texto}");
        }
    }

    #[test]
    fn aplicacao_adiada_e_lista_indisponivel_tambem_nao_saem_com_zero() {
        let mut p = Plano {
            sistema: Default::default(),
            sugestoes_driver: Vec::new(),
            gerado_unix: 1,
            perfis: vec![],
            reversoes: vec![],
            ignorados: vec![],
            jogos: vec![],
            adiada: Some(adiamento(AcaoAdiada::AplicarPlano, true)),
        };
        let res = mostrar(&Resposta::Plano(p.clone()), false, true);
        assert_eq!(codigo_saida(&res), SAIDA_ADIADA);
        let texto = res.unwrap_err().to_string();
        assert!(texto.contains("não foi possível verificar se há jogo aberto"));
        assert!(
            !texto.contains("lista de processos indisponível)"),
            "{texto}"
        );
        // No próprio processo (sem serviço) também falha, com 1.
        let res = mostrar(&Resposta::Plano(p.clone()), false, false);
        assert_eq!(codigo_saida(&res), 1);
        p.adiada = None;
        assert_eq!(codigo_saida(&mostrar(&Resposta::Plano(p), false, true)), 0);
    }
}
