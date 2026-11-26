use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
};

use nvt_core::usuario::{RefreshRacional, SettingUsuario::*, ValorUsuario};
use nvt_windows::{
    ArmazemUsuario, Erro,
    diagnosticos::*,
    mouse::Spi,
    registro::{DestinoRegistro, Registro, ValorRegistro},
    tela::Display,
};

#[derive(Default)]
struct RegistroFalso {
    valores: RefCell<BTreeMap<DestinoRegistro, ValorRegistro>>,
    escritas: Cell<usize>,
    ignorar: Cell<bool>,
}

impl RegistroFalso {
    fn definir(&self, destino: DestinoRegistro, valor: Option<ValorRegistro>) {
        let mut valores = self.valores.borrow_mut();
        match valor {
            Some(valor) => {
                valores.insert(destino, valor);
            }
            None => {
                valores.remove(&destino);
            }
        }
    }
}

impl Registro for &RegistroFalso {
    fn ler(&self, destino: DestinoRegistro) -> Result<Option<ValorRegistro>, Erro> {
        Ok(self.valores.borrow().get(&destino).cloned())
    }

    fn gravar_captura(&self, valor: Option<bool>) -> Result<(), Erro> {
        self.escritas.set(self.escritas.get() + 1);
        if !self.ignorar.get() {
            self.definir(
                DestinoRegistro::Captura,
                valor.map(|v| ValorRegistro::Dword(u32::from(v))),
            );
        }
        Ok(())
    }

    fn gravar_directx(&self, esperado: Option<&str>, valor: Option<&str>) -> Result<(), Erro> {
        if self.ler(DestinoRegistro::DirectX)? != esperado.map(|s| ValorRegistro::Sz(s.into())) {
            return Err(Erro::Alterado);
        }
        self.escritas.set(self.escritas.get() + 1);
        if !self.ignorar.get() {
            self.definir(
                DestinoRegistro::DirectX,
                valor.map(|s| ValorRegistro::Sz(s.into())),
            );
        }
        Ok(())
    }
}

struct SpiFalso {
    trio: Cell<[i32; 3]>,
    escritas: Cell<usize>,
    ignorar: Cell<bool>,
}

impl Default for SpiFalso {
    fn default() -> Self {
        Self {
            trio: Cell::new([5, 12, 2]),
            escritas: Cell::default(),
            ignorar: Cell::default(),
        }
    }
}

impl Spi for &SpiFalso {
    fn ler(&self) -> Result<[i32; 3], Erro> {
        Ok(self.trio.get())
    }

    fn gravar(&self, trio: [i32; 3]) -> Result<(), Erro> {
        self.escritas.set(self.escritas.get() + 1);
        if !self.ignorar.get() {
            self.trio.set(trio);
        }
        Ok(())
    }
}

const ANTES: RefreshRacional = RefreshRacional {
    numerador: 60_000,
    denominador: 1_001,
};
const DEPOIS: RefreshRacional = RefreshRacional {
    numerador: 144_000,
    denominador: 1_001,
};
const DIVERGENTE: RefreshRacional = RefreshRacional {
    numerador: 144,
    denominador: 1,
};

struct DisplayFalso {
    taxa: Cell<RefreshRacional>,
    falha: RefCell<Option<Erro>>,
    resultados: RefCell<VecDeque<RefreshRacional>>,
    preparadas: RefCell<Vec<RefreshRacional>>,
    escritas: RefCell<Vec<RefreshRacional>>,
}

impl Default for DisplayFalso {
    fn default() -> Self {
        Self {
            taxa: Cell::new(ANTES),
            falha: RefCell::default(),
            resultados: RefCell::default(),
            preparadas: RefCell::default(),
            escritas: RefCell::default(),
        }
    }
}

impl Display for &DisplayFalso {
    type Configuracao = RefreshRacional;

    fn ler(&self, alvo: &str) -> Result<RefreshRacional, Erro> {
        if alvo != "monitor-estavel" {
            return Err(Erro::AlvoIndisponivel);
        }
        match self.falha.borrow().as_ref() {
            Some(e) => Err(e.clone()),
            None => Ok(self.taxa.get()),
        }
    }

    fn preparar(
        &self,
        alvo: &str,
        taxa: RefreshRacional,
    ) -> Result<(RefreshRacional, Self::Configuracao), Erro> {
        self.preparadas.borrow_mut().push(taxa);
        Ok((self.ler(alvo)?, taxa))
    }

    fn gravar(&self, taxa: Self::Configuracao) -> Result<(), Erro> {
        self.escritas.borrow_mut().push(taxa);
        self.taxa
            .set(self.resultados.borrow_mut().pop_front().unwrap_or(taxa));
        Ok(())
    }
}

fn autorizar() -> Result<(), Erro> {
    Ok(())
}

#[test]
fn captura_so_corrige_um_e_restaura_snapshot_inclusive_ausencia() {
    let r = RegistroFalso::default();
    let s = SpiFalso::default();
    let d = DisplayFalso::default();
    let a = ArmazemUsuario::novo(&r, &s, &d);
    let ausente = a.ler(GamedvrCapturaHistorica, None).unwrap();
    assert_eq!(ausente.valor, ValorUsuario::Ausente);
    assert!(ausente.registro_ausente);
    assert_eq!(
        a.escrever(&ausente, &ValorUsuario::Dword(0), autorizar),
        Err(Erro::ValorInvalido)
    );
    r.definir(DestinoRegistro::Captura, Some(ValorRegistro::Dword(1)));
    let antes = a.ler(GamedvrCapturaHistorica, None).unwrap();
    a.escrever(&antes, &ValorUsuario::Dword(0), autorizar)
        .unwrap();
    let atual = a.ler(GamedvrCapturaHistorica, None).unwrap();
    assert_eq!(atual.valor, ValorUsuario::Dword(0));
    a.restaurar(&atual, &antes, autorizar).unwrap();
    assert_eq!(a.ler(GamedvrCapturaHistorica, None).unwrap(), antes);
    a.restaurar(&antes, &ausente, autorizar).unwrap();
    assert_eq!(a.ler(GamedvrCapturaHistorica, None).unwrap(), ausente);
    assert_eq!(r.escritas.get(), 3);
}

#[test]
fn directx_preserva_vazio_ausencia_e_edicoes_concorrentes() {
    let r = RegistroFalso::default();
    let s = SpiFalso::default();
    let d = DisplayFalso::default();
    let a = ArmazemUsuario::novo(&r, &s, &d);
    for inicial in [None, Some(""), Some("Outros=á;")] {
        let destino = DestinoRegistro::DirectX;
        r.definir(destino, inicial.map(|v| ValorRegistro::Sz(v.into())));
        let original = a.ler(DxSwapEffectUpgrade, None).unwrap();
        assert_eq!(original.valor, ValorUsuario::Ausente);
        assert_eq!(original.registro_ausente, inicial.is_none());
        a.escrever(&original, &ValorUsuario::Token("1".into()), autorizar)
            .unwrap();
        let esperado = a.ler(DxSwapEffectUpgrade, None).unwrap();
        a.restaurar(&esperado, &original, autorizar).unwrap();
        assert_eq!(
            (&r).ler(destino).unwrap(),
            inicial.map(|v| ValorRegistro::Sz(v.into()))
        );
    }
    r.definir(DestinoRegistro::DirectX, None);
    let original = a.ler(DxSwapEffectUpgrade, None).unwrap();
    let mut chamadas = 0;
    a.escrever(&original, &ValorUsuario::Token("1".into()), || {
        chamadas += 1;
        if chamadas == 1 {
            r.definir(
                DestinoRegistro::DirectX,
                Some(ValorRegistro::Sz("Terceiro=á🦀;".into())),
            );
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(chamadas, 2);
    let esperado = a.ler(DxSwapEffectUpgrade, None).unwrap();
    a.restaurar(&esperado, &original, autorizar).unwrap();
    assert_eq!(
        (&r).ler(DestinoRegistro::DirectX).unwrap(),
        Some(ValorRegistro::Sz("Terceiro=á🦀;".into()))
    );
}

#[test]
fn registro_recusa_tipo_formato_deriva_e_detecta_gravacao_sem_efeito() {
    let r = RegistroFalso::default();
    let s = SpiFalso::default();
    let d = DisplayFalso::default();
    let a = ArmazemUsuario::novo(&r, &s, &d);
    let original = a.ler(DxSwapEffectUpgrade, None).unwrap();
    for (bruto, erro) in [
        (ValorRegistro::Dword(1), Erro::ValorInvalido),
        (ValorRegistro::Invalido, Erro::ValorInvalido),
        (
            ValorRegistro::Sz("Outros=1;outros=2;".into()),
            Erro::FormatoAmbiguo,
        ),
        (
            ValorRegistro::Sz("SwapEffectUpgradeEnable=1;".into()),
            Erro::Alterado,
        ),
    ] {
        r.definir(DestinoRegistro::DirectX, Some(bruto));
        assert_eq!(
            a.escrever(&original, &ValorUsuario::Token("0".into()), autorizar),
            Err(erro)
        );
    }
    assert_eq!(r.escritas.get(), 0);
    r.definir(DestinoRegistro::DirectX, None);
    r.ignorar.set(true);
    assert_eq!(
        a.escrever(&original, &ValorUsuario::Token("1".into()), autorizar),
        Err(Erro::Releitura)
    );
}

#[test]
fn spi_preserva_trio_autoriza_e_confere_releitura() {
    let r = RegistroFalso::default();
    let s = SpiFalso::default();
    let d = DisplayFalso::default();
    let a = ArmazemUsuario::novo(&r, &s, &d);
    let original = a.ler(MouseAceleracao, None).unwrap();
    let desligado = ValorUsuario::Spi([0; 3]);
    assert_eq!(original.valor, ValorUsuario::Spi([5, 12, 2]));
    assert_eq!(
        a.escrever(&original, &desligado, || Err(Erro::Bloqueado)),
        Err(Erro::Bloqueado)
    );
    assert_eq!(s.escritas.get(), 0);
    s.ignorar.set(true);
    assert_eq!(
        a.escrever(&original, &desligado, autorizar),
        Err(Erro::Releitura)
    );
    s.ignorar.set(false);
    a.escrever(&original, &desligado, autorizar).unwrap();
    let esperado = a.ler(MouseAceleracao, None).unwrap();
    assert_eq!(esperado.valor, desligado);
    a.restaurar(&esperado, &original, autorizar).unwrap();
    assert_eq!(a.ler(MouseAceleracao, None).unwrap(), original);
}

#[test]
fn display_preserva_racional_e_recusa_precondicoes_sem_gravar() {
    let r = RegistroFalso::default();
    let s = SpiFalso::default();
    let d = DisplayFalso::default();
    let a = ArmazemUsuario::novo(&r, &s, &d);
    let anterior = a.ler(TelaRefresh, Some("monitor-estavel")).unwrap();
    assert_eq!(anterior.valor, ValorUsuario::Refresh(ANTES));
    assert_eq!(
        a.ler(TelaRefresh, Some("desconectado")),
        Err(Erro::AlvoIndisponivel)
    );
    for erro in [
        Erro::AlvoIndisponivel,
        Erro::AlvoAmbiguo,
        Erro::Clone,
        Erro::ModoIndisponivel,
    ] {
        d.falha.replace(Some(erro.clone()));
        assert_eq!(
            a.escrever(&anterior, &ValorUsuario::Refresh(DEPOIS), autorizar),
            Err(erro)
        );
    }
    assert!(d.escritas.borrow().is_empty());
    d.falha.replace(None);
    a.escrever(&anterior, &ValorUsuario::Refresh(DEPOIS), autorizar)
        .unwrap();
    let atual = a.ler(TelaRefresh, Some("monitor-estavel")).unwrap();
    assert_eq!(atual.valor, ValorUsuario::Refresh(DEPOIS));
    a.restaurar(&atual, &anterior, autorizar).unwrap();
    assert_eq!(*d.escritas.borrow(), [DEPOIS, ANTES]);
}

#[test]
fn display_rele_e_compensa_com_vetores_novos_e_mesmo_portao() {
    for bloqueada in [false, true] {
        let r = RegistroFalso::default();
        let s = SpiFalso::default();
        let d = DisplayFalso::default();
        let a = ArmazemUsuario::novo(&r, &s, &d);
        let anterior = a.ler(TelaRefresh, Some("monitor-estavel")).unwrap();
        d.resultados.borrow_mut().push_back(DIVERGENTE);
        let mut chamadas = 0;
        let erro = a
            .escrever(&anterior, &ValorUsuario::Refresh(DEPOIS), || {
                chamadas += 1;
                if bloqueada && chamadas == 2 {
                    Err(Erro::Bloqueado)
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        let compensacao = if bloqueada {
            Err(Box::new(Erro::Bloqueado))
        } else {
            Ok(())
        };
        assert_eq!(
            erro,
            Erro::RefreshNaoConfirmado {
                falha: Box::new(Erro::Releitura),
                compensacao
            }
        );
        assert_eq!(chamadas, 2);
        assert_eq!(*d.preparadas.borrow(), [DEPOIS, ANTES]);
        assert_eq!(d.taxa.get(), if bloqueada { DIVERGENTE } else { ANTES });
        assert_eq!(d.escritas.borrow().len(), if bloqueada { 1 } else { 2 });
    }
}

#[derive(Default)]
struct FonteSomenteLeitura {
    chamadas_fso: RefCell<Vec<String>>,
}

impl FonteDiagnosticos for FonteSomenteLeitura {
    fn ler_sessao(&self, consulta: ConsultaSessao<'_>) -> Option<String> {
        if let ConsultaSessao::Fso(exe) = consulta {
            self.chamadas_fso.borrow_mut().push(exe.to_owned());
        }
        None
    }
    fn ler_maquina(&self, consulta: ConsultaMaquina) -> Option<String> {
        (consulta == ConsultaMaquina::WindowsBuild).then(|| "26100".into())
    }
    fn ler_device_guard(&self) -> DeviceGuard {
        DeviceGuard::default()
    }
}

#[test]
fn diagnosticos_usam_so_leitura_e_distinguem_indisponibilidade_e_orientacao() {
    let fonte = FonteSomenteLeitura::default();
    let sessao = diagnosticos_sessao(&fonte, &[r"C:\Jogos\conhecido.exe"]);
    let maquina = diagnosticos_maquina(&fonte);
    let tem = |itens: &[Diagnostico], codigo: &str, estado: EstadoDiagnostico| {
        assert!(
            itens
                .iter()
                .any(|d| d.codigo == codigo && d.estado == estado)
        );
    };
    assert_eq!(*fonte.chamadas_fso.borrow(), [r"C:\Jogos\conhecido.exe"]);
    tem(&sessao, "drr", EstadoDiagnostico::Indisponivel);
    tem(&sessao, "polling_mouse", EstadoDiagnostico::Orientacao);
    tem(
        &maquina,
        "windows_11",
        EstadoDiagnostico::Conhecido {
            valor: "26100".into(),
        },
    );
    tem(&maquina, "iommu", EstadoDiagnostico::Indisponivel);
    tem(&maquina, "useplatformclock", EstadoDiagnostico::Orientacao);
}

#[test]
#[ignore = "consulta a sessao e a maquina reais"]
fn leituras_reais() {
    let fonte = DiagnosticosWindows;
    assert!(fonte.windows_build().is_some());
    assert!(!diagnosticos_sessao(&fonte, &[]).is_empty());
    assert!(!diagnosticos_maquina(&fonte).is_empty());
    let armazem = ArmazemUsuario::default();
    let _ = armazem.ler(MouseAceleracao, None);
    let _ = armazem.ler(DxSwapEffectUpgrade, None);
    let _ = armazem.ler(GamedvrCapturaHistorica, None);
}
