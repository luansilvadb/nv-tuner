//! Snapshots e restauração: captura do valor anterior, conclusão,
//! recuperação e compensação.

use super::*;

impl<A: LeituraUsuario + EscritaUsuario, T: Transporte, P: Persistencia> Execucao<'_, A, T, P> {
    pub(super) fn restaurar_snapshot(
        &mut self,
        geracao: Option<u64>,
        tarefa: Option<&ReferenciaTarefa>,
    ) -> io::Result<BTreeMap<SettingUsuario, ResultadoUsuario>> {
        let destinos: Vec<_> = self
            .estado
            .destinos
            .iter()
            .filter(|d| self.acessivel(d) && geracao.is_none_or(|g| d.geracao_snapshot < g))
            .map(|d| d.destino.clone())
            .collect();
        let mut resultados = BTreeMap::new();
        let operacao = if geracao.is_some() {
            Operacao::Snapshot
        } else {
            Operacao::SnapshotManual
        };
        for destino in destinos {
            let r = self.transicao(
                &destino,
                operacao,
                None,
                tarefa,
                geracao.unwrap_or(self.estado.geracao_vista),
            )?;
            self.registrar(self.agente.sessao, tarefa, r.clone());
            self.salvar()?;
            resultados.insert(destino, r);
        }
        Ok(resultados)
    }

    pub(super) fn capturar(
        &mut self,
        destino: &SettingUsuario,
        atual: ValorLido,
        geracao: u64,
    ) -> usize {
        let i = self.estado.destinos.len();
        self.estado.destinos.push(Destino {
            destino: destino.clone(),
            sessao: self.escopo(destino),
            snapshot: atual,
            geracao_snapshot: geracao,
            antes_gestao: None,
            esperado: None,
            travado: false,
            bloquear_snapshot_ate: None,
            intencao: None,
        });
        i
    }

    pub(super) fn concluir(
        &mut self,
        i: usize,
        operacao: Operacao,
        anterior: &ValorLido,
        atual: &ValorLido,
    ) {
        if operacao.snapshot() {
            let d = self.estado.destinos.remove(i);
            if operacao == Operacao::SnapshotManual {
                let backup = Backup {
                    destino: d.destino,
                    sessao: d.sessao,
                    valor: d.snapshot,
                };
                if !self.estado.backups_msi.contains(&backup) {
                    self.estado.backups_msi.push(backup);
                }
            }
            return;
        }
        let d = &mut self.estado.destinos[i];
        if operacao == Operacao::Aplicar {
            d.antes_gestao.get_or_insert_with(|| anterior.clone());
            d.esperado = Some(atual.clone());
        } else {
            d.esperado = None;
            d.antes_gestao = None;
            d.travado = operacao == Operacao::Reverter;
        }
    }

    pub(super) fn finalizar(
        &mut self,
        i: usize,
        status: StatusMudanca,
        detalhe: Option<&str>,
    ) -> io::Result<ResultadoUsuario> {
        let d = &mut self.estado.destinos[i];
        let intencao = d.intencao.take().expect("intenção persistida");
        let mut r = resultado(&d.destino, status, detalhe);
        match status {
            StatusMudanca::Aplicada => {
                r.mudanca = mudanca(
                    &r.destino,
                    intencao.anterior.clone(),
                    intencao.pedido.clone(),
                );
                self.concluir(i, intencao.operacao, &intencao.anterior, &intencao.pedido);
            }
            StatusMudanca::Travada => {
                d.travado = true;
                d.bloquear_snapshot_ate = Some(intencao.geracao);
            }
            _ => {}
        }
        self.registrar(intencao.sessao, intencao.tarefa.as_ref(), r.clone());
        self.salvar()?;
        Ok(r)
    }

    pub(super) fn recuperar(&mut self) -> io::Result<()> {
        for i in (0..self.estado.destinos.len()).rev() {
            let d = &self.estado.destinos[i];
            let Some(intent) = d.intencao.as_ref().filter(|_| self.acessivel(d)) else {
                continue;
            };
            let Ok(atual) = self.ler(&d.destino) else {
                continue;
            };
            use StatusMudanca::*;
            if intent.rollback.is_some() {
                if iguais(&atual, &intent.anterior) {
                    self.finalizar(i, Falhou, Some("compensação confirmada"))?;
                } else if !intent.rollback.as_ref().is_some_and(|v| iguais(&atual, v)) {
                    self.finalizar(i, Travada, Some("alteração durante a compensação"))?;
                }
            } else if atingiu(&atual, &intent.pedido) {
                self.finalizar(i, Aplicada, None)?;
            } else if iguais(&atual, &intent.anterior) {
                self.finalizar(i, Falhou, Some("escrita interrompida no valor anterior"))?;
            } else {
                self.finalizar(
                    i,
                    Travada,
                    Some("intenção interrompida com valor divergente"),
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn compensacoes(&mut self) -> io::Result<()> {
        for i in 0..self.estado.destinos.len() {
            let d = &self.estado.destinos[i];
            if self.acessivel(d) && d.intencao.as_ref().is_some_and(|t| t.rollback.is_some()) {
                self.compensar(i)?;
            }
        }
        Ok(())
    }

    pub(super) fn compensar(&mut self, i: usize) -> io::Result<ResultadoUsuario> {
        use StatusMudanca::*;
        let d = &self.estado.destinos[i];
        let intent = d.intencao.as_ref().expect("intenção persistida");
        let destino = &d.destino;
        let Some(observado) = &intent.rollback else {
            return Err(erro("compensação sem releitura"));
        };
        let Ok(atual) = self.ler(destino) else {
            return Ok(resultado(
                destino,
                Pendente,
                Some("releitura da compensação indisponível"),
            ));
        };
        if iguais(&atual, &intent.anterior) {
            return self.finalizar(i, Falhou, Some("compensação confirmada"));
        }
        if !iguais(&atual, observado) {
            return self.finalizar(i, Travada, Some("valor divergiu antes da compensação"));
        }
        let destino = destino.clone();
        let anterior = intent.anterior.clone();
        if let Err(motivo) = self.escrever_autorizado(&destino, &atual, &anterior, true) {
            return Ok(resultado(&destino, Pendente, Some(motivo)));
        }
        if self.ler(&destino).is_ok_and(|v| iguais(&v, &anterior)) {
            return self.finalizar(
                i,
                Falhou,
                Some("refresh divergente; compensação confirmada"),
            );
        }
        Ok(resultado(
            &destino,
            Pendente,
            Some("compensação aguarda confirmação"),
        ))
    }
}
