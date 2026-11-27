//! Abertura do Coordenador, preferências, limites, restauração, processos e
//! atualização vista pelas sessões.

use super::*;

impl Coordenador {
    pub(crate) fn abrir(
        armazenamento: impl Armazenamento + 'static,
        drs: &ConjuntoRegras,
        windows_build: Option<u32>,
        geracao: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let dados = armazenamento.carregar()?;
        let regras = ConjuntoRegrasUsuario::embutido(drs)?;
        let metadados = regras
            .metadados()
            .map(|m| {
                let meta = MetaRegra {
                    id: m.id.to_owned(),
                    descricao: m.descricao.to_owned(),
                    status: m.status,
                    impacto_visual: m.impacto_visual,
                };
                (meta.id.clone(), meta)
            })
            .collect();
        let catalogo_jogo = BaseJogos::embutida()?.config_jogo().clone();
        let mut coordenador = Self {
            armazenamento: Box::new(armazenamento),
            dados,
            sessoes: HashMap::new(),
            regras,
            metadados,
            drs: drs
                .regras
                .iter()
                .filter(|r| r.status == Status::Experimental)
                .map(|r| r.id.clone())
                .collect(),
            catalogo_jogo,
            prefs: Preferences::default(),
            geracao: 0,
            pausado: false,
            serial: SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as u64,
            windows_build,
            processos: BTreeSet::new(),
            atualizacao: nvt_ipc::EstadoAtualizacao::default(),
        };
        coordenador.atualizar_restauracao(geracao, false);
        Ok(coordenador)
    }

    pub(crate) fn aceites_drs(&self, sid_console: Option<&str>) -> BTreeSet<String> {
        sid_console
            .and_then(|sid| self.dados.por_sid.get(sid))
            .map(|dados| dados.aceites.intersection(&self.drs).cloned().collect())
            .unwrap_or_default()
    }

    /// Preferências vigentes (o lote 3 chama a cada mudança). Só `limite_fps`
    /// afeta a família jogo; mudança invalida as tarefas em voo.
    pub(crate) fn definir_preferencias(&mut self, prefs: &Preferences) {
        if self.prefs.limite_fps != prefs.limite_fps {
            for sessao in self.sessoes.values_mut() {
                sessao.invalidar();
            }
        }
        self.prefs = prefs.clone();
    }

    /// F3-13: jogos cujo limite de FPS no jogo está efetivamente aplicado no
    /// SID da sessão de console (regra aceita no plano e arquivo no valor ou
    /// escrita confirmada pelo agente), sem falha nem adiamento relatado em
    /// nenhuma sessão desse SID. Id do jogo → regra, pronto para
    /// [`nvt_core::config_jogo::desligar_limite_drs`]. Sem SID de console,
    /// vazio: o driver mantém o limite.
    pub(crate) fn limites_no_jogo_efetivos(
        &self,
        sid_console: Option<&str>,
    ) -> BTreeMap<IdJogo, String> {
        let Some(sid) = sid_console else {
            return BTreeMap::new();
        };
        let mut efetivos = BTreeMap::new();
        let mut bloqueados = BTreeSet::new();
        for (_, s) in self.sessoes.iter().filter(|(id, _)| id.sid == sid) {
            let plano = self.plano_jogo(sid, s);
            for m in plano.mudancas.values().filter(|m| m.limite_no_jogo) {
                let jogo = m.destino.instancia.jogo.clone();
                let aplicado = m.atual == m.valor
                    || s.gestao_jogo
                        .get(&m.destino)
                        .is_some_and(|g| g.esperado == m.valor);
                if aplicado && !s.falhas_jogo.contains_key(&m.destino) {
                    efetivos.insert(jogo, m.regra.clone());
                } else {
                    bloqueados.insert(jogo);
                }
            }
        }
        efetivos.retain(|jogo, _| !bloqueados.contains(jogo));
        efetivos
    }

    /// A integração fornece a geração efetiva. Repetir a mesma intenção não a avança.
    pub(crate) fn atualizar_restauracao(&mut self, geracao: u64, pausado: bool) {
        if geracao < self.geracao || (geracao, pausado) == (self.geracao, self.pausado) {
            return;
        }
        self.geracao = geracao;
        self.pausado = pausado;
        for sessao in self.sessoes.values_mut() {
            sessao.invalidar();
        }
    }

    pub(crate) fn geracao(&self) -> u64 {
        self.geracao
    }

    pub(crate) fn atualizar_processos(&mut self, processos: BTreeSet<String>) {
        if self.processos != processos {
            self.processos = processos;
            for sessao in self.sessoes.values_mut() {
                sessao.invalidar();
            }
        }
    }

    /// Estado atual da atualização automática, para validar
    /// `MarcarAtualizacaoVista` (U-52).
    pub(crate) fn definir_atualizacao(&mut self, estado: nvt_ipc::EstadoAtualizacao) {
        self.atualizacao = estado;
    }

    /// Sessões com agente conectado agora (U-28, U-43), sem repetição.
    pub(crate) fn sessoes_agente(&self, agora: Instant) -> Vec<nvt_atualizacao::SessaoAgente> {
        let mut v: Vec<_> = self
            .sessoes
            .iter()
            .filter(|(_, s)| s.presente(agora))
            .map(|(id, _)| nvt_atualizacao::SessaoAgente {
                sessao: id.sessao,
                sid: id.sid.clone(),
            })
            .collect();
        v.sort_by(|a, b| (a.sessao, &a.sid).cmp(&(b.sessao, &b.sid)));
        v.dedup();
        v
    }

    /// A última atualização concluída ainda não foi vista por `sid` (U-53).
    pub(crate) fn nao_vista(&self, sid: &str, estado: &nvt_ipc::EstadoAtualizacao) -> bool {
        estado.ultima_atualizacao.as_ref().is_some_and(|u| {
            self.dados
                .por_sid
                .get(sid)
                .and_then(|d| d.atualizacao_vista.as_deref())
                != Some(u.para.as_str())
        })
    }

    pub(super) fn marcar_atualizacao_vista(
        &mut self,
        identidade: &IdentidadeCliente,
        versao: &str,
    ) -> Result<Resposta, Erro> {
        let ultima = self.atualizacao.ultima_atualizacao.as_ref();
        if ultima.map(|u| u.para.as_str()) != Some(versao) {
            return Err((
                CodigoErro::EntradaInvalida,
                "versão diferente da última atualização concluída".into(),
            ));
        }
        let vista = self
            .dados
            .por_sid
            .get(&identidade.sid)
            .and_then(|d| d.atualizacao_vista.as_deref());
        if vista == Some(versao) {
            return Ok(Resposta::Ok);
        }
        transacao(&mut self.dados, &*self.armazenamento, |dados| {
            dados
                .por_sid
                .entry(identidade.sid.clone())
                .or_default()
                .atualizacao_vista = Some(versao.to_owned());
        })?;
        Ok(Resposta::Ok)
    }
}
