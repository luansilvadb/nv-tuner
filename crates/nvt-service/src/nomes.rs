//! Nomes legíveis de settings e valores para a UI e os relatórios.
//!
//! Fonte: `data/settings-referencia.json` (NVIDIA Profile Inspector,
//! MIT), filtrada no `build.rs` para os IDs do catálogo. Na falta de um nome
//! na referência, usa a descrição e os nomes de valores do catálogo
//! (`regras/settings.toml`).

use std::collections::HashMap;
use std::sync::LazyLock;

use nvt_core::DefSetting;
use serde::Deserialize;

#[derive(Deserialize)]
struct NomeRef {
    id: u32,
    nome: String,
    valores: Vec<(u32, String)>,
}

static NOMES: LazyLock<HashMap<u32, NomeRef>> = LazyLock::new(|| {
    let lista: Vec<NomeRef> =
        serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/nomes.json")))
            .unwrap_or_default();
    lista.into_iter().map(|n| (n.id, n)).collect()
});

/// Nome legível da setting (referência do NPI ou descrição do catálogo).
pub fn nome_setting(def: &DefSetting) -> String {
    NOMES
        .get(&def.id)
        .map(|n| n.nome.clone())
        .unwrap_or_else(|| def.descricao.clone())
}

/// Valor legível: rótulo do NPI, senão nome do catálogo, senão o número.
/// `None` = não definido.
pub fn valor(def: Option<&DefSetting>, v: Option<u32>) -> Option<String> {
    let v = v?;
    let rotulo = def.and_then(|d| {
        NOMES
            .get(&d.id)
            .and_then(|n| n.valores.iter().find(|(x, _)| *x == v))
            .map(|(_, r)| limpar(r))
            .or_else(|| d.nome_do_valor(v).map(str::to_owned))
    });
    Some(rotulo.unwrap_or_else(|| format!("{v} (0x{v:X})")))
}

/// Remove marcadores do NPI como `(${DlssVersion})` dos rótulos.
fn limpar(r: &str) -> String {
    let mut s = r.to_owned();
    while let Some(i) = s.find("${") {
        let fim = s[i..].find('}').map_or(s.len(), |j| i + j + 1);
        s.replace_range(i..fim, "");
    }
    s.replace(" ()", "").replace("()", "").trim().to_owned()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn nomes_da_referencia() {
        let regras = nvt_core::ConjuntoRegras::embutido().unwrap();
        let preset = regras.catalogo.get("dlss_preset").unwrap();
        assert_eq!(nome_setting(preset), "DLSS - Forced Preset Letter");
        assert_eq!(
            valor(Some(preset), Some(5)).as_deref(),
            Some("Preset E (CNN)")
        );
        assert_eq!(valor(Some(preset), None), None);
        assert_eq!(limpar("On - latest (${DlssVersion})"), "On - latest");
        assert_eq!(valor(None, Some(7)).as_deref(), Some("7 (0x7)"));
    }
}
