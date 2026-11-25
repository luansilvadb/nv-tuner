//! Fatos do jogo levantados em disco: exe principal, API gráfica, DLSS,
//! Reflex e anticheat, cada um com nível de confiança.

pub mod anticheat;
pub mod arquivos;
pub mod exe;
pub mod pe;

use std::path::PathBuf;

use crate::model::DetectedGame;
use crate::registro::ServicosSistema;
use arquivos::{Arquivo, Listagem};

pub use anticheat::FatoAnticheat;
pub use exe::{CandidatoExe, MotivoExe};

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

    pub fn tem_streamline(&self) -> bool {
        !self.streamline_modulos.is_empty()
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

fn dll_nvidia(listagem: &Listagem, nome: &str) -> Option<DllNvidia> {
    listagem
        .arquivos
        .iter()
        .filter(|a| a.nome == nome)
        .map(|a| DllNvidia {
            caminho: a.caminho.clone(),
            versao: pe::versao(&a.caminho).ok().and_then(|v| v.versao),
        })
        // Várias cópias: fica a de maior versão.
        .max_by(|x, y| {
            pe::comparar_versoes(
                x.versao.as_deref().unwrap_or("0"),
                y.versao.as_deref().unwrap_or("0"),
            )
        })
}

fn dlss(listagem: &Listagem) -> Fato<FatosDlss> {
    let mut f = FatosDlss {
        super_resolution: dll_nvidia(listagem, "nvngx_dlss.dll"),
        ray_reconstruction: dll_nvidia(listagem, "nvngx_dlssd.dll"),
        frame_generation: dll_nvidia(listagem, "nvngx_dlssg.dll"),
        deep_dvc: dll_nvidia(listagem, "nvngx_deepdvc.dll"),
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
    f.streamline_versao = dll_nvidia(listagem, "sl.interposer.dll").and_then(|d| d.versao);
    let evid = match &f.super_resolution {
        Some(d) => format!(
            "nvngx_dlss.dll {}",
            d.versao.as_deref().unwrap_or("(versão ilegível)")
        ),
        None => "nenhum nvngx_dlss.dll na instalação".into(),
    };
    // Ausência em disco é forte para DLSS (a DLL precisa ser distribuída com o jogo).
    Fato::new(f, Confianca::Alta, evid)
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

/// Igual a [`analisar`], a partir de uma listagem já feita.
pub fn analisar_listagem(
    jogo: &DetectedGame,
    listagem: &Listagem,
    servicos: &ServicosSistema,
) -> GameTraits {
    let candidatos = exe::candidatos(listagem, jogo.exe_indicado.as_deref());
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
        match pe::imports(&c.caminho) {
            Ok(imp) => {
                let sistema = imp.dlls.iter().any(|d| {
                    d == "kernel32.dll" || d == "ntdll.dll" || d.starts_with("api-ms-win-")
                });
                // Exe normal importa kernel32; tabela mínima sem ele = protegido/cifrado.
                ofuscados = !sistema || (imp.delay_ilegiveis > 0 && imp.dlls.len() <= 3);
                api = api_de_imports(&imp.dlls);
                imports = imp.dlls;
            }
            Err(_) => ofuscados = true,
        }
    }
    let api = api
        .or_else(|| principal.and_then(|_| api_por_vizinhos(listagem, &exe_dir_rel)))
        .map(|(v, c, e)| Fato::new(Some(v), c, e))
        .unwrap_or_else(|| Fato::new(None, Confianca::Baixa, "nenhum indício de API"));

    GameTraits {
        exe_principal: principal.map(|c| c.caminho.clone()),
        candidatos_exe: candidatos,
        api,
        imports,
        imports_ofuscados: ofuscados,
        dlss: dlss(listagem),
        reflex: reflex(listagem),
        anticheats: anticheat::detectar(listagem, jogo.exige_vanguard, servicos),
        listagem_truncada: listagem.truncada,
    }
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
        let d = dlss(&l).valor;
        assert!(d.presente());
        assert_eq!(d.streamline_modulos, vec!["interposer", "reflex"]);
        let vazio = Listagem::default();
        assert!(!reflex(&vazio).valor);
        assert!(!dlss(&vazio).valor.presente());
    }
}
