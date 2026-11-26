//! Textos da tela Partidas e do grupo Métricas (build de teste, spec
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

// Métricas (Avançado) ----------------------------------------------------
pub const GRUPO_METRICAS: &str = "Métricas";
pub const GRUPO_METRICAS_DESC: &str =
    "Números detalhados da última partida gravada e o que cada um significa.";
pub const EXPLICA_FRAMETIME: &str = "Frametime: quanto tempo cada quadro leva para aparecer, em \
     milissegundos. Quanto menor e mais regular, mais liso o jogo.";
pub const EXPLICA_1PCT: &str = "1% low (\"piores momentos\"): o FPS do 1% de quadros mais lentos \
     (percentil 99 do frametime convertido em FPS). Mostra as quedas que a média esconde.";
pub const EXPLICA_ENGASGO: &str = "Engasgo: quadro que demorou mais de 2,5 vezes a mediana dos \
     2 segundos anteriores e mais de 25 ms.";
pub const SEM_PARTIDA: &str = "Nenhuma partida gravada para detalhar.";
pub const ROTULO_JOGO: &str = "Partida";
pub const ROTULO_FRAMETIME: &str = "Frametime médio";
pub const ROTULO_MEDIANA: &str = "FPS mediano";
pub const ROTULO_1PCT: &str = "1% low";
pub const ROTULO_01PCT: &str = "0,1% low";
pub const ROTULO_ENGASGADO: &str = "Tempo engasgado";
pub const ROTULO_USO_GPU: &str = "Uso médio da placa de vídeo";
pub const ROTULO_PERDA: &str = "Eventos perdidos na medição";
pub const BOTAO_APAGAR: &str = "Apagar histórico de partidas";
pub const CONFIRMAR_TITULO: &str = "Apagar o histórico de partidas?";
pub const CONFIRMAR_TEXTO: &str =
    "Todas as partidas gravadas deste usuário serão apagadas. Isso não pode ser desfeito.";
pub const CONFIRMAR_OK: &str = "Apagar";
pub const CANCELAR: &str = "Cancelar";
pub const APAGADO: &str = "Histórico de partidas apagado.";

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
    if n == 1 {
        "1 partida".into()
    } else {
        format!("{n} partidas")
    }
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

pub fn ms(v: f32) -> String {
    format!("{v:.1} ms").replace('.', ",")
}

pub fn percentual(v: f32) -> String {
    format!("{v:.1}%").replace('.', ",")
}

pub fn tempo_engasgado(ms_total: u32) -> String {
    if ms_total < 1000 {
        format!("{ms_total} ms")
    } else {
        format!("{:.1} s", f64::from(ms_total) / 1000.0).replace('.', ",")
    }
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
        assert_eq!(ms(4.26), "4,3 ms");
        assert_eq!(tempo_engasgado(1500), "1,5 s");
    }
}
