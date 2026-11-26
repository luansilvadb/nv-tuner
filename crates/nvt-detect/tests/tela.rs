use nvt_detect::tela::{
    CaminhoAtivo, FonteTela, FormatoTela, ModoTela, RefreshRacional, TelaDoSistema,
    TelaIndisponivel, ler_tela_primaria,
};
use std::num::NonZeroU32;

const FORMATO: FormatoTela = FormatoTela {
    largura: 1920,
    altura: 1080,
    bits_por_pixel: 32,
};

struct TelaFalsa {
    caminhos: Result<Vec<CaminhoAtivo>, TelaIndisponivel>,
    modos: Vec<ModoTela>,
}

impl FonteTela for TelaFalsa {
    fn caminhos_ativos(&self) -> Result<Vec<CaminhoAtivo>, TelaIndisponivel> {
        self.caminhos.clone()
    }

    fn modos(
        &self,
        nome_gdi_origem: &str,
    ) -> Result<(FormatoTela, Vec<ModoTela>), TelaIndisponivel> {
        assert_eq!(nome_gdi_origem, r"\\.\DISPLAY2");
        Ok((FORMATO, self.modos.clone()))
    }
}

fn caminho_primario(numerador: u32, denominador: u32) -> CaminhoAtivo {
    CaminhoAtivo {
        posicao_origem: (0, 0),
        nome_gdi_origem: r"\\.\DISPLAY2".into(),
        caminho_monitor: Some(r"\\?\DISPLAY#PRIMARIO".into()),
        refresh_atual: RefreshRacional {
            numerador,
            denominador: NonZeroU32::new(denominador).unwrap(),
        },
    }
}

#[test]
fn identifica_primario_e_preserva_taxa_racional() {
    let primario = caminho_primario(60_000, 1_001);
    let secundario = CaminhoAtivo {
        posicao_origem: (-1920, 0),
        nome_gdi_origem: r"\\.\DISPLAY1".into(),
        caminho_monitor: Some(r"\\?\DISPLAY#SECUNDARIO".into()),
        ..caminho_primario(240, 1)
    };
    let fonte = TelaFalsa {
        caminhos: Ok(vec![secundario, primario.clone()]),
        modos: vec![
            ModoTela {
                formato: FORMATO,
                refresh_hz: 60,
            },
            ModoTela {
                formato: FORMATO,
                refresh_hz: 144,
            },
            ModoTela {
                formato: FormatoTela {
                    largura: 1280,
                    ..FORMATO
                },
                refresh_hz: 240,
            },
            ModoTela {
                formato: FormatoTela {
                    bits_por_pixel: 16,
                    ..FORMATO
                },
                refresh_hz: 360,
            },
        ],
    };
    let leitura = ler_tela_primaria(&fonte).unwrap();
    assert_eq!(leitura.caminho_monitor, primario.caminho_monitor.unwrap());
    assert_eq!(leitura.nome_gdi_origem, primario.nome_gdi_origem);
    assert_eq!(leitura.refresh_atual, primario.refresh_atual);
    assert_eq!(leitura.formato, FORMATO);
    assert_eq!(leitura.refresh_max_hz, 144);
    assert!(leitura.refresh_abaixo_do_maximo);
}

#[test]
fn compara_fracao_sem_arredondar() {
    for (numerador, esperado) in [(119, true), (120, false), (121, false)] {
        let fonte = TelaFalsa {
            caminhos: Ok(vec![caminho_primario(numerador, 2)]),
            modos: vec![ModoTela {
                formato: FORMATO,
                refresh_hz: 60,
            }],
        };
        assert_eq!(
            ler_tela_primaria(&fonte).unwrap().refresh_abaixo_do_maximo,
            esperado
        );
    }
}

#[test]
fn alvo_removido() {
    let fonte = TelaFalsa {
        caminhos: Ok(vec![CaminhoAtivo {
            caminho_monitor: None,
            ..caminho_primario(60, 1)
        }]),
        modos: vec![],
    };
    assert_eq!(
        ler_tela_primaria(&fonte),
        Err(TelaIndisponivel::AlvoIndisponivel)
    );
}

#[test]
fn rejeita_primario_clonado() {
    let mut outro_alvo = caminho_primario(144, 1);
    outro_alvo.caminho_monitor = Some(r"\\?\DISPLAY#CLONE".into());
    let fonte = TelaFalsa {
        caminhos: Ok(vec![caminho_primario(60, 1), outro_alvo]),
        modos: vec![],
    };
    assert_eq!(
        ler_tela_primaria(&fonte),
        Err(TelaIndisponivel::PrimarioAmbiguo)
    );
}

#[test]
fn modo_incompativel_ou_taxa_padrao_nao_produz_maximo() {
    let fonte = TelaFalsa {
        caminhos: Ok(vec![caminho_primario(60, 1)]),
        modos: vec![
            ModoTela {
                formato: FORMATO,
                refresh_hz: 1,
            },
            ModoTela {
                formato: FormatoTela {
                    altura: 720,
                    ..FORMATO
                },
                refresh_hz: 144,
            },
            ModoTela {
                formato: FormatoTela {
                    bits_por_pixel: 16,
                    ..FORMATO
                },
                refresh_hz: 240,
            },
        ],
    };
    assert_eq!(
        ler_tela_primaria(&fonte),
        Err(TelaIndisponivel::SemModoCompativel)
    );
}

#[test]
fn sessao_sem_tela() {
    let fonte = TelaFalsa {
        caminhos: Err(TelaIndisponivel::SessaoSemTela),
        modos: vec![],
    };
    assert_eq!(
        ler_tela_primaria(&fonte),
        Err(TelaIndisponivel::SessaoSemTela)
    );
}

#[test]
#[ignore = "requer a sessão interativa com tela"]
fn leitura_real() {
    let leitura = ler_tela_primaria(&TelaDoSistema).expect("tela primária indisponível");
    println!("{leitura:#?}");
}
