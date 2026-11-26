//! Conta Steam (F3-10) e Steam Cloud do CS2 (F3-09).

use crate::{Erro, Resultado, arquivo, codificacao, vdf};
use std::path::{Path, PathBuf};

/// SteamID64 da conta individual de número 0 (`SteamID3 = SteamID64 − BASE`).
pub const BASE_STEAMID64: u64 = 76_561_197_960_265_728;
/// AppID do Counter-Strike 2.
pub const APPID_CS2: u32 = 730;

/// Fonte de `HKCU\Software\Valve\Steam\SteamPath` (injetável).
pub trait RegistroSteam {
    fn steam_path(&self) -> Resultado<Option<PathBuf>>;
}

/// Registro real do usuário atual.
#[derive(Clone, Copy, Debug, Default)]
pub struct RegistroSteamWindows;

impl RegistroSteam for RegistroSteamWindows {
    fn steam_path(&self) -> Resultado<Option<PathBuf>> {
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        let chave = match hkcu.open_subkey(r"Software\Valve\Steam") {
            Ok(k) => k,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        match chave.get_value::<String, _>("SteamPath") {
            Ok(s) if !s.trim().is_empty() => Ok(Some(normalizar(&s))),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// Valor fixo (testes).
#[derive(Clone, Debug, Default)]
pub struct SteamPathFixo(pub Option<PathBuf>);

impl RegistroSteam for SteamPathFixo {
    fn steam_path(&self) -> Resultado<Option<PathBuf>> {
        Ok(self.0.clone())
    }
}

/// A Steam grava `SteamPath` com `/`; o resto do crate usa `\`.
pub fn normalizar(s: &str) -> PathBuf {
    PathBuf::from(s.trim().replace('/', "\\"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContaSteam {
    pub steamid64: u64,
    pub steamid3: u32,
}

/// `SteamID64 − 76561197960265728`, só para contas individuais (32 bits baixos).
pub fn steamid3(steamid64: u64) -> Resultado<u32> {
    steamid64
        .checked_sub(BASE_STEAMID64)
        .and_then(|d| u32::try_from(d).ok())
        .filter(|&d| d != 0)
        .ok_or_else(|| Erro::formato("SteamID64 fora da faixa de contas individuais"))
}

/// Conta com `MostRecent "1"` em `loginusers.vdf`. Nenhuma ou mais de uma → [`Erro::ContaAmbigua`].
pub fn conta_mais_recente(loginusers: &str) -> Resultado<ContaSteam> {
    let mut achadas = Vec::new();
    for f in vdf::folhas(loginusers)? {
        let [raiz, id, chave] = f.caminho.as_slice() else {
            continue;
        };
        if raiz.eq_ignore_ascii_case("users")
            && chave.eq_ignore_ascii_case("MostRecent")
            && f.valor == "1"
        {
            let id64: u64 = id
                .parse()
                .map_err(|_| Erro::formato("SteamID64 não numérico"))?;
            achadas.push(id64);
        }
    }
    achadas.sort_unstable();
    achadas.dedup();
    match achadas.as_slice() {
        [id64] => Ok(ContaSteam {
            steamid64: *id64,
            steamid3: steamid3(*id64)?,
        }),
        _ => Err(Erro::ContaAmbigua),
    }
}

/// Instalação da Steam com a conta resolvida.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalacaoSteam {
    pub pasta: PathBuf,
    pub conta: ContaSteam,
}

/// `SteamPath` → `config\loginusers.vdf` → conta `MostRecent`.
pub fn resolver(registro: &dyn RegistroSteam) -> Resultado<InstalacaoSteam> {
    let pasta = registro
        .steam_path()?
        .ok_or_else(|| Erro::ArquivoAusente(PathBuf::from("SteamPath")))?;
    let login = pasta.join("config").join("loginusers.vdf");
    let lido = arquivo::ler(&pasta, &login)?;
    let texto = codificacao::decodificar(&lido.bytes)?.texto;
    Ok(InstalacaoSteam {
        conta: conta_mais_recente(&texto)?,
        pasta,
    })
}

impl InstalacaoSteam {
    /// `userdata\<SteamID3>`.
    pub fn userdata(&self) -> PathBuf {
        self.pasta
            .join("userdata")
            .join(self.conta.steamid3.to_string())
    }

    /// `userdata\<SteamID3>\config\localconfig.vdf`.
    pub fn localconfig(&self) -> PathBuf {
        self.userdata().join("config").join("localconfig.vdf")
    }

    /// `cs2_video.txt` editável (F3-09): presente em `730\remote\cfg` → [`Erro::ArquivoNaNuvem`];
    /// senão o de `730\local\cfg`; nenhum → [`Erro::ArquivoAusente`].
    pub fn cs2_video(&self) -> Resultado<PathBuf> {
        cs2_video(&self.pasta, self.conta.steamid3)
    }
}

/// Caminho de `LaunchOptions` do app em `localconfig.vdf`.
pub fn caminho_launch_options(appid: u32) -> crate::Chave {
    crate::Chave::vdf(&format!(
        "UserLocalConfigStore/Software/Valve/Steam/apps/{appid}/LaunchOptions"
    ))
}

/// Ver [`InstalacaoSteam::cs2_video`].
pub fn cs2_video(pasta_steam: &Path, steamid3: u32) -> Resultado<PathBuf> {
    let app = pasta_steam
        .join("userdata")
        .join(steamid3.to_string())
        .join(APPID_CS2.to_string());
    let remoto = app.join("remote").join("cfg").join("cs2_video.txt");
    if arquivo::existe(pasta_steam, &remoto)? {
        return Err(Erro::ArquivoNaNuvem(remoto));
    }
    let local = app.join("local").join("cfg").join("cs2_video.txt");
    if arquivo::existe(pasta_steam, &local)? {
        return Ok(local);
    }
    Err(Erro::ArquivoAusente(local))
}
