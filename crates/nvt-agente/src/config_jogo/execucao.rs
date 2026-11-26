//! Ciclo de escrita de arquivo de jogo: mesma sequência da Fase 2 (intenção e backup
//! persistidos antes, escrita, releitura, persistência do resultado e relato).

use std::io;
use std::path::Path;

use nvt_core::config_jogo::{DefArquivoJogo, FormatoArquivo, SettingJogo, ValorJogo};
use nvt_ipc::{
    ARQUIVOS_JOGO_MAX, AcaoJogo, DestinoJogoIpc, FatoConfigJogo, MotivoNaoAplicada,
    ReferenciaTarefa, ResultadoJogo, StatusMudanca,
};
use nvt_jogocfg::restauracao::{self, ChaveGravada, Modo, Restauracao};
use nvt_jogocfg::{Erro, Hash, arquivo, backup, sha256};

use super::armazem::{
    Instancia, chave, fato, ler_backup, ler_chave, motivo, texto_para, valor_core, valor_ipc,
};
use super::{
    ArquivoGerido, ChaveGerida, IntencaoJogo, OperacaoJogo, destino_ipc, final_jogo, janela,
};
use crate::ciclo::{Execucao, Transporte};
use crate::{EscritaUsuario, LeituraUsuario, Persistencia};

fn res(
    destino: &DestinoJogoIpc,
    status: StatusMudanca,
    motivo: Option<MotivoNaoAplicada>,
) -> ResultadoJogo {
    ResultadoJogo {
        destino: destino.clone(),
        status,
        motivo,
        antes: None,
        depois: None,
        hash_antes: None,
        hash_depois: None,
        chaves_alteradas: Vec::new(),
    }
}

/// Pedido sobre uma instância: um destino (aplicar/reverter/restaurar) ou todas as
/// chaves alteradas (restauração por geração).
struct Pedido<'a> {
    arquivo: &'a str,
    instancia: Option<&'a str>,
    operacao: OperacaoJogo,
    settings: Vec<SettingJogo>,
    valor: Option<ValorJogo>,
    hash_esperado: Option<&'a str>,
    tarefa: Option<&'a ReferenciaTarefa>,
    geracao: u64,
}

/// Escrita preparada, ainda não autorizada.
struct Escrita {
    novos: Vec<u8>,
    chaves_novas: Vec<ChaveGerida>,
    resultados: Vec<ResultadoJogo>,
}

impl<A: LeituraUsuario + EscritaUsuario, T: Transporte, P: Persistencia> Execucao<'_, A, T, P> {
    fn indice_jogo(&self, arquivo: &str, instancia: &str) -> Option<usize> {
        self.estado
            .jogo
            .arquivos
            .iter()
            .position(|a| a.arquivo == arquivo && a.instancia == instancia)
    }

    /// Fatos anonimizados dos arquivos do catálogo (só leitura).
    pub(crate) fn fatos_jogo(&self) -> Vec<FatoConfigJogo> {
        let Some(amb) = self.agente.jogo.as_ref() else {
            return Vec::new();
        };
        let base = &self.agente.base;
        let mut v = Vec::new();
        for def in base.config_jogo().arquivos() {
            if v.len() >= ARQUIVOS_JOGO_MAX {
                break;
            }
            let Some(inst) = amb.resolver(base, def) else {
                continue;
            };
            let gerido = self
                .indice_jogo(&def.id, &inst.instancia)
                .map(|i| &self.estado.jogo.arquivos[i]);
            if let Some(f) = fato(amb, def, &inst, gerido) {
                v.push(f);
            }
        }
        v
    }

    fn registrar_jogo(&mut self, sessao: u32, tarefa: Option<&ReferenciaTarefa>, r: ResultadoJogo) {
        if let Some(trabalho) = self
            .estado
            .sessoes
            .get_mut(&sessao)
            .and_then(|s| s.trabalho.as_mut())
            .filter(|t| Some(&t.tarefa) == tarefa && !t.pronto)
        {
            match trabalho
                .resultados_jogo
                .iter_mut()
                .find(|a| a.destino == r.destino)
            {
                Some(anterior) => *anterior = r,
                None => trabalho.resultados_jogo.push(r),
            }
        }
    }

    fn resultado_jogo_registrado(&self, destino: &DestinoJogoIpc) -> bool {
        self.sessao()
            .trabalho
            .as_ref()
            .is_some_and(|t| t.resultados_jogo.iter().any(|r| &r.destino == destino))
    }

    pub(crate) fn tem_intencao_jogo(&self, tarefa: &ReferenciaTarefa) -> bool {
        self.estado.jogo.arquivos.iter().any(|a| {
            a.intencao.as_ref().is_some_and(|i| {
                i.sessao == self.agente.sessao && i.tarefa.as_ref() == Some(tarefa)
            })
        })
    }

    /// Há instância com snapshot anterior à geração `g`.
    pub(crate) fn geracao_jogo_pendente(&self, g: u64) -> bool {
        self.estado.jogo.arquivos.iter().any(|a| {
            a.geracao < g && (a.intencao.is_some() || a.chaves.iter().any(ChaveGerida::alterada))
        })
    }

    /// Executa uma ação da tarefa de jogo (uma vez por destino).
    pub(crate) fn executar_jogo(
        &mut self,
        acao: &AcaoJogo,
        tarefa: &ReferenciaTarefa,
        geracao: u64,
    ) -> io::Result<()> {
        let destino = acao.destino().clone();
        if self.resultado_jogo_registrado(&destino) {
            return Ok(());
        }
        let nvt_ipc::AlvoIpc::ConfigJogo { jogo, arquivo } = &destino.alvo;
        let setting = SettingJogo::de_codigo(&destino.setting)
            .filter(|s| s.jogo() == jogo && s.arquivo() == arquivo);
        let resultados = match setting {
            None => vec![res(&destino, StatusMudanca::Recusada, None)],
            Some(setting) => {
                let (operacao, valor, hash) = match acao {
                    AcaoJogo::Aplicar {
                        valor,
                        hash_esperado,
                        ..
                    } => (
                        OperacaoJogo::Aplicar,
                        Some(valor_core(valor)),
                        hash_esperado,
                    ),
                    AcaoJogo::Restaurar { hash_esperado, .. } => {
                        (OperacaoJogo::Restaurar, None, hash_esperado)
                    }
                    AcaoJogo::Reverter { hash_esperado, .. } => {
                        (OperacaoJogo::Reverter, None, hash_esperado)
                    }
                };
                self.operar(&Pedido {
                    arquivo,
                    instancia: Some(&destino.instancia),
                    operacao,
                    settings: vec![setting],
                    valor,
                    hash_esperado: Some(hash),
                    tarefa: Some(tarefa),
                    geracao,
                })?
            }
        };
        for r in resultados {
            self.registrar_jogo(self.agente.sessao, Some(tarefa), r);
        }
        self.salvar()
    }

    /// Restauração por geração (`Some`) ou do MSI (`None`): devolve o original de
    /// cada instância gerida (inteiro se o hash bate; senão só as chaves do app).
    pub(crate) fn restaurar_jogo(
        &mut self,
        geracao: Option<u64>,
        tarefa: Option<&ReferenciaTarefa>,
    ) -> io::Result<Vec<ResultadoJogo>> {
        let mut todos = Vec::new();
        for i in 0..self.estado.jogo.arquivos.len() {
            let a = &self.estado.jogo.arquivos[i];
            if geracao.is_some_and(|g| a.geracao >= g) {
                continue;
            }
            let settings: Vec<_> = a
                .chaves
                .iter()
                .filter(|c| c.alterada())
                .map(|c| c.setting)
                .collect();
            if settings.is_empty() && a.intencao.is_none() {
                if let Some(g) = geracao {
                    self.estado.jogo.arquivos[i].geracao = g;
                    self.salvar()?;
                }
                continue;
            }
            let (arquivo, instancia) = (a.arquivo.clone(), a.instancia.clone());
            let rs = self.operar(&Pedido {
                arquivo: &arquivo,
                instancia: Some(&instancia),
                operacao: if geracao.is_some() {
                    OperacaoJogo::Snapshot
                } else {
                    OperacaoJogo::SnapshotManual
                },
                settings,
                valor: None,
                hash_esperado: None,
                tarefa,
                geracao: geracao.unwrap_or(self.estado.geracao_vista),
            })?;
            if let Some(g) = geracao
                && rs.iter().all(final_jogo)
            {
                let a = &mut self.estado.jogo.arquivos[i];
                a.geracao = a.geracao.max(g);
            }
            for r in &rs {
                self.registrar_jogo(self.agente.sessao, tarefa, r.clone());
            }
            self.salvar()?;
            todos.extend(rs);
        }
        Ok(todos)
    }

    fn operar(&mut self, p: &Pedido<'_>) -> io::Result<Vec<ResultadoJogo>> {
        use StatusMudanca::*;
        let Some(def) = self.agente.base.config_jogo().arquivo(p.arquivo).cloned() else {
            return Ok(Vec::new());
        };
        let jogo = def.jogo.as_str().to_owned();
        let instancia_pedida = p.instancia.unwrap_or(super::armazem::INSTANCIA_LOCAL);
        let destinos: Vec<_> = p
            .settings
            .iter()
            .map(|s| destino_ipc(&def.id, &jogo, *s, instancia_pedida))
            .collect();
        let todos = |status, motivo: Option<MotivoNaoAplicada>| {
            Ok(destinos
                .iter()
                .map(|d| res(d, status, motivo.clone()))
                .collect())
        };
        let Some(amb) = self.agente.jogo.as_ref() else {
            return todos(Pendente, None);
        };
        let Some(inst) = amb.resolver(&self.agente.base, &def) else {
            return todos(Recusada, Some(MotivoNaoAplicada::ArquivoAusente));
        };
        if inst.conta_ambigua {
            return todos(Recusada, Some(MotivoNaoAplicada::ContaAmbigua));
        }
        if inst.instancia != instancia_pedida {
            // Outra conta ativa desde a consulta: o serviço replaneja.
            return todos(Pendente, None);
        }
        if inst.na_nuvem {
            return todos(Recusada, Some(MotivoNaoAplicada::ArquivoNaNuvem));
        }
        let indice = self.indice_jogo(&def.id, &inst.instancia);
        if indice.is_some_and(|i| self.estado.jogo.arquivos[i].intencao.is_some()) {
            return todos(Pendente, None);
        }
        let lido = match arquivo::ler(&inst.raiz, &inst.caminho) {
            Ok(l) => l,
            Err(Erro::Io(_)) => return todos(Pendente, None),
            Err(e) => return todos(Recusada, motivo(&e)),
        };
        if lido.somente_leitura {
            return todos(Recusada, Some(MotivoNaoAplicada::ArquivoSomenteLeitura));
        }
        let hash_atual = lido.hash.hex();
        if let Some(esperado) = p.hash_esperado {
            // Ações anteriores da mesma tarefa no mesmo arquivo físico já mudaram o hash.
            let encadeado = self
                .cadeia
                .get(&inst.caminho)
                .is_some_and(|(antes, depois)| antes == esperado && *depois == hash_atual);
            if esperado != hash_atual && !encadeado {
                return todos(Recusada, Some(MotivoNaoAplicada::ArquivoAlteradoPeloJogo));
            }
        }
        let gerido = indice.map(|i| self.estado.jogo.arquivos[i].clone());
        let preparada = match p.operacao {
            OperacaoJogo::Aplicar => {
                let (Some(valor), [setting]) = (&p.valor, p.settings.as_slice()) else {
                    return todos(Recusada, None);
                };
                self.preparar_aplicar(
                    &def,
                    &lido.bytes,
                    gerido.as_ref(),
                    indice,
                    *setting,
                    valor,
                    &destinos[0],
                )?
            }
            _ => {
                self.preparar_restaurar(&def, &lido.bytes, gerido.as_ref(), indice, p, &destinos)?
            }
        };
        let escrita = match preparada {
            Ok(e) => e,
            Err(resultados) => return Ok(resultados),
        };
        self.escrever_jogo(&def, &inst, &lido, indice, escrita, p)
    }

    #[expect(clippy::too_many_arguments, reason = "passos explícitos do ciclo")]
    fn preparar_aplicar(
        &mut self,
        def: &DefArquivoJogo,
        bytes: &[u8],
        gerido: Option<&ArquivoGerido>,
        indice: Option<usize>,
        setting: SettingJogo,
        pedido: &ValorJogo,
        destino: &DestinoJogoIpc,
    ) -> io::Result<Result<Escrita, Vec<ResultadoJogo>>> {
        use StatusMudanca::*;
        let um = |status, motivo| Ok(Err(vec![res(destino, status, motivo)]));
        let Some(c) = def.chave.iter().find(|c| c.setting == setting) else {
            return um(Recusada, None);
        };
        if !c.aceita(pedido) {
            return um(Recusada, None);
        }
        let (bruto, atual) = match ler_chave(bytes, def, c) {
            Ok(v) => v,
            Err(e) => return um(Recusada, motivo(&e)),
        };
        let anterior = gerido.and_then(|g| g.chaves.iter().find(|k| k.setting == setting));
        if let Some(k) = anterior {
            if k.travado {
                return um(Travada, None);
            }
            if k.alterada() && k.esperado != atual {
                // Deriva: o jogo ou o usuário mudou o valor; nunca regravar em loop.
                let i = indice.expect("gestão existente");
                if let Some(k) = self.estado.jogo.arquivos[i]
                    .chaves
                    .iter_mut()
                    .find(|k| k.setting == setting)
                {
                    k.travado = true;
                }
                self.salvar()?;
                return um(Travada, Some(MotivoNaoAplicada::ArquivoAlteradoPeloJogo));
            }
        }
        if atual == *pedido {
            let mut r = res(destino, JaNoValor, None);
            r.antes = Some(valor_ipc(&atual));
            r.depois = Some(valor_ipc(&atual));
            return Ok(Err(vec![r]));
        }
        let novo_bruto = match texto_para(c, &bruto, pedido) {
            Ok(t) => t,
            Err(e) => return um(Recusada, motivo(&e)),
        };
        let novos =
            match chave(def, c).and_then(|k| nvt_jogocfg::definir_valor(bytes, &k, &novo_bruto)) {
                Ok(n) => n,
                Err(e) => return um(Recusada, motivo(&e)),
            };
        let mut chaves_novas: Vec<_> = gerido.map(|g| g.chaves.clone()).unwrap_or_default();
        let nova = ChaveGerida {
            setting,
            antes: anterior.map_or_else(|| atual.clone(), |k| k.antes.clone()),
            esperado: pedido.clone(),
            texto_antes: anterior.map_or_else(|| bruto.clone(), |k| k.texto_antes.clone()),
            texto_gravado: novo_bruto,
            travado: false,
        };
        match chaves_novas.iter_mut().find(|k| k.setting == setting) {
            Some(k) => *k = nova,
            None => chaves_novas.push(nova),
        }
        let mut r = res(destino, Aplicada, None);
        r.antes = Some(valor_ipc(&atual));
        r.depois = Some(valor_ipc(pedido));
        r.chaves_alteradas = vec![setting.codigo().to_owned()];
        Ok(Ok(Escrita {
            novos,
            chaves_novas,
            resultados: vec![r],
        }))
    }

    fn preparar_restaurar(
        &mut self,
        def: &DefArquivoJogo,
        bytes: &[u8],
        gerido: Option<&ArquivoGerido>,
        indice: Option<usize>,
        p: &Pedido<'_>,
        destinos: &[DestinoJogoIpc],
    ) -> io::Result<Result<Escrita, Vec<ResultadoJogo>>> {
        use StatusMudanca::*;
        let destino_de = |s: SettingJogo| {
            destinos
                .iter()
                .find(|d| d.setting == s.codigo())
                .expect("destino do pedido")
        };
        let ja_no_valor = |s: SettingJogo| res(destino_de(s), JaNoValor, None);
        let Some(g) = gerido else {
            return Ok(Err(p.settings.iter().map(|s| ja_no_valor(*s)).collect()));
        };
        let alvo: Vec<&ChaveGerida> = g
            .chaves
            .iter()
            .filter(|c| c.alterada() && p.settings.contains(&c.setting))
            .collect();
        if alvo.is_empty() {
            if p.operacao == OperacaoJogo::Reverter {
                let i = indice.expect("gestão existente");
                for c in &mut self.estado.jogo.arquivos[i].chaves {
                    if p.settings.contains(&c.setting) {
                        c.travado = true;
                    }
                }
                self.salvar()?;
            }
            return Ok(Err(p.settings.iter().map(|s| ja_no_valor(*s)).collect()));
        }
        let original = match ler_backup(
            self.agente.jogo.as_ref().expect("ambiente"),
            def.jogo.as_str(),
            g,
        ) {
            Ok(b) => b,
            Err(_) => {
                return Ok(Err(p
                    .settings
                    .iter()
                    .map(|s| res(destino_de(*s), Falhou, None))
                    .collect()));
            }
        };
        // Inteiro só se todas as chaves alteradas saem agora, o backup é o arquivo de
        // antes da gestão e nenhum outro destino gerido usa o mesmo arquivo físico.
        let todas = g.chaves.iter().filter(|c| c.alterada()).count() == alvo.len();
        let compartilhado = self.estado.jogo.arquivos.iter().any(|o| {
            o.arquivo != g.arquivo
                && o.instancia == g.instancia
                && o.chaves.iter().any(ChaveGerida::alterada)
                && self
                    .agente
                    .base
                    .config_jogo()
                    .arquivo(&o.arquivo)
                    .is_some_and(|d| d.raiz == def.raiz && d.caminho == def.caminho)
        });
        let hash_depois = g
            .hash_depois
            .as_deref()
            .and_then(Hash::de_hex)
            .filter(|_| g.restaurar_inteiro && todas && !compartilhado)
            .unwrap_or(Hash([0; 32]));
        let modo = if def.formato == FormatoArquivo::Lancamento {
            Modo::TokenLancamento
        } else {
            Modo::Valor
        };
        let mut gravadas = Vec::new();
        for c in &alvo {
            let Some(dc) = def.chave.iter().find(|d| d.setting == c.setting) else {
                return Ok(Err(vec![res(destino_de(c.setting), Recusada, None)]));
            };
            match chave(def, dc) {
                Ok(k) => gravadas.push((
                    c.setting,
                    ChaveGravada {
                        chave: k,
                        modo,
                        valor_antes: c.texto_antes.clone(),
                        valor_gravado: c.texto_gravado.clone(),
                    },
                )),
                Err(e) => return Ok(Err(vec![res(destino_de(c.setting), Recusada, motivo(&e))])),
            }
        }
        let lista: Vec<_> = gravadas.iter().map(|(_, c)| c.clone()).collect();
        let (novos, revertidas, alteradas): (Option<Vec<u8>>, Vec<SettingJogo>, Vec<SettingJogo>) =
            match restauracao::planejar(bytes, &original, &hash_depois, &lista) {
                Ok(Restauracao::Original(b)) => (
                    Some(b),
                    gravadas.iter().map(|(s, _)| *s).collect(),
                    Vec::new(),
                ),
                Ok(Restauracao::Parcial {
                    bytes,
                    revertidas,
                    alteradas_pelo_jogo,
                }) => {
                    let mapa = |ks: &[nvt_jogocfg::Chave]| {
                        gravadas
                            .iter()
                            .filter(|(_, c)| ks.contains(&c.chave))
                            .map(|(s, _)| *s)
                            .collect::<Vec<_>>()
                    };
                    (Some(bytes), mapa(&revertidas), mapa(&alteradas_pelo_jogo))
                }
                Err(Erro::ArquivoAlteradoPeloJogo) => {
                    (None, Vec::new(), gravadas.iter().map(|(s, _)| *s).collect())
                }
                Err(e) => {
                    return Ok(Err(p
                        .settings
                        .iter()
                        .map(|s| res(destino_de(*s), Recusada, motivo(&e)))
                        .collect()));
                }
            };
        let reverter = p.operacao == OperacaoJogo::Reverter;
        let chaves_novas: Vec<ChaveGerida> = g
            .chaves
            .iter()
            .filter_map(|c| {
                if revertidas.contains(&c.setting) {
                    reverter.then(|| ChaveGerida {
                        esperado: c.antes.clone(),
                        texto_gravado: c.texto_antes.clone(),
                        travado: true,
                        ..c.clone()
                    })
                } else if alteradas.contains(&c.setting) {
                    Some(ChaveGerida {
                        travado: true,
                        ..c.clone()
                    })
                } else {
                    Some(c.clone())
                }
            })
            .collect();
        let mut resultados = Vec::new();
        for s in &p.settings {
            let d = destino_de(*s);
            let c = g.chaves.iter().find(|c| c.setting == *s);
            let mut r = if revertidas.contains(s) {
                let mut r = res(d, Aplicada, None);
                r.chaves_alteradas = vec![s.codigo().to_owned()];
                r
            } else if alteradas.contains(s) {
                res(d, Travada, Some(MotivoNaoAplicada::ArquivoAlteradoPeloJogo))
            } else {
                res(d, JaNoValor, None)
            };
            if let Some(c) = c {
                r.antes = Some(valor_ipc(&c.esperado));
                r.depois = Some(valor_ipc(&c.antes));
            }
            resultados.push(r);
        }
        let Some(novos) = novos.filter(|_| !revertidas.is_empty()) else {
            // Nada a gravar: o jogo mudou todas as chaves do app; só a gestão trava.
            let i = indice.expect("gestão existente");
            self.estado.jogo.arquivos[i].chaves = chaves_novas;
            self.salvar()?;
            return Ok(Err(resultados));
        };
        Ok(Ok(Escrita {
            novos,
            chaves_novas,
            resultados,
        }))
    }

    /// Backup (1ª vez) → intenção → portão duplo → gravação com hash → releitura.
    fn escrever_jogo(
        &mut self,
        def: &DefArquivoJogo,
        inst: &Instancia,
        lido: &arquivo::Lido,
        indice: Option<usize>,
        escrita: Escrita,
        p: &Pedido<'_>,
    ) -> io::Result<Vec<ResultadoJogo>> {
        use StatusMudanca::*;
        let Escrita {
            novos,
            chaves_novas,
            mut resultados,
        } = escrita;
        let marcar = |rs: &mut Vec<ResultadoJogo>, status, motivo: Option<MotivoNaoAplicada>| {
            for r in rs.iter_mut() {
                r.status = status;
                r.motivo = motivo.clone();
                r.chaves_alteradas.clear();
            }
        };
        let jogo = def.jogo.as_str();
        let processos = janela::processos(&self.agente.base, def);
        let amb = self.agente.jogo.as_ref().expect("ambiente");
        let mut backup_novo: Option<std::path::PathBuf> = None;
        let i = match indice {
            Some(i) => {
                if ler_backup(amb, jogo, &self.estado.jogo.arquivos[i]).is_err() {
                    // Backup comprometido: nenhuma escrita.
                    marcar(&mut resultados, Falhou, None);
                    return Ok(resultados);
                }
                i
            }
            None => {
                let nome = format!("{}-{}", def.id, inst.instancia);
                let b = match amb
                    .dir_backups()
                    .and_then(|dir| backup::criar(&dir, jogo, &nome, &lido.bytes))
                {
                    Ok(b) => b,
                    Err(_) => {
                        marcar(&mut resultados, Falhou, None);
                        return Ok(resultados);
                    }
                };
                backup_novo = b.criado_agora.then(|| b.caminho.clone());
                self.estado.jogo.arquivos.push(ArquivoGerido {
                    arquivo: def.id.clone(),
                    instancia: inst.instancia.clone(),
                    backup: nome,
                    hash_original: b.hash.hex(),
                    hash_depois: None,
                    geracao: p.geracao,
                    restaurar_inteiro: true,
                    chaves: Vec::new(),
                    intencao: None,
                });
                self.estado.jogo.arquivos.len() - 1
            }
        };
        let hash_anterior = lido.hash.hex();
        let g = &self.estado.jogo.arquivos[i];
        let restaurar_inteiro = if p.operacao != OperacaoJogo::Aplicar {
            g.restaurar_inteiro
        } else if !g.chaves.iter().any(ChaveGerida::alterada) {
            hash_anterior == g.hash_original
        } else {
            g.restaurar_inteiro && g.hash_depois.as_deref() == Some(hash_anterior.as_str())
        };
        for r in &mut resultados {
            r.hash_antes = Some(hash_anterior.clone());
        }
        self.estado.jogo.arquivos[i].intencao = Some(IntencaoJogo {
            operacao: p.operacao,
            hash_anterior: hash_anterior.clone(),
            hash_novo: sha256(&novos).hex(),
            chaves_novas,
            restaurar_inteiro,
            resultados: resultados.clone(),
            sessao: self.agente.sessao,
            tarefa: p.tarefa.copied(),
            geracao: p.geracao,
            raiz: inst.raiz.clone(),
            caminho: inst.caminho.clone(),
        });
        self.salvar()?;
        // Portão duplo imediatamente antes da gravação (competitivo §3): nenhuma E/S
        // entre a consulta e o `gravar`.
        let portao = janela::verificar(
            &self.agente.portao,
            &self.agente.base,
            self.vigiados.as_ref(),
            &processos,
        );
        if let Err(b) = portao {
            // Recusa: a intenção nunca foi executada; some do estado para a recuperação
            // não a tomar por gravação interrompida. Gestão recém-criada também some, com
            // o backup criado agora (órfão, divergiria do arquivo na próxima tentativa).
            if indice.is_none() {
                self.estado.jogo.arquivos.pop();
            } else {
                self.estado.jogo.arquivos[i].intencao = None;
            }
            self.salvar()?;
            if let Some(orig) = backup_novo {
                let mut h = orig.clone().into_os_string();
                h.push(".sha256");
                let _ = std::fs::remove_file(h);
                let _ = std::fs::remove_file(&orig);
                // Pastas vazias criadas para o backup (jogo e base), se vazias.
                let base = self.agente.jogo.as_ref().and_then(|a| a.dir_backups().ok());
                let mut pasta = orig.parent().map(Path::to_path_buf);
                while let Some(p) = pasta {
                    if base.as_deref().is_none_or(|b| !p.starts_with(b))
                        || std::fs::remove_dir(&p).is_err()
                    {
                        break;
                    }
                    pasta = p.parent().map(Path::to_path_buf);
                }
            }
            marcar(&mut resultados, Pendente, b.motivo());
            return Ok(resultados);
        }
        let amb = self.agente.jogo.as_ref().expect("ambiente");
        let gravado = amb
            .gravador
            .gravar(&inst.raiz, &inst.caminho, &novos, &lido.hash);
        let hash_novo = sha256(&novos).hex();
        let rs = match gravado {
            Ok(h) => self.finalizar_jogo(i, Some(h.hex()), None)?,
            Err(e) => match arquivo::ler(&inst.raiz, &inst.caminho) {
                Ok(l) if l.hash.hex() == hash_novo => {
                    self.finalizar_jogo(i, Some(hash_novo.clone()), None)?
                }
                Ok(l) if l.hash == lido.hash => {
                    let status = if matches!(e, Erro::Io(_)) {
                        Falhou
                    } else {
                        Recusada
                    };
                    self.finalizar_jogo(i, None, Some((status, motivo(&e))))?
                }
                Ok(_) => self.finalizar_jogo(
                    i,
                    None,
                    Some((Recusada, Some(MotivoNaoAplicada::ArquivoAlteradoPeloJogo))),
                )?,
                Err(_) => {
                    // Releitura indisponível: a intenção fica para a recuperação.
                    marcar(&mut resultados, Pendente, None);
                    return Ok(resultados);
                }
            },
        };
        if rs.iter().any(|r| r.status == Aplicada) {
            let entrada = self
                .cadeia
                .entry(inst.caminho.clone())
                .or_insert_with(|| (hash_anterior, String::new()));
            entrada.1 = hash_novo;
        }
        Ok(rs)
    }

    /// Conclui a intenção: `hash = Some` confirma a gravação; senão desfaz a intenção
    /// com o status dado.
    fn finalizar_jogo(
        &mut self,
        i: usize,
        hash: Option<String>,
        falha: Option<(StatusMudanca, Option<MotivoNaoAplicada>)>,
    ) -> io::Result<Vec<ResultadoJogo>> {
        let a = &mut self.estado.jogo.arquivos[i];
        let intencao = a.intencao.take().expect("intenção persistida");
        let mut rs = intencao.resultados.clone();
        match (hash, falha) {
            (Some(h), _) => {
                a.chaves = intencao.chaves_novas;
                a.restaurar_inteiro = intencao.restaurar_inteiro;
                a.hash_depois = a
                    .chaves
                    .iter()
                    .any(ChaveGerida::alterada)
                    .then(|| h.clone());
                if matches!(
                    intencao.operacao,
                    OperacaoJogo::Snapshot | OperacaoJogo::SnapshotManual
                ) {
                    a.geracao = a.geracao.max(intencao.geracao);
                }
                for r in &mut rs {
                    r.hash_depois = Some(h.clone());
                }
            }
            (None, falha) => {
                let (status, motivo) = falha.unwrap_or((StatusMudanca::Falhou, None));
                if status == StatusMudanca::Travada {
                    for c in &mut a.chaves {
                        if rs.iter().any(|r| r.destino.setting == c.setting.codigo()) {
                            c.travado = true;
                        }
                    }
                }
                for r in &mut rs {
                    r.status = status;
                    r.motivo = motivo.clone();
                    r.depois = None;
                    r.chaves_alteradas.clear();
                }
            }
        }
        for r in &rs {
            self.registrar_jogo(intencao.sessao, intencao.tarefa.as_ref(), r.clone());
        }
        self.salvar()?;
        Ok(rs)
    }

    /// Recupera intenções interrompidas pelo hash do arquivo, sem dupla aplicação.
    pub(crate) fn recuperar_jogo(&mut self) -> io::Result<()> {
        self.preparar_segredo()?;
        for i in 0..self.estado.jogo.arquivos.len() {
            let Some(intencao) = self.estado.jogo.arquivos[i].intencao.clone() else {
                continue;
            };
            let Ok(lido) = arquivo::ler(&intencao.raiz, &intencao.caminho) else {
                continue;
            };
            let h = lido.hash.hex();
            if h == intencao.hash_novo {
                self.finalizar_jogo(i, Some(h), None)?;
            } else if h == intencao.hash_anterior {
                self.finalizar_jogo(i, None, Some((StatusMudanca::Falhou, None)))?;
            } else {
                self.finalizar_jogo(
                    i,
                    None,
                    Some((
                        StatusMudanca::Travada,
                        Some(MotivoNaoAplicada::ArquivoAlteradoPeloJogo),
                    )),
                )?;
            }
            limpar_temporarios(&intencao.raiz, &intencao.caminho);
        }
        Ok(())
    }

    /// Carrega (ou gera e persiste na primeira vez) o segredo das instâncias de conta
    /// e migra a gestão gravada com a instância legada sem segredo.
    fn preparar_segredo(&mut self) -> io::Result<()> {
        if self.agente.jogo.is_none() {
            return Ok(());
        }
        let segredo = match self.estado.jogo.segredo() {
            Some(s) => s,
            None => {
                let s = super::armazem::gerar_segredo()?;
                self.estado.jogo.segredo_conta =
                    Some(s.iter().map(|b| format!("{b:02x}")).collect());
                self.salvar()?;
                s
            }
        };
        if let Some(amb) = self.agente.jogo.as_mut() {
            amb.segredo = Some(segredo);
        }
        let Some(amb) = self.agente.jogo.as_ref() else {
            return Ok(());
        };
        let base = &self.agente.base;
        let mut mudou = false;
        for a in &mut self.estado.jogo.arquivos {
            if a.instancia == super::armazem::INSTANCIA_LOCAL {
                continue;
            }
            let Some(def) = base.config_jogo().arquivo(&a.arquivo) else {
                continue;
            };
            if amb.instancia_legada(def).as_deref() != Some(a.instancia.as_str()) {
                continue;
            }
            if let Some(inst) = amb.resolver(base, def).filter(|i| !i.conta_ambigua) {
                a.instancia = inst.instancia;
                mudou = true;
            }
        }
        if mudou {
            self.salvar()?;
        }
        Ok(())
    }

    /// Deriva na janela fechada: valor diferente do gravado trava a chave (nunca regrava).
    pub(crate) fn deriva_jogo(&mut self) -> io::Result<()> {
        let Some(amb) = self.agente.jogo.as_ref() else {
            return Ok(());
        };
        let base = &self.agente.base;
        let mut mudou = false;
        for a in &mut self.estado.jogo.arquivos {
            if a.intencao.is_some() || !a.chaves.iter().any(|c| c.alterada() && !c.travado) {
                continue;
            }
            let Some(def) = base.config_jogo().arquivo(&a.arquivo) else {
                continue;
            };
            let Some(inst) = amb
                .resolver(base, def)
                .filter(|i| i.instancia == a.instancia)
            else {
                continue;
            };
            if janela::fechada(&self.agente.portao, base, &janela::processos(base, def)).is_err() {
                continue;
            }
            let Ok(lido) = arquivo::ler(&inst.raiz, &inst.caminho) else {
                continue;
            };
            for c in a.chaves.iter_mut().filter(|c| c.alterada() && !c.travado) {
                let atual = def
                    .chave
                    .iter()
                    .find(|d| d.setting == c.setting)
                    .and_then(|d| ler_chave(&lido.bytes, def, d).ok())
                    .map(|(_, v)| v);
                if atual.as_ref() != Some(&c.esperado) {
                    c.travado = true;
                    mudou = true;
                }
            }
        }
        if mudou {
            self.salvar()?;
        }
        Ok(())
    }
}

/// Remove temporários `<nome>.nvt-*.tmp` deixados por uma queda entre a escrita do
/// temporário e o `ReplaceFileW`.
fn limpar_temporarios(raiz: &Path, caminho: &Path) {
    let (Some(pasta), Some(nome)) = (caminho.parent(), caminho.file_name()) else {
        return;
    };
    if arquivo::verificar_componentes(raiz, pasta).is_err() {
        return;
    }
    let prefixo = format!("{}.nvt-", nome.to_string_lossy());
    let Ok(entradas) = std::fs::read_dir(pasta) else {
        return;
    };
    for e in entradas.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with(&prefixo)
            && n.ends_with(".tmp")
            && e.file_type().is_ok_and(|t| t.is_file())
        {
            let _ = std::fs::remove_file(e.path());
        }
    }
}
