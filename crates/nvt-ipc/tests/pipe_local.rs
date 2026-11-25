//! Integração servidor + cliente no mesmo processo, sem privilégios.
//!
//! Cada teste usa um nome de pipe único para poder rodar em paralelo.

#![cfg(windows)]

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use nvt_ipc::pipe::{Cliente, ConfigCliente, ConfigServidor, Parada, Servidor};
use nvt_ipc::{
    AcaoHistorico, CodigoErro, EntradaHistorico, ErroIpc, Estado, PROTOCOL_VERSION, Preferencias,
    Qualidade, Requisicao, Resposta, Sincronismo, TAMANHO_MAXIMO, codificar, decodificar,
    ler_quadro,
};

fn nome_unico(teste: &str) -> String {
    static N: AtomicU32 = AtomicU32::new(0);
    format!(
        r"\\.\pipe\nv-tuner-teste-{}-{}-{}",
        std::process::id(),
        teste,
        N.fetch_add(1, Ordering::Relaxed)
    )
}

fn config_cliente() -> ConfigCliente {
    ConfigCliente {
        timeout_conexao: Duration::from_secs(5),
        timeout_leitura: Duration::from_secs(5),
        timeout_escrita: Duration::from_secs(5),
        // O servidor de teste roda com o usuário comum como dono.
        exigir_dono_privilegiado: false,
    }
}

/// Tratador falso: responde algo plausível para cada requisição.
fn tratador_falso(req: Requisicao) -> Resposta {
    match req {
        Requisicao::ObterEstado => Resposta::Estado(Estado {
            versao_servico: "teste".into(),
            driver: None,
            gpu: None,
            preferencias: Preferencias {
                sincronismo: Sincronismo::Desligado,
                qualidade: Qualidade::Equilibrado,
                limite_fps: None,
            },
            jogos: 3,
            ultima_aplicacao: None,
            alertas_deriva: vec![],
            avisos: vec![],
        }),
        Requisicao::ListarHistorico { limite } => Resposta::Historico {
            entradas: (0..u64::from(limite))
                .map(|id| EntradaHistorico {
                    id,
                    quando_unix: 0,
                    acao: AcaoHistorico::Aplicacao,
                    perfil: None,
                    setting: None,
                    antes: None,
                    depois: None,
                    descricao: format!("entrada {id}"),
                })
                .collect(),
        },
        Requisicao::TravarSetting { setting, .. } if setting == "proibida" => {
            Resposta::erro(CodigoErro::SettingNaoPermitida, "fora da lista")
        }
        Requisicao::ReescanearJogos => panic!("pânico proposital do tratador"),
        _ => Resposta::Ok,
    }
}

struct ServidorDeTeste {
    nome: String,
    parada: Parada,
    thread: Option<JoinHandle<std::io::Result<()>>>,
}

impl ServidorDeTeste {
    fn iniciar(teste: &str, config: ConfigServidor) -> Self {
        let nome = nome_unico(teste);
        let parada = Parada::nova().unwrap();
        let mut servidor = Servidor::criar(&nome, config, parada.clone()).unwrap();
        let thread = thread::spawn(move || servidor.executar(&mut tratador_falso));
        Self {
            nome,
            parada,
            thread: Some(thread),
        }
    }

    fn parar(mut self) {
        self.parada.parar();
        self.thread.take().unwrap().join().unwrap().unwrap();
    }
}

impl Drop for ServidorDeTeste {
    fn drop(&mut self) {
        self.parada.parar();
    }
}

/// Abre o pipe "na mão" com o mesmo acesso que a ACL concede, para enviar
/// quadros arbitrários.
fn abrir_bruto(nome: &str) -> File {
    OpenOptions::new()
        .access_mode(0x0012_008b) // FILE_GENERIC_READ | FILE_WRITE_DATA
        .open(nome)
        .unwrap()
}

#[test]
fn requisicoes_e_respostas() {
    let s = ServidorDeTeste::iniciar("basico", ConfigServidor::default());
    let mut c = Cliente::conectar(&s.nome, &config_cliente()).unwrap();

    match c.requisitar(&Requisicao::ObterEstado).unwrap() {
        Resposta::Estado(e) => assert_eq!(e.jogos, 3),
        outra => panic!("{outra:?}"),
    }
    match c
        .requisitar(&Requisicao::ListarHistorico { limite: 4 })
        .unwrap()
    {
        Resposta::Historico { entradas } => assert_eq!(entradas.len(), 4),
        outra => panic!("{outra:?}"),
    }
    assert_eq!(
        c.requisitar(&Requisicao::AplicarAgora).unwrap(),
        Resposta::Ok
    );
    assert!(matches!(
        c.requisitar(&Requisicao::TravarSetting {
            perfil: "global".into(),
            setting: "proibida".into(),
        })
        .unwrap(),
        Resposta::Erro {
            codigo: CodigoErro::SettingNaoPermitida,
            ..
        }
    ));
    drop(c);
    s.parar();
}

#[test]
fn conexoes_sequenciais() {
    let s = ServidorDeTeste::iniciar("sequencial", ConfigServidor::default());
    for _ in 0..5 {
        let mut c = Cliente::conectar(&s.nome, &config_cliente()).unwrap();
        assert_eq!(c.requisitar(&Requisicao::ObterPlano).unwrap(), Resposta::Ok);
    }
    // Duas threads disputando: a segunda espera a primeira liberar.
    let nome = s.nome.clone();
    let outras: Vec<_> = (0..3)
        .map(|_| {
            let nome = nome.clone();
            thread::spawn(move || {
                let mut c = Cliente::conectar(&nome, &config_cliente()).unwrap();
                c.requisitar(&Requisicao::RestaurarOriginal).unwrap()
            })
        })
        .collect();
    for t in outras {
        assert_eq!(t.join().unwrap(), Resposta::Ok);
    }
    s.parar();
}

#[test]
fn validacao_de_forma_no_servidor() {
    let s = ServidorDeTeste::iniciar("validacao", ConfigServidor::default());
    let mut c = Cliente::conectar(&s.nome, &config_cliente()).unwrap();
    for req in [
        Requisicao::ListarHistorico { limite: 0 },
        Requisicao::DestravarSetting {
            perfil: "global".into(),
            setting: "0x10A879CF".into(),
        },
    ] {
        assert!(matches!(
            c.requisitar(&req).unwrap(),
            Resposta::Erro {
                codigo: CodigoErro::RequisicaoInvalida,
                ..
            }
        ));
    }
    // A conexão continua utilizável depois de erros de validação.
    assert_eq!(c.requisitar(&Requisicao::ObterPlano).unwrap(), Resposta::Ok);
    drop(c);
    s.parar();
}

#[test]
fn panico_do_tratador_vira_erro_interno() {
    let s = ServidorDeTeste::iniciar("panico", ConfigServidor::default());
    let mut c = Cliente::conectar(&s.nome, &config_cliente()).unwrap();
    assert!(matches!(
        c.requisitar(&Requisicao::ReescanearJogos).unwrap(),
        Resposta::Erro {
            codigo: CodigoErro::Interno,
            ..
        }
    ));
    assert_eq!(c.requisitar(&Requisicao::ObterPlano).unwrap(), Resposta::Ok);
    drop(c);
    s.parar();
}

#[test]
fn versao_errada_e_json_invalido_pelo_pipe() {
    let s = ServidorDeTeste::iniciar("versao", ConfigServidor::default());
    let mut f = abrir_bruto(&s.nome);

    let enviar = |f: &mut File, payload: &[u8]| {
        f.write_all(&(payload.len() as u32).to_le_bytes()).unwrap();
        f.write_all(payload).unwrap();
        decodificar::<Resposta>(&ler_quadro(f).unwrap()).unwrap()
    };

    let r = enviar(&mut f, br#"{"versao":999,"corpo":{"tipo":"obter_estado"}}"#);
    assert!(matches!(
        r,
        Resposta::Erro {
            codigo: CodigoErro::VersaoIncompativel,
            ..
        }
    ));
    let r = enviar(
        &mut f,
        br#"{"versao":3,"corpo":{"tipo":"executar","cmd":"x"}}"#,
    );
    assert!(matches!(
        r,
        Resposta::Erro {
            codigo: CodigoErro::MensagemInvalida,
            ..
        }
    ));
    // Mensagem válida depois dos erros: o enquadramento segue alinhado.
    let r = enviar(&mut f, &codificar(&Requisicao::ObterPlano).unwrap());
    assert_eq!(r, Resposta::Ok);
    assert_eq!(PROTOCOL_VERSION, 3);
    drop(f);
    s.parar();
}

#[test]
fn quadro_acima_do_limite_encerra_a_conexao() {
    let s = ServidorDeTeste::iniciar("grande", ConfigServidor::default());
    let mut f = abrir_bruto(&s.nome);
    f.write_all(&(TAMANHO_MAXIMO + 1).to_le_bytes()).unwrap();
    let r = decodificar::<Resposta>(&ler_quadro(&mut f).unwrap()).unwrap();
    assert!(matches!(
        r,
        Resposta::Erro {
            codigo: CodigoErro::MensagemGrande,
            ..
        }
    ));
    drop(f);
    // O servidor volta a aceitar conexões.
    let mut c = Cliente::conectar(&s.nome, &config_cliente()).unwrap();
    assert_eq!(c.requisitar(&Requisicao::ObterPlano).unwrap(), Resposta::Ok);
    drop(c);
    s.parar();
}

#[test]
fn quadro_truncado_e_tempo_limite() {
    let config = ConfigServidor {
        timeout_leitura: Duration::from_millis(200),
        timeout_escrita: Duration::from_secs(1),
        ..ConfigServidor::default()
    };
    let s = ServidorDeTeste::iniciar("truncado", config);
    let mut f = abrir_bruto(&s.nome);
    // Cabeçalho promete 100 bytes, mas só 3 chegam: o servidor desiste.
    f.write_all(&100u32.to_le_bytes()).unwrap();
    f.write_all(b"abc").unwrap();
    let mut buf = [0u8; 16];
    // O servidor desconecta sem responder.
    assert!(matches!(f.read(&mut buf), Ok(0) | Err(_)));
    drop(f);
    let mut c = Cliente::conectar(&s.nome, &config_cliente()).unwrap();
    assert_eq!(c.requisitar(&Requisicao::ObterPlano).unwrap(), Resposta::Ok);
    drop(c);
    s.parar();
}

#[test]
fn cliente_ocioso_nao_bloqueia_o_servico() {
    let config = ConfigServidor {
        timeout_leitura: Duration::from_millis(200),
        timeout_escrita: Duration::from_secs(1),
        ..ConfigServidor::default()
    };
    let s = ServidorDeTeste::iniciar("ocioso", config);
    let _ocioso = abrir_bruto(&s.nome);
    // O servidor derruba o ocioso após o tempo limite e atende o próximo.
    let mut c = Cliente::conectar(&s.nome, &config_cliente()).unwrap();
    assert_eq!(c.requisitar(&Requisicao::ObterPlano).unwrap(), Resposta::Ok);
    drop(c);
    s.parar();
}

#[test]
fn parada_interrompe_espera_e_conexao() {
    let s = ServidorDeTeste::iniciar("parada", ConfigServidor::default());
    let _c = Cliente::conectar(&s.nome, &config_cliente()).unwrap();
    // Servidor está esperando a primeira requisição (timeout de 30 s);
    // a parada precisa interrompê-lo de imediato.
    let inicio = std::time::Instant::now();
    s.parar();
    assert!(inicio.elapsed() < Duration::from_secs(5));
}

#[test]
fn servico_indisponivel() {
    let nome = nome_unico("ausente");
    assert!(matches!(
        Cliente::conectar(&nome, &config_cliente()),
        Err(ErroIpc::ServicoIndisponivel)
    ));
}

#[test]
fn segunda_instancia_com_mesmo_nome_e_recusada() {
    let s = ServidorDeTeste::iniciar("unica", ConfigServidor::default());
    let parada = Parada::nova().unwrap();
    assert!(Servidor::criar(&s.nome, ConfigServidor::default(), parada).is_err());
    s.parar();
}

#[test]
fn acl_nao_concede_criacao_de_instancia() {
    let s = ServidorDeTeste::iniciar("acl", ConfigServidor::default());
    // GENERIC_WRITE inclui FILE_APPEND_DATA (= FILE_CREATE_PIPE_INSTANCE),
    // que a ACL não concede a usuários autenticados.
    let r = OpenOptions::new().read(true).write(true).open(&s.nome);
    if !processo_elevado() {
        let e = r.expect_err("GENERIC_WRITE deveria ser negado");
        assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied);
    }
    s.parar();
}

#[test]
fn cliente_recusa_dono_sem_privilegio() {
    if processo_elevado() {
        // Elevado, o dono do pipe de teste é Administradores: nada a testar.
        return;
    }
    let s = ServidorDeTeste::iniciar("dono", ConfigServidor::default());
    let config = ConfigCliente {
        exigir_dono_privilegiado: true,
        ..config_cliente()
    };
    assert!(matches!(
        Cliente::conectar(&s.nome, &config),
        Err(ErroIpc::ServidorNaoConfiavel)
    ));
    s.parar();
}

fn processo_elevado() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: pseudo-handle do processo atual; o token é fechado ao final.
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut el = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut n = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            (&raw mut el).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut n,
        );
        CloseHandle(token);
        ok != 0 && el.TokenIsElevated != 0
    }
}

#[test]
fn taxa_acima_do_limite_responde_ocupado_sem_chamar_o_tratador() {
    let config = ConfigServidor {
        requisicoes_por_segundo: 1,
        rajada: 2,
        ..ConfigServidor::default()
    };
    let s = ServidorDeTeste::iniciar("taxa", config);
    let mut c = Cliente::conectar(&s.nome, &config_cliente()).unwrap();
    assert_eq!(c.requisitar(&Requisicao::ObterPlano).unwrap(), Resposta::Ok);
    assert_eq!(c.requisitar(&Requisicao::ObterPlano).unwrap(), Resposta::Ok);
    // Balde vazio: `ocupado`, e o tratador (que entraria em pânico) não roda.
    assert!(matches!(
        c.requisitar(&Requisicao::ReescanearJogos).unwrap(),
        Resposta::Erro {
            codigo: CodigoErro::Ocupado,
            ..
        }
    ));
    drop(c);
    // Reconectar não renova o balde.
    let mut c = Cliente::conectar(&s.nome, &config_cliente()).unwrap();
    assert!(matches!(
        c.requisitar(&Requisicao::ObterPlano).unwrap(),
        Resposta::Erro {
            codigo: CodigoErro::Ocupado,
            ..
        }
    ));
    // Depois de 1 s há ficha de novo.
    thread::sleep(Duration::from_millis(1100));
    assert_eq!(c.requisitar(&Requisicao::ObterPlano).unwrap(), Resposta::Ok);
    drop(c);
    s.parar();
}

#[test]
fn limite_de_requisicoes_por_conexao_encerra_a_conexao() {
    let config = ConfigServidor {
        requisicoes_por_conexao: 3,
        ..ConfigServidor::default()
    };
    let s = ServidorDeTeste::iniciar("por-conexao", config);
    let mut c = Cliente::conectar(&s.nome, &config_cliente()).unwrap();
    for _ in 0..3 {
        assert_eq!(c.requisitar(&Requisicao::ObterPlano).unwrap(), Resposta::Ok);
    }
    assert!(matches!(
        c.requisitar(&Requisicao::ObterPlano).unwrap(),
        Resposta::Erro {
            codigo: CodigoErro::Ocupado,
            ..
        }
    ));
    // A conexão foi encerrada pelo servidor.
    assert!(c.requisitar(&Requisicao::ObterPlano).is_err());
    drop(c);
    let mut c = Cliente::conectar(&s.nome, &config_cliente()).unwrap();
    assert_eq!(c.requisitar(&Requisicao::ObterPlano).unwrap(), Resposta::Ok);
    drop(c);
    s.parar();
}

#[test]
fn conexao_que_pinga_e_encerrada_na_duracao_maxima() {
    let config = ConfigServidor {
        timeout_leitura: Duration::from_secs(5),
        duracao_maxima_conexao: Duration::from_millis(400),
        ..ConfigServidor::default()
    };
    let s = ServidorDeTeste::iniciar("duracao", config);
    let mut f = abrir_bruto(&s.nome);
    // Um byte por vez, sempre dentro do tempo limite de leitura: o prazo
    // total da conexão a encerra mesmo assim.
    let inicio = std::time::Instant::now();
    let mut encerrada = false;
    while inicio.elapsed() < Duration::from_secs(4) {
        if f.write_all(&[0u8]).is_err() {
            encerrada = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(encerrada, "o servidor deveria ter encerrado a conexão");
    drop(f);
    let mut c = Cliente::conectar(&s.nome, &config_cliente()).unwrap();
    assert_eq!(c.requisitar(&Requisicao::ObterPlano).unwrap(), Resposta::Ok);
    drop(c);
    s.parar();
}
