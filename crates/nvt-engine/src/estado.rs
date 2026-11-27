//! Estado persistente do motor (JSON num diretório configurável).
//!
//! Arquivos:
//!
//! - `snapshot-original.json` — estado do DRS antes da **primeira** aplicação.
//!   Gravado num temporário (com `sync_all`) e publicado por *hard link*, que
//!   falha se o destino existir: nunca é sobrescrito, nem parcialmente.
//! - `estado.json` — preferências, escolhas travadas, histórico, estado
//!   esperado, versão do driver vista, perfis criados pelo app e originais de
//!   perfis que só passaram a ser geridos depois do snapshot. Gravado num
//!   temporário com `sync_all` e trocado por `rename` (substituição atômica no
//!   NTFS); a versão anterior válida fica em `estado.anterior.json`.
//! - `perfis-criados.json` — cópia redundante da lista de perfis criados pelo
//!   app (gravada a cada [`Armazem::salvar`]). Sobrevive à perda de
//!   `estado.json` e `estado.anterior.json`: na abertura, é unida à lista do
//!   estado, para a restauração continuar encontrando esses perfis.
//! - `snapshot-perdido.json` — marcador gravado pelo serviço quando o
//!   diretório de estado vai para a quarentena (ver
//!   [`Armazem::marcar_snapshot_perdido`]). Enquanto existir, o snapshot
//!   capturado depois disso é uma **base pós-quarentena** (o driver pode já
//!   ter mudanças do app), e a abertura avisa
//!   ([`CodigoAviso::SnapshotOriginalPerdido`]).
//!
//! Arquivo corrompido não prende o serviço (ver [`Armazem::abrir`]).

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use nvt_core::{Alvo, EscolhaUsuario, Origem, Preferences};
use nvt_drs::{ProfileRef, ProfileSnapshot, Snapshot};
use serde::{Deserialize, Serialize};

use crate::aviso::{Aviso, CodigoAviso, NivelAviso};
use crate::erro::{ErroEngine, Resultado};

/// Versão do formato de `estado.json`.
pub const VERSAO_ESTADO: u32 = 1;

/// Limite padrão de entradas do histórico (as mais antigas são descartadas).
pub const LIMITE_HISTORICO_PADRAO: usize = 2000;

const ARQ_ESTADO: &str = "estado.json";
const ARQ_ESTADO_ANTERIOR: &str = "estado.anterior.json";
const ARQ_SNAPSHOT: &str = "snapshot-original.json";
const ARQ_PERFIS_CRIADOS: &str = "perfis-criados.json";
const ARQ_SNAPSHOT_PERDIDO: &str = "snapshot-perdido.json";

/// Máximo de escolhas travadas. Acima disso, travar pelo IPC é recusado com
/// erro (o estado é relido e regravado a cada pedido).
pub const MAX_TRAVADAS: usize = 500;

/// Valor de (perfil, setting) imediatamente antes de o app passar a geri-la.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", content = "valor")]
pub enum ValorAntes {
    /// A setting não estava gravada no perfil (ausente, herdada ou
    /// predefinida pela NVIDIA): reverter = remover do perfil.
    Inexistente,
    /// Valor DWORD gravado no próprio perfil: reverter = gravá-lo de volta.
    Valor(u32),
}

/// Compara nomes de perfil como o DRS: sem diferenciar maiúsculas.
pub fn mesmo_nome_perfil(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b) || a.to_lowercase() == b.to_lowercase()
}

/// Compara alvos com o nome do perfil sem diferenciar maiúsculas.
pub fn mesmo_alvo(a: &Alvo, b: &Alvo) -> bool {
    match (a, b) {
        (Alvo::Global, Alvo::Global) => true,
        (Alvo::Jogo(x), Alvo::Jogo(y)) => mesmo_nome_perfil(x, y),
        _ => false,
    }
}

/// Setting travada como escolha do usuário.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Travada {
    pub alvo: Alvo,
    /// Nome simbólico (catálogo).
    pub setting: String,
    /// Valor encontrado quando a mudança manual foi detectada.
    pub valor_usuario: Option<u32>,
    pub data: String,
    pub motivo: String,
}

/// Valor que o app gravou e espera continuar encontrando no driver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Esperado {
    pub alvo: Alvo,
    pub perfil: ProfileRef,
    pub setting: String,
    pub id: u32,
    pub valor: u32,
    pub origem: Origem,
    /// Valor imediatamente antes da primeira gravação do app nesta (perfil,
    /// setting): é o que a reversão restaura quando a setting sai do plano.
    /// `None` = desconhecido (estado antigo): vale o snapshot original.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub antes_da_gestao: Option<ValorAntes>,
}

/// De onde veio um registro do histórico.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo")]
pub enum OrigemRegistro {
    /// Plano do avaliador (regra ou preferência), inclusive a reversão ao
    /// original de uma setting que saiu do plano.
    Plano { origem: Origem },
    /// Reaplicação após reset em massa.
    Reaplicacao { origem: Origem },
    /// Mudança manual detectada (vira escolha travada).
    Usuario,
    /// Restauração do snapshot original.
    Restauracao,
    /// Perfil ou aplicativo criado no DRS.
    CriacaoPerfil,
}

/// Uma linha do histórico de mudanças.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistroHistorico {
    /// ISO 8601 UTC.
    pub data: String,
    /// Nome do perfil (`Base Profile` para o global).
    pub perfil: String,
    /// Nome simbólico, ou vazio em criação de perfil/aplicativo.
    pub setting: String,
    pub id: u32,
    pub antes: Option<u32>,
    pub depois: Option<u32>,
    pub origem: OrigemRegistro,
    pub motivo: String,
}

/// Perfil criado pelo app (`CreateProfile` + `CreateApplication`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerfilCriado {
    pub nome: String,
    pub exe: String,
}

/// Conteúdo de `estado.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Estado {
    pub versao: u32,
    /// Preferências do usuário. `escolhas_usuario` é derivado de `travadas`.
    pub preferencias: Preferences,
    pub travadas: Vec<Travada>,
    pub historico: Vec<RegistroHistorico>,
    pub esperado: Vec<Esperado>,
    /// Versão bruta do driver vista na última aplicação/verificação.
    pub driver_visto: Option<u32>,
    pub perfis_criados: Vec<PerfilCriado>,
    /// Estado original de perfis que passaram a ser geridos depois do snapshot.
    pub perfis_adicionais: Vec<ProfileSnapshot>,
}

impl Default for Estado {
    fn default() -> Self {
        Self {
            versao: VERSAO_ESTADO,
            preferencias: Preferences::default(),
            travadas: Vec::new(),
            historico: Vec::new(),
            esperado: Vec::new(),
            driver_visto: None,
            perfis_criados: Vec::new(),
            perfis_adicionais: Vec::new(),
        }
    }
}

impl Estado {
    /// Preferências com as escolhas travadas aplicadas (entrada do avaliador).
    pub fn preferencias_efetivas(&self) -> Preferences {
        let mut p = self.preferencias.clone();
        p.escolhas_usuario = self
            .travadas
            .iter()
            .map(|t| EscolhaUsuario {
                alvo: t.alvo.clone(),
                setting: t.setting.clone(),
            })
            .collect();
        p
    }

    /// Insere ou atualiza o valor esperado de (perfil, id). Ao atualizar,
    /// mantém o [`Esperado::antes_da_gestao`] já registrado (a gestão não
    /// recomeçou).
    pub fn registrar_esperado(&mut self, mut e: Esperado) {
        match self
            .esperado
            .iter_mut()
            .find(|x| x.perfil == e.perfil && x.id == e.id)
        {
            Some(x) => {
                if x.antes_da_gestao.is_some() {
                    e.antes_da_gestao = x.antes_da_gestao;
                }
                *x = e;
            }
            None => self.esperado.push(e),
        }
    }

    /// Há escolha travada para (alvo, setting), com o perfil comparado sem
    /// diferenciar maiúsculas?
    pub fn travada(&self, alvo: &Alvo, setting: &str) -> bool {
        self.travadas
            .iter()
            .any(|t| t.setting == setting && mesmo_alvo(&t.alvo, alvo))
    }

    /// Une perfis criados (sem duplicar, sem diferenciar maiúsculas).
    fn unir_perfis_criados(&mut self, outros: Vec<PerfilCriado>) -> bool {
        let mut mudou = false;
        for p in outros {
            if !self.criado_pelo_app(&p.nome) {
                self.perfis_criados.push(p);
                mudou = true;
            }
        }
        mudou
    }

    /// Remove travas duplicadas só pela grafia do perfil (estados antigos).
    fn deduplicar_travadas(&mut self) {
        let mut vistas: Vec<Travada> = Vec::with_capacity(self.travadas.len());
        for t in std::mem::take(&mut self.travadas) {
            if !vistas
                .iter()
                .any(|v| v.setting == t.setting && mesmo_alvo(&v.alvo, &t.alvo))
            {
                vistas.push(t);
            }
        }
        self.travadas = vistas;
    }

    /// Para de vigiar (perfil, id).
    pub fn remover_esperado(&mut self, perfil: &ProfileRef, id: u32) {
        self.esperado
            .retain(|x| !(&x.perfil == perfil && x.id == id));
    }

    /// Acrescenta ao histórico, com coalescência e limite.
    ///
    /// Coalescência: um registro de origem [`OrigemRegistro::Usuario`] (travar)
    /// idêntico ao último registro da mesma setting no mesmo perfil (mesmos
    /// valores e motivo) só atualiza a data desse registro, em vez de criar outro;
    /// travar/destravar repetidos não fazem o histórico crescer.
    /// Limite: acima de `limite` entradas, as mais antigas são descartadas.
    pub fn registrar_historico(&mut self, r: RegistroHistorico, limite: usize) {
        if r.origem == OrigemRegistro::Usuario
            && let Some(ultimo) = self
                .historico
                .iter_mut()
                .rev()
                .find(|h| h.perfil == r.perfil && h.id == r.id && h.setting == r.setting)
            && ultimo.origem == r.origem
            && ultimo.antes == r.antes
            && ultimo.depois == r.depois
            && ultimo.motivo == r.motivo
        {
            ultimo.data = r.data;
        } else {
            self.historico.push(r);
        }
        self.podar_historico(limite);
    }

    /// Descarta as entradas mais antigas acima de `limite`.
    pub fn podar_historico(&mut self, limite: usize) {
        if self.historico.len() > limite {
            let excesso = self.historico.len() - limite;
            self.historico.drain(..excesso);
        }
    }

    /// Perfil criado pelo app?
    pub fn criado_pelo_app(&self, nome: &str) -> bool {
        self.perfis_criados
            .iter()
            .any(|p| mesmo_nome_perfil(&p.nome, nome))
    }
}

/// Estado carregado e onde gravá-lo.
#[derive(Debug, Clone)]
pub struct Armazem {
    dir: Option<PathBuf>,
    pub estado: Estado,
    snapshot_original: Option<Snapshot>,
    /// Motivo, se `snapshot-original.json` existe mas não pôde ser lido.
    snapshot_indisponivel: Option<String>,
    /// Problemas encontrados ao abrir (arquivo corrompido, recuperação).
    avisos: Vec<Aviso>,
    /// Aberto com [`Armazem::abrir_somente_leitura`]: nenhuma gravação.
    somente_leitura: bool,
    limite_historico: usize,
    /// Detalhe do marcador `snapshot-perdido.json`, se existir.
    base_pos_quarentena: Option<String>,
}

impl Armazem {
    /// `%ProgramData%\nv-tuner`.
    pub fn diretorio_padrao() -> PathBuf {
        let base = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        base.join(nvt_core::produto::PASTA_DADOS)
    }

    /// Estado só em memória (testes e dry-run): nada é gravado em disco.
    pub fn em_memoria() -> Self {
        Self {
            dir: None,
            estado: Estado::default(),
            snapshot_original: None,
            snapshot_indisponivel: None,
            avisos: Vec::new(),
            somente_leitura: false,
            limite_historico: LIMITE_HISTORICO_PADRAO,
            base_pos_quarentena: None,
        }
    }

    /// Carrega o estado de `dir` (arquivos ausentes = estado vazio).
    ///
    /// Arquivos ilegíveis por conteúdo não fazem a abertura falhar:
    ///
    /// - `estado.json` inválido (vazio, truncado, JSON ruim ou versão não
    ///   suportada): usa-se `estado.anterior.json` se for válido, senão um
    ///   estado limpo. O inválido é renomeado para
    ///   `estado.json.corrompido-<data>` e o estado recuperado (ou limpo) é
    ///   **gravado na hora** em `estado.json`. Se essa gravação falhar, a
    ///   próxima abertura acha `estado.json` ausente e recupera de novo da
    ///   cópia (abaixo): nunca volta a um estado vazio. O snapshot original é
    ///   preservado.
    /// - `estado.json` ausente com `estado.anterior.json` válido (recuperação
    ///   interrompida, arquivo apagado): o estado vem da cópia, com aviso, e
    ///   é gravado na hora.
    /// - `perfis-criados.json` é unido à lista de perfis criados do estado
    ///   (sobrevive à perda das duas cópias do estado).
    /// - `snapshot-original.json` inválido: o snapshot fica **indisponível**
    ///   ([`Armazem::snapshot_indisponivel`]), a restauração é recusada com aviso
    ///   e o arquivo **não** é sobrescrito nem renomeado (é o único registro do
    ///   estado original; só uma pessoa decide o que fazer com ele).
    /// - `snapshot-perdido.json` presente: aviso
    ///   [`CodigoAviso::SnapshotOriginalPerdido`] (ver
    ///   [`Armazem::base_pos_quarentena`]).
    ///
    /// Os arquivos são abertos sem seguir reparse points: link simbólico ou
    /// junção no lugar de um deles é recusado (erro). Erros de E/S (permissão,
    /// disco) continuam sendo erro.
    pub fn abrir(dir: impl Into<PathBuf>) -> Resultado<Self> {
        Self::abrir_com(dir.into(), false)
    }

    /// Como [`Armazem::abrir`], mas sem nenhum efeito no disco (`status`,
    /// `dry-run`, `--somente-leitura`): nunca renomeia, cria nem grava nada.
    ///
    /// `estado.json` inválido (ou ausente) fica como está; o estado vem de
    /// `estado.anterior.json` (se válido) ou começa limpo, só em memória.
    /// [`Armazem::salvar`] e [`Armazem::guardar_snapshot_original`] são
    /// recusados com [`ErroEngine::SomenteLeitura`].
    pub fn abrir_somente_leitura(dir: impl Into<PathBuf>) -> Resultado<Self> {
        Self::abrir_com(dir.into(), true)
    }

    /// Grava o marcador `snapshot-perdido.json` em `dir` (o diretório novo e
    /// seguro). O serviço chama isto logo depois de pôr o diretório de estado
    /// em quarentena: o snapshot original ficou lá e não é confiável, então o
    /// próximo snapshot capturado é só uma base pós-quarentena (o driver pode
    /// já ter mudanças do app).
    pub fn marcar_snapshot_perdido(dir: &Path, detalhe: &str) -> Resultado<()> {
        let json = serde_json::json!({
            "motivo": "diretório de estado posto em quarentena",
            "detalhe": detalhe,
            "data": agora_iso(),
        });
        let tmp = dir.join(format!("{ARQ_SNAPSHOT_PERDIDO}.tmp"));
        gravar_duravel(&tmp, json.to_string().as_bytes())?;
        fs::rename(&tmp, dir.join(ARQ_SNAPSHOT_PERDIDO)).map_err(io)
    }

    fn abrir_com(dir: PathBuf, somente_leitura: bool) -> Resultado<Self> {
        let mut avisos = Vec::new();
        let caminho_estado = dir.join(ARQ_ESTADO);
        let caminho_anterior = dir.join(ARQ_ESTADO_ANTERIOR);
        // Gravar o estado já ao fim da abertura (recuperado/limpo/unido).
        let mut gravar_ja = false;
        let mut estado = match ler_opcional(&caminho_estado)? {
            None => match ler_opcional(&caminho_anterior)?.map(|s| interpretar_estado(&s)) {
                Some(Ok(e)) => {
                    avisos.push(Aviso::novo(
                        NivelAviso::Atencao,
                        CodigoAviso::EstadoRecuperado,
                        if somente_leitura {
                            "O arquivo de estado não existe; o estado foi lido da cópia de \
                             segurança (estado.anterior.json), só em memória."
                        } else {
                            "O arquivo de estado não existe; o estado foi recuperado da cópia \
                             de segurança (estado.anterior.json)."
                        },
                        Some(format!(
                            "{ARQ_ESTADO} ausente; estado recuperado de {ARQ_ESTADO_ANTERIOR}"
                        )),
                    ));
                    gravar_ja = true;
                    e
                }
                _ => Estado::default(),
            },
            Some(s) => match interpretar_estado(&s) {
                Ok(e) => e,
                Err(motivo) => {
                    // Lida antes de renomear: se a cópia for recusada (E/S,
                    // reparse point), nada foi mexido no diretório.
                    let anterior = ler_opcional(&caminho_anterior)?.map(|s| interpretar_estado(&s));
                    if somente_leitura {
                        avisos.push(Aviso::novo(
                            NivelAviso::Atencao,
                            CodigoAviso::EstadoCorrompido,
                            "O arquivo de estado está corrompido. Em modo somente leitura ele \
                             não foi movido nem alterado; o serviço normal o guarda à parte \
                             (estado.json.corrompido-<data>).",
                            Some(format!(
                                "{ARQ_ESTADO} inválido ({motivo}); mantido no lugar (somente leitura)"
                            )),
                        ));
                    } else {
                        let destino = guardar_corrompido(&caminho_estado)?;
                        avisos.push(Aviso::novo(
                            NivelAviso::Atencao,
                            CodigoAviso::EstadoCorrompido,
                            "O arquivo de estado estava corrompido e foi guardado à parte \
                             (estado.json.corrompido-<data>).",
                            Some(format!(
                                "{ARQ_ESTADO} inválido ({motivo}); movido para {}",
                                destino.display()
                            )),
                        ));
                        gravar_ja = true;
                    }
                    if let Some(Ok(e)) = anterior {
                        avisos.push(Aviso::novo(
                            NivelAviso::Info,
                            CodigoAviso::EstadoRecuperado,
                            if somente_leitura {
                                "O estado foi lido da cópia de segurança \
                                 (estado.anterior.json), só em memória."
                            } else {
                                "O estado foi recuperado da cópia de segurança \
                                 (estado.anterior.json)."
                            },
                            Some(format!(
                                "estado recuperado da cópia de segurança {ARQ_ESTADO_ANTERIOR}"
                            )),
                        ));
                        e
                    } else {
                        avisos.push(Aviso::novo(
                            NivelAviso::Atencao,
                            CodigoAviso::EstadoReiniciado,
                            if somente_leitura {
                                "Sem cópia de segurança válida, o estado é lido como vazio \
                                 (só em memória; nada foi apagado). O snapshot original \
                                 foi preservado."
                            } else {
                                "Sem cópia de segurança válida, o estado recomeçou do zero: \
                                 preferências, escolhas travadas e histórico anteriores foram \
                                 perdidos. O snapshot original e a lista de perfis criados \
                                 pelo app foram preservados."
                            },
                            Some("iniciado com estado limpo (snapshot original preservado)".into()),
                        ));
                        Estado::default()
                    }
                }
            },
        };
        // Cópia redundante dos perfis criados (inválida = ignorada: a lista
        // do estado continua valendo).
        if let Some(s) = ler_opcional(&dir.join(ARQ_PERFIS_CRIADOS))?
            && let Ok(lista) = serde_json::from_str::<Vec<PerfilCriado>>(&s)
            && estado.unir_perfis_criados(lista)
        {
            gravar_ja = true;
        }
        estado.deduplicar_travadas();
        let mut snapshot_indisponivel = None;
        let snapshot_original = match ler_opcional(&dir.join(ARQ_SNAPSHOT))? {
            Some(s) => match Snapshot::from_json(&s) {
                Ok(snap) => Some(snap),
                Err(e) => {
                    let motivo = format!(
                        "{ARQ_SNAPSHOT} ilegível ({e}): snapshot original indisponível e \
                         restauração desabilitada; o arquivo não é sobrescrito \
                         automaticamente (confira-o ou remova-o manualmente)"
                    );
                    avisos.push(Aviso::novo(
                        NivelAviso::Erro,
                        CodigoAviso::SnapshotIndisponivel,
                        "O snapshot original está ilegível, por isso a restauração está \
                         desabilitada. O arquivo não é sobrescrito: confira-o ou remova-o \
                         manualmente (a próxima aplicação captura um novo original).",
                        Some(motivo.clone()),
                    ));
                    snapshot_indisponivel = Some(motivo);
                    None
                }
            },
            None => None,
        };
        let base_pos_quarentena = ler_opcional(&dir.join(ARQ_SNAPSHOT_PERDIDO))?.map(|s| {
            serde_json::from_str::<serde_json::Value>(&s)
                .ok()
                .and_then(|v| v.get("detalhe").and_then(|d| d.as_str()).map(str::to_owned))
                .unwrap_or_else(|| s.trim().to_owned())
        });
        if let Some(detalhe) = &base_pos_quarentena {
            avisos.push(Aviso::novo(
                NivelAviso::Atencao,
                CodigoAviso::SnapshotOriginalPerdido,
                if snapshot_original.is_some() {
                    "O snapshot original foi perdido quando o diretório de estado foi posto \
                     em quarentena. O snapshot atual é uma base pós-quarentena: pode já \
                     conter mudanças do nv-tuner, e “voltar ao original” volta a essa base, \
                     não ao estado anterior ao app."
                } else {
                    "O snapshot original foi perdido quando o diretório de estado foi posto \
                     em quarentena. A próxima aplicação registra o driver atual como base \
                     pós-quarentena, que pode já conter mudanças do nv-tuner; “voltar ao \
                     original” passará a voltar a essa base."
                },
                Some(format!("{ARQ_SNAPSHOT_PERDIDO}: {detalhe}")),
            ));
        }
        let mut a = Self {
            dir: Some(dir),
            estado,
            snapshot_original,
            snapshot_indisponivel,
            avisos,
            somente_leitura,
            limite_historico: LIMITE_HISTORICO_PADRAO,
            base_pos_quarentena,
        };
        a.estado.podar_historico(a.limite_historico);
        // Persiste já o recuperado: o `estado.json` inválido foi renomeado, e
        // sem isto a recuperação só existiria em memória.
        if gravar_ja && !somente_leitura {
            a.salvar()?;
        }
        Ok(a)
    }

    /// O snapshot original foi perdido numa quarentena do diretório de
    /// estado (marcador `snapshot-perdido.json`)? Se sim, o snapshot atual
    /// (se houver) é só uma base pós-quarentena. Devolve o detalhe do marcador.
    pub fn base_pos_quarentena(&self) -> Option<&str> {
        self.base_pos_quarentena.as_deref()
    }

    /// Aberto com [`Armazem::abrir_somente_leitura`]?
    pub fn somente_leitura(&self) -> bool {
        self.somente_leitura
    }

    /// Diretório do estado (`None` em memória).
    pub fn diretorio(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Snapshot original, se já capturado e legível.
    pub fn snapshot_original(&self) -> Option<&Snapshot> {
        self.snapshot_original.as_ref()
    }

    /// Motivo, se o arquivo do snapshot original existe mas está ilegível.
    pub fn snapshot_indisponivel(&self) -> Option<&str> {
        self.snapshot_indisponivel.as_deref()
    }

    /// Avisos da abertura (arquivos corrompidos, recuperação, snapshot ilegível).
    pub fn avisos(&self) -> &[Aviso] {
        &self.avisos
    }

    /// Limite de entradas do histórico.
    pub fn limite_historico(&self) -> usize {
        self.limite_historico
    }

    /// Troca o limite de entradas do histórico (mínimo 1) e poda o excesso.
    pub fn definir_limite_historico(&mut self, limite: usize) {
        self.limite_historico = limite.max(1);
        self.estado.podar_historico(self.limite_historico);
    }

    /// Guarda o snapshot original. Se já existir (ou estiver indisponível por
    /// arquivo ilegível), não faz nada e devolve `false`.
    pub fn guardar_snapshot_original(&mut self, s: Snapshot) -> Resultado<bool> {
        if self.snapshot_original.is_some() || self.snapshot_indisponivel.is_some() {
            return Ok(false);
        }
        if self.somente_leitura {
            return Err(ErroEngine::SomenteLeitura);
        }
        if let Some(dir) = &self.dir {
            fs::create_dir_all(dir).map_err(io)?;
            let caminho = dir.join(ARQ_SNAPSHOT);
            let tmp = dir.join(format!("{ARQ_SNAPSHOT}.tmp"));
            gravar_duravel(&tmp, s.to_json()?.as_bytes())?;
            // Publica sem nunca substituir: o hard link falha se o destino existir.
            let publicado = match fs::hard_link(&tmp, &caminho) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(e),
                // Sistema de arquivos sem hard link: confere e renomeia.
                Err(_) if !caminho.exists() => fs::rename(&tmp, &caminho),
                Err(e) => Err(e),
            };
            let _ = fs::remove_file(&tmp);
            publicado.map_err(|e| {
                ErroEngine::Io(format!(
                    "{}: {e} (snapshot original nunca é sobrescrito)",
                    caminho.display()
                ))
            })?;
        }
        self.snapshot_original = Some(s);
        Ok(true)
    }

    /// Grava `estado.json` de forma atômica: temporário + `sync_all`, cópia de
    /// segurança da versão anterior válida e `rename`.
    pub fn salvar(&self) -> Resultado<()> {
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        if self.somente_leitura {
            return Err(ErroEngine::SomenteLeitura);
        }
        fs::create_dir_all(dir).map_err(io)?;
        let json = serde_json::to_string_pretty(&self.estado)
            .map_err(|e| ErroEngine::Json(e.to_string()))?;
        let tmp = dir.join(format!("{ARQ_ESTADO}.tmp"));
        gravar_duravel(&tmp, json.as_bytes())?;
        let destino = dir.join(ARQ_ESTADO);
        if let Ok(Some(atual)) = ler_opcional(&destino)
            && interpretar_estado(&atual).is_ok()
        {
            let anterior_tmp = dir.join(format!("{ARQ_ESTADO_ANTERIOR}.tmp"));
            gravar_duravel(&anterior_tmp, atual.as_bytes())?;
            fs::rename(&anterior_tmp, dir.join(ARQ_ESTADO_ANTERIOR)).map_err(io)?;
        }
        fs::rename(&tmp, destino).map_err(io)?;
        // Cópia redundante dos perfis criados (ver o módulo): só é regravada
        // quando muda. Nunca encolhe por causa de um estado perdido, porque a
        // abertura já uniu a cópia à lista do estado.
        let lista = serde_json::to_string_pretty(&self.estado.perfis_criados)
            .map_err(|e| ErroEngine::Json(e.to_string()))?;
        let caminho = dir.join(ARQ_PERFIS_CRIADOS);
        if ler_opcional(&caminho).ok().flatten().as_deref() != Some(lista.as_str()) {
            let tmp = dir.join(format!("{ARQ_PERFIS_CRIADOS}.tmp"));
            gravar_duravel(&tmp, lista.as_bytes())?;
            fs::rename(&tmp, caminho).map_err(io)?;
        }
        Ok(())
    }
}

/// Grava e força para o disco antes de qualquer `rename`. O temporário é
/// sempre criado do zero (`create_new`, sem seguir reparse points): um link
/// plantado no lugar dele é removido (o próprio link, não o alvo) ou faz a
/// gravação falhar, nunca é seguido.
fn gravar_duravel(p: &Path, dados: &[u8]) -> Resultado<()> {
    let _ = fs::remove_file(p);
    let mut f = abrir_sem_reparse(p, fs::OpenOptions::new().write(true).create_new(true))
        .map_err(|e| ErroEngine::Io(format!("{}: {e}", p.display())))?;
    f.write_all(dados).map_err(io)?;
    f.sync_all().map_err(io)
}

fn interpretar_estado(s: &str) -> Result<Estado, String> {
    let e: Estado = serde_json::from_str(s).map_err(|e| e.to_string())?;
    if e.versao != VERSAO_ESTADO {
        return Err(format!(
            "versão {} não suportada (esperada {VERSAO_ESTADO})",
            e.versao
        ));
    }
    Ok(e)
}

/// Renomeia o arquivo inválido para `<nome>.corrompido-<data>` (sem sobrescrever).
fn guardar_corrompido(p: &Path) -> Resultado<PathBuf> {
    let carimbo = agora_iso().replace(':', "-");
    let nome = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut destino = p.with_file_name(format!("{nome}.corrompido-{carimbo}"));
    let mut n = 1;
    while destino.exists() {
        n += 1;
        destino = p.with_file_name(format!("{nome}.corrompido-{carimbo}-{n}"));
    }
    fs::rename(p, &destino).map_err(|e| ErroEngine::Io(format!("{}: {e}", p.display())))?;
    Ok(destino)
}

fn io(e: std::io::Error) -> ErroEngine {
    ErroEngine::Io(e.to_string())
}

/// Lê o arquivo sem seguir reparse points (`None` se não existir).
fn ler_opcional(p: &Path) -> Resultado<Option<String>> {
    let mut f = match abrir_sem_reparse(p, fs::OpenOptions::new().read(true)) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(ErroEngine::Io(format!("{}: {e}", p.display()))),
    };
    let mut b = Vec::new();
    f.read_to_end(&mut b)
        .map_err(|e| ErroEngine::Io(format!("{}: {e}", p.display())))?;
    // Bytes que não são UTF-8 contam como conteúdo inválido, não erro de E/S.
    Ok(Some(String::from_utf8_lossy(&b).into_owned()))
}

/// `FILE_FLAG_OPEN_REPARSE_POINT`: abre o próprio link/junção, não o alvo.
#[cfg(windows)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
/// `FILE_ATTRIBUTE_REPARSE_POINT`.
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

/// Abre `p` sem seguir reparse points e recusa o que não for arquivo comum
/// (link simbólico, junção, diretório). Defesa em profundidade: o serviço já
/// confere o diretório de estado, mas um link plantado dentro dele nunca é
/// seguido.
fn abrir_sem_reparse(p: &Path, opcoes: &mut fs::OpenOptions) -> std::io::Result<fs::File> {
    #[cfg(windows)]
    let f = {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        let f = opcoes.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).open(p)?;
        // Atributos do próprio handle (o link, se for um).
        if f.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(std::io::Error::other(
                "é link simbólico ou junção (reparse point); recusado",
            ));
        }
        f
    };
    #[cfg(not(windows))]
    let f = {
        if fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(std::io::Error::other("é link simbólico; recusado"));
        }
        opcoes.open(p)?
    };
    if !f.metadata()?.is_file() {
        return Err(std::io::Error::other("não é um arquivo comum; recusado"));
    }
    Ok(f)
}

/// Agora em ISO 8601 UTC.
pub fn agora_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    iso8601_utc(secs)
}

/// Segundos Unix para ISO 8601 UTC (`civil_from_days`, Howard Hinnant).
fn iso8601_utc(secs: u64) -> String {
    let dias = (secs / 86_400) as i64;
    let s = secs % 86_400;
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        s / 3600,
        (s / 60) % 60,
        s % 60
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn iso_conhecida() {
        assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601_utc(1_764_078_300), "2025-11-25T13:45:00Z");
    }
}
