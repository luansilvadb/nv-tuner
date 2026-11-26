//! Testes de linguagem dos textos: R-UI-03, R-UI-04 e R-UI-14, e cobertura
//! do mapeamento modelo -> texto.

use std::collections::BTreeSet;

use super::{avancado, boas_vindas, estado, inicio, jogos};
use crate::modelo::{LinhaEstado, MotivoIndisponivel, StatusJogo, TipoSinal, Veredito};

/// Um caso de cada variante de [`Veredito`] (e de cada sinal em "piorou").
fn casos_veredito() -> Vec<Veredito> {
    let mut v = vec![
        Veredito::Verificando,
        Veredito::Indisponivel(MotivoIndisponivel::ProtocoloIncompativel),
        Veredito::Indisponivel(MotivoIndisponivel::ServicoAusente),
        Veredito::SemPlacaNvidia,
        Veredito::AjustesDesligados,
        Veredito::EsperandoJogo,
        Veredito::Melhorias { quantidade: 3 },
        Veredito::Otimizado,
        Veredito::NadaAFazer,
    ];
    v.extend(
        [
            TipoSinal::EspacoLivre,
            TipoSinal::MemoriaRepouso,
            TipoSinal::CpuRepouso,
            TipoSinal::DriverDesatualizado,
            TipoSinal::PlanoEnergia,
            TipoSinal::SensorArmazenamento,
        ]
        .map(Veredito::Piorou),
    );
    v
}

/// Índice da variante; o `match` exaustivo quebra a compilação quando o
/// modelo ganha uma variante, obrigando a incluí-la nos casos.
fn variante_veredito(v: &Veredito) -> usize {
    match v {
        Veredito::Verificando => 0,
        Veredito::Indisponivel(_) => 1,
        Veredito::SemPlacaNvidia => 2,
        Veredito::AjustesDesligados => 3,
        Veredito::EsperandoJogo => 4,
        Veredito::Piorou(_) => 5,
        Veredito::Melhorias { .. } => 6,
        Veredito::Otimizado => 7,
        Veredito::NadaAFazer => 8,
    }
}
const VARIANTES_VEREDITO: usize = 9;

fn casos_linha(jogo: Option<&str>) -> Vec<LinhaEstado> {
    vec![
        LinhaEstado::ProtocoloIncompativel,
        LinhaEstado::ServicoAusente,
        LinhaEstado::AtualizacaoConcluida,
        LinhaEstado::JogoAberto {
            nome: jogo.map(str::to_owned),
        },
        LinhaEstado::AtualizacaoPendente,
        LinhaEstado::AgenteAusente,
    ]
}

fn variante_linha(l: &LinhaEstado) -> usize {
    match l {
        LinhaEstado::ProtocoloIncompativel => 0,
        LinhaEstado::ServicoAusente => 1,
        LinhaEstado::AtualizacaoConcluida => 2,
        LinhaEstado::JogoAberto { .. } => 3,
        LinhaEstado::AtualizacaoPendente => 4,
        LinhaEstado::AgenteAusente => 5,
    }
}
const VARIANTES_LINHA: usize = 6;

fn casos_status(ajustes: usize) -> Vec<StatusJogo> {
    vec![
        StatusJogo::AguardandoFechar,
        StatusJogo::FacaAjustes {
            quantidade: ajustes,
        },
        StatusJogo::Otimizado,
        StatusJogo::NadaAAjustar,
    ]
}

fn variante_status(s: StatusJogo) -> usize {
    match s {
        StatusJogo::AguardandoFechar => 0,
        StatusJogo::FacaAjustes { .. } => 1,
        StatusJogo::Otimizado => 2,
        StatusJogo::NadaAAjustar => 3,
    }
}
const VARIANTES_STATUS: usize = 4;

/// Siglas proibidas na tela principal (R-UI-03).
const SIGLAS: &[&str] = &[
    "GPU", "VRAM", "DLSS", "VRR", "G-SYNC", "FPS", "BAR", "DRS", "HAGS", "MPO", "SID", "IPC",
    "ETW", "MSI", "CPU", "RAM", "SSD", "ReBAR", "NVAPI", "API",
];

/// Jargão do modelo interno, também fora da tela principal.
const JARGAO: &[&str] = &[
    "deriva",
    "travada",
    "agente",
    "experimental",
    "services.msc",
    "serviço",
];

/// Maior frase aceita na tela principal, em caracteres.
const MAX_FRASE: usize = 80;

/// Fontes de `textos/`, para a varredura de emoji.
const FONTES: &[(&str, &str)] = &[
    ("mod.rs", include_str!("mod.rs")),
    ("inicio.rs", include_str!("inicio.rs")),
    ("jogos.rs", include_str!("jogos.rs")),
    ("boas_vindas.rs", include_str!("boas_vindas.rs")),
    ("estado.rs", include_str!("estado.rs")),
    ("avancado.rs", include_str!("avancado.rs")),
];

/// Todos os textos da tela principal, fixos e gerados.
fn tela_principal() -> Vec<String> {
    let mut v: Vec<String> = inicio::TODOS
        .iter()
        .chain(jogos::TODOS)
        .chain(boas_vindas::TODOS)
        .map(|s| (*s).to_owned())
        .collect();
    for c in casos_veredito() {
        let t = inicio::veredito(&c);
        v.push(t.titulo);
        v.push(t.frase);
    }
    for c in casos_linha(Some("Counter-Strike 2")) {
        v.push(estado::texto(&c));
    }
    for c in casos_status(2) {
        v.push(jogos::status(c));
    }
    for n in [0, 1, 2, 12] {
        v.push(inicio::melhorias_frase(n));
        v.push(inicio::aplicadas(n));
        v.push(inicio::inicializacao_novos(n));
        v.push(jogos::encontrados(n));
        v.push(jogos::faca_ajustes(n.max(1)));
    }
    v.push(inicio::piorou_titulo(inicio::MOTIVO_POUCO_ESPACO));
    v.push(inicio::espaco_livre(12_000_000_000, true));
    v.push(inicio::espaco_livre(210_000_000_000, false));
    v.push(estado::jogo_aberto(""));
    v.push(jogos::passo(1, "No menu Vídeo, ligue \"Baixa latência\"."));
    v
}

/// Palavras inteiras do texto (letras, dígitos e hífen).
fn palavras(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '.'))
        .map(|p| p.trim_matches('.'))
        .filter(|p| !p.is_empty())
}

#[test]
fn r_ui_03_sem_siglas_na_tela_principal() {
    for t in tela_principal() {
        for p in palavras(&t) {
            assert!(!SIGLAS.contains(&p), "sigla {p:?} em {t:?}");
        }
        let minusculo = t.to_lowercase();
        for j in JARGAO {
            assert!(!minusculo.contains(j), "jargão {j:?} em {t:?}");
        }
    }
}

#[test]
fn r_ui_03_avancado_explica_siglas() {
    // Toda sigla do Avançado vem acompanhada de explicação na mesma frase.
    for (sigla, extenso) in [
        ("GPU", "Placa de vídeo"),
        ("VRAM", "memória de vídeo"),
        ("VRR", "sincronismo adaptável"),
        ("FPS", "quadros por segundo"),
        ("ReBAR", "Resizable BAR"),
    ] {
        let achou = avancado::TODOS
            .iter()
            .find(|t| palavras(t).any(|p| p == sigla))
            .unwrap_or_else(|| panic!("sigla {sigla} sem texto no Avançado"));
        assert!(
            achou.contains(extenso),
            "{sigla} sem {extenso:?}: {achou:?}"
        );
    }
}

fn eh_emoji(c: char) -> bool {
    let u = u32::from(c);
    u > 0xFFFF || (0x2600..=0x27BF).contains(&u)
}

#[test]
fn r_ui_04_sem_emoji_em_textos() {
    for (nome, fonte) in FONTES {
        for (i, linha) in fonte.lines().enumerate() {
            if let Some(c) = linha.chars().find(|c| eh_emoji(*c)) {
                panic!("{nome}:{}: caractere U+{:04X}", i + 1, u32::from(c));
            }
        }
    }
    for t in tela_principal() {
        assert!(!t.chars().any(eh_emoji), "emoji em {t:?}");
    }
}

#[test]
fn r_ui_14_visualizacao_sem_services_msc() {
    let v = inicio::veredito(&Veredito::Indisponivel(MotivoIndisponivel::ServicoAusente));
    assert_eq!(v.titulo, "O nv-tuner não está ativo neste PC");
    assert_eq!(
        v.frase,
        "Reinstale o nv-tuner para que ele volte a cuidar do PC."
    );
    let t = estado::SERVICO_AUSENTE;
    assert!(!t.contains("services.msc"), "{t:?}");
    assert!(!t.contains('\n'), "aviso de várias linhas: {t:?}");
    assert!(t.chars().count() <= MAX_FRASE, "aviso longo: {t:?}");
    for (nome, fonte) in FONTES {
        // A própria lista de jargão deste arquivo não entra na varredura.
        assert!(!fonte.contains("services.msc"), "services.msc em {nome}");
    }
}

#[test]
fn todo_enum_do_modelo_tem_texto() {
    let vereditos = casos_veredito();
    let cobertos: BTreeSet<_> = vereditos.iter().map(variante_veredito).collect();
    assert_eq!(cobertos, (0..VARIANTES_VEREDITO).collect());
    for c in &vereditos {
        let t = inicio::veredito(c);
        assert!(
            !t.titulo.trim().is_empty() && !t.frase.trim().is_empty(),
            "{c:?}"
        );
    }
    let linhas = casos_linha(None);
    let cobertas: BTreeSet<_> = linhas.iter().map(variante_linha).collect();
    assert_eq!(cobertas, (0..VARIANTES_LINHA).collect());
    for c in &linhas {
        assert!(!estado::texto(c).trim().is_empty(), "{c:?}");
    }
    let status = casos_status(0);
    let cobertos: BTreeSet<_> = status.iter().copied().map(variante_status).collect();
    assert_eq!(cobertos, (0..VARIANTES_STATUS).collect());
    for c in status {
        assert!(!jogos::status(c).trim().is_empty(), "{c:?}");
    }
    assert_eq!(
        estado::texto(&LinhaEstado::JogoAberto {
            nome: Some("Valorant".into())
        }),
        "Valorant está aberto. As mudanças esperam o jogo fechar."
    );
    assert_eq!(
        inicio::veredito(&Veredito::Piorou(TipoSinal::EspacoLivre)).titulo,
        "Algo piorou: pouco espaço livre"
    );
}

#[test]
fn frases_curtas_na_tela_principal() {
    for t in tela_principal() {
        for frase in t.split(". ") {
            let n = frase.chars().count();
            assert!(n <= MAX_FRASE, "frase com {n} caracteres: {frase:?}");
        }
    }
    for t in avancado::TODOS {
        assert!(t.chars().count() <= 100, "rótulo longo no Avançado: {t:?}");
    }
}

#[test]
fn plurais() {
    assert_eq!(
        inicio::aplicadas(1),
        "Aplicamos 1 melhoria. Para voltar atrás, use Desfazer tudo."
    );
    assert_eq!(
        inicio::aplicadas(3),
        "Aplicamos 3 melhorias. Para voltar atrás, use Desfazer tudo."
    );
    assert_eq!(
        jogos::status(StatusJogo::FacaAjustes { quantidade: 2 }),
        "Faça 2 ajustes no jogo"
    );
    assert_eq!(jogos::encontrados(4), "Encontramos 4 jogos neste PC.");
    assert_eq!(
        inicio::espaco_livre(12_400_000_000, true),
        "Restam 12 GB no disco do Windows."
    );
}
