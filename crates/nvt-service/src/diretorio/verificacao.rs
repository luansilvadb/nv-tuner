//! Verificação de segurança do diretório: dono, DACL, reparse points e
//! vínculos internos.

use super::*;

/// Abre `p` sem seguir reparse points (o handle é do próprio link, se for um).
pub(super) fn abrir_item(p: &Path) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(p)
}

pub(super) fn info(f: &File) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
    // SAFETY: estrutura POD preenchida pela chamada; handle válido.
    let mut i: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(f.as_raw_handle() as HANDLE, &mut i) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(i)
}

pub(crate) fn sid_texto(sid: PSID) -> String {
    let mut s: windows_sys::core::PWSTR = null_mut();
    // SAFETY: `sid` válido; `s` recebe memória LocalAlloc, liberada abaixo.
    if unsafe { ConvertSidToStringSidW(sid, &mut s) } == 0 || s.is_null() {
        return "SID desconhecido".into();
    }
    // SAFETY: `s` é UTF-16 terminado em zero.
    let txt = unsafe {
        let mut n = 0;
        while *s.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(s, n))
    };
    // SAFETY: alocado por ConvertSidToStringSidW.
    unsafe { LocalFree(s.cast()) };
    txt
}

pub(super) fn privilegiado(sid: PSID) -> bool {
    // SAFETY: `sid` aponta para um SID válido.
    unsafe {
        IsWellKnownSid(sid, WinLocalSystemSid) != 0
            || IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
    }
}

/// Confere dono e DACL de um item aberto.
pub(super) fn conferir_seguranca(f: &File, p: &Path, problemas: &mut Vec<Problema>, privado: bool) {
    let mut dono: PSID = null_mut();
    let mut dacl: *mut ACL = null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: handle com READ_CONTROL; saídas apontam para dentro de `sd`.
    let r = unsafe {
        GetSecurityInfo(
            f.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut dono,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut sd,
        )
    };
    if r != 0 {
        problemas.push(Problema::Io(
            p.to_owned(),
            io::Error::from_raw_os_error(r as i32).to_string(),
        ));
        return;
    }
    if dono.is_null() || !privilegiado(dono) {
        let s = if dono.is_null() {
            "ninguém".into()
        } else {
            sid_texto(dono)
        };
        problemas.push(Problema::Dono(p.to_owned(), s));
    }
    if privado {
        let mut controle = 0;
        let mut revisao = 0;
        // SAFETY: sd permanece válido até LocalFree.
        if unsafe { GetSecurityDescriptorControl(sd, &mut controle, &mut revisao) } == 0
            || controle & SE_DACL_PROTECTED == 0
        {
            problemas.push(Problema::Io(
                p.to_owned(),
                "DACL privada sem proteção de herança".into(),
            ));
        }
    }
    if dacl.is_null() {
        problemas.push(Problema::DaclNula(p.to_owned()));
    } else {
        // SAFETY: `dacl` válido enquanto `sd` existir.
        let n = u32::from(unsafe { (*dacl).AceCount });
        for i in 0..n {
            let mut ace: *mut core::ffi::c_void = null_mut();
            // SAFETY: índice dentro de AceCount.
            if unsafe { GetAce(dacl, i, &mut ace) } == 0 || ace.is_null() {
                if privado {
                    problemas.push(Problema::Io(p.to_owned(), "ACE privada ilegível".into()));
                }
                continue;
            }
            // SAFETY: toda ACE começa com ACE_HEADER.
            let cab = unsafe { *(ace as *const ACE_HEADER) };
            match cab.AceType {
                ACE_PERMITIR | ACE_PERMITIR_CALLBACK => {
                    // SAFETY: os dois tipos têm o layout de ACCESS_ALLOWED_ACE.
                    let a = ace as *const ACCESS_ALLOWED_ACE;
                    let mascara = unsafe { (*a).Mask };
                    let sid: PSID = unsafe { (&raw const (*a).SidStart) as PSID };
                    if (!privado && mascara & DIREITOS_ESCRITA == 0) || privilegiado(sid) {
                        continue;
                    }
                    // CREATOR OWNER só herdável vira o dono do filho (SYSTEM
                    // ou Administradores, pois só eles criam itens aqui).
                    let herdavel_so = u32::from(cab.AceFlags) & INHERIT_ONLY_ACE != 0;
                    // SAFETY: `sid` válido.
                    if !privado
                        && herdavel_so
                        && unsafe { IsWellKnownSid(sid, WinCreatorOwnerSid) } != 0
                    {
                        continue;
                    }
                    problemas.push(Problema::Escrita(p.to_owned(), sid_texto(sid)));
                }
                ACE_NEGAR | ACE_NEGAR_CALLBACK => {}
                t => problemas.push(Problema::AceDesconhecida(p.to_owned(), t)),
            }
        }
    }
    // SAFETY: alocado por GetSecurityInfo.
    unsafe { LocalFree(sd) };
}

/// Caminho de `dir` relativo ao volume, em minúsculas e sem barra final
/// (`\programdata\nv-tuner`), no formato de `FindFirstFileNameW`.
pub(super) fn caminho_no_volume(dir: &Path) -> Option<String> {
    let c = fs::canonicalize(dir).ok()?;
    let s = c.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    // Só caminhos com letra de unidade (`C:\...`); UNC fica de fora.
    let resto = s.get(2..).filter(|_| s.as_bytes().get(1) == Some(&b':'))?;
    Some(resto.trim_end_matches('\\').to_lowercase())
}

/// Todos os nomes (hard links) de `arquivo` estão dentro do diretório cujo
/// caminho no volume é `prefixo`? Em erro, `false`.
pub(super) fn vinculos_internos(arquivo: &Path, prefixo: &str) -> bool {
    let nome = utf16(arquivo);
    let mut buf = vec![0u16; 1024];
    let dentro = |buf: &[u16], n: u32| {
        let n = (n as usize).min(buf.len());
        let s = String::from_utf16_lossy(&buf[..n]);
        let s = s.trim_end_matches('\0').to_lowercase();
        s.strip_prefix(prefixo).is_some_and(|r| r.starts_with('\\'))
    };
    let mut n = buf.len() as u32;
    // SAFETY: nome terminado em zero; `buf` com `n` posições.
    let h = unsafe { FindFirstFileNameW(nome.as_ptr(), 0, &mut n, buf.as_mut_ptr()) };
    if h == INVALID_HANDLE_VALUE {
        return false;
    }
    let mut ok = dentro(&buf, n);
    let mut vistos = 1;
    while ok && vistos < 64 {
        n = buf.len() as u32;
        // SAFETY: handle de busca válido; `buf` com `n` posições.
        if unsafe { FindNextFileNameW(h, &mut n, buf.as_mut_ptr()) } == 0 {
            break;
        }
        ok = dentro(&buf, n);
        vistos += 1;
    }
    if vistos >= 64 {
        ok = false;
    }
    // SAFETY: handle de busca aberto acima.
    unsafe { FindClose(h) };
    ok
}

/// Verifica `dir` (que precisa existir) e todo o conteúdo, sem seguir
/// reparse points. Lista vazia = seguro.
///
/// Item interno que some durante a verificação (rotação de log,
/// `estado.json.tmp` renomeado pelo motor) não é problema: o que não existe
/// não pode ser usado contra o serviço. Só a raiz sumida é erro.
pub fn verificar(dir: &Path) -> Vec<Problema> {
    verificar_com(dir, |_| {})
}

/// [`verificar`] com um gancho chamado antes de abrir cada item (testes de
/// corrida).
pub(super) fn verificar_com(dir: &Path, mut antes_de_abrir: impl FnMut(&Path)) -> Vec<Problema> {
    let prefixo_volume = caminho_no_volume(dir);
    let mut problemas = Vec::new();
    let mut pendentes = vec![(dir.to_path_buf(), true)];
    let mut vistos = 0usize;
    while let Some((p, raiz)) = pendentes.pop() {
        vistos += 1;
        if vistos > ITENS_MAX {
            problemas.push(Problema::Demais);
            break;
        }
        antes_de_abrir(&p);
        let f = match abrir_item(&p) {
            Ok(f) => f,
            Err(e) if !raiz && e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => {
                problemas.push(Problema::Io(p, e.to_string()));
                continue;
            }
        };
        let i = match info(&f) {
            Ok(i) => i,
            Err(e) => {
                problemas.push(Problema::Io(p, e.to_string()));
                continue;
            }
        };
        let eh_dir = i.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
        if i.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            // Não entra nem confere o destino.
            problemas.push(Problema::ReparsePoint(p));
            continue;
        }
        if raiz && !eh_dir {
            problemas.push(Problema::NaoEDiretorio(p));
            continue;
        }
        // Vínculos internos (o motor publica o snapshot por hard link a
        // partir de um temporário) são aceitos; só um nome fora do
        // diretório é problema.
        if !eh_dir
            && i.nNumberOfLinks > 1
            && !prefixo_volume
                .as_deref()
                .is_some_and(|pre| vinculos_internos(&p, pre))
        {
            problemas.push(Problema::VariosVinculos(p.clone()));
        }
        conferir_seguranca(&f, &p, &mut problemas, false);
        drop(f);
        if eh_dir {
            match fs::read_dir(&p) {
                Ok(it) => {
                    for e in it {
                        match e {
                            Ok(e) => pendentes.push((e.path(), false)),
                            Err(e) => problemas.push(Problema::Io(p.clone(), e.to_string())),
                        }
                    }
                }
                Err(e) if !raiz && e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => problemas.push(Problema::Io(p.clone(), e.to_string())),
            }
        }
    }
    problemas
}
