# Revisão cética: `competitivo-windows.md` e `competitivo-anticheat.md`

> Revisão independente feita em 2026-11-26. Objetivo: tentar **refutar** as afirmações mais importantes dos dois documentos com fontes primárias (Microsoft Learn/Support, DirectX dev blog, Riot, FACEIT, Valve, Bitsum) e medições.
>
> Método e limite: as páginas foram consultadas por busca na web, a partir dos trechos indexados. Não houve leitura integral de cada página. Por isso, o que depende de uma frase exata de página oficial está marcado como "confirmado pelo trecho indexado". Antes do lançamento, reabrir manualmente as páginas da FACEIT e da Riot, que mudam com frequência.
>
> Vereditos: **Confirmada**, **Parcialmente correta** (com correção), **Refutada**.

## Resumo dos vereditos

| # | Afirmação | Veredito |
|---|---|---|
| 1a | Taxa de atualização no máximo | Confirmada (corrigir a API de aplicação) |
| 1b | Aceleração do mouse desligada (`SPI_SETMOUSE`, trio 1/6/10) | Confirmada |
| 1c | Game Mode ligado | Parcialmente correta: o efeito é misto; vale como "manter o padrão", não como ganho |
| 1d | `HistoricalCaptureEnabled=0` | Confirmada, mas na maioria dos PCs já é o padrão (ajuste corretivo, não otimização) |
| 1e | `SwapEffectUpgradeEnable` em `DirectXUserGlobalSettings` | Confirmada pela comunidade; a Microsoft documenta só a interface |
| 1f | Remover `useplatformclock` | Confirmada (Microsoft Learn sustenta o custo maior do timer de plataforma) |
| 2a | Timer resolution = mito | Confirmada, mas o doc de anticheat atribui à Microsoft algo que ela não documenta |
| 2b | `disabledynamictick` = mito | Confirmada (não há medição controlada) |
| 2c | MSI mode = mito em GPUs atuais | Parcialmente correta: "detectar e informar" está certo, mas "drivers atuais já usam MSI" carece de fonte NVIDIA |
| 2d | Nagle = mito em FPS competitivos | Confirmada; faltou dizer que `TCPNoDelay` global não é um controle documentado do Winsock |
| 3 | Plano de energia / core parking / X3D / Game Bar | Parcialmente correta: a ressalva X3D procede; faltam o "modo de energia" do Win11 e os PCs com Modern Standby |
| 4 | HAGS condicional, exigido para DLSS FG | Confirmada; o critério "decisivo" é fraco no contexto competitivo |
| 5 | IFEO `PerfOptions`: condicional (Windows) × proibido (anticheat) | O doc de anticheat tem razão pela política do projeto; o de Windows deve reclassificar o item |
| 6 | Requisitos de Vanguard e FACEIT | Confirmada; falta o requisito de Windows 11 da FACEIT (out/2026) |
| 7 | Gravar só sem jogo protegido aberto; snapshot/ETW são seguros | Parcialmente correta: a regra é prudente, mas por motivo prático, não por fonte de anticheat; o snapshot é seguro **só** com `TH32CS_SNAPPROCESS` |
| 8 | Nunca abrir handle, nem `PROCESS_QUERY_LIMITED_INFORMATION` | Parcialmente correta: é excesso técnico, mas defensável como política "na dúvida, não" |

---

## 1. Itens "recomendados" de Windows

### 1a. Taxa de atualização no máximo — **Confirmada (com correção de API)**

- O efeito é física do display: o período de quadro cai de 16,7 ms (60 Hz) para 4,2 ms (240 Hz). Não há o que refutar.
- **Correção:** `DEVMODE.dmDisplayFrequency` é inteiro. Aplicar pelo `ChangeDisplaySettingsExW` perde a taxa racional (ex.: 143,98 Hz vs. 144 Hz) e pode escolher um timing diferente do que o driver enumera. Para aplicar e reverter com precisão, preferir `QueryDisplayConfig`/`SetDisplayConfig` (que trabalham com `DISPLAYCONFIG_RATIONAL`) e guardar o caminho/modo original completo. O próprio doc já cita `QueryDisplayConfig` para detecção; deve usá-la também para aplicar.
- **Omissão:** trocar o modo de vídeo com um jogo em tela cheia exclusiva aberto força perda de foco/reset do dispositivo. Isso é motivo prático para a regra 12 (ver item 7).

### 1b. Aceleração do mouse desligada — **Confirmada**

- `HKCU\Control Panel\Mouse`: `MouseSpeed "1"`, `MouseThreshold1 "6"`, `MouseThreshold2 "10"` como padrão, e `0/0/0` desligado. O caminho correto é `SystemParametersInfoW(SPI_GETMOUSE/SPI_SETMOUSE)` com um vetor de 3 inteiros e `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`. Está correto.
- A ressalva de que jogos com raw input ignoram a curva também está correta. O efeito real se limita a menus, MOBAs/RTS e jogos sem raw input.

### 1c. Game Mode ligado — **Parcialmente correta**

- A chave `HKCU\Software\Microsoft\GameBar\AutoGameModeEnabled` está correta.
- **Correção de efeito:** as medições são mistas. PCWorld e PC Gamer (Win10) acharam ganho só com carga em segundo plano. Um teste de 2025 (Win11 25H2) achou ganho de 1% low em cargas sensíveis a segundo plano, mas outro teste do mesmo autor achou **até +5,7% de média e +13% de 1% low com o Game Mode desligado** em dois jogos CPU-bound. A classificação "Recomendado" só se sustenta como "manter/restaurar o padrão". O nv-tuner não deve apresentar o Game Mode como ganho garantido.
- A dependência do Ryzen dual-CCD X3D (driver AMD + Game Mode + Game Bar) está confirmada por imprensa técnica (Neowin, HotHardware, PCWorld) e é o argumento mais forte para não desligá-lo.

### 1d. `HistoricalCaptureEnabled=0` — **Confirmada, com ressalva de relevância**

- Caminho e valor corretos: `HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR\HistoricalCaptureEnabled` (DWORD; `0` = desligado). O blog do Xbox (2016) diz que a gravação em segundo plano vem desligada **por padrão** porque custa desempenho.
- **Correção:** como o padrão já é `0`, o item é **corretivo** (só agir se estiver `1`), não uma otimização universal. A tabela deveria dizer isso, como já faz no item 6.
- `GameDVR_Enabled`: as fontes divergem até no sentido (`1` = ligado em guias; em um Q&A da Microsoft, `0` aparece como padrão). O doc acerta ao não tratá-lo como confiável. Recomendação: **não escrever** nele.

### 1e. `SwapEffectUpgradeEnable` — **Confirmada (fonte comunitária)**

- `HKCU\Software\Microsoft\DirectX\UserGpuPreferences\DirectXUserGlobalSettings`, token `SwapEffectUpgradeEnable=1;`. É o que a interface grava, segundo ElevenForum, TheWindowsClub e MajorGeeks. A Microsoft documenta **só a interface** (Configurações > Tela > Gráficos) e o efeito (DX10/11 em janela/sem bordas passam ao flip model). A ligação automática quando o Auto HDR é ligado consta no artigo de suporte da Microsoft.
- **Ressalva:** as fontes divergem sobre o padrão (desligado em uns guias, ligado em outros, conforme o build). O app deve **ler** o estado e não supor. Não vale para DX9, OpenGL nem Vulkan.
- O opt-out por executável é plausível (a interface de Gráficos grava por app no mesmo key), mas não achei documentação Microsoft do token por app. Marcar como "não documentado".

### 1f. Remover `useplatformclock` — **Confirmada**

- O Microsoft Learn ("Acquiring high-resolution time stamps") diz que timers de plataforma têm latência de acesso maior que o TSC e escalam mal com chamadas concorrentes. Isso sustenta o argumento. O padrão (valor ausente) usa o TSC invariante quando disponível. Medições de fórum (Blur Busters) são anedóticas e mistas, mas nenhuma mostra ganho consistente com HPET forçado.
- A ressalva sobre BitLocker está correta em espírito: alterar o BCD pode disparar a recuperação, conforme o perfil de PCR. Recomendo suspender o BitLocker por um reinício (`manage-bde -protectors -disable C: -RebootCount 1`) ou não agir.

## 2. Classificação como mito

### 2a. Timer resolution — **Confirmada (com erro de atribuição no doc de anticheat)**

- Microsoft Learn (`timeBeginPeriod`): desde o Win10 2004, a função não afeta mais a resolução global. No Win11, se a janela do processo estiver oculta, minimizada ou inaudível, a Microsoft não garante resolução acima do padrão. Ferramentas externas não mudam o timer do jogo. O veredito "mito" se sustenta.
- **Erro no `competitivo-anticheat.md` (seção 2.8, linha TimerResolution):** ele afirma, citando a Microsoft como [O], que "no Windows 11 só vale com `GlobalTimerResolutionRequests`". A página oficial **não menciona** essa chave. A chave é conhecimento comunitário e não documentado (o próprio `competitivo-windows.md` diz isso corretamente). Corrigir a etiqueta para [C].

### 2b. `disabledynamictick` / `useplatformtick` — **Confirmada**

- Não há documentação Microsoft de ganho nem medição controlada. Os relatos de fórum são contraditórios (inclusive de piora de input lag com `useplatformtick`). O veredito "mito" se sustenta, e o custo de energia é real (o tick dinâmico existe para economizar energia).

### 2c. MSI mode — **Parcialmente correta**

- O caminho do registro e o fato de o valor vir do INF (e ser sobrescrito em reinstalações) estão no Microsoft Learn.
- **Correção:** "drivers NVIDIA atuais já habilitam MSI" é amplamente relatado pela comunidade, mas **não achei fonte NVIDIA** que o declare. Reescrever como "na maioria das GPUs atuais, segundo relatos; verificar no sistema". A conduta (detectar e informar, não alterar) está correta. A classificação da afinidade como prejudicial ou sem evidência também se sustenta.

### 2d. Nagle — **Confirmada (com complemento)**

- Nagle e ACK atrasado são mecanismos de TCP; o tráfego de jogo de CS2, Valorant, Overwatch e Apex é UDP. A KB 328890 documenta `TcpAckFrequency`.
- **Complemento:** no Winsock, Nagle se controla **por socket** (`TCP_NODELAY`). O valor global `TCPNoDelay` no registro não é um controle documentado do Winsock atual; guias o citam por herança. É mais um motivo para "mito". `NetworkThrottlingIndex`/`SystemResponsiveness` também não têm medição de ganho. Veredito mantido.

## 3. Plano de energia, core parking e Ryzen X3D — **Parcialmente correta**

- **GUIDs e aliases conferem:** Equilibrado `381b4222-…`, Alto desempenho `8c5e7fda-…`, Desempenho máximo `e9a42b02-…` (exige `-duplicatescheme`), `CPMINCORES` = `0cc5b647-…`, `IDLEDISABLE` = `5d76a2ca-…`.
- **X3D dual-CCD:** confirmada por imprensa técnica (XDA, via WindowsForum; VRChat wiki; HotHardware). Os relatos dizem que a AMD recomenda Equilibrado e que, com Alto desempenho, o parking do CCD sem cache não acontece. O mecanismo envolve o *3D V-Cache Performance Optimizer* (pacote de chipset AMD), o Game Mode e a Game Bar (lista de jogos conhecidos, com a opção "Lembrar que isto é um jogo"). Não localizei, nesta revisão, um documento da AMD com a frase literal; marcar como [C] até achar a fonte da AMD.
- **Correção 1:** há relatos de usuários de X3D que duplicam o Equilibrado e o ajustam sem perder o parking. O problema é o plano Alto desempenho em si, não qualquer mudança. O doc pode simplificar como "não trocar o plano em X3D dual-CCD".
- **Correção 2 (omissão):** no Windows 11, o "Modo de energia" (Configurações > Energia: Melhor eficiência / Equilibrado / Melhor desempenho) é uma **sobreposição** sobre o plano, aplicada por `PowerSetActiveOverlayScheme`. Em PCs com Modern Standby (quase todos os notebooks), os planos Alto desempenho e Desempenho máximo ficam ocultos ou indisponíveis, e o único controle suportado é essa sobreposição. O doc só trata de `powercfg /setactive`. A detecção precisa ler também a sobreposição ativa (`PowerGetActualOverlayScheme`).
- A falta de benchmark controlado de fonte primária, admitida pelo doc, se confirma. "Condicional/medir antes" é a classificação honesta.

## 4. HAGS — **Confirmada (com ressalva de critério)**

- DLSS Frame Generation exige HAGS ligado: Tom's Hardware, CD Projekt RED, Nixxes, 505 Games e outros. A NVIDIA não explica o motivo. No Win11, o HAGS vem ligado por padrão nas GPUs suportadas; no Win10, não.
- `HwSchMode` (`2` = ligado, `1` = desligado) é a convenção comunitária consistente. Não é API documentada pela Microsoft. Há relatos de valor ausente por padrão e de blocklists do driver que forçam o desligamento (Intel Arc). Admin e reinício obrigatórios: confirmados.
- **Ressalva:** chamar o DLSS FG de "critério decisivo para o nv-tuner" é fraco **no contexto competitivo**: Frame Generation acrescenta latência e quase nenhum jogador competitivo o usa. O critério correto para competitivo é "manter o padrão do sistema; não desligar se o usuário usa FG em outros jogos".

## 5. Conflito sobre IFEO `PerfOptions` — **o `competitivo-anticheat.md` tem razão**

- **O que existe de fonte:** a Bitsum, fabricante do Process Lasso [F], documenta o "Enforce by registry" por IFEO, diz que ele funciona em processos protegidos por anticheat porque o Windows aplica a prioridade na criação do processo, e afirma que o método "is benign so is not known to cause any alerts within anti-cheat systems". Isso é **declaração do fabricante da ferramenta**, não do anticheat.
- **O que não existe:** nenhuma página oficial de Riot, FACEIT, EAC, BattlEye ou Valve aprova ou proíbe a chave. Também não localizei relato de banimento. Usuários de EAC relatam "access denied" ao mudar a prioridade em tempo de execução, e o IFEO contorna isso **antes** do anticheat. Esse é justamente o tipo de contorno que a política do projeto evita.
- **Conclusão:** pela regra explícita do projeto ("risco zero; sem fonte oficial favorável, não entra"), a proibição do `competitivo-anticheat.md` é a posição coerente. O `competitivo-windows.md` não está factualmente errado ao dizer "sem relato de banimento", mas viola o próprio critério (3) da sua introdução ("não conflita com anticheat"), que exige ausência de conflito, não ausência de relato. Somado ao ganho não medido e à cobertura do Game Mode, o item 14 deve passar a **"Fora do escopo (política de anticheat)"**.

## 6. Requisitos de Vanguard e FACEIT — **Confirmada (com omissões)**

**Vanguard (Riot, páginas oficiais):**
- TPM 2.0 e Secure Boot exigidos no Windows 11 (VAN9001/VAN9003): confirmado na página "Vanguard Security Requirements".
- "VAN: RESTRICTION: 5": a página da Riot diz que o erro aparece quando o HVCI não inicializa e que o Vanguard então bloqueia o jogo. O doc descreve o erro como aplicado "a contas/máquinas sob restrição", e os trechos oficiais falam em bloqueio pela falha do HVCI. A nuance de "sob restrição" aparece em guias de terceiros. Manter como está, mas sem afirmar que vale para todos.
- Modo On-Demand: lançado em **24/06/2026** (blog da Riot, repercutido por PC Gamer, Tom's Hardware e TechPowerUp). Exige Windows 11 25H2, TPM 2.0, Secure Boot, VBS, HVCI e IOMMU (Vanguard Pre-Check). A Riot estimou que ~35% dos jogadores cumprem. **Confirmado**. O doc não dá a data; acrescentar.

**FACEIT (support.faceit.com):**
- TPM 2.0 e Secure Boot obrigatórios para todos desde **25/11/2025**: confirmado no FAQ oficial e em imprensa (dust2, esports.ru). O FAQ fala de implantação do Secure Boot "em ondas até todos".
- IOMMU e VBS: implantação limitada desde **abril/2025** e obrigatória para todos acima de **3.000 de Elo desde agosto/2025**. A FACEIT diz que exige VBS para que o IOMMU funcione de forma confiável. Confirmado. Não achei data oficial para quem está abaixo de 3.000.
- **Omissão importante:** o FAQ da FACEIT indica **Windows 11 obrigatório a partir de outubro/2026**, prazo que já passou na data desta revisão. Nenhum dos dois documentos menciona isso. Também diz que BIOS sem VT-d/AMD-Vi não cumprirá o requisito quando a aplicação universal começar.

**Outras omissões:**
- Ricochet/Black Ops 7 (TPM + Secure Boot) aparece só no `competitivo-anticheat.md`; o `competitivo-windows.md`, seção 2.15, deveria citá-lo.
- **Certificados do Secure Boot:** a Microsoft anunciou a expiração, a partir de junho/2026, dos certificados UEFI CA de 2011, com troca pelos de 2023 via atualização. Um PC sem a atualização pode ter problemas com Secure Boot, o que afeta diretamente quem precisa dele para FACEIT e Vanguard. Esta revisão não verificou os detalhes; vale um item de diagnóstico ("Secure Boot ligado, mas com certificados desatualizados").

## 7. Regra "gravar ajustes só com nenhum jogo protegido aberto" — **Parcialmente correta**

- **Necessária pelos anticheats?** Não há fonte oficial de que gravar no HKCU, no DRS ou em `powercfg` com o jogo aberto seja detectado ou punido. As fontes que o doc cita ([2], [9], [16]) falam de handles, de software que interage com o processo (CS2 Trusted Mode) e de quando o driver do Ricochet roda, não de gravações no sistema. O Vanguard roda **desde o boot**, então "jogo aberto" nem define a janela em que ele observa. Como exigência de anticheat, a regra é **excessiva**.
- **Necessária por outros motivos?** Sim, e com fonte melhor:
  - O CS2 Trusted Mode diz que iniciar software incompatível com a partida aberta pode desconectar o jogador [O]. O app não deve lançar processos auxiliares nesse momento.
  - Trocar o modo de vídeo, o HAGS (que exige reinício) ou o MPO com o jogo aberto causa reset de tela ou estado inconsistente.
  - O DRS e o IFEO são lidos na criação do processo; gravar durante o jogo não tem efeito e só cria a interação "deriva → reaplicação" que o doc já aponta.
  - O jogo pode regravar o próprio arquivo de config ao fechar.
  - **Recomendação:** manter a regra como **política de produto** e reescrever a justificativa ("evitar efeito colateral e interação desnecessária"), sem atribuí-la aos anticheats.
- **`CreateToolhelp32Snapshot` é seguro?** Só com `TH32CS_SNAPPROCESS`. Esse modo lista nome, PID e PID pai sem abrir handle para o alvo. `TH32CS_SNAPMODULE`/`SNAPMODULE32` com o PID do jogo **lê a memória do processo** e abre acesso a ele: proibir explicitamente. `SNAPTHREAD` é global e não abre o processo, mas não serve para nada aqui. Nenhum anticheat publica posição; o argumento de segurança é técnico (não há handle para o `ObRegisterCallbacks` filtrar) e é o mesmo caminho do Gerenciador de Tarefas.
- **ETW:** o provedor `Microsoft-Windows-Kernel-Process` (início e fim de processo) exige admin ou SYSTEM, ou seja, o serviço. É passivo, não toca o jogo e é o mesmo que EDRs usam. Seguro no mecanismo, mas, de novo, sem declaração oficial de anticheat. Alternativa sem ETW: um snapshot periódico (a cada poucos segundos) já basta.
- **Omissão:** identificar por nome é falsificável (`cs2.exe` qualquer) e pode haver colisão de nomes. Para a regra conservadora, um falso positivo só adia a gravação, o que é aceitável. Não "melhorar" a identificação abrindo handle para ler o caminho completo.

## 8. Regra "nunca abrir handle, nem `PROCESS_QUERY_LIMITED_INFORMATION`" — **Parcialmente correta**

- **Tecnicamente excessiva:** o Microsoft Learn documenta que o `ObRegisterCallbacks` permite ao driver **reduzir** os direitos concedidos; não é um alarme por si só. Análises técnicas de anticheat descrevem `PROCESS_QUERY_LIMITED_INFORMATION` como inofensivo e pedido por todo gerenciador de tarefas: anticheats removem `VM_READ`, `VM_WRITE`, `TERMINATE`, `CREATE_THREAD` etc., não esse direito. O Gerenciador de Tarefas, o Explorer, antivírus e o Discord abrem esse tipo de handle o tempo todo. Dizer que "coloca o app na mesma categoria de um cheat externo" não tem fonte.
- **Defensável como política:** de fato não há fonte **oficial** que o declare seguro, e a regra custa pouco, porque o snapshot de processos já dá nome e PID. Pela política do projeto, manter a proibição é coerente.
- **Correção de redação:** o argumento "a tentativa de abrir o handle é visível" vale para qualquer handle, inclusive os do Windows. O argumento honesto é: "não há necessidade funcional e não há fonte oficial; por isso não". Também registrar que `QueryFullProcessImageNameW`, `GetPriorityClass` e `GetProcessTimes` sobre o jogo ficam proibidos por consequência.

---

## Omissões relevantes (consolidado)

1. FACEIT: **Windows 11 obrigatório desde out/2026** e VT-d/AMD-Vi exigidos na aplicação universal. Nenhum dos documentos cita isso.
2. Win11: o **modo de energia (sobreposição)** e os PCs com Modern Standby, onde `powercfg /setactive` para Alto desempenho não se aplica.
3. `TH32CS_SNAPMODULE` sobre o jogo lê a memória do processo. A regra 12 deve restringir o snapshot a `TH32CS_SNAPPROCESS`.
4. Mudar o modo de vídeo com o jogo em tela cheia exclusiva (motivo prático para a regra 12). Aplicar a taxa de atualização por `SetDisplayConfig`, não por `ChangeDisplaySettingsExW`.
5. Erro de atribuição: `GlobalTimerResolutionRequests` marcado como [O, Microsoft] no doc de anticheat.
6. Vanguard On-Demand: data de lançamento (24/06/2026) e adesão de ~35%.
7. Certificados do Secure Boot de 2011 expirando em 2026 (diagnóstico útil; detalhes a verificar).
8. A justificativa de Frame Generation para o HAGS é pouco relevante no competitivo.
9. `HistoricalCaptureEnabled` já é `0` por padrão: o item é corretivo.
10. O item IFEO do doc de Windows contradiz o próprio critério (3) da introdução.

## Fontes consultadas nesta revisão

Oficiais:
- [Microsoft Learn — timeBeginPeriod](https://learn.microsoft.com/en-us/windows/win32/api/timeapi/nf-timeapi-timebeginperiod)
- [Microsoft Learn — Acquiring high-resolution time stamps](https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps)
- [Microsoft Learn — ObRegisterCallbacks](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-obregistercallbacks) · [OB_PRE_CREATE_HANDLE_INFORMATION](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/ns-wdm-_ob_pre_create_handle_information) · [Process Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/procthread/process-security-and-access-rights)
- [Microsoft Support — Optimizations for windowed games in Windows 11](https://support.microsoft.com/en-us/topic/3f006843-2c7e-4ed0-9a5e-f9389e535952)
- [Xbox Wire — Tips on Windows 10 and Game DVR optimization](https://news.xbox.com/en-us/2016/10/14/tips-on-windows-10-and-game-dvr-optimization)
- [Riot — Vanguard Security Requirements](https://support.riotgames.com/en-us/riot/performance/vanguard-security-requirements) · [Error VAN: RESTRICTION: 5](https://support-2xko.riotgames.com/hc/en-us/articles/49277516508563-Error-VAN-RESTRICTION-5) · [Enabling Memory Integrity](https://support.riotgames.com/riot/support-tools/enabling-memory-integrity) · [What is Vanguard](https://support.riotgames.com/en-us/riot/performance/what-is-vanguard) · [Vanguard Pre-Check](https://support.riotgames.com/ar-ae/riot/performance/vanguard-pre-check)
- [FACEIT — Windows Security Requirements FAQ](https://support.faceit.com/hc/en-us/articles/23117181142556) · [How can I check if I need to make any changes](https://support.faceit.com/hc/en-us/articles/23117375791772-How-can-I-check-if-I-need-to-make-any-changes-to-my-PC-before-using-the-FACEIT-Anti-Cheat) · [Known issues with Anti-Cheat Requirements](https://support.faceit.com/hc/en-us/articles/22851956652956-Known-issues-with-Anti-Cheat-Requirements) · [Enabling TPM 2.0](https://support.faceit.com/hc/en-us/articles/4407006362002-Enabling-TPM-2-0)

Fabricante de ferramenta [F]:
- [Bitsum — Process Lasso e anticheat](https://bitsum.com/apps/process-lasso/docs/help/anti-cheat/) · [Bitsum — Priorities / Enforce by registry (IFEO)](https://bitsum.com/apps/process-lasso/docs/rules/priorities/) · [Fórum Bitsum — Vanguard parando com Process Lasso (mar/2026)](https://community.bitsum.com/forum/index.php?msg=73326)

Imprensa e comunidade [C]:
- Vanguard On-Demand: [Tom's Hardware](https://www.tomshardware.com/video-games/pc-gaming/riot-vanguard-adds-an-on-demand-mode-that-stops-anti-cheat-loading-at-boot-on-secured-windows-11-pcs) · [PC Gamer](https://pcgamer.com/games/you-can-finally-turn-riots-vanguard-anti-cheat-off-when-youre-not-playing-a-game/) · [TechPowerUp](https://www.techpowerup.com/350391/vanguard-anti-cheat-adds-on-demand-mode)
- FACEIT 25/11/2025: [dust2.in](https://www.dust2.in/news/67071/faceit-announces-tpm-20-deadline) · [esports.ru](https://esports.ru/counter-strike-2/news/faceit-vveli-novye-antichit-trebovaniya-dlya-igrokov/)
- HAGS e DLSS FG: [Tom's Hardware](https://www.tomshardware.com/how-to/enable-frame-generation-windows-10) · [CD Projekt RED](https://support.cdprojektred.com/en/witcher-3/pc/saves/issue/2278/dlss-frame-generation-how-to-enable-hardware-accelerated-gpu-scheduling) · [Nixxes](https://support.nixxes.com/hc/en-us/articles/48432027293971-How-to-use-DLSS-Frame-Generation) · [Winaero — HwSchMode](https://winaero.com/turn-on-or-off-hardware-accelerated-gpu-scheduling-in-windows-10) · [Microsoft Q&A — HAGS toggle](https://learn.microsoft.com/en-us/answers/questions/5911034/hardware-accelerated-gpu-scheduling-(hags)-toggle)
- Game Mode: [WindowsForum — 1% lows](https://windowsforum.com/news/windows-11-game-mode-raises-1-lows-not-average-fps.437707/) · [WindowsForum — piora em CPU-bound](https://windowsforum.com/windows-news.4/windows-11-game-mode-can-hurt-cpu-bound-fps-in-some-games.441277/) · [PCWorld](https://www.pcworld.com/article/406291/tested-windows-10s-game-mode-makes-unplayable-games-playable-sometimes.html) · [PC Gamer](https://pcgamer.com/windows-10-game-mode-tested-good-for-minimum-fps-bad-for-multitasking)
- X3D: [WindowsForum — Balanced em dual-CCD](https://windowsforum.com/news/amd-ryzen-dual-ccd-x3d-gaming-why-windows-balanced-power-plan-matters.449796/) · [HotHardware](https://hothardware.com/news/amds-chipset-driver-for-v-cache-cpus) · [Neowin](https://www.neowin.net/news/microsofts-xbox-game-bar-boosts-windows-11-and-10-gaming-performance-on-amd-ryzen-7950x3d/) · [VRChat Wiki — AMD X3D](https://wiki.vrchat.com/wiki/AMD_X3D_Series_Processors)
- Game DVR e windowed: [ElevenForum — Record what happened](https://elevenforum.com/t/enable-or-disable-record-what-happened-for-gaming-captures-in-windows-11.17900/) · [ElevenForum — Optimizations for windowed games](https://www.elevenforum.com/t/turn-on-or-off-optimizations-for-windowed-games-in-windows-11.4943/) · [TheWindowsClub](https://thewindowsclub.com/enable-optimizations-for-windowed-games-in-windows)
- HPET: [Blur Busters — useplatformclock](https://forums.blurbusters.com/viewtopic.php?p=110004)
- Handles e anticheat: [ObCallbacks: Stripping Dangerous Handles](https://acblog.pages.dev/blog/peregrine-obcallbacks) · [SpecterOps — Kernel Callbacks](https://specterops.io/blog/2023/06/12/understanding-telemetry-kernel-callbacks/)
- IFEO e EAC: [Steam — Insurgency: Sandstorm](https://steamcommunity.com/app/581320/discussions/0/1736589519991624421)
