//! Fuso horário local (Windows), para exibir datas do protocolo (UTC).

use crate::modelo::{civil, unix_de_civil};

/// Diferença, em segundos, entre a hora local e UTC no instante `unix`
/// (inclui horário de verão). Zero se a conversão falhar.
#[cfg(windows)]
pub fn deslocamento_local(unix: i64) -> i64 {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::Time::SystemTimeToTzSpecificLocalTime;

    let (y, m, d, h, mi, s) = civil(unix);
    let Ok(ano) = u16::try_from(y) else {
        return 0;
    };
    let utc = SYSTEMTIME {
        wYear: ano,
        wMonth: m as u16,
        wDayOfWeek: 0,
        wDay: d as u16,
        wHour: h as u16,
        wMinute: mi as u16,
        wSecond: s as u16,
        wMilliseconds: 0,
    };
    let mut local = utc;
    // SAFETY: ponteiros para estruturas válidas na pilha; fuso nulo = fuso atual.
    let ok = unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) };
    if ok == 0 {
        return 0;
    }
    unix_de_civil(
        i64::from(local.wYear),
        u32::from(local.wMonth),
        u32::from(local.wDay),
        u32::from(local.wHour),
        u32::from(local.wMinute),
        u32::from(local.wSecond),
    ) - unix
}

#[cfg(not(windows))]
pub fn deslocamento_local(_unix: i64) -> i64 {
    0
}
