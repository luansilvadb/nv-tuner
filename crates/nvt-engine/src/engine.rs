//! O motor: plano → diff contra o DRS → aplicação verificada → histórico,
//! detecção de deriva e restauração do snapshot original.
mod construcao;
mod deriva;
mod planejamento;
mod restauracao;

use std::collections::BTreeSet;

use nvt_core::{
    Alvo, BaseJogos, Capabilities, ConjuntoRegras, DefSetting, ErroRegra, ErrosRegras, GameFacts,
    Plan, Preferences, SettingChange,
};
use nvt_drs::{ProfileRef, RestoreOp, Setting, SettingValue};
use serde::{Deserialize, Serialize};

use crate::deteccao::{JogoEntrada, capacidades, casar_na_base, fatos_do_jogo};
use crate::erro::{ErroEngine, Resultado};
use crate::estado::{
    Armazem, Esperado, Estado, OrigemRegistro, PerfilCriado, RegistroHistorico, Travada,
    ValorAntes, agora_iso, mesmo_alvo,
};
use crate::portao::{Adiamento, Guardado, Licenca, Portao};
use crate::store::DriverStore;

/// Nome com que o histórico e os relatórios mostram o perfil global.
pub const NOME_PERFIL_BASE: &str = "Base Profile";

/// Reset em massa: mínimo absoluto de settings geridas que voltaram ao padrão.
pub const LIMIAR_RESET_MIN: usize = 3;
/// Reset em massa: fração mínima (em %) das settings geridas que voltaram ao padrão.
pub const LIMIAR_RESET_PCT: usize = 50;

/// Máximo de unidades UTF-16 do nome de um perfil criado pelo app. O DRS aceita
/// até 2047; o nome vindo do launcher é truncado aqui, com folga para o sufixo
/// ` (nv-tuner N)`.
pub const MAX_NOME_PERFIL: usize = 200;
/// Máximo de unidades UTF-16 do executável de um jogo (`MAX_PATH`).
pub const MAX_EXE: usize = 260;
/// Tentativas de sufixo ` (nv-tuner N)` antes de desistir de um jogo.
const MAX_SUFIXOS: usize = 100;

/// Jogo deixado de fora do plano por um erro só dele (nome/executável inválido,
/// falha da NVAPI nesse perfil). Os demais jogos e o perfil global seguem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JogoIgnorado {
    pub nome: String,
    pub exe: String,
    pub motivo: String,
}

/// O que fazer com uma setting que o app gravou e que saiu do plano.
#[derive(Debug, Clone, PartialEq)]
pub enum AcaoReversao {
    /// Grava o valor de antes da gestão ([`Esperado::antes_da_gestao`]; em
    /// estado antigo, o do snapshot original).
    Gravar(SettingValue),
    /// Remove a setting do perfil (não existia no original); volta ao predefinido
    /// da NVIDIA quando `predefinido_valido`.
    Remover { predefinido_valido: bool },
    /// O driver já está como no original (ou o original é desconhecido, ou o
    /// perfil sumiu): só deixa de vigiar.
    SoDeixarDeVigiar,
}

/// Setting gerida que o plano atual não quer mais (preferência trocada, teto de
/// qualidade, regra que deixou de casar ou foi removida).
#[derive(Debug, Clone, PartialEq)]
pub struct Reversao {
    pub esperado: Esperado,
    /// Valor efetivo atual.
    pub atual: Option<u32>,
    pub acao: AcaoReversao,
}

/// Original de (perfil, id) segundo o snapshot.
enum Original {
    Valor(SettingValue),
    Ausente,
    Desconhecido,
}

fn perfil_ausente(e: &nvt_drs::Error) -> bool {
    matches!(
        e,
        nvt_drs::Error::Nvapi {
            status: nvt_drs::Status::ProfileNotFound,
            ..
        }
    )
}

/// Isola o erro de um perfil de jogo: registra a mensagem em `erros` e devolve
/// `Ok(None)`; erro no perfil global é propagado.
fn isolar<T>(
    perfil: &ProfileRef,
    r: Resultado<T>,
    erros: &mut Vec<String>,
    msg: impl FnOnce(&ErroEngine) -> String,
) -> Resultado<Option<T>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(e) if matches!(perfil, ProfileRef::Named(_)) => {
            erros.push(msg(&e));
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

fn mesmo_perfil(a: &ProfileRef, b: &ProfileRef) -> bool {
    match (a, b) {
        (ProfileRef::Base, ProfileRef::Base) => true,
        (ProfileRef::Named(x), ProfileRef::Named(y)) => x.eq_ignore_ascii_case(y),
        _ => false,
    }
}

fn truncar_utf16(s: &str, max: usize) -> &str {
    let mut n = 0;
    for (i, c) in s.char_indices() {
        n += c.len_utf16();
        if n > max {
            return &s[..i];
        }
    }
    s
}

/// Confere o executável antes de qualquer chamada à NVAPI.
fn validar_exe(exe: &str) -> Result<(), String> {
    if exe.trim().is_empty() {
        return Err("executável vazio".into());
    }
    if exe.chars().any(char::is_control) {
        return Err("executável com caractere de controle".into());
    }
    let n = exe.encode_utf16().count();
    if n > MAX_EXE {
        return Err(format!(
            "executável com {n} unidades UTF-16 (máximo {MAX_EXE})"
        ));
    }
    Ok(())
}

/// Nome de perfil seguro a partir do nome do launcher: sem caracteres de
/// controle, aparado e limitado a [`MAX_NOME_PERFIL`]; vazio vira o nome do exe.
fn nome_perfil_seguro(nome: &str, exe: &str) -> String {
    let limpo: String = nome.chars().filter(|c| !c.is_control()).collect();
    let limpo = limpo.trim();
    let base = if limpo.is_empty() {
        std::path::Path::new(exe)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| exe.to_owned())
    } else {
        limpo.to_owned()
    };
    truncar_utf16(&base, MAX_NOME_PERFIL).trim_end().to_owned()
}

/// De onde vem o perfil de um jogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrigemPerfil {
    /// Perfil predefinido pela NVIDIA (vem com o driver).
    Driver,
    /// Perfil de usuário que já existia (criado por outra ferramenta).
    Usuario,
    /// Perfil criado pelo nv-tuner.
    CriadoPeloApp,
    /// Não há perfil: será criado (`CreateProfile` + `CreateApplication`) se houver mudança.
    ACriar,
}

/// Jogo cruzado com o DRS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JogoResolvido {
    pub entrada: JogoEntrada,
    /// Nome do perfil no DRS (existente ou a criar); também é `GameFacts::id`.
    pub perfil: String,
    pub origem: OrigemPerfil,
    pub fatos: GameFacts,
}

/// Uma mudança do plano já posicionada no DRS. `mudanca.valor_anterior` é o
/// valor efetivo lido do driver (`None` = não definido/perfil inexistente).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MudancaPlanejada {
    pub alvo: Alvo,
    pub perfil: ProfileRef,
    pub mudanca: SettingChange,
}

impl MudancaPlanejada {
    /// O que passa a ser vigiado depois de gravada (ou já conforme).
    fn esperado(&self, antes_da_gestao: Option<ValorAntes>) -> Esperado {
        Esperado {
            alvo: self.alvo.clone(),
            perfil: self.perfil.clone(),
            setting: self.mudanca.setting.clone(),
            id: self.mudanca.id,
            valor: self.mudanca.valor,
            origem: self.mudanca.origem.clone(),
            antes_da_gestao,
        }
    }
}

/// Resultado de [`Engine::planejar`]: o que precisa mudar para o DRS ficar conforme o plano.
#[derive(Debug, Clone, PartialEq)]
pub struct Diff {
    pub capacidades: Capabilities,
    pub jogos: Vec<JogoResolvido>,
    /// Plano completo do avaliador (inclui respeitadas, experimentais e descartes).
    pub plano: Plan,
    /// Perfis + aplicativos a criar.
    pub criar: Vec<PerfilCriado>,
    /// Somente as mudanças necessárias.
    pub mudancas: Vec<MudancaPlanejada>,
    /// Mudanças do plano que o driver já tem.
    pub conformes: Vec<MudancaPlanejada>,
    /// Settings geridas que saíram do plano: voltam ao original e deixam de ser vigiadas.
    pub reverter: Vec<Reversao>,
    /// Jogos deixados de fora por erro isolado.
    pub ignorados: Vec<JogoIgnorado>,
}

impl Diff {
    /// Nada a fazer no driver nem no estado esperado.
    pub fn vazio(&self) -> bool {
        self.criar.is_empty() && self.mudancas.is_empty() && self.reverter.is_empty()
    }

    /// Mudanças necessárias de um alvo.
    pub fn mudancas_de<'a>(&'a self, alvo: &'a Alvo) -> impl Iterator<Item = &'a MudancaPlanejada> {
        self.mudancas.iter().filter(move |m| &m.alvo == alvo)
    }

    /// Mudanças conformes de um alvo.
    pub fn conformes_de<'a>(
        &'a self,
        alvo: &'a Alvo,
    ) -> impl Iterator<Item = &'a MudancaPlanejada> {
        self.conformes.iter().filter(move |m| &m.alvo == alvo)
    }
}

/// Resultado de [`Engine::aplicar`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Aplicacao {
    /// `false` quando chamado sem confirmação (nada foi feito).
    pub aplicada: bool,
    /// O snapshot original foi capturado nesta aplicação.
    pub snapshot_capturado: bool,
    pub perfis_criados: Vec<PerfilCriado>,
    /// Gravadas e confirmadas na releitura.
    pub verificadas: Vec<MudancaPlanejada>,
    /// Gravadas, mas a releitura devolveu outro valor (lido).
    pub falhas: Vec<(MudancaPlanejada, Option<u32>)>,
    /// Settings que saíram do plano e deixaram de ser vigiadas (revertidas ao
    /// original quando havia algo a desfazer).
    pub revertidas: Vec<Esperado>,
    /// Erros isolados por perfil de jogo (o resto da aplicação seguiu).
    pub erros: Vec<String>,
}

/// Motivo de um reset em massa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MotivoReset {
    /// A versão do driver mudou desde a última aplicação.
    VersaoDriver { antes: u32, agora: u32 },
    /// Muitas settings geridas voltaram ao padrão ao mesmo tempo.
    MuitasSettings { revertidas: usize, total: usize },
}

/// Classificação da deriva.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClasseDeriva {
    /// Tudo como esperado.
    Nenhuma,
    /// Reset em massa: reaplicar o esperado.
    ResetEmMassa(MotivoReset),
    /// Mudança pontual: travar como escolha do usuário.
    Pontual,
}

/// Setting gerida que não está com o valor esperado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergencia {
    pub esperado: Esperado,
    /// Valor efetivo atual.
    pub atual: Option<u32>,
    /// Não está mais gravada pelo usuário no perfil (ausente, herdada ou predefinida).
    pub voltou_ao_padrao: bool,
}

/// Resultado de [`Engine::verificar_deriva`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deriva {
    pub classe: ClasseDeriva,
    pub divergencias: Vec<Divergencia>,
    pub versao_driver: u32,
}

/// Resultado de [`Engine::tratar_deriva`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TratamentoDeriva {
    pub aplicado: bool,
    pub reaplicadas: Vec<Esperado>,
    pub travadas: Vec<Travada>,
    /// Reaplicadas cuja releitura não confere (valor lido).
    pub falhas: Vec<(Esperado, Option<u32>)>,
    /// Perfil sumiu do driver: deixaram de ser vigiadas (o próximo plano recria
    /// o perfil se alguma regra ainda o exigir).
    pub removidas: Vec<Esperado>,
    /// Erros isolados por perfil de jogo (o perfil global e os demais seguiram).
    pub erros: Vec<String>,
}

/// Plano de restauração do snapshot original.
#[derive(Debug, Clone, PartialEq)]
pub struct Restauracao {
    pub ops: Vec<RestoreOp>,
    /// Operações sobre IDs fora da lista permitida: o app nunca gravou nesses IDs,
    /// então não os toca.
    pub ignoradas: Vec<RestoreOp>,
    pub aplicada: bool,
}

/// Classifica a deriva (puro). Limiares em [`LIMIAR_RESET_MIN`] e [`LIMIAR_RESET_PCT`].
///
/// 1. Versão do driver diferente da vista na última aplicação (com algo gerido) → reset em massa.
/// 2. `revertidas ≥ LIMIAR_RESET_MIN` **e** `revertidas ≥ LIMIAR_RESET_PCT % do total` →
///    reset em massa.
/// 3. Todas as settings geridas revertidas, com pelo menos duas → reset em massa (cobre
///    conjuntos pequenos).
/// 4. Alguma divergência → pontual. 5. Senão, nenhuma.
pub fn classificar_deriva(
    total: usize,
    divergencias: &[Divergencia],
    driver_visto: Option<u32>,
    versao_atual: u32,
) -> ClasseDeriva {
    if total > 0
        && let Some(antes) = driver_visto
        && antes != versao_atual
    {
        return ClasseDeriva::ResetEmMassa(MotivoReset::VersaoDriver {
            antes,
            agora: versao_atual,
        });
    }
    let revertidas = divergencias.iter().filter(|d| d.voltou_ao_padrao).count();
    let massa = (revertidas >= LIMIAR_RESET_MIN && revertidas * 100 >= total * LIMIAR_RESET_PCT)
        || (total >= 2 && revertidas == total);
    if massa {
        ClasseDeriva::ResetEmMassa(MotivoReset::MuitasSettings { revertidas, total })
    } else if divergencias.is_empty() {
        ClasseDeriva::Nenhuma
    } else {
        ClasseDeriva::Pontual
    }
}

/// Motor do nv-tuner sobre um [`DriverStore`].
pub struct Engine<S: DriverStore> {
    /// Só aceita gravação com a [`Licenca`] do [`Portao`] (T-02).
    store: Guardado<S>,
    regras: ConjuntoRegras,
    /// Base curada de jogos competitivos (fatos curados, F1-03/F1-04).
    base: BaseJogos,
    armazem: Armazem,
    /// Portão único de gravação (jogo ou anticheat em execução adia).
    portao: Portao,
}

/// Nome legível do perfil.
pub fn nome_perfil(r: &ProfileRef) -> String {
    match r {
        ProfileRef::Base => NOME_PERFIL_BASE.to_owned(),
        ProfileRef::Named(n) => n.clone(),
    }
}
