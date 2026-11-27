//! Relato ao serviço: envio de resultados pendentes, saúde avulsa e
//! registro dos resultados da tarefa.

use super::*;

impl<A: LeituraUsuario + EscritaUsuario, T: Transporte, P: Persistencia> Execucao<'_, A, T, P> {
    pub(super) fn enviar_pendente(
        &mut self,
        tarefa_atual: Option<&TarefaUsuario>,
    ) -> io::Result<Controle> {
        let Some(t) = self
            .sessao()
            .trabalho
            .as_ref()
            .filter(|t| t.pronto && !t.confirmado)
        else {
            return Ok(Controle::Continuar);
        };
        let anterior_obsoleto = tarefa_atual.is_some_and(|atual| atual.tarefa != t.tarefa)
            || t.tarefa.revisao < self.sessao().revisao;
        let req = Requisicao::AgenteRelatar {
            id_tarefa: t.tarefa.id_tarefa,
            operacao: t.tarefa.operacao,
            revisao: t.tarefa.revisao,
            resultados: t.resultados.clone(),
            resultados_jogo: t.resultados_jogo.clone(),
            saude: self.saude_pendente(),
        };
        let com_saude = matches!(&req, Requisicao::AgenteRelatar { saude: Some(_), .. });
        match self.agente.transporte.trocar(&req)? {
            Resposta::Encerrar => return Ok(Controle::Encerrar),
            Resposta::Ok => {
                if com_saude {
                    self.saude_confirmada();
                }
                let sessao = self.sessao_mut();
                let trabalho = sessao.trabalho.as_mut().expect("relato existente");
                trabalho.confirmado = true;
                if let Some(geracao) = trabalho.geracao_concluida {
                    sessao.geracao = sessao.geracao.max(geracao);
                }
            }
            Resposta::Erro { .. } if anterior_obsoleto => self.sessao_mut().trabalho = None,
            _ => return Err(erro("relato não confirmado pelo serviço")),
        }
        self.salvar()?;
        Ok(Controle::Continuar)
    }

    /// Relato só de saúde, sem tarefa (o serviço aceita sem tarefa em voo).
    /// A pendência só sai após `Ok`; qualquer outra resposta a mantém para a
    /// consulta seguinte, no mesmo ritmo das consultas.
    pub(super) fn enviar_saude_avulsa(&mut self) -> io::Result<Controle> {
        if self
            .sessao()
            .trabalho
            .as_ref()
            .is_some_and(|t| !t.confirmado)
        {
            return Ok(Controle::Continuar);
        }
        let Some(saude) = self.saude_pendente() else {
            return Ok(Controle::Continuar);
        };
        let req = Requisicao::AgenteRelatar {
            id_tarefa: 0,
            operacao: OperacaoUsuario::Aplicar,
            revisao: self.sessao().revisao,
            resultados: Vec::new(),
            resultados_jogo: Vec::new(),
            saude: Some(saude),
        };
        match self.agente.transporte.trocar(&req)? {
            Resposta::Encerrar => Ok(Controle::Encerrar),
            Resposta::Ok => {
                self.saude_confirmada();
                Ok(Controle::Continuar)
            }
            _ => Ok(Controle::Continuar),
        }
    }

    pub(super) fn registrar(
        &mut self,
        sessao: u32,
        tarefa: Option<&ReferenciaTarefa>,
        r: ResultadoUsuario,
    ) {
        if let Some(trabalho) = self
            .estado
            .sessoes
            .get_mut(&sessao)
            .and_then(|s| s.trabalho.as_mut())
            .filter(|t| Some(&t.tarefa) == tarefa && !t.pronto)
        {
            if let Some(anterior) = trabalho
                .resultados
                .iter_mut()
                .find(|a| a.destino == r.destino)
            {
                *anterior = r;
            } else {
                trabalho.resultados.push(r);
            }
        }
    }

    pub(super) fn tem_intencao(&self, tarefa: &ReferenciaTarefa) -> bool {
        self.estado.destinos.iter().any(|d| {
            d.intencao.as_ref().is_some_and(|i| {
                i.sessao == self.agente.sessao && i.tarefa.as_ref() == Some(tarefa)
            })
        })
    }

    pub(super) fn resultado_registrado(&self, destino: &SettingUsuario) -> bool {
        self.sessao()
            .trabalho
            .as_ref()
            .is_some_and(|t| t.resultados.iter().any(|r| &r.destino == destino))
    }
}
