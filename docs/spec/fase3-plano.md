# Plano de execução da Fase 3

O escopo é a configuração de jogo da [especificação competitiva](competitivo.md) (§9), sob as regras anticheat de §3, a decisão de §6.2 (agente do usuário) e as decisões D1–D6. O serviço LocalSystem continua sendo o **único avaliador** e não lê nem grava arquivos no perfil do usuário. O **agente** resolve caminhos, lê, faz backup, grava e restaura os arquivos de configuração dos jogos com os direitos do próprio usuário. A UI só apresenta e encaminha pedidos. Nenhum componente fecha, abre ou inspeciona processos (`crates/nvt-core/tests/proibicoes.rs` continua valendo: sem `Command::new`, sem `OpenProcess`, só `TH32CS_SNAPPROCESS`).

## Decisões desta fase

| ID | Decisão |
|---|---|
| D7 | Escritas de config de jogo só no agente; o serviço planeja e registra. |
| D8 | Portão duplo antes de **cada** escrita (aplicação, deriva, retirada, reversão individual, restauração, desinstalação): portão de processos atual (S1) **e** janela fechada do destino (jogo, `processos_protegidos` do jogo e, para arquivos da Steam, `steam.exe`) (F3-03). Falha em qualquer parte adia; o agente nunca fecha processo. |
| D9 | Todas as regras da Fase 3 entram como `experimental` e só vão ao plano com Aceitar do usuário (D2), por SID. Nenhuma regra de config de jogo é estável nesta fase, mesmo após laboratório; a promoção é decisão de uma versão seguinte. |
| D10 | Modelo **paralelo dentro da camada usuário** (justificativa abaixo). |
| D11 | IPC v7, sem negociação com v6; o instalador atualiza tudo junto (como D6). |
| D12 | Amostras de teste **sintéticas**, escritas do zero no repositório. Proibido copiar arquivos reais da máquina (contêm SteamID, nomes de conta, caminhos com nome de usuário). Um teste varre `crates/nvt-jogocfg/tests/amostras/` contra padrões de SteamID64 real (`7656119…` fora da faixa de teste reservada), e-mails e `C:\Users\<nome>` diferente de `Usuario`. |

### D10 — como estender a camada usuário

Opções consideradas:

1. **Novas variantes em `SettingUsuario`** (`R6FpsLimit`, …). Rejeitada: o enum é `Copy`, fechado, serializado em `snake_case` e usado como chave de `BTreeMap` em `FatosUsuario.gestao`, no estado do agente e no IPC v6; destinos de jogo dependem de instância (conta Steam, conta Ubisoft, app id), o que forçaria variantes com dados e quebraria a serialização como chave de mapa JSON e todos os `match` exaustivos da Fase 2.
2. **Destino genérico `DestinoUsuario { Windows(SettingUsuario), Jogo(..) }` substituindo as chaves atuais.** Rejeitada: obriga migrar estado do agente, `usuarios.json` e todo o ciclo já validado em laboratório (L2-*), justamente o que D5 evitou.
3. **Escolhida: família paralela no mesmo coordenador.** `nvt_core::config_jogo` define `SettingJogo` (enum fechado e `Copy` das chaves do catálogo), `InstanciaJogo` (jogo + conta resolvida + arquivo), `DestinoJogo { setting, instancia }` com forma textual canônica estável (`"r6_fps_limit@<hash-da-conta>"`) usada como chave de mapa, `ValorJogo` (`Ausente`, `Inteiro(i64)`, `Booleano`, `Texto` restrito, `TokenLancamento(Option<u32>)`), `GestaoJogo` (antes, esperado, `hash_antes`, `hash_depois`, id do backup) e `FatosConfigJogo`. `FatosUsuario`, `PlanoUsuario` e o estado do agente ganham campos `jogo: …` com `#[serde(default)]`: um estado v6 lido pela Fase 3 carrega com mapa vazio, sem migração, e nenhum `match` de `SettingUsuario` muda. Snapshot, gestão reidratada pelo agente, travas, deriva, aceite, reversão individual, geração de restauração e histórico reaproveitam os mesmos mecanismos (funções genéricas sobre um traço `Destino` extraído do código da Fase 2 **só** onde a extração é mecânica e coberta pelos testes existentes; o resto fica duplicado de propósito). Menor risco: o caminho Windows não muda de comportamento e os testes da Fase 2 continuam passando sem edição.

## Arquitetura

### Crates e responsabilidades

| Onde | O quê |
|---|---|
| `crates/nvt-jogocfg` (novo, sem dependência de `nvt-core`) | `ini.rs`: tokenizador INI que guarda cada linha como fatias do buffer original (seção, chave, separador, valor, comentário, fim de linha); `definir(secao, chave, novo) -> Result<Vec<u8>>` troca só os bytes do valor. `vdf.rs`: KeyValues texto (aspas, escapes, `//`, blocos aninhados, chaves duplicadas detectadas) com o mesmo contrato por caminho de chaves (`UserLocalConfigStore/Software/Valve/Steam/apps/730/LaunchOptions`). `codificacao.rs`: detecta UTF-8 com/sem BOM e UTF-16LE com BOM; outra codificação ou mistura → `FormatoInesperado`; regrava na mesma codificação. Chave ausente → `ChaveAusente` (não cria; mapeado a `FormatoInesperado` no motivo). Arquivo não reconhecido por inteiro → aborta. `lancamento.rs`: edição por token de `LaunchOptions` (acrescenta/troca só `+fps_max <n>`; preserva o resto; recusa e nunca emite `-allow_third_party_software`; remoção só do token do app ou devolução do original se a string não mudou desde a gravação). `arquivo.rs` (único `unsafe` do crate, fachada Win32): leitura com compartilhamento total e handle curto (S3); abertura com `FILE_FLAG_OPEN_REPARSE_POINT`, recusa se o alvo ou qualquer componente sob a raiz conhecida for link/junção; `FILE_ATTRIBUTE_READONLY` → `ArquivoSomenteLeitura` (nunca marca nem desmarca, N13/F3-06); gravação em temporário na mesma pasta + `FlushFileBuffers` + `ReplaceFileW` (F3-05). `backup.rs`: cópia `.orig` imutável em `%LOCALAPPDATA%\nv-tuner\backups\<jogo>\<arquivo-ou-hash>.orig` + `.sha256`, criada uma única vez (falha se já existe com hash diferente), sem seguir reparse (F3-04). `restauracao.rs`: decide entre devolver o original inteiro (hash atual = `hash_depois`) ou reverter só as chaves que ainda têm o valor do app; chave mudada pelo jogo → `ArquivoAlteradoPeloJogo`, sem mexer (F3-07). `steam.rs`: `loginusers.vdf` → conta `MostRecent = "1"`, SteamID3 = SteamID64 − 76561197960265728 (F3-10); detecção `local` × `remote` de `cs2_video.txt` → `ArquivoNaNuvem` (F3-09). `ubisoft.rs`: escolha da pasta de conta por mtime do `GameSettings.ini`, empate → `ContaAmbigua` (F3-12). `sha256` via crate `sha2` (dependência nova, revisada). |
| `nvt-core::config_jogo` (novo módulo) | Tipos de D10; carga e validação da seção `[[arquivo]]` de `regras/jogos.toml` (schema 2): jogo, arquivo lógico, raiz (`documentos`, `local_app_data`, `steam_userdata`), caminho relativo com marcadores (`<conta>`, `<steamid3>`, `<appid>`), formato (`ini`/`vdf`/`lancamento`), seção/caminho de chave, tipo, `valores`/`faixa` **iguais ao menu** (F3-02), `processos_fechados`, `exige_steam_fechada`. Regras de `config_jogo` com condições `jogo`, `limite_fps_definido`, `refresh_conhecido`, `arquivo_local`. Avaliação: valor fora de `valores`/`faixa` é descartado; arredondamento para baixo ao valor do menu (Fortnite); `min(refresh, 240)` só em `jogo-dota2-fps-max-padrao`. Trava de base: jogo com `orientar`/`proibido` (Valorant, LoL, Marvel Rivals, CoD, Apex, OW2, PUBG, The Finals) não pode ter `[[arquivo]]` nem regra `config_jogo` — erro de carga. Recusa caminhos que apontem para a pasta de instalação (N8) e qualquer nome do §9.1 “fora” (`autoexec.cfg`, `video.txt`, `boot.vcfg`, `gameinfo.gi`). |
| `nvt-core::rules` / avaliador DRS | F3-13: quando uma regra de limite no jogo está **no plano do SID da console** (aceita e com destino resolvido pelo agente), o perfil DRS daquele jogo recebe `limite_fps = off`; sem isso, nada muda no DRS. Se o agente relatar falha/adiamento do arquivo, o DRS **mantém** o limite do driver (nunca fica sem limitador algum). Nunca empilha limitadores. |
| `nvt-agente::config_jogo` (novo módulo e pasta) | `ArmazemConfigJogo` sobre `nvt-jogocfg`: resolve raízes com `SHGetKnownFolderPath` (Documentos/OneDrive, LocalAppData) e `HKCU\Software\Valve\Steam\SteamPath`; produz `FatosConfigJogo` (instâncias resolvidas, valores atuais, `somente_leitura`, `na_nuvem`, `conta_ambigua`, hashes) sem expor caminhos completos ao serviço (só o rótulo `%USERPROFILE%\…`). `janela.rs` estende o portão: `JanelaFechadaExigida` se algum processo do destino estiver na lista. Ciclo: mesma sequência da Fase 2 (intenção + snapshot/backup persistidos antes, escrita, releitura, persistência do resultado e relato), deriva na janela fechada seguinte vira trava (F3-08), restauração por geração. |
| `nvt-service::usuario` | Coordenador ganha a família jogo: guarda gestão/travas/aceites por `DestinoJogo`, gera tarefas tipadas, valida relatos (destino pertence ao catálogo, valor aceito por `SettingJogo`), alimenta F3-13 para o avaliador DRS e o histórico. Nunca abre arquivo do perfil. |
| `nvt-ipc` v7 | `AlvoIpc::ConfigJogo { jogo, arquivo, setting }`; `FatosConfigJogoIpc` em `AgenteConsultar`; ações `AcaoJogo { destino, operacao, valor, hash_esperado }` na resposta; relato com `hash_antes`/`hash_depois`/chaves alteradas; motivos `JanelaFechadaExigida`, `FormatoInesperado`, `ArquivoSomenteLeitura`, `ArquivoNaNuvem`, `ArquivoAlteradoPeloJogo`, `ContaAmbigua`, `ArquivoAusente`, `ReparseRecusado`; `ReverterAjusteUsuario` aceita destino de jogo. v6 é rejeitada antes de desserializar o corpo. DTOs de fio independem do núcleo. |
| `nvt-ui` | Cartão do jogo: “Arquivo do jogo” (caminho anonimizado), antes/depois, estado do backup, Aceitar/Retirar, Reverter; mensagens “feche o jogo/a Steam para aplicar”, “desmarque somente leitura” (F3-06), “arquivo na nuvem” e conta escolhida (R6). |

### Fluxo serviço ↔ agente (config de jogo)

1. Agente (conexão curta, logon/mudança/60 s) lê arquivos com compartilhamento total e envia `FatosConfigJogo` + estado das tarefas em `AgenteConsultar`.
2. Serviço valida forma e catálogo, avalia regras `config_jogo` com os aceites do SID, calcula vencedores e devolve ações tipadas (`aplicar`, `reverter`, `restaurar` por geração) e o conjunto vigiado (exes da base, anticheats, `processos_protegidos`, `steam.exe`).
3. Agente, fora da conexão e sob a trava do SID: revalida revisão/geração → portão de processos → janela fechada do destino → backup (1ª vez) → persiste intenção → relê e confere `hash_esperado` → edita bytes → `ReplaceFileW` → relê e confere valor e hash → persiste → `AgenteRelatar`.
4. Serviço registra no histórico (`origem = agente`, SID), atualiza gestão e, se for limite no jogo aplicado, reavalia o DRS sob o portão (F3-13).
5. Qualquer passo com jogo/Steam aberto, formato inesperado ou divergência de hash → adia ou trava; nunca regrava em loop.

## Lotes sequenciais

Cada lote começa após todos os critérios do anterior. Caminhos relativos à raiz; diretório reserva toda a árvore à tarefa naquele lote. Leitura cruzada é permitida; arquivos de escrita são disjuntos no mesmo lote. L1 fixa as assinaturas usadas em L2. `Cargo.toml` da raiz e `Cargo.lock` ficam com um único responsável (L1-a para registrar o crate novo; L3 para a integração final).

### L1 — contratos, editores e catálogo

| Tarefa | Arquivos de escrita | Pronto quando |
|---|---|---|
| L1-a Crate `nvt-jogocfg` (editores e arquivo) | `crates/nvt-jogocfg/`, `Cargo.toml` (membro do workspace), `Cargo.lock` | Ida e volta byte a byte em todas as amostras sintéticas (UTF-8 com/sem BOM, UTF-16LE, CRLF/LF misto rejeitado ou preservado conforme o caso, comentários, espaços, chaves duplicadas); só os bytes do valor mudam (diff de bytes testado); chave ausente e formato irreconhecível não gravam; token: `+fps_max` acrescentado/trocado/removido preservando o resto, trava que recusa `-allow_third_party_software` em qualquer entrada; junção e link recusados (modelo `crates/nvt-service/src/diretorio.rs:747-796`); somente leitura recusado sem alterar atributos; `ReplaceFileW` preserva ACL/atributos; backup imutável com SHA-256 e recusa de sobrescrita; restauração por hash e por chaves com `ArquivoAlteradoPeloJogo`; `loginusers.vdf` sintético → SteamID3; `local`/`remote`; conta Ubisoft única/várias/empate. Teste de D12 sobre as amostras. |
| L1-b Núcleo: tipos, catálogo e regras | `crates/nvt-core/src/config_jogo.rs`, `crates/nvt-core/src/lib.rs`, `crates/nvt-core/src/usuario.rs` (só campos `jogo` com `serde(default)`), `crates/nvt-core/src/jogos.rs`, `crates/nvt-core/src/rules.rs`, `crates/nvt-core/tests/config_jogo.rs`, `regras/jogos.toml` (schema 2: estrutura de `[[arquivo]]` e regras de §9.3 com chaves **parametrizadas** marcadas `a_confirmar = true`) | Carga da base real; testes rejeitam chave fora do catálogo, valor fora do menu, caminho na pasta de instalação, nomes proibidos e `[[arquivo]]`/regra em jogo `orientar`/`proibido` (critério 9.5.4); regra `a_confirmar` nunca entra no plano; todas exigem aceite; F3-13 (DRS off só com limite no jogo no plano; sem limite do usuário nada é criado, exceto Dota 2 padrão `min(refresh,240)`); arredondamento Fortnite; estado/fatos v6 desserializam sem a família jogo; testes da Fase 2 passam sem edição. |
| L1-c IPC v7 | `crates/nvt-ipc/` | Envelope recusa v6; DTOs, alvos e motivos novos com ida e volta JSON; limites de tamanho para fatos de jogo; caminhos completos não cabem no DTO (só rótulo anonimizado); testes de identidade da v6 mantidos. |

### L2 — implementações em paralelo, após L1

| Tarefa | Arquivos de escrita | Pronto quando |
|---|---|---|
| L2-a Catálogo de chaves (consome `docs/pesquisa/fase3-confirmacoes.md`) | `regras/jogos.toml` | Cada `[[arquivo]]` tem chave, seção, tipo e `valores`/`faixa` com fonte citada na pesquisa; só remove `a_confirmar` do que a pesquisa confirmou; o resto fica marcado (ex.: Reflex do CS2, Max FPS do RL) e fora do plano; carga e testes de L1-b passam sem mudar código. Se a pesquisa não estiver pronta, a tarefa espera sem bloquear L2-b..e. |
| L2-b Agente: armazém e ciclo de config de jogo | `crates/nvt-agente/src/config_jogo.rs`, `crates/nvt-agente/src/config_jogo/`, `crates/nvt-agente/src/config_jogo_testes.rs`, `crates/nvt-agente/src/lib.rs`, `crates/nvt-agente/Cargo.toml` | Com sistema de arquivos e lista de processos falsos: portão duplo antes de cada escrita (inclusive retirada, reversão, restauração, desinstalação), `steam.exe` aberto bloqueia só destinos da Steam, falha de enumeração bloqueia; backup antes da 1ª escrita; intenção persistida; crash entre temporário e `ReplaceFileW` recupera sem dupla aplicação; deriva vira trava; restauração por geração idêntica por hash; somente leitura/nuvem/conta ambígua relatados sem escrita; nenhum caminho com nome do usuário no relato; compila com `proibicoes.rs`. |
| L2-c Agente: integração ao ciclo existente | `crates/nvt-agente/src/ciclo.rs`, `crates/nvt-agente/src/estado.rs`, `crates/nvt-agente/src/windows/janela.rs` | Ciclo chama o armazém de jogo depois do Windows sob a mesma trava e geração; estado do agente v6 carrega; testes existentes de `ciclo_testes.rs` passam sem edição; janela fechada usa o conjunto vigiado do serviço unido à base local. |
| L2-d Coordenador no serviço | `crates/nvt-service/src/usuario.rs`, `crates/nvt-service/src/usuario_testes.rs` | Tarefas e relatos de jogo isolados por SID/sessão, idempotentes, com revisão obsoleta descartada; aceite/retirada por regra; relato com destino fora do catálogo rejeitado; sinal de F3-13 só do SID da console; falha no arquivo mantém o limite do driver; `usuarios.json` antigo carrega. |
| L2-e Interface | `crates/nvt-ui/` | Modelos cobrem “Arquivo do jogo” com caminho anonimizado, antes/depois, backup, Aceitar/Retirar, Reverter e cada motivo novo com texto e ação orientada; nenhuma mensagem atribui S1 a exigência de anticheat (S2). |

### L3 — integração, após L2

| Tarefa | Arquivos de escrita | Pronto quando |
|---|---|---|
| L3-a Serviço, CLI e workspace | `crates/nvt-service/Cargo.toml`, `crates/nvt-service/src/{main,laco,nucleo,relatorio,testes}.rs`, `crates/nvt-service/examples/cliente_ipc.rs`, `Cargo.lock` | Pipe real liga agente↔coordenador para jogo; F3-13 reavalia DRS sob o portão; “Restaurar original” cobre driver, Windows e jogo; CLI `status` mostra destinos de jogo; `cargo fmt --check`, `cargo test --workspace` (com `proibicoes.rs`), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace` passam. |
| L3-b Instalador | `instalador/nv-tuner.wxs`, `instalador/build-msi.ps1` | Restauração de usuário na desinstalação inclui config de jogo e não apaga backups; build com ICE. |

### L4 — documentação e laboratório, após L3

| Tarefa | Arquivos de escrita | Pronto quando |
|---|---|---|
| L4-a Documentação | `README.md`, `CHANGELOG.md`, `docs/{arquitetura,ipc,servico,ui,regras}.md` | Promessa pública completa (§4, F3-14), IPC v7, permissões do agente sobre arquivos de jogo, backups e restauração. |
| L4-b Registro do laboratório | `docs/spec/fase3-laboratorio.md` | L3-1 a L3-6 registrados (ambiente, passos, resultado, decisão de promoção para a versão seguinte). |

## Laboratório (fora dos testes automatizados)

Usa contas e máquinas de teste; nenhum arquivo real vai para o repositório (D12).

- **L3-1** R6 `FPSLimit` vs. limite do driver: CPU, p99, entrada→fóton; valor persiste após abrir/fechar; conta escolhida correta com duas contas.
- **L3-2** Fortnite `FrameRateLimit`: idem; arquivo idêntico fora da chave após partida; somente leitura orientado.
- **L3-3** CS2 `+fps_max` via `localconfig.vdf`: valor efetivo no console após abrir a Steam; Steam Cloud não reverte `LaunchOptions`; `local`/`remote` de `cs2_video.txt`; restauração remove só o token.
- **L3-4** Rocket League Max FPS e `OneFrameThreadLag`: latência com/sem; cópia na pasta de instalação não é tocada.
- **L3-5** Dota 2 `+fps_max` acima de 120 sem regressão de p99; teto do menu confirmado.
- **L3-6** Anticheat: abrir cada jogo após edição e após restauração; nenhum erro de integridade (EAC, BattlEye, VAC); nenhum handle aberto pelo nv-tuner sobre os processos do jogo.

Também no laboratório: Documentos redirecionado ao OneDrive, troca rápida de usuário, Steam com duas contas, jogo regravando o arquivo ao fechar (F3-07/F3-08), desinstalação com `RESTAURAR_PERFIL=1`. Conferir as páginas de Riot, FACEIT, Valve, Epic e Ubisoft antes do lançamento (S8).
