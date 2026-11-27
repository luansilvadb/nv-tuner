//! Preferências da UI em `ui.json` (marca de boas-vindas vista).

use super::*;

// ---------------------------------------------------------------------------
// Preferências da UI (spec interface-e-saude.md §7.5, R-UI-09)
// ---------------------------------------------------------------------------

/// Arquivo de preferências da UI, dentro da pasta local do usuário.
pub const ARQUIVO_UI: &str = "ui.json";
/// Chave da marca de boas-vindas vista.
pub(super) const CHAVE_BOAS_VINDAS: &str = "boas_vindas_vista";

/// `%LOCALAPPDATA%\nv-tuner`, se a variável existir.
pub fn pasta_ui() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(std::path::PathBuf::from(base).join(nvt_core::produto::PASTA_DADOS))
}

/// As boas-vindas já foram vistas? Qualquer falha de leitura conta como
/// "não vistas": no máximo a tela aparece de novo.
pub fn boas_vindas_vista_em(pasta: &Path) -> bool {
    std::fs::read_to_string(pasta.join(ARQUIVO_UI))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| {
            v.get(CHAVE_BOAS_VINDAS)
                .and_then(serde_json::Value::as_bool)
        })
        .unwrap_or(false)
}

/// Grava a marca de boas-vindas vista, preservando outras chaves. Grava num
/// arquivo temporário e renomeia, para nunca deixar um `ui.json` pela metade.
pub fn marcar_boas_vindas_em(pasta: &Path) -> Result<(), String> {
    std::fs::create_dir_all(pasta).map_err(|e| e.to_string())?;
    let caminho = pasta.join(ARQUIVO_UI);
    let mut valor = std::fs::read_to_string(&caminho)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    valor[CHAVE_BOAS_VINDAS] = serde_json::Value::Bool(true);
    let texto = serde_json::to_string_pretty(&valor).map_err(|e| e.to_string())?;
    let temporario = pasta.join(format!("{ARQUIVO_UI}.tmp"));
    std::fs::write(&temporario, texto).map_err(|e| e.to_string())?;
    std::fs::rename(&temporario, &caminho).map_err(|e| e.to_string())
}

/// [`boas_vindas_vista_em`] na pasta padrão; sem pasta, "não vistas".
pub fn boas_vindas_vista() -> bool {
    pasta_ui().is_some_and(|p| boas_vindas_vista_em(&p))
}

/// [`marcar_boas_vindas_em`] na pasta padrão.
pub fn marcar_boas_vindas() -> Result<(), String> {
    let pasta = pasta_ui().ok_or_else(|| "LOCALAPPDATA ausente".to_owned())?;
    marcar_boas_vindas_em(&pasta)
}

#[cfg(test)]
mod testes_preferencias_ui {
    use super::*;

    fn pasta_temporaria(nome: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "nvt-ui-teste-{nome}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn sem_arquivo_nao_vista_e_marca_persiste() {
        let p = pasta_temporaria("marca");
        assert!(!boas_vindas_vista_em(&p));
        marcar_boas_vindas_em(&p).unwrap();
        assert!(boas_vindas_vista_em(&p));
        let texto = std::fs::read_to_string(p.join(ARQUIVO_UI)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&texto).unwrap();
        assert_eq!(v, serde_json::json!({ "boas_vindas_vista": true }));
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn arquivo_corrompido_nao_bloqueia() {
        let p = pasta_temporaria("corrompido");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join(ARQUIVO_UI), "{ isto não é json").unwrap();
        assert!(!boas_vindas_vista_em(&p));
        marcar_boas_vindas_em(&p).unwrap();
        assert!(boas_vindas_vista_em(&p));
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn preserva_outras_chaves_e_tipo_errado_conta_como_nao_vista() {
        let p = pasta_temporaria("chaves");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(
            p.join(ARQUIVO_UI),
            r#"{ "outra": 1, "boas_vindas_vista": "sim" }"#,
        )
        .unwrap();
        assert!(!boas_vindas_vista_em(&p));
        marcar_boas_vindas_em(&p).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(p.join(ARQUIVO_UI)).unwrap()).unwrap();
        assert_eq!(v["outra"], 1);
        assert_eq!(v["boas_vindas_vista"], true);
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn falha_ao_gravar_devolve_erro_sem_panico() {
        let p = pasta_temporaria("arquivo-no-lugar");
        // Um arquivo onde deveria estar a pasta: a gravação falha.
        std::fs::write(&p, "x").unwrap();
        assert!(marcar_boas_vindas_em(&p).is_err());
        assert!(!boas_vindas_vista_em(&p));
        let _ = std::fs::remove_file(&p);
    }
}
