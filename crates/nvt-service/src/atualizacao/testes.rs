//! Testes do [`Atualizador`] só com fachadas falsas: sem rede, sem processo, sem
//! escrita em disco (tarefa 3.1: U-09, U-14, U-17, U-26..U-28, U-33..U-38, U-43, U-46).

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use minisign::KeyPair;
use nvt_atualizacao::{
    Chave, EstadoPersistido, Fase, MotivoAdiamento, SessaoAgente, URL_ASSINATURA, URL_MANIFESTO,
    UltimoResultado, Versao, comentario_manifesto, gerar_manifesto_json, nome_log, nome_msi,
    sha256_hex, url_msi,
};
use nvt_ipc::{ErroVerificacao as IpcErro, MotivoPendente};

use super::fachadas::falsas::*;
use super::fachadas::{Fachadas, Relogio, Verificador, VerificadorOficial};
use super::http::ErroRede;
use super::lancador::{ErroLancador, LancadorFalso};
use super::{Atualizador, ESTADO, ESTADO_BAK};

const MIN: u64 = 60;
const HORA: u64 = 3600;
const T0_UNIX: u64 = 1_800_000_000;

// ---------------------------------------------------------------- apoio

struct Par {
    par: KeyPair,
    publica: String,
}

fn novo_par() -> Par {
    let par = KeyPair::generate_unencrypted_keypair().unwrap();
    let publica = par.pk.to_base64();
    Par { par, publica }
}

fn par() -> &'static Par {
    static P: OnceLock<Par> = OnceLock::new();
    P.get_or_init(novo_par)
}

fn assinar(p: &Par, dados: &[u8], comentario: &str) -> Vec<u8> {
    minisign::sign(Some(&p.par.pk), &p.par.sk, dados, Some(comentario), None)
        .unwrap()
        .into_string()
        .into_bytes()
}

fn v(s: &str) -> Versao {
    s.parse().unwrap()
}

fn msi_de(versao: &str) -> Vec<u8> {
    format!("MSI falso da versão {versao}").into_bytes()
}

/// Verificador que simula a chave oficial provisória.
struct VerificadorProvisorio;

impl Verificador for VerificadorProvisorio {
    fn chave_provisoria(&self) -> bool {
        true
    }

    fn verificar(
        &self,
        _: &[u8],
        _: &[u8],
    ) -> Result<nvt_atualizacao::Manifesto, nvt_atualizacao::ErroVerificacao> {
        Err(nvt_atualizacao::ErroVerificacao::ChaveProvisoria)
    }
}

struct Amb {
    rede: RedeFalsa,
    disco: DiscoFalso,
    portao: PortaoFalso,
    relogio: RelogioFalso,
    aleatorio: AleatorioFalso,
    sessoes: SessoesFalsas,
    lancador: LancadorCompartilhado,
    t0: Instant,
    s: u64,
}

impl Amb {
    fn novo() -> Amb {
        Amb::com_lancador(LancadorFalso::default())
    }

    fn com_lancador(l: LancadorFalso) -> Amb {
        let relogio = RelogioFalso::default();
        relogio.definir(T0_UNIX);
        Amb {
            rede: RedeFalsa::default(),
            disco: DiscoFalso::novo(),
            portao: PortaoFalso::default(),
            relogio,
            aleatorio: AleatorioFalso::default(),
            sessoes: SessoesFalsas::default(),
            lancador: LancadorCompartilhado(Arc::new(l)),
            t0: Instant::now(),
            s: 0,
        }
    }

    fn fachadas(&self, verificador: Box<dyn Verificador>) -> Fachadas {
        Fachadas {
            rede: Box::new(self.rede.clone()),
            disco: Box::new(self.disco.clone()),
            lancador: Box::new(self.lancador.clone()),
            portao: Box::new(self.portao.clone()),
            relogio: Box::new(self.relogio.clone()),
            aleatorio: Box::new(self.aleatorio.clone()),
            verificador,
            sessoes: Box::new(self.sessoes.clone()),
        }
    }

    /// Atualizador com a chave de teste injetada.
    fn atualizador(&self, instalada: &str) -> Atualizador {
        let chave = Chave::de_base64(&par().publica).unwrap();
        Atualizador::novo(
            self.fachadas(Box::new(VerificadorTeste(chave))),
            v(instalada),
        )
    }

    fn agora(&self) -> Instant {
        self.t0 + Duration::from_secs(self.s)
    }

    /// Avança `s` segundos (monotônico e parede) e roda um tick.
    fn passar(&mut self, a: &mut Atualizador, s: u64) -> Option<Duration> {
        self.s += s;
        self.relogio.avancar(s);
        a.tick(self.agora())
    }

    /// Avança até o próximo prazo devolvido por `tick` e roda.
    fn proximo(&mut self, a: &mut Atualizador, prazo: Option<Duration>) -> Option<Duration> {
        self.passar(a, prazo.expect("há prazo").as_secs())
    }

    fn publicar_manifesto(&self, manifesto: &[u8], assinatura: &[u8]) {
        self.rede.responder(URL_MANIFESTO, Ok(manifesto.to_vec()));
        self.rede.responder(URL_ASSINATURA, Ok(assinatura.to_vec()));
    }

    /// Publica a versão com manifesto assinado pela chave de teste e o MSI.
    fn publicar(&self, versao: &str) {
        let msi = msi_de(versao);
        let m = gerar_manifesto_json(v(versao), msi.len() as u64, &sha256_hex(&msi));
        let sig = assinar(par(), m.as_bytes(), &comentario_manifesto(v(versao)));
        self.publicar_manifesto(m.as_bytes(), &sig);
        self.rede.responder(&url_msi(v(versao)), Ok(msi));
    }

    fn lancamentos(&self) -> usize {
        self.lancador.0.msiexec.lock().unwrap().len()
    }

    fn agentes(&self) -> Vec<(u32, String)> {
        self.lancador.0.agentes.lock().unwrap().clone()
    }

    fn pedidos_msi(&self) -> usize {
        self.rede
            .pedidos()
            .iter()
            .filter(|u| u.ends_with(".msi"))
            .count()
    }

    fn persistido(&self) -> EstadoPersistido {
        serde_json::from_slice(&self.disco.conteudo(ESTADO).expect("estado.json")).unwrap()
    }

    fn por_log(&self, versao: &str, codigo: i64) {
        let texto =
            format!("MSI (s) (A4:B8) [10:00:01:124]: MainEngineThread is returning {codigo}\r\n");
        self.disco.por(&nome_log(v(versao)), texto.as_bytes());
    }

    fn por_estado(&self, e: &EstadoPersistido) {
        self.disco.por(ESTADO, &e.serializar());
        self.disco.por(ESTADO_BAK, &e.serializar());
    }
}

fn segundos(d: Option<Duration>) -> u64 {
    d.expect("há prazo").as_secs()
}

fn instalando(de: &str, para: &str, inicio_unix: u64, sessoes: Vec<SessaoAgente>) -> Fase {
    Fase::Instalando {
        de: v(de),
        para: v(para),
        sha256: "0".repeat(64),
        msi: nome_msi(v(para)),
        log: nome_log(v(para)),
        pid: Some(4242),
        inicio_unix,
        sessoes_agente: sessoes,
    }
}

fn sessao(n: u32, sid: &str) -> SessaoAgente {
    SessaoAgente {
        sessao: n,
        sid: sid.into(),
    }
}

// ---------------------------------------------------------------- fluxo

#[test]
fn atualizador_e_send() {
    fn exige<T: Send>() {}
    exige::<Atualizador>();
}

#[test]
fn fluxo_completo_versao_nova_lanca() {
    let mut amb = Amb::novo();
    amb.publicar("0.2.0");
    let mut a = amb.atualizador("0.1.0");
    a.definir_sessoes_agente(vec![sessao(1, "S-1-5-21-1-1001")]);

    // U-01: primeira verificação 5–15 min após iniciar; nada antes.
    let p = amb.passar(&mut a, 0);
    assert_eq!(segundos(p), 5 * MIN);
    assert!(amb.rede.pedidos().is_empty());
    assert_eq!(
        amb.disco.limpezas.lock().unwrap().len(),
        1,
        "limpeza no início"
    );

    let p = amb.proximo(&mut a, p);
    let log = a
        .ultimo_lancamento()
        .expect("lançamento registrado")
        .to_owned();
    assert_eq!(amb.lancamentos(), 1);
    assert_eq!(*amb.portao.consultas.lock().unwrap(), 1);
    let (msi, log_msi) = amb.lancador.0.msiexec.lock().unwrap()[0].clone();
    let pasta = DiscoFalso::pasta_falsa();
    assert_eq!(msi, pasta.join("nv-tuner-0.2.0-x64.msi"));
    assert_eq!(log_msi, pasta.join("instalacao-0.2.0.log"));

    // U-28: `instalando` com as sessões persistido (e o PID depois do lançamento).
    let e = amb.persistido();
    assert_eq!(e.maior_vista, Some(v("0.2.0")));
    match &e.fase {
        Fase::Instalando {
            de,
            para,
            pid,
            sessoes_agente,
            sha256,
            ..
        } => {
            assert_eq!((*de, *para, *pid), (v("0.1.0"), v("0.2.0"), Some(4242)));
            assert_eq!(sessoes_agente, &[sessao(1, "S-1-5-21-1-1001")]);
            assert_eq!(sha256, &sha256_hex(&msi_de("0.2.0")));
        }
        f => panic!("fase {f:?}"),
    }
    assert_eq!(
        amb.disco.conteudo(ESTADO),
        amb.disco.conteudo(ESTADO_BAK),
        "cópia .bak a cada mudança"
    );

    // U-34: registro do lançamento.
    for trecho in [
        "msiexec lançado",
        "de=0.1.0",
        "para=0.2.0",
        "pid=4242",
        "flags=0x",
        "quando_unix=",
    ] {
        assert!(log.contains(trecho), "{trecho} em {log}");
    }
    assert!(log.contains(&sha256_hex(&msi_de("0.2.0"))));

    // U-33: o serviço não espera; relê o log em 60 s.
    assert_eq!(segundos(p), 60);
    let ipc = a.estado_ipc();
    assert_eq!(ipc.versao_instalada, "0.1.0");
    assert_eq!(
        ipc.pendente.as_ref().map(|p| p.motivo),
        Some(MotivoPendente::Instalando)
    );
    assert!(!ipc.nao_vista);
}

#[test]
fn jogo_aberto_adia_e_reavalia() {
    let mut amb = Amb::novo();
    amb.publicar("0.2.0");
    *amb.portao.adiar.lock().unwrap() = Some(MotivoAdiamento::JogoAberto);
    let mut a = amb.atualizador("0.1.0");
    let p = amb.passar(&mut a, 0);
    let p = amb.proximo(&mut a, p);
    assert_eq!(amb.lancamentos(), 0, "U-26: portão adia, nada lançado");
    assert!(matches!(a.estado().fase, Fase::Pronta { .. }));
    assert_eq!(
        a.estado().ultimo_resultado,
        Some(UltimoResultado::AdiadaPortao)
    );
    assert_eq!(segundos(p), 60, "reavalia a cada 60 s");
    let ipc = a.estado_ipc();
    let pend = ipc.pendente.unwrap();
    assert_eq!(pend.motivo, MotivoPendente::JogoAberto);
    assert_eq!(pend.versao, "0.2.0");
    assert!(ipc.erro_verificacao.is_none());

    // Continua aberto: reconsulta sem regravar.
    let gravacoes = amb.disco.gravacoes.lock().unwrap().len();
    let p = amb.proximo(&mut a, p);
    assert_eq!(*amb.portao.consultas.lock().unwrap(), 2);
    assert_eq!(amb.disco.gravacoes.lock().unwrap().len(), gravacoes);

    // Lista ilegível também adia (falha fechada).
    *amb.portao.adiar.lock().unwrap() = Some(MotivoAdiamento::ListaIndisponivel);
    let p = amb.proximo(&mut a, p);
    assert_eq!(
        a.estado_ipc().pendente.unwrap().motivo,
        MotivoPendente::ListaIndisponivel
    );
    assert_eq!(amb.lancamentos(), 0);

    // Fecha o jogo: lança no passo seguinte, sem nova verificação na rede.
    *amb.portao.adiar.lock().unwrap() = None;
    let pedidos = amb.rede.pedidos().len();
    amb.proximo(&mut a, p);
    assert_eq!(amb.lancamentos(), 1);
    assert_eq!(amb.rede.pedidos().len(), pedidos);
}

#[test]
fn msiexecute_ocupado_adia_30_min() {
    let mut amb = Amb::novo();
    amb.publicar("0.2.0");
    *amb.portao.ocupado.lock().unwrap() = true;
    let mut a = amb.atualizador("0.1.0");
    let p = amb.passar(&mut a, 0);
    let p = amb.proximo(&mut a, p);
    assert_eq!(amb.lancamentos(), 0);
    assert_eq!(segundos(p), 30 * MIN, "U-27");
    assert_eq!(
        a.estado_ipc().pendente.unwrap().motivo,
        MotivoPendente::OutraInstalacao
    );
    assert!(a.estado().falhas.is_empty(), "não conta falha");
    *amb.portao.ocupado.lock().unwrap() = false;
    amb.proximo(&mut a, p);
    assert_eq!(amb.lancamentos(), 1);
}

#[test]
fn msi_alterado_em_pronta_descarta_e_verifica_de_novo() {
    let mut amb = Amb::novo();
    amb.publicar("0.2.0");
    *amb.portao.adiar.lock().unwrap() = Some(MotivoAdiamento::JogoAberto);
    let mut a = amb.atualizador("0.1.0");
    let p = amb.passar(&mut a, 0);
    let p = amb.proximo(&mut a, p);
    *amb.disco.intacto.lock().unwrap() = false;
    let p = amb.proximo(&mut a, p);
    let pedidos = amb.rede.pedidos();
    assert_eq!(
        pedidos.iter().filter(|u| *u == URL_MANIFESTO).count(),
        2,
        "verifica de novo já"
    );
    assert_eq!(a.estado().fase, Fase::Ocioso);
    assert_eq!(segundos(p), 30 * MIN);
    assert_eq!(amb.lancamentos(), 0);
}

#[test]
fn falha_ao_persistir_instalando_nao_lanca() {
    let mut amb = Amb::novo();
    amb.publicar("0.2.0");
    *amb.portao.adiar.lock().unwrap() = Some(MotivoAdiamento::JogoAberto);
    let mut a = amb.atualizador("0.1.0");
    let p = amb.passar(&mut a, 0);
    let p = amb.proximo(&mut a, p);
    *amb.portao.adiar.lock().unwrap() = None;
    *amb.disco.falhar_gravacao.lock().unwrap() = true;
    let p = amb.proximo(&mut a, p);
    assert_eq!(amb.lancamentos(), 0, "U-28: falha ao persistir → não lança");
    assert!(matches!(a.estado().fase, Fase::Pronta { .. }));
    assert_eq!(a.estado_ipc().erro_verificacao, Some(IpcErro::Disco));
    assert!(segundos(p) >= 60, "sem laço agressivo");
    *amb.disco.falhar_gravacao.lock().unwrap() = false;
    amb.proximo(&mut a, p);
    assert_eq!(amb.lancamentos(), 1);
    assert!(matches!(amb.persistido().fase, Fase::Instalando { .. }));
}

#[test]
fn falha_do_create_process_conta_falha() {
    let mut amb = Amb::com_lancador(LancadorFalso {
        resposta_msiexec: Err(ErroLancador::Win32 {
            api: "msiexec",
            codigo: 2,
        }),
        ..LancadorFalso::default()
    });
    amb.publicar("0.2.0");
    let mut a = amb.atualizador("0.1.0");
    let p = amb.passar(&mut a, 0);
    amb.proximo(&mut a, p);
    let e = amb.persistido();
    assert_eq!(e.fase, Fase::Ocioso);
    assert_eq!(e.falhas.get(&v("0.2.0")), Some(&1));
    assert_eq!(e.ultima_falha.unwrap().codigo, 2);
}

// ---------------------------------------------------------------- códigos do MSI

/// Leva o atualizador até `instalando` com a versão `para` publicada.
fn ate_lancar(amb: &mut Amb, a: &mut Atualizador, para: &str, p: Option<Duration>) {
    amb.publicar(para);
    let antes = amb.lancamentos();
    amb.proximo(a, p);
    assert_eq!(amb.lancamentos(), antes + 1);
}

#[test]
fn codigo_1618_adia_sem_contar() {
    let mut amb = Amb::novo();
    let mut a = amb.atualizador("0.1.0");
    let p = amb.passar(&mut a, 0);
    ate_lancar(&mut amb, &mut a, "0.2.0", p);
    // Serviço antigo vivo: o msiexec terminou com 1618 (U-33, via log).
    amb.por_log("0.2.0", 1618);
    let p = amb.passar(&mut a, 60);
    let e = amb.persistido();
    assert_eq!(e.fase, Fase::Ocioso);
    assert_eq!(e.ultimo_resultado, Some(UltimoResultado::AdiadaMsiOcupado));
    assert!(e.falhas.is_empty(), "U-37: 1618 não conta");
    assert_eq!(segundos(p), 30 * MIN);
    assert_eq!(
        a.estado_ipc().pendente.unwrap().motivo,
        MotivoPendente::OutraInstalacao
    );
}

#[test]
fn tres_falhas_abandonam_ate_surgir_maior() {
    let mut amb = Amb::novo();
    let mut a = amb.atualizador("0.1.0");
    let mut p = amb.passar(&mut a, 0);
    for n in 1..=3u8 {
        ate_lancar(&mut amb, &mut a, "0.2.0", p);
        amb.por_log("0.2.0", 1603);
        p = amb.passar(&mut a, 60);
        let e = amb.persistido();
        assert_eq!(e.fase, Fase::Ocioso);
        assert_eq!(e.ultima_falha.as_ref().unwrap().codigo, 1603);
        if n < 3 {
            assert_eq!(e.falhas.get(&v("0.2.0")), Some(&n));
            assert!(segundos(p) >= 5 * HORA, "próxima tentativa só na regular");
        }
    }
    let e = amb.persistido();
    assert_eq!(e.abandonadas, vec![v("0.2.0")], "U-38");
    let f = a.estado_ipc().falha.unwrap();
    assert!(f.abandonada);
    assert_eq!(
        (f.versao.as_str(), f.codigo, f.tentativas),
        ("0.2.0", 1603, 3)
    );

    // Abandonada: o manifesto da mesma versão não baixa nada.
    amb.publicar("0.2.0");
    let msis = amb.pedidos_msi();
    p = amb.proximo(&mut a, p);
    assert_eq!(amb.pedidos_msi(), msis);
    assert_eq!(amb.lancamentos(), 3);

    // U-19: versão maior volta a ser tentada.
    ate_lancar(&mut amb, &mut a, "0.2.1", p);
}

#[test]
fn codigo_1625_abandona_de_imediato() {
    let mut amb = Amb::novo();
    let mut a = amb.atualizador("0.1.0");
    let p = amb.passar(&mut a, 0);
    ate_lancar(&mut amb, &mut a, "0.2.0", p);
    amb.por_log("0.2.0", 1625);
    amb.passar(&mut a, 60);
    assert_eq!(amb.persistido().abandonadas, vec![v("0.2.0")]);
}

// ---------------------------------------------------------------- manifesto

#[test]
fn versao_igual_menor_ou_abaixo_da_maior_vista_recusada() {
    for (instalada, maior_vista, publicada) in [
        ("0.2.0", None, "0.2.0"),
        ("0.2.0", None, "0.1.9"),
        ("0.2.0", Some("0.3.0"), "0.2.5"),
    ] {
        let mut amb = Amb::novo();
        if let Some(m) = maior_vista {
            amb.por_estado(&EstadoPersistido {
                maior_vista: Some(v(m)),
                ..EstadoPersistido::default()
            });
        }
        amb.publicar(publicada);
        let mut a = amb.atualizador(instalada);
        let p = amb.passar(&mut a, 0);
        let p = amb.proximo(&mut a, p);
        assert_eq!(amb.pedidos_msi(), 0, "U-17: {publicada} sobre {instalada}");
        assert_eq!(amb.lancamentos(), 0);
        let e = amb.persistido();
        assert_eq!(e.ultimo_resultado, Some(UltimoResultado::SemNovidade));
        assert_eq!(e.maior_vista, Some(v(maior_vista.unwrap_or(instalada))));
        assert!(segundos(p) >= 5 * HORA + 30 * MIN);
    }
}

#[test]
fn maior_vista_recuperada_do_bak() {
    let mut amb = Amb::novo();
    amb.disco.por(ESTADO, b"{ corrompido");
    amb.disco
        .por(ESTADO_BAK, br#"{"maior_vista":"0.5.0","lixo":1}"#);
    amb.publicar("0.4.0");
    let mut a = amb.atualizador("0.1.0");
    let p = amb.passar(&mut a, 0);
    amb.proximo(&mut a, p);
    assert_eq!(amb.pedidos_msi(), 0, "U-18");
    assert_eq!(amb.persistido().maior_vista, Some(v("0.5.0")));
}

#[test]
fn assinatura_invalida_nao_baixa() {
    let outro = novo_par();
    let casos: Vec<(Vec<u8>, Vec<u8>)> = {
        let msi = msi_de("0.2.0");
        let m = gerar_manifesto_json(v("0.2.0"), msi.len() as u64, &sha256_hex(&msi));
        let c = comentario_manifesto(v("0.2.0"));
        let adulterado = m.replace("0.2.0", "0.3.0");
        vec![
            // Outra chave.
            (m.clone().into_bytes(), assinar(&outro, m.as_bytes(), &c)),
            // Manifesto adulterado depois de assinado.
            (adulterado.into_bytes(), assinar(par(), m.as_bytes(), &c)),
            // Comentário confiável de outra versão.
            (
                m.clone().into_bytes(),
                assinar(par(), m.as_bytes(), &comentario_manifesto(v("0.1.0"))),
            ),
            // Assinatura ausente/lixo.
            (m.into_bytes(), b"lixo".to_vec()),
        ]
    };
    for (manifesto, sig) in casos {
        let mut amb = Amb::novo();
        amb.publicar_manifesto(&manifesto, &sig);
        let mut a = amb.atualizador("0.1.0");
        let p = amb.passar(&mut a, 0);
        let p = amb.proximo(&mut a, p);
        assert_eq!(amb.pedidos_msi(), 0);
        assert_eq!(
            amb.persistido().ultimo_resultado,
            Some(UltimoResultado::ErroAssinatura)
        );
        assert_eq!(a.estado_ipc().erro_verificacao, Some(IpcErro::Assinatura));
        assert!(segundos(p) >= 5 * HORA + 30 * MIN, "não é falha de rede");
    }
}

#[test]
fn manifesto_404_e_sem_novidade() {
    let mut amb = Amb::novo();
    amb.rede
        .responder(URL_MANIFESTO, Err(ErroRede::NaoEncontrado));
    let mut a = amb.atualizador("0.1.0");
    let p = amb.passar(&mut a, 0);
    let p = amb.proximo(&mut a, p);
    assert_eq!(
        amb.persistido().ultimo_resultado,
        Some(UltimoResultado::SemNovidade)
    );
    assert!(a.estado_ipc().erro_verificacao.is_none());
    assert!(segundos(p) >= 5 * HORA + 30 * MIN);
}

// ---------------------------------------------------------------- download

#[test]
fn sha_ou_tamanho_divergente_descarta() {
    for corpo in [b"MSI adulterado com o mesmo tamanho?".to_vec(), Vec::new()] {
        let mut amb = Amb::novo();
        let msi = msi_de("0.2.0");
        let m = gerar_manifesto_json(v("0.2.0"), msi.len() as u64, &sha256_hex(&msi));
        let sig = assinar(par(), m.as_bytes(), &comentario_manifesto(v("0.2.0")));
        amb.publicar_manifesto(m.as_bytes(), &sig);
        let mut ruim = corpo;
        if ruim.is_empty() {
            // Mesmo tamanho, um byte trocado.
            ruim = msi.clone();
            ruim[0] ^= 1;
        }
        amb.rede.responder(&url_msi(v("0.2.0")), Ok(ruim));
        let mut a = amb.atualizador("0.1.0");
        let p = amb.passar(&mut a, 0);
        let p = amb.proximo(&mut a, p);
        assert_eq!(amb.pedidos_msi(), 1);
        assert_eq!(amb.lancamentos(), 0, "U-14");
        assert_eq!(*amb.portao.consultas.lock().unwrap(), 0);
        assert!(
            *amb.disco.descartes.lock().unwrap() >= 1,
            ".parcial apagado"
        );
        let e = amb.persistido();
        assert_eq!(e.fase, Fase::Ocioso);
        assert_eq!(
            e.maior_vista,
            Some(v("0.2.0")),
            "persistida antes de baixar"
        );
        assert!(segundos(p) >= 5 * HORA + 30 * MIN, "hash: 6 h");
    }
}

#[test]
fn download_interrompido_retoma_com_a_maior_vista() {
    let mut amb = Amb::novo();
    let msi = msi_de("0.2.0");
    let m = gerar_manifesto_json(v("0.2.0"), msi.len() as u64, &sha256_hex(&msi));
    let sig = assinar(par(), m.as_bytes(), &comentario_manifesto(v("0.2.0")));
    amb.publicar_manifesto(m.as_bytes(), &sig);
    amb.rede
        .responder(&url_msi(v("0.2.0")), Err(ErroRede::TempoEsgotado));
    let mut a = amb.atualizador("0.1.0");
    let p = amb.passar(&mut a, 0);
    let p = amb.proximo(&mut a, p);
    assert_eq!(segundos(p), 30 * MIN, "rede: espera exponencial");
    assert_eq!(
        amb.persistido().ultimo_resultado,
        Some(UltimoResultado::ErroRede)
    );
    // Igual à maior vista segue para download (U-17).
    ate_lancar(&mut amb, &mut a, "0.2.0", p);
}

// ---------------------------------------------------------------- rede

#[test]
fn sem_internet_backoff_sem_bloquear() {
    let mut amb = Amb::novo();
    let mut a = amb.atualizador("0.1.0");
    let mut p = amb.passar(&mut a, 0);
    for esperado in [30, 60, 120, 240, 360, 360] {
        p = amb.proximo(&mut a, p);
        assert_eq!(segundos(p), esperado * MIN, "U-02");
        let ipc = a.estado_ipc();
        assert_eq!(ipc.erro_verificacao, Some(IpcErro::SemConexao), "U-09");
        assert!(ipc.pendente.is_none() && ipc.falha.is_none());
        assert_eq!(ipc.ultima_verificacao_unix, Some(amb.relogio.agora_unix()));
        assert!(ipc.proxima_verificacao_unix.unwrap() > amb.relogio.agora_unix());
    }
    assert_eq!(amb.lancamentos(), 0);
    // Volta a conexão: zera a espera.
    amb.publicar("0.2.0");
    amb.proximo(&mut a, p);
    assert_eq!(amb.lancamentos(), 1);
    assert_eq!(amb.persistido().espera_rede_s, 0);
}

#[test]
fn retry_after_e_ratelimit_reset() {
    for (retry, reset, esperado) in [
        (Some("7200".to_owned()), None, 2 * HORA),
        (
            None,
            Some((T0_UNIX + 5 * MIN + 3 * HORA).to_string()),
            3 * HORA,
        ),
        (Some("10".to_owned()), None, 30 * MIN),
        (Some("999999".to_owned()), None, 24 * HORA),
    ] {
        let mut amb = Amb::novo();
        amb.rede.responder(
            URL_MANIFESTO,
            Err(ErroRede::Limitado {
                status: 429,
                retry_after: retry,
                ratelimit_reset: reset,
            }),
        );
        let mut a = amb.atualizador("0.1.0");
        let p = amb.passar(&mut a, 0);
        let p = amb.proximo(&mut a, p);
        assert_eq!(segundos(p), esperado);
        assert_eq!(
            a.estado_ipc().erro_verificacao,
            Some(IpcErro::RecusadoHttp(429))
        );
    }
}

// ---------------------------------------------------------------- reinício

#[test]
fn reinicio_no_meio_le_log() {
    let mut amb = Amb::novo();
    amb.por_estado(&EstadoPersistido {
        maior_vista: Some(v("0.2.0")),
        fase: instalando("0.1.0", "0.2.0", T0_UNIX - 10 * MIN, vec![]),
        ..EstadoPersistido::default()
    });
    amb.publicar("0.3.0");
    let mut a = amb.atualizador("0.1.0");
    assert_eq!(
        amb.disco.limpezas.lock().unwrap().as_slice(),
        &[Some(v("0.2.0"))],
        "U-25: mantém MSI e log da instalação em curso"
    );
    // Sem código no log: continua `instalando`, relê a cada 60 s, sem verificar.
    let mut p = amb.passar(&mut a, 0);
    for _ in 0..20 {
        assert_eq!(segundos(p), 60);
        assert!(matches!(a.estado().fase, Fase::Instalando { .. }));
        p = amb.proximo(&mut a, p);
    }
    assert!(amb.rede.pedidos().is_empty(), "nenhuma verificação nova");
    assert_eq!(amb.lancamentos(), 0, "nem lançamento");
    amb.por_log("0.2.0", 1603);
    p = amb.proximo(&mut a, p);
    let e = amb.persistido();
    assert_eq!(e.fase, Fase::Ocioso);
    assert_eq!(e.falhas.get(&v("0.2.0")), Some(&1));
    assert!(segundos(p) >= 5 * HORA);
}

#[test]
fn instalando_sem_codigo_por_mais_de_2h_conta_falha_desconhecida() {
    let mut amb = Amb::novo();
    amb.por_estado(&EstadoPersistido {
        fase: instalando("0.1.0", "0.2.0", T0_UNIX - 2 * HORA - 1, vec![]),
        ..EstadoPersistido::default()
    });
    let mut a = amb.atualizador("0.1.0");
    amb.passar(&mut a, 0);
    let e = amb.persistido();
    assert_eq!(e.fase, Fase::Ocioso);
    assert_eq!(e.ultima_falha.unwrap().codigo, -1);
}

#[test]
fn conclusao_apos_upgrade() {
    for (codigo, reinicio) in [(Some(0), false), (Some(3010), true), (None, false)] {
        let mut amb = Amb::novo();
        let mut e = EstadoPersistido {
            fase: instalando("0.1.0", "0.2.0", T0_UNIX - MIN, vec![]),
            ..EstadoPersistido::default()
        };
        e.falhas.insert(v("0.2.0"), 2);
        amb.por_estado(&e);
        if let Some(c) = codigo {
            amb.por_log("0.2.0", c);
        }
        let mut a = amb.atualizador("0.2.0");
        let p = amb.passar(&mut a, 0);
        let e = amb.persistido();
        assert_eq!(e.fase, Fase::Ocioso);
        assert!(e.falhas.is_empty());
        let u = e.ultima_atualizacao.unwrap();
        assert_eq!((u.de, u.para), (v("0.1.0"), v("0.2.0")));
        assert_eq!(u.codigo, codigo.unwrap_or(0), "U-35");
        let ipc = a.estado_ipc();
        let c = ipc.ultima_atualizacao.unwrap();
        assert_eq!((c.para.as_str(), c.reinicio_pendente), ("0.2.0", reinicio));
        assert!(ipc.falha.is_none() && ipc.pendente.is_none());
        assert!((5 * MIN..=15 * MIN).contains(&segundos(p)));
    }
}

#[test]
fn relanca_agente_so_nas_sessoes_certas_e_uma_vez() {
    let mut amb = Amb::com_lancador(LancadorFalso {
        falham: vec![4],
        ..LancadorFalso::default()
    });
    amb.por_estado(&EstadoPersistido {
        fase: instalando(
            "0.1.0",
            "0.2.0",
            T0_UNIX - MIN,
            vec![
                sessao(1, "S-1"),
                sessao(2, "S-2"),
                sessao(3, "S-3"),
                sessao(4, "S-4"),
                sessao(5, "S-5"),
            ],
        ),
        ..EstadoPersistido::default()
    });
    amb.por_log("0.2.0", 0);
    // 2 inativa; 3 já com agente conectado; 5 com agente presente (mutex).
    *amb.sessoes.ativas.lock().unwrap() = vec![1, 3, 4, 5, 9];
    *amb.sessoes.com_agente.lock().unwrap() = vec![5];
    let mut a = amb.atualizador("0.2.0");
    a.definir_sessoes_agente(vec![sessao(3, "S-3")]);
    amb.passar(&mut a, 0);
    assert_eq!(
        amb.agentes(),
        vec![(1, "S-1".to_owned()), (4, "S-4".to_owned())],
        "U-43: só sessões gravadas, ativas e sem agente; SID conferido pelo lançador"
    );
    // Persistido sem as sessões antes de relançar.
    assert_eq!(amb.persistido().fase, Fase::Ocioso);

    // Mais ticks e um novo serviço (queda após relançar) não repetem.
    amb.passar(&mut a, 15 * MIN);
    let mut b = amb.atualizador("0.2.0");
    amb.passar(&mut b, 0);
    assert_eq!(amb.agentes().len(), 2, "uma vez só");
}

// ---------------------------------------------------------------- chave provisória

#[test]
fn chave_provisoria_sem_atualizacao_verificavel_e_sem_laco() {
    let mut amb = Amb::novo();
    amb.publicar("0.2.0");
    let mut a = Atualizador::novo(amb.fachadas(Box::new(VerificadorProvisorio)), v("0.1.0"));
    let mut p = amb.passar(&mut a, 0);
    for _ in 0..3 {
        p = amb.proximo(&mut a, p);
        assert!(segundos(p) >= 5 * HORA + 30 * MIN, "sem laço agressivo");
    }
    assert!(amb.rede.pedidos().is_empty(), "nem contata a rede");
    assert_eq!(amb.lancamentos(), 0);
    assert_eq!(
        amb.persistido().ultimo_resultado,
        Some(UltimoResultado::ErroAssinatura)
    );
    assert_eq!(a.estado_ipc().erro_verificacao, Some(IpcErro::Assinatura));
}

#[test]
fn verificador_de_producao_com_chave_provisoria_recusa() {
    if !nvt_atualizacao::chave_oficial_provisoria() {
        return; // Chave oficial definitiva: coberto pelos testes do crate.
    }
    let mut amb = Amb::novo();
    amb.publicar("0.2.0");
    let mut a = Atualizador::novo(amb.fachadas(Box::new(VerificadorOficial)), v("0.1.0"));
    let p = amb.passar(&mut a, 0);
    let p = amb.proximo(&mut a, p);
    assert!(segundos(p) >= 5 * HORA + 30 * MIN);
    assert_eq!(amb.pedidos_msi(), 0);
    assert_eq!(amb.lancamentos(), 0);
}

// ---------------------------------------------------------------- relógio

#[test]
fn proxima_persistida_e_so_informativa() {
    let mut amb = Amb::novo();
    amb.por_estado(&EstadoPersistido {
        proxima_verificacao_unix: Some(T0_UNIX + 30 * 24 * HORA),
        ultima_verificacao_unix: Some(T0_UNIX + 400 * 24 * HORA),
        ..EstadoPersistido::default()
    });
    *amb.aleatorio.0.lock().unwrap() = 600;
    let mut a = amb.atualizador("0.1.0");
    let p = amb.passar(&mut a, 0);
    assert_eq!(segundos(p), 5 * MIN + 600, "U-01: 5–15 min do início");
    // Salto do relógio de parede para trás não muda o prazo monotônico.
    amb.relogio.definir(T0_UNIX - 3 * 365 * 24 * HORA);
    let p2 = a.tick(amb.agora());
    assert_eq!(p2, p);
}
