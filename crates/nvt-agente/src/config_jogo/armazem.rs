//! Resolução de caminhos, leitura de valores e fatos anonimizados.

use std::path::{Path, PathBuf};

use nvt_core::config_jogo::{
    DefArquivoJogo, DefChaveJogo, FormatoArquivo, RaizJogo, TipoValorJogo, ValorJogo,
};
use nvt_core::jogos::BaseJogos;
use nvt_ipc::{
    EstadoBackup, Fato, FatoConfigJogo, GestaoJogoIpc, MotivoNaoAplicada, RotuloCaminho,
    ValorAtualJogo, ValorJogoIpc,
};
use nvt_jogocfg::pastas::PastasConhecidas;
use nvt_jogocfg::steam::RegistroSteam;
use nvt_jogocfg::{Chave, Erro, Hash, Motivo, Resultado, backup, lancamento, sha256};

use super::ArquivoGerido;

/// Instância usada para arquivos sem conta.
pub(crate) const INSTANCIA_LOCAL: &str = "local";
/// Instância relatada quando a conta não pôde ser escolhida (F3-12).
pub(crate) const INSTANCIA_AMBIGUA: &str = "ambigua";

/// Gravação atômica (fachada injetável para simular queda nos testes).
pub trait GravadorJogo {
    fn gravar(&self, raiz: &Path, caminho: &Path, novo: &[u8], hash: &Hash) -> Resultado<Hash>;
}

/// `nvt_jogocfg::arquivo::gravar` (temporário + `FlushFileBuffers` + `ReplaceFileW`).
#[derive(Debug, Clone, Copy, Default)]
pub struct GravadorReal;

impl GravadorJogo for GravadorReal {
    fn gravar(&self, raiz: &Path, caminho: &Path, novo: &[u8], hash: &Hash) -> Resultado<Hash> {
        nvt_jogocfg::arquivo::gravar(raiz, caminho, novo, hash)
    }
}

/// Fontes do perfil do usuário (pastas conhecidas, Steam, gravação).
pub struct AmbienteJogo {
    pub pastas: Box<dyn PastasConhecidas>,
    pub steam: Box<dyn RegistroSteam>,
    pub gravador: Box<dyn GravadorJogo>,
}

impl AmbienteJogo {
    /// Perfil do usuário do processo.
    pub fn do_usuario() -> Self {
        Self {
            pastas: Box::new(nvt_jogocfg::pastas::PastasWindows),
            steam: Box::new(nvt_jogocfg::steam::RegistroSteamWindows),
            gravador: Box::new(GravadorReal),
        }
    }

    /// `%LOCALAPPDATA%\nv-tuner\backups`.
    pub(crate) fn dir_backups(&self) -> Resultado<PathBuf> {
        Ok(self
            .pastas
            .local_app_data()?
            .join("nv-tuner")
            .join("backups"))
    }

    /// Resolve a instância de `def` no perfil. `None`: jogo/Steam não instalados.
    pub(crate) fn resolver(&self, base: &BaseJogos, def: &DefArquivoJogo) -> Option<Instancia> {
        let rotulo = rotulo(def)?;
        let jogo = def.jogo.as_str();
        let ambigua = || {
            Some(Instancia {
                raiz: PathBuf::new(),
                caminho: PathBuf::new(),
                instancia: INSTANCIA_AMBIGUA.into(),
                rotulo: rotulo.clone(),
                conta_ambigua: true,
                na_nuvem: false,
            })
        };
        let (raiz, caminho, instancia) = match def.raiz {
            RaizJogo::Documentos | RaizJogo::LocalAppData => {
                let raiz = match def.raiz {
                    RaizJogo::Documentos => self.pastas.documentos(),
                    _ => self.pastas.local_app_data(),
                }
                .ok()?;
                match def.caminho.split_once("<conta>") {
                    None => (
                        raiz.clone(),
                        raiz.join(&def.caminho),
                        INSTANCIA_LOCAL.into(),
                    ),
                    Some((pre, suf)) => {
                        if suf.trim_start_matches('\\') != nvt_jogocfg::ubisoft::ARQUIVO {
                            return None;
                        }
                        let pasta = raiz.join(pre.trim_end_matches('\\'));
                        match nvt_jogocfg::ubisoft::escolher_conta(&raiz, &pasta) {
                            Ok(c) => (raiz, c.arquivo, instancia_conta(jogo, &c.id)),
                            Err(Erro::ContaAmbigua) => return ambigua(),
                            Err(_) => return None,
                        }
                    }
                }
            }
            RaizJogo::SteamUserdata => match nvt_jogocfg::steam::resolver(self.steam.as_ref()) {
                Ok(s) => {
                    let mut rel = def
                        .caminho
                        .replace("<steamid3>", &s.conta.steamid3.to_string());
                    if rel.contains("<appid>") {
                        let appid = base.jogo(jogo)?.steam_appid?;
                        rel = rel.replace("<appid>", &appid.to_string());
                    }
                    let caminho = s.pasta.join("userdata").join(rel);
                    let instancia = instancia_conta("steam", &s.conta.steamid64.to_string());
                    (s.pasta, caminho, instancia)
                }
                Err(Erro::ContaAmbigua) => return ambigua(),
                Err(_) => return None,
            },
        };
        let na_nuvem = nuvem(&raiz, &caminho);
        Some(Instancia {
            raiz,
            caminho,
            instancia,
            rotulo,
            conta_ambigua: false,
            na_nuvem,
        })
    }
}

/// Instância resolvida. `raiz`/`caminho` ficam no agente.
#[derive(Debug, Clone)]
pub(crate) struct Instancia {
    pub raiz: PathBuf,
    pub caminho: PathBuf,
    pub instancia: String,
    pub rotulo: RotuloCaminho,
    pub conta_ambigua: bool,
    pub na_nuvem: bool,
}

/// Hash opaco da conta: o identificador em claro nunca sai do agente.
pub(crate) fn instancia_conta(escopo: &str, id: &str) -> String {
    sha256(format!("nv-tuner/{escopo}/{id}").as_bytes()).hex()[..16].to_owned()
}

/// Steam Cloud (F3-09): existe o par `remote` de um arquivo em `local`.
fn nuvem(raiz: &Path, caminho: &Path) -> bool {
    let texto = caminho.to_string_lossy();
    let Some(i) = texto.to_ascii_lowercase().rfind("\\local\\") else {
        return false;
    };
    let remoto = format!("{}\\remote\\{}", &texto[..i], &texto[i + 7..]);
    matches!(
        nvt_jogocfg::arquivo::existe(raiz, Path::new(&remoto)),
        Ok(true)
    )
}

fn rotulo(def: &DefArquivoJogo) -> Option<RotuloCaminho> {
    let prefixo = match def.raiz {
        RaizJogo::Documentos => "%USERPROFILE%\\Documents\\",
        RaizJogo::LocalAppData => "%LOCALAPPDATA%\\",
        RaizJogo::SteamUserdata => "%STEAM%\\userdata\\",
    };
    let r = RotuloCaminho(format!("{prefixo}{}", def.caminho));
    r.validar().is_ok().then_some(r)
}

/// Endereço da chave no arquivo.
pub(crate) fn chave(def: &DefArquivoJogo, c: &DefChaveJogo) -> Resultado<Chave> {
    if c.chave.is_empty() {
        return Err(Erro::FormatoInesperado("chave sem endereço".into()));
    }
    Ok(match def.formato {
        FormatoArquivo::Ini => Chave::ini(
            c.secao
                .as_deref()
                .ok_or_else(|| Erro::FormatoInesperado("chave INI sem seção".into()))?,
            &c.chave,
        ),
        FormatoArquivo::Vdf | FormatoArquivo::Lancamento => Chave::vdf(&c.chave),
    })
}

/// Chaves legíveis do arquivo (fora `a_confirmar` e sem endereço).
pub(crate) fn chaves_legiveis(def: &DefArquivoJogo) -> impl Iterator<Item = &DefChaveJogo> {
    def.chave
        .iter()
        .filter(|c| !c.a_confirmar && !c.chave.is_empty())
}

/// Texto bruto e valor tipado da chave.
pub(crate) fn ler_chave(
    bytes: &[u8],
    def: &DefArquivoJogo,
    c: &DefChaveJogo,
) -> Resultado<(String, ValorJogo)> {
    let bruto = nvt_jogocfg::ler_valor(bytes, &chave(def, c)?)?;
    let valor = interpretar(c.tipo, &bruto)?;
    Ok((bruto, valor))
}

fn interpretar(tipo: TipoValorJogo, bruto: &str) -> Resultado<ValorJogo> {
    let t = bruto.trim();
    let inesperado = || Erro::FormatoInesperado("valor não reconhecido".into());
    Ok(match tipo {
        TipoValorJogo::Inteiro => ValorJogo::Inteiro(t.parse().map_err(|_| inesperado())?),
        TipoValorJogo::Decimal => {
            let f: f64 = t.parse().map_err(|_| inesperado())?;
            if !f.is_finite() || f.fract() != 0.0 || f.abs() > 1e15 {
                return Err(inesperado());
            }
            // Inteiro exato e limitado acima: a conversão não trunca.
            ValorJogo::Inteiro(f as i64)
        }
        TipoValorJogo::Booleano => match t.to_ascii_lowercase().as_str() {
            "true" | "1" => ValorJogo::Booleano(true),
            "false" | "0" => ValorJogo::Booleano(false),
            _ => return Err(inesperado()),
        },
        TipoValorJogo::Texto => {
            if t.len() > 64 || !t.bytes().all(|b| (0x20..0x7f).contains(&b)) {
                return Err(inesperado());
            }
            ValorJogo::Texto(t.to_owned())
        }
        TipoValorJogo::TokenLancamento => {
            ValorJogo::TokenLancamento(lancamento::ler_fps_max(bruto)?)
        }
    })
}

/// Texto a gravar para `valor`, no estilo do texto atual.
pub(crate) fn texto_para(c: &DefChaveJogo, atual: &str, valor: &ValorJogo) -> Resultado<String> {
    let inesperado = || Erro::FormatoInesperado("valor não gravável".into());
    Ok(match (c.tipo, valor) {
        (TipoValorJogo::Inteiro, ValorJogo::Inteiro(n)) => n.to_string(),
        (TipoValorJogo::Decimal, ValorJogo::Inteiro(n)) => match atual.trim().split_once('.') {
            Some((_, casas)) if !casas.is_empty() => format!("{n}.{}", "0".repeat(casas.len())),
            _ => n.to_string(),
        },
        (TipoValorJogo::Booleano, ValorJogo::Booleano(b)) => {
            let t = atual.trim();
            if t == "0" || t == "1" {
                u8::from(*b).to_string()
            } else if t.bytes().all(|x| x.is_ascii_lowercase()) {
                b.to_string()
            } else if *b {
                "True".into()
            } else {
                "False".into()
            }
        }
        (TipoValorJogo::TokenLancamento, ValorJogo::TokenLancamento(Some(n))) => {
            lancamento::definir_fps_max(atual, *n)?
        }
        _ => return Err(inesperado()),
    })
}

pub(crate) fn valor_ipc(v: &ValorJogo) -> ValorJogoIpc {
    match v {
        ValorJogo::Ausente => ValorJogoIpc::Ausente,
        ValorJogo::Inteiro(n) => ValorJogoIpc::Inteiro(*n),
        ValorJogo::Booleano(b) => ValorJogoIpc::Booleano(*b),
        ValorJogo::Texto(t) => ValorJogoIpc::Texto(t.clone()),
        ValorJogo::TokenLancamento(n) => ValorJogoIpc::TokenLancamento(*n),
    }
}

pub(crate) fn valor_core(v: &ValorJogoIpc) -> ValorJogo {
    match v {
        ValorJogoIpc::Ausente => ValorJogo::Ausente,
        ValorJogoIpc::Inteiro(n) => ValorJogo::Inteiro(*n),
        ValorJogoIpc::Booleano(b) => ValorJogo::Booleano(*b),
        ValorJogoIpc::Texto(t) => ValorJogo::Texto(t.clone()),
        ValorJogoIpc::TokenLancamento(n) => ValorJogo::TokenLancamento(*n),
    }
}

/// Motivo de relato (só motivos da família de arquivo de jogo).
pub(crate) fn motivo(e: &Erro) -> Option<MotivoNaoAplicada> {
    Some(match e.motivo() {
        Motivo::FormatoInesperado => MotivoNaoAplicada::FormatoInesperado,
        Motivo::ArquivoSomenteLeitura => MotivoNaoAplicada::ArquivoSomenteLeitura,
        Motivo::ReparseRecusado => MotivoNaoAplicada::ReparseRecusado,
        Motivo::ArquivoAusente => MotivoNaoAplicada::ArquivoAusente,
        Motivo::ArquivoNaNuvem => MotivoNaoAplicada::ArquivoNaNuvem,
        Motivo::ArquivoAlteradoPeloJogo => MotivoNaoAplicada::ArquivoAlteradoPeloJogo,
        Motivo::ContaAmbigua => MotivoNaoAplicada::ContaAmbigua,
        Motivo::FalhaBackup | Motivo::FalhaIo => return None,
    })
}

/// Backup íntegro? Devolve os bytes do original.
pub(crate) fn ler_backup(
    ambiente: &AmbienteJogo,
    jogo: &str,
    g: &ArquivoGerido,
) -> Resultado<Vec<u8>> {
    let bytes = backup::ler(&ambiente.dir_backups()?, jogo, &g.backup)?;
    if sha256(&bytes).hex() != g.hash_original {
        return Err(Erro::BackupCorrompido(PathBuf::from(&g.backup)));
    }
    Ok(bytes)
}

/// Fato anonimizado de uma instância.
pub(crate) fn fato(
    ambiente: &AmbienteJogo,
    def: &DefArquivoJogo,
    inst: &Instancia,
    gerido: Option<&ArquivoGerido>,
) -> Option<FatoConfigJogo> {
    let alvo = nvt_ipc::AlvoIpc::ConfigJogo {
        jogo: def.jogo.as_str().to_owned(),
        arquivo: def.id.clone(),
    };
    let mut fato = FatoConfigJogo {
        alvo,
        instancia: inst.instancia.clone(),
        rotulo: inst.rotulo.clone(),
        presente: false,
        hash: None,
        somente_leitura: false,
        na_nuvem: inst.na_nuvem,
        conta_ambigua: inst.conta_ambigua,
        backup: EstadoBackup::SemBackup,
        valores: Vec::new(),
        gestao: Vec::new(),
    };
    if inst.conta_ambigua {
        return Some(fato);
    }
    match nvt_jogocfg::arquivo::ler(&inst.raiz, &inst.caminho) {
        Ok(lido) => {
            fato.presente = true;
            fato.hash = Some(lido.hash.hex());
            fato.somente_leitura = lido.somente_leitura;
            for c in chaves_legiveis(def) {
                let valor = match ler_chave(&lido.bytes, def, c) {
                    Ok((_, v)) => Fato::Conhecido(valor_ipc(&v)),
                    Err(Erro::ChaveAusente(_)) => Fato::Ausente,
                    Err(_) => Fato::Desconhecido,
                };
                fato.valores.push(ValorAtualJogo {
                    setting: c.setting.codigo().to_owned(),
                    valor,
                });
            }
        }
        Err(Erro::ArquivoAusente(_)) if gerido.is_none() => return None,
        Err(Erro::ArquivoAusente(_)) => {}
        Err(_) => fato.presente = true,
    }
    if let Some(g) = gerido {
        fato.backup = match ler_backup(ambiente, def.jogo.as_str(), g) {
            Ok(_) => EstadoBackup::Integro,
            Err(_) => EstadoBackup::Comprometido,
        };
        let depois = g.hash_depois.clone().or_else(|| fato.hash.clone());
        for c in &g.chaves {
            if let Some(hash_depois) = &depois {
                fato.gestao.push(GestaoJogoIpc {
                    setting: c.setting.codigo().to_owned(),
                    antes: valor_ipc(&c.antes),
                    esperado: valor_ipc(&c.esperado),
                    hash_antes: g.hash_original.clone(),
                    hash_depois: hash_depois.clone(),
                    travado: c.travado,
                });
            }
        }
    }
    fato.validar().is_ok().then_some(fato)
}
