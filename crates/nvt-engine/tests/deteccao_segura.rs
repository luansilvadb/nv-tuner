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
fn deteccao_local_omite_jogos_quando_listagem_truncada() {
    let com_exe = preparar_pasta("local-truncado-parcial");
    let sem_exe = preparar_pasta("local-truncado-sem-exe");
    std::fs::remove_file(sem_exe.join("Indie.exe")).unwrap();
    for dir in [&com_exe, &sem_exe] {
        let limite = nvt_detect::facts::arquivos::PROFUNDIDADE_MAX;
        let profunda = (0..limite).fold(dir.clone(), |pasta, _| pasta.join("d"));
        std::fs::create_dir_all(profunda).unwrap();
    }
    let jogos = [jogo_fora_da_base(&com_exe), jogo_fora_da_base(&sem_exe)];
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
