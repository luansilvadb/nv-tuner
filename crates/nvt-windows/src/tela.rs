use std::{mem::size_of, ptr};

use nvt_core::usuario::RefreshRacional;
use nvt_detect::tela::{FonteTela, TelaDoSistema, TelaIndisponivel};
use windows_sys::Win32::{
    Devices::Display::*,
    Foundation::{
        ERROR_BAD_CONFIGURATION, ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_PARAMETER, ERROR_SUCCESS,
        LUID,
    },
};

use crate::{Erro, mesma_taxa};

const INDICE_INVALIDO: u32 = u32::MAX;

pub trait Display {
    type Configuracao;

    fn ler(&self, alvo: &str) -> Result<RefreshRacional, Erro>;
    /// Reconstroi e valida os vetores a partir da configuracao ativa.
    fn preparar(
        &self,
        alvo: &str,
        taxa: RefreshRacional,
    ) -> Result<(RefreshRacional, Self::Configuracao), Erro>;
    /// Confere que a configuracao ativa ainda e a preparada. Chamado antes do portao.
    fn conferir(&self, configuracao: &Self::Configuracao) -> Result<(), Erro>;
    /// Apenas envia os vetores preparados, sem consultas.
    fn gravar(&self, configuracao: &Self::Configuracao) -> Result<(), Erro>;
    /// Rele depois de gravar e exige o estado esperado inteiro (topologia,
    /// resolucao, tamanho ativo, profundidade, rotacao, escala e varredura),
    /// com a taxa do alvo igual a pedida.
    fn confirmar(&self, configuracao: &Self::Configuracao) -> Result<(), Erro>;
    /// Prepara a volta aos vetores originais consultados em `preparar`
    /// (inclusive o modo de destino), conferida contra o estado observado agora.
    fn preparar_compensacao(
        &self,
        configuracao: Self::Configuracao,
    ) -> Result<Self::Configuracao, Erro>;
}

#[derive(Debug, Default)]
pub struct DisplayWindows;

/// Vetores transitorios para uma unica tentativa de escrita.
pub struct ConfiguracaoDisplay {
    /// Estado que deve estar ativo imediatamente antes de gravar.
    ativa: Vec<EstadoCaminho>,
    /// Estado exigido depois de gravar, exceto a taxa do alvo.
    esperada: Vec<EstadoCaminho>,
    alvo: String,
    pedida: RefreshRacional,
    anterior: RefreshRacional,
    vetores: Vetores,
    /// Vetores como consultados, com o modo de destino original.
    original: Vetores,
}

#[derive(Clone)]
struct Vetores {
    caminhos: Vec<DISPLAYCONFIG_PATH_INFO>,
    modos: Vec<DISPLAYCONFIG_MODE_INFO>,
}

#[derive(Clone, PartialEq, Eq)]
struct EstadoCaminho {
    origem: (i32, u32, u32),
    destino: (i32, u32, u32),
    posicao: (i32, i32),
    formato_origem: (u32, u32, DISPLAYCONFIG_PIXELFORMAT),
    dimensoes_sinal: (u32, u32),
    taxa: RefreshRacional,
    rotacao: DISPLAYCONFIG_ROTATION,
    escala: DISPLAYCONFIG_SCALING,
    varredura: (
        DISPLAYCONFIG_SCANLINE_ORDERING,
        DISPLAYCONFIG_SCANLINE_ORDERING,
    ),
}

impl EstadoCaminho {
    fn igual_exceto_taxa(&self, outro: &Self) -> bool {
        Self {
            taxa: outro.taxa,
            ..self.clone()
        } == *outro
    }
}

impl Display for DisplayWindows {
    type Configuracao = ConfiguracaoDisplay;

    fn ler(&self, alvo: &str) -> Result<RefreshRacional, Erro> {
        let config = consultar()?;
        let indice = config.identificar(alvo)?;
        taxa(&config.caminhos[indice])
    }

    fn preparar(
        &self,
        alvo: &str,
        pedida: RefreshRacional,
    ) -> Result<(RefreshRacional, Self::Configuracao), Erro> {
        if !pedida.valido() {
            return Err(Erro::ValorInvalido);
        }
        let mut vetores = consultar()?;
        let indice = vetores.identificar(alvo)?;
        let caminho = vetores.caminhos[indice];
        let ativa = vetores.estados()?;
        let anterior = taxa(&caminho)?;
        let original = vetores.clone();
        let configuracao = |vetores| ConfiguracaoDisplay {
            esperada: ativa.clone(),
            ativa: ativa.clone(),
            alvo: alvo.to_owned(),
            pedida,
            anterior,
            vetores,
            original: original.clone(),
        };
        if mesma_taxa(anterior, pedida) {
            return Ok((anterior, configuracao(vetores)));
        }
        let nome_gdi = nome_origem(&caminho.sourceInfo)?;
        let (formato, modos) = TelaDoSistema.modos(&nome_gdi).map_err(|e| match e {
            TelaIndisponivel::Win32(codigo) => Erro::Win32(codigo),
            _ => Erro::ModoIndisponivel,
        })?;
        // GDI enumera taxas nominais em Hz inteiros. A taxa exata e enviada
        // intacta a CCD, validada pelo driver e conferida depois de gravar.
        let den = u64::from(pedida.denominador);
        let num = u64::from(pedida.numerador);
        let (inteiro, nominal) = (num / den, (num + den / 2) / den);
        if !modos.iter().any(|m| {
            m.formato == formato
                && u64::from(m.refresh_hz) >= inteiro
                && u64::from(m.refresh_hz) <= nominal
                && m.refresh_hz > 1
        }) {
            return Err(Erro::ModoIndisponivel);
        }

        vetores.alterar_taxa(indice, pedida)?;
        vetores.enviar(SDC_VALIDATE).map_err(|e| match e {
            Erro::Win32(ERROR_INVALID_PARAMETER | ERROR_BAD_CONFIGURATION) => {
                Erro::ModoIndisponivel
            }
            _ => e,
        })?;
        Ok((anterior, configuracao(vetores)))
    }

    fn conferir(&self, configuracao: &Self::Configuracao) -> Result<(), Erro> {
        let atual = consultar()?;
        atual.identificar(&configuracao.alvo)?;
        if atual.estados()? != configuracao.ativa {
            return Err(Erro::Alterado);
        }
        Ok(())
    }

    fn gravar(&self, configuracao: &Self::Configuracao) -> Result<(), Erro> {
        configuracao
            .vetores
            .enviar(SDC_APPLY | SDC_SAVE_TO_DATABASE)
    }

    fn confirmar(&self, configuracao: &Self::Configuracao) -> Result<(), Erro> {
        let atual = consultar()?;
        let indice = atual.identificar(&configuracao.alvo)?;
        let estados = atual.estados()?;
        if estados.len() != configuracao.esperada.len() {
            return Err(Erro::Releitura);
        }
        let divergente =
            estados
                .iter()
                .zip(&configuracao.esperada)
                .enumerate()
                .any(|(i, (lido, esperado))| {
                    if i == indice {
                        !lido.igual_exceto_taxa(esperado)
                            || !mesma_taxa(lido.taxa, configuracao.pedida)
                    } else {
                        lido != esperado
                    }
                });
        if divergente {
            return Err(Erro::Releitura);
        }
        Ok(())
    }

    fn preparar_compensacao(
        &self,
        configuracao: Self::Configuracao,
    ) -> Result<Self::Configuracao, Erro> {
        let atual = consultar()?;
        atual.identificar(&configuracao.alvo)?;
        Ok(ConfiguracaoDisplay {
            ativa: atual.estados()?,
            esperada: configuracao.esperada,
            alvo: configuracao.alvo,
            pedida: configuracao.anterior,
            anterior: configuracao.pedida,
            vetores: configuracao.original.clone(),
            original: configuracao.original,
        })
    }
}

impl Vetores {
    fn alterar_taxa(&mut self, indice: usize, pedida: RefreshRacional) -> Result<(), Erro> {
        // Um target mode explicito sobrepoe refreshRate com o timing antigo.
        // Fornecer um target mode com a taxa nova exigiria sintetizar um timing
        // completo (pixelRate, totalSize, hSync/vSync) que a CCD nao enumera nem
        // garante aceitar; por isso o modo e removido e o driver escolhe o
        // timing do alvo. Como ele pode trocar mais que a taxa, `confirmar`
        // rele o estado inteiro e `escrever` compensa com os vetores originais.
        // SAFETY: QDC_ONLY_ACTIVE_PATHS, sem modo virtual, usa modeInfoIdx.
        let modo_alvo = unsafe { self.caminhos[indice].targetInfo.Anonymous.modeInfoIdx };
        // SAFETY: a mesma consulta garante o membro nao virtual em cada caminho.
        if self
            .caminhos
            .iter()
            .enumerate()
            .any(|(i, c)| i != indice && unsafe { c.targetInfo.Anonymous.modeInfoIdx } == modo_alvo)
        {
            return Err(Erro::AlvoAmbiguo);
        }
        self.modos.remove(modo_alvo as usize);
        for (i, c) in self.caminhos.iter_mut().enumerate() {
            // SAFETY: o formato nao virtual usa apenas modeInfoIdx na origem e no alvo.
            let (origem, destino) = unsafe {
                (
                    c.sourceInfo.Anonymous.modeInfoIdx,
                    c.targetInfo.Anonymous.modeInfoIdx,
                )
            };
            c.sourceInfo.Anonymous.modeInfoIdx = reindexar(origem, modo_alvo);
            c.targetInfo.Anonymous.modeInfoIdx = if i == indice {
                INDICE_INVALIDO
            } else {
                reindexar(destino, modo_alvo)
            };
        }
        self.caminhos[indice].targetInfo.refreshRate = DISPLAYCONFIG_RATIONAL {
            Numerator: pedida.numerador,
            Denominator: pedida.denominador,
        };
        Ok(())
    }

    fn identificar(&self, alvo: &str) -> Result<usize, Erro> {
        let mut encontrado = None;
        for (i, caminho) in self.caminhos.iter().enumerate() {
            let info = &caminho.targetInfo;
            if info.targetAvailable != 0
                && nome_alvo(info)?.eq_ignore_ascii_case(alvo)
                && encontrado.replace(i).is_some()
            {
                return Err(Erro::AlvoAmbiguo);
            }
        }
        let indice = encontrado.ok_or(Erro::AlvoIndisponivel)?;
        let origem = self.origem(&self.caminhos[indice].sourceInfo)?;
        for (i, caminho) in self.caminhos.iter().enumerate() {
            if i == indice {
                continue;
            }
            let outra = self.origem(&caminho.sourceInfo)?;
            if origem.position.x == outra.position.x && origem.position.y == outra.position.y {
                return Err(Erro::Clone);
            }
        }
        Ok(indice)
    }

    fn origem(
        &self,
        origem: &DISPLAYCONFIG_PATH_SOURCE_INFO,
    ) -> Result<DISPLAYCONFIG_SOURCE_MODE, Erro> {
        // SAFETY: a consulta sem QDC_VIRTUAL_MODE_AWARE usa modeInfoIdx.
        let indice = unsafe { origem.Anonymous.modeInfoIdx };
        let modo = self
            .modos
            .get(indice as usize)
            .filter(|modo| {
                modo.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE
                    && modo.id == origem.id
                    && mesmo_adaptador(modo.adapterId, origem.adapterId)
            })
            .ok_or(Erro::ModoIndisponivel)?;
        // SAFETY: infoType identifica sourceMode.
        Ok(unsafe { modo.Anonymous.sourceMode })
    }

    fn estados(&self) -> Result<Vec<EstadoCaminho>, Erro> {
        self.caminhos
            .iter()
            .map(|caminho| {
                let origem = self.origem(&caminho.sourceInfo)?;
                let alvo = caminho.targetInfo;
                if alvo.targetAvailable == 0 {
                    return Err(Erro::AlvoIndisponivel);
                }
                // SAFETY: a consulta sem QDC_VIRTUAL_MODE_AWARE usa modeInfoIdx.
                let indice = unsafe { alvo.Anonymous.modeInfoIdx };
                let modo = self
                    .modos
                    .get(indice as usize)
                    .filter(|modo| {
                        modo.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_TARGET
                            && modo.id == alvo.id
                            && mesmo_adaptador(modo.adapterId, alvo.adapterId)
                    })
                    .ok_or(Erro::ModoIndisponivel)?;
                // SAFETY: infoType identifica targetMode.
                let sinal = unsafe { modo.Anonymous.targetMode.targetVideoSignalInfo };
                let adaptador = |luid: LUID, id| (luid.HighPart, luid.LowPart, id);
                Ok(EstadoCaminho {
                    origem: adaptador(caminho.sourceInfo.adapterId, caminho.sourceInfo.id),
                    destino: adaptador(alvo.adapterId, alvo.id),
                    posicao: (origem.position.x, origem.position.y),
                    formato_origem: (origem.width, origem.height, origem.pixelFormat),
                    dimensoes_sinal: (sinal.activeSize.cx, sinal.activeSize.cy),
                    taxa: taxa(caminho)?,
                    rotacao: alvo.rotation,
                    escala: alvo.scaling,
                    varredura: (alvo.scanLineOrdering, sinal.scanLineOrdering),
                })
            })
            .collect()
    }

    fn enviar(&self, flags: SET_DISPLAY_CONFIG_FLAGS) -> Result<(), Erro> {
        // SAFETY: os vetores estao vivos e os comprimentos sao os da consulta,
        // com indices ajustados para a remocao seletiva do modo de destino.
        verificar(unsafe {
            SetDisplayConfig(
                self.caminhos.len() as u32,
                self.caminhos.as_ptr(),
                self.modos.len() as u32,
                self.modos.as_ptr(),
                flags | SDC_USE_SUPPLIED_DISPLAY_CONFIG,
            )
        } as u32)
    }
}

fn reindexar(indice: u32, removido: u32) -> u32 {
    if indice != INDICE_INVALIDO && indice > removido {
        indice - 1
    } else {
        indice
    }
}

fn consultar() -> Result<Vetores, Erro> {
    for _ in 0..4 {
        let (mut caminhos, mut modos) = (0, 0);
        // SAFETY: ponteiros para dois contadores gravaveis.
        verificar(unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut caminhos, &mut modos)
        })?;
        if caminhos == 0 {
            return Err(Erro::AlvoIndisponivel);
        }
        let mut config = Vetores {
            caminhos: vec![DISPLAYCONFIG_PATH_INFO::default(); caminhos as usize],
            modos: vec![DISPLAYCONFIG_MODE_INFO::default(); modos as usize],
        };
        // SAFETY: buffers dimensionados pelos contadores. A consulta ativa
        // exige ponteiro nulo para o identificador de topologia.
        let codigo = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut caminhos,
                config.caminhos.as_mut_ptr(),
                &mut modos,
                config.modos.as_mut_ptr(),
                ptr::null_mut(),
            )
        };
        if codigo == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        verificar(codigo)?;
        config.caminhos.truncate(caminhos as usize);
        config.modos.truncate(modos as usize);
        return Ok(config);
    }
    Err(Erro::Win32(ERROR_INSUFFICIENT_BUFFER))
}

fn taxa(caminho: &DISPLAYCONFIG_PATH_INFO) -> Result<RefreshRacional, Erro> {
    let taxa = caminho.targetInfo.refreshRate;
    let taxa = RefreshRacional {
        numerador: taxa.Numerator,
        denominador: taxa.Denominator,
    };
    taxa.valido().then_some(taxa).ok_or(Erro::ModoIndisponivel)
}

fn nome_alvo(alvo: &DISPLAYCONFIG_PATH_TARGET_INFO) -> Result<String, Erro> {
    let mut nome = DISPLAYCONFIG_TARGET_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
            adapterId: alvo.adapterId,
            id: alvo.id,
        },
        ..Default::default()
    };
    // SAFETY: o cabecalho inicial descreve a estrutura inteira e o buffer e gravavel.
    verificar(unsafe { DisplayConfigGetDeviceInfo(ptr::from_mut(&mut nome).cast()) } as u32)?;
    texto(&nome.monitorDevicePath).ok_or(Erro::AlvoIndisponivel)
}

fn nome_origem(origem: &DISPLAYCONFIG_PATH_SOURCE_INFO) -> Result<String, Erro> {
    let mut nome = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
            adapterId: origem.adapterId,
            id: origem.id,
        },
        ..Default::default()
    };
    // SAFETY: o cabecalho inicial descreve a estrutura inteira e o buffer e gravavel.
    verificar(unsafe { DisplayConfigGetDeviceInfo(ptr::from_mut(&mut nome).cast()) } as u32)?;
    texto(&nome.viewGdiDeviceName).ok_or(Erro::ModoIndisponivel)
}

fn texto(buffer: &[u16]) -> Option<String> {
    let fim = buffer.iter().position(|&u| u == 0)?;
    (fim > 0).then_some(())?;
    String::from_utf16(&buffer[..fim]).ok()
}

fn mesmo_adaptador(a: LUID, b: LUID) -> bool {
    a.LowPart == b.LowPart && a.HighPart == b.HighPart
}

fn verificar(codigo: u32) -> Result<(), Erro> {
    if codigo == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(Erro::Win32(codigo))
    }
}

pub(crate) fn escrever(
    display: &impl Display,
    alvo: &str,
    antes: RefreshRacional,
    depois: RefreshRacional,
    autorizar: &mut impl FnMut() -> Result<(), Erro>,
) -> Result<(), Erro> {
    let (atual, config) = display.preparar(alvo, depois)?;
    if !mesma_taxa(atual, antes) {
        return Err(Erro::Alterado);
    }
    if mesma_taxa(atual, depois) {
        return Ok(());
    }
    display.conferir(&config)?;
    autorizar()?;
    display.gravar(&config)?;
    let falha = match display.confirmar(&config) {
        Ok(()) => return Ok(()),
        Err(falha) => falha,
    };
    // Sem modo de destino o driver escolhe o timing e pode trocar mais que a
    // taxa. Volta aos vetores originais completos, com portao colado.
    let compensacao = (|| {
        let config = display.preparar_compensacao(config)?;
        display.conferir(&config)?;
        autorizar()?;
        display.gravar(&config)?;
        display.confirmar(&config)
    })();
    Err(Erro::RefreshNaoConfirmado {
        falha: Box::new(falha),
        compensacao: compensacao.map_err(Box::new),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAMINHO_ATIVO: u32 = 1;

    #[test]
    fn vetores_preservam_origens_e_o_outro_alvo() {
        let caminho = |id, origem, alvo| {
            let mut caminho = DISPLAYCONFIG_PATH_INFO::default();
            caminho.sourceInfo.id = id;
            caminho.sourceInfo.Anonymous.modeInfoIdx = origem;
            caminho.targetInfo.id = id;
            caminho.targetInfo.Anonymous.modeInfoIdx = alvo;
            caminho.targetInfo.targetAvailable = 1;
            caminho.targetInfo.refreshRate = DISPLAYCONFIG_RATIONAL {
                Numerator: 60,
                Denominator: 1,
            };
            caminho.flags = CAMINHO_ATIVO;
            caminho
        };
        let origem = |id, largura| {
            let mut modo = DISPLAYCONFIG_MODE_INFO {
                infoType: DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
                id,
                ..Default::default()
            };
            modo.Anonymous.sourceMode = DISPLAYCONFIG_SOURCE_MODE {
                width: largura,
                height: 1080,
                pixelFormat: DISPLAYCONFIG_PIXELFORMAT_32BPP,
                ..Default::default()
            };
            modo
        };
        let alvo = |id| DISPLAYCONFIG_MODE_INFO {
            infoType: DISPLAYCONFIG_MODE_INFO_TYPE_TARGET,
            id,
            ..Default::default()
        };
        let mut vetores = Vetores {
            caminhos: vec![caminho(1, 0, 1), caminho(2, 2, 3)],
            modos: vec![origem(1, 1920), alvo(1), origem(2, 2560), alvo(2)],
        };
        let pedida = RefreshRacional {
            numerador: 144_000,
            denominador: 1_001,
        };
        vetores.alterar_taxa(0, pedida).unwrap();
        let primeiro = vetores.caminhos[0];
        let segundo = vetores.caminhos[1];
        assert_eq!(taxa(&primeiro), Ok(pedida));
        assert_eq!(
            taxa(&segundo),
            Ok(RefreshRacional {
                numerador: 60,
                denominador: 1
            })
        );
        // SAFETY: os caminhos de teste foram preenchidos com os indices nao virtuais.
        assert_eq!(
            unsafe { primeiro.targetInfo.Anonymous.modeInfoIdx },
            INDICE_INVALIDO
        );
        // SAFETY: o outro alvo continua referenciando seu modo original.
        assert_eq!(unsafe { segundo.targetInfo.Anonymous.modeInfoIdx }, 2);
        assert_eq!(vetores.modos[2].id, segundo.targetInfo.id);
        for (caminho, largura) in [(primeiro, 1920), (segundo, 2560)] {
            let origem = vetores.origem(&caminho.sourceInfo).unwrap();
            assert_eq!(
                (origem.width, origem.height, origem.pixelFormat),
                (largura, 1080, DISPLAYCONFIG_PIXELFORMAT_32BPP)
            );
            assert_eq!(caminho.flags, CAMINHO_ATIVO);
        }
    }
}
