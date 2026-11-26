//! T-01 (docs/spec/competitivo.md §3 e §5): nenhum crate de produção chama as APIs
//! proibidas pelas regras anticheat N1/N2.
//!
//! O teste percorre `crates/*/src/**/*.rs` e procura cada termo proibido como palavra
//! inteira (fronteira de identificador: `OpenProcess` não casa com `OpenProcessToken`).
//!
//! Fora da busca:
//! - diretórios `tests/`, `examples/` e `benches/`, e arquivos `build.rs`;
//! - comentários `//`, `///`, `//!` e `/* ... */` (texto dentro de literais de string conta);
//! - o item que segue um atributo `#[cfg(test)]` (tipicamente `mod tests { ... }`).
//!
//! Limitações do detector (é um lexer simples, não um parser de Rust):
//! - o item sob `#[cfg(test)]` é delimitado contando chaves `{`/`}` fora de strings e
//!   comentários, a partir da primeira `{`; se aparecer `;` antes de qualquer `{`
//!   (ex.: `#[cfg(test)] mod tests;` ou `#[cfg(test)] use x;`), o item termina ali.
//!   Só o atributo exato `#[cfg(test)]` é reconhecido; `#[cfg(all(test, ...))]` não é;
//! - um módulo de teste em arquivo separado (`mod tests;`) só é excluído se morar em um
//!   diretório `tests/`;
//! - macros que geram código não são expandidas; byte strings `b"..."` são tratadas como
//!   strings comuns;
//! - termos quebrados em várias linhas ou com espaços extras (`Command :: new`) não casam.

use std::fs;
use std::path::{Path, PathBuf};

/// Termos proibidos em código de produção (N1/N2).
const PROIBIDOS: &[&str] = &[
    "OpenProcess",
    "TH32CS_SNAPMODULE",
    "TH32CS_SNAPMODULE32",
    "ReadProcessMemory",
    "WriteProcessMemory",
    "SetWindowsHookEx",
    "SetWindowsHookExW",
    "SetWindowsHookExA",
    "CreateRemoteThread",
    "SetPriorityClass",
    "SetProcessAffinityMask",
    "Image File Execution Options",
    "Command::new",
];

const DIRETORIOS_EXCLUIDOS: &[&str] = &["tests", "examples", "benches"];

#[derive(Debug, PartialEq, Eq)]
struct Ocorrencia {
    linha: usize,
    termo: &'static str,
}

/// Uma linha já processada pelo lexer.
struct Linha {
    /// Código sem comentários, com o conteúdo das strings preservado (usado na busca).
    busca: String,
    /// Código sem comentários e sem conteúdo de strings/chars (usado para contar chaves).
    estrutura: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Estado {
    Codigo,
    Str,
    /// String bruta `r"..."`/`r#"..."#`, com o número de `#`.
    StrBruta(usize),
    ComentarioBloco(u32),
}

/// Remove comentários e separa o texto de strings, mantendo o estado entre linhas.
fn lexer(fonte: &str) -> Vec<Linha> {
    let mut estado = Estado::Codigo;
    let mut linhas = Vec::new();
    for texto in fonte.lines() {
        let c: Vec<char> = texto.chars().collect();
        let mut busca = String::new();
        let mut estrutura = String::new();
        let mut i = 0;
        while i < c.len() {
            match estado {
                Estado::ComentarioBloco(n) => {
                    if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                        estado = if n == 1 {
                            Estado::Codigo
                        } else {
                            Estado::ComentarioBloco(n - 1)
                        };
                        i += 2;
                    } else if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                        estado = Estado::ComentarioBloco(n + 1);
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                Estado::Str => {
                    if c[i] == '\\' {
                        busca.push(c[i]);
                        if let Some(&p) = c.get(i + 1) {
                            busca.push(p);
                        }
                        i += 2;
                    } else if c[i] == '"' {
                        busca.push('"');
                        estrutura.push('"');
                        estado = Estado::Codigo;
                        i += 1;
                    } else {
                        busca.push(c[i]);
                        i += 1;
                    }
                }
                Estado::StrBruta(n) => {
                    let fecha = c[i] == '"' && (1..=n).all(|k| c.get(i + k) == Some(&'#'));
                    if fecha {
                        busca.push('"');
                        estrutura.push('"');
                        estado = Estado::Codigo;
                        i += 1 + n;
                    } else {
                        busca.push(c[i]);
                        i += 1;
                    }
                }
                Estado::Codigo => {
                    if let Some((prefixo, n)) = inicio_str_bruta(&c, i) {
                        busca.push('"');
                        estrutura.push('"');
                        estado = Estado::StrBruta(n);
                        i += prefixo;
                    } else if c[i] == '/' && c.get(i + 1) == Some(&'/') {
                        break;
                    } else if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                        estado = Estado::ComentarioBloco(1);
                        busca.push(' ');
                        estrutura.push(' ');
                        i += 2;
                    } else if c[i] == '"' {
                        busca.push('"');
                        estrutura.push('"');
                        estado = Estado::Str;
                        i += 1;
                    } else if c[i] == '\'' {
                        // Literal de char ('x', '\n', '\u{..}') ou lifetime ('a).
                        if c.get(i + 1) == Some(&'\\') {
                            let fim = (i + 2..c.len()).find(|&j| c[j] == '\'');
                            let fim = fim.map_or(c.len(), |j| j + 1);
                            busca.push_str("' '");
                            estrutura.push_str("' '");
                            i = fim;
                        } else if c.get(i + 2) == Some(&'\'') {
                            busca.push_str("' '");
                            estrutura.push_str("' '");
                            i += 3;
                        } else {
                            busca.push('\'');
                            estrutura.push('\'');
                            i += 1;
                        }
                    } else {
                        busca.push(c[i]);
                        estrutura.push(c[i]);
                        i += 1;
                    }
                }
            }
        }
        linhas.push(Linha { busca, estrutura });
    }
    linhas
}

/// Se em `c[i]` começa uma string bruta (`r"`, `r#"`, `br"`...), devolve
/// (tamanho do prefixo até a aspa inclusive, número de `#`).
fn inicio_str_bruta(c: &[char], i: usize) -> Option<(usize, usize)> {
    if i > 0 && eh_ident(c[i - 1]) {
        return None;
    }
    let mut j = i;
    if c.get(j) == Some(&'b') {
        j += 1;
    }
    if c.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let mut n = 0;
    while c.get(j) == Some(&'#') {
        n += 1;
        j += 1;
    }
    (c.get(j) == Some(&'"')).then_some((j + 1 - i, n))
}

/// Marca as linhas pertencentes a itens sob `#[cfg(test)]`.
fn linhas_de_teste(linhas: &[Linha]) -> Vec<bool> {
    let mut excluida = vec![false; linhas.len()];
    let mut i = 0;
    while i < linhas.len() {
        let Some(pos) = linhas[i].estrutura.find("#[cfg(test)]") else {
            i += 1;
            continue;
        };
        // Percorre a partir do fim do atributo até fechar o item.
        let mut profundidade: i64 = 0;
        let mut abriu = false;
        let mut j = i;
        let mut inicio = pos + "#[cfg(test)]".len();
        'item: while j < linhas.len() {
            excluida[j] = true;
            for ch in linhas[j].estrutura[inicio..].chars() {
                match ch {
                    '{' => {
                        profundidade += 1;
                        abriu = true;
                    }
                    '}' => {
                        profundidade -= 1;
                        if abriu && profundidade == 0 {
                            break 'item;
                        }
                    }
                    ';' if !abriu => break 'item,
                    _ => {}
                }
            }
            j += 1;
            inicio = 0;
        }
        i = j + 1;
    }
    excluida
}

fn eh_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Procura `termo` como palavra inteira em `texto`.
fn contem_palavra(texto: &str, termo: &str) -> bool {
    let mut desde = 0;
    while let Some(rel) = texto[desde..].find(termo) {
        let ini = desde + rel;
        let fim = ini + termo.len();
        let antes_ok = texto[..ini]
            .chars()
            .next_back()
            .is_none_or(|c| !eh_ident(c));
        let depois_ok = texto[fim..].chars().next().is_none_or(|c| !eh_ident(c));
        if antes_ok && depois_ok {
            return true;
        }
        desde = ini + termo.chars().next().map_or(1, char::len_utf8);
    }
    false
}

/// Analisa um arquivo-fonte e devolve as ocorrências proibidas (linhas a partir de 1).
fn analisar(fonte: &str) -> Vec<Ocorrencia> {
    let linhas = lexer(fonte);
    let excluida = linhas_de_teste(&linhas);
    let mut achados = Vec::new();
    for (n, linha) in linhas.iter().enumerate() {
        if excluida[n] {
            continue;
        }
        for &termo in PROIBIDOS {
            if contem_palavra(&linha.busca, termo) {
                achados.push(Ocorrencia {
                    linha: n + 1,
                    termo,
                });
            }
        }
    }
    achados
}

fn coletar_rs(dir: &Path, saida: &mut Vec<PathBuf>) {
    let Ok(entradas) = fs::read_dir(dir) else {
        return;
    };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        let nome = entrada.file_name();
        let nome = nome.to_string_lossy();
        if caminho.is_dir() {
            if !DIRETORIOS_EXCLUIDOS.contains(&nome.as_ref()) {
                coletar_rs(&caminho, saida);
            }
        } else if caminho.extension().is_some_and(|e| e == "rs") && nome != "build.rs" {
            saida.push(caminho);
        }
    }
}

fn raiz_workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("CARGO_MANIFEST_DIR deve estar em <workspace>/crates/nvt-core")
        .to_path_buf()
}

#[test]
fn nenhuma_api_proibida_em_codigo_de_producao() {
    let raiz = raiz_workspace();
    let crates = raiz.join("crates");
    let mut arquivos = Vec::new();
    let mut dirs: Vec<PathBuf> = fs::read_dir(&crates)
        .expect("diretório crates/ do workspace")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in &dirs {
        coletar_rs(&dir.join("src"), &mut arquivos);
    }
    arquivos.sort();
    assert!(
        !arquivos.is_empty(),
        "nenhum .rs encontrado em {}",
        crates.display()
    );

    let mut relatorio = Vec::new();
    for arquivo in &arquivos {
        let fonte = fs::read_to_string(arquivo)
            .unwrap_or_else(|e| panic!("ler {}: {e}", arquivo.display()));
        let relativo = arquivo.strip_prefix(&raiz).unwrap_or(arquivo);
        for oc in analisar(&fonte) {
            relatorio.push(format!(
                "  {}:{}: `{}`",
                relativo.display().to_string().replace('\\', "/"),
                oc.linha,
                oc.termo
            ));
        }
    }
    assert!(
        relatorio.is_empty(),
        "T-01 (docs/spec/competitivo.md §3, N1/N2): {} ocorrência(s) de API proibida em \
         código de produção:\n{}\n",
        relatorio.len(),
        relatorio.join("\n")
    );
}

// Testes do próprio detector.

fn termos(fonte: &str) -> Vec<(usize, &'static str)> {
    analisar(fonte)
        .into_iter()
        .map(|o| (o.linha, o.termo))
        .collect()
}

#[test]
fn casa_open_process() {
    let fonte = "fn f() {\n    let h = unsafe { OpenProcess(PROCESS_VM_READ, false, pid) };\n}\n";
    assert_eq!(termos(fonte), vec![(2, "OpenProcess")]);
}

#[test]
fn nao_casa_open_process_token() {
    let fonte = "unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut t) };\n";
    assert!(termos(fonte).is_empty());
    assert!(!contem_palavra("MyOpenProcess()", "OpenProcess"));
    assert!(!contem_palavra("open_OpenProcess", "OpenProcess"));
}

#[test]
fn casa_caminho_qualificado_e_string() {
    let fonte = concat!(
        "use windows::Win32::System::Threading::OpenProcess;\n",
        "let c = std::process::Command::new(\"x\");\n",
        "let k = r\"SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options\";\n",
        "let s = TH32CS_SNAPMODULE32 | TH32CS_SNAPMODULE;\n",
        r###"let p = r"\\?\"; let q = r#"a " b"#; SetPriorityClass();"###,
        "\n",
    );
    assert_eq!(
        termos(fonte),
        vec![
            (1, "OpenProcess"),
            (2, "Command::new"),
            (3, "Image File Execution Options"),
            (4, "TH32CS_SNAPMODULE"),
            (4, "TH32CS_SNAPMODULE32"),
            (5, "SetPriorityClass"),
        ]
    );
}

#[test]
fn ignora_comentarios() {
    let fonte = concat!(
        "// OpenProcess é proibido\n",
        "/// Não usa ReadProcessMemory.\n",
        "//! Nem Command::new.\n",
        "let x = 1; // SetPriorityClass aqui também não\n",
        "/* CreateRemoteThread\n   SetWindowsHookExW */ let y = 2;\n",
        "let url = \"http://exemplo\"; // OpenProcess\n",
    );
    assert!(termos(fonte).is_empty(), "{:?}", termos(fonte));
}

#[test]
fn ignora_bloco_cfg_test() {
    let fonte = concat!(
        "fn producao() {}\n",
        "\n",
        "#[cfg(test)]\n",
        "mod tests {\n",
        "    use std::process::Command;\n",
        "    #[test]\n",
        "    fn t() {\n",
        "        let s = \"}\"; let c = '}';\n",
        "        Command::new(\"cmd\").status().unwrap();\n",
        "    }\n",
        "}\n",
        "fn depois() { OpenProcess(); }\n",
    );
    assert_eq!(termos(fonte), vec![(12, "OpenProcess")]);
}

#[test]
fn cfg_test_em_item_sem_corpo() {
    let fonte = concat!(
        "#[cfg(test)]\n",
        "mod tests;\n",
        "fn f() { Command::new(\"x\"); }\n",
    );
    assert_eq!(termos(fonte), vec![(3, "Command::new")]);
}
