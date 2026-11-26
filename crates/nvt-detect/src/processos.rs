//! Lista de processos em execução, só por nome (regras N1 e N2 da fase competitiva).
//!
//! Usa exclusivamente `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)` com
//! `Process32FirstW`/`Process32NextW`. Nunca usa `TH32CS_SNAPMODULE*` nem
//! `TH32CS_SNAPHEAPLIST`, e nunca abre handle para processo (`OpenProcess`).
//! Falso positivo por nome (outro `cs2.exe`) é aceito: só adia gravações.

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
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

/// Origem da lista de processos (injetável para testes).
pub trait FonteProcessos {
    fn processos(&self) -> Vec<Processo>;
}

/// Fonte real: [`listar_processos`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessosDoSistema;

impl FonteProcessos for ProcessosDoSistema {
    fn processos(&self) -> Vec<Processo> {
        listar_processos()
    }
}

/// Fonte fixa para testes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessosFalsos(pub Vec<Processo>);

impl ProcessosFalsos {
    /// Processos com os nomes dados (PIDs sequenciais, pai 0).
    pub fn com_nomes(nomes: &[&str]) -> Self {
        Self(
            nomes
                .iter()
                .enumerate()
                .map(|(i, n)| Processo::new(*n, i as u32 + 1, 0))
                .collect(),
        )
    }
}

impl FonteProcessos for ProcessosFalsos {
    fn processos(&self) -> Vec<Processo> {
        self.0.clone()
    }
}

/// Nomes de `vigiados` que estão em execução, na ordem e grafia de `vigiados`,
/// sem repetição. Comparação sem diferenciar maiúsculas. Função pura.
pub fn em_execucao<S: AsRef<str>>(processos: &[Processo], vigiados: &[S]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for v in vigiados {
        let v = v.as_ref();
        if processos.iter().any(|p| p.nome.eq_ignore_ascii_case(v))
            && !out.iter().any(|x| x.eq_ignore_ascii_case(v))
        {
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

/// Processos em execução (nome do exe, PID, PID pai). Falha = lista vazia.
pub fn listar_processos() -> Vec<Processo> {
    // SAFETY: chamada sem ponteiros; só TH32CS_SNAPPROCESS (N2). O pid 0 é
    // ignorado nesse modo. O retorno é verificado antes do uso.
    let h = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if h == INVALID_HANDLE_VALUE || h.is_null() {
        return Vec::new();
    }
    let snap = Snapshot(h);

    // SAFETY: PROCESSENTRY32W é POD (inteiros e array de u16); todo-zero é válido.
    let mut e: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

    let mut out = Vec::new();
    // SAFETY: `snap.0` é um snapshot válido e vivo; `e` é uma estrutura válida,
    // exclusiva, com `dwSize` preenchido, como a API exige.
    let mut ok = unsafe { Process32FirstW(snap.0, &mut e) } != 0;
    while ok {
        out.push(Processo {
            nome: nome_de(&e),
            pid: e.th32ProcessID,
            pid_pai: e.th32ParentProcessID,
        });
        // SAFETY: as mesmas garantias da chamada a Process32FirstW.
        ok = unsafe { Process32NextW(snap.0, &mut e) } != 0;
    }
    drop(snap);
    out
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
    fn snapshot_real_contem_este_processo() {
        let ps = ProcessosDoSistema.processos();
        let meu = std::process::id();
        let eu = ps.iter().find(|p| p.pid == meu).expect("processo atual");
        assert!(eu.nome.to_lowercase().ends_with(".exe"));
        assert!(!eu.nome.contains('\\'));
    }
}
