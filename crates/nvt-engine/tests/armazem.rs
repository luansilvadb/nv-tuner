//! Abertura do estado: avisos estruturados, modo somente leitura e recusa de
//! reparse points (link simbólico/junção) no lugar de `estado.json` e
//! `snapshot-original.json`.

use std::path::{Path, PathBuf};

use nvt_engine::{Armazem, CodigoAviso, DriverFalso, DriverStore, Engine, ErroEngine, NivelAviso};

fn dir_teste(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nvt-armazem-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Nomes e conteúdos do diretório (para provar que nada mudou).
fn conteudo(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            let dados = std::fs::read(e.path()).unwrap_or_default();
            (e.file_name().to_string_lossy().into_owned(), dados)
        })
        .collect();
    v.sort();
    v
}

fn codigos(a: &Armazem) -> Vec<CodigoAviso> {
    a.avisos().iter().map(|x| x.codigo).collect()
}

/// Diretório com estado válido, cópia anterior válida e snapshot original.
fn preparar_estado(nome: &str) -> PathBuf {
    let dir = dir_teste(nome);
    let d = DriverFalso::turing();
    let mut a = Armazem::abrir(&dir).unwrap();
    a.guardar_snapshot_original(d.capturar(&[]).unwrap())
        .unwrap();
    a.estado.preferencias.limite_fps = Some(117);
    a.salvar().unwrap();
    a.salvar().unwrap(); // segunda gravação: cria estado.anterior.json
    assert!(dir.join("estado.anterior.json").exists());
    dir
}

#[test]
fn codigos_estaveis_iguais_ao_protocolo() {
    assert_eq!(
        CodigoAviso::EstadoCorrompido.como_str(),
        "estado_corrompido"
    );
    assert_eq!(
        CodigoAviso::EstadoRecuperado.como_str(),
        "estado_recuperado"
    );
    assert_eq!(
        CodigoAviso::EstadoReiniciado.como_str(),
        "estado_reiniciado"
    );
    assert_eq!(
        CodigoAviso::SnapshotIndisponivel.como_str(),
        "snapshot_indisponivel"
    );
}

#[test]
fn modo_normal_recupera_e_guarda_o_corrompido() {
    let dir = preparar_estado("normal");
    std::fs::write(dir.join("estado.json"), b"{ truncado").unwrap();
    let a = Armazem::abrir(&dir).unwrap();
    assert!(!a.somente_leitura());
    assert_eq!(
        a.estado.preferencias.limite_fps,
        Some(117),
        "da cópia anterior"
    );
    assert_eq!(
        codigos(&a),
        [CodigoAviso::EstadoCorrompido, CodigoAviso::EstadoRecuperado]
    );
    assert_eq!(a.avisos()[0].nivel, NivelAviso::Atencao);
    assert!(
        a.avisos()[0]
            .detalhe
            .as_deref()
            .unwrap()
            .contains("movido para")
    );
    // Renomeado e o recuperado gravado na hora (item #7 da reverificação).
    assert_eq!(
        Armazem::abrir(&dir).unwrap().estado.preferencias.limite_fps,
        Some(117),
        "recuperado persistido em estado.json"
    );
    assert!(
        conteudo(&dir)
            .iter()
            .any(|(n, c)| n.starts_with("estado.json.corrompido-") && c == b"{ truncado")
    );
    // Sem cópia anterior válida: estado limpo.
    std::fs::write(dir.join("estado.json"), b"").unwrap();
    std::fs::write(dir.join("estado.anterior.json"), b"lixo").unwrap();
    let a = Armazem::abrir(&dir).unwrap();
    assert_eq!(
        codigos(&a),
        [CodigoAviso::EstadoCorrompido, CodigoAviso::EstadoReiniciado]
    );
    assert!(a.snapshot_original().is_some(), "snapshot preservado");
    let e = Engine::new(DriverFalso::turing(), a).unwrap();
    assert_eq!(e.avisos().len(), 2, "o motor expõe os mesmos avisos");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn somente_leitura_nunca_renomeia_cria_nem_grava() {
    let dir = preparar_estado("leitura");
    std::fs::write(dir.join("estado.json"), b"{ truncado").unwrap();
    let antes = conteudo(&dir);

    let a = Armazem::abrir_somente_leitura(&dir).unwrap();
    assert!(a.somente_leitura());
    assert_eq!(
        a.estado.preferencias.limite_fps,
        Some(117),
        "da cópia, em memória"
    );
    assert_eq!(
        codigos(&a),
        [CodigoAviso::EstadoCorrompido, CodigoAviso::EstadoRecuperado]
    );
    assert!(
        a.avisos()[0]
            .detalhe
            .as_deref()
            .unwrap()
            .contains("mantido no lugar")
    );
    assert_eq!(conteudo(&dir), antes, "nada mudou no disco");
    // Gravações recusadas.
    assert!(matches!(a.salvar(), Err(ErroEngine::SomenteLeitura)));
    assert_eq!(conteudo(&dir), antes);

    // Sem cópia válida: estado vazio só em memória, nada apagado.
    std::fs::write(dir.join("estado.anterior.json"), b"lixo").unwrap();
    let antes = conteudo(&dir);
    let a = Armazem::abrir_somente_leitura(&dir).unwrap();
    assert_eq!(
        codigos(&a),
        [CodigoAviso::EstadoCorrompido, CodigoAviso::EstadoReiniciado]
    );
    assert_eq!(a.estado.preferencias.limite_fps, None);
    assert_eq!(conteudo(&dir), antes);
    // Abrir de novo dá o mesmo resultado (o arquivo continua lá).
    let b = Armazem::abrir_somente_leitura(&dir).unwrap();
    assert_eq!(b.avisos(), a.avisos());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn somente_leitura_nao_cria_diretorio_nem_snapshot() {
    let dir = dir_teste("leitura-vazio");
    std::fs::remove_dir(&dir).unwrap();
    let mut a = Armazem::abrir_somente_leitura(&dir).unwrap();
    assert!(a.avisos().is_empty());
    let snap = DriverFalso::turing().capturar(&[]).unwrap();
    assert!(matches!(
        a.guardar_snapshot_original(snap),
        Err(ErroEngine::SomenteLeitura)
    ));
    assert!(matches!(a.salvar(), Err(ErroEngine::SomenteLeitura)));
    assert!(!dir.exists(), "diretório não criado");
}

#[test]
fn snapshot_ilegivel_vira_aviso_de_erro_nos_dois_modos() {
    let dir = dir_teste("snap");
    std::fs::write(dir.join("snapshot-original.json"), b"{").unwrap();
    for a in [
        Armazem::abrir(&dir).unwrap(),
        Armazem::abrir_somente_leitura(&dir).unwrap(),
    ] {
        assert_eq!(codigos(&a), [CodigoAviso::SnapshotIndisponivel]);
        assert_eq!(a.avisos()[0].nivel, NivelAviso::Erro);
        assert_eq!(a.avisos()[0].detalhe.as_deref(), a.snapshot_indisponivel());
    }
    assert_eq!(
        std::fs::read(dir.join("snapshot-original.json")).unwrap(),
        b"{"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── Reparse points ─────────────────────────────────────────────────────────

/// Link simbólico de arquivo (exige o modo de desenvolvedor ou administrador;
/// `false` se o sistema não permitir).
fn link_arquivo(alvo: &Path, link: &Path) -> bool {
    match std::os::windows::fs::symlink_file(alvo, link) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("link simbólico indisponível ({e}); parte do teste pulada");
            false
        }
    }
}

/// Junção (não exige privilégio).
fn juncao(alvo: &Path, link: &Path) {
    let st = std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(alvo)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(st.success(), "mklink /J");
}

#[test]
fn estado_e_snapshot_como_link_simbolico_sao_recusados() {
    let dir = dir_teste("link");
    let fora = dir_teste("link-fora");
    // Conteúdo inválido fora do diretório: seguir o link levaria a renomeá-lo.
    let alvo = fora.join("alvo.json");
    std::fs::write(&alvo, b"{ alvo").unwrap();
    if !link_arquivo(&alvo, &dir.join("estado.json")) {
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&fora);
        return;
    }
    for abrir in [Armazem::abrir, Armazem::abrir_somente_leitura] {
        let e = abrir(dir.clone()).expect_err("link recusado");
        assert!(e.to_string().contains("reparse point"), "{e}");
    }
    assert_eq!(std::fs::read(&alvo).unwrap(), b"{ alvo", "alvo intacto");
    assert_eq!(conteudo(&fora).len(), 1, "nada renomeado fora");
    assert!(
        std::fs::symlink_metadata(dir.join("estado.json"))
            .unwrap()
            .file_type()
            .is_symlink(),
        "o link não foi renomeado"
    );
    std::fs::remove_file(dir.join("estado.json")).unwrap();

    // Snapshot original como link: recusado (não vira "indisponível" nem é lido).
    let snap = DriverFalso::turing().capturar(&[]).unwrap();
    std::fs::write(&alvo, snap.to_json().unwrap()).unwrap();
    assert!(link_arquivo(&alvo, &dir.join("snapshot-original.json")));
    for abrir in [Armazem::abrir, Armazem::abrir_somente_leitura] {
        assert!(abrir(dir.clone()).is_err());
    }
    std::fs::remove_file(dir.join("snapshot-original.json")).unwrap();

    // Link plantado no temporário: a gravação não escreve no alvo.
    std::fs::write(&alvo, b"intocado").unwrap();
    assert!(link_arquivo(&alvo, &dir.join("estado.json.tmp")));
    let a = Armazem::abrir(&dir).unwrap();
    a.salvar().unwrap();
    assert_eq!(std::fs::read(&alvo).unwrap(), b"intocado");
    assert!(Armazem::abrir(&dir).unwrap().avisos().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&fora);
}

#[test]
fn estado_e_snapshot_como_juncao_sao_recusados() {
    let dir = dir_teste("juncao");
    let fora = dir_teste("juncao-fora");
    std::fs::write(fora.join("x"), b"x").unwrap();
    for nome in [
        "estado.json",
        "snapshot-original.json",
        "estado.anterior.json",
    ] {
        juncao(&fora, &dir.join(nome));
        if nome == "estado.anterior.json" {
            // A cópia anterior só é lida com estado.json inválido.
            std::fs::write(dir.join("estado.json"), b"{").unwrap();
        }
        let antes = conteudo(&fora);
        for abrir in [Armazem::abrir, Armazem::abrir_somente_leitura] {
            assert!(abrir(dir.clone()).is_err(), "{nome}: junção recusada");
        }
        assert_eq!(conteudo(&fora), antes, "{nome}: alvo intacto");
        assert!(
            std::fs::symlink_metadata(dir.join(nome))
                .unwrap()
                .file_type()
                .is_symlink(),
            "{nome}: a junção continua no lugar"
        );
        std::fs::remove_dir(dir.join(nome)).unwrap();
        let _ = std::fs::remove_file(dir.join("estado.json"));
    }
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&fora);
}

#[test]
fn motor_somente_leitura_recusa_gravar_antes_do_driver() {
    // Snapshot já existe: a recusa não depende de guardar o snapshot.
    let dir = preparar_estado("motor-leitura");
    let d = DriverFalso::turing();
    let mut e = Engine::new(d.clone(), Armazem::abrir_somente_leitura(&dir).unwrap()).unwrap();
    let antes = conteudo(&dir);
    let g = d.gravacoes();
    let mut j = nvt_engine::JogoEntrada::new("Jogo", "jogo.exe");
    j.dlss = Some(nvt_core::Versao::new(310, 0, 0, 0));
    let diff = e.planejar(&[j]).unwrap();
    assert!(!diff.vazio(), "há o que aplicar");
    assert!(matches!(
        e.aplicar(&diff, true),
        Err(ErroEngine::SomenteLeitura)
    ));
    assert!(matches!(
        e.definir_preferencias(e.estado().preferencias.clone()),
        Err(ErroEngine::SomenteLeitura)
    ));
    // Nada aplicado: a restauração não tem operações (e nada grava).
    match e.restaurar(true) {
        Err(ErroEngine::SomenteLeitura) | Ok(None) => {}
        Ok(Some(r)) => assert!(r.ops.is_empty() && !r.aplicada),
        Err(x) => panic!("{x}"),
    }
    assert_eq!(d.gravacoes(), g, "driver intocado");
    assert_eq!(conteudo(&dir), antes, "estado intocado");
    let _ = std::fs::remove_dir_all(&dir);
}
