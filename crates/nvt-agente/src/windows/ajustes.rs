use std::{
    io,
    path::Path,
    ptr::{null, null_mut},
};

use nvt_core::{
    jogos::BaseJogos,
    usuario::{SettingUsuario as Destino, ValorUsuario},
};
use nvt_detect::tela::{TelaDoSistema, ler_tela_primaria};
use nvt_ipc::{Diagnostico, EstadoDiagnostico, Fato, FatoTela, RefreshRacional, SettingUsuario};
use nvt_windows::{
    ArmazemUsuario, Erro, LeituraUsuario as LeituraWindows,
    diagnosticos::{DiagnosticosWindows, diagnosticos_sessao},
    registro::{DestinoRegistro, Registro, RegistroWindows, ValorRegistro},
};
use windows_sys::Win32::{Foundation::ERROR_SUCCESS, System::Registry::RegEnumValueW};
use winreg::{RegKey, enums::HKEY_CURRENT_USER};

use crate::{EscritaUsuario, LeituraUsuario, ValorLido};

pub struct Ajustes {
    armazem: ArmazemUsuario,
    base: BaseJogos,
}

impl Ajustes {
    pub fn novo() -> io::Result<Self> {
        Ok(Self {
            armazem: ArmazemUsuario::default(),
            base: BaseJogos::embutida().map_err(io::Error::other)?,
        })
    }
}

fn destino(setting: &SettingUsuario) -> (Destino, Option<&str>) {
    match setting {
        SettingUsuario::Refresh { alvo } => (Destino::TelaRefresh, Some(alvo)),
        SettingUsuario::MouseAceleracao => (Destino::MouseAceleracao, None),
        SettingUsuario::DxSwapEffectUpgrade => (Destino::DxSwapEffectUpgrade, None),
        SettingUsuario::GameDvrCapturaHistorica => (Destino::GamedvrCapturaHistorica, None),
    }
}

fn leitura_windows(setting: &SettingUsuario, lido: &ValorLido) -> LeituraWindows {
    let (setting, alvo) = destino(setting);
    LeituraWindows {
        setting,
        valor: lido.valor.clone(),
        alvo_tela: alvo.map(str::to_owned),
        registro_ausente: match setting {
            Destino::DxSwapEffectUpgrade => !lido.registro_dx_presente,
            Destino::GamedvrCapturaHistorica => lido.valor == ValorUsuario::Ausente,
            _ => false,
        },
    }
}

impl LeituraUsuario for Ajustes {
    fn ler(&self, setting: &SettingUsuario) -> io::Result<ValorLido> {
        let (destino, alvo) = destino(setting);
        let lido = self.armazem.ler(destino, alvo).map_err(io::Error::other)?;
        let registro_dx_presente =
            destino == Destino::DxSwapEffectUpgrade && !lido.registro_ausente;
        let outros_tokens_dx =
            if registro_dx_presente && lido.valor == ValorUsuario::Ausente {
                // A fachada tipada já validou o REG_SZ. A releitura só decide se
                // um REG_SZ sem o token alvo precisou ser preservado.
                match RegistroWindows
                    .ler(DestinoRegistro::DirectX)
                    .map_err(io::Error::other)?
                {
                    Some(ValorRegistro::Sz(texto))
                        if !texto.split(';').filter_map(|t| t.split_once('=')).any(
                            |(chave, _)| chave.eq_ignore_ascii_case("SwapEffectUpgradeEnable"),
                        ) =>
                    {
                        !texto.is_empty()
                    }
                    _ => return Err(io::Error::other(Erro::Alterado)),
                }
            } else {
                false
            };
        Ok(ValorLido {
            valor: lido.valor,
            registro_dx_presente,
            outros_tokens_dx,
        })
    }

    fn tela(&self) -> Fato<FatoTela> {
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

    fn diagnosticos(&self) -> Vec<Diagnostico> {
        let fonte = DiagnosticosWindows;
        let caminhos = caminhos_fso(&self.base);
        let exes: Vec<_> = caminhos.iter().map(String::as_str).collect();
        let mut itens = diagnosticos_sessao(&fonte, &exes);
        itens.push(Diagnostico {
            codigo: "windows_11".into(),
            estado: fonte
                .windows_build()
                .map_or(EstadoDiagnostico::Indisponivel, |build| {
                    EstadoDiagnostico::Conhecido {
                        valor: build.to_string(),
                    }
                }),
        });
        itens
    }
}

fn caminhos_fso(base: &BaseJogos) -> Vec<String> {
    // Layers é indexado por caminho absoluto. A fachada interpreta os flags
    // somente para entradas cujo nome de executável consta na base.
    let raiz = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(chave) =
        raiz.open_subkey(r"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers")
    else {
        return Vec::new();
    };
    let mut caminhos = Vec::new();
    let mut buffer = vec![0; 16_384];
    for indice in 0.. {
        let mut tamanho = buffer.len() as u32;
        // Solicita apenas o nome. Os dados de entradas desconhecidas não são lidos.
        let status = unsafe {
            RegEnumValueW(
                chave.raw_handle(),
                indice,
                buffer.as_mut_ptr(),
                &mut tamanho,
                null(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if status != ERROR_SUCCESS {
            break;
        }
        let Ok(nome) = String::from_utf16(&buffer[..tamanho as usize]) else {
            continue;
        };
        let caminho = Path::new(&nome);
        if caminho.is_absolute()
            && nome.len() <= 1022
            && !nome.chars().any(char::is_control)
            && caminho
                .file_name()
                .and_then(|exe| exe.to_str())
                .is_some_and(|exe| base.casar(exe, None).is_some())
        {
            caminhos.push(nome);
            if caminhos.len() == 251 {
                break;
            }
        }
    }
    caminhos.sort_unstable();
    caminhos
}

impl EscritaUsuario for Ajustes {
    fn escrever(
        &mut self,
        setting: &SettingUsuario,
        anterior: &ValorLido,
        pedido: &ValorLido,
        restauracao: bool,
        autorizar: &mut dyn FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        let esperado = leitura_windows(setting, anterior);
        let portao = || {
            if super::janela::encerrando() {
                return Err(Erro::Bloqueado);
            }
            autorizar().map_err(|_| Erro::Bloqueado)
        };
        let resultado = if restauracao {
            self.armazem
                .restaurar(&esperado, &leitura_windows(setting, pedido), portao)
        } else {
            self.armazem.escrever(&esperado, &pedido.valor, portao)
        };
        resultado.map_err(io::Error::other)
    }
}

pub(crate) fn refresh_divergente(erro: &io::Error) -> bool {
    matches!(
        erro.get_ref().and_then(|e| e.downcast_ref::<Erro>()),
        Some(Erro::RefreshNaoConfirmado {
            compensacao: Err(_),
            ..
        })
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    #[ignore = "lê os ajustes reais da sessão do Windows"]
    fn leitura_real_sessao() {
        let ajustes = Ajustes::novo().unwrap();
        let tela = ajustes.tela();
        println!("Tela: {tela:?}");
        if let Fato::Conhecido(tela) = tela {
            println!(
                "Refresh por alvo: {:?}",
                ajustes.ler(&SettingUsuario::Refresh { alvo: tela.alvo })
            );
        }
        for destino in [
            SettingUsuario::MouseAceleracao,
            SettingUsuario::DxSwapEffectUpgrade,
            SettingUsuario::GameDvrCapturaHistorica,
        ] {
            println!("{destino:?}: {:?}", ajustes.ler(&destino));
        }
        println!("Diagnósticos: {:?}", ajustes.diagnosticos());
    }
}
