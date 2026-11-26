//! R-UI-08 (docs/spec/interface-e-saude.md §4.1): o redesenho não pode perder função da
//! UI v0.2.0.
//!
//! Forma verificável: cada `Requisicao` que a UI v0.2.0 envia ao serviço (levantamento do
//! L1 em `crates/nvt-ui/src`) precisa continuar aparecendo como `Requisicao::<Variante>`
//! no código de produção de `crates/nvt-ui/src` (fora de comentários e de itens sob
//! `#[cfg(test)]`). A lista só cresce: retirar uma variante exige mudar a spec.
//!
//! O teste não prova que o ponto de chamada está ligado a um botão; isso fica com os testes
//! de modelo e com o Lab-UI. Ele impede o caso comum do redesenho: apagar a tela antiga e,
//! com ela, o único envio de uma requisição. Requisições novas (v8) não entram aqui.
//!
//! Variantes que a UI v0.2.0 não envia (não fazem parte do inventário): `TravarSetting`,
//! `AgenteConsultar`, `AgenteRelatar`.

use std::fs;
use std::path::{Path, PathBuf};

/// (variante, o que a UI v0.2.0 oferece com ela).
const INVENTARIO: &[(&str, &str)] = &[
    (
        "ObterEstado",
        "estado do serviço, sistema, capacidades, conformidade",
    ),
    (
        "ObterPlano",
        "mudanças por perfil, deriva, jogos, sugestões de driver",
    ),
    ("ListarHistorico", "histórico técnico"),
    ("AplicarAgora", "aplicar agora / voltar a ajustar"),
    ("RestaurarOriginal", "restaurar original / desfazer tudo"),
    ("ReescanearJogos", "reescanear jogos"),
    ("DefinirPreferencias", "preferências técnicas"),
    ("DestravarSetting", "destravar ajuste"),
    (
        "DefinirAceiteExperimental",
        "aceitar e retirar experimentais",
    ),
    ("ReverterAjusteUsuario", "Windows: reverter o que o app fez"),
    ("ReverterConfigJogo", "arquivo do jogo: reversão por chave"),
    (
        "MarcarAtualizacaoVista",
        "verificar atualização / aviso visto",
    ),
];

fn raiz_workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("CARGO_MANIFEST_DIR deve estar em <workspace>/crates/nvt-core")
        .to_path_buf()
}

fn coletar_rs(dir: &Path, saida: &mut Vec<PathBuf>) {
    let Ok(entradas) = fs::read_dir(dir) else {
        return;
    };
    for e in entradas.flatten() {
        let p = e.path();
        if p.is_dir() {
            if e.file_name() != "tests" {
                coletar_rs(&p, saida);
            }
        } else if p.extension().is_some_and(|x| x == "rs") {
            saida.push(p);
        }
    }
}

/// Remove comentários (`//` e `/* */`) e itens sob `#[cfg(test)]`. Lexer simples: strings
/// comuns são respeitadas (um `//` dentro de `"..."` não corta a linha); strings brutas não.
fn codigo_de_producao(fonte: &str) -> String {
    let c: Vec<char> = fonte.chars().collect();
    let mut sem_coment = String::new();
    let (mut i, mut em_str, mut bloco) = (0, false, 0u32);
    while i < c.len() {
        let prox = c.get(i + 1).copied();
        if bloco > 0 {
            if c[i] == '*' && prox == Some('/') {
                bloco -= 1;
                i += 2;
            } else if c[i] == '/' && prox == Some('*') {
                bloco += 1;
                i += 2;
            } else {
                if c[i] == '\n' {
                    sem_coment.push('\n');
                }
                i += 1;
            }
        } else if em_str {
            sem_coment.push(c[i]);
            if c[i] == '\\' {
                if let Some(p) = prox {
                    sem_coment.push(p);
                }
                i += 2;
                continue;
            }
            if c[i] == '"' {
                em_str = false;
            }
            i += 1;
        } else if c[i] == '/' && prox == Some('/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if c[i] == '/' && prox == Some('*') {
            bloco = 1;
            i += 2;
        } else {
            if c[i] == '"' {
                em_str = true;
            }
            sem_coment.push(c[i]);
            i += 1;
        }
    }
    // Retira cada item sob `#[cfg(test)]`: até a `}` que fecha a primeira `{`, ou até `;`.
    let mut saida = String::new();
    let mut resto = sem_coment.as_str();
    while let Some(pos) = resto.find("#[cfg(test)]") {
        saida.push_str(&resto[..pos]);
        let depois = &resto[pos + "#[cfg(test)]".len()..];
        let mut prof = 0i64;
        let mut fim = depois.len();
        for (k, ch) in depois.char_indices() {
            match ch {
                '{' => prof += 1,
                '}' => {
                    prof -= 1;
                    if prof == 0 {
                        fim = k + 1;
                        break;
                    }
                }
                ';' if prof == 0 => {
                    fim = k + 1;
                    break;
                }
                _ => {}
            }
        }
        resto = &depois[fim..];
    }
    saida.push_str(resto);
    saida
}

fn eh_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `Requisicao::<variante>` como palavra inteira (aceita `nvt_ipc::Requisicao::...`).
fn envia(codigo: &str, variante: &str) -> bool {
    let alvo = format!("Requisicao::{variante}");
    codigo.match_indices(&alvo).any(|(ini, _)| {
        let antes = codigo[..ini].chars().next_back();
        let depois = codigo[ini + alvo.len()..].chars().next();
        antes.is_none_or(|c| !eh_ident(c)) && depois.is_none_or(|c| !eh_ident(c))
    })
}

#[test]
fn ui_continua_enviando_cada_requisicao_da_v0_2_0() {
    let src = raiz_workspace().join("crates/nvt-ui/src");
    let mut arquivos = Vec::new();
    coletar_rs(&src, &mut arquivos);
    assert!(!arquivos.is_empty(), "nenhum .rs em {}", src.display());
    let codigo: String = arquivos
        .iter()
        .map(|a| {
            let f = fs::read_to_string(a).unwrap_or_else(|e| panic!("ler {}: {e}", a.display()));
            codigo_de_producao(&f)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let faltando: Vec<String> = INVENTARIO
        .iter()
        .filter(|(v, _)| !envia(&codigo, v))
        .map(|(v, o)| format!("  Requisicao::{v} ({o})"))
        .collect();
    assert!(
        faltando.is_empty(),
        "R-UI-08: a UI deixou de enviar {} requisição(ões) da v0.2.0:\n{}\n",
        faltando.len(),
        faltando.join("\n")
    );
}

#[test]
fn detector_ignora_comentario_e_teste() {
    let f = concat!(
        "// Requisicao::AplicarAgora\n",
        "/* Requisicao::ObterPlano */\n",
        "fn f() { let s = \"// x\"; nvt_ipc::Requisicao::ObterEstado; }\n",
        "#[cfg(test)]\n",
        "mod tests { fn t() { Requisicao::RestaurarOriginal; } }\n",
        "fn g() { Requisicao::ReescanearJogosX; }\n",
    );
    let c = codigo_de_producao(f);
    assert!(envia(&c, "ObterEstado"));
    assert!(!envia(&c, "AplicarAgora"));
    assert!(!envia(&c, "ObterPlano"));
    assert!(!envia(&c, "RestaurarOriginal"));
    assert!(!envia(&c, "ReescanearJogos"));
}
