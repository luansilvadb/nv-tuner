# Ajustes do Windows 10/11 para jogos competitivos — pesquisa

> Escopo: ajustes na camada do **Windows** (não do driver NVIDIA, que o nv-tuner já cobre) que melhoram de forma mensurável a fluidez (frametime/1% low) e a responsividade (latência de entrada até a imagem) em jogos competitivos. Overclock e undervolt estão fora do escopo.
>
> Critérios para recomendar: (1) há evidência primária ou medição; (2) dá para **detectar** o estado atual e **reverter** com precisão ao valor original; (3) não conflita com anticheat (Vanguard, FACEIT, EAC, BattlEye, VAC).
>
> Data da pesquisa: 2026-11-26. Valores de registro não documentados pela Microsoft estão marcados como tal.

## 1. Tabela-resumo

| # | Ajuste | Classificação | Condição / observação | Admin | Reinício |
|---|---|---|---|---|---|
| 1 | Taxa de atualização do monitor no máximo | **Recomendado** | Erro mais comum e de maior impacto; detectar modo abaixo do máximo | Não | Não |
| 2 | Aceleração do mouse (Enhance pointer precision) desligada | **Recomendado** | Sem efeito em jogos com raw input; afeta jogos sem raw input e menus | Não | Não (via `SPI_SETMOUSE`) |
| 3 | Game Mode ligado (padrão) | **Recomendado** (manter/restaurar o padrão) | Ganho em 1% low com carga em segundo plano; necessário para o scheduler de Ryzen X3D | Não | Não |
| 4 | Gravação em segundo plano (Game DVR “gravar o que aconteceu”) desligada | **Recomendado** | Desligar só a captura; **não** remover a Game Bar (Ryzen X3D depende dela) | Não | Não |
| 5 | “Otimizações para jogos em janela” (flip model para DX10/11) ligada | **Recomendado** (Win11) | Reduz latência em janela/sem bordas; opt-out por jogo se houver problema | Não | Não (reabrir o jogo) |
| 6 | Remover `useplatformclock` do BCD se estiver `Yes` | **Recomendado** (corretivo) | Só agir se alguém já forçou HPET; não “otimizar” além disso | Sim | Sim |
| 7 | Plano de energia Alto desempenho | **Condicional** | Intel/AMD de CCD único em desktop, e só se medição mostrar ganho; **não** em Ryzen dual‑CCD X3D (exige Equilibrado) nem em notebook na bateria | Não (troca) / Sim (criar Desempenho máximo) | Não |
| 8 | Desativar core parking (CPMINCORES=100%) | **Condicional** | Mesmas condições do item 7; nunca em X3D dual‑CCD | Não* | Não |
| 9 | HAGS | **Condicional** | **Exigido** para DLSS Frame Generation; fora isso, efeito pequeno e dependente de driver — manter o padrão do sistema | Não* | **Sim** |
| 10 | VRR do Windows (“Taxa de atualização variável”) | **Condicional** | Só com monitor G‑SYNC/FreeSync e jogos DX11/DX12 sem suporte nativo a VRR | Não | Não |
| 11 | Taxa de atualização dinâmica (DRR, Win11) desligada | **Condicional** | Só se DRR estiver ligado e limitando jogos sem VRR (documentado pela Microsoft) | Não | Não |
| 12 | Desativar MPO | **Condicional** (corretivo) | Só com sintoma: cintilação, tela preta, stutter em sem bordas/multimonitor. Arquivo oficial da NVIDIA; relatos de que não funciona mais em builds recentes | Sim | Sim |
| 13 | Desativar Fullscreen Optimizations por jogo | **Condicional** | Só por executável, só se houver problema medido; global = mito | Não | Não |
| 14 | Prioridade via IFEO `PerfOptions` | **Condicional** (baixo ganho) | Só jogo CPU‑bound com carga em segundo plano; Game Mode já cobre a maior parte | Sim | Não (vale no próximo lançamento) |
| 15 | Taxa de polling do mouse | **Condicional** (fora do Windows) | ≥1000 Hz sempre; 2000–8000 Hz em monitores 240 Hz+ com CPU folgada. Configurado no software do mouse; o app só pode orientar | — | — |
| 16 | `GlobalTimerResolutionRequests` / ferramentas de timer | **Mito ou prejudicial** | Não documentado; jogos já pedem a resolução que precisam | Sim | Sim |
| 17 | `disabledynamictick` / `useplatformtick` | **Mito ou prejudicial** | Resultados contraditórios, sem medição controlada; mais consumo | Sim | Sim |
| 18 | Forçar HPET (`useplatformclock yes`) | **Prejudicial** | Aumenta o custo do `QueryPerformanceCounter` | Sim | Sim |
| 19 | MSI mode / afinidade de interrupção da GPU | **Mito** (para GPUs atuais) / afinidade **prejudicial** | Drivers atuais já usam MSI; só detectar e informar | Sim | Sim |
| 20 | Nagle (`TcpAckFrequency`/`TCPNoDelay`), `NetworkThrottlingIndex` | **Mito** para FPS competitivos | Jogos competitivos usam UDP; só afeta TCP | Sim | Sim |
| 21 | Desligar Memory Integrity/VBS/HVCI | **Prejudicial** no contexto competitivo | Vanguard e FACEIT podem **exigir** VBS/HVCI; ganho típico ≤ ~3–5% | Sim | Sim |
| 22 | Desativar “serviços em segundo plano” em massa | **Mito ou prejudicial** | Quebra atualizações, busca, áudio, anticheat; ganho não mensurável | Sim | Às vezes |
| 23 | Desabilitar idle do processador (`IDLEDISABLE`) | **Prejudicial** | Rouba margem térmica do boost; mais calor e consumo | Não* | Não |

\* `powercfg` alterando o plano do usuário em geral não exige admin; HAGS e chaves em HKLM exigem.

Contexto importante: os ganhos de latência ordenados por tamanho são, de longe, (a) taxa de atualização correta + VRR configurado + fila de render curta (NVIDIA Reflex/limitador — camada do driver/jogo), (b) modo de apresentação (exclusivo ou flip independente, sem composição do DWM), (c) mouse sem aceleração e com polling adequado. Os “tweaks” de kernel (timer, HPET, MSI, prioridade) somam, quando somam, frações de milissegundo e são os que mais produzem efeitos colaterais.

**Ferramenta de validação**: PresentMon (Intel, código aberto) e CapFrameX mostram a coluna **PresentMode** (`Hardware: Independent Flip`, `Hardware Composed: Independent Flip`, `Composed: Flip`, `Composed: Copy with GPU GDI`) e as métricas de latência (`MsPCLatency`/`MsClickToPhotonLatency` em versões recentes). Qualquer ajuste de apresentação deve ser validado por essa coluna, não por percepção.

---

## 2. Detalhes por item

### 2.1 Taxa de atualização abaixo do máximo — **Recomendado**

- **Efeito**: monitor 144/240 Hz rodando a 60 Hz é o erro de configuração mais comum (padrão após troca de cabo, driver novo, atualização do Windows, uso de HDMI antigo). A latência média de exibição cai proporcionalmente ao período de quadro (16,7 ms → 4,2 ms em 240 Hz) e a nitidez em movimento melhora. Relatos na Microsoft Q&A confirmam atualizações do Windows derrubando a taxa para 60 Hz.
- **Mito?** Não; é física do display.
- **Riscos**: modos marcados com asterisco no Windows mudam a resolução; modos “overclockados” do monitor podem falhar. Escolher só modos enumerados pelo EDID na resolução nativa.
- **Detecção**: `EnumDisplaySettingsExW(ENUM_CURRENT_SETTINGS)` vs. maior `dmDisplayFrequency` em `EnumDisplaySettingsExW(i)` na mesma resolução/bits; ou `QueryDisplayConfig` (precisão racional, ex.: 143,98 Hz). Verificar também se o cabo/porta limita (DP vs. HDMI).
- **Aplicar/reverter**: `ChangeDisplaySettingsExW(device, &mode, NULL, CDS_UPDATEREGISTRY, NULL)`; guardar o `DEVMODE` original (largura, altura, frequência, bits) e reaplicar para reverter. Sem admin, sem reinício.
- **Anticheat**: nenhum.
- Relacionado — **DRR (Win11)**: a Microsoft documenta que a taxa de atualização dinâmica “pode limitar a taxa máxima de outros jogos” que não são VRR e recomenda desligá-la nesses casos. Configurações > Sistema > Tela > Tela avançada. Condicional (item 11).

### 2.2 Aceleração do mouse (Aprimorar precisão do ponteiro) — **Recomendado**

- **Efeito**: com a opção ligada, o Windows aplica uma curva de aceleração ao ponteiro; o mesmo movimento físico gera deslocamentos diferentes conforme a velocidade, prejudicando memória muscular. Jogos com **raw input** (CS2, Valorant, Apex, Overwatch etc.) ignoram a curva; jogos sem raw input, menus e jogos que usam o cursor do sistema (MOBAs, RTS) são afetados.
- **Mito?** Não, mas o efeito se limita a jogos sem raw input.
- **Chaves** (`HKCU\Control Panel\Mouse`, tipo REG_SZ): padrão ligado = `MouseSpeed "1"`, `MouseThreshold1 "6"`, `MouseThreshold2 "10"`; desligado = `"0"`, `"0"`, `"0"`.
- **Aplicar/reverter**: preferir `SystemParametersInfoW(SPI_GETMOUSE)` para ler o trio original e `SPI_SETMOUSE` com `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE` para aplicar/restaurar (efeito imediato). Escrever só o registro exige novo logon. Também preservar `MouseSensitivity` (velocidade do ponteiro, padrão `"10"`); não mexer.
- **Admin/reinício**: não/não. **Anticheat**: nenhum.

### 2.3 Game Mode — **Recomendado (manter ligado, que é o padrão)**

- **Efeito**: a Microsoft descreve o Game Mode como dar ao jogo prioridade/“acesso mais exclusivo” a recursos e suprimir Windows Update/notificações durante o jogo. Medições independentes mostram FPS médio praticamente igual e melhora de 1% low quando há carga em segundo plano; há relatos isolados de piora em jogos CPU‑bound. Os testes de G‑SYNC 101 da Blur Busters não encontraram diferença mensurável de input lag entre combinações de Game Mode, Game Bar e FSO.
- **Por que manter**: em Ryzen dual‑CCD com 3D V‑Cache (7950X3D, 9950X3D etc.) o driver AMD 3D V‑Cache Performance Optimizer usa a detecção de jogo da Game Bar/Game Mode para estacionar o CCD sem V‑Cache; desligar degrada o desempenho.
- **Chave**: `HKCU\Software\Microsoft\GameBar\AutoGameModeEnabled` (DWORD; ausente ou `1` = ligado; `0` = desligado).
- **Reverter**: restaurar o valor original ou apagar o valor se ele não existia. Sem admin, sem reinício.
- **Anticheat**: nenhum.

### 2.4 Xbox Game Bar / Game DVR / captura em segundo plano — **Recomendado: desligar só a gravação em segundo plano**

- **Efeito**: o blog oficial do Xbox afirma que a gravação em segundo plano “provavelmente prejudica a taxa de quadros em todas as GPUs, exceto as mais potentes” e, por isso, vem desligada por padrão. A Game Bar em si (overlay) não mostrou diferença de input lag nos testes da Blur Busters.
- **Não remover a Game Bar**: quebra a detecção de jogos usada por Game Mode e pelo driver de V‑Cache da AMD.
- **Chaves** (HKCU, sem admin):
  - `HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR\HistoricalCaptureEnabled` (DWORD; `1` = gravar o que aconteceu; `0` = desligado) — é esta a que importa.
  - `HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR\AppCaptureEnabled` (DWORD; captura em geral).
  - `HKCU\System\GameConfigStore\GameDVR_Enabled` (DWORD). Fontes divergem sobre a semântica em builds recentes — tratar como espelho do `AppCaptureEnabled`.
  - Política: `HKLM\SOFTWARE\Policies\Microsoft\Windows\GameDVR\AllowGameDVR = 0` (admin). **Evitar**: política desabilita a funcionalidade inteira e aparece como “gerenciado pela organização”.
- **Detecção/reversão**: ler os valores (ou ausência) antes; restaurar exatamente, apagando os que não existiam. Preferir a interface oficial (Configurações > Jogos > Capturas), que é o que a Microsoft recomenda.
- **Anticheat**: nenhum.

### 2.5 “Otimizações para jogos em janela” (flip model) e Fullscreen Optimizations — **Recomendado (Win11) / FSO por jogo condicional**

- **Efeito (Otimizações para jogos em janela)**: o DirectX dev blog (fev/2022) explica que jogos DX10/DX11 em janela ou sem bordas que usam o modelo *blt* passam ao modelo *flip*, o que reduz latência (o DWM deixa de copiar o quadro) e habilita VRR e Auto HDR em janela. Jogos DX12 já usam flip. Com flip model e *Independent Flip* a latência pode chegar a 1 quadro, equivalente ao exclusivo.
- **Chave**: `HKCU\Software\Microsoft\DirectX\UserGpuPreferences`, valor `DirectXUserGlobalSettings` (REG_SZ, lista `chave=valor;`), token `SwapEffectUpgradeEnable=1;` (liga) / `=0;` (desliga). **Opt‑out por jogo**: valor nomeado com o caminho completo do executável no mesmo key, contendo `SwapEffectUpgradeEnable=0;`. Atenção: a mesma string guarda `VRROptimizeEnable` e `AutoHDREnable` — ler, alterar só o token e regravar preservando o resto. Ligar Auto HDR liga isto automaticamente.
- **Fullscreen Optimizations (FSO)**: o blog “Demystifying Full Screen Optimizations” (2019) explica que, com FSO, um jogo em “exclusivo” roda como janela sem bordas em flip independente, com desempenho equivalente ao exclusivo e permitindo overlays. Testes na Blur Busters: sem aumento de input lag médio de FSO vs. exclusivo real quando a apresentação está em Independent Flip; quando a apresentação cai para composição (`Composed: Flip`), há ~1 quadro a mais.
  - **Desligar por jogo** (condicional, só se PresentMon mostrar `Composed` ou houver problema): `HKCU\Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers`, valor = caminho do .exe, dado `~ DISABLEDXMAXIMIZEDWINDOWEDMODE` (combinar com flags já existentes, ex.: `~ HIGHDPIAWARE DISABLEDXMAXIMIZEDWINDOWEDMODE`). Reverter: remover só o token; apagar o valor se ficar vazio.
  - **Global** (`HKCU\System\GameConfigStore\GameDVR_FSEBehaviorMode=2`, `GameDVR_HonorUserFSEBehaviorMode`, `GameDVR_DXGIHonorFSEWindowsCompatible`): não documentado, ignorado em boa parte do Win11 — **mito**.
- **Admin/reinício**: não/não (reabrir o jogo). **Anticheat**: nenhum conhecido; os executáveis não são alterados.

### 2.6 VRR do Windows — **Condicional**

- **Efeito**: a opção “Taxa de atualização variável” (Win10 1903+) estende VRR a jogos DX11 em tela cheia que não suportam VRR nativamente (DirectX dev blog, 2019). Não substitui o G‑SYNC/FreeSync do driver; é complementar.
- **Condição**: monitor com VRR ativo no driver; jogo sem suporte nativo. Sem monitor VRR a opção não aparece.
- **Chave**: token `VRROptimizeEnable=1;` / `=0;` em `HKCU\Software\Microsoft\DirectX\UserGpuPreferences\DirectXUserGlobalSettings` (mesma string do item 2.5). Reverter restaurando o token original (ou removendo se ausente).
- **Admin/reinício**: não/não. **Anticheat**: nenhum.

### 2.7 Plano de energia, core parking e idle do processador — **Condicional / idle = prejudicial**

- **Efeito**: em desktop moderno, Alto desempenho/Desempenho máximo dão pouco ou nenhum ganho de FPS médio; o benefício possível é reduzir latência de “despertar” de núcleos (core parking/EPP) em cargas intermitentes, perceptível em alguns jogos CPU‑bound como 1% low. Não há benchmark controlado de fonte primária que isole o efeito; tratar como “medir antes”.
- **Contraindicações**:
  - **Ryzen dual‑CCD X3D**: a AMD e a comunidade apontam o plano **Equilibrado** como necessário para o driver estacionar o CCD sem cache; Alto desempenho desliga o parking e espalha o jogo pelos dois CCDs (pior).
  - Intel híbrido (12ª geração+) com Thread Director: o plano Equilibrado já é ajustado; mudanças de EPP/parking podem piorar a alocação P/E‑cores.
  - Notebook na bateria: consumo/temperatura.
- **GUIDs**: Equilibrado `381b4222-f694-41f0-9685-ff5bb260df2e`; Alto desempenho `8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c`; Desempenho máximo (modelo) `e9a42b02-d5df-448d-aa00-03f14749eb61` — exige `powercfg -duplicatescheme e9a42b02-d5df-448d-aa00-03f14749eb61` (cria um GUID novo).
- **Core parking**: `powercfg /setacvalueindex SCHEME_CURRENT SUB_PROCESSOR CPMINCORES 100` + `powercfg /setactive SCHEME_CURRENT` (GUID do parâmetro `0cc5b647-c1df-4637-891a-dec35c318583`). Verificação: Monitor de Recursos (núcleos “Estacionado”).
- **Idle do processador** (`IDLEDISABLE`, `5d76a2ca-e8c0-402f-a133-2158492d58ad`): desativar C‑states mantém os núcleos quentes e reduz a margem de boost — **prejudicial**.
- **Detecção/reversão**: `powercfg /getactivescheme` (guardar GUID); `powercfg /query SCHEME_CURRENT SUB_PROCESSOR CPMINCORES` (guardar índice AC/DC). Reverter com `powercfg /setactive <GUID original>`, `powercfg /setacvalueindex ... <valor original>` e, se o app criou um plano duplicado, `powercfg /delete <GUID criado>`. Preferir alterar um plano criado pelo app a modificar o plano do usuário.
- **Anticheat**: nenhum.

### 2.8 HAGS (Hardware‑accelerated GPU scheduling) — **Condicional**

- **Efeito**: o DirectX dev blog (jun/2020) descreve a transferência do agendamento para um processador dedicado na GPU, reduzindo overhead e a necessidade de bufferização. Medições (GamersNexus, Puget, PCGH) mostram diferenças pequenas e dependentes de GPU/driver em FPS e frametime. **NVIDIA DLSS Frame Generation exige HAGS ligado** — é o critério decisivo para o nv-tuner.
- **Chave**: `HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers\HwSchMode` (DWORD; `2` = ligado, `1` = desligado; ausente = padrão do driver/sistema). Requer WDDM 2.7+.
- **Riscos**: relatos esparsos de stutter/instabilidade com captura/OBS ou drivers antigos.
- **Detecção/reversão**: ler o valor (ou ausência); restaurar/apagar. Admin e **reinício** obrigatórios. Na interface: Configurações > Sistema > Tela > Gráficos > Configurações padrão.
- **Anticheat**: nenhum.

### 2.9 MPO (multiplane overlay) — **Condicional (corretivo)**

- **Efeito**: MPO permite que o hardware de exibição componha planos sem o DWM, e é o que viabiliza `Hardware Composed: Independent Flip` em sem bordas — ou seja, ajuda latência quando funciona. Problemas conhecidos (vários drivers, sobretudo multimonitor e taxas mistas): cintilação, telas pretas, stutter em janelas sem bordas, artefatos em navegadores/Discord.
- **Ação**: a NVIDIA publica um artigo de suporte (“What is Multi‑Plane Overlay (MPO) in Windows 11”) com `mpo_disable.reg` e `mpo_restore.reg`. O valor conhecido é `HKLM\SOFTWARE\Microsoft\Windows\Dwm\OverlayTestMode = 5` (DWORD), **não documentado pela Microsoft**; há relatos (TechPowerUp, 2025) de que deixou de ter efeito em builds recentes do Win11.
- **Condição**: só com sintoma reproduzível. Desligar sem sintoma pode piorar latência em sem bordas.
- **Reverter**: apagar `OverlayTestMode` (o padrão é ausente). Admin; reinício (ou reiniciar DWM).
- **Anticheat**: nenhum.

### 2.10 Resolução do timer — **Mito ou prejudicial**

- **Fato documentado** (Microsoft Learn, `timeBeginPeriod`): desde o Windows 10 2004 a função não altera mais a resolução global; cada processo recebe a resolução que pediu. No Windows 11, se a janela do processo estiver oculta/minimizada/inaudível, o Windows não garante resolução maior que a padrão. Resolução maior aumenta trocas de contexto e consumo.
- **Implicação**: jogos competitivos já chamam `timeBeginPeriod(1)` ou usam waitable timers de alta resolução; ferramentas externas de “timer resolution” não afetam o jogo desde 2004.
- **`GlobalTimerResolutionRequests`** (`HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\kernel`, DWORD `1`): restauraria o comportamento global antigo; **não documentado** pela Microsoft, sem medição controlada de ganho. Não aplicar. Reverter: apagar o valor; reinício.
- **Anticheat**: nenhum, mas sem benefício comprovado.

### 2.11 HPET e `bcdedit` (`useplatformclock`, `disabledynamictick`, `useplatformtick`) — **Corretivo / mito**

- **Fato**: com `useplatformclock yes`, o `QueryPerformanceCounter` passa a usar o HPET, que tem acesso muito mais caro que o TSC invariante; jogos chamam QPC milhares de vezes por segundo, então isso tende a piorar frametime. O padrão do Windows (valor ausente) já escolhe a melhor fonte.
- **Recomendação**: se `bcdedit /enum {current}` mostrar `useplatformclock Yes`, remover com `bcdedit /deletevalue {current} useplatformclock` (volta ao padrão). Desligar HPET na BIOS ou no Gerenciador de Dispositivos: não fazer.
- **`disabledynamictick yes`** e **`useplatformtick yes`**: resultados conflitantes em fóruns (Blur Busters, Overclock.net — inclusive relato de mais input lag com `useplatformtick`), sem medição controlada; aumentam consumo. **Mito**.
- **Riscos**: alterar BCD com BitLocker ativo pode, em algumas configurações, exigir a chave de recuperação no próximo boot — suspender o BitLocker antes ou não mexer. Nunca usar `testsigning`/`nointegritychecks`: desligam a verificação de assinatura e **todos** os anticheats de kernel recusam rodar.
- **Detecção/reversão**: ler `bcdedit /enum {current}`; guardar a presença/valor original; reverter com `bcdedit /set {current} <opção> <valor original>` ou `/deletevalue`. Admin e reinício.

### 2.12 Prioridade de processo via IFEO `PerfOptions` — **Condicional (baixo ganho)**

- **Mecanismo**: `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\<jogo>.exe\PerfOptions`, DWORD `CpuPriorityClass` (`1` ociosa, `2` normal, `3` alta, `5` abaixo do normal, `6` acima do normal; **nunca** `4` tempo real) e `IoPriority` (`0`–`2`). Pouco documentado; aplicado na criação do processo.
- **Efeito**: só ajuda quando há competição de CPU com processos em segundo plano; o Game Mode já cobre isso. Sem medição primária de ganho em jogo competitivo. Prioridade alta pode atrasar threads de áudio/entrada de outros processos.
- **Anticheat**: não há relato confiável de banimento por `PerfOptions`. Porém a mesma chave IFEO hospeda o valor `Debugger`, vetor clássico de malware; EDRs/antivírus monitoram a chave. Alguns jogos com EAC resetam a prioridade. Nunca criar `Debugger`, `GlobalFlag` etc.
- **Reverter**: apagar a subchave `PerfOptions` (e a chave do .exe, se foi o app que a criou e ela não tem outros valores). Admin; vale no próximo lançamento do jogo.

### 2.13 MSI mode e afinidade de interrupção da GPU — **Mito (GPUs atuais) / afinidade prejudicial**

- **Mecanismo** (Microsoft Learn, “Enabling Message‑Signaled Interrupts in the Registry”): `HKLM\SYSTEM\CurrentControlSet\Enum\PCI\<VEN_10DE&DEV_...>\<instância>\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties\MSISupported` (DWORD `1`). A Microsoft documenta que o valor é definido pelo INF do driver — instalações de driver sobrescrevem mudanças manuais.
- **Realidade**: drivers NVIDIA atuais já habilitam MSI nas GPUs modernas; o ajuste vinha de uma época em que GPUs usavam interrupção por linha (INTx) compartilhada. Para o nv-tuner: **detectar e informar** (IRQ negativo em “Recursos por conexão”, ou o valor acima), não alterar.
- **Afinidade** (`Interrupt Management\Affinity Policy\DevicePolicy`, `AssignmentSetOverride`): fixar interrupções num núcleo específico pode colidir com o núcleo principal do jogo ou com núcleo estacionado; sem evidência de ganho. **Prejudicial/mito**.
- **Reverter**: restaurar/apagar os valores originais; admin e reinício. Risco de tela preta se o dispositivo não suportar o modo.

### 2.14 Nagle e ajustes de rede — **Mito para FPS competitivos**

- **Mecanismo**: `HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{GUID}\TcpAckFrequency = 1` e `TCPNoDelay = 1` (KB 328890 documenta `TcpAckFrequency`). Afetam **apenas TCP**.
- **Realidade**: CS2, Valorant, Apex, Overwatch, Fortnite, LoL usam **UDP** para o tráfego de jogo; Nagle e ACK atrasado não existem em UDP. O relato de ganho vem de MMOs antigos em TCP (WoW, Diablo III). Efeito colateral: mais pacotes pequenos e pior desempenho de transferências.
- **`NetworkThrottlingIndex`** e **`SystemResponsiveness`** (`HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile`, padrões `10` e `20`): afetam o MMCSS para tráfego multimídia; sem medição de ganho em jogos. **Mito**.
- **Reverter**: apagar os valores criados. Admin; reinício ou reinício do adaptador.

### 2.15 Memory Integrity / VBS / HVCI — **Prejudicial desligar (contexto competitivo)**

- **Microsoft**: o artigo de suporte “Opções para otimizar o desempenho de jogos no Windows 11” admite impacto de Memory Integrity e da Plataforma de Máquina Virtual em “alguns cenários e configurações” e recomenda **religar após jogar**. Benchmarks (Tom's Hardware) mostraram ~3% de diferença média com ambos desligados; maior em CPUs antigas sem MBEC/GMET.
- **Anticheats que exigem o contrário**:
  - **Riot Vanguard** (Valorant, LoL, 2XKO): TPM 2.0 e Secure Boot são exigidos no Windows 11 (erro VAN9001/VAN9003). O erro **VAN: RESTRICTION: 5** significa que o Vanguard não conseguiu confirmar que Memory Integrity (HVCI/VBS) está ligado — aplicado a contas/máquinas sob restrição. O modo **On‑Demand** (Vanguard sem iniciar no boot) exige Win11 25H2+, Secure Boot, TPM 2.0, IOMMU, VBS e HVCI.
  - **FACEIT**: TPM 2.0 e Secure Boot obrigatórios para todos desde 25/11/2025; IOMMU e VBS exigidos em ondas (inicialmente acima de 3.000 de Elo, ago/2025), HVCI quando o cliente sinaliza.
- **Recomendação ao nv-tuner**: **somente detectar e informar**, inclusive avisando quando VBS/HVCI estiverem **desligados** e um jogo Riot/FACEIT estiver instalado. Nunca desligar automaticamente.
- **Detecção**: `Get-CimInstance -Namespace root\Microsoft\Windows\DeviceGuard -ClassName Win32_DeviceGuard` (`VirtualizationBasedSecurityStatus`: 2 = rodando; `SecurityServicesRunning` contém 2 = HVCI). Registro: `HKLM\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity\Enabled`. TPM: `Get-Tpm`; Secure Boot: `Confirm-SecureBootUEFI`.
- **Reverter** (se o usuário desligou manualmente): Segurança do Windows > Segurança do dispositivo > Isolamento de núcleo > Integridade de memória; reinício.

### 2.16 Serviços em segundo plano — **Mito ou prejudicial**

- Desligar SysMain, Windows Search, Spooler, telemetria etc. não tem ganho mensurável em frametime num sistema com SSD e RAM suficientes, e quebra funções (atualizações, busca, impressão) e às vezes o próprio anticheat (Vanguard `vgc`, FACEIT, EAC/BattlEye dependem de serviços e do Windows Update/CI). O Game Mode já adia Windows Update e notificações durante o jogo. Ganho real vem de **fechar** overlays/apps concorrentes (gravadores, RGB, navegadores com vídeo), o que o app pode no máximo sugerir.

### 2.17 Taxa de polling do mouse — **Condicional (fora do Windows)**

- **Efeito**: 125 Hz → 1000 Hz reduz a latência média de amostragem de ~4 ms para ~0,5 ms e o “micro‑stutter” do movimento em telas de alta frequência. Acima disso (2000–8000 Hz) o ganho de latência é < 0,5 ms, mas a suavidade em 240–500 Hz melhora (Blur Busters defende ≥2000 Hz para 360 Hz+). Custo de CPU é real a 8000 Hz (relatos de queda de FPS em CPUs fracas ou FPS descapado) e requer DPI suficiente para gerar dados.
- **Windows**: não controla a taxa (é do firmware/software do fabricante). O app pode **medir** (intervalo entre `WM_INPUT`) e orientar. Sem impacto em anticheat.

---

## 3. Notas de implementação para o nv-tuner

- **Snapshot antes de tudo**: para cada item, gravar “existia? tipo? valor?” e reverter restaurando exatamente (apagando o que não existia). Strings compostas (`DirectXUserGlobalSettings`, `AppCompatFlags\Layers`) devem ser editadas por token.
- **Escopo HKCU primeiro**: itens 1–5, 10, 11, 13 funcionam sem admin e sem reinício — boa primeira entrega. HKLM/BCD (6, 9, 12, 14) ficam no serviço elevado, sinalizando reinício pendente.
- **Detecção de CPU**: verificar X3D dual‑CCD (nome do processador + presença do driver `amd3dvcache`) e Intel híbrido antes de sugerir qualquer mudança de plano de energia.
- **Validação**: integrar leitura do PresentMon (`PresentMode`) para confirmar flip independente após ajustes de apresentação.
- **Nada que toque segurança de boot/kernel** (VBS, HVCI, Secure Boot, assinatura de drivers, `testsigning`) — apenas diagnóstico.

## 4. Fontes

Primárias (Microsoft, NVIDIA, AMD, Riot, FACEIT):
- [DirectX Dev Blog — Updates in Graphics and Gaming (otimizações para jogos em janela, fev/2022)](https://devblogs.microsoft.com/directx/updates-in-graphics-and-gaming/)
- [Microsoft Support — Optimizations for windowed games in Windows 11](https://support.microsoft.com/topic/3f006843-2c7e-4ed0-9a5e-f9389e535952)
- [DirectX Dev Blog — DXGI flip model](https://devblogs.microsoft.com/directx/dxgi-flip-model/)
- [Microsoft Learn — For best performance, use DXGI flip model](https://learn.microsoft.com/windows/win32/direct3ddxgi/for-best-performance--use-dxgi-flip-model)
- [DirectX Dev Blog — Demystifying Full Screen Optimizations (2019)](https://devblogs.microsoft.com/directx/demystifying-full-screen-optimizations/)
- [DirectX Dev Blog — Hardware Accelerated GPU Scheduling (2020)](https://devblogs.microsoft.com/directx/hardware-accelerated-gpu-scheduling/)
- [DirectX Dev Blog — OS Variable Refresh Rate (2019)](https://devblogs.microsoft.com/directx/os-variable-refresh-rate/)
- [Microsoft Support — Alterar a taxa de atualização do monitor (inclui DRR)](https://support.microsoft.com/pt-BR/Windows/Hardware/Display-Graphics/change-the-refresh-rate-on-your-monitor-in-windows)
- [Microsoft Learn — timeBeginPeriod](https://learn.microsoft.com/windows/win32/api/timeapi/nf-timeapi-timebeginperiod)
- [Microsoft Learn — Timer Resolution](https://learn.microsoft.com/en-us/windows/win32/multimedia/timer-resolution)
- [Microsoft Learn — Enabling Message‑Signaled Interrupts in the Registry](https://learn.microsoft.com/windows-hardware/drivers/kernel/enabling-message-signaled-interrupts-in-the-registry)
- [Microsoft Learn — Multiplane overlay support](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/multiplane-overlay-support)
- [Microsoft Support — Options to Optimize Gaming Performance in Windows 11 (Memory Integrity / VMP)](https://support.microsoft.com/en-us/topic/a255f612-2949-4373-a566-ff6f3f474613)
- [Microsoft Support — KB 328890, TcpAckFrequency](https://support.microsoft.com/kb/328890)
- [Xbox Wire — Tips on Windows 10 and Game DVR optimization](https://news.xbox.com/en-us/2016/10/14/tips-on-windows-10-and-game-dvr-optimization)
- [NVIDIA — What is Multi‑Plane Overlay (MPO) in Windows 11 (mpo_disable.reg / mpo_restore.reg)](https://nvidia.custhelp.com/app/answers/detail/a_id/5157/)
- [Riot — Vanguard Security Requirements](https://support.riotgames.com/en-us/riot/performance/vanguard-security-requirements)
- [Riot — Vanguard Pre‑Check (On‑Demand)](https://support.riotgames.com/en-us/riot/support-tools/vanguard-pre-check)
- [Riot — Error VAN: RESTRICTION: 5 (HVCI/VBS)](https://support.riotgames.com/riot/penalties/error-van-restriction-5)
- [FACEIT — Windows Security Requirements FAQ](https://support.faceit.com/hc/en-us/articles/23117181142556)
- [FACEIT — Como verificar se o PC precisa de mudanças](https://support.faceit.com/hc/en-us/articles/23117375791772-How-can-I-check-if-I-need-to-make-any-changes-to-my-PC-before-using-the-FACEIT-Anti-Cheat)
- [FACEIT — Known issues with Anti‑Cheat Requirements](https://support.faceit.com/hc/en-us/articles/22851956652956-Known-issues-with-Anti-Cheat-Requirements)

Medições e fóruns técnicos:
- [Blur Busters Forums — FSO vs. Exclusive Fullscreen](https://forums.blurbusters.com/viewtopic.php?p=112998)
- [Blur Busters Forums — Do Fullscreen Optimizations Affect Input Latency?](https://forums.blurbusters.com/viewtopic.php?p=109120)
- [Blur Busters Forums — Mouse 1000 Hz vs 8000 Hz](https://forums.blurbusters.com/viewtopic.php?p=96747)
- [Blur Busters Forums — Polling rate analysis: 1000 Hz is not enough](https://forums.blurbusters.com/viewtopic.php?p=46265)
- [Blur Busters Forums — useplatformclock (Windows stutters)](https://forums.blurbusters.com/viewtopic.php?p=103138)
- [Blur Busters Forums — BCDEdit timer settings on Windows 11 / Ryzen X3D](https://forums.blurbusters.com/viewtopic.php?p=126918)
- [Overclock.net — useplatformtick yes causes input lag](https://www.overclock.net/threads/win-10-1909-bcdedit-set-useplatformtick-yes-causes-input-lag.1742922/page-5)
- [GamersNexus — Windows 10 HAGS benchmarks (frametimes)](https://gamersnexus.net/guides/3599-windows-10-hardware-accelerated-gpu-scheduling-benchmarks)
- [PCGH — HAGS im Hands‑on‑Test](https://www.pcgameshardware.de/Windows-10-Software-259581/Specials/HAGS-Benchmark-Test-1352947/)
- [Tom's Hardware — Benchmarked: Windows 11 VBS/HVCI e desempenho](https://www.tomshardware.com/news/windows-11-gaming-benchmarks-performance-vbs-hvci-security)
- [TechPowerUp Forums — Disabling MPO in 2025](https://www.techpowerup.com/forums/threads/disabling-mpo-multiplane-overlay-in-2025.331858/page-2)
- [TechPowerUp Forums — Set Message Signaled Interrupts](https://www.techpowerup.com/forums/threads/feature-request-set-message-signaled-interrupts.262199/page-3)
- [Neowin — Xbox Game Bar melhora desempenho do Ryzen 7950X3D](https://www.neowin.net/news/microsofts-xbox-game-bar-boosts-windows-11-and-10-gaming-performance-on-amd-ryzen-7950x3d/)
- [PCWorld — Como usar o V‑Cache do Ryzen 7000 no Windows](https://www.pcworld.com/article/1524857/how-to-use-amds-ryzen-7000-v-cache-on-windows.html)
- [TechRadar — Windows 10 Game Mode gut check](https://techradar.com/news/windows-10-game-mode-gut-check-a-clear-benefit-youll-barely-notice)
- [SpeedGuide — Nagle/TcpAckFrequency em jogos](https://www.speedguide.net/articles/-5812/p-1)
- [Bitsum — ParkControl (core parking)](https://bitsum.com/parkcontrol/)
- [Microsoft Q&A — Toggle de VRR sumiu / taxa caiu para 60 Hz após atualização](https://learn.microsoft.com/en-us/answers/questions/5910507/variable-refresh-rate-(vrr)-toggle-completely-vani)

Ferramentas de validação: [PresentMon](https://github.com/GameTechDev/PresentMon), [CapFrameX](https://www.capframex.com/).

### Lacunas
- Não foi encontrada medição de Battle(non)sense específica para os itens; as medições de latência citadas vêm da Blur Busters e dos testes de imprensa.
- `GlobalTimerResolutionRequests`, `OverlayTestMode` e chaves globais de FSO não têm documentação oficial; a semântica de `GameDVR_Enabled` diverge entre fontes.
- Requisitos de VBS/HVCI do FACEIT e do Vanguard mudam em ondas; reconsultar as páginas oficiais antes de cada versão.
