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

mod aplicacao;
mod consulta;
mod relato;
mod snapshots;

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
            if (geracao_ok && !nova_geracao) || ref_restauracao.is_some() {
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
        } else if exec.enviar_saude_avulsa()? == Controle::Encerrar {
            // Sem tarefa nesta consulta: a saúde vai num relato avulso na mesma conexão curta.
            return Ok(Controle::Encerrar);
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
