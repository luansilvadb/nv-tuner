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

use nvt_ipc::pipe::{
    Cliente, ConfigCliente, ConfigServidor, IdentidadeCliente, Parada, Servidor, Tratador,
};
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
fn tratador_falso(_identidade: IdentidadeCliente, req: Requisicao) -> Resposta {
    match req {
        Requisicao::ObterEstado => Resposta::Estado(Estado {
            agente: nvt_ipc::EstadoAgente::default(),
            diagnosticos: vec![],
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
            adiamento: None,
            atualizacao: Default::default(),
        }),
        Requisicao::ListarHistorico { limite } => Resposta::Historico {
            entradas: (0..u64::from(limite))
                .map(|id| EntradaHistorico {
                    camada: nvt_ipc::Camada::Driver,
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
        Self::com_tratador(teste, config, tratador_falso)
    }

    fn com_tratador(
        teste: &str,
        config: ConfigServidor,
        mut tratador: impl Tratador + Send + 'static,
    ) -> Self {
        let nome = nome_unico(teste);
        let parada = Parada::nova().unwrap();
        let mut servidor = Servidor::criar(&nome, config, parada.clone()).unwrap();
        let thread = thread::spawn(move || servidor.executar(&mut tratador));
        Self {
            nome,
            parada,
            thread: Some(thread),
        }
    }

    fn pedir(&self, req: &Requisicao) -> Resposta {
        Cliente::conectar(&self.nome, &config_cliente())
            .unwrap()
            .requisitar(req)
            .unwrap()
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
    use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;
    use windows_sys::Win32::System::Pipes::WaitNamedPipeW;

    let nome16: Vec<_> = nome.encode_utf16().chain(Some(0)).collect();
    let prazo = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match OpenOptions::new()
            .access_mode(0x0012_008b) // FILE_GENERIC_READ | FILE_WRITE_DATA
            .open(nome)
        {
            Ok(f) => return f,
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                let ms = prazo
                    .saturating_duration_since(std::time::Instant::now())
                    .as_millis();
                // SAFETY: nome terminado em zero e válido durante a chamada.
                assert!(ms > 0 && unsafe { WaitNamedPipeW(nome16.as_ptr(), ms as u32) } != 0);
            }
            Err(e) => panic!("falha ao abrir pipe de teste: {e}"),
        }
    }
}

#[test]
fn requisicoes_e_respostas() {
    let s = ServidorDeTeste::iniciar("basico", ConfigServidor::default());

    match s.pedir(&Requisicao::ObterEstado) {
        Resposta::Estado(e) => assert_eq!(e.jogos, 3),
        outra => panic!("{outra:?}"),
    }
    match s.pedir(&Requisicao::ListarHistorico { limite: 4 }) {
        Resposta::Historico { entradas } => assert_eq!(entradas.len(), 4),
        outra => panic!("{outra:?}"),
    }
    assert_eq!(s.pedir(&Requisicao::AplicarAgora), Resposta::Ok);
    assert!(matches!(
        s.pedir(&Requisicao::TravarSetting {
            perfil: "global".into(),
            setting: "proibida".into(),
        }),
        Resposta::Erro {
            codigo: CodigoErro::SettingNaoPermitida,
            ..
        }
    ));
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
    for req in [
        Requisicao::ListarHistorico { limite: 0 },
        Requisicao::DestravarSetting {
            perfil: "global".into(),
            setting: "0x10A879CF".into(),
        },
    ] {
        assert!(matches!(
            s.pedir(&req),
            Resposta::Erro {
                codigo: CodigoErro::RequisicaoInvalida,
                ..
            }
        ));
    }
    // O servidor aceita conexões após erros de validação.
    assert_eq!(s.pedir(&Requisicao::ObterPlano), Resposta::Ok);
    s.parar();
}

#[test]
fn panico_do_tratador_vira_erro_interno() {
    let s = ServidorDeTeste::iniciar("panico", ConfigServidor::default());
    assert!(matches!(
        s.pedir(&Requisicao::ReescanearJogos),
        Resposta::Erro {
            codigo: CodigoErro::Interno,
            ..
        }
    ));
    assert_eq!(s.pedir(&Requisicao::ObterPlano), Resposta::Ok);
    s.parar();
}

#[test]
fn versao_errada_e_json_invalido_pelo_pipe() {
    let s = ServidorDeTeste::iniciar("versao", ConfigServidor::default());
    for (payload, codigo) in [
        (
            br#"{"versao":4,"corpo":{"tipo":"executar","cmd":"x"}}"#.as_slice(),
            CodigoErro::VersaoIncompativel,
        ),
        (
            br#"{"versao":5,"corpo":{"tipo":"obter_estado"}}"#.as_slice(),
            CodigoErro::VersaoIncompativel,
        ),
        (
            br#"{"versao":6,"corpo":{"tipo":"executar","cmd":"x"}}"#.as_slice(),
            CodigoErro::VersaoIncompativel,
        ),
        (
            br#"{"versao":7,"corpo":{"tipo":"executar","cmd":"x"}}"#.as_slice(),
            CodigoErro::MensagemInvalida,
        ),
    ] {
        let mut f = abrir_bruto(&s.nome);
        nvt_ipc::quadro::escrever_quadro(&mut f, payload).unwrap();
        let r = decodificar::<Resposta>(&ler_quadro(&mut f).unwrap()).unwrap();
        assert!(matches!(r, Resposta::Erro { codigo: recebido, .. } if recebido == codigo));
    }
    assert_eq!(s.pedir(&Requisicao::ObterPlano), Resposta::Ok);
    assert_eq!(PROTOCOL_VERSION, 7);
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
    assert_eq!(s.pedir(&Requisicao::ObterPlano), Resposta::Ok);
    assert_eq!(s.pedir(&Requisicao::ObterPlano), Resposta::Ok);
    // Balde vazio: `ocupado`, e o tratador (que entraria em pânico) não roda.
    assert!(matches!(
        s.pedir(&Requisicao::ReescanearJogos),
        Resposta::Erro {
            codigo: CodigoErro::Ocupado,
            ..
        }
    ));
    // Reconectar não renova o balde.
    assert!(matches!(
        s.pedir(&Requisicao::ObterPlano),
        Resposta::Erro {
            codigo: CodigoErro::Ocupado,
            ..
        }
    ));
    // Depois de 1 s há ficha de novo.
    thread::sleep(Duration::from_millis(1100));
    assert_eq!(s.pedir(&Requisicao::ObterPlano), Resposta::Ok);
    s.parar();
}

#[test]
fn uma_requisicao_por_conexao() {
    let s = ServidorDeTeste::iniciar("por-conexao", ConfigServidor::default());
    let mut c = Cliente::conectar(&s.nome, &config_cliente()).unwrap();
    assert_eq!(c.requisitar(&Requisicao::ObterPlano).unwrap(), Resposta::Ok);
    assert!(matches!(
        c.requisitar(&Requisicao::ObterPlano),
        Err(ErroIpc::Fechado)
    ));
    let mut bruto = abrir_bruto(&s.nome);
    nvt_ipc::enviar(&mut bruto, &Requisicao::ObterPlano).unwrap();
    assert_eq!(
        nvt_ipc::receber::<_, Resposta>(&mut bruto).unwrap(),
        Resposta::Ok
    );
    let _ = nvt_ipc::enviar(&mut bruto, &Requisicao::ReescanearJogos);
    assert!(ler_quadro(&mut bruto).is_err());
    assert_eq!(s.pedir(&Requisicao::ObterPlano), Resposta::Ok);
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

fn consulta_agente() -> Requisicao {
    use nvt_ipc::{Fato, FatosUsuario};
    Requisicao::AgenteConsultar {
        fatos: FatosUsuario {
            config_jogo: vec![],
            tela: Fato::Desconhecido,
            mouse: Fato::Desconhecido,
            swap_effect_upgrade: Fato::Ausente,
            captura_historica: Fato::Conhecido(1),
            diagnosticos: vec![],
            gestao: vec![],
        },
        ultima_tarefa: None,
        geracao_restaurada: 0,
    }
}

#[test]
fn identidade_do_token_sem_impersonacao_no_tratador() {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_NO_TOKEN, LocalFree};
    use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
    use windows_sys::Win32::Security::{
        EqualSid, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenSessionId, TokenUser,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken,
    };

    let (tx, rx) = std::sync::mpsc::channel();
    let s = ServidorDeTeste::com_tratador(
        "identidade",
        ConfigServidor::default(),
        move |identidade: IdentidadeCliente, req| {
            assert_eq!(req, consulta_agente());
            let mut token = std::ptr::null_mut();
            // SAFETY: pseudo-handle local e saída gravável.
            assert_eq!(
                unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) },
                0
            );
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(ERROR_NO_TOKEN as i32)
            );
            tx.send(identidade).unwrap();
            Resposta::Encerrar
        },
    );
    assert_eq!(s.pedir(&consulta_agente()), Resposta::Encerrar);
    let identidade = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let mut token = std::ptr::null_mut();
    let mut sessao = 0u32;
    let mut usuario = [0usize; 32];
    let mut tamanho = 0;
    let mut sid = std::ptr::null_mut();
    let sid16: Vec<_> = identidade.sid.encode_utf16().chain(Some(0)).collect();
    // SAFETY: pseudo-handle local; buffers graváveis e alinhados; SID e
    // token são liberados após as consultas.
    unsafe {
        assert_ne!(
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token),
            0
        );
        assert_ne!(
            GetTokenInformation(
                token,
                TokenSessionId,
                (&raw mut sessao).cast(),
                4,
                &mut tamanho
            ),
            0
        );
        assert_ne!(
            GetTokenInformation(
                token,
                TokenUser,
                usuario.as_mut_ptr().cast(),
                size_of_val(&usuario) as u32,
                &mut tamanho
            ),
            0
        );
        assert_ne!(ConvertStringSidToSidW(sid16.as_ptr(), &mut sid), 0);
        assert_ne!(
            EqualSid(sid, (*usuario.as_ptr().cast::<TOKEN_USER>()).User.Sid),
            0
        );
        LocalFree(sid.cast());
        CloseHandle(token);
    }
    assert_eq!(identidade.sessao, sessao);
    s.parar();
}

#[test]
fn token_anonimo_e_sid_forjado_nao_chegam_ao_tratador() {
    use windows_sys::Win32::Storage::FileSystem::{SECURITY_ANONYMOUS, SECURITY_SQOS_PRESENT};

    let (tx, rx) = std::sync::mpsc::channel();
    let s = ServidorDeTeste::com_tratador(
        "anonimo",
        ConfigServidor::default(),
        move |_: IdentidadeCliente, req| {
            tx.send(req).unwrap();
            Resposta::Ok
        },
    );
    {
        let mut f = OpenOptions::new()
            .access_mode(0x0012_008b)
            .security_qos_flags(SECURITY_SQOS_PRESENT | SECURITY_ANONYMOUS)
            .open(&s.nome)
            .unwrap();
        nvt_ipc::enviar(&mut f, &consulta_agente()).unwrap();
        assert!(matches!(
            nvt_ipc::receber::<_, Resposta>(&mut f).unwrap(),
            Resposta::Erro {
                codigo: CodigoErro::IdentidadeIndisponivel,
                ..
            }
        ));
    }
    {
        let mut f = abrir_bruto(&s.nome);
        let mut payload: serde_json::Value =
            serde_json::from_slice(&codificar(&consulta_agente()).unwrap()).unwrap();
        payload["corpo"]["sid"] = "S-1-5-18".into();
        payload["corpo"]["sessao"] = 0.into();
        nvt_ipc::quadro::escrever_quadro(&mut f, &serde_json::to_vec(&payload).unwrap()).unwrap();
        assert!(matches!(
            nvt_ipc::receber::<_, Resposta>(&mut f).unwrap(),
            Resposta::Erro {
                codigo: CodigoErro::MensagemInvalida,
                ..
            }
        ));
    }
    assert_eq!(s.pedir(&Requisicao::ObterEstado), Resposta::Ok);
    assert_eq!(rx.try_recv().unwrap(), Requisicao::ObterEstado);
    assert!(rx.try_recv().is_err());
    s.parar();
}

#[test]
fn ui_intercalada_com_dois_agentes_em_conexoes_curtas() {
    use nvt_ipc::{OperacaoUsuario, PlanoAgente};

    let s = ServidorDeTeste::com_tratador(
        "intercalado",
        ConfigServidor::default(),
        |identidade: IdentidadeCliente, req| {
            assert!(identidade.sid.starts_with("S-1-"));
            match req {
                Requisicao::AgenteConsultar { .. } => Resposta::PlanoAgente(PlanoAgente {
                    tarefa_jogo: None,
                    revisao: 7,
                    geracao_restauracao: 2,
                    tarefa: None,
                    processos_vigiados: ["cs2.exe".into()].into(),
                }),
                Requisicao::AgenteRelatar { revisao: 7, .. } => Resposta::Ok,
                req => tratador_falso(identidade, req),
            }
        },
    );
    let (consultas, recebidas) = std::sync::mpsc::channel();
    let agentes: Vec<_> = (1..=2)
        .map(|id_tarefa| {
            let (liberar, continuar) = std::sync::mpsc::channel();
            let nome = s.nome.clone();
            let consultas = consultas.clone();
            let agente = thread::spawn(move || {
                let pedir = |req| {
                    Cliente::conectar(&nome, &config_cliente())
                        .unwrap()
                        .requisitar(&req)
                        .unwrap()
                };
                let Resposta::PlanoAgente(plano) = pedir(consulta_agente()) else {
                    panic!("consulta sem plano");
                };
                consultas.send(()).unwrap();
                continuar.recv_timeout(Duration::from_secs(10)).unwrap();
                assert_eq!(
                    pedir(Requisicao::AgenteRelatar {
                        resultados_jogo: vec![],
                        id_tarefa,
                        operacao: OperacaoUsuario::Aplicar,
                        revisao: plano.revisao,
                        resultados: vec![],
                    }),
                    Resposta::Ok
                );
            });
            (liberar, agente)
        })
        .collect();
    for _ in 0..2 {
        recebidas.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    assert!(matches!(
        s.pedir(&Requisicao::ObterEstado),
        Resposta::Estado(_)
    ));
    for (liberar, agente) in agentes {
        liberar.send(()).unwrap();
        agente.join().unwrap();
        assert!(matches!(
            s.pedir(&Requisicao::ListarHistorico { limite: 1 }),
            Resposta::Historico { .. }
        ));
    }
    s.parar();
}
