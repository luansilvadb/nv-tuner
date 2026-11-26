//! Núcleo puro da atualização automática do nv-tuner (`docs/spec/atualizacao.md`).
//!
//! Sem Windows, sem rede e sem E/S: só funções sobre bytes e estruturas. O serviço
//! (`nvt-service::atualizacao`) e a ferramenta `nvt-assinar` usam o mesmo verificador
//! (U-16).

// Segunda camada de U-15a; a primeira é o `build.rs` (perfil/opt-level).
#[cfg(all(feature = "laboratorio", not(debug_assertions)))]
compile_error!(
    "a feature `laboratorio` (URL e chave de ensaio) não pode entrar em build de release (U-15a)"
);

pub mod chave;
pub mod estado;
pub mod log_msi;
pub mod manifesto;
pub mod politica;
pub mod versao;

#[cfg(test)]
mod teste;

pub use chave::{CHAVE_OFICIAL, Chave, PUB_OFICIAL, chave_oficial_provisoria};
pub use estado::{
    AtualizacaoConcluida, Contexto, Efeito, EstadoPersistido, Evento, FalhaDownload,
    FalhaRegistrada, Fase, MotivoAdiamento, PedidoDownload, PedidoLancamento, SessaoAgente,
    UltimoResultado, transicao,
};
pub use manifesto::{
    ErroVerificacao, Manifesto, comentario_manifesto, comentario_msi, gerar_manifesto_json,
    nome_log, nome_msi, sha256_hex, url_msi, verificar_manifesto, verificar_manifesto_oficial,
};
pub use politica::{
    Decisao, ErroUrl, ResultadoMsi, UrlPermitida, interpretar_codigo_msi, politica_url,
};
pub use versao::Versao;

/// Nome exato do produto no manifesto.
pub const PRODUTO: &str = "nv-tuner";

/// Base fixa das releases (nunca lida da rede).
pub const URL_REPOSITORIO: &str = "https://github.com/luansilvadb/nv-tuner";

/// URL fixa do manifesto da release Latest (§6.1).
pub const URL_MANIFESTO: &str =
    "https://github.com/luansilvadb/nv-tuner/releases/latest/download/atualizacao.json";

/// URL fixa da assinatura do manifesto (§6.1).
pub const URL_ASSINATURA: &str =
    "https://github.com/luansilvadb/nv-tuner/releases/latest/download/atualizacao.json.minisig";

/// URL base do repositório em uso: a oficial; com a feature `laboratorio`, a de
/// ensaio (`NVT_ATUALIZACAO_URL_ENSAIO` em compilação), se definida.
pub fn url_base() -> &'static str {
    #[cfg(feature = "laboratorio")]
    if let Some(u) = laboratorio::URL_BASE_ENSAIO {
        let u = u.trim().trim_end_matches('/');
        if !u.is_empty() {
            return u;
        }
    }
    URL_REPOSITORIO
}

/// URL do manifesto sobre [`url_base`] (igual a [`URL_MANIFESTO`] sem a feature).
pub fn url_manifesto() -> String {
    format!("{}/releases/latest/download/atualizacao.json", url_base())
}

/// URL da assinatura sobre [`url_base`] (igual a [`URL_ASSINATURA`] sem a feature).
pub fn url_assinatura() -> String {
    format!(
        "{}/releases/latest/download/atualizacao.json.minisig",
        url_base()
    )
}

/// Hosts exatos aceitos, inclusive em redirecionamentos (U-04).
pub const HOSTS_PERMITIDOS: &[&str] = &[
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// Máximo de saltos de redirecionamento (U-04).
pub const MAX_REDIRECIONAMENTOS: u8 = 5;

/// Limites de tamanho (U-06).
pub const LIMITE_MANIFESTO: usize = 16 * 1024;
pub const LIMITE_ASSINATURA: usize = 4 * 1024;
pub const LIMITE_MSI: u64 = 64 * 1024 * 1024;
/// Quanto do fim do log `/l*v` é lido (U-35).
pub const LIMITE_LOG_MSI: u64 = 8 * 1024 * 1024;

/// URL e chave de ensaio para o laboratório (§13). Só existem com a feature
/// `laboratorio`, que não compila em release; valores vêm do ambiente de compilação.
#[cfg(feature = "laboratorio")]
pub mod laboratorio {
    /// URL base do repositório de ensaio, ex.: `https://github.com/<fork>/nv-tuner`.
    pub const URL_BASE_ENSAIO: Option<&str> = option_env!("NVT_ATUALIZACAO_URL_ENSAIO");
    /// Linha base64 da chave pública de ensaio.
    pub const CHAVE_ENSAIO: Option<&str> = option_env!("NVT_ATUALIZACAO_CHAVE_ENSAIO");
}

#[cfg(all(test, not(feature = "laboratorio")))]
mod testes_url {
    use super::*;

    #[test]
    fn sem_laboratorio_urls_sao_as_oficiais() {
        assert_eq!(url_base(), URL_REPOSITORIO);
        assert_eq!(url_manifesto(), URL_MANIFESTO);
        assert_eq!(url_assinatura(), URL_ASSINATURA);
        assert!(url_msi("1.2.3".parse().unwrap()).starts_with(URL_REPOSITORIO));
    }
}
