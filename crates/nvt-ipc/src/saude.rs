//! DTOs de saúde do PC e de partidas (v8, `docs/spec/interface-e-saude.md` §7).
//!
//! Nada aqui carrega caminho, linha de comando, título de janela, nome de
//! usuário, SID ou identificador de conta: programas de inicialização viajam
//! só com nome exibido e nome do executável; a limpeza só com contagens.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::usuario::validar_texto;

/// Máximo de programas de inicialização num relato ou numa resposta.
pub const PROGRAMAS_INICIO_MAX: usize = 256;
/// Comprimento máximo (bytes UTF-8) do nome exibido de um programa.
pub const NOME_INICIO_MAX: usize = 128;
/// Comprimento máximo (bytes UTF-8) do nome do executável.
pub const EXECUTAVEL_MAX: usize = 64;
/// Máximo de sinais num relato ou numa resposta.
pub const SINAIS_MAX: usize = 16;
/// Máximo de resumos pedidos em `ListarPartidas`.
pub const PARTIDAS_MAX: u32 = 200;
/// Máximo de amostras por segundo numa resposta `Partida` (cabe no quadro).
pub const AMOSTRAS_MAX: usize = 4096;
/// Faixas do histograma de frametime.
pub const FAIXAS_HISTOGRAMA: usize = 64;

/// Nome sem separador de caminho nem `:` (impede caminho com nome de usuário).
fn validar_nome_simples(texto: &str, limite: usize) -> Result<(), String> {
    validar_texto(texto, limite)?;
    if texto.contains(['\\', '/', ':']) {
        return Err("nome não pode conter separador de caminho".into());
    }
    Ok(())
}

/// Validação do nome em `MarcarInicioVisto`.
pub fn validar_nome_inicio(nome: &str) -> Result<(), String> {
    validar_nome_simples(nome, NOME_INICIO_MAX)
}

// ── Limpeza (§7.1) ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModoLimpeza {
    Desligada,
    /// Dry-run: percorre e conta, nunca apaga.
    Simulacao,
    Ativa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlvoLimpeza {
    TempUsuario,
    TempWindows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotivoPulo {
    Recente,
    ReparsePoint,
    SomenteLeitura,
    Sistema,
    EmUso,
    AcessoNegado,
    Limite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotivoInterrupcao {
    JogoAberto,
    Instalacao,
    Tempo,
    Erro,
}

/// Resultado de uma passada de limpeza: só contagens e bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultadoLimpeza {
    pub modo: ModoLimpeza,
    pub alvo: AlvoLimpeza,
    pub inicio_unix: u64,
    pub duracao_ms: u32,
    /// Apagados (`Ativa`) ou que seriam apagados (`Simulacao`).
    pub arquivos: u32,
    pub bytes: u64,
    pub pulados: BTreeMap<MotivoPulo, u32>,
    pub interrompida: Option<MotivoInterrupcao>,
}

impl ResultadoLimpeza {
    pub fn validar(&self) -> Result<(), String> {
        if self.modo == ModoLimpeza::Desligada && (self.arquivos != 0 || self.bytes != 0) {
            return Err("limpeza desligada não percorre nada".into());
        }
        Ok(())
    }
}

// ── Inicialização (§7.2) ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FonteInicio {
    RunUsuario,
    RunOnceUsuario,
    RunMaquina,
    RunOnceMaquina,
    RunMaquina32,
    PastaUsuario,
    PastaComum,
    Tarefa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SituacaoInicio {
    NaBase,
    Novo,
    Mudou,
}

/// Programa de inicialização (somente leitura; o comando nunca sai do agente).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramaInicio {
    pub fonte: FonteInicio,
    /// Nome do valor, do atalho ou da tarefa.
    pub nome: String,
    /// Só o nome do arquivo, sem pasta.
    pub executavel: String,
    /// StartupApproved; `None` = desconhecido.
    pub habilitado: Option<bool>,
    pub situacao: SituacaoInicio,
    pub detectado_unix: u64,
}

impl ProgramaInicio {
    pub fn validar(&self) -> Result<(), String> {
        validar_nome_simples(&self.nome, NOME_INICIO_MAX)?;
        validar_nome_simples(&self.executavel, EXECUTAVEL_MAX)
    }
}

// ── Sinais (§7.3) ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoSinal {
    EspacoLivre,
    MemoriaRepouso,
    CpuRepouso,
    DriverDesatualizado,
    PlanoEnergia,
    SensorArmazenamento,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gravidade {
    Ok,
    Informativo,
    Atencao,
    Indisponivel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ValorSinal {
    Bytes { livres: u64, total: u64 },
    Percentual(u8),
    Versao { atual: u32, ultima: u32 },
    Booleano(bool),
    Nenhum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sinal {
    pub tipo: TipoSinal,
    pub gravidade: Gravidade,
    pub valor: ValorSinal,
    pub lido_unix: u64,
}

impl Sinal {
    pub fn validar(&self) -> Result<(), String> {
        match self.valor {
            ValorSinal::Bytes { livres, total } if livres > total => {
                Err("espaço livre maior que o total".into())
            }
            ValorSinal::Percentual(p) if p > 100 => Err("percentual acima de 100".into()),
            _ => Ok(()),
        }
    }
}

fn validar_conjunto(
    sinais: &[Sinal],
    inicializacao: &[ProgramaInicio],
    limpezas: &[&ResultadoLimpeza],
) -> Result<(), String> {
    if sinais.len() > SINAIS_MAX {
        return Err("muitos sinais".into());
    }
    let mut tipos = BTreeSet::new();
    for s in sinais {
        s.validar()?;
        if !tipos.insert(s.tipo) {
            return Err("sinal repetido".into());
        }
    }
    if inicializacao.len() > PROGRAMAS_INICIO_MAX {
        return Err("muitos programas de inicialização".into());
    }
    let mut ids = BTreeSet::new();
    for p in inicializacao {
        p.validar()?;
        if !ids.insert((p.fonte, p.nome.as_str())) {
            return Err("programa de inicialização repetido".into());
        }
    }
    let mut alvos = BTreeSet::new();
    for l in limpezas {
        l.validar()?;
        if !alvos.insert(l.alvo) {
            return Err("limpeza repetida para o mesmo alvo".into());
        }
    }
    Ok(())
}

/// Saúde lida pelo agente na sessão do usuário (`AgenteRelatar.saude`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaudeSessao {
    pub sinais: Vec<Sinal>,
    pub inicializacao: Vec<ProgramaInicio>,
    /// Última passada em `%TEMP%` do usuário.
    pub limpeza: Option<ResultadoLimpeza>,
}

impl SaudeSessao {
    pub fn validar(&self) -> Result<(), String> {
        if let Some(l) = &self.limpeza
            && l.alvo != AlvoLimpeza::TempUsuario
        {
            return Err("o agente só limpa o temporário do usuário".into());
        }
        validar_conjunto(
            &self.sinais,
            &self.inicializacao,
            &self.limpeza.iter().collect::<Vec<_>>(),
        )
    }
}

/// Resposta de `ObterSaude`: dados do SID chamador e da máquina.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Saude {
    pub sinais: Vec<Sinal>,
    pub inicializacao: Vec<ProgramaInicio>,
    /// Modo vigente neste build (dry-run = `Simulacao`).
    pub modo_limpeza: ModoLimpeza,
    /// Última passada por alvo (no máximo uma de cada).
    pub limpeza: Vec<ResultadoLimpeza>,
}

impl Saude {
    pub fn validar(&self) -> Result<(), String> {
        validar_conjunto(
            &self.sinais,
            &self.inicializacao,
            &self.limpeza.iter().collect::<Vec<_>>(),
        )
    }
}

// ── Partidas (§7.4; o serviço só as atende com `medicao-partidas`) ─────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodigoErroMedicao {
    SessaoPerdida,
    EventosPerdidos,
    Interno,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumoPartida {
    pub id: u64,
    pub jogo_id: String,
    pub executavel: String,
    pub inicio_unix: u64,
    pub duracao_s: u32,
    pub quadros_total: u64,
    pub fps_medio: f32,
    pub fps_mediana: f32,
    pub fps_1pct_low: f32,
    pub fps_01pct_low: f32,
    pub engasgos: u32,
    pub tempo_engasgado_ms: u32,
    pub histograma: Vec<u32>,
    pub gpu_temp_max_c: Option<u8>,
    pub gpu_uso_medio_pct: Option<u8>,
    pub perda_eventos_pct: f32,
    pub encerrada_por_erro: Option<CodigoErroMedicao>,
    pub versao_formato: u16,
}

impl ResumoPartida {
    pub fn validar(&self) -> Result<(), String> {
        validar_nome_simples(&self.jogo_id, 128)?;
        validar_nome_simples(&self.executavel, EXECUTAVEL_MAX)?;
        if self.histograma.len() != FAIXAS_HISTOGRAMA {
            return Err("histograma com número de faixas inválido".into());
        }
        let finitos = [
            self.fps_medio,
            self.fps_mediana,
            self.fps_1pct_low,
            self.fps_01pct_low,
            self.perda_eventos_pct,
        ];
        if finitos.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err("métrica de partida inválida".into());
        }
        if !(0.0..=100.0).contains(&self.perda_eventos_pct)
            || self.gpu_uso_medio_pct.is_some_and(|p| p > 100)
        {
            return Err("percentual fora de faixa".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmostraSegundo {
    pub t_rel_s: u32,
    pub quadros: u16,
    pub frametime_medio_us: u32,
    pub frametime_max_us: u32,
    pub engasgos: u8,
    pub gpu_uso_pct: Option<u8>,
    pub gpu_temp_c: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Partida {
    pub resumo: ResumoPartida,
    pub amostras: Vec<AmostraSegundo>,
}

impl Partida {
    pub fn validar(&self) -> Result<(), String> {
        self.resumo.validar()?;
        if self.amostras.len() > AMOSTRAS_MAX {
            return Err("muitas amostras".into());
        }
        if self
            .amostras
            .iter()
            .any(|a| a.gpu_uso_pct.is_some_and(|p| p > 100))
        {
            return Err("percentual fora de faixa".into());
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;

    pub(crate) fn programa() -> ProgramaInicio {
        ProgramaInicio {
            fonte: FonteInicio::RunUsuario,
            nome: "Discord".into(),
            executavel: "Update.exe".into(),
            habilitado: Some(true),
            situacao: SituacaoInicio::Novo,
            detectado_unix: 1_700_000_000,
        }
    }

    pub(crate) fn limpeza(alvo: AlvoLimpeza) -> ResultadoLimpeza {
        ResultadoLimpeza {
            modo: ModoLimpeza::Simulacao,
            alvo,
            inicio_unix: 1_700_000_000,
            duracao_ms: 1200,
            arquivos: 42,
            bytes: 1 << 30,
            pulados: BTreeMap::from([(MotivoPulo::Recente, 3), (MotivoPulo::EmUso, 1)]),
            interrompida: None,
        }
    }

    pub(crate) fn sinal(tipo: TipoSinal, valor: ValorSinal) -> Sinal {
        Sinal {
            tipo,
            gravidade: Gravidade::Informativo,
            valor,
            lido_unix: 1_700_000_000,
        }
    }

    pub(crate) fn saude_sessao() -> SaudeSessao {
        SaudeSessao {
            sinais: vec![sinal(
                TipoSinal::SensorArmazenamento,
                ValorSinal::Booleano(false),
            )],
            inicializacao: vec![programa()],
            limpeza: Some(limpeza(AlvoLimpeza::TempUsuario)),
        }
    }

    pub(crate) fn saude() -> Saude {
        Saude {
            sinais: vec![
                sinal(
                    TipoSinal::EspacoLivre,
                    ValorSinal::Bytes {
                        livres: 10,
                        total: 100,
                    },
                ),
                sinal(TipoSinal::CpuRepouso, ValorSinal::Percentual(7)),
                sinal(
                    TipoSinal::DriverDesatualizado,
                    ValorSinal::Versao {
                        atual: 56601,
                        ultima: 57102,
                    },
                ),
                sinal(TipoSinal::PlanoEnergia, ValorSinal::Nenhum),
            ],
            inicializacao: vec![programa()],
            modo_limpeza: ModoLimpeza::Simulacao,
            limpeza: vec![
                limpeza(AlvoLimpeza::TempUsuario),
                limpeza(AlvoLimpeza::TempWindows),
            ],
        }
    }

    pub(crate) fn partida() -> Partida {
        Partida {
            resumo: ResumoPartida {
                id: 9,
                jogo_id: "cs2".into(),
                executavel: "cs2.exe".into(),
                inicio_unix: 1_700_000_000,
                duracao_s: 1800,
                quadros_total: 400_000,
                fps_medio: 222.5,
                fps_mediana: 230.0,
                fps_1pct_low: 140.0,
                fps_01pct_low: 90.0,
                engasgos: 2,
                tempo_engasgado_ms: 80,
                histograma: vec![0; FAIXAS_HISTOGRAMA],
                gpu_temp_max_c: Some(71),
                gpu_uso_medio_pct: Some(88),
                perda_eventos_pct: 0.0,
                encerrada_por_erro: None,
                versao_formato: 1,
            },
            amostras: vec![AmostraSegundo {
                t_rel_s: 0,
                quadros: 220,
                frametime_medio_us: 4500,
                frametime_max_us: 9000,
                engasgos: 0,
                gpu_uso_pct: Some(90),
                gpu_temp_c: None,
            }],
        }
    }

    #[test]
    fn exemplares_validos() {
        saude_sessao().validar().unwrap();
        saude().validar().unwrap();
        partida().validar().unwrap();
    }

    #[test]
    fn nome_com_caminho_e_recusado() {
        for (nome, exe) in [
            (r"C:\Users\fulano\app", "app.exe"),
            ("app", r"C:\Users\fulano\app.exe"),
            ("app", "dir/app.exe"),
            ("", "app.exe"),
            ("app", ""),
            ("a\u{0}b", "app.exe"),
        ] {
            let p = ProgramaInicio {
                nome: nome.into(),
                executavel: exe.into(),
                ..programa()
            };
            assert!(p.validar().is_err(), "{nome:?} {exe:?}");
        }
        let longo = ProgramaInicio {
            nome: "x".repeat(NOME_INICIO_MAX + 1),
            ..programa()
        };
        assert!(longo.validar().is_err());
        let exe_longo = ProgramaInicio {
            executavel: format!("{}.exe", "x".repeat(EXECUTAVEL_MAX)),
            ..programa()
        };
        assert!(exe_longo.validar().is_err());
    }

    #[test]
    fn limites_de_quantidade() {
        let mut s = saude_sessao();
        s.inicializacao = (0..=PROGRAMAS_INICIO_MAX)
            .map(|i| ProgramaInicio {
                nome: format!("p{i}"),
                ..programa()
            })
            .collect();
        assert!(s.validar().is_err());
        s.inicializacao.pop();
        s.validar().unwrap();

        let mut s = saude_sessao();
        s.inicializacao.push(programa());
        assert!(s.validar().is_err(), "(fonte, nome) repetido");

        let mut s = saude_sessao();
        s.sinais.push(s.sinais[0]);
        assert!(s.validar().is_err(), "sinal repetido");

        let mut s = saude_sessao();
        s.limpeza = Some(limpeza(AlvoLimpeza::TempWindows));
        assert!(s.validar().is_err(), "agente não limpa C:\\Windows\\Temp");

        let mut t = saude();
        t.limpeza.push(limpeza(AlvoLimpeza::TempUsuario));
        assert!(t.validar().is_err(), "alvo repetido");
    }

    #[test]
    fn valores_fora_de_faixa() {
        assert!(
            sinal(TipoSinal::CpuRepouso, ValorSinal::Percentual(101))
                .validar()
                .is_err()
        );
        assert!(
            sinal(
                TipoSinal::EspacoLivre,
                ValorSinal::Bytes {
                    livres: 2,
                    total: 1
                }
            )
            .validar()
            .is_err()
        );
        let mut l = limpeza(AlvoLimpeza::TempUsuario);
        l.modo = ModoLimpeza::Desligada;
        assert!(l.validar().is_err());

        let mut p = partida();
        p.resumo.histograma.pop();
        assert!(p.validar().is_err());
        let mut p = partida();
        p.resumo.fps_medio = f32::NAN;
        assert!(p.validar().is_err());
        let mut p = partida();
        p.amostras = vec![p.amostras[0]; AMOSTRAS_MAX + 1];
        assert!(p.validar().is_err());
    }

    #[test]
    fn campos_desconhecidos_recusados() {
        let mut v = serde_json::to_value(programa()).unwrap();
        v["caminho"] = serde_json::json!(r"C:\Users\fulano\app.exe");
        assert!(serde_json::from_value::<ProgramaInicio>(v).is_err());

        let mut v = serde_json::to_value(saude_sessao()).unwrap();
        v["comando"] = serde_json::json!("x");
        assert!(serde_json::from_value::<SaudeSessao>(v).is_err());

        let mut v = serde_json::to_value(limpeza(AlvoLimpeza::TempUsuario)).unwrap();
        v["arquivos_nomes"] = serde_json::json!([]);
        assert!(serde_json::from_value::<ResultadoLimpeza>(v).is_err());
    }

    #[test]
    fn ida_e_volta() {
        let s = saude();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Saude>(&json).unwrap(), s);
        let p = partida();
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Partida>(&json).unwrap(), p);
    }
}
