//! Abstração do DRS ([`DriverStore`]) e implementação real sobre o `nvt-drs`.
//!
//! O motor só fala com o driver por este trait, o que permite testar o ciclo
//! inteiro com [`crate::DriverFalso`]. A lista permitida **não** fica aqui: ela
//! é aplicada pelo motor antes de qualquer chamada de gravação.

use nvt_drs::{
    DriverVersion, Error, GpuInfo, InfoExibicao, NvApi, Profile, ProfileRef, Result, Session,
    Setting, SettingValue, Snapshot, Status,
};

/// Perfil que contém um executável.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerfilEncontrado {
    pub nome: String,
    /// Perfil predefinido pela NVIDIA (vem com o driver).
    pub predefinido: bool,
}

/// Operações do DRS usadas pelo motor.
///
/// Alterações (`criar_*`, `gravar_setting`, `remover_setting`) valem só na
/// sessão até [`DriverStore::salvar`].
pub trait DriverStore {
    /// Versão do driver.
    fn versao_driver(&self) -> Result<DriverVersion>;
    /// GPUs físicas NVIDIA.
    fn gpus(&self) -> Result<Vec<GpuInfo>>;
    /// Perfil que contém o executável (`find_application`); `None` se nenhum.
    fn perfil_do_exe(&self, exe: &str) -> Result<Option<PerfilEncontrado>>;
    /// Existe perfil com este nome?
    fn perfil_existe(&self, nome: &str) -> Result<bool>;
    /// Nome do perfil como o DRS o guarda (a busca por nome não diferencia
    /// maiúsculas); `None` se não existir. O padrão devolve `nome` quando
    /// [`DriverStore::perfil_existe`].
    fn nome_canonico(&self, nome: &str) -> Result<Option<String>> {
        Ok(self.perfil_existe(nome)?.then(|| nome.to_owned()))
    }
    /// Cria um perfil de usuário vazio.
    fn criar_perfil(&mut self, nome: &str) -> Result<()>;
    /// Associa o executável ao perfil.
    fn criar_aplicacao(&mut self, perfil: &str, exe: &str) -> Result<()>;
    /// Valor efetivo da setting no perfil (próprio, herdado ou padrão do driver).
    fn ler_setting(&self, perfil: &ProfileRef, id: u32) -> Result<Option<Setting>>;
    /// Grava o valor no perfil.
    fn gravar_setting(&mut self, perfil: &ProfileRef, id: u32, valor: &SettingValue) -> Result<()>;
    /// Volta ao predefinido da NVIDIA (`predefinido_valido`) ou apaga a setting do perfil.
    fn remover_setting(
        &mut self,
        perfil: &ProfileRef,
        id: u32,
        predefinido_valido: bool,
    ) -> Result<()>;
    /// Settings gravadas no próprio perfil.
    fn settings_do_perfil(&self, perfil: &ProfileRef) -> Result<Vec<Setting>>;
    /// Snapshot do perfil base e dos perfis dos executáveis (somente leitura).
    fn capturar(&self, exes: &[&str]) -> Result<Snapshot>;
    /// Persiste a sessão (exige administrador no driver real).
    fn salvar(&mut self) -> Result<()>;
    /// Refresh e VRR do monitor principal (`nvt_drs::ler_exibicao_principal`).
    /// Nunca falha: desconhecido vira `None`. O padrão não lê nada (tudo
    /// desconhecido), para fontes sem NVAPI.
    fn exibicao(&self) -> InfoExibicao {
        InfoExibicao::default()
    }
}

/// [`DriverStore`] sobre uma sessão DRS real.
///
/// Leituras funcionam sem administrador; [`DriverStore::salvar`] exige.
pub struct DrsReal<'a> {
    sessao: Session<'a>,
}

impl<'a> DrsReal<'a> {
    /// Abre a sessão DRS (carrega as settings do driver).
    pub fn abrir(api: &'a NvApi) -> Result<Self> {
        Ok(Self {
            sessao: Session::open(api)?,
        })
    }

    fn perfil(&self, r: &ProfileRef) -> Result<Profile<'_>> {
        match r {
            ProfileRef::Base => self.sessao.base_profile(),
            ProfileRef::Named(n) => self.sessao.find_profile_by_name(n)?.ok_or(Error::Nvapi {
                function: "NvAPI_DRS_FindProfileByName",
                status: Status::ProfileNotFound,
                message: format!("perfil “{n}” não encontrado"),
            }),
        }
    }
}

impl DriverStore for DrsReal<'_> {
    fn versao_driver(&self) -> Result<DriverVersion> {
        self.sessao.api().driver_version()
    }

    fn gpus(&self) -> Result<Vec<GpuInfo>> {
        self.sessao.api().physical_gpus()
    }

    fn perfil_do_exe(&self, exe: &str) -> Result<Option<PerfilEncontrado>> {
        match self.sessao.find_application(exe)? {
            None => Ok(None),
            Some((p, _)) => {
                let info = self.sessao.profile_info(p)?;
                Ok(Some(PerfilEncontrado {
                    nome: info.name,
                    predefinido: info.is_predefined,
                }))
            }
        }
    }

    fn perfil_existe(&self, nome: &str) -> Result<bool> {
        Ok(self.sessao.find_profile_by_name(nome)?.is_some())
    }

    fn nome_canonico(&self, nome: &str) -> Result<Option<String>> {
        match self.sessao.find_profile_by_name(nome)? {
            None => Ok(None),
            Some(p) => Ok(Some(self.sessao.profile_info(p)?.name)),
        }
    }

    fn criar_perfil(&mut self, nome: &str) -> Result<()> {
        self.sessao.create_profile(nome).map(|_| ())
    }

    fn criar_aplicacao(&mut self, perfil: &str, exe: &str) -> Result<()> {
        let p = self.perfil(&ProfileRef::Named(perfil.to_owned()))?;
        self.sessao.create_application(p, exe, None)
    }

    fn ler_setting(&self, perfil: &ProfileRef, id: u32) -> Result<Option<Setting>> {
        self.sessao.get_setting(self.perfil(perfil)?, id)
    }

    fn gravar_setting(&mut self, perfil: &ProfileRef, id: u32, valor: &SettingValue) -> Result<()> {
        self.sessao.set_setting(self.perfil(perfil)?, id, valor)
    }

    fn remover_setting(
        &mut self,
        perfil: &ProfileRef,
        id: u32,
        predefinido_valido: bool,
    ) -> Result<()> {
        let p = self.perfil(perfil)?;
        if predefinido_valido {
            self.sessao.restore_setting_default(p, id)
        } else {
            self.sessao.delete_setting(p, id)
        }
    }

    fn settings_do_perfil(&self, perfil: &ProfileRef) -> Result<Vec<Setting>> {
        self.sessao.enum_settings(self.perfil(perfil)?)
    }

    fn capturar(&self, exes: &[&str]) -> Result<Snapshot> {
        nvt_drs::capture(&self.sessao, exes)
    }

    fn salvar(&mut self) -> Result<()> {
        self.sessao.save()
    }

    fn exibicao(&self) -> InfoExibicao {
        nvt_drs::ler_exibicao_principal(self.sessao.api())
    }
}
