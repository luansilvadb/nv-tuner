//! Erros e validação do catálogo de configuração de jogo, com a carga a partir da base.

use super::*;

/// Erro de validação de `[[arquivo]]`/`[[regra]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroConfigJogo {
    /// Jogo fora da base.
    JogoDesconhecido { item: String, jogo: String },
    /// `[[arquivo]]` ou regra em jogo com `orientar`/`proibido` (9.5.4).
    JogoSemEdicao { item: String, jogo: String },
    /// Caminho absoluto, com `..`, na pasta de instalação (N8) ou com nome proibido (§9.1).
    CaminhoProibido { arquivo: String, motivo: String },
    /// Setting em arquivo/jogo diferente do da lista fechada, repetida ou ausente.
    ChaveForaDoCatalogo { item: String, setting: String },
    /// Definição de chave/arquivo incoerente (formato, tipo, faixa, processos).
    DefinicaoInvalida { item: String, mensagem: String },
    /// Regra que grava chave `a_confirmar`.
    ChaveAConfirmar { regra: String, setting: SettingJogo },
    /// Valor fixo fora do menu (F3-02).
    ValorForaDoMenu {
        regra: String,
        setting: SettingJogo,
        valor: i64,
    },
    /// Regra incoerente (status, impacto, condições, ação).
    RegraInvalida { regra: String, mensagem: String },
}

impl fmt::Display for ErroConfigJogo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JogoDesconhecido { item, jogo } => {
                write!(f, "{item}: jogo \"{jogo}\" fora da base")
            }
            Self::JogoSemEdicao { item, jogo } => write!(
                f,
                "{item}: jogo \"{jogo}\" só aceita orientar ou proibido; nada gravável"
            ),
            Self::CaminhoProibido { arquivo, motivo } => {
                write!(f, "arquivo \"{arquivo}\": caminho proibido ({motivo})")
            }
            Self::ChaveForaDoCatalogo { item, setting } => {
                write!(f, "{item}: chave \"{setting}\" fora do catálogo")
            }
            Self::DefinicaoInvalida { item, mensagem } => write!(f, "{item}: {mensagem}"),
            Self::ChaveAConfirmar { regra, setting } => write!(
                f,
                "regra \"{regra}\": chave \"{setting}\" a confirmar não pode ser gravada"
            ),
            Self::ValorForaDoMenu {
                regra,
                setting,
                valor,
            } => write!(
                f,
                "regra \"{regra}\": valor {valor} fora do menu de \"{setting}\""
            ),
            Self::RegraInvalida { regra, mensagem } => write!(f, "regra \"{regra}\": {mensagem}"),
        }
    }
}

fn validar_caminho(caminho: &str) -> Result<(), String> {
    if caminho.trim().is_empty() {
        return Err("vazio".into());
    }
    if caminho.contains('/') {
        return Err("use \\ como separador".into());
    }
    if caminho.starts_with('\\') || caminho.contains(':') {
        return Err("caminho absoluto".into());
    }
    let partes: Vec<&str> = caminho.split('\\').collect();
    for parte in &partes {
        let minuscula = parte.to_lowercase();
        if parte.is_empty() || *parte == "." || *parte == ".." {
            return Err(format!("componente \"{parte}\" inválido"));
        }
        if PASTAS_DE_INSTALACAO.contains(&minuscula.as_str()) {
            return Err(format!("pasta de instalação (\"{parte}\")"));
        }
        let mut resto = minuscula.as_str();
        while let Some(i) = resto.find('<') {
            let fim = resto[i..].find('>').ok_or("marcador sem \">\"")? + i;
            if !MARCADORES.contains(&&resto[i..=fim]) {
                return Err(format!("marcador \"{}\" desconhecido", &resto[i..=fim]));
            }
            resto = &resto[fim + 1..];
        }
        if resto.contains('>') {
            return Err("marcador sem \"<\"".into());
        }
    }
    let nome = partes.last().map(|p| p.to_lowercase()).unwrap_or_default();
    if ARQUIVOS_PROIBIDOS.contains(&nome.as_str()) {
        return Err(format!("arquivo \"{nome}\" fora do escopo"));
    }
    Ok(())
}

fn validar_chave(arq: &DefArquivoJogo, c: &DefChaveJogo, erros: &mut Vec<ErroConfigJogo>) {
    let item = format!("arquivo \"{}\"", arq.id);
    let mut def = |m: &str| {
        erros.push(ErroConfigJogo::DefinicaoInvalida {
            item: format!("{item}, chave \"{}\"", c.setting),
            mensagem: m.to_owned(),
        });
    };
    match arq.formato {
        FormatoArquivo::Ini => {
            if c.secao.as_deref().is_none_or(|s| s.trim().is_empty()) {
                def("INI exige `secao`");
            }
            if c.token.is_some() {
                def("INI não usa `token`");
            }
        }
        FormatoArquivo::Vdf | FormatoArquivo::Lancamento => {
            if c.secao.is_some() {
                def("VDF não usa `secao`");
            }
        }
    }
    let lancamento = arq.formato == FormatoArquivo::Lancamento;
    if lancamento != (c.tipo == TipoValorJogo::TokenLancamento) {
        def("`token_lancamento` é o tipo exclusivo do formato `lancamento`");
    }
    if lancamento != (c.token.as_deref() == Some(TOKEN_FPS_MAX)) {
        def("formato `lancamento` exige token = \"+fps_max\" (único token gravável)");
    }
    if let Some(f) = c.faixa
        && f.min > f.max
    {
        def("faixa com min > max");
    }
    if c.valores.windows(2).any(|w| w[0] >= w[1]) {
        def("`valores` deve estar em ordem crescente e sem repetição");
    }
    if c.arredondar == Arredondamento::Baixo && c.valores.is_empty() && !c.a_confirmar {
        def("arredondar = \"baixo\" exige `valores`");
    }
    let numerico = matches!(
        c.tipo,
        TipoValorJogo::Inteiro | TipoValorJogo::Decimal | TipoValorJogo::TokenLancamento
    );
    if !numerico && (!c.valores.is_empty() || c.faixa.is_some()) {
        def("`valores`/`faixa` só para tipos numéricos");
    }
    if !c.a_confirmar {
        if c.chave.trim().is_empty() {
            def("chave vazia (só permitido com a_confirmar)");
        }
        if numerico && c.valores.is_empty() && c.faixa.is_none() {
            def("chave numérica exige `valores` ou `faixa` iguais ao menu (F3-02)");
        }
        if c.tipo == TipoValorJogo::Texto {
            def("tipo texto só com a_confirmar nesta fase");
        }
    }
}

fn checar_jogo<'a>(
    jogos: &'a [JogoCurado],
    item: String,
    id: &IdJogo,
    erros: &mut Vec<ErroConfigJogo>,
) -> Option<&'a JogoCurado> {
    match jogos.iter().find(|j| j.id == *id) {
        None => {
            erros.push(ErroConfigJogo::JogoDesconhecido {
                item,
                jogo: id.to_string(),
            });
            None
        }
        Some(j) if !j.config.grava() || JOGOS_SEM_EDICAO.contains(&id.as_str()) => {
            erros.push(ErroConfigJogo::JogoSemEdicao {
                item,
                jogo: id.to_string(),
            });
            None
        }
        Some(j) => Some(j),
    }
}

/// Valida `[[arquivo]]` e `[[regra]]` contra a base de jogos. `completo`
/// exige todas as chaves de [`SettingJogo`] (schema 2).
pub(crate) fn carregar(
    jogos: &[JogoCurado],
    arquivos: Vec<DefArquivoJogo>,
    regras: Vec<RegraJogo>,
    completo: bool,
    erros: &mut Vec<ErroConfigJogo>,
) -> CatalogoConfigJogo {
    let mut ids = BTreeSet::new();
    let mut vistas = BTreeSet::new();
    for arq in &arquivos {
        let item = format!("arquivo \"{}\"", arq.id);
        if !id_valido(&arq.id) || !ids.insert(arq.id.clone()) {
            erros.push(ErroConfigJogo::DefinicaoInvalida {
                item: item.clone(),
                mensagem: "id inválido ou repetido".into(),
            });
        }
        if let Err(motivo) = validar_caminho(&arq.caminho) {
            erros.push(ErroConfigJogo::CaminhoProibido {
                arquivo: arq.id.clone(),
                motivo,
            });
        }
        let mut def = |m: &str| {
            erros.push(ErroConfigJogo::DefinicaoInvalida {
                item: item.clone(),
                mensagem: m.to_owned(),
            });
        };
        if arq.evidencia.trim().is_empty() {
            def("evidencia vazia");
        }
        if arq.chave.is_empty() {
            def("sem chaves");
        }
        if arq.raiz == RaizJogo::SteamUserdata && !arq.exige_steam_fechada {
            def("arquivo da Steam exige exige_steam_fechada = true (F3-03)");
        }
        if arq.formato == FormatoArquivo::Lancamento && arq.raiz != RaizJogo::SteamUserdata {
            def("opções de inicialização só em localconfig.vdf da Steam");
        }
        if let Some(j) = checar_jogo(jogos, item.clone(), &arq.jogo, erros) {
            if j.config == PoliticaConfig::OpcoesInicializacao
                && arq.formato != FormatoArquivo::Lancamento
            {
                erros.push(ErroConfigJogo::DefinicaoInvalida {
                    item: item.clone(),
                    mensagem: "jogo com opcoes_inicializacao só grava formato lancamento".into(),
                });
            }
            let fechados: BTreeSet<_> = arq
                .processos_fechados
                .iter()
                .map(|p| p.to_lowercase())
                .collect();
            if j.executaveis
                .iter()
                .any(|e| !fechados.contains(&e.to_lowercase()))
            {
                erros.push(ErroConfigJogo::DefinicaoInvalida {
                    item: item.clone(),
                    mensagem: "processos_fechados deve conter os executáveis do jogo".into(),
                });
            }
        }
        for c in &arq.chave {
            if c.setting.arquivo() != arq.id
                || c.setting.jogo() != arq.jogo.as_str()
                || !vistas.insert(c.setting)
            {
                erros.push(ErroConfigJogo::ChaveForaDoCatalogo {
                    item: item.clone(),
                    setting: c.setting.to_string(),
                });
            }
            validar_chave(arq, c, erros);
        }
    }
    if completo {
        for s in SettingJogo::TODAS {
            if !vistas.contains(&s) {
                erros.push(ErroConfigJogo::ChaveForaDoCatalogo {
                    item: "catálogo".into(),
                    setting: format!("{s} (ausente)"),
                });
            }
        }
    }

    let catalogo = CatalogoConfigJogo {
        arquivos,
        regras: Vec::new(),
    };
    let mut ids_regra = BTreeSet::new();
    for r in &regras {
        validar_regra(r, jogos, &catalogo, &mut ids_regra, erros);
    }
    CatalogoConfigJogo { regras, ..catalogo }
}

fn validar_regra(
    r: &RegraJogo,
    jogos_base: &[JogoCurado],
    catalogo: &CatalogoConfigJogo,
    ids: &mut BTreeSet<String>,
    erros: &mut Vec<ErroConfigJogo>,
) {
    let inv = |m: &str, erros: &mut Vec<ErroConfigJogo>| {
        erros.push(ErroConfigJogo::RegraInvalida {
            regra: r.id.clone(),
            mensagem: m.to_owned(),
        });
    };
    if !id_valido(&r.id) || !r.id.starts_with("jogo-") || !ids.insert(r.id.clone()) {
        inv("id inválido, repetido ou sem prefixo \"jogo-\"", erros);
    }
    if r.status != Status::Experimental {
        inv(
            "toda regra de config de jogo é experimental nesta fase (D9)",
            erros,
        );
    }
    if r.impacto_visual != ImpactoVisual::Nenhum {
        inv("impacto_visual deve ser \"nenhum\"", erros);
    }
    for (campo, vazio) in [
        ("descricao", r.descricao.trim().is_empty()),
        ("motivo", r.motivo.trim().is_empty()),
        ("evidencia", r.evidencia.trim().is_empty()),
        ("aplicar", r.aplicar.is_empty()),
        ("quando.jogo", r.quando.jogo.is_empty()),
    ] {
        if vazio {
            inv(&format!("campo `{campo}` vazio"), erros);
        }
    }
    let mut jogos = BTreeSet::new();
    for j in &r.quando.jogo {
        if !jogos.insert(j.as_str()) {
            inv("jogo repetido em quando.jogo", erros);
        }
        checar_jogo(jogos_base, format!("regra \"{}\"", r.id), j, erros);
    }
    let q = &r.quando;
    for (&setting, acao) in &r.aplicar {
        if !jogos.contains(setting.jogo()) {
            inv(
                &format!("\"{setting}\" é de jogo fora de quando.jogo"),
                erros,
            );
        }
        let Some(def) = catalogo.chave(setting) else {
            erros.push(ErroConfigJogo::ChaveForaDoCatalogo {
                item: format!("regra \"{}\"", r.id),
                setting: setting.to_string(),
            });
            continue;
        };
        if def.a_confirmar {
            erros.push(ErroConfigJogo::ChaveAConfirmar {
                regra: r.id.clone(),
                setting,
            });
            continue;
        }
        let numerico = matches!(
            def.tipo,
            TipoValorJogo::Inteiro | TipoValorJogo::Decimal | TipoValorJogo::TokenLancamento
        );
        match acao {
            AcaoJogo::Preferencia(PreferenciaJogo::LimiteFps) => {
                if !numerico || q.limite_fps_definido != Some(true) {
                    inv(
                        "preferencia limite_fps exige chave numérica e limite_fps_definido = true",
                        erros,
                    );
                }
            }
            AcaoJogo::RefreshLimitadoAoMenu => {
                if setting.limite_padrao().is_none()
                    || q.limite_fps_definido != Some(false)
                    || q.refresh_conhecido != Some(true)
                    || def.faixa.is_none()
                {
                    inv(
                        "refresh_limitado_ao_menu só em dota2_fps_max, com limite_fps_definido = false e refresh_conhecido = true (F3-13)",
                        erros,
                    );
                }
            }
            AcaoJogo::Booleano(_) => {
                if def.tipo != TipoValorJogo::Booleano {
                    inv(&format!("\"{setting}\" não é booleana"), erros);
                }
            }
            AcaoJogo::Inteiro(n) => {
                if !numerico || def.ajustar(*n) != Some(*n) {
                    erros.push(ErroConfigJogo::ValorForaDoMenu {
                        regra: r.id.clone(),
                        setting,
                        valor: *n,
                    });
                }
            }
        }
        if r.limite_no_jogo && !matches!(acao, AcaoJogo::Preferencia(_)) {
            inv(
                "limite_no_jogo exige ações { preferencia = \"limite_fps\" }",
                erros,
            );
        }
    }
}
