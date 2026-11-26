//! F3-09, F3-10 e F3-12: contas Steam e Ubisoft, Steam Cloud do CS2, pastas conhecidas.

mod comum;

use comum::*;
use nvt_jogocfg::pastas::{PastasConhecidas, PastasFixas, PastasWindows};
use nvt_jogocfg::steam::{
    self, BASE_STEAMID64, ContaSteam, SteamPathFixo, conta_mais_recente, steamid3,
};
use nvt_jogocfg::ubisoft::{self, escolher, escolher_conta};
use nvt_jogocfg::{Erro, ler_valor};
use std::fs::{self, File};
use std::time::{Duration, SystemTime};

#[test]
fn steamid3_por_subtracao() {
    assert_eq!(steamid3(BASE_STEAMID64 + 1).unwrap(), 1);
    assert_eq!(steamid3(76_561_197_960_265_730).unwrap(), 2);
    assert_eq!(
        steamid3(BASE_STEAMID64 + u32::MAX as u64).unwrap(),
        u32::MAX
    );
    for x in [
        0,
        BASE_STEAMID64,
        BASE_STEAMID64 + (1 << 32),
        BASE_STEAMID64 - 1,
    ] {
        assert!(steamid3(x).is_err(), "{x}");
    }
}

#[test]
fn loginusers_sintetico_da_a_conta_most_recent() {
    let c = conta_mais_recente(&texto("loginusers.vdf")).unwrap();
    assert_eq!(
        c,
        ContaSteam {
            steamid64: 76_561_197_960_265_730,
            steamid3: 2
        }
    );
    let nenhuma =
        texto("loginusers.vdf").replace("\"MostRecent\"\t\t\"1\"", "\"MostRecent\"\t\t\"0\"");
    assert!(matches!(
        conta_mais_recente(&nenhuma),
        Err(Erro::ContaAmbigua)
    ));
    let duas =
        texto("loginusers.vdf").replace("\"MostRecent\"\t\t\"0\"", "\"MostRecent\"\t\t\"1\"");
    assert!(matches!(conta_mais_recente(&duas), Err(Erro::ContaAmbigua)));
    assert!(conta_mais_recente("\"users\" {").is_err());
}

fn steam_falsa(t: &Temp) -> std::path::PathBuf {
    let s = t.0.join("Steam");
    fs::create_dir_all(s.join("config")).unwrap();
    fs::write(s.join(r"config\loginusers.vdf"), amostra("loginusers.vdf")).unwrap();
    fs::create_dir_all(s.join(r"userdata\2\config")).unwrap();
    fs::write(
        s.join(r"userdata\2\config\localconfig.vdf"),
        amostra("localconfig.vdf"),
    )
    .unwrap();
    s
}

#[test]
fn resolve_localconfig_pelo_registro_injetado() {
    let t = Temp::novo("steam");
    let s = steam_falsa(&t);
    let com_barras = s.to_string_lossy().replace('\\', "/");
    assert_eq!(steam::normalizar(&com_barras), s);
    let inst = steam::resolver(&SteamPathFixo(Some(steam::normalizar(&com_barras)))).unwrap();
    assert_eq!(inst.conta.steamid3, 2);
    assert_eq!(
        inst.localconfig(),
        s.join(r"userdata\2\config\localconfig.vdf")
    );
    let lido = nvt_jogocfg::arquivo::ler(&s, &inst.localconfig()).unwrap();
    assert_eq!(
        ler_valor(&lido.bytes, &steam::caminho_launch_options(730)).unwrap(),
        "-novid -console +exec \"auto.cfg\""
    );
    assert!(matches!(
        steam::resolver(&SteamPathFixo(None)),
        Err(Erro::ArquivoAusente(_))
    ));
}

#[test]
fn cs2_video_local_remote() {
    let t = Temp::novo("cs2");
    let s = steam_falsa(&t);
    let inst = steam::resolver(&SteamPathFixo(Some(s.clone()))).unwrap();
    assert!(matches!(inst.cs2_video(), Err(Erro::ArquivoAusente(_))));
    let local = s.join(r"userdata\2\730\local\cfg");
    fs::create_dir_all(&local).unwrap();
    fs::write(local.join("cs2_video.txt"), amostra("cs2_video.txt")).unwrap();
    assert_eq!(inst.cs2_video().unwrap(), local.join("cs2_video.txt"));
    let remoto = s.join(r"userdata\2\730\remote\cfg");
    fs::create_dir_all(&remoto).unwrap();
    fs::write(remoto.join("cs2_video.txt"), amostra("cs2_video.txt")).unwrap();
    let e = inst.cs2_video().unwrap_err();
    assert!(matches!(e, Erro::ArquivoNaNuvem(_)));
    assert_eq!(e.motivo(), nvt_jogocfg::Motivo::ArquivoNaNuvem);
}

#[test]
fn escolha_pura_da_conta_ubisoft() {
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let t1 = t0 + Duration::from_secs(10);
    assert_eq!(escolher(&[]).unwrap(), None);
    assert_eq!(escolher(&[("a".into(), t0)]).unwrap().as_deref(), Some("a"));
    assert_eq!(
        escolher(&[("a".into(), t0), ("b".into(), t1)])
            .unwrap()
            .as_deref(),
        Some("b")
    );
    assert!(matches!(
        escolher(&[("a".into(), t1), ("b".into(), t1), ("c".into(), t0)]),
        Err(Erro::ContaAmbigua)
    ));
}

fn conta(raiz: &std::path::Path, id: &str, quando: SystemTime) {
    let p = raiz.join(id);
    fs::create_dir_all(&p).unwrap();
    let f = p.join(ubisoft::ARQUIVO);
    fs::write(&f, amostra("r6_GameSettings.ini")).unwrap();
    File::options()
        .write(true)
        .open(&f)
        .unwrap()
        .set_modified(quando)
        .unwrap();
}

#[test]
fn conta_ubisoft_unica_varias_e_empate() {
    let t = Temp::novo("ubisoft");
    let docs = t.0.join("Documentos");
    let jogo = docs.join(ubisoft::R6_RELATIVO);
    fs::create_dir_all(&jogo).unwrap();
    assert!(matches!(
        escolher_conta(&docs, &jogo),
        Err(Erro::ArquivoAusente(_))
    ));
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let id_a = "00000000-0000-4000-8000-00000000000a";
    let id_b = "00000000-0000-4000-8000-00000000000b";
    conta(&jogo, id_a, t0);
    fs::create_dir_all(jogo.join("pasta-sem-ini")).unwrap();
    let c = escolher_conta(&docs, &jogo).unwrap();
    assert_eq!(c.id, id_a);
    assert_eq!(c.arquivo, jogo.join(id_a).join("GameSettings.ini"));
    conta(&jogo, id_b, t0 + Duration::from_secs(60));
    assert_eq!(escolher_conta(&docs, &jogo).unwrap().id, id_b);
    conta(&jogo, id_a, t0 + Duration::from_secs(60));
    assert!(matches!(
        escolher_conta(&docs, &jogo),
        Err(Erro::ContaAmbigua)
    ));
    // Pasta de conta que é junção é ignorada.
    let fora = t.0.join("fora");
    conta(&fora, "x", t0 + Duration::from_secs(600));
    juncao(&jogo.join("conta-juncao"), &fora.join("x"));
    assert!(matches!(
        escolher_conta(&docs, &jogo),
        Err(Erro::ContaAmbigua)
    ));
}

#[test]
fn pastas_conhecidas_reais_e_injetadas() {
    let docs = PastasWindows.documentos().unwrap();
    let lad = PastasWindows.local_app_data().unwrap();
    assert!(docs.is_absolute() && docs.is_dir(), "{docs:?}");
    assert!(lad.is_absolute() && lad.is_dir(), "{lad:?}");
    let f = PastasFixas {
        documentos: "D:\\docs".into(),
        local_app_data: "D:\\lad".into(),
    };
    let dynp: &dyn PastasConhecidas = &f;
    assert_eq!(
        dynp.documentos().unwrap(),
        std::path::PathBuf::from("D:\\docs")
    );
    assert_eq!(
        dynp.local_app_data().unwrap(),
        std::path::PathBuf::from("D:\\lad")
    );
}
