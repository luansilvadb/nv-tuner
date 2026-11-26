# Revisão cética — driver e estado atual (fase competitiva)

Data: 2026-11-26. Revisão independente de
[`competitivo-driver.md`](competitivo-driver.md) e
[`competitivo-estado-atual.md`](competitivo-estado-atual.md), cruzada com
`regras/base.toml`, `regras/settings.toml` e
`crates/nvt-engine/src/deteccao.rs`. O objetivo era **refutar**. Onde não deu,
a afirmação está marcada como confirmada.

Legenda: **confirmada** · **parcialmente correta** (com a correção) ·
**refutada**. As fontes estão no fim do documento. Fonte de fórum é marcada
como **[F]**.

## Resumo

| # | Afirmação | Veredito |
|---|---|---|
| 1 | `refresh_hz` nunca é detectado e `sincronismo-gsync-limite` nunca casa | **Confirmada no código.** O impacto descrito está **exagerado**: o Ultra global já impõe um limite automático |
| 2 | Limite `min(Hz−3, Hz·3600/(Hz+3600))`; “−3” fica apertado acima de 144 Hz | **Parcialmente correta.** A fórmula é heurística de fórum; “−3” é o mínimo da Blur Busters; a regra quase não age hoje |
| 3 | Ultra global pode piorar com limite de FPS ou CPU-bound; com Reflex é irrelevante | **Parcialmente correta.** A evidência é de cenário limitado por FPS, não de CPU-bound; o laboratório do projeto contradiz em parte |
| 4 | ULLM sem efeito em DX12/Vulkan/OpenGL; DX12 a partir do 551.23 | **Parcialmente correta.** DX12 ≥ 551.23 e Vulkan sem efeito confirmados; OpenGL não tem fonte; a ressalva do 551.23 não importa no driver do laboratório (621.40) |
| 5 | Max Frame Rate custa ~½–1 frame a mais que o limitador do jogo ou do Reflex | **Confirmada**, mas a fonte é de fórum/G-SYNC 101 e não há medição isolada do limitador V3 |
| 6 | Energia “desempenho máximo” melhora latência | **Refutada como ganho de latência.** Sem medição e fora do guia da NVIDIA; o próprio relatório só fala em consistência. A regra é global, contra a própria recomendação de aplicar por jogo |
| 7 | DRS é seguro com anticheat; o caso Anti-Lag+ | **Confirmada**, com ressalvas (risco baixo, não nulo; o override de DLSS carrega DLL do driver no processo) |
| 8 | A detecção não toca processos de jogos | **Confirmada** (só disco e registro), mas abre e mapeia binários dos jogos e dos anticheats |

## 1. `refresh_hz` e a regra `sincronismo-gsync-limite`

**No código: confirmada.**

- `crates/nvt-engine/src/deteccao.rs:64` grava `refresh_hz: None`, e o comentário
  em `deteccao.rs:44` admite isso. É a única construção de `Capabilities` em
  produção (`engine.rs:407-408`, usada em `nucleo.rs:623`). O outro `None` está
  no `Default` (`crates/nvt-core/src/capabilities.rs:107`). Valores reais só
  aparecem em testes (`crates/nvt-core/tests/avaliador.rs:219`, `271`).
- `refresh_conhecido` é avaliado como `Some(caps.refresh_hz.is_some())`
  (`crates/nvt-core/src/rules.rs:259-261`). Com isso a condição de
  `regras/base.toml:131-133` é sempre falsa. Mesmo que casasse, `RefreshMenos`
  descartaria a ação com `refresh_hz = None` (`evaluator.rs:319-337`).
- A UI declara `vrr: false` pelo mesmo motivo (`crates/nvt-ui/src/local.rs:612-613`).

**No impacto: refutada em grande parte.** O estado atual (§6.1) diz que o modo
G-SYNC fica “sem limite de FPS — exatamente a combinação que … encosta no teto”.
Isso ignora três fatos:

- `baseline-baixa-latencia-ultra` grava Ultra no **global**, sem condição
  (`regras/base.toml:11-23`).
- A NVIDIA documenta que G-SYNC + V-Sync + **Ultra ou Reflex** limita o FPS
  automaticamente abaixo do refresh (guia de latência da NVIDIA; G-SYNC 101,
  “Optimal Settings”).
- O laboratório do projeto registra o limite em funcionamento: “o driver capou o
  FPS automaticamente (224,3 FPS a 240 Hz)” (`regras/base.toml:110`). 224 é o
  valor do limite automático (~224 @240), não o teto de 240. A evidência de
  `base.toml:129`, que diz “o FPS encostou no teto”, interpreta mal o próprio
  dado.

Por isso, no modo `gsync`, o limite já existe em jogos DX9/DX11/DX12 sem Reflex
(via Ultra) e em jogos com Reflex. Ficam sem limite:

- jogos **Vulkan/OpenGL** sem Reflex;
- o caso em que o usuário trava `low_latency_cpl` em On/Off;
- jogos em que o limite automático do Ultra não pega. A Blur Busters afirma que
  ele “não é confiável em todos os jogos” [F].

Mesmo se a regra casasse, `Hz−3` = 237 @240 fica **acima** do limite automático
de 224. O limitador mais baixo vence, então a regra não teria efeito justamente
onde o limite automático funciona. O ganho é real só nesses casos residuais. Não
é o “ganho competitivo barato” anunciado.

## 2. Fórmula do limite e a crítica ao “−3”

- **NVIDIA:** não publica número nem fórmula. O guia de latência diz apenas que
  V-Sync + Reflex/Ultra “will automatically cap the framerate below the refresh
  rate”. A NVIDIA também admite que isso dá “slightly higher latency than just
  letting your FPS run uncapped with NVIDIA Reflex enabled”, o que reforça o
  padrão `sem_sync`.
- **Blur Busters (G-SYNC 101):** recomenda limite **“pelo menos 3 FPS abaixo”**
  do refresh quando não há limite automático. Os valores observados do
  automático são ~59 @60, ~97 @100, ~116 @120, ~138 @144 e ~224 @240.
- **Fórmula:** um moderador do fórum diz explicitamente que não existe fórmula
  oficial e que todas são estimativas [F]. Circulam duas:
  - `Hz·3600/(Hz+3600)`, usada pelo relatório: dá 225 @240, 327 @360, 423 @480;
  - `Hz − Hz²/3600`: dá 224 @240 (bate exatamente com o valor da Blur Busters),
    324 @360, 416 @480.

  A segunda reproduz melhor os pontos conhecidos. O relatório admite a
  diferença de 1 FPS a 240 Hz, mas escolheu a fórmula que diverge.
- **A crítica ao “−3”** é **coerente no mecanismo**. A 240 Hz, 237 FPS = 4,22 ms
  contra 4,17 ms do refresh, uma folga de 0,05 ms, menor que o jitter típico do
  limitador. Ela **não é** o que a Blur Busters recomenda textualmente (“−3” é o
  mínimo, sem escala por refresh) e não foi medida em lugar nenhum.
- **Correção recomendada:** se a regra for reativada, usar o mesmo alvo do
  limite automático (`Hz − Hz²/3600`, piso de `Hz−3`). O limite manual então
  coincide com o do Reflex/Ultra em vez de ficar acima dele. Só tem efeito
  prático em Vulkan/OpenGL ou com o LLM travado pelo usuário. Antes disso, é
  preciso detectar o refresh e se o VRR está **ativo** (item 1).

## 3. Ultra global, limite de FPS, CPU-bound e jogos com Reflex

- **NVIDIA:** o Low Latency Mode tem “most impact when your game is GPU bound,
  and framerates are between 60 and 100 FPS” (artigo do 436.02).
- **Battle(non)sense:** os números conhecidos vêm de discussões que citam o
  vídeo, não do vídeo. O cenário em que “Off” venceu “Ultra” é **FPS limitado
  com GPU abaixo de 95%**, não CPU-bound em si. A Blur Busters (jorimt) conclui
  “Off, no máximo On, não Ultra” para a menor latência. Diz ainda que, com
  limite de FPS e sem GPU-bound, a fila já é ~0 e a setting quase não atua. Não
  achei medição séria de Ultra em CPU-bound puro (a pergunta sobre CS:GO no
  fórum ficou sem resposta).
- **Contraponto local:** LoL a ~391 FPS numa RTX 2060 SUPER é provavelmente
  limitado pela CPU, e o Ultra **melhorou** a latência média (1,54 vs 1,71 ms,
  `regras/base.toml:19`). Isso não prova o caso geral, mas contradiz a
  generalização “Ultra piora em CPU-bound”.
- **Correção:** a frase deve dizer “com FPS limitado (pelo jogo ou pelo
  driver)”, não “CPU-bound”. O efeito medido é de poucos ms e vem da era dos
  drivers 43x.
- **Jogos com Reflex nativo** (CS2, Valorant, Fortnite, Apex, OW2, R6, CoD):
  “NVIDIA Reflex will override Ultra Low Latency functionality” (guia da
  NVIDIA). Nesses jogos o Ultra global é **inócuo** quando o usuário liga o
  Reflex. O problema prático está em outro ponto: o app não sabe se o jogo tem
  Reflex. Esses títulos usam Reflex via NVAPI, sem `sl.reflex.dll`, e
  `deteccao.rs:91` devolve `None`. O app também não liga o Reflex do jogo. A
  ação útil é **orientar o usuário** a ligar o Reflex (On, ou On+Boost se a CPU
  sobrar) e a usar o limitador do jogo, não do driver. Também é preciso uma base
  curada por executável para `jogo_reflex`.

## 4. ULLM em DX12/Vulkan/OpenGL e o driver 551.23

- **DX9/DX11:** suportado desde o 436.02. Em 2019 a NVIDIA dizia: “in DX12 and
  Vulkan titles, the game decides when to queue the frame”.
- **DX12:** suporte adicionado no 551.23 (jan. 2024), segundo as notas de versão
  reproduzidas pela imprensa e a nota da G-SYNC 101. **Confirmado.** Não achei
  evidência de que o **limite automático** do Ultra com G-SYNC funcione em DX12.
  Isso fica em aberto.
- **Vulkan:** sem suporte (G-SYNC 101). **Confirmado**, embora a Blur Busters
  diga “currently” num texto antigo. Jogos Vulkan com Reflex usam
  `NvLowLatencyVk`.
- **OpenGL:** **sem fonte.** A NVIDIA não cita OpenGL em nenhum dos textos lidos.
  Tratar como “não documentado”, não como “sem efeito comprovado”.
- **A ressalva do 551.23** é correta, mas irrelevante para o laboratório (621.40).
  Vale como `driver_min` numa regra futura que dependa de DX12.

## 5. Max Frame Rate × limitador do jogo/Reflex

- **Confirmada pela Blur Busters (jorimt) [F + G-SYNC 101 p. 11]:** limitadores
  no motor ficam “roughly 1/2 to 1 frame lower latency than external limiters
  (RTSS, Nvidia Max Frame Rate)”. O Ultra com G-SYNC usa o mesmo limitador do
  Max Frame Rate, que “typically incurs 1 frame of additional delay over a good
  in-game limiter”.
- **Ressalvas:**
  - As medições da G-SYNC 101 são com RTSS (2017–2019). O V3 do painel
    (441.87+) foi equiparado por raciocínio, sem medição isolada de mesma
    qualidade.
  - A vantagem vale para um **bom** limitador de jogo. Alguns limitadores
    internos têm frametime irregular, e a própria Blur Busters diz que
    limitadores externos estabilizam melhor.
  - Para um limite manual vencer o do Reflex, ele precisa ficar **abaixo** do
    limite automático (jorimt).
- **Não verificado:** os números “~16 ms Reflex vs ~18 ms Max Frame Rate/RTSS vs
  ~24 ms” que o relatório atribui à Blur Busters/Overwatch. Não os encontrei.
  Citar com a fonte exata ou remover.

## 6. Energia “desempenho máximo”

- O guia de latência da NVIDIA **não menciona** Power Management Mode. Não há
  medição séria (Battle(non)sense, Blur Busters, imprensa) de ganho de latência.
  O que existe sobre “planos de energia” trata do Windows, não do driver, e é
  anedótico [F].
- O relatório do driver já diz “ganho é de consistência de clock, não de
  latência média”, o que está correto. A justificativa da regra
  (`regras/base.toml:32-33`) é plausível, mas a evidência é só “sem regressão”.
- **Incoerência:** o relatório recomenda **por perfil de jogo** e
  “condicional no global”, mas a regra é global e incondicional
  (`base.toml:25-36`). Isso gera custo de consumo e calor em qualquer app 3D
  (navegador com aceleração, players) e em notebook. `escopo = "por_jogo"` já
  existe no motor. Não há fato de “notebook/bateria”.

## 7. Anticheat e o caso AMD Anti-Lag+

- **Confirmado:** a Valve avisou em out. 2023 que o Anti-Lag+ gerava VAC ban no
  CS2, porque fazia *detour* de funções do motor. A AMD retirou o recurso, a
  Valve reverteu os bans e passou a checar drivers AMD (PC Gamer, Tom's
  Hardware). A conclusão “o limite é mexer no processo/arquivo do jogo” é bem
  fundamentada.
- **Ressalvas:**
  - “Nenhum ban conhecido por perfil DRS” é ausência de evidência, não garantia.
    Nenhum anticheat publicou política sobre DRS.
  - O override de DLSS (`regras/base.toml:167`: “seguro com anticheat de
    kernel”) faz o **driver carregar uma DLL própria** no processo do jogo. É
    código assinado pela NVIDIA e por isso tolerado, mas não é “não toca no
    processo” no sentido estrito.
  - O caso do Valorant foi o filtro do GeForce Experience/NVIDIA App (vantagem
    visual), não DRS. Ele é relevante para **vibrância digital** e filtros
    (ver omissões).

## 8. A detecção não toca processos

- **Confirmada.** Não há `OpenProcess`, Toolhelp, `EnumProcesses`,
  `ReadProcessMemory`, `EnumProcessModules` nem `QueryFullProcessImageName` em
  `crates/` (busca refeita nesta revisão). O único `OpenProcessToken` é no próprio
  processo (`crates/nvt-service/src/diretorio.rs:67`, `677`) e num teste
  (`crates/nvt-ipc/tests/pipe_local.rs:381-385`). A leitura de PE usa
  `FileMap::open` sobre o **arquivo** (`crates/nvt-detect/src/facts/pe.rs:92`,
  `110`).
- **Ressalva já apontada no estado atual e mantida:** o serviço (LocalSystem)
  mapeia exes e DLLs de anticheat. Isso não é acesso a processo, mas é atividade
  visível para drivers de kernel que monitoram acesso aos próprios arquivos.
  Também pode bloquear a escrita de um patch enquanto o mapeamento existe.
  Convém pular binários de anticheat ou ler só cabeçalhos com handle curto.
- Gravar no DRS **afeta** o jogo no próximo início (o driver lê o perfil). É o
  mecanismo pretendido, não toque no processo.

## 9. Omissões relevantes

1. **Interação Ultra global × modo G-SYNC.** É o fato central que os dois
   relatórios deixaram escapar (item 1). Qualquer mudança de Ultra para On ou
   Off no global **remove o limite automático** do modo `gsync`. Nesse caso
   `sincronismo-gsync-limite` passa a ser necessária, e com o valor certo. As
   duas regras precisam ser decididas juntas.
2. **V-Sync do painel e modo janela.** O guia da NVIDIA diz que o V-Sync ligado
   no painel só vale em tela cheia e manda usar o V-Sync do jogo em janela. Isso
   conflita com a premissa de `sincronismo-gsync` (`base.toml:109`: “o modo
   janela cobre jogos em janela sem bordas”). É preciso verificar no driver
   atual com flip model.
3. **Detecção de Reflex.** Os competitivos citados usam Reflex via NVAPI, sem
   rastro em disco. Sem uma base curada por executável, nenhuma regra
   “com/sem Reflex” vai casar (`deteccao.rs:72-73`, `91`).
4. **Detecção de VRR ativo**, não só do refresh. Os IDs `0x10A879CE` e
   `0x109DB0D3` e `NvAPI_Disp_GetVRRInfo` aparecem no relatório, mas não estão
   ligados a nenhum fato do motor.
5. **Vibrância digital** (NVAPI DVC, não DRS). É o ajuste de painel mais usado
   em CS2 e Valorant e dá vantagem visual. Precisa de decisão explícita
   (provavelmente “não aplicar”), alinhada à posição sobre filtros.
6. **Escalonamento** (“Perform scaling on: GPU/Display”) e resolução esticada.
   São comuns em CS2/Valorant 4:3. Não aparecem nos relatórios. Há relatos
   anedóticos de diferença de latência [F]; falta medição.
7. **Limite global afeta tudo.** `limite_fps` do usuário e o limite do modo
   G-SYNC vão para o perfil **global** (`estado-atual` §1.2). Isso inclui jogos
   com Reflex, em que o limitador do jogo é melhor (item 5). Um limite por jogo,
   ou que respeite jogos com Reflex, ficou de fora.
8. **Ultra + limite automático em DX12** (item 4): sem evidência. Se o
   G-SYNC depender disso em DX12, é preciso medir.
9. **ULMB/ULMB 2 e strobing.** São ajustes de monitor, não de DRS, e
   incompatíveis com G-SYNC. Para competitivo, competem com a escolha
   `gsync` × `sem_sync`. Vale ao menos citar como fora de escopo.

## Fontes

- NVIDIA — [How To Reduce Lag – System Latency Optimization Guide](https://www.nvidia.com/en-us/geforce/guides/system-latency-optimization-guide) (Reflex prevalece sobre Ultra; limite automático com V-Sync + Reflex/Ultra; V-Sync do painel só em tela cheia; nada sobre energia)
- NVIDIA — [Gamescom 2019 Game Ready Driver](https://www.nvidia.com/en-gb/geforce/news/gamescom-2019-game-ready-driver) (DX9/DX11; “in DX12 and Vulkan titles, the game decides”; maior impacto GPU-bound a 60–100 FPS)
- 551.23 — [TechPowerUp](https://www.techpowerup.com/318252/nvidia-releases-geforce-551-23-whql-supports-rtx-4070-ti-super-rtx-video-hdr), [TechSpot](https://www.techspot.com/drivers/driver/file/information/18334/), [DSOGaming](https://www.dsogaming.com/news/nvidia-geforce-551-23-whql-driver-released-optimized-for-tekken-8-suicide-squad-kill-the-justice-league/)
- Blur Busters — [G-SYNC 101: Optimal Settings](https://www.blurbusters.com/gsync/gsync101-input-lag-tests-and-settings/14/), [G-SYNC Ceiling vs. FPS Limit](https://www.blurbusters.com/gsync/gsync101-input-lag-tests-and-settings/5/), [In-game vs. External FPS Limiters](https://www.blurbusters.com/gsync/gsync101-input-lag-tests-and-settings/11/)
- Fórum Blur Busters [F] — [fórmula do limite do Reflex](https://forums.blurbusters.com/viewtopic.php?p=125570), [sem fórmula oficial](https://forums.blurbusters.com/viewtopic.php?p=121997), [Reflex – How It Works](https://forums.blurbusters.com/viewtopic.php?p=67231), [Reflex e LLM](https://forums.blurbusters.com/viewtopic.php?p=67470), [Ultra vs Off fora do GPU-bound (Battle(non)sense)](https://forums.blurbusters.com/viewtopic.php?p=44275), [Cap your fps people](https://forums.blurbusters.com/viewtopic.php?p=44719), [limite automático do Ultra a 165 Hz](https://forums.blurbusters.com/viewtopic.php?p=78369)
- [Wccftech – limitadores testados](https://wccftech.com/how-fps-limiters-impact-gameplay-a-guide/) (heurística `Hz − Hz²/3600`)
- Anticheat — [PC Gamer: Anti-Lag+ e VAC](https://pcgamer.com/valve-warns-counter-strike-2-players-use-amds-anti-lag-feature-get-banned), [Tom's Hardware: CS2 checa drivers AMD](https://www.tomshardware.com/news/counter-strike-2-check-amd-drivers-anti-lag-plus-prevent-bans)
- Código: `crates/nvt-engine/src/deteccao.rs`, `crates/nvt-core/src/{rules,evaluator,capabilities}.rs`, `crates/nvt-ui/src/local.rs`, `crates/nvt-detect/src/facts/pe.rs`, `regras/base.toml`
