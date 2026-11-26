//! Regressões dos achados da revisão adversarial da v0.1 (`docs/revisao-v0.1.md`).
//! Cada teste reproduz a prova original e afirma o comportamento corrigido.

mod comum;

use std::path::PathBuf;

use nvt_core::{Alvo, Api, Origem, Preferences, Qualidade, Sincronismo, Versao};
use nvt_drs::{
    DriverVersion, Error, GpuInfo, ProfileRef, Result, Setting, SettingValue, Snapshot, Status,
};
use nvt_engine::{
    Armazem, ClasseDeriva, CodigoAviso, Deriva, Divergencia, DriverFalso, DriverStore, Engine,
    Esperado, JogoEntrada, MAX_NOME_PERFIL, OrigemRegistro, PerfilEncontrado,
};

const NOME_NOVO: &str = "Jogo Sem Perfil";
const EXE_NOVO: &str = "jogosemperfil.exe";

fn jogo_novo() -> JogoEntrada {
    let mut j = JogoEntrada::new(NOME_NOVO, EXE_NOVO);
    j.api = Api::Dx12;
    j.dlss = Some(Versao::new(310, 6, 0, 0));
    j
}

fn jogo_predefinido() -> JogoEntrada {
    let mut j = JogoEntrada::new("Jogo", "jogo.exe");
    j.dlss = Some(Versao::new(310, 0, 0, 0));
    j
}

fn id<S: DriverStore>(e: &Engine<S>, n: &str) -> u32 {
    e.regras().catalogo.get(n).expect(n).id
}

fn valor_nomeado<S: DriverStore>(e: &Engine<S>, setting: &str, valor: &str) -> u32 {
    e.regras().catalogo.get(setting).expect(setting).valores[valor]
}

fn prefs(s: Sincronismo, q: Qualidade, l: Option<u32>) -> Preferences {
    Preferences {
        sincronismo: s,
        qualidade: q,
        limite_fps: l,
        ..Preferences::default()
    }
}

fn aplicar<S: DriverStore>(e: &mut Engine<S>, jogos: &[JogoEntrada]) {
    let d = e.planejar(jogos).unwrap();
    let r = e.aplicar(&d, true).unwrap();
    assert!(r.falhas.is_empty(), "falhas: {:?}", r.falhas);
    assert!(r.erros.is_empty(), "erros: {:?}", r.erros);
}

fn dir_teste(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nvt-revisao-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ── #1 perfil criado pelo app some + versão nova do driver ──────────────────

#[test]
fn perfil_do_app_apagado_nao_trava_o_ciclo_e_e_recriado() {
    let f = DriverFalso::turing();
    let mut e = comum::motor(f.clone(), Armazem::em_memoria());
    let jogos = vec![jogo_novo()];
    aplicar(&mut e, &jogos);
    assert!(f.perfis().iter().any(|p| p == NOME_NOVO));
    // Spec competitivo F1-11: energia passou a ser por jogo; o exemplo de
    // setting global passa a ser a baixa latência Ultra.
    let global = id(&e, "low_latency_cpl");
    let preset = id(&e, "dlss_preset");
    let max = valor_nomeado(&e, "low_latency_cpl", "ultra");
    assert_eq!(f.valor(&ProfileRef::Base, global), Some(max));

    // Instalação limpa: settings resetadas, versão nova, perfil do app apagado.
    f.resetar_settings();
    f.mudar_versao(62200);
    f.apagar_perfil(NOME_NOVO);
    let der = e.verificar_deriva().unwrap();
    assert!(matches!(der.classe, ClasseDeriva::ResetEmMassa(_)));

    let t = e
        .tratar_deriva(&der, true)
        .expect("perfil ausente não derruba o ciclo");
    assert!(t.aplicado);
    assert!(t.erros.is_empty(), "{:?}", t.erros);
    assert_eq!(
        t.removidas.len(),
        3,
        "as 3 settings do DLSS saem do esperado (jogo fora da base: sem energia por jogo)"
    );
    // O global segue.
    assert_eq!(f.valor(&ProfileRef::Base, global), Some(max));
    assert!(
        e.estado()
            .esperado
            .iter()
            .all(|x| x.perfil == ProfileRef::Base)
    );

    // A regra ainda exige: o próximo plano recria perfil + aplicativo.
    let diff = e.planejar(&jogos).unwrap();
    assert!(diff.criar.iter().any(|c| c.nome == NOME_NOVO));
    aplicar(&mut e, &jogos);
    assert_eq!(f.apps(NOME_NOVO), vec![EXE_NOVO.to_owned()]);
    let p = ProfileRef::Named(NOME_NOVO.into());
    assert_eq!(f.valor(&p, preset), Some(5));
    assert_eq!(e.verificar_deriva().unwrap().classe, ClasseDeriva::Nenhuma);
}

/// Store que falha em toda operação sobre um perfil (erro da NVAPI qualquer).
#[derive(Clone)]
struct PerfilQuebrado {
    f: DriverFalso,
    perfil: String,
}

impl PerfilQuebrado {
    fn chk(&self, p: &ProfileRef) -> Result<()> {
        match p {
            ProfileRef::Named(n) if n.eq_ignore_ascii_case(&self.perfil) => Err(Error::Nvapi {
                function: "NvAPI_DRS_GetSetting",
                status: Status::from_code(-1),
                message: "falha simulada".into(),
            }),
            _ => Ok(()),
        }
    }
}

impl DriverStore for PerfilQuebrado {
    fn versao_driver(&self) -> Result<DriverVersion> {
        self.f.versao_driver()
    }
    fn gpus(&self) -> Result<Vec<GpuInfo>> {
        self.f.gpus()
    }
    fn perfil_do_exe(&self, exe: &str) -> Result<Option<PerfilEncontrado>> {
        self.f.perfil_do_exe(exe)
    }
    fn perfil_existe(&self, nome: &str) -> Result<bool> {
        self.f.perfil_existe(nome)
    }
    fn criar_perfil(&mut self, nome: &str) -> Result<()> {
        self.f.criar_perfil(nome)
    }
    fn criar_aplicacao(&mut self, perfil: &str, exe: &str) -> Result<()> {
        self.f.criar_aplicacao(perfil, exe)
    }
    fn ler_setting(&self, p: &ProfileRef, id: u32) -> Result<Option<Setting>> {
        self.chk(p)?;
        self.f.ler_setting(p, id)
    }
    fn gravar_setting(&mut self, p: &ProfileRef, id: u32, v: &SettingValue) -> Result<()> {
        self.chk(p)?;
        self.f.gravar_setting(p, id, v)
    }
    fn remover_setting(&mut self, p: &ProfileRef, id: u32, pv: bool) -> Result<()> {
        self.chk(p)?;
        self.f.remover_setting(p, id, pv)
    }
    fn settings_do_perfil(&self, p: &ProfileRef) -> Result<Vec<Setting>> {
        self.chk(p)?;
        self.f.settings_do_perfil(p)
    }
    fn capturar(&self, exes: &[&str]) -> Result<Snapshot> {
        self.f.capturar(exes)
    }
    fn salvar(&mut self) -> Result<()> {
        self.f.salvar()
    }
}

#[test]
fn erro_num_perfil_de_jogo_fica_isolado_e_o_global_segue() {
    let f = DriverFalso::turing();
    f.adicionar_perfil_predefinido("Jogo", &["jogo.exe"], &[]);
    let s = PerfilQuebrado {
        f: f.clone(),
        perfil: "Jogo".into(),
    };
    let mut e = comum::motor(s, Armazem::em_memoria());
    let jogos = vec![jogo_predefinido(), jogo_novo()];
    let diff = e
        .planejar(&jogos)
        .expect("um perfil quebrado não derruba o plano");
    assert_eq!(diff.ignorados.len(), 1);
    assert_eq!(diff.ignorados[0].exe, "jogo.exe");
    assert!(diff.mudancas.iter().any(|m| m.perfil == ProfileRef::Base));
    e.aplicar(&diff, true).unwrap();
    let p = ProfileRef::Named(NOME_NOVO.into());
    assert_eq!(f.valor(&p, id(&e, "dlss_preset")), Some(5));
}

// ── #2 um jogo derruba o plano ──────────────────────────────────────────────

/// Store que impõe o limite de 2047 unidades UTF-16 / NUL do `DrsReal`
/// (`to_unicode_string`) em toda chamada que recebe nome de perfil ou exe.
#[derive(Clone)]
struct Limite(DriverFalso);

fn nome_ok(s: &str) -> Result<()> {
    nvt_drs::to_unicode_string(s).map(|_| ())
}

fn ref_ok(p: &ProfileRef) -> Result<()> {
    match p {
        ProfileRef::Named(n) => nome_ok(n),
        ProfileRef::Base => Ok(()),
    }
}

impl DriverStore for Limite {
    fn versao_driver(&self) -> Result<DriverVersion> {
        self.0.versao_driver()
    }
    fn gpus(&self) -> Result<Vec<GpuInfo>> {
        self.0.gpus()
    }
    fn perfil_do_exe(&self, exe: &str) -> Result<Option<PerfilEncontrado>> {
        nome_ok(exe)?;
        self.0.perfil_do_exe(exe)
    }
    fn perfil_existe(&self, n: &str) -> Result<bool> {
        nome_ok(n)?;
        self.0.perfil_existe(n)
    }
    fn criar_perfil(&mut self, n: &str) -> Result<()> {
        nome_ok(n)?;
        self.0.criar_perfil(n)
    }
    fn criar_aplicacao(&mut self, p: &str, e: &str) -> Result<()> {
        nome_ok(p)?;
        nome_ok(e)?;
        self.0.criar_aplicacao(p, e)
    }
    fn ler_setting(&self, p: &ProfileRef, id: u32) -> Result<Option<Setting>> {
        ref_ok(p)?;
        self.0.ler_setting(p, id)
    }
    fn gravar_setting(&mut self, p: &ProfileRef, id: u32, v: &SettingValue) -> Result<()> {
        ref_ok(p)?;
        self.0.gravar_setting(p, id, v)
    }
    fn remover_setting(&mut self, p: &ProfileRef, id: u32, pv: bool) -> Result<()> {
        ref_ok(p)?;
        self.0.remover_setting(p, id, pv)
    }
    fn settings_do_perfil(&self, p: &ProfileRef) -> Result<Vec<Setting>> {
        ref_ok(p)?;
        self.0.settings_do_perfil(p)
    }
    fn capturar(&self, e: &[&str]) -> Result<Snapshot> {
        self.0.capturar(e)
    }
    fn salvar(&mut self) -> Result<()> {
        self.0.salvar()
    }
}

#[test]
fn jogo_com_nome_longo_nao_derruba_o_plano() {
    let f = DriverFalso::turing();
    let mut e = comum::motor(Limite(f.clone()), Armazem::em_memoria());
    let mut longo = JogoEntrada::new("A".repeat(3000), "longo.exe");
    longo.dlss = Some(Versao::new(310, 0, 0, 0));
    let mut nul = JogoEntrada::new("Com\0NUL", "nul.exe");
    nul.dlss = Some(Versao::new(310, 0, 0, 0));
    let mut exe_ruim = JogoEntrada::new("Exe Ruim", format!("{}.exe", "B".repeat(3000)));
    exe_ruim.dlss = Some(Versao::new(310, 0, 0, 0));
    let mut exe_nul = JogoEntrada::new("Exe NUL", "a\0b.exe");
    exe_nul.dlss = Some(Versao::new(310, 0, 0, 0));
    let jogos = vec![jogo_novo(), longo, nul, exe_ruim, exe_nul];

    let diff = e.planejar(&jogos).expect("planejar não falha por um jogo");
    // Executáveis inválidos: ignorados antes de qualquer chamada à NVAPI.
    let ignorados: Vec<&str> = diff.ignorados.iter().map(|i| i.nome.as_str()).collect();
    assert_eq!(ignorados, vec!["Exe Ruim", "Exe NUL"]);
    // Nome longo: truncado; NUL: removido.
    let longo = diff
        .criar
        .iter()
        .find(|c| c.exe == "longo.exe")
        .expect("nome longo vira nome limitado");
    assert_eq!(longo.nome.encode_utf16().count(), MAX_NOME_PERFIL);
    assert!(diff.criar.iter().any(|c| c.nome == "ComNUL"));
    assert!(diff.criar.iter().any(|c| c.nome == NOME_NOVO));
    assert!(diff.mudancas.iter().any(|m| m.perfil == ProfileRef::Base));

    aplicar(&mut e, &jogos);
    let p = ProfileRef::Named(longo.nome.clone());
    assert_eq!(f.valor(&p, id(&e, "dlss_preset")), Some(5));
}

// ── #3 settings que saíram do plano voltam ao original ──────────────────────

#[test]
fn desligar_limite_fps_volta_ao_original_e_reset_nao_regrava() {
    let d = DriverFalso::turing();
    let mut e = comum::motor(d.clone(), Armazem::em_memoria());
    let lim = id(&e, "limite_fps");
    let original = d.valor(&ProfileRef::Base, lim);
    e.definir_preferencias(prefs(
        Sincronismo::SemSync,
        Qualidade::Equilibrado,
        Some(141),
    ))
    .unwrap();
    aplicar(&mut e, &[]);
    assert_eq!(d.valor(&ProfileRef::Base, lim), Some(141));

    e.definir_preferencias(prefs(Sincronismo::SemSync, Qualidade::Equilibrado, None))
        .unwrap();
    let diff = e.planejar(&[]).unwrap();
    assert!(diff.reverter.iter().any(|r| r.esperado.id == lim));
    aplicar(&mut e, &[]);
    assert_eq!(
        d.valor(&ProfileRef::Base, lim),
        original,
        "volta ao original"
    );
    assert!(!d.proprias(&ProfileRef::Base).contains_key(&lim));
    assert!(
        e.estado().esperado.iter().all(|x| x.id != lim),
        "esperado podado"
    );
    assert!(e.planejar(&[]).unwrap().vazio());

    // Atualização de driver não regrava o limite que o usuário removeu.
    d.resetar_settings();
    d.mudar_versao(62200);
    let der = e.verificar_deriva().unwrap();
    e.tratar_deriva(&der, true).unwrap();
    assert_eq!(d.valor(&ProfileRef::Base, lim), original);
}

#[test]
fn limite_fps_volta_ao_valor_original_gravado_pelo_usuario() {
    let d = DriverFalso::turing();
    d.mudar_pelo_usuario(&ProfileRef::Base, 0x1083_5002, 60);
    let mut e = comum::motor(d.clone(), Armazem::em_memoria());
    let lim = id(&e, "limite_fps");
    assert_eq!(lim, 0x1083_5002);
    e.definir_preferencias(prefs(
        Sincronismo::SemSync,
        Qualidade::Equilibrado,
        Some(141),
    ))
    .unwrap();
    aplicar(&mut e, &[]);
    assert_eq!(d.valor(&ProfileRef::Base, lim), Some(141));
    e.definir_preferencias(prefs(Sincronismo::SemSync, Qualidade::Equilibrado, None))
        .unwrap();
    aplicar(&mut e, &[]);
    assert_eq!(d.valor(&ProfileRef::Base, lim), Some(60));
    let h = e.estado().historico.last().unwrap();
    assert_eq!((h.antes, h.depois), (Some(141), Some(60)));
}

fn ids_dlss<S: DriverStore>(e: &Engine<S>) -> [u32; 3] {
    [
        id(e, "dlss_override"),
        id(e, "dlss_perfil_preset"),
        id(e, "dlss_preset"),
    ]
}

#[test]
fn equilibrado_para_fidelidade_remove_as_settings_do_dlss() {
    let d = DriverFalso::turing();
    d.adicionar_perfil_predefinido("Jogo", &["jogo.exe"], &[]);
    let mut e = comum::motor(d.clone(), Armazem::em_memoria());
    let jogos = vec![jogo_predefinido()];
    aplicar(&mut e, &jogos);
    let p = ProfileRef::Named("Jogo".into());
    let ids = ids_dlss(&e);
    assert!(ids.iter().all(|i| d.proprias(&p).contains_key(i)));

    e.definir_preferencias(prefs(Sincronismo::SemSync, Qualidade::Fidelidade, None))
        .unwrap();
    aplicar(&mut e, &jogos);
    let proprias = d.proprias(&p);
    assert!(
        ids.iter().all(|i| !proprias.contains_key(i)),
        "as 3 settings do DLSS saem do perfil: {proprias:?}"
    );
    // Jogo fora da base competitiva: sem energia por jogo (F1-11, §11 item
    // 19), então nenhuma setting do perfil fica no esperado.
    assert!(e.estado().esperado.iter().all(|x| x.perfil != p));
    assert!(e.planejar(&jogos).unwrap().vazio());
}

#[test]
fn fidelidade_limpa_perfil_criado_pelo_app() {
    let d = DriverFalso::turing();
    let mut e = comum::motor(d.clone(), Armazem::em_memoria());
    let jogos = vec![jogo_novo()];
    aplicar(&mut e, &jogos);
    let p = ProfileRef::Named(NOME_NOVO.into());
    // Só as 3 do DLSS: jogo fora da base competitiva não recebe a energia
    // por jogo (spec competitivo F1-11, §11 item 19).
    assert_eq!(d.proprias(&p).len(), 3);
    e.definir_preferencias(prefs(Sincronismo::SemSync, Qualidade::Fidelidade, None))
        .unwrap();
    aplicar(&mut e, &jogos);
    assert!(d.proprias(&p).is_empty());
}

#[test]
fn jogo_fora_da_varredura_mantem_o_que_tinha() {
    let d = DriverFalso::turing();
    d.adicionar_perfil_predefinido("Jogo", &["jogo.exe"], &[]);
    let mut e = comum::motor(d.clone(), Armazem::em_memoria());
    aplicar(&mut e, &[jogo_predefinido()]);
    let diff = e.planejar(&[]).unwrap();
    assert!(diff.reverter.is_empty());
}

// ── #4 transições de sincronismo ────────────────────────────────────────────

#[test]
fn todas_as_transicoes_de_sincronismo_equivalem_a_aplicar_do_zero() {
    use Sincronismo::*;
    let todos = [SemSync, GSync, VSync];
    let do_zero = |s: Sincronismo| {
        let d = DriverFalso::turing();
        let mut e = comum::motor(d.clone(), Armazem::em_memoria());
        e.definir_preferencias(prefs(s, Qualidade::Equilibrado, None))
            .unwrap();
        aplicar(&mut e, &[]);
        d.proprias(&ProfileRef::Base)
    };
    for a in todos {
        for b in todos {
            if a == b {
                continue;
            }
            let d = DriverFalso::turing();
            let mut e = comum::motor(d.clone(), Armazem::em_memoria());
            e.definir_preferencias(prefs(a, Qualidade::Equilibrado, None))
                .unwrap();
            aplicar(&mut e, &[]);
            e.definir_preferencias(prefs(b, Qualidade::Equilibrado, None))
                .unwrap();
            aplicar(&mut e, &[]);
            assert_eq!(
                d.proprias(&ProfileRef::Base),
                do_zero(b),
                "transição {a:?} → {b:?}"
            );
            let gsync = d.valor(&ProfileRef::Base, id(&e, "gsync_global"));
            let ligado = valor_nomeado(&e, "gsync_global", "on");
            assert_eq!(gsync == Some(ligado), b == GSync, "G-SYNC em {a:?} → {b:?}");
        }
    }
}

#[test]
fn gsync_para_vsync_desliga_gsync() {
    let d = DriverFalso::turing();
    let mut e = comum::motor(d.clone(), Armazem::em_memoria());
    e.definir_preferencias(prefs(Sincronismo::GSync, Qualidade::Equilibrado, None))
        .unwrap();
    aplicar(&mut e, &[]);
    e.definir_preferencias(prefs(Sincronismo::VSync, Qualidade::Equilibrado, None))
        .unwrap();
    aplicar(&mut e, &[]);
    let b = ProfileRef::Base;
    assert_eq!(
        d.valor(&b, id(&e, "gsync_global")),
        Some(valor_nomeado(&e, "gsync_global", "off"))
    );
    assert_eq!(
        d.valor(&b, id(&e, "gsync_app_state")),
        Some(valor_nomeado(&e, "gsync_app_state", "forcar_off"))
    );
    assert!(!d.proprias(&b).contains_key(&id(&e, "gsync_app_mode")));
}

// ── #5 estado corrompido ────────────────────────────────────────────────────

fn arquivos(dir: &PathBuf) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn estado_vazio_ou_truncado_vira_corrompido_e_inicia_limpo() {
    for (n, conteudo) in [("vazio", ""), ("truncado", "{\"versao\":1,\"prefer")] {
        let dir = dir_teste(&format!("estado-{n}"));
        let d = DriverFalso::turing();
        {
            let mut e = comum::motor(d.clone(), Armazem::abrir(&dir).unwrap());
            aplicar(&mut e, &[]);
        }
        let snap = std::fs::read_to_string(dir.join("snapshot-original.json")).unwrap();
        // Sem cópia anterior válida: só um salvamento até aqui.
        let _ = std::fs::remove_file(dir.join("estado.anterior.json"));
        std::fs::write(dir.join("estado.json"), conteudo).unwrap();

        let a = Armazem::abrir(&dir).expect("estado inválido não prende o serviço");
        assert!(a.estado.esperado.is_empty(), "estado limpo");
        assert!(a.snapshot_original().is_some(), "snapshot preservado");
        assert!(!a.avisos().is_empty());
        let nomes = arquivos(&dir);
        // O estado limpo já foi gravado (a próxima abertura não repete o aviso).
        assert!(nomes.iter().any(|x| x == "estado.json"));
        let corrompido = nomes
            .iter()
            .find(|x| x.starts_with("estado.json.corrompido-"))
            .expect("arquivo inválido preservado com outro nome");
        assert_eq!(
            std::fs::read_to_string(dir.join(corrompido)).unwrap(),
            conteudo
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("snapshot-original.json")).unwrap(),
            snap
        );
        // O motor volta a funcionar a partir daí.
        let e = comum::motor(d.clone(), a);
        e.planejar(&[]).unwrap();
        e.armazem().salvar().unwrap();
        assert!(Armazem::abrir(&dir).unwrap().avisos().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn estado_truncado_e_recuperado_da_copia_anterior() {
    let dir = dir_teste("estado-anterior");
    let d = DriverFalso::turing();
    {
        let mut e = comum::motor(d.clone(), Armazem::abrir(&dir).unwrap());
        aplicar(&mut e, &[]);
        // Segundo salvamento: a versão anterior vira a cópia de segurança.
        e.armazem().salvar().unwrap();
    }
    assert!(dir.join("estado.anterior.json").exists());
    assert!(!dir.join("estado.json.tmp").exists());
    let bom = Armazem::abrir(&dir).unwrap().estado;
    let txt = std::fs::read_to_string(dir.join("estado.json")).unwrap();
    std::fs::write(dir.join("estado.json"), &txt[..txt.len() / 2]).unwrap();

    let a = Armazem::abrir(&dir).unwrap();
    assert_eq!(a.estado, bom);
    assert!(
        a.avisos()
            .iter()
            .any(|x| x.codigo == CodigoAviso::EstadoRecuperado)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn snapshot_truncado_ou_vazio_nao_trava_e_nao_e_sobrescrito() {
    for (n, conteudo) in [("vazio", ""), ("truncado", "{\"version\":1,\"pro")] {
        let dir = dir_teste(&format!("snap-{n}"));
        std::fs::write(dir.join("snapshot-original.json"), conteudo).unwrap();
        let a = Armazem::abrir(&dir).expect("snapshot ilegível não impede abrir");
        assert!(a.snapshot_original().is_none());
        let motivo = a.snapshot_indisponivel().expect("marcado indisponível");
        assert!(motivo.contains("restauração desabilitada"));

        let d = DriverFalso::turing();
        let mut e = comum::motor(d.clone(), a);
        let r = e.plano_restauracao();
        assert!(r.is_err(), "restauração recusada com aviso");
        assert!(e.restaurar(true).is_err());
        // O motor ainda aplica, mas nunca regrava o snapshot por cima.
        aplicar(&mut e, &[]);
        assert_eq!(
            std::fs::read_to_string(dir.join("snapshot-original.json")).unwrap(),
            conteudo
        );
        assert!(!dir.join("snapshot-original.json.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn snapshot_e_gravado_sem_temporario_sobrando_e_nunca_substituido() {
    let dir = dir_teste("snap-novo");
    let d = DriverFalso::turing();
    let mut a = Armazem::abrir(&dir).unwrap();
    assert!(
        a.guardar_snapshot_original(d.capturar(&[]).unwrap())
            .unwrap()
    );
    assert_eq!(arquivos(&dir), vec!["snapshot-original.json".to_owned()]);
    // Corrida: o arquivo apareceu depois da abertura; não é substituído.
    let mut c = Armazem::abrir(dir_teste("snap-novo-2")).unwrap();
    let dir2 = c.diretorio().unwrap().to_path_buf();
    std::fs::write(dir2.join("snapshot-original.json"), "original").unwrap();
    assert!(
        c.guardar_snapshot_original(d.capturar(&[]).unwrap())
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(dir2.join("snapshot-original.json")).unwrap(),
        "original"
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}

// ── #6 histórico limitado e coalescido ──────────────────────────────────────

#[test]
fn historico_tem_limite_e_descarta_as_mais_antigas() {
    let d = DriverFalso::turing();
    let mut a = Armazem::em_memoria();
    a.definir_limite_historico(10);
    let mut e = comum::motor(d.clone(), a);
    for limite in 100..130 {
        e.definir_preferencias(prefs(
            Sincronismo::SemSync,
            Qualidade::Equilibrado,
            Some(limite),
        ))
        .unwrap();
        aplicar(&mut e, &[]);
    }
    let h = &e.estado().historico;
    assert_eq!(h.len(), 10);
    assert_eq!(h.last().unwrap().depois, Some(129), "mantém as mais novas");
}

#[test]
fn limite_padrao_do_historico_e_2000() {
    assert_eq!(Armazem::em_memoria().limite_historico(), 2000);
}

/// Travar como o `nvt-service` faz (mudança pontual sintética).
fn travar<S: DriverStore>(e: &mut Engine<S>, setting: &str) {
    let id = id(e, setting);
    let atual = e
        .store()
        .ler_setting(&ProfileRef::Base, id)
        .unwrap()
        .and_then(|s| s.value.as_u32());
    let esperado = e
        .estado()
        .esperado
        .iter()
        .find(|x| x.perfil == ProfileRef::Base && x.id == id)
        .cloned()
        .unwrap_or(Esperado {
            alvo: Alvo::Global,
            perfil: ProfileRef::Base,
            setting: setting.into(),
            id,
            valor: atual.unwrap_or(0),
            origem: Origem::Preferencia {
                preferencia: "usuario".into(),
                regra: None,
            },
            antes_da_gestao: None,
        });
    let deriva = Deriva {
        classe: ClasseDeriva::Pontual,
        divergencias: vec![Divergencia {
            esperado,
            atual,
            voltou_ao_padrao: false,
        }],
        versao_driver: e.store().versao_driver().unwrap().raw,
    };
    e.tratar_deriva(&deriva, true).unwrap();
}

#[test]
fn travar_destravar_repetidos_sao_coalescidos() {
    let d = DriverFalso::turing();
    let mut e = comum::motor(d.clone(), Armazem::em_memoria());
    aplicar(&mut e, &[]);
    let antes = e.estado().historico.len();
    for _ in 0..200 {
        travar(&mut e, "energia");
        assert!(e.liberar_travada(&Alvo::Global, "energia").unwrap());
        aplicar(&mut e, &[]);
    }
    let usuario = e
        .estado()
        .historico
        .iter()
        .filter(|h| h.origem == OrigemRegistro::Usuario)
        .count();
    assert_eq!(usuario, 1);
    assert_eq!(e.estado().historico.len(), antes + 1);
}
