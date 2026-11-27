//! Construção do [`Engine`], acessores e gravação pela lista permitida.

use super::*;

impl<S: DriverStore> Engine<S> {
    /// Motor com as regras e a base de jogos embutidas.
    pub fn new(store: S, armazem: Armazem) -> Resultado<Self> {
        let base = BaseJogos::embutida().map_err(|e| {
            ErroEngine::Regras(ErrosRegras(vec![ErroRegra::Toml {
                arquivo: "jogos.toml".into(),
                mensagem: e.to_string(),
            }]))
        })?;
        Ok(Self::montar(
            store,
            armazem,
            ConjuntoRegras::embutido()?,
            base,
        ))
    }

    /// Motor com as regras dadas e a base de jogos embutida (vazia se a base
    /// embutida for inválida, o que o teste de carga impede).
    pub fn com_regras(store: S, armazem: Armazem, regras: ConjuntoRegras) -> Self {
        let base = BaseJogos::embutida().unwrap_or_else(|_| BaseJogos::vazia());
        Self::montar(store, armazem, regras, base)
    }

    fn montar(store: S, armazem: Armazem, regras: ConjuntoRegras, base: BaseJogos) -> Self {
        Self {
            store: Guardado::new(store),
            regras,
            base,
            armazem,
            portao: Portao::do_sistema(),
        }
    }

    /// Troca o portão de gravação (fonte de processos injetável; o serviço
    /// passa o mesmo portão a cada motor recriado).
    pub fn com_portao(mut self, portao: Portao) -> Self {
        self.portao = portao;
        self
    }

    /// Portão de gravação.
    pub fn portao(&self) -> &Portao {
        &self.portao
    }

    /// Portão único de gravação (T-02): consulta a lista de processos agora
    /// e diz se o DRS pode ser gravado. Toda gravação do motor passa por
    /// ele (ver [`crate::portao`]).
    pub fn pode_gravar(&self) -> Result<(), Adiamento> {
        self.portao.verificar(&self.base, &[]).map(|_| ())
    }

    /// Licença para uma operação de gravação: estado gravável e portão livre
    /// (com os exes `extras` também vigiados).
    pub(super) fn licenca(&self, extras: &[&str]) -> Resultado<Licenca> {
        self.exigir_gravacao()?;
        self.portao
            .verificar(&self.base, extras)
            .map_err(ErroEngine::Adiada)
    }

    /// Troca a base curada de jogos.
    pub fn com_base(mut self, base: BaseJogos) -> Self {
        self.base = base;
        self
    }

    /// Base curada de jogos competitivos.
    pub fn base_jogos(&self) -> &BaseJogos {
        &self.base
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn estado(&self) -> &Estado {
        &self.armazem.estado
    }

    pub fn armazem(&self) -> &Armazem {
        &self.armazem
    }

    /// Avisos estruturados do estado (os da abertura do [`Armazem`]).
    pub fn avisos(&self) -> &[crate::Aviso] {
        self.armazem.avisos()
    }

    /// Recusa gravar (driver ou estado) com o armazém somente leitura, antes
    /// de qualquer chamada ao driver.
    pub(super) fn exigir_gravacao(&self) -> Resultado<()> {
        if self.armazem.somente_leitura() {
            return Err(ErroEngine::SomenteLeitura);
        }
        Ok(())
    }

    pub fn regras(&self) -> &ConjuntoRegras {
        &self.regras
    }

    /// Troca as preferências (as escolhas travadas são mantidas à parte) e persiste.
    pub fn definir_preferencias(&mut self, mut prefs: Preferences) -> Resultado<()> {
        self.exigir_gravacao()?;
        prefs.escolhas_usuario.clear();
        self.armazem.estado.preferencias = prefs;
        self.armazem.salvar()
    }

    /// Libera uma escolha travada (perfil comparado sem diferenciar
    /// maiúsculas); o próximo ciclo volta a gerir a setting.
    pub fn liberar_travada(&mut self, alvo: &Alvo, setting: &str) -> Resultado<bool> {
        self.exigir_gravacao()?;
        let antes = self.armazem.estado.travadas.len();
        self.armazem
            .estado
            .travadas
            .retain(|t| !(mesmo_alvo(&t.alvo, alvo) && t.setting == setting));
        let mudou = self.armazem.estado.travadas.len() != antes;
        if mudou {
            self.armazem.salvar()?;
        }
        Ok(mudou)
    }

    /// Capacidades da GPU principal.
    pub fn capacidades(&self) -> Resultado<Capabilities> {
        capacidades(&*self.store)
    }

    // ── Lista permitida ────────────────────────────────────────────────────

    /// Confere o ID (e, se dado, o valor) contra o catálogo.
    pub fn validar(&self, id: u32, valor: Option<u32>) -> Resultado<&DefSetting> {
        let def = self
            .regras
            .catalogo
            .por_id(id)
            .ok_or(ErroEngine::IdNaoPermitido { id })?;
        if let Some(v) = valor
            && !def.aceita(v)
        {
            return Err(ErroEngine::ValorNaoPermitido {
                setting: def.nome.clone(),
                valor: v,
            });
        }
        Ok(def)
    }

    /// Única porta de gravação de valores do motor: valida contra a lista permitida.
    /// Não persiste (quem chama decide quando `salvar`).
    /// Passa pelo portão ([`Engine::pode_gravar`]).
    pub fn gravar_permitido(&mut self, perfil: &ProfileRef, id: u32, valor: u32) -> Resultado<()> {
        let licenca = self.licenca(&[])?;
        self.gravar_com(&licenca, perfil, id, valor)
    }

    pub(super) fn gravar_com(
        &mut self,
        licenca: &Licenca,
        perfil: &ProfileRef,
        id: u32,
        valor: u32,
    ) -> Resultado<()> {
        self.exigir_gravacao()?;
        self.validar(id, Some(valor))?;
        self.store
            .escrita(licenca)
            .gravar_setting(perfil, id, &SettingValue::Dword(valor))?;
        Ok(())
    }

    pub(super) fn gravar_valor_original(
        &mut self,
        licenca: &Licenca,
        perfil: &ProfileRef,
        id: u32,
        valor: &SettingValue,
    ) -> Resultado<()> {
        self.validar(id, None)?;
        self.store
            .escrita(licenca)
            .gravar_setting(perfil, id, valor)?;
        Ok(())
    }

    pub(super) fn remover_permitido(
        &mut self,
        licenca: &Licenca,
        perfil: &ProfileRef,
        id: u32,
        predef: bool,
    ) -> Resultado<()> {
        self.validar(id, None)?;
        self.store
            .escrita(licenca)
            .remover_setting(perfil, id, predef)?;
        Ok(())
    }

    pub(super) fn nome_setting(&self, id: u32) -> String {
        self.regras
            .catalogo
            .por_id(id)
            .map_or_else(|| format!("0x{id:08X}"), |d| d.nome.clone())
    }

    pub(super) fn ler_u32(&self, perfil: &ProfileRef, id: u32) -> Resultado<Option<u32>> {
        Ok(self
            .store
            .ler_setting(perfil, id)?
            .and_then(|s| s.value.as_u32()))
    }
}
