//! Saúde na sessão do usuário (R-S-02, R-S-07,
//! R-L-01, R-L-05 a R-L-07).
//!
//! Leituras somente leitura (inicialização e sinais da sessão) e a passada de limpeza do
//! `%TEMP%` do usuário no modo fixado na compilação ([`nvt_manutencao::MODO_LIMPEZA`]).
//! Tudo passa antes pelo portão de jogo/anticheat: com jogo aberto, ou sem saber o que
//! está aberto, nada é lido nem percorrido. O relato ([`SaudeSessao`]) leva só nomes,
//! enums e contagens; comandos e caminhos ficam no processo (R-S-03, R-S-07, R-L-09).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use nvt_core::jogos::BaseJogos;
use nvt_engine::Portao;
use nvt_ipc::{
    AlvoLimpeza, FonteInicio, ModoLimpeza, PROGRAMAS_INICIO_MAX, ProgramaInicio, ResultadoLimpeza,
    SINAIS_MAX, SaudeSessao, Sinal, SituacaoInicio,
};
use nvt_manutencao::{Excluidor, Instalacao, Politica, Raiz, Relogio, SistemaArquivos};
use nvt_windows::saude::{
    EntradaInicio, LeitorInicio, LeitorSinaisSessao, ler_inicio_sessao, ler_sinais_sessao,
};

use crate::estado::{EstadoSaude, ItemBase, LinhaBase};

#[cfg(test)]
#[path = "saude/testes.rs"]
mod testes;

/// Releitura da inicialização (R-S-07).
pub(crate) const INTERVALO_INICIO: Duration = Duration::from_secs(6 * 3600);
/// Releitura dos sinais da sessão (R-S-07).
pub(crate) const INTERVALO_SINAIS: Duration = Duration::from_secs(600);
/// Nova tentativa quando o portão recusou (jogo aberto ou lista indisponível).
pub(crate) const INTERVALO_ADIADO: Duration = Duration::from_secs(600);

/// Fontes de saúde da sessão. A implementação real lê o Windows; os testes usam falsas.
pub trait FonteSaude {
    fn inicio(&self) -> Vec<EntradaInicio>;
    fn sinais(&self, agora_unix: u64) -> Vec<Sinal>;
    /// Uma passada no `%TEMP%` do usuário. `None` = política diária recusou ou alvo
    /// indisponível.
    fn limpar(
        &mut self,
        modo: ModoLimpeza,
        portao: &dyn nvt_manutencao::Portao,
        politica: &mut dyn Politica,
    ) -> Option<ResultadoLimpeza>;
}

/// Composição das fachadas: leitor do Windows, sistema de arquivos, excluidor e detecção
/// de instalação MSI.
pub struct FachadaSaude<L, F, E, I> {
    pub leitor: L,
    pub fs: F,
    pub excluidor: E,
    pub instalacao: I,
    /// Raiz do `%TEMP%` do usuário; `None` = não normalizável, sem limpeza.
    pub raiz: Option<Raiz>,
    pub relogio: Box<dyn Relogio>,
}

impl<L, F, E, I> FonteSaude for FachadaSaude<L, F, E, I>
where
    L: LeitorInicio + LeitorSinaisSessao,
    F: SistemaArquivos,
    E: Excluidor,
    I: Instalacao,
{
    fn inicio(&self) -> Vec<EntradaInicio> {
        ler_inicio_sessao(&self.leitor).entradas
    }

    fn sinais(&self, agora_unix: u64) -> Vec<Sinal> {
        ler_sinais_sessao(&self.leitor, agora_unix)
    }

    fn limpar(
        &mut self,
        modo: ModoLimpeza,
        portao: &dyn nvt_manutencao::Portao,
        politica: &mut dyn Politica,
    ) -> Option<ResultadoLimpeza> {
        let raiz = self.raiz.as_ref()?;
        if raiz.alvo() != AlvoLimpeza::TempUsuario {
            return None;
        }
        let mut ctx = nvt_manutencao::Contexto {
            relogio: self.relogio.as_ref(),
            portao,
            instalacao: &self.instalacao,
            politica,
        };
        nvt_manutencao::limpar(modo, raiz, &self.fs, &mut self.excluidor, &mut ctx)
    }
}

/// Fachada real da sessão.
pub fn fachada_windows() -> impl FonteSaude {
    use nvt_manutencao::windows::{ExcluidorReal, MutexMsi, SistemaArquivosWindows, raiz_do_alvo};
    FachadaSaude {
        leitor: nvt_windows::saude::SaudeWindows,
        fs: SistemaArquivosWindows,
        excluidor: ExcluidorReal,
        instalacao: MutexMsi::padrao(),
        raiz: raiz_do_alvo(AlvoLimpeza::TempUsuario).ok(),
        relogio: Box::new(nvt_manutencao::RelogioSistema),
    }
}

/// Agenda e últimas leituras. Fica fora do atendimento do pipe: roda no ciclo do agente.
pub(crate) struct AgendaSaude {
    fonte: Box<dyn FonteSaude>,
    modo: ModoLimpeza,
    /// `None` = ainda não lido: vence já.
    proximo_inicio: Option<Instant>,
    proximos_sinais: Option<Instant>,
    programas: Vec<ProgramaInicio>,
    sinais: Vec<Sinal>,
    /// Relato ainda não confirmado pelo serviço.
    pub(crate) pendente: Option<SaudeSessao>,
}

impl AgendaSaude {
    pub(crate) fn nova(fonte: Box<dyn FonteSaude>, modo: ModoLimpeza) -> Self {
        Self {
            fonte,
            modo,
            proximo_inicio: None,
            proximos_sinais: None,
            programas: Vec::new(),
            sinais: Vec::new(),
            pendente: None,
        }
    }

    /// Lê o que estiver vencido. Recusa do portão adia tudo, sem ler nada. Devolve
    /// `true` quando o estado persistido mudou.
    pub(crate) fn passo(
        &mut self,
        agora: Instant,
        agora_unix: u64,
        portao: &dyn nvt_manutencao::Portao,
        estado: &mut EstadoSaude,
    ) -> bool {
        let inicio = self.proximo_inicio.is_none_or(|t| agora >= t);
        let sinais = self.proximos_sinais.is_none_or(|t| agora >= t);
        if !inicio && !sinais {
            return false;
        }
        if portao.jogo_aberto() {
            let depois = agora + INTERVALO_ADIADO;
            self.proximo_inicio = Some(self.proximo_inicio.map_or(depois, |t| t.max(depois)));
            self.proximos_sinais = Some(self.proximos_sinais.map_or(depois, |t| t.max(depois)));
            return false;
        }
        let antes = estado.clone();
        if sinais {
            let mut s = self.fonte.sinais(agora_unix);
            s.retain(|s| s.validar().is_ok());
            s.truncate(SINAIS_MAX);
            self.sinais = s;
            self.proximos_sinais = Some(agora + INTERVALO_SINAIS);
        }
        if inicio {
            self.programas = comparar(estado, &self.fonte.inicio(), agora_unix);
            let mut politica = nvt_manutencao::PoliticaDiaria {
                ultimas: estado.ultimas_limpezas.clone(),
            };
            if let Some(r) = self.fonte.limpar(self.modo, portao, &mut politica)
                && r.alvo == AlvoLimpeza::TempUsuario
            {
                estado.limpeza = Some(r);
            }
            estado.ultimas_limpezas = politica.ultimas;
            self.proximo_inicio = Some(agora + INTERVALO_INICIO);
        }
        self.pendente = Some(SaudeSessao {
            sinais: self.sinais.clone(),
            inicializacao: self.programas.clone(),
            limpeza: estado.limpeza.clone(),
        });
        *estado != antes
    }
}

fn hash(comando: &str) -> String {
    nvt_jogocfg::sha256(comando.as_bytes()).hex()
}

/// Diferença contra a linha de base (R-S-02). Primeira execução grava a base e não marca
/// nada como novo; depois, `Novo` = `(fonte, nome)` ausente da base e `Mudou` = hash do
/// comando diferente. Só "Marcar como visto" incorpora à base.
pub(crate) fn comparar(
    estado: &mut EstadoSaude,
    entradas: &[EntradaInicio],
    agora_unix: u64,
) -> Vec<ProgramaInicio> {
    let validas: Vec<&EntradaInicio> = entradas
        .iter()
        .filter(|e| e.programa(SituacaoInicio::NaBase, 0).validar().is_ok())
        .take(PROGRAMAS_INICIO_MAX)
        .collect();
    let Some(base) = &estado.base else {
        let itens = validas
            .iter()
            .map(|e| ItemBase {
                fonte: e.fonte,
                nome: e.nome.clone(),
                hash: hash(&e.comando),
                detectado_unix: agora_unix,
            })
            .collect();
        estado.base = Some(LinhaBase {
            criada_unix: agora_unix,
            itens,
        });
        estado.fora_da_base.clear();
        return validas
            .iter()
            .map(|e| e.programa(SituacaoInicio::NaBase, agora_unix))
            .collect();
    };
    let indice: BTreeMap<(FonteInicio, &str), &ItemBase> = base
        .itens
        .iter()
        .map(|i| ((i.fonte, i.nome.as_str()), i))
        .collect();
    let anteriores: BTreeMap<(FonteInicio, String, String), u64> = estado
        .fora_da_base
        .iter()
        .map(|i| ((i.fonte, i.nome.clone(), i.hash.clone()), i.detectado_unix))
        .collect();
    let mut fora = Vec::new();
    let mut saida = Vec::with_capacity(validas.len());
    for e in validas {
        let h = hash(&e.comando);
        let situacao = match indice.get(&(e.fonte, e.nome.as_str())) {
            Some(i) if i.hash == h => {
                saida.push(e.programa(SituacaoInicio::NaBase, i.detectado_unix));
                continue;
            }
            Some(_) => SituacaoInicio::Mudou,
            None => SituacaoInicio::Novo,
        };
        let detectado = anteriores
            .get(&(e.fonte, e.nome.clone(), h.clone()))
            .copied()
            .unwrap_or(agora_unix);
        fora.push(ItemBase {
            fonte: e.fonte,
            nome: e.nome.clone(),
            hash: h,
            detectado_unix: detectado,
        });
        saida.push(e.programa(situacao, detectado));
    }
    estado.fora_da_base = fora;
    saida
}

/// Portão do agente para a passada: mesma base de jogos/anticheat e a lista do serviço.
/// Sem a lista do serviço, ou sem enumeração de processos, conta como jogo aberto.
pub(crate) struct PortaoAgente<'a> {
    pub portao: &'a Portao,
    pub base: &'a BaseJogos,
    pub extras: Option<Vec<&'a str>>,
}

impl nvt_manutencao::Portao for PortaoAgente<'_> {
    fn jogo_aberto(&self) -> bool {
        match &self.extras {
            Some(extras) => self.portao.verificar(self.base, extras).is_err(),
            None => true,
        }
    }
}
