//! Estado da interface, independente da janela (testável).
//!
//! A thread de comunicação produz [`Evento`]s; [`EstadoUi::aplicar`] os
//! incorpora. A renderização só lê o estado e emite [`Comando`]s.

use nvt_ipc::{
    Adiamento, AlertaDeriva, Aviso, Camada, EntradaHistorico, Estado, EstadoAtualizacao,
    EstadoExperimental, ImpactoVisual, Jogo, JogoIgnorado, MotivoPendente, Plano, Preferencias,
    Qualidade, Resposta, ResumoSistema, Reversao, SettingUsuario, Sincronismo, StatusMudanca,
    SugestaoExperimental, codigo_aviso,
};

use crate::textos;

// Consumido pelas telas novas no L2.
#[allow(dead_code)]
pub mod painel;
#[allow(unused_imports)]
pub use painel::{
    Fatos, LinhaEstado, MotivoIndisponivel, Saude, StatusJogo, TipoSinal, Veredito, linha_estado,
    linha_estado_visivel, status_jogo, veredito,
};

/// Telas da navegação lateral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tela {
    VisaoGeral,
    Jogos,
    Mudancas,
    Windows,
    Preferencias,
    Historico,
    Restaurar,
}

impl Tela {
    pub const TODAS: [Tela; 7] = [
        Tela::VisaoGeral,
        Tela::Jogos,
        Tela::Mudancas,
        Tela::Windows,
        Tela::Preferencias,
        Tela::Historico,
        Tela::Restaurar,
    ];

    /// Identificador usado na linha de comando (`--tela`) e nas capturas.
    pub fn chave(self) -> &'static str {
        match self {
            Tela::VisaoGeral => "visao-geral",
            Tela::Jogos => "jogos",
            Tela::Mudancas => "mudancas",
            Tela::Windows => "windows",
            Tela::Preferencias => "preferencias",
            Tela::Historico => "historico",
            Tela::Restaurar => "restaurar",
        }
    }

    pub fn de_chave(s: &str) -> Option<Tela> {
        Tela::TODAS.into_iter().find(|t| t.chave() == s)
    }
}

/// Situação da conexão com o serviço.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conexao {
    /// Primeira tentativa ainda em andamento.
    Conectando,
    /// Serviço respondendo.
    Servico,
    /// Serviço ausente: dados calculados localmente, somente leitura.
    Visualizacao,
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

impl GrupoMudancas {
    /// (pendentes, já no valor, com falha, sugestões experimentais).
    pub fn resumo(&self) -> (usize, usize, usize, usize) {
        let mut r = (0, 0, 0, 0);
        for i in &self.itens {
            if i.experimental == EstadoExperimental::Aceitavel {
                r.3 += 1;
                continue;
            }
            match i.status {
                StatusMudanca::Pendente => r.0 += 1,
                StatusMudanca::Aplicada | StatusMudanca::JaNoValor => r.1 += 1,
                StatusMudanca::Recusada | StatusMudanca::Falhou => r.2 += 1,
                StatusMudanca::Travada => {}
            }
        }
        r
    }
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
    AplicarAgora,
    RestaurarOriginal,
    Reescanear,
    DefinirPreferencias(Preferencias),
    Destravar {
        perfil: String,
        setting: String,
    },
    DefinirAceiteExperimental {
        regra: String,
        aceito: bool,
    },
    ReverterAjusteUsuario {
        destino: SettingUsuario,
    },
    /// Reversão individual de uma chave de arquivo de jogo (Fase 3).
    ReverterConfigJogo {
        destino: nvt_ipc::DestinoJogoIpc,
    },
    /// O usuário dispensou o aviso “Atualizado para a vX” (U-47).
    MarcarAtualizacaoVista {
        versao: String,
    },
    /// Incorpora um programa de inicialização à linha de base (v8).
    MarcarInicioVisto {
        fonte: nvt_ipc::FonteInicio,
        nome: String,
    },
}

pub fn destino_usuario(setting: &str, alvo_tela: Option<&str>) -> Option<SettingUsuario> {
    match setting {
        "tela_refresh" => Some(SettingUsuario::Refresh {
            alvo: alvo_tela?.into(),
        }),
        "mouse_aceleracao" => Some(SettingUsuario::MouseAceleracao),
        "dx_swap_effect_upgrade" => Some(SettingUsuario::DxSwapEffectUpgrade),
        "gamedvr_captura_historica" => Some(SettingUsuario::GameDvrCapturaHistorica),
        _ => None,
    }
}

impl Comando {
    /// Grava algo no driver ou no estado do serviço?
    pub fn escreve(&self) -> bool {
        !matches!(self, Comando::Atualizar | Comando::Reescanear)
    }

    pub fn mensagem_sucesso(&self) -> Option<&'static str> {
        match self {
            Comando::Atualizar => None,
            Comando::AplicarAgora => Some(textos::OK_APLICADO),
            Comando::RestaurarOriginal => Some(textos::OK_RESTAURADO),
            Comando::Reescanear => Some(textos::OK_REESCANEADO),
            Comando::DefinirPreferencias(_) => Some(textos::OK_PREFS),
            Comando::Destravar { .. } => Some(textos::OK_DESTRAVADO),
            Comando::DefinirAceiteExperimental { .. } => Some(textos::OK_ACEITE),
            Comando::ReverterAjusteUsuario { .. } => Some(textos::OK_REVERSAO_USUARIO),
            Comando::ReverterConfigJogo { .. } => Some(crate::arquivo_jogo::OK_REVERSAO_JOGO),
            Comando::MarcarAtualizacaoVista { .. } => None,
            Comando::MarcarInicioVisto { .. } => Some(textos::saude::OK_MARCADO_VISTO),
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
    /// Serviço ausente; dados locais somente leitura (ou erro ao calculá-los).
    Visualizacao(Result<Dados, String>),
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

/// Contagem de conformidade do plano.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Conformidade {
    pub pendentes: usize,
    pub conformes: usize,
    pub travadas: usize,
    pub falhas: usize,
}

impl Conformidade {
    pub fn de(grupos: &[GrupoMudancas], respeitadas: usize) -> Self {
        let mut c = Conformidade {
            travadas: respeitadas,
            ..Default::default()
        };
        for i in grupos
            .iter()
            .flat_map(|g| &g.itens)
            .filter(|i| i.experimental != EstadoExperimental::Aceitavel)
        {
            match i.status {
                StatusMudanca::Pendente => c.pendentes += 1,
                StatusMudanca::Aplicada | StatusMudanca::JaNoValor => c.conformes += 1,
                StatusMudanca::Travada => {}
                StatusMudanca::Recusada | StatusMudanca::Falhou => c.falhas += 1,
            }
        }
        c
    }

    pub fn conforme(&self) -> bool {
        self.pendentes == 0 && self.falhas == 0
    }
}

/// Estado completo da interface.
#[derive(Debug, Clone)]
pub struct EstadoUi {
    pub tela: Tela,
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
    /// Versão cujo aviso “Atualizado para” já foi dispensado nesta janela.
    pub atualizacao_dispensada: Option<String>,
}

pub fn preferencias_padrao() -> Preferencias {
    Preferencias {
        sincronismo: Sincronismo::Desligado,
        qualidade: Qualidade::Equilibrado,
        limite_fps: None,
    }
}

impl EstadoUi {
    pub fn new(tela: Tela) -> Self {
        Self {
            tela,
            conexao: Conexao::Conectando,
            dados: Dados::default(),
            erro_local: None,
            incompativel: None,
            mensagem: None,
            ocupado: None,
            rascunho: preferencias_padrao(),
            atualizado: false,
            atualizacao_dispensada: None,
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

    pub fn conformidade(&self) -> Conformidade {
        Conformidade::de(&self.dados.grupos, self.dados.respeitadas.len())
    }

    pub fn alertas(&self) -> &[AlertaDeriva] {
        self.dados
            .estado
            .as_ref()
            .map_or(&[], |e| e.alertas_deriva.as_slice())
    }

    /// Avisos do serviço (ou do cálculo local), mais graves primeiro.
    pub fn avisos(&self) -> &[Aviso] {
        self.dados
            .estado
            .as_ref()
            .map_or(&[], |e| e.avisos.as_slice())
    }

    /// Explicação, se a restauração estiver bloqueada (snapshot ilegível).
    pub fn restauracao_bloqueada(&self) -> Option<&Aviso> {
        self.dados
            .estado
            .as_ref()
            .and_then(|e| e.restauracao_bloqueada())
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
            .filter(|v| self.atualizacao_dispensada.as_deref() != Some(*v))
    }

    /// Dispensa o aviso: some na hora e devolve o comando que grava a
    /// versão como vista no serviço.
    pub fn dispensar_aviso_atualizado(&mut self) -> Option<Comando> {
        let versao = self.aviso_atualizado()?.to_owned();
        self.atualizacao_dispensada = Some(versao.clone());
        Some(Comando::MarcarAtualizacaoVista { versao })
    }

    /// Linhas de estado da atualização para a Visão geral (U-48).
    pub fn linhas_atualizacao(&self, agora_unix: u64) -> Vec<String> {
        self.atualizacao()
            .map(|a| textos::linhas_atualizacao(a, agora_unix))
            .unwrap_or_default()
    }

    pub fn restauracao_usuario_pendente(&self) -> bool {
        self.dados
            .estado
            .as_ref()
            .is_some_and(|e| e.agente.restauracao_pendente)
    }

    /// O botão “Restaurar original” pode ser usado?
    pub fn pode_restaurar(&self) -> bool {
        self.pode_escrever() && self.restauracao_bloqueada().is_none()
    }

    /// Jogos em ordem alfabética: do plano do serviço quando conectado, do
    /// cálculo local no modo de visualização (mesmo formato).
    /// Resumo da saúde para o veredito; `None` sem resposta de `ObterSaude`.
    pub fn saude(&self) -> Option<Saude> {
        self.dados.saude.as_ref().map(Saude::de_ipc)
    }

    pub fn jogos(&self) -> Vec<Jogo> {
        let mut jogos = self.dados.jogos.clone();
        jogos.sort_by_key(|j| j.nome.to_lowercase());
        jogos
    }

    pub fn aplicar(&mut self, ev: Evento) {
        match ev {
            Evento::Servico(d) => {
                let rascunho_limpo = !self.rascunho_alterado();
                self.conexao = Conexao::Servico;
                self.erro_local = None;
                self.incompativel = None;
                self.dados = d;
                if rascunho_limpo {
                    self.rascunho = self.preferencias_atuais();
                }
            }
            Evento::Visualizacao(r) => {
                let rascunho_limpo = !self.rascunho_alterado();
                // Uma operação pendente não termina mais: o serviço caiu.
                self.ocupado = None;
                self.conexao = Conexao::Visualizacao;
                match r {
                    Ok(d) => {
                        self.dados = d;
                        self.erro_local = None;
                    }
                    Err(e) => {
                        self.dados = Dados::default();
                        self.erro_local = Some(e);
                    }
                }
                if rascunho_limpo {
                    self.rascunho = self.preferencias_atuais();
                }
            }
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
                if matches!(comando, Comando::DefinirPreferencias(_)) {
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

/// Item do plano de uma setting num perfil (para mostrar valor e situação
/// das settings de uma regra aplicada).
pub fn item_no_perfil<'a>(
    grupos: &'a [GrupoMudancas],
    perfil: &str,
    setting: &str,
) -> Option<&'a ItemMudanca> {
    grupos
        .iter()
        .filter(|g| g.perfil.eq_ignore_ascii_case(perfil))
        .flat_map(|g| &g.itens)
        .find(|i| i.experimental != EstadoExperimental::Aceitavel && i.setting == setting)
}

/// Linha de uma setting aplicada: “Nome: valor (situação)”.
pub fn linha_setting(grupos: &[GrupoMudancas], perfil: &str, setting: &str) -> String {
    let nome = textos::setting_legivel(setting);
    match item_no_perfil(grupos, perfil, setting) {
        Some(i) => format!(
            "{}: {} ({})",
            i.nome_legivel,
            i.depois.as_deref().unwrap_or(textos::NAO_DEFINIDO),
            textos::status_mudanca(i.status)
        ),
        None => nome,
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

/// `dd/mm/aaaa hh:mm` a partir de um instante já no fuso desejado.
pub fn formatar_data_civil(unix_no_fuso: i64) -> String {
    let (y, m, d, h, mi, _) = civil(unix_no_fuso);
    format!("{d:02}/{m:02}/{y:04} {h:02}:{mi:02}")
}

/// Data no fuso local do Windows.
pub fn formatar_data(unix: u64) -> String {
    let unix = i64::try_from(unix).unwrap_or(i64::MAX / 2);
    formatar_data_civil(unix + crate::fuso::deslocamento_local(unix))
}

#[cfg(test)]
mod testes {
    use super::*;
    use nvt_ipc::{
        CodigoErro, MotivoNaoAplicada, Mudanca, OrigemPerfil, PlanoPerfil, RegraAplicada,
        RegraNaoAplicada, ResumoJogo,
    };

    fn mudanca(setting: &str, status: StatusMudanca) -> Mudanca {
        Mudanca {
            alvo_tela: None,
            camada: Camada::Driver,
            experimental: EstadoExperimental::Estavel,
            setting: setting.into(),
            nome_legivel: format!("Nome {setting}"),
            antes: None,
            depois: Some("Ligado".into()),
            motivo: "motivo".into(),
            regra: "regra.x".into(),
            impacto_visual: ImpactoVisual::Baixo,
            status,
        }
    }

    fn plano() -> Plano {
        Plano {
            sistema: Default::default(),
            sugestoes_driver: vec![],
            gerado_unix: 0,
            perfis: vec![
                PlanoPerfil {
                    perfil: "global".into(),
                    nome_legivel: "Perfil global".into(),
                    mudancas: vec![
                        mudanca("vsync", StatusMudanca::Aplicada),
                        mudanca("energia", StatusMudanca::Pendente),
                    ],
                },
                PlanoPerfil {
                    perfil: "WARDOGS".into(),
                    nome_legivel: "WARDOGS".into(),
                    mudancas: vec![
                        mudanca("dlss_preset", StatusMudanca::JaNoValor),
                        mudanca("dlss_override", StatusMudanca::Travada),
                        mudanca("shader_cache", StatusMudanca::Falhou),
                    ],
                },
                PlanoPerfil {
                    perfil: "Vazio".into(),
                    nome_legivel: "Vazio".into(),
                    mudancas: vec![mudanca("vsync", StatusMudanca::Travada)],
                },
            ],
            reversoes: vec![],
            ignorados: vec![],
            jogos: vec![],
            adiada: None,
        }
    }

    fn estado(prefs: Preferencias) -> Estado {
        Estado {
            agente: Default::default(),
            diagnosticos: vec![],
            versao_servico: "0.0.1".into(),
            driver: None,
            gpu: None,
            preferencias: prefs,
            jogos: 0,
            ultima_aplicacao: None,
            alertas_deriva: vec![],
            avisos: vec![],
            adiamento: None,
            atualizacao: Default::default(),
            autonomia_pausada: false,
        }
    }

    #[test]
    fn telas_por_chave() {
        for t in Tela::TODAS {
            assert_eq!(Tela::de_chave(t.chave()), Some(t));
        }
        assert_eq!(Tela::de_chave("inexistente"), None);
    }

    #[test]
    fn plano_vira_grupos_e_respeitadas() {
        let (g, r) = grupos_do_plano(&plano());
        assert_eq!(g.len(), 2, "perfil só com travadas não vira grupo");
        assert_eq!(g[1].itens.len(), 2);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].perfil, "WARDOGS");
        assert_eq!(r[0].setting, "dlss_override");
        let c = Conformidade::de(&g, r.len());
        assert_eq!(
            c,
            Conformidade {
                pendentes: 1,
                conformes: 2,
                travadas: 2,
                falhas: 1
            }
        );
        assert!(!c.conforme());
        assert_eq!(g[0].resumo(), (1, 1, 0, 0));
        assert_eq!(g[1].resumo(), (0, 1, 1, 0));
        assert_eq!(
            textos::resumo_grupo(0, 17, 0, 0),
            "nada pendente · 17 já no valor"
        );
        assert_eq!(
            textos::resumo_grupo(2, 0, 1, 1),
            "2 pendentes · 1 com falha · 1 sugestão experimental"
        );
    }

    #[test]
    fn experimentais_nao_contam_na_conformidade() {
        let (mut g, _) = grupos_do_plano(&plano());
        for i in &mut g[0].itens {
            i.experimental = EstadoExperimental::Aceitavel;
        }
        let c = Conformidade::de(&g, 0);
        assert_eq!((c.pendentes, c.conformes), (0, 1));
    }

    #[test]
    fn linha_de_setting_mostra_valor_e_situacao() {
        let (g, _) = grupos_do_plano(&plano());
        assert_eq!(
            linha_setting(&g, "wardogs", "dlss_preset"),
            "Nome dlss_preset: Ligado (já no valor)"
        );
        // Fora do plano: só o nome legível.
        assert_eq!(
            linha_setting(&g, "outro", "dlss_preset"),
            "DLSS: preset forçado"
        );
    }

    #[test]
    fn escrita_so_com_servico_ocioso() {
        let mut ui = EstadoUi::new(Tela::VisaoGeral);
        assert!(!ui.pode_escrever(), "conectando");
        ui.aplicar(Evento::Visualizacao(Ok(Dados::default())));
        assert_eq!(ui.conexao, Conexao::Visualizacao);
        assert!(!ui.pode_escrever());
        assert!(!ui.pode_executar(&Comando::AplicarAgora));
        assert!(!ui.pode_executar(&Comando::RestaurarOriginal));
        assert!(ui.pode_executar(&Comando::Reescanear));
        ui.aplicar(Evento::Servico(Dados::default()));
        assert!(ui.pode_escrever());
        ui.aplicar(Evento::Ocupado(Comando::AplicarAgora));
        assert!(!ui.pode_escrever());
        assert!(!ui.pode_executar(&Comando::Atualizar));
        ui.aplicar(Evento::Concluido {
            comando: Comando::AplicarAgora,
            resultado: Ok(Desfecho::Feito),
        });
        assert!(ui.pode_escrever());
        assert_eq!(
            ui.mensagem,
            Some((TipoMensagem::Sucesso, textos::OK_APLICADO.into()))
        );
    }

    #[test]
    fn queda_do_servico_libera_operacao_pendente() {
        let mut ui = EstadoUi::new(Tela::VisaoGeral);
        ui.aplicar(Evento::Servico(Dados::default()));
        ui.aplicar(Evento::Ocupado(Comando::AplicarAgora));
        ui.aplicar(Evento::Visualizacao(Err("sem driver".into())));
        assert_eq!(ui.ocupado, None);
        assert_eq!(ui.erro_local.as_deref(), Some("sem driver"));
        assert!(!ui.pode_escrever());
    }

    #[test]
    fn erro_aparece_como_mensagem() {
        let mut ui = EstadoUi::new(Tela::VisaoGeral);
        ui.aplicar(Evento::Concluido {
            comando: Comando::RestaurarOriginal,
            resultado: Err(textos::erro_servico(CodigoErro::Ocupado, "aguarde")),
        });
        let (tipo, texto) = ui.mensagem.clone().unwrap();
        assert_eq!(tipo, TipoMensagem::Erro);
        assert!(texto.contains("ocupado"), "{texto}");
    }

    #[test]
    fn rascunho_de_preferencias() {
        let mut ui = EstadoUi::new(Tela::Preferencias);
        let prefs = Preferencias {
            sincronismo: Sincronismo::GSync,
            qualidade: Qualidade::Qualidade,
            limite_fps: Some(141),
        };
        ui.aplicar(Evento::Servico(Dados {
            estado: Some(estado(prefs.clone())),
            ..Default::default()
        }));
        assert_eq!(ui.rascunho, prefs, "rascunho segue o serviço");
        ui.rascunho.qualidade = Qualidade::Desempenho;
        assert!(ui.rascunho_alterado());
        // Atualização do serviço com as mesmas preferências não apaga a edição.
        ui.aplicar(Evento::Servico(Dados {
            estado: Some(estado(prefs.clone())),
            ..Default::default()
        }));
        assert_eq!(ui.rascunho.qualidade, Qualidade::Desempenho);
        ui.aplicar(Evento::Concluido {
            comando: Comando::DefinirPreferencias(ui.rascunho.clone()),
            resultado: Ok(Desfecho::Feito),
        });
        assert!(!ui.rascunho_alterado());
    }

    fn jogo(nome: &str, resumo: ResumoJogo) -> Jogo {
        Jogo {
            config_jogo: None,
            nome: nome.into(),
            perfil: nome.into(),
            origem_perfil: OrigemPerfil::Usuario,
            launcher: "Steam".into(),
            api: "dx12".into(),
            dlss: Some("310.6".into()),
            reflex: Some(true),
            anticheat_kernel: None,
            competitivo: None,
            em_execucao: false,
            orientacoes: vec![],
            resumo,
        }
    }

    #[test]
    fn jogos_vem_do_servico_ou_do_calculo_local() {
        let mut ui = EstadoUi::new(Tela::Jogos);
        ui.aplicar(Evento::Visualizacao(Ok(Dados {
            jogos: vec![jogo("local", ResumoJogo::default())],
            ..Default::default()
        })));
        assert_eq!(ui.jogos()[0].nome, "local");
        ui.aplicar(Evento::Servico(Dados {
            jogos: vec![
                jogo("Zeta", ResumoJogo::default()),
                jogo("alfa", ResumoJogo::default()),
            ],
            ..Default::default()
        }));
        let nomes: Vec<String> = ui.jogos().into_iter().map(|j| j.nome).collect();
        assert_eq!(nomes, ["alfa", "Zeta"], "do serviço, em ordem alfabética");
    }

    fn nao(regra: &str, motivo: MotivoNaoAplicada) -> RegraNaoAplicada {
        RegraNaoAplicada {
            regra: regra.into(),
            descricao: format!("descrição de {regra}"),
            motivo,
        }
    }

    #[test]
    fn textos_da_coluna_o_que_o_app_fez() {
        let turing = ResumoJogo {
            aplicadas: vec![RegraAplicada {
                regra: "dlss-preset-e-turing".into(),
                descricao: "DLSS-SR com preset E".into(),
                settings: vec!["dlss_preset".into()],
            }],
            nao_aplicadas: vec![nao(
                "dlss-preset-e-ampere",
                MotivoNaoAplicada::CondicaoNaoAtendida {
                    condicao: "arquitetura".into(),
                    esperado: "ampere".into(),
                    encontrado: "turing".into(),
                },
            )],
            herda_do_global: vec!["energia".into(); 12],
        };
        assert_eq!(textos::resumo_curto(&turing), "Preset E do DLSS (Turing)");
        assert_eq!(textos::herda_globais(12), "herda 12 ajustes globais");
        assert_eq!(textos::herda_globais(1), "herda 1 ajuste global");
        assert_eq!(textos::herda_globais(0), "");
        assert_eq!(textos::herda_globais_lista(2), "Herda 2 ajustes globais");
        assert_eq!(
            textos::motivo_longo(&turing.nao_aplicadas[0].motivo),
            "Condição “arquitetura da GPU” não atendida: a regra pede Ampere; aqui é Turing."
        );

        let ada = MotivoNaoAplicada::ArquiteturaNaoPrecisa {
            arquitetura: "ada".into(),
        };
        let r = ResumoJogo {
            nao_aplicadas: vec![nao("a", ada.clone()), nao("b", ada.clone())],
            ..Default::default()
        };
        assert_eq!(
            textos::resumo_curto(&r),
            "Nenhum ajuste específico — GPU Ada roda o DLSS padrão sem custo extra"
        );
        assert_eq!(
            textos::motivo_longo(&ada),
            "GPU Ada Lovelace: o modelo padrão do DLSS já roda com FP8, forçar outro preset \
             não ganharia desempenho."
        );

        let sem_dlss = ResumoJogo {
            nao_aplicadas: vec![nao("a", MotivoNaoAplicada::JogoSemDlss)],
            ..Default::default()
        };
        assert_eq!(
            textos::resumo_curto(&sem_dlss),
            "Nenhum ajuste específico — jogo sem DLSS"
        );

        // Experimental vence o motivo de arquitetura da outra regra.
        let ampere = ResumoJogo {
            nao_aplicadas: vec![
                nao("ampere", MotivoNaoAplicada::Experimental),
                nao(
                    "turing",
                    MotivoNaoAplicada::CondicaoNaoAtendida {
                        condicao: "arquitetura".into(),
                        esperado: "turing".into(),
                        encontrado: "ampere".into(),
                    },
                ),
            ],
            ..Default::default()
        };
        assert_eq!(
            textos::resumo_curto(&ampere),
            "Experimental: aguardando confirmação"
        );
        assert!(
            textos::motivo_longo(&MotivoNaoAplicada::Experimental)
                .contains("aguardando confirmação por medição")
        );

        assert_eq!(
            textos::resumo_curto(&ResumoJogo {
                nao_aplicadas: vec![nao("a", MotivoNaoAplicada::SemTensorCores)],
                ..Default::default()
            }),
            "Nenhum ajuste específico — GPU sem Tensor Cores (sem DLSS)"
        );
        let teto = MotivoNaoAplicada::BloqueadaPeloTetoDeQualidade {
            teto: Qualidade::Qualidade,
        };
        assert_eq!(
            textos::motivo_curto(&teto),
            "bloqueado pelo teto de qualidade “Fidelidade”"
        );
        let travada = MotivoNaoAplicada::EscolhaDoUsuario {
            setting: "dlss_preset".into(),
        };
        assert_eq!(
            textos::motivo_curto(&travada),
            "você travou “DLSS: preset forçado”"
        );
        assert!(textos::motivo_longo(&travada).contains("destrave em Mudanças"));
        assert_eq!(
            textos::motivo_curto(&MotivoNaoAplicada::FatoDesconhecido {
                fato: "jogo_reflex".into()
            }),
            "não foi possível detectar: Reflex no jogo"
        );
        assert!(
            textos::motivo_longo(&MotivoNaoAplicada::ConflitoComRegraMaisEspecifica {
                regra: "preferencia:sincronismo".into()
            })
            .contains("preferência “sincronismo”")
        );
        assert_eq!(
            textos::resumo_curto(&ResumoJogo::default()),
            "Nenhum ajuste específico"
        );
        assert_eq!(textos::COL_APLICADO, "O que o app fez");
    }

    #[test]
    fn snapshot_indisponivel_desabilita_restaurar() {
        let mut ui = EstadoUi::new(Tela::Restaurar);
        let mut e = estado(preferencias_padrao());
        ui.aplicar(Evento::Servico(Dados {
            estado: Some(e.clone()),
            ..Default::default()
        }));
        assert!(ui.pode_restaurar());
        assert!(ui.avisos().is_empty());
        e.avisos = vec![
            Aviso::erro_perfil("perfil X", false, 2),
            Aviso::snapshot_indisponivel("snapshot ilegível", 1),
        ];
        ui.aplicar(Evento::Servico(Dados {
            estado: Some(e),
            ..Default::default()
        }));
        assert!(!ui.pode_restaurar());
        assert!(ui.pode_escrever(), "o resto continua liberado");
        assert_eq!(
            ui.restauracao_bloqueada()
                .and_then(|a| a.detalhe.as_deref()),
            Some("snapshot ilegível")
        );
        assert_eq!(ui.avisos().len(), 2);
    }

    #[test]
    fn versao_incompativel_mostra_instrucao_ate_reconectar() {
        let mut ui = EstadoUi::new(Tela::VisaoGeral);
        let m = textos::versao_incompativel(3, 2);
        assert!(m.contains("v3") && m.contains("v2"), "{m}");
        assert!(m.contains("mais novo"));
        assert!(m.contains("Atualize o nv-tuner"));
        assert!(textos::versao_incompativel(1, 2).contains("mais antigo"));
        ui.aplicar(Evento::Incompativel(Some(m.clone())));
        ui.aplicar(Evento::Visualizacao(Ok(Dados::default())));
        assert_eq!(ui.incompativel.as_deref(), Some(m.as_str()));
        ui.aplicar(Evento::Servico(Dados::default()));
        assert_eq!(ui.incompativel, None, "serviço compatível limpa o aviso");
    }

    fn com_atualizacao(a: EstadoAtualizacao) -> Dados {
        Dados {
            estado: Some(Estado {
                atualizacao: a,
                ..estado(preferencias_padrao())
            }),
            ..Default::default()
        }
    }

    fn concluida(para: &str, reinicio: bool) -> nvt_ipc::AtualizacaoConcluida {
        nvt_ipc::AtualizacaoConcluida {
            de: "1.0.0".into(),
            para: para.into(),
            quando_unix: 100,
            reinicio_pendente: reinicio,
        }
    }

    #[test]
    fn aviso_atualizado_uma_vez_e_marca_vista() {
        let mut ui = EstadoUi::new(Tela::VisaoGeral);
        let a = EstadoAtualizacao {
            versao_instalada: "1.1.0".into(),
            ultima_atualizacao: Some(concluida("1.1.0", false)),
            nao_vista: true,
            ..Default::default()
        };
        ui.aplicar(Evento::Servico(com_atualizacao(a.clone())));
        assert_eq!(ui.aviso_atualizado(), Some("1.1.0"));
        assert_eq!(
            textos::atualizado_para("1.1.0"),
            "Atualizado para a v1.1.0."
        );
        let c = ui.dispensar_aviso_atualizado();
        assert_eq!(
            c,
            Some(Comando::MarcarAtualizacaoVista {
                versao: "1.1.0".into()
            })
        );
        assert!(c.as_ref().unwrap().escreve());
        assert_eq!(c.unwrap().mensagem_sucesso(), None);
        // Some na hora, mesmo antes de o serviço gravar a marca.
        ui.aplicar(Evento::Servico(com_atualizacao(a.clone())));
        assert_eq!(ui.aviso_atualizado(), None);
        assert_eq!(ui.dispensar_aviso_atualizado(), None);
        // Já vista no serviço: nada a mostrar.
        let mut ui = EstadoUi::new(Tela::VisaoGeral);
        ui.aplicar(Evento::Servico(com_atualizacao(EstadoAtualizacao {
            nao_vista: false,
            ..a.clone()
        })));
        assert_eq!(ui.aviso_atualizado(), None);
        // Uma versão mais nova volta a mostrar o aviso.
        let mut ui2 = EstadoUi::new(Tela::VisaoGeral);
        ui2.atualizacao_dispensada = Some("1.1.0".into());
        ui2.aplicar(Evento::Servico(com_atualizacao(EstadoAtualizacao {
            ultima_atualizacao: Some(concluida("1.2.0", false)),
            ..a
        })));
        assert_eq!(ui2.aviso_atualizado(), Some("1.2.0"));
    }

    #[test]
    fn linhas_de_estado_da_atualizacao() {
        use nvt_ipc::{AtualizacaoPendente, ErroVerificacao, FalhaAtualizacao};
        let agora = 10_000;
        let mut a = EstadoAtualizacao {
            versao_instalada: "1.1.0".into(),
            ultima_verificacao_unix: Some(agora - 5 * 60),
            proxima_verificacao_unix: Some(agora + 6 * 3600),
            ..Default::default()
        };
        let mut ui = EstadoUi::new(Tela::VisaoGeral);
        assert!(ui.linhas_atualizacao(agora).is_empty(), "sem serviço");
        ui.aplicar(Evento::Servico(com_atualizacao(a.clone())));
        assert_eq!(
            ui.linhas_atualizacao(agora),
            vec!["Versão 1.1.0 · última verificação há 5 min · próxima em 6 h"]
        );
        assert!(
            textos::linha_versao_atualizacao(&EstadoAtualizacao::default(), agora)
                .contains("ainda não verificou")
        );

        let motivos = [
            (MotivoPendente::JogoAberto, "pendente: jogo aberto"),
            (
                MotivoPendente::OutraInstalacao,
                "aguardando outra instalação do Windows",
            ),
            (
                MotivoPendente::ListaIndisponivel,
                "não foi possível conferir",
            ),
            (MotivoPendente::Baixando, "Baixando a atualização v1.2.0"),
            (
                MotivoPendente::Instalando,
                "Instalando a atualização v1.2.0",
            ),
        ];
        for (motivo, esperado) in motivos {
            let t = textos::pendencia_atualizacao(&AtualizacaoPendente {
                versao: "1.2.0".into(),
                motivo,
                desde_unix: 0,
            });
            assert!(t.contains(esperado), "{t}");
        }

        a.ultima_atualizacao = Some(concluida("1.1.0", true));
        a.falha = Some(FalhaAtualizacao {
            versao: "1.2.0".into(),
            codigo: 1603,
            tentativas: 1,
            abandonada: false,
            quando_unix: 0,
        });
        a.erro_verificacao = Some(ErroVerificacao::SemConexao);
        let linhas = textos::linhas_atualizacao(&a, agora);
        assert_eq!(linhas.len(), 4);
        assert_eq!(
            linhas[1],
            "Reinicie o Windows quando puder para concluir a atualização."
        );
        assert_eq!(
            linhas[2],
            "Falha ao instalar v1.2.0 (código 1603); nova tentativa em 6 h."
        );
        assert_eq!(
            linhas[3],
            "Sem conexão com o GitHub; nova tentativa em 6 h."
        );
        let abandonada = textos::falha_atualizacao(
            &FalhaAtualizacao {
                abandonada: true,
                tentativas: 3,
                ..a.falha.clone().unwrap()
            },
            None,
            agora,
        );
        assert!(abandonada.contains("código 1603") && abandonada.contains("3 tentativas"));
        for (e, esperado) in [
            (ErroVerificacao::RecusadoHttp(403), "HTTP 403"),
            (ErroVerificacao::Assinatura, "assinatura"),
            (ErroVerificacao::Manifesto, "inválido"),
            (ErroVerificacao::Disco, "disco"),
        ] {
            let t = textos::erro_verificacao(e, None, agora);
            assert!(t.contains(esperado) && t.ends_with('.'), "{t}");
        }
        // U-49: nada oferece desligar, adiar ou voltar de versão.
        for l in linhas {
            let l = l.to_lowercase();
            assert!(!l.contains("deslig") && !l.contains("adiar") && !l.contains("voltar"));
        }
    }

    #[test]
    fn tempos_relativos() {
        assert_eq!(textos::ha(100, 130), "agora há pouco");
        assert_eq!(textos::ha(0, 3 * 3600), "há 3 h");
        assert_eq!(textos::ha(0, 86_400), "há 1 dia");
        assert_eq!(textos::ha(0, 3 * 86_400), "há 3 dias");
        assert_eq!(textos::em(0, 10), "em instantes");
        assert_eq!(textos::em(90, 0), "em 2 min");
        assert_eq!(textos::em(2 * 86_400, 0), "em 2 dias");
    }

    #[test]
    fn queda_durante_upgrade_vira_aviso_de_reabrir() {
        let versao_ui = "1.1.0";
        let com = |versao: &str, motivo: Option<MotivoPendente>| Estado {
            versao_servico: versao.into(),
            atualizacao: EstadoAtualizacao {
                pendente: motivo.map(|motivo| nvt_ipc::AtualizacaoPendente {
                    versao: "1.2.0".into(),
                    motivo,
                    desde_unix: 0,
                }),
                ..Default::default()
            },
            ..estado(preferencias_padrao())
        };
        let mesma = com(versao_ui, None);
        let instalando = com(versao_ui, Some(MotivoPendente::Instalando));
        let baixando = com(versao_ui, Some(MotivoPendente::Baixando));
        let jogo = com(versao_ui, Some(MotivoPendente::JogoAberto));
        // Pipe caiu: só é atualização se estava baixando/instalando.
        assert!(upgrade_detectado(Some(&instalando), None, versao_ui));
        assert!(upgrade_detectado(Some(&baixando), None, versao_ui));
        assert!(!upgrade_detectado(Some(&mesma), None, versao_ui));
        assert!(!upgrade_detectado(Some(&jogo), None, versao_ui));
        assert!(!upgrade_detectado(None, None, versao_ui));
        // Serviço respondeu com outra versão.
        assert!(upgrade_detectado(
            None,
            Some(&com("1.2.0", None)),
            versao_ui
        ));
        assert!(!upgrade_detectado(None, Some(&mesma), versao_ui));
        assert!(!upgrade_detectado(None, Some(&com("", None)), versao_ui));

        let mut ui = EstadoUi::new(Tela::VisaoGeral);
        ui.aplicar(Evento::Servico(Dados {
            estado: Some(instalando),
            ..Default::default()
        }));
        ui.aplicar(Evento::Ocupado(Comando::AplicarAgora));
        ui.aplicar(Evento::Atualizado);
        assert!(ui.atualizado);
        assert_eq!(ui.ocupado, None);
        assert!(!ui.pode_escrever());
        assert!(!ui.pode_executar(&Comando::Atualizar));
        assert!(textos::ATUALIZADO_REABRA.contains("foi atualizado"));
        assert!(textos::ATUALIZADO_REABRA.contains("abra de novo"));
    }

    #[test]
    fn textos_de_reversao() {
        use nvt_ipc::AcaoReversao;
        assert_eq!(
            textos::acao_reversao(AcaoReversao::GravarOriginal, Some("Ligado")),
            "volta a Ligado"
        );
        assert_eq!(
            textos::status_reversao(StatusMudanca::Pendente),
            "a reverter"
        );
        assert_eq!(
            textos::status_reversao(StatusMudanca::Aplicada),
            "revertida"
        );
        // Mesma frase do serviço (nvt-service::nucleo::motivo_reversao).
        assert_eq!(
            textos::motivo_reversao(
                &textos::causa_preferencia("limite_fps", None),
                AcaoReversao::Remover
            ),
            "Saiu do plano porque a preferência “limite_fps” mudou: sai do perfil (volta ao predefinido da NVIDIA, se houver)."
        );
    }

    #[test]
    fn todo_codigo_de_orientacao_tem_texto() {
        for o in nvt_core::Orientacao::TODAS {
            let texto = textos::orientacao(o.codigo())
                .unwrap_or_else(|| panic!("sem texto: {}", o.codigo()));
            assert!(texto.ends_with('.'), "{texto}");
            assert!(
                !texto.contains(o.codigo()),
                "código vazou no texto: {texto}"
            );
        }
        assert_eq!(
            textos::orientacao("reflex_on_boost"),
            Some("Ligue o NVIDIA Reflex em On + Boost (Ativado + Turbo).")
        );
        // Código desconhecido (serviço mais novo): não quebra, mostra o código.
        assert_eq!(textos::orientacao("reflex_turbo"), None);
        assert_eq!(textos::orientacao_ou_codigo("reflex_turbo"), "reflex_turbo");
        assert_eq!(textos::faca_no_jogo_resumo(0), "");
        assert_eq!(textos::faca_no_jogo_resumo(1), "Faça no jogo: 1 item");
        assert_eq!(textos::faca_no_jogo_resumo(3), "Faça no jogo: 3 itens");
    }

    fn adiamento(acao: nvt_ipc::AcaoAdiada) -> Adiamento {
        Adiamento {
            processos: vec!["cs2.exe".into()],
            desde_unix: 1,
            acao,
            lista_indisponivel: false,
        }
    }

    #[test]
    fn adiamento_vira_mensagem_e_nao_erro() {
        use nvt_ipc::AcaoAdiada;
        let mut p = plano();
        assert_eq!(
            Desfecho::da_resposta(&Resposta::Plano(p.clone())),
            Desfecho::Feito
        );
        p.adiada = Some(adiamento(AcaoAdiada::AplicarPlano));
        let d = Desfecho::da_resposta(&Resposta::Plano(p));
        assert_eq!(d, Desfecho::Adiado(adiamento(AcaoAdiada::AplicarPlano)));

        let mut ui = EstadoUi::new(Tela::VisaoGeral);
        ui.aplicar(Evento::Servico(Dados::default()));
        ui.aplicar(Evento::Ocupado(Comando::AplicarAgora));
        ui.aplicar(Evento::Concluido {
            comando: Comando::AplicarAgora,
            resultado: Ok(d),
        });
        assert_eq!(
            ui.mensagem,
            Some((
                TipoMensagem::Adiada,
                "Jogo aberto (cs2.exe): as alterações serão aplicadas quando você fechar o jogo."
                    .into()
            ))
        );
        assert!(ui.pode_escrever());

        let r = Desfecho::da_resposta(&Resposta::Restauracao {
            adiada: Some(adiamento(AcaoAdiada::RestaurarOriginal)),
        });
        ui.aplicar(Evento::Concluido {
            comando: Comando::RestaurarOriginal,
            resultado: Ok(r),
        });
        let (tipo, texto) = ui.mensagem.clone().unwrap();
        assert_eq!(tipo, TipoMensagem::Adiada);
        assert!(
            texto.contains("valores originais serão restaurados"),
            "{texto}"
        );
        assert_eq!(
            Desfecho::da_resposta(&Resposta::Restauracao { adiada: None }),
            Desfecho::Feito
        );
        assert!(
            textos::adiamento(&adiamento(AcaoAdiada::TratarDeriva))
                .contains("será tratada quando você fechar o jogo")
        );
    }

    #[test]
    fn faixa_de_adiamento_vem_do_estado_sem_repetir_a_mensagem() {
        use nvt_ipc::AcaoAdiada;
        let mut ui = EstadoUi::new(Tela::VisaoGeral);
        let mut e = estado(preferencias_padrao());
        ui.aplicar(Evento::Servico(Dados {
            estado: Some(e.clone()),
            ..Default::default()
        }));
        assert_eq!(ui.faixa_adiamento(), None);
        e.adiamento = Some(adiamento(AcaoAdiada::AplicarPlano));
        ui.aplicar(Evento::Servico(Dados {
            estado: Some(e),
            ..Default::default()
        }));
        let faixa = ui.faixa_adiamento().unwrap();
        assert!(faixa.starts_with("Jogo aberto (cs2.exe):"), "{faixa}");
        ui.mensagem = Some((TipoMensagem::Adiada, faixa));
        assert_eq!(ui.mensagem_visivel(), None, "a faixa já diz o mesmo");
        ui.mensagem = Some((TipoMensagem::Sucesso, textos::OK_PREFS.into()));
        assert!(ui.mensagem_visivel().is_some());
    }

    /// Preferências e Destravar adiados respondem `Resposta::Adiada`: a UI
    /// mostra o adiamento, não “feito”.
    #[test]
    fn preferencias_e_destravar_adiados_mostram_o_adiamento() {
        use nvt_ipc::AcaoAdiada;
        let r = Resposta::Adiada {
            adiamento: adiamento(AcaoAdiada::AplicarPlano),
        };
        assert_eq!(
            Desfecho::da_resposta(&r),
            Desfecho::Adiado(adiamento(AcaoAdiada::AplicarPlano))
        );
        assert_eq!(Desfecho::da_resposta(&Resposta::Ok), Desfecho::Feito);
        let mut ui = EstadoUi::new(Tela::VisaoGeral);
        ui.aplicar(Evento::Servico(Dados::default()));
        for comando in [
            Comando::DefinirPreferencias(preferencias_padrao()),
            Comando::Destravar {
                perfil: "global".into(),
                setting: "vsync".into(),
            },
        ] {
            ui.aplicar(Evento::Concluido {
                comando,
                resultado: Ok(Desfecho::da_resposta(&r)),
            });
            let (tipo, texto) = ui.mensagem.clone().unwrap();
            assert_eq!(tipo, TipoMensagem::Adiada);
            assert!(texto.starts_with("Jogo aberto (cs2.exe):"), "{texto}");
            assert_ne!(texto, textos::OK_PREFS);
            assert_ne!(texto, textos::OK_DESTRAVADO);
        }
    }

    /// Lista de processos indisponível: sinal próprio do protocolo, texto
    /// próprio na UI (nunca “Jogo aberto (lista de processos indisponível)”).
    #[test]
    fn lista_de_processos_indisponivel_tem_texto_proprio() {
        use nvt_ipc::AcaoAdiada;
        let a = Adiamento {
            processos: vec![],
            desde_unix: 1,
            acao: AcaoAdiada::AplicarPlano,
            lista_indisponivel: true,
        };
        let texto = textos::adiamento(&a);
        assert_eq!(
            texto,
            "Não foi possível verificar se há jogo aberto; as alterações ficam pendentes."
        );
        assert_eq!(textos::titulo_adiamento(&texto), "Alterações pendentes");
        assert_eq!(
            textos::titulo_adiamento(&textos::adiamento(&adiamento(AcaoAdiada::AplicarPlano))),
            textos::ADIAMENTO_TITULO
        );
        let mut ui = EstadoUi::new(Tela::VisaoGeral);
        let mut e = estado(preferencias_padrao());
        e.adiamento = Some(a);
        ui.aplicar(Evento::Servico(Dados {
            estado: Some(e),
            ..Default::default()
        }));
        let faixa = ui.faixa_adiamento().unwrap();
        assert_eq!(faixa, textos::ADIAMENTO_SEM_LISTA);
        assert!(!faixa.contains("Jogo aberto"));
    }

    fn com_vrr(possivel: Option<bool>, ativo: Option<bool>) -> Estado {
        let mut e = estado(Preferencias {
            sincronismo: Sincronismo::GSync,
            ..preferencias_padrao()
        });
        e.gpu = Some(nvt_ipc::Gpu {
            nome: "GPU".into(),
            arquitetura: "ada".into(),
            vram_mb: 8192,
            capacidades: nvt_ipc::Capacidades {
                dlss: true,
                geracao_quadros: true,
                reflex: true,
                rebar: true,
                refresh_mhz: Some(239_757),
                vrr: nvt_ipc::Vrr { possivel, ativo },
            },
        });
        e
    }

    #[test]
    fn aviso_de_sincronismo_segue_o_rascunho_e_o_vrr() {
        let mut ui = EstadoUi::new(Tela::Preferencias);
        ui.aplicar(Evento::Servico(Dados {
            estado: Some(com_vrr(Some(false), Some(false))),
            ..Default::default()
        }));
        assert_eq!(ui.aviso_sincronismo(), Some(textos::GSYNC_SEM_VRR));
        assert!(textos::GSYNC_SEM_VRR.starts_with("Seu monitor não tem G-SYNC/VRR"));
        ui.rascunho.sincronismo = Sincronismo::Desligado;
        assert_eq!(ui.aviso_sincronismo(), None, "sem G-SYNC, sem aviso");

        let mut ui = EstadoUi::new(Tela::Preferencias);
        ui.aplicar(Evento::Servico(Dados {
            estado: Some(com_vrr(Some(true), Some(false))),
            ..Default::default()
        }));
        assert_eq!(
            ui.aviso_sincronismo(),
            Some(textos::VRR_DESLIGADO_NO_PAINEL)
        );
        assert!(textos::VRR_DESLIGADO_NO_PAINEL.contains("Configurar G-SYNC"));

        let mut ui = EstadoUi::new(Tela::Preferencias);
        ui.aplicar(Evento::Servico(Dados {
            estado: Some(com_vrr(Some(true), Some(true))),
            ..Default::default()
        }));
        assert_eq!(ui.aviso_sincronismo(), None);

        // VRR desconhecido: vale o aviso do serviço.
        let mut e = com_vrr(None, None);
        e.avisos = vec![Aviso::gsync_sem_vrr(1)];
        let mut ui = EstadoUi::new(Tela::Preferencias);
        ui.aplicar(Evento::Servico(Dados {
            estado: Some(e),
            ..Default::default()
        }));
        assert_eq!(ui.aviso_sincronismo(), Some(textos::GSYNC_SEM_VRR));
        assert_eq!(
            textos::mensagem_aviso(&ui.avisos()[0]),
            textos::GSYNC_SEM_VRR
        );
    }

    #[test]
    fn refresh_e_vrr_desconhecidos_dizem_desconhecido() {
        assert_eq!(textos::refresh(Some(239_757)), "240 Hz");
        assert_eq!(textos::refresh(Some(143_999)), "144 Hz");
        assert_eq!(textos::refresh(Some(60_000)), "60 Hz");
        assert_eq!(textos::refresh(None), "desconhecido");
        assert_eq!(textos::refresh(Some(0)), "desconhecido");
        assert_eq!(textos::sim_nao_desconhecido(None), "desconhecido");
        assert_eq!(textos::sim_nao_desconhecido(Some(true)), "Sim");
        assert_eq!(textos::sim_nao_desconhecido(Some(false)), "Não");
    }

    #[test]
    fn motivos_vrr_inativo_e_jogo_com_reflex_tem_texto_proprio() {
        let generico = textos::motivo_curto(&MotivoNaoAplicada::CondicaoNaoAtendida {
            condicao: "x".into(),
            esperado: "a".into(),
            encontrado: "b".into(),
        });
        for m in [
            MotivoNaoAplicada::VrrInativo,
            MotivoNaoAplicada::JogoComReflex,
        ] {
            assert_ne!(textos::motivo_curto(&m), generico);
            assert!(!textos::motivo_longo(&m).contains("Condições da regra"));
        }
        assert!(textos::motivo_longo(&MotivoNaoAplicada::VrrInativo).contains("Configurar G-SYNC"));
        assert!(textos::motivo_longo(&MotivoNaoAplicada::JogoComReflex).contains("Reflex"));
        // Mais relevante que a condição genérica no texto curto.
        let r = ResumoJogo {
            nao_aplicadas: vec![
                nao(
                    "a",
                    MotivoNaoAplicada::CondicaoNaoAtendida {
                        condicao: "x".into(),
                        esperado: "a".into(),
                        encontrado: "b".into(),
                    },
                ),
                nao("b", MotivoNaoAplicada::VrrInativo),
            ],
            ..Default::default()
        };
        assert_eq!(
            textos::resumo_curto(&r),
            "Nenhum ajuste específico — G-SYNC (VRR) não está ativo"
        );
    }

    #[test]
    fn comandos_de_escrita() {
        assert!(!Comando::Atualizar.escreve());
        assert!(!Comando::Reescanear.escreve());
        assert!(Comando::AplicarAgora.escreve());
        assert!(Comando::RestaurarOriginal.escreve());
        assert!(Comando::DefinirPreferencias(preferencias_padrao()).escreve());
        assert!(
            Comando::Destravar {
                perfil: "a".into(),
                setting: "b".into()
            }
            .escreve()
        );
    }

    #[test]
    fn dados_de_usuario_sao_descartados_se_a_leitura_local_falhar() {
        let mut ui = EstadoUi::new(Tela::Windows);
        let mut e = estado(preferencias_padrao());
        e.agente.presente = true;
        e.agente.restauracao_pendente = true;
        let mut ajuste = mudanca("mouse_aceleracao", StatusMudanca::Aplicada);
        ajuste.camada = Camada::Usuario;
        ajuste.experimental = EstadoExperimental::Aceita;
        ui.aplicar(Evento::Servico(Dados {
            estado: Some(e),
            sistema: ResumoSistema {
                mudancas: vec![ajuste],
                sugestoes: vec![],
            },
            ..Dados::default()
        }));
        assert!(ui.restauracao_usuario_pendente());
        ui.aplicar(Evento::Concluido {
            comando: Comando::RestaurarOriginal,
            resultado: Ok(Desfecho::Feito),
        });
        assert_eq!(ui.mensagem_visivel().unwrap().1, textos::DRIVER_RESTAURADO);

        let reverter = Comando::ReverterAjusteUsuario {
            destino: SettingUsuario::MouseAceleracao,
        };
        let aceite = Comando::DefinirAceiteExperimental {
            regra: "regra.x".into(),
            aceito: true,
        };
        assert!(ui.pode_executar(&reverter));
        assert!(ui.pode_executar(&aceite));
        ui.aplicar(Evento::Visualizacao(Err("sem driver".into())));
        assert_eq!(ui.dados, Dados::default());
        assert!(!ui.restauracao_usuario_pendente());
        assert!(!ui.pode_executar(&reverter));
        assert!(!ui.pode_executar(&aceite));
    }

    #[test]
    fn destinos_windows_e_diagnostico_desconhecido() {
        assert_eq!(destino_usuario("tela_refresh", None), None);
        assert_eq!(
            destino_usuario("tela_refresh", Some("monitor")),
            Some(SettingUsuario::Refresh {
                alvo: "monitor".into(),
            })
        );
        assert_eq!(
            destino_usuario("mouse_aceleracao", None),
            Some(SettingUsuario::MouseAceleracao)
        );
        assert_eq!(
            destino_usuario("dx_swap_effect_upgrade", None),
            Some(SettingUsuario::DxSwapEffectUpgrade)
        );
        assert_eq!(
            destino_usuario("gamedvr_captura_historica", None),
            Some(SettingUsuario::GameDvrCapturaHistorica)
        );
        assert_eq!(destino_usuario("nova_setting", None), None);
        assert!(textos::diagnostico("novo_codigo").is_none());
    }

    #[test]
    fn datas() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(formatar_data_civil(1_790_000_000), "21/09/2026 14:13");
        assert_eq!(ler_iso("2026-09-21T14:13:20Z"), Some(1_790_000_000));
        assert_eq!(ler_iso("2024-02-29T23:59:59.123Z"), Some(1_709_251_199));
        assert_eq!(ler_iso("2026-13-01T00:00:00Z"), None);
        assert_eq!(ler_iso("lixo"), None);
        for t in [0_i64, 951_782_400, 1_709_251_199, 4_102_444_800] {
            let (y, m, d, h, mi, s) = civil(t);
            assert_eq!(unix_de_civil(y, m, d, h, mi, s), t);
        }
    }
}
