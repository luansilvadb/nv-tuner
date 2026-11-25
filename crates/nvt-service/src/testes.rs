//! Testes do laço e do tratador IPC sobre o `DriverFalso`, com diretório de
//! estado temporário e relógio controlado (sem SCM e sem driver real).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use nvt_core::{Alvo, Api, Versao};
use nvt_drs::ProfileRef;
use nvt_engine::{DriverFalso, JogoEntrada};
use nvt_ipc::{CodigoErro, NivelAviso, Requisicao, Resposta, StatusMudanca, codigo_aviso};

use crate::nucleo::{Assinatura, Config, Nucleo};

const EXE_WARDOGS: &str = "WardogsClient-Win64-Shipping.exe";
const EXE_LOL: &str = "League of Legends.exe";
const ID_VSYNC: u32 = 0x00A8_79CF;
const ID_PRESET: u32 = 0x10E4_1DF3;
const VSYNC_APP: u32 = 0x6092_5292;
const VSYNC_OFF: u32 = 0x0841_6747;
const VSYNC_ON: u32 = 0x4781_4940;

fn driver() -> DriverFalso {
    let d = DriverFalso::turing();
    d.definir_padrao(ID_VSYNC, VSYNC_APP);
    d.definir_padrao(0x0019_8FFF, 1);
    d.definir_padrao(0x1095_F16F, 1);
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

/// Diretório temporário removido no `Drop`.
struct DirTemp(PathBuf);

impl DirTemp {
    fn novo() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "nvt-service-teste-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        Self(p)
    }
}

impl Drop for DirTemp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cfg(somente_leitura: bool) -> Config {
    Config {
        somente_leitura,
        ..Config::default()
    }
}

/// Assinatura falsa do diretório do DRS, controlada pelo teste.
type Sinal = Rc<RefCell<Assinatura>>;

fn nucleo<'a>(
    d: &DriverFalso,
    dir: &DirTemp,
    c: Config,
    sinal: &Sinal,
    t: Instant,
) -> Nucleo<'a, DriverFalso> {
    let d = d.clone();
    let s = sinal.clone();
    Nucleo::novo(
        Box::new(move || Ok(d.clone())),
        Box::new(jogos),
        Box::new(move || Some(s.borrow().clone())),
        Some(dir.0.clone()),
        c,
        t,
    )
    .expect("núcleo")
}

fn sinal() -> Sinal {
    Rc::new(RefCell::new(vec![("nvdrsdb0.bin".into(), 100, 1)]))
}

fn wardogs() -> ProfileRef {
    ProfileRef::Named("Wardogs".into())
}

#[test]
fn aplicacao_inicial_com_snapshot_e_estado() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    n.iniciar(t0);
    assert_eq!(d.valor(&wardogs(), ID_PRESET), Some(5));
    assert_eq!(d.valor(&ProfileRef::Base, ID_VSYNC), Some(VSYNC_OFF));
    assert!(dir.0.join("snapshot-original.json").exists());
    assert!(dir.0.join("estado.json").exists());
    assert!(n.servico().ultima_aplicacao.is_some_and(|u| u.mudancas > 0));
    assert!(dir.0.join("servico.json").exists());
    // Segundo ciclo: nada a gravar.
    let g = d.gravacoes();
    n.ciclo(t0 + Duration::from_secs(60), "teste");
    assert_eq!(d.gravacoes(), g);
}

#[test]
fn reset_em_massa_reaplica_apos_debounce() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    n.iniciar(t0);
    d.resetar_settings();
    *s.borrow_mut() = vec![("nvdrsdb0.bin".into(), 90, 2)];
    let t1 = t0 + Duration::from_secs(60);
    assert!(n.notificar_mudanca_drs(t1));
    // Ainda dentro do debounce: nada.
    n.tick(t1 + Duration::from_secs(1));
    assert_eq!(d.valor(&wardogs(), ID_PRESET), None);
    n.tick(t1 + Duration::from_secs(4));
    assert!(!n.mudanca_pendente());
    assert_eq!(d.valor(&wardogs(), ID_PRESET), Some(5));
    assert_eq!(d.valor(&ProfileRef::Base, ID_VSYNC), Some(VSYNC_OFF));
    assert!(n.engine().estado().travadas.is_empty());
}

#[test]
fn mudanca_pontual_trava_na_verificacao_periodica() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    n.iniciar(t0);
    d.mudar_pelo_usuario(&ProfileRef::Base, ID_VSYNC, VSYNC_ON);
    n.tick(t0 + Duration::from_secs(11 * 60));
    let est = n.engine().estado();
    assert!(
        est.travadas
            .iter()
            .any(|t| t.alvo == Alvo::Global && t.setting == "vsync")
    );
    assert_eq!(d.valor(&ProfileRef::Base, ID_VSYNC), Some(VSYNC_ON));
    assert!(n.alertas().iter().any(|a| a.setting == "vsync"));
    // O próximo ciclo respeita a escolha.
    n.ciclo(t0 + Duration::from_secs(30 * 60), "teste");
    assert_eq!(d.valor(&ProfileRef::Base, ID_VSYNC), Some(VSYNC_ON));
}

#[test]
fn restaurar_original_pausa_autonomia_ate_aplicar_agora() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    n.iniciar(t0);
    assert_eq!(n.tratar(Requisicao::RestaurarOriginal, t0), Resposta::Ok);
    assert!(n.servico().autonomia_pausada);
    assert_eq!(d.valor(&ProfileRef::Base, ID_VSYNC), Some(VSYNC_APP));
    assert_eq!(d.valor(&wardogs(), ID_PRESET), None);

    // Ciclos periódicos e reinício do serviço não reaplicam.
    n.tick(t0 + Duration::from_secs(11 * 60));
    assert_eq!(d.valor(&wardogs(), ID_PRESET), None);
    let t1 = t0 + Duration::from_secs(3600);
    let mut n2 = nucleo(&d, &dir, cfg(false), &s, t1);
    assert!(n2.servico().autonomia_pausada);
    n2.iniciar(t1);
    assert_eq!(d.valor(&wardogs(), ID_PRESET), None);

    // Aplicar agora reativa.
    let Resposta::Plano(p) = n2.tratar(Requisicao::AplicarAgora, t1) else {
        panic!("esperava plano");
    };
    assert!(
        p.perfis
            .iter()
            .flat_map(|x| &x.mudancas)
            .any(|m| m.setting == "dlss_preset" && m.status == StatusMudanca::Aplicada)
    );
    assert!(!n2.servico().autonomia_pausada);
    assert_eq!(d.valor(&wardogs(), ID_PRESET), Some(5));
}

#[test]
fn ignora_as_proprias_gravacoes() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    n.iniciar(t0);
    // Dentro da janela após a gravação própria.
    assert!(!n.notificar_mudanca_drs(t0 + Duration::from_secs(2)));
    // Fora da janela, mas a assinatura é a registrada logo após gravar.
    assert!(!n.notificar_mudanca_drs(t0 + Duration::from_secs(40)));
    assert!(!n.mudanca_pendente());
    // Alguém mudou o arquivo: processa.
    *s.borrow_mut() = vec![("nvdrsdb0.bin".into(), 100, 99)];
    assert!(n.notificar_mudanca_drs(t0 + Duration::from_secs(41)));
    let g = d.gravacoes();
    n.tick(t0 + Duration::from_secs(45));
    // Sem deriva real: nada regravado.
    assert_eq!(d.gravacoes(), g);
}

#[test]
fn tratador_ipc_consulta_e_trava() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    n.iniciar(t0);

    let Resposta::Estado(e) = n.tratar(Requisicao::ObterEstado, t0) else {
        panic!("esperava estado");
    };
    assert_eq!(e.driver.as_ref().unwrap().versao, "621.40");
    assert_eq!(e.jogos, 2);
    assert!(e.gpu.as_ref().unwrap().capacidades.dlss);
    assert!(e.ultima_aplicacao.is_some());

    let Resposta::Plano(p) = n.tratar(Requisicao::ObterPlano, t0) else {
        panic!("esperava plano");
    };
    let w = p.perfis.iter().find(|x| x.perfil == "Wardogs").unwrap();
    assert_eq!(w.nome_legivel, "WARDOGS");
    let preset = w
        .mudancas
        .iter()
        .find(|m| m.setting == "dlss_preset")
        .unwrap();
    assert_eq!(preset.status, StatusMudanca::JaNoValor);
    assert_eq!(preset.nome_legivel, "DLSS - Forced Preset Letter");
    assert_eq!(preset.depois.as_deref(), Some("Preset E (CNN)"));

    let r = n.tratar(
        Requisicao::TravarSetting {
            perfil: "Wardogs".into(),
            setting: "nao_existe".into(),
        },
        t0,
    );
    assert!(matches!(
        r,
        Resposta::Erro {
            codigo: CodigoErro::SettingNaoPermitida,
            ..
        }
    ));
    let r = n.tratar(
        Requisicao::TravarSetting {
            perfil: "Perfil Inexistente".into(),
            setting: "dlss_preset".into(),
        },
        t0,
    );
    assert!(matches!(
        r,
        Resposta::Erro {
            codigo: CodigoErro::PerfilNaoEncontrado,
            ..
        }
    ));

    let g = d.gravacoes();
    let travar = Requisicao::TravarSetting {
        perfil: "Wardogs".into(),
        setting: "dlss_preset".into(),
    };
    assert_eq!(n.tratar(travar, t0), Resposta::Ok);
    assert_eq!(d.gravacoes(), g, "travar não grava no driver");
    let alvo = Alvo::Jogo("Wardogs".into());
    assert!(
        n.engine()
            .estado()
            .travadas
            .iter()
            .any(|t| t.alvo == alvo && t.setting == "dlss_preset")
    );
    let Resposta::Plano(p) = n.tratar(Requisicao::ObterPlano, t0) else {
        panic!();
    };
    assert!(
        p.perfis
            .iter()
            .flat_map(|x| &x.mudancas)
            .any(|m| m.setting == "dlss_preset" && m.status == StatusMudanca::Travada)
    );
    let destravar = Requisicao::DestravarSetting {
        perfil: "Wardogs".into(),
        setting: "dlss_preset".into(),
    };
    assert_eq!(n.tratar(destravar, t0), Resposta::Ok);
    assert!(n.engine().estado().travadas.is_empty());

    let Resposta::Historico { entradas } = n.tratar(Requisicao::ListarHistorico { limite: 3 }, t0)
    else {
        panic!();
    };
    assert_eq!(entradas.len(), 3);
    assert!(entradas[0].id > entradas[1].id, "mais recentes primeiro");
    assert!(entradas[0].quando_unix > 1_700_000_000);

    let prefs = Requisicao::DefinirPreferencias {
        sincronismo: nvt_ipc::Sincronismo::VSync,
        qualidade: nvt_ipc::Qualidade::Equilibrado,
        limite_fps: Some(141),
    };
    assert_eq!(n.tratar(prefs, t0), Resposta::Ok);
    assert_eq!(n.engine().estado().preferencias.limite_fps, Some(141));
    assert_eq!(n.tratar(Requisicao::ReescanearJogos, t0), Resposta::Ok);
}

#[test]
fn somente_leitura_nunca_grava() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    let mut n = nucleo(&d, &dir, cfg(true), &s, t0);
    n.iniciar(t0);
    assert_eq!(d.gravacoes(), 0);
    for req in [
        Requisicao::AplicarAgora,
        Requisicao::RestaurarOriginal,
        Requisicao::TravarSetting {
            perfil: "global".into(),
            setting: "vsync".into(),
        },
    ] {
        assert!(matches!(n.tratar(req, t0), Resposta::Erro { .. }));
    }
    assert!(matches!(
        n.tratar(Requisicao::ObterPlano, t0),
        Resposta::Plano(_)
    ));
    n.tick(t0 + Duration::from_secs(7 * 3600));
    assert_eq!(d.gravacoes(), 0);
    assert_eq!(d.salvamentos(), 0);
    assert!(!dir.0.exists(), "nenhum arquivo de estado criado");
}

#[test]
fn laco_atende_ipc_pelo_pipe_e_para() {
    use std::sync::mpsc;

    use nvt_ipc::pipe::{Cliente, ConfigCliente};

    use crate::laco::{self, Evento};

    let dir = DirTemp::novo();
    let caminho = dir.0.clone();
    let pipe = format!(r"\\.\pipe\nvt-service-teste-{}", std::process::id());
    let (tx, rx) = mpsc::channel::<Evento>();
    let (parada, h_ipc) = laco::iniciar_ipc(&pipe, tx.clone()).expect("pipe");
    let h = std::thread::spawn(move || {
        let d = driver();
        let s = sinal();
        let mut n = Nucleo::novo(
            Box::new(move || Ok(d.clone())),
            Box::new(jogos),
            Box::new(move || Some(s.borrow().clone())),
            Some(caminho),
            cfg(true),
            Instant::now(),
        )
        .unwrap();
        n.iniciar(Instant::now());
        laco::laco_principal(&mut n, &rx);
    });
    let cfg_cli = ConfigCliente {
        exigir_dono_privilegiado: false,
        ..ConfigCliente::default()
    };
    let mut c = Cliente::conectar(&pipe, &cfg_cli).expect("conectar");
    let Resposta::Estado(e) = c.requisitar(&Requisicao::ObterEstado).unwrap() else {
        panic!("esperava estado");
    };
    assert_eq!(e.jogos, 2);
    let r = c.requisitar(&Requisicao::AplicarAgora).unwrap();
    assert!(matches!(r, Resposta::Erro { .. }), "somente leitura recusa");
    drop(c);
    tx.send(Evento::Parar).unwrap();
    h.join().unwrap();
    parada.parar();
    h_ipc.join().unwrap();
}

#[test]
fn nomes_de_pipe_normalizados() {
    use crate::normalizar_pipe;
    let completo = r"\\.\pipe\nvt-teste";
    assert_eq!(normalizar_pipe(completo), completo);
    assert_eq!(normalizar_pipe("nvt-teste"), completo);
    assert_eq!(normalizar_pipe(r"\.\pipe\nvt-teste"), completo);
    assert_eq!(normalizar_pipe(r"pipe\nvt-teste"), completo);
}

// ── Avisos, reversões e jogos ignorados ───────────────────────────────────

fn estado_de(n: &mut Nucleo<'_, DriverFalso>, t: Instant) -> nvt_ipc::Estado {
    match n.tratar(Requisicao::ObterEstado, t) {
        Resposta::Estado(e) => e,
        outro => panic!("esperava estado, veio {outro:?}"),
    }
}

fn plano_de(n: &mut Nucleo<'_, DriverFalso>, req: Requisicao, t: Instant) -> nvt_ipc::Plano {
    match n.tratar(req, t) {
        Resposta::Plano(p) => p,
        outro => panic!("esperava plano, veio {outro:?}"),
    }
}

#[test]
fn estado_corrompido_vira_aviso_e_o_recuperado_e_gravado() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    {
        let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
        n.iniciar(t0);
        // Segunda gravação do estado: cria estado.anterior.json válido.
        n.tratar(
            Requisicao::TravarSetting {
                perfil: "Wardogs".into(),
                setting: "dlss_preset".into(),
            },
            t0,
        );
    }
    assert!(dir.0.join("estado.anterior.json").exists());
    std::fs::write(dir.0.join("estado.json"), b"{ truncado").unwrap();
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    let e = estado_de(&mut n, t0);
    let codigos: Vec<&str> = e.avisos.iter().map(|a| a.codigo.as_str()).collect();
    assert!(
        codigos.contains(&codigo_aviso::ESTADO_CORROMPIDO),
        "{codigos:?}"
    );
    assert!(
        codigos.contains(&codigo_aviso::ESTADO_RECUPERADO),
        "{codigos:?}"
    );
    assert!(e.restauracao_bloqueada().is_none());
    // O recuperado foi gravado: a próxima abertura acha estado.json válido.
    let texto = std::fs::read_to_string(dir.0.join("estado.json")).unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&texto).is_ok());
    assert!(
        !n.engine().estado().historico.is_empty(),
        "estado recuperado"
    );
    // Os avisos persistem nas próximas consultas (cada uma reabre o estado).
    let e = estado_de(&mut n, t0);
    assert_eq!(
        e.avisos
            .iter()
            .filter(|a| a.codigo == codigo_aviso::ESTADO_CORROMPIDO)
            .count(),
        1,
        "sem duplicar"
    );
    assert!(!n.engine().estado().historico.is_empty(), "não se perde");
}

#[test]
fn snapshot_ilegivel_bloqueia_a_restauracao() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    std::fs::create_dir_all(&dir.0).unwrap();
    std::fs::write(dir.0.join("snapshot-original.json"), b"").unwrap();
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    n.iniciar(t0);
    let e = estado_de(&mut n, t0);
    let a = e.restauracao_bloqueada().expect("aviso do snapshot");
    assert_eq!(a.nivel, NivelAviso::Erro);
    assert!(a.mensagem.contains("restauração está desabilitada"));
    assert_eq!(
        e.avisos
            .iter()
            .filter(|a| a.codigo == codigo_aviso::SNAPSHOT_INDISPONIVEL)
            .count(),
        1
    );
    assert_eq!(e.avisos[0].nivel, NivelAviso::Erro, "mais grave primeiro");
}

#[test]
fn reversao_aparece_no_plano_pendente_e_depois_aplicada() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    n.iniciar(t0);
    let prefs = Requisicao::DefinirPreferencias {
        sincronismo: nvt_ipc::Sincronismo::Desligado,
        qualidade: nvt_ipc::Qualidade::Equilibrado,
        limite_fps: Some(141),
    };
    assert_eq!(n.tratar(prefs, t0), Resposta::Ok);
    // Outro processo desliga o limite no estado salvo (o serviço relê o disco).
    let caminho = dir.0.join("estado.json");
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&caminho).unwrap()).unwrap();
    v["preferencias"]["limite_fps"] = serde_json::Value::Null;
    std::fs::write(&caminho, serde_json::to_string(&v).unwrap()).unwrap();

    let p = plano_de(&mut n, Requisicao::ObterPlano, t0);
    let r = p
        .reversoes
        .iter()
        .find(|r| r.setting == "limite_fps")
        .expect("limite de FPS sai do plano");
    assert_eq!(r.status, StatusMudanca::Pendente);
    assert_eq!(r.perfil, "global");
    assert!(r.motivo.starts_with("Saiu do plano porque"), "{}", r.motivo);
    assert!(r.atual.is_some());

    let p = plano_de(&mut n, Requisicao::AplicarAgora, t0);
    let r = p
        .reversoes
        .iter()
        .find(|r| r.setting == "limite_fps")
        .unwrap();
    assert_eq!(r.status, StatusMudanca::Aplicada);
    let p = plano_de(&mut n, Requisicao::ObterPlano, t0);
    assert!(p.reversoes.is_empty(), "revertida sai do estado esperado");
}

#[test]
fn jogo_invalido_e_ignorado_com_aviso() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    let d2 = d.clone();
    let s2 = s.clone();
    let mut n = Nucleo::novo(
        Box::new(move || Ok(d2.clone())),
        Box::new(|| {
            let mut v = jogos();
            v.push(JogoEntrada::new("Jogo Ruim", "ruim\u{0}.exe"));
            v
        }),
        Box::new(move || Some(s2.borrow().clone())),
        Some(dir.0.clone()),
        cfg(false),
        t0,
    )
    .unwrap();
    n.iniciar(t0);
    let p = plano_de(&mut n, Requisicao::ObterPlano, t0);
    assert_eq!(p.ignorados.len(), 1);
    assert_eq!(p.ignorados[0].nome, "Jogo Ruim");
    assert!(!p.ignorados[0].motivo.is_empty());
    let e = estado_de(&mut n, t0);
    let a = e
        .avisos
        .iter()
        .find(|a| a.codigo == codigo_aviso::JOGOS_IGNORADOS)
        .expect("aviso de jogos ignorados");
    assert_eq!(a.nivel, NivelAviso::Atencao);
    assert!(a.detalhe.as_deref().unwrap().starts_with("Jogo Ruim: "));
    // Os demais jogos seguem no plano.
    assert!(p.perfis.iter().any(|x| x.perfil == "Wardogs"));
}

#[test]
fn mapeamento_dos_avisos_do_armazem() {
    use crate::nucleo::{avisos_do_armazem, codigo_ipc};
    use nvt_engine::{Aviso as AvisoMotor, CodigoAviso, NivelAviso as NivelMotor};
    // Os códigos do motor são os do protocolo v2.
    for c in [
        CodigoAviso::EstadoCorrompido,
        CodigoAviso::EstadoRecuperado,
        CodigoAviso::EstadoReiniciado,
        CodigoAviso::SnapshotIndisponivel,
    ] {
        assert_eq!(codigo_ipc(c), c.como_str());
    }
    // Mapeado pelo código, com qualquer texto.
    let motor = [
        (NivelMotor::Atencao, CodigoAviso::EstadoCorrompido),
        (NivelMotor::Info, CodigoAviso::EstadoRecuperado),
        (NivelMotor::Atencao, CodigoAviso::EstadoReiniciado),
        (NivelMotor::Erro, CodigoAviso::SnapshotIndisponivel),
    ]
    .map(|(n, c)| AvisoMotor::novo(n, c, "texto qualquer", Some("detalhe".into())));
    let v = avisos_do_armazem(&motor, Some("detalhe"), 42);
    let r: Vec<(&str, NivelAviso)> = v.iter().map(|a| (a.codigo.as_str(), a.nivel)).collect();
    assert_eq!(
        r,
        [
            (codigo_aviso::ESTADO_CORROMPIDO, NivelAviso::Atencao),
            (codigo_aviso::ESTADO_RECUPERADO, NivelAviso::Info),
            (codigo_aviso::ESTADO_REINICIADO, NivelAviso::Atencao),
            (codigo_aviso::SNAPSHOT_INDISPONIVEL, NivelAviso::Erro),
        ]
    );
    assert!(v.iter().all(|a| a.quando_unix == 42
        && a.mensagem == "texto qualquer"
        && a.detalhe.as_deref() == Some("detalhe")));
    // Snapshot indisponível sem aviso correspondente também vira aviso.
    let v = avisos_do_armazem(&[], Some("motivo"), 1);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].codigo, codigo_aviso::SNAPSHOT_INDISPONIVEL);
    assert!(avisos_do_armazem(&[], None, 1).is_empty());
}

#[test]
fn somente_leitura_com_estado_corrompido_nao_mexe_no_diretorio() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    {
        let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
        n.iniciar(t0);
        n.tratar(
            Requisicao::TravarSetting {
                perfil: "Wardogs".into(),
                setting: "dlss_preset".into(),
            },
            t0,
        );
    }
    std::fs::write(dir.0.join("estado.json"), b"{ truncado").unwrap();
    let listar = || {
        let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (
                    e.file_name().to_string_lossy().into_owned(),
                    std::fs::read(e.path()).unwrap_or_default(),
                )
            })
            .collect();
        v.sort();
        v
    };
    let antes = listar();
    let g = d.gravacoes();
    let mut n = nucleo(&d, &dir, cfg(true), &s, t0);
    n.iniciar(t0);
    let e = estado_de(&mut n, t0);
    let codigos: Vec<&str> = e.avisos.iter().map(|a| a.codigo.as_str()).collect();
    assert!(
        codigos.contains(&codigo_aviso::ESTADO_CORROMPIDO)
            && codigos.contains(&codigo_aviso::ESTADO_RECUPERADO),
        "{codigos:?}"
    );
    assert!(
        !n.engine().estado().historico.is_empty(),
        "lido da cópia anterior"
    );
    n.ciclo(t0 + Duration::from_secs(60), "teste");
    let e = estado_de(&mut n, t0);
    assert_eq!(
        e.avisos
            .iter()
            .filter(|a| a.codigo == codigo_aviso::ESTADO_CORROMPIDO)
            .count(),
        1,
        "sem duplicar a cada reabertura"
    );
    assert_eq!(listar(), antes, "nada renomeado, criado nem gravado");
    assert_eq!(d.gravacoes(), g, "nada gravado no driver");
}

#[test]
fn motivo_de_reversao_legivel() {
    use crate::nucleo::motivo_reversao;
    let m = motivo_reversao(
        &nvt_core::Origem::Preferencia {
            preferencia: "limite_fps".into(),
            regra: None,
        },
        nvt_ipc::AcaoReversao::Remover,
    );
    assert_eq!(
        m,
        "Saiu do plano porque a preferência “limite_fps” mudou: sai do perfil (volta ao predefinido da NVIDIA, se houver)."
    );
    let m = motivo_reversao(
        &nvt_core::Origem::Regra {
            id: "dlss.preset".into(),
        },
        nvt_ipc::AcaoReversao::GravarOriginal,
    );
    assert!(m.contains("a regra “dlss.preset” não se aplica mais: volta ao valor original"));
}

// ── Reverificação (commit c20974c): regressões dos itens parciais ──────────

const ID_LIMITE: u32 = 0x1083_5002;

fn prefs_limite(limite_fps: Option<u32>) -> Requisicao {
    Requisicao::DefinirPreferencias {
        sincronismo: nvt_ipc::Sincronismo::Desligado,
        qualidade: nvt_ipc::Qualidade::Equilibrado,
        limite_fps,
    }
}

/// #4: limite 141 via IPC → usuário põe 100 no NPI → limite `None` ⇒ a
/// mudança manual vira trava do usuário (antes: limite final `None`).
#[test]
fn rv4_preferencia_durante_deriva_pendente_trava_a_escolha_do_usuario() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    n.iniciar(t0);
    assert_eq!(n.tratar(prefs_limite(Some(141)), t0), Resposta::Ok);
    assert_eq!(d.valor(&ProfileRef::Base, ID_LIMITE), Some(141));
    d.mudar_pelo_usuario(&ProfileRef::Base, ID_LIMITE, 100);
    assert_eq!(n.tratar(prefs_limite(None), t0), Resposta::Ok);
    assert_eq!(
        d.valor(&ProfileRef::Base, ID_LIMITE),
        Some(100),
        "escolha do usuário preservada"
    );
    let est = n.engine().estado();
    assert!(
        est.travadas.iter().any(|t| t.alvo == Alvo::Global
            && t.setting == "limite_fps"
            && t.valor_usuario == Some(100)),
        "{:?}",
        est.travadas
    );
    // E o próximo ciclo continua respeitando.
    n.ciclo(t0 + Duration::from_secs(60), "teste");
    assert_eq!(d.valor(&ProfileRef::Base, ID_LIMITE), Some(100));
}

/// #4, variante de `destravar`: mudança manual pendente noutra setting não
/// é revertida pelo plano disparado ao destravar.
#[test]
fn rv4_destravar_durante_deriva_pendente_nao_reverte_mudanca_manual() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    n.iniciar(t0);
    assert_eq!(n.tratar(prefs_limite(Some(141)), t0), Resposta::Ok);
    let travar = Requisicao::TravarSetting {
        perfil: "Wardogs".into(),
        setting: "dlss_preset".into(),
    };
    assert_eq!(n.tratar(travar, t0), Resposta::Ok);
    d.mudar_pelo_usuario(&ProfileRef::Base, ID_LIMITE, 100);
    let destravar = Requisicao::DestravarSetting {
        perfil: "WARDOGS".into(),
        setting: "dlss_preset".into(),
    };
    assert_eq!(n.tratar(destravar, t0), Resposta::Ok);
    assert_eq!(d.valor(&ProfileRef::Base, ID_LIMITE), Some(100));
    let est = n.engine().estado();
    assert!(est.travada(&Alvo::Global, "limite_fps"));
    assert!(
        !est.travada(&Alvo::Jogo("Wardogs".into()), "dlss_preset"),
        "destravado sem diferenciar maiúsculas"
    );
}

/// #7: trava do usuário → `estado.json` truncado → `status` (somente
/// leitura) → serviço normal: travas e histórico preservados, e a abertura
/// seguinte não volta ao estado vazio.
#[test]
fn rv7_somente_leitura_nao_faz_o_servico_perder_o_estado_recuperado() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    {
        let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
        n.iniciar(t0);
        let travar = Requisicao::TravarSetting {
            perfil: "global".into(),
            setting: "vsync".into(),
        };
        assert_eq!(n.tratar(travar, t0), Resposta::Ok);
        // Outra gravação: `estado.anterior.json` (a versão anterior) já tem
        // a trava do vsync.
        let travar = Requisicao::TravarSetting {
            perfil: "Wardogs".into(),
            setting: "dlss_preset".into(),
        };
        assert_eq!(n.tratar(travar, t0), Resposta::Ok);
    }
    std::fs::write(dir.0.join("estado.json"), b"{ truncado").unwrap();
    {
        // `nvt-service status`.
        let mut n = nucleo(&d, &dir, cfg(true), &s, t0);
        let _ = estado_de(&mut n, t0);
        assert!(n.engine().estado().travada(&Alvo::Global, "vsync"));
    }
    assert_eq!(
        std::fs::read(dir.0.join("estado.json")).unwrap(),
        b"{ truncado",
        "somente leitura não renomeia"
    );
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    let e = estado_de(&mut n, t0);
    assert!(
        e.avisos
            .iter()
            .any(|a| a.codigo == codigo_aviso::ESTADO_RECUPERADO),
        "{:?}",
        e.avisos
    );
    assert!(n.engine().estado().travada(&Alvo::Global, "vsync"));
    assert!(!n.engine().estado().historico.is_empty());
    drop(n);
    // Nova abertura (reinício do serviço): o recuperado já estava gravado.
    let n = nucleo(&d, &dir, cfg(false), &s, t0);
    assert!(n.engine().estado().travada(&Alvo::Global, "vsync"));
    assert!(!n.engine().estado().historico.is_empty());
    drop(n);
    // Mesmo que `estado.json` se perca, a cópia anterior é usada.
    std::fs::remove_file(dir.0.join("estado.json")).unwrap();
    let n = nucleo(&d, &dir, cfg(false), &s, t0);
    assert!(
        n.engine().estado().travada(&Alvo::Global, "vsync"),
        "não volta ao vazio"
    );
}

/// #10: 256 grafias de "league of legends" × 2 settings ⇒ 2 travas, com o
/// nome canônico do DRS.
#[test]
fn rv10_grafias_do_perfil_viram_uma_trava_por_setting() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    n.iniciar(t0);
    let base = "league of legends";
    let letras: Vec<usize> = base
        .char_indices()
        .filter(|(_, c)| c.is_ascii_alphabetic())
        .map(|(i, _)| i)
        .take(8)
        .collect();
    for mascara in 0u32..256 {
        let grafia: String = base
            .char_indices()
            .map(|(i, c)| match letras.iter().position(|&l| l == i) {
                Some(b) if mascara & (1 << b) != 0 => c.to_ascii_uppercase(),
                _ => c,
            })
            .collect();
        for setting in ["dlss_preset", "vsync"] {
            let r = n.tratar(
                Requisicao::TravarSetting {
                    perfil: grafia.clone(),
                    setting: setting.into(),
                },
                t0,
            );
            assert_eq!(r, Resposta::Ok, "{grafia} {setting}");
        }
    }
    let travadas = &n.engine().estado().travadas;
    assert_eq!(travadas.len(), 2, "{travadas:?}");
    assert!(
        travadas
            .iter()
            .all(|t| t.alvo == Alvo::Jogo("League of Legends".into())),
        "nome canônico do DRS"
    );
}

/// #10: perfil que existe no DRS mas não é gerenciável é recusado, e o
/// total de travas é limitado com erro claro.
#[test]
fn rv10_perfil_nao_gerenciavel_e_limite_de_travas() {
    let (d, dir, s, t0) = (driver(), DirTemp::novo(), sinal(), Instant::now());
    d.adicionar_perfil_predefinido("Outro Programa", &["outro.exe"], &[]);
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    n.iniciar(t0);
    let r = n.tratar(
        Requisicao::TravarSetting {
            perfil: "Outro Programa".into(),
            setting: "vsync".into(),
        },
        t0,
    );
    assert!(
        matches!(
            r,
            Resposta::Erro {
                codigo: CodigoErro::PerfilNaoEncontrado,
                ..
            }
        ),
        "{r:?}"
    );
    assert!(n.engine().estado().travadas.is_empty());
    drop(n);

    // Estado com o máximo de travas: travar outra é recusado.
    {
        let mut a = nvt_engine::Armazem::abrir(&dir.0).unwrap();
        for i in 0..nvt_engine::MAX_TRAVADAS {
            a.estado.travadas.push(nvt_engine::Travada {
                alvo: Alvo::Jogo(format!("Perfil {i}")),
                setting: "vsync".into(),
                valor_usuario: None,
                data: String::new(),
                motivo: String::new(),
            });
        }
        a.salvar().unwrap();
    }
    let mut n = nucleo(&d, &dir, cfg(false), &s, t0);
    let r = n.tratar(
        Requisicao::TravarSetting {
            perfil: "Wardogs".into(),
            setting: "dlss_preset".into(),
        },
        t0,
    );
    let Resposta::Erro { mensagem, .. } = r else {
        panic!("esperava erro, veio {r:?}");
    };
    assert!(mensagem.contains("limite de 500"), "{mensagem}");
    assert_eq!(n.engine().estado().travadas.len(), nvt_engine::MAX_TRAVADAS);
}
