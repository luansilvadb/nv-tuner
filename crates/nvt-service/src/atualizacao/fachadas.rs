//! Fachadas do atualizador (spec §9.2, tarefa 2.4): rede, disco protegido, lançador,
//! portão, relógio, aleatório, verificador do manifesto e WTS, agrupadas em
//! [`Fachadas`]. As implementações reais tocam o Windows; as falsas (`#[cfg(test)]`)
//! ficam em memória e registram o que lhes foi pedido.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use nvt_atualizacao::{ErroVerificacao, Manifesto, MotivoAdiamento, Versao};

pub(crate) use super::http::Rede;
pub(crate) use super::lancador::Lancador;

/// MSI em download (`.parcial` com handle exclusivo de escrita). Soltar descarta.
pub(crate) trait DownloadMsi: Send {
    fn escritor(&mut self) -> &mut dyn Write;
    /// Tamanho e SHA-256 pelo mesmo handle que escreveu (U-14).
    fn concluir(self: Box<Self>) -> io::Result<Box<dyn MsiBaixado>>;
}

/// `.parcial` completo com hash calculado. Soltar apaga o `.parcial`.
pub(crate) trait MsiBaixado: Send {
    fn sha256(&self) -> &str;
    fn tamanho(&self) -> u64;
    /// Renomeia pelo handle e troca para o handle só de leitura (U-22).
    fn finalizar(self: Box<Self>) -> io::Result<Box<dyn MsiPronto>>;
}

/// MSI final protegido pelo handle de leitura (mantido até o msiexec abrir).
pub(crate) trait MsiPronto: Send {
    fn caminho(&self) -> &Path;
    fn sha256(&self) -> &str;
    fn tamanho(&self) -> u64;
    /// O arquivo continua o verificado (handle vivo, mesmo conteúdo).
    fn intacto(&self) -> bool;
}

/// Pasta protegida `%ProgramData%\nv-tuner\atualizacao` (U-20..U-25).
pub(crate) trait Disco: Send {
    /// Pasta canônica, sem `\\?\` (caminhos do msiexec e do log).
    fn pasta(&self) -> &Path;
    /// Lê `nome` da pasta sem seguir reparse; no máximo os últimos `limite` bytes.
    /// Ausente → `Ok(None)`.
    fn ler(&self, nome: &str, limite: u64) -> io::Result<Option<Vec<u8>>>;
    /// Grava `nome` de forma atômica (arquivo novo privado + troca), sem seguir reparse.
    fn gravar(&self, nome: &str, bytes: &[u8]) -> io::Result<()>;
    fn novo_download(&self, versao: Versao) -> io::Result<Box<dyn DownloadMsi>>;
    /// Limpeza do início (U-25): mantém só MSI e log de `manter`.
    fn limpar(&self, manter: Option<Versao>);
}

/// Portão de processos e mutex `Global\_MSIExecute` (U-26, U-27).
pub(crate) trait PortaoAtualizacao: Send {
    /// Mesmo `Portao::verificar(&base, &extras)` das gravações DRS.
    fn consultar(&self) -> Result<(), MotivoAdiamento>;
    /// `OpenMutexW(SYNCHRONIZE, FALSE, "Global\\_MSIExecute")` bem-sucedido.
    fn msi_ocupado(&self) -> bool;
}

/// Relógio de parede (só para registro e prazos persistidos; os prazos correm no
/// `Instant` passado a `tick`).
pub(crate) trait Relogio: Send {
    fn agora_unix(&self) -> u64;
}

/// Jitter do RNG do sistema, nunca persistido nem enviado (U-01).
pub(crate) trait Aleatorio: Send {
    fn sortear(&self) -> u64;
}

/// Verificação Minisign do manifesto (produção: só a chave oficial).
pub(crate) trait Verificador: Send {
    /// A chave não serve (provisória): nada é verificável, nem se contata a rede.
    fn chave_provisoria(&self) -> bool;
    fn verificar(&self, manifesto: &[u8], assinatura: &[u8]) -> Result<Manifesto, ErroVerificacao>;
}

/// Sessões do Windows (WTS) e presença do agente (U-43).
pub(crate) trait Sessoes: Send {
    /// Sessões em `WTSActive`.
    fn ativas(&self) -> Vec<u32>;
    /// Já há agente na sessão.
    fn agente_presente(&self, sessao: u32) -> bool;
}

/// Todas as dependências externas do atualizador.
pub(crate) struct Fachadas {
    pub rede: Box<dyn Rede + Send>,
    pub disco: Box<dyn Disco>,
    pub lancador: Box<dyn Lancador + Send>,
    pub portao: Box<dyn PortaoAtualizacao>,
    pub relogio: Box<dyn Relogio>,
    pub aleatorio: Box<dyn Aleatorio>,
    pub verificador: Box<dyn Verificador>,
    pub sessoes: Box<dyn Sessoes>,
}

/// Versão do próprio serviço (o MSI instala os três binários juntos, §7).
pub(crate) fn versao_instalada() -> Versao {
    env!("CARGO_PKG_VERSION")
        .parse()
        .expect("CARGO_PKG_VERSION é X.Y.Z dentro dos limites do MSI")
}

// ---------------------------------------------------------------- reais

/// Verificador de produção: `verificar_manifesto_oficial` (falha fechada com a chave
/// provisória).
pub(crate) struct VerificadorOficial;

impl Verificador for VerificadorOficial {
    fn chave_provisoria(&self) -> bool {
        nvt_atualizacao::chave_oficial_provisoria()
    }

    fn verificar(&self, manifesto: &[u8], assinatura: &[u8]) -> Result<Manifesto, ErroVerificacao> {
        nvt_atualizacao::verificar_manifesto_oficial(manifesto, assinatura)
    }
}

/// Relógio do sistema.
pub(crate) struct RelogioSistema;

impl Relogio for RelogioSistema {
    fn agora_unix(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
}

/// Portão real: a função de consulta vem do laço (mesma base e extras das gravações
/// DRS); o mutex do Windows Installer é aberto aqui.
pub(crate) struct PortaoReal<F>(pub F);

impl<F> PortaoAtualizacao for PortaoReal<F>
where
    F: Fn() -> Result<(), MotivoAdiamento> + Send,
{
    fn consultar(&self) -> Result<(), MotivoAdiamento> {
        (self.0)()
    }

    fn msi_ocupado(&self) -> bool {
        real::msi_ocupado()
    }
}

/// Converte o resultado de `Portao::verificar` no motivo do atualizador.
pub(crate) fn motivo_do_portao(
    r: Result<nvt_engine::Licenca, nvt_engine::Adiamento>,
) -> Result<(), MotivoAdiamento> {
    match r {
        Ok(_) => Ok(()),
        Err(a) if a.lista_indisponivel => Err(MotivoAdiamento::ListaIndisponivel),
        Err(_) => Err(MotivoAdiamento::JogoAberto),
    }
}

pub(crate) use real::{AleatorioSistema, DiscoReal, SessoesWts};

impl Fachadas {
    /// Fachadas de produção. `portao` consulta o portão de processos do laço;
    /// `cancelar` interrompe a rede no stop do serviço.
    pub(crate) fn reais(
        portao: impl Fn() -> Result<(), MotivoAdiamento> + Send + 'static,
        cancelar: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> io::Result<Fachadas> {
        let rede = super::http::winhttp::RedeWinHttp::nova(cancelar)
            .map_err(|e| io::Error::other(e.to_string()))?;
        Ok(Fachadas {
            rede: Box::new(rede),
            disco: Box::new(DiscoReal::abrir(&super::arquivo::pasta_padrao()?)?),
            lancador: Box::new(super::lancador::LancadorReal),
            portao: Box::new(PortaoReal(portao)),
            relogio: Box::new(RelogioSistema),
            aleatorio: Box::new(AleatorioSistema),
            verificador: Box::new(VerificadorOficial),
            sessoes: Box::new(SessoesWts),
        })
    }
}

mod real {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::io::{Read, Seek, SeekFrom};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;

    use crate::atualizacao::arquivo::{self, Baixado, Download, MsiProtegido};

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    const SYNCHRONIZE: u32 = 0x0010_0000;

    pub(super) fn msi_ocupado() -> bool {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::OpenMutexW;
        let nome: Vec<u16> = std::ffi::OsStr::new("Global\\_MSIExecute")
            .encode_wide()
            .chain(Some(0))
            .collect();
        // SAFETY: nome terminado em zero vivo durante a chamada.
        let h = unsafe { OpenMutexW(SYNCHRONIZE, 0, nome.as_ptr()) };
        if h.is_null() {
            return false;
        }
        // SAFETY: handle próprio, fechado uma vez.
        unsafe { CloseHandle(h) };
        true
    }

    /// `BCryptGenRandom` com o RNG preferido do sistema; falha → 0 (o prazo fica no
    /// mínimo do intervalo, nunca um laço).
    pub(crate) struct AleatorioSistema;

    impl Aleatorio for AleatorioSistema {
        fn sortear(&self) -> u64 {
            use windows_sys::Win32::Security::Cryptography::{
                BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
            };
            let mut b = [0u8; 8];
            // SAFETY: buffer gravável com o tamanho informado; algoritmo nulo com a flag
            // do RNG do sistema.
            let st = unsafe {
                BCryptGenRandom(
                    std::ptr::null_mut(),
                    b.as_mut_ptr(),
                    b.len() as u32,
                    BCRYPT_USE_SYSTEM_PREFERRED_RNG,
                )
            };
            if st < 0 { 0 } else { u64::from_le_bytes(b) }
        }
    }

    /// Sessões via `WTSEnumerateSessionsW`. A presença do agente é decidida pelo
    /// coordenador (agentes conectados) e pelo mutex `Local\nv-tuner-agente`, que o
    /// próprio agente usa para não duplicar (U-46); daqui não se abre mutex de outra
    /// sessão.
    pub(crate) struct SessoesWts;

    impl Sessoes for SessoesWts {
        fn ativas(&self) -> Vec<u32> {
            use windows_sys::Win32::System::RemoteDesktop::{
                WTS_CURRENT_SERVER_HANDLE, WTS_SESSION_INFOW, WTSActive, WTSEnumerateSessionsW,
                WTSFreeMemory,
            };
            let mut p: *mut WTS_SESSION_INFOW = std::ptr::null_mut();
            let mut n = 0u32;
            // SAFETY: saídas graváveis; servidor local.
            if unsafe { WTSEnumerateSessionsW(WTS_CURRENT_SERVER_HANDLE, 0, 1, &mut p, &mut n) }
                == 0
            {
                tracing::warn!(
                    "WTSEnumerateSessionsW falhou: {}",
                    io::Error::last_os_error()
                );
                return Vec::new();
            }
            // SAFETY: a API devolveu `n` estruturas contíguas em `p`.
            let v = unsafe { std::slice::from_raw_parts(p, n as usize) }
                .iter()
                .filter(|s| s.State == WTSActive)
                .map(|s| s.SessionId)
                .collect();
            // SAFETY: memória alocada pela API, liberada uma vez.
            unsafe { WTSFreeMemory(p.cast()) };
            v
        }

        fn agente_presente(&self, _sessao: u32) -> bool {
            false
        }
    }

    /// Disco real sobre a pasta protegida, conferida a cada uso (U-20).
    pub(crate) struct DiscoReal {
        pasta: PathBuf,
    }

    impl DiscoReal {
        pub(crate) fn abrir(pasta: &Path) -> io::Result<DiscoReal> {
            Ok(DiscoReal {
                pasta: arquivo::preparar_pasta(pasta)?,
            })
        }

        fn conferir(&self) -> io::Result<()> {
            arquivo::preparar_pasta(&self.pasta).map(|_| ())
        }
    }

    fn nome_valido(nome: &str) -> io::Result<()> {
        let ok = !nome.is_empty()
            && nome
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
            && !nome.starts_with('.');
        if ok {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("nome recusado na pasta da atualização: {nome:?}"),
            ))
        }
    }

    fn eh_reparse(p: &Path) -> io::Result<bool> {
        match fs::symlink_metadata(p) {
            Ok(m) => Ok(m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    impl Disco for DiscoReal {
        fn pasta(&self) -> &Path {
            &self.pasta
        }

        fn ler(&self, nome: &str, limite: u64) -> io::Result<Option<Vec<u8>>> {
            nome_valido(nome)?;
            let p = self.pasta.join(nome);
            let mut f = match crate::diretorio::abrir_sem_reparse(&p, OpenOptions::new().read(true))
            {
                Ok(f) => f,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e),
            };
            let tam = f.metadata()?.len();
            if tam > limite {
                f.seek(SeekFrom::Start(tam - limite))?;
            }
            let mut v = Vec::new();
            f.take(limite).read_to_end(&mut v)?;
            Ok(Some(v))
        }

        fn gravar(&self, nome: &str, bytes: &[u8]) -> io::Result<()> {
            nome_valido(nome)?;
            self.conferir()?;
            let destino = self.pasta.join(nome);
            if eh_reparse(&destino)? {
                return Err(io::Error::other(format!(
                    "{} é junção/link; recusado",
                    destino.display()
                )));
            }
            let (tmp, mut f) = (0..100)
                .find_map(|i| {
                    let tmp = self
                        .pasta
                        .join(format!("{nome}.{}-{i}.tmp", std::process::id()));
                    match crate::diretorio::criar_arquivo_privado(&tmp) {
                        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => None,
                        r => Some(r.map(|f| (tmp, f))),
                    }
                })
                .ok_or_else(|| io::Error::other(format!("sem nome temporário para {nome}")))??;
            let gravado = f.write_all(bytes).and_then(|()| f.sync_all());
            drop(f);
            // A troca substitui a entrada (um link no destino é trocado, não seguido).
            let r = gravado.and_then(|()| fs::rename(&tmp, &destino));
            if r.is_err() {
                let _ = fs::remove_file(&tmp);
            }
            r
        }

        fn novo_download(&self, versao: Versao) -> io::Result<Box<dyn DownloadMsi>> {
            self.conferir()?;
            Ok(Box::new(DownloadReal(Download::criar(
                &self.pasta,
                versao,
            )?)))
        }

        fn limpar(&self, manter: Option<Versao>) {
            match arquivo::limpar(&self.pasta, manter) {
                Ok(v) if !v.is_empty() => {
                    tracing::info!("atualização: limpeza apagou {} arquivo(s)", v.len());
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("atualização: limpeza falhou: {e}"),
            }
        }
    }

    struct DownloadReal(Download);

    impl DownloadMsi for DownloadReal {
        fn escritor(&mut self) -> &mut dyn Write {
            self.0.arquivo()
        }

        fn concluir(self: Box<Self>) -> io::Result<Box<dyn MsiBaixado>> {
            Ok(Box::new(BaixadoReal(self.0.concluir()?)))
        }
    }

    struct BaixadoReal(Baixado);

    impl MsiBaixado for BaixadoReal {
        fn sha256(&self) -> &str {
            self.0.sha256()
        }

        fn tamanho(&self) -> u64 {
            self.0.tamanho()
        }

        fn finalizar(self: Box<Self>) -> io::Result<Box<dyn MsiPronto>> {
            Ok(Box::new(ProntoReal(self.0.finalizar()?)))
        }
    }

    struct ProntoReal(MsiProtegido);

    impl MsiPronto for ProntoReal {
        fn caminho(&self) -> &Path {
            self.0.caminho()
        }

        fn sha256(&self) -> &str {
            self.0.sha256()
        }

        fn tamanho(&self) -> u64 {
            self.0.tamanho()
        }

        fn intacto(&self) -> bool {
            // O handle nega escrita e exclusão a terceiros; basta o caminho ainda
            // apontar para um arquivo comum do mesmo tamanho.
            fs::symlink_metadata(self.0.caminho()).is_ok_and(|m| {
                m.is_file()
                    && m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
                    && m.len() == self.0.tamanho()
            })
        }
    }
}

// ---------------------------------------------------------------- falsas

#[cfg(test)]
pub(crate) mod falsas {
    use super::*;
    use std::collections::{BTreeMap, HashMap, VecDeque};
    use std::sync::{Arc, Mutex};

    use super::super::http::{ErroRede, Resposta};
    use nvt_atualizacao::{UrlPermitida, sha256_hex};

    type Fila = VecDeque<Result<Vec<u8>, ErroRede>>;

    /// Rede falsa: respostas em fila por URL; fila vazia → falha de conexão.
    #[derive(Clone, Default)]
    pub(crate) struct RedeFalsa {
        pub respostas: Arc<Mutex<HashMap<String, Fila>>>,
        pub pedidos: Arc<Mutex<Vec<String>>>,
    }

    fn texto(u: &UrlPermitida) -> String {
        format!("https://{}{}", u.host, u.caminho)
    }

    impl RedeFalsa {
        pub(crate) fn responder(&self, url: &str, r: Result<Vec<u8>, ErroRede>) {
            self.respostas
                .lock()
                .unwrap()
                .entry(url.to_owned())
                .or_default()
                .push_back(r);
        }

        pub(crate) fn pedidos(&self) -> Vec<String> {
            self.pedidos.lock().unwrap().clone()
        }

        fn proxima(&self, u: &UrlPermitida) -> Result<Vec<u8>, ErroRede> {
            let t = texto(u);
            self.pedidos.lock().unwrap().push(t.clone());
            self.respostas
                .lock()
                .unwrap()
                .get_mut(&t)
                .and_then(VecDeque::pop_front)
                .unwrap_or(Err(ErroRede::Windows {
                    funcao: "WinHttpSendRequest",
                    codigo: 12007,
                }))
        }
    }

    impl Rede for RedeFalsa {
        fn obter(&self, url: &UrlPermitida, limite: u64) -> Result<Resposta, ErroRede> {
            let corpo = self.proxima(url)?;
            if corpo.len() as u64 > limite {
                return Err(ErroRede::TamanhoExcedido { limite });
            }
            Ok(Resposta {
                corpo,
                url_final: url.clone(),
            })
        }

        fn baixar(
            &self,
            url: &UrlPermitida,
            limite: u64,
            destino: &mut dyn Write,
        ) -> Result<u64, ErroRede> {
            let corpo = self.proxima(url)?;
            if corpo.len() as u64 > limite {
                return Err(ErroRede::TamanhoExcedido { limite });
            }
            destino
                .write_all(&corpo)
                .map_err(|e| ErroRede::Escrita(e.kind()))?;
            Ok(corpo.len() as u64)
        }
    }

    /// Disco em memória (nenhuma escrita real).
    #[derive(Clone, Default)]
    pub(crate) struct DiscoFalso {
        pub arquivos: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
        pub falhar_gravacao: Arc<Mutex<bool>>,
        pub gravacoes: Arc<Mutex<Vec<String>>>,
        /// MSIs prontos vivos (o handle ainda está aberto) → intactos?
        pub intacto: Arc<Mutex<bool>>,
        pub descartes: Arc<Mutex<u32>>,
        pub limpezas: Arc<Mutex<Vec<Option<Versao>>>>,
    }

    impl DiscoFalso {
        pub(crate) fn novo() -> Self {
            let d = DiscoFalso::default();
            *d.intacto.lock().unwrap() = true;
            d
        }

        pub(crate) fn pasta_falsa() -> PathBuf {
            PathBuf::from(r"C:\ProgramData\nv-tuner\atualizacao")
        }

        pub(crate) fn por(&self, nome: &str, bytes: &[u8]) {
            self.arquivos
                .lock()
                .unwrap()
                .insert(nome.to_owned(), bytes.to_vec());
        }

        pub(crate) fn conteudo(&self, nome: &str) -> Option<Vec<u8>> {
            self.arquivos.lock().unwrap().get(nome).cloned()
        }
    }

    impl Disco for DiscoFalso {
        fn pasta(&self) -> &Path {
            static P: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
            P.get_or_init(DiscoFalso::pasta_falsa)
        }

        fn ler(&self, nome: &str, limite: u64) -> io::Result<Option<Vec<u8>>> {
            Ok(self.conteudo(nome).map(|v| {
                let ini = v.len().saturating_sub(limite as usize);
                v[ini..].to_vec()
            }))
        }

        fn gravar(&self, nome: &str, bytes: &[u8]) -> io::Result<()> {
            if *self.falhar_gravacao.lock().unwrap() {
                return Err(io::Error::other("disco cheio (falso)"));
            }
            self.gravacoes.lock().unwrap().push(nome.to_owned());
            self.por(nome, bytes);
            Ok(())
        }

        fn novo_download(&self, versao: Versao) -> io::Result<Box<dyn DownloadMsi>> {
            Ok(Box::new(DownloadFalso {
                disco: self.clone(),
                versao,
                dados: Vec::new(),
                concluido: false,
            }))
        }

        fn limpar(&self, manter: Option<Versao>) {
            self.limpezas.lock().unwrap().push(manter);
        }
    }

    struct DownloadFalso {
        disco: DiscoFalso,
        versao: Versao,
        dados: Vec<u8>,
        concluido: bool,
    }

    impl Drop for DownloadFalso {
        fn drop(&mut self) {
            if !self.concluido {
                *self.disco.descartes.lock().unwrap() += 1;
            }
        }
    }

    impl DownloadMsi for DownloadFalso {
        fn escritor(&mut self) -> &mut dyn Write {
            &mut self.dados
        }

        fn concluir(mut self: Box<Self>) -> io::Result<Box<dyn MsiBaixado>> {
            self.concluido = true;
            Ok(Box::new(BaixadoFalso {
                disco: self.disco.clone(),
                versao: self.versao,
                sha256: sha256_hex(&self.dados),
                tamanho: self.dados.len() as u64,
                finalizado: false,
            }))
        }
    }

    struct BaixadoFalso {
        disco: DiscoFalso,
        versao: Versao,
        sha256: String,
        tamanho: u64,
        finalizado: bool,
    }

    impl Drop for BaixadoFalso {
        fn drop(&mut self) {
            if !self.finalizado {
                *self.disco.descartes.lock().unwrap() += 1;
            }
        }
    }

    impl MsiBaixado for BaixadoFalso {
        fn sha256(&self) -> &str {
            &self.sha256
        }

        fn tamanho(&self) -> u64 {
            self.tamanho
        }

        fn finalizar(mut self: Box<Self>) -> io::Result<Box<dyn MsiPronto>> {
            self.finalizado = true;
            Ok(Box::new(ProntoFalso {
                disco: self.disco.clone(),
                caminho: DiscoFalso::pasta_falsa().join(nvt_atualizacao::nome_msi(self.versao)),
                sha256: self.sha256.clone(),
                tamanho: self.tamanho,
            }))
        }
    }

    struct ProntoFalso {
        disco: DiscoFalso,
        caminho: PathBuf,
        sha256: String,
        tamanho: u64,
    }

    impl MsiPronto for ProntoFalso {
        fn caminho(&self) -> &Path {
            &self.caminho
        }

        fn sha256(&self) -> &str {
            &self.sha256
        }

        fn tamanho(&self) -> u64 {
            self.tamanho
        }

        fn intacto(&self) -> bool {
            *self.disco.intacto.lock().unwrap()
        }
    }

    /// Portão falso: resposta programável e contagem de consultas.
    #[derive(Clone, Default)]
    pub(crate) struct PortaoFalso {
        pub adiar: Arc<Mutex<Option<MotivoAdiamento>>>,
        pub ocupado: Arc<Mutex<bool>>,
        pub consultas: Arc<Mutex<u32>>,
    }

    impl PortaoAtualizacao for PortaoFalso {
        fn consultar(&self) -> Result<(), MotivoAdiamento> {
            *self.consultas.lock().unwrap() += 1;
            match *self.adiar.lock().unwrap() {
                Some(m) => Err(m),
                None => Ok(()),
            }
        }

        fn msi_ocupado(&self) -> bool {
            *self.ocupado.lock().unwrap()
        }
    }

    #[derive(Clone, Default)]
    pub(crate) struct RelogioFalso(pub Arc<Mutex<u64>>);

    impl RelogioFalso {
        pub(crate) fn avancar(&self, s: u64) {
            *self.0.lock().unwrap() += s;
        }

        pub(crate) fn definir(&self, s: u64) {
            *self.0.lock().unwrap() = s;
        }
    }

    impl Relogio for RelogioFalso {
        fn agora_unix(&self) -> u64 {
            *self.0.lock().unwrap()
        }
    }

    #[derive(Clone, Default)]
    pub(crate) struct AleatorioFalso(pub Arc<Mutex<u64>>);

    impl Aleatorio for AleatorioFalso {
        fn sortear(&self) -> u64 {
            *self.0.lock().unwrap()
        }
    }

    /// Verificador com chave injetada (só nos testes; produção usa a oficial).
    pub(crate) struct VerificadorTeste(pub nvt_atualizacao::Chave);

    impl Verificador for VerificadorTeste {
        fn chave_provisoria(&self) -> bool {
            false
        }

        fn verificar(
            &self,
            manifesto: &[u8],
            assinatura: &[u8],
        ) -> Result<Manifesto, ErroVerificacao> {
            nvt_atualizacao::verificar_manifesto(manifesto, assinatura, &self.0)
        }
    }

    #[derive(Clone, Default)]
    pub(crate) struct SessoesFalsas {
        pub ativas: Arc<Mutex<Vec<u32>>>,
        pub com_agente: Arc<Mutex<Vec<u32>>>,
    }

    impl Sessoes for SessoesFalsas {
        fn ativas(&self) -> Vec<u32> {
            self.ativas.lock().unwrap().clone()
        }

        fn agente_presente(&self, sessao: u32) -> bool {
            self.com_agente.lock().unwrap().contains(&sessao)
        }
    }

    /// Lançador falso compartilhável (o `LancadorFalso` do lote 2 atrás de `Arc`).
    #[derive(Clone, Default)]
    pub(crate) struct LancadorCompartilhado(pub Arc<super::super::lancador::LancadorFalso>);

    impl Lancador for LancadorCompartilhado {
        fn lancar_msiexec(
            &self,
            msi: &Path,
            log: &Path,
        ) -> Result<super::super::lancador::Lancamento, super::super::lancador::ErroLancador>
        {
            self.0.lancar_msiexec(msi, log)
        }

        fn relancar_agente(
            &self,
            sessoes: &[(u32, String)],
        ) -> Vec<(u32, Result<u32, super::super::lancador::ErroLancador>)> {
            self.0.relancar_agente(sessoes)
        }
    }
}
