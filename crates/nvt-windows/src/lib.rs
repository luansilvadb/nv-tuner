//! Acesso tipado aos ajustes interativos e aos diagnosticos do Windows.

mod com;
pub mod conversao;
pub mod diagnosticos;
pub mod mouse;
pub mod registro;
pub mod saude;
pub mod tela;
mod token;

use std::{cmp::Ordering, fmt};

use nvt_core::usuario::{SettingUsuario, ValorUsuario};
use serde::{Deserialize, Serialize};

use crate::{
    mouse::{Spi, SpiWindows},
    registro::{DestinoRegistro, Registro, RegistroWindows},
    tela::{Display, DisplayWindows},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Erro {
    Win32(u32),
    ValorInvalido,
    FormatoAmbiguo,
    AlvoIndisponivel,
    AlvoAmbiguo,
    Clone,
    ModoIndisponivel,
    Alterado,
    Bloqueado,
    Releitura,
    RefreshNaoConfirmado {
        falha: Box<Erro>,
        compensacao: Result<(), Box<Erro>>,
    },
}

impl fmt::Display for Erro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Erro {}

/// Snapshot persistivel. A ausencia do valor REG_SZ e independente da do token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeituraUsuario {
    pub setting: SettingUsuario,
    pub valor: ValorUsuario,
    pub alvo_tela: Option<String>,
    pub registro_ausente: bool,
}

pub struct ArmazemUsuario<R = RegistroWindows, S = SpiWindows, D = DisplayWindows> {
    registro: R,
    spi: S,
    display: D,
}

impl Default for ArmazemUsuario {
    fn default() -> Self {
        Self::novo(RegistroWindows, SpiWindows, DisplayWindows)
    }
}

impl<R: Registro, S: Spi, D: Display> ArmazemUsuario<R, S, D> {
    pub fn novo(registro: R, spi: S, display: D) -> Self {
        Self {
            registro,
            spi,
            display,
        }
    }

    pub fn ler(&self, setting: SettingUsuario, alvo: Option<&str>) -> Result<LeituraUsuario, Erro> {
        use SettingUsuario::*;
        validar_alvo(setting, alvo)?;
        let (valor, registro_ausente) = match setting {
            GamedvrCapturaHistorica => registro::ler(&self.registro, DestinoRegistro::Captura)?,
            DxSwapEffectUpgrade => registro::ler(&self.registro, DestinoRegistro::DirectX)?,
            MouseAceleracao => (ValorUsuario::Spi(self.spi.ler()?), false),
            TelaRefresh => (
                ValorUsuario::Refresh(self.display.ler(alvo.ok_or(Erro::AlvoIndisponivel)?)?),
                false,
            ),
        };
        if !setting.aceita(&valor) {
            return Err(Erro::ValorInvalido);
        }
        Ok(LeituraUsuario {
            setting,
            valor,
            alvo_tela: alvo.map(str::to_owned),
            registro_ausente,
        })
    }

    /// Aplica uma transicao se a leitura ainda corresponder ao esperado.
    /// O agente fornece o portao, chamado antes de cada gravacao, inclusive compensacao.
    pub fn escrever(
        &self,
        esperado: &LeituraUsuario,
        valor: &ValorUsuario,
        autorizar: impl FnMut() -> Result<(), Erro>,
    ) -> Result<(), Erro> {
        use SettingUsuario::*;
        let permitido = match (esperado.setting, valor) {
            (GamedvrCapturaHistorica, ValorUsuario::Dword(0)) => {
                esperado.valor == ValorUsuario::Dword(1)
            }
            (MouseAceleracao, ValorUsuario::Spi(trio)) => *trio == [0; 3],
            (DxSwapEffectUpgrade, ValorUsuario::Token(_))
            | (TelaRefresh, ValorUsuario::Refresh(_)) => true,
            _ => false,
        };
        if !permitido {
            return Err(Erro::ValorInvalido);
        }
        self.transicao(esperado, valor, false, autorizar)
    }

    pub fn restaurar(
        &self,
        esperado: &LeituraUsuario,
        original: &LeituraUsuario,
        autorizar: impl FnMut() -> Result<(), Erro>,
    ) -> Result<(), Erro> {
        if esperado.setting != original.setting || esperado.alvo_tela != original.alvo_tela {
            return Err(Erro::ValorInvalido);
        }
        if original.registro_ausente && original.valor != ValorUsuario::Ausente {
            return Err(Erro::ValorInvalido);
        }
        self.transicao(
            esperado,
            &original.valor,
            original.registro_ausente,
            autorizar,
        )
    }

    fn transicao(
        &self,
        esperado: &LeituraUsuario,
        valor: &ValorUsuario,
        apagar_vazio: bool,
        mut autorizar: impl FnMut() -> Result<(), Erro>,
    ) -> Result<(), Erro> {
        use SettingUsuario::*;
        let setting = esperado.setting;
        let alvo = esperado.alvo_tela.as_deref();
        validar_alvo(setting, alvo)?;
        if !setting.aceita(valor) || !setting.aceita(&esperado.valor) {
            return Err(Erro::ValorInvalido);
        }
        match (setting, &esperado.valor, valor) {
            (GamedvrCapturaHistorica, _, _) => registro::escrever(
                &self.registro,
                DestinoRegistro::Captura,
                &esperado.valor,
                valor,
                apagar_vazio,
                autorizar,
            ),
            (DxSwapEffectUpgrade, _, _) => registro::escrever(
                &self.registro,
                DestinoRegistro::DirectX,
                &esperado.valor,
                valor,
                apagar_vazio,
                autorizar,
            ),
            (MouseAceleracao, ValorUsuario::Spi(antes), ValorUsuario::Spi(depois)) => {
                mouse::escrever(&self.spi, *antes, *depois, autorizar)
            }
            (TelaRefresh, ValorUsuario::Refresh(antes), ValorUsuario::Refresh(depois)) => {
                tela::escrever(
                    &self.display,
                    alvo.ok_or(Erro::AlvoIndisponivel)?,
                    *antes,
                    *depois,
                    &mut autorizar,
                )
            }
            _ => Err(Erro::ValorInvalido),
        }
    }
}

fn validar_alvo(setting: SettingUsuario, alvo: Option<&str>) -> Result<(), Erro> {
    match (setting, alvo) {
        (SettingUsuario::TelaRefresh, Some(alvo))
            if !alvo.trim().is_empty() && !alvo.contains('\0') =>
        {
            Ok(())
        }
        (SettingUsuario::TelaRefresh, _) => Err(Erro::AlvoIndisponivel),
        (_, None) => Ok(()),
        _ => Err(Erro::ValorInvalido),
    }
}

fn mesma_taxa(
    a: nvt_core::usuario::RefreshRacional,
    b: nvt_core::usuario::RefreshRacional,
) -> bool {
    a.comparar(b) == Some(Ordering::Equal)
}
