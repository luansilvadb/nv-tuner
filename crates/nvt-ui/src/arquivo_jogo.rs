//! “Arquivo do jogo” (Fase 3): textos e modelo da seção do cartão do jogo.
//!
//! Só lê a projeção `Jogo.config_jogo` (IPC v7) e emite comandos; a UI não
//! grava nada. Caminhos chegam já anonimizados (`%USERPROFILE%\…`).

use nvt_ipc::{
    ArquivoJogoUi, DestinoJogoIpc, EstadoBackup, EstadoExperimental, Jogo, MotivoNaoAplicada,
    MudancaArquivoJogo, OrientacaoConfigJogo, SugestaoExperimental, ValorJogoIpc,
};

use crate::textos;

pub const SECAO_ARQUIVO_JOGO: &str = "Arquivo do jogo";
pub const ROTULO_CONTA: &str = "Conta";
pub const ROTULO_BACKUP: &str = "Backup";
pub const SECAO_SUGESTOES_JOGO: &str = "Sugestões experimentais deste jogo";
pub const SEM_MUDANCAS_ARQUIVO: &str = "Nenhuma chave alterada neste arquivo.";
pub const REVERTER_INDISPONIVEL: &str =
    "Reversão indisponível: o serviço não informou a conta deste arquivo.";
pub const OK_REVERSAO_JOGO: &str = "Reversão do arquivo do jogo solicitada.";

/// Estado do backup `.orig`.
pub fn backup(b: EstadoBackup) -> &'static str {
    match b {
        EstadoBackup::SemBackup => "sem backup (nada foi gravado ainda)",
        EstadoBackup::Integro => "original guardado e íntegro",
        EstadoBackup::Comprometido => {
            "backup ausente ou alterado: o app não grava mais neste arquivo"
        }
    }
}

/// Valor de uma chave, para “antes → depois”.
pub fn valor(v: &ValorJogoIpc) -> String {
    match v {
        ValorJogoIpc::Ausente => "ausente".into(),
        ValorJogoIpc::Inteiro(n) => n.to_string(),
        ValorJogoIpc::Booleano(b) => if *b { textos::SIM } else { textos::NAO }.into(),
        ValorJogoIpc::Texto(t) => format!("“{t}”"),
        ValorJogoIpc::TokenLancamento(Some(n)) => format!("+fps_max {n}"),
        ValorJogoIpc::TokenLancamento(None) => "sem +fps_max".into(),
    }
}

/// Linha de uma chave: nome, antes → depois e situação.
pub fn linha_mudanca(m: &MudancaArquivoJogo) -> String {
    let base = format!(
        "{}: {} → {} ({})",
        textos::setting_legivel(&m.setting),
        valor(&m.antes),
        valor(&m.depois),
        textos::status_mudanca(m.status)
    );
    match &m.motivo {
        Some(mo) => format!("{base}: {}", motivo_curto(mo)),
        None => base,
    }
}

/// Nome do arquivo (último componente do rótulo).
pub fn nome_arquivo(rotulo: &str) -> &str {
    rotulo.rsplit('\\').next().unwrap_or(rotulo)
}

fn lista(processos: &[String]) -> String {
    processos.join(", ")
}

fn exige_steam(processos: &[String]) -> bool {
    processos
        .iter()
        .any(|p| p.eq_ignore_ascii_case("steam.exe"))
}

/// Texto de uma orientação; `arquivo` é o nome do arquivo envolvido, se houver.
pub fn orientacao(o: &OrientacaoConfigJogo, arquivo: Option<&str>) -> String {
    match o {
        OrientacaoConfigJogo::FecharSteam => "Feche a Steam para aplicar.".into(),
        OrientacaoConfigJogo::FecharJogo { processos } if processos.is_empty() => {
            "Feche o jogo para aplicar.".into()
        }
        OrientacaoConfigJogo::FecharJogo { processos } => {
            format!("Feche o jogo para aplicar ({}).", lista(processos))
        }
        OrientacaoConfigJogo::DesmarcarSomenteLeitura => match arquivo {
            Some(a) => format!("Desmarque Somente leitura em {a}."),
            None => "Desmarque Somente leitura no arquivo do jogo.".into(),
        },
        OrientacaoConfigJogo::AjustarNoJogo => {
            "O arquivo é sincronizado na nuvem: faça o ajuste no menu do jogo.".into()
        }
        OrientacaoConfigJogo::EscolherConta => {
            "Mais de uma conta encontrada: abra o jogo com a conta que você usa.".into()
        }
    }
}

/// Arquivo a citar em “Desmarque Somente leitura em …”: o que relatou o
/// motivo; na falta, o único arquivo do jogo.
pub fn arquivo_somente_leitura(arquivos: &[ArquivoJogoUi]) -> Option<&str> {
    arquivos
        .iter()
        .find(|a| {
            a.mudancas
                .iter()
                .any(|m| m.motivo == Some(MotivoNaoAplicada::ArquivoSomenteLeitura))
        })
        .or(match arquivos {
            [a] => Some(a),
            _ => None,
        })
        .map(|a| nome_arquivo(&a.rotulo.0))
}

/// Textos de todas as orientações do jogo.
pub fn orientacoes(j: &Jogo) -> Vec<String> {
    let Some(c) = &j.config_jogo else {
        return vec![];
    };
    let ro = arquivo_somente_leitura(&c.arquivos);
    c.orientacoes.iter().map(|o| orientacao(o, ro)).collect()
}

/// Motivo da Fase 3 em poucas palavras; `None` fora da família.
pub fn motivo_curto(m: &MotivoNaoAplicada) -> String {
    motivo_curto_jogo(m).unwrap_or_else(|| textos::motivo_curto(m))
}

pub fn motivo_curto_jogo(m: &MotivoNaoAplicada) -> Option<String> {
    Some(match m {
        MotivoNaoAplicada::ConfigPolitica { .. } => {
            "este jogo não permite ajuste pelo arquivo: faça no menu do jogo".into()
        }
        MotivoNaoAplicada::JanelaFechadaExigida { processos } if exige_steam(processos) => {
            "Feche a Steam para aplicar".into()
        }
        MotivoNaoAplicada::JanelaFechadaExigida { .. } => "Feche o jogo para aplicar".into(),
        MotivoNaoAplicada::ArquivoNaNuvem => {
            "arquivo sincronizado na nuvem: ajuste no menu do jogo".into()
        }
        MotivoNaoAplicada::ArquivoSomenteLeitura => "arquivo marcado como somente leitura".into(),
        MotivoNaoAplicada::FormatoInesperado => "formato do arquivo não reconhecido".into(),
        MotivoNaoAplicada::ArquivoAlteradoPeloJogo => {
            "o valor foi mudado no jogo; o app respeita".into()
        }
        MotivoNaoAplicada::ContaAmbigua => {
            "Mais de uma conta encontrada: abra o jogo com a conta que você usa".into()
        }
        MotivoNaoAplicada::ArquivoAusente => "arquivo do jogo não encontrado".into(),
        MotivoNaoAplicada::ReparseRecusado => "caminho com link ou junção: recusado".into(),
        _ => return None,
    })
}

/// Motivo da Fase 3 por extenso; `None` fora da família.
pub fn motivo_longo_jogo(m: &MotivoNaoAplicada) -> Option<String> {
    Some(match m {
        MotivoNaoAplicada::ConfigPolitica { politica } => format!(
            "A base de jogos marca este jogo como “{}”: o app não edita os arquivos dele. \
             Faça o ajuste no menu do próprio jogo.",
            politica_legivel(politica)
        ),
        MotivoNaoAplicada::JanelaFechadaExigida { processos } if exige_steam(processos) => {
            format!(
                "Feche a Steam para aplicar: ela regrava este arquivo enquanto está aberta \
                 ({}). O app aplica sozinho quando a Steam estiver fechada.",
                lista(processos)
            )
        }
        MotivoNaoAplicada::JanelaFechadaExigida { processos } => format!(
            "Feche o jogo para aplicar ({}). O app só grava com o jogo fechado e aplica \
             sozinho na próxima vez.",
            lista(processos)
        ),
        MotivoNaoAplicada::ArquivoNaNuvem => "O arquivo usado pelo jogo é o sincronizado na \
            nuvem; uma edição local seria desfeita. Faça o ajuste no menu do jogo."
            .into(),
        MotivoNaoAplicada::ArquivoSomenteLeitura => "O arquivo está marcado como Somente \
            leitura. Desmarque o atributo nas propriedades do arquivo para o app aplicar; o \
            app nunca muda esse atributo."
            .into(),
        MotivoNaoAplicada::FormatoInesperado => "O conteúdo do arquivo não está no formato \
            esperado (codificação, sintaxe ou chave ausente). Nada foi gravado."
            .into(),
        MotivoNaoAplicada::ArquivoAlteradoPeloJogo => "O valor foi mudado no jogo depois do \
            ajuste do app. O app respeita a sua escolha e não grava de novo."
            .into(),
        MotivoNaoAplicada::ContaAmbigua => "Mais de uma conta encontrada: abra o jogo com a \
            conta que você usa, para o app saber qual arquivo ajustar."
            .into(),
        MotivoNaoAplicada::ArquivoAusente => "O arquivo de configuração do jogo não foi \
            encontrado. Abra o jogo uma vez para ele criar o arquivo."
            .into(),
        MotivoNaoAplicada::ReparseRecusado => "O caminho do arquivo passa por um link ou \
            junção; por segurança, o app não grava nele."
            .into(),
        _ => return None,
    })
}

fn politica_legivel(p: &str) -> &str {
    match p {
        "orientar" => "só orientar",
        "proibido" => "sem edição",
        outro => outro,
    }
}

/// Destino de “Reverter” de uma chave. A projeção v7 não traz a instância
/// opaca; usa-se `conta` quando ela já está na forma canônica de instância.
pub fn destino_reverter(a: &ArquivoJogoUi, setting: &str) -> Option<DestinoJogoIpc> {
    if !a.reversivel {
        return None;
    }
    let d = DestinoJogoIpc {
        alvo: a.alvo.clone(),
        setting: setting.to_owned(),
        instancia: a.conta.clone()?,
    };
    d.validar().is_ok().then_some(d)
}

/// Sugestões experimentais de config deste jogo (`jogo-<id>-…`), com
/// botão Aceitar/Retirar (mesmo fluxo da Fase 2).
pub fn sugestoes_do_jogo<'a>(
    j: &Jogo,
    sugestoes: &'a [SugestaoExperimental],
) -> Vec<&'a SugestaoExperimental> {
    let Some(id) = &j.competitivo else {
        return vec![];
    };
    let prefixo = format!("jogo-{id}-");
    sugestoes
        .iter()
        .filter(|s| s.regra.starts_with(&prefixo) && s.estado != EstadoExperimental::Estavel)
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;
    use nvt_ipc::AlvoIpc;
    use nvt_ipc::{
        Camada, ConfigJogoUi, ImpactoVisual, OrigemPerfil, ResumoJogo, RotuloCaminho, StatusMudanca,
    };

    fn todos_motivos() -> Vec<MotivoNaoAplicada> {
        vec![
            MotivoNaoAplicada::ConfigPolitica {
                politica: "orientar".into(),
            },
            MotivoNaoAplicada::JanelaFechadaExigida {
                processos: vec!["steam.exe".into()],
            },
            MotivoNaoAplicada::JanelaFechadaExigida {
                processos: vec!["RainbowSix.exe".into()],
            },
            MotivoNaoAplicada::ArquivoNaNuvem,
            MotivoNaoAplicada::ArquivoSomenteLeitura,
            MotivoNaoAplicada::FormatoInesperado,
            MotivoNaoAplicada::ArquivoAlteradoPeloJogo,
            MotivoNaoAplicada::ContaAmbigua,
            MotivoNaoAplicada::ArquivoAusente,
            MotivoNaoAplicada::ReparseRecusado,
        ]
    }

    fn sem_emoji(t: &str) {
        assert!(
            t.chars().all(|c| (c as u32) < 0x2190 || "→“”…".contains(c)),
            "caractere fora do esperado: {t}"
        );
    }

    #[test]
    fn todo_motivo_da_fase3_tem_texto_proprio() {
        let generico_curto = "condições da regra não atendidas";
        let generico_longo = "Condições da regra não atendidas.";
        for m in todos_motivos() {
            assert!(m.e_config_jogo());
            let c = textos::motivo_curto(&m);
            let l = textos::motivo_longo(&m);
            assert_ne!(c, generico_curto, "{m:?}");
            assert_ne!(l, generico_longo, "{m:?}");
            assert!(l.ends_with('.'), "{l}");
            assert!(!l.to_lowercase().contains("anticheat"), "{l}");
            sem_emoji(&c);
            sem_emoji(&l);
        }
        assert_eq!(
            textos::motivo_curto(&MotivoNaoAplicada::JanelaFechadaExigida {
                processos: vec!["steam.exe".into()]
            }),
            "Feche a Steam para aplicar"
        );
        assert_eq!(
            textos::motivo_curto(&MotivoNaoAplicada::JanelaFechadaExigida {
                processos: vec!["cs2.exe".into()]
            }),
            "Feche o jogo para aplicar"
        );
        // Fora da família: continua com os textos da Fase 2.
        assert_eq!(motivo_curto_jogo(&MotivoNaoAplicada::JogoSemDlss), None);
    }

    #[test]
    fn toda_orientacao_tem_texto_proprio() {
        let todas = [
            OrientacaoConfigJogo::FecharSteam,
            OrientacaoConfigJogo::FecharJogo {
                processos: vec!["RainbowSix.exe".into()],
            },
            OrientacaoConfigJogo::DesmarcarSomenteLeitura,
            OrientacaoConfigJogo::AjustarNoJogo,
            OrientacaoConfigJogo::EscolherConta,
        ];
        let mut vistos = std::collections::BTreeSet::new();
        for o in &todas {
            let t = orientacao(o, Some("GameSettings.ini"));
            assert!(t.ends_with('.'), "{t}");
            sem_emoji(&t);
            assert!(vistos.insert(t));
        }
        assert_eq!(
            orientacao(&OrientacaoConfigJogo::FecharSteam, None),
            "Feche a Steam para aplicar."
        );
        assert_eq!(
            orientacao(
                &OrientacaoConfigJogo::FecharJogo { processos: vec![] },
                None
            ),
            "Feche o jogo para aplicar."
        );
        assert_eq!(
            orientacao(
                &OrientacaoConfigJogo::DesmarcarSomenteLeitura,
                Some("GameSettings.ini")
            ),
            "Desmarque Somente leitura em GameSettings.ini."
        );
        assert_eq!(
            orientacao(&OrientacaoConfigJogo::EscolherConta, None),
            "Mais de uma conta encontrada: abra o jogo com a conta que você usa."
        );
    }

    fn arquivo(conta: Option<&str>, reversivel: bool) -> ArquivoJogoUi {
        ArquivoJogoUi {
            alvo: AlvoIpc::ConfigJogo {
                jogo: "r6".into(),
                arquivo: "game_settings".into(),
            },
            rotulo: RotuloCaminho(
                "%USERPROFILE%\\Documents\\My Games\\Rainbow Six - Siege\\<conta>\\GameSettings.ini"
                    .into(),
            ),
            conta: conta.map(Into::into),
            backup: EstadoBackup::Integro,
            mudancas: vec![
                MudancaArquivoJogo {
                    setting: "r6_fps_limit".into(),
                    antes: ValorJogoIpc::Inteiro(0),
                    depois: ValorJogoIpc::Inteiro(141),
                    status: StatusMudanca::Aplicada,
                    motivo: None,
                },
                MudancaArquivoJogo {
                    setting: "r6_vsync".into(),
                    antes: ValorJogoIpc::Booleano(true),
                    depois: ValorJogoIpc::Booleano(false),
                    status: StatusMudanca::Pendente,
                    motivo: Some(MotivoNaoAplicada::ArquivoSomenteLeitura),
                },
            ],
            reversivel,
        }
    }

    fn jogo(c: Option<ConfigJogoUi>) -> Jogo {
        Jogo {
            nome: "Rainbow Six Siege".into(),
            perfil: "Rainbow Six Siege".into(),
            origem_perfil: OrigemPerfil::Driver,
            launcher: "Ubisoft".into(),
            api: "dx11".into(),
            dlss: None,
            reflex: Some(true),
            anticheat_kernel: Some(true),
            competitivo: Some("r6".into()),
            em_execucao: false,
            orientacoes: vec![],
            resumo: ResumoJogo::default(),
            config_jogo: c,
        }
    }

    #[test]
    fn arquivo_do_jogo_mostra_caminho_antes_depois_e_backup() {
        let a = arquivo(Some("a1b2c3"), true);
        assert!(a.rotulo.0.starts_with("%USERPROFILE%\\"));
        assert_eq!(nome_arquivo(&a.rotulo.0), "GameSettings.ini");
        assert_eq!(backup(a.backup), "original guardado e íntegro");
        assert_eq!(
            valor(&ValorJogoIpc::TokenLancamento(Some(240))),
            "+fps_max 240"
        );
        assert_eq!(valor(&ValorJogoIpc::TokenLancamento(None)), "sem +fps_max");
        assert_eq!(valor(&ValorJogoIpc::Texto("x".into())), "“x”");
        assert_eq!(valor(&ValorJogoIpc::Ausente), "ausente");
        assert!(linha_mudanca(&a.mudancas[0]).contains("0 → 141 (aplicada)"));
        assert!(
            linha_mudanca(&a.mudancas[1])
                .ends_with("pendente): arquivo marcado como somente leitura")
        );
        for b in [
            EstadoBackup::SemBackup,
            EstadoBackup::Integro,
            EstadoBackup::Comprometido,
        ] {
            sem_emoji(backup(b));
        }
    }

    #[test]
    fn reverter_so_com_gestao_e_instancia() {
        let a = arquivo(Some("a1b2c3"), true);
        let d = destino_reverter(&a, "r6_fps_limit").unwrap();
        assert_eq!(d.instancia, "a1b2c3");
        assert!(
            nvt_ipc::Requisicao::ReverterConfigJogo { destino: d }
                .validar()
                .is_ok()
        );
        assert!(destino_reverter(&arquivo(Some("a1b2c3"), false), "r6_fps_limit").is_none());
        assert!(destino_reverter(&arquivo(None, true), "r6_fps_limit").is_none());
        assert!(destino_reverter(&arquivo(Some("Conta 1"), true), "r6_fps_limit").is_none());
    }

    #[test]
    fn orientacoes_do_jogo_citam_o_arquivo() {
        assert!(orientacoes(&jogo(None)).is_empty());
        let j = jogo(Some(ConfigJogoUi {
            arquivos: vec![arquivo(None, false)],
            orientacoes: vec![
                OrientacaoConfigJogo::DesmarcarSomenteLeitura,
                OrientacaoConfigJogo::FecharSteam,
            ],
        }));
        assert_eq!(
            orientacoes(&j),
            vec![
                "Desmarque Somente leitura em GameSettings.ini.".to_string(),
                "Feche a Steam para aplicar.".to_string(),
            ]
        );
    }

    #[test]
    fn sugestoes_experimentais_do_jogo() {
        let s = |regra: &str, estado| SugestaoExperimental {
            regra: regra.into(),
            descricao: "d".into(),
            camada: Camada::Driver,
            impacto_visual: ImpactoVisual::Nenhum,
            estado,
        };
        let todas = vec![
            s("jogo-r6-fps-limit", EstadoExperimental::Aceitavel),
            s("jogo-r6-vsync", EstadoExperimental::Aceita),
            s("jogo-r6-outra", EstadoExperimental::Estavel),
            s("jogo-cs2-fps-max", EstadoExperimental::Aceitavel),
            s("dlss-preset", EstadoExperimental::Aceitavel),
        ];
        let r: Vec<_> = sugestoes_do_jogo(&jogo(None), &todas)
            .iter()
            .map(|s| s.regra.as_str())
            .collect();
        assert_eq!(r, ["jogo-r6-fps-limit", "jogo-r6-vsync"]);
        let mut fora = jogo(None);
        fora.competitivo = None;
        assert!(sugestoes_do_jogo(&fora, &todas).is_empty());
    }
}
