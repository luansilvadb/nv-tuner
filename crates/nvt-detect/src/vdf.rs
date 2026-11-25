//! Leitor mínimo de KeyValues (VDF/ACF) da Steam, em formato texto.

/// Nó de um documento VDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Vdf {
    Texto(String),
    Objeto(Vec<(String, Vdf)>),
}

impl Vdf {
    /// Procura uma chave (sem diferenciar maiúsculas) em um objeto.
    pub fn get(&self, chave: &str) -> Option<&Vdf> {
        match self {
            Vdf::Objeto(itens) => itens
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(chave))
                .map(|(_, v)| v),
            Vdf::Texto(_) => None,
        }
    }

    pub fn texto(&self) -> Option<&str> {
        match self {
            Vdf::Texto(s) => Some(s),
            Vdf::Objeto(_) => None,
        }
    }

    pub fn get_texto(&self, chave: &str) -> Option<&str> {
        self.get(chave).and_then(Vdf::texto)
    }

    /// Pares filhos de um objeto (vazio para texto).
    pub fn itens(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Objeto(i) => i,
            Vdf::Texto(_) => &[],
        }
    }
}

#[derive(Debug, PartialEq)]
enum Token {
    Str(String),
    Abre,
    Fecha,
}

fn tokens(src: &str) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let mut it = src.chars().peekable();
    while let Some(&c) = it.peek() {
        match c {
            c if c.is_whitespace() => {
                it.next();
            }
            '{' => {
                it.next();
                out.push(Token::Abre);
            }
            '}' => {
                it.next();
                out.push(Token::Fecha);
            }
            '/' => {
                it.next();
                if it.peek() == Some(&'/') {
                    for c in it.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                } else {
                    return Err("'/' solto".into());
                }
            }
            '"' => {
                it.next();
                let mut s = String::new();
                loop {
                    match it.next() {
                        None => return Err("string sem fechamento".into()),
                        Some('"') => break,
                        Some('\\') => match it.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(o) => s.push(o),
                            None => return Err("escape no fim do arquivo".into()),
                        },
                        Some(o) => s.push(o),
                    }
                }
                out.push(Token::Str(s));
            }
            _ => {
                // Token sem aspas (raro): até espaço ou chave.
                let mut s = String::new();
                while let Some(&c) = it.peek() {
                    if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                        break;
                    }
                    s.push(c);
                    it.next();
                }
                out.push(Token::Str(s));
            }
        }
    }
    Ok(out)
}

fn objeto(toks: &[Token], pos: &mut usize, dentro: bool) -> Result<Vec<(String, Vdf)>, String> {
    let mut itens = Vec::new();
    while *pos < toks.len() {
        match &toks[*pos] {
            Token::Fecha => {
                if !dentro {
                    return Err("'}' sem abertura".into());
                }
                *pos += 1;
                return Ok(itens);
            }
            Token::Abre => return Err("'{' sem chave".into()),
            Token::Str(k) => {
                *pos += 1;
                let v = match toks.get(*pos) {
                    Some(Token::Str(v)) => {
                        *pos += 1;
                        // Condicionais da Steam ([$WIN32]) são ignorados.
                        if let Some(Token::Str(cond)) = toks.get(*pos) {
                            if cond.starts_with('[') {
                                *pos += 1;
                            }
                        }
                        Vdf::Texto(v.clone())
                    }
                    Some(Token::Abre) => {
                        *pos += 1;
                        Vdf::Objeto(objeto(toks, pos, true)?)
                    }
                    _ => return Err(format!("valor ausente para '{k}'")),
                };
                itens.push((k.clone(), v));
            }
        }
    }
    if dentro {
        Err("objeto sem fechamento".into())
    } else {
        Ok(itens)
    }
}

/// Converte o texto VDF em um objeto raiz.
pub fn parse(src: &str) -> Result<Vdf, String> {
    let toks = tokens(src.trim_start_matches('\u{feff}'))?;
    let mut pos = 0;
    Ok(Vdf::Objeto(objeto(&toks, &mut pos, false)?))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn basico() {
        let v = parse(
            "// comentário\n\"A\"\n{\n\t\"path\"\t\t\"C:\\\\Jogos\"\n\t\"apps\" { \"1\" \"2\" }\n}\n",
        )
        .unwrap();
        let a = v.get("a").unwrap();
        assert_eq!(a.get_texto("PATH"), Some("C:\\Jogos"));
        assert_eq!(a.get("apps").unwrap().itens().len(), 1);
    }

    #[test]
    fn erro_sem_fechamento() {
        assert!(parse("\"A\" { \"b\" \"c\"").is_err());
    }
}
