//! Segurança do diretório de estado (`%ProgramData%\nv-tuner`).
//!
//! Em `C:\ProgramData`, usuários comuns podem criar subpastas e viram donos
//! delas. Se alguém pré-criar o diretório de estado (ou plantar junções,
//! links simbólicos ou arquivos nele), o serviço, como SYSTEM, leria um
//! snapshot forjado ou escreveria logs onde o atacante quisesse. Por isso,
//! antes de usar o diretório, [`preparar`]:
//!
//! 1. cria o diretório, se faltar, com descritor próprio ([`SDDL_ESTADO`]):
//!    dono Administradores, DACL protegida (sem herança), SYSTEM e
//!    Administradores com controle total e Usuários só com leitura;
//! 2. percorre o diretório e todo o conteúdo **sem seguir reparse points**
//!    e confere, em cada item: não é reparse point (junção, link simbólico
//!    etc.), não tem mais de um vínculo (hard link), o dono é SYSTEM ou
//!    Administradores e a DACL não concede nenhum direito de escrita a
//!    outras contas;
//! 3. se o único problema for o dono de itens internos (a raiz é segura e
//!    nenhuma DACL concede escrita a outros), Administradores toma posse
//!    deles: são arquivos criados por um administrador elevado cujo dono
//!    padrão era o próprio usuário (ver [`definir_dono_padrao`]);
//! 4. qualquer outro problema: em [`Modo::Exigir`] move o diretório inteiro
//!    para uma quarentena ao lado (`nv-tuner.inseguro-<unix>-<pid>`, nada é
//!    apagado), cria um novo com o descritor seguro e grava nele o marcador
//!    `snapshot-perdido.json` (o snapshot original ficou na quarentena: o
//!    próximo é só uma base pós-quarentena, com aviso do motor); se algo
//!    disso falhar, devolve erro e quem chama recusa operar.
//!
//! Item interno que some durante a verificação (`NotFound`) não é problema,
//! e uma verificação com falha de leitura é repetida antes de corrigir.
//!
//! O conteúdo de um diretório que já foi gravável por usuários comuns não é
//! confiável (um `snapshot-original.json` forjado seria restaurado como se
//! fosse o original), por isso a correção é a quarentena, e não só tomar
//! posse e reescrever a DACL.
//!
//! [`abrir_sem_reparse`] abre arquivos com `FILE_FLAG_OPEN_REPARSE_POINT` e
//! recusa o que não for um arquivo comum com um único vínculo.

use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INVALID_OWNER, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_FILE_OBJECT, SetSecurityInfo,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, CreateWellKnownSid, DACL_SECURITY_INFORMATION, GetAce,
    GetSecurityDescriptorControl, INHERIT_ONLY_ACE, IsWellKnownSid, OWNER_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, SECURITY_MAX_SID_SIZE,
    SetTokenInformation, TOKEN_ADJUST_DEFAULT, TOKEN_OWNER, TOKEN_QUERY, TokenOwner,
    WinBuiltinAdministratorsSid, WinCreatorOwnerSid, WinLocalSystemSid,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CREATE_NEW, CreateDirectoryW, CreateFileW,
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FindClose, FindFirstFileNameW,
    FindNextFileNameW, GetFileInformationByHandle,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

mod criacao;
mod posse;
mod verificacao;

pub(crate) use criacao::*;
pub(crate) use posse::*;
pub(crate) use verificacao::*;

/// Descritor do diretório de estado (o mesmo do instalador, com dono).
pub const SDDL_ESTADO: &str = "O:BAD:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)";
/// Igual, sem dono (fica o dono padrão do processo, SYSTEM no serviço).
const SDDL_ESTADO_SEM_DONO: &str = "D:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)";

/// Limite de itens percorridos (proteção contra diretórios gigantes).
const ITENS_MAX: usize = 20_000;

const READ_CONTROL: u32 = 0x0002_0000;
const FILE_READ_ATTRIBUTES: u32 = 0x0080;
const WRITE_OWNER: u32 = 0x0008_0000;

/// Direitos que permitem alterar o item, sua ACL, dono ou conteúdo.
const DIREITOS_ESCRITA: u32 = 0x0000_0002 // FILE_WRITE_DATA / FILE_ADD_FILE
    | 0x0000_0004 // FILE_APPEND_DATA / FILE_ADD_SUBDIRECTORY
    | 0x0000_0010 // FILE_WRITE_EA
    | 0x0000_0040 // FILE_DELETE_CHILD
    | 0x0000_0100 // FILE_WRITE_ATTRIBUTES
    | 0x0001_0000 // DELETE
    | 0x0004_0000 // WRITE_DAC
    | 0x0008_0000 // WRITE_OWNER
    | 0x1000_0000 // GENERIC_ALL
    | 0x4000_0000; // GENERIC_WRITE

const ACE_PERMITIR: u8 = 0x00; // ACCESS_ALLOWED_ACE_TYPE
const ACE_NEGAR: u8 = 0x01; // ACCESS_DENIED_ACE_TYPE
const ACE_PERMITIR_CALLBACK: u8 = 0x09;
const ACE_NEGAR_CALLBACK: u8 = 0x0A;

/// Um problema encontrado num item do diretório de estado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problema {
    /// Junção, link simbólico ou outro reparse point.
    ReparsePoint(PathBuf),
    /// Arquivo com mais de um vínculo (hard link).
    VariosVinculos(PathBuf),
    /// O caminho do diretório de estado não é um diretório.
    NaoEDiretorio(PathBuf),
    /// Dono diferente de SYSTEM e Administradores (SID em texto).
    Dono(PathBuf, String),
    /// DACL nula (acesso total a todos).
    DaclNula(PathBuf),
    /// Conta (SID) com algum direito de escrita.
    Escrita(PathBuf, String),
    /// ACE de tipo não reconhecido numa DACL.
    AceDesconhecida(PathBuf, u8),
    /// Itens demais para verificar.
    Demais,
    /// Falha ao ler o item.
    Io(PathBuf, String),
}

impl fmt::Display for Problema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReparsePoint(p) => write!(f, "{} é junção/link (reparse point)", p.display()),
            Self::VariosVinculos(p) => write!(f, "{} tem mais de um vínculo", p.display()),
            Self::NaoEDiretorio(p) => write!(f, "{} não é um diretório", p.display()),
            Self::Dono(p, s) => write!(f, "{} pertence a {s}", p.display()),
            Self::DaclNula(p) => write!(f, "{} não tem DACL (acesso total)", p.display()),
            Self::Escrita(p, s) => write!(f, "{} concede escrita a {s}", p.display()),
            Self::AceDesconhecida(p, t) => {
                write!(f, "{} tem ACE de tipo desconhecido ({t})", p.display())
            }
            Self::Demais => write!(f, "itens demais no diretório de estado"),
            Self::Io(p, e) => write!(f, "{}: {e}", p.display()),
        }
    }
}

/// O que fazer com um diretório inseguro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modo {
    /// Corrige (quarentena + diretório novo) ou falha. Processos privilegiados.
    Exigir,
    /// Só cria o diretório se faltar e relata os problemas, sem mexer em
    /// nada. Processos sem privilégio (desenvolvimento): não ganham nada que
    /// já não tenham.
    Avisar,
}

/// Resultado de [`preparar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Situacao {
    /// Já existia e passou na verificação.
    Seguro,
    /// Não existia e, em [`Modo::Avisar`], não foi criado.
    Ausente,
    /// Não existia; foi criado com o descritor seguro.
    Criado,
    /// Itens internos com dono errado (e só isso): Administradores tomou posse.
    PosseTomada { itens: usize },
    /// Estava inseguro; foi movido para `quarentena` e recriado.
    Corrigido {
        quarentena: PathBuf,
        problemas: Vec<Problema>,
    },
    /// Inseguro em [`Modo::Avisar`] (nada foi alterado).
    Inseguro(Vec<Problema>),
}

/// Texto UTF-16 terminado em zero para as APIs `W` do Windows.
pub(crate) fn utf16(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}

/// Lista legível dos primeiros problemas.
pub fn resumo(problemas: &[Problema]) -> String {
    let mut s: Vec<String> = problemas.iter().take(5).map(ToString::to_string).collect();
    if problemas.len() > 5 {
        s.push(format!("e mais {}", problemas.len() - 5));
    }
    s.join("; ")
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::teste_util::DirTemp;
    use std::process::Command;

    fn juncao(link: &Path, alvo: &Path) {
        let s = Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(alvo)
            .output()
            .expect("cmd");
        assert!(s.status.success(), "mklink /J falhou: {s:?}");
    }

    #[test]
    fn juncao_e_detectada_sem_ser_seguida() {
        let t = DirTemp::criado("juncao");
        let estado = t.0.join("estado");
        let alvo = t.0.join("alvo");
        fs::create_dir_all(estado.join("logs-real")).unwrap();
        fs::create_dir_all(&alvo).unwrap();
        fs::write(alvo.join("segredo.txt"), "x").unwrap();
        juncao(&estado.join("logs"), &alvo);
        let p = verificar(&estado);
        assert!(
            p.contains(&Problema::ReparsePoint(estado.join("logs"))),
            "junção não detectada: {p:?}"
        );
        // Não seguiu a junção: nada do alvo aparece.
        assert!(
            !p.iter().any(|x| x.to_string().contains("segredo")),
            "seguiu a junção: {p:?}"
        );
    }

    #[test]
    fn diretorio_raiz_como_juncao_e_detectado() {
        let t = DirTemp::criado("raiz");
        let alvo = t.0.join("alvo");
        fs::create_dir_all(&alvo).unwrap();
        let estado = t.0.join("estado");
        juncao(&estado, &alvo);
        assert_eq!(
            verificar(&estado),
            vec![Problema::ReparsePoint(estado.clone())]
        );
        assert!(!diretorio_comum(&estado).unwrap());
        assert!(diretorio_comum(&alvo).unwrap());
    }

    #[test]
    fn hard_link_e_detectado_e_recusado_na_abertura() {
        let t = DirTemp::criado("vinculo");
        let estado = t.0.join("estado");
        fs::create_dir_all(&estado).unwrap();
        let fora = t.0.join("fora.txt");
        fs::write(&fora, "x").unwrap();
        let dentro = estado.join("estado.json");
        fs::hard_link(&fora, &dentro).unwrap();
        assert!(verificar(&estado).contains(&Problema::VariosVinculos(dentro.clone())));
        // Vínculo só interno (snapshot publicado por hard link) é aceito.
        let interno = t.0.join("interno");
        fs::create_dir_all(&interno).unwrap();
        fs::write(interno.join("snapshot-original.json.tmp"), "s").unwrap();
        fs::hard_link(
            interno.join("snapshot-original.json.tmp"),
            interno.join("snapshot-original.json"),
        )
        .unwrap();
        assert!(
            !verificar(&interno)
                .iter()
                .any(|x| matches!(x, Problema::VariosVinculos(_))),
            "vínculo interno não deveria ser problema"
        );
        assert!(abrir_sem_reparse(&dentro, OpenOptions::new().read(true)).is_err());
        assert!(abrir_sem_reparse(&estado.join("x"), OpenOptions::new().read(true)).is_err());
        let novo = estado.join("novo.txt");
        fs::write(&novo, "y").unwrap();
        assert!(abrir_sem_reparse(&novo, OpenOptions::new().read(true)).is_ok());
    }

    #[test]
    fn link_simbolico_e_recusado_na_abertura() {
        let t = DirTemp::criado("symlink");
        let alvo = t.0.join("alvo.txt");
        fs::write(&alvo, "x").unwrap();
        let link = t.0.join("link.txt");
        // Links simbólicos exigem privilégio ou modo de desenvolvedor.
        if std::os::windows::fs::symlink_file(&alvo, &link).is_err() {
            return;
        }
        let e = abrir_sem_reparse(&link, OpenOptions::new().append(true)).unwrap_err();
        assert!(e.to_string().contains("junção/link"), "{e}");
        assert_eq!(fs::read_to_string(&alvo).unwrap(), "x");
    }

    #[test]
    fn pasta_temporaria_comum_nao_passa() {
        // A pasta temporária do usuário pertence ao usuário (ou, elevado, a
        // Administradores) e concede escrita a ele: nunca é "segura".
        let t = DirTemp::criado("comum");
        let p = verificar(&t.0);
        assert!(
            p.iter()
                .any(|x| matches!(x, Problema::Dono(..) | Problema::Escrita(..))),
            "{p:?}"
        );
    }

    #[test]
    fn arquivo_privado_restringe_a_dacl_desde_a_criacao() {
        let t = DirTemp::criado("privado");
        let p = t.0.join("usuarios.json");
        let f = criar_arquivo_privado(&p).unwrap();
        let mut problemas = Vec::new();
        conferir_seguranca(&f, &p, &mut problemas, true);
        assert!(
            problemas.iter().all(|p| matches!(p, Problema::Dono(..))),
            "{problemas:?}"
        );
        assert_eq!(
            criar_arquivo_privado(&p).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
    }

    #[test]
    fn criado_com_descritor_seguro_so_tem_problema_de_dono_sem_privilegio() {
        let t = DirTemp::criado("criado");
        let estado = t.0.join("estado");
        assert_eq!(preparar(&estado, Modo::Avisar), Ok(Situacao::Ausente));
        assert!(!estado.exists(), "Avisar não cria");
        criar_seguro(&estado).unwrap();
        let p = verificar(&estado);
        if processo_privilegiado() {
            assert!(p.is_empty(), "{p:?}");
        } else {
            // Sem privilégio, só o dono (o próprio usuário) falha: a DACL
            // protegida não concede escrita a ninguém além de SY/BA.
            assert!(p.iter().all(|x| matches!(x, Problema::Dono(..))), "{p:?}");
        }
        // Arquivos criados dentro herdam a DACL protegida (sem privilégio,
        // nem o dono consegue gravar nele: só SY/BA escrevem).
        if processo_privilegiado() {
            definir_dono_padrao().unwrap();
            fs::write(estado.join("estado.json"), "{}").unwrap();
            let p = verificar(&estado);
            assert!(p.is_empty(), "{p:?}");
        } else {
            assert!(fs::write(estado.join("estado.json"), "{}").is_err());
        }
        // Já existe: criar de novo falha (nada pré-existente é reaproveitado).
        assert!(criar_seguro(&estado).is_err());
    }

    #[test]
    fn exigir_poe_em_quarentena_e_recria() {
        let t = DirTemp::criado("quarentena");
        let estado = t.estado();
        let alvo = t.0.join("alvo");
        fs::create_dir_all(&estado).unwrap();
        fs::create_dir_all(&alvo).unwrap();
        fs::write(estado.join("snapshot-original.json"), "forjado").unwrap();
        juncao(&estado.join("logs"), &alvo);
        let r = preparar(&estado, Modo::Exigir);
        // Sem privilégio o diretório recriado tem o usuário como dono e a
        // verificação final falha; com privilégio, fica seguro. Nos dois
        // casos o conteúdo inseguro foi para a quarentena.
        let quarentenas: Vec<_> = fs::read_dir(&t.0)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("nv-tuner.inseguro-")
            })
            .collect();
        assert_eq!(quarentenas.len(), 1);
        let q = quarentenas[0].path();
        assert_eq!(
            fs::read_to_string(q.join("snapshot-original.json")).unwrap(),
            "forjado"
        );
        assert!(diretorio_comum(&estado).unwrap());
        let restantes: Vec<String> = fs::read_dir(&estado)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "snapshot-perdido.json")
            .collect();
        assert!(
            restantes.is_empty(),
            "recriado vazio (só o marcador): {restantes:?}"
        );
        if processo_privilegiado() {
            assert!(matches!(r, Ok(Situacao::Corrigido { .. })), "{r:?}");
            // Reverificação, item 6: o próximo ciclo não captura em silêncio
            // um "original" que pode já ter mudanças do app.
            assert!(estado.join("snapshot-perdido.json").exists());
            let a = nvt_engine::Armazem::abrir(&estado).unwrap();
            assert!(a.base_pos_quarentena().is_some());
            assert!(
                a.avisos()
                    .iter()
                    .any(|x| x.codigo == nvt_engine::CodigoAviso::SnapshotOriginalPerdido),
                "{:?}",
                a.avisos()
            );
        } else {
            assert!(r.unwrap_err().contains("continua inseguro"));
        }
        // O alvo da junção ficou intacto.
        assert!(alvo.exists());
    }

    /// Reverificação, item 6: arquivo que some entre a listagem e a abertura
    /// (rotação de log, `estado.json.tmp`) não é problema nem leva à
    /// quarentena.
    #[test]
    fn arquivo_que_some_durante_a_verificacao_nao_e_problema() {
        let t = DirTemp::criado("some");
        let estado = t.estado();
        fs::create_dir_all(estado.join("logs")).unwrap();
        let tmp = estado.join("estado.json.tmp");
        let log = estado.join("logs").join("nv-tuner.log.1");
        fs::write(&tmp, "x").unwrap();
        fs::write(&log, "y").unwrap();
        let mut sumidos = 0;
        let p = verificar_com(&estado, |item| {
            if item == tmp || item == log {
                fs::remove_file(item).unwrap();
                sumidos += 1;
            }
        });
        assert_eq!(sumidos, 2);
        assert!(
            !p.iter().any(|x| matches!(x, Problema::Io(..))),
            "sumiço tratado como erro: {p:?}"
        );
        // Diretório interno que some antes de ser listado também é benigno.
        let sub = estado.join("logs");
        let p = verificar_com(&estado, |item| {
            if item == sub {
                fs::remove_dir_all(item).unwrap();
            }
        });
        assert!(!p.iter().any(|x| matches!(x, Problema::Io(..))), "{p:?}");
        // A raiz sumida continua sendo erro.
        let p = verificar_com(&estado, |item| {
            if item == estado {
                fs::remove_dir_all(item).unwrap();
            }
        });
        assert!(matches!(p.as_slice(), [Problema::Io(..)]), "{p:?}");
    }

    /// Reverificação, item 6, com privilégio: diretório seguro com um
    /// arquivo que some no meio da verificação continua `Seguro`, sem
    /// quarentena.
    #[test]
    fn exigir_nao_poe_em_quarentena_por_arquivo_sumido() {
        if !processo_privilegiado() {
            return; // criar com O:BA e gravar dentro exigem privilégio
        }
        definir_dono_padrao().unwrap();
        let t = DirTemp::criado("sumido-exigir");
        let estado = t.estado();
        criar_seguro(&estado).unwrap();
        fs::write(estado.join("estado.json"), "{}").unwrap();
        fs::write(estado.join("estado.json.tmp"), "{}").unwrap();
        // Simula o sumiço entre duas verificações: a primeira vê o arquivo,
        // ele some, e o resultado final não pode ser quarentena.
        let p = verificar_com(&estado, |item| {
            if item.ends_with("estado.json.tmp") {
                fs::remove_file(item).unwrap();
            }
        });
        assert!(p.is_empty(), "{p:?}");
        assert_eq!(preparar(&estado, Modo::Exigir), Ok(Situacao::Seguro));
        let quarentenas = fs::read_dir(&t.0)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".inseguro-"))
            .count();
        assert_eq!(quarentenas, 0);
    }

    #[test]
    fn dono_errado_so_em_item_interno_toma_posse() {
        if !processo_privilegiado() {
            return; // definir dono e criar com O:BA exigem privilégio
        }
        let t = DirTemp::criado("posse");
        let estado = t.estado();
        criar_seguro(&estado).unwrap();
        let arq = estado.join("estado.json");
        fs::write(&arq, "{}").unwrap();
        let usuario = std::env::var("USERNAME").unwrap();
        let s = Command::new("icacls")
            .arg(&arq)
            .arg("/setowner")
            .arg(&usuario)
            .output()
            .unwrap();
        assert!(s.status.success(), "{s:?}");
        assert!(matches!(verificar(&estado).as_slice(), [Problema::Dono(p, _)] if *p == arq));
        assert_eq!(
            preparar(&estado, Modo::Exigir),
            Ok(Situacao::PosseTomada { itens: 1 })
        );
        assert!(verificar(&estado).is_empty());
        assert_eq!(
            fs::read_to_string(&arq).unwrap(),
            "{}",
            "conteúdo preservado"
        );
    }

    #[test]
    fn avisar_nao_altera_nada() {
        let t = DirTemp::criado("avisar");
        let estado = t.estado();
        fs::create_dir_all(&estado).unwrap();
        fs::write(estado.join("estado.json"), "{}").unwrap();
        let r = preparar(&estado, Modo::Avisar).unwrap();
        assert!(
            matches!(r, Situacao::Inseguro(ref p) if !p.is_empty()),
            "{r:?}"
        );
        assert!(estado.join("estado.json").exists());
        assert_eq!(fs::read_dir(&t.0).unwrap().count(), 1);
    }
}
