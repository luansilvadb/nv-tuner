//! Tela Início (§5.3): veredito, botões, jogos e saúde do PC.

use crate::modelo::{TipoSinal, Veredito};

/// Título e frase de apoio do veredito.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextoVeredito {
    pub titulo: String,
    pub frase: String,
}

pub const TITULO: &str = "Início";

// Vereditos fixos ----------------------------------------------------------
pub const VERIFICANDO_TITULO: &str = "Verificando o PC…";
pub const VERIFICANDO_FRASE: &str = "Isso leva só alguns segundos.";
pub const INDISPONIVEL_TITULO: &str = "O nv-tuner não está ativo neste PC";
pub const INDISPONIVEL_FRASE: &str = "Reinstale o nv-tuner para que ele volte a cuidar do PC.";
pub const SEM_PLACA_TITULO: &str = "Não encontramos uma placa de vídeo NVIDIA";
pub const SEM_PLACA_FRASE: &str = "O nv-tuner só ajusta PCs com placa NVIDIA.";
pub const PAUSADO_TITULO: &str = "Os ajustes estão desligados";
pub const PAUSADO_FRASE: &str = "Você pediu para desfazer tudo.";
pub const ESPERANDO_TITULO: &str = "Há mudanças esperando o jogo fechar";
pub const ESPERANDO_FRASE: &str = "Nada é mexido enquanto você joga.";
pub const PIOROU_TITULO: &str = "Algo piorou no PC";
pub const PIOROU_FRASE: &str = "Veja abaixo, em Saúde do PC, o que mudou.";
pub const MELHORIAS_TITULO: &str = "Há melhorias disponíveis";
pub const OTIMIZADO_TITULO: &str = "Seu PC está otimizado";
pub const OTIMIZADO_FRASE: &str = "Tudo o que recomendamos já está aplicado.";
pub const NADA_A_FAZER_TITULO: &str = "Não há nada a ajustar neste PC agora.";
pub const NADA_A_FAZER_FRASE: &str = "Se algo mudar, avisamos aqui.";
pub const MOTIVO_POUCO_ESPACO: &str = "pouco espaço livre";

// Botões e respostas -------------------------------------------------------
pub const BOTAO_APLICAR_RECOMENDADAS: &str = "Aplicar melhorias recomendadas";
pub const BOTAO_DESFAZER_TUDO: &str = "Desfazer tudo";
pub const LINK_DESFAZER_TUDO: &str = "Desfazer tudo que o nv-tuner mudou";
pub const BOTAO_VOLTAR_A_AJUSTAR: &str = "Voltar a ajustar";
pub const BOTAO_CANCELAR: &str = "Cancelar";
pub const APLICAR_INDISPONIVEL: &str = "Disponível quando o nv-tuner estiver ativo.";
pub const APLICAR_ADIADO: &str = "As melhorias serão aplicadas quando o jogo fechar.";
pub const VOLTAR_A_AJUSTAR_OK: &str = "Os ajustes foram ligados de novo.";

// Confirmação de Desfazer tudo (R-UI-07) ---------------------------------
pub const CONFIRMAR_DESFAZER_TITULO: &str = "Desfazer tudo?";
pub const CONFIRMAR_DESFAZER_VOLTA: &str =
    "Todos os ajustes que o nv-tuner fez voltam a ser como eram antes.";
pub const CONFIRMAR_DESFAZER_JOGO: &str =
    "Se um jogo estiver aberto, isso acontece quando ele fechar.";
pub const CONFIRMAR_DESFAZER_PARA: &str = "O nv-tuner para de ajustar até você pedir de novo.";
pub const DESFEITO_OK: &str = "Pronto. Tudo voltou a ser como era antes.";
pub const DESFAZER_ADIADO: &str = "Tudo volta a ser como era quando o jogo fechar.";

// Seus jogos ---------------------------------------------------------------
pub const SECAO_JOGOS: &str = "Seus jogos";
pub const VER_TODOS: &str = "Ver todos";

// Saúde do PC --------------------------------------------------------------
pub const SECAO_SAUDE: &str = "Saúde do PC";
pub const SAUDE_VERIFICANDO: &str = "Verificando o PC…";
pub const SAUDE_TUDO_CERTO: &str = "Tudo certo com a saúde do PC";
pub const ROTULO_ESPACO: &str = "Espaço livre";
pub const ROTULO_INICIALIZACAO: &str = "Inicialização";
pub const ROTULO_PLACA: &str = "Placa de vídeo";
pub const ROTULO_ENERGIA: &str = "Energia";
pub const ROTULO_REPOUSO: &str = "Uso em repouso";
pub const ROTULO_LIMPEZA: &str = "Limpeza do Windows";
pub const BOTAO_ABRIR_INICIALIZACAO: &str = "Abrir inicialização";
pub const INICIALIZACAO_OK: &str = "Nenhum programa novo começa junto com o Windows.";
pub const PLACA_OK: &str = "O driver está em dia.";
pub const PLACA_ANTIGO: &str = "Há um driver mais novo da NVIDIA.";
pub const PLACA_ATUALIZAR: &str = "Para atualizar, abra o aplicativo da NVIDIA.";
pub const ENERGIA_OK: &str = "O PC está no modo de energia recomendado.";
pub const ENERGIA_ATENCAO: &str = "O PC está num modo de energia que poupa desempenho.";
pub const ENERGIA_CAMINHO: &str = "Para mudar: Configurações > Sistema > Energia.";
pub const REPOUSO_OK: &str = "O PC está tranquilo quando você não está jogando.";
pub const REPOUSO_ATENCAO: &str = "O PC trabalha muito mesmo quando você não está jogando.";
pub const LIMPEZA_LIGADA: &str = "O Sensor de Armazenamento está ligado.";
pub const LIMPEZA_DESLIGADA: &str = "O Sensor de Armazenamento está desligado.";
pub const LIMPEZA_CAMINHO: &str = "Para ligar: Configurações > Sistema > Armazenamento.";

/// Todas as frases fixas da tela, para os testes de linguagem.
pub const TODOS: &[&str] = &[
    TITULO,
    INDISPONIVEL_TITULO,
    INDISPONIVEL_FRASE,
    SEM_PLACA_TITULO,
    SEM_PLACA_FRASE,
    PAUSADO_TITULO,
    PAUSADO_FRASE,
    ESPERANDO_TITULO,
    ESPERANDO_FRASE,
    PIOROU_TITULO,
    PIOROU_FRASE,
    MELHORIAS_TITULO,
    OTIMIZADO_TITULO,
    OTIMIZADO_FRASE,
    NADA_A_FAZER_TITULO,
    NADA_A_FAZER_FRASE,
    MOTIVO_POUCO_ESPACO,
    BOTAO_APLICAR_RECOMENDADAS,
    BOTAO_DESFAZER_TUDO,
    LINK_DESFAZER_TUDO,
    BOTAO_VOLTAR_A_AJUSTAR,
    BOTAO_CANCELAR,
    APLICAR_INDISPONIVEL,
    APLICAR_ADIADO,
    VOLTAR_A_AJUSTAR_OK,
    CONFIRMAR_DESFAZER_TITULO,
    CONFIRMAR_DESFAZER_VOLTA,
    CONFIRMAR_DESFAZER_JOGO,
    CONFIRMAR_DESFAZER_PARA,
    DESFEITO_OK,
    DESFAZER_ADIADO,
    SECAO_JOGOS,
    VER_TODOS,
    SECAO_SAUDE,
    SAUDE_VERIFICANDO,
    SAUDE_TUDO_CERTO,
    ROTULO_ESPACO,
    ROTULO_INICIALIZACAO,
    ROTULO_PLACA,
    ROTULO_ENERGIA,
    ROTULO_REPOUSO,
    ROTULO_LIMPEZA,
    BOTAO_ABRIR_INICIALIZACAO,
    INICIALIZACAO_OK,
    PLACA_OK,
    PLACA_ANTIGO,
    PLACA_ATUALIZAR,
    ENERGIA_OK,
    ENERGIA_ATENCAO,
    ENERGIA_CAMINHO,
    REPOUSO_OK,
    REPOUSO_ATENCAO,
    LIMPEZA_LIGADA,
    LIMPEZA_DESLIGADA,
    LIMPEZA_CAMINHO,
];

/// "3 ajustes podem deixar seus jogos mais leves." Sem prometer ganho medido.
pub fn melhorias_frase(n: usize) -> String {
    match n {
        0 | 1 => "1 ajuste pode deixar seus jogos mais leves.".to_owned(),
        n => format!("{n} ajustes podem deixar seus jogos mais leves."),
    }
}

/// "Aplicamos N melhorias. Para voltar atrás, use Desfazer tudo." (R-UI-06)
pub fn aplicadas(n: usize) -> String {
    match n {
        1 => "Aplicamos 1 melhoria. Para voltar atrás, use Desfazer tudo.".to_owned(),
        n => format!("Aplicamos {n} melhorias. Para voltar atrás, use Desfazer tudo."),
    }
}

/// Título de "piorou" com o motivo: "Algo piorou: pouco espaço livre".
pub fn piorou_titulo(motivo: &str) -> String {
    let m = motivo.trim();
    if m.is_empty() {
        PIOROU_TITULO.to_owned()
    } else {
        format!("Algo piorou: {m}")
    }
}

/// Motivo curto de um sinal em atenção; vazio cai no título genérico.
pub(crate) fn motivo(sinal: TipoSinal) -> &'static str {
    match sinal {
        TipoSinal::EspacoLivre => MOTIVO_POUCO_ESPACO,
        TipoSinal::MemoriaRepouso
        | TipoSinal::CpuRepouso
        | TipoSinal::DriverDesatualizado
        | TipoSinal::PlanoEnergia
        | TipoSinal::SensorArmazenamento => "",
    }
}

/// Texto do veredito do modelo: mapeamento único (R-UI-02).
pub fn veredito(v: &Veredito) -> TextoVeredito {
    let (titulo, frase): (String, String) = match v {
        Veredito::Verificando => (VERIFICANDO_TITULO.into(), VERIFICANDO_FRASE.into()),
        Veredito::Indisponivel(_) => (INDISPONIVEL_TITULO.into(), INDISPONIVEL_FRASE.into()),
        Veredito::SemPlacaNvidia => (SEM_PLACA_TITULO.into(), SEM_PLACA_FRASE.into()),
        Veredito::AjustesDesligados => (PAUSADO_TITULO.into(), PAUSADO_FRASE.into()),
        Veredito::EsperandoJogo => (ESPERANDO_TITULO.into(), ESPERANDO_FRASE.into()),
        Veredito::Piorou(s) => (piorou_titulo(motivo(*s)), PIOROU_FRASE.into()),
        Veredito::Melhorias { quantidade } => {
            (MELHORIAS_TITULO.into(), melhorias_frase(*quantidade))
        }
        Veredito::Otimizado => (OTIMIZADO_TITULO.into(), OTIMIZADO_FRASE.into()),
        Veredito::NadaAFazer => (NADA_A_FAZER_TITULO.into(), NADA_A_FAZER_FRASE.into()),
    };
    TextoVeredito { titulo, frase }
}

/// "Restam 210 GB. Tudo certo." / "Restam 12 GB no disco do Windows."
/// Gigabytes inteiros, arredondados para baixo.
pub fn espaco_livre(bytes_livres: u64, pouco: bool) -> String {
    let gb = bytes_livres / 1_000_000_000;
    if pouco {
        format!("Restam {gb} GB no disco do Windows.")
    } else {
        format!("Restam {gb} GB. Tudo certo.")
    }
}

/// "2 programas novos começam junto com o Windows."
pub fn inicializacao_novos(n: usize) -> String {
    match n {
        0 => INICIALIZACAO_OK.to_owned(),
        1 => "1 programa novo começa junto com o Windows.".to_owned(),
        n => format!("{n} programas novos começam junto com o Windows."),
    }
}

// Resultados das ações do Início (L2) --------------------------------------
pub const PARA_VOLTAR_ATRAS: &str = "Para voltar atrás, use Desfazer tudo.";
pub const NENHUMA_APLICADA: &str = "Não havia melhorias para aplicar agora.";
pub const RECOMENDADAS_AUSENTE: &str =
    "Esta versão ainda não aplica tudo de uma vez. Escolha uma a uma no Avançado.";
pub const RECURSO_AUSENTE: &str = "Esta versão do nv-tuner ainda não faz isso.";
pub const VOLTAR_ADIADO: &str = "Os ajustes voltam quando o jogo fechar.";
pub const AGUARDE: &str = "Só um instante…";

/// Frases fixas acrescentadas no L2, para os testes de linguagem.
pub const TODOS_L2: &[&str] = &[
    PARA_VOLTAR_ATRAS,
    NENHUMA_APLICADA,
    RECOMENDADAS_AUSENTE,
    RECURSO_AUSENTE,
    VOLTAR_ADIADO,
    AGUARDE,
];

/// "e mais 3 jogos" no resumo de Seus jogos.
pub fn mais_jogos(n: usize) -> String {
    match n {
        1 => "e mais 1 jogo".to_owned(),
        n => format!("e mais {n} jogos"),
    }
}
