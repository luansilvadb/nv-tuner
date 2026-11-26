//! Formato e armazenamento das partidas (§7.4, R-M-06).
//!
//! Onde: no serviço, `%ProgramData%\nv-tuner\partidas\<id-conta>\` ([`pasta_partidas`]),
//! com `<id-conta>` = HMAC por usuário (nenhum SID em claro). A ACL SYSTEM/Administradores
//! da pasta é aplicada pelo serviço ao criá-la; este módulo recusa reparse points.
//!
//! Arquivos: `<id>.bin` por partida e `indice.json` com os resumos. Só agregados.
//!
//! `<id>.bin` (little-endian):
//! - `b"NVTP"`, `versao: u16`, `reservado: u16`;
//! - `tam_resumo: u32` e o `ResumoPartida` em JSON;
//! - `n_amostras: u32` e `n` registros de [`TAMANHO_AMOSTRA`] bytes: `t_rel_s: u32`,
//!   `quadros: u16`, `frametime_medio_us: u32`, `frametime_max_us: u32`, `engasgos: u8`,
//!   `gpu_uso_pct: u8`, `gpu_temp_c: u8` (`0xFF` = ausente).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use nvt_ipc::{AMOSTRAS_MAX, AmostraSegundo, PARTIDAS_MAX, Partida, ResumoPartida};

use crate::nucleo::VERSAO_FORMATO;

pub const MAGICO: &[u8; 4] = b"NVTP";
pub const TAMANHO_AMOSTRA: usize = 17;
/// Retenção: 30 dias ou [`RETENCAO_MAX_PARTIDAS`] por usuário, o que vier primeiro.
pub const RETENCAO_S: u64 = 30 * 86_400;
pub const RETENCAO_MAX_PARTIDAS: usize = PARTIDAS_MAX as usize;
pub const NOME_INDICE: &str = "indice.json";
const RESUMO_MAX: usize = 64 * 1024;
const AUSENTE: u8 = 0xFF;

/// Pasta das partidas de um usuário no serviço. `id_conta` deve ser hexadecimal (HMAC).
pub fn pasta_partidas(program_data: &Path, id_conta: &str) -> Result<PathBuf, String> {
    if id_conta.is_empty()
        || id_conta.len() > 128
        || !id_conta.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("identificador de conta inválido".into());
    }
    Ok(program_data
        .join("nv-tuner")
        .join("partidas")
        .join(id_conta))
}

pub fn codificar(partida: &Partida) -> Result<Vec<u8>, String> {
    partida.validar()?;
    let resumo = serde_json::to_vec(&partida.resumo).map_err(|e| e.to_string())?;
    let mut b = Vec::with_capacity(16 + resumo.len() + partida.amostras.len() * TAMANHO_AMOSTRA);
    b.extend_from_slice(MAGICO);
    b.extend_from_slice(&VERSAO_FORMATO.to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes());
    b.extend_from_slice(&(resumo.len() as u32).to_le_bytes());
    b.extend_from_slice(&resumo);
    b.extend_from_slice(&(partida.amostras.len() as u32).to_le_bytes());
    for a in &partida.amostras {
        b.extend_from_slice(&a.t_rel_s.to_le_bytes());
        b.extend_from_slice(&a.quadros.to_le_bytes());
        b.extend_from_slice(&a.frametime_medio_us.to_le_bytes());
        b.extend_from_slice(&a.frametime_max_us.to_le_bytes());
        b.push(a.engasgos);
        b.push(a.gpu_uso_pct.unwrap_or(AUSENTE));
        b.push(a.gpu_temp_c.unwrap_or(AUSENTE));
    }
    Ok(b)
}

struct Leitor<'a>(&'a [u8]);

impl<'a> Leitor<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.0.len() < n {
            return Err("arquivo de partida truncado".into());
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.bytes(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(
            self.bytes(2)?.try_into().unwrap_or_default(),
        ))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.bytes(4)?.try_into().unwrap_or_default(),
        ))
    }
}

fn opcional(v: u8) -> Option<u8> {
    (v != AUSENTE).then_some(v)
}

pub fn decodificar(dados: &[u8]) -> Result<Partida, String> {
    let mut l = Leitor(dados);
    if l.bytes(4)? != MAGICO {
        return Err("arquivo de partida sem assinatura".into());
    }
    let versao = l.u16()?;
    if versao != VERSAO_FORMATO {
        return Err(format!("versão de formato desconhecida: {versao}"));
    }
    l.u16()?;
    let tam = l.u32()? as usize;
    if tam > RESUMO_MAX {
        return Err("resumo grande demais".into());
    }
    let resumo: ResumoPartida = serde_json::from_slice(l.bytes(tam)?).map_err(|e| e.to_string())?;
    let n = l.u32()? as usize;
    if n > AMOSTRAS_MAX {
        return Err("muitas amostras".into());
    }
    let mut amostras = Vec::with_capacity(n);
    for _ in 0..n {
        amostras.push(AmostraSegundo {
            t_rel_s: l.u32()?,
            quadros: l.u16()?,
            frametime_medio_us: l.u32()?,
            frametime_max_us: l.u32()?,
            engasgos: l.u8()?,
            gpu_uso_pct: opcional(l.u8()?),
            gpu_temp_c: opcional(l.u8()?),
        });
    }
    if !l.0.is_empty() {
        return Err("bytes sobrando no arquivo de partida".into());
    }
    let p = Partida { resumo, amostras };
    p.validar()?;
    Ok(p)
}

/// IDs a apagar: mais antigas que 30 dias e as que passam das 200 mais recentes.
pub fn a_podar(resumos: &[ResumoPartida], agora_unix: u64) -> Vec<u64> {
    let mut ordem: Vec<&ResumoPartida> = resumos.iter().collect();
    ordem.sort_by(|a, b| b.inicio_unix.cmp(&a.inicio_unix).then(b.id.cmp(&a.id)));
    let limite = agora_unix.saturating_sub(RETENCAO_S);
    ordem
        .iter()
        .enumerate()
        .filter(|(i, r)| *i >= RETENCAO_MAX_PARTIDAS || r.inicio_unix < limite)
        .map(|(_, r)| r.id)
        .collect()
}

fn eh_reparse(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || meta.file_type().is_symlink()
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

fn recusar_reparse(caminho: &Path) -> io::Result<()> {
    match fs::symlink_metadata(caminho) {
        Ok(m) if eh_reparse(&m) => Err(io::Error::other("reparse point recusado")),
        _ => Ok(()),
    }
}

/// Gravação atômica: temporário na mesma pasta e rename (`MoveFileExW` com substituição
/// no Windows).
fn gravar_atomico(destino: &Path, dados: &[u8]) -> io::Result<()> {
    recusar_reparse(destino)?;
    let tmp = destino.with_extension("tmp");
    recusar_reparse(&tmp)?;
    fs::write(&tmp, dados)?;
    fs::rename(&tmp, destino)
}

/// Pasta de partidas de um usuário.
#[derive(Debug, Clone)]
pub struct Armazem {
    dir: PathBuf,
}

impl Armazem {
    /// Abre (criando) a pasta; recusa se ela ou um ancestral imediato for reparse point.
    pub fn abrir(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        recusar_reparse(&dir)?;
        if let Some(pai) = dir.parent() {
            recusar_reparse(pai)?;
        }
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn arquivo(&self, id: u64) -> PathBuf {
        self.dir.join(format!("{id}.bin"))
    }

    pub fn listar(&self) -> io::Result<Vec<ResumoPartida>> {
        let caminho = self.dir.join(NOME_INDICE);
        recusar_reparse(&caminho)?;
        match fs::read(&caminho) {
            Ok(b) => serde_json::from_slice(&b).map_err(io::Error::other),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    fn gravar_indice(&self, resumos: &[ResumoPartida]) -> io::Result<()> {
        let b = serde_json::to_vec(resumos).map_err(io::Error::other)?;
        gravar_atomico(&self.dir.join(NOME_INDICE), &b)
    }

    /// Grava a partida e o índice, e poda a retenção.
    pub fn gravar(&self, partida: &Partida, agora_unix: u64) -> io::Result<()> {
        let b = codificar(partida).map_err(io::Error::other)?;
        gravar_atomico(&self.arquivo(partida.resumo.id), &b)?;
        let mut indice = self.listar()?;
        indice.retain(|r| r.id != partida.resumo.id);
        indice.push(partida.resumo.clone());
        self.gravar_indice(&indice)?;
        self.podar(agora_unix)
    }

    pub fn ler(&self, id: u64) -> io::Result<Partida> {
        let caminho = self.arquivo(id);
        recusar_reparse(&caminho)?;
        decodificar(&fs::read(caminho)?).map_err(io::Error::other)
    }

    /// Retenção (ao fim de cada partida e na partida do serviço).
    pub fn podar(&self, agora_unix: u64) -> io::Result<()> {
        let indice = self.listar()?;
        let apagar = a_podar(&indice, agora_unix);
        if apagar.is_empty() {
            return Ok(());
        }
        for id in &apagar {
            self.remover(*id)?;
        }
        let restantes: Vec<ResumoPartida> = indice
            .into_iter()
            .filter(|r| !apagar.contains(&r.id))
            .collect();
        self.gravar_indice(&restantes)
    }

    fn remover(&self, id: u64) -> io::Result<()> {
        let caminho = self.arquivo(id);
        recusar_reparse(&caminho)?;
        match fs::remove_file(caminho) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// "Apagar todas as partidas" (Avançado).
    pub fn apagar_todas(&self) -> io::Result<()> {
        for r in self.listar()? {
            self.remover(r.id)?;
        }
        self.gravar_indice(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvt_ipc::FAIXAS_HISTOGRAMA;

    fn resumo(id: u64, inicio: u64) -> ResumoPartida {
        ResumoPartida {
            id,
            jogo_id: "cs2".into(),
            executavel: "cs2.exe".into(),
            inicio_unix: inicio,
            duracao_s: 600,
            quadros_total: 60_000,
            fps_medio: 100.0,
            fps_mediana: 100.0,
            fps_1pct_low: 80.0,
            fps_01pct_low: 60.0,
            engasgos: 2,
            tempo_engasgado_ms: 120,
            histograma: vec![1; FAIXAS_HISTOGRAMA],
            gpu_temp_max_c: Some(71),
            gpu_uso_medio_pct: None,
            perda_eventos_pct: 0.0,
            encerrada_por_erro: None,
            versao_formato: VERSAO_FORMATO,
        }
    }

    fn partida(id: u64, inicio: u64) -> Partida {
        Partida {
            resumo: resumo(id, inicio),
            amostras: (0..3600)
                .map(|s| AmostraSegundo {
                    t_rel_s: s,
                    quadros: 100,
                    frametime_medio_us: 10_000,
                    frametime_max_us: 12_000,
                    engasgos: 0,
                    gpu_uso_pct: Some(90),
                    gpu_temp_c: None,
                })
                .collect(),
        }
    }

    #[test]
    fn formato_ida_e_volta_e_tamanho_por_hora() {
        let p = partida(1, 1_000);
        let b = codificar(&p).unwrap();
        // Cerca de 75 KB por hora (§7.4): 3600 x 17 bytes + resumo.
        assert!(b.len() < 75 * 1024, "{}", b.len());
        assert_eq!(decodificar(&b).unwrap(), p);
        assert!(decodificar(&b[..b.len() - 1]).is_err());
        let mut ruim = b.clone();
        ruim[4] = 9;
        assert!(decodificar(&ruim).is_err());
    }

    #[test]
    fn formato_nao_tem_frametime_bruto() {
        // Só agregados: o arquivo é cabeçalho + resumo + amostras de tamanho fixo.
        let p = partida(1, 1_000);
        let resumo = serde_json::to_vec(&p.resumo).unwrap();
        let b = codificar(&p).unwrap();
        assert_eq!(b.len(), 16 + resumo.len() + 3600 * TAMANHO_AMOSTRA);
    }

    #[test]
    fn retencao_30_dias_e_200_partidas() {
        let agora = 100 * 86_400;
        let mut r = vec![
            resumo(1, agora - RETENCAO_S - 1),
            resumo(2, agora - RETENCAO_S),
        ];
        assert_eq!(a_podar(&r, agora), vec![1]);
        r = (0..205).map(|i| resumo(i, agora - 1000 + i)).collect();
        let mut podadas = a_podar(&r, agora);
        podadas.sort_unstable();
        assert_eq!(podadas, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn pasta_do_servico_e_validada() {
        let p = pasta_partidas(Path::new("C:\\ProgramData"), "ab12").unwrap();
        assert!(p.ends_with("nv-tuner\\partidas\\ab12") || p.ends_with("nv-tuner/partidas/ab12"));
        assert!(pasta_partidas(Path::new("C:\\ProgramData"), "..\\x").is_err());
        assert!(pasta_partidas(Path::new("C:\\ProgramData"), "").is_err());
    }

    #[test]
    fn armazem_grava_le_poda_e_apaga() {
        let dir = std::env::temp_dir().join(format!("nvt-medicao-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let a = Armazem::abrir(&dir).unwrap();
        let agora = 100 * 86_400;
        a.gravar(&partida(1, agora - RETENCAO_S - 10), agora - RETENCAO_S)
            .unwrap();
        a.gravar(&partida(2, agora - 10), agora).unwrap();
        // A primeira saiu pela retenção ao gravar a segunda.
        let ids: Vec<u64> = a.listar().unwrap().iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![2]);
        assert!(a.ler(1).is_err());
        assert_eq!(a.ler(2).unwrap(), partida(2, agora - 10));
        a.apagar_todas().unwrap();
        assert!(a.listar().unwrap().is_empty());
        assert!(!dir.join("2.bin").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
