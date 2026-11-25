//! Pipeline real em modo **somente leitura**: detecta GPU e jogos, cruza com o
//! DRS, avalia as regras e mostra o diff que seria aplicado. Não chama
//! `aplicar`, `tratar_deriva` nem `restaurar`, e não grava estado em disco.
//!
//! ```text
//! cargo run -p nvt-engine --example dry_run
//! ```

use std::process::ExitCode;

use nvt_core::Alvo;
use nvt_drs::NvApi;
use nvt_engine::{
    Armazem, Diff, DrsReal, Engine, MudancaPlanejada, OrigemPerfil, jogos_da_maquina,
};

fn valor(def: Option<&nvt_core::DefSetting>, v: Option<u32>) -> String {
    match v {
        None => "(não definido)".into(),
        Some(v) => match def.and_then(|d| d.nome_do_valor(v)) {
            Some(n) => format!("{n} ({v:#X})"),
            None => format!("{v} ({v:#X})"),
        },
    }
}

fn linha(e: &Engine<DrsReal<'_>>, m: &MudancaPlanejada) -> String {
    let def = e.regras().catalogo.get(&m.mudanca.setting);
    format!(
        "{:<28} {} → {}  [{}]",
        m.mudanca.setting,
        valor(def, m.mudanca.valor_anterior),
        valor(def, Some(m.mudanca.valor)),
        m.mudanca.origem.regra().unwrap_or("preferência")
    )
}

fn alvo(e: &Engine<DrsReal<'_>>, diff: &Diff, titulo: &str, a: &Alvo) {
    let n_mud = diff.mudancas_de(a).count();
    let n_conf = diff.conformes_de(a).count();
    println!("\n== {titulo}: {n_mud} mudança(s), {n_conf} conforme(s)");
    for m in diff.mudancas_de(a) {
        println!("   MUDA     {}", linha(e, m));
    }
    for m in diff.conformes_de(a) {
        println!("   conforme {}", linha(e, m));
    }
    for r in diff.plano.respeitadas.iter().filter(|r| &r.alvo == a) {
        println!("   travada  {} (escolha do usuário)", r.mudanca.setting);
    }
    for s in diff.plano.experimentais.iter().filter(|s| &s.alvo == a) {
        println!("   sugestão {} (experimental, não aplicada)", s.regra);
    }
    for d in diff.plano.descartes.iter().filter(|d| &d.alvo == a) {
        println!(
            "   descarte {}: {}",
            d.regra.as_deref().unwrap_or("preferência"),
            d.motivo
        );
    }
}

fn executar() -> Result<(), Box<dyn std::error::Error>> {
    let api = NvApi::load()?;
    let store = DrsReal::abrir(&api)?;
    let dir = Armazem::diretorio_padrao();
    let armazem = if dir.join("estado.json").exists() {
        println!("Estado: {}", dir.display());
        Armazem::abrir_somente_leitura(&dir)?
    } else {
        println!(
            "Estado: nenhum em {} (estado vazio, em memória)",
            dir.display()
        );
        Armazem::em_memoria()
    };
    let engine = Engine::new(store, armazem)?;

    let (jogos, avisos) = jogos_da_maquina();
    for a in &avisos {
        eprintln!("aviso: {a}");
    }
    let diff = engine.planejar(&jogos)?;
    let c = &diff.capacidades;
    println!(
        "GPU: {} | tensor {} | FP8 {} | FG {} | VRAM {} MB | driver {}",
        c.arquitetura, c.tem_tensor_cores, c.tensor_fp8, c.frame_gen, c.vram_mb, c.driver_version
    );
    println!("Jogos: {}", diff.jogos.len());
    for j in &diff.jogos {
        let origem = match j.origem {
            OrigemPerfil::Driver => "perfil do driver",
            OrigemPerfil::Usuario => "perfil de usuário",
            OrigemPerfil::CriadoPeloApp => "perfil criado pelo app",
            OrigemPerfil::ACriar => "sem perfil (seria criado)",
        };
        println!(
            "  {} → exe {} → “{}” ({origem}); API {:?}, DLSS {}, anticheat kernel {:?}",
            j.entrada.nome,
            j.entrada.exe,
            j.perfil,
            j.entrada.api,
            j.entrada
                .dlss
                .map_or_else(|| "não".to_string(), |v| v.to_string()),
            j.entrada.anticheat_kernel
        );
    }

    alvo(&engine, &diff, "Global (Base Profile)", &Alvo::Global);
    for j in &diff.jogos {
        alvo(
            &engine,
            &diff,
            &format!("{} (perfil “{}”)", j.entrada.nome, j.perfil),
            &Alvo::Jogo(j.perfil.clone()),
        );
    }
    for c in &diff.criar {
        println!("\nCriaria perfil “{}” com o executável {}", c.nome, c.exe);
    }
    println!(
        "\nTotal: {} mudança(s) necessária(s), {} conforme(s). Nada foi gravado (dry-run).",
        diff.mudancas.len(),
        diff.conformes.len()
    );

    let deriva = engine.verificar_deriva()?;
    println!(
        "Deriva: {:?} ({} divergência(s) contra o estado esperado salvo)",
        deriva.classe,
        deriva.divergencias.len()
    );
    Ok(())
}

fn main() -> ExitCode {
    match executar() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Erro: {e}");
            ExitCode::FAILURE
        }
    }
}
