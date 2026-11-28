//! Ponto único de escolha entre o serviço real e a demonstração.
//!
//! `main` escolhe o [`Backend`] uma vez ([`Backend::escolher`]); as telas
//! pedem [`Backend::atual`] e chamam as operações abaixo, sem decidir por
//! conta própria. Para adicionar uma ação: um método aqui com os dois braços
//! (serviço e demonstração); a tela só chama o método.

use crate::abrir::{self, FalhaAbrir};
use crate::demonstracao::{self, Cenario};
use crate::local;
use crate::servico::{self, AcaoInicio, Canal, FalhaInicio, RespostaInicio};

/// De onde vêm os dados e para onde vão as ações.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Serviço real pelo named pipe (ou visualização local sem ele).
    Servico,
    /// Dados fictícios fixos (`--demonstracao`); nada fala com o serviço.
    Demonstracao(Cenario),
}

/// Resultado de "Abrir inicialização".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Abertura {
    Aberta,
    /// Demonstração: nada foi aberto.
    Simulada,
    Falhou(FalhaAbrir),
}

impl Backend {
    /// Escolhe o backend do processo (uma vez, em `main`).
    pub fn escolher(demonstracao: Option<Cenario>) -> Backend {
        match demonstracao {
            Some(c) => {
                demonstracao::ativar(c);
                Backend::Demonstracao(c)
            }
            None => Backend::Servico,
        }
    }

    /// Backend escolhido em `main` (serviço se nada foi escolhido).
    pub fn atual() -> Backend {
        demonstracao::ativo().map_or(Backend::Servico, Backend::Demonstracao)
    }

    /// Canal de comandos e eventos da janela.
    pub fn conectar(self, pipe: String) -> Canal {
        match self {
            Backend::Servico => servico::iniciar(pipe),
            Backend::Demonstracao(c) => demonstracao::iniciar(c),
        }
    }

    /// Indicador de demonstração (selo da lateral, textos simulados).
    pub fn eh_demonstracao(self) -> bool {
        matches!(self, Backend::Demonstracao(_))
    }

    /// Cenário `otimizado` da demonstração (texto próprio no Início).
    pub fn demonstracao_otimizada(self) -> bool {
        self == Backend::Demonstracao(Cenario::Otimizado)
    }

    /// Ação do Início (bloqueante; chamar fora da thread da janela).
    pub fn executar_inicio(self, acao: AcaoInicio) -> Result<RespostaInicio, FalhaInicio> {
        match self {
            Backend::Servico => servico::executar_inicio(acao),
            Backend::Demonstracao(c) => demonstracao::executar_inicio(c, acao),
        }
    }

    /// "Abrir inicialização"; na demonstração nada abre.
    pub fn abrir_inicializacao(
        self,
        estado: Option<&nvt_ipc::Estado>,
        jogos: &[nvt_ipc::Jogo],
    ) -> Abertura {
        match self {
            Backend::Servico => match abrir::abrir_inicializacao(estado, jogos) {
                Ok(()) => Abertura::Aberta,
                Err(f) => Abertura::Falhou(f),
            },
            Backend::Demonstracao(_) => Abertura::Simulada,
        }
    }

    /// As boas-vindas já foram vistas? Na demonstração decide o cenário e
    /// nada é lido.
    pub fn boas_vindas_vista(self) -> bool {
        match self {
            Backend::Servico => local::boas_vindas_vista(),
            Backend::Demonstracao(c) => !c.boas_vindas(),
        }
    }

    /// Grava a marca das boas-vindas (falha não bloqueia); na demonstração
    /// nada é gravado.
    pub fn marcar_boas_vindas(self) {
        if self == Backend::Servico {
            let _ = local::marcar_boas_vindas();
        }
    }

    /// Pedido de partidas (bloqueante; chamar fora da thread da janela).
    #[cfg(feature = "medicao-partidas")]
    pub fn partidas(self, p: PedidoPartidas) -> Result<RetornoPartidas, FalhaInicio> {
        match self {
            Backend::Servico => {
                servico::requisitar(&p.requisicao()).and_then(|r| interpretar_partidas(p, r))
            }
            Backend::Demonstracao(c) => demonstracao::pedir_partidas(c, p),
        }
    }

    /// Instante de referência das partidas: relógio real, ou o fixo da
    /// demonstração.
    #[cfg(feature = "medicao-partidas")]
    pub fn agora_partidas(self) -> u64 {
        match self {
            Backend::Servico => crate::fuso::agora_unix(),
            Backend::Demonstracao(_) => demonstracao::AGORA_PARTIDAS,
        }
    }
}

/// O que se pede ao serviço (ou à demonstração) na tela Partidas.
#[cfg(feature = "medicao-partidas")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PedidoPartidas {
    Listar,
    Obter(u64),
}

#[cfg(feature = "medicao-partidas")]
impl PedidoPartidas {
    pub fn requisicao(self) -> nvt_ipc::Requisicao {
        match self {
            PedidoPartidas::Listar => nvt_ipc::Requisicao::ListarPartidas {
                desde_unix: 0,
                limite: nvt_ipc::PARTIDAS_MAX,
            },
            PedidoPartidas::Obter(id) => nvt_ipc::Requisicao::ObterPartida { id },
        }
    }
}

/// Resultado de um pedido de partidas, já interpretado.
#[cfg(feature = "medicao-partidas")]
#[derive(Debug, Clone, PartialEq)]
pub enum RetornoPartidas {
    Lista(Vec<nvt_ipc::ResumoPartida>),
    Detalhe(Box<nvt_ipc::Partida>),
}

/// Resposta do serviço para o pedido; inválida ou de outro tipo = falha.
#[cfg(feature = "medicao-partidas")]
pub fn interpretar_partidas(
    p: PedidoPartidas,
    r: nvt_ipc::Resposta,
) -> Result<RetornoPartidas, FalhaInicio> {
    use nvt_ipc::Resposta;
    match (p, r) {
        (PedidoPartidas::Listar, Resposta::Partidas { mut resumos }) => {
            resumos.retain(|r| r.validar().is_ok());
            resumos.sort_by_key(|r| std::cmp::Reverse(r.inicio_unix));
            Ok(RetornoPartidas::Lista(resumos))
        }
        (PedidoPartidas::Obter(_), Resposta::Partida(d)) if d.validar().is_ok() => {
            Ok(RetornoPartidas::Detalhe(Box::new(d)))
        }
        (_, Resposta::Erro { codigo, mensagem }) => Err(servico::falha_do_erro(codigo, &mensagem)),
        _ => Err(FalhaInicio::Outra(crate::textos::partidas::FALHA.into())),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn escolha_e_indicador() {
        assert_eq!(Backend::escolher(None), Backend::Servico);
        assert!(!Backend::Servico.eh_demonstracao());
        assert!(Backend::Demonstracao(Cenario::Melhorias).eh_demonstracao());
        assert!(Backend::Demonstracao(Cenario::Otimizado).demonstracao_otimizada());
        assert!(!Backend::Demonstracao(Cenario::Melhorias).demonstracao_otimizada());
        assert!(!Backend::Servico.demonstracao_otimizada());
    }

    #[test]
    fn demonstracao_responde_como_antes() {
        for &(c, _) in demonstracao::CENARIOS {
            let b = Backend::Demonstracao(c);
            for acao in [
                AcaoInicio::AplicarRecomendadas,
                AcaoInicio::DesfazerTudo,
                AcaoInicio::VoltarAAjustar,
            ] {
                assert_eq!(
                    b.executar_inicio(acao),
                    demonstracao::executar_inicio(c, acao)
                );
            }
            assert_eq!(b.boas_vindas_vista(), !c.boas_vindas());
            assert_eq!(b.abrir_inicializacao(None, &[]), Abertura::Simulada);
        }
    }

    #[cfg(feature = "medicao-partidas")]
    #[test]
    fn partidas_da_demonstracao() {
        let b = Backend::Demonstracao(Cenario::Partidas);
        let todas = demonstracao::partidas(Cenario::Partidas);
        let resumos: Vec<_> = todas.iter().map(|p| p.resumo.clone()).collect();
        assert_eq!(
            b.partidas(PedidoPartidas::Listar),
            Ok(RetornoPartidas::Lista(resumos))
        );
        assert_eq!(
            b.partidas(PedidoPartidas::Obter(todas[0].resumo.id)),
            Ok(RetornoPartidas::Detalhe(Box::new(todas[0].clone())))
        );
        assert_eq!(
            b.partidas(PedidoPartidas::Obter(u64::MAX)),
            Err(FalhaInicio::Outra(crate::textos::partidas::FALHA.into()))
        );
        assert_eq!(b.agora_partidas(), demonstracao::AGORA_PARTIDAS);
    }
}
