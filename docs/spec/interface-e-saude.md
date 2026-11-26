# Especificação — interface para leigos e saúde do PC

Data: 2026-11-26. Base: v0.2.0 (commit aa89003). Fonte de verdade do escopo:
[`docs/intent/interface-e-saude.md`](../intent/interface-e-saude.md). Pesquisas:
[`ui-auditoria.md`](../pesquisa/ui-auditoria.md), [`saude-medicao.md`](../pesquisa/saude-medicao.md),
[`saude-manutencao.md`](../pesquisa/saude-manutencao.md). Regras anticheat:
[`competitivo.md`](competitivo.md) §3 (N1–N15, S1–S8), que continuam valendo sem exceção
além da registrada em D3.

## 1. Objetivo

Um leigo instala, abre e entende sozinho como o PC está e, depois de jogar, como foi a
partida. O app mantém o PC otimizado sozinho; a parte técnica continua inteira no
**Avançado**.

Entregas:

1. Redesenho da UI (build normal): Início com veredito único, Jogos simplificado,
   Avançado com tudo o que existe hoje, boas-vindas curta, visual estilo Configurações
   do Windows 11 / NVIDIA App.
2. "Aplicar melhorias recomendadas" e "Desfazer tudo" no Início.
3. Sinais de saúde só mostrados: programas de inicialização (novos desde a última vez),
   espaço livre, memória e CPU em repouso, driver NVIDIA desatualizado, plano de
   energia trocado, Storage Sense desligado.
4. Limpeza de temporários como modo controlado (dry-run no build de teste, desligada no
   build normal até validação, depois automática).
5. Gravação de desempenho das partidas (ETW Present + NVML) somente no build de teste,
   atrás de feature de compilação, até a validação de custo e de anticheat.

## 2. Não-objetivos

- Overlay, injeção, hook, camada Vulkan, driver de kernel (N1–N4).
- Telemetria enviada para fora; qualquer dado pessoal no IPC ou no disco além do
  descrito em §7.
- Desativar, editar ou remover programas de inicialização; configurar Storage Sense;
  limpar registro, Prefetch, shader caches, SoftwareDistribution, Delivery
  Optimization, Lixeira, WinSxS, logs ou dumps.
- Prometer ganho de FPS pela limpeza.
- Opção do usuário para ligar a gravação de partidas ou a limpeza (D1, D2).
- Mudar portões, restauração, atualização ou qualquer regra de segurança.
- Trocar o GPUI; outros idiomas; mais de uma tela de boas-vindas.

## 3. Decisões

| # | Decisão | Consequência |
|---|---|---|
| D1 | A gravação de partidas não é opção do usuário e não vai para usuários antes de validada. Fica atrás da feature `medicao-partidas` (crate `nvt-medicao`, propagada por `nvt-service`, `nvt-agente` e `nvt-ui`; liga também a feature `build-de-teste` de `nvt-manutencao`), desligada no release. | Trava tripla (§8.4): `build.rs` + `compile_error!`, passo do `release.yml` e marcador visível "Build de teste" na UI. No build normal o cartão de partidas e a tela Histórico de partidas não existem (código `#[cfg]`), e o IPC responde `Erro { RecursoAusente }`. |
| D1a | Diferente de `laboratorio`, a medição **precisa** de build otimizado (o custo de FPS só vale medido em release). A trava não usa `PROFILE`: o `build.rs` exige `NVT_BUILD_DE_TESTE=1` quando a feature está ativa e falha se `GITHUB_ACTIONS` ou `CI` estiverem definidos. | O mantenedor gera localmente `cargo build --release --features medicao-partidas` com a variável; o CI nunca consegue. O binário de teste mostra "Build de teste" na barra de título e na versão (`0.3.0+teste`). |
| D2 | Limpeza de temporários é um **modo** (`ModoLimpeza`): `Desligada`, `Simulacao` (dry-run), `Ativa`. Build de teste: `Simulacao` fixo. Build normal: `Desligada` até o mantenedor validar o dry-run; uma versão posterior muda a constante para `Ativa`. Sem opção do usuário. | O modo é constante de compilação (`nvt-manutencao::MODO_LIMPEZA`, escolhido por `cfg`), não preferência nem algo que o IPC altere (o `Estado` só informa o modo). Testes provam que `Simulacao` nunca chama a fachada de exclusão (§4.4, R-L-06). |
| D3 | Inicialização: só detectar e mostrar. Para "Abrir inicialização", **abrir `ms-settings:startupapps` por `ShellExecuteW` na UI**, como exceção auditada em `proibicoes.rs`. | Justificativa abaixo. |
| D4 | Outros sinais (espaço livre, memória/CPU em repouso, driver, plano de energia, Storage Sense): só mostrar, em frase simples, com o caminho do Windows quando houver ação manual. | Nenhuma escrita nova. |
| D5 | Redesenho para leigos no build normal (§5). "Aplicar melhorias recomendadas" usa **requisição IPC nova** `AplicarRecomendadas`. "Desfazer tudo" = `RestaurarOriginal` existente. | Justificativa abaixo. |
| D6 | IPC v8. Mensagens novas sem dados pessoais (§7.6). v7 é rejeitada antes de desserializar o corpo, como nas versões anteriores. | UI e serviço sobem juntos no MSI; a UI antiga recebe `VersaoIncompativel` e o texto "Feche e abra de novo". |
| D7 | Q9: "Desfazer tudo" (`RestaurarOriginal`) mantém os aceites experimentais; "Voltar a ajustar" (`AplicarAgora`) os reaplica. | Desfazer e refazer simétricos para o leigo; R-AR-06 fica como está. |
| D8 | Q10: a v0.3.0 já sai com IPC v8; `ObterSaude` e afins respondem `Erro { RecursoAusente }` até o L4. | O MSI instala UI e serviço juntos; não há segunda troca de protocolo depois. |
| D9 | Q11: um único build de teste (`medicao-partidas` + `NVT_BUILD_DE_TESTE=1`) liga a gravação de partidas e o dry-run da limpeza. | As travas não se separam; o Lab-L usa o mesmo binário do Lab-M. |

**Justificativa de D3.** O texto "Configurações > Aplicativos > Inicialização" é seguro mas
falha o critério de sucesso (o leigo não acha a página). `ShellExecuteW` com um URI
`ms-settings:` documentado é o mesmo que clicar num link: quem cria o processo é o shell,
o processo é do Windows (`SystemSettings.exe`), a ação é explícita e a UI roda com o token
do usuário. O espírito de N14 (nenhum auxiliar em segundo plano, nada com jogo aberto) é
preservado com estas condições, todas testáveis: (a) só em `crates/nvt-ui/src/abrir.rs`,
um único `ShellExecuteW` com constante literal `ms-settings:startupapps` (nenhum argumento
vindo de dados); (b) só no tratador de clique; (c) o botão fica desabilitado e a função
recusa se o último estado do serviço indicar jogo ou anticheat aberto, ou se o estado for
desconhecido; (d) exceção registrada em `PERMITIDOS` com contagem exata e teste que
confere a constante; (e) serviço e agente continuam sem nenhuma ocorrência. Os outros
caminhos do Windows citados em D4 (Storage Sense, Windows Update) ficam como **texto**,
sem segunda exceção.

**Justificativa de D5 (requisição nova).** `DefinirAceiteExperimental` em lote pela UI
seria N conexões, sem atomicidade: uma falha no meio deixaria parte aceita, e cada aceite
dispara reavaliação e revisão de plano. `AplicarRecomendadas` faz no serviço, sob uma
única persistência das preferências do SID chamador, o aceite de todas as experimentais
**aplicáveis agora** (condição casada, camada disponível) das camadas DRS, Windows e
arquivo de jogo, e então dispara o mesmo caminho de "Aplicar agora" (portão S1 incluso).
A resposta lista os IDs aceitos. O conjunto é calculado pelo serviço, não enviado pela UI,
para que a UI não precise conhecer IDs. Aceites DRS continuam sujeitos à regra do SID do
console (fase 2). `DefinirAceiteExperimental` individual permanece no Avançado.

## 4. Requisitos

Formato: ID, requisito, como testar. "Teste" = teste automatizado no workspace; "Lab" =
roteiro de §10.

### 4.1 Interface (R-UI)

| ID | Requisito | Teste |
|---|---|---|
| R-UI-01 | Navegação lateral com três itens visíveis no build normal: Início, Jogos, Avançado (ícones Lucide distintos e visíveis). No build de teste, um quarto item "Partidas". | Teste de modelo: `Tela::visiveis()` por cfg. |
| R-UI-02 | Início mostra exatamente um veredito, calculado por função pura `veredito(&Estado, Option<&Saude>) -> Veredito`, com prioridade: indisponível (serviço ausente ou protocolo incompatível) > sem placa NVIDIA ("Não encontramos uma placa de vídeo NVIDIA") > pausado após Desfazer tudo ("Os ajustes estão desligados", botão "Voltar a ajustar", que envia `AplicarAgora`) > esperando o jogo ("Há mudanças esperando o jogo fechar") > piorou > melhorias > otimizado (inclui "nada a fazer": "Não há nada a ajustar neste PC agora."). Atualização e agente ausente ficam só na linha de estado. Sem saúde (`None`), "piorou" nunca é escolhido. | Tabela de casos em `modelo` cobrindo cada entrada. |
| R-UI-03 | Nenhuma sigla na tela principal (Início, Jogos, boas-vindas). Lista de siglas proibidas (GPU, VRAM, DLSS, VRR, G-SYNC, FPS, BAR, DRS, HAGS, MPO, SID, IPC, ETW, MSI, CPU, RAM, SSD). No Avançado podem aparecer com o nome por extenso na primeira ocorrência. | Teste que varre as constantes de `textos/inicio.rs`, `textos/jogos.rs`, `textos/boas_vindas.rs`. Exceção: nome do jogo e marca "NVIDIA". |
| R-UI-04 | Nenhum emoji em texto nem em recurso. | Teste: nenhum caractere fora de BMP nem nos blocos de símbolo (U+2600–U+27BF, U+1F000+) em `textos/`. |
| R-UI-05 | Estados (serviço ausente, protocolo incompatível, jogo aberto, atualização, agente ausente) aparecem numa única **linha de estado** no topo do conteúdo, uma por vez, por prioridade, com no máximo uma ação. Nenhum alerta repetido de várias linhas. | Teste de modelo `linha_estado()`: prioridade e um único resultado. |
| R-UI-06 | Botão "Aplicar melhorias recomendadas" aparece só se houver experimental aplicável; desabilitado com motivo na própria linha (não só tooltip) com serviço ausente. Com jogo aberto continua habilitado e a resposta `adiada` vira "As melhorias serão aplicadas quando o jogo fechar." (R-AR-02). Depois do clique, o Início mostra "Aplicamos N melhorias. Para voltar atrás, use Desfazer tudo." Sem diálogo de confirmação: a ação é reversível. | Teste de modelo. Lab: clique aceita todas e aplica. |
| R-UI-07 | "Desfazer tudo" pede confirmação em linguagem simples e envia `RestaurarOriginal`. | Teste de modelo do diálogo; o caminho IPC já é coberto. |
| R-UI-08 | Tudo que a v0.2.0 faz continua acessível no Avançado: Visão técnica (sistema, capacidades, conformidade, deriva), Mudanças por perfil com Aceitar/Retirar e Destravar, Windows (o que o app fez, reverter, experimentais, diagnóstico), Arquivo do jogo e reversão por chave, Preferências técnicas, Histórico técnico, Restaurar original, Reescanear jogos, Verificar atualização. | Teste de inventário: cada `Requisicao` enviada pela UI v0.2.0 tem um ponto de chamada alcançável a partir de uma `Tela` (lista em teste). |
| R-UI-09 | Boas-vindas aparece só na primeira abertura (marca em `%LOCALAPPDATA%\nv-tuner\ui.json`), uma tela, um botão "Começar". Falha ao ler ou gravar a marca não bloqueia: mostra no máximo de novo. | Teste do armazenamento com pasta temporária. |
| R-UI-10 | Tema escuro fixo (a intenção pede "escuro"; tema claro fica fora). Destaque ciano do logo (`#38C8F0`). Fonte `Segoe UI Variable`, queda para `Segoe UI`. | Teste dos tokens; contraste do texto sobre fundo e sobre destaque >= 4,5:1 calculado em teste. |
| R-UI-11 | Escala tipográfica fixa: título de página 28, veredito 20 semibold, título de cartão 15 semibold, corpo 14, legenda 12. Cartões usam `SettingPage`/`SettingGroup`/`SettingItem` do gpui-component. | Revisão visual por capturas (Lab-UI). |
| R-UI-12 | Jogos mostra por jogo: nome, um status ("Otimizado", "Faça 2 ajustes no jogo", "Aguardando o jogo fechar") e detalhe ao clicar; nada de API, versão de DLSS, perfil ou anticheat na lista. O detalhe técnico tem link "Ver no Avançado". | Teste de modelo `status_jogo()`. |
| R-UI-13 | Capturas de referência novas em `docs/ui-capturas/` para cada tela e cada estado da linha. | Lab-UI. |
| R-UI-14 | Modo de visualização (sem serviço) continua somente leitura e passa a dizer, no Início: "O nv-tuner não está ativo neste PC" + "Reinstale o nv-tuner para que ele volte a cuidar do PC". Sem `services.msc`. | Teste de texto. |

### 4.2 Aplicar recomendadas (R-AR)

| ID | Requisito | Teste |
|---|---|---|
| R-AR-01 | `AplicarRecomendadas` aceita, para o SID chamador, todas as experimentais aplicáveis das três camadas numa única gravação de preferências e responde `Recomendadas { aceitas: Vec<String>, adiada: bool }`. | Teste do serviço com fachadas falsas. |
| R-AR-02 | Com jogo/anticheat aberto, os aceites são persistidos e a aplicação fica `adiada: true`, como "Aplicar agora". | Teste. |
| R-AR-03 | Repetir a requisição é idempotente (nenhum aceite duplicado, mesma revisão se nada mudou). | Teste. |
| R-AR-04 | Experimental DRS fora do SID do console é aceita mas indicada como inativa (regra da fase 2). Aceites Windows e de arquivo de jogo valem só para o SID chamador. | Teste. |
| R-AR-05 | Só entram regras marcadas experimentais no catálogo e aplicáveis agora; regra não experimental, de condição não casada, de camada indisponível ou travada pelo usuário nunca é aceita. Falha na gravação das preferências = nenhum aceite e nenhuma aplicação (tudo ou nada). | Teste com catálogo falso contendo cada caso. |
| R-AR-06 | "Desfazer tudo" depois de `AplicarRecomendadas` volta tudo ao original e pausa a autonomia (`autonomia_pausada`), como hoje; os aceites continuam gravados e voltam a valer com "Voltar a ajustar" (ver Q9). | Teste de sequência Aplicar -> Restaurar -> AplicarAgora. |

Desvio registrado (L2, R-AR-04): na implementação, `AplicarRecomendadas` vindo de um SID que não é o da sessão de console não aceita a experimental DRS (ela não entra no lote), em vez de aceitá-la como inativa. Motivo: é mais simples e não deixa efeito escondido; um aceite inativo passaria a valer sozinho quando aquele usuário viesse a ser o do console, sem novo pedido. Os aceites Windows e de arquivo de jogo continuam valendo só para o SID chamador. O aceite individual pelo Avançado (`DefinirAceiteExperimental`) mantém a regra da fase 2.

### 4.3 Saúde: sinais e inicialização (R-S)

| ID | Requisito | Teste |
|---|---|---|
| R-S-01 | O agente lê, somente leitura, as fontes de inicialização: Run/RunOnce de HKCU e HKLM (incl. WOW6432Node), StartupApproved (Run, Run32, StartupFolder) de HKCU e HKLM, pastas Inicializar do usuário e comum (só o nome do `.lnk` e o nome do executável alvo), tarefas agendadas com gatilho de logon/boot fora de `\Microsoft\`. Serviços automáticos de terceiros ficam para depois (questão Q4). | Teste com fachada de registro/pasta/tarefas falsa; fachada real só expõe leitura (sem método de escrita no trait). |
| R-S-02 | Primeira execução grava linha de base e não marca nada como novo. Depois, "novo" = identidade `(fonte, nome)` ausente da base; alteração do comando (hash) aparece como "mudou". "Marcar como visto" incorpora à base. | Testes de diferença. |
| R-S-03 | Nada sai do agente com caminho completo ou linha de comando: o IPC leva nome exibido, nome do executável, fonte (enum), habilitado, novo/mudou, data de detecção. | Teste de DTO: campos permitidos; `validar_texto` limita tamanho e recusa separador de caminho no nome do executável. |
| R-S-04 | Botão "Abrir inicialização" conforme D3. | Teste em `proibicoes.rs` + teste de modelo do bloqueio. |
| R-S-05 | Sinais: espaço livre do volume do sistema (`GetDiskFreeSpaceExW`; alerta < 10% ou < 20 GB), memória comprometida em repouso (`GlobalMemoryStatusEx`) e CPU em repouso (`GetSystemTimes`, média de 5 min, só sem jogo e após 10 min do logon), driver NVIDIA abaixo da última versão conhecida (lista embutida no catálogo, só informar), plano de energia diferente do esperado pela camada Windows, Storage Sense desligado (leitura de `HKCU\...\StorageSense\Parameters\StoragePolicy`, valor `01`). | Teste por sinal com fachada falsa; limiares em constantes. |
| R-S-06 | Cada sinal vira `Sinal { tipo, gravidade, valor_resumido }` traduzido pela UI numa frase. Só "Algo piorou" do veredito usa gravidade `Atencao`. | Teste de tradução e de veredito. |
| R-S-07 | Leituras de sinais e inicialização rodam fora do atendimento do pipe, com cache (inicialização a cada 6 h e no logon; sinais a cada 10 min). | Teste do agendador com relógio falso. |

### 4.4 Limpeza de temporários (R-L)

| ID | Requisito | Teste |
|---|---|---|
| R-L-01 | Alvos: `%TEMP%` do usuário pelo agente; `C:\Windows\Temp` (via `GetSystemWindowsDirectoryW`) pelo serviço. O serviço nunca toca perfis. | Teste de configuração dos alvos. |
| R-L-02 | Candidato: arquivo com `CreationTime` **e** `LastWriteTime` >= 7 dias, sem `FILE_ATTRIBUTE_REPARSE_POINT`, sem `READONLY`, sem `SYSTEM`. Diretório só se vazio após a passada e com idade >= 7 dias; qualquer filho recente preserva o diretório. | Testes de filtro com metadados falsos. |
| R-L-03 | Enumeração por `FindFirstFileExW` sem seguir reparse points; abertura com `DELETE`, `dwShareMode = 0`, `FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS`; reconferência de atributos pelo handle; falha = pular. Exclusão por `SetFileInformationByHandle(FileDispositionInfoEx, DELETE)` sem `POSIX_SEMANTICS` nem `IGNORE_READONLY`. | Teste de integração em pasta temporária com junção apontando para fora: alvo intacto. Teste de arquivo aberto por outro handle: pulado. |
| R-L-04 | Nunca: Prefetch, D3DSCache, NVIDIA DXCache/GLCache/NV_Cache, SoftwareDistribution, Delivery Optimization, Lixeira, WinSxS, Installer, Logs, Minidump, LiveKernelReports. Garantia estrutural: só os dois alvos existem; teste adicional recusa qualquer raiz fora deles e qualquer caminho que, normalizado, saia da raiz. | Teste. |
| R-L-05 | Portões: nenhum jogo/anticheat (mesma lista do `Portao`, verificada antes da passada e a cada 200 entradas), mutex `Global\_MSIExecute` inexistente (`OpenMutexW`, sem processo), no máximo uma passada por 24 h (registro persistido), limite de 60 s e 20.000 entradas por passada. | Testes com relógio e portão falsos. |
| R-L-06 | `Simulacao` percorre e calcula `{ arquivos, bytes, pulados_por_motivo }` e **nunca** chama exclusão. Garantia: a passada é genérica sobre `trait Excluidor`; em `Simulacao` o tipo é `ExcluidorNulo`, que não tem acesso ao handle (o código de abertura com `DELETE` só existe em `ExcluidorReal`). Teste com `ExcluidorEspiao` em `Simulacao` conta zero chamadas; teste de integração em pasta real em `Simulacao` confere que todos os arquivos continuam lá, byte a byte. | Teste. |
| R-L-07 | `Desligada` não percorre nada. `MODO_LIMPEZA` por cfg: feature `build-de-teste` de `nvt-manutencao` (ligada só por `medicao-partidas` de serviço e agente, com a mesma trava de `build.rs` de §8.4) => `Simulacao`; build normal => `Desligada`. Teste de compilação afirma o modo por cfg. | Teste. |
| R-L-08 | Resultado da última passada (modo, data, arquivos, bytes, contagens de pulados) fica no estado e no histórico técnico. UI: no build de teste, Avançado > Manutenção mostra "Simulação: apagaria N arquivos e liberaria X GB". No build normal com `Ativa` (futuro), Início pode dizer "Liberamos X GB de arquivos temporários antigos". Nunca citar FPS ou velocidade. | Teste de texto (nenhuma ocorrência de "FPS", "rápido", "desempenho" nos textos de limpeza). |
| R-L-09 | O log registra só contagens e bytes; nunca nomes de arquivo do usuário. | Teste do relatório. |

### 4.5 Gravação de partidas (R-M), só com `medicao-partidas`

| ID | Requisito | Teste |
|---|---|---|
| R-M-01 | Serviço abre uma sessão ETW em tempo real com nome fixo `nv-tuner-partidas`, com consumidor, somente `Microsoft-Windows-DXGI` e `Microsoft-Windows-D3D9` (eventos de Present), nível mínimo. Nunca NT Kernel Logger, stack walk, provedores de kernel ou de outros fins; nunca altera `EtwMaxLoggers` nem para sessões alheias. Sessão existente com o mesmo nome (crash anterior) é parada **só** se for a nossa (nome exato). | Teste de configuração; Lab-M. |
| R-M-02 | Associação PID -> executável só por `TH32CS_SNAPPROCESS` e base curada; nenhum handle (N1). Snapshot renovado em PID desconhecido; associação encerrada quando o PID some. | Teste com snapshot falso; `proibicoes.rs` continua verde sem exceção nova. |
| R-M-03 | GPU por NVML (`nvml.dll` por caminho completo do driver, `LoadLibraryExW` com `LOAD_LIBRARY_SEARCH_SYSTEM32` ou caminho absoluto), uso e temperatura a 1 Hz. Falha = campos ausentes, sem abortar a partida. | Teste com fachada falsa. |
| R-M-04 | Partida começa com executável da base no snapshot e termina quando o PID some (polling 2 s). Descartar < 60 s ou sem Presents. | Teste. |
| R-M-05 | Engasgo: frametime > 2,5 x mediana móvel de 2 s **e** > 25 ms. 1% low: percentil 99 do frametime convertido em FPS (documentado na UI do Avançado). | Testes com sequências sintéticas. |
| R-M-06 | Agregados apenas (§7.4); nenhum frametime bruto em disco. | Teste do formato. |
| R-M-07 | Qualquer erro do consumidor (perda de eventos acima de 5%, `ProcessTrace` falhando, sessão negada) encerra a sessão e não tenta de novo naquela partida; registra código. | Teste. |
| R-M-08 | Custo e anticheat validados no Lab-M antes de qualquer discussão de entrega. | Lab-M. |

### 4.6 Build e segurança (R-B)

| ID | Requisito | Teste |
|---|---|---|
| R-B-01 | `release.yml` falha se `cargo tree -e features` mostrar `medicao-partidas` ou `build-de-teste` (mesmo padrão do passo `laboratorio`). | Revisão do workflow + ensaio local do comando. |
| R-B-02 | `crates/nvt-medicao/build.rs` falha se a feature estiver ativa e (`NVT_BUILD_DE_TESTE` != `1` ou `CI`/`GITHUB_ACTIONS` definidos); `lib.rs` tem `compile_error!` equivalente para `all(feature = "medicao-partidas", not(nvt_build_de_teste))` com o cfg emitido pelo `build.rs`. | Teste de build: `cargo check --features medicao-partidas` sem a variável falha (script em `scripts/`). |
| R-B-03 | Build de teste mostra "Build de teste" na barra de título e a versão com sufixo `+teste`. O MSI de teste é gerado à parte, com nome `nv-tuner-<versão>-teste.msi`, e nunca publicado. | Teste de texto; inspeção do MSI. |
| R-B-04 | `proibicoes.rs`: exceção única nova `("crates/nvt-ui/src/abrir.rs", "ShellExecuteW", 1)` e teste que exige a constante literal `ms-settings:startupapps` como único argumento de URI. Nenhum outro termo novo permitido. Termos novos proibidos fora de `nvt-medicao`: `StartTraceW`, `EnableTraceEx2`, `OpenTraceW`, `ProcessTrace`, `ControlTraceW`; proibidos em todo lugar: `SystemTraceControlGuid`, `KERNEL_LOGGER_NAME`, `EtwMaxLoggers`. | Teste. |
| R-B-05 | Toda escrita nova (limpeza) respeita S1 e roda só sem jogo/anticheat. Leituras novas não abrem handle de processo. | Testes de portão de R-L-05. |

## 5. Telas

Mocks em texto; largura de referência 1200 px. `[ ]` = botão, `>` = linha clicável. Os
textos finais ficam em `crates/nvt-ui/src/textos/`.

### 5.1 Estrutura comum

```
+----------------+---------------------------------------------------------+
| [logo] nv-tuner| <linha de estado, só quando houver>                     |
|                |                                                         |
| > Início       | <Título da página, 28>                                  |
| > Jogos        |                                                         |
| > Avançado     | <conteúdo em SettingGroup/SettingItem>                  |
| (> Partidas)*  |                                                         |
|                |                                                         |
| versão 0.3.0   |                                                         |
+----------------+---------------------------------------------------------+
* só no build de teste
```

Linha de estado (uma por vez, por prioridade):

| Prioridade | Estado | Texto | Ação |
|---|---|---|---|
| 1 | Protocolo incompatível | "O nv-tuner foi atualizado. Feche esta janela e abra de novo." | nenhuma |
| 2 | Serviço ausente | "O nv-tuner não está ativo neste PC. Mostrando só informações." | nenhuma |
| 3a | Atualização concluída (versão mudou desde a última abertura; some ao trocar de tela) | "O nv-tuner foi atualizado." | nenhuma |
| 3 | Jogo aberto | "<Nome do jogo> está aberto. As mudanças esperam o jogo fechar." | nenhuma |
| 4 | Atualização pronta ou instalando (falha não aparece aqui) | "Uma atualização do nv-tuner será instalada quando nenhum jogo estiver aberto." | nenhuma |
| 5 | Agente ausente | "Parte dos ajustes do Windows está pausada até você entrar de novo no Windows." | nenhuma |

Códigos de falha (HTTP, assinatura, 3010) só aparecem em Avançado > Atualização.

### 5.2 Boas-vindas (primeira abertura)

```
              [logo grande]
        Bem-vindo ao nv-tuner

  O nv-tuner ajusta o seu PC para jogos sozinho, sem mexer
  nos seus jogos enquanto eles estão abertos.
  Você pode desfazer tudo a qualquer momento.

                 [ Começar ]
```

### 5.3 Início

```
Início

+-----------------------------------------------------------------+
| (ícone grande)  Seu PC está otimizado                           |
|                 Tudo o que recomendamos já está aplicado.       |
|                                             [ Desfazer tudo ]   |
+-----------------------------------------------------------------+
  Variações do veredito:
  "Há melhorias disponíveis" / "3 ajustes podem deixar seus jogos mais leves."
        [ Aplicar melhorias recomendadas ]  [ Desfazer tudo ]
  "Algo piorou: pouco espaço livre" / "Restam 12 GB no disco do Windows."
  "O nv-tuner não está ativo" / "Reinstale o nv-tuner para que ele volte a cuidar do PC."
  "Há mudanças esperando o jogo fechar" / "Nada é mexido enquanto você joga."
  "Os ajustes estão desligados" / "Você pediu para desfazer tudo."  [ Voltar a ajustar ]
  "Não encontramos uma placa de vídeo NVIDIA" / "O nv-tuner só ajusta PCs com placa NVIDIA."
  "Não há nada a ajustar neste PC agora." (só com Desfazer tudo)

Seus jogos                                                 > Ver todos
  Counter-Strike 2        Otimizado
  Cyberpunk 2077          Faça 2 ajustes no jogo

Saúde do PC
  Espaço livre            Restam 210 GB. Tudo certo.
  Inicialização           2 programas novos começam junto com o Windows.
                          [ Abrir inicialização ]
  Placa de vídeo          O driver está em dia.
  Energia                 O PC está no modo de energia recomendado.
  Uso em repouso          O PC está tranquilo quando você não está jogando.
  Limpeza do Windows      O Sensor de Armazenamento está desligado.
                          Para ligar: Configurações > Sistema > Armazenamento.

(build de teste) Últimas partidas                          > Ver todas
  Counter-Strike 2   hoje, 40 min   Estável   287 quadros por segundo em média
```

Regras: "quadros por segundo" por extenso; "Placa de vídeo" no lugar de GPU. Só a linha de
inicialização tem botão; os outros sinais são texto, com o caminho do Windows quando
houver ação manual. Se nenhum sinal estiver em atenção, a seção vira uma linha única
"Tudo certo com a saúde do PC", expansível. Sem resposta de `ObterSaude` (`RecursoAusente`, como na v0.3.0) a seção não aparece; "Verificando o PC" só enquanto a primeira resposta não chega.

Confirmação de "Desfazer tudo":

```
Desfazer tudo?
Todos os ajustes que o nv-tuner fez voltam a ser como eram antes.
Se um jogo estiver aberto, isso acontece quando ele fechar.
O nv-tuner para de ajustar até você pedir de novo.
                                   [ Cancelar ]  [ Desfazer tudo ]
```

### 5.4 Jogos

```
Jogos
Encontramos 4 jogos neste PC.                      [ Procurar jogos de novo ]

> Counter-Strike 2      Otimizado
> Cyberpunk 2077        Faça 2 ajustes no jogo
> Valorant              Aguardando o jogo fechar
> Fortnite              Nada a ajustar

Detalhe (ao clicar):
  Cyberpunk 2077
  O que o nv-tuner fez      Deixou a placa de vídeo com menos atraso neste jogo.
  Faça no jogo              1. No menu Vídeo, ligue "Baixa latência".
                            2. ...
  [ Ver detalhes técnicos ]   (abre Avançado > Jogos)
```

### 5.5 Avançado

Uma `SettingPage` com grupos recolhíveis. Cada grupo reaproveita o conteúdo da tela
equivalente da v0.2.0, com textos revisados (sigla por extenso na primeira vez):

| Grupo | Conteúdo (origem v0.2.0) | Ações |
|---|---|---|
| Sistema e recursos | Visão geral técnica: GPU, arquitetura, VRAM, driver, DLSS, Reflex, Resizable BAR, VRR, conformidade, deriva | Aplicar agora, Reescanear jogos |
| Mudanças por perfil | Tela Mudanças, com ID da regra e impacto visual | Aceitar/Retirar, Destravar |
| Sugestões em teste | Experimentais das três camadas, com explicação | Aceitar/Retirar individual |
| Windows | O que o app fez, Diagnóstico, estado do agente | Reverter individual |
| Jogos (técnico) | Tabela atual com API, DLSS, anticheat, Arquivo do jogo | Reverter chave |
| Preferências | Sincronismo, Qualidade visual, Limite de quadros, com "Recomendado para seu monitor" pré-marcado | Salvar, Descartar |
| Inicialização | Lista completa por fonte, novo/mudou, data de detecção | Marcar como visto, Abrir inicialização |
| Manutenção | Última passada da limpeza (modo, data, contagens, bytes) | nenhuma |
| Atualização | Cartão atual, com códigos | Verificar agora |
| Histórico técnico | Tela Histórico atual | nenhuma |
| Desfazer tudo | Tela Restaurar original | Desfazer tudo |
| Métricas detalhadas (build de teste) | Por partida: 1% low, 0,1% low, histograma, frametime por segundo, uso e temperatura | Apagar todas as partidas |

### 5.6 Partidas (build de teste)

```
Partidas
Esta semana: 12 partidas, 3 com engasgos. Semana passada: 9 partidas, 4 com engasgos.

> Counter-Strike 2   hoje 21:10    40 min   Estável
    [gráfico pequeno de quadros por segundo]
    Média 287   Piores momentos 141   Placa de vídeo chegou a 71 °C
> Valorant           ontem 22:03   35 min   5 engasgos
```

Gráfico: `chart::LineChart` sobre os agregados por segundo; se custar caro, `canvas` com
`PathBuilder`. "Piores momentos" = 1% low; a definição aparece no Avançado.

## 6. Arquitetura

- **UI (`nvt-ui`)**: `ui.rs` vira `ui/` com um módulo por tela (`estrutura.rs`,
  `inicio.rs`, `boas_vindas.rs`, `jogos.rs`, `avancado.rs`, `partidas.rs`); `textos.rs`
  vira `textos/` com um módulo por tela. `modelo.rs` ganha as funções puras `veredito`,
  `linha_estado` e `status_jogo`. `tema.rs` define tokens e lê o modo do sistema.
  `abrir.rs` contém a única chamada `ShellExecuteW`.
- **Agente (`nvt-agente`)**: módulo `saude/` (inicialização, sinais de sessão, limpeza
  de `%TEMP%`), relatado em `AgenteRelatar` (campo novo `saude`).
- **Serviço (`nvt-service`)**: `saude.rs` (sinais de máquina, agregação por SID, limpeza
  de `C:\Windows\Temp`) e `recomendadas.rs` (D5). Com `medicao-partidas`, `partidas.rs`
  liga `nvt-medicao` ao laço e ao portão.
- **Crates novos**: `nvt-manutencao` (passada genérica, `ModoLimpeza`, `Excluidor`,
  filtros; Windows por fachada) e `nvt-medicao` (só com a feature: consumidor ETW, NVML,
  agregação e armazenamento das partidas).
- **Leituras Windows novas** em `nvt-windows/src/saude/` (registro e pastas de
  inicialização, tarefas por COM, espaço, memória, CPU, Storage Sense, plano ativo), por
  traits sem métodos de escrita.

## 7. Modelo de dados

### 7.1 Manutenção

```rust
pub enum ModoLimpeza { Desligada, Simulacao, Ativa }
pub enum AlvoLimpeza { TempUsuario, TempWindows }
pub enum MotivoPulo { Recente, ReparsePoint, SomenteLeitura, Sistema, EmUso, AcessoNegado, Limite }
pub enum MotivoInterrupcao { JogoAberto, Instalacao, Tempo, Erro }
pub struct ResultadoLimpeza {
    pub modo: ModoLimpeza,
    pub alvo: AlvoLimpeza,
    pub inicio_unix: u64,
    pub duracao_ms: u32,
    pub arquivos: u32,                 // apagados (Ativa) ou que seriam apagados (Simulacao)
    pub bytes: u64,
    pub pulados: BTreeMap<MotivoPulo, u32>,
    pub interrompida: Option<MotivoInterrupcao>,
}
```

Persistência: agente em `%LOCALAPPDATA%\nv-tuner\usuario\manutencao.json` (última passada
e data); serviço em `%ProgramData%\nv-tuner\manutencao.json` (ACL SYSTEM/Administradores).
Sem nomes de arquivo. Histórico técnico guarda as últimas 30 passadas.

### 7.2 Inicialização

```rust
pub enum FonteInicio { RunUsuario, RunOnceUsuario, RunMaquina, RunOnceMaquina, RunMaquina32, PastaUsuario, PastaComum, Tarefa }
pub enum SituacaoInicio { NaBase, Novo, Mudou }
pub struct ProgramaInicio {
    pub fonte: FonteInicio,
    pub nome: String,              // nome do valor, do atalho ou da tarefa (máx. 128)
    pub executavel: String,        // só o nome do arquivo, sem pasta (máx. 64)
    pub habilitado: Option<bool>,  // StartupApproved; None = desconhecido
    pub situacao: SituacaoInicio,
    pub detectado_unix: u64,
}
```

Linha de base no agente: `%LOCALAPPDATA%\nv-tuner\usuario\inicializacao.json`, mapa
`(fonte, nome) -> sha256(comando completo)` e data. O comando completo nunca sai do agente.

### 7.3 Sinais

```rust
pub enum TipoSinal { EspacoLivre, MemoriaRepouso, CpuRepouso, DriverDesatualizado, PlanoEnergia, SensorArmazenamento }
pub enum Gravidade { Ok, Informativo, Atencao, Indisponivel }
pub enum ValorSinal { Bytes { livres: u64, total: u64 }, Percentual(u8), Versao { atual: u32, ultima: u32 }, Booleano(bool), Nenhum }
pub struct Sinal { pub tipo: TipoSinal, pub gravidade: Gravidade, pub valor: ValorSinal, pub lido_unix: u64 }
```

O serviço lê espaço, memória, CPU, driver e plano; o agente lê o Storage Sense (HKCU). Só
cache em memória.

### 7.4 Partidas (só `medicao-partidas`)

Por segundo (`AmostraSegundo`, cerca de 20 bytes): `t_rel_s: u32`, `quadros: u16`,
`frametime_medio_us: u32`, `frametime_max_us: u32`, `engasgos: u8`,
`gpu_uso_pct: Option<u8>`, `gpu_temp_c: Option<u8>`.

Por partida (`ResumoPartida`): `id: u64`, `jogo_id` (ID da base curada), `executavel` (nome
do arquivo), `inicio_unix`, `duracao_s`, `quadros_total`, `fps_medio`, `fps_mediana`,
`fps_1pct_low`, `fps_01pct_low`, `engasgos`, `tempo_engasgado_ms`, `histograma: [u32; 64]`
(faixas logarítmicas de 0,5 ms a 500 ms), `gpu_temp_max_c`, `gpu_uso_medio_pct`,
`perda_eventos_pct`, `encerrada_por_erro: Option<CodigoErroMedicao>`, `versao_formato: u16`.
Veredito: "Estável" com zero engasgos; senão "N engasgos". Comparação entre semanas:
contagem de partidas, partidas com engasgo e média de FPS médio por jogo, semana ISO.

Armazenamento: serviço em `%ProgramData%\nv-tuner\partidas\<id-conta>\`, onde `<id-conta>`
é o HMAC por usuário já usado na fase 3 (nenhum SID em claro); um arquivo por partida
(`<id>.bin`: cabeçalho versionado, resumo e amostras; cerca de 75 KB por hora) e
`indice.json` com os resumos. ACL SYSTEM/Administradores; a UI só lê pelo IPC, filtrado
pelo SID chamador. Atribuição ao usuário: `ProcessIdToSessionId` sobre o PID do evento
(não abre handle; ver Q2) e o SID dono daquela sessão, obtido pelo serviço como hoje.
Retenção: 30 dias ou 200 partidas por usuário, o que vier primeiro, podada ao fim de cada
partida e na partida do serviço. Gravação atômica (temporário + `MoveFileExW`), sem
seguir reparse points. "Apagar todas as partidas" no Avançado.

### 7.5 Preferências da UI

`%LOCALAPPDATA%\nv-tuner\ui.json`: `{ "boas_vindas_vista": true }`. Nenhum dado do serviço.

### 7.6 IPC v8

| Mensagem | Direção | Conteúdo |
|---|---|---|
| `AplicarRecomendadas` | UI -> serviço | sem corpo; resposta `Recomendadas { aceitas: Vec<String>, adiada: bool }` |
| `ObterSaude` | UI -> serviço | resposta `Saude { sinais, inicializacao, limpeza }` do SID chamador e da máquina |
| `MarcarInicioVisto { fonte, nome }` | UI -> serviço; agente busca na próxima consulta | incorpora à linha de base |
| `AgenteRelatar.saude` | agente -> serviço | `SaudeSessao { sinais, inicializacao, limpeza }`, no máximo 256 programas |
| `ListarPartidas { desde_unix, limite }` | UI -> serviço | resumos; sem a feature, `Erro { RecursoAusente }` |
| `ObterPartida { id }` | UI -> serviço | resumo e amostras, só do SID chamador |
| `ApagarPartidas` | UI -> serviço | apaga as do SID chamador |

`Estado` ganha `build_de_teste: bool` e `modo_limpeza`. Nenhum caminho, linha de comando,
título de janela, nome de usuário, SID ou identificador de conta trafega. Todos os textos
passam por `validar_texto` com limites.

## 8. Segurança

### 8.1 Nada com jogo aberto

A limpeza (serviço e agente) passa pelo `Portao` antes e durante a passada (R-L-05).
"Aplicar melhorias recomendadas" usa o caminho de "Aplicar agora". "Abrir inicialização"
fica bloqueado com jogo ou anticheat aberto ou com estado desconhecido. Leituras de saúde
podem rodar com jogo aberto, menos a amostra de CPU em repouso (sem sentido) e a
enumeração de tarefas por COM (adiada por cautela de custo).

### 8.2 ETW sem handle

Sessão própria com consumidor no mesmo processo, só provedores de Present (R-M-01).
PID -> nome por `TH32CS_SNAPPROCESS`; sessão por `ProcessIdToSessionId`. Nenhum
`OpenProcess`, provedor de kernel, sessão alheia, alteração de `EtwMaxLoggers` ou de grupos.

### 8.3 `proibicoes.rs`

- Exceção nova: `crates/nvt-ui/src/abrir.rs`, `ShellExecuteW`, uma ocorrência (D3), com
  teste da constante.
- Termos de controle de sessão ETW (`StartTraceW`, `EnableTraceEx2`, `OpenTraceW`,
  `ProcessTrace`, `ControlTraceW`) permitidos só em `crates/nvt-medicao/src/etw.rs`, com
  contagem exata; `SystemTraceControlGuid`, `KERNEL_LOGGER_NAME` e `EtwMaxLoggers`
  proibidos em todo lugar.
- `DeleteFileW`, `RemoveDirectoryW` e `FileDispositionInfoEx` passam a ser vigiados:
  permitidos em `crates/nvt-manutencao/src/windows.rs` e nos usos já existentes, listados
  com contagem no L1.

### 8.4 Trava da feature `medicao-partidas`

1. `crates/nvt-medicao/build.rs`: com `CARGO_FEATURE_MEDICAO_PARTIDAS`, exigir
   `NVT_BUILD_DE_TESTE=1` e ausência de `CI` e `GITHUB_ACTIONS`, senão `panic!`; emitir
   `cargo:rustc-cfg=nvt_build_de_teste` e `cargo:rerun-if-env-changed` das três variáveis.
   Diferente de `laboratorio`, não bloquear perfil release: o custo de FPS só vale medido
   em build otimizado.
2. `crates/nvt-medicao/src/lib.rs`: `compile_error!` sob
   `all(feature = "medicao-partidas", not(nvt_build_de_teste))`.
3. `release.yml`: passo "Conferir feature medicao-partidas desligada", ao lado do passo
   U-15a, sobre `cargo tree -e features` do workspace.
4. Binário e UI marcados "Build de teste"; versão com sufixo `+teste`.

## 9. Plano de lotes

Cada lote começa após todos os critérios do anterior. Caminhos relativos à raiz; um
diretório reserva toda a sua árvore à tarefa. Arquivos de escrita são disjuntos no mesmo
lote; `Cargo.toml` da raiz e `Cargo.lock` têm um único responsável por lote (indicado).
O redesenho sai em L1–L2 e pode ser publicado sozinho como v0.3.0, antes da saúde.

### L1 — fundação da UI e contratos

| Tarefa | Arquivos de escrita | Pronto quando |
|---|---|---|
| Tema e estrutura | `crates/nvt-ui/src/tema.rs`, `crates/nvt-ui/src/main.rs`, `crates/nvt-ui/src/ui.rs` (movido para `ui/legado.rs`: `ui.rs` e `ui/mod.rs` não coexistem), `crates/nvt-ui/src/ui/mod.rs`, `crates/nvt-ui/src/ui/estrutura.rs`, `crates/nvt-ui/src/ui/legado.rs`, `crates/nvt-ui/Cargo.toml` (declara a feature `medicao-partidas` vazia, para os `#[cfg]` passarem no Clippy) | Lateral com Início/Jogos/Avançado e ícones distintos; linha de estado única; Segoe UI Variable; ciano; tema do sistema; testes de R-UI-01 e R-UI-10. |
| Modelo puro | `crates/nvt-ui/src/modelo.rs`, `crates/nvt-ui/src/modelo/` | `veredito`, `linha_estado`, `status_jogo` com tabelas de casos (R-UI-02, R-UI-05, R-UI-12). |
| Textos | `crates/nvt-ui/src/textos.rs` (migrado para `textos/mod.rs`), `crates/nvt-ui/src/textos/` | Textos de §5 revisados; R-UI-03, R-UI-04 e R-UI-14 verdes. |
| IPC v8 | `crates/nvt-ipc/`, `crates/nvt-service/src/nucleo.rs`, `crates/nvt-agente/src/ciclo.rs` | DTOs de §7.6 com validação e limites; v7 recusada; testes de envelope e de campos (R-S-03). `AgenteRelatar.saude` é `Option` com `#[serde(default)]`; requisições novas respondem `Erro { RecursoAusente }` em `nucleo.rs` até serem implementadas, para o workspace compilar ao fim do lote. |
| Proibições e inventário | `crates/nvt-core/tests/proibicoes.rs`, `crates/nvt-core/tests/inventario_ui.rs` | Termos de §8.3 vigiados, com levantamento dos usos atuais; exceção de `abrir.rs` declarada com teste da constante. |

### L2 — telas do redesenho e recomendadas

| Tarefa | Arquivos de escrita | Pronto quando |
|---|---|---|
| Início e boas-vindas | `crates/nvt-ui/src/ui/inicio.rs`, `crates/nvt-ui/src/ui/boas_vindas.rs`, `crates/nvt-ui/src/local.rs` | Todas as variações de R-UI-02; R-UI-06, R-UI-07, R-UI-09, R-UI-14; sem saúde a seção não aparece. |
| Jogos | `crates/nvt-ui/src/ui/jogos.rs`, `crates/nvt-ui/src/arquivo_jogo.rs` | Lista e detalhe (R-UI-12). |
| Avançado | `crates/nvt-ui/src/ui/avancado.rs`, `crates/nvt-ui/src/ui/legado.rs` (removido ao migrar) | Grupos de §5.5, menos Inicialização, Manutenção e Métricas; R-UI-08 verde. |
| Recomendadas no serviço | `crates/nvt-service/src/recomendadas.rs`, `crates/nvt-service/src/nucleo.rs`, `crates/nvt-service/src/testes.rs` | R-AR-01 a R-AR-06 com fachadas falsas. |
| Cliente e integração (responsável pelo lockfile) | `crates/nvt-ui/src/servico.rs`, `crates/nvt-ui/Cargo.toml`, `Cargo.lock` | Chamadas v8 e `RecursoAusente`; testes, fmt, Clippy e build do workspace verdes; capturas novas (R-UI-13). |

### L3 — saúde: leituras e limpeza

| Tarefa | Arquivos de escrita | Pronto quando |
|---|---|---|
| Leituras Windows | `crates/nvt-windows/src/saude.rs`, `crates/nvt-windows/src/saude/`, `crates/nvt-windows/src/lib.rs`, `crates/nvt-windows/Cargo.toml` | R-S-01 e R-S-05 por traits só de leitura; testes de fachada e de indisponibilidade. |
| Manutenção | `crates/nvt-manutencao/` (novo) | R-L-02 a R-L-07 e R-L-09; testes de junção, arquivo em uso e `Simulacao` byte a byte. |
| Saúde no agente | `crates/nvt-agente/src/saude.rs`, `crates/nvt-agente/src/saude/`, `crates/nvt-agente/src/ciclo.rs`, `crates/nvt-agente/src/ciclo_testes.rs`, `crates/nvt-agente/Cargo.toml` | R-S-02, R-S-07, sinais de sessão e limpeza de `%TEMP%` com portão; relato em `AgenteRelatar.saude`. |
| Saúde no serviço | `crates/nvt-service/src/saude.rs`, `crates/nvt-service/src/saude_testes.rs`, `crates/nvt-service/src/laco.rs`, `crates/nvt-service/src/usuario.rs`, `crates/nvt-service/src/usuario_testes.rs`, `crates/nvt-service/src/nucleo.rs`, `crates/nvt-service/Cargo.toml` | Sinais de máquina; limpeza de `C:\Windows\Temp` com portão e mutex MSI; `ObterSaude` por SID; `MarcarInicioVisto`. |
| Integração L3 (responsável pelo lockfile) | `Cargo.lock` | Atualizado depois das tarefas acima (o workspace já inclui `crates/*`); testes, fmt, Clippy e build verdes. |

### L4 — saúde na UI e integração

| Tarefa | Arquivos de escrita | Pronto quando |
|---|---|---|
| UI de saúde | `crates/nvt-ui/src/ui/inicio.rs`, `crates/nvt-ui/src/ui/avancado.rs`, `crates/nvt-ui/src/textos/saude.rs`, `crates/nvt-ui/src/abrir.rs` | Saúde no Início; grupos Inicialização e Manutenção; R-S-06 e R-L-08; bloqueio de `abrir.rs` testado (R-S-04). |
| Integração (responsável pelo lockfile) | `Cargo.lock`, `crates/nvt-service/src/main.rs` | Workspace com `nvt-manutencao`; testes, fmt, Clippy e build verdes; teste afirma `MODO_LIMPEZA = Desligada` no build normal. |

### L5 — medição (só build de teste)

| Tarefa | Arquivos de escrita | Pronto quando |
|---|---|---|
| Crate de medição | `crates/nvt-medicao/` (novo) | Trava de §8.4 itens 1 e 2 com verificação por script; agregação, engasgo, 1% low, retenção e formato (R-M-04 a R-M-07) testados sem ETW; ETW e NVML atrás de fachada. |
| Serviço e agente | `crates/nvt-service/src/partidas.rs`, `crates/nvt-service/Cargo.toml`, `crates/nvt-service/src/laco.rs`, `crates/nvt-agente/Cargo.toml`, `crates/nvt-manutencao/Cargo.toml` | Feature propagada; sessão ligada ao laço; `ListarPartidas`, `ObterPartida`, `ApagarPartidas`; sem a feature, `RecursoAusente` testado. |
| UI de partidas | `crates/nvt-ui/src/ui/partidas.rs`, `crates/nvt-ui/src/textos/partidas.rs`, `crates/nvt-ui/Cargo.toml` | Tela §5.6 e grupo Métricas sob `#[cfg]`; marcador "Build de teste". |
| Release e build de teste (responsável pelo lockfile) | `.github/workflows/release.yml`, `scripts/build-teste.ps1`, `instalador/build-msi.ps1`, `Cargo.toml`, `Cargo.lock` | Passo de §8.4 item 3; script gera binários e MSI `-teste`; ensaio local mostra o passo falhando com a feature ativa e passando sem ela. |

### L6 — documentação e laboratório

| Tarefa | Arquivos de escrita | Pronto quando |
|---|---|---|
| Documentação | `README.md`, `CHANGELOG.md`, `docs/arquitetura.md` | Sem emojis; limpeza descrita só como espaço em disco; partidas citadas como recurso em teste, fora do release. |
| Roteiro de laboratório | `docs/spec/interface-e-saude-laboratorio.md` | Roteiro de §10 com planilhas de registro. |

## 10. Plano de laboratório

### 10.1 Lab-UI

Capturas de todas as telas, variações do veredito e estados da linha em 1200x800 e
1920x1080. Teste com três pessoas leigas, sem instrução: responder "seu PC está bem?" e
"como desfazer o que o app fez?" em até 30 s cada.

### 10.2 Dry-run da limpeza (Lab-L)

1. Build de teste em duas máquinas (uma de uso diário, uma recém-instalada).
2. Antes: inventário com tamanho e datas de `%TEMP%` e `C:\Windows\Temp`.
3. Sete dias de `Simulacao`; no período, atualizar o driver NVIDIA, instalar um MSI e
   jogar um título com anticheat.
4. Conferir: nenhum arquivo sumiu por ação do nv-tuner; o relatório não inclui nada com
   menos de 7 dias, reparse point ou somente leitura; nenhuma passada com jogo aberto ou
   `_MSIExecute`; no máximo uma por dia; duração abaixo de 60 s.
5. Junção maliciosa: em `C:\Windows\Temp`, junção antiga apontando para pasta de teste com
   arquivos antigos; a simulação não a percorre.
6. Aprovação: zero violações em 4 e 5, e números de 4 coerentes com o inventário. Só então
   uma versão posterior muda o build normal para `Ativa`, depois de um ciclo curto em
   `Ativa` numa máquina descartável.

### 10.3 Custo de FPS da gravação (Lab-M)

Jogos mínimos: CS2, Valorant, Cyberpunk 2077, Fortnite.

1. Mesma máquina, driver, configuração e cena reprodutível (benchmark interno, demo ou
   rota fixa de 120 s); 5 min de repouso antes; temperatura inicial semelhante.
2. Referência externa: PresentMon de console ligado nas duas condições.
3. Condições A (build de teste com gravação desligada por `NVT_MEDICAO_DESLIGADA=1`, só
   lida no build de teste) e B (gravação ligada), na ordem ABBAABBAAB.
4. Registrar FPS médio e 1% low de cada execução pelo PresentMon.
5. Aprovação: em FPS médio e em 1% low, a diferença entre as médias de B e A fica dentro
   do desvio padrão de A e abaixo de 1%; CPU do serviço abaixo de 1% de um núcleo.
6. Exatidão: nv-tuner contra PresentMon com FPS médio dentro de 1% e 1% low dentro de 3%;
   perda de eventos abaixo de 1%; nenhuma sessão `nv-tuner-partidas` sobrando após
   encerrar o serviço à força e reiniciar.

### 10.4 Anticheat (Lab-A)

Vanguard (Valorant), EAC (Fortnite ou Apex), BattlEye (Rainbow Six Siege ou PUBG), VAC
com Trusted Mode (CS2), FACEIT (CS2 pelo cliente FACEIT), Ricochet (Call of Duty). Para
cada um, com gravação ligada: três partidas completas, casuais quando houver; conferir
que o jogo abre, que não há desconexão nem aviso do anticheat, que nada nos logs do
anticheat cita o nv-tuner e que a partida foi gravada. Repetir após atualização do
anticheat e reconferir as páginas oficiais (S8). Aprovação: zero incidentes. Um incidente
tira aquele anticheat da gravação ou encerra o recurso.

## 11. Questões em aberto

- Q1. NVML responde como LocalSystem na sessão 0 em todas as GPUs suportadas? Verificar
  no Lab-M; senão, NVAPI.
- Q2. `ProcessIdToSessionId` dá a sessão sem abrir handle em todas as versões suportadas,
  inclusive com troca rápida de usuário? Confirmar na documentação e em máquina.
- Q3. Formato de `StartupApproved` não é documentado: conferir no Windows 10 22H2 e
  Windows 11 24H2/25H2; divergência resulta em `habilitado = None`.
- Q4. Serviços automáticos de terceiros e apps MSIX com `StartupTask` ficam para depois?
  Proposta: sim.
- Q5. "Última versão do driver": lista embutida no catálogo a cada release, sem rede
  nova, com texto "pode haver versão mais nova". Confirmar.
- Q6. `chart::LineChart` com 3.600 pontos: reduzir para um ponto a cada 10 s na tela?
- Q7. Limiares de "Atenção" para memória (proposta: > 85% comprometida) e CPU em repouso
  (> 15% em 5 min) precisam de calibração.
- Q9. (Decidida: D7.) "Desfazer tudo" deve também retirar os aceites feitos por "Aplicar melhorias
  recomendadas"? Hoje (R-AR-06) volta ao original e pausa, mas "Voltar a ajustar" reaplica
  os aceites. Alternativa: `RestaurarOriginal` limpa os aceites experimentais do SID.
- Q10. (Decidida: D8.) A v0.3.0 (só redesenho) já leva IPC v8 com `ObterSaude` e afins respondidos
  `RecursoAusente`, evitando nova troca de protocolo depois. Alternativa: v0.3.0 em v7.
- Q11. (Decidida: D9.) Dry-run da limpeza depende da feature de medição (`build-de-teste` só ligada por
  `medicao-partidas`): validar a limpeza exige o build com gravação. Separar as duas travas?
- Q8. Com a limpeza `Ativa`, o Início cita o espaço liberado só acima de 1 GB? Proposta:
  sim.
