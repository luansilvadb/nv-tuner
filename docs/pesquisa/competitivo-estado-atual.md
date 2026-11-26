# Estado atual — auditoria para a fase "desempenho competitivo"

Data: 2026-11-26. Auditoria somente leitura do código em `main` (após
`c5e5c08`), feita para planejar a fase descrita em
`docs/intent/competitivo.md`. Referências no formato `arquivo:linha`.

---

## 1. Settings DRS aplicadas hoje

Todas as regras estão em `regras/base.toml` e todas gravam **apenas** IDs do
catálogo `regras/settings.toml`, que também é a lista permitida do serviço
(`regras/settings.toml:3-5`, `crates/nvt-core/src/catalogo.rs:4`). Os dois
arquivos são embutidos no binário (`crates/nvt-core/src/lib.rs:35-42`): não há
regras carregadas em runtime.

Evidência registrada: laboratório único (RTX 2060 SUPER/Turing, driver 621.40,
1080p 240 Hz, PresentMon 2.6.0, mediana de 3×60 s) — `regras/base.toml:3-5`.

### 1.1 Global (perfil base), sempre que casam

| Regra (`base.toml`) | Condição | Settings → valor | Justificativa registrada |
|---|---|---|---|
| `baseline-baixa-latencia-ultra` (11-23) | nenhuma | `low_latency_cpl = ultra` (0x0005F543=2), `low_latency_enabled = on` (0x10835000=1) | Reduz a fila de frames da CPU sem Reflex; LoL DX11: entrada→fóton 1,54 ms vs 1,71 ms |
| `baseline-energia-desempenho-maximo` (25-36) | nenhuma | `energia = desempenho_maximo` (0x1057EB71=1) | Evita quedas de clock/picos de frametime; mantido sem regressão em LoL e WARDOGS |
| `baseline-shader-cache` (38-49) | nenhuma | `shader_cache = on` (0x00198FFF=1) | Evita recompilação/engasgos no início da partida (é o padrão do driver) |
| `baseline-indicadores-desligados` (51-64) | nenhuma | `indicador_vrr`, `indicador_mfaa`, `indicador_triple_buffering = off` | Overlays de depuração; o do VRR vem ligado por padrão |
| `baseline-filtragem-pela-aplicacao` (66-80) | nenhuma | `af_modo = aplicacao`, `af_otimizacao = off`, `af_otimizacao_amostras = off`, `aa_modo = aplicacao` | Não brigar com TAA/DLSS do jogo; limpar herança de "otimizadores" |

### 1.2 Global, traduzindo a preferência de sincronismo (prioridade 60)

| Regra | Condição | Settings → valor | Justificativa |
|---|---|---|---|
| `sincronismo-sem-sync` (84-100) | `sincronismo = sem_sync` (padrão) | `vsync = force_off`, `gsync_global = off`, `gsync_app_state = forcar_off` | Menor latência; LoL: 391,5 FPS vs 224,3 da estratégia G-SYNC+V-Sync+limite; Present→exibição 0,86→0,64 ms |
| `sincronismo-gsync` (102-119) | `sincronismo = gsync` | `gsync_global = on`, `gsync_app_mode = tela_cheia_e_janela`, `gsync_app_state = permitir`, `vsync = force_on` | VRR sem tearing, cobre janela sem bordas |
| `sincronismo-gsync-limite` (121-136) | `gsync` **e** `refresh_conhecido = true` | `limite_fps = refresh − 3` | Mantém o FPS na janela VRR (ver lacuna 6.1: nunca casa hoje) |
| `sincronismo-vsync` (138-154) | `sincronismo = vsync` | `vsync = force_on`, `gsync_global = off`, `gsync_app_state = forcar_off` | Sem tearing, sem VRR; não medido isoladamente |

Preferência direta: `limite_fps` do usuário grava `limite_fps` (0x10835002,
faixa 20–1000, `regras/settings.toml:70-75`) no perfil global com a maior
classe de precedência (`crates/nvt-core/src/evaluator.rs:145-181`).

### 1.3 Por jogo

| Regra | Status | Condição | Settings → valor | Justificativa |
|---|---|---|---|---|
| `dlss-preset-e-turing` (160-184) | estável | `arquitetura = turing`, `tem_tensor_cores`, `jogo_dlss` | `dlss_override = on`, `dlss_perfil_preset = personalizado`, `dlss_preset = e` | Transformer caro sem FP8; WARDOGS: +16,8% FPS, −14,1% p99, −15,1% entrada→fóton; "só usa o perfil do driver (seguro com anticheat de kernel)" |
| `dlss-preset-e-ampere` (186-210) | **experimental** | igual, `ampere` | idem | Extrapolação de Turing; só sugestão, não aplicada |

Ada/Blackwell declaram `arquitetura_nao_precisa` (`base.toml:178-179`,
`204-205`). Nenhuma regra usa `jogo_api`, `jogo_reflex` ou
`jogo_anticheat_kernel` hoje.

### 1.4 Catálogo sem regra

Existem no catálogo, mas nenhuma regra os grava: `vsync_tear_control`
(`settings.toml:28`) e `platform_boost` (`settings.toml:78`). Os valores
`fast_sync`, `meio_refresh` etc. de `vsync` (`settings.toml:25`) também não
são usados.

---

## 2. Motor de regras

### 2.1 Fatos disponíveis (`quando`)

`Condicoes` (`crates/nvt-core/src/rules.rs:61-85`), E lógico, fato
desconhecido (`None`) nunca casa (`rules.rs:59-60`, `143-285`):

- **GPU** (`Capabilities`, `crates/nvt-core/src/capabilities.rs:66-84`):
  `arquitetura` (turing/ampere/ada/blackwell/desconhecida,
  `capabilities.rs:14`), `tem_tensor_cores`, `tensor_fp8`, `frame_gen`, `mfg`,
  `rebar_suportado`, `rebar_ativo`, `vram_mb_min`, `driver_min`,
  `refresh_conhecido`.
- **Jogo** (`GameFacts`, `crates/nvt-core/src/facts.rs:40-52`, só em escopo
  `por_jogo`): `jogo_api`, `jogo_dlss`, `jogo_dlss_min`, `jogo_reflex`,
  `jogo_anticheat_kernel`. `GameFacts::id` é o **nome do perfil DRS**
  (`facts.rs:42-44`).
- **Preferências**: `sincronismo`, `qualidade`, `limite_fps_definido`.

Não há fatos de: Windows (versão, HAGS, Game Mode, plano de energia, VBS/HVCI),
CPU, monitor além do refresh, resolução, nem gênero/"competitivo".

### 2.2 Preferências do usuário

`Preferences` (`crates/nvt-core/src/preferences.rs:88-97`): `sincronismo`
(padrão `SemSync`, `preferences.rs:9-18`), `qualidade` (padrão
`Equilibrado`, `preferences.rs:36-44`), `limite_fps` e `escolhas_usuario`
(travas por `(Alvo, setting)`, `preferences.rs:79-84`). No IPC são só
`sincronismo`, `qualidade`, `limite_fps` (`crates/nvt-ipc/src/protocolo.rs:137-141`).

### 2.3 Resolução (avaliador)

`crates/nvt-core/src/evaluator.rs:37-269`:

1. Regras que não casam → motivo estruturado por jogo (`evaluator.rs:71-81`).
2. **Teto de qualidade**: `impacto_visual > teto` descarta
   (`evaluator.rs:82-101`); Desempenho≤Forte, Equilibrado≤Leve,
   Fidelidade≤Nenhum (`preferences.rs:48-54`).
3. **Experimental** vira só `Sugestao` (`evaluator.rs:115-125`); não há
   caminho para o usuário aceitar uma sugestão (o serviço só a imprime no
   dry-run, `crates/nvt-service/src/relatorio.rs:271-279`).
4. Conflito por `(alvo, setting)` com chave `(classe, por_jogo, nº de
   condições, prioridade, −id)` (`evaluator.rs:28-29`, `128-134`):
   classe 2 = preferência direta, 1 = regra que traduz preferência
   (`governada_por`, `evaluator.rs:343-361`), 0 = regra comum.
5. **Escolha manual** travada: a mudança vencedora vai para `respeitadas` e
   não entra no plano (`evaluator.rs:210-223`).

### 2.4 "Estabilidade > latência > FPS"

É política documental (`docs/arquitetura.md:91-93`, `README.md:89`,
`CONTRIBUTING.md:22`, `docs/regras.md:246-247`), **não está codificada**:
não há campo de risco/estabilidade na regra nem critério no avaliador. Ela só
atua na curadoria (status `experimental` → `estavel` com medição). O único
eixo de risco codificado é `impacto_visual`.

---

## 3. Detecção de jogos e de anticheat

### 3.1 Jogos

`nvt_detect::varrer_bibliotecas` (`crates/nvt-detect/src/lib.rs:31-33`):
Steam, Riot, Epic, GOG, Ubisoft, EA, Battle.net, Xbox e genérico por chaves de
desinstalação, lendo manifests/registro (`launchers/*.rs`,
`registro.rs:13-20` com `KEY_READ`). Fatos por jogo
(`crates/nvt-detect/src/facts/mod.rs:278-330`):

- listagem da pasta, até 14 níveis/200 000 arquivos
  (`facts/arquivos.rs:5-8`, `43-`);
- API pelos imports do exe principal (`facts/mod.rs:152-189`) ou vizinhos
  (`197-231`);
- DLSS por `nvngx_*.dll` + VERSIONINFO (`facts/mod.rs:233-281`);
- Reflex por `sl.reflex.dll`/`nvlowlatencyvk.dll`/`sl.pcl.dll`
  (`facts/mod.rs:283-307`). Reflex via NVAPI não deixa rastro: o motor só
  afirma Reflex com confiança média/alta, senão `None`
  (`crates/nvt-engine/src/deteccao.rs:91`) — na prática `jogo_reflex = false`
  nunca casa.

Conversão para o motor descarta a pasta: `JogoEntrada` tem só nome,
launcher, nome do exe, API, DLSS, Reflex, anticheat
(`crates/nvt-engine/src/deteccao.rs:12-24`). O cruzamento com o DRS é pelo
**nome do exe** (`crates/nvt-engine/src/engine.rs:528-567`).

### 3.2 Anticheat

Tabela de dados `crates/nvt-detect/data/anticheats.toml` (EAC, BattlEye,
Vanguard, Elytra; EA Javelin, ACE, GameGuard, Ricochet não verificados,
`anticheats.toml:48`). Indícios: pastas/arquivos na instalação, VERSIONINFO de
DLL, metadado Riot e serviços em `HKLM\SYSTEM\CurrentControlSet\Services`
(`facts/anticheat.rs:66-140`, `registro.rs:105-125`). Serviço sozinho não
atribui anticheat a um jogo (`facts/anticheat.rs:62-65`).
`anticheat_kernel` só vira `Some(true)`; ausência fica `None`
(`deteccao.rs:74`, `92`) — uma regra com `jogo_anticheat_kernel = false`
nunca casaria.

### 3.3 Noção de "jogo competitivo"

**Não existe.** Não há fato, lista, gênero nem condição de regra; a palavra só
aparece no texto da UI sobre "sem sincronismo"
(`crates/nvt-ui/src/textos.rs:146-147`). Anticheat de kernel é o proxy mais
próximo disponível, mas é incompleto (CS2/VAC, Dota 2, Overwatch etc. não têm
anticheat de kernel).

### 3.4 Toca processos de jogos?

**Não.** Busca por `OpenProcess`, Toolhelp, `EnumProcesses`,
`ReadProcessMemory`, `Module32*`, `EnumProcessModules`,
`QueryFullProcessImageName` não encontra nada; o único `OpenProcessToken` é no
próprio processo (`crates/nvt-service/src/diretorio.rs:677`). `Command::new`
só aparece em testes (`diretorio.rs:719+`, `logs.rs:214+`). A detecção é
**somente em disco e registro**:

- abre arquivos do jogo para leitura via `pelite::FileMap`
  (`facts/pe.rs:92-99`, `110-`), que usa `CreateFileW(GENERIC_READ,
  FILE_SHARE_READ)` — mapeia o **arquivo**, não o processo. Efeito colateral:
  enquanto o mapeamento existe, ninguém consegue abrir o arquivo para escrita
  (patch do launcher/anticheat pode falhar momentaneamente);
- inclui DLLs de anticheat (`EasyAntiCheat_x64.dll`, `coreinit.dll`) para ler
  VERSIONINFO (`facts/anticheat.rs:101-117`).

Quem roda: o serviço (LocalSystem, `instalador/nv-tuner.wxs:149`) a cada 6 h
e no início (`crates/nvt-service/src/nucleo.rs:65-72`, `293-309`,
`laco.rs:390`), e a UI no modo de visualização, como usuário
(`crates/nvt-ui/src/local.rs:161`). Como LocalSystem, HKCU é o do SYSTEM
(`docs/revisao-v0.1.md:349-358`).

O único "vigia" é `ReadDirectoryChangesW` no diretório do DRS
(`crates/nvt-service/src/vigia.rs:1-4`). Nada detecta jogo em execução.

---

## 4. Snapshot, restauração, histórico e reaplicação

- **Snapshot original** (`crates/nvt-engine/src/estado.rs:5-7`): DRS antes
  da primeira aplicação (perfil base + perfis dos exes,
  `crates/nvt-drs/src/snapshot.rs:90-130`), publicado por hard link, nunca
  sobrescrito. Perfis geridos depois entram em `perfis_adicionais`
  (`engine.rs:1020-1062`).
- **Estado** (`estado.rs:149-160`): preferências, travadas, histórico
  (limite 2000, `estado.rs:42`), `esperado`, `driver_visto`,
  `perfis_criados`. Diretório `%ProgramData%\nv-tuner` (`estado.rs:313-319`).
- **Aplicar** (`engine.rs:786-967`): valida tudo na lista permitida
  (`792-794`), garante snapshot, cria perfis, grava, `salvar`, **relê** e só
  então registra `Esperado` + histórico; o valor "antes da gestão" por
  setting é guardado para reverter quando a setting sai do plano
  (`969-1001`, `855-903`).
- **Deriva** (`engine.rs:300-325`, `1067-`, `1120-`): troca de driver ou
  reset em massa → reaplica; mudança pontual → vira escolha travada. Serviço
  verifica a cada 10 min e após notificação do DRS (debounce 3 s)
  (`nucleo.rs:65-72`, `312-370`), e aplica o plano com autonomia ativa
  (`nucleo.rs:374-`). `RestaurarOriginal` pausa a autonomia
  (`nucleo.rs:85-88`).
- **Restaurar** (`engine.rs:1249-1369`): diff snapshot × DRS atual
  (`nvt_drs::restore_plan`), ignorando IDs fora da lista permitida, zerando
  settings de perfis criados pelo app; limpa `esperado`, mantém travadas.

**Acoplamento ao DRS: total.** `Esperado`, `Travada`, `RegistroHistorico` e
`Reversao` usam `ProfileRef` + `id: u32` DRS (`estado.rs:32`, `93-104`,
`126-137`); o snapshot é `nvt_drs::Snapshot`; `DriverStore` é um trait de
perfis/settings DRS (`crates/nvt-engine/src/store.rs:24-59`); a deriva
depende da versão do driver e da semântica "voltou ao padrão" do DRS;
`SettingChange.valor` é `u32` (`crates/nvt-core/src/plan.rs:38-51`). A parte
reaproveitável é o padrão (snapshot imutável + antes da gestão + releitura +
histórico + travas), não os tipos.

---

## 5. Pontos de extensão para novas camadas

### 5.1 Onde encaixar

| Peça | Hoje | Para uma camada nova (Windows/arquivo do jogo) |
|---|---|---|
| Catálogo/lista permitida | `DefSetting { nome, id: u32, valores, faixa }` (`catalogo.rs:51-66`) | Precisa de um "destino" por camada (DRS id / chave+valor de registro / arquivo+chave) com tipos além de DWORD (string, float, INI/JSON/cfg) |
| Regra | `aplicar: nome → ValorAcao` (`rules.rs:47-57`, `335`) | Reaproveitável se o nome simbólico identificar a camada; `ValorAcao` só tem `Nome`/`Numero`/`RefreshMenos` |
| Fatos | `Capabilities`/`GameFacts` | Faltam fatos de SO e do jogo (caminho do config, título conhecido, "competitivo") |
| Plano | `Plan.global` / `por_jogo` de `SettingChange` com `id`/`valor: u32` | Um `SettingChange` genérico ou uma lista por camada |
| Aplicação | `Engine<S: DriverStore>` (`engine.rs:328-332`) | Um "store" por camada com ler/gravar/remover/capturar e semântica própria de snapshot e deriva |
| Estado/snapshot | tipos DRS (seção 4) | Snapshot original por camada (valor anterior do registro; cópia íntegra do arquivo de config + hash) e nova `VERSAO_ESTADO` (`estado.rs:39`) |
| Explicação | `MotivoNaoAplicada` (`explicacao.rs:78-105`), `TipoMotivo` fechado (`17-24`) | Novos motivos (ex.: "jogo em execução", "arquivo alterado pelo jogo", "requer reinício") |
| UI | nomes legíveis por setting (`crates/nvt-ui/src/textos.rs:444-`), `nomes.rs` no serviço | Textos e agrupamento por camada |

### 5.2 Privilégios

- O serviço (LocalSystem) é o **único** que grava (`crates/nvt-service/src/main.rs:3-5`);
  a UI roda sem privilégios e, localmente, é estritamente somente leitura
  (`crates/nvt-ui/src/local.rs:1-13`).
- **Registro HKLM / `powercfg`**: cabem no serviço, mas hoje não há nenhuma
  escrita de registro (`registro.rs` só lê) nem execução de processos; a
  política diz que o serviço "não executa comandos arbitrários nem lê/grava
  arquivos fora dos seus próprios diretórios" (`docs/arquitetura.md:118-119`)
  — precisa ser revista (e já é contrariada pela leitura das pastas dos
  jogos). Preferir API (`PowerSetActiveScheme`/`PowerWriteACValueIndex`) a
  executar `powercfg.exe`.
- **HKCU e arquivos de config de jogos** são por usuário
  (`%LOCALAPPDATA%`, `Documents`, pasta Steam `userdata`). Como LocalSystem, o
  serviço vê o HKCU errado (seção 3.4). Opções: o serviço resolver o usuário
  interativo (sessão ativa / SID do cliente do pipe — hoje o pipe só
  identifica, `docs/ipc.md` "Demais proteções") e gravar no perfil dele com
  cuidado contra links/reparse points (há infraestrutura em
  `crates/nvt-service/src/diretorio.rs`, ex. `abrir_sem_reparse`), ou um
  componente por usuário. Gravar como SYSTEM em pasta controlada pelo usuário
  é vetor clássico de escalada de privilégio.

### 5.3 Protocolo IPC (v3)

`PROTOCOL_VERSION = 3` (`crates/nvt-ipc/src/lib.rs:25`), sem negociação; toda
mudança incompatível incrementa a versão (`docs/ipc.md:89`). DTOs usam
`deny_unknown_fields` (ex. `protocolo.rs:437-438`, `589-590`). Requisições
são comandos de alto nível (`protocolo.rs:28-51`); settings só por nome
simbólico (`protocolo.rs:1-6`, `docs/ipc.md:10-17`). Novas camadas exigem v4:
`PlanoPerfil`/`Mudanca` assumem "perfil do driver" (`protocolo.rs:580-605`),
`Jogo` carrega fatos de DRS/DLSS (`438-457`), `MotivoNaoAplicada` é fechado
(`507-530`), `Preferencias` tem só três campos (`137-141`). `TravarSetting
{ perfil, setting }` (`protocolo.rs:47`) precisa de um alvo que não seja
perfil DRS.

### 5.4 Plano auditável e "o que o app fez"

Cada mudança já carrega origem e explicação (`plan.rs:36-51`), o plano guarda
`respeitadas`, `experimentais` e `descartes` (`plan.rs:81-97`) e o resumo por
jogo lista aplicadas/não aplicadas/herdadas do global
(`evaluator.rs:238-266`). Isso se estende bem se as novas ações forem
`SettingChange` com camada; o `herda_do_global` é conceito do DRS e não vale
para arquivos de jogo.

---

## 6. Lacunas e riscos

1. **Refresh nunca é lido**: `refresh_hz: None` e `rebar_ativo: None` fixos
   (`crates/nvt-engine/src/deteccao.rs:44`, `61`, `64`). Logo
   `sincronismo-gsync-limite` **nunca casa**: no modo G-SYNC o app liga
   G-SYNC + V-Sync sem limite de FPS — exatamente a combinação que a própria
   evidência mostra encostar no teto e somar latência (`base.toml:128-129`).
   Ganho competitivo barato e já com regra pronta.
2. **Prioridade estabilidade > latência > FPS não é código** (2.4); novas
   camadas com risco variável precisarão de um eixo de risco explícito além de
   `impacto_visual`.
3. **Sem noção de jogo competitivo** nem de jogo em execução (3.3, 3.4).
   Mudanças em arquivos de config com o jogo aberto podem ser sobrescritas
   pelo jogo ao sair; DRS só vale na próxima abertura do jogo.
4. **Reflex quase sempre desconhecido** (3.1): regras do tipo "sem Reflex →
   X" não conseguem casar; a base global ULL Ultra já cobre o caso, mas nada
   por jogo distingue Reflex.
5. **Anticheat só positivo** (3.2): "sem anticheat" é inexprimível; lista não
   verificada para 4 dos 8 anticheats; VAC/FACEIT/Vanguard fora de Riot
   dependem de serviço + indício na pasta.
6. **Leitura de arquivos dos jogos com `FILE_SHARE_READ`** (3.4): não é
   acesso a processo, mas pode bloquear brevemente um patch; para uma fase
   que vai mexer em arquivos dos jogos, convém evitar mapear binários de
   anticheat e jogos em execução.
7. **Detecção como LocalSystem usa o HKCU do SYSTEM** (seção 3.4,
   `docs/revisao-v0.1.md:349-358`): bloqueia diretamente a camada "config do
   jogo/HKCU".
8. **Promessa pública conflitante**: README e arquitetura afirmam "apenas na
   camada do driver" e "não altera arquivos de configuração dos jogos"
   (`README.md:91-92`, `docs/arquitetura.md:9-14`); a nova fase precisa
   reescrevê-la (como já previsto em `docs/intent/competitivo.md`).
9. **DLSS override troca a DLL pela do driver** (`settings.toml:148`):
   feito pelo driver, não por arquivo do jogo (seguro com anticheat), mas o
   risco é de compatibilidade/estabilidade, não de banimento.
10. **Experimentais sem caminho de adoção** (2.3): não há opt-in do usuário;
    regras novas da fase ficarão invisíveis até serem promovidas.
11. **Catálogo restrito a DWORD** e `SettingChange.valor: u32`
    (`plan.rs:43`): config de jogo (strings, floats, chaves aninhadas) não
    cabe no modelo atual.
12. **Telemetria inexistente** (`crates/nvt-telemetry/src/lib.rs` tem 5
    linhas) embora a política preveja rollback por regressão
    (`docs/arquitetura.md:99-100`); como a intenção dispensa medição no app,
    a promoção de regras segue dependente do laboratório de uma única GPU
    Turing.
13. **Identidade do jogo = nome do perfil DRS / nome do exe**
    (`facts.rs:42-44`, `engine.rs:528-567`): exe genérico (ex.:
    `Client-Win64-Shipping.exe`) pode colidir; a camada de config precisa do
    caminho da instalação, que hoje é descartado em `JogoEntrada`.
