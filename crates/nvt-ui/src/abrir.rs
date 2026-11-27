//! Abre a página de inicialização das Configurações do Windows (decisão D3,
//! R-S-04).
//!
//! Exceção auditada em `crates/nvt-core/tests/proibicoes.rs`: este é o único
//! arquivo do workspace com `ShellExecuteW`, numa única chamada, com o URI
//! fixo [`URI_INICIALIZACAO`] (nenhum argumento vem de dados). Só o tratador
//! de clique chama [`abrir_inicializacao`], que recusa quando o último estado
//! do serviço indica jogo ou anticheat aberto, ou quando o estado é
//! desconhecido.

use nvt_ipc::{Estado, Jogo};

/// Página "Aplicativos > Inicialização" das Configurações do Windows.
const URI_INICIALIZACAO: &str = "ms-settings:startupapps";

/// Por que o botão está bloqueado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bloqueio {
    /// Há jogo ou anticheat aberto (adiamento ou jogo em execução).
    JogoAberto,
    /// Sem estado do serviço: não dá para saber se há jogo aberto.
    EstadoDesconhecido,
}

/// Falha ao abrir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FalhaAbrir {
    Bloqueado(Bloqueio),
    /// O Windows recusou (código de `ShellExecuteW` <= 32).
    Windows,
}

/// Regra de bloqueio (pura). `estado` é o último estado **do serviço**
/// (`None` no modo de visualização, ao conectar ou sem resposta).
pub fn bloqueio(estado: Option<&Estado>, jogos: &[Jogo]) -> Option<Bloqueio> {
    let Some(e) = estado else {
        return Some(Bloqueio::EstadoDesconhecido);
    };
    if e.adiamento.is_some() || jogos.iter().any(|j| j.em_execucao) {
        return Some(Bloqueio::JogoAberto);
    }
    None
}

/// Abre a página de inicialização, se a regra permitir. Chamar só no
/// tratador de clique.
pub fn abrir_inicializacao(estado: Option<&Estado>, jogos: &[Jogo]) -> Result<(), FalhaAbrir> {
    if let Some(b) = bloqueio(estado, jogos) {
        return Err(FalhaAbrir::Bloqueado(b));
    }
    shell_abrir()
}

#[cfg(windows)]
fn shell_abrir() -> Result<(), FalhaAbrir> {
    let uri: Vec<u16> = URI_INICIALIZACAO.encode_utf16().chain(Some(0)).collect();
    let verbo: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    // SAFETY: cadeias UTF-16 terminadas em zero que vivem até o fim da
    // chamada; janela, parâmetros e pasta nulos.
    let r = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            verbo.as_ptr(),
            uri.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        )
    };
    if r as usize > 32 {
        Ok(())
    } else {
        Err(FalhaAbrir::Windows)
    }
}

#[cfg(not(windows))]
fn shell_abrir() -> Result<(), FalhaAbrir> {
    let _ = URI_INICIALIZACAO;
    Err(FalhaAbrir::Windows)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn estado() -> Estado {
        crate::demonstracao::dados(crate::demonstracao::Cenario::Otimizado)
            .estado
            .expect("estado da demonstração")
    }

    #[test]
    fn r_s_04_estado_desconhecido_bloqueia() {
        assert_eq!(bloqueio(None, &[]), Some(Bloqueio::EstadoDesconhecido));
        // A função pública recusa antes de chegar ao Windows.
        assert_eq!(
            abrir_inicializacao(None, &[]),
            Err(FalhaAbrir::Bloqueado(Bloqueio::EstadoDesconhecido))
        );
    }

    #[test]
    fn r_s_04_jogo_ou_anticheat_aberto_bloqueia() {
        let d = crate::demonstracao::dados(crate::demonstracao::Cenario::JogoAberto);
        let e = d.estado.as_ref().unwrap();
        assert!(e.adiamento.is_some());
        assert_eq!(bloqueio(Some(e), &[]), Some(Bloqueio::JogoAberto));
        assert_eq!(
            abrir_inicializacao(Some(e), &[]),
            Err(FalhaAbrir::Bloqueado(Bloqueio::JogoAberto))
        );
        // Jogo em execução sem adiamento também bloqueia.
        let livre = estado();
        assert!(livre.adiamento.is_none());
        let mut jogos = d.jogos.clone();
        assert!(jogos.iter().any(|j| j.em_execucao));
        assert_eq!(bloqueio(Some(&livre), &jogos), Some(Bloqueio::JogoAberto));
        for j in &mut jogos {
            j.em_execucao = false;
        }
        assert_eq!(bloqueio(Some(&livre), &jogos), None);
    }
}
