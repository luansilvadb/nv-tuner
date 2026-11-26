# Anticheats e ferramentas de otimização: o que é detectado, bloqueado ou punido

Pesquisa feita em 2026-11-26 para orientar a expansão do nv-tuner. Hoje o app ajusta o perfil do driver NVIDIA via NVAPI DRS. A ideia é que passe a ajustar também o Windows e os arquivos de configuração de jogos competitivos.

**Restrição absoluta do projeto:** risco zero de banimento ou de bloqueio por anticheat. Por isso, onde não há fonte oficial que garanta que uma prática é segura, este documento a trata como **proibida**, e não como "provavelmente ok".

Legenda de confiabilidade das fontes:

- **[O]**: oficial, do fabricante do anticheat, do publisher ou da Microsoft.
- **[F]**: fabricante da ferramenta citada (Bitsum, Special K etc.).
- **[C]**: relato da comunidade ou da imprensa (fórum, issue, matéria). Serve como indício, não como regra.

---

## 1. Panorama: o que os anticheats atuais fazem

| Anticheat | Onde roda | Quando roda | Exigências de plataforma |
|---|---|---|---|
| Riot Vanguard (Valorant, LoL, 2XKO) | driver de kernel `vgk.sys` + cliente | desde o boot (modo padrão) ou sob demanda (modo novo, só em PCs "seguros") | TPM 2.0, Secure Boot, e em alguns casos HVCI/VBS e IOMMU/DMA remapping [O] |
| Easy Anti-Cheat (Epic) | driver de kernel + módulo no jogo | enquanto o jogo protegido roda | integridade de arquivos do sistema e do jogo ("Untrusted system file") [O] |
| BattlEye | driver de kernel + serviço `BEService` | enquanto o jogo roda | sem test-signing, sem kernel debugging, sem kernel modificado [O] |
| VAC (Valve) + Trusted Mode do CS2 | modo usuário | enquanto o jogo roda | Trusted Mode bloqueia arquivos de terceiros que interagem com o processo [O] |
| FACEIT AC | driver de kernel | enquanto o cliente FACEIT roda | bloqueia drivers vulneráveis, exige atualizações de segurança do Windows e, em alguns casos, HVCI [O] |
| Ricochet (Call of Duty) | driver de kernel | só com um CoD protegido aberto | TPM 2.0 e Secure Boot obrigatórios a partir de Black Ops 7 [O] |
| ESEA | driver de kernel | enquanto o cliente roda | foco declarado em cheats de hardware (DMA) [O] |
| Blizzard (Warden / Defense Matrix) | modo usuário + servidor | enquanto o jogo roda | sem lista pública; proíbe "software de terceiros não autorizado" [O] |
| EA Javelin | driver de kernel | enquanto o jogo roda | bloqueia drivers incompatíveis (ex.: WinRing0) e remapeadores que usam dispositivos virtuais [O]/[C] |

Padrão comum entre todos:

1. **Banimento é reservado a cheats** ou a componentes de cheats: leitura ou escrita na memória do jogo, injeção de código, automação, emulação de entrada. BattlEye: *"Generally we only ever ban for the use of actual cheats/hacks or components of such hacks"* [O]. A Valve diz o mesmo: só modificações feitas para dar vantagem acionam o VAC, e ela **não** dá garantia sobre quais programas são seguros [O].
2. **Bloqueio não é banimento, mas também é falha.** Drivers vulneráveis, DLLs injetadas e handles abertos para o processo protegido são **bloqueados**: o jogo não abre, o anticheat recusa o carregamento ou derruba a partida. Para o nv-tuner, qualquer bloqueio viola a restrição "zero bloqueio".
3. **Falsos positivos existem e são documentados.** O caso mais conhecido é o AMD Anti-Lag+ (out/2023), que fazia hook em funções do motor do CS2 e gerou banimentos VAC em massa, depois revertidos pela Valve [C]. Também há EAC acusando `amdihk64.dll` (componente do driver AMD) como "untrusted" [C] e relatos de Vanguard parando com o Process Lasso instalado [C]. Lição: **até software legítimo de fabricante grande foi punido por injetar código no processo do jogo.**

---

## 2. Tópico a tópico

### 2.1 Abrir handles para o processo do jogo (incluindo `PROCESS_SET_INFORMATION`)

- Os anticheats de kernel (Vanguard, EAC, BattlEye, FACEIT) usam callbacks de objeto (`ObRegisterCallbacks`) para **remover direitos** de handles que terceiros abrem para o processo protegido. Por isso o Process Lasso documenta que *"anti-cheat systems … block other software, including Process Lasso, from opening their processes"* [F]. A tentativa de abrir o handle **é visível** para o anticheat. Mesmo quando não gera punição, coloca o app na mesma categoria comportamental de um cheat externo, que começa justamente com `OpenProcess`.
- A própria Bitsum recomenda contornar o bloqueio aplicando a regra no **launcher**, já que *"Child processes inherit the CPU affinity of their parent"*, ou pelo registro (IFEO), *"so it works even on processes that cannot be opened after launch"* [F].
- Há relatos de Vanguard parando de rodar com o Process Lasso ativo, sem causa confirmada [C].
- `PROCESS_QUERY_LIMITED_INFORMATION` é o direito mais fraco, mas, pelo mesmo motivo, também não há fonte oficial que o declare seguro contra processo protegido.

**Conclusão:** o nv-tuner **não deve abrir handle algum** para processos de jogo ou de anticheat. Isso vale para mudar prioridade e afinidade e também para leitura ou consulta. Para saber se um jogo está aberto, usar só um snapshot de nomes (`CreateToolhelp32Snapshot`/`Process32Next`, que não abre handle para o processo alvo) ou ETW de início e fim de processo.

### 2.2 IFEO `PerfOptions` (`CpuPriorityClass`, `IoPriority`)

- A chave `HKLM\…\Image File Execution Options\<jogo>.exe\PerfOptions` faz o kernel aplicar a prioridade **na criação do processo**, sem que nenhuma ferramenta abra handle. É a técnica que guias comunitários recomendam justamente para quando o EAC já protegeu o processo [C].
- **Não encontrei fonte oficial** de nenhum anticheat que aprove ou proíba a chave. Também não encontrei relato documentado de banimento por ela.
- Riscos concretos, mesmo sem banimento:
  - IFEO é um vetor clássico de persistência de malware (a subchave `Debugger`). Antivírus e anticheats monitoram essa árvore, e escrever lá é comportamento "suspeito" por natureza.
  - Prioridade `High` ou `Realtime` no jogo pode deixar o próprio anticheat e o áudio sem CPU. FACEIT tem artigo sobre problemas de desempenho causados por configurações do sistema, como HPET ligado por software de terceiros [O].
- **Conclusão:** como a política do projeto é "na dúvida, não", **não usar IFEO** (nem `PerfOptions`, nem, em hipótese alguma, `Debugger`/`GlobalFlag`/`VerifierDlls`). Se um dia for reconsiderado, só com fonte oficial do anticheat em questão.

### 2.3 Drivers de kernel de terceiros e drivers vulneráveis

- **Vanguard** bloqueia drivers com vulnerabilidades conhecidas e avisa na bandeja que *"Vanguard has blocked something from loading on your machine"* [O]. O erro "VAN: Incompatible Software" manda desinstalar o software e apagar o `.sys` indicado [O]. Em 2020 a Riot afrouxou a lista após reclamações, mas os drivers "mais graves" continuam impedindo o jogo [C]. Também já bloqueou o `inpoutx64.sys` do FanControl [C] e DLLs do OpenRGB [C].
- **FACEIT** bloqueia manualmente drivers com vulnerabilidades conhecidas (artigo "Forbidden driver") [O]. Há relatos de `WinRing0x64.sys` e `asio.sys` (ASUS) bloqueados e até de um driver novo e legítimo (PawnIO) recusado por causa do certificado do assinante [C].
- **BattlEye:** *"BattlEye is blocking certain software that is using kernel drivers which contain known security issues"* [O].
- **EA Javelin** acusa o WinRing0 como "incompatible driver" (erro 119) [C].
- **WinRing0** (CVE-2020-14979) é classificado pela Microsoft como vulnerável: *"This detection is valid. The WinRing0 driver has been classified as a known vulnerability"* [O]. Afeta MSI Afterburner (versões antigas), HWiNFO, OpenRGB, CapFrameX, FanControl, Libre Hardware Monitor [O].
- **Verificação no nv-tuner (somente leitura, 2026-11-26):** nenhum crate (`Cargo.toml` do workspace e de `crates/*`) depende de driver de kernel. Não há `.sys`, WinRing0, inpout, PawnIO, `DeviceIoControl` nem instalação de driver. O acesso ao hardware NVIDIA passa só pela `nvapi64.dll` do driver oficial (`crates/nvt-drs/src/api.rs`). Dependências nativas: `windows-sys`, `windows-service`, `winreg`, `pelite`. Nenhuma carrega driver.
- **Conclusão:** o nv-tuner **nunca** deve trazer, instalar ou carregar driver de kernel próprio ou de terceiros. Leituras de sensores (temperatura, clock, MSR), se vierem a existir, devem usar APIs do fabricante (NVAPI, NVML) ou do Windows (WMI/PDH), nunca acesso direto a porta ou MSR.

### 2.4 Overlays e injeção

- **Special K** (do próprio autor): *"Close Special K before playing a multiplayer game where anti-cheat protection might be present!"* e *"the sole presence of Special K, even though it is inert and idle, might be enough for some devs to call foul"* [F]. O serviço global injeta em *"any process that deals with system input"* [F].
- **RTSS/MSI Afterburner:** o overlay injeta hook no Direct3D do jogo. Já foi bloqueado em Battlefield 2042 (EA AC), Throne and Liberty (EAC), Roblox (Hyperion) e com versões específicas do F1 25 [C]. A EA diz que banimento por esse overlay é "raro" [C], o que não é zero.
- **BattlEye:** *"non-cheat overlays and visual enhancement tools like Reshade or SweetFX are generally supported"*, mas pede para remover `d3d9.dll`/`dxgi.dll`/`dsound.dll` (proxies de injeção) da pasta do jogo [O].
- **CS2 Trusted Mode:** bloqueia arquivos de terceiros que interagem com o jogo, e iniciar software incompatível com a partida aberta pode desconectar o jogador [O]. A opção `-allow_third_party_software` desliga a proteção e, segundo notas da Valve, *pode* afetar o Trust Factor [C/O].
- **Precedente AMD Anti-Lag+:** hook em DLL do motor gerou banimentos VAC [C].
- **Conclusão:** o nv-tuner **nunca** injeta DLL, não instala hook global (`SetWindowsHookEx`, CBT, AppInit_DLLs), não desenha overlay sobre o jogo, não coloca DLL proxy na pasta do jogo e não recomenda `-allow_third_party_software`. Telemetria de quadros, se vier, só por ETW passivo (PresentMon), sem tocar no processo.

### 2.5 Serviços que rodam enquanto o jogo roda

- Os anticheats não banem por serviços legítimos em execução. O que bloqueiam é a **interação** com o jogo (handles, injeção) e **drivers** [O]. O Vanguard no modo padrão roda desde o boot e observa tudo o que carrega depois [O].
- O Ricochet afirma que seu driver *"operates only when a protected Call of Duty title is running"* [O]. Ou seja, a fiscalização se concentra na janela em que o jogo está aberto.
- **Conclusão:** o serviço do nv-tuner pode continuar rodando, desde que: (a) não carregue driver; (b) não abra handle para processo de jogo ou anticheat; (c) seja assinado digitalmente e instalado em `Program Files` (binário não assinado rodando como SYSTEM é sinal típico de rootkit, e a FACEIT associa driver "sem nome" a rootkit [O]); (d) **não faça gravações** enquanto um jogo protegido estiver aberto (ver 2.6 e 2.7).

### 2.6 Alterar NVAPI/DRS com o jogo aberto

- Não há fonte oficial de nenhum anticheat sobre DRS. A NVAPI DRS altera o banco de perfis do driver (`C:\ProgramData\NVIDIA Corporation\Drs`), não o processo do jogo. É o mesmo mecanismo usado pelo Painel de Controle NVIDIA e pelo NVIDIA Profile Inspector.
- Não achei relato confiável de banimento pelo Profile Inspector [C]. O risco documentado é **outro**: configurações do driver que dão vantagem visual. O exemplo são os filtros NVIDIA no CS2, que exigiam `-allow_third_party_software` e permitiam ver através da fumaça [C]. Um ajuste de driver que altere a **imagem** de forma a revelar informação pode ser considerado cheat pelo publisher.
- O driver lê o perfil quando o processo inicia. Gravar com o jogo aberto em geral não tem efeito até reiniciar o jogo, mas o driver pode recarregar o banco. Não há garantia de comportamento estável.
- **Conclusão:** (a) só gravar configurações que o próprio Painel de Controle NVIDIA expõe ou que sejam neutras para a jogabilidade (latência, sincronização, cache de shader, energia), **nunca** configurações que alterem visibilidade (LOD bias negativo extremo, desligar névoa e fumaça, filtros de nitidez e realce de cor usados para ver através de efeitos, wireframe); (b) adiar gravações (inclusive a reaplicação automática por deriva) enquanto um jogo com anticheat estiver aberto.

### 2.7 Alterar arquivos de configuração de jogos

- **VAC:** a detecção abrange *"modifications to a game's core executable files and DLLs"* [O]. Arquivos de **configuração** do usuário (`.cfg`, `.ini`, `GameUserSettings.ini`) não são executável nem DLL. A FACEIT lista "config changes" entre as coisas permitidas [O, via resumo de busca; a página retornou 403 na verificação direta].
- **EAC/BattlEye** verificam a integridade dos arquivos do jogo (catálogos assinados). Alterar qualquer arquivo **coberto pelo catálogo** gera "Untrusted file" ou impede o início [O].
- Alguns jogos validam a configuração no servidor ou regravam o arquivo ao fechar. Gravar com o jogo aberto pode ser sobrescrito ou corromper o arquivo.
- Valores fora do que o menu do jogo permite (ex.: desligar grama, sombras ou fumaça por `.ini` quando o menu não permite) dão vantagem visual. Pela definição de cheat dos próprios anticheats (vantagem sobre outros jogadores [11]), podem ser tratados como exploração. Não localizei, nesta pesquisa, fonte oficial de um caso específico.
- **Conclusão:** só editar arquivos de config **do usuário** (normalmente em `%LOCALAPPDATA%`, `Documents` ou `Saved\Config`), nunca nada dentro da pasta de instalação coberta por verificação de integridade. Só com o jogo **fechado**, com backup e só com valores que o menu do próprio jogo oferece. Arquivo somente-leitura forçado (para impedir que o jogo sobrescreva) também fica fora.

### 2.8 Ferramentas conhecidas: como cada anticheat as trata

| Ferramenta | O que faz | Tratamento documentado | Risco para o nv-tuner imitar |
|---|---|---|---|
| ISLC (Wagnard) | limpa a standby list (API do sistema) e pede timer resolution | nenhuma fonte oficial de anticheat; o autor diz "use por sua conta e risco" [F]; relatos comunitários de uso com VAC sem problemas [C] | baixo no mecanismo (não toca o jogo), mas sem garantia oficial; **não imitar** sem fonte |
| TimerResolution | `NtSetTimerResolution` global | sem fonte oficial; desde o Windows 10 2004 o pedido deixou de ser global, e no Windows 11 só vale com `GlobalTimerResolutionRequests` [O, Microsoft] | ganho incerto e risco zero não comprovado; **não imitar** |
| NVIDIA Profile Inspector | edita o DRS via NVAPI | sem relato confiável de banimento [C]; risco está em configurações que dão vantagem visual | é o que o nv-tuner já faz; manter só configurações neutras |
| Process Lasso | `OpenProcess` + `SetPriorityClass`/afinidade | bloqueado por anticheats de kernel [F]; relatos de conflito com Vanguard [C] | **não imitar** (handle no jogo) |
| MSI Afterburner / RTSS | overlay injetado + driver de hardware (WinRing0 em versões antigas) | overlay bloqueado em vários títulos [C]; WinRing0 bloqueado por FACEIT/EA e detectado pelo Defender [O/C] | **não imitar** (injeção e driver) |
| Special K | injeção global de DLL | o próprio autor manda fechar antes de jogos com anticheat [F] | **não imitar** |
| ReShade/SweetFX | DLL proxy de pós-processamento | "generally supported" pelo BattlEye [O]; bloqueado pelo Trusted Mode do CS2 [O] | **não imitar** |

### 2.9 Requisitos de segurança impostos pelos anticheats (que o app nunca deve desligar)

- **Vanguard:** TPM 2.0, Secure Boot e Memory Integrity (HVCI/VBS) conforme o PC; erros VAN 9001, 9003 e "VAN: RESTRICTION" [O]. O modo sob demanda exige Windows 11 25H2+, Secure Boot, TPM 2.0, VBS, HVCI e IOMMU [O]. "VAN: Incompatible OEM Driver" bloqueia drivers de armazenamento e Wi-Fi sem suporte a DMA remapping [O].
- **Ricochet / Black Ops 7:** TPM 2.0 e Secure Boot obrigatórios; sem eles, não joga [O].
- **FACEIT:** exige atualizações de segurança do Windows ("Your system is missing important Windows security updates") e HVCI quando o AC pedir; recusa kernel modificado [O].
- **BattlEye:** sem test-signing, sem kernel debugging, sem kernel "hackeado" [O].
- **EAC:** arquivos do sistema íntegros (o "Untrusted system file" manda rodar SFC) [O].
- **Microsoft Vulnerable Driver Blocklist:** guias de cheat mandam desligá-la [C]. Desligá-la reduz a proteção e contraria o que os anticheats exigem.

**Lista de "nunca tocar"** (tweaks comuns de "otimização" que quebram esses requisitos): Secure Boot, TPM, VBS, HVCI/Memory Integrity, Credential Guard, IOMMU/Kernel DMA Protection, Vulnerable Driver Blocklist, DEP/NX (`bcdedit nx`), `bcdedit testsigning`, `nointegritychecks`, `debug`, `hypervisorlaunchtype`, Windows Defender e Windows Update de segurança. Mitigações de CPU (Spectre/Meltdown, `FeatureSettingsOverride`) também ficam fora: não há fonte de que um anticheat exija desligá-las e há fonte de que exigem sistema atualizado e íntegro.

Observação: o BattlEye pede para **desligar** a "Hardware-enforced Stack Protection" (incompatível com alguns anticheats) [O]. Isso é decisão do usuário diante de um erro específico, **nunca** ação automática do nv-tuner. Mexer em segurança em qualquer direção está fora do escopo do app.

---

## 3. Verificação do código atual (somente leitura)

Escopo: `crates/nvt-detect`, `crates/nvt-service`, e de passagem `crates/nvt-drs` e `crates/nvt-engine`, que o serviço usa.

| Ponto | Onde | Interage com jogo em execução? | Avaliação |
|---|---|---|---|
| Nenhum `OpenProcess`, `CreateToolhelp32Snapshot`, `EnumProcesses`, `SetPriorityClass`, `ReadProcessMemory`, `SetWindowsHookEx`, IFEO | busca em todos os `.rs` | não | seguro |
| Único `OpenProcessToken` | `crates/nvt-service/src/diretorio.rs:677` | sobre `GetCurrentProcess()` (o próprio serviço) | seguro |
| Sem driver de kernel; só `nvapi64.dll` oficial | `crates/nvt-drs/src/api.rs:46` | não | seguro |
| Vigia do DRS com `ReadDirectoryChangesW` | `crates/nvt-service/src/vigia.rs:104` | não (diretório do driver, compartilhamento total `READ\|WRITE\|DELETE`) | seguro |
| Leitura de PE (imports, VERSIONINFO) dos exes e DLLs dos jogos via `pelite::FileMap` | `crates/nvt-detect/src/facts/pe.rs:93`, `:111` | pode ocorrer com o jogo aberto (varredura a cada 6 h e por pedido) | **risco baixo**: o `pelite` 0.10 abre com `GENERIC_READ` e **só** `FILE_SHARE_READ`. Enquanto o arquivo está mapeado, um atualizador do jogo ou do anticheat que tente abri-lo para escrita recebe *sharing violation*. Isso pode fazer o patch falhar ou um "verificar arquivos" acusar erro. Não é detecção de cheat, mas é uma interação com arquivos protegidos. Mitigação: abrir com `FILE_SHARE_READ\|FILE_SHARE_WRITE\|FILE_SHARE_DELETE` e ler para buffer, sem manter o mapeamento, ou não varrer com o jogo/launcher aberto |
| Detecção de anticheat por arquivos e pela chave `HKLM\SYSTEM\CurrentControlSet\Services` | `crates/nvt-detect/src/facts/anticheat.rs` | não (só listagem e registro) | seguro; lê `coreinit.dll` (Elytra) via PE, mesmo ponto de atenção acima |
| Reaplicação automática de deriva no DRS | `crates/nvt-service/src/nucleo.rs:311-370` (verificação a cada 10 min e 3 s após mudança no DRS) | **sim, potencialmente**: grava no DRS a qualquer momento, inclusive com jogo competitivo aberto | **risco baixo, não documentado**: não há fonte de banimento, mas contraria a regra "não gravar com jogo protegido aberto" (seção 2.6). Hoje o serviço não tem como saber se há jogo aberto |
| Telemetria PresentMon | `crates/nvt-telemetry` (placeholder) | futuro | manter ETW passivo, sem injeção |

Nada no código atual abre handle para processo de jogo, injeta código, carrega driver ou mexe em recursos de segurança do Windows.

---

## 4. REGRAS DE SEGURANÇA

Política geral: **se não houver fonte oficial dizendo que é seguro, não faça.** Cada regra cita a fonte que a motiva (numeração na seção 5).

### Nunca faça

1. **Nunca** abra handle (`OpenProcess`, nem só para consulta) para processo de jogo ou de anticheat, inclusive para prioridade ou afinidade. Fontes: [2], [3], [16].
2. **Nunca** injete DLL, faça hook (`SetWindowsHookEx`, AppInit_DLLs, CBT), desenhe overlay ou coloque DLL proxy (`dxgi.dll`, `d3d9.dll`, `dsound.dll`) na pasta do jogo. Fontes: [1], [9], [10], [12], [17].
3. **Nunca** instale, empacote ou carregue driver de kernel (WinRing0, inpout, PawnIO, `.sys` próprio); sensores só via NVAPI/NVML/WMI. Fontes: [1], [4], [5], [6], [7], [18].
4. **Nunca** escreva em Image File Execution Options (`PerfOptions`, `Debugger`, `GlobalFlag`, `VerifierDlls`). Não há aprovação oficial, e é árvore monitorada. Fontes: [3], [19].
5. **Nunca** desligue nem altere Secure Boot, TPM, VBS, HVCI/Memory Integrity, IOMMU/Kernel DMA Protection, Vulnerable Driver Blocklist, DEP, Defender ou Windows Update de segurança. Fontes: [4], [5], [8], [13], [14].
6. **Nunca** use `bcdedit` para `testsigning`, `nointegritychecks`, `debug` ou `hypervisorlaunchtype off`. Fontes: [1], [4], [14].
7. **Nunca** altere executáveis, DLLs ou qualquer arquivo dentro da pasta de instalação do jogo coberto por verificação de integridade. Fontes: [10], [11].
8. **Nunca** aplique configuração de driver ou de jogo que altere visibilidade ou informação (fumaça, folhagem, névoa, LOD extremo, filtros para "ver através"). Fontes: [11], [15], [20].
9. **Nunca** recomende nem adicione `-allow_third_party_software` ou opções que desliguem proteções do jogo. Fontes: [9], [15].
10. **Nunca** emule ou remapeie entrada por dispositivo virtual, nem automatize ações no jogo (macros). Fontes: [1], [21], [22].
11. **Nunca** imite Process Lasso, Special K, RTSS, ISLC ou TimerResolution sem fonte oficial do anticheat que libere o mecanismo. Fontes: [3], [12], [17], [23].

### Faça

12. **Faça** toda gravação (DRS, Windows, config de jogo) só com nenhum jogo protegido aberto, inclusive a reaplicação de deriva. A detecção de jogo aberto deve ser feita por nome (snapshot ou ETW), sem abrir handle. Fontes: [2], [9], [16].
13. **Faça** edições de config apenas em arquivos do usuário, com o jogo fechado, com backup e restauração, e só com valores que o menu do próprio jogo oferece. Fontes: [10], [11], [20].
14. **Faça** ajustes de driver só com configurações que o Painel de Controle NVIDIA ou o próprio jogo expõem e que sejam neutras para a jogabilidade (latência, sincronização, energia, cache). Fontes: [15], [20].
15. **Faça** leituras de arquivos de jogos com compartilhamento total (`FILE_SHARE_READ|WRITE|DELETE`) e sem manter o arquivo mapeado (corrigir o uso atual de `pelite::FileMap`). Fonte: [10] (integridade e atualização dos arquivos do jogo).
16. **Faça** o serviço e a UI serem binários assinados, instalados em `Program Files`, sem ofuscação e sem empacotador que pareça malware. Fontes: [6], [8].
17. **Faça** a detecção de requisitos (TPM, Secure Boot, HVCI) servir só para **informar** o usuário, nunca para alterar. Fontes: [4], [5], [13].
18. **Faça** a revisão destas regras a cada nova categoria de ajuste. Política: sem fonte oficial favorável, o ajuste não entra. Fonte: [11] (a Valve não garante quais programas são seguros).

---

## 5. Fontes

1. [O] BattlEye — FAQ: https://www.battleye.com/support/faq/
2. [O] Riot — What is Vanguard: https://support.riotgames.com/valorant/performance/what-is-vanguard
3. [F] Bitsum — Process Lasso e anticheats: https://bitsum.com/apps/process-lasso/docs/help/anti-cheat/ · relatos de conflito com Vanguard [C]: https://community.bitsum.com/forum/index.php?msg=73326
4. [O] Riot — Vanguard Security Requirements: https://support.riotgames.com/riot/performance/vanguard-security-requirements · Vanguard Pre-Check: https://support.riotgames.com/en-us/riot/performance/vanguard-pre-check · VAN: Incompatible OEM Driver: https://support-valorant.riotgames.com/hc/en-us/articles/47638826320147-VAN-Incompatible-OEM-Driver · VAN: RESTRICTION: https://support-legendsofruneterra.riotgames.com/hc/en-us/articles/48069697996435-Error-VAN-RESTRICTION
5. [O] Riot — VAN: Incompatible Software: https://support.riotgames.com/en-us/valorant/performance/van-incompatible-software · [C] hotfix de compatibilidade de drivers (2020): https://dotesports.com/valorant/news/riot-is-shipping-a-hotfix-to-increase-vanguards-compatibility-with-cheat-vulnerable-drivers · [C] FanControl/inpoutx64 bloqueado: https://github.com/Rem0o/FanControl.Releases/issues/1694
6. [O] FACEIT — Forbidden driver error message and blocked drivers: https://support.faceit.com/hc/en-us/articles/360014237259--Forbidden-driver-error-message-and-blocked-drivers · Incompatible driver detected: https://support.faceit.com/hc/en-us/articles/25296204531996-Incompatible-driver-detected · [C] PawnIO bloqueado pelo certificado: https://github.com/namazso/PawnIO.Setup/issues/1
7. [O] Microsoft — VulnerableDriver:WinNT/Winring0: https://support.microsoft.com/windows/security/threat-malware-protection/microsoft-defender-antivirus-alert-vulnerabledriver-winnt-winring0 · [O] Microsoft recommended driver block rules: https://learn.microsoft.com/en-us/windows/security/application-security/application-control/app-control-for-business/design/microsoft-recommended-driver-block-rules
8. [O] FACEIT — Error: your Windows kernel has been modified: https://support.faceit.com/hc/en-us/articles/360014174880-Error-your-Windows-kernel-has-been-modified · Missing Windows security updates: https://support.faceit.com/hc/en-us/articles/360014237719-Your-system-is-missing-important-Windows-security-updates
9. [O] Steam — CS2 Trusted Mode: https://help.steampowered.com/faqs/view/09A0-4879-4353-EF95
10. [O] Epic — EAC "Untrusted system file": https://www.epicgames.com/help/c-202300000001639/c-202300000001736/easy-anti-cheat-eac-error-untrusted-system-file-a202300000013813 · [C] amdihk64.dll acusado pelo EAC: https://forums.ea.com/discussions/apex-legends-technical-issues-es/easy-anti-cheat-shows-untrusted-system-file-error-for-amdihk64-dll-located-at/13298829
11. [O] Steam — Valve Anti-Cheat (VAC) System: https://help.steampowered.com/faqs/view/571A-97DA-70E9-FF74 · [O] Valve Developer Wiki — VAC: https://developer.valvesoftware.com/wiki/Valve_Anti-Cheat · [C] resposta do suporte Steam ("we do not provide guidance on what software you can and cannot use"): https://www.github.com/JustArchiNET/ArchiSteamFarm/discussions/2895
12. [F] Special K — Global injection: https://wiki.special-k.info/SpecialK/Global
13. [O] FACEIT — Enabling Memory Integrity (HVCI): https://support.faceit.com/hc/en-us/articles/8546882512284-Enabling-Memory-Integrity-HVCI
14. [O] Epic — BattlEye "Windows Test-Signing Mode not supported": https://www.epicgames.com/help/technical-support-c-36624475/c-Trending_0/failed-to-initialize-battleye-service-windows-test-signing-mode-not-supported-battleye-windows-a17144167 · "Windows Kernel modification detected": https://www.epicgames.com/help/technical-support-c-36624475/c-Trending_0/windows-kernel-modification-detected-error-in-battleye-a24877227
15. [C] Filtros NVIDIA permitindo ver através da fumaça no CS2: https://dotesports.com/counter-strike/news/nvidia-driver-settings-let-cs2-players-see-through-smokes · [C] -allow_third_party_software e Trust Factor: https://turbosmurfs.gg/article/cs2-launch-options
16. [O] Call of Duty — Ricochet, Black Ops 7 Beta (TPM 2.0/Secure Boot; driver ativo só com o jogo aberto): https://www.callofduty.com/blog/2025/09/call-of-duty-black-ops-7-beta-ricochet-anti-cheat-update · Day One: https://www.callofduty.com/blog/2025/11/call-of-duty-black-ops-7-ricochet-anticheat-day-one
17. [C] AMD Anti-Lag+ gerando banimentos VAC no CS2: https://hothardware.com/news/valve-counter-strike-2-ban-wave-obliterating-seemingly-innocent-accounts · https://wccftech.com/amd-might-have-overlooked-major-flaw-in-anti-lag-its-getting-gamers-banned-esports-titles/
18. [C] EA AntiCheat acusando WinRing0 (erro 119): https://forums.ea.com/discussions/battlefield-2042-technical-issues-en/ea-anticheat-has-detected-an-incompatible-driver-need-help-/12244323 · [O] ESEA e cheats de hardware: https://blog.esea.net/esea-hardware-cheats
19. [C] IFEO PerfOptions — uso comunitário para contornar EAC, sem posição oficial: https://steamcommunity.com/app/581320/discussions/0/1736589519991624421 · [O] Microsoft Q&A sobre prioridade permanente via registro: https://learn.microsoft.com/en-us/answers/questions/4362251/how-to-permanently-set-priority-processes-using-re
20. [O] FACEIT — What is deemed to be a cheat ("config changes" permitidas; página com 403 na verificação direta, conteúdo obtido via resumo de busca): https://support.faceit.com/hc/en-us/articles/360015788779-What-is-deemed-to-be-a-cheat
21. [O] EA — EA AntiCheat (remapeadores com dispositivo virtual bloqueados): https://help.ea.com/en-us/help/pc/ea-anticheat/ · https://www.ea.com/games/battlefield/redsec/news/battlefield-6-anticheat-update-season-4
22. [O] Blizzard — Defense Matrix / software de terceiros: https://overwatch.blizzard.com/en-us/news/24173281/ · [C] https://www.pcgamer.com/blizzard-is-cracking-down-on-third-party-apps-in-overwatch
23. [F] Wagnard — ISLC (uso por conta e risco): https://www.wagnardsoft.com/forums/viewtopic.php?p=11697 · [C] ISLC e VAC: https://steamcommunity.com/discussions/forum/9/3283699032280539786 · [O] Microsoft — `timeBeginPeriod` (comportamento desde o Windows 10 2004): https://learn.microsoft.com/en-us/windows/win32/api/timeapi/nf-timeapi-timebeginperiod

### Lacunas conhecidas

- Nenhum anticheat publica lista de software permitido. Blizzard/Warden e ESEA não têm documentação pública sobre ferramentas de otimização.
- As páginas da FACEIT e da Steam (VAC, Trusted Mode) bloquearam a leitura direta (403 ou corpo vazio) durante a pesquisa. As citações vêm dos resumos de busca. Convém confirmar manualmente antes de cada lançamento.
- Não há fonte oficial sobre DRS/NVAPI, IFEO `PerfOptions`, ISLC ou TimerResolution em nenhum anticheat. Por isso as regras acima os tratam de forma conservadora.
