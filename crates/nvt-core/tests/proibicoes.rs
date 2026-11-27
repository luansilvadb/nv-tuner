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
    "CreateProcessW",
    "CreateProcessA",
    "CreateProcessAsUserW",
    "CreateProcessAsUserA",
    "CreateProcessWithTokenW",
    "CreateProcessWithLogonW",
    "ShellExecuteW",
    "ShellExecuteA",
    "ShellExecuteExW",
    "ShellExecuteExA",
    "WinExec",
    // §8.3 (docs/spec/interface-e-saude.md): sessão ETW só no módulo de medição.
    "StartTraceW",
    "EnableTraceEx2",
    "OpenTraceW",
    "ProcessTrace",
    "ControlTraceW",
    // §8.3: logger de kernel e esgotamento de sessões, proibidos em todo lugar.
    "SystemTraceControlGuid",
    "KERNEL_LOGGER_NAME",
    "EtwMaxLoggers",
    // §8.3: exclusão de arquivo pela API Win32 só nos módulos autorizados. Levantamento
    // do L1: nenhum uso em código de produção (as exclusões atuais usam `std::fs`).
    "DeleteFileW",
    "RemoveDirectoryW",
    "FileDispositionInfoEx",
];

/// Exceções exatas (docs/spec/atualizacao.md §9.6, U-29): (arquivo relativo à raiz,
/// termo, linhas com ocorrência). Fora desta lista, qualquer ocorrência falha; no arquivo
/// listado, contagem diferente (a mais ou a menos) falha quando o arquivo existe.
const PERMITIDOS: &[(&str, &str, usize)] = &[
    (
        "crates/nvt-service/src/atualizacao/lancador.rs",
        "CreateProcessW",
        1,
    ),
    (
        "crates/nvt-service/src/atualizacao/lancador.rs",
        "CreateProcessAsUserW",
        1,
    ),
    // D3 (docs/spec/interface-e-saude.md): abrir `ms-settings:startupapps` (L4), numa
    // única chamada; a constante literal é conferida em `abrir_usa_so_a_constante...`.
    ("crates/nvt-ui/src/abrir.rs", "ShellExecuteW", 1),
    // §8.3: sessão ETW da medição, uma linha por termo (contagens fixadas no L5).
    ("crates/nvt-medicao/src/etw.rs", "StartTraceW", 1),
    ("crates/nvt-medicao/src/etw.rs", "EnableTraceEx2", 1),
    ("crates/nvt-medicao/src/etw.rs", "OpenTraceW", 1),
    ("crates/nvt-medicao/src/etw.rs", "ProcessTrace", 1),
    ("crates/nvt-medicao/src/etw.rs", "ControlTraceW", 1),
    // §8.3: exclusão da limpeza de temporários (L3): só `FileDispositionInfoEx` em
    // `ExcluidorReal`; `DeleteFileW` e `RemoveDirectoryW` seguem proibidos.
    ("crates/nvt-manutencao/src/windows.rs", "DeleteFileW", 0),
    (
        "crates/nvt-manutencao/src/windows.rs",
        "RemoveDirectoryW",
        0,
    ),
    (
        "crates/nvt-manutencao/src/windows.rs",
        "FileDispositionInfoEx",
        1,
    ),
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

/// Confronta as ocorrências com `permitidos` e devolve as violações (vazio = ok).
/// `existentes` lista os arquivos analisados (caminhos relativos com `/`).
fn conferir(
    achados: &[(String, Ocorrencia)],
    existentes: &[String],
    permitidos: &[(&str, &str, usize)],
) -> Vec<String> {
    let mut relatorio = Vec::new();
    for (arquivo, oc) in achados {
        let permitido = permitidos
            .iter()
            .any(|&(a, t, _)| a == arquivo && t == oc.termo);
        if !permitido {
            relatorio.push(format!("  {arquivo}:{}: `{}`", oc.linha, oc.termo));
        }
    }
    for &(arquivo, termo, esperado) in permitidos {
        if !existentes.iter().any(|e| e == arquivo) {
            continue;
        }
        let linhas: Vec<String> = achados
            .iter()
            .filter(|(a, oc)| a == arquivo && oc.termo == termo)
            .map(|(_, oc)| format!("{arquivo}:{}", oc.linha))
            .collect();
        if linhas.len() != esperado {
            relatorio.push(format!(
                "  {arquivo}: `{termo}` permitido {esperado} vez(es), encontrado {} [{}]",
                linhas.len(),
                linhas.join(", ")
            ));
        }
    }
    relatorio
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

    let mut achados = Vec::new();
    let mut existentes = Vec::new();
    for arquivo in &arquivos {
        let fonte = fs::read_to_string(arquivo)
            .unwrap_or_else(|e| panic!("ler {}: {e}", arquivo.display()))
            .replace("\r\n", "\n");
        let relativo = arquivo.strip_prefix(&raiz).unwrap_or(arquivo);
        let relativo = relativo.display().to_string().replace('\\', "/");
        for oc in analisar(&fonte) {
            achados.push((relativo.clone(), oc));
        }
        existentes.push(relativo);
    }
    let relatorio = conferir(&achados, &existentes, PERMITIDOS);
    assert!(
        relatorio.is_empty(),
        "T-01 (docs/spec/competitivo.md §3, N1/N2): {} ocorrência(s) de API proibida em \
         código de produção:\n{}\n",
        relatorio.len(),
        relatorio.join("\n")
    );
}

/// §9.6: o lançador monta o msiexec pela pasta de sistema, nunca pelo PATH.
#[test]
fn lancador_usa_msiexec_da_pasta_de_sistema() {
    let caminho = raiz_workspace().join("crates/nvt-service/src/atualizacao/lancador.rs");
    let fonte = fs::read_to_string(&caminho)
        .expect("ler lancador.rs")
        .replace("\r\n", "\n");
    let codigo: String = lexer(&fonte).into_iter().map(|l| l.busca + "\n").collect();
    assert!(
        codigo.contains("const MSIEXEC: &str = \"msiexec.exe\""),
        "lancador.rs deve conter `MSIEXEC: &str = \"msiexec.exe\"`"
    );
    assert!(
        contem_palavra(&codigo, "GetSystemDirectoryW"),
        "lancador.rs deve montar o caminho por GetSystemDirectoryW"
    );
}

/// D3: o único URI aberto por `abrir.rs` é a constante literal
/// `ms-settings:startupapps` (nenhum argumento vindo de dados).
#[test]
fn abrir_usa_so_a_constante_de_inicializacao() {
    let caminho = raiz_workspace().join("crates/nvt-ui/src/abrir.rs");
    let fonte = fs::read_to_string(&caminho)
        .expect("abrir.rs existe desde o L4")
        .replace("\r\n", "\n");
    conferir_abrir(&fonte).unwrap_or_else(|e| panic!("abrir.rs: {e}"));
}

fn conferir_abrir(fonte: &str) -> Result<(), String> {
    let linhas = lexer(fonte);
    let excluida = linhas_de_teste(&linhas);
    let codigo: String = linhas
        .into_iter()
        .zip(excluida)
        .filter(|(_, ex)| !ex)
        .map(|(l, _)| l.busca + "\n")
        .collect();
    let literal = "\"ms-settings:startupapps\"";
    if codigo.matches(literal).count() != 1 {
        return Err(format!("{literal} deve aparecer exatamente uma vez"));
    }
    if codigo.matches("ms-settings:").count() != 1 {
        return Err("nenhum outro URI `ms-settings:` é permitido".into());
    }
    let declarada = codigo.lines().any(|l| {
        let l = l.trim_start();
        let l = l.strip_prefix("pub(crate) ").unwrap_or(l);
        let l = l.strip_prefix("pub ").unwrap_or(l);
        l.starts_with("const ") && l.contains(literal)
    });
    if !declarada {
        return Err(format!("{literal} deve ser uma constante (`const`)"));
    }
    Ok(())
}

#[test]
fn conferencia_de_abrir() {
    let ok = "const URI: &str = \"ms-settings:startupapps\";\nfn f() { ShellExecuteW(URI); }\n";
    assert!(conferir_abrir(ok).is_ok());
    let dois = concat!(
        "const URI: &str = \"ms-settings:startupapps\";\n",
        "const B: &str = \"ms-settings:storagesense\";\n",
    );
    assert!(conferir_abrir(dois).is_err());
    let sem_const = "fn f() { ShellExecuteW(\"ms-settings:startupapps\"); }\n";
    assert!(conferir_abrir(sem_const).is_err());
    let comentado = "// \"ms-settings:startupapps\"\nfn f(u: &str) { ShellExecuteW(u); }\n";
    assert!(conferir_abrir(comentado).is_err());
}

#[test]
fn termos_de_etw_e_exclusao_sao_vigiados() {
    let fonte = concat!(
        "StartTraceW(a); ProcessTraceX();\n",
        "let g = SystemTraceControlGuid;\n",
        "DeleteFileW(p); RemoveDirectoryW(q);\n",
    );
    assert_eq!(
        termos(fonte),
        vec![
            (1, "StartTraceW"),
            (2, "SystemTraceControlGuid"),
            (3, "DeleteFileW"),
            (3, "RemoveDirectoryW"),
        ]
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

fn oc(arquivo: &str, linha: usize, termo: &'static str) -> (String, Ocorrencia) {
    (arquivo.to_string(), Ocorrencia { linha, termo })
}

const LANC: &str = "crates/x/src/lancador.rs";
const PERM_TESTE: &[(&str, &str, usize)] = &[(LANC, "CreateProcessW", 1)];

#[test]
fn permitido_com_contagem_exata_passa() {
    let achados = vec![oc(LANC, 10, "CreateProcessW")];
    assert!(conferir(&achados, &[LANC.to_string()], PERM_TESTE).is_empty());
}

#[test]
fn permitido_com_contagem_a_mais_falha() {
    let achados = vec![
        oc(LANC, 10, "CreateProcessW"),
        oc(LANC, 20, "CreateProcessW"),
    ];
    let r = conferir(&achados, &[LANC.to_string()], PERM_TESTE);
    assert_eq!(r.len(), 1, "{r:?}");
    assert!(r[0].contains(&format!("{LANC}:20")), "{r:?}");
}

#[test]
fn permitido_com_contagem_a_menos_falha_se_arquivo_existe() {
    let r = conferir(&[], &[LANC.to_string()], PERM_TESTE);
    assert_eq!(r.len(), 1, "{r:?}");
    assert!(r[0].contains("encontrado 0"), "{r:?}");
    // Arquivo ainda inexistente: não cobra a contagem.
    assert!(conferir(&[], &[], PERM_TESTE).is_empty());
}

#[test]
fn permitido_em_arquivo_errado_falha() {
    let outro = "crates/x/src/outro.rs";
    let achados = vec![
        oc(LANC, 10, "CreateProcessW"),
        oc(outro, 5, "CreateProcessW"),
    ];
    let r = conferir(&achados, &[LANC.to_string(), outro.to_string()], PERM_TESTE);
    assert_eq!(r, vec![format!("  {outro}:5: `CreateProcessW`")]);
}

#[test]
fn termo_nao_permitido_no_arquivo_permitido_falha() {
    let achados = vec![
        oc(LANC, 10, "CreateProcessW"),
        oc(LANC, 11, "ShellExecuteW"),
    ];
    let r = conferir(&achados, &[LANC.to_string()], PERM_TESTE);
    assert_eq!(r, vec![format!("  {LANC}:11: `ShellExecuteW`")]);
}

#[test]
fn termos_de_processo_sao_palavra_inteira() {
    let fonte = concat!(
        "CreateProcessWithTokenW(a);\n",
        "CreateProcessAsUserW(b);\n",
        "ShellExecuteExW(&mut i);\n",
        "MyCreateProcessW(); WinExecX();\n",
        "CreateProcessW(c);\n",
    );
    assert_eq!(
        termos(fonte),
        vec![
            (1, "CreateProcessWithTokenW"),
            (2, "CreateProcessAsUserW"),
            (3, "ShellExecuteExW"),
            (5, "CreateProcessW"),
        ]
    );
}
