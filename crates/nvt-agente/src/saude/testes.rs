use std::cell::Cell;
use std::ffi::OsString;
use std::path::Path;
use std::rc::Rc;

use nvt_ipc::{MotivoPulo, OperacaoUsuario, Requisicao};
use nvt_manutencao::{Entrada, PoliticaDiaria};
use nvt_windows::saude::{
    Aprovacao, ArquivoPasta, ChaveRun, LeituraDword, PastaInicio, TarefaBruta,
};

use super::*;

#[derive(Default, Clone)]
struct Contador(Rc<Cell<u32>>);

impl Contador {
    fn somar(&self) {
        self.0.set(self.0.get() + 1);
    }
    fn valor(&self) -> u32 {
        self.0.get()
    }
}

#[derive(Default)]
struct LeitorFalso {
    run: Vec<(String, String)>,
    leituras: Contador,
}

impl LeitorInicio for LeitorFalso {
    fn run(&self, chave: ChaveRun) -> Option<Vec<(String, String)>> {
        self.leituras.somar();
        Some(if chave == ChaveRun::RunUsuario {
            self.run.clone()
        } else {
            vec![]
        })
    }
    fn aprovacao(&self, _: Aprovacao) -> Option<Vec<(String, Vec<u8>)>> {
        self.leituras.somar();
        Some(vec![])
    }
    fn pasta(&self, _: PastaInicio) -> Option<Vec<ArquivoPasta>> {
        self.leituras.somar();
        Some(vec![])
    }
    fn tarefas(&self) -> Option<Vec<TarefaBruta>> {
        self.leituras.somar();
        Some(vec![])
    }
}

impl LeitorSinaisSessao for LeitorFalso {
    fn storage_policy_01(&self) -> Option<LeituraDword> {
        self.leituras.somar();
        Some(LeituraDword::Valor(0))
    }
}

/// Um arquivo velho na raiz; conta as enumerações.
#[derive(Default)]
struct FsFalso(Contador);

impl SistemaArquivos for FsFalso {
    fn listar(&self, _: &Path) -> Result<Vec<Entrada>, MotivoPulo> {
        self.0.somar();
        Ok(vec![Entrada {
            nome: OsString::from("velho.tmp"),
            diretorio: false,
            reparse: false,
            somente_leitura: false,
            sistema: false,
            criacao_unix: 1,
            escrita_unix: 1,
            tamanho: 10,
        }])
    }
}

#[derive(Default)]
struct ExcluidorEspiao(Contador);

impl Excluidor for ExcluidorEspiao {
    fn excluir(&mut self, _: &Path, _: &Entrada) -> Result<(), MotivoPulo> {
        self.0.somar();
        Ok(())
    }
}

struct SemInstalacao;

impl Instalacao for SemInstalacao {
    fn em_andamento(&self) -> bool {
        false
    }
}

struct RelogioFixo;

impl Relogio for RelogioFixo {
    fn agora_ms(&self) -> u64 {
        30 * 24 * 3600 * 1000
    }
}

struct PortaoFixo(bool);

impl nvt_manutencao::Portao for PortaoFixo {
    fn jogo_aberto(&self) -> bool {
        self.0
    }
}

struct Espias {
    leituras: Contador,
    listagens: Contador,
    exclusoes: Contador,
}

fn fachada(run: &[(&str, &str)]) -> (Box<dyn FonteSaude>, Espias) {
    let leitor = LeitorFalso {
        run: run
            .iter()
            .map(|(n, c)| ((*n).to_owned(), (*c).to_owned()))
            .collect(),
        ..Default::default()
    };
    let esp = Espias {
        leituras: leitor.leituras.clone(),
        listagens: Contador::default(),
        exclusoes: Contador::default(),
    };
    let f = FachadaSaude {
        leitor,
        fs: FsFalso(esp.listagens.clone()),
        excluidor: ExcluidorEspiao(esp.exclusoes.clone()),
        instalacao: SemInstalacao,
        raiz: Some(
            Raiz::nova(
                AlvoLimpeza::TempUsuario,
                r"C:\Users\fulano\AppData\Local\Temp",
            )
            .unwrap(),
        ),
        relogio: Box::new(RelogioFixo),
    };
    (Box::new(f), esp)
}

const RUN: &[(&str, &str)] = &[
    (
        "Discord",
        r#""C:\Users\fulano\AppData\Local\Discord\Update.exe" --processStart Discord.exe"#,
    ),
    (
        "Steam",
        r#""C:\Program Files (x86)\Steam\steam.exe" -silent"#,
    ),
];

#[test]
fn nada_e_lido_nem_limpo_com_jogo_aberto() {
    let (fonte, esp) = fachada(RUN);
    let agora = Instant::now();
    let mut agenda = AgendaSaude::nova(fonte, ModoLimpeza::Simulacao);
    let mut estado = EstadoSaude::default();
    assert!(!agenda.passo(agora, 100, &PortaoFixo(true), &mut estado));
    assert_eq!(esp.leituras.valor(), 0);
    assert_eq!(esp.listagens.valor(), 0);
    assert_eq!(esp.exclusoes.valor(), 0);
    assert!(agenda.pendente.is_none());
    assert!(estado.vazio());
    // Adiado: a próxima tentativa só depois do intervalo, mesmo sem jogo.
    assert!(!agenda.passo(
        agora + Duration::from_secs(1),
        101,
        &PortaoFixo(false),
        &mut estado
    ));
    assert_eq!(esp.leituras.valor(), 0);
    assert!(agenda.passo(
        agora + INTERVALO_ADIADO,
        700,
        &PortaoFixo(false),
        &mut estado
    ));
    assert!(esp.leituras.valor() > 0);
}

#[test]
fn portao_sem_lista_do_servico_conta_como_jogo_aberto() {
    let base = BaseJogos::embutida().unwrap();
    let portao = Portao::new(nvt_detect::ProcessosFalsos::default());
    let sem = PortaoAgente {
        portao: &portao,
        base: &base,
        extras: None,
    };
    assert!(nvt_manutencao::Portao::jogo_aberto(&sem));
    let com = PortaoAgente {
        portao: &portao,
        base: &base,
        extras: Some(vec![]),
    };
    assert!(!nvt_manutencao::Portao::jogo_aberto(&com));
    let jogo = Portao::new(nvt_detect::ProcessosFalsos::com_nomes(&["extra.exe"]));
    let aberto = PortaoAgente {
        portao: &jogo,
        base: &base,
        extras: Some(vec!["extra.exe"]),
    };
    assert!(nvt_manutencao::Portao::jogo_aberto(&aberto));
}

#[test]
fn modo_da_compilacao_controla_o_percurso_do_temp() {
    let (fonte, esp) = fachada(RUN);
    let agora = Instant::now();
    let mut agenda = AgendaSaude::nova(fonte, nvt_manutencao::MODO_LIMPEZA);
    let mut estado = EstadoSaude::default();
    agenda.passo(agora, 100, &PortaoFixo(false), &mut estado);
    assert_eq!(esp.exclusoes.valor(), 0, "nenhum modo do agente apaga hoje");
    let r = estado.limpeza.as_ref().unwrap();
    assert_eq!(r.alvo, AlvoLimpeza::TempUsuario);
    assert_eq!(r.modo, nvt_manutencao::MODO_LIMPEZA);
    if cfg!(feature = "build-de-teste") {
        assert_eq!(nvt_manutencao::MODO_LIMPEZA, ModoLimpeza::Simulacao);
        assert!(esp.listagens.valor() > 0);
        assert_eq!((r.arquivos, r.bytes), (1, 10));
        assert!(
            estado
                .ultimas_limpezas
                .contains_key(&AlvoLimpeza::TempUsuario)
        );
    } else {
        assert_eq!(nvt_manutencao::MODO_LIMPEZA, ModoLimpeza::Desligada);
        assert_eq!(
            esp.listagens.valor(),
            0,
            "build normal não percorre o %TEMP%"
        );
        assert!(estado.ultimas_limpezas.is_empty());
    }
}

#[test]
fn simulacao_respeita_politica_diaria_persistida() {
    let (fonte, esp) = fachada(RUN);
    let agora = Instant::now();
    let mut agenda = AgendaSaude::nova(fonte, ModoLimpeza::Simulacao);
    let mut estado = EstadoSaude::default();
    agenda.passo(agora, 100, &PortaoFixo(false), &mut estado);
    assert_eq!(esp.listagens.valor(), 1);
    assert_eq!(esp.exclusoes.valor(), 0);
    // Estado recarregado do disco: a política continua valendo.
    let json = serde_json::to_string(&estado).unwrap();
    let mut estado: EstadoSaude = serde_json::from_str(&json).unwrap();
    agenda.passo(
        agora + INTERVALO_INICIO,
        200,
        &PortaoFixo(false),
        &mut estado,
    );
    assert_eq!(esp.listagens.valor(), 1, "uma passada por 24 h");
    let mut p = PoliticaDiaria {
        ultimas: estado.ultimas_limpezas.clone(),
    };
    assert!(p.ultima_unix(AlvoLimpeza::TempUsuario).is_some());
    p.registrar(AlvoLimpeza::TempUsuario, 0);
}

fn entrada(nome: &str, comando: &str) -> EntradaInicio {
    EntradaInicio {
        fonte: FonteInicio::RunUsuario,
        nome: nome.into(),
        executavel: nvt_windows::saude::executavel_de(comando),
        habilitado: None,
        comando: comando.into(),
    }
}

#[test]
fn linha_de_base_nao_marca_tudo_como_novo() {
    let mut estado = EstadoSaude::default();
    let primeira = [
        entrada("Steam", "steam.exe -silent"),
        entrada("Discord", "Update.exe"),
    ];
    let r = comparar(&mut estado, &primeira, 100);
    assert!(r.iter().all(|p| p.situacao == SituacaoInicio::NaBase));
    assert_eq!(estado.base.as_ref().unwrap().itens.len(), 2);

    let segunda = [
        entrada("Steam", "steam.exe -silent"),
        entrada("Discord", "Update.exe --outro"),
        entrada("Novo", "novo.exe"),
    ];
    let r = comparar(&mut estado, &segunda, 200);
    let sit: Vec<_> = r
        .iter()
        .map(|p| (p.nome.as_str(), p.situacao, p.detectado_unix))
        .collect();
    assert_eq!(
        sit,
        [
            ("Steam", SituacaoInicio::NaBase, 100),
            ("Discord", SituacaoInicio::Mudou, 200),
            ("Novo", SituacaoInicio::Novo, 200),
        ]
    );
    // A base não absorve o novo; a data da detecção fica estável.
    assert_eq!(estado.base.as_ref().unwrap().itens.len(), 2);
    let r = comparar(&mut estado, &segunda, 300);
    assert_eq!(r[2].situacao, SituacaoInicio::Novo);
    assert_eq!(r[2].detectado_unix, 200);
    // Removido some da lista fora da base.
    comparar(&mut estado, &segunda[..1], 400);
    assert!(estado.fora_da_base.is_empty());
}

#[test]
fn estado_nao_guarda_comandos() {
    let mut estado = EstadoSaude::default();
    comparar(
        &mut estado,
        &[entrada("Steam", r"C:\Jogos\steam.exe -silent")],
        1,
    );
    let json = serde_json::to_string(&estado).unwrap();
    assert!(!json.contains("steam.exe"), "{json}");
    assert!(!json.contains("Jogos"), "{json}");
}

#[test]
fn relato_passa_na_validacao_do_ipc_sem_caminhos() {
    let (fonte, _esp) = fachada(RUN);
    let agora = Instant::now();
    let mut agenda = AgendaSaude::nova(fonte, ModoLimpeza::Simulacao);
    let mut estado = EstadoSaude::default();
    agenda.passo(agora, 100, &PortaoFixo(false), &mut estado);
    let saude = agenda.pendente.clone().unwrap();
    assert_eq!(saude.inicializacao.len(), 2);
    assert!(!saude.sinais.is_empty());
    assert!(saude.limpeza.is_some());
    let req = Requisicao::AgenteRelatar {
        id_tarefa: 1,
        operacao: OperacaoUsuario::Aplicar,
        revisao: 1,
        resultados: vec![],
        resultados_jogo: vec![],
        saude: Some(saude),
    };
    req.validar().unwrap();
    let json = serde_json::to_string(&req).unwrap();
    for proibido in [
        "fulano",
        "Program Files",
        ":\\\\",
        "--processStart",
        "-silent",
    ] {
        assert!(!json.contains(proibido), "{proibido} em {json}");
    }
}
