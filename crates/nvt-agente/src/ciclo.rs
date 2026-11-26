use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    io,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nvt_core::{jogos::BaseJogos, usuario::ValorUsuario};
use nvt_engine::Portao;
use nvt_ipc::{
    AcaoUsuario, EstadoTarefa, Fato, FatosUsuario, OperacaoUsuario, PlanoAgente, ProgressoTarefa,
    ReferenciaTarefa, Requisicao, Resposta, ResultadoUsuario, SettingUsuario, StatusMudanca,
    TarefaUsuario,
};

use crate::config_jogo::final_jogo;
use crate::estado::{Backup, Destino, Intencao, Operacao, Trabalho};
use crate::valores::{atingiu, iguais, mouse, mudanca, valido, valores};
use crate::{EscritaUsuario, Estado, LeituraUsuario, Persistencia, ValorLido};

#[cfg(test)]
#[path = "ciclo_testes.rs"]
mod testes;

/// Cada chamada abre, usa e fecha uma conexão.
pub trait Transporte {
    fn trocar(&mut self, requisicao: &Requisicao) -> io::Result<Resposta>;
}

/// Só leitura: executável do agente e entrada Run gravada pelo instalador.
pub trait Instalacao {
    fn presente(&self) -> bool;
}

const ABANDONO: Duration = Duration::from_secs(120);

#[derive(Debug, PartialEq, Eq)]
pub enum Controle {
    Continuar,
    Encerrar,
    Restaurado,
    Bloqueado(String),
}

pub struct Agente<A, T> {
    pub armazem: A,
    pub transporte: T,
    pub(crate) portao: Portao,
    pub(crate) base: BaseJogos,
    pub(crate) sessao: u32,
    /// Perfil para arquivos de jogo (Fase 3); `None` desliga a família jogo.
    #[cfg(windows)]
    pub(crate) jogo: Option<crate::config_jogo::AmbienteJogo>,
    /// Leituras de saúde e limpeza do `%TEMP%` (L3); `None` desliga.
    #[cfg(windows)]
    pub(crate) saude: Option<crate::saude::AgendaSaude>,
    sessoes_existentes: Option<BTreeSet<u32>>,
    fatos_enviados: Option<FatosUsuario>,
    proxima_consulta: Instant,
    retentar_em: Instant,
    proxima_deriva: Instant,
    falhas: u32,
    indisponivel_desde: Option<Instant>,
    /// Primeira observação da instalação ausente, zerada ao vê-la presente.
    ausente_desde: Cell<Option<Instant>>,
}

/// Envelope de outra versão (serviço novo ou antigo no meio de um upgrade):
/// o serviço existe, só não fala o nosso protocolo agora (U-41).
fn versao_incompativel(erro: &io::Error) -> bool {
    erro.get_ref()
        .and_then(|e| e.downcast_ref::<nvt_ipc::ErroIpc>())
        .is_some_and(|e| matches!(e, nvt_ipc::ErroIpc::VersaoIncompativel { .. }))
}

fn erro(mensagem: &'static str) -> io::Error {
    io::Error::other(mensagem)
}

fn resultado(
    destino: &SettingUsuario,
    status: StatusMudanca,
    detalhe: Option<&str>,
) -> ResultadoUsuario {
    ResultadoUsuario {
        destino: destino.clone(),
        status,
        detalhe: detalhe.map(str::to_owned),
        mudanca: None,
    }
}

fn confirmado(r: &ResultadoUsuario) -> bool {
    matches!(r.status, StatusMudanca::Aplicada | StatusMudanca::JaNoValor)
}

/// Mensagem de `--restaurar` bloqueado por arquivo de jogo; os backups ficam.
fn bloqueio_jogo(r: &nvt_ipc::ResultadoJogo) -> String {
    match &r.motivo {
        Some(nvt_ipc::MotivoNaoAplicada::JanelaFechadaExigida { processos }) => format!(
            "configuração de jogo não restaurada: feche {} e tente de novo (backups preservados)",
            processos.join(", ")
        ),
        _ => format!(
            "configuração de jogo não restaurada ({}): restauração pendente (backups preservados)",
            r.destino.setting
        ),
    }
}

impl<A: LeituraUsuario + EscritaUsuario, T: Transporte> Agente<A, T> {
    pub fn novo(armazem: A, transporte: T, portao: Portao, sessao: u32) -> io::Result<Self> {
        let agora = Instant::now();
        Ok(Self {
            armazem,
            transporte,
            portao,
            base: BaseJogos::embutida().map_err(io::Error::other)?,
            sessao,
            sessoes_existentes: None,
            fatos_enviados: None,
            proxima_consulta: agora,
            retentar_em: agora,
            proxima_deriva: agora,
            falhas: 0,
            indisponivel_desde: None,
            ausente_desde: Cell::new(None),
            #[cfg(windows)]
            jogo: None,
            #[cfg(windows)]
            saude: None,
        })
    }

    /// Liga a configuração de arquivo de jogo sobre o perfil dado.
    #[cfg(windows)]
    pub fn definir_ambiente_jogo(&mut self, ambiente: crate::config_jogo::AmbienteJogo) {
        self.jogo = Some(ambiente);
    }

    /// Liga as leituras de saúde da sessão com a fonte dada.
    #[cfg(windows)]
    pub fn definir_saude(&mut self, fonte: Box<dyn crate::saude::FonteSaude>) {
        self.saude = Some(crate::saude::AgendaSaude::nova(
            fonte,
            nvt_manutencao::MODO_LIMPEZA,
        ));
    }

    /// Uma lista confiável permite recuperar o snapshot do mesmo monitor após
    /// logoff, quando o identificador da sessão anterior já deixou de existir.
    pub fn definir_sessoes_existentes(&mut self, sessoes: Option<BTreeSet<u32>>) {
        self.sessoes_existentes = sessoes.filter(|s| s.contains(&self.sessao));
    }

    /// Depois de `ABANDONO` sem serviço, a ausência do executável ou da
    /// entrada Run, contínua também por `ABANDONO`, indica desinstalação: o
    /// agente sai e libera o arquivo. Um upgrade do MSI remove e regrava os
    /// dois; uma ausência momentânea não basta (U-41).
    pub fn desinstalado(&self, agora: Instant, instalacao: &impl Instalacao) -> bool {
        if instalacao.presente() {
            self.ausente_desde.set(None);
            return false;
        }
        let ausente = self.ausente_desde.get().map_or(agora, |d| d.min(agora));
        self.ausente_desde.set(Some(ausente));
        let passou = |desde: Instant| agora.saturating_duration_since(desde) >= ABANDONO;
        passou(ausente) && self.indisponivel_desde.is_some_and(passou)
    }

    pub fn passo(
        &mut self,
        persistencia: &impl Persistencia,
        agora: Instant,
        restaurar: bool,
    ) -> io::Result<Controle> {
        let resposta = self.ciclo(persistencia, agora, restaurar);
        if resposta.is_err() {
            self.falhas = (self.falhas + 1).min(5);
            let jitter = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_millis();
            self.retentar_em = agora
                + Duration::from_secs(1_u64 << self.falhas)
                + Duration::from_millis(u64::from(jitter));
        } else if agora >= self.retentar_em {
            self.falhas = 0;
        }
        resposta
    }

    fn ciclo(
        &mut self,
        p: &impl Persistencia,
        agora: Instant,
        restaurar: bool,
    ) -> io::Result<Controle> {
        let mut estado = p.carregar()?;
        estado.validar()?;
        estado.sessoes.entry(self.sessao).or_default();
        let mut exec = Execucao {
            agente: self,
            p,
            estado,
            vigiados: None,
            cadeia: BTreeMap::new(),
        };
        let tela = exec.agente.armazem.tela();
        exec.recuperar_escopo_tela(&tela)?;
        exec.recuperar()?;
        exec.recuperar_jogo()?;
        if restaurar
            && !exec.estado.destinos.iter().any(|d| exec.acessivel(d))
            && !exec.estado.jogo.gerido()
        {
            return Ok(Controle::Restaurado);
        }
        if agora >= exec.agente.proxima_deriva {
            exec.deriva()?;
            exec.deriva_jogo()?;
            exec.agente.proxima_deriva = agora + Duration::from_secs(600);
        }

        let fatos = exec.fatos(tela);
        let relato_pendente = exec
            .sessao()
            .trabalho
            .as_ref()
            .is_some_and(|t| t.pronto && !t.confirmado);
        if !restaurar
            && (agora < exec.agente.retentar_em
                || (!relato_pendente
                    && agora < exec.agente.proxima_consulta
                    && exec.agente.fatos_enviados.as_ref() == Some(&fatos)))
        {
            return Ok(Controle::Continuar);
        }

        let sessao = exec.sessao();
        let ultima_tarefa = sessao.trabalho.as_ref().map(|trabalho| EstadoTarefa {
            tarefa: trabalho.tarefa,
            progresso: if trabalho.confirmado {
                ProgressoTarefa::Confirmada
            } else if trabalho.pronto {
                ProgressoTarefa::RelatoPendente
            } else {
                ProgressoTarefa::EmExecucao
            },
        });
        let geracao_restaurada = sessao.geracao;
        let resposta = exec.agente.transporte.trocar(&Requisicao::AgenteConsultar {
            fatos: fatos.clone(),
            ultima_tarefa,
            geracao_restaurada,
        });
        let incompativel = match &resposta {
            Ok(Resposta::Erro { codigo, .. }) => *codigo == nvt_ipc::CodigoErro::VersaoIncompativel,
            Ok(_) => false,
            Err(erro) => versao_incompativel(erro),
        };
        // Serviço presente em outra versão não conta para desinstalação.
        exec.agente.indisponivel_desde = match resposta {
            Err(_) if !incompativel => Some(exec.agente.indisponivel_desde.unwrap_or(agora)),
            _ => None,
        };
        if incompativel {
            // Indisponível temporário durante o upgrade: nada é gravado nem
            // aplicado; a consulta seguinte, com recuo, tenta de novo.
            return Err(erro("serviço com versão de protocolo diferente"));
        }
        let resposta = resposta.map_err(|erro| {
            io::Error::other(format!("lista atual do serviço indisponível: {erro}"))
        })?;
        let plano = match resposta {
            Resposta::Encerrar if restaurar => {
                return Ok(Controle::Bloqueado("serviço em encerramento".into()));
            }
            Resposta::Encerrar => return Ok(Controle::Encerrar),
            Resposta::PlanoAgente(plano) => plano,
            _ => return Err(erro("consulta sem plano atual do serviço")),
        };
        exec.validar_plano(&plano)?;
        exec.vigiados = Some(plano.processos_vigiados);
        exec.estado.geracao_vista = plano.geracao_restauracao;
        exec.sessao_mut().revisao = plano.revisao;
        exec.salvar()?;
        exec.compensacoes()?;
        exec.saude(agora)?;

        let tarefa = plano.tarefa;
        let nova_geracao = geracao_restaurada < plano.geracao_restauracao
            || exec
                .estado
                .destinos
                .iter()
                .any(|d| exec.acessivel(d) && d.geracao_snapshot < plano.geracao_restauracao)
            || exec.geracao_jogo_pendente(plano.geracao_restauracao);
        if restaurar {
            let resultados = exec.restaurar_snapshot(None, None)?;
            let jogo = exec.restaurar_jogo(None, None)?;
            if let Some(r) = resultados.values().find(|r| !confirmado(r)) {
                return Ok(Controle::Bloqueado(format!(
                    "{:?}: {}",
                    r.destino,
                    r.detalhe.as_deref().unwrap_or("restauração pendente")
                )));
            }
            return Ok(jogo
                .iter()
                .find(|r| !final_jogo(r))
                .map_or(Controle::Restaurado, |r| {
                    Controle::Bloqueado(bloqueio_jogo(r))
                }));
        }

        if exec.enviar_pendente(tarefa.as_ref())? == Controle::Encerrar {
            return Ok(Controle::Encerrar);
        }
        let ref_restauracao = tarefa
            .as_ref()
            .filter(|t| t.acoes.contains(&AcaoUsuario::RestaurarSnapshot))
            .map(|t| &t.tarefa);
        if nova_geracao {
            if let Some(referencia) = ref_restauracao {
                exec.iniciar(referencia)?;
            }
            let resultados =
                exec.restaurar_snapshot(Some(plano.geracao_restauracao), ref_restauracao)?;
            let jogo = exec.restaurar_jogo(Some(plano.geracao_restauracao), ref_restauracao)?;
            if resultados.values().all(confirmado) && jogo.iter().all(final_jogo) {
                if ref_restauracao.is_some() {
                    let trabalho = exec
                        .sessao_mut()
                        .trabalho
                        .as_mut()
                        .expect("tarefa iniciada");
                    if !trabalho.pronto
                        && trabalho.resultados.iter().all(confirmado)
                        && trabalho.resultados_jogo.iter().all(final_jogo)
                    {
                        trabalho.geracao_concluida = Some(plano.geracao_restauracao);
                    }
                } else {
                    exec.sessao_mut().geracao = plano.geracao_restauracao;
                    exec.salvar()?;
                }
            } else if ref_restauracao.is_none() {
                // Sem tarefa de restauração para relatar. A consulta seguinte tenta novamente.
                exec.agente.proxima_consulta = agora + Duration::from_secs(5);
                exec.agente.fatos_enviados = Some(fatos);
                return Ok(Controle::Continuar);
            }
        }

        // A tarefa de jogo corre junto da tarefa Windows de mesma referência ou,
        // sem tarefa Windows, sozinha; com referências diferentes, espera a seguinte.
        let referencia = tarefa
            .as_ref()
            .map(|t| t.tarefa)
            .or(plano.tarefa_jogo.as_ref().map(|t| t.tarefa));
        let tarefa_jogo = plano.tarefa_jogo.filter(|t| Some(t.tarefa) == referencia);
        if let Some(referencia) = referencia {
            let geracao_ok = exec.sessao().geracao == plano.geracao_restauracao;
            if (geracao_ok && !nova_geracao)
                || tarefa
                    .as_ref()
                    .is_some_and(|t| t.acoes.contains(&AcaoUsuario::RestaurarSnapshot))
            {
                exec.iniciar(&referencia)?;
                if !exec.sessao().trabalho.as_ref().is_some_and(|t| t.pronto) {
                    for acao in tarefa.into_iter().flat_map(|t| t.acoes) {
                        exec.executar(acao, &referencia, plano.geracao_restauracao)?;
                    }
                    for acao in tarefa_jogo.iter().flat_map(|t| &t.acoes) {
                        exec.executar_jogo(acao, &referencia, plano.geracao_restauracao)?;
                    }
                    if exec.tem_intencao(&referencia) || exec.tem_intencao_jogo(&referencia) {
                        exec.agente.fatos_enviados = Some(fatos);
                        exec.agente.proxima_consulta = agora + Duration::from_secs(5);
                        return Ok(Controle::Continuar);
                    }
                    exec.sessao_mut()
                        .trabalho
                        .as_mut()
                        .expect("tarefa iniciada")
                        .pronto = true;
                    exec.salvar()?;
                    if exec.enviar_pendente(None)? == Controle::Encerrar {
                        return Ok(Controle::Encerrar);
                    }
                }
            }
        }

        exec.agente.fatos_enviados = Some(fatos);
        // Uma geração confirmada e resultados pendentes aparecem na próxima consulta.
        exec.agente.proxima_consulta =
            agora + Duration::from_secs(if nova_geracao { 2 } else { 60 });
        Ok(Controle::Continuar)
    }
}

pub(crate) struct Execucao<'a, A, T, P> {
    pub(crate) agente: &'a mut Agente<A, T>,
    p: &'a P,
    pub(crate) estado: Estado,
    pub(crate) vigiados: Option<BTreeSet<String>>,
    /// Hash de cada arquivo de jogo antes da tarefa e depois da última escrita dela.
    pub(crate) cadeia: BTreeMap<std::path::PathBuf, (String, String)>,
}

impl<A: LeituraUsuario + EscritaUsuario, T: Transporte, P: Persistencia> Execucao<'_, A, T, P> {
    pub(crate) fn sessao(&self) -> &crate::estado::Sessao {
        &self.estado.sessoes[&self.agente.sessao]
    }

    fn sessao_mut(&mut self) -> &mut crate::estado::Sessao {
        self.estado
            .sessoes
            .get_mut(&self.agente.sessao)
            .expect("sessão inicializada")
    }

    pub(crate) fn salvar(&self) -> io::Result<()> {
        self.p.salvar(&self.estado)
    }

    /// Leituras de saúde vencidas, sob o portão com a lista atual do serviço.
    #[cfg(windows)]
    fn saude(&mut self, agora: Instant) -> io::Result<()> {
        let Some(agenda) = self.agente.saude.as_mut() else {
            return Ok(());
        };
        let portao = crate::saude::PortaoAgente {
            portao: &self.agente.portao,
            base: &self.agente.base,
            extras: self
                .vigiados
                .as_ref()
                .map(|v| v.iter().map(String::as_str).collect()),
        };
        let agora_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if agenda.passo(agora, agora_unix, &portao, &mut self.estado.saude) {
            self.salvar()?;
        }
        Ok(())
    }

    #[cfg(not(windows))]
    fn saude(&mut self, _: Instant) -> io::Result<()> {
        Ok(())
    }

    #[cfg(windows)]
    fn saude_pendente(&self) -> Option<nvt_ipc::SaudeSessao> {
        self.agente.saude.as_ref()?.pendente.clone()
    }

    #[cfg(not(windows))]
    fn saude_pendente(&self) -> Option<nvt_ipc::SaudeSessao> {
        None
    }

    fn saude_confirmada(&mut self) {
        #[cfg(windows)]
        if let Some(agenda) = self.agente.saude.as_mut() {
            agenda.pendente = None;
        }
    }

    fn escopo(&self, destino: &SettingUsuario) -> Option<u32> {
        matches!(destino, SettingUsuario::Refresh { .. }).then_some(self.agente.sessao)
    }

    fn indice(&self, destino: &SettingUsuario) -> Option<usize> {
        self.estado
            .destinos
            .iter()
            .position(|d| d.destino == *destino && d.sessao == self.escopo(destino))
    }

    fn acessivel(&self, d: &Destino) -> bool {
        d.sessao.is_none() || d.sessao == Some(self.agente.sessao)
    }

    fn recuperar_escopo_tela(&mut self, tela: &Fato<nvt_ipc::FatoTela>) -> io::Result<()> {
        let (Some(sessoes), Fato::Conhecido(tela)) = (&self.agente.sessoes_existentes, tela) else {
            return Ok(());
        };
        let destino = SettingUsuario::Refresh {
            alvo: tela.alvo.clone(),
        };
        if self.indice(&destino).is_some() {
            return Ok(());
        }
        let mut orfaos =
            self.estado.destinos.iter_mut().filter(|d| {
                d.destino == destino && d.sessao.is_some_and(|s| !sessoes.contains(&s))
            });
        if let Some(d) = orfaos.next().filter(|_| orfaos.next().is_none()) {
            d.sessao = Some(self.agente.sessao);
            self.salvar()?;
        }
        Ok(())
    }

    fn ler(&self, destino: &SettingUsuario) -> io::Result<ValorLido> {
        let valor = self.agente.armazem.ler(destino)?;
        if !valido(destino, &valor) {
            return Err(erro("leitura incompatível com o destino"));
        }
        Ok(valor)
    }

    fn fato<V>(
        &self,
        destino: SettingUsuario,
        extrair: impl FnOnce(ValorUsuario) -> Option<V>,
    ) -> Fato<V> {
        let Ok(atual) = self.ler(&destino) else {
            return Fato::Desconhecido;
        };
        if atual.valor == ValorUsuario::Ausente {
            return Fato::Ausente;
        }
        extrair(atual.valor).map_or(Fato::Desconhecido, Fato::Conhecido)
    }

    fn fatos(&self, tela: Fato<nvt_ipc::FatoTela>) -> FatosUsuario {
        use SettingUsuario::*;
        let armazem = &self.agente.armazem;
        FatosUsuario {
            tela,
            mouse: self.fato(MouseAceleracao, |v| match v {
                ValorUsuario::Spi(t) => Some(mouse(t)),
                _ => None,
            }),
            swap_effect_upgrade: self.fato(DxSwapEffectUpgrade, |v| match v {
                ValorUsuario::Token(t) => Some(t),
                _ => None,
            }),
            captura_historica: self.fato(GameDvrCapturaHistorica, |v| match v {
                ValorUsuario::Dword(t) => Some(t),
                _ => None,
            }),
            diagnosticos: armazem.diagnosticos(),
            gestao: self.gestao(),
            config_jogo: self.fatos_jogo(),
        }
    }

    /// O serviço reidrata dessa lista a gestão perdida em reinício ou nova sessão.
    fn gestao(&self) -> Vec<nvt_ipc::GestaoAgente> {
        self.estado
            .destinos
            .iter()
            .filter(|d| self.acessivel(d) && (d.travado || d.antes_gestao.is_some()))
            .map(|d| nvt_ipc::GestaoAgente {
                destino: d.destino.clone(),
                valores: d
                    .antes_gestao
                    .clone()
                    .zip(d.esperado.clone())
                    .and_then(|(antes, esperado)| mudanca(&d.destino, antes, esperado)),
                travado: d.travado,
            })
            .take(64)
            .collect()
    }

    fn deriva(&mut self) -> io::Result<()> {
        let mut mudou = false;
        for i in 0..self.estado.destinos.len() {
            let d = &self.estado.destinos[i];
            if !self.acessivel(d) || d.travado || d.intencao.is_some() {
                continue;
            }
            if let (Some(esperado), Ok(atual)) = (&d.esperado, self.ler(&d.destino)) {
                if !iguais(&atual, esperado) {
                    self.estado.destinos[i].travado = true;
                    mudou = true;
                }
            }
        }
        if mudou {
            self.salvar()?;
        }
        Ok(())
    }

    fn validar_plano(&self, plano: &PlanoAgente) -> io::Result<()> {
        if plano.geracao_restauracao < self.estado.geracao_vista
            || plano.revisao < self.sessao().revisao
        {
            return Err(erro("plano obsoleto"));
        }
        if plano.validar().is_err()
            || plano
                .tarefa_jogo
                .as_ref()
                .is_some_and(|t| t.tarefa.revisao != plano.revisao)
        {
            return Err(erro("tarefa de jogo obsoleta ou inválida"));
        }
        if plano.processos_vigiados.len() > 4096
            || plano.processos_vigiados.iter().any(|nome| {
                nome.is_empty()
                    || nome.len() > 260
                    || nome.trim() != nome
                    || nome.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
                    || nome.chars().any(char::is_control)
            })
        {
            return Err(erro("lista de processos vigiados inválida"));
        }
        if let Some(t) = &plano.tarefa {
            let mut destinos = BTreeSet::new();
            if t.tarefa.revisao != plano.revisao || t.acoes.len() > 256 {
                return Err(erro("tarefa obsoleta ou inválida"));
            }
            for acao in &t.acoes {
                let (destino, operacao) = match acao {
                    AcaoUsuario::Aplicar { ajuste } => (ajuste.destino(), OperacaoUsuario::Aplicar),
                    AcaoUsuario::Restaurar { destino } => (destino.clone(), t.tarefa.operacao),
                    AcaoUsuario::Reverter { destino } => {
                        (destino.clone(), OperacaoUsuario::Reverter)
                    }
                    AcaoUsuario::RestaurarSnapshot
                        if t.acoes.len() == 1
                            && t.tarefa.operacao == OperacaoUsuario::Restaurar =>
                    {
                        continue;
                    }
                    AcaoUsuario::RestaurarSnapshot => {
                        return Err(erro("tarefa de snapshot inválida"));
                    }
                };
                if t.tarefa.operacao != operacao
                    || !destinos.insert(destino.clone())
                    || matches!(destino, SettingUsuario::Refresh { alvo } if alvo.is_empty() || alvo.len() > 1024 || alvo.chars().any(char::is_control))
                {
                    return Err(erro("destino ou operação inválida"));
                }
            }
        }
        Ok(())
    }

    fn escrever_autorizado(
        &mut self,
        destino: &SettingUsuario,
        atual: &ValorLido,
        pedido: &ValorLido,
        restauracao: bool,
    ) -> Result<io::Result<()>, &'static str> {
        let extras: Vec<_> = self
            .vigiados
            .as_ref()
            .ok_or("lista do serviço indisponível")?
            .iter()
            .map(String::as_str)
            .collect();
        let mut autorizado = false;
        let mut bloqueio = None;
        let portao = &self.agente.portao;
        let base = &self.agente.base;
        let mut autorizar = || {
            if let Some(motivo) = bloqueio {
                return Err(erro(motivo));
            }
            match portao.verificar(base, &extras) {
                Ok(_) => {
                    autorizado = true;
                    Ok(())
                }
                Err(a) => {
                    let motivo = if a.lista_indisponivel {
                        "enumeração de processos indisponível"
                    } else {
                        "jogo ou anticheat em execução"
                    };
                    bloqueio = Some(motivo);
                    Err(erro(motivo))
                }
            }
        };
        let escrita =
            self.agente
                .armazem
                .escrever(destino, atual, pedido, restauracao, &mut autorizar);
        if !autorizado && let Some(motivo) = bloqueio {
            return Err(motivo);
        }
        Ok(escrita.and_then(|()| {
            autorizado
                .then_some(())
                .ok_or_else(|| erro("fachada não consultou o portão"))
        }))
    }

    fn iniciar(&mut self, tarefa: &ReferenciaTarefa) -> io::Result<()> {
        if self
            .sessao()
            .trabalho
            .as_ref()
            .is_some_and(|t| t.tarefa == *tarefa)
        {
            return Ok(());
        }
        self.sessao_mut().trabalho = Some(Trabalho {
            tarefa: *tarefa,
            resultados: Vec::new(),
            pronto: false,
            confirmado: false,
            geracao_concluida: None,
            resultados_jogo: Vec::new(),
        });
        self.salvar()
    }

    fn enviar_pendente(&mut self, tarefa_atual: Option<&TarefaUsuario>) -> io::Result<Controle> {
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

    fn registrar(&mut self, sessao: u32, tarefa: Option<&ReferenciaTarefa>, r: ResultadoUsuario) {
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

    fn tem_intencao(&self, tarefa: &ReferenciaTarefa) -> bool {
        self.estado.destinos.iter().any(|d| {
            d.intencao.as_ref().is_some_and(|i| {
                i.sessao == self.agente.sessao && i.tarefa.as_ref() == Some(tarefa)
            })
        })
    }

    fn resultado_registrado(&self, destino: &SettingUsuario) -> bool {
        self.sessao()
            .trabalho
            .as_ref()
            .is_some_and(|t| t.resultados.iter().any(|r| &r.destino == destino))
    }

    fn executar(
        &mut self,
        acao: AcaoUsuario,
        tarefa: &ReferenciaTarefa,
        geracao: u64,
    ) -> io::Result<()> {
        let (destino, operacao, ajuste) = match acao {
            AcaoUsuario::Aplicar { ajuste } => (ajuste.destino(), Operacao::Aplicar, Some(ajuste)),
            AcaoUsuario::Restaurar { destino } => (destino, Operacao::Retirar, None),
            AcaoUsuario::Reverter { destino } => (destino, Operacao::Reverter, None),
            AcaoUsuario::RestaurarSnapshot => return Ok(()), // Tratada antes das demais ações.
        };
        if !self.resultado_registrado(&destino) {
            let r = self.transicao(&destino, operacao, ajuste.as_ref(), Some(tarefa), geracao)?;
            self.registrar(self.agente.sessao, Some(tarefa), r);
            self.salvar()?;
        }
        Ok(())
    }

    fn restaurar_snapshot(
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

    fn transicao(
        &mut self,
        destino: &SettingUsuario,
        operacao: Operacao,
        ajuste: Option<&nvt_ipc::AjusteUsuario>,
        tarefa: Option<&ReferenciaTarefa>,
        geracao: u64,
    ) -> io::Result<ResultadoUsuario> {
        use StatusMudanca::*;
        let indice = self.indice(destino);
        if indice.is_some_and(|i| self.estado.destinos[i].intencao.is_some()) {
            return Ok(resultado(
                destino,
                Pendente,
                Some("intenção anterior aguarda recuperação"),
            ));
        }

        let Ok(atual) = self.ler(destino) else {
            return Ok(resultado(destino, Pendente, Some("destino indisponível")));
        };
        if let Some(i) = indice {
            let d = &mut self.estado.destinos[i];
            let divergiu = d.esperado.as_ref().is_some_and(|v| !iguais(v, &atual));
            if operacao == Operacao::Snapshot
                && d.bloquear_snapshot_ate
                    .is_some_and(|limite| geracao <= limite)
                && !atingiu(&atual, &d.snapshot)
            {
                return Ok(resultado(
                    destino,
                    Travada,
                    Some("intenção interrompida com valor divergente"),
                ));
            }
            if !operacao.snapshot() && (d.travado || divergiu) {
                d.travado = true;
                self.salvar()?;
                return Ok(resultado(
                    destino,
                    Travada,
                    Some("alteração local preservada"),
                ));
            }
        }

        let pedido = if let Some(ajuste) = ajuste {
            let (antes, depois) = valores(ajuste, &atual);
            if !valido(destino, &antes) || !valido(destino, &depois) {
                return Ok(resultado(destino, Recusada, Some("valor fora do catálogo")));
            }
            if !iguais(&antes, &atual) && !iguais(&depois, &atual) {
                return Ok(resultado(
                    destino,
                    Recusada,
                    Some("precondição do plano mudou"),
                ));
            }
            depois
        } else {
            indice
                .and_then(|i| {
                    let d = &self.estado.destinos[i];
                    if operacao.snapshot() {
                        Some(d.snapshot.clone())
                    } else {
                        d.antes_gestao.clone()
                    }
                })
                .unwrap_or_else(|| atual.clone())
        };

        if atingiu(&atual, &pedido) {
            if operacao == Operacao::Reverter {
                let i = indice.unwrap_or_else(|| self.capturar(destino, atual, geracao));
                self.estado.destinos[i].travado = true;
            } else if let Some(i) = indice {
                if operacao != Operacao::Aplicar {
                    self.concluir(i, operacao, &atual, &atual);
                }
            }
            return Ok(resultado(destino, JaNoValor, None));
        }

        let i = indice.unwrap_or_else(|| self.capturar(destino, atual.clone(), geracao));
        self.estado.destinos[i].intencao = Some(Intencao {
            anterior: atual.clone(),
            pedido: pedido.clone(),
            operacao,
            geracao,
            rollback: None,
            sessao: self.agente.sessao,
            tarefa: tarefa.cloned(),
        });
        self.salvar()?;
        let escrita =
            match self.escrever_autorizado(destino, &atual, &pedido, operacao != Operacao::Aplicar)
            {
                Ok(escrita) => escrita,
                Err(motivo) => {
                    self.estado.destinos[i].intencao = None;
                    if indice.is_none() {
                        self.estado.destinos.remove(i);
                    }
                    self.salvar()?;
                    return Ok(resultado(destino, Pendente, Some(motivo)));
                }
            };
        let Ok(lido) = self.ler(destino) else {
            return Ok(resultado(
                destino,
                Pendente,
                Some("releitura indisponível; intenção mantida"),
            ));
        };
        if atingiu(&lido, &pedido) {
            return self.finalizar(i, Aplicada, None);
        }
        if iguais(&lido, &atual) {
            return self.finalizar(i, Falhou, Some("releitura encontrou o valor anterior"));
        }
        #[cfg(windows)]
        let compensacao_falhou = escrita
            .as_ref()
            .err()
            .is_some_and(crate::windows::refresh_divergente);
        #[cfg(not(windows))]
        let compensacao_falhou = false;
        if matches!(destino, SettingUsuario::Refresh { .. })
            && (escrita.is_ok() || compensacao_falhou)
        {
            self.estado.destinos[i]
                .intencao
                .as_mut()
                .expect("intenção persistida")
                .rollback = Some(lido);
            self.salvar()?;
            if compensacao_falhou {
                return Ok(resultado(
                    destino,
                    Pendente,
                    Some("refresh divergente; compensação pendente"),
                ));
            }
            return self.compensar(i);
        }
        self.finalizar(
            i,
            Travada,
            Some("releitura divergente; alteração local preservada"),
        )
    }

    fn capturar(&mut self, destino: &SettingUsuario, atual: ValorLido, geracao: u64) -> usize {
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

    fn concluir(&mut self, i: usize, operacao: Operacao, anterior: &ValorLido, atual: &ValorLido) {
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

    fn finalizar(
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

    fn recuperar(&mut self) -> io::Result<()> {
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

    fn compensacoes(&mut self) -> io::Result<()> {
        for i in 0..self.estado.destinos.len() {
            let d = &self.estado.destinos[i];
            if self.acessivel(d) && d.intencao.as_ref().is_some_and(|t| t.rollback.is_some()) {
                self.compensar(i)?;
            }
        }
        Ok(())
    }

    fn compensar(&mut self, i: usize) -> io::Result<ResultadoUsuario> {
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
