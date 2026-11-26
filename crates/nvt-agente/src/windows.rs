//! Transporte IPC e persistência do agente no Windows.
use std::{
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Component, Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use nvt_detect::tela::{TelaDoSistema, ler_tela_primaria};
use nvt_engine::Portao;
use nvt_ipc::{Fato, FatoTela, RefreshRacional, Requisicao, Resposta, SettingUsuario};
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

use crate::{
    Agente, Controle, EscritaUsuario, Estado, LeituraUsuario, Persistencia, Transporte, ValorLido,
};

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
    if sessao == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "sessão não interativa",
        ));
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

/// Entrada Windows para L3, com a fachada interativa fornecida pelo chamador.
pub fn executar(
    armazem: impl LeituraUsuario + EscritaUsuario,
    restaurar: bool,
) -> io::Result<Controle> {
    let sessao = sessao_atual()?;
    let _instancia = if restaurar {
        // O MSI coordena com a instância já ativa pela mesma trava de perfil.
        None
    } else {
        let Some(instancia) = Instancia::abrir()? else {
            return Ok(Controle::Encerrar);
        };
        Some(instancia)
    };
    let local: OsString = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA indisponível"))?;
    let perfil = Perfil::abrir(Path::new(&local))?;
    let mut agente = Agente::novo(armazem, Pipe, Portao::do_sistema(), sessao)?;
    let limite_trava = Instant::now() + Duration::from_secs(15);
    let mut proxima_lista_sessoes = Instant::now();
    loop {
        let agora = Instant::now();
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
            Err(e) => return Err(e),
        }
        thread::sleep(Duration::from_secs(2));
    }
}

pub fn ler_tela() -> Fato<FatoTela> {
    ler_tela_primaria(&TelaDoSistema).map_or(Fato::Desconhecido, |tela| {
        Fato::Conhecido(FatoTela {
            alvo: tela.caminho_monitor,
            atual: RefreshRacional {
                numerador: tela.refresh_atual.numerador,
                denominador: tela.refresh_atual.denominador.get(),
            },
            maximo: RefreshRacional {
                numerador: tela.refresh_max_hz,
                denominador: 1,
            },
        })
    })
}

/// Fachada provisória do L2. A detecção de tela é disponível; os destinos
/// interativos serão fornecidos pela fachada do Windows na integração.
pub struct SomenteTela;

impl LeituraUsuario for SomenteTela {
    fn ler(&self, destino: &SettingUsuario) -> io::Result<ValorLido> {
        if let SettingUsuario::Refresh { alvo } = destino {
            if let Fato::Conhecido(tela) = ler_tela() {
                if tela.alvo == *alvo {
                    return Ok(ValorLido {
                        valor: nvt_core::usuario::ValorUsuario::Refresh(crate::valores::taxa(
                            tela.atual,
                        )),
                        registro_dx_presente: false,
                        outros_tokens_dx: false,
                    });
                }
            }
        }
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "fachada interativa indisponível",
        ))
    }

    fn tela(&self) -> Fato<FatoTela> {
        ler_tela()
    }
}

impl EscritaUsuario for SomenteTela {
    fn escrever(
        &mut self,
        _: &SettingUsuario,
        _: &ValorLido,
        _: &ValorLido,
        _: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "fachada interativa indisponível",
        ))
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
