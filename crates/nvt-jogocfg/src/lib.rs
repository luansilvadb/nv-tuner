//! Edição preservadora de bytes de arquivos de configuração de jogos
//! (F3-01 e F3-04 a F3-12).
//!
//! - [`codificacao`]: UTF-8 com/sem BOM e UTF-16LE com BOM, regravados na mesma codificação.
//! - [`ini`] e [`vdf`]: editores que trocam só os bytes do valor alvo.
//! - [`lancamento`]: edição por token de `LaunchOptions` (`+fps_max <n>`).
//! - [`arquivo`]: leitura e gravação atômica sem seguir reparse points (único `unsafe`).
//! - [`backup`] e [`restauracao`]: cópia imutável com SHA-256 e restauração por hash.
//! - [`steam`], [`ubisoft`] e [`pastas`]: resolução de contas e pastas conhecidas.
//!
//! O crate nunca cria processos nem abre handles de processos.

pub mod arquivo;
pub mod backup;
pub mod codificacao;
mod erro;
mod hash;
pub mod ini;
pub mod lancamento;
pub mod pastas;
pub mod restauracao;
pub mod steam;
pub mod ubisoft;
pub mod vdf;

pub use codificacao::Codificacao;
pub use erro::{Erro, Motivo, Resultado};
pub use hash::{Hash, sha256};

/// Endereço de um valor dentro de um arquivo de configuração.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chave {
    /// `[secao]` / `chave=valor` (Unreal, Ubisoft). Comparação sem caixa ASCII.
    Ini { secao: String, chave: String },
    /// Caminho de chaves KeyValues (Valve), ex.:
    /// `UserLocalConfigStore/Software/Valve/Steam/apps/730/LaunchOptions`.
    Vdf { caminho: Vec<String> },
}

impl Chave {
    pub fn ini(secao: &str, chave: &str) -> Self {
        Chave::Ini {
            secao: secao.to_owned(),
            chave: chave.to_owned(),
        }
    }

    /// Caminho separado por `/`.
    pub fn vdf(caminho: &str) -> Self {
        Chave::Vdf {
            caminho: caminho.split('/').map(str::to_owned).collect(),
        }
    }
}

impl std::fmt::Display for Chave {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Chave::Ini { secao, chave } => write!(f, "[{secao}] {chave}"),
            Chave::Vdf { caminho } => write!(f, "{}", caminho.join("/")),
        }
    }
}

/// Lê o valor de `chave` no arquivo inteiro `bytes` (que precisa ser reconhecido por inteiro).
pub fn ler_valor(bytes: &[u8], chave: &Chave) -> Resultado<String> {
    let t = codificacao::decodificar(bytes)?;
    match chave {
        Chave::Ini { secao, chave } => ini::ler(&t.texto, secao, chave),
        Chave::Vdf { caminho } => vdf::ler(&t.texto, &refs(caminho)),
    }
}

/// Devolve `bytes` com só o valor de `chave` trocado por `novo`, na mesma codificação.
/// Chave ausente não é criada ([`Erro::ChaveAusente`]); formato irreconhecível aborta.
pub fn definir_valor(bytes: &[u8], chave: &Chave, novo: &str) -> Resultado<Vec<u8>> {
    let t = codificacao::decodificar(bytes)?;
    let texto = match chave {
        Chave::Ini { secao, chave } => ini::definir(&t.texto, secao, chave, novo)?,
        Chave::Vdf { caminho } => vdf::definir(&t.texto, &refs(caminho), novo)?,
    };
    Ok(codificacao::codificar(t.codificacao, &texto))
}

fn refs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}
