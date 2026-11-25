# Pesquisa: detecção automática (GPU, jogos instalados, fatos do jogo, catálogo DRS)

Data: 2026-11-25. Máquina de teste: RTX 2060 SUPER (TU106), driver 621.40, Windows 11 Pro 26300.
Tudo abaixo marcado como **medido** foi lido nesta máquina em modo somente leitura (nenhuma
escrita em registro, DRS ou pastas de jogo). Tabela de arquiteturas: `capacidades-gpu.toml`.

## 1. Capacidades da GPU

| Fato | Como obter | Medido aqui |
|---|---|---|
| Arquitetura | `NvAPI_GPU_GetArchInfo` (0xD8265D24), `NV_GPU_ARCH_INFO_V2`, 16 bytes, `version = 16 \| (2<<16)` | `architecture=0x160` (TU100), `implementation=0x06` (TU106), `revision=0xA1` |
| Nome | `NvAPI_GPU_GetFullName` (0xCEEE8E9F) | "NVIDIA GeForce RTX 2060 SUPER" |
| Driver | `NvAPI_SYS_GetDriverAndBranchVersion` (0x2926AAAD) | 621.40 |
| VRAM | `NvAPI_GPU_GetMemoryInfo` ou DXGI `GetDesc1().DedicatedVideoMemory` (VendorId 0x10DE) | 8192 MiB |
| ReBAR ativo | NVML `nvmlDeviceGetBAR1MemoryInfo`: `bar1Total` ≈ VRAM = ligado; 256 MiB = desligado | BAR1 = 256 MiB → desligado (Turing não tem suporte oficial) |

IDs (NVAPI Reference, enum `_NV_GPU_ARCHITECTURE_ID`): TU100 = 0x160, GA100 = 0x170, GH100 = 0x180,
AD100 = 0x190, GB100 = 0x1A0, GB200 = 0x1B0 (GeForce RTX 50). Fontes:
<https://docs.nvidia.com/nvapi/group__gpu.html>, <https://docs.nvidia.com/nvapi/structNV__GPU__ARCH__INFO__V2.html>.

Fatos por arquitetura (detalhes e fontes no TOML):

| Arquitetura | Tensor | FP8 | DLSS FG | DLSS MFG | ReBAR oficial |
|---|---|---|---|---|---|
| Turing (RTX 20) | 2ª geração | não | não | não | não |
| Ampere (RTX 30) | 3ª geração | não | não | não | sim |
| Ada (RTX 40) | 4ª geração | **sim** (FP8 Transformer Engine) | sim | não | sim |
| Blackwell (RTX 50) | 5ª geração | **sim** (+ FP6/FP4) | sim | sim | sim |

- FP8 no Ada: a página oficial da arquitetura cita “FP8 Transformer Engine” nos Tensor Cores de 4ª geração
  (<https://www.nvidia.com/en-us/geforce/ada-lovelace-architecture>). Turing/Ampere: os whitepapers listam
  FP16/INT8/INT4 (Turing) e TF32/BF16/FP16/INT8 (Ampere), sem FP8.
- Cuidado: GTX 16xx também é `0x160`, mas TU116/TU117 não têm Tensor Cores: a regra precisa olhar a implementação.
- Arquitetura futura (`> 0x1B0`): herdar de Blackwell de forma conservadora, com `dlss_mfg = false` até
  o Streamline confirmar em runtime; `< 0x160` ou ID desconhecido no meio: “sem DLSS”.
- NVML (`nvml.dll` em System32, instalado pelo driver) roda sem admin; carregar dinamicamente (`libloading`)
  em vez de linkar, porque nem toda máquina tem o arquivo no mesmo lugar.

## 2. Bibliotecas de jogos (launchers)

| Launcher | Onde | Formato | Nesta máquina |
|---|---|---|---|
| Steam | `HKCU\Software\Valve\Steam\SteamPath` (ou `HKLM\SOFTWARE\WOW6432Node\Valve\Steam\InstallPath`) → `steamapps\libraryfolders.vdf` (lista de bibliotecas + appids) → `<lib>\steamapps\appmanifest_<appid>.acf` → `<lib>\steamapps\common\<installdir>` | VDF texto (KeyValues) | **medido**: SteamPath = `c:/program files (x86)/steam` (barra normal e minúsculas: normalizar); 1 biblioteca, apps 228980 (Steamworks Common Redistributables: filtrar) e 1867240 (`name "WARDOGS"`, `installdir "Wardogs"`, `StateFlags 4` = instalado) |
| Epic | `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item` | JSON: `DisplayName`, `InstallLocation`, `LaunchExecutable`, `AppName`, `bIsIncompleteInstall` | ausente |
| Riot | `C:\ProgramData\Riot Games\RiotClientInstalls.json` (cliente e `associated_client`: pastas de jogo) + `C:\ProgramData\Riot Games\Metadata\<produto>.live\<produto>.live.product_settings.yaml` | JSON + YAML | **medido**: `associated_client` = `C:/Riot Games/League of Legends/`; YAML do LoL: `product_install_full_path: "C:/Riot Games/League of Legends"`, `dependencies.vanguard: true`; também existe `teamfighttactics.live` (mesma pasta do LoL) |
| Battle.net | `C:\ProgramData\Battle.net\Agent\product.db` (protobuf) — mais simples: chaves `Uninstall` com `Publisher = Blizzard Entertainment` | protobuf / registro | ausente |
| EA App | `HKLM\SOFTWARE\WOW6432Node\Electronic Arts\<jogo>\Install Dir` e `C:\ProgramData\EA Desktop\...` (cache criptografado); usar registro + `Uninstall` | registro | ausente |
| Ubisoft Connect | `HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\<id>\InstallDir` | registro | ausente |
| GOG | `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<id>`: `gameName`, `path`, `exe` | registro | ausente |
| Xbox / MS Store | `C:\XboxGames\<jogo>\Content\MicrosoftGame.config` (XML: `<Executable Name=...>`), `appxmanifest.xml`; ou `Get-AppxPackage`/`PackageManager` (WinRT) | XML | ausente |
| Genérico (rede de segurança) | `HKLM`/`HKCU` `...\CurrentVersion\Uninstall\*` (`DisplayName`, `InstallLocation`, `Publisher`) | registro | **medido**: `Steam App 1867240` → WARDOGS com `InstallLocation`; `Riot Game league_of_legends.live` → `C:/Riot Games/League of Legends`; `Riot Vanguard` |

A chave `Uninstall` sozinha já achou os dois jogos desta máquina: boa como fallback universal, mas sem
o exe principal. Pastas do Xbox são protegidas por ACL (WindowsApps); `C:\XboxGames` em geral é legível.

Crates Rust (licenças conforme crates.io; reconferir no `cargo deny` antes de travar):

| Uso | Crate | Licença |
|---|---|---|
| Registro | `winreg` | MIT |
| Registro / WinRT / PE VERSIONINFO / DXGI (oficial) | `windows` | MIT OR Apache-2.0 |
| JSON (Epic, Riot) | `serde`, `serde_json` | MIT OR Apache-2.0 |
| VDF/ACF (Steam) | `keyvalues-parser` (com `keyvalues-serde`) | MIT |
| Alternativa Steam completa | `steamlocate` (localiza Steam, bibliotecas e apps) | MIT |
| YAML (Riot) | `serde_yaml` (arquivado) ou `serde_norway`/`serde_yml`; ou ler só as 2 chaves à mão | MIT OR Apache-2.0 |
| XML (Xbox) | `quick-xml` | MIT |
| PE (imports, recursos) | `pelite` (MIT) ou `goblin` (MIT) | MIT |
| Varredura de pastas | `walkdir` | MIT OR Unlicense |
| NVML dinâmico | `libloading` (ISC) ou `nvml-wrapper` (MIT OR Apache-2.0) | — |

## 3. Fatos do jogo

### 3.1 Exe principal
Heurística em ordem (parar na primeira com resposta):
1. Metadado do launcher: Epic `LaunchExecutable`, GOG `exe`, Xbox `MicrosoftGame.config`. Steam/Riot não informam.
2. **Cruzar todos os `.exe` da pasta com o DRS** (`NvAPI_DRS_FindApplicationByName`): o que casa com perfil
   é o exe que importa para o driver. Resolve LoL.
3. Padrões de engine: Unreal `*\Binaries\Win64\*-Win64-Shipping.exe`; Unity `<nome>.exe` + `UnityPlayer.dll`.
4. Maior `.exe` fora de listas de exclusão (`CrashReport*`, `*Crash*`, `*Helper*`, `*Setup*`, `unins*`,
   `crashpad_handler`, `UnityCrashHandler*`, `*Launcher*`, `vc_redist*`, `EpicWebHelper`, pastas `Engine\`,
   `_CommonRedist`, `ThirdParty`, `CEF3`).
5. Confirmação definitiva só em runtime: processo em primeiro plano com swapchain (PresentMon).

**Medido**:
- LoL: exes = `Game\League of Legends.exe` (44 MB), `LeagueClient.exe`, `LeagueClientUx.exe`,
  `LeagueClientUxRender.exe`, `LeagueCrashHandler64.exe` (×2). Passo 2: `League of Legends.exe` → perfil
  **“League of Legends”**; `LeagueClient.exe`/`LeagueClientUx.exe` → perfil “League of Legends Client”.
  O passo 4 escolheria `LeagueClient.exe` (30 MB) se não houvesse exclusão de “Client” → por isso o DRS vem antes.
- WARDOGS: exes = `Wardogs\Binaries\Win64\WardogsClient-Win64-Shipping.exe` (229 MB), `WardogsLauncher-Shipping.exe`
  (raiz), `Engine\...\CrashReportClient.exe`, `EpicWebHelper.exe`, `Elytra\Elytra-Setup.exe`, `crashpad_handler.exe`.
  Passo 3 acerta (`*-Win64-Shipping.exe` em `Binaries\Win64`; o Launcher também termina em `-Shipping.exe`, mas
  não fica em `Binaries\Win64`). Passo 2: só casa porque o **perfil “WARDOGS” foi criado pelo laboratório**
  (único perfil não predefinido de 8051): o driver não traz perfil para o jogo.

### 3.2 API gráfica
Leitura da import table (e delay-load) do PE: `d3d11.dll` → DX11, `d3d12.dll` → DX12, `vulkan-1.dll` → Vulkan,
`d3d9.dll`, `opengl32.dll`. Reforços: `D3D12\D3D12Core.dll` (Agility SDK) na pasta do exe = DX12.
Limitações: carregamento dinâmico (`LoadLibrary`) é comum em Unreal/Unity, exes protegidos/empacotados
escondem a import table, e jogos com DX11+DX12 só revelam a escolhida em runtime (PresentMon `PresentMode`/`Runtime`).

**Medido**:
- `League of Legends.exe`: imports contêm `dxgi.dll` e `d3d11.dll` → **DX11** (correto).
- `LeagueClient.exe`: importa `d3d11.dll` (CEF) — ruído, por isso analisar só o exe principal.
- `WardogsClient-Win64-Shipping.exe`: import table **ilegível** — único import legível é `coreinit.dll`
  (Elytra Anti-Cheat Framework); o resto da tabela de delay-load está cifrado. Heurística de imports falha;
  o fallback `Binaries\Win64\D3D12\x64\D3D12Core.dll` (Agility SDK 1.616.1) indica **DX12**. `vulkan-1.dll`
  existe mas dentro de `CEF3` (falso positivo se varrer a pasta toda).

### 3.3 DLSS
Procurar recursivamente `nvngx_dlss.dll` (SR), `nvngx_dlssd.dll` (Ray Reconstruction), `nvngx_dlssg.dll` (FG),
`nvngx_deepdvc.dll`; versão via VERSIONINFO (`GetFileVersionInfoW` / `pelite` resources; o `FileVersion`
vem com vírgulas, ex. `310,6,0,0`). Streamline: `sl.interposer.dll`, `sl.dlss_g.dll`, `sl.common.dll`.

**Medido**:
- LoL: nenhum `nvngx_*` nem `sl.*` → **sem DLSS**.
- WARDOGS: `nvngx_dlss.dll` **310.6.0.0**, `nvngx_dlssd.dll` 310.6.0.0, `nvngx_dlssg.dll` 310.6.0.0,
  `nvngx_deepdvc.dll`, Streamline **2.11.1** (`sl.interposer`, `sl.dlss_g`, `sl.reflex`, `sl.pcl`, `sl.deepdvc`)
  em `Wardogs\Plugins\External\Nvidia\...` → DLSS SR+RR+FG presente (FG inútil em Turing: `dlss_fg=false`).

### 3.4 Reflex
1. Streamline: `sl.reflex.dll` (e `sl.pcl.dll`, PC Latency) na pasta → Reflex quase certo.
2. Sem Streamline, Reflex usa NvAPI por ID (`NvAPI_D3D_SetSleepMode` via `nvapi_QueryInterface`): **não deixa
   string de import**. Strings como “Reflex”/“NvLowLatency” no binário são indício fraco.
3. Runtime: PresentMon 2.x expõe métricas de latência PC (`PCLatency`/`ReflexLatency` quando o jogo emite
   marcadores PCL) — é a confirmação forte.

**Medido**: LoL → nenhum indício (sem `sl.reflex`, sem strings) → **sem Reflex**. WARDOGS → `sl.reflex.dll`
2.11.1 + string “Reflex” no exe → **Reflex presente**.

### 3.5 Anticheat de kernel
| Anticheat | Indício em disco | Indício de sistema |
|---|---|---|
| Easy Anti-Cheat | pasta `EasyAntiCheat\`, `EasyAntiCheat_EOS_Setup.exe`, `start_protected_game.exe` | serviço `EasyAntiCheat` / `EasyAntiCheat_EOS` |
| BattlEye | pasta `BattlEye\`, `BEService*.exe`, `*_BE.exe` | serviço `BEService`, driver `BEDaisy` |
| Riot Vanguard | metadata Riot `dependencies.vanguard: true` | `C:\Program Files\Riot Vanguard\vgk.sys`, serviço `vgk` (Type 1 = kernel, Start 1 = boot) |
| Elytra | `coreinit.dll` (“Elytra Anti-Cheat Framework”), pasta `Elytra\` | serviço `Elytra.Service` + drivers `elytra_<aleatório>` (Type 1, kernel) |
| Outros (FACEIT, ACE/Tencent, nProtect, Javelin, Ricochet) | pastas/dlls com o nome | serviços correspondentes |

**Medido**: LoL → **Vanguard** (YAML `vanguard: true`; `vgk.sys` presente; serviço `vgk` kernel, boot).
WARDOGS → **Elytra** (kernel): `coreinit.dll` + `Elytra\Elytra-Setup.exe` + 17 drivers `elytra_*` e `Elytra.Service`.
Nenhuma pasta EAC/BattlEye. Lição: a lista de anticheats tem que ser dados (TOML), não código, e a busca por
string curta (`vgk`) no binário dá falso positivo — apareceu nos 4 exes testados, inclusive no WARDOGS.

Consequência para o app: com anticheat de kernel, **nunca** injetar, ler memória do processo ou trocar DLLs
do jogo (inclusive `nvngx_dlss.dll`); usar apenas DRS (override de DLSS pelo driver) e PresentMon (ETW).

## 4. DRS como catálogo

Sequência somente leitura: `NvAPI_Initialize` → `DRS_CreateSession` (0x0694D52E) → `DRS_LoadSettings`
(0x375DBD6B) → `DRS_GetNumProfiles` (0x1DAE4FBC) → `DRS_EnumProfiles` (0xBC371EE0) + `DRS_GetProfileInfo`
(0x61CD6FD6, `NVDRS_PROFILE_V1` 4116 bytes: `numOfApps`, `isPredefined`) → `DRS_EnumApplications` (0x7FA2173A)
→ `DRS_DestroySession` (0xDAD9CFF8). Sem admin.

**Medido (driver 621.40)**: **8051 perfis** (8050 predefinidos + 1 do usuário, “WARDOGS”) e **13107 aplicações**.

Casamento exe ↔ perfil: `NvAPI_DRS_FindApplicationByName` (0xEEE566B2, `NVDRS_APPLICATION_V4`) — o driver faz o
casamento sozinho, sem diferenciar maiúsculas e aceitando caminho completo (usa o nome do arquivo e as regras
`fileInFolder`/`commandLine` do registro). Retorno `-166` = `NVAPI_EXECUTABLE_NOT_FOUND`. **Medido**:
`League of Legends.exe` (nome ou caminho completo) → “League of Legends”; `cs2.exe` → “Counter-strike 2”;
`WardogsClient-Win64-Shipping.exe` → “WARDOGS”; `WardogsLauncher-Shipping.exe` e nomes inexistentes → -166.
Preferir `FindApplicationByName` a enumerar as 13 mil aplicações (enumerar só para índice/pesquisa offline).

Jogo sem perfil (caso do WARDOGS num PC limpo): `DRS_CreateProfile` (nome = nome do launcher, ex. “WARDOGS”) →
`DRS_CreateApplication` com `appName` = exe em minúsculas → `DRS_SetSetting` → `DRS_SaveSettings`. Exige
UAC (no laboratório a escrita passou por elevação). Cuidados: checar antes se o exe já pertence a outro perfil
(um exe só pode estar em um perfil → erro `NVAPI_EXECUTABLE_ALREADY_IN_USE`); marcar o perfil como criado pelo
app (backup .nip) para reverter com `DRS_DeleteProfile`; atualizações do driver preservam perfis do usuário,
mas um perfil predefinido novo para o mesmo exe pode colidir no futuro.

## Riscos
- Exes protegidos (WARDOGS) escondem imports: API precisa de fallback por arquivos vizinhos ou runtime.
- `0x160` cobre RTX 20 e GTX 16: decidir Tensor Cores pela implementação.
- IDs de implementação Blackwell e o comportamento de arquitetura futura não foram verificados em hardware.
- ReBAR ativo só por NVML (BAR1); sem `nvml.dll` o fato fica “desconhecido”.
- Battle.net/EA usam formatos binários/cifrados: confiar em registro/Uninstall, cobertura parcial.
- Strings curtas no binário (vgk, Reflex) geram falso positivo; usar só como indício.
- Varredura recursiva de pastas grandes (WARDOGS 54 GB) precisa de limite de profundidade e exclusões (CEF3, ThirdParty).
