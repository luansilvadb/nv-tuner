//! Transporte sobre named pipe (somente Windows).
//!
//! Implementado diretamente com `windows-sys` e E/S sobreposta
//! (*overlapped*), sem runtime assíncrono: o serviço atende uma conexão por
//! vez e a UI usa um cliente bloqueante em thread de fundo. A E/S sobreposta
//! serve só para impor tempos limite e para que um sinal de parada
//! interrompa esperas — coisas que a E/S síncrona de pipes não oferece.
//!
//! Segurança (ver `docs/ipc.md`):
//! - DACL protegida: nega tudo a `NT AUTHORITY\NETWORK`; SYSTEM e
//!   Administradores com acesso total; usuários autenticados apenas com
//!   leitura/escrita de dados (sem `FILE_CREATE_PIPE_INSTANCE`, para que não
//!   possam criar instâncias concorrentes do pipe);
//! - `PIPE_REJECT_REMOTE_CLIENTS` e `FILE_FLAG_FIRST_PIPE_INSTANCE`;
//! - o cliente conecta com `SECURITY_IDENTIFICATION` (o servidor não pode
//!   personificá-lo) e, por padrão, exige que o dono do pipe seja SYSTEM ou
//!   Administradores.

use std::io::{self, Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING, ERROR_NO_DATA,
    ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, ERROR_PIPE_NOT_CONNECTED, ERROR_SEM_TIMEOUT,
    GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_KERNEL_OBJECT,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, IsWellKnownSid, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    RevertToSelf, SECURITY_ATTRIBUTES, SecurityAnonymous, TOKEN_QUERY, TOKEN_USER,
    TokenImpersonationLevel, TokenSessionId, TokenUser, WinAnonymousSid,
    WinBuiltinAdministratorsSid, WinLocalSystemSid,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_GENERIC_READ,
    FILE_WRITE_DATA, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, SECURITY_IDENTIFICATION,
    SECURITY_SQOS_PRESENT, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, ImpersonateNamedPipeClient,
    PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT, WaitNamedPipeW,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentThread, INFINITE, OpenThreadToken, SetEvent, WaitForMultipleObjects,
    WaitForSingleObject,
};

use crate::quadro;
use crate::{CodigoErro, ErroIpc, Requisicao, Resposta};

/// Descritor de segurança do pipe, em SDDL.
///
/// - `D:P` — DACL protegida (não herda ACEs);
/// - `(D;;GA;;;NU)` — nega tudo a logons de rede (`NT AUTHORITY\NETWORK`);
/// - `(A;;GA;;;SY)` / `(A;;GA;;;BA)` — SYSTEM e Administradores;
/// - `(A;;0x0012008b;;;AU)` — usuários autenticados:
///   `FILE_GENERIC_READ | FILE_WRITE_DATA`, sem `FILE_APPEND_DATA`
///   (= `FILE_CREATE_PIPE_INSTANCE`). Anônimos e convidados ficam de fora.
pub const SDDL_PIPE: &str = "D:P(D;;GA;;;NU)(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x0012008b;;;AU)";

/// Acesso pedido pelo cliente: coincide com o que a ACL concede a `AU`.
const ACESSO_CLIENTE: u32 = FILE_GENERIC_READ | FILE_WRITE_DATA;

const BUFFER_PIPE: u32 = 64 * 1024;

/// Tipo do erro de uma espera interrompida pela [`Parada`]. Não é
/// `Interrupted`, que `read_exact` e `ler_quadro` repetem: com a parada
/// sinalizada, repetir a leitura giraria em falso até o tempo limite.
const PARADA: io::ErrorKind = io::ErrorKind::ConnectionAborted;

// ---------------------------------------------------------------------------
// Utilitários
// ---------------------------------------------------------------------------

/// `HANDLE` dono, fechado no `Drop`.
struct Handle(HANDLE);

// SAFETY: handles de kernel podem ser usados e fechados de qualquer thread.
unsafe impl Send for Handle {}
// SAFETY: as operações usadas (SetEvent, Wait*) são seguras entre threads.
unsafe impl Sync for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: o handle é válido e pertence exclusivamente a este valor.
        unsafe { CloseHandle(self.0) };
    }
}

fn evento_manual() -> io::Result<Handle> {
    // SAFETY: parâmetros nulos são aceitos; o retorno é conferido.
    let h = unsafe { CreateEventW(null(), 1, 0, null()) };
    if h.is_null() {
        return Err(io::Error::last_os_error());
    }
    Ok(Handle(h))
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn ms(d: Duration) -> u32 {
    u32::try_from(d.as_millis()).unwrap_or(INFINITE - 1)
}

fn ultimo_erro() -> u32 {
    // SAFETY: sem pré-condições.
    unsafe { GetLastError() }
}

fn erro_os(codigo: u32) -> io::Error {
    io::Error::from_raw_os_error(codigo as i32)
}

fn fim_de_conexao(codigo: u32) -> bool {
    matches!(
        codigo,
        ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED | ERROR_NO_DATA
    )
}

/// Sinal de parada para o servidor. Clonável e utilizável de outras threads.
#[derive(Clone)]
pub struct Parada(Arc<Handle>);

impl Parada {
    pub fn nova() -> io::Result<Self> {
        Ok(Self(Arc::new(evento_manual()?)))
    }

    /// Pede a parada: esperas em andamento são canceladas e
    /// [`Servidor::executar`] retorna.
    pub fn parar(&self) {
        // SAFETY: o evento é válido enquanto o `Arc` existir.
        unsafe { SetEvent(self.0.0) };
    }

    pub fn sinalizada(&self) -> bool {
        // SAFETY: idem.
        unsafe { WaitForSingleObject(self.0.0, 0) == WAIT_OBJECT_0 }
    }
}

// ---------------------------------------------------------------------------
// Pipe com E/S sobreposta e tempo limite
// ---------------------------------------------------------------------------

struct Pipe {
    handle: Handle,
    evento: Handle,
    parada: Option<Parada>,
    timeout_leitura: Duration,
    timeout_escrita: Duration,
    /// Prazo absoluto para leituras (duração máxima da conexão no servidor).
    prazo_leitura: Option<Instant>,
}

impl Pipe {
    /// Espera de uma leitura: o tempo limite, cortado pelo prazo da conexão.
    fn espera_leitura(&self) -> io::Result<Duration> {
        match self.prazo_leitura {
            None => Ok(self.timeout_leitura),
            Some(p) => {
                let resta = p.saturating_duration_since(Instant::now());
                if resta.is_zero() {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "duração máxima da conexão atingida",
                    ));
                }
                Ok(resta.min(self.timeout_leitura))
            }
        }
    }

    /// Espera a operação sobreposta `ov` concluir. `None` = sem limite.
    ///
    /// Em tempo esgotado ou parada, cancela a operação e aguarda o
    /// cancelamento antes de retornar, para que `ov` e o buffer não sejam
    /// usados pelo kernel depois de liberados.
    fn aguardar(&self, ov: &OVERLAPPED, limite: Option<Duration>) -> io::Result<u32> {
        let mut objetos = vec![self.evento.0];
        if let Some(p) = &self.parada {
            objetos.push(p.0.0);
        }
        let espera = limite.map_or(INFINITE, ms);
        // SAFETY: `objetos` contém handles válidos durante a chamada.
        let r =
            unsafe { WaitForMultipleObjects(objetos.len() as u32, objetos.as_ptr(), 0, espera) };
        let erro = match r {
            WAIT_OBJECT_0 => {
                let mut n = 0u32;
                // SAFETY: a operação concluiu; `ov` é o mesmo da chamada original.
                if unsafe { GetOverlappedResult(self.handle.0, ov, &mut n, 0) } != 0 {
                    return Ok(n);
                }
                return Err(erro_os(ultimo_erro()));
            }
            x if x == WAIT_OBJECT_0 + 1 => io::Error::new(PARADA, "parada solicitada"),
            WAIT_TIMEOUT => io::Error::new(io::ErrorKind::TimedOut, "tempo limite do pipe"),
            _ => io::Error::last_os_error(),
        };
        self.cancelar(ov);
        Err(erro)
    }

    fn cancelar(&self, ov: &OVERLAPPED) {
        let mut n = 0u32;
        // SAFETY: cancela e espera (bwait = TRUE) a conclusão da operação de `ov`.
        unsafe {
            CancelIoEx(self.handle.0, ov);
            GetOverlappedResult(self.handle.0, ov, &mut n, 1);
        }
    }

    fn novo_ov(&self) -> OVERLAPPED {
        // SAFETY: OVERLAPPED é POD; zerado é o estado inicial documentado.
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.hEvent = self.evento.0;
        ov
    }
}

impl Read for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let espera = self.espera_leitura()?;
        let ov = self.novo_ov();
        let len = buf.len().min(u32::MAX as usize) as u32;
        // SAFETY: `buf` e `ov` vivem até a conclusão (aguardar/cancelar esperam por ela).
        let ok = unsafe {
            ReadFile(
                self.handle.0,
                buf.as_mut_ptr(),
                len,
                null_mut(),
                (&raw const ov).cast_mut(),
            )
        };
        let r = if ok != 0 {
            self.aguardar(&ov, Some(Duration::ZERO))
        } else {
            match ultimo_erro() {
                ERROR_IO_PENDING => self.aguardar(&ov, Some(espera)),
                e => Err(erro_os(e)),
            }
        };
        match r {
            Ok(n) => Ok(n as usize),
            Err(e) if e.raw_os_error().is_some_and(|c| fim_de_conexao(c as u32)) => Ok(0),
            Err(e) => Err(e),
        }
    }
}

impl Write for Pipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let ov = self.novo_ov();
        let len = buf.len().min(u32::MAX as usize) as u32;
        // SAFETY: idem a `read`.
        let ok = unsafe {
            WriteFile(
                self.handle.0,
                buf.as_ptr(),
                len,
                null_mut(),
                (&raw const ov).cast_mut(),
            )
        };
        let r = if ok != 0 {
            self.aguardar(&ov, Some(Duration::ZERO))
        } else {
            match ultimo_erro() {
                ERROR_IO_PENDING => self.aguardar(&ov, Some(self.timeout_escrita)),
                e => Err(erro_os(e)),
            }
        };
        let n = r?;
        if n == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        Ok(n as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Servidor
// ---------------------------------------------------------------------------

/// Identidade do token Windows, lida pelo servidor após receber o quadro.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdentidadeCliente {
    pub sid: String,
    pub sessao: u32,
}

struct ReverterImpersonacao;

impl Drop for ReverterImpersonacao {
    fn drop(&mut self) {
        // SAFETY: a guarda é criada somente após impersonação nesta thread.
        if unsafe { RevertToSelf() } == 0 {
            // O processo não pode continuar sob uma identidade desconhecida.
            std::process::abort();
        }
    }
}

fn identidade_cliente(pipe: &Handle) -> io::Result<IdentidadeCliente> {
    // SAFETY: o pipe está conectado e o quadro foi lido nesta thread.
    if unsafe { ImpersonateNamedPipeClient(pipe.0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _reverter = ReverterImpersonacao;
    let mut token = null_mut();
    // SAFETY: a saída é gravável. OpenAsSelf permite consultar um token
    // de identificação sem pedir ao cliente direitos de impersonação.
    if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Handle(token);
    let ler_u32 = |classe| {
        let mut valor = 0u32;
        let mut tamanho = 0;
        // SAFETY: as classes consultadas aqui retornam um valor de 32 bits.
        if unsafe { GetTokenInformation(token.0, classe, (&raw mut valor).cast(), 4, &mut tamanho) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(valor)
    };
    if ler_u32(TokenImpersonationLevel)? == SecurityAnonymous as u32 {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let sessao = ler_u32(TokenSessionId)?;
    // TOKEN_USER seguido por um SID de até SECURITY_MAX_SID_SIZE (68 bytes).
    let mut buffer = [0usize; 32];
    let mut tamanho = 0;
    // SAFETY: buffer alinhado para TOKEN_USER, com espaço para o SID.
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size_of_val(&buffer) as u32,
            &mut tamanho,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: o buffer preenchido contém TOKEN_USER com SID válido.
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    if unsafe { IsWellKnownSid(sid, WinAnonymousSid) } != 0 {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut texto = null_mut();
    // SAFETY: o SID continua válido e a saída é gravável.
    if unsafe { ConvertSidToStringSidW(sid, &mut texto) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a conversão retorna uma cadeia terminada em zero, alocada
    // com LocalAlloc. Sua memória é liberada após copiarmos o SID.
    let sid = unsafe {
        let mut len = 0;
        while *texto.add(len) != 0 {
            len += 1;
        }
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(texto, len));
        LocalFree(texto.cast());
        sid
    };
    Ok(IdentidadeCliente { sid, sessao })
}

/// Tratador de requisições validadas, executado após reverter a impersonação.
pub trait Tratador {
    fn tratar(&mut self, identidade: IdentidadeCliente, req: Requisicao) -> Resposta;
}

impl<F: FnMut(IdentidadeCliente, Requisicao) -> Resposta> Tratador for F {
    fn tratar(&mut self, identidade: IdentidadeCliente, req: Requisicao) -> Resposta {
        self(identidade, req)
    }
}

/// Tempos limite e limites de uso do servidor.
#[derive(Debug, Clone, Copy)]
pub struct ConfigServidor {
    /// Espera máxima por cada leitura da requisição.
    pub timeout_leitura: Duration,
    pub timeout_escrita: Duration,
    /// Duração máxima de uma conexão: depois dela nenhuma leitura nova é
    /// aceita e a conexão é encerrada (um cliente não monopoliza o pipe).
    pub duracao_maxima_conexao: Duration,
    /// Taxa sustentada de requisições por segundo (balde de fichas do
    /// servidor, que não se renova ao reconectar). Acima dela a resposta é
    /// `ocupado`, sem chamar o tratador.
    pub requisicoes_por_segundo: u32,
    /// Rajada permitida acima da taxa (capacidade do balde).
    pub rajada: u32,
}

impl Default for ConfigServidor {
    fn default() -> Self {
        Self {
            timeout_leitura: Duration::from_secs(30),
            timeout_escrita: Duration::from_secs(5),
            duracao_maxima_conexao: Duration::from_secs(5 * 60),
            requisicoes_por_segundo: 10,
            rajada: 20,
        }
    }
}

/// Balde de fichas: `capacidade` fichas, repostas a `taxa` por segundo.
#[derive(Debug)]
struct Balde {
    fichas: f64,
    capacidade: f64,
    taxa: f64,
    ultimo: Instant,
}

impl Balde {
    fn novo(taxa: u32, rajada: u32) -> Self {
        let capacidade = f64::from(rajada.max(1));
        Self {
            fichas: capacidade,
            capacidade,
            taxa: f64::from(taxa),
            ultimo: Instant::now(),
        }
    }

    /// Consome uma ficha; `false` se o balde está vazio.
    fn consumir(&mut self, agora: Instant) -> bool {
        let dt = agora.saturating_duration_since(self.ultimo).as_secs_f64();
        self.ultimo = self.ultimo.max(agora);
        self.fichas = (self.fichas + dt * self.taxa).min(self.capacidade);
        if self.fichas >= 1.0 {
            self.fichas -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Servidor do pipe: uma instância, conexões atendidas em sequência.
pub struct Servidor {
    pipe: Pipe,
    parada: Parada,
    config: ConfigServidor,
    balde: Balde,
}

impl Servidor {
    /// Cria o pipe com a ACL restrita. Falha se já existir um pipe com o
    /// mesmo nome (`FILE_FLAG_FIRST_PIPE_INSTANCE`), o que impede que outro
    /// processo o tenha ocupado antes do serviço sem ser notado.
    pub fn criar(nome: &str, config: ConfigServidor, parada: Parada) -> io::Result<Self> {
        let sddl = utf16(SDDL_PIPE);
        let mut sd: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: `sddl` é UTF-16 terminado em zero; `sd` recebe memória LocalAlloc.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let atributos = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: 0,
        };
        let nome16 = utf16(nome);
        // SAFETY: ponteiros válidos durante a chamada; o retorno é conferido.
        let h = unsafe {
            CreateNamedPipeW(
                nome16.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                BUFFER_PIPE,
                BUFFER_PIPE,
                0,
                &atributos,
            )
        };
        let erro = io::Error::last_os_error();
        // SAFETY: `sd` veio de ConvertStringSecurityDescriptor…; o kernel já copiou.
        unsafe { LocalFree(sd) };
        if h == INVALID_HANDLE_VALUE {
            return Err(erro);
        }
        let handle = Handle(h);
        Ok(Self {
            pipe: Pipe {
                handle,
                evento: evento_manual()?,
                parada: Some(parada.clone()),
                timeout_leitura: config.timeout_leitura,
                timeout_escrita: config.timeout_escrita,
                prazo_leitura: None,
            },
            parada,
            config,
            balde: Balde::novo(config.requisicoes_por_segundo, config.rajada),
        })
    }

    /// Atende conexões até a [`Parada`] ser sinalizada.
    ///
    /// Erros de uma conexão (cliente que caiu, tempo esgotado, quadro
    /// inválido, limite de uso) encerram só aquela conexão. Pânicos do
    /// tratador viram `erro`/`interno` e o servidor continua. Retorna erro
    /// apenas em falhas do próprio pipe.
    pub fn executar<T: Tratador>(&mut self, tratador: &mut T) -> io::Result<()> {
        while !self.parada.sinalizada() {
            if !self.aguardar_conexao()? {
                break;
            }
            self.atender(tratador);
            // SAFETY: handle válido; descarta a conexão atual.
            unsafe { DisconnectNamedPipe(self.pipe.handle.0) };
        }
        Ok(())
    }

    /// `Ok(false)` se a parada foi pedida.
    fn aguardar_conexao(&self) -> io::Result<bool> {
        let ov = self.pipe.novo_ov();
        // SAFETY: `ov` vive até a conclusão (aguardar espera por ela).
        if unsafe { ConnectNamedPipe(self.pipe.handle.0, (&raw const ov).cast_mut()) } != 0 {
            return Ok(true);
        }
        match ultimo_erro() {
            ERROR_PIPE_CONNECTED | ERROR_NO_DATA => Ok(true),
            ERROR_IO_PENDING => match self.pipe.aguardar(&ov, None) {
                Ok(_) => Ok(true),
                Err(e) if e.kind() == PARADA && self.parada.sinalizada() => Ok(false),
                Err(e) if e.raw_os_error() == Some(ERROR_NO_DATA as i32) => Ok(true),
                Err(e) => Err(e),
            },
            e => Err(erro_os(e)),
        }
    }

    fn atender<T: Tratador>(&mut self, tratador: &mut T) {
        self.pipe.prazo_leitura = Some(Instant::now() + self.config.duracao_maxima_conexao);
        let resposta = match quadro::ler_quadro(&mut self.pipe) {
            Ok(payload) => self.responder(tratador, &payload),
            Err(ErroIpc::Grande { tamanho, limite }) => Resposta::erro(
                CodigoErro::MensagemGrande,
                format!("mensagem de {tamanho} bytes excede o limite de {limite}"),
            ),
            Err(_) => return,
        };
        if quadro::enviar(&mut self.pipe, &resposta).is_ok() {
            // DisconnectNamedPipe descarta bytes ainda no buffer. Damos ao
            // cliente um prazo curto para ler a resposta e fechar. Qualquer
            // byte extra encerra a conexão sem despachar outro quadro.
            self.pipe.prazo_leitura = Some(Instant::now() + self.config.timeout_escrita);
            let _ = self.pipe.read(&mut [0u8; 1]);
        }
    }

    fn responder<T: Tratador>(&mut self, tratador: &mut T, payload: &[u8]) -> Resposta {
        let req = match quadro::decodificar::<Requisicao>(payload) {
            Ok(req) => req,
            Err(e @ ErroIpc::VersaoIncompativel { .. }) => {
                return Resposta::erro(CodigoErro::VersaoIncompativel, e.to_string());
            }
            Err(e) => return Resposta::erro(CodigoErro::MensagemInvalida, e.to_string()),
        };
        if !self.balde.consumir(Instant::now()) {
            return Resposta::erro(
                CodigoErro::Ocupado,
                "muitas requisições; tente de novo em instantes",
            );
        }
        if let Err(m) = req.validar() {
            return Resposta::erro(CodigoErro::RequisicaoInvalida, m);
        }
        let Ok(identidade) = identidade_cliente(&self.pipe.handle) else {
            return Resposta::erro(
                CodigoErro::IdentidadeIndisponivel,
                "identidade do cliente indisponível",
            );
        };
        tratar_protegido(tratador, identidade, req)
    }
}

/// Chama o tratador capturando pânicos: viram `erro`/`interno`.
fn tratar_protegido<T: Tratador>(
    tratador: &mut T,
    identidade: IdentidadeCliente,
    req: Requisicao,
) -> Resposta {
    catch_unwind(AssertUnwindSafe(|| tratador.tratar(identidade, req)))
        .unwrap_or_else(|_| Resposta::erro(CodigoErro::Interno, "falha interna do serviço"))
}

// ---------------------------------------------------------------------------
// Cliente
// ---------------------------------------------------------------------------

/// Configuração do cliente.
#[derive(Debug, Clone, Copy)]
pub struct ConfigCliente {
    /// Espera máxima pela conexão quando o servidor está ocupado.
    pub timeout_conexao: Duration,
    /// Espera máxima pela resposta (aplicar um plano pode levar segundos).
    pub timeout_leitura: Duration,
    pub timeout_escrita: Duration,
    /// Exige que o dono do pipe seja SYSTEM ou Administradores. Desligar
    /// apenas em testes, quando o servidor roda sem privilégios.
    pub exigir_dono_privilegiado: bool,
}

impl Default for ConfigCliente {
    fn default() -> Self {
        Self {
            timeout_conexao: Duration::from_secs(2),
            timeout_leitura: Duration::from_secs(60),
            timeout_escrita: Duration::from_secs(5),
            exigir_dono_privilegiado: true,
        }
    }
}

/// Cliente bloqueante (para a UI usar em thread de fundo).
pub struct Cliente {
    pipe: Option<Pipe>,
}

impl Cliente {
    /// Conecta ao pipe `nome` (normalmente [`crate::NOME_PIPE`]).
    pub fn conectar(nome: &str, config: &ConfigCliente) -> Result<Self, ErroIpc> {
        let nome16 = utf16(nome);
        let prazo = Instant::now() + config.timeout_conexao;
        let h = loop {
            // SAFETY: nome terminado em zero; o retorno é conferido.
            let h = unsafe {
                CreateFileW(
                    nome16.as_ptr(),
                    ACESSO_CLIENTE,
                    0,
                    null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    null_mut(),
                )
            };
            if h != INVALID_HANDLE_VALUE {
                break Handle(h);
            }
            match ultimo_erro() {
                ERROR_FILE_NOT_FOUND => return Err(ErroIpc::ServicoIndisponivel),
                ERROR_PIPE_BUSY => {
                    let resta = prazo.saturating_duration_since(Instant::now());
                    if resta.is_zero() {
                        return Err(tempo_esgotado());
                    }
                    // SAFETY: nome terminado em zero.
                    if unsafe { WaitNamedPipeW(nome16.as_ptr(), ms(resta).max(1)) } == 0 {
                        match ultimo_erro() {
                            ERROR_SEM_TIMEOUT => return Err(tempo_esgotado()),
                            ERROR_FILE_NOT_FOUND => return Err(ErroIpc::ServicoIndisponivel),
                            e => return Err(erro_os(e).into()),
                        }
                    }
                }
                e => return Err(erro_os(e).into()),
            }
        };
        if config.exigir_dono_privilegiado && !dono_privilegiado(&h)? {
            return Err(ErroIpc::ServidorNaoConfiavel);
        }
        Ok(Self {
            pipe: Some(Pipe {
                handle: h,
                evento: evento_manual()?,
                parada: None,
                timeout_leitura: config.timeout_leitura,
                timeout_escrita: config.timeout_escrita,
                prazo_leitura: None,
            }),
        })
    }

    /// Usa a conexão uma vez e a fecha ao concluir. Erros de negócio chegam
    /// como `Ok(Resposta::Erro { .. })`.
    pub fn requisitar(&mut self, req: &Requisicao) -> Result<Resposta, ErroIpc> {
        let mut pipe = self.pipe.take().ok_or(ErroIpc::Fechado)?;
        quadro::enviar(&mut pipe, req)?;
        pipe.prazo_leitura = Some(Instant::now() + pipe.timeout_leitura);
        quadro::receber(&mut pipe)
    }
}

fn tempo_esgotado() -> ErroIpc {
    ErroIpc::Io(io::Error::new(
        io::ErrorKind::TimedOut,
        "tempo limite para conectar ao serviço",
    ))
}

/// O dono do objeto pipe é SYSTEM ou Administradores?
///
/// Um processo sem privilégios só consegue criar objetos cujo dono é ele
/// mesmo; logo, isso distingue o serviço de um impostor que tenha criado o
/// pipe enquanto o serviço estava parado.
fn dono_privilegiado(h: &Handle) -> Result<bool, ErroIpc> {
    let mut dono: PSID = null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: handle válido com READ_CONTROL (incluso em FILE_GENERIC_READ).
    let r = unsafe {
        GetSecurityInfo(
            h.0,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut dono,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut sd,
        )
    };
    if r != 0 {
        return Err(erro_os(r).into());
    }
    // SAFETY: `dono` aponta para dentro de `sd`, liberado logo depois.
    let ok = unsafe {
        IsWellKnownSid(dono, WinLocalSystemSid) != 0
            || IsWellKnownSid(dono, WinBuiltinAdministratorsSid) != 0
    };
    // SAFETY: `sd` foi alocado por GetSecurityInfo com LocalAlloc.
    unsafe { LocalFree(sd) };
    Ok(ok)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn balde_limita_rajada_e_repoe_com_o_tempo() {
        let mut b = Balde::novo(10, 3);
        let t0 = b.ultimo;
        assert!(b.consumir(t0));
        assert!(b.consumir(t0));
        assert!(b.consumir(t0));
        assert!(!b.consumir(t0), "rajada esgotada");
        // 100 ms a 10/s repõem uma ficha.
        let t1 = t0 + Duration::from_millis(100);
        assert!(b.consumir(t1));
        assert!(!b.consumir(t1));
        // Não acumula acima da capacidade.
        let t2 = t0 + Duration::from_secs(60);
        for _ in 0..3 {
            assert!(b.consumir(t2));
        }
        assert!(!b.consumir(t2));
    }

    #[test]
    fn panico_do_tratador_vira_interno() {
        let mut t = |_identidade: IdentidadeCliente, _req: Requisicao| -> Resposta {
            panic!("pânico proposital")
        };
        assert!(matches!(
            tratar_protegido(
                &mut t,
                IdentidadeCliente {
                    sid: "S-1-5-18".into(),
                    sessao: 0
                },
                Requisicao::ObterEstado
            ),
            Resposta::Erro {
                codigo: CodigoErro::Interno,
                ..
            }
        ));
    }
}
