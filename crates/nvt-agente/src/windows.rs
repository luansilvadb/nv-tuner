//! Transporte IPC e persistência do agente no Windows.
use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

use nvt_engine::Portao;
use nvt_ipc::{Requisicao, Resposta};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, ERROR_SHARING_VIOLATION, GetLastError, HANDLE,
    },
    Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, MOVEFILE_REPLACE_EXISTING,
        MOVEFILE_WRITE_THROUGH, MoveFileExW,
    },
    System::{
        RemoteDesktop::{
            ProcessIdToSessionId, WTS_SESSION_INFOW, WTSEnumerateSessionsW, WTSFreeMemory,
        },
        Threading::{CreateMutexW, GetCurrentProcessId},
    },
};

use crate::{Agente, Controle, Estado, Instalacao, Persistencia, Transporte};

mod ajustes;
mod janela;
mod perfil_interativo;

use ajustes::Ajustes;
pub(crate) use ajustes::refresh_divergente;

pub struct Pipe;

impl Transporte for Pipe {
    fn trocar(&mut self, requisicao: &Requisicao) -> io::Result<Resposta> {
        use nvt_ipc::pipe::{Cliente, ConfigCliente};
        let mut cliente = Cliente::conectar(
            nvt_ipc::NOME_PIPE,
            &ConfigCliente {
                timeout_leitura: Duration::from_secs(5),
                ..ConfigCliente::default()
            },
        )
        .map_err(io::Error::other)?;
        cliente.requisitar(requisicao).map_err(io::Error::other)
    }
}

/// Leitura do executável e da entrada Run; erros de leitura contam como presente.
struct InstalacaoWindows;

impl Instalacao for InstalacaoWindows {
    fn presente(&self) -> bool {
        use winreg::{
            RegKey,
            enums::{HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_WOW64_64KEY},
        };
        let nao_encontrado = |e: &io::Error| e.kind() == io::ErrorKind::NotFound;
        let exe = std::env::current_exe().and_then(|exe| fs::symlink_metadata(exe).map(|_| ()));
        let run = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
                KEY_QUERY_VALUE | KEY_WOW64_64KEY,
            )
            .and_then(|chave| chave.get_raw_value("nv-tuner-agente").map(|_| ()));
        !exe.as_ref().is_err_and(nao_encontrado) && !run.as_ref().is_err_and(nao_encontrado)
    }
}

fn wide(texto: &OsStr) -> Vec<u16> {
    texto.encode_wide().chain([0]).collect()
}

struct Instancia(HANDLE);

impl Instancia {
    fn abrir() -> io::Result<Option<Self>> {
        let nome = wide(OsStr::new(r"Local\nv-tuner-agente"));
        // O handle mantém o objeto nomeado vivo durante toda a instância.
        let (handle, erro) = unsafe {
            let handle = CreateMutexW(std::ptr::null(), 0, nome.as_ptr());
            (handle, GetLastError())
        };
        if handle.is_null() {
            return Err(io::Error::from_raw_os_error(erro as i32));
        }
        let instancia = Self(handle);
        Ok((erro != ERROR_ALREADY_EXISTS).then_some(instancia))
    }
}

impl Drop for Instancia {
    fn drop(&mut self) {
        // O handle pertence exclusivamente à guarda.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn sessao_atual() -> io::Result<u32> {
    let mut sessao = 0;
    // Consulta apenas o ID da própria sessão, sem abrir processos.
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut sessao) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(sessao)
}

fn sessoes_existentes() -> Option<BTreeSet<u32>> {
    let mut sessoes: *mut WTS_SESSION_INFOW = std::ptr::null_mut();
    let mut total = 0;
    // Enumera os IDs das sessões locais. A API aloca o buffer em caso de sucesso.
    let ok = unsafe { WTSEnumerateSessionsW(std::ptr::null_mut(), 0, 1, &mut sessoes, &mut total) };
    if ok == 0 || sessoes.is_null() {
        return None;
    }
    // O buffer contém exatamente os registros informados pela API.
    let ids = unsafe {
        let ids = std::slice::from_raw_parts(sessoes, total as usize)
            .iter()
            .map(|sessao| sessao.SessionId)
            .collect();
        WTSFreeMemory(sessoes.cast());
        ids
    };
    Some(ids)
}

const MAX_ESTADO: u64 = 4 * 1024 * 1024;

fn verificar(file: &File, diretorio: bool) -> io::Result<()> {
    let meta = file.metadata()?;
    if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || meta.is_dir() != diretorio {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "tipo ou reparse point no estado local",
        ));
    }
    Ok(())
}

fn opcoes_arquivo() -> OpenOptions {
    let mut opcoes = OpenOptions::new();
    opcoes
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .share_mode(0);
    opcoes
}

struct Perfil {
    caminho: PathBuf,
    _diretorios: Vec<File>,
}

impl Perfil {
    fn abrir(local: &Path) -> io::Result<Self> {
        use std::path::Prefix;
        let mut componentes = local.components();
        let local_valido = matches!(componentes.next(),
            Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
            && componentes.next() == Some(Component::RootDir)
            && componentes.all(|c| matches!(c, Component::Normal(_)));
        if !local_valido {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "LOCALAPPDATA deve ser caminho local absoluto",
            ));
        }

        let caminho = local.join("nv-tuner").join("usuario");
        let mut diretorios = Vec::new();
        // Abra da raiz para o destino. Sem compartilhamento de DELETE, os
        // componentes já verificados ficam fixos enquanto seus filhos são usados.
        for dir in caminho.ancestors().collect::<Vec<_>>().into_iter().rev() {
            if dir == caminho || Some(dir) == caminho.parent() {
                match fs::create_dir(dir) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e),
                }
            }
            let handle = OpenOptions::new()
                .access_mode(FILE_READ_ATTRIBUTES)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .open(dir)?;
            verificar(&handle, true)?;
            diretorios.push(handle);
        }
        Ok(Self {
            caminho,
            _diretorios: diretorios,
        })
    }

    fn travar(&self) -> io::Result<EstadoLocal<'_>> {
        let trava = opcoes_arquivo()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.caminho.join("trava"))?;
        verificar(&trava, false)?;
        Ok(EstadoLocal {
            perfil: self,
            _trava: trava,
        })
    }

    fn registrar_bloqueio(&self, motivo: &str) -> io::Result<()> {
        let mut arquivo = opcoes_arquivo()
            .append(true)
            .create(true)
            .open(self.caminho.join("restauracao-msi.log"))?;
        verificar(&arquivo, false)?;
        writeln!(arquivo, "Restauração do usuário bloqueada: {motivo}")
    }
}

struct EstadoLocal<'a> {
    perfil: &'a Perfil,
    _trava: File,
}

impl Persistencia for EstadoLocal<'_> {
    fn carregar(&self) -> io::Result<Estado> {
        let arquivo = self.perfil.caminho.join("estado.json");
        let f = match opcoes_arquivo().read(true).open(arquivo) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Estado::default()),
            Err(e) => return Err(e),
        };
        verificar(&f, false)?;
        let mut dados = Vec::new();
        f.take(MAX_ESTADO + 1).read_to_end(&mut dados)?;
        if dados.len() as u64 > MAX_ESTADO {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "estado local excede o limite",
            ));
        }
        Ok(serde_json::from_slice(&dados)?)
    }

    fn salvar(&self, estado: &Estado) -> io::Result<()> {
        let dados = serde_json::to_vec(estado)?;
        if dados.len() as u64 > MAX_ESTADO {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "estado local excede o limite",
            ));
        }
        let pasta = &self.perfil.caminho;
        let temporario = pasta.join("estado.tmp");
        // Um temporário abandonado pode ser removido pelo nome. A abertura
        // seguinte é exclusiva e jamais reutiliza um arquivo preexistente.
        match fs::remove_file(&temporario) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let mut f = opcoes_arquivo()
            .write(true)
            .create_new(true)
            .open(&temporario)?;
        f.write_all(&dados)?;
        f.sync_all()?;
        drop(f);
        let origem = wide(temporario.as_os_str());
        let destino = wide(pasta.join("estado.json").as_os_str());
        // Mesmo diretório e volume. A operação substitui a entrada de destino.
        if unsafe {
            MoveFileExW(
                origem.as_ptr(),
                destino.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

fn ocupado(e: &io::Error) -> bool {
    e.raw_os_error() == Some(ERROR_SHARING_VIOLATION as i32)
}

pub fn executar(restaurar: bool) -> io::Result<Controle> {
    let sessao = sessao_atual()?;
    if sessao == 0 || (restaurar && !perfil_interativo::conta_da_sessao(sessao)) {
        return Ok(Controle::Encerrar);
    }
    let local = perfil_interativo::local_app_data();
    let Some(local) = local.filter(|p| p.is_dir()) else {
        return if restaurar {
            Ok(Controle::Encerrar)
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "perfil interativo indisponível",
            ))
        };
    };
    let _instancia = if restaurar {
        // O MSI coordena com a instância já ativa pela mesma trava de perfil.
        None
    } else {
        let Some(instancia) = Instancia::abrir()? else {
            return Ok(Controle::Encerrar);
        };
        Some(instancia)
    };
    let _janela = if restaurar {
        None
    } else {
        Some(janela::Janela::criar()?)
    };
    let perfil = Perfil::abrir(&local)?;
    let controle = executar_ciclo(&perfil, sessao, restaurar);
    if restaurar {
        let motivo = match &controle {
            Ok(Controle::Bloqueado(motivo)) => Some(motivo.clone()),
            Err(erro) => Some(erro.to_string()),
            _ => None,
        };
        if let Some(motivo) = motivo {
            perfil.registrar_bloqueio(&motivo).map_err(|erro| {
                io::Error::other(format!(
                    "{motivo} (falha ao registrar o bloqueio no perfil: {erro})"
                ))
            })?;
        }
    }
    controle
}

fn executar_ciclo(perfil: &Perfil, sessao: u32, restaurar: bool) -> io::Result<Controle> {
    let mut agente = Agente::novo(Ajustes::novo()?, Pipe, Portao::do_sistema(), sessao)?;
    agente.definir_ambiente_jogo(crate::config_jogo::AmbienteJogo::do_usuario());
    let limite_trava = Instant::now() + Duration::from_secs(15);
    let mut proxima_lista_sessoes = Instant::now();
    loop {
        let agora = Instant::now();
        if janela::encerrando() || (!restaurar && agente.desinstalado(agora, &InstalacaoWindows)) {
            return Ok(Controle::Encerrar);
        }
        if agora >= proxima_lista_sessoes {
            agente.definir_sessoes_existentes(sessoes_existentes());
            proxima_lista_sessoes = agora + Duration::from_secs(60);
        }
        match perfil.travar() {
            Ok(p) => match agente.passo(&p, agora, restaurar) {
                Ok(Controle::Continuar) => {}
                Err(_) if !restaurar => {}
                resposta => return resposta,
            },
            Err(e) if ocupado(&e) && (!restaurar || agora < limite_trava) => {}
            Err(e) if ocupado(&e) => {
                return Ok(Controle::Bloqueado(
                    "trava do perfil em uso pelo agente".into(),
                ));
            }
            Err(e) => return Err(e),
        }
        janela::esperar()?;
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn trava_do_perfil_serializa_instancias_e_estado_e_substituido() {
        let local = std::env::temp_dir().join(format!("nvt-agente-teste-{}", std::process::id()));
        fs::create_dir(&local).unwrap();
        let perfil = Perfil::abrir(&local).unwrap();
        let trava = perfil.travar().unwrap();
        assert!(ocupado(
            &Perfil::abrir(&local).unwrap().travar().err().unwrap()
        ));
        trava.salvar(&Estado::default()).unwrap();
        trava
            .salvar(&Estado {
                geracao_vista: 2,
                ..Estado::default()
            })
            .unwrap();
        drop(trava);
        let outro = Perfil::abrir(&local).unwrap();
        assert_eq!(outro.travar().unwrap().carregar().unwrap().geracao_vista, 2);
        drop(outro);
        drop(perfil);
        for arquivo in ["estado.json", "trava"] {
            fs::remove_file(local.join("nv-tuner").join("usuario").join(arquivo)).unwrap();
        }
        fs::remove_dir(local.join("nv-tuner").join("usuario")).unwrap();
        fs::remove_dir(local.join("nv-tuner")).unwrap();
        fs::remove_dir(local).unwrap();
    }
}
