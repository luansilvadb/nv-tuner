# Arquitetura do nv-tuner

Este documento resume as decisões de arquitetura. Ele evolui com o projeto;
mudanças relevantes devem ser registradas aqui.

## Escopo

- Somente Windows e GPUs NVIDIA.
- Atua em **camadas de ajuste**: `driver` (perfis DRS, global e por
  aplicativo, via NVAPI com FFI própria em Rust, sem ferramentas externas nem
  .NET) e `windows` (registro do usuário, parâmetros do sistema e modo de
  vídeo, só os quatro itens da lista permitida de `regras/windows.toml`) e
  `jogo` (arquivos de configuração do usuário e opções de inicialização da
  Steam, só jogos e chaves da lista permitida de `regras/jogos.toml`; nunca a
  pasta de instalação).
- Cada camada tem snapshot do original, releitura após gravar, histórico e
  restauração. A camada `windows` usa um modelo paralelo
  (`nvt_core::usuario`) e não altera o estado DRS v1 (decisão D5 de
  `docs/spec/competitivo.md` §12).
- Regras experimentais (de qualquer camada; hoje todas as das camadas
  `windows` e `jogo`) só entram no plano com aceite explícito do usuário, por regra
  (D2).
- Não injeta código, não abre handle para processos de jogos ou de
  anticheats (jogos abertos são vistos só pela lista de processos,
  `TH32CS_SNAPPROCESS`, por nome), não altera executáveis nem arquivos da
  pasta de instalação dos jogos (só os lê, com compartilhamento total) e não
  altera recursos de segurança do sistema.
- Não grava no DRS, no Windows nem em arquivo de jogo enquanto detecta jogo ou anticheat vigiado
  em execução: a gravação fica adiada e roda quando o jogo fecha (portão
  único de gravação, `crates/nvt-engine/src/portao.rs`, usado pelo serviço e
  pelo agente).
- Não grava no BCD nem altera TPM, Secure Boot ou VBS/HVCI; esses itens só
  aparecem como diagnóstico (D1).

## Camadas

1. **`nvt-drs`** — FFI da NVAPI DRS. Único lugar com `unsafe` ligado ao
   driver: sessão DRS, leitura e gravação de configurações e perfis.
2. **`nvt-detect`** — detecção de GPU e capacidades; varredura de bibliotecas
   de jogos cruzada com o catálogo de perfis do DRS.
3. **`nvt-core`** — domínio puro e testável: capacidades, fatos, preferências,
   regras, avaliador e plano de configuração.
4. **`nvt-telemetry`** — medição passiva via PresentMon CLI (a partir da v0.2).
5. **`nvt-ipc`** — protocolo versionado (serde) do named pipe.
6. **`nvt-service`** — serviço do Windows com privilégios; **único avaliador**
   e único componente que grava no driver. Coordena os agentes por SID e
   sessão (`src/usuario.rs`) e coleta os diagnósticos de máquina.
7. **`nvt-windows`** — fachadas dos ajustes do usuário (`registro.rs`,
   `token.rs`, `mouse.rs`, `tela.rs`) e diagnósticos somente leitura
   (`diagnosticos.rs`, cuja interface não tem operação de escrita).
8. **`nvt-agente`** — processo interativo sem privilégios, um por sessão
   (mutex `Local\nv-tuner-agente`), iniciado no logon pela entrada `Run` de
   HKLM. Executa as tarefas da camada `windows` que o serviço planeja,
   guarda snapshot e estado em `%LOCALAPPDATA%\nv-tuner\usuario` e lê os
   diagnósticos de sessão (decisão D4). Também grava a camada `jogo`
   (`src/config_jogo/`; ver "Camada de configuração de jogo").
9. **`nvt-jogocfg`** — editores de arquivos de configuração de jogo, sem
   lógica de decisão (ver "Camada de configuração de jogo").
10. **`nvt-ui`** — interface sem privilégios de administrador. Telas em
    `src/ui/` (Início, Jogos, Avançado, boas-vindas e, só no build de teste,
    Partidas); `modelo.rs` tem as funções puras `veredito`, `linha_estado` e
    `status_jogo`; `demonstracao.rs` dá o modo `--demonstracao` (dados
    fictícios, sem serviço); `abrir.rs` é a exceção auditada de
    `ShellExecuteW` (ver "Segurança do serviço").
11. **`nvt-manutencao`** — limpeza segura de temporários
    (`docs/spec/interface-e-saude.md` §4.4). A passada é pura e genérica sobre
    `trait Excluidor`; sistema de arquivos, relógio, portão de jogo, detecção de
    instalação (mutex `Global\_MSIExecute`) e política diária são injetados, e o
    chamador sempre passa alvo e raiz. O modo é a constante de compilação
    `MODO_LIMPEZA`: `Desligada` no build normal (não percorre nada) e
    `Simulacao` com a feature `build-de-teste` (usa `ExcluidorNulo`, sem acesso
    a handle de exclusão). Só `src/windows.rs` abre arquivo com `DELETE`.
12. **`nvt-medicao`** — gravação de partidas, **só no build de teste** (feature
    `medicao-partidas`). Sem a feature, o crate traz só o núcleo puro
    (agregação de Presents, engasgo, 1% low, histograma, resumo e formato de
    arquivo); com ela, `etw.rs` (sessão ETW em tempo real `nv-tuner-partidas`,
    só provedores de Present DXGI e D3D9), `nvml.rs` (uso e temperatura da GPU a
    1 Hz, `nvml.dll` por caminho absoluto) e `processos.rs`
    (`TH32CS_SNAPPROCESS`, nenhum handle para o jogo).

### Saúde do PC

Leituras **somente leitura** em `nvt-windows/src/saude/` (inicialização e
sinais), por traits sem métodos de escrita:

- **Agente** (`nvt-agente/src/saude.rs`): fontes de inicialização do usuário,
  Storage Sense (HKCU) e limpeza de `%TEMP%`. A linha de base da inicialização
  (só nomes e o SHA-256 do comando, que nunca sai do agente) e a política
  diária da limpeza ficam no estado do agente (`EstadoSaude`). Relata em
  `AgenteRelatar.saude`.
- **Serviço** (`nvt-service/src/saude.rs`): thread própria que lê os sinais de
  máquina (espaço, memória, CPU em repouso, driver, plano de energia) a cada
  10 min e a inicialização da máquina a cada 6 h, fora do atendimento do pipe;
  e a limpeza de `C:\Windows\Temp` (portão de jogo antes e durante, mutex MSI,
  política diária em `manutencao.json`, exclusão com o ciclo DRS e com a
  atualização). `ObterSaude` junta a máquina e o relato da sessão do chamador.
- Programas de inicialização são só detectados e mostrados; nunca
  desativados, editados ou removidos.
- "Aplicar melhorias recomendadas" fica em `nvt-service/src/recomendadas.rs`.

### Build de teste

A feature `medicao-partidas` (em `nvt-service`, `nvt-agente` e `nvt-ui`) liga
`nvt-medicao` e a feature `build-de-teste` de `nvt-manutencao` (limpeza em
`Simulacao`). Trava tripla: os `build.rs` de `nvt-medicao` e `nvt-manutencao`
exigem `NVT_BUILD_DE_TESTE=1` e recusam `CI`/`GITHUB_ACTIONS`, o `release.yml`
falha se a feature aparecer em `cargo tree -e features`, e o binário mostra
"Build de teste". Só nesse build o serviço abre a sessão ETW, e só enquanto há
partida ativa; ela é parada ao fim da partida, no stop do serviço e num
pânico. Gerado por `scripts\build-teste.ps1`; ver
`docs/spec/interface-e-saude-laboratorio.md`.

O serviço não grava no Windows: o HKCU e as APIs de modo de vídeo e de
parâmetros do sistema são da sessão interativa, e o serviço roda na sessão
0. Por isso a gravação da camada `windows` fica no agente, que não decide
nada: recebe ações tipadas, confere o portão, grava, relê e relata.

O UAC é solicitado apenas na instalação (registro do serviço). No uso diário a
UI não precisa de elevação.

O serviço é registrado com o nome interno **`nvt-service`** e o nome de
exibição **“nv-tuner (serviço)”** (o que aparece no `services.msc`).

### Privilégio de gravação no DRS (descoberta)

No driver **621.40** (branch `r621_20`), gravar no DRS pela NVAPI
(`NvAPI_DRS_SaveSettings`) **funcionou num processo de integridade média, sem
administrador**: a escrita é feita pelo serviço do driver
(`NVDisplay.ContainerLocalSystem`), não pelo processo chamador. Detalhes e
procedimento em `docs/pesquisa/nvapi-drs.md`, seção 9.

Isso **não é garantido em outros drivers** (o esperado era a recusa `-137` sem
admin), então o app não conta com essa permissão nem a usa como barreira: a
política de quem grava fica no próprio app, e só o serviço grava. O serviço
continua justificado por:

- **medição passiva**: a captura ETW (PresentMon) exige administrador ou o
  grupo *Performance Log Users*;
- **reaplicação sem a UI aberta** (após atualização de driver, novo jogo etc.);
- **vigilância de deriva** contínua do DRS.

### Camada de configuração de jogo (Fase 3)

- **Quem decide:** o serviço avalia as regras `config_jogo` (todas
  experimentais, só com Aceitar) com as preferências do usuário e coordena por
  SID (`crates/nvt-service/src/usuario.rs`). **Quem grava:** o agente da
  sessão (`crates/nvt-agente/src/config_jogo/`: `armazem.rs`, `execucao.rs`,
  `janela.rs`), que resolve as pastas conhecidas da sessão (Documentos,
  inclusive no OneDrive, `%LOCALAPPDATA%` e a Steam por
  `HKCU\Software\Valve\Steam\SteamPath`).
- **O que o agente grava hoje:** R6 `[DISPLAY] FPSLimit` (30 a 360); token
  `+fps_max` em `LaunchOptions` do `localconfig.vdf` para CS2 (60 a 360, nunca
  0) e Dota 2 (60 a 240); `OneFrameThreadLag` do Rocket League. Chaves
  `a_confirmar` (Fortnite `FrameRateLimit`, Rocket League Max FPS, Deadlock
  `+fps_max`, Reflex) nunca são gravadas.
- **`nvt-jogocfg`:** editores INI e KeyValues/VDF que trocam só os bytes do
  valor (UTF-8 com ou sem BOM, UTF-16LE, fim de linha, comentários e ordem
  preservados); chave ausente ou formato não reconhecido por inteiro aborta
  sem gravar. Gravação atômica (temporário na mesma pasta + `ReplaceFileW`),
  sem seguir reparse point. Backup imutável antes da primeira gravação em
  `%LOCALAPPDATA%\nv-tuner\backups\<jogo>\<arquivo>.orig`, com
  `.orig.sha256`. Contas: Steam pela conta `MostRecent` de `loginusers.vdf`
  (SteamID3); Ubisoft pela única pasta de conta ou pela do `GameSettings.ini`
  modificado mais recentemente (empate: `ContaAmbigua`).
- **Janela de edição (F3-03):** grava só com o jogo, os processos protegidos
  e, nos arquivos da Steam, o `steam.exe` fechados; senão adia. Nunca fecha
  processo.
- **Recusas:** arquivo somente leitura (`ArquivoSomenteLeitura`; o app nunca
  muda o atributo), par em `remote` da Steam Cloud (`ArquivoNaNuvem`), conta
  ambígua e arquivo regravado pelo jogo (`ArquivoAlteradoPeloJogo`: vira
  escolha do usuário e não é regravado em loop).
- **Restauração (F3-07):** se o hash atual é o gravado pelo app, volta o
  arquivo original inteiro; senão, reverte só as chaves que ainda têm o valor
  do app; nas opções de inicialização, remove só o token do app.
- **Limite no jogo (F3-13):** o `limite_fps` do usuário vai para o jogo, e o
  perfil DRS daquele jogo recebe `limite_fps = off` só quando o limite no jogo
  está aplicado (detalhes em `docs/servico.md`).
- **Travas:** jogos `proibido` (VALORANT, LoL, Marvel Rivals, CoD) e
  `orientar` (Apex, OW2, PUBG, THE FINALS) não podem ter `[[arquivo]]` nem
  regra (erro de carga). Protocolo **v7**.

## Fluxo

```
detecção ──► fatos + capacidades ──► regras (TOML) + preferências
                                              │
                                              ▼
                                          avaliador
                                              │
                                              ▼
                                     plano de configuração
                                              │  (named pipe)
                                              ▼
                                 serviço: valida ─► aplica ─► registra
                                              │
                                              ▼
                                telemetria passiva (v0.2) ─► validação/rollback
```

1. **Detecção** levanta a GPU, o driver e os jogos instalados.
2. Os dados viram **fatos** (por jogo) e **capacidades** (por GPU) tipados.
3. O **avaliador** aplica as **regras** declarativas sobre fatos, capacidades e
   preferências.
4. O resultado é um **plano de configuração**: lista auditável de alterações
   com valor anterior, valor novo e regra de origem.
5. O **serviço** calcula o plano, valida e aplica, guardando o histórico
   para rollback; a UI só pede ações pelo named pipe.
6. Na camada `windows`, o **agente** envia os fatos da sessão
   (`agente_consultar`), recebe as tarefas do plano do seu usuário, executa
   sob o portão, relê e confirma com `agente_relatar`.

## Modelo de decisão

Modelo híbrido:

- **Regras** declarativas são a base de toda decisão.
- **Medição passiva** (PresentMon CLI) confirma se o plano melhorou ou piorou.
- **Testes A/B** automáticos apenas em alavancas classificadas como de baixo
  risco.

### Função objetivo

Ordem de prioridade: **estabilidade > latência > FPS**. Uma regra que ganhe
FPS às custas de estabilidade perde para uma regra conservadora. A qualidade
visual é limitada por um teto escolhido pelo usuário (padrão: "Equilibrado").

## Política de autonomia

- O app pode aplicar planos automaticamente, desde que toda alteração seja
  registrada e reversível (rollback por jogo e global).
- Regressões detectadas pela telemetria disparam rollback para o último estado
  bom.
- O estado real do driver é sempre relido após aplicar, em vez de presumir que
  a gravação funcionou.

## Mudanças manuais

Quando o usuário altera uma configuração manualmente (no app ou em outra
ferramenta), ela passa a ser uma **escolha do usuário**: o avaliador não a
sobrescreve nas execuções seguintes, até que o usuário a libere
explicitamente.

## Segurança do serviço

- O serviço só aceita conexões locais no named pipe, com ACL restrita.
- Toda requisição é validada contra uma **lista permitida de IDs de
  configuração DRS**; IDs fora da lista são rejeitados, mesmo que venham de
  uma regra válida.
- Valores são validados por tipo e faixa antes da gravação.
- **Nenhum componente executa processos externos** (nem `powercfg.exe`,
  `bcdedit.exe`, `reg.exe`, `cmd.exe`): toda ação usa APIs Win32/NVAPI
  diretamente. Exceções: o instalador e a **atualização automática**, abaixo.
- **Exceção auditada (atualização automática).** Um único arquivo de produção,
  `crates/nvt-service/src/atualizacao/lancador.rs`, cria processos, e só dois:
  - o `msiexec.exe` do upgrade: caminho montado por `GetSystemDirectoryW` +
    `\msiexec.exe` em `lpApplicationName` (nunca o PATH), linha de comando fixa
    `/i "<msi>" /qn /norestart /l*v "<log>"`, caminhos só da pasta protegida com
    nome validado, `bInheritHandles = FALSE`, diretório atual System32, ambiente
    mínimo e `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP |
    CREATE_UNICODE_ENVIRONMENT` (mais `CREATE_BREAKAWAY_FROM_JOB` se o serviço
    estiver num job); o serviço não espera o processo;
  - o relançamento do `nvt-agente.exe` na sessão do usuário depois do upgrade:
    `WTSQueryUserToken` + `CreateEnvironmentBlock` + `CreateProcessAsUserW`, só o
    executável fixo da pasta de instalação (filha de `%ProgramFiles%`, sem
    reparse), só nas sessões gravadas antes do upgrade e com o mesmo SID, uma vez.

- **Exceção auditada (abrir inicialização).** `crates/nvt-ui/src/abrir.rs` tem
  o único `ShellExecuteW` do workspace, com o URI literal
  `ms-settings:startupapps` (nenhum argumento vem de dados). Só o tratador de
  clique o chama, e ele recusa com jogo ou anticheat aberto ou com o estado do
  serviço desconhecido. Quem cria o processo é o shell, na sessão e com o token
  do usuário; serviço e agente continuam sem nenhuma ocorrência.
- **Sessão ETW** (só no build de teste): `StartTraceW`, `EnableTraceEx2`,
  `OpenTraceW`, `ProcessTrace` e `ControlTraceW` só em
  `crates/nvt-medicao/src/etw.rs`; `SystemTraceControlGuid`,
  `KERNEL_LOGGER_NAME` e `EtwMaxLoggers` proibidos em todo lugar.
- **Exclusão de arquivos** fora do estado só na limpeza de temporários
  (desligada no build normal): o serviço só em `C:\Windows\Temp`, o agente só
  no `%TEMP%` do usuário; `DeleteFileW`, `RemoveDirectoryW` e
  `FileDispositionInfoEx` são vigiados com contagem.

  `crates/nvt-core/tests/proibicoes.rs` proíbe `CreateProcess*`, `ShellExecute*`,
  `WinExec` e `Command::new` em todo o código e permite exatamente uma ocorrência
  de `CreateProcessW` e uma de `CreateProcessAsUserW` em `lancador.rs` (contagem a
  mais **ou a menos** falha). Alternativa rejeitada: `MsiInstallProduct` dentro do
  serviço, que trava com o `StopServices` do próprio pacote
  (`docs/spec/atualizacao.md` A6).
- O serviço **grava** apenas no DRS (via NVAPI) e nos seus diretórios de
  estado (inclusive `usuarios.json`, com aceites e recibos por SID, e a subpasta
  `atualizacao\`, só SYSTEM/Administradores). Nunca
  abre arquivos do perfil do usuário em nome do agente.
- O agente **grava** só no perfil do próprio usuário: o DWORD
  `HistoricalCaptureEnabled`, o token `SwapEffectUpgradeEnable` dentro de
  `DirectXUserGlobalSettings` (preservando os demais tokens), o trio SPI do
  mouse e o modo de vídeo por `SetDisplayConfig` (mesma topologia, resolução
  e profundidade), além do seu estado local.
- A identidade de quem chama o pipe vem do token do cliente; SID ou sessão no
  corpo da mensagem nunca dão autoridade.
- O serviço **lê** arquivos fora do seu estado só para detecção: pastas de
  instalação dos jogos (executáveis, com compartilhamento total
  `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`, handle curto,
  leitura para buffer e sem mapeamento) e os manifestos das bibliotecas de
  jogos. A pasta de um jogo em execução não é lida: valem os fatos da
  varredura anterior.

## Distribuição

Instalador MSI publicado em GitHub Releases com hash SHA-256 para conferência.
O MSI instala os três executáveis (`nvt-service`, `nvt-ui`, `nvt-agente`)
juntos, sem negociação de versão do protocolo. Builds de desenvolvimento
são sem assinatura; a assinatura de código dos executáveis e do MSI é
pré-requisito de lançamento da Fase 2 (D3) e ainda está pendente.

### Atualização automática

Especificação: [`docs/spec/atualizacao.md`](spec/atualizacao.md); detalhes do
serviço em [`docs/servico.md`](servico.md#atualização-automática). Obrigatória,
sem opção de desligar, nunca reinicia o Windows.

```
release.yml (tag v*, Environment release)
  MSI ─► nvt-assinar assinar ─► atualizacao.json(.minisig) + msi.minisig
      ─► rascunho ─► baixa e verifica ─► CI verde na main ─► Latest

serviço (LocalSystem), thread nvt-atualizacao
  5–15 min após iniciar, depois 6 h ± 30 min
  WinHTTP (proxy do sistema, só hosts do GitHub) ─► manifesto + assinatura
  ─► Minisign com a chave embutida (instalador/nv-tuner-minisign.pub)
  ─► versão > instalada e ≥ maior vista ─► MSI para %ProgramData%\nv-tuner\atualizacao
  ─► tamanho + SHA-256 pelo handle ─► handle só de leitura mantido
  ─► reserva contra o ciclo DRS + portão de processos (jogo aberto adia)
  ─► persiste "instalando" ─► lancador: msiexec /qn /norestart, sem esperar
msiexec: Restart Manager fecha agente/UI ─► para o serviço ─► troca arquivos ─► inicia o serviço
serviço novo: lê o código no log /l*v ─► registra ─► relança o agente ─► IPC v6 informa a UI
```

A chave pública vem de `instalador/nv-tuner-minisign.pub` por `include_str!`;
enquanto o arquivo tiver o marcador provisório, toda verificação falha fechada e o
workflow de release se recusa a publicar.
