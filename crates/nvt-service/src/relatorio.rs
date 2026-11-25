//! Comandos de console: `status` e `dry-run` (somente leitura) e
//! `apply`/`restore-original` (gravam; pelo serviço, se estiver no ar).

use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use nvt_core::Alvo;
use nvt_drs::NvApi;
use nvt_engine::{DrsReal, OrigemPerfil, jogos_da_maquina};
use nvt_ipc::pipe::{Cliente, ConfigCliente};
use nvt_ipc::{ErroIpc, Plano, Requisicao, Resposta, StatusMudanca};

use crate::nucleo::{Config, Nucleo};

type Erro = Box<dyn std::error::Error>;

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

fn sair(r: Result<(), Erro>) -> ExitCode {
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Erro: {e}");
            ExitCode::FAILURE
        }
    }
}

fn sim(b: bool) -> &'static str {
    if b { "sim" } else { "não" }
}

/// `status`: somente leitura.
pub fn status(dir: &Path) -> ExitCode {
    sair(status_interno(dir))
}

fn status_interno(dir: &Path) -> Result<(), Erro> {
    let api = NvApi::load()?;
    let mut n = nucleo(&api, dir, true)?;
    n.varrer(Instant::now());
    let agora = Instant::now();
    let Resposta::Estado(e) = n.tratar(Requisicao::ObterEstado, agora) else {
        return Err("falha ao obter o estado".into());
    };
    match &e.driver {
        Some(d) => println!(
            "Driver: {} (ramo {})",
            d.versao,
            d.ramo.as_deref().unwrap_or("?")
        ),
        None => println!("Driver: indisponível"),
    }
    if let Some(g) = &e.gpu {
        let c = &g.capacidades;
        println!(
            "GPU: {} | arquitetura {} | VRAM {} MB",
            g.nome, g.arquitetura, g.vram_mb
        );
        println!(
            "Capacidades: DLSS {} | geração de quadros {} | Reflex {} | ReBAR {}",
            sim(c.dlss),
            sim(c.geracao_quadros),
            sim(c.reflex),
            sim(c.rebar)
        );
    }
    let p = &e.preferencias;
    println!(
        "Preferências: sincronismo {:?}, qualidade {:?}, limite de FPS {}",
        p.sincronismo,
        p.qualidade,
        p.limite_fps
            .map_or_else(|| "automático".to_string(), |v| v.to_string())
    );
    let est = n.engine().estado();
    println!(
        "Estado: {} | snapshot original {} | {} setting(s) gerida(s), {} travada(s), {} registro(s) no histórico | autonomia {}",
        dir.display(),
        sim(n.engine().armazem().snapshot_original().is_some()),
        est.esperado.len(),
        est.travadas.len(),
        est.historico.len(),
        if n.servico().autonomia_pausada {
            "pausada"
        } else {
            "ativa"
        }
    );
    if let Some(u) = &e.ultima_aplicacao {
        println!(
            "Última aplicação: unix {} ({} mudança(s), {} falha(s))",
            u.quando_unix, u.mudancas, u.falhas
        );
    }

    let diff = n.engine().planejar(n.jogos())?;
    println!("Jogos detectados: {}", diff.jogos.len());
    for j in &diff.jogos {
        println!(
            "  {} → {} → perfil “{}” ({})",
            j.entrada.nome,
            j.entrada.exe,
            j.perfil,
            origem(j.origem)
        );
    }
    let travadas = diff.plano.respeitadas.len();
    let total = diff.mudancas.len() + diff.conformes.len();
    println!(
        "Conformidade: {} de {} setting(s) do plano já no valor; {} pendente(s); {} travada(s) pelo usuário; {} perfil(is) a criar",
        diff.conformes.len(),
        total,
        diff.mudancas.len(),
        travadas,
        diff.criar.len()
    );
    for j in &diff.ignorados {
        println!("  ignorado: {} ({})", j.nome, j.motivo);
    }
    if !diff.reverter.is_empty() {
        println!(
            "Saíram do plano: {} setting(s) voltariam ao original",
            diff.reverter.len()
        );
    }
    let deriva = n.engine().verificar_deriva()?;
    println!(
        "Deriva contra o estado salvo: {:?} ({} divergência(s))",
        deriva.classe,
        deriva.divergencias.len()
    );
    imprimir_avisos(&e.avisos);
    Ok(())
}

fn origem(o: OrigemPerfil) -> &'static str {
    match o {
        OrigemPerfil::Driver => "perfil do driver",
        OrigemPerfil::Usuario => "perfil de usuário",
        OrigemPerfil::CriadoPeloApp => "criado pelo nv-tuner",
        OrigemPerfil::ACriar => "sem perfil: seria criado",
    }
}

/// `dry-run`: somente leitura.
pub fn dry_run(dir: &Path) -> ExitCode {
    sair(dry_run_interno(dir))
}

fn imprimir_plano(p: &Plano) {
    for perfil in &p.perfis {
        println!("\n== {} (perfil “{}”)", perfil.nome_legivel, perfil.perfil);
        for m in &perfil.mudancas {
            let rotulo = match m.status {
                StatusMudanca::Pendente => "MUDA    ",
                StatusMudanca::JaNoValor => "conforme",
                StatusMudanca::Travada => "travada ",
                StatusMudanca::Aplicada => "aplicada",
                StatusMudanca::Falhou => "FALHOU  ",
                StatusMudanca::Recusada => "recusada",
            };
            println!(
                "   {rotulo} {:<24} {} → {}  [{}; impacto {:?}]",
                m.setting,
                m.antes.as_deref().unwrap_or("(não definido)"),
                m.depois.as_deref().unwrap_or("(não definido)"),
                m.regra,
                m.impacto_visual
            );
            println!("            {}", m.nome_legivel);
        }
    }
    if !p.reversoes.is_empty() {
        println!("\n== Saíram do plano (voltam ao original)");
        for r in &p.reversoes {
            let rotulo = match r.status {
                StatusMudanca::Aplicada => "revertida",
                StatusMudanca::Falhou => "FALHOU   ",
                _ => "REVERTE  ",
            };
            println!(
                "   {rotulo} {:<24} em “{}”: {} → {}",
                r.setting,
                r.perfil,
                r.atual.as_deref().unwrap_or("(não definido)"),
                r.original.as_deref().unwrap_or("(não definido)")
            );
            println!("            {}", r.motivo);
        }
    }
    if !p.ignorados.is_empty() {
        println!("\n== Jogos ignorados pelo plano");
        for j in &p.ignorados {
            println!("   {}: {}", j.nome, j.motivo);
        }
    }
}

fn imprimir_avisos(avisos: &[nvt_ipc::Aviso]) {
    for a in avisos {
        let nivel = match a.nivel {
            nvt_ipc::NivelAviso::Info => "info",
            nvt_ipc::NivelAviso::Atencao => "atenção",
            nvt_ipc::NivelAviso::Erro => "ERRO",
        };
        println!("Aviso [{nivel}] {}: {}", a.codigo, a.mensagem);
        if let Some(d) = &a.detalhe {
            println!("    {d}");
        }
    }
}

fn dry_run_interno(dir: &Path) -> Result<(), Erro> {
    let api = NvApi::load()?;
    let mut n = nucleo(&api, dir, true)?;
    n.varrer(Instant::now());
    let diff = n.engine().planejar(n.jogos())?;
    let c = &diff.capacidades;
    println!(
        "GPU: {} | tensor {} | FP8 {} | FG {} | VRAM {} MB | driver {}",
        c.arquitetura,
        sim(c.tem_tensor_cores),
        sim(c.tensor_fp8),
        sim(c.frame_gen),
        c.vram_mb,
        c.driver_version
    );
    println!("Estado: {}", dir.display());
    println!("Jogos: {}", diff.jogos.len());
    for j in &diff.jogos {
        println!(
            "  {} → {} → “{}” ({})",
            j.entrada.nome,
            j.entrada.exe,
            j.perfil,
            origem(j.origem)
        );
    }
    imprimir_plano(&n.plano_ipc(&diff, None));
    for c in &diff.criar {
        println!("\nCriaria o perfil “{}” com o executável {}", c.nome, c.exe);
    }
    for s in &diff.plano.experimentais {
        let alvo = match &s.alvo {
            Alvo::Global => "global".to_string(),
            Alvo::Jogo(j) => j.clone(),
        };
        println!(
            "sugestão experimental (não aplicada): {} em {alvo}",
            s.regra
        );
    }
    for d in &diff.plano.descartes {
        println!(
            "descarte: {} ({})",
            d.regra.as_deref().unwrap_or("preferência"),
            d.motivo
        );
    }
    imprimir_avisos(&n.avisos());
    println!(
        "\nTotal: {} mudança(s) necessária(s), {} conforme(s). Nada foi gravado (dry-run).",
        diff.mudancas.len(),
        diff.conformes.len()
    );
    Ok(())
}

fn eh_admin() -> bool {
    // SAFETY: sem pré-condições.
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

fn mostrar(r: &Resposta, restaurar: bool) -> Result<(), Erro> {
    match r {
        Resposta::Erro { codigo, mensagem } => Err(format!("{codigo:?}: {mensagem}").into()),
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
                    "Restauração concluída (ou nada a restaurar). Autonomia pausada até `apply --yes` ou “Aplicar agora”."
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
                        .and_then(|r| mostrar(&r, restaurar)),
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
        let api = NvApi::load()?;
        let mut n = nucleo(&api, dir, false)?;
        if !restaurar {
            n.varrer(Instant::now());
        }
        let r = n.tratar(req, Instant::now());
        mostrar(&r, restaurar)
    })())
}
