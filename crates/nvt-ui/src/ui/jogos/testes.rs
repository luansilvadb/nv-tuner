//! Testes da tela Jogos.

use super::{feito, linha, passos, t, tecnicos, tj};
use nvt_ipc::Jogo;
use nvt_ipc::{OrigemPerfil, ResumoJogo};

fn jogo(nome: &str) -> Jogo {
    Jogo {
        nome: nome.into(),
        perfil: format!("perfil-{nome}"),
        origem_perfil: OrigemPerfil::Driver,
        launcher: "Steam".into(),
        api: "dx12".into(),
        dlss: Some("3.7.10".into()),
        reflex: Some(true),
        anticheat_kernel: Some(true),
        competitivo: Some("cs2".into()),
        em_execucao: false,
        orientacoes: vec![],
        resumo: ResumoJogo::default(),
        config_jogo: None,
    }
}

#[test]
fn lista_mostra_so_nome_selo_e_status() {
    let mut j = jogo("Cyberpunk 2077");
    j.orientacoes = vec!["reflex_on_boost".into(), "x".into()];
    let l = linha(&j, false);
    assert_eq!(l.texto_status, "Faça 2 ajustes no jogo");
    assert!(l.competitivo);
    let visivel = format!("{} {}", l.nome, l.texto_status);
    for proibido in ["dx12", "DirectX", "3.7.10", "perfil-", "kernel", "Steam"] {
        assert!(!visivel.contains(proibido), "{proibido} na lista");
    }
}

#[test]
fn status_de_cada_caso() {
    let mut j = jogo("Valorant");
    assert_eq!(linha(&j, false).texto_status, tj::STATUS_NADA);
    j.em_execucao = true;
    assert_eq!(linha(&j, true).texto_status, tj::STATUS_AGUARDANDO);
    j.orientacoes = vec!["a".into()];
    assert_eq!(linha(&j, false).texto_status, "Faça 1 ajuste no jogo");
}

#[test]
fn detalhe_numera_passos_e_tecnicos_ficam_a_parte() {
    let mut j = jogo("CS2");
    j.orientacoes = vec!["a".into(), "b".into()];
    let p = passos(&j);
    assert_eq!(p.len(), 2);
    assert!(p[0].starts_with("1. ") && p[1].starts_with("2. "));
    assert!(feito(&j).is_empty());
    let tec = tecnicos(&j);
    assert!(tec.iter().any(|(_, v)| v == "3.7.10"));
    assert!(tec.iter().any(|(r, _)| *r == t::COL_ANTICHEAT));
}
