//! Coordenação da camada interativa. A identidade vem exclusivamente do pipe.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nvt_core::usuario::{
    self, ConjuntoRegrasUsuario, FatosUsuario, GestaoUsuario, PlanoUsuario,
    SettingUsuario as Setting, ValorUsuario,
};
use nvt_core::{ConjuntoRegras, Qualidade, Status};
use nvt_ipc::pipe::IdentidadeCliente;
use nvt_ipc::{
    AcaoHistorico, AcaoUsuario, AjusteUsuario, Camada, CodigoErro, EntradaHistorico, EstadoAgente,
    EstadoExperimental, EstadoTarefa, Fato, ImpactoVisual, Mudanca, OperacaoUsuario, PlanoAgente,
    ProgressoTarefa, ReferenciaTarefa, Requisicao, Resposta, ResultadoUsuario, ResumoSistema,
    SettingUsuario, StatusMudanca, SugestaoExperimental, TarefaUsuario,
};
use serde::{Deserialize, Serialize};

const PRESENCA: Duration = Duration::from_secs(180);
const SETTINGS: [Setting; 4] = [
    Setting::TelaRefresh,
    Setting::MouseAceleracao,
    Setting::DxSwapEffectUpgrade,
    Setting::GamedvrCapturaHistorica,
];
const RECIBOS_POR_SESSAO: usize = 32;
const HISTORICO_POR_SID: usize = nvt_ipc::HISTORICO_MAX as usize;
type Erro = (CodigoErro, String);

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct DadosUsuarios {
    por_sid: BTreeMap<String, DadosSid>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct DadosSid {
    aceites: BTreeSet<String>,
    historico: VecDeque<EntradaHistorico>,
    recibos: BTreeMap<u32, VecDeque<Recibo>>,
}

#[derive(Clone, Deserialize, Serialize)]
struct Recibo {
    tarefa: ReferenciaTarefa,
    resultados: Vec<ResultadoUsuario>,
}

pub(crate) trait Armazenamento {
    fn carregar(&self) -> io::Result<DadosUsuarios>;
    fn salvar(&self, dados: &DadosUsuarios) -> io::Result<()>;
}

/// Deve ser construído sobre o diretório de estado privilegiado já verificado.
pub(crate) struct ArquivoUsuarios(pub PathBuf);

impl Armazenamento for ArquivoUsuarios {
    fn carregar(&self) -> io::Result<DadosUsuarios> {
        match crate::diretorio::abrir_arquivo_privado(&self.0.join("usuarios.json")) {
            Ok(f) => Ok(serde_json::from_reader(io::BufReader::new(f))?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(DadosUsuarios::default()),
            Err(e) => Err(e),
        }
    }

    fn salvar(&self, dados: &DadosUsuarios) -> io::Result<()> {
        let (tmp, mut f) = (0..100)
            .find_map(|tentativa| {
                let tmp = self.0.join(format!(
                    "usuarios.json.{}-{tentativa}.tmp",
                    std::process::id()
                ));
                match crate::diretorio::criar_arquivo_privado(&tmp) {
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => None,
                    r => Some(r.map(|f| (tmp, f))),
                }
            })
            .ok_or_else(|| io::Error::other("sem nome temporário livre para usuarios.json"))??;
        let gravado = (|| {
            serde_json::to_writer(&mut f, dados)?;
            f.flush()?;
            f.sync_all()
        })();
        drop(f);
        let r = gravado.and_then(|()| fs::rename(&tmp, self.0.join("usuarios.json")));
        if r.is_err() {
            let _ = fs::remove_file(tmp);
        }
        r
    }
}

#[derive(Deserialize)]
struct Catalogo {
    regra: Vec<MetaRegra>,
}

#[derive(Deserialize)]
struct MetaRegra {
    id: String,
    descricao: String,
    status: Status,
    impacto_visual: nvt_core::ImpactoVisual,
}

#[derive(Default)]
struct Sessao {
    contato: Option<Instant>,
    restaurada: u64,
    snapshot_ofertado: Option<u64>,
    snapshot_confirmado: Option<u64>,
    revisao: u64,
    fatos: Option<FatosUsuario>,
    diagnosticos: Vec<nvt_ipc::Diagnostico>,
    em_voo: Option<TarefaUsuario>,
    gestao: BTreeMap<SettingUsuario, GestaoUsuario>,
    travas: BTreeSet<SettingUsuario>,
    reversoes: BTreeSet<SettingUsuario>,
}

impl Sessao {
    fn invalidar(&mut self) {
        self.revisao += 1;
        self.em_voo = None;
    }

    fn presente(&self, agora: Instant) -> bool {
        self.contato
            .is_some_and(|contato| agora.saturating_duration_since(contato) < PRESENCA)
    }
}

pub(crate) struct Coordenador {
    armazenamento: Box<dyn Armazenamento>,
    dados: DadosUsuarios,
    sessoes: HashMap<IdentidadeCliente, Sessao>,
    regras: ConjuntoRegrasUsuario,
    metadados: BTreeMap<String, MetaRegra>,
    drs: BTreeSet<String>,
    geracao: u64,
    pausado: bool,
    serial: u64,
    windows_build: Option<u32>,
    processos: BTreeSet<String>,
}

impl Coordenador {
    pub(crate) fn abrir(
        armazenamento: impl Armazenamento + 'static,
        drs: &ConjuntoRegras,
        windows_build: Option<u32>,
        geracao: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let dados = armazenamento.carregar()?;
        let regras = ConjuntoRegrasUsuario::embutido(drs)?;
        let catalogo: Catalogo = toml::from_str(usuario::WINDOWS_TOML)?;
        let mut coordenador = Self {
            armazenamento: Box::new(armazenamento),
            dados,
            sessoes: HashMap::new(),
            regras,
            metadados: catalogo
                .regra
                .into_iter()
                .map(|regra| (regra.id.clone(), regra))
                .collect(),
            drs: drs
                .regras
                .iter()
                .filter(|r| r.status == Status::Experimental)
                .map(|r| r.id.clone())
                .collect(),
            geracao: 0,
            pausado: false,
            serial: SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as u64,
            windows_build,
            processos: BTreeSet::new(),
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

    pub(crate) fn tratar(
        &mut self,
        identidade: &IdentidadeCliente,
        req: &Requisicao,
        agora: Instant,
        unix: u64,
    ) -> Option<Resposta> {
        let resultado = match req {
            Requisicao::AgenteConsultar {
                fatos,
                ultima_tarefa,
                geracao_restaurada,
            } => req
                .validar()
                .and_then(|()| {
                    self.consultar(
                        identidade,
                        fatos,
                        ultima_tarefa.as_ref(),
                        *geracao_restaurada,
                        agora,
                    )
                })
                .map_err(invalida),
            Requisicao::AgenteRelatar {
                id_tarefa,
                operacao,
                revisao,
                resultados,
            } => req.validar().map_err(invalida).and_then(|()| {
                self.relatar(
                    identidade,
                    ReferenciaTarefa {
                        id_tarefa: *id_tarefa,
                        operacao: *operacao,
                        revisao: *revisao,
                    },
                    resultados,
                    agora,
                    unix,
                )
            }),
            Requisicao::DefinirAceiteExperimental { regra, aceito } => {
                self.definir_aceite(identidade, regra, *aceito)
            }
            Requisicao::ReverterAjusteUsuario { destino, .. } => req
                .validar()
                .and_then(|()| self.reverter(identidade, destino))
                .map_err(invalida),
            _ => return None,
        };
        Some(resultado.unwrap_or_else(|(codigo, mensagem)| Resposta::erro(codigo, mensagem)))
    }

    fn definir_aceite(
        &mut self,
        identidade: &IdentidadeCliente,
        regra: &str,
        aceito: bool,
    ) -> Result<Resposta, Erro> {
        // A lista DRS usa o mesmo filtro exposto para a sessão de console.
        let experimental = self.drs.contains(regra)
            || self
                .metadados
                .get(regra)
                .is_some_and(|r| r.status == Status::Experimental);
        if !experimental {
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

    fn consultar(
        &mut self,
        identidade: &IdentidadeCliente,
        fatos: &nvt_ipc::FatosUsuario,
        ultima: Option<&EstadoTarefa>,
        restaurada: u64,
        agora: Instant,
    ) -> Result<Resposta, String> {
        if restaurada > self.geracao {
            return Err("geração de restauração desconhecida".into());
        }
        let geracao = self.geracao();
        let aceites = self.dados.por_sid.get(&identidade.sid).map(|d| &d.aceites);
        let vazio = BTreeSet::new();
        let s = self
            .sessoes
            .entry(identidade.clone())
            .or_insert_with(|| Sessao {
                revisao: self.serial,
                ..Sessao::default()
            });
        // Uma releitura pode já refletir a escrita que o agente ainda vai relatar.
        if ultima.is_some_and(|ultima| {
            ultima.progresso == ProgressoTarefa::RelatoPendente
                && s.em_voo.as_ref().is_some_and(|t| t.tarefa == ultima.tarefa)
        }) {
            s.contato = Some(agora);
            s.diagnosticos.clone_from(&fatos.diagnosticos);
            return Ok(Resposta::PlanoAgente(PlanoAgente {
                revisao: s.revisao,
                geracao_restauracao: geracao,
                tarefa: None,
                processos_vigiados: self.processos.clone(),
            }));
        }
        if restaurada > s.restaurada
            && s.snapshot_ofertado == Some(restaurada)
            && s.snapshot_confirmado != Some(restaurada)
        {
            return Err("restauração do snapshot ainda não confirmada".into());
        }
        let mut fatos_convertidos = fatos_core(fatos, self.windows_build);
        if s.fatos.as_ref() != Some(&fatos_convertidos) || restaurada != s.restaurada {
            s.invalidar();
        }
        if restaurada > s.restaurada {
            s.gestao.clear();
            s.travas.clear();
            s.reversoes.clear();
        }
        if reidratar(s, &fatos.gestao) {
            s.invalidar();
        }
        s.restaurada = restaurada;
        s.contato = Some(agora);
        s.diagnosticos.clone_from(&fatos.diagnosticos);
        s.fatos = Some(fatos_convertidos.clone());
        preencher_gestao(&mut fatos_convertidos, s);
        let plano = usuario::avaliar(
            &fatos_convertidos,
            Qualidade::default(),
            &self.regras,
            aceites.unwrap_or(&vazio),
        );

        let desejada = if SETTINGS
            .iter()
            .all(|&setting| fatos_convertidos.valor_atual(setting).is_none())
        {
            None
        } else if restaurada < self.geracao {
            s.snapshot_ofertado = Some(geracao);
            s.snapshot_confirmado = None;
            Some((
                OperacaoUsuario::Restaurar,
                vec![AcaoUsuario::RestaurarSnapshot],
            ))
        } else {
            planejar(s, &fatos_convertidos, &plano, self.pausado)
        };
        let tarefa = match desejada {
            Some((operacao, acoes)) => {
                if !s
                    .em_voo
                    .as_ref()
                    .is_some_and(|t| t.tarefa.operacao == operacao && t.acoes == acoes)
                {
                    s.invalidar();
                    self.serial += 1;
                    s.em_voo = Some(TarefaUsuario {
                        tarefa: ReferenciaTarefa {
                            id_tarefa: self.serial,
                            operacao,
                            revisao: s.revisao,
                        },
                        acoes,
                    });
                }
                s.em_voo.clone()
            }
            None => {
                if s.em_voo.is_some() {
                    s.invalidar();
                }
                None
            }
        };
        Ok(Resposta::PlanoAgente(PlanoAgente {
            revisao: s.revisao,
            geracao_restauracao: geracao,
            tarefa,
            processos_vigiados: self.processos.clone(),
        }))
    }

    fn reverter(
        &mut self,
        identidade: &IdentidadeCliente,
        destino: &SettingUsuario,
    ) -> Result<Resposta, String> {
        let s = self
            .sessoes
            .get_mut(identidade)
            .filter(|s| s.gestao.contains_key(destino))
            .ok_or("ajuste sem valor anterior conhecido nesta sessão")?;
        if s.reversoes.insert(destino.clone()) {
            s.travas.insert(destino.clone());
            s.invalidar();
        }
        Ok(Resposta::Ok)
    }

    fn relatar(
        &mut self,
        identidade: &IdentidadeCliente,
        referencia: ReferenciaTarefa,
        resultados: &[ResultadoUsuario],
        agora: Instant,
        unix: u64,
    ) -> Result<Resposta, Erro> {
        let desconhecida = || invalida("tarefa desconhecida ou revisão obsoleta");
        if let Some(anterior) = self
            .dados
            .por_sid
            .get(&identidade.sid)
            .and_then(|d| d.recibos.get(&identidade.sessao))
            .and_then(|recibos| recibos.iter().find(|r| r.tarefa == referencia))
        {
            if anterior.resultados != resultados {
                return Err(invalida("relato repetido com resultados diferentes"));
            }
            if let Some(s) = self.sessoes.get_mut(identidade) {
                s.contato = Some(agora);
            }
            return Ok(Resposta::Ok);
        }
        let s = self.sessoes.get_mut(identidade).ok_or_else(desconhecida)?;
        let tarefa = s
            .em_voo
            .as_ref()
            .filter(|t| t.tarefa == referencia && s.revisao == referencia.revisao)
            .ok_or_else(desconhecida)?;
        validar_resultados(s, tarefa, resultados).map_err(invalida)?;
        let snapshot_concluido = tarefa.acoes == [AcaoUsuario::RestaurarSnapshot]
            && resultados
                .iter()
                .all(|r| matches!(r.status, StatusMudanca::Aplicada | StatusMudanca::JaNoValor));

        let mut dados = self.dados.clone();
        let sid = dados.por_sid.entry(identidade.sid.clone()).or_default();
        let recibos = sid.recibos.entry(identidade.sessao).or_default();
        recibos.push_front(Recibo {
            tarefa: referencia,
            resultados: resultados.to_vec(),
        });
        recibos.truncate(RECIBOS_POR_SESSAO);
        for resultado in resultados {
            self.serial += 1;
            sid.historico
                .push_front(entrada(self.serial, unix, referencia.operacao, resultado));
        }
        sid.historico.truncate(HISTORICO_POR_SID);
        self.armazenamento
            .salvar(&dados)
            .map_err(erro_armazenamento)?;
        self.dados = dados;
        if snapshot_concluido {
            s.snapshot_confirmado = s.snapshot_ofertado;
        }

        for resultado in resultados {
            if referencia.operacao == OperacaoUsuario::Aplicar
                && resultado.status == StatusMudanca::Aplicada
                && let Some(ajuste) = &resultado.mudanca
            {
                let (antes, depois) = valores(ajuste);
                let gestao = s
                    .gestao
                    .entry(resultado.destino.clone())
                    .or_insert_with(|| GestaoUsuario {
                        antes,
                        esperado: depois.clone(),
                        alvo_tela: alvo(&resultado.destino).map(str::to_owned),
                    });
                gestao.esperado = depois;
            }
            if resultado.status == StatusMudanca::Travada {
                s.travas.insert(resultado.destino.clone());
            }
            if referencia.operacao != OperacaoUsuario::Aplicar
                && matches!(
                    resultado.status,
                    StatusMudanca::Aplicada | StatusMudanca::JaNoValor | StatusMudanca::Travada
                )
            {
                s.gestao.remove(&resultado.destino);
                s.reversoes.remove(&resultado.destino);
            }
        }
        // Uma nova consulta deve trazer a releitura antes de outra aplicação.
        s.fatos = None;
        s.contato = Some(agora);
        s.em_voo = None;
        Ok(Resposta::Ok)
    }

    pub(crate) fn projetar(
        &self,
        identidade: &IdentidadeCliente,
        resposta: &mut Resposta,
        limite: usize,
        agora: Instant,
    ) {
        let s = self.sessoes.get(identidade);
        let presente = s.is_some_and(|s| s.presente(agora));
        match resposta {
            Resposta::Estado(estado) => {
                estado.agente = EstadoAgente {
                    presente,
                    restauracao_pendente: self.geracao > s.map_or(0, |s| s.restaurada),
                };
                if let Some(s) = s.filter(|_| presente) {
                    estado.diagnosticos.extend(s.diagnosticos.clone());
                }
            }
            Resposta::Plano(plano) => {
                plano.sistema = self.resumo(identidade, s.filter(|_| presente));
            }
            Resposta::Historico { entradas } => {
                if let Some(dados) = self.dados.por_sid.get(&identidade.sid) {
                    entradas.extend(dados.historico.iter().take(limite).cloned());
                    entradas.sort_by_key(|e| std::cmp::Reverse((e.quando_unix, e.id)));
                    entradas.truncate(limite);
                }
            }
            _ => {}
        }
    }

    fn resumo(&self, identidade: &IdentidadeCliente, sessao: Option<&Sessao>) -> ResumoSistema {
        let mut resumo = ResumoSistema::default();
        let aceites = self.dados.por_sid.get(&identidade.sid).map(|d| &d.aceites);
        let vazio = BTreeSet::new();
        let fatos = sessao.and_then(|s| {
            let mut fatos = s.fatos.clone()?;
            preencher_gestao(&mut fatos, s);
            Some(fatos)
        });
        let plano = fatos.as_ref().map(|fatos| {
            usuario::avaliar(
                fatos,
                Qualidade::default(),
                &self.regras,
                aceites.unwrap_or(&vazio),
            )
        });
        for meta in self
            .metadados
            .values()
            .filter(|r| r.status == Status::Experimental)
        {
            let estado = if aceites.is_some_and(|a| a.contains(&meta.id)) {
                EstadoExperimental::Aceita
            } else if plano
                .as_ref()
                .is_some_and(|p| p.resumo.estado_experimental.contains_key(&meta.id))
            {
                EstadoExperimental::Aceitavel
            } else {
                continue;
            };
            resumo.sugestoes.push(SugestaoExperimental {
                regra: meta.id.clone(),
                descricao: meta.descricao.clone(),
                camada: Camada::Usuario,
                impacto_visual: impacto(meta.impacto_visual),
                estado,
            });
        }
        if let (Some(plano), Some(fatos)) = (plano, fatos) {
            resumo
                .mudancas
                .extend(plano.resumo.mudancas.values().map(|m| {
                    let destino =
                        destino(m.setting, m.alvo_tela.as_deref()).expect("destino do avaliador");
                    let meta = &self.metadados[&m.regra];
                    Mudanca {
                        camada: Camada::Usuario,
                        alvo_tela: alvo(&destino).map(str::to_owned),
                        setting: nome(&destino).into(),
                        nome_legivel: m.descricao.clone(),
                        antes: valor_legivel(&m.atual),
                        depois: valor_legivel(&m.valor),
                        motivo: if m.codigo.is_some() {
                            format!("{} (experimental aceita por você)", m.motivo)
                        } else {
                            m.motivo.clone()
                        },
                        regra: m.regra.clone(),
                        impacto_visual: impacto(meta.impacto_visual),
                        experimental: if m.codigo.is_some() {
                            EstadoExperimental::Aceita
                        } else {
                            EstadoExperimental::Estavel
                        },
                        status: if corresponde(&m.atual, &m.valor) {
                            StatusMudanca::JaNoValor
                        } else {
                            StatusMudanca::Pendente
                        },
                    }
                }));
            resumo
                .mudancas
                .extend(
                    plano
                        .resumo
                        .respeitadas
                        .iter()
                        .filter_map(|(&setting, motivo)| {
                            let tela = fatos.tela.as_ref().map(|tela| tela.alvo.as_str());
                            let destino = destino(setting, tela)?;
                            Some(Mudanca {
                                camada: Camada::Usuario,
                                alvo_tela: alvo(&destino).map(str::to_owned),
                                setting: nome(&destino).into(),
                                nome_legivel: nome(&destino).into(),
                                antes: None,
                                depois: None,
                                motivo: match motivo {
                                    usuario::MotivoRespeitadaUsuario::Travada => {
                                        "Ajuste travado pelo usuário"
                                    }
                                    usuario::MotivoRespeitadaUsuario::AlteradaForaDaGestao => {
                                        "Alteração manual preservada"
                                    }
                                }
                                .into(),
                                regra: String::new(),
                                impacto_visual: ImpactoVisual::Nenhum,
                                experimental: EstadoExperimental::Estavel,
                                status: StatusMudanca::Travada,
                            })
                        }),
                );
        }
        resumo
    }
}

fn invalida(mensagem: impl Into<String>) -> Erro {
    (CodigoErro::RequisicaoInvalida, mensagem.into())
}

fn erro_armazenamento(erro: io::Error) -> Erro {
    tracing::error!("falha ao gravar estado de usuário: {erro}");
    (
        CodigoErro::Interno,
        "falha ao gravar estado de usuário".into(),
    )
}

fn impacto(impacto: nvt_core::ImpactoVisual) -> ImpactoVisual {
    match impacto {
        nvt_core::ImpactoVisual::Nenhum => ImpactoVisual::Nenhum,
        nvt_core::ImpactoVisual::Leve => ImpactoVisual::Baixo,
        nvt_core::ImpactoVisual::Forte => ImpactoVisual::Alto,
    }
}

#[cfg(test)]
#[path = "usuario_testes.rs"]
mod testes;

fn fatos_core(fatos: &nvt_ipc::FatosUsuario, windows_build: Option<u32>) -> FatosUsuario {
    FatosUsuario {
        windows_build,
        tela: match &fatos.tela {
            Fato::Conhecido(t) => Some(usuario::TelaUsuario {
                alvo: t.alvo.clone(),
                atual: taxa_core(t.atual),
                maximo_nativo: Some(taxa_core(t.maximo)),
            }),
            _ => None,
        },
        mouse_aceleracao: match fatos.mouse {
            Fato::Conhecido(m) => Some([m.limiar1, m.limiar2, m.aceleracao]),
            _ => None,
        },
        dx_swap_effect_upgrade: valor_fato(&fatos.swap_effect_upgrade, ValorUsuario::Token),
        gamedvr_captura_historica: valor_fato(&fatos.captura_historica, ValorUsuario::Dword),
        ..Default::default()
    }
}

fn valor_fato<T: Clone>(
    fato: &Fato<T>,
    converter: impl FnOnce(T) -> ValorUsuario,
) -> Option<ValorUsuario> {
    match fato {
        Fato::Conhecido(v) => Some(converter(v.clone())),
        Fato::Ausente => Some(ValorUsuario::Ausente),
        Fato::Desconhecido => None,
    }
}

fn taxa_core(t: nvt_ipc::RefreshRacional) -> usuario::RefreshRacional {
    usuario::RefreshRacional {
        numerador: t.numerador,
        denominador: t.denominador,
    }
}

/// O agente persiste a gestão da camada usuário; a sessão em memória do
/// serviço só a reconstrói, sem nunca apagar o que já conhece.
fn reidratar(s: &mut Sessao, gestao: &[nvt_ipc::GestaoAgente]) -> bool {
    let mut mudou = false;
    for g in gestao {
        if let Some(ajuste) = g.valores.as_ref().filter(|a| a.destino() == g.destino) {
            let (antes, esperado) = valores(ajuste);
            let tipo = setting(&g.destino);
            if tipo.aceita(&antes) && tipo.aceita(&esperado) {
                let nova = GestaoUsuario {
                    antes,
                    esperado,
                    alvo_tela: alvo(&g.destino).map(str::to_owned),
                };
                if s.gestao.get(&g.destino) != Some(&nova) {
                    s.gestao.insert(g.destino.clone(), nova);
                    mudou = true;
                }
            }
        }
        if g.travado {
            mudou |= s.travas.insert(g.destino.clone());
        }
    }
    mudou
}

fn preencher_gestao(fatos: &mut FatosUsuario, s: &Sessao) {
    let tela = fatos.tela.as_ref().map(|t| t.alvo.as_str());
    for (destino, gestao) in &s.gestao {
        if alvo(destino).is_none() || alvo(destino) == tela {
            fatos.gestao.insert(setting(destino), gestao.clone());
        }
    }
    fatos.travadas.extend(
        s.travas
            .iter()
            .filter(|d| alvo(d).is_none() || alvo(d) == tela)
            .map(setting),
    );
}

fn alvo(destino: &SettingUsuario) -> Option<&str> {
    match destino {
        SettingUsuario::Refresh { alvo } => Some(alvo),
        _ => None,
    }
}

fn setting(destino: &SettingUsuario) -> Setting {
    match destino {
        SettingUsuario::Refresh { .. } => Setting::TelaRefresh,
        SettingUsuario::MouseAceleracao => Setting::MouseAceleracao,
        SettingUsuario::DxSwapEffectUpgrade => Setting::DxSwapEffectUpgrade,
        SettingUsuario::GameDvrCapturaHistorica => Setting::GamedvrCapturaHistorica,
    }
}

fn destino(setting: Setting, tela: Option<&str>) -> Option<SettingUsuario> {
    Some(match setting {
        Setting::TelaRefresh => SettingUsuario::Refresh { alvo: tela?.into() },
        Setting::MouseAceleracao => SettingUsuario::MouseAceleracao,
        Setting::DxSwapEffectUpgrade => SettingUsuario::DxSwapEffectUpgrade,
        Setting::GamedvrCapturaHistorica => SettingUsuario::GameDvrCapturaHistorica,
    })
}

fn corresponde(a: &ValorUsuario, b: &ValorUsuario) -> bool {
    match (a, b) {
        (ValorUsuario::Refresh(a), ValorUsuario::Refresh(b)) => {
            a.comparar(*b) == Some(std::cmp::Ordering::Equal)
        }
        _ => a == b,
    }
}

fn planejar(
    s: &Sessao,
    fatos: &FatosUsuario,
    plano: &PlanoUsuario,
    pausado: bool,
) -> Option<(OperacaoUsuario, Vec<AcaoUsuario>)> {
    let tela = fatos.tela.as_ref().map(|t| t.alvo.as_str());
    let legivel = |d: &SettingUsuario| {
        (alvo(d).is_none() || alvo(d) == tela) && fatos.valor_atual(setting(d)).is_some()
    };
    let reverter: Vec<_> = s
        .reversoes
        .iter()
        .filter(|d| legivel(d))
        .map(|d| AcaoUsuario::Reverter { destino: d.clone() })
        .collect();
    if !reverter.is_empty() {
        return Some((OperacaoUsuario::Reverter, reverter));
    }
    if pausado {
        return None;
    }
    let restaurar: Vec<_> = s
        .gestao
        .iter()
        .filter(|(d, g)| {
            legivel(d)
                && !plano.resumo.mudancas.contains_key(&setting(d))
                && fatos
                    .valor_atual(setting(d))
                    .is_some_and(|v| corresponde(&v, &g.esperado))
        })
        .map(|(d, _)| AcaoUsuario::Restaurar { destino: d.clone() })
        .collect();
    if !restaurar.is_empty() {
        return Some((OperacaoUsuario::Restaurar, restaurar));
    }
    let aplicar: Vec<_> = plano
        .resumo
        .mudancas
        .values()
        .filter(|m| !corresponde(&m.atual, &m.valor))
        .filter_map(|m| {
            ajuste(m.alvo_tela.as_deref(), &m.atual, &m.valor)
                .map(|ajuste| AcaoUsuario::Aplicar { ajuste })
        })
        .collect();
    (!aplicar.is_empty()).then_some((OperacaoUsuario::Aplicar, aplicar))
}

fn ajuste(
    alvo: Option<&str>,
    antes: &ValorUsuario,
    depois: &ValorUsuario,
) -> Option<AjusteUsuario> {
    let taxa = |t: &usuario::RefreshRacional| nvt_ipc::RefreshRacional {
        numerador: t.numerador,
        denominador: t.denominador,
    };
    let spi = |&[limiar1, limiar2, aceleracao]: &[i32; 3]| nvt_ipc::MouseSpi {
        limiar1,
        limiar2,
        aceleracao,
    };
    let token = |v: &ValorUsuario| match v {
        ValorUsuario::Token(s) => Some(Some(s.clone())),
        ValorUsuario::Ausente => Some(None),
        _ => None,
    };
    let dword = |v: &ValorUsuario| match v {
        ValorUsuario::Dword(v) => Some(Some(*v)),
        ValorUsuario::Ausente => Some(None),
        _ => None,
    };
    Some(match (antes, depois) {
        (ValorUsuario::Refresh(a), ValorUsuario::Refresh(b)) => AjusteUsuario::Refresh {
            alvo: alvo?.into(),
            antes: taxa(a),
            depois: taxa(b),
        },
        (ValorUsuario::Spi(a), ValorUsuario::Spi(b)) => AjusteUsuario::MouseAceleracao {
            antes: spi(a),
            depois: spi(b),
        },
        _ if matches!(antes, ValorUsuario::Token(_))
            || matches!(depois, ValorUsuario::Token(_)) =>
        {
            AjusteUsuario::DxSwapEffectUpgrade {
                antes: token(antes)?,
                depois: token(depois)?,
            }
        }
        _ => AjusteUsuario::GameDvrCapturaHistorica {
            antes: dword(antes)?,
            depois: dword(depois)?,
        },
    })
}

fn valores(ajuste: &AjusteUsuario) -> (ValorUsuario, ValorUsuario) {
    let spi = |m: &nvt_ipc::MouseSpi| ValorUsuario::Spi([m.limiar1, m.limiar2, m.aceleracao]);
    let token = |v: &Option<String>| v.clone().map_or(ValorUsuario::Ausente, ValorUsuario::Token);
    let dword = |v: &Option<u32>| v.map_or(ValorUsuario::Ausente, ValorUsuario::Dword);
    match ajuste {
        AjusteUsuario::Refresh { antes, depois, .. } => (
            ValorUsuario::Refresh(taxa_core(*antes)),
            ValorUsuario::Refresh(taxa_core(*depois)),
        ),
        AjusteUsuario::MouseAceleracao { antes, depois } => (spi(antes), spi(depois)),
        AjusteUsuario::DxSwapEffectUpgrade { antes, depois } => (token(antes), token(depois)),
        AjusteUsuario::GameDvrCapturaHistorica { antes, depois } => (dword(antes), dword(depois)),
    }
}

fn validar_resultados(
    s: &Sessao,
    tarefa: &TarefaUsuario,
    resultados: &[ResultadoUsuario],
) -> Result<(), &'static str> {
    let snapshot = tarefa.acoes == [AcaoUsuario::RestaurarSnapshot];
    if !snapshot && resultados.len() != tarefa.acoes.len() {
        return Err("relato não cobre todos os destinos da tarefa");
    }
    for r in resultados {
        let acao = tarefa
            .acoes
            .iter()
            .find(|a| match a {
                AcaoUsuario::Aplicar { ajuste } => ajuste.destino() == r.destino,
                AcaoUsuario::Restaurar { destino } | AcaoUsuario::Reverter { destino } => {
                    *destino == r.destino
                }
                AcaoUsuario::RestaurarSnapshot => true,
            })
            .ok_or("destino não faz parte da tarefa")?;
        if (r.status == StatusMudanca::Pendente && r.mudanca.is_some())
            || (r.status == StatusMudanca::Aplicada && r.mudanca.is_none())
        {
            return Err("resultado sem confirmação da mudança");
        }
        if let Some(mudanca) = &r.mudanca {
            let (antes, depois) = valores(mudanca);
            let tipo = setting(&r.destino);
            if !tipo.aceita(&antes) || !tipo.aceita(&depois) {
                return Err("valor inválido para o destino");
            }
            if matches!(r.status, StatusMudanca::Aplicada | StatusMudanca::JaNoValor) {
                match acao {
                    AcaoUsuario::Aplicar { ajuste } => {
                        let (previsto, alvo) = valores(ajuste);
                        if !corresponde(&depois, &alvo)
                            || (!corresponde(&antes, &previsto)
                                && !(r.status == StatusMudanca::JaNoValor
                                    && corresponde(&antes, &alvo)))
                        {
                            return Err("mudança não corresponde ao plano");
                        }
                    }
                    AcaoUsuario::Restaurar { .. } | AcaoUsuario::Reverter { .. } => {
                        let g = &s.gestao[&r.destino];
                        if !corresponde(&depois, &g.antes)
                            || (!corresponde(&antes, &g.esperado)
                                && !(r.status == StatusMudanca::JaNoValor
                                    && corresponde(&antes, &g.antes)))
                        {
                            return Err("restauração não corresponde ao valor anterior");
                        }
                    }
                    AcaoUsuario::RestaurarSnapshot => {}
                }
            }
        }
    }
    Ok(())
}

fn nome(destino: &SettingUsuario) -> &'static str {
    match destino {
        SettingUsuario::Refresh { .. } => "tela_refresh",
        SettingUsuario::MouseAceleracao => "mouse_aceleracao",
        SettingUsuario::DxSwapEffectUpgrade => "dx_swap_effect_upgrade",
        SettingUsuario::GameDvrCapturaHistorica => "gamedvr_captura_historica",
    }
}

fn valor_legivel(valor: &ValorUsuario) -> Option<String> {
    match valor {
        ValorUsuario::Ausente => None,
        ValorUsuario::Dword(v) => Some(v.to_string()),
        ValorUsuario::Token(v) => Some(v.clone()),
        ValorUsuario::Spi([a, b, c]) => Some(format!("{a}, {b}, {c}")),
        ValorUsuario::Refresh(t) => Some(format!("{}/{} Hz", t.numerador, t.denominador)),
    }
}

fn entrada(
    id: u64,
    unix: u64,
    operacao: OperacaoUsuario,
    resultado: &ResultadoUsuario,
) -> EntradaHistorico {
    let (antes, depois) = resultado.mudanca.as_ref().map_or((None, None), |m| {
        let (a, d) = valores(m);
        (valor_legivel(&a), valor_legivel(&d))
    });
    let descricao = resultado.detalhe.clone().unwrap_or_else(|| {
        match resultado.status {
            StatusMudanca::Aplicada => "Mudança confirmada pelo agente",
            StatusMudanca::JaNoValor => "Já no valor solicitado",
            StatusMudanca::Travada => "Ajuste travado",
            StatusMudanca::Recusada => "Mudança recusada",
            StatusMudanca::Falhou => "Falha na mudança",
            StatusMudanca::Pendente => "Mudança pendente",
        }
        .into()
    });
    EntradaHistorico {
        camada: Camada::Usuario,
        id,
        quando_unix: unix,
        acao: match operacao {
            OperacaoUsuario::Aplicar => AcaoHistorico::Aplicacao,
            OperacaoUsuario::Restaurar => AcaoHistorico::Restauracao,
            OperacaoUsuario::Reverter => AcaoHistorico::Rollback,
        },
        perfil: alvo(&resultado.destino).map(str::to_owned),
        setting: Some(nome(&resultado.destino).into()),
        antes,
        depois,
        descricao,
    }
}
