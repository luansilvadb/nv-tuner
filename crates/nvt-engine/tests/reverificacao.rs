//! Regressões da reverificação adversarial (commit `c20974c`,
//! `docs/revisao-v0.1.md`, seção "Reverificação"). Cada teste reproduz o
//! cenário do relatório e afirma o comportamento corrigido. Só `DriverFalso`.

use std::path::PathBuf;

use nvt_core::{Alvo, Api, Preferences, Versao};
use nvt_drs::ProfileRef;
use nvt_engine::{
    AcaoReversao, Armazem, CodigoAviso, DriverFalso, DriverStore, Engine, JogoEntrada, Travada,
    ValorAntes,
};

fn dir_teste(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nvt-reverif-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn id<S: DriverStore>(e: &Engine<S>, n: &str) -> u32 {
    e.regras().catalogo.get(n).expect(n).id
}

fn aplicar<S: DriverStore>(e: &mut Engine<S>, jogos: &[JogoEntrada]) {
    let d = e.planejar(jogos).unwrap();
    let r = e.aplicar(&d, true).unwrap();
    assert!(r.falhas.is_empty(), "falhas: {:?}", r.falhas);
    assert!(r.erros.is_empty(), "erros: {:?}", r.erros);
}

fn com_limite(e: &Engine<impl DriverStore>, limite: Option<u32>) -> Preferences {
    let mut p = e.estado().preferencias.clone();
    p.limite_fps = limite;
    p
}

fn jogo_novo() -> JogoEntrada {
    let mut j = JogoEntrada::new("Jogo Sem Perfil", "jogosemperfil.exe");
    j.api = Api::Dx12;
    j.dlss = Some(Versao::new(310, 6, 0, 0));
    j
}

// ── R1: a reversão restaura o valor de antes da gestão ───────────────────

/// 1ª aplicação (snapshot) → usuário põe limite 90 → preferência 141 →
/// preferência `None` ⇒ volta a 90 (antes: `Remover`, valor final `None`).
#[test]
fn r1_reversao_volta_ao_valor_de_antes_da_gestao() {
    let f = DriverFalso::turing();
    let mut e = Engine::new(f.clone(), Armazem::em_memoria()).unwrap();
    aplicar(&mut e, &[]);
    assert!(e.armazem().snapshot_original().is_some());
    let limite = id(&e, "limite_fps");
    assert_eq!(f.valor(&ProfileRef::Base, limite), None);

    f.mudar_pelo_usuario(&ProfileRef::Base, limite, 90);
    e.definir_preferencias(com_limite(&e, Some(141))).unwrap();
    aplicar(&mut e, &[]);
    assert_eq!(f.valor(&ProfileRef::Base, limite), Some(141));
    let esp = e.estado().esperado.iter().find(|x| x.id == limite).unwrap();
    assert_eq!(esp.antes_da_gestao, Some(ValorAntes::Valor(90)));

    e.definir_preferencias(com_limite(&e, None)).unwrap();
    let d = e.planejar(&[]).unwrap();
    let r = d.reverter.iter().find(|r| r.esperado.id == limite).unwrap();
    assert_eq!(
        r.acao,
        AcaoReversao::Gravar(nvt_drs::SettingValue::Dword(90))
    );
    e.aplicar(&d, true).unwrap();
    assert_eq!(f.valor(&ProfileRef::Base, limite), Some(90), "volta a 90");

    // "Voltar tudo ao original" continua usando o snapshot inicial (sem limite).
    e.restaurar(true).unwrap();
    assert_eq!(f.valor(&ProfileRef::Base, limite), None);
}

/// Setting que não existia antes da gestão é removida na reversão, e uma
/// segunda gravação do app não troca o valor de antes da gestão.
#[test]
fn r1_inexistente_e_preservado_entre_gravacoes() {
    let f = DriverFalso::turing();
    let mut e = Engine::new(f.clone(), Armazem::em_memoria()).unwrap();
    aplicar(&mut e, &[]);
    let limite = id(&e, "limite_fps");
    e.definir_preferencias(com_limite(&e, Some(141))).unwrap();
    aplicar(&mut e, &[]);
    e.definir_preferencias(com_limite(&e, Some(120))).unwrap();
    aplicar(&mut e, &[]);
    let esp = e.estado().esperado.iter().find(|x| x.id == limite).unwrap();
    assert_eq!(esp.valor, 120);
    assert_eq!(esp.antes_da_gestao, Some(ValorAntes::Inexistente));
    e.definir_preferencias(com_limite(&e, None)).unwrap();
    aplicar(&mut e, &[]);
    assert_eq!(f.valor(&ProfileRef::Base, limite), None);
}

// ── R2: perfis criados pelo app sobrevivem à perda do estado ──────────────

/// `estado.json` e `estado.anterior.json` corrompidos: o perfil criado pelo
/// app continua reconhecido (via `perfis-criados.json`) e a restauração
/// limpa o preset E dele.
#[test]
fn r2_perfis_criados_sobrevivem_a_perda_das_duas_copias() {
    let dir = dir_teste("r2");
    let f = DriverFalso::turing();
    let jogos = vec![jogo_novo()];
    let preset;
    {
        let mut e = Engine::new(f.clone(), Armazem::abrir(&dir).unwrap()).unwrap();
        aplicar(&mut e, &jogos);
        aplicar(&mut e, &jogos); // segunda gravação: cria estado.anterior.json
        preset = id(&e, "dlss_preset");
    }
    let perfil = ProfileRef::Named("Jogo Sem Perfil".into());
    assert_eq!(f.valor(&perfil, preset), Some(5));
    assert!(dir.join("perfis-criados.json").exists());
    std::fs::write(dir.join("estado.json"), b"{ lixo").unwrap();
    std::fs::write(dir.join("estado.anterior.json"), b"lixo").unwrap();

    let mut e = Engine::new(f.clone(), Armazem::abrir(&dir).unwrap()).unwrap();
    let codigos: Vec<_> = e.avisos().iter().map(|a| a.codigo).collect();
    assert!(
        codigos.contains(&CodigoAviso::EstadoReiniciado),
        "{codigos:?}"
    );
    assert!(e.estado().criado_pelo_app("JOGO SEM PERFIL"));
    let d = e.planejar(&jogos).unwrap();
    assert_eq!(d.jogos[0].origem, nvt_engine::OrigemPerfil::CriadoPeloApp);
    e.restaurar(true).unwrap();
    assert_eq!(f.valor(&perfil, preset), None, "preset E removido");
    // A cópia continua lá para a próxima abertura.
    let a = Armazem::abrir(&dir).unwrap();
    assert!(a.estado.criado_pelo_app("Jogo Sem Perfil"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ── #7: recuperação persistida e nunca de volta ao vazio ──────────────────

fn preparar_com_trava(nome: &str) -> PathBuf {
    let dir = dir_teste(nome);
    let mut a = Armazem::abrir(&dir).unwrap();
    a.estado.travadas.push(Travada {
        alvo: Alvo::Global,
        setting: "vsync".into(),
        valor_usuario: Some(1),
        data: "2026-11-25T00:00:00Z".into(),
        motivo: "teste".into(),
    });
    a.salvar().unwrap();
    a.salvar().unwrap(); // cria estado.anterior.json
    dir
}

#[test]
fn r7_recuperacao_e_gravada_na_hora_e_somente_leitura_nao_mexe() {
    let dir = preparar_com_trava("r7");
    std::fs::write(dir.join("estado.json"), b"{ truncado").unwrap();
    // `status`/`dry-run`: nada muda no disco, mas o estado vem da cópia.
    let a = Armazem::abrir_somente_leitura(&dir).unwrap();
    assert_eq!(a.estado.travadas.len(), 1);
    assert_eq!(
        std::fs::read(dir.join("estado.json")).unwrap(),
        b"{ truncado"
    );
    // Serviço normal: recupera e grava na hora.
    let a = Armazem::abrir(&dir).unwrap();
    assert_eq!(a.estado.travadas.len(), 1);
    let gravado = std::fs::read_to_string(dir.join("estado.json")).unwrap();
    assert!(gravado.contains("vsync"), "recuperado persistido");
    // Abertura seguinte: sem aviso e com a trava.
    let b = Armazem::abrir(&dir).unwrap();
    assert!(b.avisos().is_empty(), "{:?}", b.avisos());
    assert_eq!(b.estado.travadas.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `estado.json` ausente (gravação do recuperado perdida, arquivo apagado)
/// com cópia válida: recupera da cópia em vez de voltar ao estado vazio.
#[test]
fn r7_estado_ausente_recupera_da_copia_anterior() {
    let dir = preparar_com_trava("r7-ausente");
    std::fs::remove_file(dir.join("estado.json")).unwrap();
    let a = Armazem::abrir_somente_leitura(&dir).unwrap();
    assert_eq!(a.estado.travadas.len(), 1);
    assert!(
        !dir.join("estado.json").exists(),
        "somente leitura não grava"
    );
    let a = Armazem::abrir(&dir).unwrap();
    assert_eq!(a.estado.travadas.len(), 1, "não volta ao vazio");
    assert!(
        a.avisos()
            .iter()
            .any(|x| x.codigo == CodigoAviso::EstadoRecuperado)
    );
    assert!(dir.join("estado.json").exists(), "gravado na hora");
    assert!(Armazem::abrir(&dir).unwrap().avisos().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

// ── #10: grafias do perfil ────────────────────────────────────────────────

#[test]
fn r10_travas_com_grafias_diferentes_sao_uma_so() {
    let dir = dir_teste("r10");
    let mut a = Armazem::abrir(&dir).unwrap();
    for nome in [
        "League of Legends",
        "LEAGUE OF LEGENDS",
        "league of legends",
    ] {
        a.estado.travadas.push(Travada {
            alvo: Alvo::Jogo(nome.into()),
            setting: "dlss_preset".into(),
            valor_usuario: None,
            data: String::new(),
            motivo: String::new(),
        });
    }
    a.salvar().unwrap();
    // Estado antigo com duplicatas: deduplicado na abertura.
    let a = Armazem::abrir(&dir).unwrap();
    assert_eq!(a.estado.travadas.len(), 1);
    assert!(
        a.estado
            .travada(&Alvo::Jogo("lEaGuE oF lEgEnDs".into()), "dlss_preset")
    );
    let mut e = Engine::new(DriverFalso::turing(), a).unwrap();
    assert!(
        e.liberar_travada(&Alvo::Jogo("LEAGUE of legends".into()), "dlss_preset")
            .unwrap()
    );
    assert!(e.estado().travadas.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

// ── Quarentena: snapshot original perdido ────────────────────────────────

#[test]
fn quarentena_marca_o_snapshot_novo_como_base_pos_quarentena() {
    let dir = dir_teste("quarentena");
    Armazem::marcar_snapshot_perdido(&dir, "quarentena em X").unwrap();
    let f = DriverFalso::turing();
    let a = Armazem::abrir(&dir).unwrap();
    assert_eq!(a.base_pos_quarentena(), Some("quarentena em X"));
    let aviso = a
        .avisos()
        .iter()
        .find(|x| x.codigo == CodigoAviso::SnapshotOriginalPerdido)
        .expect("aviso de snapshot perdido");
    assert!(aviso.mensagem.contains("próxima aplicação"));
    assert_eq!(
        CodigoAviso::SnapshotOriginalPerdido.como_str(),
        "snapshot_original_perdido"
    );
    let mut e = Engine::new(f.clone(), a).unwrap();
    aplicar(&mut e, &[]);
    assert!(e.armazem().snapshot_original().is_some());
    // Depois da captura, o aviso continua e diz que é base pós-quarentena.
    let a = Armazem::abrir(&dir).unwrap();
    let aviso = a
        .avisos()
        .iter()
        .find(|x| x.codigo == CodigoAviso::SnapshotOriginalPerdido)
        .expect("aviso persiste");
    assert!(aviso.mensagem.contains("base pós-quarentena"));
    // Somente leitura também vê o aviso, sem gravar nada.
    assert!(
        Armazem::abrir_somente_leitura(&dir)
            .unwrap()
            .base_pos_quarentena()
            .is_some()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
