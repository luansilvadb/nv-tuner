//! Envelope versionado e enquadramento por tamanho.
//!
//! Cada mensagem trafega como `u32` little-endian com o tamanho do payload,
//! seguido do payload: um objeto JSON `{"versao": N, "corpo": {...}}`.
//! A versão é conferida **antes** de interpretar o corpo, de modo que um
//! par com versão diferente é identificado mesmo que o corpo seja
//! incompreensível para nós.

use std::io::{self, Read, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::{ErroIpc, PROTOCOL_VERSION};

/// Tamanho máximo de payload aceito ou enviado (1 MiB).
pub const TAMANHO_MAXIMO: u32 = 1024 * 1024;

#[derive(Serialize)]
struct EnvelopeSaida<'a, T> {
    versao: u32,
    corpo: &'a T,
}

/// Serializa `msg` dentro do envelope da versão atual.
pub fn codificar<T: Serialize>(msg: &T) -> Result<Vec<u8>, ErroIpc> {
    let bytes = serde_json::to_vec(&EnvelopeSaida {
        versao: PROTOCOL_VERSION,
        corpo: msg,
    })?;
    Ok(bytes)
}

/// Interpreta um payload: confere a versão e só então desserializa o corpo.
pub fn decodificar<T: DeserializeOwned>(payload: &[u8]) -> Result<T, ErroIpc> {
    #[derive(Deserialize)]
    struct Cabecalho<'a> {
        #[serde(borrow)]
        versao: &'a RawValue,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope<'a> {
        #[serde(borrow, rename = "versao")]
        _versao: &'a RawValue,
        #[serde(borrow)]
        corpo: &'a RawValue,
    }

    fn erro_envelope(erro: serde_json::Error) -> ErroIpc {
        if erro.is_data() {
            ErroIpc::EnvelopeInvalido(erro.to_string())
        } else {
            erro.into()
        }
    }

    // Mapas estruturados do Serde também aceitam arrays posicionais.
    if payload.iter().copied().find(|b| !b.is_ascii_whitespace()) == Some(b'[') {
        return Err(ErroIpc::EnvelopeInvalido(
            "o payload não é um objeto".into(),
        ));
    }
    let cabecalho: Cabecalho<'_> = serde_json::from_slice(payload).map_err(erro_envelope)?;
    let versao: u64 = serde_json::from_str(cabecalho.versao.get()).map_err(erro_envelope)?;
    if versao != u64::from(PROTOCOL_VERSION) {
        return Err(ErroIpc::VersaoIncompativel {
            recebida: versao,
            esperada: PROTOCOL_VERSION,
        });
    }
    let envelope: Envelope<'_> = serde_json::from_slice(payload).map_err(erro_envelope)?;
    Ok(serde_json::from_str(envelope.corpo.get())?)
}

/// Escreve um quadro (cabeçalho de tamanho + payload).
pub fn escrever_quadro<W: Write>(w: &mut W, payload: &[u8]) -> Result<(), ErroIpc> {
    let tamanho = u32::try_from(payload.len())
        .ok()
        .filter(|t| *t <= TAMANHO_MAXIMO)
        .ok_or(ErroIpc::Grande {
            tamanho: payload.len() as u64,
            limite: TAMANHO_MAXIMO,
        })?;
    w.write_all(&tamanho.to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()?;
    Ok(())
}

/// Lê um quadro completo.
///
/// Retorna [`ErroIpc::Fechado`] se a conexão terminar antes do primeiro byte,
/// [`ErroIpc::Truncado`] se terminar no meio, e [`ErroIpc::Grande`] se o
/// cabeçalho declarar mais que [`TAMANHO_MAXIMO`] (nada é alocado nesse caso).
pub fn ler_quadro<R: Read>(r: &mut R) -> Result<Vec<u8>, ErroIpc> {
    let mut cabecalho = [0u8; 4];
    let mut lidos = 0;
    while lidos < cabecalho.len() {
        match r.read(&mut cabecalho[lidos..]) {
            Ok(0) if lidos == 0 => return Err(ErroIpc::Fechado),
            Ok(0) => return Err(ErroIpc::Truncado),
            Ok(n) => lidos += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    let tamanho = u32::from_le_bytes(cabecalho);
    if tamanho > TAMANHO_MAXIMO {
        return Err(ErroIpc::Grande {
            tamanho: u64::from(tamanho),
            limite: TAMANHO_MAXIMO,
        });
    }
    let mut payload = vec![0u8; tamanho as usize];
    r.read_exact(&mut payload).map_err(|e| {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            ErroIpc::Truncado
        } else {
            e.into()
        }
    })?;
    Ok(payload)
}

/// Codifica e envia uma mensagem em um quadro.
pub fn enviar<W: Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), ErroIpc> {
    escrever_quadro(w, &codificar(msg)?)
}

/// Recebe um quadro e decodifica a mensagem.
pub fn receber<R: Read, T: DeserializeOwned>(r: &mut R) -> Result<T, ErroIpc> {
    decodificar(&ler_quadro(r)?)
}

#[cfg(test)]
mod testes {
    use std::io::Cursor;

    use super::*;
    use crate::{Requisicao, Resposta};

    fn quadro_de(payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        escrever_quadro(&mut v, payload).unwrap();
        v
    }

    #[test]
    fn ida_e_volta_pelo_quadro() {
        let mut buf = Vec::new();
        enviar(&mut buf, &Requisicao::ObterEstado).unwrap();
        enviar(&mut buf, &Requisicao::ListarHistorico { limite: 5 }).unwrap();
        let mut c = Cursor::new(buf);
        assert_eq!(
            receber::<_, Requisicao>(&mut c).unwrap(),
            Requisicao::ObterEstado
        );
        assert_eq!(
            receber::<_, Requisicao>(&mut c).unwrap(),
            Requisicao::ListarHistorico { limite: 5 }
        );
        assert!(matches!(
            receber::<_, Requisicao>(&mut c),
            Err(ErroIpc::Fechado)
        ));
    }

    #[test]
    fn cabecalho_e_little_endian() {
        let q = quadro_de(b"abc");
        assert_eq!(&q[..4], &[3, 0, 0, 0]);
        assert_eq!(&q[4..], b"abc");
    }

    #[test]
    fn cabecalho_truncado() {
        let mut c = Cursor::new(vec![5u8, 0]);
        assert!(matches!(ler_quadro(&mut c), Err(ErroIpc::Truncado)));
    }

    #[test]
    fn payload_truncado() {
        let mut q = quadro_de(&codificar(&Requisicao::ObterPlano).unwrap());
        q.truncate(q.len() - 3);
        assert!(matches!(
            ler_quadro(&mut Cursor::new(q)),
            Err(ErroIpc::Truncado)
        ));
    }

    #[test]
    fn tamanho_acima_do_limite_na_leitura() {
        let mut q = (TAMANHO_MAXIMO + 1).to_le_bytes().to_vec();
        q.extend_from_slice(b"{}");
        match ler_quadro(&mut Cursor::new(q)) {
            Err(ErroIpc::Grande { tamanho, limite }) => {
                assert_eq!(tamanho, u64::from(TAMANHO_MAXIMO) + 1);
                assert_eq!(limite, TAMANHO_MAXIMO);
            }
            outro => panic!("esperado Grande, veio {outro:?}"),
        }
        let q = u32::MAX.to_le_bytes().to_vec();
        assert!(matches!(
            ler_quadro(&mut Cursor::new(q)),
            Err(ErroIpc::Grande { .. })
        ));
    }

    #[test]
    fn tamanho_no_limite_e_aceito() {
        let payload = vec![b' '; TAMANHO_MAXIMO as usize];
        let q = quadro_de(&payload);
        assert_eq!(
            ler_quadro(&mut Cursor::new(q)).unwrap().len(),
            payload.len()
        );
    }

    #[test]
    fn tamanho_acima_do_limite_na_escrita() {
        let payload = vec![0u8; TAMANHO_MAXIMO as usize + 1];
        let mut buf = Vec::new();
        assert!(matches!(
            escrever_quadro(&mut buf, &payload),
            Err(ErroIpc::Grande { .. })
        ));
        assert!(buf.is_empty(), "nada deve ser escrito");
    }

    #[test]
    fn versao_errada_e_recusada() {
        let payload = br#"{"versao":99,"corpo":{"tipo":"obter_estado"}}"#;
        match decodificar::<Requisicao>(payload) {
            Err(ErroIpc::VersaoIncompativel { recebida, esperada }) => {
                assert_eq!(recebida, 99);
                assert_eq!(esperada, PROTOCOL_VERSION);
            }
            outro => panic!("esperado VersaoIncompativel, veio {outro:?}"),
        }
    }

    #[test]
    fn versao_conferida_antes_do_corpo() {
        // Corpo desconhecido para nós, mas a versão é o que importa reportar.
        let payload = br#"{"versao":4,"corpo":{"tipo":"algo_do_futuro","x":1}}"#;
        assert!(matches!(
            decodificar::<Resposta>(payload),
            Err(ErroIpc::VersaoIncompativel { recebida: 4, .. })
        ));
    }

    #[test]
    fn envelope_malformado() {
        for p in [
            &br#"{"corpo":{"tipo":"obter_estado"}}"#[..],
            br#"{"versao":"1","corpo":{"tipo":"obter_estado"}}"#,
            br#"{"versao":8}"#,
            br#"{"versao":8,"corpo":{"tipo":"obter_estado"},"extra":0}"#,
            br#"[1,2]"#,
        ] {
            assert!(
                matches!(
                    decodificar::<Requisicao>(p),
                    Err(ErroIpc::EnvelopeInvalido(_))
                ),
                "{}",
                String::from_utf8_lossy(p)
            );
        }
        assert!(matches!(
            decodificar::<Requisicao>(b"nao e json"),
            Err(ErroIpc::Json(_))
        ));
        assert!(matches!(
            decodificar::<Requisicao>(br#"{"versao":8,"corpo":{"tipo":"formatar_disco"}}"#),
            Err(ErroIpc::Json(_))
        ));
    }
}

#[cfg(test)]
mod testes_v8 {
    use super::*;
    use crate::{PROTOCOL_VERSION, Requisicao};

    #[test]
    fn v7_e_recusada() {
        assert_eq!(PROTOCOL_VERSION, 8);
        let payload = br#"{"versao":7,"corpo":{"tipo":"obter_estado"}}"#;
        assert!(matches!(
            decodificar::<Requisicao>(payload),
            Err(ErroIpc::VersaoIncompativel {
                recebida: 7,
                esperada: 8
            })
        ));
    }

    #[test]
    fn relato_sem_saude_e_aceito() {
        let payload = br#"{"versao":8,"corpo":{"tipo":"agente_relatar","id_tarefa":1,"operacao":"aplicar","revisao":1,"resultados":[]}}"#;
        match decodificar::<Requisicao>(payload) {
            Ok(Requisicao::AgenteRelatar { saude: None, .. }) => {}
            outro => panic!("{outro:?}"),
        }
    }

    #[test]
    fn requisicoes_novas_no_envelope() {
        for corpo in [
            r#"{"tipo":"aplicar_recomendadas"}"#,
            r#"{"tipo":"obter_saude"}"#,
            r#"{"tipo":"marcar_inicio_visto","fonte":"run_usuario","nome":"Discord"}"#,
            r#"{"tipo":"listar_partidas","desde_unix":0,"limite":10}"#,
            r#"{"tipo":"obter_partida","id":3}"#,
            r#"{"tipo":"apagar_partidas"}"#,
        ] {
            let p = format!(r#"{{"versao":8,"corpo":{corpo}}}"#);
            assert!(decodificar::<Requisicao>(p.as_bytes()).is_ok(), "{corpo}");
        }
        for corpo in [
            r#"{"tipo":"marcar_inicio_visto","fonte":"run_usuario","nome":"x","caminho":"c"}"#,
            r#"{"tipo":"marcar_inicio_visto","fonte":"servico","nome":"x"}"#,
        ] {
            let p = format!(r#"{{"versao":8,"corpo":{corpo}}}"#);
            assert!(decodificar::<Requisicao>(p.as_bytes()).is_err(), "{corpo}");
        }
    }
}
