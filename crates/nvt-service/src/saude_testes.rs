use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;

use nvt_manutencao::{Entrada, MotivoPulo};
use nvt_windows::saude::{
    Aprovacao, ArquivoPasta, ChaveRun, EntradaInicio, PastaInicio, TarefaBruta, TemposCpu,
};

use super::*;

const DIA_MS: u64 = 24 * 3600 * 1000;
const T0_MS: u64 = 1_800_000_000_000;

// ── Falsos ─────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Leitor {
    leituras: Cell<u32>,
    cpu: Cell<bool>,
    inicios: Cell<u32>,
    run: RefCell<Vec<(String, String)>>,
}

impl LeitorSinaisMaquina for Leitor {
    fn disco_sistema(&self) -> Option<(u64, u64)> {
        self.leituras.set(self.leituras.get() + 1);
        Some((50 << 30, 100 << 30))
    }
    fn memoria_comprometida(&self) -> Option<(u64, u64)> {
        self.leituras.set(self.leituras.get() + 1);
        Some((4 << 30, 16 << 30))
    }
    fn tempos_cpu(&self) -> Option<TemposCpu> {
        self.leituras.set(self.leituras.get() + 1);
        self.cpu.get().then_some(TemposCpu {
            ocioso: 0,
            kernel: 0,
            usuario: 0,
        })
    }
    fn plano_ativo(&self) -> Option<u128> {
        self.leituras.set(self.leituras.get() + 1);
        None
    }
    fn driver_nvidia(&self) -> Option<u32> {
        self.leituras.set(self.leituras.get() + 1);
        None
    }
}

impl LeitorInicio for Leitor {
    fn run(&self, chave: ChaveRun) -> Option<Vec<(String, String)>> {
        self.leituras.set(self.leituras.get() + 1);
        self.inicios.set(self.inicios.get() + 1);
        Some(if chave == ChaveRun::RunMaquina {
            self.run.borrow().clone()
        } else {
            vec![]
        })
    }
    fn aprovacao(&self, _: Aprovacao) -> Option<Vec<(String, Vec<u8>)>> {
        Some(vec![])
    }
    fn pasta(&self, _: PastaInicio) -> Option<Vec<ArquivoPasta>> {
        Some(vec![])
    }
    fn tarefas(&self) -> Option<Vec<TarefaBruta>> {
        Some(vec![])
    }
}

struct Amb {
    jogo: Cell<bool>,
    msi: Cell<bool>,
    agora_ms: Cell<u64>,
    exclusao: Arc<crate::laco::Exclusao>,
    desde_inicio: Cell<u64>,
    /// A próxima espera recebe o pedido de parada.
    parar_na_espera: Cell<bool>,
    parado: Cell<bool>,
}

impl Default for Amb {
    fn default() -> Self {
        Self {
            jogo: Cell::new(false),
            msi: Cell::new(false),
            agora_ms: Cell::new(T0_MS),
            exclusao: Arc::default(),
            desde_inicio: Cell::new(0),
            parar_na_espera: Cell::new(false),
            parado: Cell::new(false),
        }
    }
}

impl Ambiente for Amb {
    fn jogo_aberto(&self) -> bool {
        self.jogo.get()
    }
    fn instalacao_msi(&self) -> bool {
        self.msi.get()
    }
    fn reservar_limpeza(&self) -> bool {
        self.exclusao.tentar_limpeza()
    }
    fn liberar_limpeza(&self) {
        self.exclusao.liberar_limpeza();
    }
    fn desde_inicio_s(&self) -> u64 {
        self.desde_inicio.get()
    }
    fn agora_ms(&self) -> u64 {
        self.agora_ms.get()
    }
    fn esperar(&self, _: Duration) -> bool {
        if self.parar_na_espera.get() {
            self.parado.set(true);
        }
        !self.parado.get()
    }
    fn parada_pedida(&self) -> bool {
        self.parado.get()
    }
}

/// `C:\Windows\Temp` falso: um arquivo antigo de 100 bytes.
#[derive(Default)]
struct Fs {
    listagens: Rc<Cell<u32>>,
}

impl SistemaArquivos for Fs {
    fn listar(&self, _: &Path) -> Result<Vec<Entrada>, MotivoPulo> {
        self.listagens.set(self.listagens.get() + 1);
        Ok(vec![Entrada {
            nome: "velho.tmp".into(),
            diretorio: false,
            reparse: false,
            somente_leitura: false,
            sistema: false,
            criacao_unix: 1,
            escrita_unix: 1,
            tamanho: 100,
        }])
    }
}

#[derive(Default)]
struct Espiao(Rc<Cell<u32>>);

impl Excluidor for Espiao {
    fn excluir(&mut self, _: &Path, _: &Entrada) -> Result<(), MotivoPulo> {
        self.0.set(self.0.get() + 1);
        Ok(())
    }
}

#[derive(Clone, Default)]
struct Armazem(Arc<Mutex<EstadoManutencao>>);

impl ArmazemManutencao for Armazem {
    fn carregar(&self) -> io::Result<EstadoManutencao> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn salvar(&self, estado: &EstadoManutencao) -> io::Result<()> {
        *self.0.lock().unwrap() = estado.clone();
        Ok(())
    }
}

struct Montagem {
    leitor: LeitorSaude<Leitor, Amb, Fs, Espiao>,
    cache: Arc<Mutex<SaudeMaquina>>,
    listagens: Rc<Cell<u32>>,
    exclusoes: Rc<Cell<u32>>,
    raizes: Arc<Mutex<u32>>,
    armazem: Armazem,
}

fn montar(modo: ModoLimpeza) -> Montagem {
    let cache = Arc::new(Mutex::new(SaudeMaquina::default()));
    let fs = Fs::default();
    let listagens = fs.listagens.clone();
    let espiao = Espiao::default();
    let exclusoes = espiao.0.clone();
    let raizes = Arc::new(Mutex::new(0));
    let contador = raizes.clone();
    let armazem = Armazem::default();
    let leitor = LeitorSaude::novo(
        Leitor::default(),
        Amb::default(),
        fs,
        espiao,
        modo,
        Box::new(move || {
            *contador.lock().unwrap() += 1;
            Raiz::nova(AlvoLimpeza::TempWindows, r"C:\Windows\Temp")
        }),
        Box::new(armazem.clone()),
        cache.clone(),
    );
    Montagem {
        leitor,
        cache,
        listagens,
        exclusoes,
        raizes,
        armazem,
    }
}

#[test]
fn parada_na_janela_de_cpu_nao_le_inicio_nem_limpa() {
    let mut m = montar(ModoLimpeza::Simulacao);
    m.leitor.leitor.cpu.set(true);
    m.leitor.ambiente().desde_inicio.set(u64::MAX / 2);
    m.leitor.ambiente().parar_na_espera.set(true);
    m.leitor.passo();
    assert!(m.leitor.ambiente().parado.get(), "a janela de CPU esperou");
    assert_eq!(m.leitor.leitor.inicios.get(), 0, "sem ler_inicio");
    assert_eq!(m.listagens.get(), 0, "sem limpar");
    assert_eq!(*m.raizes.lock().unwrap(), 0);
    assert!(m.cache.lock().unwrap().limpeza.is_none());
}

#[test]
fn parada_fecha_o_portao_da_limpeza() {
    let m = montar(ModoLimpeza::Simulacao);
    m.leitor.ambiente().parado.set(true);
    assert!(Portao::jogo_aberto(&Adaptador(m.leitor.ambiente())));
}

// ── Limpeza ────────────────────────────────────────────────────────────────

#[test]
#[cfg(not(feature = "build-de-teste"))]
fn build_normal_nao_percorre_windows_temp() {
    assert_eq!(MODO_LIMPEZA, ModoLimpeza::Desligada);
    let mut m = montar(MODO_LIMPEZA);
    for dia in 0..3 {
        m.leitor.ambiente().agora_ms.set(T0_MS + dia * DIA_MS);
        m.leitor.passo();
    }
    assert_eq!(m.listagens.get(), 0, "nenhuma enumeração");
    assert_eq!(*m.raizes.lock().unwrap(), 0, "nem resolve a raiz");
    assert_eq!(m.exclusoes.get(), 0);
    assert!(m.cache.lock().unwrap().limpeza.is_none());
    assert!(m.armazem.0.lock().unwrap().historico.is_empty());
    let Resposta::Saude(s) = atender(
        None,
        &cliente("S-1-5-21-1", 1),
        &nvt_ipc::Requisicao::ObterSaude,
        &m.cache.lock().unwrap(),
        std::time::Instant::now(),
        1,
    ) else {
        panic!("esperado Saude");
    };
    assert_eq!(s.modo_limpeza, ModoLimpeza::Desligada);
    assert!(s.limpeza.is_empty());
}

#[test]
fn simulacao_conta_sem_excluir_e_respeita_politica_diaria() {
    let mut m = montar(ModoLimpeza::Simulacao);
    m.leitor.passo();
    let r = m.cache.lock().unwrap().limpeza.clone().expect("passada");
    assert_eq!(
        (r.modo, r.alvo, r.arquivos, r.bytes),
        (ModoLimpeza::Simulacao, AlvoLimpeza::TempWindows, 1, 100)
    );
    assert_eq!(m.exclusoes.get(), 0, "simulação nunca exclui");
    let persistido = m.armazem.0.lock().unwrap().clone();
    assert_eq!(persistido.historico.len(), 1);
    assert_eq!(
        persistido.ultimas.get(&AlvoLimpeza::TempWindows),
        Some(&(T0_MS / 1000))
    );
    // Uma hora depois: a política diária recusa.
    let listagens = m.listagens.get();
    m.leitor.ambiente().agora_ms.set(T0_MS + 2 * 3600 * 1000);
    m.leitor.passo();
    assert_eq!(m.listagens.get(), listagens);
    assert_eq!(m.leitor.estado().historico.len(), 1);
    // Um dia depois: nova passada.
    m.leitor.ambiente().agora_ms.set(T0_MS + DIA_MS + 1000);
    m.leitor.passo();
    assert_eq!(m.leitor.estado().historico.len(), 2);
}

#[test]
fn politica_persistida_sobrevive_ao_reinicio() {
    let m = montar(ModoLimpeza::Simulacao);
    m.armazem
        .0
        .lock()
        .unwrap()
        .ultimas
        .insert(AlvoLimpeza::TempWindows, T0_MS / 1000 - 60);
    let mut m2 = montar(ModoLimpeza::Simulacao);
    m2.leitor = LeitorSaude::novo(
        Leitor::default(),
        Amb::default(),
        Fs {
            listagens: m2.listagens.clone(),
        },
        Espiao::default(),
        ModoLimpeza::Simulacao,
        Box::new(|| Raiz::nova(AlvoLimpeza::TempWindows, r"C:\Windows\Temp")),
        Box::new(m.armazem.clone()),
        m2.cache.clone(),
    );
    m2.leitor.passo();
    assert_eq!(m2.listagens.get(), 0, "passada há menos de 24 h");
}

#[test]
fn nada_com_jogo_aberto() {
    let mut m = montar(ModoLimpeza::Simulacao);
    m.leitor.ambiente().jogo.set(true);
    for dia in 0..3 {
        m.leitor.ambiente().agora_ms.set(T0_MS + dia * DIA_MS);
        m.leitor.passo();
    }
    assert_eq!(m.leitor.leitor.leituras.get(), 0, "nenhuma leitura");
    assert_eq!(m.listagens.get(), 0, "nenhuma limpeza");
    assert_eq!(*m.cache.lock().unwrap(), SaudeMaquina::default());
    // Fechou o jogo: tudo volta.
    m.leitor.ambiente().jogo.set(false);
    m.leitor.passo();
    assert!(m.leitor.leitor.leituras.get() > 0);
    assert_eq!(m.listagens.get(), 1);
}

#[test]
fn limpeza_respeita_msi_ciclo_drs_e_instalacao() {
    let mut m = montar(ModoLimpeza::Simulacao);
    m.leitor.ambiente().msi.set(true);
    m.leitor.passo();
    assert_eq!(m.listagens.get(), 0, "instalação MSI em andamento");

    let mut m = montar(ModoLimpeza::Simulacao);
    let exclusao = m.leitor.ambiente().exclusao.clone();
    {
        let _drs = exclusao.ciclo_drs();
        m.leitor.passo();
    }
    assert_eq!(m.listagens.get(), 0, "ciclo DRS em curso");

    let mut m = montar(ModoLimpeza::Simulacao);
    let exclusao = m.leitor.ambiente().exclusao.clone();
    assert!(exclusao.reservar_instalacao(&AtomicBool::new(false)));
    m.leitor.passo();
    assert_eq!(m.listagens.get(), 0, "instalação de atualização reservada");
    exclusao.liberar_instalacao();
}

#[test]
fn instalacao_espera_a_limpeza() {
    let exclusao = crate::laco::Exclusao::default();
    assert!(exclusao.tentar_limpeza());
    assert!(!exclusao.tentar_limpeza(), "uma passada por vez");
    assert!(
        !exclusao.reservar_instalacao(&AtomicBool::new(true)),
        "instalação não entra durante a limpeza"
    );
    exclusao.liberar_limpeza();
    assert!(exclusao.reservar_instalacao(&AtomicBool::new(false)));
    exclusao.liberar_instalacao();
    // O ciclo DRS não espera a limpeza (o motor não bloqueia).
    assert!(exclusao.tentar_limpeza());
    drop(exclusao.ciclo_drs());
    exclusao.liberar_limpeza();
}

// ── Leituras ───────────────────────────────────────────────────────────────

#[test]
fn leituras_com_cache_e_intervalos() {
    let mut m = montar(ModoLimpeza::Desligada);
    m.leitor
        .leitor
        .run
        .borrow_mut()
        .push(("Atualizador".into(), r#""C:\App\upd.exe" /x"#.into()));
    m.leitor.passo();
    let lidas = m.leitor.leitor.leituras.get();
    {
        let c = m.cache.lock().unwrap();
        assert_eq!(c.sinais.len(), 5);
        assert_eq!(c.inicializacao.len(), 1);
        assert_eq!(c.inicializacao[0].situacao, SituacaoInicio::NaBase);
        assert_eq!(c.inicializacao[0].executavel, "upd.exe");
    }
    // Dentro do intervalo: nada é relido.
    m.leitor.ambiente().agora_ms.set(T0_MS + 60_000);
    m.leitor.passo();
    assert_eq!(m.leitor.leitor.leituras.get(), lidas);
    // 10 min: só os sinais.
    m.leitor.ambiente().agora_ms.set(T0_MS + 601_000);
    m.leitor.passo();
    assert!(m.leitor.leitor.leituras.get() > lidas);
    assert!(m.armazem.0.lock().unwrap().base_inicio.is_some());
}

fn entrada(nome: &str, comando: &str) -> EntradaInicio {
    EntradaInicio {
        fonte: FonteInicio::RunMaquina,
        nome: nome.into(),
        executavel: "a.exe".into(),
        habilitado: Some(true),
        comando: comando.into(),
    }
}

#[test]
fn linha_de_base_da_maquina() {
    let mut base = None;
    let leitura = |e: Vec<EntradaInicio>| LeituraInicio {
        entradas: e,
        fontes_indisponiveis: vec![],
    };
    let p = diferenca(&mut base, &leitura(vec![entrada("A", "a")]), 10);
    assert_eq!(
        p[0].situacao,
        SituacaoInicio::NaBase,
        "primeira leitura = base"
    );
    let p = diferenca(
        &mut base,
        &leitura(vec![entrada("A", "a2"), entrada("B", "b")]),
        20,
    );
    assert_eq!(p[0].situacao, SituacaoInicio::Mudou);
    assert_eq!(p[1].situacao, SituacaoInicio::Novo);
    assert_eq!(p[1].detectado_unix, 20);
    let p = diferenca(
        &mut base,
        &leitura(vec![entrada("A", "a"), entrada("B", "b")]),
        30,
    );
    assert_eq!(
        p[0].situacao,
        SituacaoInicio::NaBase,
        "voltou ao comando da base"
    );
    assert_eq!(p[1].detectado_unix, 20, "data de detecção estável");
}

// ── Projeção ───────────────────────────────────────────────────────────────

fn cliente(sid: &str, sessao: u32) -> nvt_ipc::pipe::IdentidadeCliente {
    nvt_ipc::pipe::IdentidadeCliente {
        sid: sid.into(),
        sessao,
    }
}

pub(crate) fn programa(fonte: FonteInicio, nome: &str, situacao: SituacaoInicio) -> ProgramaInicio {
    ProgramaInicio {
        fonte,
        nome: nome.into(),
        executavel: "x.exe".into(),
        habilitado: None,
        situacao,
        detectado_unix: 100,
    }
}

fn sinal(tipo: TipoSinal) -> Sinal {
    Sinal {
        tipo,
        gravidade: Gravidade::Ok,
        valor: ValorSinal::Nenhum,
        lido_unix: 1,
    }
}

#[test]
fn projecao_junta_maquina_e_sessao_com_vistos() {
    let maquina = SaudeMaquina {
        sinais: vec![sinal(TipoSinal::EspacoLivre)],
        inicializacao: vec![programa(FonteInicio::RunMaquina, "M", SituacaoInicio::Novo)],
        limpeza: None,
    };
    let sessao = SaudeSessao {
        sinais: vec![
            sinal(TipoSinal::SensorArmazenamento),
            sinal(TipoSinal::EspacoLivre),
        ],
        inicializacao: vec![
            programa(FonteInicio::RunUsuario, "U", SituacaoInicio::Novo),
            programa(FonteInicio::RunMaquina, "M", SituacaoInicio::Novo),
        ],
        limpeza: None,
    };
    let mut vistos = Vistos::new();
    vistos.insert((FonteInicio::RunUsuario, "U".into()), 100);
    vistos.insert((FonteInicio::RunMaquina, "M".into()), 99);
    let s = projetar(&maquina, Some(&sessao), &vistos);
    s.validar().unwrap();
    assert_eq!(s.sinais.len(), 2);
    assert_eq!(s.inicializacao.len(), 2, "sem repetição");
    assert_eq!(
        s.inicializacao[0].situacao,
        SituacaoInicio::Novo,
        "marca anterior à detecção"
    );
    assert_eq!(s.inicializacao[1].situacao, SituacaoInicio::NaBase);
}

#[test]
fn relato_do_agente_so_com_fontes_da_sessao() {
    let f = filtrar_relato(&SaudeSessao {
        sinais: vec![
            sinal(TipoSinal::EspacoLivre),
            sinal(TipoSinal::SensorArmazenamento),
        ],
        inicializacao: vec![
            programa(FonteInicio::RunMaquina, "M", SituacaoInicio::Novo),
            programa(FonteInicio::Tarefa, "T", SituacaoInicio::Novo),
        ],
        limpeza: None,
    });
    assert_eq!(f.sinais, vec![sinal(TipoSinal::SensorArmazenamento)]);
    assert_eq!(f.inicializacao.len(), 1);
    assert_eq!(f.inicializacao[0].fonte, FonteInicio::Tarefa);
}

use nvt_ipc::Resposta;

#[test]
#[cfg(feature = "build-de-teste")]
fn build_de_teste_so_simula_windows_temp() {
    assert_eq!(MODO_LIMPEZA, ModoLimpeza::Simulacao);
    let mut m = montar(MODO_LIMPEZA);
    m.leitor.passo();
    assert_eq!(*m.raizes.lock().unwrap(), 1, "resolve a raiz uma vez");
    assert_eq!(m.exclusoes.get(), 0, "simulação nunca exclui");
    let Resposta::Saude(s) = atender(
        None,
        &cliente("S-1-5-21-1", 1),
        &nvt_ipc::Requisicao::ObterSaude,
        &m.cache.lock().unwrap(),
        std::time::Instant::now(),
        1,
    ) else {
        panic!("esperado Saude");
    };
    assert_eq!(s.modo_limpeza, ModoLimpeza::Simulacao);
}
