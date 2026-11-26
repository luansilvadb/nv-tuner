//! Limpeza segura de temporários (docs/spec/interface-e-saude.md §4.4, R-L-02 a R-L-07 e
//! R-L-09; docs/pesquisa/saude-manutencao.md).
//!
//! A passada é pura: o sistema de arquivos, a exclusão, o relógio, o portão de jogo, a
//! detecção de instalação e a política de frequência são injetados. O adaptador Windows
//! fica em [`windows`]. O chamador sempre passa o alvo e a raiz; a biblioteca nunca
//! escolhe pasta sozinha.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

pub use nvt_ipc::saude::{
    AlvoLimpeza, ModoLimpeza, MotivoInterrupcao, MotivoPulo, ResultadoLimpeza,
};

#[cfg(windows)]
pub mod windows;

#[cfg(all(feature = "build-de-teste", not(nvt_build_de_teste)))]
compile_error!("build-de-teste exige a trava do build.rs (NVT_BUILD_DE_TESTE=1, fora de CI)");

/// Modo da limpeza fixado na compilação (D2, R-L-07).
#[cfg(nvt_build_de_teste)]
pub const MODO_LIMPEZA: ModoLimpeza = ModoLimpeza::Simulacao;
/// Modo da limpeza fixado na compilação (D2, R-L-07).
#[cfg(not(nvt_build_de_teste))]
pub const MODO_LIMPEZA: ModoLimpeza = ModoLimpeza::Desligada;

/// Idade mínima de criação e de escrita (R-L-02).
pub const IDADE_MINIMA_S: u64 = 7 * 24 * 3600;
/// Intervalo mínimo entre passadas no mesmo alvo (R-L-05).
pub const INTERVALO_MINIMO_S: u64 = 24 * 3600;
/// Limite de tempo por passada (R-L-05).
pub const TEMPO_MAXIMO_MS: u64 = 60_000;
/// Limite de entradas por passada (R-L-05).
pub const ENTRADAS_MAXIMAS: u32 = 20_000;
/// Portão e instalação são reconsultados a cada tantas entradas (R-L-05).
pub const PERIODO_PORTAO: u32 = 200;

/// Nomes (comparação sem caixa) que a passada nunca toca nem percorre (R-L-04).
pub const EXCLUSOES: &[&str] = &[
    "prefetch",
    "d3dscache",
    "dxcache",
    "glcache",
    "nv_cache",
    "nvidia",
    "nvidia corporation",
    "softwaredistribution",
    "deliveryoptimization",
    "delivery optimization",
    "$recycle.bin",
    "recycler",
    "winsxs",
    "installer",
    "logs",
    "minidump",
    "livekernelreports",
];

/// Extensões (sem caixa) de logs e dumps, nunca apagadas (R-L-04).
pub const EXTENSOES_EXCLUIDAS: &[&str] = &["log", "etl", "evtx", "dmp", "mdmp", "hdmp"];

/// `true` se o nome cai na lista fixa de exclusões.
pub fn excluido(nome: &OsStr) -> bool {
    let n = nome.to_string_lossy().to_lowercase();
    if EXCLUSOES.contains(&n.as_str()) {
        return true;
    }
    Path::new(&n)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|e| EXTENSOES_EXCLUIDAS.contains(&e))
}

// ── Raiz ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroRaiz {
    /// Caminho relativo, com `..`, `.` ou sem nome final.
    NaoNormalizada,
    /// O último componente não é `Temp`.
    NaoETemp,
    /// Algum componente está na lista de exclusões.
    Excluida,
}

/// Raiz validada de uma passada: absoluta, normalizada, terminada em `Temp` e sem
/// componente da lista de exclusões.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raiz {
    alvo: AlvoLimpeza,
    caminho: PathBuf,
}

impl Raiz {
    pub fn nova(alvo: AlvoLimpeza, caminho: impl Into<PathBuf>) -> Result<Self, ErroRaiz> {
        let caminho = caminho.into();
        if !caminho.is_absolute() {
            return Err(ErroRaiz::NaoNormalizada);
        }
        let mut ultimo = None;
        for c in caminho.components() {
            match c {
                Component::Prefix(_) | Component::RootDir => {}
                Component::Normal(n) => {
                    if excluido(n) {
                        return Err(ErroRaiz::Excluida);
                    }
                    ultimo = Some(n);
                }
                Component::CurDir | Component::ParentDir => {
                    return Err(ErroRaiz::NaoNormalizada);
                }
            }
        }
        match ultimo {
            None => Err(ErroRaiz::NaoNormalizada),
            Some(n) if !n.eq_ignore_ascii_case("temp") => Err(ErroRaiz::NaoETemp),
            Some(_) => Ok(Self { alvo, caminho }),
        }
    }

    pub fn alvo(&self) -> AlvoLimpeza {
        self.alvo
    }

    pub fn caminho(&self) -> &Path {
        &self.caminho
    }
}

// ── Portas injetáveis ──────────────────────────────────────────────────────

/// Metadados de uma entrada, lidos sem seguir reparse points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entrada {
    pub nome: OsString,
    pub diretorio: bool,
    pub reparse: bool,
    pub somente_leitura: bool,
    pub sistema: bool,
    pub criacao_unix: u64,
    pub escrita_unix: u64,
    pub tamanho: u64,
}

/// Enumeração de um diretório; nunca segue reparse points.
pub trait SistemaArquivos {
    fn listar(&self, dir: &Path) -> Result<Vec<Entrada>, MotivoPulo>;
}

/// Exclusão de uma entrada já aprovada pelos filtros. Falha = pular com o motivo.
pub trait Excluidor {
    fn excluir(&mut self, caminho: &Path, entrada: &Entrada) -> Result<(), MotivoPulo>;
}

/// Excluidor de `Simulacao`: não tem acesso a handle nem a caminho; nunca apaga (R-L-06).
#[derive(Debug, Default, Clone, Copy)]
pub struct ExcluidorNulo;

impl Excluidor for ExcluidorNulo {
    fn excluir(&mut self, _: &Path, _: &Entrada) -> Result<(), MotivoPulo> {
        Ok(())
    }
}

/// Relógio em milissegundos Unix.
pub trait Relogio {
    fn agora_ms(&self) -> u64;
}

/// Relógio do sistema.
#[derive(Debug, Default, Clone, Copy)]
pub struct RelogioSistema;

impl Relogio for RelogioSistema {
    fn agora_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64)
    }
}

/// Portão de jogo/anticheat (mesma lista do `Portao` do serviço).
pub trait Portao {
    fn jogo_aberto(&self) -> bool;
}

/// Instalação MSI em andamento (mutex `Global\_MSIExecute`).
pub trait Instalacao {
    fn em_andamento(&self) -> bool;
}

/// Registro da última passada por alvo; a persistência cabe ao chamador.
pub trait Politica {
    fn ultima_unix(&self, alvo: AlvoLimpeza) -> Option<u64>;
    fn registrar(&mut self, alvo: AlvoLimpeza, inicio_unix: u64);
}

/// Política em memória (o chamador persiste [`PoliticaDiaria::ultimas`]).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PoliticaDiaria {
    pub ultimas: BTreeMap<AlvoLimpeza, u64>,
}

impl Politica for PoliticaDiaria {
    fn ultima_unix(&self, alvo: AlvoLimpeza) -> Option<u64> {
        self.ultimas.get(&alvo).copied()
    }
    fn registrar(&mut self, alvo: AlvoLimpeza, inicio_unix: u64) {
        self.ultimas.insert(alvo, inicio_unix);
    }
}

/// Dependências de uma passada.
pub struct Contexto<'a> {
    pub relogio: &'a dyn Relogio,
    pub portao: &'a dyn Portao,
    pub instalacao: &'a dyn Instalacao,
    pub politica: &'a mut dyn Politica,
}

// ── Passada ────────────────────────────────────────────────────────────────

/// Executa uma passada.
///
/// - `Desligada`: não percorre nada nem consulta a política; resultado zerado.
/// - `Simulacao`: percorre e conta com [`ExcluidorNulo`]; o `excluidor` dado é ignorado.
/// - `Ativa`: apaga pelo `excluidor`.
///
/// Devolve `None` quando a política diária recusa (passada há menos de 24 h).
pub fn limpar<S: SistemaArquivos, E: Excluidor>(
    modo: ModoLimpeza,
    raiz: &Raiz,
    fs: &S,
    excluidor: &mut E,
    ctx: &mut Contexto<'_>,
) -> Option<ResultadoLimpeza> {
    match modo {
        ModoLimpeza::Desligada => Some(ResultadoLimpeza {
            modo,
            alvo: raiz.alvo,
            inicio_unix: ctx.relogio.agora_ms() / 1000,
            duracao_ms: 0,
            arquivos: 0,
            bytes: 0,
            pulados: BTreeMap::new(),
            interrompida: None,
        }),
        ModoLimpeza::Simulacao => passada(modo, raiz, fs, &mut ExcluidorNulo, ctx),
        ModoLimpeza::Ativa => passada(modo, raiz, fs, excluidor, ctx),
    }
}

struct Passada<'p, 'c, S, E> {
    fs: &'p S,
    excluidor: &'p mut E,
    ctx: &'p Contexto<'c>,
    inicio_ms: u64,
    agora_s: u64,
    entradas: u32,
    arquivos: u32,
    bytes: u64,
    pulados: BTreeMap<MotivoPulo, u32>,
    interrompida: Option<MotivoInterrupcao>,
    no_limite: bool,
}

fn passada<S: SistemaArquivos, E: Excluidor>(
    modo: ModoLimpeza,
    raiz: &Raiz,
    fs: &S,
    excluidor: &mut E,
    ctx: &mut Contexto<'_>,
) -> Option<ResultadoLimpeza> {
    let inicio_ms = ctx.relogio.agora_ms();
    let inicio_unix = inicio_ms / 1000;
    if let Some(u) = ctx.politica.ultima_unix(raiz.alvo)
        && inicio_unix < u.saturating_add(INTERVALO_MINIMO_S)
    {
        return None;
    }
    ctx.politica.registrar(raiz.alvo, inicio_unix);
    let (interrompida, arquivos, bytes, pulados) = {
        let mut p = Passada {
            fs,
            excluidor,
            ctx,
            inicio_ms,
            agora_s: inicio_unix,
            entradas: 0,
            arquivos: 0,
            bytes: 0,
            pulados: BTreeMap::new(),
            interrompida: None,
            no_limite: false,
        };
        if p.portoes() {
            p.percorrer(raiz.caminho());
        }
        (p.interrompida, p.arquivos, p.bytes, p.pulados)
    };
    let duracao = ctx.relogio.agora_ms().saturating_sub(inicio_ms);
    Some(ResultadoLimpeza {
        modo,
        alvo: raiz.alvo,
        inicio_unix,
        duracao_ms: u32::try_from(duracao).unwrap_or(u32::MAX),
        arquivos,
        bytes,
        pulados,
        interrompida,
    })
}

impl<S: SistemaArquivos, E: Excluidor> Passada<'_, '_, S, E> {
    fn pular(&mut self, m: MotivoPulo) {
        *self.pulados.entry(m).or_insert(0) += 1;
    }

    fn parar(&self) -> bool {
        self.interrompida.is_some() || self.no_limite
    }

    /// `true` se pode seguir.
    fn portoes(&mut self) -> bool {
        if self.ctx.portao.jogo_aberto() {
            self.interrompida = Some(MotivoInterrupcao::JogoAberto);
        } else if self.ctx.instalacao.em_andamento() {
            self.interrompida = Some(MotivoInterrupcao::Instalacao);
        } else if self.ctx.relogio.agora_ms().saturating_sub(self.inicio_ms) > TEMPO_MAXIMO_MS {
            self.interrompida = Some(MotivoInterrupcao::Tempo);
        }
        self.interrompida.is_none()
    }

    fn velho(&self, e: &Entrada) -> bool {
        let ok = |t: u64| t <= self.agora_s && self.agora_s - t >= IDADE_MINIMA_S;
        ok(e.criacao_unix) && ok(e.escrita_unix)
    }

    /// Percorre `dir`; devolve `true` se ele ficou (ou ficaria) vazio.
    fn percorrer(&mut self, dir: &Path) -> bool {
        let filhos = match self.fs.listar(dir) {
            Ok(f) => f,
            Err(m) => {
                self.pular(m);
                return false;
            }
        };
        let mut vazio = true;
        for e in filhos {
            if self.parar() {
                return false;
            }
            if self.entradas >= ENTRADAS_MAXIMAS {
                self.no_limite = true;
                self.pular(MotivoPulo::Limite);
                return false;
            }
            self.entradas += 1;
            if self.entradas % PERIODO_PORTAO == 0 && !self.portoes() {
                return false;
            }
            if !self.entrada(dir, &e) {
                vazio = false;
            }
        }
        vazio && !self.parar()
    }

    /// Trata uma entrada; devolve `true` se ela foi (ou seria) apagada.
    fn entrada(&mut self, dir: &Path, e: &Entrada) -> bool {
        if excluido(&e.nome) {
            self.pular(MotivoPulo::Sistema);
            return false;
        }
        if e.reparse {
            self.pular(MotivoPulo::ReparsePoint);
            return false;
        }
        if e.sistema {
            self.pular(MotivoPulo::Sistema);
            return false;
        }
        if e.somente_leitura {
            self.pular(MotivoPulo::SomenteLeitura);
            return false;
        }
        let caminho = dir.join(&e.nome);
        if e.diretorio {
            // Desce sempre; só apaga se ficou vazio e é velho. Diretório não conta em
            // `arquivos` nem em `pulados` quando preservado por filho.
            if !self.percorrer(&caminho) || !self.velho(e) {
                return false;
            }
            return match self.excluidor.excluir(&caminho, e) {
                Ok(()) => true,
                Err(m) => {
                    self.pular(m);
                    false
                }
            };
        }
        if !self.velho(e) {
            self.pular(MotivoPulo::Recente);
            return false;
        }
        match self.excluidor.excluir(&caminho, e) {
            Ok(()) => {
                self.arquivos += 1;
                self.bytes += e.tamanho;
                true
            }
            Err(m) => {
                self.pular(m);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const AGORA: u64 = 1_800_000_000;
    const VELHO: u64 = AGORA - IDADE_MINIMA_S;

    #[derive(Default)]
    struct FsFalso {
        dirs: BTreeMap<PathBuf, Vec<Entrada>>,
    }

    impl FsFalso {
        fn com(mut self, dir: &str, e: Entrada) -> Self {
            self.dirs.entry(PathBuf::from(dir)).or_default().push(e);
            self
        }
    }

    impl SistemaArquivos for FsFalso {
        fn listar(&self, dir: &Path) -> Result<Vec<Entrada>, MotivoPulo> {
            Ok(self.dirs.get(dir).cloned().unwrap_or_default())
        }
    }

    fn arq(nome: &str, idade_c: u64, idade_e: u64) -> Entrada {
        Entrada {
            nome: nome.into(),
            diretorio: false,
            reparse: false,
            somente_leitura: false,
            sistema: false,
            criacao_unix: idade_c,
            escrita_unix: idade_e,
            tamanho: 10,
        }
    }

    fn dir(nome: &str) -> Entrada {
        Entrada {
            diretorio: true,
            tamanho: 0,
            ..arq(nome, VELHO, VELHO)
        }
    }

    #[derive(Default)]
    struct Espiao {
        chamadas: Vec<PathBuf>,
    }

    impl Excluidor for Espiao {
        fn excluir(&mut self, c: &Path, _: &Entrada) -> Result<(), MotivoPulo> {
            self.chamadas.push(c.to_path_buf());
            Ok(())
        }
    }

    struct Relo(Cell<u64>, u64);
    impl Relogio for Relo {
        fn agora_ms(&self) -> u64 {
            let v = self.0.get();
            self.0.set(v + self.1);
            v
        }
    }
    struct Flag(bool);
    impl Portao for Flag {
        fn jogo_aberto(&self) -> bool {
            self.0
        }
    }
    impl Instalacao for Flag {
        fn em_andamento(&self) -> bool {
            self.0
        }
    }
    struct PortaoContado(Cell<u32>, u32);
    impl Portao for PortaoContado {
        fn jogo_aberto(&self) -> bool {
            self.0.set(self.0.get() + 1);
            self.0.get() > self.1
        }
    }

    fn raiz() -> Raiz {
        Raiz::nova(AlvoLimpeza::TempUsuario, r"C:\u\Temp").unwrap()
    }

    fn rodar(
        modo: ModoLimpeza,
        fs: &FsFalso,
        esp: &mut Espiao,
        portao: &dyn Portao,
        inst: &dyn Instalacao,
        pol: &mut PoliticaDiaria,
    ) -> Option<ResultadoLimpeza> {
        let relogio = Relo(Cell::new(AGORA * 1000), 1);
        let mut ctx = Contexto {
            relogio: &relogio,
            portao,
            instalacao: inst,
            politica: pol,
        };
        limpar(modo, &raiz(), fs, esp, &mut ctx)
    }

    fn simples(modo: ModoLimpeza, fs: &FsFalso, esp: &mut Espiao) -> ResultadoLimpeza {
        rodar(
            modo,
            fs,
            esp,
            &Flag(false),
            &Flag(false),
            &mut PoliticaDiaria::default(),
        )
        .unwrap()
    }

    #[test]
    fn modo_por_cfg() {
        #[cfg(nvt_build_de_teste)]
        assert_eq!(MODO_LIMPEZA, ModoLimpeza::Simulacao);
        #[cfg(not(nvt_build_de_teste))]
        assert_eq!(MODO_LIMPEZA, ModoLimpeza::Desligada);
    }

    #[test]
    fn idade_exige_criacao_e_escrita() {
        let r = r"C:\u\Temp";
        let fs = FsFalso::default()
            .com(r, arq("a", VELHO, VELHO))
            .com(r, arq("b", VELHO, AGORA - 10))
            .com(r, arq("c", AGORA - 10, VELHO))
            .com(r, arq("d", VELHO + 1, VELHO))
            .com(r, arq("e", AGORA + 99, AGORA + 99));
        let mut esp = Espiao::default();
        let res = simples(ModoLimpeza::Ativa, &fs, &mut esp);
        assert_eq!(esp.chamadas, vec![PathBuf::from(r).join("a")]);
        assert_eq!((res.arquivos, res.bytes), (1, 10));
        assert_eq!(res.pulados[&MotivoPulo::Recente], 4);
    }

    #[test]
    fn filtros_de_atributo_e_exclusoes() {
        let r = r"C:\u\Temp";
        let mut ro = arq("ro", VELHO, VELHO);
        ro.somente_leitura = true;
        let mut sis = arq("sis", VELHO, VELHO);
        sis.sistema = true;
        let mut rep = dir("junc");
        rep.reparse = true;
        let fs = FsFalso::default()
            .com(r, ro)
            .com(r, sis)
            .com(r, rep)
            .com(r, arq("x.log", VELHO, VELHO))
            .com(r, arq("crash.DMP", VELHO, VELHO))
            .com(r, dir("Prefetch"))
            .com(r"C:\u\Temp\Prefetch", arq("p", VELHO, VELHO))
            .com(r"C:\u\Temp\junc", arq("fora", VELHO, VELHO));
        let mut esp = Espiao::default();
        let res = simples(ModoLimpeza::Ativa, &fs, &mut esp);
        assert!(esp.chamadas.is_empty(), "{:?}", esp.chamadas);
        assert_eq!(res.pulados[&MotivoPulo::SomenteLeitura], 1);
        assert_eq!(res.pulados[&MotivoPulo::ReparsePoint], 1);
        assert_eq!(res.pulados[&MotivoPulo::Sistema], 4);
    }

    #[test]
    fn lista_de_exclusoes_fixa() {
        for n in [
            "Prefetch",
            "D3DSCache",
            "DXCache",
            "GLCache",
            "NV_Cache",
            "SoftwareDistribution",
            "DeliveryOptimization",
            "Delivery Optimization",
            "$Recycle.Bin",
            "WinSxS",
            "Installer",
            "Logs",
            "Minidump",
            "LiveKernelReports",
            "setup.log",
            "MEMORY.DMP",
        ] {
            assert!(excluido(OsStr::new(n)), "{n}");
        }
        assert!(!excluido(OsStr::new("tmp123.tmp")));
    }

    #[test]
    fn raiz_recusa_fora_dos_alvos() {
        use AlvoLimpeza::TempWindows as W;
        assert!(Raiz::nova(W, r"C:\Windows\Temp").is_ok());
        assert!(Raiz::nova(W, r"C:\Users\x\AppData\Local\Temp").is_ok());
        assert_eq!(Raiz::nova(W, r"Temp"), Err(ErroRaiz::NaoNormalizada));
        assert_eq!(
            Raiz::nova(W, r"C:\Windows\Temp\..\Temp"),
            Err(ErroRaiz::NaoNormalizada)
        );
        assert_eq!(Raiz::nova(W, r"C:\Windows"), Err(ErroRaiz::NaoETemp));
        assert_eq!(Raiz::nova(W, r"C:\"), Err(ErroRaiz::NaoNormalizada));
        assert_eq!(
            Raiz::nova(W, r"C:\Windows\Prefetch\Temp"),
            Err(ErroRaiz::Excluida)
        );
        assert_eq!(
            Raiz::nova(W, r"C:\Windows\SoftwareDistribution\Temp"),
            Err(ErroRaiz::Excluida)
        );
    }

    #[test]
    fn diretorio_so_sai_vazio_e_velho() {
        let r = r"C:\u\Temp";
        let mut novo = dir("novo");
        novo.criacao_unix = AGORA;
        let fs = FsFalso::default()
            .com(r, dir("vazio_velho"))
            .com(r, novo)
            .com(r, dir("com_recente"))
            .com(r"C:\u\Temp\com_recente", arq("r", AGORA, AGORA))
            .com(r"C:\u\Temp\com_recente", arq("v", VELHO, VELHO));
        let mut esp = Espiao::default();
        let res = simples(ModoLimpeza::Ativa, &fs, &mut esp);
        let base = PathBuf::from(r);
        assert_eq!(
            esp.chamadas,
            vec![base.join("vazio_velho"), base.join(r"com_recente\v")]
        );
        assert_eq!(res.arquivos, 1);
    }

    #[test]
    fn simulacao_nunca_chama_excluidor() {
        let r = r"C:\u\Temp";
        let fs = FsFalso::default()
            .com(r, arq("a", VELHO, VELHO))
            .com(r, dir("d"))
            .com(r"C:\u\Temp\d", arq("b", VELHO, VELHO));
        let mut esp = Espiao::default();
        let res = simples(ModoLimpeza::Simulacao, &fs, &mut esp);
        assert!(esp.chamadas.is_empty());
        assert_eq!((res.arquivos, res.bytes), (2, 20));
        assert_eq!(res.modo, ModoLimpeza::Simulacao);
        res.validar().unwrap();
    }

    #[test]
    fn desligada_nao_percorre() {
        let fs = FsFalso::default().com(r"C:\u\Temp", arq("a", VELHO, VELHO));
        let mut esp = Espiao::default();
        let mut pol = PoliticaDiaria::default();
        let res = rodar(
            ModoLimpeza::Desligada,
            &fs,
            &mut esp,
            &Flag(false),
            &Flag(false),
            &mut pol,
        )
        .unwrap();
        assert!(esp.chamadas.is_empty());
        assert_eq!((res.arquivos, res.bytes), (0, 0));
        assert!(pol.ultimas.is_empty());
        res.validar().unwrap();
    }

    #[test]
    fn portao_e_instalacao_antes_de_comecar() {
        let fs = FsFalso::default().com(r"C:\u\Temp", arq("a", VELHO, VELHO));
        let mut pol = PoliticaDiaria::default();
        let mut esp = Espiao::default();
        let r = rodar(
            ModoLimpeza::Ativa,
            &fs,
            &mut esp,
            &Flag(true),
            &Flag(false),
            &mut pol,
        )
        .unwrap();
        assert_eq!(r.interrompida, Some(MotivoInterrupcao::JogoAberto));
        let mut pol = PoliticaDiaria::default();
        let r = rodar(
            ModoLimpeza::Ativa,
            &fs,
            &mut esp,
            &Flag(false),
            &Flag(true),
            &mut pol,
        )
        .unwrap();
        assert_eq!(r.interrompida, Some(MotivoInterrupcao::Instalacao));
        assert!(esp.chamadas.is_empty());
        assert_eq!(r.arquivos, 0);
    }

    #[test]
    fn portao_interrompe_durante() {
        let mut fs = FsFalso::default();
        for i in 0..1000 {
            fs = fs.com(r"C:\u\Temp", arq(&format!("f{i}"), VELHO, VELHO));
        }
        let portao = PortaoContado(Cell::new(0), 2);
        let mut esp = Espiao::default();
        let r = rodar(
            ModoLimpeza::Ativa,
            &fs,
            &mut esp,
            &portao,
            &Flag(false),
            &mut PoliticaDiaria::default(),
        )
        .unwrap();
        assert_eq!(r.interrompida, Some(MotivoInterrupcao::JogoAberto));
        // Consultas: antes, na 200ª e na 400ª (esta fecha o portão).
        assert_eq!(r.arquivos, 2 * PERIODO_PORTAO - 1);
        assert_eq!(esp.chamadas.len() as u32, r.arquivos);
    }

    #[test]
    fn limite_de_tempo_e_entradas() {
        let mut fs = FsFalso::default();
        for i in 0..(ENTRADAS_MAXIMAS + 5) {
            fs = fs.com(r"C:\u\Temp", arq(&format!("f{i}"), VELHO, VELHO));
        }
        let mut esp = Espiao::default();
        let r = simples(ModoLimpeza::Simulacao, &fs, &mut esp);
        assert_eq!(r.arquivos, ENTRADAS_MAXIMAS);
        assert_eq!(r.pulados[&MotivoPulo::Limite], 1);

        // Relógio que anda 30 s por leitura: a consulta da 400ª entrada passa de 60 s.
        let relogio = Relo(Cell::new(AGORA * 1000), 30_000);
        let mut pol = PoliticaDiaria::default();
        let mut ctx = Contexto {
            relogio: &relogio,
            portao: &Flag(false),
            instalacao: &Flag(false),
            politica: &mut pol,
        };
        let r = limpar(ModoLimpeza::Simulacao, &raiz(), &fs, &mut esp, &mut ctx).unwrap();
        assert_eq!(r.interrompida, Some(MotivoInterrupcao::Tempo));
        assert_eq!(r.arquivos, 2 * PERIODO_PORTAO - 1);
    }

    #[test]
    fn uma_vez_por_dia() {
        let fs = FsFalso::default();
        let mut esp = Espiao::default();
        let mut pol = PoliticaDiaria::default();
        pol.registrar(AlvoLimpeza::TempUsuario, AGORA - INTERVALO_MINIMO_S + 1);
        assert!(
            rodar(
                ModoLimpeza::Simulacao,
                &fs,
                &mut esp,
                &Flag(false),
                &Flag(false),
                &mut pol
            )
            .is_none()
        );
        pol.registrar(AlvoLimpeza::TempUsuario, AGORA - INTERVALO_MINIMO_S);
        assert!(
            rodar(
                ModoLimpeza::Simulacao,
                &fs,
                &mut esp,
                &Flag(false),
                &Flag(false),
                &mut pol
            )
            .is_some()
        );
        assert_eq!(pol.ultimas[&AlvoLimpeza::TempUsuario], AGORA);
        // Outro alvo tem registro próprio.
        assert_eq!(pol.ultima_unix(AlvoLimpeza::TempWindows), None);
    }

    #[test]
    fn resultado_sem_caminhos() {
        let fs = FsFalso::default().com(r"C:\u\Temp", arq("segredo_usuario.txt", VELHO, VELHO));
        let mut esp = Espiao::default();
        let r = simples(ModoLimpeza::Simulacao, &fs, &mut esp);
        let s = format!("{r:?}");
        assert!(!s.contains("segredo") && !s.contains(r"C:\u"));
    }
}
