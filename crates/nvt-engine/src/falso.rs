//! [`DriverFalso`]: DRS em memória para testes do motor.
//!
//! Imita a semântica relevante do driver: settings próprias do perfil, herança
//! do perfil base para perfis de aplicativo, padrões do driver, valores
//! predefinidos pela NVIDIA e `find_application` sem diferenciar caixa. O
//! handle é compartilhado (`Clone`), para o teste manipular o "driver" enquanto
//! o motor o usa (simular o usuário, um reset ou uma atualização).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use nvt_drs::{
    ArchInfo, DriverVersion, Error, FonteFalsa, GpuInfo, InfoExibicao, ProfileRef, ProfileSnapshot,
    Result, SNAPSHOT_FORMAT, Setting, SettingLocation, SettingSnapshot, SettingValue, Snapshot,
    Status,
};

use crate::store::{DriverStore, PerfilEncontrado};

#[derive(Debug, Clone)]
struct ValorFalso {
    valor: SettingValue,
    predefinido: Option<SettingValue>,
}

impl ValorFalso {
    fn do_usuario(&self) -> bool {
        self.predefinido.as_ref() != Some(&self.valor)
    }
}

#[derive(Debug, Clone, Default)]
struct PerfilFalso {
    nome: String,
    predefinido: bool,
    apps: Vec<String>,
    settings: BTreeMap<u32, ValorFalso>,
}

#[derive(Debug)]
struct Interno {
    versao: DriverVersion,
    gpus: Vec<GpuInfo>,
    base: PerfilFalso,
    perfis: Vec<PerfilFalso>,
    padroes: BTreeMap<u32, u32>,
    gravacoes: usize,
    salvamentos: usize,
    exibicao: FonteFalsa,
}

impl Interno {
    fn perfil(&self, r: &ProfileRef) -> Result<&PerfilFalso> {
        match r {
            ProfileRef::Base => Ok(&self.base),
            ProfileRef::Named(n) => self
                .perfis
                .iter()
                .find(|p| p.nome.eq_ignore_ascii_case(n))
                .ok_or_else(|| nao_encontrado(n)),
        }
    }

    fn perfil_mut(&mut self, r: &ProfileRef) -> Result<&mut PerfilFalso> {
        match r {
            ProfileRef::Base => Ok(&mut self.base),
            ProfileRef::Named(n) => self
                .perfis
                .iter_mut()
                .find(|p| p.nome.eq_ignore_ascii_case(n))
                .ok_or_else(|| nao_encontrado(n)),
        }
    }

    fn perfil_do_exe(&self, exe: &str) -> Option<&PerfilFalso> {
        self.perfis
            .iter()
            .find(|p| p.apps.iter().any(|a| a.eq_ignore_ascii_case(exe)))
    }

    fn ler(&self, r: &ProfileRef, id: u32) -> Result<Option<Setting>> {
        let p = self.perfil(r)?;
        let setting = |v: &ValorFalso, location| Setting {
            id,
            name: String::new(),
            location,
            is_current_predefined: !v.do_usuario(),
            predefined: v.predefinido.clone(),
            value: v.valor.clone(),
        };
        if let Some(v) = p.settings.get(&id) {
            return Ok(Some(setting(v, SettingLocation::Current)));
        }
        if !matches!(r, ProfileRef::Base)
            && let Some(v) = self.base.settings.get(&id)
        {
            return Ok(Some(setting(v, SettingLocation::Global)));
        }
        Ok(self.padroes.get(&id).map(|&v| Setting {
            id,
            name: String::new(),
            location: SettingLocation::Default,
            is_current_predefined: true,
            predefined: None,
            value: SettingValue::Dword(v),
        }))
    }
}

fn nao_encontrado(n: &str) -> Error {
    Error::Nvapi {
        function: "NvAPI_DRS_FindProfileByName",
        status: Status::ProfileNotFound,
        message: format!("perfil “{n}” não encontrado (falso)"),
    }
}

fn snapshot_perfil(p: &PerfilFalso, r: ProfileRef, apps: Vec<String>) -> ProfileSnapshot {
    ProfileSnapshot {
        profile: r,
        name: p.nome.clone(),
        is_predefined: p.predefinido,
        applications: apps,
        settings: p
            .settings
            .iter()
            .filter(|(_, v)| v.do_usuario())
            .map(|(&id, v)| SettingSnapshot {
                id,
                name: String::new(),
                value: v.valor.clone(),
                predefined_valid: v.predefinido.is_some(),
            })
            .collect(),
    }
}

/// DRS em memória. Clonar compartilha o mesmo estado.
#[derive(Debug, Clone)]
pub struct DriverFalso(Rc<RefCell<Interno>>);

impl DriverFalso {
    /// Driver vazio com a versão e as GPUs dadas.
    pub fn novo(versao_raw: u32, gpus: Vec<GpuInfo>) -> Self {
        Self(Rc::new(RefCell::new(Interno {
            versao: DriverVersion {
                raw: versao_raw,
                branch: "r620_00".into(),
            },
            gpus,
            base: PerfilFalso {
                nome: "Base Profile".into(),
                predefinido: true,
                ..PerfilFalso::default()
            },
            perfis: Vec::new(),
            padroes: BTreeMap::new(),
            gravacoes: 0,
            salvamentos: 0,
            exibicao: FonteFalsa::sem_nvapi(),
        })))
    }

    /// RTX 2060 SUPER (Turing TU106, 8 GiB) com driver 621.40.
    pub fn turing() -> Self {
        Self::novo(
            62140,
            vec![GpuInfo {
                index: 0,
                name: "NVIDIA GeForce RTX 2060 SUPER".into(),
                arch: Some(ArchInfo {
                    architecture: ArchInfo::TURING,
                    implementation: 0x06,
                    revision: 0xA1,
                }),
                vram_kib: Some(8 * 1024 * 1024),
            }],
        )
    }

    /// Monitor principal visto pela NVAPI (refresh e VRR). Padrão: NVAPI sem as
    /// funções de exibição (refresh e VRR desconhecidos).
    pub fn definir_exibicao(&self, fonte: FonteFalsa) {
        self.0.borrow_mut().exibicao = fonte;
    }

    /// Valor padrão do driver para a setting (o que se lê quando ninguém a gravou).
    pub fn definir_padrao(&self, id: u32, valor: u32) {
        self.0.borrow_mut().padroes.insert(id, valor);
    }

    /// Adiciona um perfil predefinido pela NVIDIA com executáveis e settings predefinidas.
    pub fn adicionar_perfil_predefinido(&self, nome: &str, exes: &[&str], settings: &[(u32, u32)]) {
        self.0.borrow_mut().perfis.push(PerfilFalso {
            nome: nome.into(),
            predefinido: true,
            apps: exes.iter().map(|s| (*s).to_owned()).collect(),
            settings: settings
                .iter()
                .map(|&(id, v)| {
                    (
                        id,
                        ValorFalso {
                            valor: SettingValue::Dword(v),
                            predefinido: Some(SettingValue::Dword(v)),
                        },
                    )
                })
                .collect(),
        });
    }

    /// Simula uma mudança manual (painel da NVIDIA, NPI, outro app).
    pub fn mudar_pelo_usuario(&self, perfil: &ProfileRef, id: u32, valor: u32) {
        let mut i = self.0.borrow_mut();
        let p = i.perfil_mut(perfil).expect("perfil inexistente no falso");
        let predefinido = p.settings.get(&id).and_then(|v| v.predefinido.clone());
        p.settings.insert(
            id,
            ValorFalso {
                valor: SettingValue::Dword(valor),
                predefinido,
            },
        );
    }

    /// Simula o reset de uma atualização de driver: todas as settings gravadas pelo
    /// usuário somem (as predefinidas voltam). Perfis continuam existindo.
    pub fn resetar_settings(&self) {
        let mut i = self.0.borrow_mut();
        let limpar = |p: &mut PerfilFalso| {
            p.settings.retain(|_, v| v.predefinido.is_some());
            for v in p.settings.values_mut() {
                v.valor = v.predefinido.clone().expect("retido só com predefinido");
            }
        };
        limpar(&mut i.base);
        i.perfis.iter_mut().for_each(limpar);
    }

    /// Simula um perfil apagado (instalação limpa do driver, ou apagado no NPI/painel).
    pub fn apagar_perfil(&self, nome: &str) {
        self.0
            .borrow_mut()
            .perfis
            .retain(|p| !p.nome.eq_ignore_ascii_case(nome));
    }

    /// Settings gravadas no próprio perfil (id → valor DWORD), sem herança.
    pub fn proprias(&self, perfil: &ProfileRef) -> BTreeMap<u32, u32> {
        self.0
            .borrow()
            .perfil(perfil)
            .map(|p| {
                p.settings
                    .iter()
                    .filter_map(|(&id, v)| v.valor.as_u32().map(|x| (id, x)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Troca a versão do driver.
    pub fn mudar_versao(&self, raw: u32) {
        self.0.borrow_mut().versao.raw = raw;
    }

    /// Valor DWORD efetivo da setting no perfil.
    pub fn valor(&self, perfil: &ProfileRef, id: u32) -> Option<u32> {
        self.0
            .borrow()
            .ler(perfil, id)
            .ok()
            .flatten()
            .and_then(|s| s.value.as_u32())
    }

    /// Nomes dos perfis (sem o base).
    pub fn perfis(&self) -> Vec<String> {
        self.0
            .borrow()
            .perfis
            .iter()
            .map(|p| p.nome.clone())
            .collect()
    }

    /// Executáveis de um perfil.
    pub fn apps(&self, perfil: &str) -> Vec<String> {
        self.0
            .borrow()
            .perfil(&ProfileRef::Named(perfil.into()))
            .map(|p| p.apps.clone())
            .unwrap_or_default()
    }

    /// Quantas gravações/remoções de setting foram feitas.
    pub fn gravacoes(&self) -> usize {
        self.0.borrow().gravacoes
    }

    /// Quantas vezes `salvar` foi chamado.
    pub fn salvamentos(&self) -> usize {
        self.0.borrow().salvamentos
    }
}

impl DriverStore for DriverFalso {
    fn versao_driver(&self) -> Result<DriverVersion> {
        Ok(self.0.borrow().versao.clone())
    }

    fn gpus(&self) -> Result<Vec<GpuInfo>> {
        Ok(self.0.borrow().gpus.clone())
    }

    fn perfil_do_exe(&self, exe: &str) -> Result<Option<PerfilEncontrado>> {
        Ok(self
            .0
            .borrow()
            .perfil_do_exe(exe)
            .map(|p| PerfilEncontrado {
                nome: p.nome.clone(),
                predefinido: p.predefinido,
            }))
    }

    fn perfil_existe(&self, nome: &str) -> Result<bool> {
        Ok(self
            .0
            .borrow()
            .perfis
            .iter()
            .any(|p| p.nome.eq_ignore_ascii_case(nome)))
    }

    fn nome_canonico(&self, nome: &str) -> Result<Option<String>> {
        Ok(self
            .0
            .borrow()
            .perfis
            .iter()
            .find(|p| p.nome.eq_ignore_ascii_case(nome))
            .map(|p| p.nome.clone()))
    }

    fn criar_perfil(&mut self, nome: &str) -> Result<()> {
        if self.perfil_existe(nome)? {
            return Err(Error::Nvapi {
                function: "NvAPI_DRS_CreateProfile",
                status: Status::from_code(-175),
                message: format!("perfil “{nome}” já existe (falso)"),
            });
        }
        self.0.borrow_mut().perfis.push(PerfilFalso {
            nome: nome.into(),
            ..PerfilFalso::default()
        });
        Ok(())
    }

    fn criar_aplicacao(&mut self, perfil: &str, exe: &str) -> Result<()> {
        let mut i = self.0.borrow_mut();
        if i.perfil_do_exe(exe).is_some() {
            return Err(Error::Nvapi {
                function: "NvAPI_DRS_CreateApplication",
                status: Status::from_code(-180),
                message: format!("“{exe}” já pertence a um perfil (falso)"),
            });
        }
        i.perfil_mut(&ProfileRef::Named(perfil.into()))?
            .apps
            .push(exe.into());
        Ok(())
    }

    fn ler_setting(&self, perfil: &ProfileRef, id: u32) -> Result<Option<Setting>> {
        self.0.borrow().ler(perfil, id)
    }

    fn gravar_setting(&mut self, perfil: &ProfileRef, id: u32, valor: &SettingValue) -> Result<()> {
        let mut i = self.0.borrow_mut();
        i.gravacoes += 1;
        let p = i.perfil_mut(perfil)?;
        let predefinido = p.settings.get(&id).and_then(|v| v.predefinido.clone());
        p.settings.insert(
            id,
            ValorFalso {
                valor: valor.clone(),
                predefinido,
            },
        );
        Ok(())
    }

    fn remover_setting(
        &mut self,
        perfil: &ProfileRef,
        id: u32,
        predefinido_valido: bool,
    ) -> Result<()> {
        let mut i = self.0.borrow_mut();
        i.gravacoes += 1;
        let p = i.perfil_mut(perfil)?;
        match p.settings.get_mut(&id) {
            Some(v) if predefinido_valido && v.predefinido.is_some() => {
                v.valor = v.predefinido.clone().expect("conferido acima");
            }
            _ => {
                p.settings.remove(&id);
            }
        }
        Ok(())
    }

    fn settings_do_perfil(&self, perfil: &ProfileRef) -> Result<Vec<Setting>> {
        let i = self.0.borrow();
        let ids: Vec<u32> = i.perfil(perfil)?.settings.keys().copied().collect();
        Ok(ids
            .into_iter()
            .filter_map(|id| i.ler(perfil, id).ok().flatten())
            .collect())
    }

    fn capturar(&self, exes: &[&str]) -> Result<Snapshot> {
        let i = self.0.borrow();
        let mut profiles = vec![snapshot_perfil(&i.base, ProfileRef::Base, Vec::new())];
        let mut missing = Vec::new();
        for &exe in exes {
            match i.perfil_do_exe(exe) {
                None => missing.push(exe.to_owned()),
                Some(p) => {
                    let r = ProfileRef::Named(p.nome.clone());
                    if let Some(ps) = profiles.iter_mut().find(|ps| ps.profile == r) {
                        ps.applications.push(exe.to_owned());
                    } else {
                        profiles.push(snapshot_perfil(p, r, vec![exe.to_owned()]));
                    }
                }
            }
        }
        Ok(Snapshot {
            format: SNAPSHOT_FORMAT,
            driver_version: i.versao.raw,
            driver_branch: i.versao.branch.clone(),
            captured_at: "1970-01-01T00:00:00Z".into(),
            captured_at_unix: 0,
            profiles,
            missing_applications: missing,
        })
    }

    fn exibicao(&self) -> InfoExibicao {
        nvt_drs::ler_exibicao_principal(&self.0.borrow().exibicao)
    }

    fn salvar(&mut self) -> Result<()> {
        self.0.borrow_mut().salvamentos += 1;
        Ok(())
    }
}
