//! Estado da interface, independente da janela (testável).
//!
//! A thread de comunicação produz [`Evento`]s; [`EstadoUi::aplicar`] os
//! incorpora. A renderização só lê o estado e emite [`Comando`]s.

use nvt_ipc::{
    Adiamento, Aviso, Camada, EntradaHistorico, Estado, EstadoAtualizacao, EstadoExperimental,
    ImpactoVisual, Jogo, JogoIgnorado, MotivoPendente, Plano, Preferencias, Qualidade, Resposta,
    ResumoSistema, Reversao, Sincronismo, StatusMudanca, SugestaoExperimental, codigo_aviso,
};

use crate::textos;

pub mod painel;
pub use painel::{
    Fatos, LinhaEstado, MotivoIndisponivel, Saude, StatusJogo, TipoSinal, Veredito,
    linha_estado_visivel, status_jogo, veredito,
};

/// Situação da conexão com o serviço.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conexao {
    /// Primeira tentativa ainda em andamento.
    Conectando,
    /// Serviço respondendo.
    Servico,
    /// Ausência confirmada: dados locais somente leitura.
    Visualizacao,
    /// Serviço iniciando: dados locais somente leitura.
    Iniciando,
    /// Serviço sem resposta: dados locais somente leitura.
    SemResposta,
}

/// Uma linha da tela de mudanças.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemMudanca {
    pub setting: String,
    pub nome_legivel: String,
    pub antes: Option<String>,
    pub depois: Option<String>,
    pub motivo: String,
    pub regra: String,
    pub impacto: ImpactoVisual,
    pub status: StatusMudanca,
    pub camada: Camada,
    pub experimental: EstadoExperimental,
}

impl From<&nvt_ipc::Mudanca> for ItemMudanca {
    fn from(m: &nvt_ipc::Mudanca) -> Self {
        Self {
            setting: m.setting.clone(),
            nome_legivel: m.nome_legivel.clone(),
            antes: m.antes.clone(),
            depois: m.depois.clone(),
            motivo: m.motivo.clone(),
            regra: m.regra.clone(),
            impacto: m.impacto_visual,
            status: m.status,
            camada: m.camada,
            experimental: m.experimental,
        }
    }
}

/// Mudanças de um perfil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrupoMudancas {
    pub perfil: String,
    pub nome_legivel: String,
    pub itens: Vec<ItemMudanca>,
}

/// Setting que o plano mudaria, mas está travada por escolha do usuário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Respeitada {
    pub perfil: String,
    pub nome_perfil: String,
    pub setting: String,
    pub nome_legivel: String,
    pub motivo: String,
}

/// Dados exibidos (vindos do serviço ou do cálculo local).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Dados {
    pub estado: Option<Estado>,
    pub sistema: ResumoSistema,
    pub sugestoes_driver: Vec<SugestaoExperimental>,
    pub grupos: Vec<GrupoMudancas>,
    pub respeitadas: Vec<Respeitada>,
    pub historico: Vec<EntradaHistorico>,
    /// Settings que saíram do plano e voltam ao original.
    pub reversoes: Vec<Reversao>,
    /// Jogos deixados de fora do plano.
    pub ignorados: Vec<JogoIgnorado>,
    /// Jogos do plano (fatos e explicação), do serviço ou do cálculo local.
    pub jogos: Vec<Jogo>,
    /// Última resposta de `ObterSaude` (cache da thread de comunicação);
    /// `None` = sem dados (serviço antigo, visualização ou ainda lendo).
    pub saude: Option<nvt_ipc::Saude>,
}

/// Ações que o usuário pode disparar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comando {
    Atualizar,
    RestaurarOriginal,
    Reescanear,
    DefinirPreferencias(Preferencias),
    /// Reversão individual de uma chave de arquivo de jogo (Fase 3).
    ReverterConfigJogo {
        destino: nvt_ipc::DestinoJogoIpc,
    },
}

impl Comando {
    /// Grava algo no driver ou no estado do serviço?
    pub fn escreve(&self) -> bool {
        !matches!(self, Comando::Atualizar | Comando::Reescanear)
    }

    pub fn mensagem_sucesso(&self) -> Option<&'static str> {
        match self {
            Comando::Atualizar => None,
            Comando::RestaurarOriginal => Some(textos::OK_RESTAURADO),
            Comando::Reescanear => Some(textos::OK_REESCANEADO),
            Comando::DefinirPreferencias(_) => Some(textos::OK_PREFS),
            Comando::ReverterConfigJogo { .. } => Some(crate::arquivo_jogo::OK_REVERSAO_JOGO),
        }
    }
}

/// Tipo de mensagem mostrada no topo da tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoMensagem {
    Sucesso,
    /// O serviço aceitou, mas adiou a gravação (jogo aberto): não é erro.
    Adiada,
    Erro,
}

/// Desfecho de um comando aceito pelo serviço.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Desfecho {
    /// Feito agora.
    Feito,
    /// Jogo aberto: a gravação fica pendente até ele fechar (IPC v4).
    Adiado(Adiamento),
}

impl Desfecho {
    /// Desfecho de uma resposta do serviço: `Plano.adiada` (Aplicar agora),
    /// `Restauracao.adiada` (Restaurar original) e `Adiada` (Preferências e
    /// Destravar) viram [`Desfecho::Adiado`].
    pub fn da_resposta(r: &Resposta) -> Self {
        let adiada = match r {
            Resposta::Plano(p) => p.adiada.clone(),
            Resposta::Restauracao { adiada } => adiada.clone(),
            Resposta::Adiada { adiamento } => Some(adiamento.clone()),
            _ => None,
        };
        adiada.map_or(Desfecho::Feito, Desfecho::Adiado)
    }
}

/// Evento produzido pela thread de comunicação.
#[derive(Debug, Clone)]
pub enum Evento {
    /// O serviço respondeu; dados completos.
    Servico(Dados),
    /// Diagnóstico e dados locais somente leitura (ou erro ao calculá-los).
    Visualizacao(Conexao, Result<Dados, String>),
    /// Mudança no diagnóstico com os mesmos dados locais.
    Conexao(Conexao),
    /// Serviço com versão de protocolo diferente (mensagem com a instrução
    /// de atualizar) ou `None` quando deixa de valer.
    Incompativel(Option<String>),
    /// O serviço foi trocado por uma atualização (U-42): a janela deve ser
    /// reaberta; a thread de comunicação para de reconectar.
    Atualizado,
    /// Uma operação começou.
    Ocupado(Comando),
    /// Uma operação terminou.
    Concluido {
        comando: Comando,
        resultado: Result<Desfecho, String>,
    },
}

/// Estado completo da interface.
#[derive(Debug, Clone)]
pub struct EstadoUi {
    pub conexao: Conexao,
    pub dados: Dados,
    /// Erro ao montar o modo de visualização local.
    pub erro_local: Option<String>,
    /// Serviço de outra versão do protocolo: instrução para atualizar.
    pub incompativel: Option<String>,
    pub mensagem: Option<(TipoMensagem, String)>,
    pub ocupado: Option<Comando>,
    /// Preferências em edição (ainda não enviadas).
    pub rascunho: Preferencias,
    /// O nv-tuner foi atualizado com esta janela aberta (U-42).
    pub atualizado: bool,
    /// Nome e launcher de cada jogo de `dados.jogos`, em minúsculas, para a
    /// busca da tela Jogos (calculados quando os dados chegam).
    chaves_jogos: Vec<(String, String)>,
}

pub fn preferencias_padrao() -> Preferencias {
    Preferencias {
        sincronismo: Sincronismo::Desligado,
        qualidade: Qualidade::Equilibrado,
        limite_fps: None,
    }
}

impl EstadoUi {
    pub fn new() -> Self {
        Self {
            conexao: Conexao::Conectando,
            dados: Dados::default(),
            erro_local: None,
            incompativel: None,
            mensagem: None,
            ocupado: None,
            rascunho: preferencias_padrao(),
            atualizado: false,
            chaves_jogos: Vec::new(),
        }
    }

    /// Preferências atualmente em vigor (do serviço ou do estado salvo).
    pub fn preferencias_atuais(&self) -> Preferencias {
        self.dados
            .estado
            .as_ref()
            .map(|e| e.preferencias.clone())
            .unwrap_or_else(preferencias_padrao)
    }

    pub fn rascunho_alterado(&self) -> bool {
        self.rascunho != self.preferencias_atuais()
    }

    /// Ações de escrita só existem com o serviço conectado e ocioso.
    pub fn pode_escrever(&self) -> bool {
        self.conexao == Conexao::Servico && self.ocupado.is_none() && !self.atualizado
    }

    pub fn pode_executar(&self, c: &Comando) -> bool {
        if c.escreve() {
            self.pode_escrever()
        } else {
            self.ocupado.is_none() && !self.atualizado
        }
    }

    /// Avisos do serviço (ou do cálculo local), mais graves primeiro.
    pub fn avisos(&self) -> &[Aviso] {
        self.dados
            .estado
            .as_ref()
            .map_or(&[], |e| e.avisos.as_slice())
    }

    /// Gravação pendente porque há jogo aberto (faixa de estado).
    pub fn adiamento(&self) -> Option<&Adiamento> {
        self.dados
            .estado
            .as_ref()
            .and_then(|e| e.adiamento.as_ref())
    }

    /// Texto da faixa de estado “Jogo aberto (…)”, se houver adiamento.
    pub fn faixa_adiamento(&self) -> Option<String> {
        self.adiamento().map(textos::adiamento)
    }

    /// A mensagem do topo, sem repetir a faixa de adiamento (a resposta
    /// “adiada” e o estado seguinte dizem a mesma coisa).
    pub fn mensagem_visivel(&self) -> Option<&(TipoMensagem, String)> {
        self.mensagem.as_ref().filter(|(tipo, texto)| {
            !(*tipo == TipoMensagem::Adiada
                && self.faixa_adiamento().as_deref() == Some(texto.as_str()))
        })
    }

    /// Aviso ao lado da escolha de sincronismo nas Preferências (F1-15),
    /// calculado para o rascunho: G-SYNC escolhido sem VRR, ou com VRR
    /// desligado no painel. Sem a leitura do monitor, vale o aviso do serviço
    /// (se o rascunho é o que está em vigor).
    pub fn aviso_sincronismo(&self) -> Option<&'static str> {
        if self.rascunho.sincronismo != Sincronismo::GSync {
            return None;
        }
        let vrr = self
            .dados
            .estado
            .as_ref()
            .and_then(|e| e.gpu.as_ref())
            .map(|g| g.capacidades.vrr)
            .unwrap_or_default();
        match (vrr.possivel, vrr.ativo) {
            (Some(false), _) => Some(textos::GSYNC_SEM_VRR),
            (Some(true), Some(false)) => Some(textos::VRR_DESLIGADO_NO_PAINEL),
            (Some(true), _) => None,
            (None, _) if self.rascunho_alterado() => None,
            (None, _) => self.avisos().iter().find_map(|a| match a.codigo.as_str() {
                codigo_aviso::GSYNC_SEM_VRR => Some(textos::GSYNC_SEM_VRR),
                codigo_aviso::VRR_DESLIGADO_NO_PAINEL => Some(textos::VRR_DESLIGADO_NO_PAINEL),
                _ => None,
            }),
        }
    }

    /// Estado da atualização automática, só com o serviço conectado.
    pub fn atualizacao(&self) -> Option<&EstadoAtualizacao> {
        self.dados
            .estado
            .as_ref()
            .filter(|_| self.conexao == Conexao::Servico)
            .map(|e| &e.atualizacao)
    }

    /// Versão do aviso “Atualizado para a vX”, enquanto não visto (U-47).
    pub fn aviso_atualizado(&self) -> Option<&str> {
        self.atualizacao()
            .filter(|a| a.nao_vista)
            .and_then(|a| a.ultima_atualizacao.as_ref())
            .map(|u| u.para.as_str())
    }

    pub fn restauracao_usuario_pendente(&self) -> bool {
        self.dados
            .estado
            .as_ref()
            .is_some_and(|e| e.agente.restauracao_pendente)
    }

    /// Resumo da saúde para o veredito; `None` sem resposta de `ObterSaude`.
    pub fn saude(&self) -> Option<Saude> {
        self.dados.saude.as_ref().map(Saude::de_ipc)
    }

    /// Jogos em ordem alfabética: do plano do serviço quando conectado, do
    /// cálculo local no modo de visualização (mesmo formato).
    pub fn jogos(&self) -> &[Jogo] {
        &self.dados.jogos
    }

    /// O jogo `i` de [`EstadoUi::jogos`] tem `termo` (já em minúsculas) no
    /// nome ou no launcher?
    pub fn jogo_corresponde(&self, i: usize, termo: &str) -> bool {
        termo.is_empty()
            || self
                .chaves_jogos
                .get(i)
                .is_some_and(|(nome, launcher)| nome.contains(termo) || launcher.contains(termo))
    }

    /// Troca os dados exibidos, já com os jogos ordenados e as chaves de
    /// busca prontas.
    fn receber_dados(&mut self, mut d: Dados) {
        let mut chaves: Vec<_> = std::mem::take(&mut d.jogos)
            .into_iter()
            .map(|j| ((j.nome.to_lowercase(), j.launcher.to_lowercase()), j))
            .collect();
        chaves.sort_by(|a, b| a.0.0.cmp(&b.0.0));
        (self.chaves_jogos, d.jogos) = chaves.into_iter().unzip();
        self.dados = d;
    }

    pub fn aplicar(&mut self, ev: Evento) {
        match ev {
            Evento::Servico(d) => {
                let rascunho_limpo = !self.rascunho_alterado();
                self.conexao = Conexao::Servico;
                self.erro_local = None;
                self.incompativel = None;
                self.receber_dados(d);
                if rascunho_limpo {
                    self.rascunho = self.preferencias_atuais();
                }
            }
            Evento::Visualizacao(conexao, r) => {
                let rascunho_limpo = !self.rascunho_alterado();
                // Uma operação pendente não termina mais: o serviço caiu.
                self.ocupado = None;
                self.conexao = conexao;
                match r {
                    Ok(d) => {
                        self.receber_dados(d);
                        self.erro_local = None;
                    }
                    Err(e) => {
                        self.receber_dados(Dados::default());
                        self.erro_local = Some(e);
                    }
                }
                if rascunho_limpo {
                    self.rascunho = self.preferencias_atuais();
                }
            }
            Evento::Conexao(conexao) => self.conexao = conexao,
            Evento::Incompativel(m) => self.incompativel = m,
            Evento::Atualizado => {
                self.atualizado = true;
                self.ocupado = None;
                self.incompativel = None;
            }
            Evento::Ocupado(c) => {
                self.ocupado = Some(c);
                self.mensagem = None;
            }
            Evento::Concluido { comando, resultado } => {
                self.ocupado = None;
                let falhou = resultado.is_err();
                self.mensagem = match resultado {
                    Ok(Desfecho::Feito)
                        if comando == Comando::RestaurarOriginal
                            && self.restauracao_usuario_pendente() =>
                    {
                        Some((TipoMensagem::Sucesso, textos::DRIVER_RESTAURADO.into()))
                    }
                    Ok(Desfecho::Feito)
                        if comando == Comando::RestaurarOriginal
                            && self.conexao != Conexao::Servico =>
                    {
                        Some((TipoMensagem::Sucesso, textos::RESTAURACAO_SOLICITADA.into()))
                    }
                    Ok(Desfecho::Feito) => comando
                        .mensagem_sucesso()
                        .map(|m| (TipoMensagem::Sucesso, m.to_string())),
                    Ok(Desfecho::Adiado(a)) => Some((TipoMensagem::Adiada, textos::adiamento(&a))),
                    Err(e) => Some((TipoMensagem::Erro, e)),
                };
                // Com salvamento automático o usuário pode escolher de novo
                // enquanto o envio anterior corre: essa escolha nova não pode
                // ser apagada. Só volta ao estado do serviço se nada mudou
                // desde o envio, ou se o envio falhou.
                if let Comando::DefinirPreferencias(enviadas) = &comando
                    && (falhou || self.rascunho == *enviadas)
                {
                    self.rascunho = self.preferencias_atuais();
                }
                if self.atualizado {
                    // A faixa “foi atualizado” já explica; sem erro de transporte.
                    self.mensagem = None;
                }
            }
        }
    }
}

/// A troca do serviço é uma atualização do nv-tuner (U-42)? `atual` é o
/// estado recém-lido (`None` quando o pipe caiu depois de já ter conectado);
/// `anterior`, o último estado lido. Com o pipe caído, só conta como
/// atualização se o serviço estava baixando ou instalando uma versão; fora
/// disso é serviço parado (modo de visualização).
pub fn upgrade_detectado(
    anterior: Option<&Estado>,
    atual: Option<&Estado>,
    versao_ui: &str,
) -> bool {
    match atual {
        Some(e) => !e.versao_servico.is_empty() && e.versao_servico != versao_ui,
        None => anterior
            .and_then(|e| e.atualizacao.pendente.as_ref())
            .is_some_and(|p| {
                matches!(
                    p.motivo,
                    MotivoPendente::Baixando | MotivoPendente::Instalando
                )
            }),
    }
}

/// Converte o plano do serviço em grupos e itens respeitados.
pub fn grupos_do_plano(plano: &Plano) -> (Vec<GrupoMudancas>, Vec<Respeitada>) {
    let mut grupos = Vec::new();
    let mut respeitadas = Vec::new();
    for p in &plano.perfis {
        let mut itens = Vec::new();
        for m in &p.mudancas {
            if m.status == StatusMudanca::Travada {
                respeitadas.push(Respeitada {
                    perfil: p.perfil.clone(),
                    nome_perfil: p.nome_legivel.clone(),
                    setting: m.setting.clone(),
                    nome_legivel: m.nome_legivel.clone(),
                    motivo: m.motivo.clone(),
                });
            } else {
                itens.push(ItemMudanca::from(m));
            }
        }
        if !itens.is_empty() {
            grupos.push(GrupoMudancas {
                perfil: p.perfil.clone(),
                nome_legivel: p.nome_legivel.clone(),
                itens,
            });
        }
    }
    (grupos, respeitadas)
}

// ---------------------------------------------------------------------------
// Datas
// ---------------------------------------------------------------------------

/// Data civil (UTC) de um instante Unix: (ano, mês, dia, hora, minuto, segundo).
pub fn civil(unix: i64) -> (i64, u32, u32, u32, u32, u32) {
    let dias = unix.div_euclid(86_400);
    let resto = unix.rem_euclid(86_400);
    // Algoritmo de Howard Hinnant (days_from_civil inverso).
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        y,
        m,
        d,
        (resto / 3600) as u32,
        (resto % 3600 / 60) as u32,
        (resto % 60) as u32,
    )
}

/// Instante Unix de uma data civil UTC.
pub fn unix_de_civil(y: i64, m: u32, d: u32, h: u32, mi: u32, s: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let dias = era * 146_097 + doe - 719_468;
    dias * 86_400 + i64::from(h) * 3600 + i64::from(mi) * 60 + i64::from(s)
}

/// Lê `AAAA-MM-DDTHH:MM:SS` (com `Z` ou fração opcionais) como UTC.
pub fn ler_iso(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<u32>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (h, mi, se) = (n(11..13)?, n(14..16)?, n(17..19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    u64::try_from(unix_de_civil(i64::from(y), m, d, h, mi, se)).ok()
}

#[cfg(test)]
mod testes;
