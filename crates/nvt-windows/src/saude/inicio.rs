//! Programas de inicialização: composição testável sobre [`LeitorInicio`].

use std::{collections::BTreeSet, fmt};

use nvt_ipc::saude::{
    EXECUTAVEL_MAX, FonteInicio, NOME_INICIO_MAX, PROGRAMAS_INICIO_MAX, ProgramaInicio,
    SituacaoInicio,
};

/// Chaves Run/RunOnce lidas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChaveRun {
    RunUsuario,
    RunOnceUsuario,
    RunMaquina,
    RunOnceMaquina,
    /// `HKLM\Software\WOW6432Node\...\Run`.
    RunMaquina32,
}

/// Chaves `Explorer\StartupApproved\*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aprovacao {
    RunUsuario,
    PastaUsuario,
    RunMaquina,
    Run32Maquina,
    PastaMaquina,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PastaInicio {
    Usuario,
    Comum,
}

/// Arquivo na pasta Inicializar. `alvo` é o caminho do alvo do `.lnk`, se resolvido.
#[derive(Clone, PartialEq, Eq)]
pub struct ArquivoPasta {
    pub nome_arquivo: String,
    pub alvo: Option<String>,
}

/// Tarefa agendada com gatilho de logon ou boot.
#[derive(Clone, PartialEq, Eq)]
pub struct TarefaBruta {
    pub nome: String,
    /// Caminho da tarefa no Agendador (ex.: `\Fornecedor\Atualizar`).
    pub caminho_tarefa: String,
    /// Caminho do executável da primeira ação, se for de execução.
    pub executavel: Option<String>,
    pub habilitada: bool,
}

/// Fachada somente leitura. `None` = fonte indisponível; chave ou pasta ausente = vazio.
pub trait LeitorInicio {
    /// Valores `(nome, comando)` do tipo texto.
    fn run(&self, chave: ChaveRun) -> Option<Vec<(String, String)>>;
    /// Valores binários `(nome, bytes)` do StartupApproved.
    fn aprovacao(&self, chave: Aprovacao) -> Option<Vec<(String, Vec<u8>)>>;
    fn pasta(&self, pasta: PastaInicio) -> Option<Vec<ArquivoPasta>>;
    /// Tarefas com gatilho de logon/boot (todas as pastas; o filtro de `\Microsoft\` é aqui).
    fn tarefas(&self) -> Option<Vec<TarefaBruta>>;
    /// Executa uma coleta inteira; a fachada real mantém o COM ativo durante `f`.
    fn coleta<R>(&self, f: impl FnOnce() -> R) -> R {
        f()
    }
}

/// Entrada lida. `comando` fica no processo (só para o hash da linha de base) e nunca
/// aparece em `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct EntradaInicio {
    pub fonte: FonteInicio,
    pub nome: String,
    pub executavel: String,
    pub habilitado: Option<bool>,
    pub comando: String,
}

impl fmt::Debug for EntradaInicio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntradaInicio")
            .field("fonte", &self.fonte)
            .field("nome", &self.nome)
            .field("executavel", &self.executavel)
            .field("habilitado", &self.habilitado)
            .finish_non_exhaustive()
    }
}

impl EntradaInicio {
    pub fn programa(&self, situacao: SituacaoInicio, detectado_unix: u64) -> ProgramaInicio {
        ProgramaInicio {
            fonte: self.fonte,
            nome: self.nome.clone(),
            executavel: self.executavel.clone(),
            habilitado: self.habilitado,
            situacao,
            detectado_unix,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeituraInicio {
    /// Sem `(fonte, nome)` repetido, no máximo [`PROGRAMAS_INICIO_MAX`].
    pub entradas: Vec<EntradaInicio>,
    pub fontes_indisponiveis: Vec<FonteInicio>,
}

/// Sessão (agente): HKCU Run/RunOnce, pasta do usuário e tarefas agendadas.
pub fn ler_inicio_sessao(l: &impl LeitorInicio) -> LeituraInicio {
    l.coleta(|| {
        let mut c = Coletor::default();
        c.run(l, ChaveRun::RunUsuario, Some(Aprovacao::RunUsuario));
        c.run(l, ChaveRun::RunOnceUsuario, None);
        c.pasta(l, PastaInicio::Usuario, Aprovacao::PastaUsuario);
        c.tarefas(l);
        c.fim()
    })
}

/// Máquina (serviço): HKLM Run/RunOnce, WOW6432Node e pasta comum.
pub fn ler_inicio_maquina(l: &impl LeitorInicio) -> LeituraInicio {
    l.coleta(|| {
        let mut c = Coletor::default();
        c.run(l, ChaveRun::RunMaquina, Some(Aprovacao::RunMaquina));
        c.run(l, ChaveRun::RunOnceMaquina, None);
        c.run(l, ChaveRun::RunMaquina32, Some(Aprovacao::Run32Maquina));
        c.pasta(l, PastaInicio::Comum, Aprovacao::PastaMaquina);
        c.fim()
    })
}

#[derive(Default)]
struct Coletor {
    leitura: LeituraInicio,
    vistos: BTreeSet<(FonteInicio, String)>,
}

impl Coletor {
    fn indisponivel(&mut self, fonte: FonteInicio) {
        if !self.leitura.fontes_indisponiveis.contains(&fonte) {
            self.leitura.fontes_indisponiveis.push(fonte);
        }
    }

    fn incluir(&mut self, e: EntradaInicio) {
        if self.leitura.entradas.len() < PROGRAMAS_INICIO_MAX
            && self.vistos.insert((e.fonte, e.nome.clone()))
        {
            self.leitura.entradas.push(e);
        }
    }

    fn run(&mut self, l: &impl LeitorInicio, chave: ChaveRun, aprov: Option<Aprovacao>) {
        let fonte = match chave {
            ChaveRun::RunUsuario => FonteInicio::RunUsuario,
            ChaveRun::RunOnceUsuario => FonteInicio::RunOnceUsuario,
            ChaveRun::RunMaquina => FonteInicio::RunMaquina,
            ChaveRun::RunOnceMaquina => FonteInicio::RunOnceMaquina,
            ChaveRun::RunMaquina32 => FonteInicio::RunMaquina32,
        };
        let Some(valores) = l.run(chave) else {
            return self.indisponivel(fonte);
        };
        let aprovados = aprov.map(|a| l.aprovacao(a));
        for (nome_valor, comando) in valores {
            let Some(nome) = nome_simples(&nome_valor, NOME_INICIO_MAX) else {
                continue;
            };
            let habilitado = match &aprovados {
                // RunOnce não passa pelo StartupApproved: roda no próximo logon.
                None => Some(true),
                Some(mapa) => habilitado_em(mapa.as_deref(), &nome_valor),
            };
            self.incluir(EntradaInicio {
                fonte,
                nome,
                executavel: executavel_de(&comando),
                habilitado,
                comando,
            });
        }
    }

    fn pasta(&mut self, l: &impl LeitorInicio, pasta: PastaInicio, aprov: Aprovacao) {
        let fonte = match pasta {
            PastaInicio::Usuario => FonteInicio::PastaUsuario,
            PastaInicio::Comum => FonteInicio::PastaComum,
        };
        let Some(arquivos) = l.pasta(pasta) else {
            return self.indisponivel(fonte);
        };
        let aprovados = l.aprovacao(aprov);
        for a in arquivos {
            if a.nome_arquivo.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }
            let atalho = tem_extensao(&a.nome_arquivo, ".lnk");
            let exibido = if atalho {
                &a.nome_arquivo[..a.nome_arquivo.len() - 4]
            } else {
                a.nome_arquivo.as_str()
            };
            let Some(nome) = nome_simples(exibido, NOME_INICIO_MAX) else {
                continue;
            };
            let executavel = match (&a.alvo, atalho) {
                (Some(alvo), _) => executavel_de(alvo),
                (None, false) => executavel_de(&a.nome_arquivo),
                (None, true) => DESCONHECIDO.into(),
            };
            self.incluir(EntradaInicio {
                fonte,
                nome,
                executavel,
                habilitado: habilitado_em(aprovados.as_deref(), &a.nome_arquivo),
                comando: a.alvo.unwrap_or_else(|| a.nome_arquivo.clone()),
            });
        }
    }

    fn tarefas(&mut self, l: &impl LeitorInicio) {
        let Some(tarefas) = l.tarefas() else {
            return self.indisponivel(FonteInicio::Tarefa);
        };
        for t in tarefas {
            if tarefa_do_sistema(&t.caminho_tarefa) {
                continue;
            }
            let Some(nome) = nome_simples(&t.nome, NOME_INICIO_MAX) else {
                continue;
            };
            let executavel = t
                .executavel
                .as_deref()
                .map_or_else(|| DESCONHECIDO.into(), executavel_de);
            self.incluir(EntradaInicio {
                fonte: FonteInicio::Tarefa,
                nome,
                executavel,
                habilitado: Some(t.habilitada),
                comando: format!(
                    "{}|{}",
                    t.caminho_tarefa,
                    t.executavel.as_deref().unwrap_or("")
                ),
            });
        }
    }

    fn fim(self) -> LeituraInicio {
        self.leitura
    }
}

const DESCONHECIDO: &str = "desconhecido";

fn tarefa_do_sistema(caminho: &str) -> bool {
    caminho
        .get(..11)
        .is_some_and(|p| p.eq_ignore_ascii_case(r"\Microsoft\"))
}

fn tem_extensao(nome: &str, ext: &str) -> bool {
    nome.len() > ext.len()
        && nome
            .get(nome.len() - ext.len()..)
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

fn habilitado_em(mapa: Option<&[(String, Vec<u8>)]>, nome: &str) -> Option<bool> {
    let mapa = mapa?;
    match mapa.iter().find(|(n, _)| n.eq_ignore_ascii_case(nome)) {
        // Sem entrada no StartupApproved, o Windows trata como habilitado.
        None => Some(true),
        Some((_, bytes)) => estado_aprovacao(bytes),
    }
}

/// Decodifica um valor do StartupApproved (formato não documentado: 12 bytes, primeiro
/// byte 0x02/0x06 habilitado, 0x01/0x03/0x07 desabilitado). Qualquer outra coisa é
/// desconhecida.
pub fn estado_aprovacao(bytes: &[u8]) -> Option<bool> {
    if bytes.len() != 12 {
        return None;
    }
    match bytes[0] {
        0x02 | 0x06 => Some(true),
        0x01 | 0x03 | 0x07 => Some(false),
        _ => None,
    }
}

/// Só o nome do arquivo do executável de um comando, sem pasta, sem `:` e limitado a
/// [`EXECUTAVEL_MAX`] bytes. Nunca vazio.
pub fn executavel_de(comando: &str) -> String {
    let s = comando.trim();
    let caminho = if let Some(resto) = s.strip_prefix('"') {
        resto.split('"').next().unwrap_or("")
    } else if let Some(i) = s.to_ascii_lowercase().find(".exe") {
        &s[..i + 4]
    } else {
        s.split_whitespace().next().unwrap_or("")
    };
    let arquivo = caminho.rsplit(['\\', '/', ':']).next().unwrap_or("").trim();
    nome_simples(arquivo, EXECUTAVEL_MAX).unwrap_or_else(|| DESCONHECIDO.into())
}

/// Troca separadores e controles por `_`, apara e corta no limite em bytes.
fn nome_simples(texto: &str, limite: usize) -> Option<String> {
    let limpo: String = texto
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\\' | '/' | ':') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut fim = limpo.len().min(limite);
    while !limpo.is_char_boundary(fim) {
        fim -= 1;
    }
    let cortado = limpo[..fim].trim();
    (!cortado.is_empty()).then(|| cortado.to_owned())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[derive(Default)]
    struct Falso {
        run_usuario: Option<Vec<(String, String)>>,
        run_maquina: Option<Vec<(String, String)>>,
        aprov_run: Option<Vec<(String, Vec<u8>)>>,
        pasta: Option<Vec<ArquivoPasta>>,
        aprov_pasta: Option<Vec<(String, Vec<u8>)>>,
        tarefas: Option<Vec<TarefaBruta>>,
    }

    impl LeitorInicio for Falso {
        fn run(&self, chave: ChaveRun) -> Option<Vec<(String, String)>> {
            match chave {
                ChaveRun::RunUsuario => self.run_usuario.clone(),
                ChaveRun::RunMaquina => self.run_maquina.clone(),
                _ => Some(vec![]),
            }
        }
        fn aprovacao(&self, chave: Aprovacao) -> Option<Vec<(String, Vec<u8>)>> {
            match chave {
                Aprovacao::RunUsuario => self.aprov_run.clone(),
                Aprovacao::PastaUsuario => self.aprov_pasta.clone(),
                _ => Some(vec![]),
            }
        }
        fn pasta(&self, pasta: PastaInicio) -> Option<Vec<ArquivoPasta>> {
            match pasta {
                PastaInicio::Usuario => self.pasta.clone(),
                PastaInicio::Comum => Some(vec![]),
            }
        }
        fn tarefas(&self) -> Option<Vec<TarefaBruta>> {
            self.tarefas.clone()
        }
    }

    fn par(a: &str, b: &str) -> (String, String) {
        (a.into(), b.into())
    }

    fn aprov(nome: &str, b0: u8) -> (String, Vec<u8>) {
        let mut v = vec![0u8; 12];
        v[0] = b0;
        (nome.into(), v)
    }

    #[test]
    fn executavel_sem_pasta() {
        for (cmd, exe) in [
            (
                r#""C:\Users\fulano\AppData\Local\App\app.exe" --min"#,
                "app.exe",
            ),
            (r"C:\Program Files\Fornecedor X\bin\tray.exe /s", "tray.exe"),
            (
                r"C:\Windows\system32\rundll32.exe x.dll,Run",
                "rundll32.exe",
            ),
            ("%LOCALAPPDATA%\\a\\b.cmd", "b.cmd"),
            ("C:app.exe", "app.exe"),
            ("", DESCONHECIDO),
            (r#""C:\x\""#, DESCONHECIDO),
        ] {
            assert_eq!(executavel_de(cmd), exe, "{cmd}");
        }
        let longo = executavel_de(&format!(r"C:\{}.exe", "é".repeat(60)));
        assert!(longo.len() <= EXECUTAVEL_MAX);
    }

    #[test]
    fn aprovacao_inesperada_e_desconhecida() {
        let mut v = [0u8; 12];
        v[0] = 0x02;
        assert_eq!(estado_aprovacao(&v), Some(true));
        v[0] = 0x03;
        assert_eq!(estado_aprovacao(&v), Some(false));
        v[0] = 0x99;
        assert_eq!(estado_aprovacao(&v), None);
        assert_eq!(estado_aprovacao(&[0x02]), None);
        assert_eq!(estado_aprovacao(&[]), None);
    }

    #[test]
    fn sessao_compoe_fontes() {
        let f = Falso {
            run_usuario: Some(vec![
                par("Chat", r#""C:\Users\fulano\chat.exe" -m"#),
                par("Desligado", r"C:\x\off.exe"),
                par("Estranho", r"C:\x\e.exe"),
                par("Chat", r"C:\dup.exe"),
                par("  ", r"C:\vazio.exe"),
            ]),
            aprov_run: Some(vec![aprov("desligado", 0x03), aprov("Estranho", 0x42)]),
            pasta: Some(vec![
                ArquivoPasta {
                    nome_arquivo: "Atalho.lnk".into(),
                    alvo: Some(r"C:\Users\fulano\a.exe".into()),
                },
                ArquivoPasta {
                    nome_arquivo: "Quebrado.lnk".into(),
                    alvo: None,
                },
                ArquivoPasta {
                    nome_arquivo: "desktop.ini".into(),
                    alvo: None,
                },
            ]),
            aprov_pasta: Some(vec![aprov("Atalho.lnk", 0x07)]),
            tarefas: Some(vec![
                TarefaBruta {
                    nome: "Atualizar".into(),
                    caminho_tarefa: r"\Fornecedor\Atualizar".into(),
                    executavel: Some(r"C:\Program Files\F\up.exe".into()),
                    habilitada: false,
                },
                TarefaBruta {
                    nome: "Sistema".into(),
                    caminho_tarefa: r"\microsoft\Windows\X".into(),
                    executavel: None,
                    habilitada: true,
                },
            ]),
            ..Falso::default()
        };
        let l = ler_inicio_sessao(&f);
        assert!(l.fontes_indisponiveis.is_empty());
        let resumo: Vec<_> = l
            .entradas
            .iter()
            .map(|e| {
                (
                    e.fonte,
                    e.nome.as_str(),
                    e.executavel.as_str(),
                    e.habilitado,
                )
            })
            .collect();
        use FonteInicio::*;
        assert_eq!(
            resumo,
            [
                (RunUsuario, "Chat", "chat.exe", Some(true)),
                (RunUsuario, "Desligado", "off.exe", Some(false)),
                (RunUsuario, "Estranho", "e.exe", None),
                (PastaUsuario, "Atalho", "a.exe", Some(false)),
                (PastaUsuario, "Quebrado", DESCONHECIDO, Some(true)),
                (Tarefa, "Atualizar", "up.exe", Some(false)),
            ]
        );
        for e in &l.entradas {
            e.programa(SituacaoInicio::NaBase, 1).validar().unwrap();
            assert!(!format!("{e:?}").contains("fulano"));
        }
    }

    #[test]
    fn fonte_indisponivel_nao_e_fatal() {
        let f = Falso {
            run_usuario: Some(vec![par("A", "a.exe")]),
            aprov_run: None,
            pasta: None,
            tarefas: None,
            ..Falso::default()
        };
        let l = ler_inicio_sessao(&f);
        assert_eq!(l.entradas.len(), 1);
        assert_eq!(l.entradas[0].habilitado, None);
        assert_eq!(
            l.fontes_indisponiveis,
            [FonteInicio::PastaUsuario, FonteInicio::Tarefa]
        );
    }

    #[test]
    fn maquina_nao_le_fontes_da_sessao_e_respeita_limite() {
        let f = Falso {
            run_usuario: Some(vec![par("Usuario", "u.exe")]),
            run_maquina: Some(
                (0..PROGRAMAS_INICIO_MAX + 10)
                    .map(|i| (format!(r"p\{i}"), "x.exe".to_owned()))
                    .collect(),
            ),
            tarefas: Some(vec![]),
            ..Falso::default()
        };
        let l = ler_inicio_maquina(&f);
        assert_eq!(l.entradas.len(), PROGRAMAS_INICIO_MAX);
        assert!(
            l.entradas
                .iter()
                .all(|e| e.fonte == FonteInicio::RunMaquina)
        );
        assert_eq!(l.entradas[0].nome, "p_0");
    }
}
