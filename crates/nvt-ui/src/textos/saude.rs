//! Saúde do PC (spec `interface-e-saude.md` §5.3 e §5.5, R-S-06, R-L-08):
//! frases dos sinais, da inicialização e da manutenção, e a montagem pura
//! das linhas mostradas no Início e no Avançado.
//!
//! Textos de limpeza nunca citam quadros por segundo, velocidade nem
//! desempenho (R-L-08): a limpeza só libera espaço.

use nvt_ipc::{
    AlvoLimpeza, FonteInicio, Gravidade, ModoLimpeza, MotivoInterrupcao, ResultadoLimpeza, Saude,
    Sinal, SituacaoInicio, TipoSinal, ValorSinal,
};

// Início --------------------------------------------------------------------
pub const ROTULO_ESPACO: &str = "Espaço livre";
pub const ROTULO_INICIALIZACAO: &str = "Inicialização";
pub const ROTULO_PLACA: &str = "Placa de vídeo";
pub const ROTULO_ENERGIA: &str = "Energia";
pub const ROTULO_REPOUSO: &str = "Uso em repouso";
pub const ROTULO_LIMPEZA_WINDOWS: &str = "Limpeza do Windows";
pub const TUDO_CERTO: &str = "Tudo certo com a saúde do PC";
pub const INICIALIZACAO_OK: &str = "Nenhum programa novo inicia com o Windows.";
pub const PLACA_OK: &str = "O driver está em dia.";
pub const PLACA_ANTIGO: &str = "Há um driver mais novo da NVIDIA.";
pub const PLACA_CAMINHO: &str = "Para atualizar, abra o aplicativo da NVIDIA.";
pub const ENERGIA_OK: &str = "O PC está no plano de energia recomendado.";
pub const ENERGIA_ATENCAO: &str = "Plano de energia diferente do recomendado";
pub const ENERGIA_CAMINHO: &str = "Para mudar: Configurações > Sistema > Energia.";
pub const REPOUSO_OK: &str = "O PC está tranquilo quando você não está jogando.";
pub const REPOUSO_ATENCAO: &str = "O PC trabalha muito mesmo quando você não está jogando.";
pub const REPOUSO_CAMINHO: &str = "Para ver quem está usando: Gerenciador de Tarefas.";
pub const SENSOR_LIGADO: &str = "O Sensor de Armazenamento está ligado.";
pub const SENSOR_DESLIGADO: &str = "O Sensor de Armazenamento está desligado.";
pub const SENSOR_CAMINHO: &str = "Para ligar: Configurações > Sistema > Armazenamento.";
pub const ESPACO_CAMINHO: &str = "Para liberar: Configurações > Sistema > Armazenamento.";

// Abrir inicialização (D3, R-S-04) ----------------------------------------
pub const BOTAO_ABRIR_INICIO: &str = "Abrir inicialização";
pub const BOTAO_ABRIR_CONFIGURACOES: &str = "Abrir configurações de inicialização";
pub const ABRIR_BLOQUEADO_JOGO: &str = "Feche o jogo para abrir as configurações do Windows.";
pub const ABRIR_BLOQUEADO_DESCONHECIDO: &str =
    "Disponível quando o nv-tuner estiver ativo e souber que nenhum jogo está aberto.";
pub const ABRIR_FALHOU: &str = "O Windows não abriu as configurações de inicialização.";
pub const ABRIR_DEMONSTRACAO: &str = "Na demonstração, as configurações do Windows não abrem.";

// Avançado > Inicialização -------------------------------------------------
pub const BOTAO_MARCAR_VISTO: &str = "Marcar como visto";
pub const OK_MARCADO_VISTO: &str = "Pronto. O programa não aparece mais como novo.";
pub const SITUACAO_NOVO: &str = "Novo";
pub const SITUACAO_MUDOU: &str = "Mudou";
pub const SITUACAO_VISTO: &str = "Visto";
pub const DESLIGADO_NO_WINDOWS: &str = "desligado no Windows";
pub const SEM_PROGRAMAS: &str = "Nenhum programa inicia com o Windows.";
pub const SEM_DADOS: &str =
    "Sem dados de saúde: o nv-tuner precisa estar ativo e atualizado para ler o PC.";
pub const EXPLICA_INICIALIZACAO: &str =
    "O nv-tuner só mostra a lista. Para desligar um programa, use as configurações do Windows.";

// Avançado > Manutenção ----------------------------------------------------
pub const MODO_DESLIGADA: &str = "A limpeza de arquivos temporários está desligada por enquanto.";
pub const MODO_SIMULACAO: &str = "Build de teste: a limpeza só simula. Nenhum arquivo é apagado.";
pub const MODO_ATIVA: &str = "A limpeza apaga arquivos temporários antigos, uma vez por dia.";
pub const ROTULO_MODO: &str = "Modo";
pub const ROTULO_ULTIMAS: &str = "Últimas passadas";
pub const SEM_PASSADAS: &str = "Nenhuma passada ainda.";
pub const ALVO_USUARIO: &str = "Seus arquivos temporários";
pub const ALVO_WINDOWS: &str = "Arquivos temporários do Windows";
pub const EXPLICA_LIMPEZA: &str = "Só arquivos com mais de 7 dias, e nunca com um jogo aberto.";

/// Frases fixas, para os testes de linguagem e de emoji.
pub const TODOS: &[&str] = &[
    ROTULO_ESPACO,
    ROTULO_INICIALIZACAO,
    ROTULO_PLACA,
    ROTULO_ENERGIA,
    ROTULO_REPOUSO,
    ROTULO_LIMPEZA_WINDOWS,
    TUDO_CERTO,
    INICIALIZACAO_OK,
    PLACA_OK,
    PLACA_ANTIGO,
    PLACA_CAMINHO,
    ENERGIA_OK,
    ENERGIA_ATENCAO,
    ENERGIA_CAMINHO,
    REPOUSO_OK,
    REPOUSO_ATENCAO,
    REPOUSO_CAMINHO,
    SENSOR_LIGADO,
    SENSOR_DESLIGADO,
    SENSOR_CAMINHO,
    ESPACO_CAMINHO,
    BOTAO_ABRIR_INICIO,
    BOTAO_ABRIR_CONFIGURACOES,
    ABRIR_BLOQUEADO_JOGO,
    ABRIR_BLOQUEADO_DESCONHECIDO,
    ABRIR_FALHOU,
    ABRIR_DEMONSTRACAO,
    BOTAO_MARCAR_VISTO,
    OK_MARCADO_VISTO,
    SITUACAO_NOVO,
    SITUACAO_MUDOU,
    SITUACAO_VISTO,
    DESLIGADO_NO_WINDOWS,
    SEM_PROGRAMAS,
    SEM_DADOS,
    EXPLICA_INICIALIZACAO,
];

/// Frases de limpeza fixas (R-L-08: sem FPS, velocidade ou desempenho).
pub const TODOS_LIMPEZA: &[&str] = &[
    MODO_DESLIGADA,
    MODO_SIMULACAO,
    MODO_ATIVA,
    ROTULO_MODO,
    ROTULO_ULTIMAS,
    SEM_PASSADAS,
    ALVO_USUARIO,
    ALVO_WINDOWS,
    EXPLICA_LIMPEZA,
];

// Formatação -----------------------------------------------------------------

/// Gigabytes com uma casa ("2,4 GB"); abaixo de 1 GB, megabytes inteiros.
pub fn tamanho(bytes: u64) -> String {
    const GB: u64 = 1_000_000_000;
    if bytes < GB {
        return format!("{} MB", bytes / 1_000_000);
    }
    let decimos = bytes / (GB / 10);
    match decimos % 10 {
        0 => format!("{} GB", decimos / 10),
        d => format!("{},{d} GB", decimos / 10),
    }
}

/// Gigabytes inteiros, arredondados para baixo ("8 GB").
fn gb_inteiros(bytes: u64) -> u64 {
    bytes / 1_000_000_000
}

fn plural(n: u64, um: &str, varios: &str) -> String {
    if n == 1 {
        format!("1 {um}")
    } else {
        format!("{n} {varios}")
    }
}

// Início: linhas da seção Saúde do PC ----------------------------------------

/// Uma linha da seção Saúde do PC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaSaude {
    pub rotulo: &'static str,
    pub frase: String,
    /// Caminho do Windows quando há ação manual.
    pub caminho: Option<&'static str>,
    pub atencao: bool,
    /// Só a linha de inicialização com novidade tem botão.
    pub botao_abrir: bool,
}

/// Conteúdo da seção: linhas, ou "Tudo certo" quando nada pede atenção.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecaoSaude {
    TudoCerto,
    Linhas(Vec<LinhaSaude>),
}

/// "Pouco espaço livre no disco: 8 GB" / "Restam 210 GB. Tudo certo."
pub fn espaco(livres: u64, atencao: bool) -> String {
    if atencao {
        format!("Pouco espaço livre no disco: {} GB", gb_inteiros(livres))
    } else {
        format!("Restam {} GB. Tudo certo.", gb_inteiros(livres))
    }
}

/// "2 programas novos iniciam com o Windows" (e "1 mudou" quando houver).
pub fn inicializacao(novos: usize, mudaram: usize) -> String {
    let novos = novos as u64;
    let mudaram = mudaram as u64;
    match (novos, mudaram) {
        (0, 0) => INICIALIZACAO_OK.to_owned(),
        (n, 0) => format!(
            "{} com o Windows",
            plural(n, "programa novo inicia", "programas novos iniciam")
        ),
        (0, m) => format!(
            "{} desde a última vez",
            plural(
                m,
                "programa da inicialização mudou",
                "programas da inicialização mudaram"
            )
        ),
        (n, m) => format!(
            "{} com o Windows, e {}",
            plural(n, "programa novo inicia", "programas novos iniciam"),
            plural(m, "outro mudou", "outros mudaram")
        ),
    }
}

/// Quantos programas estão novos e quantos mudaram.
pub fn contagem_inicio(s: &Saude) -> (usize, usize) {
    let conta = |sit| s.inicializacao.iter().filter(|p| p.situacao == sit).count();
    (conta(SituacaoInicio::Novo), conta(SituacaoInicio::Mudou))
}

fn linha(
    rotulo: &'static str,
    frase: String,
    caminho: Option<&'static str>,
    atencao: bool,
) -> LinhaSaude {
    LinhaSaude {
        rotulo,
        frase,
        caminho: caminho.filter(|_| atencao),
        atencao,
        botao_abrir: false,
    }
}

/// Linha de um sinal; `None` se indisponível ou sem valor legível.
pub fn linha_sinal(s: &Sinal) -> Option<LinhaSaude> {
    if s.gravidade == Gravidade::Indisponivel {
        return None;
    }
    let atencao = s.gravidade == Gravidade::Atencao;
    Some(match (s.tipo, s.valor) {
        (TipoSinal::EspacoLivre, ValorSinal::Bytes { livres, .. }) => linha(
            ROTULO_ESPACO,
            espaco(livres, atencao),
            Some(ESPACO_CAMINHO),
            atencao,
        ),
        (TipoSinal::EspacoLivre, _) => return None,
        (TipoSinal::DriverDesatualizado, v) => {
            let antigo =
                atencao || matches!(v, ValorSinal::Versao { atual, ultima } if atual < ultima);
            let mut l = linha(
                ROTULO_PLACA,
                (if antigo { PLACA_ANTIGO } else { PLACA_OK }).to_owned(),
                Some(PLACA_CAMINHO),
                antigo,
            );
            l.atencao = atencao;
            l
        }
        (TipoSinal::PlanoEnergia, _) => linha(
            ROTULO_ENERGIA,
            (if atencao { ENERGIA_ATENCAO } else { ENERGIA_OK }).to_owned(),
            Some(ENERGIA_CAMINHO),
            atencao,
        ),
        (TipoSinal::MemoriaRepouso | TipoSinal::CpuRepouso, _) => linha(
            ROTULO_REPOUSO,
            (if atencao { REPOUSO_ATENCAO } else { REPOUSO_OK }).to_owned(),
            Some(REPOUSO_CAMINHO),
            atencao,
        ),
        (TipoSinal::SensorArmazenamento, v) => {
            let desligado = atencao || v == ValorSinal::Booleano(false);
            LinhaSaude {
                rotulo: ROTULO_LIMPEZA_WINDOWS,
                frase: (if desligado {
                    SENSOR_DESLIGADO
                } else {
                    SENSOR_LIGADO
                })
                .to_owned(),
                caminho: desligado.then_some(SENSOR_CAMINHO),
                atencao,
                botao_abrir: false,
            }
        }
    })
}

/// Monta a seção Saúde do PC do Início. Memória e processador em repouso
/// viram uma linha só ("Uso em repouso"), a pior das duas.
pub fn secao_inicio(s: &Saude) -> SecaoSaude {
    let mut linhas: Vec<LinhaSaude> = Vec::new();
    for sinal in &s.sinais {
        let Some(l) = linha_sinal(sinal) else {
            continue;
        };
        if let Some(existente) = linhas.iter_mut().find(|x| x.rotulo == l.rotulo) {
            if l.atencao && !existente.atencao {
                *existente = l;
            }
            continue;
        }
        linhas.push(l);
    }
    let (novos, mudaram) = contagem_inicio(s);
    let novidade = novos + mudaram > 0;
    linhas.insert(
        usize::from(linhas.first().is_some_and(|l| l.rotulo == ROTULO_ESPACO)),
        LinhaSaude {
            rotulo: ROTULO_INICIALIZACAO,
            frase: inicializacao(novos, mudaram),
            caminho: None,
            atencao: false,
            botao_abrir: novidade,
        },
    );
    if !novidade && linhas.iter().all(|l| !l.atencao && l.caminho.is_none()) {
        SecaoSaude::TudoCerto
    } else {
        SecaoSaude::Linhas(linhas)
    }
}

// Avançado > Inicialização ---------------------------------------------------

pub fn fonte(f: FonteInicio) -> &'static str {
    match f {
        FonteInicio::RunUsuario => "Registro do usuário",
        FonteInicio::RunOnceUsuario => "Registro do usuário (uma vez)",
        FonteInicio::RunMaquina => "Registro do PC",
        FonteInicio::RunOnceMaquina => "Registro do PC (uma vez)",
        FonteInicio::RunMaquina32 => "Registro do PC (32 bits)",
        FonteInicio::PastaUsuario => "Pasta Inicializar do usuário",
        FonteInicio::PastaComum => "Pasta Inicializar de todos",
        FonteInicio::Tarefa => "Tarefa agendada",
    }
}

pub fn situacao(s: SituacaoInicio) -> &'static str {
    match s {
        SituacaoInicio::NaBase => SITUACAO_VISTO,
        SituacaoInicio::Novo => SITUACAO_NOVO,
        SituacaoInicio::Mudou => SITUACAO_MUDOU,
    }
}

/// "Registro do usuário · discord.exe · detectado em 01/11/2026 10:00"
pub fn detalhe_programa(
    f: FonteInicio,
    executavel: &str,
    data: &str,
    habilitado: Option<bool>,
) -> String {
    let mut s = format!("{} · {executavel} · detectado em {data}", fonte(f));
    if habilitado == Some(false) {
        s.push_str(" · ");
        s.push_str(DESLIGADO_NO_WINDOWS);
    }
    s
}

// Avançado > Manutenção -----------------------------------------------------

pub fn modo(m: ModoLimpeza) -> &'static str {
    match m {
        ModoLimpeza::Desligada => MODO_DESLIGADA,
        ModoLimpeza::Simulacao => MODO_SIMULACAO,
        ModoLimpeza::Ativa => MODO_ATIVA,
    }
}

pub fn alvo(a: AlvoLimpeza) -> &'static str {
    match a {
        AlvoLimpeza::TempUsuario => ALVO_USUARIO,
        AlvoLimpeza::TempWindows => ALVO_WINDOWS,
    }
}

fn interrupcao(m: MotivoInterrupcao) -> &'static str {
    match m {
        MotivoInterrupcao::JogoAberto => "parou porque um jogo abriu",
        MotivoInterrupcao::Instalacao => "parou porque havia uma instalação em andamento",
        MotivoInterrupcao::Tempo => "parou no limite de tempo",
        MotivoInterrupcao::Erro => "parou por um erro",
    }
}

/// Resumo do modo e das passadas: "Simulação: apagaria N arquivos e
/// liberaria X GB" (build de teste) ou "Liberamos X GB..." (`Ativa`).
/// `None` com a limpeza desligada ou sem passadas.
pub fn resumo_limpeza(modo: ModoLimpeza, passadas: &[ResultadoLimpeza]) -> Option<String> {
    let do_modo = passadas.iter().filter(|p| p.modo == modo);
    let (arquivos, bytes) = do_modo.fold((0u64, 0u64), |(a, b), p| {
        (a + u64::from(p.arquivos), b + p.bytes)
    });
    if passadas.iter().all(|p| p.modo != modo) {
        return None;
    }
    match modo {
        ModoLimpeza::Desligada => None,
        ModoLimpeza::Simulacao => Some(format!(
            "Simulação: apagaria {} e liberaria {}",
            plural(arquivos, "arquivo", "arquivos"),
            tamanho(bytes)
        )),
        ModoLimpeza::Ativa => Some(format!(
            "Liberamos {} de arquivos temporários antigos.",
            tamanho(bytes)
        )),
    }
}

/// Uma passada: "01/11/2026 10:00 · 1284 arquivos, 2,3 GB · 415 pulados".
pub fn passada(p: &ResultadoLimpeza, data: &str) -> String {
    let verbo = match p.modo {
        ModoLimpeza::Simulacao => "apagaria",
        _ => "apagou",
    };
    let pulados: u64 = p.pulados.values().map(|n| u64::from(*n)).sum();
    let mut s = format!(
        "{data} · {verbo} {}, {}",
        plural(u64::from(p.arquivos), "arquivo", "arquivos"),
        tamanho(p.bytes)
    );
    if pulados > 0 {
        s.push_str(&format!(" · {} preservados", pulados));
    }
    if let Some(m) = p.interrompida {
        s.push_str(" · ");
        s.push_str(interrupcao(m));
    }
    s
}

#[cfg(test)]
mod testes {
    use super::*;
    use nvt_ipc::{MotivoPulo, ProgramaInicio};

    fn sinal(tipo: TipoSinal, gravidade: Gravidade, valor: ValorSinal) -> Sinal {
        Sinal {
            tipo,
            gravidade,
            valor,
            lido_unix: 0,
        }
    }

    fn programa(nome: &str, situacao: SituacaoInicio) -> ProgramaInicio {
        ProgramaInicio {
            fonte: FonteInicio::RunUsuario,
            nome: nome.into(),
            executavel: "a.exe".into(),
            habilitado: Some(true),
            situacao,
            detectado_unix: 0,
        }
    }

    fn saude(sinais: Vec<Sinal>, inicializacao: Vec<ProgramaInicio>) -> Saude {
        Saude {
            sinais,
            inicializacao,
            modo_limpeza: ModoLimpeza::Desligada,
            limpeza: vec![],
        }
    }

    const GB: u64 = 1_000_000_000;

    #[test]
    fn r_s_06_frases_dos_sinais() {
        let pouco = sinal(
            TipoSinal::EspacoLivre,
            Gravidade::Atencao,
            ValorSinal::Bytes {
                livres: 8 * GB + 7,
                total: 500 * GB,
            },
        );
        let l = linha_sinal(&pouco).unwrap();
        assert_eq!(l.frase, "Pouco espaço livre no disco: 8 GB");
        assert!(l.atencao);
        assert_eq!(l.caminho, Some(ESPACO_CAMINHO));
        let energia = sinal(
            TipoSinal::PlanoEnergia,
            Gravidade::Atencao,
            ValorSinal::Booleano(false),
        );
        assert_eq!(
            linha_sinal(&energia).unwrap().frase,
            "Plano de energia diferente do recomendado"
        );
        let indisponivel = sinal(
            TipoSinal::CpuRepouso,
            Gravidade::Indisponivel,
            ValorSinal::Nenhum,
        );
        assert_eq!(linha_sinal(&indisponivel), None);
        let sensor = sinal(
            TipoSinal::SensorArmazenamento,
            Gravidade::Informativo,
            ValorSinal::Booleano(false),
        );
        let l = linha_sinal(&sensor).unwrap();
        assert_eq!(l.frase, SENSOR_DESLIGADO);
        assert_eq!(l.caminho, Some(SENSOR_CAMINHO));
        assert!(!l.atencao);
    }

    #[test]
    fn frases_da_inicializacao() {
        assert_eq!(inicializacao(0, 0), INICIALIZACAO_OK);
        assert_eq!(inicializacao(1, 0), "1 programa novo inicia com o Windows");
        assert_eq!(
            inicializacao(2, 0),
            "2 programas novos iniciam com o Windows"
        );
        assert_eq!(
            inicializacao(0, 1),
            "1 programa da inicialização mudou desde a última vez"
        );
        assert_eq!(
            inicializacao(2, 1),
            "2 programas novos iniciam com o Windows, e 1 outro mudou"
        );
    }

    #[test]
    fn secao_tudo_certo_e_com_novidades() {
        let ok = sinal(
            TipoSinal::EspacoLivre,
            Gravidade::Ok,
            ValorSinal::Bytes {
                livres: 210 * GB,
                total: 1000 * GB,
            },
        );
        let s = saude(vec![ok], vec![programa("a", SituacaoInicio::NaBase)]);
        assert_eq!(secao_inicio(&s), SecaoSaude::TudoCerto);
        let s = saude(
            vec![ok],
            vec![
                programa("a", SituacaoInicio::Novo),
                programa("b", SituacaoInicio::Novo),
            ],
        );
        let SecaoSaude::Linhas(l) = secao_inicio(&s) else {
            panic!("esperava linhas");
        };
        assert_eq!(l[0].rotulo, ROTULO_ESPACO);
        assert_eq!(l[1].rotulo, ROTULO_INICIALIZACAO);
        assert_eq!(l[1].frase, "2 programas novos iniciam com o Windows");
        assert!(l[1].botao_abrir);
        assert_eq!(l.iter().filter(|x| x.botao_abrir).count(), 1);
    }

    #[test]
    fn repouso_vira_uma_linha_a_pior() {
        let s = saude(
            vec![
                sinal(
                    TipoSinal::MemoriaRepouso,
                    Gravidade::Ok,
                    ValorSinal::Percentual(30),
                ),
                sinal(
                    TipoSinal::CpuRepouso,
                    Gravidade::Atencao,
                    ValorSinal::Percentual(40),
                ),
            ],
            vec![],
        );
        let SecaoSaude::Linhas(l) = secao_inicio(&s) else {
            panic!("esperava linhas");
        };
        let repouso: Vec<_> = l.iter().filter(|x| x.rotulo == ROTULO_REPOUSO).collect();
        assert_eq!(repouso.len(), 1);
        assert!(repouso[0].atencao);
    }

    fn passada_simulada(arquivos: u32, bytes: u64) -> ResultadoLimpeza {
        ResultadoLimpeza {
            modo: ModoLimpeza::Simulacao,
            alvo: AlvoLimpeza::TempUsuario,
            inicio_unix: 0,
            duracao_ms: 10,
            arquivos,
            bytes,
            pulados: [(MotivoPulo::Recente, 2), (MotivoPulo::EmUso, 1)].into(),
            interrompida: Some(MotivoInterrupcao::Tempo),
        }
    }

    #[test]
    fn r_l_08_resumo_da_limpeza() {
        let p = vec![passada_simulada(1000, 2_350_000_000), {
            let mut b = passada_simulada(96, 410_000_000);
            b.alvo = AlvoLimpeza::TempWindows;
            b
        }];
        assert_eq!(
            resumo_limpeza(ModoLimpeza::Simulacao, &p).unwrap(),
            "Simulação: apagaria 1096 arquivos e liberaria 2,7 GB"
        );
        assert_eq!(resumo_limpeza(ModoLimpeza::Desligada, &[]), None);
        assert_eq!(resumo_limpeza(ModoLimpeza::Simulacao, &[]), None);
        let mut ativa = passada_simulada(1, 3 * GB);
        ativa.modo = ModoLimpeza::Ativa;
        assert_eq!(
            resumo_limpeza(ModoLimpeza::Ativa, &[ativa]).unwrap(),
            "Liberamos 3 GB de arquivos temporários antigos."
        );
        assert_eq!(
            passada(&p[0], "01/11/2026 10:00"),
            "01/11/2026 10:00 · apagaria 1000 arquivos, 2,3 GB · 3 preservados · parou no limite de tempo"
        );
        assert_eq!(tamanho(410_000_000), "410 MB");
    }

    #[test]
    fn r_l_08_limpeza_sem_promessa_de_velocidade() {
        let p = passada_simulada(5, 2 * GB);
        let mut ativa = p.clone();
        ativa.modo = ModoLimpeza::Ativa;
        let mut textos: Vec<String> = TODOS_LIMPEZA.iter().map(|s| (*s).to_owned()).collect();
        textos.push(resumo_limpeza(ModoLimpeza::Simulacao, std::slice::from_ref(&p)).unwrap());
        textos.push(resumo_limpeza(ModoLimpeza::Ativa, std::slice::from_ref(&ativa)).unwrap());
        textos.push(passada(&p, "x"));
        for m in [
            MotivoInterrupcao::JogoAberto,
            MotivoInterrupcao::Instalacao,
            MotivoInterrupcao::Tempo,
            MotivoInterrupcao::Erro,
        ] {
            textos.push(interrupcao(m).to_owned());
        }
        for t in textos {
            let m = t.to_lowercase();
            for proibido in [
                "fps",
                "quadros",
                "rápido",
                "rapido",
                "desempenho",
                "velocidade",
            ] {
                assert!(!m.contains(proibido), "{proibido:?} em {t:?}");
            }
        }
    }
}
