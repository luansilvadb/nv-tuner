//! Estado da interface, independente da janela (testável).
//!
//! A thread de comunicação produz [`Evento`]s; [`EstadoUi::aplicar`] os
//! incorpora. A renderização só lê o estado e emite [`Comando`]s.

use nvt_ipc::{
    AlertaDeriva, Aviso, EntradaHistorico, Estado, ImpactoVisual, JogoIgnorado, Plano,
    Preferencias, Qualidade, Reversao, Sincronismo, StatusMudanca,
};

use crate::textos;

/// Telas da navegação lateral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tela {
    VisaoGeral,
    Jogos,
    Mudancas,
    Preferencias,
    Historico,
    Restaurar,
}

impl Tela {
    pub const TODAS: [Tela; 6] = [
        Tela::VisaoGeral,
        Tela::Jogos,
        Tela::Mudancas,
        Tela::Preferencias,
        Tela::Historico,
        Tela::Restaurar,
    ];

    pub fn titulo(self) -> &'static str {
        match self {
            Tela::VisaoGeral => textos::TELA_VISAO_GERAL,
            Tela::Jogos => textos::TELA_JOGOS,
            Tela::Mudancas => textos::TELA_MUDANCAS,
            Tela::Preferencias => textos::TELA_PREFERENCIAS,
            Tela::Historico => textos::TELA_HISTORICO,
            Tela::Restaurar => textos::TELA_RESTAURAR,
        }
    }

    /// Identificador usado na linha de comando (`--tela`) e nas capturas.
    pub fn chave(self) -> &'static str {
        match self {
            Tela::VisaoGeral => "visao-geral",
            Tela::Jogos => "jogos",
            Tela::Mudancas => "mudancas",
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

/// Jogo detectado, pronto para exibição.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JogoInfo {
    pub nome: String,
    pub launcher: String,
    pub api: String,
    /// Versão do DLSS (`Some("")` = tem DLSS, versão desconhecida).
    pub dlss: Option<String>,
    pub reflex: Option<bool>,
    pub anticheat_kernel: Option<bool>,
    /// Perfil do driver usado pelo jogo.
    pub perfil: String,
    pub origem_perfil: &'static str,
    /// O que o app aplicou no perfil (“setting: valor”), segundo o estado salvo.
    pub aplicado: Vec<String>,
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
    /// Regra experimental: sugestão, nunca aplicada automaticamente.
    pub experimental: bool,
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
            if i.experimental {
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
    pub grupos: Vec<GrupoMudancas>,
    pub respeitadas: Vec<Respeitada>,
    pub historico: Vec<EntradaHistorico>,
    /// Settings que saíram do plano e voltam ao original.
    pub reversoes: Vec<Reversao>,
    /// Jogos deixados de fora do plano.
    pub ignorados: Vec<JogoIgnorado>,
}

/// Ações que o usuário pode disparar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comando {
    Atualizar,
    AplicarAgora,
    RestaurarOriginal,
    Reescanear,
    DefinirPreferencias(Preferencias),
    Destravar { perfil: String, setting: String },
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
        }
    }
}

/// Tipo de mensagem mostrada no topo da tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoMensagem {
    Sucesso,
    Erro,
}

/// Evento produzido pela thread de comunicação.
#[derive(Debug, Clone)]
pub enum Evento {
    /// O serviço respondeu; dados completos.
    Servico(Dados),
    /// Serviço ausente; dados locais somente leitura (ou erro ao calculá-los).
    Visualizacao(Result<Dados, String>),
    /// Jogos detectados localmente (vale nos dois modos).
    Jogos(Vec<JogoInfo>),
    /// Serviço com versão de protocolo diferente (mensagem com a instrução
    /// de atualizar) ou `None` quando deixa de valer.
    Incompativel(Option<String>),
    /// Uma operação começou.
    Ocupado(Comando),
    /// Uma operação terminou.
    Concluido {
        comando: Comando,
        resultado: Result<(), String>,
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
            .filter(|i| !i.experimental)
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
    pub jogos: Option<Vec<JogoInfo>>,
    /// Erro ao montar o modo de visualização local.
    pub erro_local: Option<String>,
    /// Serviço de outra versão do protocolo: instrução para atualizar.
    pub incompativel: Option<String>,
    pub mensagem: Option<(TipoMensagem, String)>,
    pub ocupado: Option<Comando>,
    /// Preferências em edição (ainda não enviadas).
    pub rascunho: Preferencias,
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
            jogos: None,
            erro_local: None,
            incompativel: None,
            mensagem: None,
            ocupado: None,
            rascunho: preferencias_padrao(),
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
        self.conexao == Conexao::Servico && self.ocupado.is_none()
    }

    pub fn pode_executar(&self, c: &Comando) -> bool {
        if c.escreve() {
            self.pode_escrever()
        } else {
            self.ocupado.is_none()
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

    /// O botão “Restaurar original” pode ser usado?
    pub fn pode_restaurar(&self) -> bool {
        self.pode_escrever() && self.restauracao_bloqueada().is_none()
    }

    /// Jogos com o que foi aplicado em cada perfil. Com o serviço, o plano
    /// dele (mudanças aplicadas ou já no valor) prevalece sobre o estado local.
    pub fn jogos(&self) -> Vec<JogoInfo> {
        let mut jogos = self.jogos.clone().unwrap_or_default();
        if self.conexao == Conexao::Servico {
            for j in &mut jogos {
                j.aplicado = aplicado_no_perfil(&self.dados.grupos, &j.perfil);
            }
        }
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
                    Err(e) => self.erro_local = Some(e),
                }
                if rascunho_limpo {
                    self.rascunho = self.preferencias_atuais();
                }
            }
            Evento::Jogos(j) => self.jogos = Some(j),
            Evento::Incompativel(m) => self.incompativel = m,
            Evento::Ocupado(c) => {
                self.ocupado = Some(c);
                self.mensagem = None;
            }
            Evento::Concluido { comando, resultado } => {
                self.ocupado = None;
                self.mensagem = match resultado {
                    Ok(()) => comando
                        .mensagem_sucesso()
                        .map(|m| (TipoMensagem::Sucesso, m.to_string())),
                    Err(e) => Some((TipoMensagem::Erro, e)),
                };
                if matches!(comando, Comando::DefinirPreferencias(_)) {
                    self.rascunho = self.preferencias_atuais();
                }
            }
        }
    }
}

/// O que o serviço aplicou (ou já encontrou no valor) num perfil.
pub fn aplicado_no_perfil(grupos: &[GrupoMudancas], perfil: &str) -> Vec<String> {
    grupos
        .iter()
        .filter(|g| g.perfil.eq_ignore_ascii_case(perfil))
        .flat_map(|g| &g.itens)
        .filter(|i| {
            !i.experimental
                && matches!(i.status, StatusMudanca::Aplicada | StatusMudanca::JaNoValor)
        })
        .map(|i| {
            format!(
                "{}: {}",
                i.nome_legivel,
                i.depois.as_deref().unwrap_or(textos::NAO_DEFINIDO)
            )
        })
        .collect()
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
                itens.push(ItemMudanca {
                    setting: m.setting.clone(),
                    nome_legivel: m.nome_legivel.clone(),
                    antes: m.antes.clone(),
                    depois: m.depois.clone(),
                    motivo: m.motivo.clone(),
                    regra: m.regra.clone(),
                    impacto: m.impacto_visual,
                    status: m.status,
                    experimental: false,
                });
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
    use nvt_ipc::{CodigoErro, Mudanca, PlanoPerfil};

    fn mudanca(setting: &str, status: StatusMudanca) -> Mudanca {
        Mudanca {
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
        }
    }

    fn estado(prefs: Preferencias) -> Estado {
        Estado {
            versao_servico: "0.0.1".into(),
            driver: None,
            gpu: None,
            preferencias: prefs,
            jogos: 0,
            ultima_aplicacao: None,
            alertas_deriva: vec![],
            avisos: vec![],
        }
    }

    #[test]
    fn telas_por_chave() {
        for t in Tela::TODAS {
            assert_eq!(Tela::de_chave(t.chave()), Some(t));
            assert!(!t.titulo().is_empty());
        }
        assert_eq!(Tela::de_chave("inexistente"), None);
        assert_eq!(Tela::Mudancas.titulo(), "Mudanças");
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
            i.experimental = true;
        }
        let c = Conformidade::de(&g, 0);
        assert_eq!((c.pendentes, c.conformes), (0, 1));
    }

    #[test]
    fn aplicado_no_perfil_usa_aplicadas_e_ja_no_valor() {
        let (g, _) = grupos_do_plano(&plano());
        assert_eq!(
            aplicado_no_perfil(&g, "wardogs"),
            vec!["Nome dlss_preset: Ligado".to_string()]
        );
        assert!(aplicado_no_perfil(&g, "outro").is_empty());
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
            resultado: Ok(()),
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
            resultado: Ok(()),
        });
        assert!(!ui.rascunho_alterado());
    }

    #[test]
    fn jogos_com_servico_usam_o_plano() {
        let mut ui = EstadoUi::new(Tela::Jogos);
        let jogo = JogoInfo {
            nome: "WARDOGS".into(),
            launcher: "Steam".into(),
            api: "DirectX 12".into(),
            dlss: Some("3.7".into()),
            reflex: Some(true),
            anticheat_kernel: None,
            perfil: "WARDOGS".into(),
            origem_perfil: textos::PERFIL_USUARIO,
            aplicado: vec!["local".into()],
        };
        ui.aplicar(Evento::Jogos(vec![jogo]));
        ui.aplicar(Evento::Visualizacao(Ok(Dados::default())));
        assert_eq!(ui.jogos()[0].aplicado, vec!["local".to_string()]);
        let (grupos, respeitadas) = grupos_do_plano(&plano());
        ui.aplicar(Evento::Servico(Dados {
            grupos,
            respeitadas,
            ..Default::default()
        }));
        assert_eq!(
            ui.jogos()[0].aplicado,
            vec!["Nome dlss_preset: Ligado".to_string()]
        );
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
