# Ajustes de driver NVIDIA para jogos competitivos

Pesquisa de 2026-11-26. Escopo: o que a camada do **driver** (perfil DRS) muda em latência
e frametime em jogos competitivos (CS2, Valorant, Fortnite, Apex, Overwatch 2, R6 Siege,
LoL, CoD etc.). Frame generation (DLSS-FG/MFG, Smooth Motion), overclock e undervolt estão
**fora do escopo**.

Base cruzada: [`nvapi-drs.md`](nvapi-drs.md), [`settings-referencia.json`](settings-referencia.json)
(IDs e valores), [`capacidades-gpu.toml`](capacidades-gpu.toml) e `regras/*.toml` (o que o
app já aplica). IDs DRS em hexadecimal; “catálogo” = já existe em `regras/settings.toml`.

Qualidade da evidência: **[M]** medição com instrumento (LDAT, Reflex Latency Analyzer,
câmera de alta velocidade, PresentMon/FrameView); **[N]** documentação/afirmação da NVIDIA;
**[F]** fórum técnico ou relato sem metodologia publicada; **[L]** laboratório do projeto
(RTX 2060 SUPER, driver 621.40, 240 Hz, PresentMon — ver `regras/base.toml`).

## 1. Tabela-resumo

| # | Ajuste | ID DRS | Classificação | Condição / observação |
|---|---|---|---|---|
| 1 | Reflex do jogo ligado (não é DRS) | — | **recomendado** | Sempre que o jogo tiver Reflex; o app só precisa saber que existe para não empilhar ajustes do driver |
| 2 | Low Latency Mode **Ultra** | `0x0005F543`=2 + `0x10835000`=1 | **condicional** | Jogo **sem** Reflex, DX9/DX11/DX12 (driver ≥ 551.23 para DX12), e **limitado pela GPU** (GPU ~95–99%). Sem efeito com Reflex (Reflex prevalece), sem efeito em Vulkan/OpenGL |
| 3 | Low Latency Mode **On** (= pre-rendered frames 1) | `0x0005F543`=1 (+ `0x10835000`=0) | **condicional** | Jogo sem Reflex e **não** limitado pela GPU (CPU-bound ou FPS travado por limite): fila de 1 frame sem o just-in-time do Ultra |
| 4 | Maximum Pre-Rendered Frames (legado) | `0x007BA09E` | **mito/prejudicial** (mexer à parte) | Substituído pelo Low Latency Mode desde o 436.02; gravar à parte pode conflitar com o estado do LLM. Não gravar |
| 5 | G-SYNC/VRR + V-Sync (driver) + limite abaixo do refresh | `0x1094F157`, `0x1194F158`, `0x10A879CF`, `0x00A879CF`, `0x10835002` | **condicional** | Monitor com VRR ativo e usuário que **não aceita tearing**. Limite = `Hz·3600/(Hz+3600)` (138 @144, 225 @240, 327 @360, 423 @480) ou no mínimo Hz−3 |
| 6 | V-Sync forçado desligado, sem limite (ou limite só para tirar a GPU de 99%) | `0x00A879CF`=`0x08416747` | **condicional** | Usuário prioriza latência mínima e aceita tearing, com FPS bem acima do refresh. É o que o laboratório mediu como melhor no LoL |
| 7 | Max Frame Rate do driver como limitador | `0x10835002` | **condicional** | Só quando o jogo **não** tem limitador interno nem Reflex, ou para manter o FPS dentro da janela VRR. Custa ~½–1 frame frente a limitador do jogo/Reflex |
| 8 | Limite de FPS para tirar a GPU de 99% | `0x10835002` | **condicional** | Jogo sem Reflex e GPU-bound: limitar alguns FPS abaixo do que a GPU sustenta reduz a fila. Com Reflex, desnecessário |
| 9 | Modo de energia “Desempenho máximo” | `0x1057EB71`=1 | **recomendado** (por perfil de jogo) / condicional no global | Desktop. Em notebook na bateria, não. Ganho é de consistência de clock, não de latência média |
| 10 | Shader cache ligado | `0x00198FFF`=1 | **recomendado** | Já é o padrão; nunca desligar |
| 11 | Tamanho do shader cache maior (10 GB/100 GB/Ilimitado) | `0x00AC8497` | **condicional** | Disco com espaço; ajuda contra recompilação quando o cache padrão enche (ex.: Overwatch). Evidência [F] |
| 12 | Threaded Optimization forçado On/Off | `0x20C1221E` | **mito** (deixar Auto) | Sem medição séria de ganho; forçar Off/On pode piorar em títulos específicos |
| 13 | Texture Filtering Quality “High performance” | `0x00CE2691`=`0x14` | **mito** (ganho desprezível) / condicional | Em GPUs modernas o custo de filtragem é irrisório; só faria sentido em GPU muito fraca. Piora a imagem (shimmering) |
| 14 | Trilinear optimization / Negative LOD clamp | `0x002ECAF2`, `0x0019BB68` | **mito** (não mexer) | Sem efeito em latência; clamp de LOD negativo pode piorar DLSS/TAA |
| 15 | Resizable BAR forçado por perfil | `0x000F00BA` (+ `0x000F00BB`, `0x000F00FF`) | **condicional** (não aplicar automaticamente) | Só Ampere/Ada/Blackwell com ReBAR **ativo** no sistema, e só com medição por jogo: há ganhos e regressões |
| 16 | Triple buffering do driver | `0x20FDD1F9` | **mito/prejudicial** (Off) | Só afeta OpenGL; com V-Sync adiciona fila. Manter Off |
| 17 | Fast Sync | `0x00A879CF`=`0x18888888` | **condicional** | Sem VRR e FPS **muitas vezes** acima do refresh; abaixo disso gera stutter. Com VRR, não usar |
| 18 | Adaptive V-Sync / meio refresh | `0x005A375C`, `0x00A879CF` | **prejudicial** | Não serve a competitivo |
| 19 | DLSS Super Resolution em competitivo | `0x10E41E01`, `0x00634291`, `0x10E41DF3` | **condicional** | Só ajuda se o jogo é **GPU-bound**; em CPU-bound adiciona custo fixo por frame. Turing/Ampere: preset CNN (E) já é regra do app |
| 20 | Preferred refresh rate “Highest available” | `0x0064B541`=1 | **condicional** | Só para jogos (geralmente antigos, tela cheia exclusiva) que abrem em 60 Hz; inofensivo nos demais |
| 21 | Limite de FPS em segundo plano | `0x10835005` | **condicional** (conforto) | Reduz calor/consumo com o jogo minimizado; nenhum efeito na partida |
| 22 | Indicadores/overlays de depuração Off | `0x1095F16F`, `0x00666665`, `0x00676635` | **recomendado** | Já é regra base |
| 23 | Settings “Experimental - …” e flags internas | vários `0x0045AB1F` etc. | **prejudicial** | Sem documentação; não gravar |

## 2. Detalhes por item

### 2.1 Reflex (do jogo) e a relação com o driver

- **Efeito:** Reflex controla, de dentro do motor, quando a CPU começa a simular o frame,
  esvaziando a fila de render quando a GPU é o gargalo. NVIDIA afirma redução “acima” das
  técnicas só de driver (Ultra Low Latency) **[N]**. TechPowerUp com LDAT v2 confirmou
  ganho em cenários GPU-bound e nenhum fora deles; “On + Boost” não fez diferença notável no
  Overwatch **[M]**. TechSpot: ganho aparece principalmente em qualidade alta/ultra, GPU-bound **[M]**.
- **Interação:** com Reflex ligado, o driver ignora o Ultra Low Latency (“Reflex overrides
  Ultra”) **[N]**. O limitador automático do Reflex (com G-SYNC + V-Sync) mediu como o do
  jogo, sem frame extra **[M]** (Blur Busters, Overwatch: ~16 ms Reflex vs ~18 ms Max Frame
  Rate/RTSS vs ~24 ms sem limitador).
- **Dependências:** suporte no jogo (CS2, Valorant, Fortnite, Apex, Overwatch 2, R6 Siege,
  CoD têm; LoL não tem — o laboratório usou LoL justamente por isso).
- **Anticheat:** é recurso do próprio jogo; sem risco.
- **Para o app:** saber se o jogo tem Reflex muda a recomendação de LLM e de limitador do
  driver. Não há setting DRS para ligar o Reflex.
- **Reflex 2 / Frame Warp:** anunciado na CES 2025; até a última cobertura encontrada (jul.
  2026) não foi lançado em nenhum jogo nem tem chave no driver **[F]**. Ignorar por ora.

### 2.2 Low Latency Mode (Off / On / Ultra) e pre-rendered frames

- **Mecânica [N]:** “On” limita a fila a 1 frame (equivale a `Max_Prerendered_Frames = 1` dos
  drivers antigos). “Ultra” submete o frame “just in time” para a GPU e, com G-SYNC + V-Sync,
  também limita o FPS automaticamente abaixo do refresh (mesmo deslocamento do Reflex).
  Introduzido no driver 436.02 (gamescom 2019) para DX9/DX11; DX12 só a partir do 551.23
  (release notes da série 551); **sem efeito em Vulkan** e OpenGL.
- **Quando vale [N][M]:** máximo efeito com o jogo **GPU-bound** entre ~60 e 100 FPS (texto
  da NVIDIA). Battle(non)sense mediu que, **fora** do GPU-bound (FPS limitado, GPU < 95%),
  “Off” chegou a vencer “Ultra” em Overwatch, BF V e PUBG **[M via F]**; o motivo provável é o
  just-in-time submeter tarde demais para a janela do refresh **[F]**. Laboratório do projeto:
  LoL (DX11, sem Reflex) com Ultra teve entrada→fóton média 1,54 ms contra 1,71 ms **[L]**.
- **Classificação:** Ultra é **condicional** (jogo sem Reflex, API suportada, GPU-bound); On
  é a opção conservadora para jogo sem Reflex e CPU-bound. Com Reflex: irrelevante.
- **Riscos:** Ultra em jogo CPU-bound pode aumentar ligeiramente a latência média e o
  frametime p99 (stutter de “frame atrasado”). Nota: a regra `baseline-baixa-latencia-ultra`
  aplica Ultra no **global**; a evidência externa sugere preferir On (ou Off) em jogos
  limitados pela CPU/por limite de FPS — vale medir por jogo antes de mudar.
- **IDs:** `0x0005F543` (CPL State: 0 Off, 1 On, 2 Ultra; **oculta** da NVAPI pública) e
  `0x10835000` (FRL Low Latency: 1 só no Ultra). Legado: `0x007BA09E` Maximum Pre-Rendered
  Frames (0 = aplicação, 1–8). O painel atual escreve os dois primeiros; gravar o legado
  separado é redundante e pode divergir do que o painel mostra → **não gravar**.
- **Anticheat:** setting do driver, sem injeção no processo; nenhum caso de ban conhecido.

### 2.3 Limite de FPS do driver (Max Frame Rate)

- **ID:** `0x10835002` Frame Rate Limiter V3 (catálogo `limite_fps`, 20–1000 FPS, driver ≥
  441.87). Fundo: `0x10835005` (Background Application). O antigo `0x10834FEE` não usar.
- **Efeito [M]:** limitador de CPU no driver, comparável ao RTSS. Blur Busters: ~½ a 1 frame
  a mais que um bom limitador interno do jogo ou o do Reflex. O limitador antigo do Inspector
  media 2+ frames; o V3 do painel é melhor.
- **Quando limitar ajuda [M via F]:** Battle(non)sense mostrou que manter a GPU abaixo de
  ~95% reduz bastante a latência (Overwatch: 46 ms limitado a 60 FPS vs 72 ms GPU a 99% com
  81 FPS). O limite em si é neutro; o ganho vem de esvaziar a fila da GPU. Em jogo CPU-bound,
  limitar só adiciona latência. Com Reflex, o Reflex já faz isso sozinho.
- **Ordem de preferência:** Reflex > limitador do jogo (CS2 `fps_max`, Valorant, Fortnite,
  Apex `+fps_max`, OW2, R6, CoD têm) > Max Frame Rate do driver > RTSS. **Não empilhar**
  limitadores.
- **Classificação:** condicional. Para o app, o uso seguro e automatizável é o teto de VRR
  (2.4). Limite “para tirar a GPU de 99%” depende de FPS medido por jogo.

### 2.4 G-SYNC/VRR + V-Sync + limite abaixo do refresh

- **Receita Blur Busters (G-SYNC 101) [M]:** G-SYNC ligado + **V-Sync ligado no painel**
  (desligado no jogo) + limite de FPS **pelo menos 3 FPS abaixo** do refresh (2 FPS já evita
  o teto no teste deles; 3 é a margem segura). Sem o limite, ao encostar no refresh o G-SYNC
  volta a se comportar como V-Sync (fila e latência) ou rasga (V-Sync off). O V-Sync do painel
  serve para segurar oscilações de frametime perto do teto; dentro da janela VRR ele não
  adiciona latência.
- **Quanto abaixo:** com Reflex ou Ultra + G-SYNC + V-Sync, o próprio driver limita a
  ~59 @60, ~116 @120, ~138 @144, ~224 @240 **[N/M]**; a fórmula atribuída (fórum Blur Busters,
  não confirmada pela NVIDIA) é `Hz·3600/(Hz+3600)` ⇒ 225 @240, 327 @360, 423 @480 **[F]**.
  A margem cresce com o refresh porque a variação de frametime em ms pesa mais. Recomenda-se
  ao app usar `min(Hz − 3, floor(Hz·3600/(Hz+3600)))` em vez de “−3” fixo, que é apertado
  demais a 360/480 Hz. A regra atual (`refresh_menos = 3`) fica correta só até ~144 Hz.
- **Contra-evidência para competitivo [L][M]:** no laboratório (LoL, 240 Hz), G-SYNC + V-Sync
  + limite levou a 224 FPS contra 391 sem sincronismo e piorou entrada→fóton p99 (5,27 vs
  3,58 ms). Blur Busters (“G-SYNC vs V-SYNC OFF”) também mostra que, com FPS muito acima do
  refresh, V-Sync off sem limite entrega frames mais recentes (menor latência), ao custo de
  tearing. Ou seja: G-SYNC é a melhor combinação **sem tearing**, não a de menor latência
  absoluta quando a máquina passa muito do refresh.
- **Classificação:** condicional à preferência do usuário (tearing aceitável ou não) e a
  VRR ativo. A escolha já existe no app (`sincronismo = gsync | sem_sync | vsync`).
- **Dependências:** monitor G-SYNC/G-SYNC Compatible com VRR **ativo** (Windows e painel),
  refresh conhecido; jogo em tela cheia ou janela sem bordas com flip model (modo
  `tela_cheia_e_janela` cobre a janela). VRR não funciona com o FPS abaixo da faixa mínima
  do monitor (LFC cobre parte).
- **IDs:** catálogo (`gsync_global` `0x1094F157`, `gsync_app_mode` `0x1194F158`,
  `gsync_app_state` `0x10A879CF`, `vsync` `0x00A879CF`, `limite_fps` `0x10835002`). Úteis para
  diagnóstico: `0x10A879CE` VRR Control, `0x109DB0D3` G-SYNC Compatibility, `0x00A879FF`
  OS VRR override (não gravar).

### 2.5 Modo de V-Sync, triple buffering e Fast Sync

- **V-Sync do driver sem VRR:** double buffer a 60–144 Hz acrescenta até vários frames de
  latência e cai para metade do refresh ao perder a janela **[M]**. Em competitivo, só para
  quem não tolera tearing e não tem VRR (regra `sincronismo-vsync` já existe).
- **Fast Sync (`0x18888888`) [M]:** “triple buffering verdadeiro”; só tem latência menor que
  V-Sync quando o FPS está **muito** acima do refresh; abaixo disso repete frames e
  engasga. Com VRR, não usar. Condicional.
- **Triple buffering (`0x20FDD1F9`) [F]:** a opção do painel só vale para OpenGL; com V-Sync
  aumenta a fila. Manter Off (padrão). Indicador `0x00676635` já é zerado pela regra base.
- **Adaptive (`0x005A375C` = `0x99941284`) e 1/2–1/4 refresh:** prejudiciais em competitivo.

### 2.6 Modo de gerenciamento de energia

- **ID:** `0x1057EB71` (0 Adaptativo, 1 Desempenho máximo, 5 Otimizado/Normal).
- **Efeito:** impede que a GPU baixe o clock base em cenas leves (menus, mapas vazios,
  jogos que usam 40–60% da GPU, típico de CS2/Valorant/LoL). Não há medição isolada séria de
  latência **[F]**; o efeito plausível é frametime mais estável quando a carga oscila. Blur
  Busters descreve o efeito como manter a GPU pelo menos no clock base de boost **[F]**.
  Laboratório: aplicado sem regressão **[L]**.
- **Riscos:** consumo e calor em idle se aplicado no **global** (a GPU não desce de P-state
  com qualquer app 3D aberto, inclusive navegador com aceleração). Melhor por perfil de jogo;
  em notebook na bateria, não aplicar.
- **Classificação:** recomendado por jogo; condicional no global (desktop).
- **Fora do driver (não é DRS):** plano de energia do Windows e core parking também afetam
  frametime **[F]**.

### 2.7 Threaded Optimization

- **ID:** `0x20C1221E` (0 Auto, 1 On, 2 Off; grupo “System”). Os IDs `0x207A8C92` /
  `0x202DC432` são flags internas do OpenGL — não mexer.
- **Efeito:** move trabalho do driver para uma thread auxiliar. A documentação da NVIDIA
  (driver OpenGL) diz que beneficia apps CPU-intensivos e pode **piorar** apps com muitas
  chamadas síncronas **[N]**. Não há benchmark sério isolando o efeito em latência no DX11
  **[F]**; guias que mandam “Off” não têm medição.
- **Classificação:** mito (forçar). Deixar Auto, que é o perfil que a NVIDIA ajusta por jogo.

### 2.8 Shader cache e tamanho

- **IDs:** `0x00198FFF` (ligado, padrão; catálogo) e `0x00AC8497` Cache Size (0 Off, 128 MB…
  `0x00002800` 10 GB, `0x00019000` 100 GB, `0xFFFFFFFF` Ilimitado). `0x00EAD189` pré-compilação
  e flags `0x00F4889B` etc. não mexer.
- **Efeito:** não muda latência; evita engasgos de compilação. O limite padrão antigo (1 GB)
  era pequeno para Overwatch; com 10 GB o cache persistiu entre sessões **[F]**. Relatos são
  mistos para outros jogos **[F]**. O driver apaga o cache ao atualizar **[N]**, então o
  primeiro jogo após update engasga de qualquer forma.
- **Riscos:** só espaço em disco. **Classificação:** cache ligado = recomendado; tamanho
  10 GB+ = condicional a espaço livre no disco do `%LOCALAPPDATA%\NVIDIA\DXCache`.

### 2.9 Filtragem de textura

- **IDs:** `0x00CE2691` Quality (`0xFFFFFFF6` High quality, 0 Quality, `0xA` Performance,
  `0x14` High performance), `0x00CE2692` substituição, `0x002ECAF2` trilinear,
  `0x0019BB68` LOD negativo, `0x00738E8F` LOD bias DX.
- **Efeito:** nenhum benchmark moderno mostra ganho mensurável de FPS; o custo de filtragem
  em GPUs RTX é irrisório **[F]**. “High performance” pode causar cintilação de texturas
  (pior para enxergar alvos). Clamp de LOD negativo atrapalha DLSS/TAA, que dependem de LOD
  negativo.
- **Classificação:** mito. A regra `baseline-filtragem-pela-aplicacao` (AF/AA da aplicação,
  otimizações off) é coerente e fica como está.

### 2.10 Resizable BAR forçado por perfil

- **IDs (ocultos da NVAPI pública):** `0x000F00BA` Enable, `0x000F00BB` Options,
  `0x000F00FF` Size Limit; `0x00E942FC` exclusão de CPU Intel.
- **Efeito [M]:** TechSpot e TechPowerUp (22 jogos, RTX 30) acharam efeito médio próximo de
  zero, com **regressões** em alguns títulos; por isso a NVIDIA habilita por jogo (lista
  branca) **[N]**. Ganhos grandes (ex.: Dead Space Remake) são relatos isolados **[F]**.
  Nenhuma medição séria em CS2/Valorant/LoL.
- **Dependências:** Ampere ou mais nova (Turing não tem suporte oficial), BIOS com
  Above 4G/ReBAR, VBIOS e driver ≥ 465.89; ReBAR **ativo** (BAR1 ≈ VRAM via NVML, já previsto
  em `capacidades-gpu.toml`).
- **Classificação:** condicional — nunca automático; só “experimental” com medição A/B por
  jogo no próprio PC. Sem ReBAR ativo, a setting não tem efeito.

### 2.11 DLSS Super Resolution em competitivo

- **Efeito:** reduz a resolução interna; só reduz latência se o jogo for **GPU-bound**.
  Em CPU-bound (caso comum em CS2/Valorant/LoL com GPUs médias), o FPS não sobe e o custo
  fixo do DLSS por frame (fração de ms a ~1–2 ms conforme GPU e resolução, tabela do DLSS
  Programming Guide **[N]**) soma latência **[F]**. Em jogos pesados (Fortnite, CoD, Apex em
  qualidade alta) pode ajudar.
- **Preset:** Turing/Ampere sem FP8 pagam caro pelo modelo transformer; o preset E (CNN)
  rendeu +16,8% FPS e −15,1% entrada→fóton no WARDOGS **[L]** (regras `dlss-preset-e-*`).
  Ada/Blackwell: deixar o do jogo.
- **Anticheat:** o override pelo perfil DRS (`0x10E41E01` / `0x00634291` / `0x10E41DF3`) não
  toca arquivos do jogo; trocar a DLL na pasta do jogo, sim, e pode acionar anticheat.
- **Classificação:** condicional (GPU-bound e jogo com DLSS).

### 2.12 Outros

- **Preferred refresh rate (`0x0064B541`):** “Highest available” só importa para jogos em
  tela cheia exclusiva que pedem 60 Hz. Condicional, risco baixo.
- **Indicadores:** já desligados pela regra base. Útil ligar só para diagnóstico:
  `0x1095F16F` (VRR), `0x10029538` (G-SYNC), `0x00280880` (flip/V-Sync).
- **Settings “Experimental - …” (`0x0045AB1F`, `0x006EA977`, `0x0012FA6F` …), CUDA,
  Powerthrottle (`0x00AE785C`), Platform Boost (`0x10834FFE`):** sem documentação pública de
  efeito em latência. Não gravar.
- **Silk Smoothness e Smooth Motion:** suavização/geração de frames; fora do escopo e
  contrários a competitivo.

## 3. Ajustes específicos por arquitetura

| Arquitetura | O que muda |
|---|---|
| Turing (RTX 20, GTX 16) | Sem ReBAR oficial (não forçar). DLSS: preset CNN E (regra existente). GTX 16 sem DLSS. Reflex e LLM funcionam igual |
| Ampere (RTX 30) | ReBAR oficial (whitelist do driver); preset E experimental (regra existente) |
| Ada (RTX 40) | Nada específico de driver para competitivo além do comum; FG fora de escopo |
| Blackwell (RTX 50) | Reflex 2/Frame Warp anunciado mas não lançado; flip metering é para MFG (fora de escopo). Nada específico de DRS |

Não foi encontrada nenhuma setting de latência que se comporte de forma diferente por
arquitetura; as diferenças são de capacidade (ReBAR, custo do DLSS).

## 4. Anticheat

- Settings DRS são lidas pelo próprio driver, no processo do jogo, por mecanismo da NVIDIA;
  não injetam código nem alteram arquivos do jogo. Nenhum caso público de ban por perfil do
  driver NVIDIA foi encontrado.
- O caso conhecido de ban foi o **AMD Anti-Lag+** no CS2 (out. 2023), que fazia *detour* de
  funções da DLL do motor; a Valve classificou como adulteração e depois reverteu os bans.
  Mostra que o limite é **mexer no processo/arquivos do jogo**, não configurar o driver.
- Riot já desaconselhou filtros do driver que dão vantagem visual (filtro do GeForce
  Experience que removia o flash no Valorant). Ajustes de latência/sincronismo não se
  enquadram, mas o app deve evitar qualquer setting que altere a imagem para ganhar vantagem.
- Não há declaração oficial de Riot/Valve/FACEIT sobre perfis DRS; risco classificado como
  baixo, não nulo.

## 5. O que o app precisa saber da máquina e do jogo

| Dado | Para quê | Como obter (indicação) |
|---|---|---|
| Taxa de atualização do monitor onde o jogo roda | Limite VRR (`Hz·3600/(Hz+3600)`), Fast Sync | `EnumDisplaySettings`/DXGI `GetDisplayModeList` |
| VRR suportado **e ativo** (G-SYNC / G-SYNC Compatible) | Liberar receita 2.4 | NVAPI (`NvAPI_Disp_GetVRRInfo`), DRS `0x10A879CE`, configuração “Variable refresh rate” do Windows |
| Preferência de tearing do usuário | Escolher sem_sync × gsync × vsync | Pergunta ao usuário (já existe) |
| Jogo tem Reflex | Dispensar LLM e limitador do driver | Base de dados por executável (o driver não informa) |
| API do jogo (DX9/11/12/Vulkan/OpenGL) | LLM não age em Vulkan/OpenGL; DX12 só ≥ 551.23; triple buffering só OpenGL | Base por jogo ou módulos carregados no processo |
| Versão do driver | LLM em DX12 (≥ 551.23), FRL V3 (≥ 441.87) | `NvAPI_SYS_GetDriverAndBranchVersion` |
| GPU-bound × CPU-bound (FPS e uso de GPU) | Ultra vs On; limite para sair de 99%; DLSS | Medição (PresentMon/NVML) por jogo |
| Jogo tem DLSS | Regras de preset | Base por jogo |
| Arquitetura e implementação | DLSS preset, ReBAR | `NvAPI_GPU_GetArchInfo` |
| ReBAR ativo | Habilitar experimento de ReBAR | NVML BAR1 ≈ VRAM |
| Desktop × notebook, bateria | Energia máxima | `GetSystemPowerStatus` |
| Espaço livre no disco do cache | Tamanho do shader cache | `GetDiskFreeSpaceEx` em `%LOCALAPPDATA%` |

## 6. Fontes

NVIDIA:
- [How To Reduce Lag – A Guide To Better System Latency](https://www.nvidia.com/en-us/geforce/guides/system-latency-optimization-guide)
- [Introducing NVIDIA Reflex](https://www.nvidia.com/en-gb/geforce/news/reflex-low-latency-platform)
- [Gamescom 2019 Game Ready Driver (Low Latency Mode)](https://www.nvidia.com/en-gb/geforce/news/gamescom-2019-game-ready-driver)
- [Reflex 2 com Frame Warp](https://www.nvidia.com/en-us/geforce/news/reflex-2-even-lower-latency-gameplay-with-frame-warp)
- [Manage 3D Settings (referência do Painel de Controle)](https://www.nvidia.com/content/Control-Panel-Help/vLatest/en-us/mergedProjects/nv3d/Manage_3D_Settings_(reference).htm)
- [GeForce RTX 30 Resizable BAR](https://www.nvidia.com/en-us/geforce/news/geforce-rtx-30-series-resizable-bar-support/)
- [DLSS Programming Guide (tempos de execução por GPU)](https://github.com/NVIDIA/DLSS/blob/main/doc/DLSS_Programming_Guide_Release.pdf)
- Release notes 551.23/551.31 (Ultra Low Latency em DX12), via [TechSpot drivers](https://www.techspot.com/drivers/driver/file/information/18334/)
- Max Frame Rate no 441.87: [PCPer](https://pcper.com/2020/01/nvidias-latest-game-ready-driver-adds-max-frame-rate-option/), [TheFPSReview (teste)](https://www.thefpsreview.com/2020/01/07/tested-nvidia-geforce-drivers-limiting-maximum-frame-rate/)

Blur Busters (G-SYNC 101 e fórum):
- [Optimal G-SYNC Settings & Conclusion](https://www.blurbusters.com/gsync/gsync101-input-lag-tests-and-settings/14/)
- [G-SYNC Ceiling vs. FPS Limit](https://www.blurbusters.com/gsync/gsync101-input-lag-tests-and-settings/5/)
- [G-SYNC Ceiling vs. V-SYNC](https://www.blurbusters.com/gsync/gsync101-input-lag-tests-and-settings/4/)
- [G-SYNC vs. V-SYNC w/ FPS Limit](https://www.blurbusters.com/gsync/gsync101-input-lag-tests-and-settings/7/)
- [G-SYNC vs. Fast Sync](https://www.blurbusters.com/gsync/gsync101-input-lag-tests-and-settings/8/)
- [G-SYNC vs. V-SYNC OFF](https://www.blurbusters.com/gsync/gsync101-input-lag-tests-and-settings/9/)
- [In-game vs. External FPS Limiters](https://www.blurbusters.com/gsync/gsync101-input-lag-tests-and-settings/11/)
- [Closing FAQ](https://www.blurbusters.com/gsync/gsync101-input-lag-tests-and-settings/15/)
- [Fórum: fórmula do limite do Reflex](https://forums.blurbusters.com/viewtopic.php?p=125570) · [Reflex Low Latency – How It Works](https://forums.blurbusters.com/viewtopic.php?p=67231) · [Low Latency Mode On ou Off](https://forums.blurbusters.com/viewtopic.php?p=63370) · [Reflex e LLM](https://forums.blurbusters.com/viewtopic.php?p=67470) · [Triple buffering](https://forums.blurbusters.com/viewtopic.php?p=36595)

Battle(non)sense (via discussões que citam os vídeos):
- [“Cap your fps people” (Blur Busters)](https://forums.blurbusters.com/viewtopic.php?p=44278)
- [Input lag sobe com GPU a 99% (Beyond3D)](https://forum.beyond3d.com/threads/input-lag-increases-when-gpu-utilization-reaches-99-battlenonsense-findings.61372)
- [Ultra vs Off fora do GPU-bound](https://forums.blurbusters.com/viewtopic.php?p=44275)

Medições de imprensa técnica:
- [TechPowerUp – NVIDIA Reflex testado com LDAT v2](https://techpowerup.com/review/nvidia-reflex-review-test-ldat-v2/3.html)
- [TechSpot – NVIDIA Reflex](https://www.techspot.com/article/2123-nvidia-reflex-rtx/)
- [igor'sLAB – Reflex/Boost vs limitador, GPU vs CPU limit](https://igorslab.de/en/nvidia-reflex-and-boost-vs-frame-limiter-windowed-mode-vs-full-screen-practice/3)
- [TechSpot – NVIDIA Resizable BAR](https://www.techspot.com/review/2234-nvidia-resizable-bar/)
- [TechPowerUp – Resizable BAR, 22 jogos](https://techpowerup.com/review/nvidia-pci-express-resizable-bar-performance-test/27.html)
- [Wccftech – limitadores testados (Reflex, RTSS, Special K)](https://wccftech.com/how-fps-limiters-impact-gameplay-a-guide/)

Shader cache, Threaded Optimization, anticheat:
- [NVIDIA dev forum – cache de 1 GB pequeno para Overwatch](https://forums.developer.nvidia.com/t/455-23-04-driver-produces-very-large-shader-caches-results-in-constant-cache-invalidation/156087/9)
- [Valve/steam-for-linux #11392 – cache 10 GB no Overwatch 2](https://github.com/ValveSoftware/steam-for-linux/issues/11392)
- [Phoronix – Threaded OpenGL optimizations](https://www.phoronix.com/review/nvidia_threaded_opts)
- [PC Gamer – Valve: Anti-Lag+ no CS2 gera VAC ban](https://pcgamer.com/valve-warns-counter-strike-2-players-use-amds-anti-lag-feature-get-banned)
- [Tom's Hardware – CS2 passa a checar drivers AMD](https://www.tomshardware.com/news/counter-strike-2-check-amd-drivers-anti-lag-plus-prevent-bans)
- [Filtro do GeForce removendo flash no Valorant](https://gameriv.com/nvidia-geforce-feature-unintentionally-removes-the-flash-effect-from-valorant/)
- [Hardware Busters – Reflex 2 ainda não lançado (jul. 2026)](https://hwbusters.com/news/nvidia-reflex-2-is-18-months-old-and-still-hasnt-shipped/)

## 7. Lacunas

- Não foram encontradas medições de Digital Foundry, Hardware Unboxed ou CapFrameX que
  isolem settings de driver em competitivo; os números de Battle(non)sense vieram de
  discussões que citam os vídeos, não dos vídeos.
- A fórmula `Hz·3600/(Hz+3600)` é de fórum (bate com 138 @144, difere em 1 FPS a 240 Hz).
- Energia, Threaded Optimization, tamanho de cache e filtragem não têm medição séria de
  latência; as classificações se baseiam em mecanismo + ausência de evidência.
- Ultra × On em jogos CPU-bound (CS2, Valorant) merece medição no laboratório do projeto
  antes de mudar a regra global.
