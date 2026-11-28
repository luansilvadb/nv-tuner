//! `dry-run`: plano calculado localmente pelo núcleo, sem gravar.

use super::*;

/// `dry-run`: somente leitura.
pub fn dry_run(dir: &Path) -> ExitCode {
    sair(dry_run_interno(dir))
}

pub(super) fn imprimir_plano(p: &Plano) {
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

pub(super) fn imprimir_avisos(avisos: &[nvt_ipc::Aviso]) {
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

pub(super) fn dry_run_interno(dir: &Path) -> Result<(), Erro> {
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
