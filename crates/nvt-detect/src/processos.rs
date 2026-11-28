//! Lista de processos em execução, só por nome (regras N1 e N2 da fase competitiva).
//!
//! Usa exclusivamente `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)` com
//! `Process32FirstW`/`Process32NextW`. Nunca usa `TH32CS_SNAPMODULE*` nem
//! `TH32CS_SNAPHEAPLIST`, e nunca abre handle para processo (`OpenProcess`).
//! Falso positivo por nome (outro `cs2.exe`) é aceito: só adia gravações.

use std::fmt;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_BAD_LENGTH, ERROR_NO_MORE_FILES, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};

/// Processos de anticheat que só existem enquanto um jogo roda. `vgc.exe`
/// (Vanguard) fica de fora de propósito: roda desde o boot.
pub const ANTICHEAT_SO_DURANTE_JOGO: &[&str] = &[
    "EasyAntiCheat.exe",
    "EasyAntiCheat_EOS.exe",
    "start_protected_game.exe",
    "BEService.exe",
    "BEService_x64.exe",
];

/// Processo visto no snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Processo {
    /// Nome do exe (`szExeFile`), sem caminho.
    pub nome: String,
    pub pid: u32,
    pub pid_pai: u32,
}

impl Processo {
    pub fn new(nome: impl Into<String>, pid: u32, pid_pai: u32) -> Self {
        Self {
            nome: nome.into(),
            pid,
            pid_pai,
        }
    }
}

/// A lista de processos não pôde ser lida (snapshot ou enumeração falhou).
/// Quem usa a lista trata como "não sei o que está aberto" (falha fechada):
/// adia gravações e não lê pastas de jogos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListaIndisponivel {
    /// Código de `GetLastError` (0 = desconhecido ou simulado).
    pub codigo: u32,
}

impl fmt::Display for ListaIndisponivel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lista de processos indisponível (erro {})", self.codigo)
    }
}

impl std::error::Error for ListaIndisponivel {}

/// Origem da lista de processos (injetável para testes).
pub trait FonteProcessos {
    /// Processos em execução. Numa fonte que pode falhar, a lista vazia não
    /// distingue "nada aberto" de "falhou": decisões de segurança usam
    /// [`FonteProcessos::ler`].
    fn processos(&self) -> Vec<Processo>;

    /// Leitura com falha explícita. Padrão: `Ok(self.processos())` (fontes
    /// que não falham).
    fn ler(&self) -> Result<Vec<Processo>, ListaIndisponivel> {
        Ok(self.processos())
    }
}

/// Fonte real: [`listar_processos`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessosDoSistema;

impl FonteProcessos for ProcessosDoSistema {
    /// Lista vazia se a leitura falhar; decisões de segurança usam `ler`.
    fn processos(&self) -> Vec<Processo> {
        listar_processos().unwrap_or_default()
    }

    fn ler(&self) -> Result<Vec<Processo>, ListaIndisponivel> {
        listar_processos()
    }
}

/// Fonte fixa para testes; [`ProcessosFalsos::falhando`] simula falha da
/// leitura.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessosFalsos {
    pub lista: Vec<Processo>,
    /// `true` = `ler` devolve [`ListaIndisponivel`] (e `processos`, vazio).
    pub falha: bool,
}

impl ProcessosFalsos {
    /// Processos com os nomes dados (PIDs sequenciais, pai 0).
    pub fn com_nomes(nomes: &[&str]) -> Self {
        Self {
            lista: nomes
                .iter()
                .enumerate()
                .map(|(i, n)| Processo::new(*n, i as u32 + 1, 0))
                .collect(),
            falha: false,
        }
    }

    /// Fonte cuja leitura sempre falha.
    pub fn falhando() -> Self {
        Self {
            lista: Vec::new(),
            falha: true,
        }
    }
}

impl FonteProcessos for ProcessosFalsos {
    fn processos(&self) -> Vec<Processo> {
        if self.falha {
            Vec::new()
        } else {
            self.lista.clone()
        }
    }

    fn ler(&self) -> Result<Vec<Processo>, ListaIndisponivel> {
        if self.falha {
            Err(ListaIndisponivel { codigo: 0 })
        } else {
            Ok(self.lista.clone())
        }
    }
}

/// Nomes de `vigiados` que estão em execução, na ordem e grafia de `vigiados`,
/// sem repetição. Comparação sem diferenciar maiúsculas. Função pura.
pub fn em_execucao<S: AsRef<str>>(processos: &[Processo], vigiados: &[S]) -> Vec<String> {
    // Chave `to_ascii_lowercase`: mesma equivalência de `eq_ignore_ascii_case`.
    let ativos: std::collections::HashSet<String> = processos
        .iter()
        .map(|p| p.nome.to_ascii_lowercase())
        .collect();
    let mut vistos = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for v in vigiados {
        let v = v.as_ref();
        let chave = v.to_ascii_lowercase();
        if ativos.contains(&chave) && vistos.insert(chave) {
            out.push(v.to_string());
        }
    }
    out
}

/// Fecha o snapshot ao sair do escopo.
struct Snapshot(HANDLE);

impl Drop for Snapshot {
    fn drop(&mut self) {
        // SAFETY: `self.0` é um handle válido devolvido por
        // `CreateToolhelp32Snapshot` (o construtor descarta INVALID_HANDLE_VALUE)
        // e é fechado uma única vez, aqui.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn nome_de(e: &PROCESSENTRY32W) -> String {
    let fim = e
        .szExeFile
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(e.szExeFile.len());
    String::from_utf16_lossy(&e.szExeFile[..fim])
}

fn ultimo_erro() -> u32 {
    // SAFETY: sem argumentos; lê o último erro da thread atual.
    unsafe { GetLastError() }
}

/// Processos em execução (nome do exe, PID, PID pai). Falha fechada: snapshot
/// que não abre (com uma nova tentativa curta em `ERROR_BAD_LENGTH`),
/// enumeração que não começa ou para com erro, ou lista vazia (sempre há ao
/// menos este processo) viram [`ListaIndisponivel`].
pub fn listar_processos() -> Result<Vec<Processo>, ListaIndisponivel> {
    match listar_uma_vez() {
        Err(e) if e.codigo == ERROR_BAD_LENGTH => {
            std::thread::sleep(std::time::Duration::from_millis(20));
            listar_uma_vez()
        }
        r => r,
    }
}

fn listar_uma_vez() -> Result<Vec<Processo>, ListaIndisponivel> {
    // SAFETY: chamada sem ponteiros; só TH32CS_SNAPPROCESS (N2). O pid 0 é
    // ignorado nesse modo. O retorno é verificado antes do uso.
    let h = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if h == INVALID_HANDLE_VALUE || h.is_null() {
        return Err(ListaIndisponivel {
            codigo: ultimo_erro(),
        });
    }
    let snap = Snapshot(h);

    // SAFETY: PROCESSENTRY32W é POD (inteiros e array de u16); todo-zero é válido.
    let mut e: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

    let mut out = Vec::new();
    // SAFETY: `snap.0` é um snapshot válido e vivo; `e` é uma estrutura válida,
    // exclusiva, com `dwSize` preenchido, como a API exige.
    if unsafe { Process32FirstW(snap.0, &mut e) } == 0 {
        return Err(ListaIndisponivel {
            codigo: ultimo_erro(),
        });
    }
    loop {
        out.push(Processo {
            nome: nome_de(&e),
            pid: e.th32ProcessID,
            pid_pai: e.th32ParentProcessID,
        });
        // SAFETY: as mesmas garantias da chamada a Process32FirstW.
        if unsafe { Process32NextW(snap.0, &mut e) } == 0 {
            let codigo = ultimo_erro();
            if codigo != ERROR_NO_MORE_FILES {
                return Err(ListaIndisponivel { codigo });
            }
            break;
        }
    }
    drop(snap);
    if out.is_empty() {
        return Err(ListaIndisponivel { codigo: 0 });
    }
    Ok(out)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn em_execucao_ignora_caixa_e_repeticao() {
        let f = ProcessosFalsos::com_nomes(&["explorer.exe", "CS2.EXE", "beservice_x64.exe"]);
        let r = em_execucao(
            &f.processos(),
            &["cs2.exe", "valorant.exe", "BEService_x64.exe", "Cs2.exe"],
        );
        assert_eq!(r, vec!["cs2.exe", "BEService_x64.exe"]);
        assert!(em_execucao(&f.processos(), &[] as &[&str]).is_empty());
    }

    #[test]
    fn lista_fixa_sem_vanguard() {
        assert!(
            !ANTICHEAT_SO_DURANTE_JOGO
                .iter()
                .any(|n| n.eq_ignore_ascii_case("vgc.exe"))
        );
        let f = ProcessosFalsos::com_nomes(&["vgc.exe", "easyanticheat_eos.exe"]);
        assert_eq!(
            em_execucao(&f.processos(), ANTICHEAT_SO_DURANTE_JOGO),
            vec!["EasyAntiCheat_EOS.exe"]
        );
    }

    #[test]
    fn falso_simula_falha() {
        let f = ProcessosFalsos::falhando();
        assert!(f.ler().is_err());
        assert!(f.processos().is_empty());
        let ok = ProcessosFalsos::com_nomes(&["a.exe"]);
        assert_eq!(ok.ler().unwrap().len(), 1);
    }

    #[test]
    fn snapshot_real_contem_este_processo() {
        let ps = ProcessosDoSistema.ler().expect("lista de processos");
        let meu = std::process::id();
        let eu = ps.iter().find(|p| p.pid == meu).expect("processo atual");
        assert!(eu.nome.to_lowercase().ends_with(".exe"));
        assert!(!eu.nome.contains('\\'));
    }
}
