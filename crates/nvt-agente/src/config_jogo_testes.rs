//! Testes da configuração de arquivo de jogo no agente. Só pastas temporárias com
//! amostras sintéticas (D12) e fachadas falsas de processos, pastas e Steam: nenhum
//! teste toca arquivos reais de jogos.

use std::{
    cell::{Cell, RefCell},
    collections::{BTreeSet, VecDeque},
    fs, io,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use nvt_detect::{FonteProcessos, ListaIndisponivel, Processo};
use nvt_engine::Portao;
use nvt_ipc::{
    AcaoJogo, DestinoJogoIpc, EstadoBackup, Fato, FatoConfigJogo, FatoTela, MotivoNaoAplicada,
    OperacaoUsuario, PlanoAgente, ReferenciaTarefa, Requisicao, Resposta, ResultadoJogo,
    SettingUsuario, StatusMudanca, TarefaJogo, ValorJogoIpc,
};
use nvt_jogocfg::pastas::PastasFixas;
use nvt_jogocfg::steam::SteamPathFixo;
use nvt_jogocfg::{Hash, Resultado, sha256};

use super::armazem::instancia_conta;
use super::{AmbienteJogo, GravadorJogo, GravadorReal};
use crate::{
    Agente, Controle, EscritaUsuario, Estado, LeituraUsuario, Persistencia, Transporte, ValorLido,
};

// ── Fachadas ────────────────────────────────────────────────────────────────

/// Lista de processos compartilhada; `None` = enumeração falha.
#[derive(Clone)]
struct Procs(Arc<Mutex<Option<Vec<String>>>>);

impl Procs {
    fn definir(&self, nomes: Option<&[&str]>) {
        *self.0.lock().unwrap() = nomes.map(|n| n.iter().map(|s| (*s).to_owned()).collect());
    }
}

impl FonteProcessos for Procs {
    fn processos(&self) -> Vec<Processo> {
        self.ler().unwrap_or_default()
    }

    fn ler(&self) -> Result<Vec<Processo>, ListaIndisponivel> {
        match &*self.0.lock().unwrap() {
            Some(v) => Ok(v
                .iter()
                .map(|n| Processo {
                    nome: n.clone(),
                    pid: 10,
                    pid_pai: 1,
                })
                .collect()),
            None => Err(ListaIndisponivel { codigo: 0 }),
        }
    }
}

#[derive(Clone, Default)]
struct Memoria(Rc<RefCell<Vec<u8>>>);

impl Persistencia for Memoria {
    fn carregar(&self) -> io::Result<Estado> {
        let d = self.0.borrow();
        if d.is_empty() {
            return Ok(Estado::default());
        }
        Ok(serde_json::from_slice(&d)?)
    }

    fn salvar(&self, estado: &Estado) -> io::Result<()> {
        *self.0.borrow_mut() = serde_json::to_vec(estado)?;
        Ok(())
    }
}

#[derive(Clone, Default)]
struct Fio {
    respostas: Rc<RefCell<VecDeque<Resposta>>>,
    pedidos: Rc<RefCell<Vec<Requisicao>>>,
}

impl Transporte for Fio {
    fn trocar(&mut self, r: &Requisicao) -> io::Result<Resposta> {
        self.pedidos.borrow_mut().push(r.clone());
        self.respostas
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| io::Error::other("sem resposta"))
    }
}

/// Sem destinos Windows: o teste cobre só a família jogo.
struct SemWindows;

impl LeituraUsuario for SemWindows {
    fn ler(&self, _: &SettingUsuario) -> io::Result<ValorLido> {
        Err(io::Error::other("fora do teste"))
    }

    fn tela(&self) -> Fato<FatoTela> {
        Fato::Desconhecido
    }
}

impl EscritaUsuario for SemWindows {
    fn escrever(
        &mut self,
        _: &SettingUsuario,
        _: &ValorLido,
        _: &ValorLido,
        _: bool,
        _: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        unreachable!("sem destinos Windows")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Queda {
    Nenhuma,
    /// Temporário escrito, queda antes do `ReplaceFileW`.
    Antes,
    /// `ReplaceFileW` feito, queda antes de persistir o resultado.
    Depois,
}

#[derive(Clone)]
struct Grav {
    gravacoes: Rc<Cell<usize>>,
    queda: Rc<Cell<Queda>>,
}

impl GravadorJogo for Grav {
    fn gravar(&self, raiz: &Path, caminho: &Path, novo: &[u8], hash: &Hash) -> Resultado<Hash> {
        if self.queda.get() == Queda::Antes {
            let nome = caminho.file_name().unwrap().to_string_lossy();
            fs::write(caminho.with_file_name(format!("{nome}.nvt-9-9.tmp")), novo).unwrap();
            panic!("queda antes do ReplaceFileW");
        }
        let h = GravadorReal.gravar(raiz, caminho, novo, hash)?;
        self.gravacoes.set(self.gravacoes.get() + 1);
        assert!(
            self.queda.get() != Queda::Depois,
            "queda depois do ReplaceFileW"
        );
        Ok(h)
    }
}

// ── Cenário com amostras sintéticas ─────────────────────────────────────────

const CONTA_UBI: &str = "0f0e0d0c-aaaa-bbbb-cccc-000000000001";
const STEAMID64: &str = "76561197960265729";

const R6: &[u8] = b"; amostra sintetica\r\n[DISPLAY]\r\nFPSLimit=60\r\nVSync=0\r\n";
const RL: &[u8] = b"[SystemSettings]\r\nOneFrameThreadLag=True\r\nMaxSmoothedFrameRate=144\r\n";
const LOGIN: &str = "\"users\"\n{\n\t\"76561197960265729\"\n\t{\n\t\t\"AccountName\"\t\t\"teste\"\n\t\t\"MostRecent\"\t\t\"1\"\n\t}\n}\n";
const LOCALCONFIG: &str = "\"UserLocalConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"apps\"\n\t\t\t\t{\n\t\t\t\t\t\"730\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"LaunchOptions\"\t\t\"-novid\"\n\t\t\t\t\t}\n\t\t\t\t\t\"570\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"LaunchOptions\"\t\t\"\"\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n";

static SEQ: AtomicU32 = AtomicU32::new(0);

struct Cenario {
    raiz: PathBuf,
    procs: Procs,
    mem: Memoria,
    fio: Fio,
    grav: Grav,
    revisao: u64,
    geracao: u64,
    agora: Instant,
}

impl Drop for Cenario {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.raiz);
    }
}

impl Cenario {
    fn novo() -> Self {
        let raiz = std::env::temp_dir().join(format!(
            "nvt-agente-jogo-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&raiz);
        let c = Self {
            raiz,
            procs: Procs(Arc::new(Mutex::new(Some(vec!["explorer.exe".into()])))),
            mem: Memoria::default(),
            fio: Fio::default(),
            grav: Grav {
                gravacoes: Rc::new(Cell::new(0)),
                queda: Rc::new(Cell::new(Queda::Nenhuma)),
            },
            revisao: 0,
            geracao: 0,
            agora: Instant::now(),
        };
        escrever(&c.r6(), R6);
        escrever(&c.rl(), RL);
        escrever(&c.steam().join(r"config\loginusers.vdf"), LOGIN.as_bytes());
        escrever(&c.localconfig(), LOCALCONFIG.as_bytes());
        c
    }

    fn docs(&self) -> PathBuf {
        self.raiz.join("Documentos")
    }
    fn local(&self) -> PathBuf {
        self.raiz.join("Local")
    }
    fn steam(&self) -> PathBuf {
        self.raiz.join("Steam")
    }
    fn r6(&self) -> PathBuf {
        self.docs()
            .join(r"My Games\Rainbow Six - Siege")
            .join(CONTA_UBI)
            .join("GameSettings.ini")
    }
    fn rl(&self) -> PathBuf {
        self.docs()
            .join(r"My Games\Rocket League\TAGame\Config\TASystemSettings.ini")
    }
    fn localconfig(&self) -> PathBuf {
        self.steam().join(r"userdata\1\config\localconfig.vdf")
    }
    fn backups(&self) -> PathBuf {
        self.local().join(r"nv-tuner\backups")
    }

    fn agente(&self) -> Agente<SemWindows, Fio> {
        let mut a = Agente::novo(
            SemWindows,
            self.fio.clone(),
            Portao::new(self.procs.clone()),
            1,
        )
        .unwrap();
        a.definir_ambiente_jogo(AmbienteJogo {
            pastas: Box::new(PastasFixas {
                documentos: self.docs(),
                local_app_data: self.local(),
            }),
            steam: Box::new(SteamPathFixo(Some(self.steam()))),
            gravador: Box::new(self.grav.clone()),
        });
        a
    }

    fn plano(&mut self, acoes: Vec<AcaoJogo>, operacao: OperacaoUsuario) -> PlanoAgente {
        self.revisao += 1;
        let tarefa = ReferenciaTarefa {
            id_tarefa: self.revisao,
            operacao,
            revisao: self.revisao,
        };
        PlanoAgente {
            revisao: self.revisao,
            geracao_restauracao: self.geracao,
            tarefa: None,
            processos_vigiados: BTreeSet::new(),
            tarefa_jogo: (!acoes.is_empty()).then_some(TarefaJogo { tarefa, acoes }),
        }
    }

    /// Um passo do agente; devolve os fatos enviados e os resultados relatados.
    fn passo_com(
        &mut self,
        a: &mut Agente<SemWindows, Fio>,
        plano: PlanoAgente,
        restaurar: bool,
    ) -> (
        io::Result<Controle>,
        Vec<FatoConfigJogo>,
        Vec<ResultadoJogo>,
    ) {
        self.agora += Duration::from_secs(61);
        self.fio.pedidos.borrow_mut().clear();
        *self.fio.respostas.borrow_mut() =
            VecDeque::from([Resposta::PlanoAgente(plano), Resposta::Ok]);
        let controle = a.passo(&self.mem, self.agora, restaurar);
        let mut fatos = Vec::new();
        let mut resultados = Vec::new();
        for p in self.fio.pedidos.borrow().iter() {
            match p {
                Requisicao::AgenteConsultar { fatos: f, .. } => fatos = f.config_jogo.clone(),
                Requisicao::AgenteRelatar {
                    resultados_jogo, ..
                } => resultados = resultados_jogo.clone(),
                _ => {}
            }
        }
        (controle, fatos, resultados)
    }

    fn rodar(
        &mut self,
        a: &mut Agente<SemWindows, Fio>,
        acoes: Vec<AcaoJogo>,
    ) -> (Vec<FatoConfigJogo>, Vec<ResultadoJogo>) {
        let operacao = match acoes.first() {
            Some(AcaoJogo::Reverter { .. }) => OperacaoUsuario::Reverter,
            Some(AcaoJogo::Restaurar { .. }) => OperacaoUsuario::Restaurar,
            _ => OperacaoUsuario::Aplicar,
        };
        let plano = self.plano(acoes, operacao);
        let (c, f, r) = self.passo_com(a, plano, false);
        assert_eq!(c.unwrap(), Controle::Continuar);
        (f, r)
    }

    fn fatos(&mut self, a: &mut Agente<SemWindows, Fio>) -> Vec<FatoConfigJogo> {
        self.rodar(a, Vec::new()).0
    }

    fn estado(&self) -> Estado {
        self.mem.carregar().unwrap()
    }
}

fn escrever(p: &Path, bytes: &[u8]) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, bytes).unwrap();
}

fn fato<'a>(fatos: &'a [FatoConfigJogo], arquivo: &str) -> &'a FatoConfigJogo {
    fatos
        .iter()
        .find(
            |f| matches!(&f.alvo, nvt_ipc::AlvoIpc::ConfigJogo { arquivo: a, .. } if a == arquivo),
        )
        .unwrap_or_else(|| panic!("sem fato de {arquivo}"))
}

fn destino(f: &FatoConfigJogo, setting: &str) -> DestinoJogoIpc {
    DestinoJogoIpc {
        alvo: f.alvo.clone(),
        setting: setting.into(),
        instancia: f.instancia.clone(),
    }
}

fn aplicar(f: &FatoConfigJogo, setting: &str, valor: ValorJogoIpc) -> AcaoJogo {
    AcaoJogo::Aplicar {
        destino: destino(f, setting),
        valor,
        hash_esperado: f.hash.clone().unwrap(),
    }
}

fn restaurar(f: &FatoConfigJogo, setting: &str) -> AcaoJogo {
    AcaoJogo::Restaurar {
        destino: destino(f, setting),
        hash_esperado: f.hash.clone().unwrap(),
    }
}

fn so(rs: &[ResultadoJogo]) -> &ResultadoJogo {
    assert_eq!(rs.len(), 1, "{rs:?}");
    &rs[0]
}

fn janela(processos: &[&str]) -> Option<MotivoNaoAplicada> {
    Some(MotivoNaoAplicada::JanelaFechadaExigida {
        processos: processos.iter().map(|s| (*s).to_owned()).collect(),
    })
}

fn ler(p: &Path) -> Vec<u8> {
    fs::read(p).unwrap()
}

fn r6_com(fps: &str) -> Vec<u8> {
    String::from_utf8(R6.to_vec())
        .unwrap()
        .replace("FPSLimit=60", &format!("FPSLimit={fps}"))
        .into_bytes()
}

// ── Testes ──────────────────────────────────────────────────────────────────

#[test]
fn fatos_so_do_catalogo_anonimizados() {
    let mut c = Cenario::novo();
    escrever(&c.docs().join(r"My Games\outro.ini"), b"[x]\r\ny=1\r\n");
    let mut a = c.agente();
    let fatos = c.fatos(&mut a);
    let r6 = fato(&fatos, "r6-game-settings");
    assert_eq!(r6.instancia, instancia_conta("r6-siege", CONTA_UBI));
    assert!(r6.presente && !r6.somente_leitura && !r6.na_nuvem && !r6.conta_ambigua);
    assert_eq!(r6.hash.as_deref(), Some(sha256(R6).hex().as_str()));
    assert_eq!(r6.backup, EstadoBackup::SemBackup);
    assert_eq!(
        r6.rotulo.0,
        r"%USERPROFILE%\Documents\My Games\Rainbow Six - Siege\<conta>\GameSettings.ini"
    );
    let valor = |f: &FatoConfigJogo, s: &str| {
        f.valores
            .iter()
            .find(|v| v.setting == s)
            .map(|v| v.valor.clone())
    };
    assert_eq!(
        valor(r6, "r6_fps_limit"),
        Some(Fato::Conhecido(ValorJogoIpc::Inteiro(60)))
    );
    assert_eq!(valor(r6, "r6_reflex"), None, "chave a confirmar não é lida");
    let rl = fato(&fatos, "rl-ta-system-settings");
    assert_eq!(rl.instancia, "local");
    assert_eq!(
        valor(rl, "rl_one_frame_thread_lag"),
        Some(Fato::Conhecido(ValorJogoIpc::Booleano(true)))
    );
    let cs2 = fato(&fatos, "cs2-launch");
    assert_eq!(
        cs2.rotulo.0,
        r"%STEAM%\userdata\<steamid3>\config\localconfig.vdf"
    );
    assert_eq!(
        valor(cs2, "cs2_fps_max"),
        Some(Fato::Conhecido(ValorJogoIpc::TokenLancamento(None)))
    );
    assert!(
        !fatos.iter().any(
            |f| matches!(&f.alvo, nvt_ipc::AlvoIpc::ConfigJogo { jogo, .. } if jogo == "fortnite")
        ),
        "arquivo ausente não é relatado"
    );
    let json = serde_json::to_string(&fatos).unwrap();
    for proibido in [
        c.raiz.to_string_lossy().as_ref(),
        CONTA_UBI,
        STEAMID64,
        "outro.ini",
        "\\Users\\",
    ] {
        assert!(!json.contains(proibido), "{proibido} vazou: {json}");
    }
    for f in &fatos {
        f.validar().unwrap();
    }
}

#[test]
fn aplica_com_backup_unico_e_imutavel_e_gestao_reidratada() {
    let mut c = Cenario::novo();
    let mut a = c.agente();
    let fatos = c.fatos(&mut a);
    let r6 = fato(&fatos, "r6-game-settings").clone();
    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(144))],
    );
    let r = so(&rs);
    assert_eq!(r.status, StatusMudanca::Aplicada, "{r:?}");
    assert_eq!(r.antes, Some(ValorJogoIpc::Inteiro(60)));
    assert_eq!(r.depois, Some(ValorJogoIpc::Inteiro(144)));
    assert_eq!(r.hash_antes.as_deref(), Some(sha256(R6).hex().as_str()));
    assert_eq!(ler(&c.r6()), r6_com("144"), "só os bytes do valor mudam");
    r.validar().unwrap();

    let nome = format!("r6-game-settings-{}.orig", r6.instancia);
    let orig = c.backups().join("r6-siege").join(&nome);
    assert_eq!(ler(&orig), R6);
    let soma = c.backups().join("r6-siege").join(format!("{nome}.sha256"));
    assert_eq!(ler(&soma), sha256(R6).hex().as_bytes());
    let mtime = fs::metadata(&orig).unwrap().modified().unwrap();

    let fatos = c.fatos(&mut a);
    let r6 = fato(&fatos, "r6-game-settings").clone();
    assert_eq!(r6.backup, EstadoBackup::Integro);
    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(240))],
    );
    assert_eq!(so(&rs).status, StatusMudanca::Aplicada);
    assert_eq!(ler(&orig), R6, "backup imutável");
    assert_eq!(fs::metadata(&orig).unwrap().modified().unwrap(), mtime);
    assert_eq!(c.grav.gravacoes.get(), 2);

    // Gestão reidratada mesmo num agente novo (reinício).
    let mut a = c.agente();
    let fatos = c.fatos(&mut a);
    let g = &fato(&fatos, "r6-game-settings").gestao;
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].setting, "r6_fps_limit");
    assert_eq!(g[0].antes, ValorJogoIpc::Inteiro(60));
    assert_eq!(g[0].esperado, ValorJogoIpc::Inteiro(240));
    assert_eq!(g[0].hash_antes, sha256(R6).hex());
    assert_eq!(g[0].hash_depois, sha256(&r6_com("240")).hex());
    assert!(!g[0].travado);

    // Já no valor: nada é gravado.
    let r6 = fato(&fatos, "r6-game-settings").clone();
    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(240))],
    );
    assert_eq!(so(&rs).status, StatusMudanca::JaNoValor);
    assert_eq!(c.grav.gravacoes.get(), 2);
}

#[test]
fn jogo_aberto_steam_aberta_e_lista_ilegivel_bloqueiam() {
    let mut c = Cenario::novo();
    let mut a = c.agente();
    let fatos = c.fatos(&mut a);
    let r6 = fato(&fatos, "r6-game-settings").clone();
    let cs2 = fato(&fatos, "cs2-launch").clone();

    c.procs.definir(Some(&["explorer.exe", "rainbowsix.exe"]));
    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(144))],
    );
    let r = so(&rs);
    assert_eq!(r.status, StatusMudanca::Pendente);
    assert_eq!(r.motivo, janela(&["RainbowSix.exe"]));
    r.validar().unwrap();
    assert_eq!(ler(&c.r6()), R6);
    assert!(!c.backups().exists(), "sem backup sem escrita");

    // Steam aberta bloqueia só o arquivo da Steam.
    c.procs.definir(Some(&["steam.exe"]));
    let (_, rs) = c.rodar(
        &mut a,
        vec![
            aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(144)),
            aplicar(
                &cs2,
                "cs2_fps_max",
                ValorJogoIpc::TokenLancamento(Some(144)),
            ),
        ],
    );
    let r6r = rs
        .iter()
        .find(|r| r.destino.setting == "r6_fps_limit")
        .unwrap();
    let cs2r = rs
        .iter()
        .find(|r| r.destino.setting == "cs2_fps_max")
        .unwrap();
    assert_eq!(r6r.status, StatusMudanca::Aplicada);
    assert_eq!(cs2r.status, StatusMudanca::Pendente);
    assert_eq!(cs2r.motivo, janela(&["steam.exe"]));
    assert_eq!(ler(&c.localconfig()), LOCALCONFIG.as_bytes());

    // Enumeração ilegível: falha fechada.
    c.procs.definir(None);
    let fatos = c.fatos(&mut a);
    let r6 = fato(&fatos, "r6-game-settings").clone();
    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(200))],
    );
    assert_eq!(so(&rs).status, StatusMudanca::Pendente);
    assert_eq!(so(&rs).motivo, None);
    assert_eq!(ler(&c.r6()), r6_com("144"));
    assert_eq!(c.grav.gravacoes.get(), 1);

    // Steam fechada: o token entra preservando o resto.
    c.procs.definir(Some(&[]));
    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(
            &cs2,
            "cs2_fps_max",
            ValorJogoIpc::TokenLancamento(Some(144)),
        )],
    );
    assert_eq!(so(&rs).status, StatusMudanca::Aplicada);
    let texto = String::from_utf8(ler(&c.localconfig())).unwrap();
    assert!(texto.contains("\"-novid +fps_max 144\""), "{texto}");
}

#[test]
fn hash_divergente_recusa_sem_backup() {
    let mut c = Cenario::novo();
    let mut a = c.agente();
    let fatos = c.fatos(&mut a);
    let mut r6 = fato(&fatos, "r6-game-settings").clone();
    r6.hash = Some(sha256(b"outro conteudo").hex());
    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(144))],
    );
    assert_eq!(so(&rs).status, StatusMudanca::Recusada);
    assert_eq!(
        so(&rs).motivo,
        Some(MotivoNaoAplicada::ArquivoAlteradoPeloJogo)
    );
    assert_eq!(ler(&c.r6()), R6);
    assert!(!c.backups().exists());
    assert_eq!(c.grav.gravacoes.get(), 0);
}

#[test]
fn restauracao_integral_parcial_e_alterada_pelo_jogo() {
    let mut c = Cenario::novo();
    let mut a = c.agente();
    let aplicar_r6 = |c: &mut Cenario, a: &mut Agente<SemWindows, Fio>, n| {
        let f = c.fatos(a);
        let r6 = fato(&f, "r6-game-settings").clone();
        let (_, rs) = c.rodar(
            a,
            vec![aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(n))],
        );
        assert_eq!(so(&rs).status, StatusMudanca::Aplicada);
    };
    let restaurar_r6 = |c: &mut Cenario, a: &mut Agente<SemWindows, Fio>| {
        let f = c.fatos(a);
        let r6 = fato(&f, "r6-game-settings").clone();
        c.rodar(a, vec![restaurar(&r6, "r6_fps_limit")]).1
    };

    // Integral: o arquivo não mudou desde a gravação → original byte a byte.
    aplicar_r6(&mut c, &mut a, 144);
    let rs = restaurar_r6(&mut c, &mut a);
    assert_eq!(so(&rs).status, StatusMudanca::Aplicada);
    assert_eq!(ler(&c.r6()), R6);
    let f = c.fatos(&mut a);
    assert!(fato(&f, "r6-game-settings").gestao.is_empty());

    // Parcial: o jogo regravou outra linha → só a chave do app volta.
    aplicar_r6(&mut c, &mut a, 144);
    let jogo = String::from_utf8(ler(&c.r6()))
        .unwrap()
        .replace("VSync=0", "VSync=1");
    fs::write(c.r6(), &jogo).unwrap();
    let rs = restaurar_r6(&mut c, &mut a);
    assert_eq!(so(&rs).status, StatusMudanca::Aplicada);
    assert_eq!(
        String::from_utf8(ler(&c.r6())).unwrap(),
        jogo.replace("FPSLimit=144", "FPSLimit=60")
    );

    // A chave do app mudada pelo jogo: nada é tocado.
    fs::write(c.r6(), R6).unwrap();
    aplicar_r6(&mut c, &mut a, 144);
    fs::write(c.r6(), r6_com("90")).unwrap();
    let rs = restaurar_r6(&mut c, &mut a);
    assert_eq!(so(&rs).status, StatusMudanca::Travada);
    assert_eq!(
        so(&rs).motivo,
        Some(MotivoNaoAplicada::ArquivoAlteradoPeloJogo)
    );
    assert_eq!(ler(&c.r6()), r6_com("90"));

    // Token de lançamento: restauração devolve a string original.
    let f = c.fatos(&mut a);
    let cs2 = fato(&f, "cs2-launch").clone();
    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(
            &cs2,
            "cs2_fps_max",
            ValorJogoIpc::TokenLancamento(Some(200)),
        )],
    );
    assert_eq!(so(&rs).status, StatusMudanca::Aplicada);
    let f = c.fatos(&mut a);
    let cs2 = fato(&f, "cs2-launch").clone();
    let (_, rs) = c.rodar(&mut a, vec![restaurar(&cs2, "cs2_fps_max")]);
    assert_eq!(so(&rs).status, StatusMudanca::Aplicada);
    assert_eq!(ler(&c.localconfig()), LOCALCONFIG.as_bytes());
}

#[test]
fn somente_leitura_nao_grava_nem_muda_atributo() {
    let mut c = Cenario::novo();
    let mut perm = fs::metadata(c.r6()).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(c.r6(), perm.clone()).unwrap();
    let mut a = c.agente();
    let fatos = c.fatos(&mut a);
    let r6 = fato(&fatos, "r6-game-settings").clone();
    assert!(r6.somente_leitura);
    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(144))],
    );
    assert_eq!(so(&rs).status, StatusMudanca::Recusada);
    assert_eq!(
        so(&rs).motivo,
        Some(MotivoNaoAplicada::ArquivoSomenteLeitura)
    );
    assert!(fs::metadata(c.r6()).unwrap().permissions().readonly());
    assert_eq!(ler(&c.r6()), R6);
    assert_eq!(c.grav.gravacoes.get(), 0);
    #[expect(
        clippy::permissions_set_readonly_false,
        reason = "limpeza da pasta temporária"
    )]
    perm.set_readonly(false);
    fs::set_permissions(c.r6(), perm).unwrap();
}

#[test]
fn arquivo_na_nuvem_relatado_sem_escrita() {
    let mut c = Cenario::novo();
    let video = b"\"video.cfg\"\n{\n\t\"setting.r_low_latency\"\t\t\"1\"\n}\n";
    let app = c.steam().join(r"userdata\1\730");
    escrever(&app.join(r"local\cfg\cs2_video.txt"), video);
    escrever(&app.join(r"remote\cfg\cs2_video.txt"), video);
    let mut a = c.agente();
    let fatos = c.fatos(&mut a);
    let v = fato(&fatos, "cs2-video").clone();
    assert!(v.na_nuvem);
    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(&v, "cs2_reflex", ValorJogoIpc::Inteiro(2))],
    );
    assert_eq!(so(&rs).status, StatusMudanca::Recusada);
    assert_eq!(so(&rs).motivo, Some(MotivoNaoAplicada::ArquivoNaNuvem));
    assert_eq!(ler(&app.join(r"local\cfg\cs2_video.txt")), video);
}

#[test]
fn conta_ambigua_relatada_sem_escrita() {
    let mut c = Cenario::novo();
    let mut a = c.agente();
    let fatos = c.fatos(&mut a);
    let r6 = fato(&fatos, "r6-game-settings").clone();
    let outra = c
        .r6()
        .parent()
        .unwrap()
        .with_file_name("0f0e0d0c-aaaa-bbbb-cccc-000000000002");
    escrever(&outra.join("GameSettings.ini"), R6);
    let t = SystemTime::now() - Duration::from_secs(3600);
    for p in [c.r6(), outra.join("GameSettings.ini")] {
        fs::File::options()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }
    let fatos = c.fatos(&mut a);
    let amb = fato(&fatos, "r6-game-settings");
    assert!(amb.conta_ambigua && !amb.presente);
    assert_eq!(amb.instancia, "ambigua");
    amb.validar().unwrap();
    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(144))],
    );
    assert_eq!(so(&rs).status, StatusMudanca::Recusada);
    assert_eq!(so(&rs).motivo, Some(MotivoNaoAplicada::ContaAmbigua));
    assert_eq!(ler(&c.r6()), R6);
}

#[test]
fn deriva_na_janela_fechada_vira_trava_sem_regravar() {
    let mut c = Cenario::novo();
    let mut a = c.agente();
    let fatos = c.fatos(&mut a);
    let r6 = fato(&fatos, "r6-game-settings").clone();
    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(144))],
    );
    assert_eq!(so(&rs).status, StatusMudanca::Aplicada);

    // Jogo aberto mudou o valor: a deriva espera a janela fechada.
    fs::write(c.r6(), r6_com("100")).unwrap();
    c.procs.definir(Some(&["RainbowSix.exe"]));
    c.agora += Duration::from_secs(601);
    let f = c.fatos(&mut a);
    assert!(!fato(&f, "r6-game-settings").gestao[0].travado);

    c.procs.definir(Some(&[]));
    c.agora += Duration::from_secs(601);
    let f = c.fatos(&mut a);
    let r6 = fato(&f, "r6-game-settings").clone();
    assert!(r6.gestao[0].travado);

    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(144))],
    );
    assert_eq!(so(&rs).status, StatusMudanca::Travada);
    assert_eq!(ler(&c.r6()), r6_com("100"));
    assert_eq!(c.grav.gravacoes.get(), 1);
}

#[test]
fn reinicio_depois_do_replace_conclui_sem_dupla_aplicacao() {
    let mut c = Cenario::novo();
    let mut a = c.agente();
    let fatos = c.fatos(&mut a);
    let r6 = fato(&fatos, "r6-game-settings").clone();
    let plano = c.plano(
        vec![aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(144))],
        OperacaoUsuario::Aplicar,
    );
    c.grav.queda.set(Queda::Depois);
    let queda = catch_unwind(AssertUnwindSafe(|| {
        c.passo_com(&mut a, plano.clone(), false)
    }));
    assert!(queda.is_err());
    assert_eq!(ler(&c.r6()), r6_com("144"));
    assert!(
        c.estado().jogo.arquivos[0].intencao.is_some(),
        "intenção persistida"
    );

    c.grav.queda.set(Queda::Nenhuma);
    let mut a = c.agente();
    let (r, _, rs) = c.passo_com(&mut a, plano, false);
    assert_eq!(r.unwrap(), Controle::Continuar);
    assert_eq!(so(&rs).status, StatusMudanca::Aplicada);
    assert_eq!(c.grav.gravacoes.get(), 1, "sem segunda gravação");
    assert!(c.estado().jogo.arquivos[0].intencao.is_none());
}

#[test]
fn reinicio_antes_do_replace_recupera_e_limpa_temporario() {
    let mut c = Cenario::novo();
    let mut a = c.agente();
    let fatos = c.fatos(&mut a);
    let r6 = fato(&fatos, "r6-game-settings").clone();
    let plano = c.plano(
        vec![aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(144))],
        OperacaoUsuario::Aplicar,
    );
    c.grav.queda.set(Queda::Antes);
    let queda = catch_unwind(AssertUnwindSafe(|| {
        c.passo_com(&mut a, plano.clone(), false)
    }));
    assert!(queda.is_err());
    let temp = c.r6().with_file_name("GameSettings.ini.nvt-9-9.tmp");
    assert!(temp.exists());
    assert_eq!(ler(&c.r6()), R6);

    c.grav.queda.set(Queda::Nenhuma);
    let mut a = c.agente();
    let (_, _, rs) = c.passo_com(&mut a, plano, false);
    assert_eq!(so(&rs).status, StatusMudanca::Falhou);
    assert!(!temp.exists(), "temporário removido");
    assert_eq!(ler(&c.r6()), R6);

    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(144))],
    );
    assert_eq!(so(&rs).status, StatusMudanca::Aplicada);
    assert_eq!(ler(&c.r6()), r6_com("144"));
    assert_eq!(c.grav.gravacoes.get(), 1);
}

#[test]
fn restaurar_do_msi_com_steam_aberta_bloqueia_e_preserva_backups() {
    let mut c = Cenario::novo();
    let mut a = c.agente();
    let fatos = c.fatos(&mut a);
    let r6 = fato(&fatos, "r6-game-settings").clone();
    let cs2 = fato(&fatos, "cs2-launch").clone();
    let (_, rs) = c.rodar(
        &mut a,
        vec![
            aplicar(&r6, "r6_fps_limit", ValorJogoIpc::Inteiro(144)),
            aplicar(
                &cs2,
                "cs2_fps_max",
                ValorJogoIpc::TokenLancamento(Some(144)),
            ),
        ],
    );
    assert!(
        rs.iter().all(|r| r.status == StatusMudanca::Aplicada),
        "{rs:?}"
    );

    c.procs.definir(Some(&["steam.exe"]));
    let plano = c.plano(Vec::new(), OperacaoUsuario::Restaurar);
    let (r, _, _) = c.passo_com(&mut a, plano, true);
    let Controle::Bloqueado(msg) = r.unwrap() else {
        panic!("deveria bloquear");
    };
    assert!(
        msg.contains("steam.exe") && msg.contains("backups preservados"),
        "{msg}"
    );
    assert_eq!(ler(&c.r6()), R6, "o que não exige a Steam fechada volta");
    assert_ne!(ler(&c.localconfig()), LOCALCONFIG.as_bytes());

    c.procs.definir(Some(&[]));
    let plano = c.plano(Vec::new(), OperacaoUsuario::Restaurar);
    let (r, _, _) = c.passo_com(&mut a, plano, true);
    assert_eq!(r.unwrap(), Controle::Restaurado);
    assert_eq!(ler(&c.localconfig()), LOCALCONFIG.as_bytes());
    let backups: Vec<_> = fs::read_dir(c.backups().join("cs2"))
        .unwrap()
        .chain(fs::read_dir(c.backups().join("r6-siege")).unwrap())
        .collect();
    assert_eq!(backups.len(), 4, "backups nunca apagados");

    // Nada mais a restaurar.
    let plano = c.plano(Vec::new(), OperacaoUsuario::Restaurar);
    let (r, _, _) = c.passo_com(&mut a, plano, true);
    assert_eq!(r.unwrap(), Controle::Restaurado);
}

#[test]
fn restauracao_por_geracao_devolve_o_original() {
    let mut c = Cenario::novo();
    let mut a = c.agente();
    let fatos = c.fatos(&mut a);
    let rl = fato(&fatos, "rl-ta-system-settings").clone();
    let (_, rs) = c.rodar(
        &mut a,
        vec![aplicar(
            &rl,
            "rl_one_frame_thread_lag",
            ValorJogoIpc::Booleano(false),
        )],
    );
    assert_eq!(so(&rs).status, StatusMudanca::Aplicada);
    assert!(
        String::from_utf8(ler(&c.rl()))
            .unwrap()
            .contains("OneFrameThreadLag=False")
    );

    // Jogo aberto: a geração espera.
    c.procs.definir(Some(&["RocketLeague.exe"]));
    c.geracao = 1;
    c.fatos(&mut a);
    assert_ne!(ler(&c.rl()), RL);
    assert_eq!(c.estado().sessoes[&1].geracao, 0);

    c.procs.definir(Some(&[]));
    c.fatos(&mut a);
    assert_eq!(ler(&c.rl()), RL, "idêntico por hash");
    assert_eq!(c.estado().sessoes[&1].geracao, 1);
    let f = c.fatos(&mut a);
    assert!(fato(&f, "rl-ta-system-settings").gestao.is_empty());
}

#[test]
fn estado_v6_carrega_sem_familia_jogo() {
    let e: Estado =
        serde_json::from_str(r#"{"geracao_vista":3,"sessoes":{},"destinos":[]}"#).unwrap();
    assert!(e.jogo.vazio());
    let json = serde_json::to_string(&e).unwrap();
    assert!(!json.contains("jogo"), "{json}");
}
