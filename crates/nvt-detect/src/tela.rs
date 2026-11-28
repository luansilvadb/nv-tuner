//! Leitura da tela primária na sessão interativa do usuário.

use std::num::NonZeroU32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshRacional {
    pub numerador: u32,
    pub denominador: NonZeroU32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatoTela {
    pub largura: u32,
    pub altura: u32,
    pub bits_por_pixel: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModoTela {
    pub formato: FormatoTela,
    pub refresh_hz: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaminhoAtivo {
    pub posicao_origem: (i32, i32),
    pub nome_gdi_origem: String,
    /// Ausente quando o alvo deixou de estar disponível ou não tem caminho estável.
    pub caminho_monitor: Option<String>,
    pub refresh_atual: RefreshRacional,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeituraTela {
    pub nome_gdi_origem: String,
    pub caminho_monitor: String,
    pub formato: FormatoTela,
    pub refresh_atual: RefreshRacional,
    /// Taxa nominal inteira retornada pela enumeração GDI.
    pub refresh_max_hz: u32,
    pub refresh_abaixo_do_maximo: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelaIndisponivel {
    SessaoSemTela,
    SemPrimario,
    PrimarioAmbiguo,
    AlvoIndisponivel,
    ModoAtualIndisponivel,
    SemModoCompativel,
    Win32(u32),
}

/// Consultas de tela usadas na sessão do agente.
pub trait FonteTela {
    fn caminhos_ativos(&self) -> Result<Vec<CaminhoAtivo>, TelaIndisponivel>;
    fn modos(
        &self,
        nome_gdi_origem: &str,
    ) -> Result<(FormatoTela, Vec<ModoTela>), TelaIndisponivel>;
}

pub fn ler_tela_primaria(fonte: &impl FonteTela) -> Result<LeituraTela, TelaIndisponivel> {
    let mut primarios = fonte
        .caminhos_ativos()?
        .into_iter()
        .filter(|caminho| caminho.posicao_origem == (0, 0));
    let primario = primarios.next().ok_or(TelaIndisponivel::SemPrimario)?;
    if primarios.next().is_some() {
        return Err(TelaIndisponivel::PrimarioAmbiguo);
    }
    let caminho_monitor = primario
        .caminho_monitor
        .filter(|caminho| !caminho.is_empty())
        .ok_or(TelaIndisponivel::AlvoIndisponivel)?;
    let (formato, modos) = fonte.modos(&primario.nome_gdi_origem)?;
    let refresh_max_hz = modos
        .iter()
        .filter(|modo| modo.formato == formato && modo.refresh_hz > 1)
        .map(|modo| modo.refresh_hz)
        .max()
        .ok_or(TelaIndisponivel::SemModoCompativel)?;
    let refresh_atual = primario.refresh_atual;

    Ok(LeituraTela {
        nome_gdi_origem: primario.nome_gdi_origem,
        caminho_monitor,
        formato,
        refresh_atual,
        refresh_max_hz,
        refresh_abaixo_do_maximo: u64::from(refresh_atual.numerador)
            < u64::from(refresh_max_hz) * u64::from(refresh_atual.denominador.get()),
    })
}

pub use win32::{TelaDoSistema, consultar_topologia, nome_alvo, nome_origem, texto_win32};

mod win32 {
    use super::{
        CaminhoAtivo, FonteTela, FormatoTela, ModoTela, RefreshRacional, TelaIndisponivel,
    };
    use std::{mem::size_of, num::NonZeroU32, ptr};
    use windows_sys::Win32::{
        Devices::Display::{
            DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO,
            DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_PATH_INFO,
            DISPLAYCONFIG_PATH_SOURCE_INFO, DISPLAYCONFIG_PATH_TARGET_INFO,
            DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
            DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
            QueryDisplayConfig,
        },
        Foundation::{ERROR_ACCESS_DENIED, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS},
        Graphics::Gdi::{
            DEVMODEW, DM_BITSPERPEL, DM_DISPLAYFREQUENCY, DM_PELSHEIGHT, DM_PELSWIDTH,
            ENUM_CURRENT_SETTINGS, ENUM_REGISTRY_SETTINGS, EnumDisplaySettingsExW,
        },
    };

    #[derive(Debug, Default)]
    pub struct TelaDoSistema;

    impl FonteTela for TelaDoSistema {
        fn caminhos_ativos(&self) -> Result<Vec<CaminhoAtivo>, TelaIndisponivel> {
            let (caminhos, modos) = consultar_topologia()?;
            caminhos
                .iter()
                .map(|caminho| ler_caminho(caminho, &modos))
                .collect()
        }

        fn modos(
            &self,
            nome_gdi_origem: &str,
        ) -> Result<(FormatoTela, Vec<ModoTela>), TelaIndisponivel> {
            let nome: Vec<u16> = nome_gdi_origem.encode_utf16().chain([0]).collect();
            let atual = modo_gdi(&nome, ENUM_CURRENT_SETTINGS)
                .and_then(|modo| formato_gdi(&modo))
                .ok_or(TelaIndisponivel::ModoAtualIndisponivel)?;
            let mut modos = Vec::new();
            for indice in 0..ENUM_REGISTRY_SETTINGS {
                let Some(modo) = modo_gdi(&nome, indice) else {
                    break;
                };
                if modo.dmFields & DM_DISPLAYFREQUENCY != 0 {
                    if let Some(formato) = formato_gdi(&modo) {
                        modos.push(ModoTela {
                            formato,
                            refresh_hz: modo.dmDisplayFrequency,
                        });
                    }
                }
            }
            Ok((atual, modos))
        }
    }

    fn verificar_status(codigo: u32) -> Result<(), TelaIndisponivel> {
        match codigo {
            ERROR_SUCCESS => Ok(()),
            ERROR_ACCESS_DENIED => Err(TelaIndisponivel::SessaoSemTela),
            codigo => Err(TelaIndisponivel::Win32(codigo)),
        }
    }

    /// Consulta os caminhos e modos ativos (`QDC_ONLY_ACTIVE_PATHS`), repetindo
    /// até quatro vezes se a topologia crescer entre o dimensionamento e a
    /// leitura. `ERROR_ACCESS_DENIED` vira [`TelaIndisponivel::SessaoSemTela`],
    /// topologia sem caminhos vira [`TelaIndisponivel::SemPrimario`] e outros
    /// códigos viram [`TelaIndisponivel::Win32`].
    pub fn consultar_topologia()
    -> Result<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>), TelaIndisponivel> {
        // Repete a consulta se a topologia crescer após o dimensionamento.
        for _ in 0..4 {
            let (mut num_caminhos, mut num_modos) = (0, 0);
            // SAFETY: ambos os ponteiros apontam para contadores graváveis.
            verificar_status(unsafe {
                GetDisplayConfigBufferSizes(
                    QDC_ONLY_ACTIVE_PATHS,
                    &mut num_caminhos,
                    &mut num_modos,
                )
            })?;
            if num_caminhos == 0 {
                return Err(TelaIndisponivel::SemPrimario);
            }
            let mut caminhos = vec![DISPLAYCONFIG_PATH_INFO::default(); num_caminhos as usize];
            let mut modos = vec![DISPLAYCONFIG_MODE_INFO::default(); num_modos as usize];
            // SAFETY: os vetores têm os comprimentos fornecidos nos contadores.
            // QDC_ONLY_ACTIVE_PATHS exige ponteiro nulo para o ID da topologia.
            let codigo = unsafe {
                QueryDisplayConfig(
                    QDC_ONLY_ACTIVE_PATHS,
                    &mut num_caminhos,
                    caminhos.as_mut_ptr(),
                    &mut num_modos,
                    modos.as_mut_ptr(),
                    ptr::null_mut(),
                )
            };
            if codigo == ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            verificar_status(codigo)?;
            caminhos.truncate(num_caminhos as usize);
            modos.truncate(num_modos as usize);
            return Ok((caminhos, modos));
        }
        Err(TelaIndisponivel::Win32(ERROR_INSUFFICIENT_BUFFER))
    }

    fn ler_caminho(
        caminho: &DISPLAYCONFIG_PATH_INFO,
        modos: &[DISPLAYCONFIG_MODE_INFO],
    ) -> Result<CaminhoAtivo, TelaIndisponivel> {
        let origem = caminho.sourceInfo;
        let alvo = caminho.targetInfo;
        // SAFETY: a consulta não usa QDC_VIRTUAL_MODE_AWARE, então o índice
        // de origem ocupa o campo modeInfoIdx inteiro.
        let indice = unsafe { origem.Anonymous.modeInfoIdx };
        let modo = modos
            .get(indice as usize)
            .filter(|modo| modo.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE)
            .ok_or(TelaIndisponivel::ModoAtualIndisponivel)?;
        // SAFETY: infoType identifica o membro sourceMode.
        let posicao = unsafe { modo.Anonymous.sourceMode.position };
        let taxa = alvo.refreshRate;

        let nome_gdi_origem = nome_origem(&origem)?;
        let caminho_monitor = if alvo.targetAvailable == 0 {
            None
        } else {
            nome_alvo(&alvo)?
        };

        let denominador = NonZeroU32::new(taxa.Denominator)
            .filter(|_| taxa.Numerator != 0)
            .ok_or(TelaIndisponivel::ModoAtualIndisponivel)?;
        Ok(CaminhoAtivo {
            posicao_origem: (posicao.x, posicao.y),
            nome_gdi_origem,
            caminho_monitor,
            refresh_atual: RefreshRacional {
                numerador: taxa.Numerator,
                denominador,
            },
        })
    }

    /// Nome GDI da origem (`viewGdiDeviceName`) via `DisplayConfigGetDeviceInfo`.
    /// Erros de status seguem o mapeamento de [`consultar_topologia`]; nome
    /// vazio ou inválido vira [`TelaIndisponivel::ModoAtualIndisponivel`].
    pub fn nome_origem(
        origem: &DISPLAYCONFIG_PATH_SOURCE_INFO,
    ) -> Result<String, TelaIndisponivel> {
        let mut nome = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                adapterId: origem.adapterId,
                id: origem.id,
            },
            ..Default::default()
        };
        // SAFETY: o ponteiro identifica o pacote inteiro, cujo tipo e tamanho
        // estão no cabeçalho inicial.
        verificar_status(
            unsafe { DisplayConfigGetDeviceInfo(ptr::from_mut(&mut nome).cast()) } as u32,
        )?;
        texto_win32(&nome.viewGdiDeviceName).ok_or(TelaIndisponivel::ModoAtualIndisponivel)
    }

    /// Caminho de dispositivo do monitor (`monitorDevicePath`) via
    /// `DisplayConfigGetDeviceInfo`. Erros de status seguem o mapeamento de
    /// [`consultar_topologia`]; nome vazio ou inválido devolve `Ok(None)`.
    /// Não verifica `targetAvailable`: isso cabe ao chamador.
    pub fn nome_alvo(
        alvo: &DISPLAYCONFIG_PATH_TARGET_INFO,
    ) -> Result<Option<String>, TelaIndisponivel> {
        let mut nome = DISPLAYCONFIG_TARGET_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                adapterId: alvo.adapterId,
                id: alvo.id,
            },
            ..Default::default()
        };
        // SAFETY: o ponteiro identifica o pacote inteiro, cujo tipo e tamanho
        // estão no cabeçalho inicial.
        verificar_status(
            unsafe { DisplayConfigGetDeviceInfo(ptr::from_mut(&mut nome).cast()) } as u32,
        )?;
        Ok(texto_win32(&nome.monitorDevicePath))
    }

    /// Texto UTF-16 terminado em NUL de um buffer Win32. `None` sem NUL, com
    /// texto vazio ou com UTF-16 inválido.
    pub fn texto_win32(buffer: &[u16]) -> Option<String> {
        let fim = buffer.iter().position(|&unidade| unidade == 0)?;
        (fim != 0)
            .then(|| String::from_utf16(&buffer[..fim]).ok())
            .flatten()
    }

    fn modo_gdi(nome: &[u16], indice: u32) -> Option<DEVMODEW> {
        let mut modo = DEVMODEW {
            dmSize: size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        // SAFETY: o nome vem de um buffer UTF-16 terminado em NUL e modo
        // tem dmSize inicializado e nenhum espaço extra de driver anunciado.
        let sucesso = unsafe { EnumDisplaySettingsExW(nome.as_ptr(), indice, &mut modo, 0) };
        (sucesso != 0).then_some(modo)
    }

    fn formato_gdi(modo: &DEVMODEW) -> Option<FormatoTela> {
        let campos = DM_PELSWIDTH | DM_PELSHEIGHT | DM_BITSPERPEL;
        (modo.dmFields & campos == campos).then_some(FormatoTela {
            largura: modo.dmPelsWidth,
            altura: modo.dmPelsHeight,
            bits_por_pixel: modo.dmBitsPerPel,
        })
    }
}
