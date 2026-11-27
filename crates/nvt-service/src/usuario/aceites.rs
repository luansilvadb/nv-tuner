//! Saúde por sessão, inícios vistos e aceites de regras experimentais.

use super::*;

impl Coordenador {
    /// Guarda o relato de saúde do agente na própria sessão (SID + sessão do
    /// pipe), filtrado para as fontes da sessão. `false` = sessão desconhecida.
    pub(super) fn receber_saude(
        &mut self,
        identidade: &IdentidadeCliente,
        saude: &nvt_ipc::SaudeSessao,
        agora: Instant,
    ) -> bool {
        let Some(s) = self.sessoes.get_mut(identidade) else {
            return false;
        };
        if saude.validar().is_err() {
            return false;
        }
        s.saude = Some(crate::saude::filtrar_relato(saude));
        s.contato = Some(agora);
        true
    }

    pub(super) fn vistos_inicio(&self, sid: &str) -> crate::saude::Vistos {
        self.dados
            .por_sid
            .get(sid)
            .map(|d| {
                d.inicio_vistos
                    .iter()
                    .map(|m| ((m.fonte, m.nome.clone()), m.marcado_unix))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn saude_da_sessao(
        &self,
        identidade: &IdentidadeCliente,
        agora: Instant,
    ) -> Option<&nvt_ipc::SaudeSessao> {
        self.sessoes
            .get(identidade)
            .filter(|s| s.presente(agora))
            .and_then(|s| s.saude.as_ref())
    }

    /// `ObterSaude`: máquina + a sessão chamadora, com as marcas do próprio SID.
    pub(crate) fn obter_saude(
        &self,
        identidade: &IdentidadeCliente,
        maquina: &crate::saude::SaudeMaquina,
        agora: Instant,
    ) -> Resposta {
        Resposta::Saude(crate::saude::projetar(
            maquina,
            self.saude_da_sessao(identidade, agora),
            &self.vistos_inicio(&identidade.sid),
        ))
    }

    /// `MarcarInicioVisto`: só programas visíveis à identidade (máquina ou o
    /// relato da própria sessão); grava em `usuarios.json` no SID chamador.
    pub(crate) fn marcar_inicio_visto(
        &mut self,
        identidade: &IdentidadeCliente,
        fonte: nvt_ipc::FonteInicio,
        nome: &str,
        maquina: &crate::saude::SaudeMaquina,
        agora: Instant,
        unix: u64,
    ) -> Resposta {
        let r = (|| {
            nvt_ipc::validar_nome_inicio(nome).map_err(invalida)?;
            let programa = maquina
                .inicializacao
                .iter()
                .chain(
                    self.saude_da_sessao(identidade, agora)
                        .into_iter()
                        .flat_map(|s| &s.inicializacao),
                )
                .find(|p| p.fonte == fonte && p.nome == nome)
                .ok_or_else(|| invalida("programa de inicialização desconhecido"))?;
            let marca = MarcaInicio {
                fonte,
                nome: nome.to_owned(),
                marcado_unix: unix.max(programa.detectado_unix),
            };
            transacao(&mut self.dados, &*self.armazenamento, |dados| {
                let sid = dados.por_sid.entry(identidade.sid.clone()).or_default();
                sid.inicio_vistos
                    .retain(|m| !(m.fonte == fonte && m.nome == nome));
                sid.inicio_vistos.push(marca);
                if sid.inicio_vistos.len() > crate::saude::VISTOS_MAX {
                    let excesso = sid.inicio_vistos.len() - crate::saude::VISTOS_MAX;
                    sid.inicio_vistos.drain(..excesso);
                }
            })?;
            Ok::<_, Erro>(Resposta::Ok)
        })();
        r.unwrap_or_else(|(codigo, mensagem)| Resposta::erro(codigo, mensagem))
    }

    /// R-AR: experimentais Windows e de config de jogo aplicáveis agora à
    /// identidade (plano da própria sessão, com agente presente) e ainda não
    /// aceitas pelo SID. O avaliador só marca `Aceitavel` regra cuja condição
    /// casou e com destino fora de trava ou de ajuste do usuário.
    pub(crate) fn experimentais_aplicaveis(
        &self,
        identidade: &IdentidadeCliente,
        agora: Instant,
    ) -> BTreeSet<String> {
        let sessao = self.sessoes.get(identidade).filter(|s| s.presente(agora));
        self.resumo(identidade, sessao)
            .sugestoes
            .into_iter()
            .filter(|s| s.estado == EstadoExperimental::Aceitavel)
            .map(|s| s.regra)
            .collect()
    }

    /// R-AR: aceita todas as `regras` do SID numa única gravação (tudo ou
    /// nada). Devolve só as que ainda não estavam aceitas; sem novidade, não
    /// grava nada (idempotente).
    pub(crate) fn aceitar_todas(
        &mut self,
        identidade: &IdentidadeCliente,
        regras: &BTreeSet<String>,
    ) -> Result<Vec<String>, Erro> {
        if let Some(r) = regras.iter().find(|r| !self.experimental(r)) {
            return Err(invalida(format!("regra experimental desconhecida: {r}")));
        }
        let mut dados = self.dados.clone();
        let aceites = &mut dados
            .por_sid
            .entry(identidade.sid.clone())
            .or_default()
            .aceites;
        let novas: Vec<String> = regras
            .iter()
            .filter(|r| aceites.insert((*r).clone()))
            .cloned()
            .collect();
        if novas.is_empty() {
            return Ok(novas);
        }
        self.armazenamento
            .salvar(&dados)
            .map_err(erro_armazenamento)?;
        self.dados = dados;
        for (id, sessao) in &mut self.sessoes {
            if id.sid == identidade.sid {
                sessao.invalidar();
            }
        }
        Ok(novas)
    }

    pub(super) fn experimental(&self, regra: &str) -> bool {
        self.drs.contains(regra)
            || self
                .metadados
                .get(regra)
                .is_some_and(|r| r.status == Status::Experimental)
            || self
                .catalogo_jogo
                .regras()
                .iter()
                .any(|r| r.id == regra && r.status == Status::Experimental)
    }

    pub(super) fn definir_aceite(
        &mut self,
        identidade: &IdentidadeCliente,
        regra: &str,
        aceito: bool,
    ) -> Result<Resposta, Erro> {
        // A lista DRS usa o mesmo filtro exposto para a sessão de console.
        if !self.experimental(regra) {
            return Err(invalida("regra experimental desconhecida"));
        }
        let mut dados = self.dados.clone();
        let aceites = &mut dados
            .por_sid
            .entry(identidade.sid.clone())
            .or_default()
            .aceites;
        let mudou = if aceito {
            aceites.insert(regra.to_owned())
        } else {
            aceites.remove(regra)
        };
        if mudou {
            self.armazenamento
                .salvar(&dados)
                .map_err(erro_armazenamento)?;
            self.dados = dados;
            for (id, sessao) in &mut self.sessoes {
                if id.sid == identidade.sid {
                    sessao.invalidar();
                }
            }
        }
        Ok(Resposta::Ok)
    }
}
