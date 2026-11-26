//! Lógica do serviço, independente do SCM, das threads e do driver real.
//!
//! [`Nucleo`] guarda o motor, os jogos detectados e o estado próprio do
//! serviço (autonomia, última aplicação, alertas) e expõe:
//!
//! - [`Nucleo::iniciar`] / [`Nucleo::ciclo`]: deriva → tratamento → plano →
//!   aplicação, respeitando autonomia e modo somente leitura;
//! - [`Nucleo::notificar_mudanca_drs`] / [`Nucleo::tick`]: debounce das
//!   notificações do diretório do DRS, verificação e varredura periódicas,
//!   ignorando as mudanças que o próprio serviço gravou;
//! - [`Nucleo::tratar`]: tratador das requisições do IPC.
//!
//! Toda gravação no DRS passa pelo portão do motor
//! ([`Engine::pode_gravar`], T-02). Com jogo ou anticheat vigiado em
//! execução, a ação fica pendente ([`AcaoPendente`]) e é reavaliada a cada
//! [`Config::intervalo_adiamento`]; ao fechar o jogo, roda uma vez (F1-07).
//!
//! O tempo é sempre passado por quem chama (`agora`), para os testes
//! controlarem o relógio.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nvt_core::{
    Alvo, AvisoPlano, BaseJogos, DefSetting, ImpactoVisual as ImpactoRegra, Origem, Preferences,
};
use nvt_drs::ProfileRef;
use nvt_engine::{
    AcaoReversao, Adiamento, Aplicacao, Armazem, ClasseDeriva, CodigoAviso, Deriva, Diff,
    Divergencia, DriverStore, Engine, ErroEngine, Esperado, JogoEntrada, MAX_TRAVADAS,
    OrigemRegistro, Portao, Resultado, casar_na_base, mesmo_alvo, mesmo_nome_perfil,
};
use nvt_ipc::{
    AcaoHistorico, Aviso, Capacidades, CodigoErro, EntradaHistorico, Mudanca, NivelAviso, Plano,
    PlanoPerfil, Requisicao, Resposta, StatusMudanca,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::nomes;

/// Abre uma sessão nova do DRS (o driver só é relido ao abrir a sessão).
pub type Fabrica<'a, S> = Box<dyn FnMut() -> Resultado<S> + 'a>;
/// Varre os jogos instalados.
pub type Varredor<'a> = Box<dyn FnMut() -> Vec<JogoEntrada> + 'a>;
/// Assinatura do diretório do DRS (nome, tamanho, data de modificação).
pub type Assinatura = Vec<(String, u64, u128)>;
/// Calcula a [`Assinatura`] atual (`None` se indisponível).
pub type Assinador<'a> = Box<dyn FnMut() -> Option<Assinatura> + 'a>;

/// Nome do perfil global no protocolo IPC.
pub const PERFIL_GLOBAL: &str = "global";
const ARQ_SERVICO: &str = "servico.json";
const ALERTAS_MAX: usize = 100;

/// Intervalos do laço.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Impede qualquer gravação (driver e estado).
    pub somente_leitura: bool,
    /// Verificação periódica de deriva.
    pub intervalo_verificacao: Duration,
    /// Nova varredura de jogos.
    pub intervalo_varredura: Duration,
    /// Silêncio exigido após a última notificação do DRS antes de verificar.
    pub debounce: Duration,
    /// Notificações até este tempo depois de uma gravação própria são ignoradas.
    pub janela_propria: Duration,
    /// Reavaliação do portão enquanto houver ação pendente (jogo aberto).
    pub intervalo_adiamento: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            somente_leitura: false,
            intervalo_verificacao: Duration::from_secs(10 * 60),
            intervalo_varredura: Duration::from_secs(6 * 60 * 60),
            debounce: Duration::from_secs(3),
            janela_propria: Duration::from_secs(15),
            intervalo_adiamento: Duration::from_secs(15),
        }
    }
}

/// Gravação que ficou para quando o jogo fechar (F1-07). Só uma por vez: a
/// mais recente substitui as anteriores, exceto [`AcaoPendente::RestaurarOriginal`],
/// que vence qualquer outra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcaoPendente {
    /// Aplicação do plano (automática, preferências, destravar, “Aplicar agora”).
    AplicarPlano,
    /// Tratamento de deriva com reaplicação no driver.
    TratarDeriva,
    /// “Restaurar original”.
    RestaurarOriginal,
}

impl AcaoPendente {
    fn ipc(self) -> nvt_ipc::AcaoAdiada {
        match self {
            Self::AplicarPlano => nvt_ipc::AcaoAdiada::AplicarPlano,
            Self::TratarDeriva => nvt_ipc::AcaoAdiada::TratarDeriva,
            Self::RestaurarOriginal => nvt_ipc::AcaoAdiada::RestaurarOriginal,
        }
    }
}

/// Ação pendente e o último adiamento visto pelo portão.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pendente {
    pub acao: AcaoPendente,
    pub adiamento: Adiamento,
}

/// Última aplicação de plano (persistida em `servico.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UltimaAplicacao {
    pub quando_unix: u64,
    pub mudancas: u32,
    pub falhas: u32,
}

/// Estado próprio do serviço (`<estado-dir>\servico.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstadoServico {
    /// Pausada por `RestaurarOriginal`; reativada por `AplicarAgora`.
    pub autonomia_pausada: bool,
    pub pausada_em_unix: Option<u64>,
    pub ultima_aplicacao: Option<UltimaAplicacao>,
}

impl EstadoServico {
    /// Carrega de `dir` (ausente ou inválido = padrão).
    pub fn carregar(dir: Option<&Path>) -> Self {
        let Some(dir) = dir else {
            return Self::default();
        };
        let ler = || -> std::io::Result<String> {
            let mut s = String::new();
            std::io::Read::read_to_string(
                &mut crate::diretorio::abrir_sem_reparse(
                    &dir.join(ARQ_SERVICO),
                    fs::OpenOptions::new().read(true),
                )?,
                &mut s,
            )?;
            Ok(s)
        };
        match ler() {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                warn!("servico.json inválido ({e}); usando o padrão");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Grava em `dir` (temporário novo + renomear). O temporário é sempre
    /// criado do zero (`create_new`, sem seguir reparse points): um link
    /// plantado no lugar dele é removido (o próprio link, não o alvo).
    pub fn salvar(&self, dir: &Path) -> std::io::Result<()> {
        fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("{ARQ_SERVICO}.tmp"));
        let _ = fs::remove_file(&tmp);
        let texto = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let mut f = crate::diretorio::abrir_sem_reparse(
            &tmp,
            fs::OpenOptions::new().write(true).create_new(true),
        )?;
        std::io::Write::write_all(&mut f, texto.as_bytes())?;
        f.sync_all()?;
        drop(f);
        fs::rename(tmp, dir.join(ARQ_SERVICO))
    }
}

/// Segundos Unix agora.
pub fn agora_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// ISO 8601 UTC (`AAAA-MM-DDTHH:MM:SSZ`, formato do `nvt-engine`) para Unix.
pub fn iso_para_unix(s: &str) -> u64 {
    let n = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<i64>().ok());
    let (Some(y), Some(m), Some(d), Some(hh), Some(mm), Some(ss)) =
        (n(0, 4), n(5, 7), n(8, 10), n(11, 13), n(14, 16), n(17, 19))
    else {
        return 0;
    };
    // days_from_civil (Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let dias = era * 146_097 + doe - 719_468;
    u64::try_from(dias * 86_400 + hh * 3600 + mm * 60 + ss).unwrap_or(0)
}

/// Lógica do serviço sobre um [`DriverStore`].
pub struct Nucleo<'a, S: DriverStore> {
    engine: Engine<S>,
    fabrica: Fabrica<'a, S>,
    varredor: Varredor<'a>,
    assinador: Assinador<'a>,
    cfg: Config,
    dir: Option<PathBuf>,
    servico: EstadoServico,
    jogos: Vec<JogoEntrada>,
    alertas: Vec<nvt_ipc::AlertaDeriva>,
    /// Avisos da abertura do estado (acumulados: o arquivo corrompido só é
    /// visto na primeira abertura, depois de renomeado).
    avisos_estado: Vec<Aviso>,
    /// Erros isolados por perfil da última aplicação.
    erros_aplicacao: Vec<Aviso>,
    /// Erros isolados por perfil do último tratamento de deriva.
    erros_deriva: Vec<Aviso>,
    /// Jogos ignorados no último plano calculado e quando.
    ignorados: Vec<nvt_ipc::JogoIgnorado>,
    ignorados_unix: u64,
    /// Avisos do último plano calculado (G-SYNC sem VRR etc.) e quando mudaram.
    avisos_plano: Vec<AvisoPlano>,
    avisos_plano_unix: u64,
    ultima_gravacao: Option<Instant>,
    assinatura_propria: Option<Assinatura>,
    mudanca_pendente: Option<Instant>,
    proxima_verificacao: Instant,
    proxima_varredura: Instant,
    /// Portão de gravação, o mesmo em cada motor recriado por `renovar`.
    portao: Portao,
    /// Gravação adiada por jogo aberto (F1-07).
    pendente: Option<Pendente>,
    /// Próxima consulta ao portão (só com `pendente`).
    proxima_reavaliacao: Option<Instant>,
}

impl<'a, S: DriverStore> Nucleo<'a, S> {
    /// Abre o estado de `dir` (`None` = só em memória) e a primeira sessão do
    /// DRS, com o portão sobre os processos do sistema.
    pub fn novo(
        fabrica: Fabrica<'a, S>,
        varredor: Varredor<'a>,
        assinador: Assinador<'a>,
        dir: Option<PathBuf>,
        cfg: Config,
        agora: Instant,
    ) -> Resultado<Self> {
        Self::novo_com_portao(
            fabrica,
            varredor,
            assinador,
            dir,
            cfg,
            agora,
            Portao::do_sistema(),
        )
    }

    /// Como [`Nucleo::novo`], com o portão dado (fonte de processos injetável).
    pub fn novo_com_portao(
        mut fabrica: Fabrica<'a, S>,
        varredor: Varredor<'a>,
        assinador: Assinador<'a>,
        dir: Option<PathBuf>,
        cfg: Config,
        agora: Instant,
        portao: Portao,
    ) -> Resultado<Self> {
        let mut avisos_estado = Vec::new();
        let armazem = match &dir {
            Some(d) => abrir_armazem(d, &cfg, &mut avisos_estado)?,
            None => Armazem::em_memoria(),
        };
        let engine = Engine::new(fabrica()?, armazem)?.com_portao(portao.clone());
        Ok(Self {
            engine,
            fabrica,
            varredor,
            assinador,
            cfg,
            servico: EstadoServico::carregar(dir.as_deref()),
            dir,
            jogos: Vec::new(),
            alertas: Vec::new(),
            avisos_estado,
            erros_aplicacao: Vec::new(),
            erros_deriva: Vec::new(),
            ignorados: Vec::new(),
            ignorados_unix: 0,
            avisos_plano: Vec::new(),
            avisos_plano_unix: 0,
            ultima_gravacao: None,
            assinatura_propria: None,
            mudanca_pendente: None,
            proxima_verificacao: agora + cfg.intervalo_verificacao,
            proxima_varredura: agora + cfg.intervalo_varredura,
            portao,
            pendente: None,
            proxima_reavaliacao: None,
        })
    }

    /// Gravação adiada por jogo aberto, se houver.
    #[cfg(test)]
    pub fn pendente(&self) -> Option<&Pendente> {
        self.pendente.as_ref()
    }

    fn restauracao_pendente(&self) -> bool {
        self.pendente
            .as_ref()
            .is_some_and(|p| p.acao == AcaoPendente::RestaurarOriginal)
    }

    /// Registra a ação adiada pelo portão. A mais recente substitui as
    /// anteriores, exceto `RestaurarOriginal`, que vence qualquer outra.
    fn adiar(&mut self, acao: AcaoPendente, adiamento: Adiamento, agora: Instant) {
        let acao = if self.restauracao_pendente() {
            AcaoPendente::RestaurarOriginal
        } else {
            acao
        };
        if self.pendente.as_ref().map(|p| p.acao) != Some(acao) {
            info!(
                "gravação adiada ({acao:?}): jogo em execução ({}); reavaliação a cada {} s",
                adiamento.processos.join(", "),
                self.cfg.intervalo_adiamento.as_secs()
            );
        }
        self.pendente = Some(Pendente { acao, adiamento });
        self.proxima_reavaliacao = Some(agora + self.cfg.intervalo_adiamento);
    }

    /// A gravação de `acao` foi feita (ou não havia o que gravar): a pendência
    /// equivalente deixa de valer. `RestaurarOriginal` pendente só sai quando
    /// a própria restauração roda.
    fn concluir_pendente(&mut self, acao: AcaoPendente) {
        let sai = match self.pendente.as_ref().map(|p| p.acao) {
            None => false,
            Some(AcaoPendente::RestaurarOriginal) => acao == AcaoPendente::RestaurarOriginal,
            Some(_) => true,
        };
        if sai {
            self.pendente = None;
            self.proxima_reavaliacao = None;
        }
    }

    /// Adiamento atual no formato do protocolo.
    fn adiamento_ipc(&self) -> Option<nvt_ipc::Adiamento> {
        self.pendente.as_ref().map(|p| nvt_ipc::Adiamento {
            processos: p.adiamento.processos.clone(),
            desde_unix: p.adiamento.desde_unix,
            acao: p.acao.ipc(),
        })
    }

    /// Com ação pendente, consulta o portão; livre, executa a ação uma vez.
    fn reavaliar_pendente(&mut self, agora: Instant) {
        if self.pendente.is_none() {
            self.proxima_reavaliacao = None;
            return;
        }
        match self.engine.pode_gravar() {
            Err(a) => {
                if let Some(p) = &mut self.pendente {
                    p.adiamento = a;
                }
                self.proxima_reavaliacao = Some(agora + self.cfg.intervalo_adiamento);
            }
            Ok(()) => {
                let Some(p) = self.pendente.take() else {
                    return;
                };
                self.proxima_reavaliacao = None;
                info!("jogo fechado: executando a ação pendente ({:?})", p.acao);
                match p.acao {
                    AcaoPendente::RestaurarOriginal => {
                        if let Err(e) = self.restaurar_original(agora) {
                            error!("restauração pendente falhou: {e}");
                        }
                    }
                    AcaoPendente::AplicarPlano | AcaoPendente::TratarDeriva => {
                        self.ciclo(agora, "jogo fechado (ação pendente)");
                    }
                }
            }
        }
    }

    pub fn engine(&self) -> &Engine<S> {
        &self.engine
    }

    pub fn servico(&self) -> &EstadoServico {
        &self.servico
    }

    pub fn jogos(&self) -> &[JogoEntrada] {
        &self.jogos
    }

    #[cfg(test)]
    pub fn alertas(&self) -> &[nvt_ipc::AlertaDeriva] {
        &self.alertas
    }

    /// O serviço pode gravar (não está em modo somente leitura)?
    pub fn pode_gravar(&self) -> bool {
        !self.cfg.somente_leitura
    }

    /// Aplicação automática ativa?
    pub fn autonomia_ativa(&self) -> bool {
        self.pode_gravar() && !self.servico.autonomia_pausada
    }

    /// Relê o driver (sessão nova) e o estado em disco, que pode ter sido
    /// alterado por `nvt-service apply`/`restore-original` em outro processo.
    fn renovar(&mut self) -> Resultado<()> {
        let store = (self.fabrica)()?;
        let armazem = match &self.dir {
            Some(d) => abrir_armazem(d, &self.cfg, &mut self.avisos_estado)?,
            None => self.engine.armazem().clone(),
        };
        let regras = self.engine.regras().clone();
        self.engine = Engine::com_regras(store, armazem, regras).com_portao(self.portao.clone());
        if self.dir.is_some() {
            self.servico = EstadoServico::carregar(self.dir.as_deref());
        }
        Ok(())
    }

    fn salvar_servico(&self) {
        if !self.pode_gravar() {
            return;
        }
        if let Some(d) = &self.dir
            && let Err(e) = self.servico.salvar(d)
        {
            error!("falha ao gravar servico.json: {e}");
        }
    }

    fn marcar_gravacao(&mut self, agora: Instant) {
        self.ultima_gravacao = Some(agora);
        self.assinatura_propria = (self.assinador)();
    }

    /// Varre os jogos instalados.
    /// Os exes encontrados passam a ser vigiados pelo portão de gravação.
    pub fn varrer(&mut self, agora: Instant) {
        self.jogos = (self.varredor)();
        self.portao
            .vigiar_jogos(self.jogos.iter().map(|j| j.exe.as_str()));
        self.proxima_varredura = agora + self.cfg.intervalo_varredura;
        info!(
            "varredura: {} jogo(s) com executável principal, {} em execução",
            self.jogos.len(),
            self.jogos.iter().filter(|j| j.em_execucao).count()
        );
    }

    /// Início do serviço: varredura e primeiro ciclo.
    pub fn iniciar(&mut self, agora: Instant) {
        self.varrer(agora);
        if self.servico.autonomia_pausada {
            info!("autonomia pausada (restauração anterior): só verificação");
        }
        self.ciclo(agora, "início");
    }

    /// Verifica a deriva e, com autonomia, trata-a e aplica o plano.
    pub fn ciclo(&mut self, agora: Instant, motivo: &str) {
        self.proxima_verificacao = agora + self.cfg.intervalo_verificacao;
        if let Err(e) = self.ciclo_interno(agora, motivo) {
            error!("ciclo ({motivo}) falhou: {e}");
        }
    }

    fn ciclo_interno(&mut self, agora: Instant, motivo: &str) -> Resultado<()> {
        debug!("ciclo: {motivo}");
        self.renovar()?;
        if self.autonomia_ativa() {
            self.tratar_e_aplicar(agora, motivo)?;
        } else {
            self.tratar_deriva_pendente(agora, motivo, false)?;
        }
        Ok(())
    }

    /// Trata a deriva e aplica o plano. Com jogo aberto (portão do motor),
    /// nada é gravado: a ação fica pendente e o retorno é `None`. Com
    /// restauração pendente, só lê a deriva (a restauração vence).
    fn tratar_e_aplicar(
        &mut self,
        agora: Instant,
        motivo: &str,
    ) -> Resultado<Option<(Diff, Aplicacao)>> {
        if self.restauracao_pendente() {
            self.tratar_deriva_pendente(agora, motivo, false)?;
            return Ok(None);
        }
        match self.tratar_deriva_pendente(agora, motivo, true) {
            Err(ErroEngine::Adiada(a)) => {
                self.adiar(AcaoPendente::TratarDeriva, a, agora);
                return Ok(None);
            }
            r => r?,
        }
        match self.aplicar_plano(agora) {
            Err(ErroEngine::Adiada(a)) => {
                self.adiar(AcaoPendente::AplicarPlano, a, agora);
                Ok(None)
            }
            Err(e) => Err(e),
            Ok(r) => {
                self.concluir_pendente(AcaoPendente::AplicarPlano);
                Ok(Some(r))
            }
        }
    }

    /// Verifica a deriva e, com `tratar`, trata-a (mudança manual vira escolha
    /// travada; reset em massa é reaplicado). Precisa vir **antes** de
    /// qualquer [`Nucleo::aplicar_plano`]: uma mudança manual ainda não vista
    /// pelo ciclo seria tratada como valor do app e revertida pelo plano novo.
    fn tratar_deriva_pendente(
        &mut self,
        agora: Instant,
        motivo: &str,
        tratar: bool,
    ) -> Resultado<()> {
        let deriva = self.engine.verificar_deriva()?;
        if deriva.classe == ClasseDeriva::Nenhuma {
            self.erros_deriva.clear();
            return Ok(());
        }
        info!(
            "deriva ({motivo}): {:?}, {} divergência(s)",
            deriva.classe,
            deriva.divergencias.len()
        );
        self.registrar_alertas(&deriva);
        if !tratar {
            return Ok(());
        }
        let t = self.engine.tratar_deriva(&deriva, true)?;
        self.registrar_erros_deriva(&t.erros);
        if !t.reaplicadas.is_empty() {
            self.marcar_gravacao(agora);
            // Reaplicadas: o alerta deixa de valer.
            for e in &t.reaplicadas {
                let p = perfil_ipc(&e.perfil);
                self.alertas
                    .retain(|a| !(a.perfil == p && a.setting == e.setting));
            }
        }
        info!(
            "deriva tratada: {} reaplicada(s), {} travada(s), {} falha(s)",
            t.reaplicadas.len(),
            t.travadas.len(),
            t.falhas.len()
        );
        Ok(())
    }

    /// Planeja e aplica (só estáveis: o plano do avaliador não inclui as
    /// experimentais). O snapshot original é capturado pelo motor antes da
    /// primeira gravação.
    fn aplicar_plano(&mut self, agora: Instant) -> Resultado<(Diff, Aplicacao)> {
        let diff = self.planejar()?;
        if diff.vazio() {
            debug!("plano: nada a mudar ({} conforme(s))", diff.conformes.len());
            self.erros_aplicacao.clear();
            return Ok((diff, Aplicacao::default()));
        }
        let ap = self.engine.aplicar(&diff, true)?;
        self.marcar_gravacao(agora);
        let quando = agora_unix();
        self.erros_aplicacao = ap
            .erros
            .iter()
            .map(|e| {
                error!("erro isolado num perfil ao aplicar: {e}");
                Aviso::erro_perfil(e.clone(), false, quando)
            })
            .collect();
        for r in &diff.reverter {
            let ok = ap.revertidas.contains(&r.esperado);
            let acao = match r.acao {
                AcaoReversao::Gravar(_) => "valor original gravado",
                AcaoReversao::Remover { .. } => "removida do perfil",
                AcaoReversao::SoDeixarDeVigiar => "deixou de ser vigiada",
            };
            if ok {
                info!(
                    "saiu do plano: {} em {} ({acao})",
                    r.esperado.setting,
                    perfil_ipc(&r.esperado.perfil)
                );
            } else {
                warn!(
                    "saiu do plano, mas a reversão não foi confirmada: {} em {}",
                    r.esperado.setting,
                    perfil_ipc(&r.esperado.perfil)
                );
            }
        }
        info!(
            "aplicação: {} verificada(s), {} falha(s), {} perfil(is) criado(s){}",
            ap.verificadas.len(),
            ap.falhas.len(),
            ap.perfis_criados.len(),
            if ap.snapshot_capturado {
                "; snapshot original capturado"
            } else {
                ""
            }
        );
        for (m, lido) in &ap.falhas {
            warn!(
                "releitura divergente: {} em {} (esperado {}, lido {lido:?})",
                m.mudanca.setting,
                perfil_ipc(&m.perfil),
                m.mudanca.valor
            );
        }
        self.servico.ultima_aplicacao = Some(UltimaAplicacao {
            quando_unix: agora_unix(),
            mudancas: u32::try_from(ap.verificadas.len()).unwrap_or(u32::MAX),
            falhas: u32::try_from(ap.falhas.len()).unwrap_or(u32::MAX),
        });
        self.salvar_servico();
        Ok((diff, ap))
    }

    fn registrar_alertas(&mut self, deriva: &Deriva) {
        let quando = agora_unix();
        for d in &deriva.divergencias {
            let def = self.def(&d.esperado.setting);
            let a = nvt_ipc::AlertaDeriva {
                perfil: perfil_ipc(&d.esperado.perfil),
                setting: d.esperado.setting.clone(),
                nome_legivel: def.map_or_else(|| d.esperado.setting.clone(), nomes::nome_setting),
                esperado: nomes::valor(def, Some(d.esperado.valor)).unwrap_or_default(),
                encontrado: nomes::valor(def, d.atual),
                detectado_unix: quando,
            };
            self.alertas
                .retain(|x| !(x.perfil == a.perfil && x.setting == a.setting));
            self.alertas.push(a);
        }
        let excesso = self.alertas.len().saturating_sub(ALERTAS_MAX);
        self.alertas.drain(..excesso);
    }

    /// Planeja e guarda os jogos ignorados (registrando no log quando mudam).
    fn planejar(&mut self) -> Resultado<Diff> {
        let diff = self.engine.planejar(&self.jogos)?;
        let ignorados: Vec<nvt_ipc::JogoIgnorado> = diff
            .ignorados
            .iter()
            .map(|j| nvt_ipc::JogoIgnorado {
                nome: j.nome.clone(),
                exe: j.exe.clone(),
                motivo: j.motivo.clone(),
            })
            .collect();
        if ignorados != self.ignorados {
            for j in &ignorados {
                warn!("jogo ignorado pelo plano: {} ({})", j.nome, j.motivo);
            }
            self.ignorados_unix = agora_unix();
            self.ignorados = ignorados;
        }
        if diff.plano.avisos != self.avisos_plano {
            for a in &diff.plano.avisos {
                warn!("aviso do plano: {a:?}");
            }
            self.avisos_plano_unix = agora_unix();
            self.avisos_plano = diff.plano.avisos.clone();
        }
        Ok(diff)
    }

    fn registrar_erros_deriva(&mut self, erros: &[String]) {
        let quando = agora_unix();
        self.erros_deriva = erros
            .iter()
            .map(|e| {
                error!("erro isolado num perfil ao tratar a deriva: {e}");
                Aviso::erro_perfil(e.clone(), true, quando)
            })
            .collect();
    }

    /// Avisos para a UI: estado, snapshot, erros do último ciclo e jogos
    /// ignorados, do mais grave para o menos grave.
    pub fn avisos(&self) -> Vec<Aviso> {
        let mut v = self.avisos_estado.clone();
        if let Some(motivo) = self.engine.armazem().snapshot_indisponivel()
            && !v
                .iter()
                .any(|a| a.codigo == nvt_ipc::codigo_aviso::SNAPSHOT_INDISPONIVEL)
        {
            v.push(Aviso::snapshot_indisponivel(motivo, agora_unix()));
        }
        v.extend(self.erros_deriva.iter().cloned());
        v.extend(self.erros_aplicacao.iter().cloned());
        if !self.ignorados.is_empty() {
            v.push(Aviso::jogos_ignorados(&self.ignorados, self.ignorados_unix));
        }
        v.extend(
            self.avisos_plano
                .iter()
                .map(|a| aviso_do_plano(*a, self.avisos_plano_unix)),
        );
        nvt_ipc::ordenar_avisos(&mut v);
        v
    }

    fn def(&self, setting: &str) -> Option<&DefSetting> {
        self.engine.regras().catalogo.get(setting)
    }

    // ── Vigia do DRS ───────────────────────────────────────────────────────

    /// A mudança no diretório do DRS é provavelmente nossa? (dentro da janela
    /// após a última gravação própria, ou assinatura igual à registrada logo
    /// depois dela).
    fn mudanca_propria(&mut self, agora: Instant) -> bool {
        if self
            .ultima_gravacao
            .is_some_and(|t| agora.saturating_duration_since(t) < self.cfg.janela_propria)
        {
            return true;
        }
        match &self.assinatura_propria {
            Some(a) => (self.assinador)().as_ref() == Some(a),
            None => false,
        }
    }

    /// Notificação do diretório do DRS. Devolve `false` se foi ignorada
    /// (gravação própria).
    pub fn notificar_mudanca_drs(&mut self, agora: Instant) -> bool {
        if self.mudanca_propria(agora) {
            debug!("mudança no DRS ignorada (gravação do próprio serviço)");
            return false;
        }
        self.mudanca_pendente = Some(agora);
        true
    }

    /// Há uma mudança externa aguardando o debounce?
    #[cfg(test)]
    pub fn mudanca_pendente(&self) -> bool {
        self.mudanca_pendente.is_some()
    }

    /// Próxima consulta ao portão com ação pendente (`None` = sem pendência).
    #[cfg(test)]
    pub fn proxima_reavaliacao(&self) -> Option<Instant> {
        self.proxima_reavaliacao
    }

    /// Temporizadores: ação pendente (jogo aberto), debounce do DRS,
    /// verificação e varredura periódicas. Sem pendência, os processos só
    /// são consultados quando há o que gravar.
    pub fn tick(&mut self, agora: Instant) {
        if self.proxima_reavaliacao.is_some_and(|t| agora >= t) {
            self.reavaliar_pendente(agora);
        }
        if let Some(t) = self.mudanca_pendente
            && agora.saturating_duration_since(t) >= self.cfg.debounce
        {
            self.mudanca_pendente = None;
            self.ciclo(agora, "mudança no DRS");
        }
        if agora >= self.proxima_varredura {
            self.varrer(agora);
            self.ciclo(agora, "varredura periódica");
        }
        if agora >= self.proxima_verificacao {
            self.ciclo(agora, "verificação periódica");
        }
    }

    // ── IPC ────────────────────────────────────────────────────────────────

    /// Trata uma requisição (já validada na forma pelo `nvt-ipc`).
    pub fn tratar(&mut self, req: Requisicao, agora: Instant) -> Resposta {
        let grava = matches!(
            req,
            Requisicao::AplicarAgora
                | Requisicao::RestaurarOriginal
                | Requisicao::DefinirPreferencias { .. }
                | Requisicao::TravarSetting { .. }
                | Requisicao::DestravarSetting { .. }
        );
        if grava && !self.pode_gravar() {
            return Resposta::erro(
                CodigoErro::Interno,
                "serviço em modo somente leitura: nenhuma gravação é feita",
            );
        }
        let r = match req {
            Requisicao::ObterEstado => self.obter_estado(),
            Requisicao::ObterPlano => self.obter_plano(),
            Requisicao::AplicarAgora => self.aplicar_agora(agora),
            Requisicao::RestaurarOriginal => self.restaurar_original(agora),
            Requisicao::DefinirPreferencias {
                sincronismo,
                qualidade,
                limite_fps,
            } => self.definir_preferencias(sincronismo, qualidade, limite_fps, agora),
            Requisicao::ListarHistorico { limite } => Ok(self.listar_historico(limite)),
            Requisicao::TravarSetting { perfil, setting } => self.travar(&perfil, &setting),
            Requisicao::DestravarSetting { perfil, setting } => {
                self.destravar(&perfil, &setting, agora)
            }
            Requisicao::ReescanearJogos => {
                self.varrer(agora);
                self.ciclo(agora, "pedido de nova varredura");
                Ok(Resposta::Ok)
            }
        };
        r.unwrap_or_else(|e| {
            error!("requisição IPC falhou: {e}");
            resposta_de_erro(&e)
        })
    }

    fn obter_estado(&mut self) -> Resultado<Resposta> {
        let lido = self.renovar().and_then(|()| {
            let store = self.engine.store();
            let versao = store.versao_driver()?;
            let gpus = store.gpus()?;
            let caps = self.engine.capacidades()?;
            Ok((versao, gpus, caps))
        });
        let (driver, gpu) = match lido {
            Ok((v, gpus, caps)) => (
                Some(nvt_ipc::Driver {
                    versao: v.to_string(),
                    ramo: (!v.branch.is_empty()).then(|| v.branch.clone()),
                }),
                gpus.first().map(|g| nvt_ipc::Gpu {
                    nome: g.name.clone(),
                    arquitetura: caps.arquitetura.to_string(),
                    vram_mb: caps.vram_mb,
                    capacidades: Capacidades {
                        dlss: caps.tem_tensor_cores,
                        geracao_quadros: caps.frame_gen,
                        reflex: !matches!(caps.arquitetura, nvt_core::Arquitetura::Unknown(_)),
                        rebar: caps.rebar_ativo.unwrap_or(caps.rebar_suportado),
                        refresh_mhz: caps.refresh_mhz,
                        vrr: nvt_ipc::Vrr {
                            possivel: caps.vrr.possivel,
                            ativo: caps.vrr.ativo,
                        },
                    },
                }),
            ),
            Err(e) => {
                warn!("estado sem driver: {e}");
                (None, None)
            }
        };
        Ok(Resposta::Estado(nvt_ipc::Estado {
            versao_servico: env!("CARGO_PKG_VERSION").to_owned(),
            driver,
            gpu,
            preferencias: prefs_ipc(&self.engine.estado().preferencias),
            jogos: u32::try_from(self.jogos.len()).unwrap_or(u32::MAX),
            ultima_aplicacao: self.servico.ultima_aplicacao.map(|u| nvt_ipc::Aplicacao {
                quando_unix: u.quando_unix,
                mudancas: u.mudancas,
                falhas: u.falhas,
            }),
            alertas_deriva: self.alertas.clone(),
            avisos: self.avisos(),
            adiamento: self.adiamento_ipc(),
        }))
    }

    fn obter_plano(&mut self) -> Resultado<Resposta> {
        self.renovar()?;
        let diff = self.planejar()?;
        Ok(Resposta::Plano(self.plano_ipc(&diff, None)))
    }

    /// Plano sem aplicar, com a ação pendente (resposta de um pedido adiado).
    fn plano_adiado(&mut self) -> Resultado<Resposta> {
        let diff = self.planejar()?;
        let mut p = self.plano_ipc(&diff, None);
        p.adiada = self.adiamento_ipc();
        Ok(Resposta::Plano(p))
    }

    fn aplicar_agora(&mut self, agora: Instant) -> Resultado<Resposta> {
        if self.restauracao_pendente() {
            // A restauração pendente vence qualquer outra ação (F1-07).
            info!("aplicar agora ignorado: restauração pendente até o jogo fechar");
            self.renovar()?;
            return self.plano_adiado();
        }
        if self.servico.autonomia_pausada {
            info!("autonomia reativada por pedido do usuário (aplicar agora)");
            self.servico.autonomia_pausada = false;
            self.servico.pausada_em_unix = None;
            self.salvar_servico();
        }
        self.renovar()?;
        self.servico.autonomia_pausada = false;
        match self.tratar_e_aplicar(agora, "aplicar agora")? {
            Some((diff, ap)) => {
                self.alertas.clear();
                Ok(Resposta::Plano(self.plano_ipc(&diff, Some(&ap))))
            }
            None => self.plano_adiado(),
        }
    }

    fn restaurar_original(&mut self, agora: Instant) -> Resultado<Resposta> {
        self.renovar()?;
        // Pausa antes de restaurar: se algo falhar no meio, o próximo ciclo
        // não reaplica por cima. Com jogo aberto, a pausa vale já (nada é
        // aplicado por cima da restauração pendente).
        self.servico.autonomia_pausada = true;
        self.servico.pausada_em_unix = Some(agora_unix());
        self.salvar_servico();
        let restaurado = match self.engine.restaurar(true) {
            Err(ErroEngine::Adiada(a)) => {
                self.adiar(AcaoPendente::RestaurarOriginal, a, agora);
                return Ok(Resposta::Restauracao {
                    adiada: self.adiamento_ipc(),
                });
            }
            r => r?,
        };
        self.concluir_pendente(AcaoPendente::RestaurarOriginal);
        match restaurado {
            None => info!("restauração: não há snapshot original (nada a restaurar)"),
            Some(r) => {
                if r.aplicada {
                    self.marcar_gravacao(agora);
                }
                info!(
                    "restauração: {} operação(ões), {} ignorada(s) fora do catálogo; autonomia pausada",
                    r.ops.len(),
                    r.ignoradas.len()
                );
            }
        }
        self.alertas.clear();
        Ok(Resposta::Restauracao { adiada: None })
    }

    fn definir_preferencias(
        &mut self,
        sincronismo: nvt_ipc::Sincronismo,
        qualidade: nvt_ipc::Qualidade,
        limite_fps: Option<u32>,
        agora: Instant,
    ) -> Resultado<Resposta> {
        self.renovar()?;
        let prefs = Preferences {
            sincronismo: match sincronismo {
                nvt_ipc::Sincronismo::Desligado => nvt_core::Sincronismo::SemSync,
                nvt_ipc::Sincronismo::VSync => nvt_core::Sincronismo::VSync,
                nvt_ipc::Sincronismo::GSync => nvt_core::Sincronismo::GSync,
            },
            qualidade: match qualidade {
                nvt_ipc::Qualidade::Desempenho => nvt_core::Qualidade::Desempenho,
                nvt_ipc::Qualidade::Equilibrado => nvt_core::Qualidade::Equilibrado,
                nvt_ipc::Qualidade::Qualidade => nvt_core::Qualidade::Fidelidade,
            },
            limite_fps,
            escolhas_usuario: Vec::new(),
        };
        // Mudança manual pendente vira escolha travada antes do plano novo.
        // Com jogo aberto, as preferências são gravadas no estado e a
        // aplicação fica pendente.
        let autonomia = self.autonomia_ativa() && !self.restauracao_pendente();
        let mut adiada = false;
        if autonomia {
            match self.tratar_deriva_pendente(agora, "preferências", true) {
                Err(ErroEngine::Adiada(a)) => {
                    self.adiar(AcaoPendente::TratarDeriva, a, agora);
                    adiada = true;
                }
                r => r?,
            }
        }
        self.engine.definir_preferencias(prefs)?;
        info!("preferências atualizadas");
        if autonomia && !adiada {
            self.aplicar_ou_adiar(agora)?;
        } else if let Err(e) = self.planejar() {
            // Só para atualizar os avisos do plano (G-SYNC sem VRR).
            warn!("plano após as preferências: {e}");
        }
        Ok(Resposta::Ok)
    }

    /// Aplica o plano; com jogo aberto, deixa `AplicarPlano` pendente.
    fn aplicar_ou_adiar(&mut self, agora: Instant) -> Resultado<()> {
        match self.aplicar_plano(agora) {
            Err(ErroEngine::Adiada(a)) => {
                self.adiar(AcaoPendente::AplicarPlano, a, agora);
                Ok(())
            }
            Err(e) => Err(e),
            Ok(_) => {
                self.concluir_pendente(AcaoPendente::AplicarPlano);
                Ok(())
            }
        }
    }

    /// Alvo e perfil de um nome vindo do IPC, com o nome **canônico** do DRS
    /// (a busca por nome não diferencia maiúsculas: "LEAGUE OF LEGENDS" e
    /// "league of legends" são o mesmo perfil e viram a mesma trava).
    ///
    /// `None` se o perfil não existe ou não é gerenciável: só o global, os
    /// perfis dos jogos detectados, os criados pelo app e os que já têm
    /// setting gerida ou trava.
    fn alvo_de(&self, perfil: &str) -> Resultado<Option<(Alvo, ProfileRef)>> {
        if perfil == PERFIL_GLOBAL {
            return Ok(Some((Alvo::Global, ProfileRef::Base)));
        }
        let Some(nome) = self.engine.store().nome_canonico(perfil)? else {
            return Ok(None);
        };
        let estado = self.engine.estado();
        let gerenciavel = estado.criado_pelo_app(&nome)
            || estado
                .esperado
                .iter()
                .any(|e| matches!(&e.perfil, ProfileRef::Named(n) if mesmo_nome_perfil(n, &nome)))
            || estado
                .travadas
                .iter()
                .any(|t| mesmo_alvo(&t.alvo, &Alvo::Jogo(nome.clone())))
            || self
                .engine
                .resolver_jogos(&self.jogos)?
                .iter()
                .any(|j| mesmo_nome_perfil(&j.perfil, &nome));
        if !gerenciavel {
            return Ok(None);
        }
        Ok(Some((Alvo::Jogo(nome.clone()), ProfileRef::Named(nome))))
    }

    /// Trava como escolha do usuário. Usa o tratamento de mudança pontual do
    /// motor (sai do estado esperado, vira `Travada`, registra no histórico),
    /// sem gravar nada no driver.
    fn travar(&mut self, perfil: &str, setting: &str) -> Resultado<Resposta> {
        self.renovar()?;
        let Some(def) = self.def(setting).cloned() else {
            return Ok(nao_permitida(setting));
        };
        let Some((alvo, pref)) = self.alvo_de(perfil)? else {
            return Ok(perfil_inexistente(perfil));
        };
        let estado = self.engine.estado();
        if estado.travada(&alvo, setting) {
            return Ok(Resposta::Ok);
        }
        if estado.travadas.len() >= MAX_TRAVADAS {
            return Ok(Resposta::erro(
                CodigoErro::Interno,
                format!(
                    "limite de {MAX_TRAVADAS} escolhas travadas atingido: destrave alguma \
                     antes de travar outra"
                ),
            ));
        }
        let atual = self
            .engine
            .store()
            .ler_setting(&pref, def.id)?
            .and_then(|s| s.value.as_u32());
        let esperado = estado
            .esperado
            .iter()
            .find(|e| e.perfil == pref && e.id == def.id)
            .cloned()
            .unwrap_or(Esperado {
                alvo,
                perfil: pref,
                setting: def.nome.clone(),
                id: def.id,
                valor: atual.unwrap_or(0),
                origem: Origem::Preferencia {
                    preferencia: "usuario".into(),
                    regra: None,
                },
                antes_da_gestao: None,
            });
        let deriva = Deriva {
            classe: ClasseDeriva::Pontual,
            divergencias: vec![Divergencia {
                esperado,
                atual,
                voltou_ao_padrao: false,
            }],
            versao_driver: self.engine.store().versao_driver()?.raw,
        };
        self.engine.tratar_deriva(&deriva, true)?;
        info!("setting {setting} travada em {perfil} pelo usuário");
        Ok(Resposta::Ok)
    }

    fn destravar(&mut self, perfil: &str, setting: &str, agora: Instant) -> Resultado<Resposta> {
        self.renovar()?;
        if self.def(setting).is_none() {
            return Ok(nao_permitida(setting));
        }
        let alvo = if perfil == PERFIL_GLOBAL {
            Alvo::Global
        } else {
            Alvo::Jogo(perfil.to_owned())
        };
        let autonomia = self.autonomia_ativa() && !self.restauracao_pendente();
        let mut adiada = false;
        if autonomia && self.engine.estado().travada(&alvo, setting) {
            // Mudança manual pendente vira escolha travada antes do plano novo.
            match self.tratar_deriva_pendente(agora, "destravar", true) {
                Err(ErroEngine::Adiada(a)) => {
                    self.adiar(AcaoPendente::TratarDeriva, a, agora);
                    adiada = true;
                }
                r => r?,
            }
        }
        // `liberar_travada` compara o perfil sem diferenciar maiúsculas.
        if self.engine.liberar_travada(&alvo, setting)? {
            info!("setting {setting} destravada em {perfil}");
            self.alertas
                .retain(|a| !(mesmo_nome_perfil(&a.perfil, perfil) && a.setting == setting));
            if autonomia && !adiada {
                self.aplicar_ou_adiar(agora)?;
            }
        }
        Ok(Resposta::Ok)
    }

    fn listar_historico(&self, limite: u32) -> Resposta {
        let cat = &self.engine.regras().catalogo;
        let hist = &self.engine.estado().historico;
        let entradas = hist
            .iter()
            .enumerate()
            .rev()
            .take(limite as usize)
            .map(|(i, r)| {
                let def = cat.get(&r.setting);
                let acao = match &r.origem {
                    OrigemRegistro::Plano { .. } | OrigemRegistro::CriacaoPerfil => {
                        AcaoHistorico::Aplicacao
                    }
                    OrigemRegistro::Reaplicacao { .. } => AcaoHistorico::DerivaDetectada,
                    OrigemRegistro::Usuario => AcaoHistorico::Travamento,
                    OrigemRegistro::Restauracao => AcaoHistorico::Restauracao,
                };
                EntradaHistorico {
                    id: i as u64 + 1,
                    quando_unix: iso_para_unix(&r.data),
                    acao,
                    perfil: (!r.perfil.is_empty()).then(|| {
                        if r.perfil == nvt_engine::NOME_PERFIL_BASE {
                            PERFIL_GLOBAL.to_owned()
                        } else {
                            r.perfil.clone()
                        }
                    }),
                    setting: (!r.setting.is_empty()).then(|| r.setting.clone()),
                    antes: nomes::valor(def, r.antes),
                    depois: nomes::valor(def, r.depois),
                    descricao: r.motivo.clone(),
                }
            })
            .collect();
        Resposta::Historico { entradas }
    }

    /// Converte o diff (e, se houver, o resultado da aplicação) para o protocolo.
    pub fn plano_ipc(&self, diff: &Diff, ap: Option<&Aplicacao>) -> Plano {
        let regras = self.engine.regras();
        let cat = &regras.catalogo;
        let mut perfis: Vec<PlanoPerfil> = Vec::new();
        let nome_legivel = |alvo: &Alvo| match alvo {
            Alvo::Global => "Global (todos os jogos)".to_owned(),
            Alvo::Jogo(id) => diff
                .jogos
                .iter()
                .find(|j| &j.perfil == id)
                .map_or_else(|| id.clone(), |j| j.entrada.nome.clone()),
        };
        let mut push = |alvo: &Alvo, perfil: String, m: Mudanca| match perfis
            .iter_mut()
            .find(|p| p.perfil == perfil)
        {
            Some(p) => p.mudancas.push(m),
            None => perfis.push(PlanoPerfil {
                perfil,
                nome_legivel: nome_legivel(alvo),
                mudancas: vec![m],
            }),
        };
        let mudanca = |c: &nvt_core::SettingChange, antes: Option<u32>, status| {
            let def = cat.get(&c.setting);
            let regra = c.origem.regra();
            Mudanca {
                setting: c.setting.clone(),
                nome_legivel: def.map_or_else(|| c.setting.clone(), nomes::nome_setting),
                antes: nomes::valor(def, antes),
                depois: nomes::valor(def, Some(c.valor)),
                motivo: c.explicacao.clone(),
                regra: match &c.origem {
                    Origem::Regra { id } => id.clone(),
                    Origem::Preferencia { preferencia, regra } => regra
                        .clone()
                        .unwrap_or_else(|| format!("preferencia:{preferencia}")),
                },
                impacto_visual: regra
                    .and_then(|id| regras.regras.iter().find(|r| r.id == id))
                    .map_or(nvt_ipc::ImpactoVisual::Nenhum, |r| match r.impacto_visual {
                        ImpactoRegra::Nenhum => nvt_ipc::ImpactoVisual::Nenhum,
                        ImpactoRegra::Leve => nvt_ipc::ImpactoVisual::Baixo,
                        ImpactoRegra::Forte => nvt_ipc::ImpactoVisual::Alto,
                    }),
                status,
            }
        };
        for m in &diff.mudancas {
            let status = match ap {
                Some(a) if a.verificadas.contains(m) => StatusMudanca::Aplicada,
                Some(a) if a.falhas.iter().any(|(f, _)| f == m) => StatusMudanca::Falhou,
                _ => StatusMudanca::Pendente,
            };
            push(
                &m.alvo,
                perfil_ipc(&m.perfil),
                mudanca(&m.mudanca, m.mudanca.valor_anterior, status),
            );
        }
        for m in &diff.conformes {
            push(
                &m.alvo,
                perfil_ipc(&m.perfil),
                mudanca(&m.mudanca, Some(m.mudanca.valor), StatusMudanca::JaNoValor),
            );
        }
        for r in &diff.plano.respeitadas {
            let perfil = match &r.alvo {
                Alvo::Global => PERFIL_GLOBAL.to_owned(),
                Alvo::Jogo(id) => id.clone(),
            };
            push(
                &r.alvo,
                perfil,
                mudanca(&r.mudanca, None, StatusMudanca::Travada),
            );
        }
        let reversoes = diff
            .reverter
            .iter()
            .map(|r| {
                let e = &r.esperado;
                let def = cat.get(&e.setting);
                let (acao, original) = match &r.acao {
                    AcaoReversao::Gravar(v) => (
                        nvt_ipc::AcaoReversao::GravarOriginal,
                        nomes::valor(def, v.as_u32()),
                    ),
                    AcaoReversao::Remover { .. } => (nvt_ipc::AcaoReversao::Remover, None),
                    AcaoReversao::SoDeixarDeVigiar => {
                        (nvt_ipc::AcaoReversao::SoDeixarDeVigiar, None)
                    }
                };
                nvt_ipc::Reversao {
                    perfil: perfil_ipc(&e.perfil),
                    nome_perfil: nome_legivel(&e.alvo),
                    setting: e.setting.clone(),
                    nome_legivel: def.map_or_else(|| e.setting.clone(), nomes::nome_setting),
                    atual: nomes::valor(def, r.atual),
                    original,
                    acao,
                    motivo: motivo_reversao(&e.origem, acao),
                    status: match ap {
                        Some(a) if a.revertidas.contains(e) => StatusMudanca::Aplicada,
                        Some(_) => StatusMudanca::Falhou,
                        None => StatusMudanca::Pendente,
                    },
                }
            })
            .collect();
        Plano {
            gerado_unix: agora_unix(),
            perfis,
            reversoes,
            ignorados: diff
                .ignorados
                .iter()
                .map(|j| nvt_ipc::JogoIgnorado {
                    nome: j.nome.clone(),
                    exe: j.exe.clone(),
                    motivo: j.motivo.clone(),
                })
                .collect(),
            jogos: jogos_ipc(diff, regras, self.engine.base_jogos()),
            adiada: None,
        }
    }
}

/// Abre o armazém e acumula os avisos novos em `acumulados` (registrando no
/// log). O estado recuperado ou reiniciado já sai gravado de
/// [`Armazem::abrir`] (que falha se não conseguir gravá-lo); e, se uma
/// gravação se perder, a abertura seguinte recupera de novo de
/// `estado.anterior.json` em vez de começar vazia.
///
/// Em modo somente leitura usa [`Armazem::abrir_somente_leitura`]: nada é
/// renomeado, criado nem gravado no diretório de estado.
fn abrir_armazem(dir: &Path, cfg: &Config, acumulados: &mut Vec<Aviso>) -> Resultado<Armazem> {
    let armazem = if cfg.somente_leitura {
        Armazem::abrir_somente_leitura(dir)?
    } else {
        Armazem::abrir(dir)?
    };
    let novos = avisos_do_armazem(
        armazem.avisos(),
        armazem.snapshot_indisponivel(),
        agora_unix(),
    );
    let estado_recuperado = novos.iter().any(|a| {
        a.codigo == nvt_ipc::codigo_aviso::ESTADO_RECUPERADO
            || a.codigo == nvt_ipc::codigo_aviso::ESTADO_REINICIADO
    });
    for a in novos {
        let repetido = acumulados
            .iter()
            .any(|x| x.codigo == a.codigo && x.detalhe == a.detalhe);
        if repetido {
            continue;
        }
        let detalhe = a.detalhe.as_deref().unwrap_or("");
        match a.nivel {
            NivelAviso::Erro => error!("estado: {} {detalhe}", a.mensagem),
            NivelAviso::Atencao => warn!("estado: {} {detalhe}", a.mensagem),
            NivelAviso::Info => info!("estado: {} {detalhe}", a.mensagem),
        }
        acumulados.push(a);
    }
    if estado_recuperado && !armazem.somente_leitura() {
        info!("estado recuperado gravado em estado.json");
    }
    Ok(armazem)
}

/// Código do protocolo IPC para um [`CodigoAviso`] do motor (correspondência
/// exaustiva: um código novo no motor não compila sem o seu par no protocolo).
pub fn codigo_ipc(c: CodigoAviso) -> &'static str {
    use nvt_ipc::codigo_aviso as ipc;
    match c {
        CodigoAviso::EstadoCorrompido => ipc::ESTADO_CORROMPIDO,
        CodigoAviso::EstadoRecuperado => ipc::ESTADO_RECUPERADO,
        CodigoAviso::EstadoReiniciado => ipc::ESTADO_REINICIADO,
        CodigoAviso::SnapshotIndisponivel => ipc::SNAPSHOT_INDISPONIVEL,
        // Ainda sem constante em `nvt_ipc::codigo_aviso`: o texto estável é o
        // do motor (a UI trata código desconhecido pelo nível e pela mensagem).
        CodigoAviso::SnapshotOriginalPerdido => CodigoAviso::SnapshotOriginalPerdido.como_str(),
    }
}

/// Nível do protocolo IPC para um [`nvt_engine::NivelAviso`].
pub fn nivel_ipc(n: nvt_engine::NivelAviso) -> NivelAviso {
    match n {
        nvt_engine::NivelAviso::Info => NivelAviso::Info,
        nvt_engine::NivelAviso::Atencao => NivelAviso::Atencao,
        nvt_engine::NivelAviso::Erro => NivelAviso::Erro,
    }
}

/// Converte os avisos estruturados do [`Armazem`] em avisos do protocolo,
/// pelo código (nunca pelo texto). O snapshot indisponível sem aviso
/// correspondente também vira [`Aviso::snapshot_indisponivel`].
pub fn avisos_do_armazem(
    avisos: &[nvt_engine::Aviso],
    snapshot_indisponivel: Option<&str>,
    quando: u64,
) -> Vec<Aviso> {
    let mut v: Vec<Aviso> = avisos
        .iter()
        .map(|a| {
            Aviso::novo(
                nivel_ipc(a.nivel),
                codigo_ipc(a.codigo),
                a.mensagem.clone(),
                a.detalhe.clone(),
                quando,
            )
        })
        .collect();
    if let Some(m) = snapshot_indisponivel
        && !v
            .iter()
            .any(|a| a.codigo == nvt_ipc::codigo_aviso::SNAPSHOT_INDISPONIVEL)
    {
        v.push(Aviso::snapshot_indisponivel(m, quando));
    }
    v
}

/// Por que uma setting gerida saiu do plano (para exibição).
pub fn motivo_reversao(origem: &Origem, acao: nvt_ipc::AcaoReversao) -> String {
    let causa = match origem {
        Origem::Regra { id } => format!("a regra “{id}” não se aplica mais"),
        Origem::Preferencia {
            preferencia,
            regra: None,
        } => format!("a preferência “{preferencia}” mudou"),
        Origem::Preferencia {
            preferencia,
            regra: Some(r),
        } => format!("a preferência “{preferencia}” (regra “{r}”) mudou"),
    };
    let efeito = match acao {
        nvt_ipc::AcaoReversao::GravarOriginal => "volta ao valor original",
        nvt_ipc::AcaoReversao::Remover => {
            "sai do perfil (volta ao predefinido da NVIDIA, se houver)"
        }
        nvt_ipc::AcaoReversao::SoDeixarDeVigiar => {
            "o driver já está como no original; só deixa de ser vigiada"
        }
    };
    format!("Saiu do plano porque {causa}: {efeito}.")
}

/// Nome do perfil no protocolo (`"global"` para o perfil base).
pub fn perfil_ipc(r: &ProfileRef) -> String {
    match r {
        ProfileRef::Base => PERFIL_GLOBAL.to_owned(),
        ProfileRef::Named(n) => n.clone(),
    }
}

/// Preferências do motor no formato do protocolo.
pub fn prefs_ipc(p: &Preferences) -> nvt_ipc::Preferencias {
    nvt_ipc::Preferencias {
        sincronismo: match p.sincronismo {
            nvt_core::Sincronismo::SemSync => nvt_ipc::Sincronismo::Desligado,
            nvt_core::Sincronismo::VSync => nvt_ipc::Sincronismo::VSync,
            nvt_core::Sincronismo::GSync => nvt_ipc::Sincronismo::GSync,
        },
        qualidade: match p.qualidade {
            nvt_core::Qualidade::Desempenho => nvt_ipc::Qualidade::Desempenho,
            nvt_core::Qualidade::Equilibrado => nvt_ipc::Qualidade::Equilibrado,
            nvt_core::Qualidade::Fidelidade => nvt_ipc::Qualidade::Qualidade,
        },
        limite_fps: p.limite_fps,
    }
}

/// Aviso do plano (F1-15) como aviso do protocolo.
pub fn aviso_do_plano(a: AvisoPlano, quando: u64) -> Aviso {
    match a {
        AvisoPlano::GsyncSemVrr => Aviso::gsync_sem_vrr(quando),
        AvisoPlano::VrrDesligadoNoPainel => Aviso::vrr_desligado_no_painel(quando),
    }
}

/// Jogos do diff (fatos, perfil e explicação) no formato do protocolo.
/// Jogo casado na base curada: Reflex e anticheat vêm da base (como no
/// avaliador, `nvt_engine::fatos_do_jogo`). O modo de visualização da UI faz
/// a mesma conversão (`nvt-ui`, `local::jogos_ipc`).
pub fn jogos_ipc(
    diff: &Diff,
    regras: &nvt_core::ConjuntoRegras,
    base: &BaseJogos,
) -> Vec<nvt_ipc::Jogo> {
    use nvt_core::MotivoNaoAplicada as M;
    use nvt_ipc::MotivoNaoAplicada as I;
    let descricao = |id: &str| {
        regras
            .regras
            .iter()
            .find(|r| r.id == id)
            .map_or_else(String::new, |r| r.descricao.clone())
    };
    let qualidade = |q: nvt_core::Qualidade| {
        prefs_ipc(&Preferences {
            qualidade: q,
            ..Default::default()
        })
        .qualidade
    };
    let motivo = |m: &M| match m {
        M::JogoSemDlss => I::JogoSemDlss,
        M::ArquiteturaNaoPrecisa { arquitetura } => I::ArquiteturaNaoPrecisa {
            arquitetura: arquitetura.nome_regra().to_owned(),
        },
        M::SemTensorCores => I::SemTensorCores,
        M::Experimental => I::Experimental,
        M::BloqueadaPeloTetoDeQualidade { teto, .. } => I::BloqueadaPeloTetoDeQualidade {
            teto: qualidade(*teto),
        },
        M::EscolhaDoUsuario { setting } => I::EscolhaDoUsuario {
            setting: setting.clone(),
        },
        M::ConflitoComRegraMaisEspecifica { regra } => I::ConflitoComRegraMaisEspecifica {
            regra: regra.clone(),
        },
        M::FatoDesconhecido { fato } => I::FatoDesconhecido { fato: fato.clone() },
        M::CondicaoNaoAtendida {
            condicao,
            esperado,
            encontrado,
        } if condicao == "vrr_ativo" && esperado == "true" && encontrado == "false" => {
            I::VrrInativo
        }
        M::CondicaoNaoAtendida {
            condicao,
            esperado,
            encontrado,
        } if condicao == "jogo_reflex" && esperado == "false" && encontrado == "true" => {
            I::JogoComReflex
        }
        M::CondicaoNaoAtendida {
            condicao,
            esperado,
            encontrado,
        } => I::CondicaoNaoAtendida {
            condicao: condicao.clone(),
            esperado: esperado.clone(),
            encontrado: encontrado.clone(),
        },
    };
    diff.jogos
        .iter()
        .map(|j| {
            let r = diff
                .plano
                .resumos
                .get(&j.perfil)
                .cloned()
                .unwrap_or_default();
            let curado = casar_na_base(base, &j.entrada);
            nvt_ipc::Jogo {
                nome: j.entrada.nome.clone(),
                perfil: j.perfil.clone(),
                origem_perfil: match j.origem {
                    nvt_engine::OrigemPerfil::Driver => nvt_ipc::OrigemPerfil::Driver,
                    nvt_engine::OrigemPerfil::Usuario => nvt_ipc::OrigemPerfil::Usuario,
                    nvt_engine::OrigemPerfil::CriadoPeloApp => nvt_ipc::OrigemPerfil::CriadoPeloApp,
                    nvt_engine::OrigemPerfil::ACriar => nvt_ipc::OrigemPerfil::ACriar,
                },
                launcher: j.entrada.launcher.clone(),
                api: j.entrada.api.nome_regra().to_owned(),
                dlss: j.entrada.dlss.map(|v| {
                    if v == nvt_core::Versao::default() {
                        String::new()
                    } else {
                        v.to_string()
                    }
                }),
                reflex: curado.map_or(j.entrada.reflex, |c| Some(c.reflex)),
                anticheat_kernel: curado
                    .map_or(j.entrada.anticheat_kernel, |c| Some(c.anticheat_kernel)),
                competitivo: curado.map(|c| c.id.as_str().to_owned()),
                em_execucao: j.entrada.em_execucao,
                orientacoes: r
                    .orientacoes
                    .iter()
                    .map(|o| o.codigo().to_owned())
                    .collect(),
                resumo: nvt_ipc::ResumoJogo {
                    aplicadas: r
                        .aplicadas
                        .iter()
                        .map(|a| nvt_ipc::RegraAplicada {
                            regra: a.regra.clone(),
                            descricao: descricao(&a.regra),
                            settings: a.settings.clone(),
                        })
                        .collect(),
                    nao_aplicadas: r
                        .nao_aplicadas
                        .iter()
                        .map(|n| nvt_ipc::RegraNaoAplicada {
                            regra: n.regra.clone(),
                            descricao: descricao(&n.regra),
                            motivo: motivo(&n.motivo),
                        })
                        .collect(),
                    herda_do_global: r.herda_do_global,
                },
            }
        })
        .collect()
}

fn nao_permitida(setting: &str) -> Resposta {
    Resposta::erro(
        CodigoErro::SettingNaoPermitida,
        format!("setting \"{setting}\" fora da lista permitida"),
    )
}

fn perfil_inexistente(perfil: &str) -> Resposta {
    Resposta::erro(
        CodigoErro::PerfilNaoEncontrado,
        format!("perfil \"{perfil}\" não existe no driver"),
    )
}

/// Erro do motor → resposta do protocolo.
pub fn resposta_de_erro(e: &ErroEngine) -> Resposta {
    let codigo = match e {
        ErroEngine::Drs(nvt_drs::Error::Nvapi {
            status: nvt_drs::Status::ProfileNotFound,
            ..
        }) => CodigoErro::PerfilNaoEncontrado,
        ErroEngine::Drs(_) | ErroEngine::SemGpu => CodigoErro::DriverIndisponivel,
        ErroEngine::IdNaoPermitido { .. } | ErroEngine::ValorNaoPermitido { .. } => {
            CodigoErro::SettingNaoPermitida
        }
        ErroEngine::Regras(_)
        | ErroEngine::Io(_)
        | ErroEngine::Json(_)
        | ErroEngine::SomenteLeitura => CodigoErro::Interno,
        // O núcleo converte o adiamento em ação pendente antes de chegar
        // aqui; se escapar, a UI vê "ocupado" (tente depois), não falha.
        ErroEngine::Adiada(_) => CodigoErro::Ocupado,
    };
    Resposta::erro(codigo, e.to_string())
}

#[cfg(test)]
mod testes {
    use super::iso_para_unix;

    #[test]
    fn iso_ida_e_volta() {
        assert_eq!(iso_para_unix("1970-01-01T00:00:00Z"), 0);
        assert_eq!(iso_para_unix("2025-11-25T13:45:00Z"), 1_764_078_300);
        assert_eq!(iso_para_unix("lixo"), 0);
    }
}
