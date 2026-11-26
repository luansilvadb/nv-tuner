//! Cliente WinHTTP da atualização (spec §9.2, U-04..U-08).
//!
//! A lógica (redirecionamento manual, status, limites, prazo total, cancelamento) é
//! genérica sobre [`Transporte`] e testada sem rede com [`FonteFalsa`]; só
//! [`WinHttp`] toca a API do Windows.
#![allow(dead_code)]

use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use nvt_atualizacao::politica::resolver_redirecionamento;
use nvt_atualizacao::{ErroUrl, MAX_REDIRECIONAMENTOS, UrlPermitida};

/// Prazo total de um pedido pequeno (manifesto, assinatura) — U-05.
pub(crate) const PRAZO_PEQUENO: Duration = Duration::from_secs(60);
/// Prazo total do download do MSI — U-05.
pub(crate) const PRAZO_MSI: Duration = Duration::from_secs(10 * 60);
/// Timeouts por operação (ms): resolução, conexão, envio, recebimento — U-05.
pub(crate) const TIMEOUTS_MS: [i32; 4] = [15_000, 15_000, 30_000, 30_000];
/// Único cabeçalho adicional enviado (o User-Agent vai na sessão) — U-08.
pub(crate) const CABECALHOS_ADICIONAIS: &str = "Accept: */*\r\n";
const BLOCO: usize = 64 * 1024;

/// `nv-tuner-atualizador/<versão>`, sem identificadores — U-08.
pub(crate) fn user_agent() -> String {
    format!("nv-tuner-atualizador/{}", env!("CARGO_PKG_VERSION"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ErroRede {
    /// URL inicial ou `Location` fora da política (U-04).
    Url(ErroUrl),
    RedirecionamentosDemais,
    RedirecionamentoSemLocation,
    /// 404: para o manifesto, “nenhuma release com atualizador” (U-07).
    NaoEncontrado,
    /// 403/429: valores crus dos cabeçalhos para `politica::espera_limite` (U-02).
    Limitado {
        status: u16,
        retry_after: Option<String>,
        ratelimit_reset: Option<String>,
    },
    /// Outro status ≠ 200 (5xx etc.).
    Status(u16),
    /// `Content-Length` ou corpo acima do limite (U-06).
    TamanhoExcedido {
        limite: u64,
    },
    /// Corpo terminou antes do `Content-Length`.
    Truncado {
        esperado: u64,
        recebido: u64,
    },
    TempoEsgotado,
    Cancelado,
    /// Falha de API WinHTTP (`GetLastError`).
    Windows {
        funcao: &'static str,
        codigo: u32,
    },
    Leitura(io::ErrorKind),
    Escrita(io::ErrorKind),
}

impl std::fmt::Display for ErroRede {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ErroRede {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Resposta {
    pub corpo: Vec<u8>,
    /// URL que respondeu 200, após os redirecionamentos.
    pub url_final: UrlPermitida,
}

/// Fachada usada pelo atualizador.
pub(crate) trait Rede {
    /// GET com corpo em memória, até `limite` bytes, prazo [`PRAZO_PEQUENO`].
    fn obter(&self, url: &UrlPermitida, limite: u64) -> Result<Resposta, ErroRede>;
    /// GET gravando em `destino`, até `limite` bytes, prazo [`PRAZO_MSI`]. Devolve os bytes escritos.
    fn baixar(
        &self,
        url: &UrlPermitida,
        limite: u64,
        destino: &mut dyn Write,
    ) -> Result<u64, ErroRede>;
}

/// Resposta de um único salto, sem seguir redirecionamento.
pub(crate) struct RespostaBruta<C> {
    pub status: u16,
    pub location: Option<String>,
    pub content_length: Option<u64>,
    pub retry_after: Option<String>,
    pub ratelimit_reset: Option<String>,
    pub corpo: C,
}

/// Um pedido GET de um salto.
pub(crate) trait Transporte {
    type Corpo: Read;
    fn enviar(&self, url: &UrlPermitida) -> Result<RespostaBruta<Self::Corpo>, ErroRede>;
}

/// Cliente genérico: aplica U-04..U-07 sobre qualquer transporte.
pub(crate) struct Cliente<T> {
    transporte: T,
    cancelar: Arc<AtomicBool>,
}

impl<T: Transporte> Cliente<T> {
    pub(crate) fn com_transporte(transporte: T, cancelar: Arc<AtomicBool>) -> Self {
        Cliente {
            transporte,
            cancelar,
        }
    }

    fn executar(
        &self,
        url: &UrlPermitida,
        limite: u64,
        prazo: Duration,
        destino: &mut dyn Write,
    ) -> Result<(u64, UrlPermitida), ErroRede> {
        let fim = Instant::now() + prazo;
        let mut atual = url.clone();
        let mut saltos = 0u8;
        let resposta = loop {
            self.conferir(fim)?;
            let r = self.transporte.enviar(&atual)?;
            match r.status {
                200 => break r,
                301 | 302 | 303 | 307 | 308 => {
                    if saltos >= MAX_REDIRECIONAMENTOS {
                        return Err(ErroRede::RedirecionamentosDemais);
                    }
                    saltos += 1;
                    let loc = r.location.ok_or(ErroRede::RedirecionamentoSemLocation)?;
                    atual = resolver_redirecionamento(&atual, &loc).map_err(ErroRede::Url)?;
                }
                404 => return Err(ErroRede::NaoEncontrado),
                403 | 429 => {
                    return Err(ErroRede::Limitado {
                        status: r.status,
                        retry_after: r.retry_after,
                        ratelimit_reset: r.ratelimit_reset,
                    });
                }
                s => return Err(ErroRede::Status(s)),
            }
        };
        if resposta.content_length.is_some_and(|n| n > limite) {
            return Err(ErroRede::TamanhoExcedido { limite });
        }
        let mut corpo = resposta.corpo;
        let mut buf = vec![0u8; BLOCO];
        let mut total = 0u64;
        loop {
            self.conferir(fim)?;
            // Lê no máximo 1 byte além do limite: basta para detectar o excesso.
            let cabe = (limite - total + 1).min(BLOCO as u64) as usize;
            let n = match corpo.read(&mut buf[..cabe]) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(erro_leitura(&e)),
            };
            total += n as u64;
            if total > limite {
                return Err(ErroRede::TamanhoExcedido { limite });
            }
            destino
                .write_all(&buf[..n])
                .map_err(|e| ErroRede::Escrita(e.kind()))?;
        }
        if let Some(esperado) = resposta.content_length
            && esperado != total
        {
            return Err(ErroRede::Truncado {
                esperado,
                recebido: total,
            });
        }
        Ok((total, atual))
    }

    fn conferir(&self, fim: Instant) -> Result<(), ErroRede> {
        if self.cancelar.load(Ordering::Relaxed) {
            Err(ErroRede::Cancelado)
        } else if Instant::now() >= fim {
            Err(ErroRede::TempoEsgotado)
        } else {
            Ok(())
        }
    }
}

impl<T: Transporte> Rede for Cliente<T> {
    fn obter(&self, url: &UrlPermitida, limite: u64) -> Result<Resposta, ErroRede> {
        let mut corpo = Vec::new();
        let (_, url_final) = self.executar(url, limite, PRAZO_PEQUENO, &mut corpo)?;
        Ok(Resposta { corpo, url_final })
    }

    fn baixar(
        &self,
        url: &UrlPermitida,
        limite: u64,
        destino: &mut dyn Write,
    ) -> Result<u64, ErroRede> {
        self.executar(url, limite, PRAZO_MSI, destino)
            .map(|(n, _)| n)
    }
}

const TIMEOUT_WINHTTP: i32 = 12002;

fn erro_leitura(e: &io::Error) -> ErroRede {
    match e.raw_os_error() {
        Some(TIMEOUT_WINHTTP) => ErroRede::TempoEsgotado,
        Some(c) => ErroRede::Windows {
            funcao: "WinHttpReadData",
            codigo: c as u32,
        },
        None => ErroRede::Leitura(e.kind()),
    }
}

// ---------------------------------------------------------------- WinHTTP

#[cfg(windows)]
pub(crate) mod winhttp {
    use super::*;
    use core::ffi::c_void;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, GetLastError};
    use windows_sys::Win32::Networking::WinHttp::*;

    /// Cliente real usado pelo serviço.
    pub(crate) type RedeWinHttp = Cliente<WinHttp>;

    impl Cliente<WinHttp> {
        /// Sessão WinHTTP; `cancelar` é ligado no stop do serviço.
        pub(crate) fn nova(cancelar: Arc<AtomicBool>) -> Result<Self, ErroRede> {
            Ok(Cliente::com_transporte(WinHttp::abrir()?, cancelar))
        }
    }

    fn largo(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn falha(funcao: &'static str) -> ErroRede {
        // SAFETY: sem pré-condições; lê o último erro da thread.
        let codigo = unsafe { GetLastError() };
        if codigo == TIMEOUT_WINHTTP as u32 {
            ErroRede::TempoEsgotado
        } else {
            ErroRede::Windows { funcao, codigo }
        }
    }

    /// Guarda de `HINTERNET`: fecha no drop.
    struct Handle(*mut c_void);

    // SAFETY: handles WinHTTP síncronos não têm afinidade de thread; o uso é exclusivo
    // (um pedido por vez) e o fechamento ocorre uma única vez no drop.
    unsafe impl Send for Handle {}
    unsafe impl Sync for Handle {}

    impl Handle {
        fn novo(h: *mut c_void, funcao: &'static str) -> Result<Self, ErroRede> {
            if h.is_null() {
                Err(falha(funcao))
            } else {
                Ok(Handle(h))
            }
        }

        fn opcao_u32(&self, opcao: u32, valor: u32) -> Result<(), ErroRede> {
            // SAFETY: handle válido; o buffer é um u32 vivo durante a chamada.
            let ok = unsafe {
                WinHttpSetOption(
                    self.0,
                    opcao,
                    (&raw const valor).cast(),
                    size_of::<u32>() as u32,
                )
            };
            if ok == 0 {
                Err(falha("WinHttpSetOption"))
            } else {
                Ok(())
            }
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: handle não nulo, aberto por WinHTTP e fechado só aqui.
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }

    /// Sessão WinHTTP (proxy automático, TLS seguro, sem redirecionamento automático).
    pub(crate) struct WinHttp {
        sessao: Handle,
    }

    impl WinHttp {
        pub(crate) fn abrir() -> Result<Self, ErroRede> {
            let ua = largo(&user_agent());
            let abrir = |flags| {
                // SAFETY: `ua` é UTF-16 terminado em nulo e vive durante a chamada;
                // proxy/bypass nulos são o esperado em AUTOMATIC_PROXY.
                unsafe {
                    WinHttpOpen(
                        ua.as_ptr(),
                        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                        null(),
                        null(),
                        flags,
                    )
                }
            };
            // SECURE_DEFAULTS só existe em builds recentes; sem ele, força TLS 1.2+.
            let (h, seguro) = match abrir(WINHTTP_FLAG_SECURE_DEFAULTS) {
                h if !h.is_null() => (h, true),
                _ => (abrir(0), false),
            };
            let sessao = Handle::novo(h, "WinHttpOpen")?;
            if !seguro {
                sessao
                    .opcao_u32(
                        WINHTTP_OPTION_SECURE_PROTOCOLS,
                        WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2 | WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3,
                    )
                    .or_else(|_| {
                        sessao.opcao_u32(
                            WINHTTP_OPTION_SECURE_PROTOCOLS,
                            WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2,
                        )
                    })?;
            }
            let [r, c, e, rc] = TIMEOUTS_MS;
            // SAFETY: handle de sessão válido.
            if unsafe { WinHttpSetTimeouts(sessao.0, r, c, e, rc) } == 0 {
                return Err(falha("WinHttpSetTimeouts"));
            }
            sessao.opcao_u32(
                WINHTTP_OPTION_REDIRECT_POLICY,
                WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
            )?;
            Ok(WinHttp { sessao })
        }
    }

    /// Corpo da resposta; segura pedido e conexão (nessa ordem de drop).
    pub(crate) struct CorpoWinHttp {
        pedido: Handle,
        _conexao: Handle,
    }

    impl Read for CorpoWinHttp {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let mut lidos = 0u32;
            let n = buf.len().min(u32::MAX as usize) as u32;
            // SAFETY: pedido válido; `buf` tem `n` bytes graváveis; `lidos` vivo.
            let ok =
                unsafe { WinHttpReadData(self.pedido.0, buf.as_mut_ptr().cast(), n, &mut lidos) };
            if ok == 0 {
                // SAFETY: sem pré-condições.
                return Err(io::Error::from_raw_os_error(
                    unsafe { GetLastError() } as i32
                ));
            }
            Ok(lidos as usize)
        }
    }

    /// Cabeçalho como texto; `None` se ausente.
    fn cabecalho(
        pedido: &Handle,
        nivel: u32,
        nome: Option<&str>,
    ) -> Result<Option<String>, ErroRede> {
        let nome_w = nome.map(largo);
        let pnome = nome_w.as_ref().map_or(null(), |n| n.as_ptr());
        let mut tamanho = 0u32;
        // SAFETY: consulta de tamanho com buffer nulo; `tamanho` vivo.
        let ok = unsafe {
            WinHttpQueryHeaders(pedido.0, nivel, pnome, null_mut(), &mut tamanho, null_mut())
        };
        if ok == 0 {
            // SAFETY: sem pré-condições.
            match unsafe { GetLastError() } {
                ERROR_WINHTTP_HEADER_NOT_FOUND => return Ok(None),
                ERROR_INSUFFICIENT_BUFFER => {}
                _ => return Err(falha("WinHttpQueryHeaders")),
            }
        }
        let mut buf = vec![0u16; (tamanho as usize).div_ceil(2) + 1];
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: `buf` tem `bytes` bytes graváveis; ponteiros vivos durante a chamada.
        let ok = unsafe {
            WinHttpQueryHeaders(
                pedido.0,
                nivel,
                pnome,
                buf.as_mut_ptr().cast(),
                &mut bytes,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(falha("WinHttpQueryHeaders"));
        }
        buf.truncate(bytes as usize / 2);
        Ok(Some(String::from_utf16_lossy(&buf)))
    }

    impl Transporte for WinHttp {
        type Corpo = CorpoWinHttp;

        fn enviar(&self, url: &UrlPermitida) -> Result<RespostaBruta<CorpoWinHttp>, ErroRede> {
            let host = largo(&url.host);
            let caminho = largo(&url.caminho);
            let verbo = largo("GET");
            let extras = largo(CABECALHOS_ADICIONAIS);
            // SAFETY: sessão válida; `host` terminado em nulo; porta 443 fixa.
            let conexao = Handle::novo(
                unsafe { WinHttpConnect(self.sessao.0, host.as_ptr(), 443, 0) },
                "WinHttpConnect",
            )?;
            // SAFETY: conexão válida; strings terminadas em nulo; versão/referrer/accept
            // nulos = HTTP/1.1, sem Referer, sem Accept automático.
            let pedido = Handle::novo(
                unsafe {
                    WinHttpOpenRequest(
                        conexao.0,
                        verbo.as_ptr(),
                        caminho.as_ptr(),
                        null(),
                        null(),
                        null(),
                        WINHTTP_FLAG_SECURE,
                    )
                },
                "WinHttpOpenRequest",
            )?;
            pedido.opcao_u32(
                WINHTTP_OPTION_REDIRECT_POLICY,
                WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
            )?;
            pedido.opcao_u32(WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_DISABLE_COOKIES)?;
            pedido.opcao_u32(
                WINHTTP_OPTION_AUTOLOGON_POLICY,
                WINHTTP_AUTOLOGON_SECURITY_LEVEL_HIGH,
            )?;
            // SAFETY: pedido válido; `extras` terminado em nulo (comprimento u32::MAX =
            // calcular); sem corpo de envio.
            if unsafe { WinHttpSendRequest(pedido.0, extras.as_ptr(), u32::MAX, null(), 0, 0, 0) }
                == 0
            {
                return Err(falha("WinHttpSendRequest"));
            }
            // SAFETY: pedido válido, enviado; reservado nulo.
            if unsafe { WinHttpReceiveResponse(pedido.0, null_mut()) } == 0 {
                return Err(falha("WinHttpReceiveResponse"));
            }
            let mut status = 0u32;
            let mut tam = size_of::<u32>() as u32;
            // SAFETY: consulta numérica em `status` (4 bytes), ponteiros vivos.
            let ok = unsafe {
                WinHttpQueryHeaders(
                    pedido.0,
                    WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                    null(),
                    (&raw mut status).cast(),
                    &mut tam,
                    null_mut(),
                )
            };
            if ok == 0 {
                return Err(falha("WinHttpQueryHeaders"));
            }
            let content_length = match cabecalho(&pedido, WINHTTP_QUERY_CONTENT_LENGTH, None)? {
                Some(v) => Some(
                    v.trim()
                        .parse()
                        .map_err(|_| ErroRede::Status(status as u16))?,
                ),
                None => None,
            };
            Ok(RespostaBruta {
                status: status as u16,
                location: cabecalho(&pedido, WINHTTP_QUERY_LOCATION, None)?,
                content_length,
                retry_after: cabecalho(&pedido, WINHTTP_QUERY_RETRY_AFTER, None)?,
                ratelimit_reset: cabecalho(
                    &pedido,
                    WINHTTP_QUERY_CUSTOM,
                    Some("x-ratelimit-reset"),
                )?,
                corpo: CorpoWinHttp {
                    pedido,
                    _conexao: conexao,
                },
            })
        }
    }
}

// ---------------------------------------------------------------- falso

/// Transporte falso: devolve respostas em fila e registra as URLs pedidas.
#[cfg(test)]
pub(crate) struct FonteFalsa {
    pub respostas:
        std::cell::RefCell<std::collections::VecDeque<RespostaBruta<io::Cursor<Vec<u8>>>>>,
    pub pedidos: std::cell::RefCell<Vec<UrlPermitida>>,
}

#[cfg(test)]
impl FonteFalsa {
    pub(crate) fn nova(respostas: Vec<RespostaBruta<io::Cursor<Vec<u8>>>>) -> Self {
        FonteFalsa {
            respostas: std::cell::RefCell::new(respostas.into()),
            pedidos: std::cell::RefCell::new(Vec::new()),
        }
    }
}

#[cfg(test)]
impl Transporte for FonteFalsa {
    type Corpo = io::Cursor<Vec<u8>>;
    fn enviar(&self, url: &UrlPermitida) -> Result<RespostaBruta<Self::Corpo>, ErroRede> {
        self.pedidos.borrow_mut().push(url.clone());
        self.respostas
            .borrow_mut()
            .pop_front()
            .ok_or(ErroRede::Status(599))
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use nvt_atualizacao::{HOSTS_PERMITIDOS, LIMITE_MANIFESTO, URL_MANIFESTO, politica_url};

    fn resp(status: u16, corpo: &[u8]) -> RespostaBruta<io::Cursor<Vec<u8>>> {
        RespostaBruta {
            status,
            location: None,
            content_length: Some(corpo.len() as u64),
            retry_after: None,
            ratelimit_reset: None,
            corpo: io::Cursor::new(corpo.to_vec()),
        }
    }

    fn redir(loc: &str) -> RespostaBruta<io::Cursor<Vec<u8>>> {
        RespostaBruta {
            location: Some(loc.into()),
            ..resp(302, b"")
        }
    }

    fn cliente(r: Vec<RespostaBruta<io::Cursor<Vec<u8>>>>) -> Cliente<FonteFalsa> {
        Cliente::com_transporte(FonteFalsa::nova(r), Arc::new(AtomicBool::new(false)))
    }

    fn manifesto() -> UrlPermitida {
        politica_url(URL_MANIFESTO).unwrap()
    }

    #[test]
    fn cabecalhos_sem_dados_pessoais() {
        let ua = user_agent();
        assert_eq!(
            ua,
            format!("nv-tuner-atualizador/{}", env!("CARGO_PKG_VERSION"))
        );
        assert!(ua.is_ascii() && !ua.contains(' '));
        assert_eq!(CABECALHOS_ADICIONAIS, "Accept: */*\r\n");
        assert_eq!(CABECALHOS_ADICIONAIS.matches("\r\n").count(), 1);
        assert_eq!(TIMEOUTS_MS, [15_000, 15_000, 30_000, 30_000]);
        assert_eq!(PRAZO_PEQUENO, Duration::from_secs(60));
        assert_eq!(PRAZO_MSI, Duration::from_secs(600));
    }

    #[test]
    fn status_200_devolve_corpo() {
        let c = cliente(vec![resp(200, b"{}")]);
        let r = c.obter(&manifesto(), LIMITE_MANIFESTO as u64).unwrap();
        assert_eq!(r.corpo, b"{}");
        assert_eq!(r.url_final, manifesto());
    }

    #[test]
    fn mapeamento_de_status() {
        let u = manifesto();
        assert_eq!(
            cliente(vec![resp(404, b"")]).obter(&u, 10),
            Err(ErroRede::NaoEncontrado)
        );
        assert_eq!(
            cliente(vec![resp(500, b"")]).obter(&u, 10),
            Err(ErroRede::Status(500))
        );
        assert_eq!(
            cliente(vec![resp(204, b"")]).obter(&u, 10),
            Err(ErroRede::Status(204))
        );
        for s in [403, 429] {
            let r = RespostaBruta {
                retry_after: Some("120".into()),
                ratelimit_reset: Some("1700000000".into()),
                ..resp(s, b"")
            };
            assert_eq!(
                cliente(vec![r]).obter(&u, 10),
                Err(ErroRede::Limitado {
                    status: s,
                    retry_after: Some("120".into()),
                    ratelimit_reset: Some("1700000000".into()),
                })
            );
        }
    }

    #[test]
    fn redirecionamento_manual_permitido() {
        let c = cliente(vec![
            redir("https://release-assets.githubusercontent.com/x?sig=1"),
            redir("/relativo"),
            resp(200, b"ok"),
        ]);
        let r = c.obter(&manifesto(), 10).unwrap();
        assert_eq!(r.corpo, b"ok");
        assert_eq!(r.url_final.host, "release-assets.githubusercontent.com");
        assert_eq!(r.url_final.caminho, "/relativo");
        assert_eq!(c.transporte.pedidos.borrow().len(), 3);
        assert!(HOSTS_PERMITIDOS.contains(&r.url_final.host.as_str()));
    }

    #[test]
    fn redirecionamento_recusado() {
        let u = manifesto();
        for (loc, erro) in [
            ("http://github.com/x", ErroUrl::NaoHttps),
            (
                "https://evil.example/x",
                ErroUrl::HostNaoPermitido("evil.example".into()),
            ),
            (
                "https://x.githubusercontent.com/",
                ErroUrl::HostNaoPermitido("x.githubusercontent.com".into()),
            ),
            ("https://github.com:8443/x", ErroUrl::Porta),
            ("https://u:p@github.com/x", ErroUrl::Credenciais),
            ("//evil.example/x", ErroUrl::Malformada),
        ] {
            assert_eq!(
                cliente(vec![redir(loc)]).obter(&u, 10),
                Err(ErroRede::Url(erro)),
                "{loc}"
            );
        }
        let sem = RespostaBruta {
            location: None,
            ..resp(301, b"")
        };
        assert_eq!(
            cliente(vec![sem]).obter(&u, 10),
            Err(ErroRede::RedirecionamentoSemLocation)
        );
    }

    #[test]
    fn teto_de_redirecionamentos() {
        let n = MAX_REDIRECIONAMENTOS as usize;
        let mut r: Vec<_> = (0..n).map(|_| redir("/de-novo")).collect();
        r.push(resp(200, b"ok"));
        assert!(cliente(r).obter(&manifesto(), 10).is_ok());
        let r: Vec<_> = (0..=n).map(|_| redir("/de-novo")).collect();
        let c = cliente(r);
        assert_eq!(
            c.obter(&manifesto(), 10),
            Err(ErroRede::RedirecionamentosDemais)
        );
        assert_eq!(c.transporte.pedidos.borrow().len(), n + 1);
    }

    #[test]
    fn content_length_acima_do_limite_recusa_antes_de_ler() {
        let c = cliente(vec![resp(200, &[0u8; 11])]);
        assert_eq!(
            c.obter(&manifesto(), 10),
            Err(ErroRede::TamanhoExcedido { limite: 10 })
        );
    }

    #[test]
    fn corpo_sem_content_length_para_ao_exceder() {
        let grande = RespostaBruta {
            content_length: None,
            ..resp(200, &[7u8; 200_000])
        };
        let mut destino = Vec::new();
        let c = cliente(vec![grande]);
        assert_eq!(
            c.baixar(&manifesto(), 100_000, &mut destino),
            Err(ErroRede::TamanhoExcedido { limite: 100_000 })
        );
        assert!(destino.len() as u64 <= 100_000);
        let exato = RespostaBruta {
            content_length: None,
            ..resp(200, &[7u8; 100_000])
        };
        let mut destino = Vec::new();
        assert_eq!(
            cliente(vec![exato]).baixar(&manifesto(), 100_000, &mut destino),
            Ok(100_000)
        );
        assert_eq!(destino.len(), 100_000);
    }

    #[test]
    fn corpo_truncado() {
        let r = RespostaBruta {
            content_length: Some(5),
            ..resp(200, b"abc")
        };
        assert_eq!(
            cliente(vec![r]).obter(&manifesto(), 10),
            Err(ErroRede::Truncado {
                esperado: 5,
                recebido: 3
            })
        );
    }

    #[test]
    fn cancelamento_e_erro_de_escrita() {
        let c = cliente(vec![resp(200, b"x")]);
        c.cancelar.store(true, Ordering::Relaxed);
        assert_eq!(c.obter(&manifesto(), 10), Err(ErroRede::Cancelado));

        struct Cheio;
        impl Write for Cheio {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::StorageFull.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(
            cliente(vec![resp(200, b"x")]).baixar(&manifesto(), 10, &mut Cheio),
            Err(ErroRede::Escrita(io::ErrorKind::StorageFull))
        );
    }

    #[test]
    fn erros_de_leitura() {
        assert_eq!(
            erro_leitura(&io::Error::from_raw_os_error(12002)),
            ErroRede::TempoEsgotado
        );
        assert_eq!(
            erro_leitura(&io::Error::from_raw_os_error(12030)),
            ErroRede::Windows {
                funcao: "WinHttpReadData",
                codigo: 12030
            }
        );
    }

    /// Manual: `cargo test -p nvt-service -- --ignored rede_real`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn rede_real_github() {
        let rede = winhttp::RedeWinHttp::nova(Arc::new(AtomicBool::new(false))).unwrap();
        let raiz = politica_url("https://github.com/").unwrap();
        let r = rede.obter(&raiz, nvt_atualizacao::LIMITE_MSI).unwrap();
        assert!(!r.corpo.is_empty());
        match rede.obter(&manifesto(), LIMITE_MANIFESTO as u64) {
            Ok(r) => assert!(HOSTS_PERMITIDOS.contains(&r.url_final.host.as_str())),
            Err(ErroRede::NaoEncontrado) => {}
            Err(e) => panic!("{e}"),
        }
        let fora = politica_url("https://github.com/luansilvadb/nv-tuner/nao-existe-xyz").unwrap();
        assert_eq!(rede.obter(&fora, 1 << 20), Err(ErroRede::NaoEncontrado));
    }
}
