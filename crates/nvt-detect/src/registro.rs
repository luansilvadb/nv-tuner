//! Leitura (somente leitura) do registro do Windows.

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};

/// Hive de origem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    Hklm,
    Hkcu,
}

fn abrir(hive: Hive, caminho: &str) -> Option<RegKey> {
    let raiz = match hive {
        Hive::Hklm => RegKey::predef(HKEY_LOCAL_MACHINE),
        Hive::Hkcu => RegKey::predef(HKEY_CURRENT_USER),
    };
    raiz.open_subkey_with_flags(caminho, KEY_READ | KEY_WOW64_64KEY)
        .ok()
}

/// Lê um valor texto (REG_SZ/REG_EXPAND_SZ).
pub fn texto(hive: Hive, caminho: &str, valor: &str) -> Option<String> {
    abrir(hive, caminho)?.get_value::<String, _>(valor).ok()
}

/// Lê um valor DWORD.
pub fn dword(hive: Hive, caminho: &str, valor: &str) -> Option<u32> {
    abrir(hive, caminho)?.get_value::<u32, _>(valor).ok()
}

/// Nomes das subchaves (vazio se a chave não existir).
pub fn subchaves(hive: Hive, caminho: &str) -> Vec<String> {
    abrir(hive, caminho)
        .map(|k| k.enum_keys().filter_map(Result::ok).collect())
        .unwrap_or_default()
}

/// Entrada de `...\CurrentVersion\Uninstall\*`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntradaDesinstalacao {
    pub chave: String,
    pub display_name: Option<String>,
    pub install_location: Option<String>,
    pub publisher: Option<String>,
}

const UNINSTALL: [(Hive, &str); 3] = [
    (
        Hive::Hklm,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
    (
        Hive::Hklm,
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
    (
        Hive::Hkcu,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
];

/// Todas as entradas de desinstalação (HKLM 64/32 bits e HKCU).
pub fn entradas_desinstalacao() -> Vec<EntradaDesinstalacao> {
    let mut out = Vec::new();
    for (hive, base) in UNINSTALL {
        let Some(raiz) = abrir(hive, base) else {
            continue;
        };
        for nome in raiz.enum_keys().filter_map(Result::ok) {
            let Ok(k) = raiz.open_subkey_with_flags(&nome, KEY_READ) else {
                continue;
            };
            let ler = |v: &str| {
                k.get_value::<String, _>(v)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            };
            out.push(EntradaDesinstalacao {
                display_name: ler("DisplayName"),
                install_location: ler("InstallLocation"),
                publisher: ler("Publisher"),
                chave: nome,
            });
        }
    }
    out
}

/// Serviço ou driver registrado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Servico {
    pub nome: String,
    /// `Type`: 1 = driver de kernel, 2 = driver de sistema de arquivos, 0x10/0x20 = processo.
    pub tipo: Option<u32>,
}

impl Servico {
    pub fn e_kernel(&self) -> bool {
        matches!(self.tipo, Some(1) | Some(2))
    }
}

/// Instantâneo de `HKLM\SYSTEM\CurrentControlSet\Services`.
#[derive(Debug, Clone, Default)]
pub struct ServicosSistema {
    pub servicos: Vec<Servico>,
}

impl ServicosSistema {
    pub fn ler() -> Self {
        let base = r"SYSTEM\CurrentControlSet\Services";
        let Some(raiz) = abrir(Hive::Hklm, base) else {
            return Self::default();
        };
        let servicos = raiz
            .enum_keys()
            .filter_map(Result::ok)
            .map(|nome| {
                let tipo = raiz
                    .open_subkey_with_flags(&nome, KEY_READ)
                    .ok()
                    .and_then(|k| k.get_value::<u32, _>("Type").ok());
                Servico { nome, tipo }
            })
            .collect();
        Self { servicos }
    }

    pub fn exato(&self, nome: &str) -> Option<&Servico> {
        self.servicos
            .iter()
            .find(|s| s.nome.eq_ignore_ascii_case(nome))
    }

    pub fn com_prefixo<'a>(&'a self, prefixo: &'a str) -> impl Iterator<Item = &'a Servico> + 'a {
        let p = prefixo.to_ascii_lowercase();
        self.servicos
            .iter()
            .filter(move |s| s.nome.to_ascii_lowercase().starts_with(&p))
    }
}
