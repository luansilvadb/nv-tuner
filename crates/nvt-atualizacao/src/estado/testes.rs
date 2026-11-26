use super::*;

fn v(s: &str) -> Versao {
    s.parse().unwrap()
}

fn ctx(agora: u64, instalada: &str) -> Contexto {
    Contexto {
        agora_unix: agora,
        instalada: v(instalada),
        aleatorio: 0,
    }
}

fn manifesto(ver: &str) -> Manifesto {
    let versao = v(ver);
    Manifesto {
        versao,
        msi: crate::nome_msi(versao),
        tamanho: 10,
        sha256: "a".repeat(64),
    }
}

fn passo(e: &EstadoPersistido, ev: Evento, c: &Contexto) -> (EstadoPersistido, Vec<Efeito>) {
    transicao(e, ev, c)
}

/// Leva de `ocioso` até `pronta` da versão dada.
fn ate_pronta(e: &EstadoPersistido, ver: &str, c: &Contexto) -> EstadoPersistido {
    let (e, ef) = passo(e, Evento::PrazoVerificacao, c);
    assert_eq!(ef, [Efeito::BuscarManifesto]);
    let (e, ef) = passo(&e, Evento::ManifestoValido(manifesto(ver)), c);
    assert_eq!(ef[0], Efeito::Persistir);
    let fim = format!("/v{ver}/nv-tuner-{ver}-x64.msi");
    assert!(matches!(&ef[1], Efeito::Baixar(p) if p.url.ends_with(&fim)));
    assert_eq!(e.maior_vista, Some(v(ver)));
    let (e, ef) = passo(&e, Evento::MsiVerificado, c);
    assert_eq!(ef, [Efeito::Persistir, Efeito::ConsultarPortao]);
    assert!(matches!(e.fase, Fase::Pronta { .. }));
    e
}

fn instalando(ver: &str, c: &Contexto) -> EstadoPersistido {
    let e = ate_pronta(&EstadoPersistido::default(), ver, c);
    let sessoes = vec![SessaoAgente {
        sessao: 1,
        sid: "S-1-5-21-1".into(),
    }];
    let (e, ef) = passo(
        &e,
        Evento::PortaoLiberou {
            sessoes_agente: sessoes,
        },
        c,
    );
    assert_eq!(ef[0], Efeito::Persistir);
    let log = format!("instalacao-{ver}.log");
    assert!(matches!(&ef[1], Efeito::Lancar(p) if p.log == log));
    e
}

#[test]
fn fluxo_completo_ate_sucesso_no_servico_novo() {
    let c = ctx(1000, "0.1.0");
    let e = instalando("0.2.0", &c);
    let (e, ef) = passo(&e, Evento::Lancado { pid: 42 }, &c);
    assert_eq!(ef, [Efeito::Persistir]);
    let novo = ctx(1500, "0.2.0");
    let (e, ef) = passo(
        &e,
        Evento::Iniciado {
            codigo_log: Some(3010),
        },
        &novo,
    );
    assert_eq!(e.fase, Fase::Ocioso);
    let u = e.ultima_atualizacao.clone().unwrap();
    assert_eq!((u.de, u.para, u.codigo), (v("0.1.0"), v("0.2.0"), 3010));
    assert!(u.reinicio_pendente());
    assert_eq!(ef[0], Efeito::Persistir);
    assert!(
        ef.iter()
            .any(|x| matches!(x, Efeito::RelancarAgentes(s) if s.len() == 1))
    );
    assert!(
        ef.iter()
            .any(|x| matches!(x, Efeito::AgendarVerificacao(_)))
    );
}

#[test]
fn sucesso_presumido_sem_codigo_no_log() {
    let c = ctx(1000, "0.1.0");
    let e = instalando("0.2.0", &c);
    let (e, _) = passo(
        &e,
        Evento::Iniciado { codigo_log: None },
        &ctx(1100, "0.2.0"),
    );
    assert_eq!(e.ultima_atualizacao.unwrap().codigo, 0);
}

#[test]
fn tres_falhas_abandonam_e_maior_volta_a_tentar() {
    let c = ctx(1000, "0.1.0");
    let mut e = EstadoPersistido::default();
    for n in 1..=3u8 {
        let mut i = instalando("0.2.0", &c);
        i.falhas = e.falhas.clone();
        i.abandonadas = e.abandonadas.clone();
        let (r, _) = passo(
            &i,
            Evento::Iniciado {
                codigo_log: Some(1603),
            },
            &c,
        );
        e = r;
        assert_eq!(e.ultima_falha.as_ref().unwrap().codigo, 1603);
        if n < 3 {
            assert_eq!(e.falhas[&v("0.2.0")], n);
        }
    }
    assert_eq!(e.abandonadas, [v("0.2.0")]);
    assert!(e.falhas.is_empty());
    let (e2, _) = passo(&e, Evento::PrazoVerificacao, &c);
    let (e2, ef) = passo(&e2, Evento::ManifestoValido(manifesto("0.2.0")), &c);
    assert_eq!(e2.fase, Fase::Ocioso);
    assert!(!ef.iter().any(|x| matches!(x, Efeito::Baixar(_))));
    // U-19: maior que a abandonada volta a ser tentada.
    let (e3, _) = passo(&e, Evento::PrazoVerificacao, &c);
    let (e3, _) = passo(&e3, Evento::ManifestoValido(manifesto("0.2.1")), &c);
    assert!(matches!(e3.fase, Fase::Baixando { .. }));
}

#[test]
fn msi_ocupado_1618_nao_conta_e_1625_abandona() {
    let c = ctx(1000, "0.1.0");
    let (e, ef) = passo(
        &instalando("0.2.0", &c),
        Evento::MsiexecTerminou { codigo: 1618 },
        &c,
    );
    assert!(e.falhas.is_empty());
    assert_eq!(e.ultimo_resultado, Some(UltimoResultado::AdiadaMsiOcupado));
    assert!(ef.contains(&Efeito::AgendarVerificacao(REAVALIAR_MSI_OCUPADO)));
    let (e, _) = passo(
        &instalando("0.2.0", &c),
        Evento::MsiexecTerminou { codigo: 1625 },
        &c,
    );
    assert_eq!(e.abandonadas, [v("0.2.0")]);
}

#[test]
fn instalando_sem_codigo_espera_ate_2h_e_depois_conta_menos_um() {
    let c = ctx(1000, "0.1.0");
    let e = instalando("0.2.0", &c);
    let (e1, ef) = passo(
        &e,
        Evento::Iniciado { codigo_log: None },
        &ctx(1000 + 3600, "0.1.0"),
    );
    assert!(matches!(e1.fase, Fase::Instalando { .. }));
    assert!(ef.contains(&Efeito::Reavaliar(RELER_LOG)));
    // Enquanto instalando, nenhuma verificação nova.
    let (e1b, ef) = passo(&e1, Evento::PrazoVerificacao, &c);
    assert!(ef.is_empty() && e1b == e1);
    let (e2, _) = passo(
        &e1,
        Evento::LogRelido { codigo_log: None },
        &ctx(1000 + 7201, "0.1.0"),
    );
    assert_eq!(e2.fase, Fase::Ocioso);
    assert_eq!(e2.ultima_falha.unwrap().codigo, -1);
}

#[test]
fn lancamento_falhou_conta_falha() {
    let c = ctx(1000, "0.1.0");
    let (e, ef) = passo(
        &instalando("0.2.0", &c),
        Evento::LancamentoFalhou { codigo_win32: 2 },
        &c,
    );
    assert_eq!(e.falhas[&v("0.2.0")], 1);
    assert_eq!(ef[0], Efeito::Persistir);
}

#[test]
fn downgrade_e_replay_recusados() {
    let c = ctx(1000, "0.2.0");
    let e = EstadoPersistido {
        maior_vista: Some(v("0.5.0")),
        ..Default::default()
    };
    for ver in ["0.1.0", "0.2.0", "0.4.9"] {
        let (x, _) = passo(&e, Evento::PrazoVerificacao, &c);
        let (x, ef) = passo(&x, Evento::ManifestoValido(manifesto(ver)), &c);
        assert_eq!(x.fase, Fase::Ocioso, "{ver}");
        assert_eq!(x.ultimo_resultado, Some(UltimoResultado::SemNovidade));
        assert_eq!(x.maior_vista, Some(v("0.5.0")));
        assert!(!ef.iter().any(|f| matches!(f, Efeito::Baixar(_))));
    }
    // Igual à maior vista: retomada permitida.
    let (x, _) = passo(&e, Evento::PrazoVerificacao, &c);
    let (x, _) = passo(&x, Evento::ManifestoValido(manifesto("0.5.0")), &c);
    assert!(matches!(x.fase, Fase::Baixando { .. }));
}

#[test]
fn falha_de_rede_exponencial_e_limite_http() {
    let c = ctx(1000, "0.1.0");
    let mut e = EstadoPersistido::default();
    let mut esperas = vec![];
    for _ in 0..6 {
        let (x, _) = passo(&e, Evento::PrazoVerificacao, &c);
        let (x, ef) = passo(
            &x,
            Evento::FalhaRede {
                limite: false,
                retry_after_s: None,
                reset_s: None,
            },
            &c,
        );
        assert_eq!(x.ultimo_resultado, Some(UltimoResultado::ErroRede));
        esperas.push(match ef.last() {
            Some(Efeito::AgendarVerificacao(d)) => d.as_secs() / 60,
            o => panic!("{o:?}"),
        });
        e = x;
    }
    assert_eq!(esperas, [30, 60, 120, 240, 360, 360]);
    let (x, _) = passo(&e, Evento::PrazoVerificacao, &c);
    let (x, _) = passo(&x, Evento::ManifestoAusente, &c);
    assert_eq!(x.espera_rede_s, 0);
    let (x, _) = passo(&x, Evento::PrazoVerificacao, &c);
    let (x, ef) = passo(
        &x,
        Evento::FalhaRede {
            limite: true,
            retry_after_s: Some(3 * 3600),
            reset_s: None,
        },
        &c,
    );
    assert_eq!(x.ultimo_resultado, Some(UltimoResultado::ErroLimite));
    assert!(ef.contains(&Efeito::AgendarVerificacao(Duration::from_secs(3 * 3600))));
}

#[test]
fn manifesto_recusado_nao_e_falha_de_rede() {
    let c = ctx(1000, "0.1.0");
    let (x, _) = passo(&EstadoPersistido::default(), Evento::PrazoVerificacao, &c);
    let (x, _) = passo(
        &x,
        Evento::ManifestoRecusado(ErroVerificacao::AssinaturaLegada),
        &c,
    );
    assert_eq!(x.ultimo_resultado, Some(UltimoResultado::ErroAssinatura));
    assert_eq!(x.espera_rede_s, 0);
    assert_eq!(x.fase, Fase::Ocioso);
}

#[test]
fn download_falho_descarta() {
    let c = ctx(1000, "0.1.0");
    let (x, _) = passo(&EstadoPersistido::default(), Evento::PrazoVerificacao, &c);
    let (x, _) = passo(&x, Evento::ManifestoValido(manifesto("0.2.0")), &c);
    let (y, ef) = passo(&x, Evento::FalhaDownload(FalhaDownload::Verificacao), &c);
    assert_eq!(y.fase, Fase::Ocioso);
    assert_eq!(ef[0], Efeito::DescartarMsi);
    assert_eq!(y.maior_vista, Some(v("0.2.0")));
    let (y, _) = passo(&x, Evento::FalhaDownload(FalhaDownload::Disco), &c);
    assert_eq!(y.ultimo_resultado, Some(UltimoResultado::ErroDisco));
    assert_eq!(y.espera_rede_s, 1800);
}

#[test]
fn portao_adia_e_pronta_expira_em_24h() {
    let c = ctx(1000, "0.1.0");
    let e = ate_pronta(&EstadoPersistido::default(), "0.2.0", &c);
    let (e, ef) = passo(&e, Evento::PortaoAdiou(MotivoAdiamento::JogoAberto), &c);
    assert_eq!(ef, [Efeito::Persistir, Efeito::Reavaliar(REAVALIAR_PORTAO)]);
    let (e, ef) = passo(
        &e,
        Evento::PortaoAdiou(MotivoAdiamento::JogoAberto),
        &ctx(1060, "0.1.0"),
    );
    assert_eq!(ef, [Efeito::Reavaliar(REAVALIAR_PORTAO)]);
    let (e2, ef) = passo(&e, Evento::MsiOcupado, &c);
    assert_eq!(e2.ultimo_resultado, Some(UltimoResultado::AdiadaMsiOcupado));
    assert!(ef.contains(&Efeito::Reavaliar(REAVALIAR_MSI_OCUPADO)));
    let tarde = ctx(1000 + 24 * 3600 + 1, "0.1.0");
    let (e3, ef) = passo(
        &e,
        Evento::PortaoLiberou {
            sessoes_agente: vec![],
        },
        &tarde,
    );
    assert_eq!(e3.fase, Fase::Ocioso);
    assert!(ef.contains(&Efeito::DescartarMsi));
    assert!(!ef.iter().any(|f| matches!(f, Efeito::Lancar(_))));
    let (e4, _) = passo(&e, Evento::ArquivoPerdido, &c);
    assert_eq!(e4.fase, Fase::Ocioso);
}

#[test]
fn iniciar_em_fase_transitoria_volta_a_ocioso() {
    let c = ctx(1000, "0.1.0");
    let e = ate_pronta(&EstadoPersistido::default(), "0.2.0", &c);
    let (e, ef) = passo(&e, Evento::Iniciado { codigo_log: None }, &c);
    assert_eq!(e.fase, Fase::Ocioso);
    assert_eq!(ef[1], Efeito::AgendarVerificacao(Duration::from_secs(300)));
    assert_eq!(e.proxima_verificacao_unix, Some(1300));
}

#[test]
fn persistencia_ida_e_volta_e_recuperacao() {
    let c = ctx(1000, "0.1.0");
    let mut e = instalando("0.2.0", &c);
    e.falhas.insert(v("0.2.0"), 1);
    e.abandonadas.push(v("0.0.9"));
    let b = e.serializar();
    let texto = String::from_utf8(b.clone()).unwrap();
    assert!(texto.contains("\"tipo\": \"instalando\""));
    assert!(texto.contains("\"0.2.0\": 1"));
    assert_eq!(
        EstadoPersistido::carregar(Some(&b), Some(&b), v("0.1.0")),
        e
    );

    // Principal corrompido: vazio, mas maior_vista do .bak.
    let r = EstadoPersistido::carregar(Some(b"{lixo"), Some(&b), v("0.1.0"));
    assert_eq!(r.maior_vista, Some(v("0.2.0")));
    assert_eq!(r.fase, Fase::Ocioso);
    // .bak parcialmente ilegível ainda entrega maior_vista.
    let r = EstadoPersistido::carregar(None, Some(br#"{"maior_vista":"0.3.0","x":1}"#), v("0.1.0"));
    assert_eq!(r.maior_vista, Some(v("0.3.0")));
    // Ambos perdidos: cai para a instalada, nunca abaixo.
    let r = EstadoPersistido::carregar(None, None, v("0.1.0"));
    assert_eq!(r.maior_vista, Some(v("0.1.0")));
    // Nunca diminui: maior entre principal e bak.
    let mut menor = e.clone();
    menor.maior_vista = Some(v("0.1.5"));
    let r = EstadoPersistido::carregar(Some(&menor.serializar()), Some(&b), v("0.1.0"));
    assert_eq!(r.maior_vista, Some(v("0.2.0")));
    // Formato diferente: ilegível.
    let r = EstadoPersistido::carregar(Some(br#"{"formato":2}"#), None, v("0.1.0"));
    assert_eq!(
        r,
        EstadoPersistido {
            maior_vista: Some(v("0.1.0")),
            ..Default::default()
        }
    );
}
