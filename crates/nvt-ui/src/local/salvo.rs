//! Leitura do estado salvo e do marcador de quarentena, sem alterar nada.

use super::*;

// ---------------------------------------------------------------------------
// Estado salvo (somente leitura)
// ---------------------------------------------------------------------------

/// Estado e snapshot lidos do disco, sem alterar nada.
#[derive(Debug, Default)]
pub struct Salvo {
    pub estado: Option<nvt_engine::Estado>,
    pub snapshot: Option<Snapshot>,
    pub avisos: Vec<Aviso>,
}

/// Conteúdo de um arquivo: `Ok(None)` = não existe.
pub type Conteudo = Result<Option<String>, String>;

pub(super) fn ler_arquivo(p: &Path) -> Conteudo {
    match std::fs::read_to_string(p) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

/// Lê o estado do serviço em `dir` sem nunca escrever (ao contrário de
/// `Armazem::abrir`, que renomeia um arquivo corrompido e grava o estado
/// recuperado).
pub fn ler_salvo(dir: &Path, agora: u64) -> Salvo {
    let mut salvo = interpretar_salvo(
        ler_arquivo(&dir.join("estado.json")),
        ler_arquivo(&dir.join("estado.anterior.json")),
        ler_arquivo(&dir.join("snapshot-original.json")),
        agora,
    );
    if let Some(a) =
        interpretar_marcador_quarentena(ler_arquivo(&dir.join("snapshot-perdido.json")), agora)
    {
        salvo.avisos.push(a);
    }
    salvo
}

/// Código do protocolo para um código do motor (correspondência exaustiva:
/// um código novo no motor não compila sem o seu par aqui).
pub fn codigo_ipc(c: CodigoAviso) -> &'static str {
    match c {
        CodigoAviso::EstadoCorrompido => codigo_aviso::ESTADO_CORROMPIDO,
        CodigoAviso::EstadoRecuperado => codigo_aviso::ESTADO_RECUPERADO,
        CodigoAviso::EstadoReiniciado => codigo_aviso::ESTADO_REINICIADO,
        CodigoAviso::SnapshotIndisponivel => codigo_aviso::SNAPSHOT_INDISPONIVEL,
        CodigoAviso::SnapshotOriginalPerdido => codigo_aviso::SNAPSHOT_ORIGINAL_PERDIDO,
    }
}

/// Aviso local com o código e o texto da UI para um código do motor.
pub(super) fn aviso_motor(
    nivel: NivelAviso,
    c: CodigoAviso,
    detalhe: Option<String>,
    agora: u64,
) -> Aviso {
    Aviso::novo(
        nivel,
        codigo_ipc(c),
        textos::texto_aviso_motor(c),
        detalhe,
        agora,
    )
}

/// Marcador `snapshot-perdido.json` (gravado pelo serviço depois de uma
/// quarentena): mesmo aviso do motor, `snapshot_original_perdido`. Erro de
/// leitura vira `estado_aviso`.
pub fn interpretar_marcador_quarentena(marcador: Conteudo, agora: u64) -> Option<Aviso> {
    match marcador {
        Ok(None) => None,
        Ok(Some(s)) => {
            let detalhe = serde_json::from_str::<serde_json::Value>(&s)
                .ok()
                .and_then(|v| v.get("detalhe").and_then(|d| d.as_str()).map(str::to_owned))
                .unwrap_or_else(|| s.trim().to_owned());
            Some(aviso_motor(
                NivelAviso::Atencao,
                CodigoAviso::SnapshotOriginalPerdido,
                Some(format!("snapshot-perdido.json: {detalhe}")),
                agora,
            ))
        }
        Err(e) => Some(Aviso::novo(
            NivelAviso::Atencao,
            codigo_aviso::ESTADO_AVISO,
            textos::LOCAL_ESTADO_ILEGIVEL,
            Some(e),
            agora,
        )),
    }
}

pub(super) fn estado_valido(s: &str) -> Result<nvt_engine::Estado, String> {
    let e: nvt_engine::Estado = serde_json::from_str(s).map_err(|e| e.to_string())?;
    if e.versao != VERSAO_ESTADO {
        return Err(format!(
            "versão {} não suportada (esperada {VERSAO_ESTADO})",
            e.versao
        ));
    }
    Ok(e)
}

/// Mesma decisão do `Armazem::abrir` (estado inválido → cópia de segurança →
/// estado limpo; snapshot ilegível → restauração bloqueada), só que em
/// avisos e sem tocar nos arquivos.
pub fn interpretar_salvo(
    estado: Conteudo,
    anterior: Conteudo,
    snapshot: Conteudo,
    agora: u64,
) -> Salvo {
    let mut r = Salvo::default();
    match estado {
        // Ausente: como o motor, usa a cópia de segurança se for válida
        // (só leitura; nada é renomeado nem gravado).
        Ok(None) => {
            if let Some(Ok(e)) = anterior.ok().flatten().map(|s| estado_valido(&s)) {
                r.estado = Some(e);
                r.avisos.push(Aviso::novo(
                    NivelAviso::Atencao,
                    codigo_ipc(CodigoAviso::EstadoRecuperado),
                    textos::LOCAL_ESTADO_AUSENTE_RECUPERAVEL,
                    Some("estado.json ausente; lido de estado.anterior.json".into()),
                    agora,
                ));
            }
        }
        Ok(Some(s)) => match estado_valido(&s) {
            Ok(e) => r.estado = Some(e),
            Err(motivo) => {
                r.avisos.push(aviso_motor(
                    NivelAviso::Atencao,
                    CodigoAviso::EstadoCorrompido,
                    Some(format!("estado.json: {motivo}")),
                    agora,
                ));
                match anterior.ok().flatten().map(|s| estado_valido(&s)) {
                    Some(Ok(e)) => {
                        r.estado = Some(e);
                        r.avisos.push(aviso_motor(
                            NivelAviso::Info,
                            CodigoAviso::EstadoRecuperado,
                            None,
                            agora,
                        ));
                    }
                    _ => r.avisos.push(aviso_motor(
                        NivelAviso::Atencao,
                        CodigoAviso::EstadoReiniciado,
                        None,
                        agora,
                    )),
                }
            }
        },
        Err(e) => r.avisos.push(Aviso::novo(
            NivelAviso::Atencao,
            codigo_aviso::ESTADO_AVISO,
            textos::LOCAL_ESTADO_ILEGIVEL,
            Some(e),
            agora,
        )),
    }
    match snapshot {
        Ok(None) => {}
        Ok(Some(s)) => match Snapshot::from_json(&s) {
            Ok(snap) => r.snapshot = Some(snap),
            Err(e) => r.avisos.push(Aviso::snapshot_indisponivel(
                format!("snapshot-original.json ilegível: {e}"),
                agora,
            )),
        },
        Err(e) => r.avisos.push(Aviso::novo(
            NivelAviso::Atencao,
            codigo_aviso::ESTADO_AVISO,
            textos::LOCAL_ESTADO_ILEGIVEL,
            Some(e),
            agora,
        )),
    }
    r
}
