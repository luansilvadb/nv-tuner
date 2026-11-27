//! Conversões do motor para os tipos IPC: plano, avisos, jogos, perfis e erros.

use super::*;

impl<'a, S: DriverStore> Nucleo<'a, S> {
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
        let mudanca = |alvo: &Alvo, c: &nvt_core::SettingChange, antes: Option<u32>, status| {
            let def = cat.get(&c.setting);
            let regra = c.origem.regra();
            let aceita = diff.plano.codigo_explicacao(alvo, &c.setting).is_some();
            Mudanca {
                alvo_tela: None,
                camada: nvt_ipc::Camada::Driver,
                experimental: if aceita {
                    nvt_ipc::EstadoExperimental::Aceita
                } else {
                    nvt_ipc::EstadoExperimental::Estavel
                },
                setting: c.setting.clone(),
                nome_legivel: def.map_or_else(|| c.setting.clone(), nomes::nome_setting),
                antes: nomes::valor(def, antes),
                depois: nomes::valor(def, Some(c.valor)),
                motivo: if aceita {
                    format!("{} (experimental aceita por você)", c.explicacao)
                } else {
                    c.explicacao.clone()
                },
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
                mudanca(&m.alvo, &m.mudanca, m.mudanca.valor_anterior, status),
            );
        }
        for m in &diff.conformes {
            push(
                &m.alvo,
                perfil_ipc(&m.perfil),
                mudanca(
                    &m.alvo,
                    &m.mudanca,
                    Some(m.mudanca.valor),
                    StatusMudanca::JaNoValor,
                ),
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
                mudanca(&r.alvo, &r.mudanca, None, StatusMudanca::Travada),
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
            sistema: Default::default(),
            sugestoes_driver: self
                .regras_drs
                .regras
                .iter()
                .filter(|regra| diff.plano.estado_experimental.contains_key(&regra.id))
                .map(sugestao_drs)
                .collect(),
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
pub(crate) fn sugestao_drs(regra: &nvt_core::Rule) -> nvt_ipc::SugestaoExperimental {
    nvt_ipc::SugestaoExperimental {
        regra: regra.id.clone(),
        descricao: regra.descricao.clone(),
        camada: nvt_ipc::Camada::Driver,
        impacto_visual: match regra.impacto_visual {
            ImpactoRegra::Nenhum => nvt_ipc::ImpactoVisual::Nenhum,
            ImpactoRegra::Leve => nvt_ipc::ImpactoVisual::Baixo,
            ImpactoRegra::Forte => nvt_ipc::ImpactoVisual::Alto,
        },
        estado: nvt_ipc::EstadoExperimental::Aceitavel,
    }
}

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
                config_jogo: None,
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
