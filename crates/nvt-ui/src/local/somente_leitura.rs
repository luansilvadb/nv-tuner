//! `DriverStore` que recusa toda escrita no DRS.

use super::*;

// ---------------------------------------------------------------------------
// DriverStore que recusa escrita
// ---------------------------------------------------------------------------

/// Envolve um [`DriverStore`] e recusa toda operação de escrita.
pub struct SomenteLeitura<S>(pub S);

pub(super) fn recusa() -> ErroDrs {
    ErroDrs::Nvapi {
        function: "nv-tuner (modo de visualização)",
        status: StatusNvapi::InvalidUserPrivilege,
        message: textos::RECUSA_VISUALIZACAO.into(),
    }
}

impl<S: DriverStore> DriverStore for SomenteLeitura<S> {
    fn versao_driver(&self) -> ResultadoDrs<DriverVersion> {
        self.0.versao_driver()
    }
    fn gpus(&self) -> ResultadoDrs<Vec<GpuInfo>> {
        let gpus = self.0.gpus()?;
        #[cfg(debug_assertions)]
        if let Some(g) = gpu_simulada() {
            return Ok(vec![g]);
        }
        Ok(gpus)
    }
    fn perfil_do_exe(&self, exe: &str) -> ResultadoDrs<Option<PerfilEncontrado>> {
        self.0.perfil_do_exe(exe)
    }
    fn perfil_existe(&self, nome: &str) -> ResultadoDrs<bool> {
        self.0.perfil_existe(nome)
    }
    fn criar_perfil(&mut self, _: &str) -> ResultadoDrs<()> {
        Err(recusa())
    }
    fn criar_aplicacao(&mut self, _: &str, _: &str) -> ResultadoDrs<()> {
        Err(recusa())
    }
    fn ler_setting(&self, perfil: &ProfileRef, id: u32) -> ResultadoDrs<Option<Setting>> {
        self.0.ler_setting(perfil, id)
    }
    fn gravar_setting(&mut self, _: &ProfileRef, _: u32, _: &SettingValue) -> ResultadoDrs<()> {
        Err(recusa())
    }
    fn remover_setting(&mut self, _: &ProfileRef, _: u32, _: bool) -> ResultadoDrs<()> {
        Err(recusa())
    }
    fn settings_do_perfil(&self, perfil: &ProfileRef) -> ResultadoDrs<Vec<Setting>> {
        self.0.settings_do_perfil(perfil)
    }
    fn capturar(&self, exes: &[&str]) -> ResultadoDrs<Snapshot> {
        self.0.capturar(exes)
    }
    fn salvar(&mut self) -> ResultadoDrs<()> {
        Err(recusa())
    }
}
