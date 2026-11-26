use std::{
    ffi::OsString,
    mem::size_of,
    os::windows::ffi::OsStringExt,
    path::PathBuf,
    ptr::{null, null_mut},
};

use windows_sys::Win32::{
    Foundation::HANDLE,
    Security::{
        EqualSid, GetTokenInformation, IsWellKnownSid, LookupAccountNameW, SECURITY_MAX_SID_SIZE,
        TOKEN_USER, TokenUser, WinLocalSystemSid,
    },
    System::{
        Com::CoTaskMemFree,
        RemoteDesktop::{
            WTS_INFO_CLASS, WTSDomainName, WTSFreeMemory, WTSQuerySessionInformationW, WTSUserName,
        },
    },
    UI::Shell::{FOLDERID_LocalAppData, KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath},
};

fn conta_wts(sessao: u32, campo: WTS_INFO_CLASS) -> Option<String> {
    let mut buffer = null_mut();
    let mut bytes = 0;
    // A API aloca um buffer UTF-16 terminado em NUL em caso de sucesso.
    unsafe {
        if WTSQuerySessionInformationW(null_mut(), sessao, campo, &mut buffer, &mut bytes) == 0
            || buffer.is_null()
        {
            return None;
        }
        let dados = std::slice::from_raw_parts(buffer, bytes as usize / 2);
        let texto = dados
            .split(|&u| u == 0)
            .next()
            .and_then(|s| String::from_utf16(s).ok());
        WTSFreeMemory(buffer.cast());
        texto.filter(|s| !s.is_empty())
    }
}

pub(super) fn conta_da_sessao(sessao: u32) -> bool {
    let (Some(usuario), Some(dominio)) = (
        conta_wts(sessao, WTSUserName),
        conta_wts(sessao, WTSDomainName),
    ) else {
        return false;
    };
    let conta: Vec<_> = format!(r"{dominio}\{usuario}")
        .encode_utf16()
        .chain([0])
        .collect();
    let mut sid_sessao = [0_u32; SECURITY_MAX_SID_SIZE as usize / size_of::<u32>()];
    let mut tamanho_sid = size_of_val(&sid_sessao) as u32;
    let mut dominio_resolvido = [0; 512];
    let mut tamanho_dominio = dominio_resolvido.len() as u32;
    let mut tipo_conta = 0;
    let mut token = [0_usize;
        (size_of::<TOKEN_USER>() + SECURITY_MAX_SID_SIZE as usize).div_ceil(size_of::<usize>())];
    let mut tamanho_token = 0;
    // Valor de GetCurrentProcessToken no SDK. O pseudohandle não é fechado.
    let token_atual = -4_isize as HANDLE;
    // Os buffers têm alinhamento e espaço para a estrutura e o SID retornados.
    unsafe {
        if GetTokenInformation(
            token_atual,
            TokenUser,
            token.as_mut_ptr().cast(),
            size_of_val(&token) as u32,
            &mut tamanho_token,
        ) == 0
        {
            return false;
        }
        let sid_atual = (*token.as_ptr().cast::<TOKEN_USER>()).User.Sid;
        IsWellKnownSid(sid_atual, WinLocalSystemSid) == 0
            && LookupAccountNameW(
                null(),
                conta.as_ptr(),
                sid_sessao.as_mut_ptr().cast(),
                &mut tamanho_sid,
                dominio_resolvido.as_mut_ptr(),
                &mut tamanho_dominio,
                &mut tipo_conta,
            ) != 0
            && EqualSid(sid_atual, sid_sessao.as_mut_ptr().cast()) != 0
    }
}

pub(super) fn local_app_data() -> Option<PathBuf> {
    let mut caminho = null_mut();
    // Usa o perfil do processo, sem depender do ambiente herdado do MSI.
    unsafe {
        let hr = SHGetKnownFolderPath(
            &FOLDERID_LocalAppData,
            KF_FLAG_DONT_VERIFY as u32,
            null_mut(),
            &mut caminho,
        );
        if caminho.is_null() {
            return None;
        }
        let local = if hr >= 0 {
            let len = (0..).take_while(|&i| *caminho.add(i) != 0).count();
            Some(PathBuf::from(OsString::from_wide(
                std::slice::from_raw_parts(caminho, len),
            )))
        } else {
            None
        };
        CoTaskMemFree(caminho.cast());
        local
    }
}
