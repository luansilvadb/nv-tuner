//! Fatos do jogo levantados em disco: exe principal, API gráfica, DLSS,
//! Reflex e anticheat, cada um com nível de confiança.

pub mod anticheat;
pub mod arquivos;
pub mod exe;
pub mod pe;

use std::collections::HashSet;
use std::path::PathBuf;

use crate::model::DetectedGame;
use crate::registro::ServicosSistema;
use arquivos::{Arquivo, Listagem};

pub use anticheat::FatoAnticheat;
pub use exe::{CandidatoExe, MotivoExe};
pub use pe::ErroPe;

/// Grau de confiança de um fato detectado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Confianca {
    Baixa,
    Media,
    Alta,
}

/// Valor detectado com confiança e evidência.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fato<T> {
    pub valor: T,
    pub confianca: Confianca,
    pub evidencia: String,
}

impl<T> Fato<T> {
    pub fn new(valor: T, confianca: Confianca, evidencia: impl Into<String>) -> Self {
        Self {
            valor,
            confianca,
            evidencia: evidencia.into(),
        }
    }
}

/// API gráfica.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiGrafica {
    Dx9,
    Dx11,
    Dx12,
    Vulkan,
    OpenGl,
}

/// DLL do ecossistema DLSS/Streamline encontrada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DllNvidia {
    pub caminho: PathBuf,
    pub versao: Option<String>,
}

/// Componentes DLSS presentes na instalação.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FatosDlss {
    /// `nvngx_dlss.dll` (Super Resolution).
    pub super_resolution: Option<DllNvidia>,
    /// `nvngx_dlssd.dll` (Ray Reconstruction).
    pub ray_reconstruction: Option<DllNvidia>,
    /// `nvngx_dlssg.dll` (Frame Generation).
    pub frame_generation: Option<DllNvidia>,
    /// `nvngx_deepdvc.dll`.
    pub deep_dvc: Option<DllNvidia>,
    /// Versão do Streamline (`sl.interposer.dll`), se presente.
    pub streamline_versao: Option<String>,
    /// Módulos `sl.*.dll` encontrados (ex.: "reflex", "dlss_g").
    pub streamline_modulos: Vec<String>,
}

impl FatosDlss {
    pub fn presente(&self) -> bool {
        self.super_resolution.is_some()
    }
}

/// Fatos completos de um jogo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameTraits {
    pub candidatos_exe: Vec<CandidatoExe>,
    /// Primeiro candidato (o cruzamento com o DRS pode trocar depois).
    pub exe_principal: Option<PathBuf>,
    /// `None` no valor = API desconhecida.
    pub api: Fato<Option<ApiGrafica>>,
    /// DLLs importadas pelo exe principal (normais e delay-load).
    pub imports: Vec<String>,
    /// Import table ilegível/cifrada (exe protegido): a API veio do fallback.
    pub imports_ofuscados: bool,
    pub dlss: Fato<FatosDlss>,
    pub reflex: Fato<bool>,
    pub anticheats: Vec<FatoAnticheat>,
    /// A listagem atingiu o limite de arquivos.
    pub listagem_truncada: bool,
}

impl GameTraits {
    /// Algum anticheat de kernel detectado.
    pub fn anticheat_kernel(&self) -> bool {
        self.anticheats.iter().any(|a| a.kernel)
    }
}

fn api_de_imports(dlls: &[String]) -> Option<(ApiGrafica, Confianca, String)> {
    let tem = |n: &str| dlls.iter().any(|d| d == n);
    let r = if tem("d3d12.dll") {
        let extra = if tem("d3d11.dll") {
            " (também importa d3d11.dll: a escolhida só aparece em runtime)"
        } else {
            ""
        };
        (
            ApiGrafica::Dx12,
            Confianca::Alta,
            format!("importa d3d12.dll{extra}"),
        )
    } else if tem("d3d11.dll") {
        (
            ApiGrafica::Dx11,
            Confianca::Alta,
            "importa d3d11.dll".into(),
        )
    } else if tem("vulkan-1.dll") {
        (
            ApiGrafica::Vulkan,
            Confianca::Alta,
            "importa vulkan-1.dll".into(),
        )
    } else if tem("d3d9.dll") {
        (ApiGrafica::Dx9, Confianca::Alta, "importa d3d9.dll".into())
    } else if tem("opengl32.dll") {
        (
            ApiGrafica::OpenGl,
            Confianca::Media,
            "importa opengl32.dll".into(),
        )
    } else {
        return None;
    };
    Some(r)
}

/// Pastas onde arquivos vizinhos não contam para a API (CEF traz `vulkan-1.dll`).
fn fora_de_ruido(a: &Arquivo) -> bool {
    !a.pastas()
        .any(|p| p == "cef3" || p == "cef" || p == "thirdparty")
}

/// Fallback pela presença de DLLs perto do exe principal.
fn api_por_vizinhos(
    listagem: &Listagem,
    exe_dir_rel: &str,
) -> Option<(ApiGrafica, Confianca, String)> {
    let perto = |a: &&Arquivo| {
        let d = a.rel.rsplit_once('\\').map(|(d, _)| d).unwrap_or("");
        (d == exe_dir_rel || d.starts_with(&format!("{exe_dir_rel}\\")) || exe_dir_rel.is_empty())
            && fora_de_ruido(a)
    };
    if let Some(a) = listagem
        .arquivos
        .iter()
        .filter(perto)
        .find(|a| a.nome == "d3d12core.dll")
    {
        return Some((
            ApiGrafica::Dx12,
            Confianca::Media,
            format!("Agility SDK presente: {}", a.rel),
        ));
    }
    if let Some(a) = listagem.arquivos.iter().find(|a| {
        a.nome == "vulkan-1.dll"
            && fora_de_ruido(a)
            && a.rel.rsplit_once('\\').map(|(d, _)| d).unwrap_or("") == exe_dir_rel
    }) {
        return Some((
            ApiGrafica::Vulkan,
            Confianca::Baixa,
            format!("vulkan-1.dll ao lado do exe: {}", a.rel),
        ));
    }
    None
}

fn dll_nvidia<E>(
    listagem: &Listagem,
    nome: &str,
    ler: &mut impl FnMut(&std::path::Path) -> Result<Result<Vec<u8>, pe::ErroPe>, E>,
) -> Result<Option<DllNvidia>, E> {
    listagem
        .arquivos
        .iter()
        .filter(|a| a.nome == nome)
        .try_fold(None, |melhor, a| {
            let dll = DllNvidia {
                caminho: a.caminho.clone(),
                versao: ler(&a.caminho)?
                    .and_then(|bytes| pe::versao_de_bytes(&bytes))
                    .ok()
                    .and_then(|v| v.versao),
            };
            Ok(melhor.into_iter().chain([dll]).max_by(|x, y| {
                pe::comparar_versoes(
                    x.versao.as_deref().unwrap_or("0"),
                    y.versao.as_deref().unwrap_or("0"),
                )
            }))
        })
}

fn dlss<E>(
    listagem: &Listagem,
    ler: &mut impl FnMut(&std::path::Path) -> Result<Result<Vec<u8>, pe::ErroPe>, E>,
) -> Result<Fato<FatosDlss>, E> {
    let mut f = FatosDlss {
        super_resolution: dll_nvidia(listagem, "nvngx_dlss.dll", ler)?,
        ray_reconstruction: dll_nvidia(listagem, "nvngx_dlssd.dll", ler)?,
        frame_generation: dll_nvidia(listagem, "nvngx_dlssg.dll", ler)?,
        deep_dvc: dll_nvidia(listagem, "nvngx_deepdvc.dll", ler)?,
        ..Default::default()
    };
    for a in &listagem.arquivos {
        if let Some(m) = a
            .nome
            .strip_prefix("sl.")
            .and_then(|r| r.strip_suffix(".dll"))
        {
            if !f.streamline_modulos.iter().any(|x| x == m) {
                f.streamline_modulos.push(m.to_string());
            }
        }
    }
    f.streamline_modulos.sort();
    f.streamline_versao = dll_nvidia(listagem, "sl.interposer.dll", ler)?.and_then(|d| d.versao);
    let evid = match &f.super_resolution {
        Some(d) => format!(
            "nvngx_dlss.dll {}",
            d.versao.as_deref().unwrap_or("(versão ilegível)")
        ),
        None => "nenhum nvngx_dlss.dll na instalação".into(),
    };
    // Ausência em disco é forte para DLSS (a DLL precisa ser distribuída com o jogo).
    Ok(Fato::new(f, Confianca::Alta, evid))
}

fn reflex(listagem: &Listagem) -> Fato<bool> {
    let achar = |n: &str| listagem.arquivos.iter().find(|a| a.nome == n);
    if let Some(a) = achar("sl.reflex.dll") {
        return Fato::new(
            true,
            Confianca::Alta,
            format!("Streamline Reflex: {}", a.rel),
        );
    }
    if let Some(a) = achar("nvlowlatencyvk.dll") {
        return Fato::new(true, Confianca::Alta, format!("Reflex Vulkan: {}", a.rel));
    }
    if let Some(a) = achar("sl.pcl.dll") {
        return Fato::new(
            true,
            Confianca::Media,
            format!("Streamline PC Latency: {}", a.rel),
        );
    }
    Fato::new(
        false,
        Confianca::Baixa,
        "sem sl.reflex/NvLowLatencyVk; Reflex via NVAPI não deixa rastro em disco",
    )
}

/// Analisa a instalação de um jogo. Somente leitura.
pub fn analisar(jogo: &DetectedGame, servicos: &ServicosSistema) -> GameTraits {
    let listagem = arquivos::listar(&jogo.pasta);
    analisar_listagem(jogo, &listagem, servicos)
}

/// Resultado de [`analisar_se_fechado`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnaliseJogo {
    /// O jogo não estava em execução: a pasta foi lida normalmente.
    Analisado(Box<GameTraits>),
    /// Algum exe do jogo está em execução: nenhum arquivo da pasta foi aberto
    /// nem listado. O chamador deve reaproveitar os fatos anteriores (F1-09).
    EmExecucao {
        /// Exes do jogo encontrados entre os processos (grafia de `em_execucao`).
        exes: Vec<String>,
    },
}

/// Nomes de exe conhecidos de um jogo sem tocar na pasta dele: o nome de
/// `exe_indicado` (vindo do launcher) mais `exes_conhecidos` (ex.: o exe
/// principal da varredura anterior). Caminhos são reduzidos ao nome do arquivo.
pub fn exes_conhecidos_do_jogo<S: AsRef<str>>(
    jogo: &DetectedGame,
    exes_conhecidos: &[S],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let indicado = jogo
        .exe_indicado
        .as_deref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned());
    let nomes = indicado.into_iter().chain(exes_conhecidos.iter().map(|s| {
        let s = s.as_ref();
        s.rsplit(['\\', '/']).next().unwrap_or(s).to_string()
    }));
    for n in nomes {
        if !n.is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
            out.push(n);
        }
    }
    out
}

/// Exes do jogo que estão em `em_execucao` (comparação sem diferenciar
/// maiúsculas). Não acessa disco.
pub fn jogo_em_execucao<S: AsRef<str>, T: AsRef<str>>(
    jogo: &DetectedGame,
    exes_conhecidos: &[S],
    em_execucao: &[T],
) -> Vec<String> {
    let conhecidos = exes_conhecidos_do_jogo(jogo, exes_conhecidos);
    let mut out: Vec<String> = Vec::new();
    for r in em_execucao {
        let r = r.as_ref();
        if conhecidos.iter().any(|c| c.eq_ignore_ascii_case(r))
            && !out.iter().any(|x| x.eq_ignore_ascii_case(r))
        {
            out.push(r.to_string());
        }
    }
    out
}

/// Igual a [`analisar`], mas, se algum exe conhecido do jogo está em
/// `em_execucao`, não abre nem lista nenhum arquivo da pasta (inclusive DLLs de
/// anticheat) e devolve [`AnaliseJogo::EmExecucao`].
///
/// Esta API de diagnóstico não garante F1-09. O serviço e a interface usam
/// [`analisar_com_lista_de_processos`].
pub fn analisar_se_fechado<S: AsRef<str>, T: AsRef<str>>(
    jogo: &DetectedGame,
    servicos: &ServicosSistema,
    exes_conhecidos: &[S],
    em_execucao: &[T],
) -> AnaliseJogo {
    let exes = jogo_em_execucao(jogo, exes_conhecidos, em_execucao);
    if exes.is_empty() {
        AnaliseJogo::Analisado(Box::new(analisar(jogo, servicos)))
    } else {
        AnaliseJogo::EmExecucao { exes }
    }
}

/// Motivo para adiar a análise protegida.
#[derive(Debug)]
pub enum AnaliseAdiada {
    ListagemIncompleta,
    ProcessosIndisponiveis(crate::ListaIndisponivel),
}

/// Usa a lista inicial para evitar tocar na pasta de um jogo conhecido em
/// execução. Enumera metadados e renova a lista antes de abrir cada arquivo.
pub fn analisar_com_lista_de_processos(
    jogo: &DetectedGame,
    servicos: &ServicosSistema,
    exes_conhecidos: &[String],
    processos: &[crate::Processo],
    fonte: &(impl crate::FonteProcessos + ?Sized),
) -> Result<(AnaliseJogo, Option<DetectedGame>), AnaliseAdiada> {
    let nomes: Vec<_> = processos.iter().map(|p| p.nome.as_str()).collect();
    let exes = jogo_em_execucao(jogo, exes_conhecidos, &nomes);
    if !exes.is_empty() {
        return Ok((AnaliseJogo::EmExecucao { exes }, None));
    }

    let raiz = jogo.pasta_instalacao();
    let conhecidos: HashSet<_> = exes_conhecidos_do_jogo(jogo, exes_conhecidos)
        .into_iter()
        .map(|n| n.to_lowercase())
        .collect();
    let verificar = |vigiados: &HashSet<String>| {
        let lista = fonte
            .ler()
            .map_err(|e| Interrupcao::Adiada(AnaliseAdiada::ProcessosIndisponiveis(e)))?;
        let mut exes: Vec<_> = lista
            .into_iter()
            .map(|p| p.nome.to_lowercase())
            .filter(|nome| vigiados.contains(nome))
            .collect();
        exes.sort_unstable();
        exes.dedup();
        if exes.is_empty() {
            Ok(())
        } else {
            Err(Interrupcao::EmExecucao(exes))
        }
    };

    let analisar = || {
        let xbox = jogo.launcher == crate::Launcher::Xbox;
        let listar = || {
            let (listagem, falhou) = arquivos::listar_limitado(raiz, raiz);
            let mut vigiados = conhecidos.clone();
            vigiados.extend(
                listagem
                    .arquivos
                    .iter()
                    .filter(|a| a.nome.ends_with(".exe"))
                    .map(|a| a.nome.clone()),
            );
            if falhou || (listagem.truncada && !arquivos::coletar_exes(raiz, xbox, &mut vigiados)) {
                return Err(listagem_incompleta());
            }
            Ok((listagem, vigiados))
        };
        let abrir = |caminho: &std::path::Path, vigiados: &HashSet<String>| {
            let pai = caminho.parent().ok_or_else(listagem_incompleta)?;
            let _guardas =
                arquivos::proteger_diretorios(raiz, pai).map_err(|_| listagem_incompleta())?;
            match std::fs::symlink_metadata(caminho) {
                Ok(m) if arquivos::ponto_de_reanalise(&m) || !m.is_file() => {
                    return Err(listagem_incompleta());
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(listagem_incompleta()),
            }
            verificar(vigiados)?;
            let arquivo = match pe::abrir(caminho) {
                Ok(a) => a,
                Err(e) => return Ok(Err(e)),
            };
            let dados = arquivo.metadata().map_err(|_| listagem_incompleta())?;
            if arquivos::ponto_de_reanalise(&dados) || !dados.is_file() {
                return Err(listagem_incompleta());
            }
            Ok(Ok(arquivo))
        };
        let analisar_pasta =
            |resolvido: &DetectedGame, listagem: &Listagem, vigiados: &HashSet<_>| {
                analisar_listagem_verificada(
                    resolvido,
                    listagem,
                    servicos,
                    |caminho| chave_exe_verificado(resolvido.pasta_instalacao(), caminho),
                    |caminho| {
                        Ok(abrir(caminho, vigiados)?
                            .map_err(|e| pe::ErroPe::Io(e.to_string()))
                            .and_then(|arquivo| pe::ler_aberto(arquivo, caminho)))
                    },
                )
                .map(|fatos| AnaliseJogo::Analisado(Box::new(fatos)))
            };
        if xbox {
            // A guarda da instalação também cobre o caminho até a raiz Content.
            let _guarda =
                arquivos::proteger_diretorios(raiz, raiz).map_err(|_| listagem_incompleta())?;
            let (listagem_raiz, mut vigiados) = listar()?;
            for pasta in [raiz.join("Content"), raiz.to_path_buf()] {
                let _guarda = match arquivos::proteger_diretorios(raiz, &pasta) {
                    Ok(g) => g,
                    Err(e)
                        if pasta != raiz
                            && matches!(
                                e.kind(),
                                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                            ) =>
                    {
                        continue;
                    }
                    Err(_) => return Err(listagem_incompleta()),
                };
                let content;
                let listagem = if pasta == raiz {
                    &listagem_raiz
                } else {
                    let (listagem, falhou) = arquivos::listar_limitado(raiz, &pasta);
                    if falhou {
                        return Err(listagem_incompleta());
                    }
                    vigiados.extend(
                        listagem
                            .arquivos
                            .iter()
                            .filter(|a| a.nome.ends_with(".exe"))
                            .map(|a| a.nome.clone()),
                    );
                    content = listagem;
                    &content
                };
                verificar(&vigiados)?;
                let caminho = pasta.join("MicrosoftGame.config");
                let metadados = abrir(&caminho, &vigiados)?
                    .ok()
                    .and_then(|arquivo| {
                        #[cfg(feature = "instrumentar-aberturas-pe")]
                        pe::registrar_abertura(&caminho);
                        std::io::read_to_string(arquivo).ok()
                    })
                    .map(|xml| crate::launchers::xbox::jogo_do_config(jogo, pasta.clone(), &xml));
                if metadados.is_none() && pasta != raiz {
                    continue;
                }
                let resolvido = metadados.as_ref().unwrap_or(jogo);
                vigiados.extend(
                    exes_conhecidos_do_jogo(resolvido, &[] as &[String])
                        .into_iter()
                        .map(|nome| nome.to_lowercase()),
                );
                return Ok((analisar_pasta(resolvido, listagem, &vigiados)?, metadados));
            }
            unreachable!("a raiz da instalação sempre é verificada")
        }
        let (listagem, vigiados) = listar()?;
        verificar(&vigiados)?;
        Ok((analisar_pasta(jogo, &listagem, &vigiados)?, None))
    };
    match analisar() {
        Ok(resultado) => Ok(resultado),
        Err(Interrupcao::EmExecucao(exes)) => Ok((AnaliseJogo::EmExecucao { exes }, None)),
        Err(Interrupcao::Adiada(motivo)) => Err(motivo),
    }
}

/// Compara caminhos da listagem sem consultar o disco pelo caminho indicado.
fn chave_exe_verificado(raiz: &std::path::Path, caminho: &std::path::Path) -> String {
    let raiz = crate::model::chave_caminho(raiz);
    let chave = crate::model::chave_caminho(caminho);
    let prefixo = format!("{}\\", raiz.trim_end_matches('\\'));
    let Some(relativo) = chave.strip_prefix(&prefixo) else {
        return chave;
    };
    let mut partes = Vec::new();
    for parte in relativo.split('\\') {
        match parte {
            "" | "." => {}
            ".." if partes.pop().is_some() => {}
            ".." => return chave,
            _ => partes.push(parte),
        }
    }
    prefixo + &partes.join("\\")
}

fn listagem_incompleta() -> Interrupcao {
    Interrupcao::Adiada(AnaliseAdiada::ListagemIncompleta)
}

enum Interrupcao {
    EmExecucao(Vec<String>),
    Adiada(AnaliseAdiada),
}

/// Igual a [`analisar`], a partir de uma listagem já feita.
pub fn analisar_listagem(
    jogo: &DetectedGame,
    listagem: &Listagem,
    servicos: &ServicosSistema,
) -> GameTraits {
    analisar_listagem_verificada(
        jogo,
        listagem,
        servicos,
        crate::model::pasta_canonica,
        |caminho| Ok::<_, std::convert::Infallible>(pe::ler_compartilhado(caminho)),
    )
    .unwrap_or_else(|impossivel| match impossivel {})
}

fn analisar_listagem_verificada<E>(
    jogo: &DetectedGame,
    listagem: &Listagem,
    servicos: &ServicosSistema,
    chave: impl Fn(&std::path::Path) -> String,
    mut ler: impl FnMut(&std::path::Path) -> Result<Result<Vec<u8>, pe::ErroPe>, E>,
) -> Result<GameTraits, E> {
    let candidatos = exe::candidatos_com_chave(listagem, jogo.exe_indicado.as_deref(), chave);
    let principal = candidatos.first();

    let mut imports = Vec::new();
    let mut ofuscados = false;
    let mut api = None;
    let mut exe_dir_rel = String::new();
    if let Some(c) = principal {
        if let Some(a) = listagem.arquivos.iter().find(|a| a.caminho == c.caminho) {
            exe_dir_rel = a
                .rel
                .rsplit_once('\\')
                .map(|(d, _)| d.to_string())
                .unwrap_or_default();
        }
        match ler(&c.caminho)?.and_then(|bytes| pe::imports_de_bytes(&bytes)) {
            Ok(imp) => {
                let sistema = imp.dlls.iter().any(|d| {
                    d == "kernel32.dll" || d == "ntdll.dll" || d.starts_with("api-ms-win-")
                });
                // Exe normal importa kernel32; tabela mínima sem ele = protegido/cifrado.
                ofuscados = !sistema || (imp.delay_ilegiveis > 0 && imp.dlls.len() <= 3);
                api = api_de_imports(&imp.dlls);
                imports = imp.dlls;
            }
            // Acima do limite de leitura: imports desconhecidos, não ofuscados.
            Err(pe::ErroPe::GrandeDemais(_)) => {}
            Err(_) => ofuscados = true,
        }
    }
    let api = api
        .or_else(|| principal.and_then(|_| api_por_vizinhos(listagem, &exe_dir_rel)))
        .map(|(v, c, e)| Fato::new(Some(v), c, e))
        .unwrap_or_else(|| Fato::new(None, Confianca::Baixa, "nenhum indício de API"));

    Ok(GameTraits {
        exe_principal: principal.map(|c| c.caminho.clone()),
        candidatos_exe: candidatos,
        api,
        imports,
        imports_ofuscados: ofuscados,
        dlss: dlss(listagem, &mut ler)?,
        reflex: reflex(listagem),
        anticheats: anticheat::detectar_verificado(
            listagem,
            jogo.exige_vanguard,
            servicos,
            &mut ler,
        )?,
        listagem_truncada: listagem.truncada,
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    fn arq(rel: &str) -> Arquivo {
        let rel = rel.to_lowercase();
        Arquivo {
            caminho: PathBuf::from(format!(r"X:\inexistente\{rel}")),
            nome: rel.rsplit('\\').next().unwrap().to_string(),
            rel,
            tamanho: 1,
        }
    }

    #[test]
    fn api_por_imports() {
        let d = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            api_de_imports(&d(&["dxgi.dll", "d3d11.dll"])).unwrap().0,
            ApiGrafica::Dx11
        );
        assert_eq!(
            api_de_imports(&d(&["d3d11.dll", "d3d12.dll"])).unwrap().0,
            ApiGrafica::Dx12
        );
        assert!(api_de_imports(&d(&["coreinit.dll"])).is_none());
    }

    #[test]
    fn fallback_ignora_cef() {
        let l = Listagem {
            arquivos: vec![
                arq(r"jogo\binaries\win64\jogo-win64-shipping.exe"),
                arq(r"engine\binaries\thirdparty\cef3\win64\vulkan-1.dll"),
                arq(r"jogo\binaries\win64\d3d12\x64\d3d12core.dll"),
            ],
            ..Default::default()
        };
        let r = api_por_vizinhos(&l, r"jogo\binaries\win64").unwrap();
        assert_eq!(r.0, ApiGrafica::Dx12);
        let l2 = Listagem {
            arquivos: vec![arq(r"engine\binaries\thirdparty\cef3\win64\vulkan-1.dll")],
            ..Default::default()
        };
        assert!(api_por_vizinhos(&l2, r"jogo\binaries\win64").is_none());
    }

    #[test]
    fn jogo_aberto_nao_toca_a_pasta() {
        use crate::model::Launcher;
        // Pasta inexistente com nome que denunciaria qualquer acesso: o teste só
        // verifica que a função devolve EmExecucao sem analisar.
        let mut j = DetectedGame::new(Launcher::Steam, "CS2", r"X:\nao-existe\cs2");
        j.exe_indicado = Some(PathBuf::from(r"X:\nao-existe\cs2\game\bin\win64\cs2.exe"));
        let sv = ServicosSistema::default();
        let r = analisar_se_fechado(&j, &sv, &[] as &[&str], &["explorer.exe", "CS2.EXE"]);
        assert_eq!(
            r,
            AnaliseJogo::EmExecucao {
                exes: vec!["CS2.EXE".into()]
            }
        );
        // Exe conhecido da varredura anterior (sem exe_indicado).
        let j2 = DetectedGame::new(Launcher::Riot, "LoL", r"X:\nao-existe\lol");
        let r = analisar_se_fechado(
            &j2,
            &sv,
            &[r"X:\nao-existe\lol\Game\League of Legends.exe"],
            &["league of legends.exe"],
        );
        assert!(matches!(r, AnaliseJogo::EmExecucao { .. }));
        // Fechado: analisa normalmente (pasta inexistente = fatos vazios).
        let r = analisar_se_fechado(&j2, &sv, &["League of Legends.exe"], &["explorer.exe"]);
        match r {
            AnaliseJogo::Analisado(t) => assert!(t.exe_principal.is_none()),
            outro => panic!("esperava Analisado, veio {outro:?}"),
        }
    }

    #[test]
    fn exes_conhecidos_sem_repeticao() {
        use crate::model::Launcher;
        let mut j = DetectedGame::new(Launcher::Epic, "F", r"X:\f");
        j.exe_indicado = Some(PathBuf::from(r"X:\f\FortniteClient-Win64-Shipping.exe"));
        let e = exes_conhecidos_do_jogo(&j, &["fortniteclient-win64-shipping.exe", "x/b.exe"]);
        assert_eq!(e, vec!["FortniteClient-Win64-Shipping.exe", "b.exe"]);
    }

    #[test]
    fn reflex_e_streamline() {
        let l = Listagem {
            arquivos: vec![
                arq(r"p\sl.reflex.dll"),
                arq(r"p\sl.interposer.dll"),
                arq(r"p\nvngx_dlss.dll"),
                arq(r"cef3\locales\sl.pak"),
            ],
            ..Default::default()
        };
        assert!(reflex(&l).valor);
        let d = dlss(&l, &mut |caminho| {
            Ok::<_, ()>(pe::ler_compartilhado(caminho))
        })
        .unwrap()
        .valor;
        assert!(d.presente());
        assert_eq!(d.streamline_modulos, vec!["interposer", "reflex"]);
        let vazio = Listagem::default();
        assert!(!reflex(&vazio).valor);
        assert!(
            !dlss(&vazio, &mut |caminho| Ok::<_, ()>(pe::ler_compartilhado(
                caminho
            )))
            .unwrap()
            .valor
            .presente()
        );
    }
}
