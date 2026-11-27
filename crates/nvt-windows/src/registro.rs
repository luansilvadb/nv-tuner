use std::io;

use winreg::{
    RegKey, RegValue,
    enums::{HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD, REG_SZ},
};

use crate::{Erro, token};
use nvt_core::usuario::ValorUsuario;

pub(crate) const DIRECTX: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";
pub(crate) const GLOBAL: &str = "DirectXUserGlobalSettings";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DestinoRegistro {
    Captura,
    DirectX,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValorRegistro {
    Dword(u32),
    Sz(String),
    Invalido,
}

pub trait Registro {
    fn ler(&self, destino: DestinoRegistro) -> Result<Option<ValorRegistro>, Erro>;
    fn gravar_captura(&self, valor: Option<bool>) -> Result<(), Erro>;
    /// Apenas grava o REG_SZ final (ou apaga), sem leituras. A validacao contra o
    /// valor lido ocorre antes do portao.
    fn gravar_directx(&self, valor: Option<&str>) -> Result<(), Erro>;
}

#[derive(Debug, Default)]
pub struct RegistroWindows;

impl Registro for RegistroWindows {
    fn ler(&self, destino: DestinoRegistro) -> Result<Option<ValorRegistro>, Erro> {
        let (caminho, nome) = destino.endereco();
        let raiz = RegKey::predef(HKEY_CURRENT_USER);
        let chave = match raiz.open_subkey_with_flags(caminho, KEY_QUERY_VALUE) {
            Ok(chave) => chave,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(win32(e)),
        };
        let bruto = match chave.get_raw_value(nome) {
            Ok(bruto) => bruto,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(win32(e)),
        };
        Ok(Some(match bruto.vtype {
            REG_DWORD => dword(&bruto).map_or(ValorRegistro::Invalido, ValorRegistro::Dword),
            REG_SZ => sz(&bruto).map_or(ValorRegistro::Invalido, ValorRegistro::Sz),
            _ => ValorRegistro::Invalido,
        }))
    }

    fn gravar_captura(&self, valor: Option<bool>) -> Result<(), Erro> {
        match valor {
            Some(valor) => chave_gravavel(DestinoRegistro::Captura)?
                .set_value(DestinoRegistro::Captura.endereco().1, &u32::from(valor))
                .map_err(win32),
            None => apagar(DestinoRegistro::Captura),
        }
    }

    fn gravar_directx(&self, valor: Option<&str>) -> Result<(), Erro> {
        match valor {
            Some(valor) => chave_gravavel(DestinoRegistro::DirectX)?
                .set_value(GLOBAL, &valor)
                .map_err(win32),
            None => apagar(DestinoRegistro::DirectX),
        }
    }
}

impl DestinoRegistro {
    fn endereco(self) -> (&'static str, &'static str) {
        match self {
            Self::Captura => (
                r"Software\Microsoft\Windows\CurrentVersion\GameDVR",
                "HistoricalCaptureEnabled",
            ),
            Self::DirectX => (DIRECTX, GLOBAL),
        }
    }
}

fn chave_gravavel(destino: DestinoRegistro) -> Result<RegKey, Erro> {
    RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags(destino.endereco().0, KEY_SET_VALUE)
        .map(|(chave, _)| chave)
        .map_err(win32)
}

fn apagar(destino: DestinoRegistro) -> Result<(), Erro> {
    let (caminho, nome) = destino.endereco();
    match RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(caminho, KEY_SET_VALUE)
        .and_then(|chave| chave.delete_value(nome))
    {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(win32(e)),
    }
}

fn win32(e: io::Error) -> Erro {
    Erro::Win32(e.raw_os_error().unwrap_or(1) as u32)
}

pub(crate) fn dword(bruto: &RegValue) -> Option<u32> {
    if bruto.vtype != REG_DWORD {
        return None;
    }
    Some(u32::from_le_bytes(bruto.bytes.as_ref().try_into().ok()?))
}

/// Unidades UTF-16 little-endian de um valor de registro; `None` se o tamanho for ímpar.
fn unidades_utf16(bytes: &[u8]) -> Option<Vec<u16>> {
    (bytes.len() % 2 == 0).then(|| {
        bytes
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect()
    })
}

/// Decodifica texto UTF-16 de um valor de registro, tolerando terminadores NUL no fim.
pub(crate) fn utf16_bytes(bytes: &[u8]) -> Option<String> {
    let mut unidades = unidades_utf16(bytes)?;
    while unidades.last() == Some(&0) {
        unidades.pop();
    }
    String::from_utf16(&unidades).ok()
}

pub(crate) fn sz(bruto: &RegValue) -> Option<String> {
    if bruto.vtype != REG_SZ {
        return None;
    }
    let unidades = unidades_utf16(&bruto.bytes)?;
    let texto = unidades.strip_suffix(&[0])?;
    if texto.contains(&0) {
        return None;
    }
    // UTF-16 valido tem representacao unica. A regravacao preserva as unidades
    // originais dos tokens intocados, inclusive pares substitutos.
    String::from_utf16(texto).ok()
}

pub(crate) fn ler(
    registro: &impl Registro,
    destino: DestinoRegistro,
) -> Result<(ValorUsuario, bool), Erro> {
    let bruto = registro.ler(destino)?;
    Ok((interpretar(destino, bruto.as_ref())?, bruto.is_none()))
}

fn interpretar(
    destino: DestinoRegistro,
    bruto: Option<&ValorRegistro>,
) -> Result<ValorUsuario, Erro> {
    match (destino, bruto) {
        (_, None) => Ok(ValorUsuario::Ausente),
        (DestinoRegistro::Captura, Some(ValorRegistro::Dword(n @ (0 | 1)))) => {
            Ok(ValorUsuario::Dword(*n))
        }
        (DestinoRegistro::DirectX, Some(ValorRegistro::Sz(texto))) => Ok(token::swap(texto)?
            .map_or(ValorUsuario::Ausente, |s| ValorUsuario::Token(s.to_owned()))),
        _ => Err(Erro::ValorInvalido),
    }
}

pub(crate) fn escrever(
    registro: &impl Registro,
    destino: DestinoRegistro,
    esperado: &ValorUsuario,
    valor: &ValorUsuario,
    apagar_vazio: bool,
    mut autorizar: impl FnMut() -> Result<(), Erro>,
) -> Result<(), Erro> {
    // Leitura, preparacao e validacao; depois o portao; depois a gravacao, sem
    // I/O entre os dois. O Registro do Windows nao oferece compare-and-swap, e
    // uma edicao externa entre portao e gravacao e detectada na releitura.
    let bruto = registro.ler(destino)?;
    if interpretar(destino, bruto.as_ref())? != *esperado {
        return Err(Erro::Alterado);
    }
    let novo = match (destino, valor) {
        (DestinoRegistro::Captura, ValorUsuario::Ausente) => None,
        (DestinoRegistro::Captura, ValorUsuario::Dword(n)) => Some(ValorRegistro::Dword(*n)),
        (DestinoRegistro::DirectX, ValorUsuario::Ausente | ValorUsuario::Token(_)) => {
            if bruto.is_none() && *valor == ValorUsuario::Ausente {
                return Ok(());
            }
            let texto = match bruto.as_ref() {
                Some(ValorRegistro::Sz(texto)) => texto,
                _ => "",
            };
            let desejado = match valor {
                ValorUsuario::Token(token) => Some(token.as_str()),
                _ => None,
            };
            let novo = token::substituir(texto, desejado)?;
            // O resultado precisa preservar os demais tokens e conter so o pedido.
            if token::substituir(texto, token::swap(&novo)?)? != novo {
                return Err(Erro::Alterado);
            }
            if novo.is_empty() && apagar_vazio {
                None
            } else {
                Some(ValorRegistro::Sz(novo))
            }
        }
        _ => return Err(Erro::ValorInvalido),
    };
    if bruto == novo {
        return Ok(());
    }
    match (destino, novo.as_ref()) {
        (DestinoRegistro::Captura, None) => {
            autorizar()?;
            registro.gravar_captura(None)?
        }
        (DestinoRegistro::Captura, Some(ValorRegistro::Dword(n))) => {
            let ligado = *n == 1;
            autorizar()?;
            registro.gravar_captura(Some(ligado))?
        }
        (DestinoRegistro::DirectX, None) => {
            autorizar()?;
            registro.gravar_directx(None)?
        }
        (DestinoRegistro::DirectX, Some(ValorRegistro::Sz(s))) => {
            autorizar()?;
            registro.gravar_directx(Some(s))?
        }
        _ => return Err(Erro::ValorInvalido),
    }
    let lido = registro.ler(destino)?;
    if interpretar(destino, lido.as_ref())? != *valor
        || (lido.is_none() && novo.is_some())
        || (novo.is_none() && matches!(lido, Some(ValorRegistro::Sz(s)) if s.is_empty()))
    {
        return Err(Erro::Releitura);
    }
    Ok(())
}
