//! Leitura comum dos arquivos TOML de regras e catálogos: parse com o nome
//! do arquivo no erro e checagem de `versao_schema`.

use serde::de::DeserializeOwned;

use crate::rules::ErroRegra;

/// Arquivo TOML com `versao_schema` no topo.
pub(crate) trait Versionado: DeserializeOwned {
    /// Versão suportada.
    const VERSAO_SCHEMA: u32;

    fn versao_schema(&self) -> u32;
}

/// Resultado de [`ler`]: o conteúdo e, se a versão não é a suportada, a
/// versão encontrada (cada carregador decide como reportar).
pub(crate) struct Lido<T> {
    pub dados: T,
    pub versao_errada: Option<u32>,
}

/// Interpreta `conteudo`; erro de parse vira [`ErroRegra::Toml`] com o
/// nome do arquivo.
pub(crate) fn ler<T: Versionado>(arquivo: &str, conteudo: &str) -> Result<Lido<T>, ErroRegra> {
    let dados: T = toml::from_str(conteudo).map_err(|e| ErroRegra::Toml {
        arquivo: arquivo.to_owned(),
        mensagem: e.to_string(),
    })?;
    let versao = dados.versao_schema();
    Ok(Lido {
        dados,
        versao_errada: (versao != T::VERSAO_SCHEMA).then_some(versao),
    })
}
