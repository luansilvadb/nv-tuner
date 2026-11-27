//! Textos da tela Partidas (build de teste, spec
//! `interface-e-saude.md` §5.6 e §7.4).

pub const NAV: &str = "Partidas";
pub const BUILD_DE_TESTE: &str = "Build de teste";

pub const VAZIO: &str =
    "Nenhuma partida gravada ainda. Jogue por pelo menos um minuto e ela aparece aqui.";
pub const RECURSO_AUSENTE: &str =
    "A gravação de partidas não está disponível nesta versão do serviço.";
pub const SEM_SERVICO: &str = "As partidas aparecem com o nv-tuner ativo.";
pub const FALHA: &str = "Não foi possível ler as partidas agora.";
pub const CARREGANDO_DETALHE: &str = "Carregando a partida…";
pub const SEM_AMOSTRAS: &str = "Sem dados por segundo nesta partida.";
pub const LEGENDA_GRAFICO: &str = "Quadros por segundo ao longo da partida";

pub const ESTAVEL: &str = "Estável";

// Métricas ------------------------------------------------------------------

/// Veredito em linguagem simples: "Estável" ou "N engasgos".
pub fn veredito(engasgos: u32) -> String {
    match engasgos {
        0 => ESTAVEL.into(),
        1 => "1 engasgo".into(),
        n => format!("{n} engasgos"),
    }
}

/// Duração curta: "40 min", "1 h 05 min", "menos de 1 min".
pub fn duracao(segundos: u32) -> String {
    let min = segundos / 60;
    match min {
        0 => "menos de 1 min".into(),
        1..=59 => format!("{min} min"),
        _ => format!("{} h {:02} min", min / 60, min % 60),
    }
}

/// Quando a partida começou, relativo ao dia de `agora` (ambos já no fuso
/// local): "hoje 21:10", "ontem 22:03" ou "12/11 19:40".
pub fn quando(inicio_local: i64, agora_local: i64) -> String {
    let (_, m, d, h, mi, _) = crate::modelo::civil(inicio_local);
    let dias = agora_local.div_euclid(86_400) - inicio_local.div_euclid(86_400);
    match dias {
        0 => format!("hoje {h:02}:{mi:02}"),
        1 => format!("ontem {h:02}:{mi:02}"),
        _ => format!("{d:02}/{m:02} {h:02}:{mi:02}"),
    }
}

pub fn fps(v: f32) -> String {
    format!("{:.0}", v.max(0.0))
}

pub fn media(v: f32) -> String {
    format!("Média {}", fps(v))
}

pub fn piores_momentos(v: f32) -> String {
    format!("Piores momentos {}", fps(v))
}

pub fn temperatura(c: u8) -> String {
    format!("Placa de vídeo chegou a {c} °C")
}

fn partidas(n: u32) -> String {
    super::plural(n.into(), "partida", "partidas")
}

/// Uma semana: "12 partidas, 3 com engasgos".
fn semana(partidas_n: u32, com_engasgo: u32) -> String {
    format!("{}, {com_engasgo} com engasgos", partidas(partidas_n))
}

/// Linha do topo da tela: esta semana contra a passada (todas as partidas).
pub fn comparacao_geral(esta: (u32, u32), passada: (u32, u32)) -> String {
    format!(
        "Esta semana: {}. Semana passada: {}.",
        semana(esta.0, esta.1),
        semana(passada.0, passada.1)
    )
}

/// Linha de um jogo: partidas e média de FPS desta semana e da passada.
pub fn comparacao_jogo(esta: (u32, Option<f32>), passada: (u32, Option<f32>)) -> String {
    let lado = |(n, m): (u32, Option<f32>)| match m {
        Some(m) if n > 0 => format!("{}, média {} FPS", partidas(n), fps(m)),
        _ => "nenhuma partida".into(),
    };
    format!(
        "Neste jogo, esta semana: {}. Semana passada: {}.",
        lado(esta),
        lado(passada)
    )
}

/// Rótulo do eixo do gráfico: minuto da partida.
pub fn minuto(t_rel_s: u32) -> String {
    format!("{} min", t_rel_s / 60)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn veredito_simples() {
        assert_eq!(veredito(0), "Estável");
        assert_eq!(veredito(1), "1 engasgo");
        assert_eq!(veredito(5), "5 engasgos");
    }

    #[test]
    fn duracoes() {
        assert_eq!(duracao(30), "menos de 1 min");
        assert_eq!(duracao(2400), "40 min");
        assert_eq!(duracao(3900), "1 h 05 min");
    }

    #[test]
    fn quando_relativo() {
        let agora = crate::modelo::unix_de_civil(2026, 11, 26, 23, 0, 0);
        let hoje = crate::modelo::unix_de_civil(2026, 11, 26, 21, 10, 0);
        let ontem = crate::modelo::unix_de_civil(2026, 11, 25, 22, 3, 0);
        let antes = crate::modelo::unix_de_civil(2026, 11, 12, 19, 40, 0);
        assert_eq!(quando(hoje, agora), "hoje 21:10");
        assert_eq!(quando(ontem, agora), "ontem 22:03");
        assert_eq!(quando(antes, agora), "12/11 19:40");
    }

    #[test]
    fn linhas_do_mock() {
        assert_eq!(
            comparacao_geral((12, 3), (9, 4)),
            "Esta semana: 12 partidas, 3 com engasgos. Semana passada: 9 partidas, 4 com engasgos."
        );
        assert_eq!(media(287.4), "Média 287");
        assert_eq!(piores_momentos(141.0), "Piores momentos 141");
        assert_eq!(temperatura(71), "Placa de vídeo chegou a 71 °C");
        assert_eq!(
            comparacao_jogo((1, Some(280.0)), (0, None)),
            "Neste jogo, esta semana: 1 partida, média 280 FPS. Semana passada: nenhuma partida."
        );
    }
}
