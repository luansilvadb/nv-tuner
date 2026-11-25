//! Lista os jogos instalados e os fatos detectados (somente leitura).
//!
//! `cargo run -p nvt-detect --example scan`

use nvt_detect::{ServicosSistema, analisar_jogo, varrer_bibliotecas};

fn main() {
    let rel = varrer_bibliotecas();
    for f in &rel.falhas {
        eprintln!("aviso: scanner {} falhou: {}", f.launcher, f.erro);
    }
    let servicos = ServicosSistema::ler();
    println!("{} jogo(s) encontrado(s)\n", rel.jogos.len());
    for j in &rel.jogos {
        let f = analisar_jogo(j, &servicos);
        println!("{} [{}]", j.nome, j.launcher);
        println!("  pasta     : {}", j.pasta.display());
        if let Some(id) = &j.app_id {
            println!("  app_id    : {id}");
        }
        match &f.exe_principal {
            Some(e) => println!("  exe       : {}", e.display()),
            None => println!("  exe       : (nenhum candidato)"),
        }
        let api = f
            .api
            .valor
            .map(|a| format!("{a:?}"))
            .unwrap_or_else(|| "desconhecida".into());
        println!(
            "  API       : {api} [{:?}] {}{}",
            f.api.confianca,
            f.api.evidencia,
            if f.imports_ofuscados {
                " (imports ofuscados)"
            } else {
                ""
            }
        );
        let d = &f.dlss.valor;
        let ver = |x: &Option<nvt_detect::DllNvidia>| {
            x.as_ref()
                .map(|d| d.versao.clone().unwrap_or_else(|| "?".into()))
                .unwrap_or_else(|| "-".into())
        };
        println!(
            "  DLSS      : SR {} | RR {} | FG {} | Streamline {}",
            ver(&d.super_resolution),
            ver(&d.ray_reconstruction),
            ver(&d.frame_generation),
            d.streamline_versao.as_deref().unwrap_or("-")
        );
        println!(
            "  Reflex    : {} [{:?}] {}",
            if f.reflex.valor { "sim" } else { "não" },
            f.reflex.confianca,
            f.reflex.evidencia
        );
        if f.anticheats.is_empty() {
            println!("  Anticheat : nenhum indício");
        }
        for a in &f.anticheats {
            println!(
                "  Anticheat : {}{} [{:?}] {}",
                a.nome,
                if a.kernel { " (kernel)" } else { "" },
                a.confianca,
                a.evidencias.join("; ")
            );
        }
        println!();
    }
}
