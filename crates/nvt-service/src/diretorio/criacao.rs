//! Criação segura de pastas e arquivos privados, gravação atômica e preparo
//! do diretório com quarentena.

use super::*;

/// Cria `dir` (o pai precisa existir) com [`SDDL_ESTADO`]. Sem privilégio
/// para definir o dono, usa o dono padrão do processo.
pub fn criar_seguro(dir: &Path) -> io::Result<()> {
    match criar_com(dir, SDDL_ESTADO) {
        Err(e) if e.raw_os_error() == Some(ERROR_INVALID_OWNER as i32) => {
            criar_com(dir, SDDL_ESTADO_SEM_DONO)
        }
        r => r,
    }
}

/// Prepara uma subpasta privada do diretório de estado (ex.:
/// `%ProgramData%\nv-tuner\atualizacao`, spec U-20): cria, se faltar, com
/// dono Administradores e DACL protegida só SYSTEM/Administradores (sem a
/// leitura de Usuários da raiz) e confere, sem seguir reparse points, a
/// subpasta e o conteúdo ([`verificar`], com o limite de itens de
/// [`Problema::Demais`]) e a DACL privada da própria subpasta. Não corrige
/// nem põe em quarentena: qualquer problema é recusa. Devolve `true` se criou.
pub fn preparar_subpasta_privada(dir: &Path) -> Result<bool, Vec<Problema>> {
    const SDDL: &str = "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";
    const SDDL_SEM_DONO: &str = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";
    let io_erro = |e: io::Error| vec![Problema::Io(dir.to_owned(), e.to_string())];
    let criada = match fs::symlink_metadata(dir) {
        Ok(_) => false,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            match criar_com(dir, SDDL) {
                Err(e) if e.raw_os_error() == Some(ERROR_INVALID_OWNER as i32) => {
                    criar_com(dir, SDDL_SEM_DONO)
                }
                r => r,
            }
            .map_err(io_erro)?;
            true
        }
        Err(e) => return Err(io_erro(e)),
    };
    let mut problemas = verificar(dir);
    if problemas.is_empty() {
        // `verificar` já recusou reparse point na raiz; aqui só a DACL privada.
        match abrir_item(dir) {
            Ok(f) => conferir_seguranca(&f, dir, &mut problemas, true),
            Err(e) => problemas.push(Problema::Io(dir.to_owned(), e.to_string())),
        }
    }
    if problemas.is_empty() {
        Ok(criada)
    } else {
        Err(problemas)
    }
}

pub(super) fn criar_com(dir: &Path, sddl: &str) -> io::Result<()> {
    com_descritor(sddl, |sa| {
        let nome = utf16(dir);
        // SAFETY: os ponteiros são válidos durante a chamada.
        if unsafe { CreateDirectoryW(nome.as_ptr(), sa) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    })
}

pub(super) fn com_descritor<T>(
    sddl: &str,
    criar: impl FnOnce(&SECURITY_ATTRIBUTES) -> io::Result<T>,
) -> io::Result<T> {
    let sddl16: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: SDDL terminado em zero; `sd` recebe memória LocalAlloc.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl16.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd,
        bInheritHandle: 0,
    };
    let resultado = criar(&sa);
    // SAFETY: alocado pela conversão de SDDL.
    unsafe { LocalFree(sd) };
    resultado
}

/// Cria um arquivo novo já com DACL protegida, antes de escrever dados.
pub fn criar_arquivo_privado(p: &Path) -> io::Result<File> {
    com_descritor("D:P(A;;FA;;;SY)(A;;FA;;;BA)", |sa| {
        let nome = utf16(p);
        // SAFETY: nome e descritor vivem durante a chamada. CREATE_NEW recusa
        // qualquer entrada existente, inclusive um link.
        let h = unsafe {
            CreateFileW(
                nome.as_ptr(),
                GENERIC_WRITE,
                0,
                sa,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: handle válido e de posse exclusiva deste File.
        Ok(unsafe { File::from_raw_handle(h) })
    })
}

pub fn abrir_arquivo_privado(p: &Path) -> io::Result<File> {
    let f = abrir_sem_reparse(p, OpenOptions::new().read(true))?;
    let mut problemas = Vec::new();
    conferir_seguranca(&f, p, &mut problemas, true);
    if problemas.is_empty() {
        Ok(f)
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            resumo(&problemas),
        ))
    }
}

/// Lê o JSON privado `p` ([`abrir_arquivo_privado`]); ausente = padrão.
pub(crate) fn ler_json_privado<T: Default + serde::de::DeserializeOwned>(
    p: &Path,
) -> io::Result<T> {
    match abrir_arquivo_privado(p) {
        Ok(f) => Ok(serde_json::from_reader(io::BufReader::new(f))?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e),
    }
}

/// Grava `dir\nome` de forma atômica: temporário novo e privado
/// ([`criar_arquivo_privado`], até 100 nomes), `sync_all` e renomeação por
/// cima do destino; em erro, o temporário é apagado.
pub(crate) fn gravar_atomico_privado(dir: &Path, nome: &str, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let (tmp, mut f) = (0..100)
        .find_map(|tentativa| {
            let tmp = dir.join(format!("{nome}.{}-{tentativa}.tmp", std::process::id()));
            match criar_arquivo_privado(&tmp) {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => None,
                r => Some(r.map(|f| (tmp, f))),
            }
        })
        .ok_or_else(|| io::Error::other(format!("sem nome temporário livre para {nome}")))??;
    let gravado = f.write_all(bytes).and_then(|()| f.sync_all());
    drop(f);
    let r = gravado.and_then(|()| fs::rename(&tmp, dir.join(nome)));
    if r.is_err() {
        let _ = fs::remove_file(tmp);
    }
    r
}

pub(super) fn quarentena_para(dir: &Path) -> PathBuf {
    let nome = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "estado".into());
    dir.with_file_name(format!(
        "{nome}.inseguro-{}-{}",
        crate::nucleo::agora_unix(),
        std::process::id()
    ))
}

/// Garante que `dir` é seguro antes do uso (ver o módulo). `Err` = o
/// processo deve recusar operar com o diretório.
pub fn preparar(dir: &Path, modo: Modo) -> Result<Situacao, String> {
    let existe = match fs::symlink_metadata(dir) {
        Ok(_) => true,
        Err(e) if e.kind() == io::ErrorKind::NotFound => false,
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    if !existe {
        if modo == Modo::Avisar {
            // Sem privilégio, criar aqui deixaria o usuário como dono.
            return Ok(Situacao::Ausente);
        }
        if let Some(pai) = dir.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(pai).map_err(|e| format!("{}: {e}", pai.display()))?;
        }
        criar_seguro(dir).map_err(|e| format!("criar {}: {e}", dir.display()))?;
        let problemas = verificar(dir);
        if problemas.is_empty() {
            return Ok(Situacao::Criado);
        }
        return Err(format!(
            "diretório de estado recém-criado continua inseguro: {}",
            resumo(&problemas)
        ));
    }
    let mut problemas = verificar(dir);
    // Falha de leitura pode ser passageira (item trocado no meio da
    // verificação): confere de novo antes de qualquer correção.
    if problemas.iter().any(|x| matches!(x, Problema::Io(..))) {
        problemas = verificar(dir);
    }
    if problemas.is_empty() {
        return Ok(Situacao::Seguro);
    }
    if modo == Modo::Avisar {
        return Ok(Situacao::Inseguro(problemas));
    }
    // Só o dono de itens internos está errado (a raiz é segura e nenhuma
    // DACL concede escrita a outros): foram criados por um administrador
    // elevado cujo dono padrão é o próprio usuário. Basta tomar posse.
    let so_dono_interno = problemas
        .iter()
        .all(|x| matches!(x, Problema::Dono(p, _) if p != dir));
    if so_dono_interno {
        let mut falhas = Vec::new();
        for x in &problemas {
            if let Problema::Dono(p, _) = x
                && let Err(e) = tomar_posse(p)
            {
                falhas.push(format!("{}: {e}", p.display()));
            }
        }
        if falhas.is_empty() && verificar(dir).is_empty() {
            return Ok(Situacao::PosseTomada {
                itens: problemas.len(),
            });
        }
    }
    let quarentena = quarentena_para(dir);
    // Renomear não segue junções: move o próprio link, se `dir` for um.
    fs::rename(dir, &quarentena).map_err(|e| {
        format!(
            "diretório de estado inseguro ({}) e não foi possível movê-lo para {}: {e}",
            resumo(&problemas),
            quarentena.display()
        )
    })?;
    criar_seguro(dir).map_err(|e| {
        format!(
            "diretório de estado inseguro movido para {}, mas recriá-lo falhou: {e}",
            quarentena.display()
        )
    })?;
    let depois = verificar(dir);
    if !depois.is_empty() {
        return Err(format!(
            "diretório de estado recriado continua inseguro: {}",
            resumo(&depois)
        ));
    }
    // O snapshot original (se havia) ficou na quarentena e não é confiável:
    // sem este marcador, a próxima aplicação capturaria como "original" um
    // driver que pode já ter mudanças do app. Com ele, o motor avisa e trata
    // o snapshot novo como base pós-quarentena.
    nvt_engine::Armazem::marcar_snapshot_perdido(
        dir,
        &format!(
            "quarentena em {} ({})",
            quarentena.display(),
            resumo(&problemas)
        ),
    )
    .map_err(|e| {
        format!(
            "diretório de estado movido para {}, mas marcar o snapshot original como \
             perdido falhou: {e}",
            quarentena.display()
        )
    })?;
    Ok(Situacao::Corrigido {
        quarentena,
        problemas,
    })
}
