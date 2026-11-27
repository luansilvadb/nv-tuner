//! Falha fechada da lista de processos (portão e detecção), jogo de exe
//! desconhecido aberto na primeira varredura (F1-09) e teste de aceite A7
//! (spec `docs/spec/competitivo.md` §7.8 item 7).

mod comum;

use std::cell::Cell;
use std::path::{Path, PathBuf};

use nvt_core::BaseJogos;
use nvt_detect::facts::pe::recolher_aberturas;
use nvt_detect::{
    DetectedGame, FonteProcessos, Launcher, Processo, ProcessosFalsos, ServicosSistema,
};
use nvt_engine::{
    Armazem, DetectorJogos, DriverFalso, Engine, ErroEngine, JogoEntrada, LISTA_INDISPONIVEL,
    Portao,
};

const EXE_WARDOGS: &str = "WardogsClient-Win64-Shipping.exe";

fn dir_teste(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nvt-segura-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ── Falha fechada no portão ─────────────────────────────────────────────────

#[test]
fn lista_de_processos_ilegivel_nao_grava_nada() {
    let d = DriverFalso::turing();
    d.definir_padrao(0x00A8_79CF, 0x6092_5292);
    d.adicionar_perfil_predefinido("Wardogs", &[EXE_WARDOGS], &[]);
    let mut w = JogoEntrada::new("WARDOGS", EXE_WARDOGS);
    w.dlss = Some(nvt_core::Versao::new(310, 6, 0, 0));
    let mut e = Engine::new(d.clone(), Armazem::em_memoria())
        .unwrap()
        .com_portao(Portao::new(ProcessosFalsos::falhando()));
    let diff = e.planejar(&[w]).unwrap();
    assert!(!diff.vazio());
    match e.aplicar(&diff, true) {
        Err(ErroEngine::Adiada(a)) => {
            assert!(a.lista_indisponivel);
            assert_eq!(a.processos, [LISTA_INDISPONIVEL]);
        }
        outro => panic!("esperava Adiada, veio {outro:?}"),
    }
    assert!(e.pode_gravar().is_err());
    assert_eq!(d.gravacoes(), 0);
    assert_eq!(d.salvamentos(), 0);
}

// ── Falha fechada na detecção ───────────────────────────────────────────────

fn jogo_fora_da_base(pasta: &Path) -> DetectedGame {
    // Sem AppID da base e sem exe indicado: exe desconhecido.
    DetectedGame::new(Launcher::Steam, "Jogo Indie", pasta).with_app_id("999999")
}

fn preparar_pasta(nome: &str) -> PathBuf {
    let dir = dir_teste(nome);
    std::fs::write(dir.join("Indie.exe"), b"MZ").unwrap();
    std::fs::write(dir.join("nvngx_dlss.dll"), b"MZ").unwrap();
    dir
}

#[test]
fn servico_encontra_exe_ativo_fora_da_listagem_limitada() {
    let dir = preparar_pasta("servico-truncado");
    let limite = nvt_detect::facts::arquivos::PROFUNDIDADE_MAX;
    let profunda = (0..=limite).fold(dir.clone(), |pasta, _| pasta.join("d"));
    std::fs::create_dir_all(&profunda).unwrap();
    std::fs::write(profunda.join("Ativo.exe"), b"MZ").unwrap();
    let mut jogo = jogo_fora_da_base(&dir);
    jogo.exe_indicado = Some(dir.join("Indie.exe"));

    let resultado = DetectorJogos::embutido().detectar_jogos(
        &[jogo],
        &[],
        &ServicosSistema::default(),
        &ProcessosFalsos::com_nomes(&["Ativo.exe"]),
    );
    let aberturas = recolher_aberturas();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(aberturas.is_empty(), "{aberturas:?}");
    assert_eq!(resultado.processos_em_execucao, ["ativo.exe"]);
    assert_eq!(resultado.jogos.len(), 1);
    assert!(!resultado.jogos[0].em_execucao);
    assert!(resultado.jogos[0].dlss.is_none());
    assert!(
        resultado
            .avisos
            .iter()
            .any(|a| a.contains("análise adiada"))
    );
}

fn preparar_xbox(raiz: &Path, content: bool) -> PathBuf {
    let base = raiz.join("Pasta Xbox");
    let pasta = if content { base.join("Content") } else { base };
    std::fs::create_dir_all(&pasta).unwrap();
    std::fs::write(pasta.join("Xbox.exe"), b"MZ").unwrap();
    let config = pasta.join("MicrosoftGame.config");
    std::fs::write(
        &config,
        r#"<Identity Name="Identidade.Xbox"/><ShellVisuals DefaultDisplayName="Jogo Xbox"/><Executable Name="Xbox.exe"/>"#,
    )
    .unwrap();
    config
}

#[test]
fn xbox_aberto_nao_le_config_nem_pe() {
    let raiz = dir_teste("xbox-aberto");
    preparar_xbox(&raiz, true);
    let jogos = nvt_detect::launchers::xbox::varrer_dir(&raiz);
    let resultado = DetectorJogos::embutido().detectar_jogos(
        &jogos,
        &[],
        &ServicosSistema::default(),
        &ProcessosFalsos::com_nomes(&["Xbox.exe"]),
    );
    let aberturas = recolher_aberturas();
    std::fs::remove_dir_all(raiz).unwrap();
    assert!(aberturas.is_empty(), "{aberturas:?}");
    assert!(resultado.jogos.is_empty());
}

#[test]
fn xbox_fechado_le_config_e_reaproveita_fatos_ao_abrir() {
    for content in [true, false] {
        let raiz = dir_teste(if content { "xbox-content" } else { "xbox-raiz" });
        let config = preparar_xbox(&raiz, content);
        let jogos = nvt_detect::launchers::xbox::varrer_dir(&raiz);
        let servicos = ServicosSistema::default();
        let mut detector = DetectorJogos::embutido();
        let fechado =
            detector.detectar_jogos_local(&jogos, &[], &servicos, &ProcessosFalsos::default());
        assert_eq!(fechado.jogos[0].nome, "Jogo Xbox");
        assert_eq!(fechado.jogos[0].exe, "Xbox.exe");
        assert_eq!(recolher_aberturas().first(), Some(&config));

        let aberto = detector.detectar_jogos_local(
            &jogos,
            &[],
            &servicos,
            &ProcessosFalsos::com_nomes(&["Xbox.exe"]),
        );
        assert!(recolher_aberturas().is_empty());
        assert!(aberto.jogos[0].em_execucao);
        assert_eq!(aberto.jogos[0].nome, fechado.jogos[0].nome);
        assert_eq!(aberto.jogos[0].exe, fechado.jogos[0].exe);
        std::fs::remove_dir_all(raiz).unwrap();
    }
}

#[test]
fn lista_ilegivel_nao_le_nenhuma_pasta_e_reaproveita_fatos() {
    let dir = preparar_pasta("lista-ilegivel");
    let jogos = [jogo_fora_da_base(&dir)];
    let sv = ServicosSistema::default();
    let fechado = ProcessosFalsos::com_nomes(&["explorer.exe"]);
    let mut det = DetectorJogos::embutido();
    // Duas varreduras com a lista lida: a primeira adia (exe desconhecido),
    // a segunda lê a pasta.
    det.detectar_jogos(&jogos, &[], &sv, &fechado);
    let r = det.detectar_jogos(&jogos, &[], &sv, &fechado);
    assert_eq!(r.jogos.len(), 1, "{:?}", r.avisos);
    let antes = r.jogos[0].clone();
    assert_eq!(antes.exe, "Indie.exe");
    assert!(antes.dlss.is_some());

    // Lista ilegível: a pasta não é lida (se fosse, o jogo perderia o exe).
    std::fs::remove_dir_all(&dir).unwrap();
    let r = det.detectar_jogos(&jogos, &[], &sv, &ProcessosFalsos::falhando());
    assert!(r.lista_processos_indisponivel);
    assert!(r.processos_em_execucao.is_empty());
    assert_eq!(r.jogos.len(), 1, "{:?}", r.avisos);
    assert_eq!(r.jogos[0].exe, antes.exe);
    assert_eq!(r.jogos[0].dlss, antes.dlss, "fatos anteriores");
    assert!(
        r.avisos
            .iter()
            .any(|a| a.contains("lista de processos indisponível"))
    );
}

#[test]
fn lista_ilegivel_sem_fatos_anteriores_nao_le_a_pasta_de_jogo_conhecido() {
    let dir = dir_teste("lista-ilegivel-cs2");
    std::fs::write(dir.join("cs2.exe"), b"MZ").unwrap();
    std::fs::write(dir.join("nvngx_dlss.dll"), b"MZ").unwrap();
    let mut j = DetectedGame::new(Launcher::Steam, "Counter-Strike 2", &dir).with_app_id("730");
    j.exe_indicado = Some(dir.join("cs2.exe"));
    let mut det = DetectorJogos::embutido();
    let r = det.detectar_jogos(
        &[j],
        &[],
        &ServicosSistema::default(),
        &ProcessosFalsos::falhando(),
    );
    assert_eq!(r.jogos.len(), 1);
    let e = &r.jogos[0];
    assert_eq!(e.exe, "cs2.exe");
    assert_eq!(e.dlss, None, "pasta não lida: fatos desconhecidos");
    assert_eq!(e.id_jogo.as_ref().map(|i| i.as_str()), Some("cs2"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ── F1-09: jogo de exe desconhecido ─────────────────────────────────────────

#[test]
fn jogo_fora_da_base_aberto_na_primeira_varredura_nao_tem_a_pasta_lida() {
    let dir = preparar_pasta("primeira-varredura");
    let jogos = [jogo_fora_da_base(&dir)];
    let sv = ServicosSistema::default();
    // Serviço recém-iniciado com o jogo (exe desconhecido) já aberto.
    let aberto = ProcessosFalsos::com_nomes(&["explorer.exe", "Indie.exe"]);
    let mut det = DetectorJogos::embutido();
    let r = det.detectar_jogos(&jogos, &[], &sv, &aberto);
    assert!(r.jogos.is_empty(), "pasta lida na primeira varredura");
    assert!(r.avisos.iter().any(|a| a.contains("análise adiada")));

    // Com outro jogo vigiado aberto, continua sem ler.
    let cs2 = ProcessosFalsos::com_nomes(&["explorer.exe", "Indie.exe", "cs2.exe"]);
    let r = det.detectar_jogos(&jogos, &[], &sv, &cs2);
    assert!(r.jogos.is_empty());
    // Anticheat que só existe durante o jogo também impede a leitura.
    let eac = ProcessosFalsos::com_nomes(&["explorer.exe", "EasyAntiCheat_EOS.exe"]);
    let r = det.detectar_jogos(&jogos, &[], &sv, &eac);
    assert!(r.jogos.is_empty());

    // Varredura seguinte sem nada vigiado: lê a pasta e passa a conhecer o
    // exe; daí em diante o jogo aberto é reconhecido pelo nome.
    let fechado = ProcessosFalsos::com_nomes(&["explorer.exe"]);
    let r = det.detectar_jogos(&jogos, &[], &sv, &fechado);
    assert_eq!(r.jogos.len(), 1, "{:?}", r.avisos);
    assert_eq!(r.jogos[0].exe, "Indie.exe");
    std::fs::remove_dir_all(&dir).unwrap();
    let r = det.detectar_jogos(&jogos, &[], &sv, &aberto);
    assert_eq!(r.jogos.len(), 1);
    assert!(r.jogos[0].em_execucao);
    assert!(r.jogos[0].dlss.is_some(), "fatos anteriores");
}

#[test]
fn jogo_de_exe_desconhecido_nao_e_lido_na_primeira_varredura_mesmo_sem_processo() {
    let dir = preparar_pasta("primeira-vazia");
    let jogos = [jogo_fora_da_base(&dir)];
    let mut det = DetectorJogos::embutido();
    let r = det.detectar_jogos(
        &jogos,
        &[],
        &ServicosSistema::default(),
        &ProcessosFalsos::com_nomes(&["explorer.exe"]),
    );
    assert!(r.jogos.is_empty());
    assert_eq!(r.adiadas_primeira_passada, ["Jogo Indie"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn deteccao_local_encontra_jogo_fora_da_base_em_uma_chamada() {
    let dir = preparar_pasta("local-fechado");
    let mut det = DetectorJogos::embutido();
    let r = det.detectar_jogos_local(
        &[jogo_fora_da_base(&dir)],
        &[],
        &ServicosSistema::default(),
        &ProcessosFalsos::com_nomes(&["explorer.exe"]),
    );
    assert_eq!(r.jogos.len(), 1, "{:?}", r.avisos);
    assert_eq!(r.jogos[0].exe, "Indie.exe");
    assert!(r.jogos[0].dlss.is_some());
    assert!(r.avisos.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn deteccao_local_nao_le_arquivos_de_jogo_aberto_fora_da_base() {
    struct JogoIniciadoEntrePassadas(Cell<bool>);

    impl FonteProcessos for JogoIniciadoEntrePassadas {
        fn processos(&self) -> Vec<Processo> {
            if self.0.replace(true) {
                vec![Processo::new("INDIE.EXE", 1, 0)]
            } else {
                Vec::new()
            }
        }
    }

    let dir = preparar_pasta("local-aberto");
    let jogos = [jogo_fora_da_base(&dir)];
    let servicos = ServicosSistema::default();
    let mut det = DetectorJogos::embutido();
    let r = det.detectar_jogos_local(
        &jogos,
        &[],
        &servicos,
        &JogoIniciadoEntrePassadas(Cell::new(false)),
    );
    assert!(r.jogos.is_empty(), "{:?}", r.jogos);
    assert_eq!(r.processos_em_execucao, ["indie.exe"]);
    assert!(recolher_aberturas().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn deteccao_aborta_quando_o_jogo_abre_antes_de_ler_conteudo() {
    struct JogoIniciadoNaLeitura {
        chamadas: Cell<usize>,
        inicia_na: usize,
    }

    impl FonteProcessos for JogoIniciadoNaLeitura {
        fn processos(&self) -> Vec<Processo> {
            let chamada = self.chamadas.get() + 1;
            self.chamadas.set(chamada);
            if chamada == self.inicia_na {
                vec![Processo::new("INDIE.EXE", 1, 0)]
            } else {
                Vec::new()
            }
        }
    }

    let dir = preparar_pasta("local-corrida");
    std::fs::write(dir.join("sl.interposer.dll"), b"MZ").unwrap();
    std::fs::write(dir.join("coreinit.dll"), b"MZ").unwrap();
    let jogos = [jogo_fora_da_base(&dir)];
    let servicos = ServicosSistema::default();

    let arquivos = [
        "Indie.exe",
        "nvngx_dlss.dll",
        "sl.interposer.dll",
        "coreinit.dll",
    ]
    .map(|nome| dir.join(nome));
    // 1-2: passadas do motor. 3: pós-enumeração. 4-7: guardas de cada PE.
    for inicia_na in 3_usize..=7 {
        let fonte = JogoIniciadoNaLeitura {
            chamadas: Cell::new(0),
            inicia_na,
        };
        let r = DetectorJogos::embutido().detectar_jogos_local(&jogos, &[], &servicos, &fonte);
        assert!(r.jogos.is_empty(), "captura {inicia_na}");
        assert_eq!(r.processos_em_execucao, ["indie.exe"]);
        assert_eq!(fonte.chamadas.get(), inicia_na, "verificou após abortar");
        assert_eq!(
            recolher_aberturas(),
            arquivos[..inicia_na.saturating_sub(4)],
            "abertura sem guarda na captura {inicia_na}"
        );
    }

    let mut conhecido = jogos[0].clone();
    conhecido.exe_indicado = Some(dir.join("Indie.exe"));
    let fonte = JogoIniciadoNaLeitura {
        chamadas: Cell::new(0),
        inicia_na: 4, // primeira DLL no caminho do serviço
    };
    let r = DetectorJogos::embutido().detectar_jogos(&[conhecido], &[], &servicos, &fonte);
    assert!(r.jogos[0].em_execucao);
    assert!(r.jogos[0].dlss.is_none());
    assert_eq!(fonte.chamadas.get(), 4);
    assert_eq!(
        recolher_aberturas(),
        arquivos[..1],
        "DLL aberta sem guarda no serviço"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn deteccao_local_omite_jogos_quando_varredura_de_nomes_trunca() {
    let com_exe = preparar_pasta("local-truncado-parcial");
    let sem_exe = preparar_pasta("local-truncado-sem-exe");
    std::fs::remove_file(sem_exe.join("Indie.exe")).unwrap();
    for dir in [&com_exe, &sem_exe] {
        let limite = nvt_detect::facts::arquivos::PROFUNDIDADE_MAX;
        let profunda = (0..limite).fold(dir.clone(), |pasta, _| pasta.join("d"));
        std::fs::create_dir_all(profunda).unwrap();
    }
    let jogos = [jogo_fora_da_base(&com_exe), jogo_fora_da_base(&sem_exe)];
    let mut detector = DetectorJogos::embutido();
    let detectar = |detector: &mut DetectorJogos, fonte: &ProcessosFalsos| {
        detector.detectar_jogos_local(&jogos, &[], &ServicosSistema::default(), fonte)
    };
    let fechados = ProcessosFalsos::com_nomes(&[]);
    assert_eq!(detectar(&mut detector, &fechados).jogos.len(), 1);
    assert!(!recolher_aberturas().is_empty());
    for dir in [&com_exe, &sem_exe] {
        std::fs::create_dir_all(dir.join(vec!["d"; 65].join("\\"))).unwrap();
    }
    for fonte in [
        ProcessosFalsos::com_nomes(&[]),
        ProcessosFalsos::com_nomes(&["Indie.exe"]),
    ] {
        let r = DetectorJogos::embutido().detectar_jogos_local(
            &jogos,
            &[],
            &ServicosSistema::default(),
            &fonte,
        );
        assert!(r.jogos.is_empty(), "{:?}", r.jogos);
        assert_eq!(
            r.avisos
                .iter()
                .filter(|a| a.contains("análise adiada"))
                .count(),
            2
        );
        assert!(recolher_aberturas().is_empty());
    }
    for dir in [com_exe, sem_exe] {
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn servico_adia_leitura_sem_promover_processo_de_nome_nao_confirmado() {
    let dir = preparar_pasta("servico-homonimo");
    std::fs::write(dir.join("Helper.exe"), b"MZ").unwrap();
    let mut jogo = jogo_fora_da_base(&dir);
    let servicos = ServicosSistema::default();
    let fonte = ProcessosFalsos::com_nomes(&["Helper.exe"]);

    let mut det = DetectorJogos::embutido();
    det.detectar_jogos(std::slice::from_ref(&jogo), &[], &servicos, &fonte);
    let r = det.detectar_jogos(std::slice::from_ref(&jogo), &[], &servicos, &fonte);
    assert!(r.jogos.is_empty());
    assert_eq!(r.processos_em_execucao, ["helper.exe"]);
    assert!(recolher_aberturas().is_empty());

    jogo.exe_indicado = Some(dir.join("Indie.exe"));
    let r = det.detectar_jogos(&[jogo], &[], &servicos, &fonte);
    assert_eq!(r.jogos.len(), 1);
    assert_eq!(r.jogos[0].exe, "Indie.exe");
    assert!(!r.jogos[0].em_execucao);
    assert!(recolher_aberturas().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn deteccao_local_repete_apenas_jogos_adiados() {
    #[derive(Default)]
    struct RemoverDllNaPassadaSeguinte {
        chamadas: Cell<usize>,
        remover: Option<(usize, PathBuf)>,
    }

    impl FonteProcessos for RemoverDllNaPassadaSeguinte {
        fn processos(&self) -> Vec<Processo> {
            let chamada = self.chamadas.get() + 1;
            self.chamadas.set(chamada);
            if let Some((quando, dll)) = &self.remover
                && chamada == *quando
            {
                std::fs::remove_file(dll).unwrap();
            }
            Vec::new()
        }
    }

    let pasta_conhecida = preparar_pasta("local-sem-repetir-conhecido");
    let pasta_adiada = preparar_pasta("local-sem-repetir-adiado");
    let mut conhecido = DetectedGame::new(Launcher::Steam, "Conhecido", &pasta_conhecida);
    conhecido.exe_indicado = Some(pasta_conhecida.join("Indie.exe"));
    let servicos = ServicosSistema::default();
    let contagem = RemoverDllNaPassadaSeguinte::default();
    let primeira = DetectorJogos::embutido().detectar_jogos_local(
        std::slice::from_ref(&conhecido),
        &[],
        &servicos,
        &contagem,
    );
    assert!(primeira.jogos[0].dlss.is_some());

    let dll = pasta_conhecida.join("nvngx_dlss.dll");
    let fonte = RemoverDllNaPassadaSeguinte {
        chamadas: Cell::new(0),
        remover: Some((contagem.chamadas.get() + 1, dll.clone())),
    };
    let r = DetectorJogos::embutido().detectar_jogos_local(
        &[conhecido, jogo_fora_da_base(&pasta_adiada)],
        &[],
        &servicos,
        &fonte,
    );
    assert!(!dll.exists(), "a segunda passada não ocorreu");
    assert_eq!(r.jogos.len(), 2, "{:?}", r.avisos);
    assert_eq!(r.jogos[0], primeira.jogos[0]);
    assert!(r.jogos[1].dlss.is_some());
    assert!(r.avisos.is_empty());

    std::fs::remove_dir_all(pasta_conhecida).unwrap();
    std::fs::remove_dir_all(pasta_adiada).unwrap();
}

#[test]
fn deteccao_local_mantem_adiamento_quando_nao_pode_verificar_processos() {
    let dir = preparar_pasta("local-adiado");
    for processos in [
        ProcessosFalsos::falhando(),
        ProcessosFalsos::com_nomes(&["cs2.exe"]),
    ] {
        let mut det = DetectorJogos::embutido();
        let r = det.detectar_jogos_local(
            &[jogo_fora_da_base(&dir)],
            &[],
            &ServicosSistema::default(),
            &processos,
        );
        assert!(r.jogos.is_empty());
        assert!(r.adiadas_primeira_passada.is_empty());
        assert!(r.avisos.iter().any(|a| a.contains("análise adiada")));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

// ── A7 (§7.8 item 7) ────────────────────────────────────────────────────────

#[test]
fn aceite_a7_os_14_jogos_da_base_sao_competitivos_com_orientacoes() {
    let base = BaseJogos::embutida().unwrap();
    assert_eq!(base.jogos().len(), 14);
    let raiz = dir_teste("a7");
    let jogos: Vec<DetectedGame> = base
        .jogos()
        .iter()
        .map(|c| {
            let pasta = raiz.join(c.id.as_str());
            std::fs::create_dir_all(&pasta).unwrap();
            let exe = &c.executaveis[0];
            std::fs::write(pasta.join(exe), b"MZ").unwrap();
            let mut j = DetectedGame::new(Launcher::Steam, c.nome.clone(), &pasta);
            if let Some(id) = c.steam_appid {
                j = j.with_app_id(id.to_string());
            }
            j.exe_indicado = Some(pasta.join(exe));
            j
        })
        .collect();
    let mut det = DetectorJogos::embutido();
    let r = det.detectar_jogos(
        &jogos,
        &[],
        &ServicosSistema::default(),
        &ProcessosFalsos::com_nomes(&["explorer.exe"]),
    );
    assert_eq!(r.jogos.len(), 14, "{:?}", r.avisos);

    let e = comum::motor(DriverFalso::turing(), Armazem::em_memoria());
    let res = e.resolver_jogos(&r.jogos).unwrap();
    assert_eq!(res.len(), 14);
    for c in base.jogos() {
        let j = res
            .iter()
            .find(|j| j.fatos.jogo_base.as_ref() == Some(&c.id))
            .unwrap_or_else(|| panic!("{} não casou com a base", c.id.as_str()));
        assert!(j.fatos.competitivo, "{}", c.id.as_str());
        assert!(!j.fatos.orientacoes.is_empty(), "{}", c.id.as_str());
    }
    for id in ["valorant", "lol", "marvel-rivals", "cod"] {
        let c = base.jogo(id).unwrap_or_else(|| panic!("{id} na base"));
        assert!(!c.config.grava(), "{id}: política não pode gravar");
    }
    let _ = std::fs::remove_dir_all(&raiz);
}

#[test]
fn servico_analisa_jogo_fechado_com_pasta_profunda() {
    let dir = preparar_pasta("servico-profundo-fechado");
    std::fs::create_dir_all(dir.join(vec!["d"; 24].join("\\"))).unwrap();
    let mut jogo = jogo_fora_da_base(&dir);
    jogo.exe_indicado = Some(dir.join("Indie.exe"));
    let resultado = DetectorJogos::embutido().detectar_jogos(
        &[jogo],
        &[],
        &ServicosSistema::default(),
        &ProcessosFalsos::com_nomes(&[]),
    );
    let aberturas = recolher_aberturas();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(resultado.jogos.len(), 1);
    assert_eq!(
        aberturas,
        [dir.join("Indie.exe"), dir.join("nvngx_dlss.dll")]
    );
}

#[test]
fn falha_de_enumeracao_adia_no_servico_e_no_modo_local() {
    use std::os::windows::fs::OpenOptionsExt;

    let dir = preparar_pasta("enumeracao-ilegivel");
    let subpasta = dir.join("ilegivel");
    std::fs::create_dir(&subpasta).unwrap();
    let exclusivo = std::fs::OpenOptions::new()
        .access_mode(1) // FILE_LIST_DIRECTORY
        .share_mode(0)
        .custom_flags(0x0200_0000) // FILE_FLAG_BACKUP_SEMANTICS
        .open(&subpasta)
        .unwrap();
    let mut jogo = jogo_fora_da_base(&dir);
    jogo.exe_indicado = Some(dir.join("Indie.exe"));
    let jogos = [jogo];
    let servicos = ServicosSistema::default();
    let fonte = ProcessosFalsos::com_nomes(&[]);
    for r in [
        DetectorJogos::embutido().detectar_jogos(&jogos, &[], &servicos, &fonte),
        DetectorJogos::embutido().detectar_jogos_local(&jogos, &[], &servicos, &fonte),
    ] {
        assert!(r.jogos.is_empty(), "{:?}", r.jogos);
        assert!(
            r.avisos
                .iter()
                .any(|a| a.contains("listagem de arquivos incompleta"))
        );
        assert!(recolher_aberturas().is_empty());
    }
    drop(exclusivo);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn aviso_de_adiamento_so_reaparece_quando_o_motivo_volta() {
    let dir = preparar_pasta("aviso-adiado");
    let profunda = dir.join(vec!["d"; 65].join("\\"));
    std::fs::create_dir_all(&profunda).unwrap();
    let mut jogo = jogo_fora_da_base(&dir);
    jogo.exe_indicado = Some(dir.join("Indie.exe"));
    let jogos = [jogo];
    let servicos = ServicosSistema::default();
    let fonte = ProcessosFalsos::com_nomes(&[]);
    let mut detector = DetectorJogos::embutido();
    let avisos = |detector: &mut DetectorJogos| {
        detector
            .detectar_jogos(&jogos, &[], &servicos, &fonte)
            .avisos
    };
    assert_eq!(avisos(&mut detector).len(), 1);
    assert!(avisos(&mut detector).is_empty());
    std::fs::remove_dir_all(dir.join("d")).unwrap();
    assert!(avisos(&mut detector).is_empty());
    std::fs::create_dir_all(profunda).unwrap();
    assert_eq!(avisos(&mut detector).len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn xbox_adia_content_em_juncao_sem_abrir_arquivos() {
    let raiz = dir_teste("xbox-juncao");
    let bibliotecas = raiz.join("XboxGames");
    let base = bibliotecas.join("Pasta Xbox");
    std::fs::create_dir_all(&base).unwrap();
    let alvo = preparar_xbox(&raiz, false);
    let juncao = base.join("Content");
    let status = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&juncao)
        .arg(alvo.parent().unwrap())
        .output();
    if !status.as_ref().is_ok_and(|o| o.status.success()) {
        eprintln!(
            "Teste pulado: cmd /c mklink /J não conseguiu criar a junção Content: {status:?}"
        );
        std::fs::remove_dir_all(raiz).unwrap();
        return;
    }
    let resultado = DetectorJogos::embutido().detectar_jogos(
        &nvt_detect::launchers::xbox::varrer_dir(&bibliotecas),
        &[],
        &ServicosSistema::default(),
        &ProcessosFalsos::com_nomes(&[]),
    );
    let aberturas = recolher_aberturas();
    std::fs::remove_dir(juncao).unwrap();
    std::fs::remove_dir_all(raiz).unwrap();
    assert!(aberturas.is_empty(), "{aberturas:?}");
    assert!(resultado.jogos.is_empty());
    assert!(
        resultado
            .avisos
            .iter()
            .any(|a| a.contains("listagem de arquivos incompleta"))
    );
}

#[test]
fn xbox_content_completo_independe_de_pasta_irma_profunda() {
    let raiz = dir_teste("xbox-content-irma");
    let config = preparar_xbox(&raiz, true);
    std::fs::create_dir_all(raiz.join("Pasta Xbox").join(vec!["d"; 65].join("\\"))).unwrap();
    let resultado = DetectorJogos::embutido().detectar_jogos(
        &nvt_detect::launchers::xbox::varrer_dir(&raiz),
        &[],
        &ServicosSistema::default(),
        &ProcessosFalsos::com_nomes(&[]),
    );
    let aberturas = recolher_aberturas();
    std::fs::remove_dir_all(raiz).unwrap();
    assert_eq!(resultado.jogos.len(), 1);
    assert_eq!(resultado.jogos[0].nome, "Jogo Xbox");
    assert_eq!(aberturas.first(), Some(&config));
}

struct XboxAbreNaTentativa {
    chamadas: Cell<usize>,
    tentativa: usize,
}

impl FonteProcessos for XboxAbreNaTentativa {
    fn processos(&self) -> Vec<Processo> {
        let chamada = self.chamadas.get() + 1;
        self.chamadas.set(chamada);
        if chamada == self.tentativa {
            vec![Processo::new("Xbox.exe", 1, 0)]
        } else {
            Vec::new()
        }
    }
}

#[test]
fn xbox_renova_processos_imediatamente_antes_do_manifesto_content() {
    let raiz = dir_teste("xbox-corrida-content");
    preparar_xbox(&raiz, true);
    std::fs::create_dir_all(raiz.join("Pasta Xbox").join(vec!["d"; 15].join("\\"))).unwrap();
    let fonte = XboxAbreNaTentativa {
        chamadas: Cell::new(0),
        tentativa: 3,
    };
    let resultado = DetectorJogos::embutido().detectar_jogos(
        &nvt_detect::launchers::xbox::varrer_dir(&raiz),
        &[],
        &ServicosSistema::default(),
        &fonte,
    );
    let aberturas = recolher_aberturas();
    std::fs::remove_dir_all(raiz).unwrap();
    assert!(aberturas.is_empty(), "{aberturas:?}");
    assert_eq!(resultado.processos_em_execucao, ["xbox.exe"]);
    assert_eq!(fonte.chamadas.get(), 3);
}

#[test]
fn xbox_renova_processos_imediatamente_antes_do_manifesto_fallback() {
    let raiz = dir_teste("xbox-corrida-fallback");
    preparar_xbox(&raiz, false);
    std::fs::create_dir(raiz.join("Pasta Xbox").join("Content")).unwrap();
    let fonte = XboxAbreNaTentativa {
        chamadas: Cell::new(0),
        tentativa: 5,
    };
    let resultado = DetectorJogos::embutido().detectar_jogos(
        &nvt_detect::launchers::xbox::varrer_dir(&raiz),
        &[],
        &ServicosSistema::default(),
        &fonte,
    );
    let aberturas = recolher_aberturas();
    std::fs::remove_dir_all(raiz).unwrap();
    assert!(aberturas.is_empty(), "{aberturas:?}");
    assert_eq!(resultado.processos_em_execucao, ["xbox.exe"]);
    assert_eq!(fonte.chamadas.get(), 5);
}

#[test]
fn f109_raiz_em_juncao() {
    let raiz = dir_teste("root-link");
    let alvo = raiz.join("Real");
    let link = raiz.join("RootLink");
    std::fs::create_dir(&alvo).unwrap();
    std::fs::write(alvo.join("Game.exe"), b"MZ").unwrap();
    let mklink = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&alvo)
        .output()
        .unwrap();
    assert!(mklink.status.success(), "{mklink:?}");
    let mut jogo = DetectedGame::new(Launcher::Steam, "RootLink", &link);
    jogo.exe_indicado = Some(link.join("Game.exe"));
    let resultado = DetectorJogos::embutido().detectar_jogos(
        &[jogo],
        &[],
        &ServicosSistema::default(),
        &ProcessosFalsos::com_nomes(&[]),
    );
    let aberturas = recolher_aberturas();
    std::fs::remove_dir(link).unwrap();
    std::fs::remove_dir_all(raiz).unwrap();
    assert_eq!(resultado.jogos.len(), 1, "{:?}", resultado.avisos);
    assert_eq!(resultado.jogos[0].exe, "Game.exe");
    assert_eq!(aberturas.len(), 1, "{aberturas:?}");
}

#[test]
fn f109_epic_normaliza_exe_indicado() {
    let raiz = dir_teste("epic-exe");
    std::fs::create_dir(raiz.join("Epic")).unwrap();
    for exe in ["First.exe", "Second.exe"] {
        std::fs::write(raiz.join(exe), b"MZ").unwrap();
    }
    let mut jogo = DetectedGame::new(Launcher::Epic, "Epic", &raiz);
    jogo.exe_indicado = Some(raiz.join(r"Epic\..\Second.exe"));
    let resultado = DetectorJogos::embutido().detectar_jogos(
        &[jogo],
        &[],
        &ServicosSistema::default(),
        &ProcessosFalsos::com_nomes(&[]),
    );
    std::fs::remove_dir_all(raiz).unwrap();
    assert_eq!(resultado.jogos[0].exe, "Second.exe");
}

#[test]
fn f109_xbox_deduplica_content() {
    let raiz = dir_teste("xbox-dedup");
    let bibliotecas = raiz.join("XboxGames");
    let config = preparar_xbox(&bibliotecas, true);
    let content = config.parent().unwrap();
    let mut jogos = nvt_detect::launchers::xbox::varrer_dir(&bibliotecas);
    let mut generico = DetectedGame::new(Launcher::Generico, "Generico", content);
    generico.exe_indicado = Some(content.join("Xbox.exe"));
    jogos.push(generico);
    let jogos = nvt_detect::deduplicar(jogos);
    let resultado = DetectorJogos::embutido().detectar_jogos(
        &jogos,
        &[],
        &ServicosSistema::default(),
        &ProcessosFalsos::com_nomes(&[]),
    );
    std::fs::remove_dir_all(raiz).unwrap();
    let nomes: Vec<_> = resultado
        .jogos
        .iter()
        .map(|j| (j.nome.as_str(), j.exe.as_str()))
        .collect();
    assert_eq!(nomes, [("Jogo Xbox", "Xbox.exe")]);
}

#[test]
fn f109_xbox_nao_le_manifesto_com_exe_irmao_aberto() {
    let raiz = dir_teste("xbox-irma-aberta");
    let config = preparar_xbox(&raiz, true);
    let base = config.parent().unwrap().parent().unwrap();
    let outra = base.join("Other");
    std::fs::create_dir(&outra).unwrap();
    std::fs::write(outra.join("Other.exe"), b"MZ").unwrap();
    std::fs::write(&config, r#"<Executable Name="..\Other\Other.exe"/>"#).unwrap();
    std::fs::create_dir_all(base.join(vec!["d"; 15].join("\\"))).unwrap();
    let resultado = DetectorJogos::embutido().detectar_jogos(
        &nvt_detect::launchers::xbox::varrer_dir(&raiz),
        &[],
        &ServicosSistema::default(),
        &ProcessosFalsos::com_nomes(&["Other.exe"]),
    );
    let aberturas = recolher_aberturas();
    std::fs::remove_dir_all(raiz).unwrap();
    assert!(aberturas.is_empty(), "{aberturas:?}");
    assert!(resultado.jogos.is_empty());
    assert_eq!(resultado.processos_em_execucao, ["other.exe"]);
}

#[test]
fn f109_listagem_limita_total_de_diretorios() {
    use nvt_detect::facts::arquivos::{ARQUIVOS_MAX, listar};

    let raiz = dir_teste("teto-diretorios");
    for grupo in 0..=ARQUIVOS_MAX / 1000 {
        let pasta = raiz.join(grupo.to_string());
        std::fs::create_dir(&pasta).unwrap();
        for indice in 0..(ARQUIVOS_MAX - grupo * 1000).min(1000) {
            std::fs::create_dir(pasta.join(indice.to_string())).unwrap();
        }
    }
    let listagem = listar(&raiz);
    std::fs::remove_dir_all(raiz).unwrap();
    assert!(listagem.truncada);
    assert_eq!(listagem.pastas.len(), ARQUIVOS_MAX);
}
