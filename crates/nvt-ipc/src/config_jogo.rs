//! DTOs de fio da configuração de arquivo de jogo (v7, Fase 3).
//!
//! Independentes de `nvt-core` e de `nvt-jogocfg`: o serviço e o agente
//! convertem entre estes tipos e os seus modelos internos. Caminhos
//! completos nunca trafegam; só o rótulo anonimizado ([`RotuloCaminho`]),
//! por exemplo `%USERPROFILE%\Documents\My Games\Rainbow Six - Siege\<conta>\GameSettings.ini`.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::protocolo::{MotivoNaoAplicada, StatusMudanca};
use crate::usuario::{Fato, ReferenciaTarefa, validar_texto};

/// Máximo de instâncias de arquivo de jogo relatadas numa consulta.
pub const ARQUIVOS_JOGO_MAX: usize = 64;
/// Máximo de chaves por arquivo (valores atuais, gestão, mudanças).
pub const CHAVES_ARQUIVO_MAX: usize = 32;
/// Máximo de ações de jogo numa tarefa e de resultados num relato.
pub const ACOES_JOGO_MAX: usize = 64;
/// Máximo de processos citados em `JanelaFechadaExigida`.
pub const PROCESSOS_MAX: usize = 32;
/// Comprimento máximo do rótulo anonimizado de caminho (bytes).
pub const ROTULO_CAMINHO_MAX: usize = 512;
/// Comprimento máximo de um valor textual de jogo (bytes).
pub const TEXTO_JOGO_MAX: usize = 256;

/// Prefixos aceitos no rótulo de caminho; nada de letra de unidade nem UNC.
pub const PREFIXOS_ROTULO: [&str; 4] = [
    "%USERPROFILE%\\",
    "%LOCALAPPDATA%\\",
    "%APPDATA%\\",
    "%STEAM%\\",
];

/// Alvo de configuração (`AlvoIpc` de `competitivo.md` §7.6). Na v7 só a
/// família de arquivo de jogo usa este tipo.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum AlvoIpc {
    /// `jogo`: id da base curada; `arquivo`: id lógico do `[[arquivo]]`.
    ConfigJogo { jogo: String, arquivo: String },
}

impl AlvoIpc {
    pub fn validar(&self) -> Result<(), String> {
        match self {
            Self::ConfigJogo { jogo, arquivo } => {
                validar_id(jogo)?;
                validar_id(arquivo)
            }
        }
    }
}

/// Destino tipado de uma chave em uma instância (conta) de arquivo de jogo.
/// `instancia` é a forma canônica opaca da conta resolvida (hash), nunca o
/// nome do usuário nem o SteamID.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DestinoJogoIpc {
    pub alvo: AlvoIpc,
    pub setting: String,
    pub instancia: String,
}

impl DestinoJogoIpc {
    pub fn validar(&self) -> Result<(), String> {
        self.alvo.validar()?;
        validar_chave(&self.setting)?;
        validar_instancia(&self.instancia)
    }
}

/// Caminho exibível, já anonimizado pelo agente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RotuloCaminho(pub String);

impl RotuloCaminho {
    pub fn validar(&self) -> Result<(), String> {
        let r = &self.0;
        validar_texto(r, ROTULO_CAMINHO_MAX)?;
        let resto = PREFIXOS_ROTULO
            .iter()
            .find_map(|p| r.strip_prefix(p))
            .ok_or("rótulo de caminho sem prefixo anonimizado")?;
        let invalido = resto.is_empty()
            || resto.contains(['/', ':', '"', '|', '?', '*'])
            || resto
                .split('\\')
                .any(|c| c.is_empty() || c == "." || c == "..")
            || resto.contains('%')
            || r.to_ascii_lowercase().contains("\\users\\");
        if invalido {
            return Err("rótulo de caminho inválido".into());
        }
        Ok(())
    }
}

/// Valor de uma chave de arquivo de jogo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "tipo",
    content = "valor",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ValorJogoIpc {
    Ausente,
    Inteiro(i64),
    Booleano(bool),
    Texto(String),
    /// Token `+fps_max <n>` de `LaunchOptions`; `None` = sem o token.
    TokenLancamento(Option<u32>),
}

impl ValorJogoIpc {
    pub fn validar(&self) -> Result<(), String> {
        if let Self::Texto(t) = self
            && (t.len() > TEXTO_JOGO_MAX || t.chars().any(char::is_control))
        {
            return Err("valor textual de jogo inválido".into());
        }
        Ok(())
    }
}

/// Valor atual de uma chave lida pelo agente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValorAtualJogo {
    pub setting: String,
    pub valor: Fato<ValorJogoIpc>,
}

/// Gestão persistida pelo agente para uma chave da instância.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GestaoJogoIpc {
    pub setting: String,
    pub antes: ValorJogoIpc,
    pub esperado: ValorJogoIpc,
    pub hash_antes: String,
    pub hash_depois: String,
    #[serde(default)]
    pub travado: bool,
}

/// Fatos de uma instância de arquivo de jogo, enviados em `AgenteConsultar`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FatoConfigJogo {
    pub alvo: AlvoIpc,
    pub instancia: String,
    pub rotulo: RotuloCaminho,
    /// `false` = arquivo ausente (`ArquivoAusente`).
    pub presente: bool,
    /// SHA-256 do arquivo atual; `None` se ausente ou ilegível.
    pub hash: Option<String>,
    pub somente_leitura: bool,
    pub na_nuvem: bool,
    pub conta_ambigua: bool,
    pub backup: EstadoBackup,
    pub valores: Vec<ValorAtualJogo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gestao: Vec<GestaoJogoIpc>,
}

impl FatoConfigJogo {
    pub fn validar(&self) -> Result<(), String> {
        self.alvo.validar()?;
        validar_instancia(&self.instancia)?;
        self.rotulo.validar()?;
        if let Some(h) = &self.hash {
            validar_hash(h)?;
        }
        if self.valores.len() > CHAVES_ARQUIVO_MAX || self.gestao.len() > CHAVES_ARQUIVO_MAX {
            return Err("muitas chaves no arquivo de jogo".into());
        }
        let mut vistos = BTreeSet::new();
        for v in &self.valores {
            validar_chave(&v.setting)?;
            if !vistos.insert(&v.setting) {
                return Err("chave de jogo repetida".into());
            }
            if let Fato::Conhecido(valor) = &v.valor {
                valor.validar()?;
            }
        }
        let mut vistos = BTreeSet::new();
        for g in &self.gestao {
            validar_chave(&g.setting)?;
            if !vistos.insert(&g.setting) {
                return Err("gestão de jogo repetida".into());
            }
            g.antes.validar()?;
            g.esperado.validar()?;
            validar_hash(&g.hash_antes)?;
            validar_hash(&g.hash_depois)?;
        }
        Ok(())
    }
}

/// Estado do backup `.orig` da instância.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoBackup {
    /// Ainda não houve escrita: não há backup.
    SemBackup,
    /// Backup criado e íntegro (hash confere).
    Integro,
    /// Backup ausente ou com hash divergente: nenhuma escrita é feita.
    Comprometido,
}

/// Ação tipada do agente sobre um arquivo de jogo. `hash_esperado` é o
/// SHA-256 que o arquivo deve ter antes da escrita; diferente → não escreve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum AcaoJogo {
    Aplicar {
        destino: DestinoJogoIpc,
        valor: ValorJogoIpc,
        hash_esperado: String,
    },
    /// Devolve o original (backup inteiro ou só as chaves do app).
    Restaurar {
        destino: DestinoJogoIpc,
        hash_esperado: String,
    },
    /// Reversão individual pedida pelo usuário.
    Reverter {
        destino: DestinoJogoIpc,
        hash_esperado: String,
    },
}

impl AcaoJogo {
    pub fn destino(&self) -> &DestinoJogoIpc {
        match self {
            Self::Aplicar { destino, .. }
            | Self::Restaurar { destino, .. }
            | Self::Reverter { destino, .. } => destino,
        }
    }

    pub fn validar(&self) -> Result<(), String> {
        self.destino().validar()?;
        match self {
            Self::Aplicar {
                valor,
                hash_esperado,
                ..
            } => {
                valor.validar()?;
                validar_hash(hash_esperado)
            }
            Self::Restaurar { hash_esperado, .. } | Self::Reverter { hash_esperado, .. } => {
                validar_hash(hash_esperado)
            }
        }
    }
}

/// Tarefa de jogo entregue ao agente em `PlanoAgente.tarefa_jogo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TarefaJogo {
    pub tarefa: ReferenciaTarefa,
    pub acoes: Vec<AcaoJogo>,
}

impl TarefaJogo {
    pub fn validar(&self) -> Result<(), String> {
        if self.acoes.len() > ACOES_JOGO_MAX {
            return Err("muitas ações de jogo".into());
        }
        let mut vistos = BTreeSet::new();
        for a in &self.acoes {
            a.validar()?;
            if !vistos.insert(a.destino()) {
                return Err("destino de jogo repetido na tarefa".into());
            }
        }
        Ok(())
    }
}

/// Resultado de uma ação de jogo relatado em `AgenteRelatar.resultados_jogo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultadoJogo {
    pub destino: DestinoJogoIpc,
    pub status: StatusMudanca,
    /// Só motivos da Fase 3 ([`MotivoNaoAplicada::e_config_jogo`]).
    pub motivo: Option<MotivoNaoAplicada>,
    pub antes: Option<ValorJogoIpc>,
    pub depois: Option<ValorJogoIpc>,
    pub hash_antes: Option<String>,
    pub hash_depois: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chaves_alteradas: Vec<String>,
}

impl ResultadoJogo {
    pub fn validar(&self) -> Result<(), String> {
        self.destino.validar()?;
        if let Some(m) = &self.motivo {
            if !m.e_config_jogo() {
                return Err("motivo fora da família de arquivo de jogo".into());
            }
            m.validar_config_jogo()?;
        }
        for v in [&self.antes, &self.depois].into_iter().flatten() {
            v.validar()?;
        }
        for h in [&self.hash_antes, &self.hash_depois].into_iter().flatten() {
            validar_hash(h)?;
        }
        if self.chaves_alteradas.len() > CHAVES_ARQUIVO_MAX {
            return Err("muitas chaves alteradas".into());
        }
        for c in &self.chaves_alteradas {
            validar_chave(c)?;
        }
        Ok(())
    }
}

pub(crate) fn validar_resultados_jogo(resultados: &[ResultadoJogo]) -> Result<(), String> {
    if resultados.len() > ACOES_JOGO_MAX {
        return Err("muitos resultados de jogo".into());
    }
    let mut vistos = BTreeSet::new();
    for r in resultados {
        r.validar()?;
        if !vistos.insert(&r.destino) {
            return Err("destino de jogo repetido no relato".into());
        }
    }
    Ok(())
}

pub(crate) fn validar_fatos_jogo(fatos: &[FatoConfigJogo]) -> Result<(), String> {
    if fatos.len() > ARQUIVOS_JOGO_MAX {
        return Err("muitos arquivos de jogo".into());
    }
    let mut vistos = BTreeSet::new();
    for f in fatos {
        f.validar()?;
        if !vistos.insert((&f.alvo, &f.instancia)) {
            return Err("instância de arquivo de jogo repetida".into());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Projeção para a UI (`plano.jogos[].config_jogo`)
// ---------------------------------------------------------------------------

/// O que o app fez nos arquivos de um jogo (seção “Arquivo do jogo”).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigJogoUi {
    pub arquivos: Vec<ArquivoJogoUi>,
    /// Orientações para destravar a aplicação (ex.: “feche a Steam”).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub orientacoes: Vec<OrientacaoConfigJogo>,
}

impl ConfigJogoUi {
    pub fn validar(&self) -> Result<(), String> {
        if self.arquivos.len() > ARQUIVOS_JOGO_MAX || self.orientacoes.len() > 16 {
            return Err("projeção de arquivo de jogo grande demais".into());
        }
        for a in &self.arquivos {
            a.validar()?;
        }
        for o in &self.orientacoes {
            if let OrientacaoConfigJogo::FecharJogo { processos } = o {
                validar_processos(processos)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArquivoJogoUi {
    pub alvo: AlvoIpc,
    pub rotulo: RotuloCaminho,
    /// Conta escolhida, já anonimizada (ex.: R6 com várias contas, F3-12).
    pub conta: Option<String>,
    pub backup: EstadoBackup,
    pub mudancas: Vec<MudancaArquivoJogo>,
    /// Há gestão do app que “Reverter” pode desfazer.
    pub reversivel: bool,
}

impl ArquivoJogoUi {
    fn validar(&self) -> Result<(), String> {
        self.alvo.validar()?;
        self.rotulo.validar()?;
        if let Some(c) = &self.conta {
            validar_texto(c, 128)?;
        }
        if self.mudancas.len() > CHAVES_ARQUIVO_MAX {
            return Err("muitas mudanças no arquivo de jogo".into());
        }
        for m in &self.mudancas {
            validar_chave(&m.setting)?;
            m.antes.validar()?;
            m.depois.validar()?;
            if let Some(motivo) = &m.motivo {
                motivo.validar_config_jogo()?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MudancaArquivoJogo {
    pub setting: String,
    pub antes: ValorJogoIpc,
    pub depois: ValorJogoIpc,
    pub status: StatusMudanca,
    pub motivo: Option<MotivoNaoAplicada>,
}

/// Orientação tipada; a UI gera o texto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum OrientacaoConfigJogo {
    /// “Feche a Steam para aplicar.”
    FecharSteam,
    /// “Feche o jogo para aplicar.”
    FecharJogo { processos: Vec<String> },
    /// “Desmarque somente leitura” (F3-06).
    DesmarcarSomenteLeitura,
    /// Arquivo sincronizado na nuvem: ajuste no jogo (F3-09).
    AjustarNoJogo,
    /// Várias contas empatadas: abra o jogo com a conta desejada (F3-12).
    EscolherConta,
}

// ---------------------------------------------------------------------------
// Validações
// ---------------------------------------------------------------------------

impl MotivoNaoAplicada {
    /// Motivo da família de arquivo de jogo (Fase 3).
    pub fn e_config_jogo(&self) -> bool {
        matches!(
            self,
            Self::ConfigPolitica { .. }
                | Self::JanelaFechadaExigida { .. }
                | Self::ArquivoNaNuvem
                | Self::ArquivoSomenteLeitura
                | Self::FormatoInesperado
                | Self::ArquivoAlteradoPeloJogo
                | Self::ContaAmbigua
                | Self::ArquivoAusente
                | Self::ReparseRecusado
        )
    }

    pub(crate) fn validar_config_jogo(&self) -> Result<(), String> {
        match self {
            Self::ConfigPolitica { politica } => validar_id(politica),
            Self::JanelaFechadaExigida { processos } => validar_processos(processos),
            _ => Ok(()),
        }
    }
}

fn validar_processos(processos: &[String]) -> Result<(), String> {
    if processos.is_empty() || processos.len() > PROCESSOS_MAX {
        return Err("lista de processos inválida".into());
    }
    for p in processos {
        validar_texto(p, 128)?;
        if p.contains(['\\', '/', ':']) {
            return Err("processo deve ser só o nome do executável".into());
        }
    }
    Ok(())
}

/// Id kebab/snake da base: `[a-z0-9_-]`, 1 a 64, começando por letra ou dígito.
fn validar_id(id: &str) -> Result<(), String> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'));
    if ok {
        Ok(())
    } else {
        Err("identificador de jogo/arquivo inválido".into())
    }
}

fn validar_chave(chave: &str) -> Result<(), String> {
    crate::protocolo::validar_setting(chave)
}

/// Instância opaca: 1 a 64 caracteres `[a-z0-9_-]`.
fn validar_instancia(instancia: &str) -> Result<(), String> {
    validar_id(instancia).map_err(|_| "instância de arquivo de jogo inválida".into())
}

/// SHA-256 em 64 dígitos hexadecimais minúsculos.
pub fn validar_hash(hash: &str) -> Result<(), String> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("hash SHA-256 inválido".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "config_jogo_testes.rs"]
pub(crate) mod testes;
