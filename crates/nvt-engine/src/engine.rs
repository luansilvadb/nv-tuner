//! O motor: plano → diff contra o DRS → aplicação verificada → histórico,
//! detecção de deriva e restauração do snapshot original.

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

impl<S: DriverStore> Engine<S> {
    /// Motor com as regras e a base de jogos embutidas.
    pub fn new(store: S, armazem: Armazem) -> Resultado<Self> {
        let base = BaseJogos::embutida().map_err(|e| {
            ErroEngine::Regras(ErrosRegras(vec![ErroRegra::Toml {
                arquivo: "jogos.toml".into(),
                mensagem: e.to_string(),
            }]))
        })?;
        Ok(Self::montar(
            store,
            armazem,
            ConjuntoRegras::embutido()?,
            base,
        ))
    }

    /// Motor com as regras dadas e a base de jogos embutida (vazia se a base
    /// embutida for inválida, o que o teste de carga impede).
    pub fn com_regras(store: S, armazem: Armazem, regras: ConjuntoRegras) -> Self {
        let base = BaseJogos::embutida().unwrap_or_else(|_| BaseJogos::vazia());
        Self::montar(store, armazem, regras, base)
    }

    fn montar(store: S, armazem: Armazem, regras: ConjuntoRegras, base: BaseJogos) -> Self {
        Self {
            store: Guardado::new(store),
            regras,
            base,
            armazem,
            portao: Portao::do_sistema(),
        }
    }

    /// Troca o portão de gravação (fonte de processos injetável; o serviço
    /// passa o mesmo portão a cada motor recriado).
    pub fn com_portao(mut self, portao: Portao) -> Self {
        self.portao = portao;
        self
    }

    /// Portão de gravação.
    pub fn portao(&self) -> &Portao {
        &self.portao
    }

    /// Portão único de gravação (T-02): consulta a lista de processos agora
    /// e diz se o DRS pode ser gravado. Toda gravação do motor passa por
    /// ele (ver [`crate::portao`]).
    pub fn pode_gravar(&self) -> Result<(), Adiamento> {
        self.portao.verificar(&self.base, &[]).map(|_| ())
    }

    /// Licença para uma operação de gravação: estado gravável e portão livre
    /// (com os exes `extras` também vigiados).
    fn licenca(&self, extras: &[&str]) -> Resultado<Licenca> {
        self.exigir_gravacao()?;
        self.portao
            .verificar(&self.base, extras)
            .map_err(ErroEngine::Adiada)
    }

    /// Troca a base curada de jogos.
    pub fn com_base(mut self, base: BaseJogos) -> Self {
        self.base = base;
        self
    }

    /// Base curada de jogos competitivos.
    pub fn base_jogos(&self) -> &BaseJogos {
        &self.base
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn estado(&self) -> &Estado {
        &self.armazem.estado
    }

    pub fn armazem(&self) -> &Armazem {
        &self.armazem
    }

    /// Avisos estruturados do estado (os da abertura do [`Armazem`]).
    pub fn avisos(&self) -> &[crate::Aviso] {
        self.armazem.avisos()
    }

    /// Recusa gravar (driver ou estado) com o armazém somente leitura, antes
    /// de qualquer chamada ao driver.
    fn exigir_gravacao(&self) -> Resultado<()> {
        if self.armazem.somente_leitura() {
            return Err(ErroEngine::SomenteLeitura);
        }
        Ok(())
    }

    pub fn regras(&self) -> &ConjuntoRegras {
        &self.regras
    }

    /// Troca as preferências (as escolhas travadas são mantidas à parte) e persiste.
    pub fn definir_preferencias(&mut self, mut prefs: Preferences) -> Resultado<()> {
        self.exigir_gravacao()?;
        prefs.escolhas_usuario.clear();
        self.armazem.estado.preferencias = prefs;
        self.armazem.salvar()
    }

    /// Libera uma escolha travada (perfil comparado sem diferenciar
    /// maiúsculas); o próximo ciclo volta a gerir a setting.
    pub fn liberar_travada(&mut self, alvo: &Alvo, setting: &str) -> Resultado<bool> {
        self.exigir_gravacao()?;
        let antes = self.armazem.estado.travadas.len();
        self.armazem
            .estado
            .travadas
            .retain(|t| !(mesmo_alvo(&t.alvo, alvo) && t.setting == setting));
        let mudou = self.armazem.estado.travadas.len() != antes;
        if mudou {
            self.armazem.salvar()?;
        }
        Ok(mudou)
    }

    /// Capacidades da GPU principal.
    pub fn capacidades(&self) -> Resultado<Capabilities> {
        capacidades(&*self.store)
    }

    // ── Lista permitida ────────────────────────────────────────────────────

    /// Confere o ID (e, se dado, o valor) contra o catálogo.
    pub fn validar(&self, id: u32, valor: Option<u32>) -> Resultado<&DefSetting> {
        let def = self
            .regras
            .catalogo
            .por_id(id)
            .ok_or(ErroEngine::IdNaoPermitido { id })?;
        if let Some(v) = valor
            && !def.aceita(v)
        {
            return Err(ErroEngine::ValorNaoPermitido {
                setting: def.nome.clone(),
                valor: v,
            });
        }
        Ok(def)
    }

    /// Única porta de gravação de valores do motor: valida contra a lista permitida.
    /// Não persiste (quem chama decide quando `salvar`).
    /// Passa pelo portão ([`Engine::pode_gravar`]).
    pub fn gravar_permitido(&mut self, perfil: &ProfileRef, id: u32, valor: u32) -> Resultado<()> {
        let licenca = self.licenca(&[])?;
        self.gravar_com(&licenca, perfil, id, valor)
    }

    fn gravar_com(
        &mut self,
        licenca: &Licenca,
        perfil: &ProfileRef,
        id: u32,
        valor: u32,
    ) -> Resultado<()> {
        self.exigir_gravacao()?;
        self.validar(id, Some(valor))?;
        self.store
            .escrita(licenca)
            .gravar_setting(perfil, id, &SettingValue::Dword(valor))?;
        Ok(())
    }

    fn gravar_valor_original(
        &mut self,
        licenca: &Licenca,
        perfil: &ProfileRef,
        id: u32,
        valor: &SettingValue,
    ) -> Resultado<()> {
        self.validar(id, None)?;
        self.store
            .escrita(licenca)
            .gravar_setting(perfil, id, valor)?;
        Ok(())
    }

    fn remover_permitido(
        &mut self,
        licenca: &Licenca,
        perfil: &ProfileRef,
        id: u32,
        predef: bool,
    ) -> Resultado<()> {
        self.validar(id, None)?;
        self.store
            .escrita(licenca)
            .remover_setting(perfil, id, predef)?;
        Ok(())
    }

    fn nome_setting(&self, id: u32) -> String {
        self.regras
            .catalogo
            .por_id(id)
            .map_or_else(|| format!("0x{id:08X}"), |d| d.nome.clone())
    }

    fn ler_u32(&self, perfil: &ProfileRef, id: u32) -> Resultado<Option<u32>> {
        Ok(self
            .store
            .ler_setting(perfil, id)?
            .and_then(|s| s.value.as_u32()))
    }

    // ── Pipeline ───────────────────────────────────────────────────────────

    /// Acha o perfil de cada jogo pelo exe principal. Jogos que caem no mesmo perfil
    /// ficam só com o primeiro. Jogos com erro (ver [`Engine::resolver_jogos_isolado`])
    /// ficam de fora.
    pub fn resolver_jogos(&self, jogos: &[JogoEntrada]) -> Resultado<Vec<JogoResolvido>> {
        Ok(self.resolver_jogos_isolado(jogos).0)
    }

    /// Como [`Engine::resolver_jogos`], devolvendo também os jogos ignorados. Nome e
    /// executável vindos das bibliotecas são validados/limitados antes de qualquer
    /// chamada à NVAPI; um erro de um jogo nunca derruba os demais.
    pub fn resolver_jogos_isolado(
        &self,
        jogos: &[JogoEntrada],
    ) -> (Vec<JogoResolvido>, Vec<JogoIgnorado>) {
        let mut saida: Vec<JogoResolvido> = Vec::new();
        let mut ignorados = Vec::new();
        let mut nomes_novos: BTreeSet<String> = BTreeSet::new();
        for j in jogos {
            let (perfil, origem) = match self.resolver_um(j, &nomes_novos) {
                Ok(r) => r,
                Err(motivo) => {
                    ignorados.push(JogoIgnorado {
                        nome: j.nome.clone(),
                        exe: j.exe.clone(),
                        motivo,
                    });
                    continue;
                }
            };
            if origem == OrigemPerfil::ACriar {
                nomes_novos.insert(perfil.to_lowercase());
            }
            if saida.iter().any(|s| s.perfil.eq_ignore_ascii_case(&perfil)) {
                continue;
            }
            let fatos = fatos_do_jogo(perfil.clone(), j, casar_na_base(&self.base, j));
            saida.push(JogoResolvido {
                entrada: j.clone(),
                perfil,
                origem,
                fatos,
            });
        }
        (saida, ignorados)
    }

    fn resolver_um(
        &self,
        j: &JogoEntrada,
        nomes_novos: &BTreeSet<String>,
    ) -> Result<(String, OrigemPerfil), String> {
        validar_exe(&j.exe)?;
        let encontrado = self
            .store
            .perfil_do_exe(&j.exe)
            .map_err(|e| format!("busca do perfil do executável falhou: {e}"))?;
        if let Some(p) = encontrado {
            let origem = if self.armazem.estado.criado_pelo_app(&p.nome) {
                OrigemPerfil::CriadoPeloApp
            } else if p.predefinido {
                OrigemPerfil::Driver
            } else {
                OrigemPerfil::Usuario
            };
            return Ok((p.nome, origem));
        }
        let base = nome_perfil_seguro(&j.nome, &j.exe);
        let mut nome = base.clone();
        for n in 1..=MAX_SUFIXOS {
            let existe = self
                .store
                .perfil_existe(&nome)
                .map_err(|e| format!("consulta do perfil “{nome}” falhou: {e}"))?;
            let ocupado = (existe && !self.armazem.estado.criado_pelo_app(&nome))
                || nomes_novos.contains(&nome.to_lowercase());
            if !ocupado {
                return Ok((nome, OrigemPerfil::ACriar));
            }
            nome = if n == 1 {
                format!("{base} (nv-tuner)")
            } else {
                format!("{base} (nv-tuner {n})")
            };
        }
        Err(format!("nenhum nome livre para o perfil “{base}”"))
    }

    /// Detecta (capacidades + perfis), avalia e compara com o DRS. Somente leitura.
    ///
    /// Erros de um jogo (resolução ou leitura do perfil dele) só tiram esse jogo do
    /// plano ([`Diff::ignorados`]); o perfil global sempre segue.
    pub fn planejar(&self, jogos: &[JogoEntrada]) -> Resultado<Diff> {
        let caps = self.capacidades()?;
        let (resolvidos, mut ignorados) = self.resolver_jogos_isolado(jogos);
        let fatos: Vec<GameFacts> = resolvidos.iter().map(|r| r.fatos.clone()).collect();
        let prefs = self.armazem.estado.preferencias_efetivas();
        let plano = nvt_core::avaliar(&caps, &fatos, &prefs, &self.regras);

        let mut mudancas = Vec::new();
        let mut conformes = Vec::new();
        let mut criar = Vec::new();

        for m in &plano.global {
            self.classificar(
                Alvo::Global,
                ProfileRef::Base,
                m,
                true,
                &mut mudancas,
                &mut conformes,
            )?;
        }
        let mut validos = Vec::with_capacity(resolvidos.len());
        for r in resolvidos {
            let alvo = Alvo::Jogo(r.perfil.clone());
            let lista = plano.mudancas(&alvo);
            if lista.is_empty() {
                validos.push(r);
                continue;
            }
            let existe = r.origem != OrigemPerfil::ACriar;
            let perfil = ProfileRef::Named(r.perfil.clone());
            let mut m_jogo = Vec::new();
            let mut c_jogo = Vec::new();
            let lido = lista.iter().try_for_each(|m| {
                self.classificar(
                    alvo.clone(),
                    perfil.clone(),
                    m,
                    existe,
                    &mut m_jogo,
                    &mut c_jogo,
                )
            });
            if let Err(e) = lido {
                ignorados.push(JogoIgnorado {
                    nome: r.entrada.nome.clone(),
                    exe: r.entrada.exe.clone(),
                    motivo: format!("leitura do perfil “{}” falhou: {e}", r.perfil),
                });
                continue;
            }
            if !existe {
                criar.push(PerfilCriado {
                    nome: r.perfil.clone(),
                    exe: r.entrada.exe.clone(),
                });
            }
            mudancas.extend(m_jogo);
            conformes.extend(c_jogo);
            validos.push(r);
        }
        let reverter = self.reversoes(&validos, &mudancas, &conformes);
        Ok(Diff {
            capacidades: caps,
            jogos: validos,
            plano,
            criar,
            mudancas,
            conformes,
            reverter,
            ignorados,
        })
    }

    /// Settings vigiadas que o plano não quer mais. Só considera o perfil global e
    /// os perfis dos jogos presentes neste plano: um jogo que sumiu da varredura
    /// (biblioteca desconectada, erro isolado) mantém o que tinha.
    fn reversoes(
        &self,
        jogos: &[JogoResolvido],
        mudancas: &[MudancaPlanejada],
        conformes: &[MudancaPlanejada],
    ) -> Vec<Reversao> {
        let mut saida = Vec::new();
        for e in &self.armazem.estado.esperado {
            let considerado = match &e.perfil {
                ProfileRef::Base => true,
                ProfileRef::Named(n) => jogos.iter().any(|j| j.perfil.eq_ignore_ascii_case(n)),
            };
            let no_plano = mudancas
                .iter()
                .chain(conformes)
                .any(|m| m.mudanca.id == e.id && mesmo_perfil(&m.perfil, &e.perfil));
            if !considerado || no_plano {
                continue;
            }
            // Erro de leitura: continua vigiada e tenta de novo no próximo ciclo.
            if let Ok(r) = self.reversao_de(e) {
                saida.push(r);
            }
        }
        saida
    }

    fn reversao_de(&self, e: &Esperado) -> Resultado<Reversao> {
        let lido = match self.store.ler_setting(&e.perfil, e.id) {
            Ok(s) => s,
            Err(err) => {
                let err = ErroEngine::from(err);
                if matches!(&err, ErroEngine::Drs(e) if perfil_ausente(e)) {
                    return Ok(Reversao {
                        esperado: e.clone(),
                        atual: None,
                        acao: AcaoReversao::SoDeixarDeVigiar,
                    });
                }
                return Err(err);
            }
        };
        let atual = lido.as_ref().and_then(|s| s.value.as_u32());
        let propria = lido.as_ref().filter(|s| s.is_user_specified());
        // O valor de antes da gestão (registrado na primeira gravação do app)
        // vale mais que o snapshot inicial: o usuário pode ter mudado a
        // setting depois do snapshot e antes de o app passar a geri-la.
        let original = match e.antes_da_gestao {
            Some(ValorAntes::Valor(v)) => Original::Valor(SettingValue::Dword(v)),
            Some(ValorAntes::Inexistente) => Original::Ausente,
            None => self.original_de(&e.perfil, e.id),
        };
        let acao = match original {
            Original::Desconhecido => AcaoReversao::SoDeixarDeVigiar,
            Original::Valor(v) if propria.is_some_and(|s| s.value == v) => {
                AcaoReversao::SoDeixarDeVigiar
            }
            Original::Valor(v) => AcaoReversao::Gravar(v),
            Original::Ausente => match propria {
                None => AcaoReversao::SoDeixarDeVigiar,
                Some(s) => AcaoReversao::Remover {
                    predefinido_valido: s.predefined.is_some(),
                },
            },
        };
        Ok(Reversao {
            esperado: e.clone(),
            atual,
            acao,
        })
    }

    /// Valor de (perfil, id) antes do app: perfil criado pelo app não tinha nada;
    /// senão vale o snapshot original (ou o original do perfil adicional).
    fn original_de(&self, perfil: &ProfileRef, id: u32) -> Original {
        if let ProfileRef::Named(n) = perfil
            && self.armazem.estado.criado_pelo_app(n)
        {
            return Original::Ausente;
        }
        let snap = self
            .armazem
            .snapshot_original()
            .into_iter()
            .flat_map(|s| s.profiles.iter());
        let p = snap
            .chain(self.armazem.estado.perfis_adicionais.iter())
            .find(|p| mesmo_perfil(&p.profile, perfil));
        match p {
            None => Original::Desconhecido,
            Some(p) => match p.settings.iter().find(|s| s.id == id) {
                Some(s) => Original::Valor(s.value.clone()),
                None => Original::Ausente,
            },
        }
    }

    fn registrar(&mut self, r: RegistroHistorico) {
        let limite = self.armazem.limite_historico();
        self.armazem.estado.registrar_historico(r, limite);
    }

    fn classificar(
        &self,
        alvo: Alvo,
        perfil: ProfileRef,
        m: &SettingChange,
        perfil_existe: bool,
        mudancas: &mut Vec<MudancaPlanejada>,
        conformes: &mut Vec<MudancaPlanejada>,
    ) -> Resultado<()> {
        let atual = if perfil_existe {
            self.ler_u32(&perfil, m.id)?
        } else {
            None
        };
        let mut mudanca = m.clone();
        mudanca.valor_anterior = atual;
        let item = MudancaPlanejada {
            alvo,
            perfil,
            mudanca,
        };
        if atual == Some(m.valor) {
            conformes.push(item);
        } else {
            mudancas.push(item);
        }
        Ok(())
    }

    /// Aplica o diff. **Só age com `confirmar = true`.** Valida tudo contra a lista
    /// permitida antes de tocar no driver; captura o snapshot original antes da primeira
    /// mudança; cria perfis; grava; salva; relê para verificar; registra o histórico.
    pub fn aplicar(&mut self, diff: &Diff, confirmar: bool) -> Resultado<Aplicacao> {
        let mut rel = Aplicacao::default();
        if !confirmar || diff.vazio() {
            return Ok(rel);
        }
        self.exigir_gravacao()?;
        for m in &diff.mudancas {
            self.validar(m.mudanca.id, Some(m.mudanca.valor))?;
        }
        // Portão (T-02): antes de qualquer efeito (snapshot, perfis, settings).
        let exes: Vec<&str> = diff.jogos.iter().map(|j| j.entrada.exe.as_str()).collect();
        let licenca = self.licenca(&exes)?;

        rel.snapshot_capturado = self.garantir_snapshot(diff, &exes)?;

        let agora = agora_iso();
        // Perfis de jogo com erro nesta aplicação: o resto deles é pulado.
        let mut falhos: BTreeSet<String> = BTreeSet::new();
        let falho = |f: &BTreeSet<String>, p: &ProfileRef| match p {
            ProfileRef::Base => false,
            ProfileRef::Named(n) => f.contains(&n.to_lowercase()),
        };
        for c in &diff.criar {
            if let Err(e) = self.criar_perfil_e_app(&licenca, c) {
                rel.erros
                    .push(format!("criação do perfil “{}” falhou: {e}", c.nome));
                falhos.insert(c.nome.to_lowercase());
                continue;
            }
            if !self.armazem.estado.criado_pelo_app(&c.nome) {
                self.armazem.estado.perfis_criados.push(c.clone());
            }
            self.registrar(RegistroHistorico {
                data: agora.clone(),
                perfil: c.nome.clone(),
                setting: String::new(),
                id: 0,
                antes: None,
                depois: None,
                origem: OrigemRegistro::CriacaoPerfil,
                motivo: format!(
                    "jogo sem perfil no driver: criado com o executável {}",
                    c.exe
                ),
            });
            rel.perfis_criados.push(c.clone());
        }

        // Valor de antes da gestão, lido antes da primeira gravação do app em
        // cada (perfil, setting) ainda não gerida.
        let antes_da_gestao: Vec<Option<ValorAntes>> = diff
            .mudancas
            .iter()
            .map(|m| self.valor_antes_da_gestao(&m.perfil, m.mudanca.id))
            .collect();
        for m in &diff.mudancas {
            if falho(&falhos, &m.perfil) {
                continue;
            }
            let gravado = self.gravar_com(&licenca, &m.perfil, m.mudanca.id, m.mudanca.valor);
            let n = nome_perfil(&m.perfil);
            let msg =
                |e: &ErroEngine| format!("gravação de {} em “{n}” falhou: {e}", m.mudanca.setting);
            if isolar(&m.perfil, gravado, &mut rel.erros, msg)?.is_none() {
                falhos.insert(n.to_lowercase());
            }
        }
        let mut revertidas: Vec<&Reversao> = Vec::new();
        for r in &diff.reverter {
            let e = &r.esperado;
            if falho(&falhos, &e.perfil) {
                continue;
            }
            let feito = match &r.acao {
                AcaoReversao::Gravar(v) => self.gravar_valor_original(&licenca, &e.perfil, e.id, v),
                AcaoReversao::Remover { predefinido_valido } => {
                    self.remover_permitido(&licenca, &e.perfil, e.id, *predefinido_valido)
                }
                AcaoReversao::SoDeixarDeVigiar => Ok(()),
            };
            let msg = |err: &ErroEngine| {
                format!(
                    "reversão de {} em “{}” falhou: {err}",
                    e.setting,
                    nome_perfil(&e.perfil)
                )
            };
            if isolar(&e.perfil, feito, &mut rel.erros, msg)?.is_some() {
                revertidas.push(r);
            }
        }
        self.store.escrita(&licenca).salvar()?;
        rel.aplicada = true;

        for r in revertidas {
            let e = &r.esperado;
            self.armazem.estado.remover_esperado(&e.perfil, e.id);
            rel.revertidas.push(e.clone());
            if r.acao == AcaoReversao::SoDeixarDeVigiar {
                continue;
            }
            let depois = self.ler_u32(&e.perfil, e.id).unwrap_or(None);
            self.registrar(RegistroHistorico {
                data: agora.clone(),
                perfil: nome_perfil(&e.perfil),
                setting: e.setting.clone(),
                id: e.id,
                antes: r.atual,
                depois,
                origem: OrigemRegistro::Plano {
                    origem: e.origem.clone(),
                },
                motivo: "saiu do plano (preferência, teto de qualidade ou regra): \
                         revertida ao valor original"
                    .into(),
            });
        }

        for (m, antes) in diff.mudancas.iter().zip(&antes_da_gestao) {
            if falho(&falhos, &m.perfil) {
                continue;
            }
            let msg = |e: &ErroEngine| {
                format!(
                    "releitura de {} em “{}” falhou: {e}",
                    m.mudanca.setting,
                    nome_perfil(&m.perfil)
                )
            };
            let lido = self.ler_u32(&m.perfil, m.mudanca.id);
            let Some(lido) = isolar(&m.perfil, lido, &mut rel.erros, msg)? else {
                continue;
            };
            if lido == Some(m.mudanca.valor) {
                self.armazem.estado.registrar_esperado(m.esperado(*antes));
                self.registrar(RegistroHistorico {
                    data: agora.clone(),
                    perfil: nome_perfil(&m.perfil),
                    setting: m.mudanca.setting.clone(),
                    id: m.mudanca.id,
                    antes: m.mudanca.valor_anterior,
                    depois: Some(m.mudanca.valor),
                    origem: OrigemRegistro::Plano {
                        origem: m.mudanca.origem.clone(),
                    },
                    motivo: m.mudanca.explicacao.clone(),
                });
                rel.verificadas.push(m.clone());
            } else {
                rel.falhas.push((m.clone(), lido));
            }
        }
        // Conformes também passam a ser vigiadas pela detecção de deriva.
        for m in &diff.conformes {
            if falho(&falhos, &m.perfil) {
                continue;
            }
            let antes_da_gestao = self.valor_antes_da_gestao(&m.perfil, m.mudanca.id);
            self.armazem
                .estado
                .registrar_esperado(m.esperado(antes_da_gestao));
        }
        self.armazem.estado.driver_visto = Some(self.store.versao_driver()?.raw);
        self.armazem.salvar()?;
        Ok(rel)
    }

    /// Valor de (perfil, id) imediatamente antes de o app passar a geri-la.
    /// Já gerida: o registrado ([`Estado::registrar_esperado`] o preserva).
    /// Perfil criado pelo app ou inexistente: [`ValorAntes::Inexistente`].
    /// Valor próprio do perfil: [`ValorAntes::Valor`]; herdado, predefinido
    /// ou padrão do driver: [`ValorAntes::Inexistente`]. Erro de leitura ou
    /// valor que não é DWORD: `None` (a reversão usa o snapshot).
    fn valor_antes_da_gestao(&self, perfil: &ProfileRef, id: u32) -> Option<ValorAntes> {
        if let Some(e) = self
            .armazem
            .estado
            .esperado
            .iter()
            .find(|e| mesmo_perfil(&e.perfil, perfil) && e.id == id)
        {
            return e.antes_da_gestao;
        }
        if let ProfileRef::Named(n) = perfil
            && self.armazem.estado.criado_pelo_app(n)
        {
            return Some(ValorAntes::Inexistente);
        }
        match self.store.ler_setting(perfil, id) {
            Ok(Some(s)) if s.is_user_specified() => s.value.as_u32().map(ValorAntes::Valor),
            Ok(_) => Some(ValorAntes::Inexistente),
            Err(e) if perfil_ausente(&e) => Some(ValorAntes::Inexistente),
            Err(_) => None,
        }
    }

    /// Cria o perfil (se não existir) e associa o executável (se ainda não estiver nele).
    fn criar_perfil_e_app(&mut self, licenca: &Licenca, c: &PerfilCriado) -> Resultado<()> {
        if !self.store.perfil_existe(&c.nome)? {
            self.store.escrita(licenca).criar_perfil(&c.nome)?;
        }
        let ja_no_perfil = self
            .store
            .perfil_do_exe(&c.exe)?
            .is_some_and(|p| p.nome.eq_ignore_ascii_case(&c.nome));
        if !ja_no_perfil {
            self.store
                .escrita(licenca)
                .criar_aplicacao(&c.nome, &c.exe)?;
        }
        Ok(())
    }

    /// Snapshot original na primeira aplicação; depois, só o original de perfis novos
    /// que ainda não estavam cobertos. Devolve `true` se capturou o original agora.
    fn garantir_snapshot(&mut self, diff: &Diff, exes: &[&str]) -> Resultado<bool> {
        if self.armazem.snapshot_original().is_none() {
            let snap = self.store.capturar(exes)?;
            return self.armazem.guardar_snapshot_original(snap);
        }
        let mut mudou = false;
        for j in &diff.jogos {
            let afetado = diff
                .mudancas
                .iter()
                .any(|m| m.perfil == ProfileRef::Named(j.perfil.clone()));
            if !afetado
                || j.origem == OrigemPerfil::ACriar
                || j.origem == OrigemPerfil::CriadoPeloApp
            {
                continue;
            }
            let r = ProfileRef::Named(j.perfil.clone());
            let coberto = self
                .armazem
                .snapshot_original()
                .is_some_and(|s| s.profiles.iter().any(|p| p.profile == r))
                || self
                    .armazem
                    .estado
                    .perfis_adicionais
                    .iter()
                    .any(|p| p.profile == r);
            if coberto {
                continue;
            }
            let snap = self.store.capturar(&[j.entrada.exe.as_str()])?;
            if let Some(p) = snap.profiles.into_iter().find(|p| p.profile == r) {
                self.armazem.estado.perfis_adicionais.push(p);
                mudou = true;
            }
        }
        if mudou {
            self.armazem.salvar()?;
        }
        Ok(false)
    }

    // ── Deriva ─────────────────────────────────────────────────────────────

    /// Compara o estado esperado com o DRS atual e classifica. Somente leitura.
    pub fn verificar_deriva(&self) -> Resultado<Deriva> {
        let versao = self.store.versao_driver()?.raw;
        let mut divergencias = Vec::new();
        for e in &self.armazem.estado.esperado {
            let lido: Option<Setting> = match self.store.ler_setting(&e.perfil, e.id) {
                Ok(s) => s,
                // Perfil sumiu: tudo dele voltou ao padrão.
                Err(err) if perfil_ausente(&err) => None,
                // Erro de um perfil de jogo: isolado (fica para o próximo ciclo).
                Err(_) if matches!(e.perfil, ProfileRef::Named(_)) => continue,
                Err(err) => return Err(err.into()),
            };
            let atual = lido.as_ref().and_then(|s| s.value.as_u32());
            if atual == Some(e.valor) {
                continue;
            }
            let voltou_ao_padrao = lido.as_ref().is_none_or(|s| !s.is_user_specified());
            divergencias.push(Divergencia {
                esperado: e.clone(),
                atual,
                voltou_ao_padrao,
            });
        }
        let classe = classificar_deriva(
            self.armazem.estado.esperado.len(),
            &divergencias,
            self.armazem.estado.driver_visto,
            versao,
        );
        Ok(Deriva {
            classe,
            divergencias,
            versao_driver: versao,
        })
    }

    /// Trata a deriva. **Só age com `confirmar = true`.**
    ///
    /// - Reset em massa: regrava as settings que voltaram ao padrão; as que mudaram para
    ///   outro valor gravado são tratadas como pontuais.
    /// - Pontual: cada divergência vira escolha travada (o avaliador deixa de tocá-la) e
    ///   sai do estado esperado.
    ///
    /// Perfil de jogo que sumiu do driver (instalação limpa, apagado no NPI) não
    /// derruba o tratamento: suas settings saem do estado esperado
    /// ([`TratamentoDeriva::removidas`]) e o próximo plano recria o perfil se alguma
    /// regra ainda o exigir. Outros erros de um perfil de jogo ficam em
    /// [`TratamentoDeriva::erros`]; o perfil global sempre segue.
    pub fn tratar_deriva(
        &mut self,
        deriva: &Deriva,
        confirmar: bool,
    ) -> Resultado<TratamentoDeriva> {
        let mut t = TratamentoDeriva::default();
        if !confirmar {
            return Ok(t);
        }
        self.exigir_gravacao()?;
        let agora = agora_iso();
        let massa = matches!(deriva.classe, ClasseDeriva::ResetEmMassa(_));
        let (reaplicar, travar): (Vec<&Divergencia>, Vec<&Divergencia>) = deriva
            .divergencias
            .iter()
            .partition(|d| massa && d.voltou_ao_padrao);

        // Portão (T-02) só se houver o que regravar: travar uma mudança
        // manual mexe apenas no estado do app, não no driver.
        let licenca = if reaplicar.is_empty() {
            None
        } else {
            Some(self.licenca(&[])?)
        };
        let mut gravadas: Vec<&Divergencia> = Vec::new();
        for d in reaplicar {
            let Some(licenca) = &licenca else { break };
            let e = &d.esperado;
            match self.gravar_com(licenca, &e.perfil, e.id, e.valor) {
                Ok(()) => gravadas.push(d),
                Err(err) if matches!(e.perfil, ProfileRef::Named(_)) => {
                    if matches!(&err, ErroEngine::Drs(e) if perfil_ausente(e)) {
                        self.armazem.estado.remover_esperado(&e.perfil, e.id);
                        t.removidas.push(e.clone());
                    } else {
                        t.erros.push(format!(
                            "reaplicação de {} em “{}” falhou: {err}",
                            e.setting,
                            nome_perfil(&e.perfil)
                        ));
                    }
                }
                Err(err) => return Err(err),
            }
        }
        let reaplicar = gravadas;
        if let Some(licenca) = &licenca
            && !reaplicar.is_empty()
        {
            self.store.escrita(licenca).salvar()?;
        }
        let motivo_massa = match &deriva.classe {
            ClasseDeriva::ResetEmMassa(MotivoReset::VersaoDriver { antes, agora }) => {
                format!("driver mudou de {antes} para {agora}: settings voltaram ao padrão")
            }
            ClasseDeriva::ResetEmMassa(MotivoReset::MuitasSettings { revertidas, total }) => {
                format!("{revertidas} de {total} settings geridas voltaram ao padrão juntas")
            }
            _ => String::new(),
        };
        for d in reaplicar {
            let msg = |err: &ErroEngine| {
                format!(
                    "releitura de {} em “{}” falhou: {err}",
                    d.esperado.setting,
                    nome_perfil(&d.esperado.perfil)
                )
            };
            let lido = self.ler_u32(&d.esperado.perfil, d.esperado.id);
            let Some(lido) = isolar(&d.esperado.perfil, lido, &mut t.erros, msg)? else {
                continue;
            };
            if lido == Some(d.esperado.valor) {
                self.registrar(RegistroHistorico {
                    data: agora.clone(),
                    perfil: nome_perfil(&d.esperado.perfil),
                    setting: d.esperado.setting.clone(),
                    id: d.esperado.id,
                    antes: d.atual,
                    depois: Some(d.esperado.valor),
                    origem: OrigemRegistro::Reaplicacao {
                        origem: d.esperado.origem.clone(),
                    },
                    motivo: format!("reaplicada: {motivo_massa}"),
                });
                t.reaplicadas.push(d.esperado.clone());
            } else {
                t.falhas.push((d.esperado.clone(), lido));
            }
        }
        for d in travar {
            let trava = Travada {
                alvo: d.esperado.alvo.clone(),
                setting: d.esperado.setting.clone(),
                valor_usuario: d.atual,
                data: agora.clone(),
                motivo: "mudança manual detectada numa setting gerida pelo app".into(),
            };
            let e = &d.esperado;
            self.armazem.estado.remover_esperado(&e.perfil, e.id);
            if !self
                .armazem
                .estado
                .travadas
                .iter()
                .any(|x| mesmo_alvo(&x.alvo, &trava.alvo) && x.setting == trava.setting)
            {
                self.armazem.estado.travadas.push(trava.clone());
            }
            self.registrar(RegistroHistorico {
                data: agora.clone(),
                perfil: nome_perfil(&e.perfil),
                setting: e.setting.clone(),
                id: e.id,
                antes: Some(e.valor),
                depois: d.atual,
                origem: OrigemRegistro::Usuario,
                motivo: "mudança manual: travada como escolha do usuário".into(),
            });
            t.travadas.push(trava);
        }
        self.armazem.estado.driver_visto = Some(deriva.versao_driver);
        self.armazem.salvar()?;
        t.aplicado = true;
        Ok(t)
    }

    // ── Restauração ────────────────────────────────────────────────────────

    /// Plano para voltar ao snapshot original (`None` se ainda não há snapshot).
    /// Inclui os originais de perfis adicionais e limpa as settings do catálogo nos
    /// perfis criados pelo app. Somente leitura.
    ///
    /// Snapshot original ilegível ([`Armazem::snapshot_indisponivel`]): erro com o
    /// motivo; a restauração fica desabilitada até alguém conferir o arquivo.
    pub fn plano_restauracao(&self) -> Resultado<Option<Restauracao>> {
        if let Some(motivo) = self.armazem.snapshot_indisponivel() {
            return Err(ErroEngine::Io(format!("restauração recusada: {motivo}")));
        }
        let Some(original) = self.armazem.snapshot_original() else {
            return Ok(None);
        };
        let mut alvo = original.clone();
        for p in &self.armazem.estado.perfis_adicionais {
            if !alvo.profiles.iter().any(|x| x.profile == p.profile) {
                alvo.profiles.push(p.clone());
            }
        }
        let mut exes: Vec<String> = Vec::new();
        for p in &alvo.profiles {
            for a in &p.applications {
                if !exes.iter().any(|e| e.eq_ignore_ascii_case(a)) {
                    exes.push(a.clone());
                }
            }
        }
        let refs: Vec<&str> = exes.iter().map(String::as_str).collect();
        let atual = self.store.capturar(&refs)?;
        let plano = nvt_drs::restore_plan(&alvo, &atual);

        let mut ops = Vec::new();
        let mut ignoradas = Vec::new();
        for op in plano.ops {
            let id = match &op {
                RestoreOp::Set { id, .. } | RestoreOp::Reset { id, .. } => Some(*id),
                _ => None,
            };
            match id {
                Some(id) if self.validar(id, None).is_err() => ignoradas.push(op),
                _ => ops.push(op),
            }
        }
        for c in &self.armazem.estado.perfis_criados {
            let r = ProfileRef::Named(c.nome.clone());
            let settings = match self.store.settings_do_perfil(&r) {
                Ok(s) => s,
                Err(_) => continue,
            };
            for s in settings {
                if s.is_user_specified() && self.validar(s.id, None).is_ok() {
                    ops.push(RestoreOp::Reset {
                        profile: r.clone(),
                        id: s.id,
                        predefined_valid: false,
                    });
                }
            }
        }
        Ok(Some(Restauracao {
            ops,
            ignoradas,
            aplicada: false,
        }))
    }

    /// Restaura o snapshot original ("voltar tudo ao original"). **Só age com
    /// `confirmar = true`.** Diferente da reversão de uma setting que sai do
    /// plano (que volta ao valor de antes da gestão daquela setting), aqui vale
    /// sempre o snapshot inicial: o driver volta a como estava antes da primeira
    /// aplicação (ou à base pós-quarentena, ver [`Armazem::base_pos_quarentena`]).
    /// O estado esperado
    /// é zerado (nada mais é gerido até o próximo ciclo); as escolhas travadas ficam.
    pub fn restaurar(&mut self, confirmar: bool) -> Resultado<Option<Restauracao>> {
        let Some(mut r) = self.plano_restauracao()? else {
            return Ok(None);
        };
        if !confirmar || r.ops.is_empty() {
            return Ok(Some(r));
        }
        let licenca = self.licenca(&[])?;
        let agora = agora_iso();
        let mut registros = Vec::new();
        for op in &r.ops {
            match op {
                RestoreOp::CreateProfile { name } => {
                    self.store.escrita(&licenca).criar_perfil(name)?;
                }
                RestoreOp::CreateApplication { profile, exe } => {
                    self.store
                        .escrita(&licenca)
                        .criar_aplicacao(&nome_perfil(profile), exe)?;
                }
                RestoreOp::Set { profile, id, value } => {
                    let antes = self.ler_u32(profile, *id)?;
                    self.gravar_valor_original(&licenca, profile, *id, value)?;
                    registros.push((profile.clone(), *id, antes, value.as_u32()));
                }
                RestoreOp::Reset {
                    profile,
                    id,
                    predefined_valid,
                } => {
                    let antes = self.ler_u32(profile, *id)?;
                    self.remover_permitido(&licenca, profile, *id, *predefined_valid)?;
                    let depois = self.ler_u32(profile, *id)?;
                    registros.push((profile.clone(), *id, antes, depois));
                }
            }
        }
        self.store.escrita(&licenca).salvar()?;
        for (perfil, id, antes, depois) in registros {
            let setting = self.nome_setting(id);
            self.registrar(RegistroHistorico {
                data: agora.clone(),
                perfil: nome_perfil(&perfil),
                setting,
                id,
                antes,
                depois,
                origem: OrigemRegistro::Restauracao,
                motivo: "restauração do snapshot original".into(),
            });
        }
        self.armazem.estado.esperado.clear();
        self.armazem.salvar()?;
        r.aplicada = true;
        Ok(Some(r))
    }
}

/// Nome legível do perfil.
pub fn nome_perfil(r: &ProfileRef) -> String {
    match r {
        ProfileRef::Base => NOME_PERFIL_BASE.to_owned(),
        ProfileRef::Named(n) => n.clone(),
    }
}
