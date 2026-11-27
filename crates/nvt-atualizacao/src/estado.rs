//! Estado persistido (§6.3) e máquina de estados (§7) como função pura
//! [`transicao`]`(estado, evento, contexto) -> (estado, efeitos)`.
//!
//! O serviço aplica os efeitos **na ordem** devolvida (ex.: `Persistir` antes de
//! `Baixar`/`Lancar`; se persistir falhar, não executa os seguintes).

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::manifesto::{ErroVerificacao, Manifesto, nome_log, url_msi};
use crate::politica::{
    self, Decisao, FALHAS_PARA_ABANDONAR, INSTALANDO_MAX_S, PRONTA_MAX_S, REAVALIAR_MSI_OCUPADO,
    REAVALIAR_PORTAO, RELER_LOG, ResultadoMsi,
};
use crate::versao::Versao;

pub const FORMATO_ESTADO: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UltimoResultado {
    SemNovidade,
    Baixada,
    AdiadaPortao,
    AdiadaMsiOcupado,
    ErroRede,
    ErroLimite,
    ErroAssinatura,
    ErroManifesto,
    ErroDisco,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessaoAgente {
    pub sessao: u32,
    pub sid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtualizacaoConcluida {
    pub de: Versao,
    pub para: Versao,
    pub quando_unix: u64,
    pub codigo: i64,
}

impl AtualizacaoConcluida {
    pub fn reinicio_pendente(&self) -> bool {
        matches!(self.codigo, 3010 | 1641)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FalhaRegistrada {
    pub versao: Versao,
    pub codigo: i64,
    pub quando_unix: u64,
}

/// `fase.tipo` (§7).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum Fase {
    #[default]
    Ocioso,
    Verificando,
    Baixando {
        versao: Versao,
        msi: String,
        tamanho: u64,
        sha256: String,
    },
    Pronta {
        versao: Versao,
        msi: String,
        sha256: String,
        desde_unix: u64,
        #[serde(default)]
        adiada_desde_unix: Option<u64>,
    },
    Instalando {
        de: Versao,
        para: Versao,
        sha256: String,
        msi: String,
        log: String,
        #[serde(default)]
        pid: Option<u32>,
        inicio_unix: u64,
        #[serde(default)]
        sessoes_agente: Vec<SessaoAgente>,
    },
}

/// `estado.json` (§6.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstadoPersistido {
    pub formato: u32,
    #[serde(default)]
    pub maior_vista: Option<Versao>,
    #[serde(default)]
    pub ultima_verificacao_unix: Option<u64>,
    #[serde(default)]
    pub ultimo_resultado: Option<UltimoResultado>,
    #[serde(default)]
    pub proxima_verificacao_unix: Option<u64>,
    #[serde(default)]
    pub espera_rede_s: u64,
    #[serde(default)]
    pub falhas: BTreeMap<Versao, u8>,
    #[serde(default)]
    pub abandonadas: Vec<Versao>,
    #[serde(default)]
    pub fase: Fase,
    #[serde(default)]
    pub ultima_atualizacao: Option<AtualizacaoConcluida>,
    #[serde(default)]
    pub ultima_falha: Option<FalhaRegistrada>,
}

impl Default for EstadoPersistido {
    fn default() -> Self {
        EstadoPersistido {
            formato: FORMATO_ESTADO,
            maior_vista: None,
            ultima_verificacao_unix: None,
            ultimo_resultado: None,
            proxima_verificacao_unix: None,
            espera_rede_s: 0,
            falhas: BTreeMap::new(),
            abandonadas: Vec::new(),
            fase: Fase::Ocioso,
            ultima_atualizacao: None,
            ultima_falha: None,
        }
    }
}

fn ler(bytes: &[u8]) -> Option<EstadoPersistido> {
    serde_json::from_slice::<EstadoPersistido>(bytes)
        .ok()
        .filter(|e| e.formato == FORMATO_ESTADO)
}

/// Só `maior_vista` de um `estado.json.bak` mesmo que o resto esteja ilegível.
fn maior_vista_de(bytes: &[u8]) -> Option<Versao> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    v.get("maior_vista")?.as_str()?.parse().ok()
}

impl EstadoPersistido {
    /// Recupera o estado de `estado.json` e `estado.json.bak` (bytes lidos pelo
    /// chamador). Ilegível → vazio, exceto `maior_vista`, que vem do maior entre os dois
    /// e nunca fica abaixo da `instalada` (U-18).
    pub fn carregar(
        principal: Option<&[u8]>,
        bak: Option<&[u8]>,
        instalada: Versao,
    ) -> EstadoPersistido {
        let mut e = principal.and_then(ler).unwrap_or_default();
        let do_bak = bak.and_then(maior_vista_de);
        e.maior_vista = [e.maior_vista, do_bak, Some(instalada)]
            .into_iter()
            .flatten()
            .max();
        e
    }

    /// Bytes para `estado.json` **e** `estado.json.bak` (mesmo conteúdo, a cada mudança).
    pub fn serializar(&self) -> Vec<u8> {
        serde_json::to_vec_pretty(self).expect("estado serializável")
    }
}

// ---------------------------------------------------------------- transições

/// Dados do instante da transição (injetados).
#[derive(Debug, Clone, Copy)]
pub struct Contexto {
    /// Relógio de parede, só para registro e prazos persistidos.
    pub agora_unix: u64,
    /// `env!("CARGO_PKG_VERSION")` do serviço.
    pub instalada: Versao,
    /// Jitter do RNG do sistema.
    pub aleatorio: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotivoAdiamento {
    JogoAberto,
    ListaIndisponivel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FalhaDownload {
    /// Rede/HTTP: espera exponencial.
    Rede,
    /// Tamanho ou SHA-256 divergente: 6 h.
    Verificacao,
    /// E/S local: espera exponencial (U-24).
    Disco,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evento {
    /// Serviço iniciou; `codigo_log` = código do log `/l*v` se a fase for `instalando`.
    Iniciado {
        codigo_log: Option<i64>,
    },
    PrazoVerificacao,
    /// Falha de rede/HTTP. `limite` = 403/429 (`Retry-After`/`x-ratelimit-reset` em s).
    FalhaRede {
        limite: bool,
        retry_after_s: Option<u64>,
        reset_s: Option<u64>,
    },
    /// 404 do manifesto (U-07).
    ManifestoAusente,
    ManifestoRecusado(ErroVerificacao),
    ManifestoValido(Manifesto),
    FalhaDownload(FalhaDownload),
    MsiVerificado,
    PortaoAdiou(MotivoAdiamento),
    /// `Global\_MSIExecute` existe.
    MsiOcupado,
    PortaoLiberou {
        sessoes_agente: Vec<SessaoAgente>,
    },
    /// Handle perdido ou arquivo alterado em `pronta`.
    ArquivoPerdido,
    Lancado {
        pid: u32,
    },
    LancamentoFalhou {
        codigo_win32: i64,
    },
    LogRelido {
        codigo_log: Option<i64>,
    },
    /// Serviço antigo ainda vivo viu o msiexec terminar.
    MsiexecTerminou {
        codigo: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PedidoDownload {
    pub versao: Versao,
    /// Derivada da versão (§6.1).
    pub url: String,
    pub msi: String,
    pub tamanho: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PedidoLancamento {
    pub de: Versao,
    pub para: Versao,
    pub msi: String,
    pub log: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Efeito {
    /// Gravar `estado.json` + `.bak`; falha interrompe os efeitos seguintes.
    Persistir,
    BuscarManifesto,
    Baixar(PedidoDownload),
    /// Consultar portão e `_MSIExecute` agora.
    ConsultarPortao,
    Lancar(PedidoLancamento),
    /// Apagar `.parcial`/MSI e soltar o handle.
    DescartarMsi,
    RelancarAgentes(Vec<SessaoAgente>),
    /// Próxima verificação (relógio monotônico).
    AgendarVerificacao(Duration),
    /// Reavaliar a fase atual (portão, `_MSIExecute`, log) após o prazo.
    Reavaliar(Duration),
    Registrar(String),
}

struct Saida<'a> {
    e: EstadoPersistido,
    ef: Vec<Efeito>,
    ctx: &'a Contexto,
}

impl Saida<'_> {
    fn agendar(&mut self, d: Duration) {
        self.e.proxima_verificacao_unix = Some(self.ctx.agora_unix + d.as_secs());
        self.ef.push(Efeito::AgendarVerificacao(d));
    }

    fn regular(&mut self) {
        self.agendar(politica::intervalo_regular(self.ctx.aleatorio));
    }

    fn rede(&mut self) {
        self.e.espera_rede_s = politica::proxima_espera_rede(self.e.espera_rede_s);
        self.agendar(Duration::from_secs(self.e.espera_rede_s));
    }

    fn verificou(&mut self, r: UltimoResultado) {
        self.e.ultima_verificacao_unix = Some(self.ctx.agora_unix);
        self.e.ultimo_resultado = Some(r);
        self.e.espera_rede_s = 0;
    }

    /// Conta falha da versão; abandona na 3ª ou já (`abandonar`).
    fn falha(&mut self, versao: Versao, codigo: i64, abandonar: bool) {
        let n = self.e.falhas.entry(versao).or_insert(0);
        *n = n.saturating_add(1);
        let n = *n;
        self.e.ultima_falha = Some(FalhaRegistrada {
            versao,
            codigo,
            quando_unix: self.ctx.agora_unix,
        });
        if abandonar || n >= FALHAS_PARA_ABANDONAR {
            self.e.falhas.remove(&versao);
            if !self.e.abandonadas.contains(&versao) {
                self.e.abandonadas.push(versao);
            }
            self.ef.push(Efeito::Registrar(format!(
                "versão {versao} abandonada (código {codigo}, {n} falha(s))"
            )));
        }
    }
}

/// Aplica um evento. Eventos fora da fase esperada não mudam nada.
pub fn transicao(
    estado: &EstadoPersistido,
    evento: Evento,
    ctx: &Contexto,
) -> (EstadoPersistido, Vec<Efeito>) {
    let mut s = Saida {
        e: estado.clone(),
        ef: Vec::new(),
        ctx,
    };
    let fase = std::mem::take(&mut s.e.fase);
    s.e.fase = fase.clone();
    match (fase, evento) {
        (fase, Evento::Iniciado { codigo_log }) => iniciar(&mut s, fase, codigo_log),

        (Fase::Ocioso, Evento::PrazoVerificacao) => {
            s.e.fase = Fase::Verificando;
            s.ef.push(Efeito::BuscarManifesto);
        }

        (
            Fase::Verificando,
            Evento::FalhaRede {
                limite,
                retry_after_s,
                reset_s,
            },
        ) => {
            s.e.fase = Fase::Ocioso;
            s.e.ultima_verificacao_unix = Some(ctx.agora_unix);
            let exp = politica::proxima_espera_rede(s.e.espera_rede_s);
            if limite {
                s.e.ultimo_resultado = Some(UltimoResultado::ErroLimite);
                s.e.espera_rede_s = exp;
                s.agendar(Duration::from_secs(politica::espera_limite(
                    exp,
                    retry_after_s,
                    reset_s,
                )));
            } else {
                s.e.ultimo_resultado = Some(UltimoResultado::ErroRede);
                s.rede();
            }
            s.ef.insert(0, Efeito::Persistir);
        }

        (Fase::Verificando, Evento::ManifestoAusente) => {
            s.e.fase = Fase::Ocioso;
            s.verificou(UltimoResultado::SemNovidade);
            s.ef.push(Efeito::Persistir);
            s.regular();
        }

        (Fase::Verificando, Evento::ManifestoRecusado(erro)) => {
            s.e.fase = Fase::Ocioso;
            s.verificou(erro.resultado());
            s.ef.push(Efeito::Persistir);
            s.ef.push(Efeito::Registrar(format!("manifesto recusado: {erro}")));
            s.regular();
        }

        (Fase::Verificando, Evento::ManifestoValido(m)) => {
            match politica::decidir_instalacao(
                m.versao,
                ctx.instalada,
                s.e.maior_vista,
                &s.e.abandonadas,
            ) {
                Decisao::Instalar => {
                    s.verificou(UltimoResultado::SemNovidade);
                    s.e.ultimo_resultado = None;
                    s.e.maior_vista = s.e.maior_vista.max(Some(m.versao));
                    s.e.fase = Fase::Baixando {
                        versao: m.versao,
                        msi: m.msi.clone(),
                        tamanho: m.tamanho,
                        sha256: m.sha256.clone(),
                    };
                    s.ef.push(Efeito::Persistir);
                    s.ef.push(Efeito::Baixar(PedidoDownload {
                        versao: m.versao,
                        url: url_msi(m.versao),
                        msi: m.msi,
                        tamanho: m.tamanho,
                        sha256: m.sha256,
                    }));
                }
                _ => {
                    s.e.fase = Fase::Ocioso;
                    s.verificou(UltimoResultado::SemNovidade);
                    s.ef.push(Efeito::Persistir);
                    s.regular();
                }
            }
        }

        (Fase::Baixando { .. }, Evento::FalhaDownload(tipo)) => {
            s.e.fase = Fase::Ocioso;
            s.ef.push(Efeito::DescartarMsi);
            match tipo {
                FalhaDownload::Rede => {
                    s.e.ultimo_resultado = Some(UltimoResultado::ErroRede);
                    s.ef.push(Efeito::Persistir);
                    s.rede();
                }
                FalhaDownload::Disco => {
                    s.e.ultimo_resultado = Some(UltimoResultado::ErroDisco);
                    s.ef.push(Efeito::Persistir);
                    s.rede();
                }
                FalhaDownload::Verificacao => {
                    s.e.ultimo_resultado = Some(UltimoResultado::ErroAssinatura);
                    s.ef.push(Efeito::Persistir);
                    s.regular();
                }
            }
        }

        (
            Fase::Baixando {
                versao,
                msi,
                sha256,
                ..
            },
            Evento::MsiVerificado,
        ) => {
            s.e.ultimo_resultado = Some(UltimoResultado::Baixada);
            s.e.espera_rede_s = 0;
            s.e.fase = Fase::Pronta {
                versao,
                msi,
                sha256,
                desde_unix: ctx.agora_unix,
                adiada_desde_unix: None,
            };
            s.ef.push(Efeito::Persistir);
            s.ef.push(Efeito::ConsultarPortao);
        }

        (Fase::Pronta { desde_unix, .. }, Evento::PortaoAdiou(_) | Evento::MsiOcupado)
        | (Fase::Pronta { desde_unix, .. }, Evento::PortaoLiberou { .. })
            if ctx.agora_unix.saturating_sub(desde_unix) > PRONTA_MAX_S =>
        {
            descartar_pronta(&mut s);
        }

        (Fase::Pronta { .. }, Evento::ArquivoPerdido) => descartar_pronta(&mut s),

        (
            Fase::Pronta {
                versao,
                msi,
                sha256,
                desde_unix,
                adiada_desde_unix,
            },
            ev @ (Evento::PortaoAdiou(_) | Evento::MsiOcupado),
        ) => {
            let (r, prazo) = if ev == Evento::MsiOcupado {
                (UltimoResultado::AdiadaMsiOcupado, REAVALIAR_MSI_OCUPADO)
            } else {
                (UltimoResultado::AdiadaPortao, REAVALIAR_PORTAO)
            };
            let mudou = s.e.ultimo_resultado != Some(r) || adiada_desde_unix.is_none();
            s.e.ultimo_resultado = Some(r);
            s.e.fase = Fase::Pronta {
                versao,
                msi,
                sha256,
                desde_unix,
                adiada_desde_unix: adiada_desde_unix.or(Some(ctx.agora_unix)),
            };
            if mudou {
                s.ef.push(Efeito::Persistir);
            }
            s.ef.push(Efeito::Reavaliar(prazo));
        }

        (
            Fase::Pronta {
                versao,
                msi,
                sha256,
                ..
            },
            Evento::PortaoLiberou { sessoes_agente },
        ) => {
            let log = nome_log(versao);
            s.e.fase = Fase::Instalando {
                de: ctx.instalada,
                para: versao,
                sha256: sha256.clone(),
                msi: msi.clone(),
                log: log.clone(),
                pid: None,
                inicio_unix: ctx.agora_unix,
                sessoes_agente,
            };
            s.ef.push(Efeito::Persistir);
            s.ef.push(Efeito::Lancar(PedidoLancamento {
                de: ctx.instalada,
                para: versao,
                msi,
                log,
                sha256,
            }));
        }

        (Fase::Instalando { .. }, Evento::Lancado { pid: novo }) => {
            if let Fase::Instalando { pid, .. } = &mut s.e.fase {
                *pid = Some(novo);
            }
            s.ef.push(Efeito::Persistir);
        }

        (Fase::Instalando { para, .. }, Evento::LancamentoFalhou { codigo_win32 }) => {
            s.e.fase = Fase::Ocioso;
            s.falha(para, codigo_win32, false);
            s.ef.insert(0, Efeito::Persistir);
            s.ef.push(Efeito::DescartarMsi);
            s.regular();
        }

        (fase @ Fase::Instalando { .. }, Evento::LogRelido { codigo_log }) => {
            resolver(&mut s, fase, codigo_log, false);
        }

        (fase @ Fase::Instalando { .. }, Evento::MsiexecTerminou { codigo }) => {
            resolver(&mut s, fase, Some(codigo), false);
        }

        _ => {}
    }
    (s.e, s.ef)
}

fn descartar_pronta(s: &mut Saida<'_>) {
    s.e.fase = Fase::Ocioso;
    s.ef.push(Efeito::DescartarMsi);
    s.ef.push(Efeito::Persistir);
    s.agendar(Duration::ZERO);
}

fn iniciar(s: &mut Saida<'_>, fase: Fase, codigo_log: Option<i64>) {
    s.e.maior_vista = s.e.maior_vista.max(Some(s.ctx.instalada));
    match fase {
        f @ Fase::Instalando { .. } => resolver(s, f, codigo_log, true),
        _ => {
            s.e.fase = Fase::Ocioso;
            s.ef.push(Efeito::Persistir);
            s.agendar(politica::atraso_inicial(s.ctx.aleatorio));
        }
    }
}

/// Resultado de uma instalação a partir da versão em execução e do código (U-35..U-38).
fn resolver(s: &mut Saida<'_>, fase: Fase, codigo: Option<i64>, ao_iniciar: bool) {
    let Fase::Instalando {
        de,
        para,
        inicio_unix,
        sessoes_agente,
        ..
    } = fase
    else {
        return;
    };
    let proxima = |s: &mut Saida<'_>| {
        if ao_iniciar {
            s.agendar(politica::atraso_inicial(s.ctx.aleatorio));
        } else {
            s.regular();
        }
    };
    if s.ctx.instalada == para {
        let codigo = codigo
            .filter(|c| {
                matches!(
                    politica::interpretar_codigo_msi(*c),
                    ResultadoMsi::Sucesso { .. }
                )
            })
            .unwrap_or(0);
        s.e.fase = Fase::Ocioso;
        s.e.falhas.remove(&para);
        s.e.ultimo_resultado = None;
        let concl = AtualizacaoConcluida {
            de,
            para,
            quando_unix: s.ctx.agora_unix,
            codigo,
        };
        let pendente = concl.reinicio_pendente();
        s.e.ultima_atualizacao = Some(concl);
        s.ef.push(Efeito::Persistir);
        s.ef.push(Efeito::Registrar(format!(
            "atualizado de {de} para {para} (código {codigo}{})",
            if pendente {
                "; reinício do Windows pendente"
            } else {
                ""
            }
        )));
        if !sessoes_agente.is_empty() {
            s.ef.push(Efeito::RelancarAgentes(sessoes_agente));
        }
        proxima(s);
        return;
    }
    let decorrido = s.ctx.agora_unix.saturating_sub(inicio_unix);
    let resultado = codigo.map(politica::interpretar_codigo_msi);
    match (codigo, resultado) {
        (Some(_), Some(ResultadoMsi::Ocupado)) => {
            s.e.fase = Fase::Ocioso;
            s.e.ultimo_resultado = Some(UltimoResultado::AdiadaMsiOcupado);
            s.ef.push(Efeito::Persistir);
            s.agendar(REAVALIAR_MSI_OCUPADO);
        }
        (Some(c), Some(ResultadoMsi::Politica)) => {
            s.e.fase = Fase::Ocioso;
            s.falha(para, c, true);
            s.ef.insert(0, Efeito::Persistir);
            proxima(s);
        }
        (Some(c), Some(ResultadoMsi::Falha)) => {
            s.e.fase = Fase::Ocioso;
            s.falha(para, c, false);
            s.ef.insert(0, Efeito::Persistir);
            proxima(s);
        }
        // Sem código (ou sucesso ainda sem a versão nova): espera até 2 h.
        _ if decorrido <= INSTALANDO_MAX_S => {
            if ao_iniciar {
                s.ef.push(Efeito::Persistir);
            }
            s.ef.push(Efeito::Reavaliar(RELER_LOG));
        }
        _ => {
            s.e.fase = Fase::Ocioso;
            s.falha(para, -1, false);
            s.ef.insert(0, Efeito::Persistir);
            proxima(s);
        }
    }
}

#[cfg(test)]
mod testes;
