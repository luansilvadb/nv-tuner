//! Testes do pipeline local somente leitura.

use super::*;
use nvt_engine::{DriverFalso, JogoEntrada};

#[test]
fn somente_leitura_recusa_toda_escrita() {
    let falso = DriverFalso::turing();
    let mut s = SomenteLeitura(falso.clone());
    assert!(s.criar_perfil("x").is_err());
    assert!(s.criar_aplicacao("x", "x.exe").is_err());
    assert!(
        s.gravar_setting(&ProfileRef::Base, 1, &SettingValue::Dword(1))
            .is_err()
    );
    assert!(s.remover_setting(&ProfileRef::Base, 1, false).is_err());
    assert!(s.salvar().is_err());
    // Leitura continua funcionando.
    assert!(s.gpus().is_ok());
    assert!(s.perfil_existe("qualquer").is_ok());
    assert_eq!((falso.gravacoes(), falso.salvamentos()), (0, 0));
}

#[test]
fn aplicar_pelo_motor_falha_em_somente_leitura() {
    let falso = DriverFalso::turing();
    let mut e = Engine::new(SomenteLeitura(falso.clone()), Armazem::em_memoria()).unwrap();
    let diff = e.planejar(&[]).unwrap();
    assert!(
        !diff.vazio(),
        "o perfil global de uma Turing tem o que mudar"
    );
    assert!(e.aplicar(&diff, true).is_err());
    assert_eq!((falso.gravacoes(), falso.salvamentos()), (0, 0));
}

fn estado_json() -> String {
    serde_json::to_string(&nvt_engine::Estado::default()).unwrap()
}

fn codigos(s: &Salvo) -> Vec<&str> {
    s.avisos.iter().map(|a| a.codigo.as_str()).collect()
}

#[test]
fn estado_salvo_lido_sem_avisos() {
    let s = interpretar_salvo(Ok(Some(estado_json())), Ok(None), Ok(None), 1);
    assert!(s.estado.is_some());
    assert!(s.snapshot.is_none());
    assert!(s.avisos.is_empty());
    let s = interpretar_salvo(Ok(None), Ok(None), Ok(None), 1);
    assert!(s.estado.is_none() && s.avisos.is_empty());
}

#[test]
fn estado_corrompido_usa_a_copia_de_seguranca() {
    let s = interpretar_salvo(
        Ok(Some("{ truncado".into())),
        Ok(Some(estado_json())),
        Ok(None),
        7,
    );
    assert!(s.estado.is_some(), "mostra a cópia de segurança");
    assert_eq!(
        codigos(&s),
        [
            codigo_aviso::ESTADO_CORROMPIDO,
            codigo_aviso::ESTADO_RECUPERADO
        ]
    );
    assert_eq!(s.avisos[0].nivel, NivelAviso::Atencao);
    assert!(s.avisos.iter().all(|a| a.quando_unix == 7));
    // Versão não suportada também é inválida; sem cópia, recomeça do zero.
    let v2 = estado_json().replace("\"versao\":1", "\"versao\":99");
    let s = interpretar_salvo(Ok(Some(v2)), Ok(Some("".into())), Ok(None), 7);
    assert!(s.estado.is_none());
    assert_eq!(
        codigos(&s),
        [
            codigo_aviso::ESTADO_CORROMPIDO,
            codigo_aviso::ESTADO_REINICIADO
        ]
    );
    assert!(
        s.avisos[0]
            .detalhe
            .as_deref()
            .unwrap()
            .contains("versão 99")
    );
}

/// Todos os códigos do motor. A `match` abaixo não compila se o motor
/// ganhar um código novo: aí ele precisa entrar nesta lista.
fn todos_os_codigos_do_motor() -> [CodigoAviso; 5] {
    fn _exaustivo(c: CodigoAviso) {
        match c {
            CodigoAviso::EstadoCorrompido
            | CodigoAviso::EstadoRecuperado
            | CodigoAviso::EstadoReiniciado
            | CodigoAviso::SnapshotIndisponivel
            | CodigoAviso::SnapshotOriginalPerdido => {}
        }
    }
    [
        CodigoAviso::EstadoCorrompido,
        CodigoAviso::EstadoRecuperado,
        CodigoAviso::EstadoReiniciado,
        CodigoAviso::SnapshotIndisponivel,
        CodigoAviso::SnapshotOriginalPerdido,
    ]
}

#[test]
fn todo_codigo_do_motor_tem_constante_no_protocolo_e_texto_na_ui() {
    let constantes = [
        codigo_aviso::ESTADO_CORROMPIDO,
        codigo_aviso::ESTADO_RECUPERADO,
        codigo_aviso::ESTADO_REINICIADO,
        codigo_aviso::ESTADO_AVISO,
        codigo_aviso::SNAPSHOT_INDISPONIVEL,
        codigo_aviso::SNAPSHOT_ORIGINAL_PERDIDO,
        codigo_aviso::ERRO_PERFIL_APLICACAO,
        codigo_aviso::ERRO_PERFIL_DERIVA,
        codigo_aviso::JOGOS_IGNORADOS,
    ];
    for c in todos_os_codigos_do_motor() {
        assert_eq!(codigo_ipc(c), c.como_str(), "{c:?}");
        assert!(constantes.contains(&c.como_str()), "{c:?} sem constante");
        assert!(!textos::texto_aviso_motor(c).trim().is_empty(), "{c:?}");
    }
}

#[test]
fn estado_ausente_usa_a_copia_de_seguranca_como_o_motor() {
    let s = interpretar_salvo(Ok(None), Ok(Some(estado_json())), Ok(None), 4);
    assert!(s.estado.is_some(), "mostra a cópia de segurança");
    assert_eq!(codigos(&s), [codigo_aviso::ESTADO_RECUPERADO]);
    assert_eq!(s.avisos[0].nivel, NivelAviso::Atencao);
    // Cópia inválida ou ilegível: estado vazio, sem aviso (como o motor).
    for anterior in [Ok(Some("{".to_owned())), Err("acesso negado".to_owned())] {
        let s = interpretar_salvo(Ok(None), anterior, Ok(None), 4);
        assert!(s.estado.is_none() && s.avisos.is_empty());
    }
}

#[test]
fn ler_salvo_nao_toca_nos_arquivos() {
    let dir = std::env::temp_dir().join(format!(
        "nvt-ui-ler-salvo-{}-{}",
        std::process::id(),
        crate::fuso::agora_unix()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let anterior = estado_json();
    std::fs::write(dir.join("estado.anterior.json"), &anterior).unwrap();
    std::fs::write(
        dir.join("snapshot-perdido.json"),
        r#"{"detalhe":"quarentena em X"}"#,
    )
    .unwrap();
    let s = ler_salvo(&dir, 2);
    assert!(s.estado.is_some());
    assert_eq!(
        codigos(&s),
        [
            codigo_aviso::ESTADO_RECUPERADO,
            codigo_aviso::SNAPSHOT_ORIGINAL_PERDIDO
        ]
    );
    assert_eq!(
        s.avisos[1].detalhe.as_deref(),
        Some("snapshot-perdido.json: quarentena em X")
    );
    // Nada foi renomeado nem criado.
    let mut nomes: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    nomes.sort();
    assert_eq!(nomes, ["estado.anterior.json", "snapshot-perdido.json"]);
    assert_eq!(
        std::fs::read_to_string(dir.join("estado.anterior.json")).unwrap(),
        anterior
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn marcador_de_quarentena_vira_snapshot_original_perdido() {
    assert!(interpretar_marcador_quarentena(Ok(None), 1).is_none());
    let a = interpretar_marcador_quarentena(Ok(Some("texto solto\n".into())), 1).unwrap();
    assert_eq!(a.codigo, codigo_aviso::SNAPSHOT_ORIGINAL_PERDIDO);
    assert_eq!(a.nivel, NivelAviso::Atencao);
    assert_eq!(
        a.detalhe.as_deref(),
        Some("snapshot-perdido.json: texto solto")
    );
    let a = interpretar_marcador_quarentena(Err("acesso negado".into()), 1).unwrap();
    assert_eq!(a.codigo, codigo_aviso::ESTADO_AVISO);
}

#[test]
fn snapshot_ilegivel_bloqueia_restauracao_no_modo_local() {
    let s = interpretar_salvo(Ok(None), Ok(None), Ok(Some(String::new())), 3);
    assert!(s.snapshot.is_none());
    assert_eq!(codigos(&s), [codigo_aviso::SNAPSHOT_INDISPONIVEL]);
    assert_eq!(s.avisos[0].nivel, NivelAviso::Erro);
    // Erro de leitura não é tratado como snapshot corrompido.
    let s = interpretar_salvo(Ok(None), Ok(None), Err("acesso negado".into()), 3);
    assert_eq!(codigos(&s), [codigo_aviso::ESTADO_AVISO]);
}

#[test]
fn reversoes_e_ignorados_calculados_localmente() {
    // Aplica com um motor gravável (só no DriverFalso) e calcula a visão
    // local com o estado resultante, como o modo de visualização faz.
    let falso = DriverFalso::turing();
    let mut e = Engine::new(falso.clone(), Armazem::em_memoria()).unwrap();
    e.definir_preferencias(nvt_core::Preferences {
        limite_fps: Some(141),
        ..Default::default()
    })
    .unwrap();
    let diff = e.planejar(&[]).unwrap();
    e.aplicar(&diff, true).unwrap();
    let mut armazem = Armazem::em_memoria();
    armazem.estado = e.estado().clone();
    armazem.estado.preferencias.limite_fps = None;
    let gravacoes = falso.gravacoes();
    let local = Engine::new(SomenteLeitura(falso.clone()), armazem).unwrap();
    let ruim = JogoEntrada::new("Jogo Ruim", "ruim\u{0}.exe");
    let diff = local.planejar(&[ruim]).unwrap();
    let r = reversoes(&local, &diff);
    let lim = r
        .iter()
        .find(|r| r.setting == "limite_fps")
        .expect("limite de FPS sai do plano");
    assert_eq!(lim.status, StatusMudanca::Pendente);
    assert_eq!(lim.perfil, "global");
    assert_eq!(lim.atual.as_deref(), Some("141 FPS"));
    assert!(lim.motivo.starts_with("Saiu do plano porque a preferência"));
    let ig = ignorados(&diff);
    assert_eq!(ig.len(), 1);
    assert_eq!(ig[0].nome, "Jogo Ruim");
    assert_eq!(falso.gravacoes(), gravacoes, "nada gravado no modo local");
}

fn jogos_de_teste() -> Vec<JogoEntrada> {
    let mut w = JogoEntrada::new("WARDOGS", "WardogsClient-Win64-Shipping.exe");
    w.launcher = "Steam".into();
    w.api = nvt_core::Api::Dx12;
    w.dlss = Some(nvt_core::Versao::new(310, 6, 0, 0));
    let mut l = JogoEntrada::new("League of Legends", "League of Legends.exe");
    l.launcher = "Riot".into();
    l.api = nvt_core::Api::Dx11;
    vec![w, l]
}

fn resumo<'a>(jogos: &'a [Jogo], nome: &str) -> &'a ResumoJogo {
    &jogos.iter().find(|j| j.nome == nome).unwrap().resumo
}

/// Resumo de uma linha do que o plano fez no jogo: as regras aplicadas,
/// ou o motivo mais relevante de nada específico ter sido feito.
fn resumo_curto(r: &nvt_ipc::ResumoJogo) -> String {
    use nvt_ipc::MotivoNaoAplicada as N;
    if !r.aplicadas.is_empty() {
        return r
            .aplicadas
            .iter()
            .map(|a| textos::nome_regra(&a.regra, &a.descricao))
            .collect::<Vec<_>>()
            .join(" + ");
    }
    let relevancia = |m: &N| match m {
        N::Experimental => 0,
        N::EscolhaDoUsuario { .. } => 1,
        N::BloqueadaPeloTetoDeQualidade { .. } => 2,
        N::SemTensorCores => 3,
        N::ArquiteturaNaoPrecisa { .. } => 4,
        N::JogoSemDlss => 5,
        N::FatoDesconhecido { .. } => 6,
        N::ConflitoComRegraMaisEspecifica { .. } | N::VrrInativo | N::JogoComReflex => 7,
        _ => 8,
    };
    match r
        .nao_aplicadas
        .iter()
        .map(|n| &n.motivo)
        .min_by_key(|m| relevancia(m))
    {
        None => "Nenhum ajuste específico".into(),
        Some(m) => format!("Nenhum ajuste específico — {}", textos::motivo_curto(m)),
    }
}

/// Descrição da regra `energia-desempenho-maximo-por-jogo` (regras/base.toml).
const ENERGIA_TXT: &str =
    "Gerenciamento de energia em desempenho máximo no perfil do jogo competitivo";

#[test]
fn jogos_locais_tem_o_mesmo_formato_do_servico() {
    let falso = DriverFalso::turing();
    let e = Engine::new(SomenteLeitura(falso.clone()), Armazem::em_memoria()).unwrap();
    let diff = e.planejar(&jogos_de_teste()).unwrap();
    let jogos = jogos_ipc(&e, &diff);
    assert_eq!(jogos.len(), 2);
    let w = jogos.iter().find(|j| j.nome == "WARDOGS").unwrap();
    assert_eq!((w.launcher.as_str(), w.api.as_str()), ("Steam", "dx12"));
    assert_eq!(w.origem_perfil, OrigemPerfilIpc::ACriar);
    // Spec competitivo F1-11 e §11 item 19: a energia por jogo só vale para
    // jogo competitivo. WARDOGS está fora da base: só o DLSS; LoL é
    // competitivo (base curada): recebe a energia.
    assert_eq!(resumo_curto(&w.resumo), "Preset E do DLSS (Turing)");
    assert_eq!(
        resumo_curto(resumo(&jogos, "League of Legends")),
        ENERGIA_TXT
    );
    assert!(!w.resumo.herda_do_global.is_empty());
    // Fase 1: jogo da base curada sai com o id, os fatos curados e as
    // orientações “faça no jogo”, todas com texto na UI.
    assert_eq!(w.competitivo, None);
    assert!(w.orientacoes.is_empty());
    let l = jogos
        .iter()
        .find(|j| j.nome == "League of Legends")
        .unwrap();
    assert_eq!(l.competitivo.as_deref(), Some("lol"));
    assert_eq!(l.reflex, Some(false), "fato da base curada");
    assert!(!l.em_execucao);
    assert!(!l.orientacoes.is_empty());
    assert!(
        l.orientacoes
            .iter()
            .all(|o| textos::orientacao(o).is_some())
    );
    assert_eq!((falso.gravacoes(), falso.salvamentos()), (0, 0));
}

#[test]
fn avisos_do_plano_iguais_aos_do_servico() {
    let a = aviso_do_plano(nvt_core::AvisoPlano::GsyncSemVrr, 9);
    assert_eq!(a.codigo, codigo_aviso::GSYNC_SEM_VRR);
    assert_eq!(a.quando_unix, 9);
    let b = aviso_do_plano(nvt_core::AvisoPlano::VrrDesligadoNoPainel, 9);
    assert_eq!(b.codigo, codigo_aviso::VRR_DESLIGADO_NO_PAINEL);
}

#[test]
fn jogo_sem_dlss_fora_da_base_explica_que_nada_foi_aplicado() {
    // Spec competitivo §11 item 19: sem a energia por jogo, o jogo não
    // competitivo e sem DLSS volta a dizer "nenhum ajuste específico".
    let falso = DriverFalso::turing();
    let e = Engine::new(SomenteLeitura(falso), Armazem::em_memoria()).unwrap();
    let mut entrada = jogos_de_teste();
    entrada.push(JogoEntrada::new("Jogo Qualquer", "qualquer.exe"));
    let diff = e.planejar(&entrada).unwrap();
    let jogos = jogos_ipc(&e, &diff);
    assert_eq!(
        resumo_curto(resumo(&jogos, "Jogo Qualquer")),
        "Nenhum ajuste específico — jogo sem DLSS"
    );
    assert_eq!(
        resumo_curto(resumo(&jogos, "League of Legends")),
        ENERGIA_TXT
    );
}

#[cfg(debug_assertions)]
#[test]
fn gpu_ada_simulada_explica_por_que_nada_foi_aplicado() {
    let ada = gpu_simulada_de("ada").unwrap();
    assert!(ada.name.contains("simulada"));
    assert!(gpu_simulada_de("rtx9090").is_none());
    // Mesma decisão do motor com a GPU simulada: a arquitetura vem dela.
    let falso = DriverFalso::turing();
    let e = Engine::new(SomenteLeitura(falso.clone()), Armazem::em_memoria()).unwrap();
    let mut diff = e.planejar(&jogos_de_teste()).unwrap();
    let caps = nvt_engine::capacidades(&GpuFixa(ada)).unwrap();
    assert_eq!(caps.arquitetura, nvt_core::Arquitetura::Ada);
    assert!(caps.tensor_fp8);
    let fatos: Vec<_> = diff.jogos.iter().map(|j| j.fatos.clone()).collect();
    diff.plano = nvt_core::avaliar(&caps, &fatos, &Default::default(), e.regras());
    let jogos = jogos_ipc(&e, &diff);
    // WARDOGS está fora da base competitiva: sem energia por jogo, o resumo
    // curto volta a explicar o motivo do DLSS (spec competitivo §11 item
    // 19). LoL é competitivo e recebe a energia.
    assert_eq!(
        resumo_curto(resumo(&jogos, "WARDOGS")),
        "Nenhum ajuste específico — GPU Ada roda o DLSS padrão sem custo extra"
    );
    assert_eq!(
        resumo_curto(resumo(&jogos, "League of Legends")),
        ENERGIA_TXT
    );
}

/// Driver mínimo que só informa uma GPU (para `nvt_engine::capacidades`).
struct GpuFixa(GpuInfo);

impl DriverStore for GpuFixa {
    fn versao_driver(&self) -> ResultadoDrs<DriverVersion> {
        DriverFalso::turing().versao_driver()
    }
    fn gpus(&self) -> ResultadoDrs<Vec<GpuInfo>> {
        Ok(vec![self.0.clone()])
    }
    fn perfil_do_exe(&self, _: &str) -> ResultadoDrs<Option<PerfilEncontrado>> {
        Ok(None)
    }
    fn perfil_existe(&self, _: &str) -> ResultadoDrs<bool> {
        Ok(false)
    }
    fn criar_perfil(&mut self, _: &str) -> ResultadoDrs<()> {
        Err(recusa())
    }
    fn criar_aplicacao(&mut self, _: &str, _: &str) -> ResultadoDrs<()> {
        Err(recusa())
    }
    fn ler_setting(&self, _: &ProfileRef, _: u32) -> ResultadoDrs<Option<Setting>> {
        Ok(None)
    }
    fn gravar_setting(&mut self, _: &ProfileRef, _: u32, _: &SettingValue) -> ResultadoDrs<()> {
        Err(recusa())
    }
    fn remover_setting(&mut self, _: &ProfileRef, _: u32, _: bool) -> ResultadoDrs<()> {
        Err(recusa())
    }
    fn settings_do_perfil(&self, _: &ProfileRef) -> ResultadoDrs<Vec<Setting>> {
        Ok(Vec::new())
    }
    fn capturar(&self, exes: &[&str]) -> ResultadoDrs<Snapshot> {
        DriverFalso::turing().capturar(exes)
    }
    fn salvar(&mut self) -> ResultadoDrs<()> {
        Err(recusa())
    }
}

#[test]
fn mapeamentos() {
    assert_eq!(impacto_ipc(ImpactoRegra::Leve), ImpactoVisual::Baixo);
    assert_eq!(impacto_ipc(ImpactoRegra::Forte), ImpactoVisual::Alto);
    let p = nvt_core::Preferences {
        qualidade: nvt_core::Qualidade::Fidelidade,
        limite_fps: Some(141),
        ..Default::default()
    };
    assert_eq!(
        preferencias_ipc(&p),
        Preferencias {
            sincronismo: Sincronismo::Desligado,
            qualidade: Qualidade::Qualidade,
            limite_fps: Some(141),
        }
    );
    assert_eq!(
        acao_de_origem(&OrigemRegistro::Restauracao),
        AcaoHistorico::Restauracao
    );
    assert_eq!(
        acao_de_origem(&OrigemRegistro::Usuario),
        AcaoHistorico::DerivaDetectada
    );
}
