//! Ciclo completo do motor sobre o [`DriverFalso`].

use nvt_core::{Alvo, Api, Versao};
use nvt_drs::{ProfileRef, SettingValue};
use nvt_engine::{
    Armazem, ClasseDeriva, Divergencia, DriverFalso, DriverStore, Engine, ErroEngine, Esperado,
    JogoEntrada, MotivoReset, OrigemPerfil, classificar_deriva,
};

const EXE_WARDOGS: &str = "WardogsClient-Win64-Shipping.exe";
const EXE_LOL: &str = "League of Legends.exe";
const ID_FORA: u32 = 0x1234_5678;

fn driver() -> DriverFalso {
    let d = DriverFalso::turing();
    d.definir_padrao(0x00A8_79CF, 0x6092_5292); // vsync: controlado pela aplicação
    d.definir_padrao(0x0019_8FFF, 1); // shader cache ligado por padrão
    d.definir_padrao(0x1095_F16F, 1); // indicador VRR ligado por padrão
    d.adicionar_perfil_predefinido("Wardogs", &[EXE_WARDOGS], &[]);
    d.adicionar_perfil_predefinido("League of Legends", &[EXE_LOL], &[(0x1057_EB71, 0)]);
    d
}

fn jogos() -> Vec<JogoEntrada> {
    let mut w = JogoEntrada::new("WARDOGS", EXE_WARDOGS);
    w.api = Api::Dx12;
    w.dlss = Some(Versao::new(310, 6, 0, 0));
    w.reflex = Some(true);
    w.anticheat_kernel = Some(true);
    let mut l = JogoEntrada::new("League of Legends", EXE_LOL);
    l.api = Api::Dx11;
    vec![w, l]
}

fn motor(d: &DriverFalso) -> Engine<DriverFalso> {
    Engine::new(d.clone(), Armazem::em_memoria()).expect("regras embutidas")
}

fn id(e: &Engine<DriverFalso>, nome: &str) -> u32 {
    e.regras().catalogo.get(nome).expect(nome).id
}

fn wardogs() -> ProfileRef {
    ProfileRef::Named("Wardogs".into())
}

fn aplicar_tudo(e: &mut Engine<DriverFalso>) {
    let diff = e.planejar(&jogos()).unwrap();
    let r = e.aplicar(&diff, true).unwrap();
    assert!(r.aplicada);
    assert!(r.falhas.is_empty(), "falhas de verificação: {:?}", r.falhas);
}

#[test]
fn plano_turing_com_dlss_aplica_preset_e() {
    let d = driver();
    let mut e = motor(&d);
    let diff = e.planejar(&jogos()).unwrap();

    let w = Alvo::Jogo("Wardogs".into());
    let preset = diff
        .mudancas_de(&w)
        .find(|m| m.mudanca.setting == "dlss_preset")
        .expect("preset E no plano");
    assert_eq!(preset.mudanca.valor, 5);
    assert_eq!(preset.mudanca.valor_anterior, None);
    // Só as 3 do DLSS: WARDOGS está fora da base competitiva, então não
    // recebe a energia por jogo (spec competitivo F1-11, §11 item 19).
    assert_eq!(diff.mudancas_de(&w).count(), 3);
    assert!(diff.mudancas_de(&w).all(|m| m.mudanca.setting != "energia"));
    assert_eq!(
        diff.jogos
            .iter()
            .find(|j| j.perfil == "Wardogs")
            .unwrap()
            .origem,
        OrigemPerfil::Driver
    );
    // LoL não tem DLSS, mas é competitivo (base curada): só a energia por
    // jogo (F1-11), sobre o predefinido 0 do perfil do driver.
    let alvo_lol = Alvo::Jogo("League of Legends".into());
    let lol: Vec<_> = diff.mudancas_de(&alvo_lol).collect();
    assert_eq!(lol.len(), 1);
    assert_eq!(lol[0].mudanca.setting, "energia");
    assert_eq!(lol[0].mudanca.valor_anterior, Some(0));
    // Shader cache já está ligado por padrão: conforme, não gravado.
    assert!(
        diff.conformes_de(&Alvo::Global)
            .any(|m| m.mudanca.setting == "shader_cache")
    );
    let vsync = diff
        .mudancas_de(&Alvo::Global)
        .find(|m| m.mudanca.setting == "vsync")
        .unwrap();
    assert_eq!(vsync.mudanca.valor_anterior, Some(0x6092_5292));

    // Sem confirmação nada acontece.
    let r = e.aplicar(&diff, false).unwrap();
    assert!(!r.aplicada);
    assert_eq!(d.gravacoes(), 0);
    assert!(e.armazem().snapshot_original().is_none());

    let r = e.aplicar(&diff, true).unwrap();
    assert!(r.aplicada && r.snapshot_capturado);
    assert_eq!(r.verificadas.len(), diff.mudancas.len());
    assert_eq!(d.valor(&wardogs(), id(&e, "dlss_preset")), Some(5));
    assert_eq!(d.valor(&wardogs(), id(&e, "dlss_override")), Some(1));
    assert_eq!(
        d.valor(&ProfileRef::Base, id(&e, "vsync")),
        Some(0x0841_6747)
    );
    assert_eq!(d.salvamentos(), 1);
    assert_eq!(e.estado().historico.len(), diff.mudancas.len());
    assert_eq!(
        e.estado().esperado.len(),
        diff.mudancas.len() + diff.conformes.len()
    );
    assert_eq!(e.estado().driver_visto, Some(62140));
}

#[test]
fn segundo_ciclo_sem_mudancas_e_no_op() {
    let d = driver();
    let mut e = motor(&d);
    aplicar_tudo(&mut e);
    let gravacoes = d.gravacoes();

    let diff = e.planejar(&jogos()).unwrap();
    assert!(diff.vazio(), "sobrou: {:?}", diff.mudancas);
    let r = e.aplicar(&diff, true).unwrap();
    assert!(!r.aplicada);
    assert_eq!(d.gravacoes(), gravacoes);
    assert_eq!(d.salvamentos(), 1);
    assert_eq!(e.verificar_deriva().unwrap().classe, ClasseDeriva::Nenhuma);
}

#[test]
fn reset_em_massa_reaplica() {
    let d = driver();
    let mut e = motor(&d);
    aplicar_tudo(&mut e);

    d.resetar_settings();
    let deriva = e.verificar_deriva().unwrap();
    assert!(
        matches!(
            deriva.classe,
            ClasseDeriva::ResetEmMassa(MotivoReset::MuitasSettings { .. })
        ),
        "{:?}",
        deriva.classe
    );
    let t = e.tratar_deriva(&deriva, true).unwrap();
    assert!(t.travadas.is_empty());
    assert!(t.falhas.is_empty());
    assert_eq!(t.reaplicadas.len(), deriva.divergencias.len());
    assert_eq!(d.valor(&wardogs(), id(&e, "dlss_preset")), Some(5));
    assert!(e.planejar(&jogos()).unwrap().vazio());
    assert_eq!(e.verificar_deriva().unwrap().classe, ClasseDeriva::Nenhuma);
}

#[test]
fn atualizacao_de_driver_dispara_reaplicacao() {
    let d = driver();
    let mut e = motor(&d);
    aplicar_tudo(&mut e);

    d.mudar_versao(62210);
    d.resetar_settings();
    let deriva = e.verificar_deriva().unwrap();
    assert_eq!(
        deriva.classe,
        ClasseDeriva::ResetEmMassa(MotivoReset::VersaoDriver {
            antes: 62140,
            agora: 62210
        })
    );
    let t = e.tratar_deriva(&deriva, true).unwrap();
    assert!(!t.reaplicadas.is_empty());
    assert_eq!(e.estado().driver_visto, Some(62210));
    assert_eq!(e.verificar_deriva().unwrap().classe, ClasseDeriva::Nenhuma);
}

#[test]
fn mudanca_pontual_vira_escolha_travada() {
    let d = driver();
    let mut e = motor(&d);
    aplicar_tudo(&mut e);

    let vsync = id(&e, "vsync");
    d.mudar_pelo_usuario(&ProfileRef::Base, vsync, 0x4781_4940); // force_on
    let deriva = e.verificar_deriva().unwrap();
    assert_eq!(deriva.classe, ClasseDeriva::Pontual);
    assert_eq!(deriva.divergencias.len(), 1);

    // Sem confirmação nada muda.
    assert!(!e.tratar_deriva(&deriva, false).unwrap().aplicado);
    assert!(e.estado().travadas.is_empty());

    let t = e.tratar_deriva(&deriva, true).unwrap();
    assert_eq!(t.travadas.len(), 1);
    assert_eq!(t.travadas[0].setting, "vsync");
    assert_eq!(t.travadas[0].valor_usuario, Some(0x4781_4940));
    assert_eq!(d.valor(&ProfileRef::Base, vsync), Some(0x4781_4940));

    let diff = e.planejar(&jogos()).unwrap();
    assert!(diff.vazio());
    assert!(
        diff.plano
            .respeitadas
            .iter()
            .any(|r| r.mudanca.setting == "vsync")
    );
    assert_eq!(e.verificar_deriva().unwrap().classe, ClasseDeriva::Nenhuma);

    assert!(e.liberar_travada(&Alvo::Global, "vsync").unwrap());
    assert_eq!(e.planejar(&jogos()).unwrap().mudancas.len(), 1);
}

#[test]
fn restaurar_volta_ao_snapshot() {
    let d = driver();
    // Estado do usuário antes do app: vsync gravado, uma setting fora do catálogo.
    d.mudar_pelo_usuario(&ProfileRef::Base, 0x00A8_79CF, 0x6092_5292);
    d.mudar_pelo_usuario(&ProfileRef::Base, ID_FORA, 7);
    let mut todos = jogos();
    let mut novo = JogoEntrada::new("Jogo Novo", "novo.exe");
    novo.dlss = Some(Versao::new(3, 7, 0, 0));
    todos.push(novo);

    let mut e = motor(&d);
    let diff = e.planejar(&todos).unwrap();
    e.aplicar(&diff, true).unwrap();
    let original = e.armazem().snapshot_original().unwrap().clone();

    // Mudança alheia fora do catálogo depois da aplicação: não é do app, não é tocada.
    d.mudar_pelo_usuario(&ProfileRef::Base, ID_FORA, 9);

    let plano = e.plano_restauracao().unwrap().unwrap();
    assert!(!plano.ops.is_empty());
    assert_eq!(plano.ignoradas.len(), 1);
    assert!(!e.restaurar(false).unwrap().unwrap().aplicada);
    assert_eq!(d.valor(&wardogs(), id(&e, "dlss_preset")), Some(5));

    let r = e.restaurar(true).unwrap().unwrap();
    assert!(r.aplicada);
    let agora = d.capturar(&[EXE_WARDOGS, EXE_LOL]).unwrap();
    let catalogo: Vec<u32> = e.regras().catalogo.ids_permitidos();
    for p in &original.profiles {
        let atual = agora
            .profiles
            .iter()
            .find(|x| x.profile == p.profile)
            .unwrap();
        let filtra = |s: &Vec<nvt_drs::SettingSnapshot>| {
            s.iter()
                .filter(|x| catalogo.contains(&x.id))
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(filtra(&atual.settings), filtra(&p.settings), "{}", p.name);
    }
    assert_eq!(d.valor(&ProfileRef::Base, ID_FORA), Some(9));
    assert_eq!(d.valor(&ProfileRef::Base, 0x00A8_79CF), Some(0x6092_5292));
    // Perfil criado pelo app fica sem settings do catálogo.
    assert_eq!(
        d.valor(
            &ProfileRef::Named("Jogo Novo".into()),
            id(&e, "dlss_preset")
        ),
        None
    );
    assert!(e.estado().esperado.is_empty());
    assert!(e.plano_restauracao().unwrap().unwrap().ops.is_empty());
}

#[test]
fn id_fora_da_lista_e_recusado() {
    let d = driver();
    let mut e = motor(&d);
    let r = e.gravar_permitido(&ProfileRef::Base, ID_FORA, 1);
    assert!(
        matches!(r, Err(ErroEngine::IdNaoPermitido { id: ID_FORA })),
        "{r:?}"
    );
    let preset = id(&e, "dlss_preset");
    let r = e.gravar_permitido(&wardogs(), preset, 99);
    assert!(
        matches!(r, Err(ErroEngine::ValorNaoPermitido { .. })),
        "{r:?}"
    );
    assert_eq!(d.gravacoes(), 0);

    // Um diff adulterado também é recusado antes de tocar no driver.
    let mut diff = e.planejar(&jogos()).unwrap();
    diff.mudancas[0].mudanca.id = ID_FORA;
    assert!(matches!(
        e.aplicar(&diff, true),
        Err(ErroEngine::IdNaoPermitido { .. })
    ));
    assert_eq!(d.gravacoes(), 0);
    assert!(e.armazem().snapshot_original().is_none());
}

#[test]
fn jogo_sem_perfil_ganha_perfil_e_aplicativo() {
    let d = driver();
    let mut e = motor(&d);
    let mut novo = JogoEntrada::new("Jogo Novo", "novo.exe");
    novo.dlss = Some(Versao::new(3, 7, 0, 0));
    let sem_dlss = JogoEntrada::new("Sem DLSS", "semdlss.exe");
    let entrada = vec![novo, sem_dlss];

    let diff = e.planejar(&entrada).unwrap();
    assert_eq!(diff.jogos[0].origem, OrigemPerfil::ACriar);
    // Spec competitivo F1-11 e §11 item 19: a energia por jogo só vale para
    // jogo competitivo, então o jogo sem DLSS fora da base não ganha perfil.
    assert_eq!(diff.criar.len(), 1, "só cria perfil quando há mudança");
    assert_eq!(diff.criar[0].exe, "novo.exe");

    e.aplicar(&diff, true).unwrap();
    assert!(d.perfis().contains(&"Jogo Novo".to_string()));
    assert!(!d.perfis().contains(&"Sem DLSS".to_string()));
    assert_eq!(d.apps("Jogo Novo"), vec!["novo.exe".to_string()]);
    assert_eq!(
        d.valor(
            &ProfileRef::Named("Jogo Novo".into()),
            id(&e, "dlss_preset")
        ),
        Some(5)
    );
    assert_eq!(e.estado().perfis_criados.len(), 1);

    let diff = e.planejar(&entrada).unwrap();
    assert_eq!(diff.jogos[0].origem, OrigemPerfil::CriadoPeloApp);
    assert!(diff.vazio());
}

#[test]
fn nome_de_perfil_ocupado_ganha_sufixo() {
    let d = driver();
    d.adicionar_perfil_predefinido("Jogo Novo", &["outro.exe"], &[]);
    let e = motor(&d);
    let r = e
        .resolver_jogos(&[JogoEntrada::new("Jogo Novo", "novo.exe")])
        .unwrap();
    assert_eq!(r[0].perfil, "Jogo Novo (nv-tuner)");
}

fn div(revertida: bool) -> Divergencia {
    Divergencia {
        esperado: Esperado {
            alvo: Alvo::Global,
            perfil: ProfileRef::Base,
            setting: "x".into(),
            id: 0,
            valor: 0,
            origem: nvt_core::Origem::Regra { id: "r".into() },
            antes_da_gestao: None,
        },
        atual: None,
        voltou_ao_padrao: revertida,
    }
}

#[test]
fn limiares_de_deriva() {
    let v = |n: usize, rev: bool| vec![div(rev); n];
    let massa = |c: ClasseDeriva| matches!(c, ClasseDeriva::ResetEmMassa(_));
    // 3 de 15 revertidas (20 %): abaixo de 50 % → pontual.
    assert_eq!(
        classificar_deriva(15, &v(3, true), Some(1), 1),
        ClasseDeriva::Pontual
    );
    // 8 de 15 (53 %) e ≥ 3 → massa.
    assert!(massa(classificar_deriva(15, &v(8, true), Some(1), 1)));
    // 7 de 15 (46 %) → pontual.
    assert_eq!(
        classificar_deriva(15, &v(7, true), Some(1), 1),
        ClasseDeriva::Pontual
    );
    // 2 de 3: abaixo do mínimo absoluto e não é "todas" → pontual.
    assert_eq!(
        classificar_deriva(3, &v(2, true), Some(1), 1),
        ClasseDeriva::Pontual
    );
    // 3 de 3 → massa; 2 de 2 (todas) → massa; 1 de 1 → pontual.
    assert!(massa(classificar_deriva(3, &v(3, true), Some(1), 1)));
    assert!(massa(classificar_deriva(2, &v(2, true), Some(1), 1)));
    assert_eq!(
        classificar_deriva(1, &v(1, true), Some(1), 1),
        ClasseDeriva::Pontual
    );
    // Mudadas para outro valor gravado não contam como revertidas.
    assert_eq!(
        classificar_deriva(10, &v(10, false), Some(1), 1),
        ClasseDeriva::Pontual
    );
    // Versão do driver mudou → massa, mesmo sem divergência.
    assert!(massa(classificar_deriva(5, &[], Some(1), 2)));
    // Nada gerido: troca de versão não é deriva.
    assert_eq!(
        classificar_deriva(0, &[], Some(1), 2),
        ClasseDeriva::Nenhuma
    );
    assert_eq!(
        classificar_deriva(5, &[], Some(1), 1),
        ClasseDeriva::Nenhuma
    );
}

#[test]
fn estado_persiste_e_snapshot_nunca_e_sobrescrito() {
    let dir = std::env::temp_dir().join(format!("nvt-engine-teste-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let d = driver();
    {
        let mut e = Engine::new(d.clone(), Armazem::abrir(&dir).unwrap()).unwrap();
        aplicar_tudo(&mut e);
    }
    assert!(dir.join("estado.json").exists());
    let snap_antes = std::fs::read_to_string(dir.join("snapshot-original.json")).unwrap();

    let mut a = Armazem::abrir(&dir).unwrap();
    assert!(!a.estado.esperado.is_empty());
    assert!(a.snapshot_original().is_some());
    let outro = d.capturar(&[]).unwrap();
    assert!(!a.guardar_snapshot_original(outro).unwrap());
    assert_eq!(
        std::fs::read_to_string(dir.join("snapshot-original.json")).unwrap(),
        snap_antes
    );

    // Novo ciclo a partir do disco: nada a fazer.
    let e = Engine::new(d.clone(), a).unwrap();
    assert!(e.planejar(&jogos()).unwrap().vazio());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn falso_respeita_heranca_do_base() {
    let d = driver();
    let mut s = d.clone();
    s.gravar_setting(&ProfileRef::Base, 0x1057_EB71, &SettingValue::Dword(1))
        .unwrap();
    // Wardogs não grava energia: herda do base.
    assert_eq!(d.valor(&wardogs(), 0x1057_EB71), Some(1));
    // LoL tem predefinido próprio: vence o base.
    assert_eq!(
        d.valor(&ProfileRef::Named("League of Legends".into()), 0x1057_EB71),
        Some(0)
    );
}
