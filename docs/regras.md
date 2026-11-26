# Regras do nv-tuner

As decisões do nv-tuner vêm de **regras declarativas em TOML** (pasta
`regras/`), avaliadas pelo crate `nvt-core` sobre três entradas:

- **capacidades** da GPU (`Capabilities`): arquitetura, Tensor Cores, FP8,
  Frame Generation, MFG, ReBAR, VRAM, driver, refresh do monitor principal
  (`refresh_mhz`, em milésimos de Hz) e VRR (`vrr: EstadoVrr { possivel,
  ativo }`);
- **fatos** de cada jogo (`GameFacts`): API gráfica, versão do DLSS, Reflex,
  anticheat de kernel e, para jogo casado na base curada (`regras/jogos.toml`),
  `competitivo`, `limitador_nativo`, `jogo_base` e `orientacoes`;
- **preferências** do usuário (`Preferences`): sincronismo, teto de qualidade,
  limite de FPS e escolhas travadas.

O resultado é um **plano** (`Plan`) com as mudanças do perfil global e de
cada jogo, sempre com origem e explicação, além de orientações por jogo e
avisos (ver “Orientações e avisos do plano”). Os arquivos são embutidos no
binário (`ConjuntoRegras::embutido()` e `BaseJogos::embutida()`).

## Arquivos

| Arquivo | Conteúdo |
|---|---|
| `regras/settings.toml` | Catálogo: nome simbólico → ID DRS e valores aceitos. É também a lista permitida do serviço |
| `regras/base.toml` | Regras base (baseline global, sincronismo, energia por jogo competitivo, DLSS) |
| `regras/jogos.toml` | Base curada de jogos competitivos (ver “Base curada de jogos”); não tem regras, só fatos e orientações |

Todo arquivo começa com `versao_schema = 1`.

## Catálogo (`settings.toml`)

```toml
[[setting]]
nome = "limite_fps"                        # snake_case; usado em `aplicar`
id = 0x10835002                            # ID DRS
descricao = "Frame Rate Limiter V3"
governada_por = ["sincronismo", "limite_fps"]  # opcional
valores = { off = 0 }                      # valores nomeados
faixa = { min = 20, max = 1000 }           # opcional: números livres aceitos
```

- `valores` ou `faixa` é obrigatório; nomes e IDs não podem se repetir.
- `governada_por`: preferências que mandam na setting (`sincronismo`,
  `limite_fps`). Uma regra só pode gravar uma setting governada se estiver
  condicionada a uma dessas preferências em `quando`
  (`sincronismo = [...]` ou `limite_fps_definido = ...`).

## Regra

```toml
[[regra]]
id = "dlss-preset-e-turing"        # kebab-case, único entre todos os arquivos
descricao = "DLSS-SR com preset E forçado pelo driver em RTX 20"
status = "estavel"                 # estavel | experimental
escopo = "por_jogo"                # global | por_jogo
impacto_visual = "leve"            # nenhum | leve | forte
prioridade = 50                    # opcional (padrão 0); maior vence empates
motivo = "Por que a regra existe."
evidencia = "Medição ou fonte que a sustenta (sem dados pessoais)."

[regra.quando]                     # opcional; todas as condições valem (E)
arquitetura = ["turing"]
tem_tensor_cores = true
jogo_dlss = true

[regra.aplicar]                    # obrigatório; nome simbólico = valor
dlss_override = "on"               # valor nomeado do catálogo
dlss_perfil_preset = "personalizado"
dlss_preset = "e"
```

`descricao`, `motivo` e `evidencia` são obrigatórios e não podem ser vazios.
Campos desconhecidos são erro (protege contra erro de digitação).

### Motivo quando a regra não se aplica (`motivo_quando_nao_aplica`)

Opcional. Diz à UI **por que** uma regra por jogo não foi aplicada quando uma
condição-chave falha, em vez do genérico “condição não atendida”:

```toml
[regra.motivo_quando_nao_aplica]
# condição = motivo (vale sempre que a condição falhar)
jogo_dlss = "jogo_sem_dlss"
# condição de lista = { valor encontrado = motivo } (outros valores: genérico)
arquitetura = { ada = "arquitetura_nao_precisa", blackwell = "arquitetura_nao_precisa" }
```

- Motivos aceitos: `jogo_sem_dlss`, `arquitetura_nao_precisa`,
  `sem_tensor_cores`.
- A chave precisa ser uma condição presente em `quando`. A forma de tabela só
  vale para condições de lista (`arquitetura`, `jogo_api`, `sincronismo`,
  `qualidade`); cada chave dela precisa ser um valor aceito pela condição e
  **fora** da lista (senão a condição nunca falharia com ele). Erros saem como
  `ErroRegra::MotivoInvalido`.
- **Padrões** (sem declarar): `jogo_dlss = true` ou `jogo_dlss_min` num jogo
  sem DLSS → `jogo_sem_dlss`; `tem_tensor_cores = true` numa GPU sem Tensor
  Cores → `sem_tensor_cores`. Um motivo declarado para a condição substitui o
  padrão dela.
- Por que por valor: a regra de Turing falha na arquitetura tanto numa Ada
  (que não precisa do ajuste) quanto numa Ampere (que tem regra própria).
  Só o primeiro caso é “arquitetura não precisa”; o segundo fica como
  condição não atendida (“a regra pede Turing; aqui é Ampere”).

### Condições (`quando`)

| Condição | Tipo | Casa quando |
|---|---|---|
| `arquitetura` | lista: `turing`, `ampere`, `ada`, `blackwell`, `desconhecida` | a arquitetura está na lista |
| `tem_tensor_cores`, `tensor_fp8`, `frame_gen`, `mfg`, `rebar_suportado` | bool | igual à capacidade |
| `rebar_ativo` | bool | igual ao lido; **não casa se desconhecido** |
| `vram_mb_min` | inteiro | VRAM ≥ valor |
| `driver_min` | texto (`"560.0"`) | driver ≥ valor |
| `refresh_conhecido` | bool | refresh do monitor conhecido (ou não); refresh = `refresh_mhz` arredondado para Hz inteiro (`Capabilities::refresh_hz`; abaixo de 0,5 Hz conta como desconhecido) |
| `vrr_possivel` | bool | igual a `vrr.possivel` (monitor principal aceita VRR); **não casa se desconhecido** |
| `vrr_ativo` | bool | igual a `vrr.ativo` (VRR ligado no painel); **não casa se desconhecido** |
| `jogo_api` | lista: `dx9`, `dx11`, `dx12`, `vulkan`, `opengl`, `desconhecida` | API do jogo na lista |
| `jogo_dlss` | bool | jogo tem (ou não tem) DLSS; DLSS não detectado conta como "não tem" |
| `jogo_dlss_min` | texto (`"310.0"`) | DLSS presente e versão ≥ valor |
| `jogo_reflex`, `jogo_anticheat_kernel` | bool | igual ao fato; **não casa se desconhecido**. Para jogo da base curada, o fato vem da base (anticheat pode ser `false`, ex.: VAC) |
| `jogo_competitivo` | bool | jogo casado (ou não) na base curada; nunca é desconhecido (fora da base = `false`) |
| `jogo_limitador_nativo` | bool | limitador de FPS nativo declarado na base; **não casa se desconhecido** (jogo fora da base) |
| `sincronismo` | lista: `sem_sync`, `gsync`, `vsync` | preferência na lista |
| `qualidade` | lista: `desempenho`, `equilibrado`, `fidelidade` | teto na lista |
| `limite_fps_definido` | bool | usuário definiu (ou não) um limite de FPS |

Condições `jogo_*` só valem em `escopo = "por_jogo"`. Listas não podem ser
vazias nem repetir itens; nomes desconhecidos são erro. Condições
contraditórias detectáveis (`jogo_dlss = false` com `jogo_dlss_min`) são erro.

### Valores (`aplicar`)

- **Nome** do catálogo: `vsync = "force_off"`.
- **Número**: `limite_fps = 141` (dentro da `faixa` ou igual a um valor
  nomeado).
- **Derivado do refresh**: `limite_fps = { refresh_menos = 3 }` (piso do refresh −
  3: 59,94 Hz conta como 59, não 60, para nunca ficar a menos de 3 FPS do
  refresh real; o arredondamento fica só na exibição). Exige
  `quando.refresh_conhecido = true` e uma setting com `faixa`; se o
  resultado sair da faixa, a mudança é descartada com motivo.
- **Limite para VRR**: `limite_fps = { limite_vrr = true }` =
  `min(Hz − 3, ⌊Hz − Hz²/3600⌋)` sobre o piso do refresh (59,94 Hz → 59;
  `nvt_core::limite_vrr`, em inteiros:
  `Hz − ⌈Hz²/3600⌉`). Coincide com o limite automático do driver no modo
  G-SYNC (≈224 a 240 Hz) e nunca fica a menos de 3 FPS do refresh. Exige
  `quando.refresh_conhecido = true` **e** `quando.vrr_ativo = true` e uma
  setting com `faixa` (senão é erro de carga); resultado não positivo ou fora
  da faixa descarta a mudança com motivo. A explicação mostra a conta
  (“mínimo entre 240 Hz − 3 e ⌊240 − 240²/3600⌋ = 224”).

  | Hz | 60 | 100 | 120 | 144 | 165 | 240 | 360 | 480 |
  |---|---|---|---|---|---|---|---|---|
  | `Hz − 3` | 57 | 97 | 117 | 141 | 162 | 237 | 357 | 477 |
  | `⌊Hz − Hz²/3600⌋` | 59 | 97 | 116 | 138 | 157 | 224 | 324 | 416 |
  | **resultado** | **57** | **97** | **116** | **138** | **157** | **224** | **324** | **416** |

  A tabela é verificada pelo teste `limite_vrr_tabela_da_spec`
  (`crates/nvt-core/tests/competitivo.rs`).

## Base curada de jogos (`jogos.toml`)

Lista os jogos competitivos que o app reconhece (`nvt_core::BaseJogos`,
módulo `jogos`). Não grava nada por si: fornece fatos curados e códigos de
orientação.

```toml
versao_schema = 1

[[jogo]]
id = "cs2"                                   # kebab-case, único
nome = "Counter-Strike 2"
executaveis = ["cs2.exe"]                    # ao menos um
steam_appid = 730                            # opcional
reflex = true
limitador_nativo = true
anticheat = "vac"                            # vac | vanguard | eac | battleye | eac_battleye | ricochet | blizzard | netease | proprio
anticheat_kernel = false
processos_protegidos = []                    # opcional: processos vigiados além dos executáveis
config = "edicao"                            # edicao | opcoes_inicializacao | orientar | proibido (fase 3)
orientacoes = ["reflex_on_boost", "limitador_do_jogo", "tela_cheia", "vsync_jogo_off"]  # opcional
evidencia = "Fonte que sustenta os fatos."
```

**Validação** (todos os erros de uma vez, `ErrosJogos`; teste de carga da
base embutida):

- TOML malformado, campo desconhecido ou ausente, `anticheat`, `config` ou
  código de orientação desconhecido → `ErroJogo::Toml`;
- `versao_schema` diferente de 1 → `VersaoSchema`;
- `id` fora do kebab-case (começa com `a-z`; só `a-z`, `0-9` e `-`; sem `-`
  no fim nem `--`) → `IdInvalido`; repetido → `IdDuplicado`;
- `nome` ou `evidencia` vazios, `executaveis` vazio ou com item vazio, item
  vazio em `processos_protegidos` → `CampoVazio`;
- o mesmo executável duas vezes, no jogo ou na base inteira (sem diferenciar
  maiúsculas) → `ExecutavelDuplicado`;
- orientação repetida no mesmo jogo → `OrientacaoRepetida`;
- `config = "edicao"` ou `"opcoes_inicializacao"` em `valorant`, `lol`,
  `marvel-rivals` ou `cod` (`JOGOS_SEM_EDICAO`, trava do escopo) →
  `ConfigProibida`.

**Casamento** (`BaseJogos::casar`): pelo nome do arquivo do exe (aceita
caminho; sem diferenciar maiúsculas). Se o jogo da base tem `steam_appid` e
quem chama informou um AppID **diferente**, não casa (exes genéricos como
`cs2.exe` de outro jogo); sem AppID de um dos lados, vale só o exe.

**Fatos curados** (`nvt_engine::fatos_do_jogo`): jogo casado tem
`competitivo = true`, `jogo_base = id`, as `orientacoes` da base, e
`reflex`, `limitador_nativo` e `anticheat_kernel` **vindos da base** (ela
vence a detecção em disco, que não acha Reflex via NVAPI). Fora da base:
fatos da detecção, `competitivo = false`, `limitador_nativo` desconhecido e
nenhuma orientação.

## Orientações e avisos do plano

- **Orientações por jogo** (`plan.resumos[id].orientacoes`): códigos da base
  (`Orientacao`), filtrados pelas preferências por `filtrar_orientacoes`, na
  ordem da base e sem repetição. Com `sincronismo = gsync`, `vsync_jogo_off`
  vira `vsync_driver_com_gsync` (V-Sync desligado no jogo e ligado no
  driver); sem G-SYNC, `vsync_driver_com_gsync` sai. Jogo fora da base: lista
  vazia. O núcleo não gera texto; a UI traduz os códigos.
- **Avisos** (`plan.avisos`, `AvisoPlano`), só com `sincronismo = gsync`:
  `GsyncSemVrr` quando `vrr.possivel = Some(false)` (sem VRR, a combinação
  vira V-Sync comum, com fila; a UI orienta “sem sincronismo”);
  `VrrDesligadoNoPainel` quando `possivel = Some(true)` e
  `ativo = Some(false)` (o app não liga o VRR do monitor). VRR desconhecido
  não gera aviso.

## Semântica do avaliador

1. **Alvos.** Regra `global` é avaliada uma vez, para o perfil global. Regra
   `por_jogo` é avaliada para cada jogo, gravando no perfil daquele jogo.
2. **Condições.** Regras cujas condições não casam são ignoradas.
3. **Teto de qualidade.** `Desempenho` aceita até `forte`; `Equilibrado`
   (padrão) até `leve`; `Fidelidade` só `nenhum`. Regras acima do teto vão
   para `plan.descartes` com o motivo.
4. **Status.** Só `estavel` entra no plano automático. `experimental` que casa
   aparece em `plan.experimentais` como sugestão, com as mudanças que faria.
5. **Conflitos** (mesmo alvo e mesma setting), em ordem:
   1. **preferência direta** do usuário (hoje: `limite_fps` → setting
      `limite_fps` no global) vence tudo;
   2. **regra que traduz preferência** (condicionada à preferência que governa
      a setting; ex.: `sincronismo-*` para V-Sync/G-SYNC) vence regra comum;
   3. **mais específica**: escopo por jogo > global; depois mais condições em
      `quando`;
   4. maior `prioridade`;
   5. menor `id` em ordem lexicográfica (desempate determinístico, independente
      da ordem nos arquivos).

   As perdedoras vão para `plan.descartes` ("suplantada por ...").
6. **Escolhas do usuário.** Setting travada (`Preferences::escolhas_usuario`,
   por alvo) nunca entra no plano; a mudança que seria feita fica em
   `plan.respeitadas`.
7. **Saída.** `plan.global` e `plan.por_jogo[id]` vêm ordenados por nome de
   setting. Cada `SettingChange` traz ID DRS, valor, nome do valor, origem
   (`Regra` ou `Preferencia`) e explicação em pt-BR. `valor_anterior` é
   preenchido pelo serviço ao ler o driver (rollback).

Entre perfis diferentes não há conflito: o perfil do jogo tem precedência
sobre o global no próprio driver.

## Explicação por jogo (`plan.resumos`)

Para cada jogo avaliado, `plan.resumos[id]` (`ResumoJogo`, módulo
`explicacao`) traz:

- `aplicadas`: regras por jogo que gravam no perfil, com as settings de cada
  uma (as que venceram e não estão travadas);
- `nao_aplicadas`: regras por jogo que ficaram de fora, com um **motivo
  estruturado** (`MotivoNaoAplicada`). Uma regra aplicada também aparece aqui
  se uma setting dela foi travada ou perdeu um conflito;
- `herda_do_global`: settings do plano global que o perfil do jogo não
  sobrescreve (a UI mostra “herda N ajustes globais”);
- `orientacoes`: códigos de orientação do jogo da base curada, já filtrados
  pelas preferências (ver “Orientações e avisos do plano”).

| Motivo | Quando |
|---|---|
| `JogoSemDlss` | condição de DLSS falhou num jogo sem DLSS (padrão ou declarado) |
| `ArquiteturaNaoPrecisa { arquitetura }` | declarado em `motivo_quando_nao_aplica` |
| `SemTensorCores` | `tem_tensor_cores = true` numa GPU sem Tensor Cores (padrão ou declarado) |
| `Experimental` | regra experimental que casou (só sugestão) |
| `BloqueadaPeloTetoDeQualidade { teto, impacto }` | impacto visual acima do teto |
| `EscolhaDoUsuario { setting }` | setting travada pelo usuário |
| `ConflitoComRegraMaisEspecifica { regra }` | outra regra (ou `preferencia:<nome>`) venceu a setting |
| `FatoDesconhecido { fato }` | a condição depende de um fato não detectado (ex.: `jogo_reflex`) |
| `CondicaoNaoAtendida { condicao, esperado, encontrado }` | qualquer outra falha |

Escolha do motivo quando a regra não casa: `Condicoes::falhas` lista as
condições que falharam em ordem fixa — **fatos do jogo** (`jogo_dlss`,
`jogo_dlss_min`, `jogo_api`, `jogo_reflex`, `jogo_anticheat_kernel`,
`jogo_competitivo`, `jogo_limitador_nativo`), depois `tem_tensor_cores`,
`tensor_fp8`, `arquitetura`, demais capacidades (terminando em
`refresh_conhecido`, `vrr_possivel`, `vrr_ativo`) e por fim preferências. Vence a primeira falha com motivo declarado ou padrão; se
nenhuma tiver, a primeira falha vira `FatoDesconhecido` (fato `None`) ou
`CondicaoNaoAtendida`. Assim, um jogo sem DLSS diz “jogo sem DLSS” em
qualquer GPU, e uma GTX 16 diz “sem Tensor Cores” antes de falar de
arquitetura. `casa` é exatamente `falhas().is_empty()` (teste
`falhas_e_casa_concordam`).

O núcleo não gera texto: a UI traduz cada motivo (`nvt-ui`, `textos.rs`).

O plano diz só o que gravar. Uma setting que o app já gravou e que deixou de
estar no plano (preferência trocada, teto de qualidade mais alto, regra que
deixou de casar ou foi removida) é revertida ao valor original pelo motor
(`docs/engine.md`, “Settings que saíram do plano”). Por isso cada opção de
uma preferência deve definir por inteiro as settings que ela governa quando o
valor original não serve: as três regras de sincronismo desligam ou ligam o
G-SYNC explicitamente (`sincronismo-vsync` desliga `gsync_global` e
`gsync_app_state`, como `sincronismo-sem-sync`); o que só uma opção usa (modo
de aplicação do G-SYNC, limite abaixo do refresh) volta ao original nas
outras.

## Regras base

| Regra | Status | Escopo | O que faz |
|---|---|---|---|
| `baseline-baixa-latencia-ultra` | estável | global | Low Latency Ultra |
| `energia-desempenho-maximo-por-jogo` | estável | por jogo | Jogo competitivo (`jogo_competitivo = true`): energia em desempenho máximo no perfil do jogo |
| `baseline-shader-cache` | estável | global | Shader cache ligado |
| `baseline-indicadores-desligados` | estável | global | Indicadores VRR/MFAA/triple buffering off |
| `baseline-filtragem-pela-aplicacao` | estável | global | AF/AA de volta à aplicação |
| `sincronismo-sem-sync` | estável | global | V-Sync force off, G-SYNC off |
| `sincronismo-gsync` | estável | global | G-SYNC tela cheia + janela, V-Sync on |
| `sincronismo-gsync-limite` | experimental | global | Com G-SYNC, refresh conhecido e VRR ativo: `limite_fps = { limite_vrr = true }` (224 a 240 Hz), só sugestão |
| `gsync-reflex-sem-limite-driver` | experimental | por jogo | Com G-SYNC, jogo competitivo com Reflex e sem limite do usuário: limitador do driver off no perfil do jogo, só sugestão |
| `sincronismo-vsync` | estável | global | V-Sync force on, G-SYNC off |
| `dlss-preset-e-turing` | estável | por jogo | RTX 20 + jogo com DLSS: preset E (impacto leve) |
| `dlss-preset-e-ampere` | experimental | por jogo | Idem para RTX 30, só sugestão |

Ada e Blackwell (FP8) e GPUs sem Tensor Cores (GTX 16) não recebem override
de DLSS. As duas regras de DLSS declaram
`arquitetura = { ada = "arquitetura_nao_precisa", blackwell = "arquitetura_nao_precisa" }`
em `motivo_quando_nao_aplica`, para a UI explicar por que nada foi aplicado
numa RTX 40/50.

**Energia só em jogos competitivos** (spec competitivo F1-11 e §11, item
19). A antiga `baseline-energia-desempenho-maximo` (global) saiu; o global
volta ao valor de antes da gestão pelo mecanismo de settings que saíram do
plano. A regra por jogo exige `jogo_competitivo = true`: um jogo fora da base
não recebe a energia e não ganha perfil só por isso, e a explicação dele
continua sendo a das outras regras (ex.: “Nenhum ajuste específico — jogo sem
DLSS”); a de energia aparece em `nao_aplicadas` como
`CondicaoNaoAtendida { condicao: "jogo_competitivo" }`, que tem a menor
relevância no resumo curto da UI. O `motivo` fala só em consistência de
clock, nunca em ganho de latência.

## Como contribuir com uma regra

1. Garanta que as settings existem em `regras/settings.toml`. Setting nova
   exige ID e valores com fonte (NVAPI `NvApiDriverSettings.h` ou NPI) — e
   entra na lista permitida do serviço, então justifique no PR.
2. Escreva a regra com `motivo` e `evidencia` (e, se uma condição-chave
   falhar por um motivo que o usuário precisa entender, `motivo_quando_nao_aplica`). Evidência boa é medição
   (PresentMon, mediana de várias capturas, comparada com a variação entre
   sessões) ou fonte oficial. **Não inclua dados pessoais**: nomes de
   usuário, caminhos da sua máquina, IDs de hardware.
3. Comece como `experimental`. Promova para `estavel` só com medição em pelo
   menos uma GPU da arquitetura alvo e sem regressão de estabilidade
   (função objetivo: estabilidade > latência > FPS).
4. Classifique o `impacto_visual` honestamente: qualquer mudança perceptível
   na imagem é pelo menos `leve`.
5. Prefira condições explícitas (ex.: `tem_tensor_cores = true` além da
   arquitetura) a depender de prioridade para vencer conflitos.
6. Adicione um teste em `crates/nvt-core/tests/` e rode
   `cargo test -p nvt-core` e `cargo clippy -p nvt-core --all-targets -- -D warnings`.
   O teste `carrega_regras_reais` falha com mensagem clara se o TOML for
   inválido.
