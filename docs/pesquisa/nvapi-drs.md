# FFI NVAPI DRS — especificação para o `nvt-drs`

Pesquisa de 2026-11-25. Base: SDK oficial [github.com/NVIDIA/nvapi] (commit
`51e5aae4bc40c3ccd3fb7d4dfd94256e7bfc496e`, 2026-11-10), código do NVIDIA Profile
Inspector 3.0.2.1 ([github.com/Orbmu2k/nvidiaProfileInspector], tag `v3.0.2.1`, commit
`bedb800569384eda737cb7aa596fbd97b5d6863c`) e o leitor PowerShell já em uso no projeto
`nvidia-inspector` (`scripts\lib\drs-leitura.ps1`). Tudo foi conferido em uma máquina real:
driver 621.40 (branch `r621_20`), RTX 2060 SUPER, Windows 11, **sem administrador e sem
gravar nada no DRS** (prova de conceito em [`nvapi-drs-poc/`](nvapi-drs-poc/)).

[github.com/NVIDIA/nvapi]: https://github.com/NVIDIA/nvapi
[github.com/Orbmu2k/nvidiaProfileInspector]: https://github.com/Orbmu2k/nvidiaProfileInspector

## 1. Carregamento da NVAPI

A NVAPI não tem uma exportação por função. A `nvapi64.dll` (instalada pelo driver em
`System32`) exporta uma única função, por nome:

```c
void* __cdecl nvapi_QueryInterface(unsigned int interfaceId);
```

Ela recebe o ID de 32 bits da função (tabela `nvapi_interface_table` em `nvapi_interface.h`)
e devolve o ponteiro da implementação, ou `NULL` se o driver não a tiver. O `nvapi64.lib`
do SDK só embrulha isso; o `nv-tuner` não precisa dele.

Sequência em Rust (sem crates externos, ver `nvapi-drs-poc/src/main.rs`):

1. `LoadLibraryW(L"nvapi64.dll")` — falha ⇒ driver NVIDIA ausente.
2. `GetProcAddress(dll, "nvapi_QueryInterface")` e `transmute` para
   `unsafe extern "C" fn(u32) -> *mut c_void`.
3. Para cada função: `qi(id)`, conferir não nulo e `transmute` para o tipo de ponteiro de
   função correspondente. No x64 `__cdecl` é a convenção única do Windows; usar `extern "C"`.
4. `NvAPI_Initialize()` antes de qualquer outra chamada; `NvAPI_Unload()` ao final
   (antes do `FreeLibrary`).
5. Toda função devolve `NvAPI_Status` (`i32`); `0` é sucesso, negativos são erro.

Sessões DRS: `CreateSession` → `LoadSettings` → (leituras/gravações em memória) →
`SaveSettings` (só para gravar) → `DestroySession`. Sem `SaveSettings` nada chega ao disco;
`DestroySession` descarta a sessão. Handles (`NvDRSSessionHandle`, `NvDRSProfileHandle`,
`NvPhysicalGpuHandle`) são ponteiros opacos (`*mut c_void`).

## 2. IDs de interface (`nvapi_interface.h`)

| Função | ID público | ID interno (NPI) | Assinatura (C) |
|---|---|---|---|
| NvAPI_Initialize | `0x0150E828` | | `()` |
| NvAPI_Unload | `0xD22BDD7E` | | `()` |
| NvAPI_GetErrorMessage | `0x6C2D048C` | | `(NvAPI_Status, NvAPI_ShortString)` |
| NvAPI_SYS_GetDriverAndBranchVersion | `0x2926AAAD` | | `(NvU32* ver, NvAPI_ShortString branch)` — `ver` = 62140 para 621.40 |
| NvAPI_EnumPhysicalGPUs | `0xE5AC921F` | | `(NvPhysicalGpuHandle[64], NvU32* count)` |
| NvAPI_GPU_GetFullName | `0xCEEE8E9F` | | `(NvPhysicalGpuHandle, NvAPI_ShortString)` |
| NvAPI_GPU_GetArchInfo | `0xD8265D24` | | `(NvPhysicalGpuHandle, NV_GPU_ARCH_INFO*)` |
| NvAPI_DRS_CreateSession | `0x0694D52E` | | `(NvDRSSessionHandle*)` |
| NvAPI_DRS_DestroySession | `0xDAD9CFF8` | | `(hSession)` |
| NvAPI_DRS_LoadSettings | `0x375DBD6B` | | `(hSession)` |
| NvAPI_DRS_SaveSettings | `0xFCBC7E14` | | `(hSession)` — **grava** |
| NvAPI_DRS_GetBaseProfile | `0xDA8466A0` | | `(hSession, NvDRSProfileHandle*)` |
| NvAPI_DRS_GetCurrentGlobalProfile | `0x617BFF9F` | | `(hSession, NvDRSProfileHandle*)` |
| NvAPI_DRS_FindProfileByName | `0x7E4A9A0B` | | `(hSession, NvAPI_UnicodeString name, NvDRSProfileHandle*)` |
| NvAPI_DRS_FindApplicationByName | `0xEEE566B2` | | `(hSession, NvAPI_UnicodeString app, NvDRSProfileHandle*, NVDRS_APPLICATION* inout)` |
| NvAPI_DRS_EnumProfiles | `0xBC371EE0` | | `(hSession, NvU32 index, NvDRSProfileHandle*)` |
| NvAPI_DRS_GetNumProfiles | `0x1DAE4FBC` | | `(hSession, NvU32*)` |
| NvAPI_DRS_GetProfileInfo | `0x61CD6FD6` | | `(hSession, hProfile, NVDRS_PROFILE*)` |
| NvAPI_DRS_EnumApplications | `0x7FA2173A` | | `(hSession, hProfile, NvU32 start, NvU32* count inout, NVDRS_APPLICATION*)` |
| NvAPI_DRS_CreateProfile | `0xCC176068` | | `(hSession, NVDRS_PROFILE*, NvDRSProfileHandle*)` — grava (memória) |
| NvAPI_DRS_CreateApplication | `0x4347A9DE` | | `(hSession, hProfile, NVDRS_APPLICATION*)` — grava (memória) |
| NvAPI_DRS_GetSetting | `0x73BF8338` | `0xEA99498D` | público `(hSession, hProfile, NvU32 id, NVDRS_SETTING*)`; interno + `NvU32* x` (NPI passa 0) |
| NvAPI_DRS_SetSetting | `0x577DD202` | `0x8A2CF5F5` | público `(hSession, hProfile, NVDRS_SETTING*)`; interno + `NvU32, NvU32` (NPI passa 0, 0) |
| NvAPI_DRS_DeleteProfileSetting | `0xE4A26362` | `0xD20D29DF` | `(hSession, hProfile, NvU32 id)` |
| NvAPI_DRS_RestoreProfileDefaultSetting | `0x53F0381E` | `0x7DD5B261` | `(hSession, hProfile, NvU32 id)` |
| NvAPI_DRS_EnumSettings | `0xAE3039DA` | `0xCFD6983E` | `(hSession, hProfile, NvU32 start, NvU32* count inout, NVDRS_SETTING*)` |
| NvAPI_DRS_EnumAvailableSettingIds | `0xF020614A` | `0xE5DE48E5` | `(NvU32* ids, NvU32* maxCount inout)` |
| NvAPI_DRS_EnumAvailableSettingValues | `0x2EC39F90` | | `(NvU32 id, NvU32* maxNumValues inout, NVDRS_SETTING_VALUES*)` |
| NvAPI_DRS_GetSettingNameFromId | `0xD61CBE6E` | `0x1EB13791` | `(NvU32 id, NvAPI_UnicodeString*)` |
| NvAPI_DRS_GetSettingIdFromName | `0xCB7309CD` | | `(NvAPI_UnicodeString, NvU32*)` |

Os IDs internos não constam do SDK; vêm de `NvapiDrsWrapper.cs` do NPI 3.0.2.1
(`GetDelegate(interno, out f, público)`: tenta o interno e cai no público se o driver não o
tiver). A POC confirmou que os quatro usados na leitura existem no 621.40.

`NvAPI_UnicodeString` como parâmetro de entrada é um ponteiro para `NvU16[2048]`; passe um
buffer de 2048 `u16` terminado em zero (o NPI e o leitor PowerShell passam `LPWStr`, que
também funciona, mas o buffer cheio é o contrato do header).

## 3. Layouts (`#[repr(C)]`)

Tipos base: `NvU32 = u32`, `NvU16 = u16`, `NvAPI_UnicodeString = [u16; 2048]` (4096 bytes),
`NvAPI_ShortString = [u8; 64]` (ANSI), `NVAPI_BINARY_DATA_MAX = 4096`,
`NVAPI_SETTING_MAX_VALUES = 100`.

Versão: `MAKE_NVAPI_VERSION(T, v) = sizeof(T) | (v << 16)` (`nvapi_lite_common.h`). O
driver recusa estrutura com versão errada (`-9 NVAPI_INCOMPATIBLE_STRUCT_VERSION`), então o
`version` precisa ser preenchido antes de cada chamada, em cada elemento de um vetor.

| Estrutura | Tamanho | `version` | Observações |
|---|---:|---|---|
| `NVDRS_SETTING_V1` | 12320 | `0x00013020` | `#pragma pack(4)` |
| `NVDRS_PROFILE_V1` | 4116 | `0x00011014` | |
| `NVDRS_APPLICATION_V4` | 20492 | `0x0004500C` | V3 = 16396 (`0x0003400C`) sem `commandLine` |
| `NVDRS_SETTING_VALUES` | 414112 | `0x000751A0` | `#pragma pack(4)`; o tamanho (`0x651A0`) passa de 16 bits e invade o campo de versão — use a macro como está, não “corrija” |
| `NV_GPU_ARCH_INFO_V2` | 16 | `0x00020010` | |

Todos os tamanhos e versões estão travados por `const _: () = assert!(...)` em
`nvapi-drs-poc/src/ffi.rs`.

### NVDRS_SETTING_V1 (offsets)

| Offset | Campo | Tipo |
|---:|---|---|
| 0 | `version` | u32 |
| 4 | `settingName` | `[u16; 2048]` |
| 4100 | `settingId` | u32 |
| 4104 | `settingType` | u32 (`NVDRS_SETTING_TYPE`: 0 DWORD, 1 BINARY, 2 STRING, 3 WSTRING, 4 QWORD, 5 FLOAT) |
| 4108 | `settingLocation` | u32 (0 CURRENT, 1 GLOBAL, 2 BASE, 3 DEFAULT) |
| 4112 | `isCurrentPredefined` | u32 (≠0 ⇒ valor atual é o predefinido; 0 ⇒ gravado pelo usuário) |
| 4116 | `isPredefinedValid` | u32 |
| 4120 | `predefinedValue` | união de 4100 bytes |
| 8220 | `currentValue` | união de 4100 bytes |

A união é `{ u32 | NVDRS_BINARY_SETTING { u32 valueLength; u8 valueData[4096] } |
NvAPI_UnicodeString | u64 | f32 }`. Sob `pack(4)` o maior membro (4100 bytes) define o
tamanho e o `u64` fica com alinhamento 4. **Não use `union` Rust com `u64`** (alinharia a 8 e
mudaria o tamanho para 12328): represente como `[u32; 1025]` com acessores, como na POC.

```rust
#[repr(C)] pub struct NvdrsValue { pub raw: [u32; 1025] }          // 4100 bytes, align 4
#[repr(C)] pub struct NvdrsSettingV1 {
    pub version: u32, pub setting_name: [u16; 2048], pub setting_id: u32,
    pub setting_type: u32, pub setting_location: u32,
    pub is_current_predefined: u32, pub is_predefined_valid: u32,
    pub predefined_value: NvdrsValue, pub current_value: NvdrsValue,
}
#[repr(C)] pub struct NvdrsProfileV1 {
    pub version: u32, pub profile_name: [u16; 2048],
    pub gpu_support: u32,      // bits: 0 geforce, 1 quadro, 2 nvs (somente leitura)
    pub is_predefined: u32, pub num_of_apps: u32, pub num_of_settings: u32,
}
#[repr(C)] pub struct NvdrsApplicationV4 {
    pub version: u32, pub is_predefined: u32,
    pub app_name: [u16; 2048], pub user_friendly_name: [u16; 2048],
    pub launcher: [u16; 2048], pub file_in_folder: [u16; 2048],
    pub flags: u32,            // bits: 0 isMetro, 1 isCommandLine, 2..31 reservados = 0
    pub command_line: [u16; 2048],
}
#[repr(C)] pub struct NvdrsSettingValues {
    pub version: u32, pub num_setting_values: u32, pub setting_type: u32,
    pub default_value: NvdrsValue, pub setting_values: [NvdrsValue; 100],
}
#[repr(C)] pub struct NvGpuArchInfoV2 { pub version: u32, pub architecture: u32,
    pub implementation: u32, pub revision: u32 }
```

Estruturas grandes (`NvdrsSettingValues`, vetores de `NvdrsSettingV1`) vão para o heap.

Arquitetura (`NV_GPU_ARCHITECTURE_ID`): `0x130` Pascal, `0x140`/`0x150` Volta, `0x160`
Turing, `0x170` Ampere, `0x190` Ada, `0x1B0` Blackwell. Implementação dentro de Turing:
`0x02` TU102, `0x04` TU104, `0x06` TU106, `0x07` TU117, `0x08` TU116.

## 4. Códigos de erro relevantes (`nvapi_lite_common.h`)

| Código | Nome | Quando aparece |
|---:|---|---|
| 0 | `NVAPI_OK` | |
| -1 | `NVAPI_ERROR` | genérico |
| -2 | `NVAPI_LIBRARY_NOT_FOUND` | |
| -3 | `NVAPI_NO_IMPLEMENTATION` | função ausente no driver |
| -4 | `NVAPI_API_NOT_INITIALIZED` | faltou `NvAPI_Initialize` |
| -5 | `NVAPI_INVALID_ARGUMENT` | |
| -6 | `NVAPI_NVIDIA_DEVICE_NOT_FOUND` | sem GPU/driver NVIDIA |
| -7 | `NVAPI_END_ENUMERATION` | fim de `EnumSettings`/`EnumApplications`/`EnumProfiles`; também `EnumApplications` no perfil base, que não tem executáveis |
| -8 | `NVAPI_INVALID_HANDLE` | |
| -9 | `NVAPI_INCOMPATIBLE_STRUCT_VERSION` | `version` errado |
| -14 | `NVAPI_INVALID_POINTER` | |
| -137 | `NVAPI_INVALID_USER_PRIVILEGE` | operação exige administrador |
| -160 | `NVAPI_SETTING_NOT_FOUND` | setting não gravada no perfil **ou** oculta da entrada pública (seção 5) |
| -161 | `NVAPI_SETTING_SIZE_TOO_LARGE` | |
| -162 | `NVAPI_TOO_MANY_SETTINGS_IN_PROFILE` | |
| -163 | `NVAPI_PROFILE_NOT_FOUND` | `FindProfileByName` sem resultado |
| -164 | `NVAPI_PROFILE_NAME_IN_USE` | `CreateProfile` com nome existente |
| -165 | `NVAPI_PROFILE_NAME_EMPTY` | |
| -166 | `NVAPI_EXECUTABLE_NOT_FOUND` | `FindApplicationByName` sem resultado (o header cita um `NVAPI_APPLICATION_NOT_FOUND` que não existe no enum) |
| -167 | `NVAPI_EXECUTABLE_ALREADY_IN_USE` | `CreateApplication` com exe já em outro perfil |
| -168 | `NVAPI_DATATYPE_MISMATCH` | `SetSetting` com tipo diferente do registrado |
| -169 | `NVAPI_PROFILE_REMOVED` | |
| -182 | `NVAPI_EXECUTABLE_PATH_IS_AMBIGUOUS` | nome de exe curto com várias correspondências |

Observação sobre o projeto anterior: `drs-leitura.ps1` testa `-12` como fim de enumeração,
mas `-12` é `NVAPI_NO_GL_EXPERT`; o certo é `-7`. Lá não quebra porque o laço também para
quando o lote vem incompleto. O `nvt-drs` deve usar `-7`.

## 5. Settings “ocultas”: por que dão -160 e como ler/gravar

**Fato medido (621.40, perfil base, sem admin):**

| Medida | Entrada pública | Entrada interna |
|---|---:|---|
| `EnumAvailableSettingIds` | 128 IDs | 220 IDs |
| `EnumSettings` (perfil base) | 90 settings | 93 settings (= `numOfSettings` de `GetProfileInfo`) |
| `GetSetting` de `0x0005F543`, `0x10835000`, `0x10834FFE`, `0x00666665`, `0x00676635`, `0x00634291` | **-160** | 0 (valor correto) |

O driver mantém uma lista de settings “expostas pela API”. As entradas públicas da NVAPI
filtram por essa lista: para um ID fora dela, `GetSetting`, `GetSettingNameFromId`,
`SetSetting`, `DeleteProfileSetting` e `RestoreProfileDefaultSetting` respondem
`NVAPI_SETTING_NOT_FOUND` (-160) **mesmo com a setting gravada no perfil**, e
`EnumSettings`/`EnumAvailableSettingIds` simplesmente a omitem. O motivo do filtro não está
documentado; é a NVIDIA restringindo ao SDK público o que ela considera suportado (DLSS
preset profile, estado do Ultra Low Latency no painel, indicadores do NVCPL, NIS, Silk etc.).

O NPI contorna isso usando IDs de interface **internos**, não documentados, que fazem o mesmo
trabalho sem o filtro (tabela da seção 2). Detalhes de `NvapiDrsWrapper.cs` (3.0.2.1):

- `GetDelegate(0xEA99498D, out _DRS_GetSetting, 0x73BF8338)`: assinatura com um quinto
  parâmetro `ref uint x` (o NPI passa 0 e ignora a saída).
- `GetDelegate(0x8A2CF5F5, out _DRS_SetSetting, 0x577DD202)`: assinatura com dois `uint`
  extras no fim (o NPI passa `0, 0`).
- `EnumSettings` `0xCFD6983E`, `EnumAvailableSettingIds` `0xE5DE48E5`,
  `GetSettingNameFromId` `0x1EB13791`, `DeleteProfileSetting` `0xD20D29DF`,
  `RestoreProfileDefaultSetting` `0x7DD5B261`: mesmas assinaturas das públicas.
- Em todos, se `nvapi_QueryInterface(interno)` devolver nulo, usa o público.
- `GetSettingNameFromId` interno devolve OK com **nome vazio** para várias ocultas
  (ex.: `0x0005F543`); o nome legível vem do `CustomSettingNames.xml`/`Reference.xml`
  (por isso o `settings-referencia.json`).
- `DrsDecrypterService`: em perfis **predefinidos**, settings marcadas como
  `InternalSettingFlag=V0` no export de texto (`SaveSettingsToFileEx`) têm o valor
  predefinido cifrado com XOR (chave de 256 bytes no NPI, deslocamento `settingId << 1`).
  Só afeta a exibição do valor **predefinido** pela NVIDIA, não o que o usuário grava.
  Fora do escopo inicial do `nv-tuner`.

**Como o `nv-tuner` (`nvt-drs`) deve fazer:**

1. Resolver cada operação por *interno com fallback para público*, como o NPI, guardando
   qual das duas foi obtida (diagnóstico).
2. Ler sempre pela entrada interna. Classificar uma setting como “oculta da API pública”
   quando a pública dá -160 e a interna dá 0 — a POC faz exatamente isso e imprime as duas
   colunas.
3. Interpretar -160 da interna como “não gravada neste perfil” (o driver usa o valor
   herdado/padrão). Em `GetSetting` de perfil de aplicação, uma setting não gravada no
   próprio perfil volta com `settingLocation` ≠ 0 (herdada do global/base); comparar
   estado real exige olhar `settingLocation == 0` ou usar `EnumSettings` do próprio perfil.
4. Gravar ocultas com `SetSetting` interno (`0x8A2CF5F5`, args extras `0, 0`) e
   `DeleteProfileSetting` interno, sempre seguido de `SaveSettings` — só no serviço
   elevado. **Não verificado nesta pesquisa** (escrita proibida no escopo); validar no
   `nvt-drs` com backup do DRS antes, comparando com o NPI.
5. Se o driver um dia remover um ID interno, a queda para o público mantém o app funcionando
   para as settings públicas; as ocultas passam a dar -160 e a UI deve marcá-las como
   “indisponíveis neste driver” em vez de tratar como ausentes.
6. Contorno do NPI a manter: IDs `0x10C7Dxxx` (`(id & 0xFFFFF000) == 0x10C7D000`) não
   devem passar por `EnumAvailableSettingValues` (estouro no driver 571.96, segundo o NPI).

Privilégios: leitura (sessão, load, get, enum) funciona sem administrador — medido. A
gravação em disco (`SaveSettings`) foi feita no projeto anterior só com elevação (UAC);
sem admin o esperado é -137, não testado aqui.

## 6. Prova de conceito (`nvapi-drs-poc/`)

Projeto Rust standalone (`[workspace]` vazio no próprio `Cargo.toml`, sem dependências).
Somente leitura: não resolve nem chama `SaveSettings`, `SetSetting`, `DeleteProfileSetting`,
`CreateProfile`, `CreateApplication` ou `RestoreProfileDefaultSetting`.

```
cd docs\pesquisa\nvapi-drs-poc
cargo run --release -- [global.nip] [wardogs-dlss-e.nip]
```

Sem argumentos usa `profiles\global.nip` e `profiles\wardogs-dlss-e.nip`, relativos ao
diretório atual (`.nip` exportados pelo NVIDIA Profile Inspector). Saída 0 = tudo confere, 1 = divergência, 2 = erro.

Resultado em 2026-11-25 (driver 621.40, sem admin):

- `Driver: 621.40 (branch r621_20)`
- `GPU 0: NVIDIA GeForce RTX 2060 SUPER — arquitetura 0x160 Turing, implementação 0x6
  (TU106), revisão 0xA1`
- Perfil base “Base Profile”: as **20/20** settings do `global.nip` conferem, todas com
  `settingLocation = 0` (gravadas no próprio perfil). Cinco delas dão -160 na pública:
  `0x0005F543` = 2 (ULL CPL State: Ultra), `0x10835000` = 1 (FRL Low Latency),
  `0x10834FFE` = 0 (Platform Boost), `0x00666665` = 0, `0x00676635` = 0.
- `wardogsclient-win64-shipping.exe` → perfil “WARDOGS” (predefinido, 3 executáveis, 12
  settings). DLSS: `0x10E41E01` = 1 (override ligado), `0x00634291` = 2 (Custom; oculta,
  -160 na pública), `0x10E41DF3` = 5 (preset E) — **3/3** conferem com
  `wardogs-dlss-e.nip`.

## 7. Tabela de nomes (`settings-referencia.json`)

926 settings: fusão por ID de `Reference.xml` (812, gerado do driver) e
`CustomSettingNames.xml` (117, curado), ambos do NPI 3.0.2.1. Regra igual à do NPI
(`SettingMetaSource`: Custom = 10 vence Reference = 40); campo vazio herda do Reference;
valores = união com rótulo do Custom prevalecendo. Campos por setting: `id`, `nome`,
`grupo`, `descricao`, `padrao` (`OverrideDefault`, null em 134), `tipo` (`FallbackType`;
null = DWORD), `oculta_npi` (`<Hidden>` do NPI, 209 — é ocultação da UI do NPI, **não** a
ocultação da NvAPI pública), `nomes_alternativos`, `driver_min`, `driver_max`,
`valores[{valor, rotulo}]`, `fonte`. IDs e valores em hexadecimal como string. Textos em
inglês, como no original; tradução fica para a UI.

O `padrao` do NPI é referência, não verdade: o padrão efetivo vem do driver
(`u32PredefinedValue` com `isPredefinedValid`, ou `EnumAvailableSettingValues`). Para as
públicas, as constantes `*_DEFAULT` de `NvApiDriverSettings.h` também servem.

## 8. Licenças e procedência

- **NVAPI SDK** (`nvapi.h`, `nvapi_lite_common.h`, `nvapi_interface.h`,
  `NvApiDriverSettings.h`, libs): **MIT**, “Copyright (c) 2019-2026 NVIDIA CORPORATION &
  AFFILIATES” (`SPDX-License-Identifier: MIT` no cabeçalho de cada arquivo e no
  `License.txt`). Transcrever IDs, constantes e layouts para Rust é permitido; manter o aviso
  de copyright da NVIDIA no módulo FFI (`nvt-drs`) e no `LICENSE`/créditos do app.
- **NVIDIA Profile Inspector**: **MIT confirmado** — `LICENSE` e `LICENSE.txt` do
  repositório, “Copyright (c) 2016 Orbmu2k”. Usamos dele: os IDs internos (fato técnico
  observado no código), as assinaturas estendidas e os dados de `Reference.xml` e
  `CustomSettingNames.xml` (redistribuídos transformados em `settings-referencia.json`).
  Redistribuir com o aviso de copyright e o texto da licença MIT do NPI.
- O `nv-tuner` não executa nem embute o NPI nem .NET; a POC só usa `kernel32` e a
  `nvapi64.dll` do driver instalado.
- Os IDs internos são API não documentada da NVIDIA: podem mudar ou sumir em qualquer
  driver. Daí o fallback obrigatório e o diagnóstico da seção 5.

## 9. Resultados da validação de escrita (2026-11-25)

Máquina de referência (driver 621.40, branch `r621_20`, RTX 2060 SUPER, Windows 11).
Atualiza o “não verificado” do item 4 da seção 5 e o parágrafo de privilégios.

**Procedimento de segurança.** Antes de gravar: cópia de `C:\ProgramData\NVIDIA
Corporation\Drs\*` (`nvdrsdb0.bin`, `nvdrsdb1.bin`, `nvdrssel.bin`, `nvdrswr.lk`,
`update.bin`, `nvAppTimestamps`) com SHA256 conferido, e um JSON de estado lógico gerado
por `cargo run -p nvt-drs --example estado`: todas as settings do perfil base (93) e do
WARDOGS (12, 3 executáveis), com `location`, `isCurrentPredefined` e valor predefinido,
mais o total de perfis (8051), a ausência do perfil de teste e o `capture()` dos dois perfis.
Só o perfil descartável `nv-tuner-teste-escrita` (aplicação `nv-tuner-teste-escrita.exe`,
inexistente) foi alterado.

**Teste `crates/nvt-drs/tests/escrita_admin.rs::escrita_perfil_descartavel`** (`#[ignore]`,
exige `NVT_TESTE_ESCRITA=1` e administrador; guard RAII apaga o perfil e grava mesmo em
pânico). Passou, duas execuções:

| Etapa | Resultado |
|---|---|
| `CreateProfile` + `CreateApplication` + `SetSetting` pública `0x1057EB71` = 1 + `SetSetting` **oculta** `0x0005F543` = 2 (entrada interna `0x8A2CF5F5`, args `0, 0`) + `SaveSettings` | OK |
| Sessão nova: perfil não predefinido com 1 executável, `FindApplicationByName` aponta para ele, as duas settings voltam com `location = 0` e como “do usuário” no `EnumSettings` | OK |
| `RestoreProfileDefaultSetting` (pública) e `DeleteProfileSetting` (oculta) + save; sessão nova: nenhuma das duas gravada no perfil | OK (num perfil de usuário, sem predefinido, *restore* equivale a apagar) |
| `DeleteApplication` (`0x2C694BC6`) + save; sessão nova: executável ausente, `numOfApps = 0` | OK |
| `DeleteProfile` (`0x17093206`) + save; sessão nova: perfil e executável ausentes | OK |

Ou seja: a gravação de settings ocultas pela entrada interna funciona e persiste, como no NPI.

**Estado depois.** O JSON de estado lógico ficou **byte a byte idêntico** ao de antes
(base, WARDOGS, total de perfis, ausência do perfil de teste), conferido após cada execução.
Os arquivos `nvdrsdb0.bin`/`nvdrsdb1.bin` mudam de hash a cada `SaveSettings` (o driver
reescreve o banco; `nvdrssel.bin` e `nvAppTimestamps` também mudaram em execuções
seguintes), por isso a comparação é lógica, não de hash; `update.bin` não mudou. Não foi
preciso restaurar o backup.

**Privilégio — o esperado (-137 sem admin) NÃO se confirmou no 621.40.** Teste
`save_sem_admin` (`#[ignore]`, exige `NVT_TESTE_SEM_ADMIN=1` e processo não administrador
conforme `IsUserAnAdmin`):

| Processo | Token | `SaveSettings` sem alteração | `SaveSettings` criando o perfil descartável |
|---|---|---|---|
| `runas /trustlevel:0x20000` (a partir de sessão elevada) | Administradores “apenas para negar”, integridade **alta**, `TokenElevation = 1` | OK | não testado |
| Tarefa agendada `/RL LIMITED /IT` (temporária, apagada em seguida) | Administradores “apenas para negar”, integridade **média** | OK | **OK — gravou e persistiu** (sessão nova leu o perfil com a setting; o guard o apagou) |

Conclusões:

1. Neste driver, um processo comum do usuário grava o DRS: a escrita é feita pelo serviço
   `NVDisplay.ContainerLocalSystem`, não pelo processo chamador. A recusa por privilégio não
   é uma barreira com que o `nv-tuner` possa contar; a política (quem pode gravar) fica no
   próprio app/serviço.
2. O mapeamento `-137 → Error::PrivilegeRequired` continua implementado e coberto por teste
   de unidade, mas não foi observado no driver real; o teste `save_sem_admin` aceita os dois
   resultados (recusa com -137 ou gravação persistida) e falha em qualquer outro erro.
3. Para detectar admin em testes, `TokenElevation` não serve (o token do `runas
   /trustlevel` diz “elevado”); use pertencimento a Administradores (`IsUserAnAdmin` /
   `CheckTokenMembership`).
4. Funções adicionadas ao `nvt-drs`: `Session::delete_profile` (`NvAPI_DRS_DeleteProfile`,
   `0x17093206`, `(hSession, hProfile)`) e `Session::delete_application`
   (`NvAPI_DRS_DeleteApplication`, `0x2C694BC6`, `(hSession, hProfile, NvAPI_UnicodeString)`),
   IDs conferidos no `nvapi_interface.h` do SDK.
