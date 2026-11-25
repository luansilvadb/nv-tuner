// Recurso Win32 comum aos executáveis (ícone ID 1 + VERSIONINFO), incluído
// pelos build.rs de nvt-ui e nvt-service com `include!`. Requer a
// build-dependency `embed-resource`.

/// Gera o .rc em OUT_DIR e o vincula aos binários do pacote (só no Windows).
fn embutir_recurso_win(descricao: &str, nome_exe: &str) {
    use std::path::PathBuf;
    use std::{env, fs};

    let assets = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../assets");
    let ico = assets.join("nv-tuner.ico");
    println!("cargo:rerun-if-changed={}", ico.display());
    println!(
        "cargo:rerun-if-changed={}",
        assets.join("recurso-win.rs").display()
    );
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let num = |v: &str| env::var(v).unwrap().parse::<u16>().unwrap_or(0);
    let (ma, mi, pa) = (
        num("CARGO_PKG_VERSION_MAJOR"),
        num("CARGO_PKG_VERSION_MINOR"),
        num("CARGO_PKG_VERSION_PATCH"),
    );
    let versao = env::var("CARGO_PKG_VERSION").unwrap();
    let caminho_ico = ico
        .canonicalize()
        .expect("assets/nv-tuner.ico")
        .display()
        .to_string()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    let esc = |s: &str| s.replace('"', "\"\"");

    let rc = format!(
        r#"#pragma code_page(65001)
1 ICON "{caminho_ico}"
1 VERSIONINFO
FILEVERSION {ma},{mi},{pa},0
PRODUCTVERSION {ma},{mi},{pa},0
FILEFLAGSMASK 0x3F
FILEFLAGS 0x0
FILEOS 0x40004
FILETYPE 0x1
FILESUBTYPE 0x0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "041604B0"
    BEGIN
      VALUE "CompanyName", "nv-tuner contributors"
      VALUE "FileDescription", "{descricao}"
      VALUE "FileVersion", "{versao}"
      VALUE "InternalName", "{nome_exe}"
      VALUE "LegalCopyright", "Licença MIT"
      VALUE "OriginalFilename", "{nome_exe}.exe"
      VALUE "ProductName", "nv-tuner"
      VALUE "ProductVersion", "{versao}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0416, 1200
  END
END
"#,
        descricao = esc(descricao),
    );
    let saida = PathBuf::from(env::var("OUT_DIR").unwrap()).join("nv-tuner.rc");
    fs::write(&saida, rc).unwrap();
    embed_resource::compile(&saida, embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
