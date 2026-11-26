use std::ptr;

use windows_sys::Win32::{
    Foundation::GetLastError,
    UI::WindowsAndMessaging::{
        SPI_GETMOUSE, SPI_SETMOUSE, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SystemParametersInfoW,
    },
};

use crate::Erro;

pub trait Spi {
    fn ler(&self) -> Result<[i32; 3], Erro>;
    fn gravar(&self, trio: [i32; 3]) -> Result<(), Erro>;
}

#[derive(Debug, Default)]
pub struct SpiWindows;

impl Spi for SpiWindows {
    fn ler(&self) -> Result<[i32; 3], Erro> {
        let mut trio = [0; 3];
        // SAFETY: o buffer gravavel contem os tres inteiros exigidos.
        let sucesso =
            unsafe { SystemParametersInfoW(SPI_GETMOUSE, 0, ptr::from_mut(&mut trio).cast(), 0) };
        verificar(sucesso)?;
        Ok(trio)
    }

    fn gravar(&self, mut trio: [i32; 3]) -> Result<(), Erro> {
        // SAFETY: o buffer contem os tres inteiros esperados pela operacao.
        let sucesso = unsafe {
            SystemParametersInfoW(
                SPI_SETMOUSE,
                0,
                ptr::from_mut(&mut trio).cast(),
                SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
            )
        };
        verificar(sucesso)
    }
}

fn verificar(sucesso: i32) -> Result<(), Erro> {
    if sucesso != 0 {
        Ok(())
    } else {
        // SAFETY: consulta o ultimo erro da thread atual.
        Err(Erro::Win32(unsafe { GetLastError() }))
    }
}

pub(crate) fn escrever(
    spi: &impl Spi,
    antes: [i32; 3],
    depois: [i32; 3],
    mut autorizar: impl FnMut() -> Result<(), Erro>,
) -> Result<(), Erro> {
    if spi.ler()? != antes {
        return Err(Erro::Alterado);
    }
    if antes != depois {
        autorizar()?;
        if spi.ler()? != antes {
            return Err(Erro::Alterado);
        }
        spi.gravar(depois)?;
    }
    if spi.ler()? != depois {
        return Err(Erro::Releitura);
    }
    Ok(())
}
