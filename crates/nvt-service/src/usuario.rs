//! Coordenação da camada interativa. A identidade vem exclusivamente do pipe.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nvt_core::config_jogo::{
    CatalogoConfigJogo, DestinoJogo, EstadoDestinoJogo, FatosConfigJogo, GestaoJogo, PlanoJogo,
    SettingJogo, ValorJogo, avaliar_config_jogo,
};
use nvt_core::usuario::{
    self, ConjuntoRegrasUsuario, FatosUsuario, GestaoUsuario, PlanoUsuario,
    SettingUsuario as Setting, ValorUsuario,
};
use nvt_core::{BaseJogos, ConjuntoRegras, IdJogo, Preferences, Qualidade, Status};
use nvt_ipc::pipe::IdentidadeCliente;
use nvt_ipc::{
    AcaoHistorico, AcaoUsuario, AjusteUsuario, Camada, CodigoErro, EntradaHistorico, EstadoAgente,
    EstadoExperimental, EstadoTarefa, Fato, ImpactoVisual, Mudanca, OperacaoUsuario, PlanoAgente,
    ProgressoTarefa, ReferenciaTarefa, Requisicao, Resposta, ResultadoUsuario, ResumoSistema,
    SettingUsuario, StatusMudanca, SugestaoExperimental, TarefaUsuario,
};
use nvt_ipc::{
    AcaoJogo, AlvoIpc, ArquivoJogoUi, ConfigJogoUi, DestinoJogoIpc, FatoConfigJogo,
    MotivoNaoAplicada, MudancaArquivoJogo, OrientacaoConfigJogo, ResultadoJogo, TarefaJogo,
    ValorJogoIpc,
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
    /// Última atualização do nv-tuner já vista por este SID (U-47, U-52).
    atualizacao_vista: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
struct Recibo {
    tarefa: ReferenciaTarefa,
    resultados: Vec<ResultadoUsuario>,
    /// Resultados de arquivo de jogo (v7); recibos antigos não têm o campo.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    resultados_jogo: Vec<ResultadoJogo>,
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
    /// Família jogo (Fase 3): fatos brutos validados, destinos convertidos,
    /// gestão reidratada do agente, travas, reversões pedidas e tarefa.
    fatos_jogo: Vec<FatoConfigJogo>,
    destinos_jogo: BTreeMap<DestinoJogo, EstadoDestinoJogo>,
    refresh_hz: Option<u32>,
    gestao_jogo: BTreeMap<DestinoJogo, GestaoJogo>,
    travas_jogo: BTreeSet<DestinoJogo>,
    reversoes_jogo: BTreeSet<DestinoJogo>,
    /// Último relato sem sucesso por destino (falha ou adiamento).
    falhas_jogo: BTreeMap<DestinoJogo, FalhaJogo>,
    em_voo_jogo: Option<TarefaJogo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FalhaJogo {
    status: StatusMudanca,
    motivo: Option<MotivoNaoAplicada>,
    /// Hash do arquivo quando a ação foi planejada: com o mesmo arquivo, a
    /// falha não se repete (nunca regrava em loop).
    hash: String,
}

impl FalhaJogo {
    fn adiamento(&self) -> bool {
        self.status == StatusMudanca::Pendente
            || matches!(
                self.motivo,
                Some(MotivoNaoAplicada::JanelaFechadaExigida { .. })
            )
    }
}

impl Sessao {
    fn invalidar(&mut self) {
        self.revisao += 1;
        self.em_voo = None;
        self.em_voo_jogo = None;
    }

    fn fatos_jogo_core(&self) -> FatosConfigJogo {
        FatosConfigJogo {
            destinos: self.destinos_jogo.clone(),
            gestao: self.gestao_jogo.clone(),
            travadas: self.travas_jogo.clone(),
        }
    }

    fn arquivo_jogo(&self, destino: &DestinoJogo) -> Option<&FatoConfigJogo> {
        self.fatos_jogo.iter().find(|f| {
            let AlvoIpc::ConfigJogo { jogo, arquivo } = &f.alvo;
            jogo == destino.instancia.jogo.as_str()
                && *arquivo == destino.instancia.arquivo
                && destino.instancia.conta.as_deref() == Some(f.instancia.as_str())
        })
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
    /// Catálogo `[[arquivo]]`/`[[regra]]` de config de jogo (Fase 3).
    catalogo_jogo: CatalogoConfigJogo,
    /// Preferências do usuário (só `limite_fps` interessa à família jogo).
    prefs: Preferences,
    geracao: u64,
    pausado: bool,
    serial: u64,
    windows_build: Option<u32>,
    processos: BTreeSet<String>,
    /// Projeção atual da atualização automática (vem da thread do atualizador).
    atualizacao: nvt_ipc::EstadoAtualizacao,
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
        let catalogo_jogo = BaseJogos::embutida()?.config_jogo().clone();
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
    // Ligado ao laço na integração (L3-a).
    #[cfg_attr(not(test), allow(dead_code))]
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
    #[cfg_attr(not(test), allow(dead_code))]
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

    /// Projeção “Arquivo do jogo” por id da base curada, só da própria
    /// identidade (SID + sessão) e com agente presente.
    pub(crate) fn config_jogo_ui(
        &self,
        identidade: &IdentidadeCliente,
        agora: Instant,
    ) -> BTreeMap<String, ConfigJogoUi> {
        let Some(s) = self.sessoes.get(identidade).filter(|s| s.presente(agora)) else {
            return BTreeMap::new();
        };
        let plano = self.plano_jogo(&identidade.sid, s);
        let mut mapa: BTreeMap<String, ConfigJogoUi> = BTreeMap::new();
        for f in &s.fatos_jogo {
            let AlvoIpc::ConfigJogo { jogo, arquivo } = &f.alvo;
            let Some(def) = self
                .catalogo_jogo
                .arquivo(arquivo)
                .filter(|d| d.jogo.as_str() == jogo)
            else {
                continue;
            };
            let mut arquivo_ui = ArquivoJogoUi {
                alvo: f.alvo.clone(),
                rotulo: f.rotulo.clone(),
                conta: Some(f.instancia.clone()),
                backup: f.backup,
                mudancas: Vec::new(),
                reversivel: false,
            };
            let mut orientacoes = Vec::new();
            if f.somente_leitura {
                orientacoes.push(OrientacaoConfigJogo::DesmarcarSomenteLeitura);
            }
            if f.na_nuvem {
                orientacoes.push(OrientacaoConfigJogo::AjustarNoJogo);
            }
            if f.conta_ambigua {
                orientacoes.push(OrientacaoConfigJogo::EscolherConta);
            }
            for chave in &def.chave {
                let Some(d) = DestinoJogo::novo(chave.setting, Some(&f.instancia)) else {
                    continue;
                };
                let falha = s.falhas_jogo.get(&d);
                if let Some(MotivoNaoAplicada::JanelaFechadaExigida { processos }) =
                    falha.and_then(|f| f.motivo.as_ref())
                {
                    orientacoes.extend(orientacoes_janela(processos));
                }
                let gestao = s.gestao_jogo.get(&d);
                arquivo_ui.reversivel |= gestao.is_some();
                let (antes, depois, status) = if let Some(m) = plano.mudancas.get(&d) {
                    let status = if m.atual == m.valor {
                        StatusMudanca::JaNoValor
                    } else {
                        StatusMudanca::Pendente
                    };
                    (m.atual.clone(), m.valor.clone(), status)
                } else if let Some(g) = gestao {
                    let status = if s.travas_jogo.contains(&d) {
                        StatusMudanca::Travada
                    } else {
                        StatusMudanca::Aplicada
                    };
                    (g.antes.clone(), g.esperado.clone(), status)
                } else {
                    continue;
                };
                if arquivo_ui.mudancas.len() < nvt_ipc::CHAVES_ARQUIVO_MAX {
                    arquivo_ui.mudancas.push(MudancaArquivoJogo {
                        setting: chave.setting.codigo().into(),
                        antes: valor_ipc(&antes),
                        depois: valor_ipc(&depois),
                        status: falha.map_or(status, |f| f.status),
                        motivo: falha.and_then(|f| f.motivo.clone()),
                    });
                }
            }
            let ui = mapa.entry(jogo.clone()).or_default();
            for o in orientacoes {
                if !ui.orientacoes.contains(&o) && ui.orientacoes.len() < 16 {
                    ui.orientacoes.push(o);
                }
            }
            if ui.arquivos.len() < nvt_ipc::ARQUIVOS_JOGO_MAX {
                ui.arquivos.push(arquivo_ui);
            }
        }
        mapa
    }

    /// Plano da família jogo de uma sessão com os aceites do SID.
    fn plano_jogo(&self, sid: &str, s: &Sessao) -> PlanoJogo {
        let vazio = BTreeSet::new();
        let aceites = self.dados.por_sid.get(sid).map_or(&vazio, |d| &d.aceites);
        avaliar_config_jogo(
            &s.fatos_jogo_core(),
            &self.prefs,
            s.refresh_hz,
            &self.catalogo_jogo,
            aceites,
        )
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

    fn marcar_atualizacao_vista(
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
        let mut dados = self.dados.clone();
        dados
            .por_sid
            .entry(identidade.sid.clone())
            .or_default()
            .atualizacao_vista = Some(versao.to_owned());
        self.armazenamento
            .salvar(&dados)
            .map_err(erro_armazenamento)?;
        self.dados = dados;
        Ok(Resposta::Ok)
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
                resultados_jogo,
            } => req.validar().map_err(invalida).and_then(|()| {
                self.relatar(
                    identidade,
                    ReferenciaTarefa {
                        id_tarefa: *id_tarefa,
                        operacao: *operacao,
                        revisao: *revisao,
                    },
                    resultados,
                    resultados_jogo,
                    agora,
                    unix,
                )
            }),
            Requisicao::ReverterConfigJogo { destino } => req
                .validar()
                .and_then(|()| self.reverter_jogo(identidade, destino))
                .map_err(invalida),
            Requisicao::DefinirAceiteExperimental { regra, aceito } => {
                self.definir_aceite(identidade, regra, *aceito)
            }
            Requisicao::ReverterAjusteUsuario { destino, .. } => req
                .validar()
                .and_then(|()| self.reverter(identidade, destino))
                .map_err(invalida),
            Requisicao::MarcarAtualizacaoVista { versao } => req
                .validar()
                .map_err(invalida)
                .and_then(|()| self.marcar_atualizacao_vista(identidade, versao)),
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
                .is_some_and(|r| r.status == Status::Experimental)
            || self
                .catalogo_jogo
                .regras()
                .iter()
                .any(|r| r.id == regra && r.status == Status::Experimental);
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
                && (s.em_voo.as_ref().is_some_and(|t| t.tarefa == ultima.tarefa)
                    || s.em_voo_jogo
                        .as_ref()
                        .is_some_and(|t| t.tarefa == ultima.tarefa))
        }) {
            s.contato = Some(agora);
            s.diagnosticos.clone_from(&fatos.diagnosticos);
            return Ok(Resposta::PlanoAgente(PlanoAgente {
                revisao: s.revisao,
                geracao_restauracao: geracao,
                tarefa: None,
                processos_vigiados: self.processos.clone(),
                tarefa_jogo: None,
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
            s.gestao_jogo.clear();
            s.travas_jogo.clear();
            s.reversoes_jogo.clear();
            s.falhas_jogo.clear();
        }
        if reidratar(s, &fatos.gestao) {
            s.invalidar();
        }
        let catalogo_jogo = &self.catalogo_jogo;
        let novos_destinos = destinos_jogo(&fatos.config_jogo, catalogo_jogo);
        if s.fatos_jogo != fatos.config_jogo || s.destinos_jogo != novos_destinos {
            s.em_voo_jogo = None;
        }
        if reidratar_jogo(s, &fatos.config_jogo, catalogo_jogo) {
            s.em_voo_jogo = None;
        }
        s.fatos_jogo.clone_from(&fatos.config_jogo);
        s.destinos_jogo = novos_destinos;
        s.refresh_hz = fatos_convertidos.tela.as_ref().and_then(|t| {
            (t.atual.denominador > 0).then(|| t.atual.numerador / t.atual.denominador)
        });
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

        let plano_jogo = avaliar_config_jogo(
            &s.fatos_jogo_core(),
            &self.prefs,
            s.refresh_hz,
            catalogo_jogo,
            aceites.unwrap_or(&vazio),
        );
        // Restauração por geração cobre a config de jogo (inclusive pausado).
        let desejada_jogo = if restaurada < self.geracao {
            restaurar_jogo(s)
        } else {
            planejar_jogo(s, &plano_jogo, self.pausado)
        };
        let tarefa_jogo = match desejada_jogo {
            Some((operacao, acoes)) => {
                if !s
                    .em_voo_jogo
                    .as_ref()
                    .is_some_and(|t| t.tarefa.operacao == operacao && t.acoes == acoes)
                {
                    self.serial += 1;
                    s.em_voo_jogo = Some(TarefaJogo {
                        tarefa: ReferenciaTarefa {
                            id_tarefa: self.serial,
                            operacao,
                            revisao: s.revisao,
                        },
                        acoes,
                    });
                }
                s.em_voo_jogo.clone()
            }
            None => {
                s.em_voo_jogo = None;
                None
            }
        };
        Ok(Resposta::PlanoAgente(PlanoAgente {
            revisao: s.revisao,
            geracao_restauracao: geracao,
            tarefa,
            processos_vigiados: self.processos.clone(),
            tarefa_jogo,
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

    fn reverter_jogo(
        &mut self,
        identidade: &IdentidadeCliente,
        destino: &DestinoJogoIpc,
    ) -> Result<Resposta, String> {
        let destino =
            destino_core(destino, &self.catalogo_jogo).ok_or("destino de jogo fora do catálogo")?;
        let s = self
            .sessoes
            .get_mut(identidade)
            .filter(|s| s.gestao_jogo.contains_key(&destino))
            .ok_or("ajuste de jogo sem valor anterior conhecido nesta sessão")?;
        if s.reversoes_jogo.insert(destino.clone()) {
            s.travas_jogo.insert(destino);
            s.invalidar();
        }
        Ok(Resposta::Ok)
    }

    fn relatar(
        &mut self,
        identidade: &IdentidadeCliente,
        referencia: ReferenciaTarefa,
        resultados: &[ResultadoUsuario],
        resultados_jogo: &[ResultadoJogo],
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
            if anterior.resultados != resultados || anterior.resultados_jogo != resultados_jogo {
                return Err(invalida("relato repetido com resultados diferentes"));
            }
            if let Some(s) = self.sessoes.get_mut(identidade) {
                s.contato = Some(agora);
            }
            return Ok(Resposta::Ok);
        }
        let s = self.sessoes.get_mut(identidade).ok_or_else(desconhecida)?;
        if s.em_voo_jogo
            .as_ref()
            .is_some_and(|t| t.tarefa == referencia && s.revisao == referencia.revisao)
        {
            if !resultados.is_empty() {
                return Err(invalida("tarefa de jogo com resultados do Windows"));
            }
            let catalogo = &self.catalogo_jogo;
            let destinos =
                validar_resultados_jogo(s, resultados_jogo, catalogo).map_err(invalida)?;
            let mut dados = self.dados.clone();
            let sid = dados.por_sid.entry(identidade.sid.clone()).or_default();
            let recibos = sid.recibos.entry(identidade.sessao).or_default();
            recibos.push_front(Recibo {
                tarefa: referencia,
                resultados: vec![],
                resultados_jogo: resultados_jogo.to_vec(),
            });
            recibos.truncate(RECIBOS_POR_SESSAO);
            for resultado in resultados_jogo {
                self.serial += 1;
                sid.historico.push_front(entrada_jogo(
                    self.serial,
                    unix,
                    referencia.operacao,
                    resultado,
                ));
            }
            sid.historico.truncate(HISTORICO_POR_SID);
            self.armazenamento
                .salvar(&dados)
                .map_err(erro_armazenamento)?;
            self.dados = dados;
            let tarefa = s.em_voo_jogo.take().expect("tarefa de jogo conferida");
            for (resultado, destino) in resultados_jogo.iter().zip(destinos) {
                registrar_jogo(s, &tarefa, resultado, destino);
            }
            s.contato = Some(agora);
            return Ok(Resposta::Ok);
        }
        if !resultados_jogo.is_empty() {
            return Err(desconhecida());
        }
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
            resultados_jogo: vec![],
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
                estado.atualizacao.nao_vista = self.nao_vista(&identidade.sid, &estado.atualizacao);
                if let Some(s) = s.filter(|_| presente) {
                    estado.diagnosticos.extend(s.diagnosticos.clone());
                }
            }
            Resposta::Plano(plano) => {
                plano.sistema = self.resumo(identidade, s.filter(|_| presente));
                let mut config = self.config_jogo_ui(identidade, agora);
                for jogo in &mut plano.jogos {
                    if let Some(ui) = jogo.competitivo.as_ref().and_then(|id| config.remove(id)) {
                        jogo.config_jogo = Some(ui);
                    }
                }
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
        let plano_jogo = sessao.map(|s| self.plano_jogo(&identidade.sid, s));
        for regra in self
            .catalogo_jogo
            .regras()
            .iter()
            .filter(|r| r.status == Status::Experimental)
        {
            let estado = if aceites.is_some_and(|a| a.contains(&regra.id)) {
                EstadoExperimental::Aceita
            } else if plano_jogo
                .as_ref()
                .is_some_and(|p| p.estado_experimental.contains_key(&regra.id))
            {
                EstadoExperimental::Aceitavel
            } else {
                continue;
            };
            resumo.sugestoes.push(SugestaoExperimental {
                regra: regra.id.clone(),
                descricao: regra.descricao.clone(),
                camada: Camada::Usuario,
                impacto_visual: impacto(regra.impacto_visual),
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

// ── Família jogo (Fase 3) ───────────────────────────────────────────────────

/// Destino do catálogo: setting conhecida, do jogo e do arquivo do alvo, com
/// a instância opaca como conta.
fn destino_de(
    jogo: &str,
    arquivo: &str,
    setting: &str,
    instancia: &str,
    catalogo: &CatalogoConfigJogo,
) -> Option<DestinoJogo> {
    let setting = SettingJogo::de_codigo(setting)?;
    let def = catalogo.arquivo(arquivo)?;
    if setting.jogo() != jogo || setting.arquivo() != arquivo || def.jogo.as_str() != jogo {
        return None;
    }
    catalogo.chave(setting)?;
    DestinoJogo::novo(setting, Some(instancia)).filter(DestinoJogo::coerente)
}

fn destino_core(destino: &DestinoJogoIpc, catalogo: &CatalogoConfigJogo) -> Option<DestinoJogo> {
    let AlvoIpc::ConfigJogo { jogo, arquivo } = &destino.alvo;
    destino_de(
        jogo,
        arquivo,
        &destino.setting,
        &destino.instancia,
        catalogo,
    )
}

fn destino_ipc(destino: &DestinoJogo) -> DestinoJogoIpc {
    DestinoJogoIpc {
        alvo: AlvoIpc::ConfigJogo {
            jogo: destino.instancia.jogo.as_str().into(),
            arquivo: destino.instancia.arquivo.clone(),
        },
        setting: destino.setting.codigo().into(),
        instancia: destino.instancia.conta.clone().unwrap_or_default(),
    }
}

fn valor_core(valor: &ValorJogoIpc) -> ValorJogo {
    match valor {
        ValorJogoIpc::Ausente => ValorJogo::Ausente,
        ValorJogoIpc::Inteiro(n) => ValorJogo::Inteiro(*n),
        ValorJogoIpc::Booleano(b) => ValorJogo::Booleano(*b),
        ValorJogoIpc::Texto(t) => ValorJogo::Texto(t.clone()),
        ValorJogoIpc::TokenLancamento(n) => ValorJogo::TokenLancamento(*n),
    }
}

fn valor_ipc(valor: &ValorJogo) -> ValorJogoIpc {
    match valor {
        ValorJogo::Ausente => ValorJogoIpc::Ausente,
        ValorJogo::Inteiro(n) => ValorJogoIpc::Inteiro(*n),
        ValorJogo::Booleano(b) => ValorJogoIpc::Booleano(*b),
        ValorJogo::Texto(t) => ValorJogoIpc::Texto(t.clone()),
        ValorJogo::TokenLancamento(n) => ValorJogoIpc::TokenLancamento(*n),
    }
}

/// Leituras do agente convertidas; entradas fora do catálogo ou de tipo
/// inválido são ignoradas (leitura desconhecida).
fn destinos_jogo(
    fatos: &[FatoConfigJogo],
    catalogo: &CatalogoConfigJogo,
) -> BTreeMap<DestinoJogo, EstadoDestinoJogo> {
    let mut destinos = BTreeMap::new();
    for f in fatos {
        let AlvoIpc::ConfigJogo { jogo, arquivo } = &f.alvo;
        for v in &f.valores {
            let Some(d) = destino_de(jogo, arquivo, &v.setting, &f.instancia, catalogo) else {
                tracing::warn!("leitura de jogo fora do catálogo ignorada");
                continue;
            };
            let def = catalogo.chave(d.setting).expect("chave conferida");
            let atual = match &v.valor {
                _ if !f.presente => None,
                Fato::Conhecido(valor) => Some(valor_core(valor)),
                Fato::Ausente => Some(ValorJogo::Ausente),
                Fato::Desconhecido => None,
            }
            .filter(|a| def.aceita_leitura(a));
            destinos.insert(
                d,
                EstadoDestinoJogo {
                    atual,
                    somente_leitura: f.somente_leitura,
                    na_nuvem: f.na_nuvem,
                    conta_ambigua: f.conta_ambigua,
                    hash: f.hash.clone(),
                },
            );
        }
    }
    destinos
}

/// Reconstrói a gestão de jogo persistida pelo agente (só o que vem desta
/// identidade, validado contra o catálogo e os tipos), sem apagar nada.
fn reidratar_jogo(s: &mut Sessao, fatos: &[FatoConfigJogo], catalogo: &CatalogoConfigJogo) -> bool {
    let mut mudou = false;
    for f in fatos {
        let AlvoIpc::ConfigJogo { jogo, arquivo } = &f.alvo;
        for g in &f.gestao {
            let Some(d) = destino_de(jogo, arquivo, &g.setting, &f.instancia, catalogo) else {
                tracing::warn!("gestão de jogo fora do catálogo ignorada");
                continue;
            };
            let def = catalogo.chave(d.setting).expect("chave conferida");
            let (antes, esperado) = (valor_core(&g.antes), valor_core(&g.esperado));
            if def.aceita_leitura(&antes) && def.aceita_leitura(&esperado) {
                let backup = s.gestao_jogo.get(&d).and_then(|a| a.backup.clone());
                let nova = GestaoJogo {
                    antes,
                    esperado,
                    hash_antes: Some(g.hash_antes.clone()),
                    hash_depois: Some(g.hash_depois.clone()),
                    backup,
                };
                if s.gestao_jogo.get(&d) != Some(&nova) {
                    s.gestao_jogo.insert(d.clone(), nova);
                    mudou = true;
                }
            }
            if g.travado {
                mudou |= s.travas_jogo.insert(d);
            }
        }
    }
    mudou
}

/// Leitura atual e hash do arquivo, exigidos para qualquer ação.
fn leitura_jogo<'a>(s: &'a Sessao, d: &DestinoJogo) -> Option<(&'a ValorJogo, &'a String)> {
    let e = s.destinos_jogo.get(d)?;
    Some((e.atual.as_ref()?, e.hash.as_ref()?))
}

/// Restauração por geração: tudo o que o app gere volta ao original.
fn restaurar_jogo(s: &Sessao) -> Option<(OperacaoUsuario, Vec<AcaoJogo>)> {
    let acoes: Vec<_> = s
        .gestao_jogo
        .keys()
        .filter_map(|d| {
            let (_, hash) = leitura_jogo(s, d)?;
            Some(AcaoJogo::Restaurar {
                destino: destino_ipc(d),
                hash_esperado: hash.clone(),
            })
        })
        .take(nvt_ipc::ACOES_JOGO_MAX)
        .collect();
    (!acoes.is_empty()).then_some((OperacaoUsuario::Restaurar, acoes))
}

fn planejar_jogo(
    s: &Sessao,
    plano: &PlanoJogo,
    pausado: bool,
) -> Option<(OperacaoUsuario, Vec<AcaoJogo>)> {
    let limitar = |acoes: Vec<AcaoJogo>| {
        acoes
            .into_iter()
            .take(nvt_ipc::ACOES_JOGO_MAX)
            .collect::<Vec<_>>()
    };
    let reverter: Vec<_> = s
        .reversoes_jogo
        .iter()
        .filter(|d| s.gestao_jogo.contains_key(*d))
        .filter_map(|d| {
            let (_, hash) = leitura_jogo(s, d)?;
            Some(AcaoJogo::Reverter {
                destino: destino_ipc(d),
                hash_esperado: hash.clone(),
            })
        })
        .collect();
    if !reverter.is_empty() {
        return Some((OperacaoUsuario::Reverter, limitar(reverter)));
    }
    if pausado {
        return None;
    }
    let restaurar: Vec<_> = s
        .gestao_jogo
        .iter()
        .filter(|(d, _)| !plano.mudancas.contains_key(*d))
        .filter_map(|(d, g)| {
            let (atual, hash) = leitura_jogo(s, d)?;
            (*atual == g.esperado).then(|| AcaoJogo::Restaurar {
                destino: destino_ipc(d),
                hash_esperado: hash.clone(),
            })
        })
        .collect();
    if !restaurar.is_empty() {
        return Some((OperacaoUsuario::Restaurar, limitar(restaurar)));
    }
    let aplicar: Vec<_> = plano
        .mudancas
        .values()
        .filter(|m| m.atual != m.valor)
        .filter_map(|m| {
            let (_, hash) = leitura_jogo(s, &m.destino)?;
            let arquivo = s.arquivo_jogo(&m.destino)?;
            let repetiria = s
                .falhas_jogo
                .get(&m.destino)
                .is_some_and(|f| !f.adiamento() && f.hash == *hash);
            (arquivo.presente
                && arquivo.backup != nvt_ipc::EstadoBackup::Comprometido
                && !repetiria)
                .then(|| AcaoJogo::Aplicar {
                    destino: destino_ipc(&m.destino),
                    valor: valor_ipc(&m.valor),
                    hash_esperado: hash.clone(),
                })
        })
        .collect();
    (!aplicar.is_empty()).then(|| (OperacaoUsuario::Aplicar, limitar(aplicar)))
}

/// Confere o relato com a tarefa de jogo em voo: cobre todas as ações, cada
/// destino no catálogo, tipos aceitos e hash/valores coerentes com o plano.
fn validar_resultados_jogo(
    s: &Sessao,
    resultados: &[ResultadoJogo],
    catalogo: &CatalogoConfigJogo,
) -> Result<Vec<DestinoJogo>, &'static str> {
    let tarefa = s
        .em_voo_jogo
        .as_ref()
        .ok_or("tarefa de jogo desconhecida")?;
    if resultados.len() != tarefa.acoes.len() {
        return Err("relato não cobre todos os destinos da tarefa de jogo");
    }
    let mut destinos = Vec::with_capacity(resultados.len());
    for r in resultados {
        let d = destino_core(&r.destino, catalogo).ok_or("destino de jogo fora do catálogo")?;
        let acao = tarefa
            .acoes
            .iter()
            .find(|a| *a.destino() == r.destino)
            .ok_or("destino não faz parte da tarefa de jogo")?;
        let def = catalogo.chave(d.setting).expect("chave conferida");
        let antes = r.antes.as_ref().map(valor_core);
        let depois = r.depois.as_ref().map(valor_core);
        if [&antes, &depois]
            .into_iter()
            .flatten()
            .any(|v| !def.aceita_leitura(v))
        {
            return Err("valor inválido para o destino de jogo");
        }
        let sucesso = matches!(r.status, StatusMudanca::Aplicada | StatusMudanca::JaNoValor);
        if sucesso && r.motivo.is_some() {
            return Err("resultado de sucesso com motivo de falha");
        }
        if r.status == StatusMudanca::Aplicada
            && (antes.is_none() || depois.is_none() || r.hash_depois.is_none())
        {
            return Err("resultado sem confirmação da mudança de jogo");
        }
        if sucesso {
            let (alvo, hash_esperado) = match acao {
                AcaoJogo::Aplicar {
                    valor,
                    hash_esperado,
                    ..
                } => (valor_core(valor), hash_esperado),
                AcaoJogo::Restaurar { hash_esperado, .. }
                | AcaoJogo::Reverter { hash_esperado, .. } => (
                    s.gestao_jogo
                        .get(&d)
                        .ok_or("restauração de jogo sem valor anterior")?
                        .antes
                        .clone(),
                    hash_esperado,
                ),
            };
            if depois.as_ref().is_some_and(|v| *v != alvo) {
                return Err("mudança de jogo não corresponde ao plano");
            }
            if r.status == StatusMudanca::Aplicada && r.hash_antes.as_ref() != Some(hash_esperado) {
                return Err("hash do arquivo de jogo diferente do esperado");
            }
        }
        destinos.push(d);
    }
    Ok(destinos)
}

fn hash_da_acao(acao: &AcaoJogo) -> &String {
    match acao {
        AcaoJogo::Aplicar { hash_esperado, .. }
        | AcaoJogo::Restaurar { hash_esperado, .. }
        | AcaoJogo::Reverter { hash_esperado, .. } => hash_esperado,
    }
}

fn registrar_jogo(s: &mut Sessao, tarefa: &TarefaJogo, r: &ResultadoJogo, d: DestinoJogo) {
    let operacao = tarefa.tarefa.operacao;
    // A releitura do agente após a escrita vale até a próxima consulta.
    if matches!(r.status, StatusMudanca::Aplicada | StatusMudanca::JaNoValor) {
        if let Some(e) = s.destinos_jogo.get_mut(&d)
            && let Some(depois) = &r.depois
        {
            e.atual = Some(valor_core(depois));
        }
        if let Some(hash) = &r.hash_depois {
            for (outro, e) in &mut s.destinos_jogo {
                if outro.instancia == d.instancia {
                    e.hash = Some(hash.clone());
                }
            }
        }
    }
    match r.status {
        StatusMudanca::Aplicada | StatusMudanca::JaNoValor => {
            s.falhas_jogo.remove(&d);
        }
        StatusMudanca::Travada => {
            s.falhas_jogo.remove(&d);
            s.travas_jogo.insert(d.clone());
        }
        status => {
            if let Some(acao) = tarefa.acoes.iter().find(|a| *a.destino() == r.destino) {
                s.falhas_jogo.insert(
                    d.clone(),
                    FalhaJogo {
                        status,
                        motivo: r.motivo.clone(),
                        hash: hash_da_acao(acao).clone(),
                    },
                );
            }
        }
    }
    if operacao == OperacaoUsuario::Aplicar
        && r.status == StatusMudanca::Aplicada
        && let (Some(antes), Some(depois)) = (&r.antes, &r.depois)
    {
        let gestao = s
            .gestao_jogo
            .entry(d.clone())
            .or_insert_with(|| GestaoJogo {
                antes: valor_core(antes),
                esperado: valor_core(depois),
                hash_antes: r.hash_antes.clone(),
                hash_depois: None,
                backup: None,
            });
        gestao.esperado = valor_core(depois);
        gestao.hash_depois.clone_from(&r.hash_depois);
    }
    if operacao != OperacaoUsuario::Aplicar
        && matches!(
            r.status,
            StatusMudanca::Aplicada | StatusMudanca::JaNoValor | StatusMudanca::Travada
        )
    {
        s.gestao_jogo.remove(&d);
        s.reversoes_jogo.remove(&d);
    }
}

fn orientacoes_janela(processos: &[String]) -> Vec<OrientacaoConfigJogo> {
    let (steam, jogo): (Vec<_>, Vec<_>) = processos
        .iter()
        .cloned()
        .partition(|p| p.eq_ignore_ascii_case("steam.exe"));
    let mut v = Vec::new();
    if !steam.is_empty() {
        v.push(OrientacaoConfigJogo::FecharSteam);
    }
    if !jogo.is_empty() {
        v.push(OrientacaoConfigJogo::FecharJogo { processos: jogo });
    }
    v
}

fn valor_legivel_jogo(valor: &ValorJogoIpc) -> Option<String> {
    match valor {
        ValorJogoIpc::Ausente | ValorJogoIpc::TokenLancamento(None) => None,
        ValorJogoIpc::Inteiro(n) => Some(n.to_string()),
        ValorJogoIpc::Booleano(b) => Some(if *b { "True" } else { "False" }.into()),
        ValorJogoIpc::Texto(t) => Some(t.clone()),
        ValorJogoIpc::TokenLancamento(Some(n)) => {
            Some(format!("{} {n}", nvt_core::config_jogo::TOKEN_FPS_MAX))
        }
    }
}

fn entrada_jogo(
    id: u64,
    unix: u64,
    operacao: OperacaoUsuario,
    r: &ResultadoJogo,
) -> EntradaHistorico {
    let AlvoIpc::ConfigJogo { jogo, .. } = &r.destino.alvo;
    EntradaHistorico {
        camada: Camada::Usuario,
        id,
        quando_unix: unix,
        acao: match operacao {
            OperacaoUsuario::Aplicar => AcaoHistorico::Aplicacao,
            OperacaoUsuario::Restaurar => AcaoHistorico::Restauracao,
            OperacaoUsuario::Reverter => AcaoHistorico::Rollback,
        },
        perfil: Some(jogo.clone()),
        setting: Some(r.destino.setting.clone()),
        antes: r.antes.as_ref().and_then(valor_legivel_jogo),
        depois: r.depois.as_ref().and_then(valor_legivel_jogo),
        descricao: match r.status {
            StatusMudanca::Aplicada => "Arquivo do jogo alterado e confirmado pelo agente",
            StatusMudanca::JaNoValor => "Arquivo do jogo já no valor solicitado",
            StatusMudanca::Travada => "Ajuste do jogo travado",
            StatusMudanca::Recusada => "Mudança no arquivo do jogo recusada",
            StatusMudanca::Falhou => "Falha na mudança do arquivo do jogo",
            StatusMudanca::Pendente => "Mudança no arquivo do jogo adiada",
        }
        .into(),
    }
}
