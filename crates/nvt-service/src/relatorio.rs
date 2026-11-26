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

/// `status`: somente leitura.
pub fn status(dir: &Path, pipe: &str) -> ExitCode {
    for linha in linhas_atualizacao(&ler_estado_atualizacao()) {
        println!("{linha}");
    }
    for linha in linhas_config_jogo(&config_jogo_do_servico(pipe)) {
        println!("{linha}");
    }
    sair(status_interno(dir))
}

/// Estado persistido da atualização (`%ProgramData%\nv-tuner\atualizacao`), só
/// leitura. `Err` com o motivo se não pôde ser lido.
fn ler_estado_atualizacao() -> Result<nvt_atualizacao::EstadoPersistido, String> {
    use std::io::Read;
    const LIMITE: u64 = 64 * 1024;
    let pasta = crate::atualizacao::arquivo::pasta_padrao().map_err(|e| e.to_string())?;
    // Mesma proteção do atualizador: componentes sem reparse point e arquivo aberto
    // sem seguir link, só leitura.
    crate::atualizacao::arquivo::componentes_sem_reparse(&pasta).map_err(|e| e.to_string())?;
    let ler = |nome: &str| -> Result<Option<Vec<u8>>, String> {
        let p = pasta.join(nome);
        match crate::diretorio::abrir_sem_reparse(&p, std::fs::OpenOptions::new().read(true)) {
            Ok(f) => {
                let mut v = Vec::new();
                f.take(LIMITE)
                    .read_to_end(&mut v)
                    .map_err(|e| e.to_string())?;
                Ok(Some(v))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    };
    let principal = ler(crate::atualizacao::ESTADO)?;
    let bak = ler(crate::atualizacao::ESTADO_BAK)?;
    Ok(nvt_atualizacao::EstadoPersistido::carregar(
        principal.as_deref(),
        bak.as_deref(),
        crate::atualizacao::fachadas::versao_instalada(),
    ))
}

/// Linhas do `status` sobre a atualização automática.
pub(crate) fn linhas_atualizacao(
    r: &Result<nvt_atualizacao::EstadoPersistido, String>,
) -> Vec<String> {
    use nvt_atualizacao::Fase;
    let mut v = vec![format!("Versão instalada: {}", env!("CARGO_PKG_VERSION"))];
    let e = match r {
        Ok(e) => e,
        Err(m) => {
            v.push(format!("Atualização: estado indisponível ({m})"));
            return v;
        }
    };
    let fase = match &e.fase {
        Fase::Ocioso => "ociosa".to_owned(),
        Fase::Verificando => "verificando".to_owned(),
        Fase::Baixando { versao, .. } => format!("baixando v{versao}"),
        Fase::Pronta {
            versao,
            adiada_desde_unix,
            ..
        } => match adiada_desde_unix {
            Some(d) => format!("v{versao} pronta, adiada desde unix {d}"),
            None => format!("v{versao} pronta para instalar"),
        },
        Fase::Instalando { de, para, .. } => format!("instalando v{de} → v{para}"),
    };
    let quando = |t: Option<u64>| t.map_or_else(|| "nunca".to_owned(), |t| format!("unix {t}"));
    v.push(format!(
        "Atualização: {fase} | última verificação {} | próxima {} | resultado {}",
        quando(e.ultima_verificacao_unix),
        e.proxima_verificacao_unix
            .map_or_else(|| "não agendada".to_owned(), |t| format!("unix {t}")),
        e.ultimo_resultado
            .map_or_else(|| "nenhum".to_owned(), |r| format!("{r:?}"))
    ));
    if let Some(u) = &e.ultima_atualizacao {
        v.push(format!(
            "  última atualização: v{} → v{} (unix {}, código {})",
            u.de, u.para, u.quando_unix, u.codigo
        ));
    }
    if let Some(f) = &e.ultima_falha {
        let abandonada = if e.abandonadas.contains(&f.versao) {
            ", abandonada"
        } else {
            ""
        };
        v.push(format!(
            "  última falha: v{} (código {}, {} tentativa(s){abandonada})",
            f.versao,
            f.codigo,
            e.falhas.get(&f.versao).copied().unwrap_or(0)
        ));
    }
    v
}

/// Projeção “Arquivo do jogo” pedida ao serviço em execução (`ObterPlano`).
/// O serviço só projeta a config de jogo da própria identidade com agente
/// presente; o `status` nunca lê arquivos do perfil do usuário.
fn config_jogo_do_servico(pipe: &str) -> Result<Vec<nvt_ipc::Jogo>, String> {
    let cfg = ConfigCliente {
        timeout_leitura: crate::ESPERA_CLIENTE,
        ..ConfigCliente::default()
    };
    let mut c = match Cliente::conectar(pipe, &cfg) {
        Ok(c) => c,
        Err(ErroIpc::ServicoIndisponivel) => return Err("serviço fora do ar".into()),
        Err(e) => return Err(e.to_string()),
    };
    match c.requisitar(&Requisicao::ObterPlano) {
        Ok(Resposta::Plano(p)) => Ok(p.jogos),
        Ok(Resposta::Erro { mensagem, .. }) => Err(mensagem),
        Ok(_) => Err("resposta inesperada".into()),
        Err(e) => Err(e.to_string()),
    }
}

/// Linhas do `status` sobre a config de arquivo de jogo: arquivos geridos e
/// pendências por jogo (Steam/jogo aberto), sem caminhos nem contas.
pub(crate) fn linhas_config_jogo(r: &Result<Vec<nvt_ipc::Jogo>, String>) -> Vec<String> {
    use nvt_ipc::OrientacaoConfigJogo as O;
    let jogos = match r {
        Ok(j) => j,
        Err(m) => return vec![format!("Config de jogo: indisponível ({m})")],
    };
    let com_config: Vec<_> = jogos
        .iter()
        .filter_map(|j| j.config_jogo.as_ref().map(|c| (j, c)))
        .collect();
    let geridos: usize = com_config
        .iter()
        .map(|(_, c)| c.arquivos.iter().filter(|a| a.reversivel).count())
        .sum();
    let mut v = vec![format!(
        "Config de jogo: {geridos} arquivo(s) gerido(s) em {} jogo(s) com agente presente",
        com_config.len()
    )];
    for (j, c) in com_config {
        let pendentes = c
            .arquivos
            .iter()
            .flat_map(|a| &a.mudancas)
            .filter(|m| m.status == StatusMudanca::Pendente)
            .count();
        if pendentes == 0 {
            continue;
        }
        let mut motivos = Vec::new();
        if c.orientacoes.contains(&O::FecharSteam) {
            motivos.push("Steam aberta");
        }
        if c.orientacoes
            .iter()
            .any(|o| matches!(o, O::FecharJogo { .. }))
        {
            motivos.push("jogo aberto");
        }
        let motivo = if motivos.is_empty() {
            String::new()
        } else {
            format!(" ({})", motivos.join(", "))
        };
        v.push(format!("  {}: {pendentes} pendente(s){motivo}", j.nome));
    }
    v
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

/// Pedido adiado pelo portão. Nos dois casos nada foi gravado agora e o
/// comando falha: pelo serviço, com [`SAIDA_ADIADA`] (a ação fica pendente no
/// serviço e roda quando o jogo fechar); no próprio processo, com 1.
///
/// A restauração adiada precisa ser explícita: na desinstalação
/// (`msiexec /x … RESTAURAR_PERFIL=1`) o serviço é removido logo depois e a
/// restauração pendente não roda.
fn mostrar_adiada(a: &nvt_ipc::Adiamento, pelo_servico: bool) -> Result<(), Erro> {
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

fn mostrar(r: &Resposta, restaurar: bool, pelo_servico: bool) -> Result<(), Erro> {
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

#[cfg(test)]
mod testes_atualizacao {
    use super::linhas_atualizacao;
    use nvt_atualizacao::{AtualizacaoConcluida, EstadoPersistido, Fase};

    #[test]
    fn status_mostra_estado_da_atualizacao() {
        let e = EstadoPersistido {
            fase: Fase::Baixando {
                versao: "1.2.0".parse().unwrap(),
                msi: "m".into(),
                tamanho: 1,
                sha256: "0".repeat(64),
            },
            ultima_verificacao_unix: Some(100),
            ultima_atualizacao: Some(AtualizacaoConcluida {
                de: "1.0.0".parse().unwrap(),
                para: "1.1.0".parse().unwrap(),
                quando_unix: 50,
                codigo: 0,
            }),
            ..EstadoPersistido::default()
        };
        let l = linhas_atualizacao(&Ok(e)).join("\n");
        assert!(l.contains("baixando v1.2.0"), "{l}");
        assert!(l.contains("última verificação unix 100"), "{l}");
        assert!(l.contains("v1.0.0 → v1.1.0"), "{l}");
        let l = linhas_atualizacao(&Err("negado".into())).join("\n");
        assert!(l.contains("indisponível (negado)"), "{l}");
    }
}

#[cfg(test)]
mod testes_config_jogo {
    use super::linhas_config_jogo;
    use nvt_ipc::{
        AlvoIpc, ArquivoJogoUi, ConfigJogoUi, EstadoBackup, Jogo, MudancaArquivoJogo,
        OrientacaoConfigJogo, OrigemPerfil, ResumoJogo, RotuloCaminho, StatusMudanca, ValorJogoIpc,
    };

    fn jogo(nome: &str, config: Option<ConfigJogoUi>) -> Jogo {
        Jogo {
            nome: nome.into(),
            perfil: nome.into(),
            origem_perfil: OrigemPerfil::Driver,
            launcher: "Steam".into(),
            api: "dx11".into(),
            dlss: None,
            reflex: None,
            anticheat_kernel: None,
            competitivo: None,
            em_execucao: false,
            orientacoes: vec![],
            resumo: ResumoJogo::default(),
            config_jogo: config,
        }
    }

    fn arquivo(reversivel: bool, status: StatusMudanca) -> ArquivoJogoUi {
        ArquivoJogoUi {
            alvo: AlvoIpc::ConfigJogo {
                jogo: "cs2".into(),
                arquivo: "cs2-launch".into(),
            },
            rotulo: RotuloCaminho("%STEAM%\\userdata\\<conta>\\config\\localconfig.vdf".into()),
            instancia: "3f9a0c1b2d4e5f60".into(),
            conta: Some("3f9a0c1b2d4e5f60".into()),
            backup: EstadoBackup::Integro,
            mudancas: vec![MudancaArquivoJogo {
                setting: "cs2_fps_max".into(),
                antes: ValorJogoIpc::TokenLancamento(None),
                depois: ValorJogoIpc::TokenLancamento(Some(141)),
                status,
                motivo: None,
            }],
            reversivel,
        }
    }

    #[test]
    fn status_resume_config_de_jogo_sem_caminhos_nem_contas() {
        let jogos = vec![
            jogo(
                "Counter-Strike 2",
                Some(ConfigJogoUi {
                    arquivos: vec![
                        arquivo(true, StatusMudanca::Aplicada),
                        arquivo(false, StatusMudanca::Pendente),
                    ],
                    orientacoes: vec![OrientacaoConfigJogo::FecharSteam],
                    experimentais: vec![],
                }),
            ),
            jogo("Sem base", None),
        ];
        let l = linhas_config_jogo(&Ok(jogos)).join("\n");
        assert!(l.contains("1 arquivo(s) gerido(s) em 1 jogo(s)"), "{l}");
        assert!(
            l.contains("Counter-Strike 2: 1 pendente(s) (Steam aberta)"),
            "{l}"
        );
        assert!(
            !l.contains('%') && !l.contains("userdata") && !l.contains("3f9a"),
            "{l}"
        );
        let l = linhas_config_jogo(&Err("serviço fora do ar".into())).join("\n");
        assert!(l.contains("indisponível (serviço fora do ar)"), "{l}");
    }
}
