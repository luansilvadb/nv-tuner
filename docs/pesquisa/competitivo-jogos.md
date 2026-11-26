# Arquivos de configuração de jogos competitivos: onde ficam, o que mexer e o que é seguro

Pesquisa feita em 2026-11-26 para orientar a próxima etapa do nv-tuner: ajustar arquivos de **configuração** de jogos competitivos para melhorar latência e frametime. O app nunca altera executáveis nem arquivos do jogo e nunca injeta código. Toda mudança precisa ser reversível.

Leia junto com [`competitivo-anticheat.md`](competitivo-anticheat.md). As regras 7, 8, 9, 12 e 13 de lá se aplicam a tudo o que está aqui:

- só arquivos **do usuário**, nada dentro da pasta de instalação;
- só com o jogo (e, quando for o caso, o launcher) **fechado**;
- backup antes e restauração depois;
- só valores que o **menu do próprio jogo** oferece;
- nada que mude o que o jogador enxerga de forma competitiva (fumaça, folhagem, sombras escondidas por `.ini`, LOD, FOV fora do menu, filtros);
- nunca marcar arquivo como somente leitura para "travar" o valor.

Legenda de confiabilidade, a mesma do documento de anticheat:

- **[O]**: oficial (desenvolvedora, publisher, anticheat, NVIDIA, Microsoft);
- **[M]**: medição publicada com método (LDAT, PresentMon, câmera de alta velocidade);
- **[C]**: comunidade, guia ou imprensa. Serve como indício, não como regra.

Os números entre colchetes (`[12]`) remetem à seção **Fontes**, no fim.

---

## 1. Perguntas transversais

### 1.1 O limite de FPS deve ficar no jogo ou no driver?

**No jogo**, ou nenhum limite manual quando houver Reflex. O driver só entra como plano B.

- Blur Busters mediu (G-SYNC 101) que o limitador **do jogo** tem menos latência que limitadores externos. O RTSS acrescenta **até 1 quadro**. O limitador antigo do driver, via Profile Inspector, acrescentava **2 quadros ou mais**. O "Max Frame Rate" do Painel NVIDIA (desde o driver 441.87) é um limitador de CPU **comparável ao RTSS**: melhor que o antigo, pior que o do jogo [1][2] [M].
- A NVIDIA diz que, com Reflex ligado, rodar **sem limite** tem latência um pouco menor que limitar abaixo da taxa de atualização. Com G-SYNC, a recomendação oficial é V-Sync ligado no driver mais Reflex (ou Ultra Low Latency). Nessa combinação o próprio Reflex limita o FPS automaticamente um pouco abaixo da taxa de atualização [3] [O]. LDAT mostrou que o limitador automático do Reflex não acrescenta atraso em relação ao limitador do jogo [6][7] [M].
- Se o jogo não tiver Reflex nem limitador próprio, o fallback é o Max Frame Rate do driver ou o Ultra Low Latency Mode. O Reflex sobrepõe o ULLM quando os dois estão ligados [3] [O].
- **Não empilhe limitadores** (jogo, driver e RTSS ao mesmo tempo). Eles brigam entre si e pioram o frame pacing [C].

**Regra para o app:** se o jogo tem Reflex, ligar Reflex e deixar o limite de FPS do driver **desligado** para esse perfil. Se o usuário quiser um teto (para estabilidade de frametime ou para ficar dentro da janela do VRR), aplicar o teto **no arquivo de config do jogo**, com o valor que o menu aceita. O Max Frame Rate do driver só serve para jogos sem limitador próprio.

### 1.2 Tela cheia exclusiva ou janela sem borda?

- A NVIDIA recomenda **tela cheia exclusiva** sempre que possível, porque ela contorna o compositor do Windows. Segundo a NVIDIA, o modo sem borda "melhorou um pouco", mas os testes dela ainda favorecem tela cheia [3] [O].
- Em DX12/Vulkan com flip model e "Otimizações para janelas" do Windows 11, a diferença medida pela comunidade em CS2 e Valorant é pequena ou nula [11][12] [C].
- **Para o app:** preferir o modo "Tela cheia" do menu do jogo. Não forçar o modo janela por linha de comando. Se o usuário escolheu janela sem borda (por usar vários monitores, por exemplo), respeitar.

### 1.3 Opções de inicialização da Steam (`localconfig.vdf`)

- Ficam em `Steam\userdata\<ID>\config\localconfig.vdf` (formato KeyValues/VDF de texto), na chave `UserLocalConfigStore > Software > Valve > Steam > apps > <AppID> > LaunchOptions`. O arquivo é **por conta Steam**.
- O cliente Steam **sobrescreve o arquivo** se ele for editado com a Steam aberta. Ferramentas que editam esse arquivo exigem a Steam fechada [5] [C]. A sincronização na nuvem pode reaplicar valores [C].
- Não há API pública da Valve para gravar opções de inicialização. Editar o VDF é o único caminho, sempre com **Steam fechada** e backup.
- Nunca adicionar `-allow_third_party_software` (regra 9) nem opções que desliguem proteções.

### 1.4 Os três ajustes de maior impacto, em comum entre os jogos

1. **NVIDIA Reflex ligado** (On, ou On + Boost quando o gargalo é a CPU). Onde não existe, usar o equivalente do jogo ("Reduce Buffering" no Overwatch, por exemplo). Reduz a fila de renderização, que é a maior fonte de latência quando a GPU está no limite [3][6] [O/M].
2. **Limite de FPS no próprio jogo**, ou nenhum limite com Reflex. Com G-SYNC/VRR: V-Sync ligado no driver mais Reflex, que limita automaticamente. Também é o principal ajuste de **frametime**: um teto estável evita oscilação de uso de CPU e GPU [1][3] [M/O].
3. **Tela cheia, com V-Sync do jogo desligado** (o V-Sync fica no driver só quando há G-SYNC). V-Sync sem VRR acrescenta quadros de fila [3] [O].

API gráfica (DX11, DX12 ou Vulkan) e "modo desempenho" vêm em seguida, mas variam muito por jogo e por hardware, e não existe regra universal.

---

## 2. Counter-Strike 2

- **Anticheat:** VAC (modo usuário) e Trusted Mode [O]. Em plataformas terceirizadas, FACEIT AC ou ESEA (kernel).
- **Arquivos:**
  - `Steam\userdata\<SteamID3>\730\local\cfg\cs2_video.txt` (KeyValues de texto, vídeo);
  - `cs2_machine_convars.vcfg` (convars da máquina, inclusive de vídeo);
  - `cs2_user_convars_0_slot0.vcfg` (convars do usuário).
  
  Todos ficam no **diretório da conta Steam**, não na pasta do jogo [8][9] [C]. Há relato de que versões de 2024 em diante gravam os `.vcfg` em `730\remote\cfg`, sincronizado pela Steam Cloud [10] [C]. O app precisa procurar nas duas pastas. `autoexec.cfg` fica na pasta de instalação (`game\csgo\cfg`).
- **Opções de inicialização:** `localconfig.vdf`, AppID 730.
- **Settings de latência e frametime:**
  - **NVIDIA Reflex:** "Ativado" ou "Ativado + Boost" [4] [O].
  - **Limite de FPS:** `fps_max`. 0 com Reflex, ou um teto estável que a máquina mantenha. Usar o campo do menu ou o convar.
  - **Modo de exibição:** tela cheia.
  - **V-Sync** desligado no jogo.
  - **Boost de contraste do jogador:** está no menu e **não mexer** (muda a visibilidade dos jogadores, ou seja, a imagem competitiva).
  - O CS2 só roda em DX11 no Windows (Vulkan só por opção de inicialização, com suporte instável).
- **Segurança da edição:** o VAC mira em executáveis e DLLs do jogo [O]. Arquivos de config no `userdata` não são verificados por integridade. A FACEIT lista "config changes" entre o que é permitido [O, ver documento de anticheat]. **Seguro** dentro das regras do projeto.
- **Armadilhas:**
  - a Steam Cloud pode restaurar a versão da nuvem (aparece o conflito "local × nuvem");
  - o jogo regrava `cs2_video.txt` e os `.vcfg` ao fechar e ao mudar as opções no menu;
  - um repositório comunitário relata que algumas chaves de `cs2_video.txt` deixaram de ter efeito [8] [C].
  
  Editar só com CS2 e Steam fechados e conferir as duas pastas (`local` e `remote`).
- **Veredito:** ✅ seguro. Ganho **moderado**: Reflex e `fps_max` já estão no menu, então o valor do app está em aplicar e manter um perfil consistente.

## 3. Valorant

- **Anticheat:** Riot Vanguard (kernel, desde o boot) [O].
- **Arquivos:** `%LOCALAPPDATA%\VALORANT\Saved\Config\` [O]. Há subpastas **por conta** (`<id>-<região>\Windows\GameUserSettings.ini`, `RiotUserSettings.ini`) e um `GameUserSettings.ini` da máquina [13][14] [O/C]. Formato INI do Unreal Engine 4.
- **Opções de inicialização:** não há (o Riot Client inicia o jogo).
- **Settings de latência e frametime:**
  - **NVIDIA Reflex:** On + Boost. Relato de Blur Busters: com gargalo de CPU, a latência caiu de cerca de 7–8 ms para cerca de 3,5–4,5 ms com Boost [15] [C].
  - **Limite de FPS:** "Limitar FPS sempre" desligado com Reflex, ou um teto estável.
  - **Modo:** tela cheia. **V-Sync** desligado.
  - **Multithreaded Rendering:** ligado em CPUs com 4 ou mais núcleos [C].
- **Segurança da edição:**
  - não há declaração oficial da Riot sobre editar esses arquivos [C];
  - **parte das configurações fica no servidor da conta**: o erro "Error retrieving settings from server" mostra que o cliente sincroniza com a Riot [14] [O]. Uma edição local pode ser sobrescrita no login;
  - a Riot só orienta **apagar** a pasta para resetar, nunca editar [13] [O];
  - a resolução esticada por `.ini` é tema controverso, sem posição oficial [16] [C].
- **Armadilhas:** o servidor sobrescreve, os arquivos são por conta e o Vanguard fica ativo desde o boot (o modo sob demanda também observa o que carrega antes do jogo).
- **Veredito:** ⛔ **evitar edição de arquivo.** O risco não é comprovado, mas não há fonte oficial favorável e a sincronização com o servidor anula o ganho. Tudo o que importa está no menu. O app pode **recomendar** os valores na interface.

## 4. League of Legends

- **Anticheat:** Riot Vanguard (desde 2024) [17] [O].
- **Arquivos:** `C:\Riot Games\League of Legends\Config\game.cfg` (INI) e `PersistedSettings.json`. Ficam **dentro da pasta de instalação** [13] [O]. O TFT usa o mesmo `game.cfg` [C].
- **Opções de inicialização:** não há.
- **Settings de latência e frametime:**
  - **Limite de quadros** no menu (`FrameCapType` no `game.cfg`): sem limite ou um teto estável [18] [C];
  - **Modo:** tela cheia;
  - **V-Sync** e **"Aguardar sincronização vertical"** desligados;
  - **anti-aliasing** desligado é escolha de imagem, não de latência.
  
  O LoL não tem Reflex. O fallback é o Ultra Low Latency do driver [3] [O].
- **Segurança da edição:**
  - o arquivo está na pasta de instalação, que a **regra 7 proíbe**;
  - guias mandam marcar como somente leitura porque o cliente sobrescreve, o que a **regra 13 proíbe** [18][19] [C];
  - parte dos ajustes do cliente é sincronizada com a conta [C].
- **Armadilhas:** o cliente regrava ao abrir. O caminho é configurável (instalação em outro disco).
- **Veredito:** ⛔ **evitar.** O ganho já é obtido pelo menu e pelo driver (ULLM e energia), sem tocar no arquivo.

## 5. Fortnite

- **Anticheat:** Easy Anti-Cheat e BattlEye (kernel, ambos ativos) [O].
- **Arquivos:** `%LOCALAPPDATA%\FortniteGame\Saved\Config\WindowsClient\GameUserSettings.ini` (INI do Unreal Engine 5). É do **usuário do Windows**, não da conta Epic. A própria Epic cita o arquivo no suporte: se o Performance Mode não muda, o arquivo está como somente leitura e é preciso desmarcar [20][21] [O].
- **Opções de inicialização:**
  - pelo Epic Games Launcher (Configurações > Fortnite > argumentos adicionais). A Epic documenta esse caminho para fixar o modo de renderização [21] [O];
  - pela Steam, não se aplica.
- **Settings de latência e frametime:**
  - **NVIDIA Reflex:** "On + Boost", recomendação da **própria Epic** para competitivo [22] [O];
  - **Limite de FPS:** `FrameRateLimit` no INI. 0 significa ilimitado. Ajustável no menu [20] [O];
  - **Modo de renderização:** DX12 ou **Performance**. Guias apontam menor latência no Performance [23] [C], mas **ele reduz a fidelidade visual**. É escolha do usuário, nunca automática;
  - **Tela cheia**, **V-Sync** desligado;
  - **"Multithreaded Rendering"** aparece no DX11/DX12 conforme a versão.
- **Segurança da edição:**
  - o arquivo é do usuário, fora da pasta coberta pelo EAC;
  - a Epic **reconhece** o arquivo no suporte oficial, sem tratá-lo como proibido [20] [O];
  - não há caso conhecido de banimento por editar valores do menu.
- **Armadilhas:**
  - o jogo regrava ao fechar;
  - o modo de renderização às vezes não persiste (a Epic orienta usar argumento de linha de comando) [21] [O];
  - **somente leitura proibido** (quebra o menu, como a própria Epic descreve).
- **Veredito:** ✅ seguro, com ganho **alto**: Reflex, `FrameRateLimit` e o modo de renderização estão num único arquivo. A troca para o modo Performance só com escolha explícita do usuário.

## 6. Apex Legends

- **Anticheat:** Easy Anti-Cheat [O].
- **Arquivos:** `%USERPROFILE%\Saved Games\Respawn\Apex\local\videoconfig.txt` (KeyValues da Source) e `settings.cfg` [24] [C]. Ficam fora da pasta de instalação, por usuário do Windows.
- **Opções de inicialização:**
  - Steam (`localconfig.vdf`, AppID 1172470) ou EA app;
  - a **EA documenta oficialmente** `+fps_max <valor>` [25] [O];
  - **DX12:** a EA documentou `-eac_launcher_settings SettingsDX12.json` no beta. Relatos posteriores apontam para `-anticheat_settings=SettingsDX12.json` ou para o DX12 já padrão [26] [O/C]. Não automatizar sem verificar a versão;
  - a EA app "perde" opções de inicialização com frequência [27] [C].
- **Settings de latência e frametime:**
  - **NVIDIA Reflex:** On + Boost [4] [O];
  - **`+fps_max`:** 0 com Reflex, ou um teto (a EA dá o exemplo de 200) [25] [O];
  - **Modo:** tela cheia;
  - **V-Sync** desligado;
  - **"Adaptive Resolution FPS Target"** em 0 (ligado, a resolução cai dinamicamente).
- **Segurança da edição:**
  - não há regra oficial da EA ou da Respawn sobre `videoconfig.txt`. Fóruns oficiais têm pedidos de esclarecimento sem resposta [28] [C];
  - há relatos (não confirmados) de banimento por "gameplay enhancement" atribuído a comandos de inicialização [29] [C];
  - o único ponto com aval oficial é `+fps_max`.
- **Armadilhas:**
  - o jogo regrava `videoconfig.txt` ao mudar qualquer opção de vídeo;
  - guias mandam travar como somente leitura (proibido aqui);
  - a EA app descarta opções de inicialização.
- **Veredito:** ⚠️ **parcial.** Só `+fps_max` pela opção de inicialização documentada pela EA. **Não editar `videoconfig.txt`**: não há aval oficial, há relatos de punição e o histórico de uso do arquivo para tirar sombras e efeitos o torna sensível.

## 7. Overwatch 2

- **Anticheat:** próprio da Blizzard (Warden/Defense Matrix) [O]. Proíbe "software de terceiros não autorizado".
- **Arquivos:** `%USERPROFILE%\Documents\Overwatch\Settings\Settings_v0.ini` (INI), por usuário do Windows [30] [C]. Se a pasta Documentos estiver no **OneDrive**, o caminho real muda [31] [C]. As configurações de herói, teclas e sensibilidade ficam **no servidor**; o INI guarda principalmente vídeo [32] [C].
- **Opções de inicialização:** pelo Battle.net ("argumentos adicionais de linha de comando").
- **Settings de latência e frametime:**
  - **NVIDIA Reflex:** On ou On + Boost [33] [O];
  - **"Reduce Buffering"** ligado: segundo um desenvolvedor da Blizzard, não deixa a GPU passar de um quadro à frente [34] [O/C];
  - **Limite de quadros:** "Custom", até 600 [C];
  - **Modo:** tela cheia;
  - **V-Sync** e **"Triple Buffering"** desligados.
  
  Há relato com PresentMon de que Reflex ou Reduce Buffering **aumentaram** a latência em um sistema específico [6] [C]. Por isso o app deve oferecer a opção, sem impor.
- **Segurança da edição:** não há fonte da Blizzard que permita ou proíba editar o `Settings_v0.ini` [35] [C]. O arquivo é do usuário e não é verificado por integridade.
- **Armadilhas:**
  - o jogo regrava ao fechar;
  - há relatos de arquivo ausente ou não salvo;
  - OneDrive;
  - o perfil do servidor reaplica o que não é vídeo.
- **Veredito:** ⚠️ **possível, mas sem aval oficial.** Ganho baixo, porque tudo já está no menu. Recomendação: começar só **informando** os valores na interface e deixar a gravação para quando houver fonte oficial.

## 8. Rainbow Six Siege (Siege X)

- **Anticheat:** BattlEye (kernel) mais o QArC da Ubisoft [O].
- **Arquivos:** `%USERPROFILE%\Documents\My Games\Rainbow Six - Siege\<ID da conta Ubisoft>\GameSettings.ini` (INI), **por conta Ubisoft** [36][37] [O/C].
- **Opções de inicialização:** Steam (AppID 359550) ou Ubisoft Connect. Desde o Y10S1 o jogo é **somente DX12**: as opções de inicialização para DX11 e Vulkan não funcionam mais [38] [C].
- **Settings de latência e frametime:**
  - **NVIDIA Reflex:** On + Boost [39] [O];
  - **`FPSLimit`** na seção `[DISPLAY]`: valor mínimo 30, abaixo disso desliga o limite [37] [C];
  - **Modo:** tela cheia;
  - **V-Sync** desligado.
- **Segurança da edição:** é o **único caso em que a própria publisher manda editar o arquivo**. A Ubisoft orienta ativar o `FPSLimit` no `GameSettings.ini` para quem tem 100% de uso de CPU, explicando que o limite impede o jogo de ocupar núcleos além do necessário [36] [O].
- **Armadilhas:**
  - várias contas criam várias pastas, e é preciso identificar a conta certa;
  - editar só com o jogo fechado.
- **Veredito:** ✅ seguro, com ganho **alto em CPUs fracas** (o `FPSLimit` resolve o 100% de CPU). Melhor caso documentado.

## 9. Call of Duty (Warzone / Black Ops 6 / Black Ops 7)

- **Anticheat:** Ricochet (kernel, ativo só com o jogo aberto). TPM 2.0 e Secure Boot obrigatórios a partir do BO7 [O].
- **Arquivos:** `%USERPROFILE%\Documents\Call of Duty\players\`, com arquivos de texto como `s.1.0.cod24.txt` (BO6) e `options.4.cod25.cst`/`s.1.0.cod25.txt` (BO7). Os nomes mudam a cada título e temporada [40][41] [C]. Há subpasta por ID de conta.
- **Opções de inicialização:** Steam ou Battle.net. Pouco usadas.
- **Settings de latência e frametime:**
  - **NVIDIA Reflex:** On + Boost [4] [O];
  - **limite de FPS personalizado** no menu;
  - **modo:** tela cheia exclusiva;
  - **V-Sync** desligado;
  - **`RendererWorkerCount`** (threads de renderização), editável só no arquivo. Guias recomendam igualar ao número de núcleos físicos ou de P-cores [40][41] [C]. **Fora do menu**: a regra 13 proíbe.
- **Segurança da edição:**
  - não há fonte oficial da Activision sobre editar esses arquivos;
  - em recurso de banimento negado, a Activision já citou "manipulação de dados do jogo" [42] [C];
  - o Ricochet teve banimentos indevidos que depois foram revertidos [42] [C].
- **Armadilhas:** os nomes de arquivo mudam por título, o jogo regrava e há valores fora do menu.
- **Veredito:** ⛔ **evitar.** O anticheat é agressivo, não há aval oficial, o ganho específico do arquivo (`RendererWorkerCount`) está fora do menu e os formatos mudam a cada lançamento.

## 10. Rocket League

- **Anticheat:** Easy Anti-Cheat desde abril de 2026 (Epic e Steam). Acabou com o BakkesMod [43][44] [O/C].
- **Arquivos:** `%USERPROFILE%\Documents\My Games\Rocket League\TAGame\Config\TASystemSettings.ini` (INI do Unreal Engine 3), por usuário do Windows. A **Epic cita o arquivo** no suporte oficial: se as configurações não salvam, desmarcar o somente leitura [45] [O]. Há relato de uma segunda cópia na pasta de instalação [C].
- **Opções de inicialização:** Steam (AppID 252950) ou Epic.
- **Settings de latência e frametime:**
  - **Rocket League não tem Reflex** [46] [C]. O fallback é o Ultra Low Latency do driver [3] [O];
  - **`Max FPS`** no menu (`MaxSmoothedFrameRate`/`bSmoothFrameRate` no INI): sem limite ou um teto estável;
  - **V-Sync** desligado;
  - **Modo:** tela cheia;
  - **`OneFrameThreadLag`** ("One Frame Thread Lag" no menu): desligado reduz a latência em troca de FPS, segundo relatos [47] [C]. Está no menu, então é permitido.
- **Segurança da edição:** o arquivo é do usuário e reconhecido pela Epic. O EAC chegou agora, então não há histórico. Os valores devem ficar restritos aos do menu (nada de `bAllowLightShafts`/LOD por INI que esconda informação).
- **Armadilhas:**
  - o jogo regrava ao fechar;
  - somente leitura quebra o salvamento (a própria Epic diz isso);
  - possível duplicata na pasta de instalação.
- **Veredito:** ✅ seguro, com ganho **moderado a alto**. É onde o ajuste de driver (ULLM) mais compensa, porque o jogo não tem Reflex.

## 11. PUBG: Battlegrounds

- **Anticheat:** BattlEye (kernel) mais o Zakynthos (próprio da Krafton, desde 2021) e o Uncheater [48] [O].
- **Arquivos:** `%LOCALAPPDATA%\TslGame\Saved\Config\WindowsNoEditor\GameUserSettings.ini` (INI do Unreal Engine 4), por usuário do Windows [49] [C].
- **Opções de inicialização:** Steam (AppID 578080).
- **Settings de latência e frametime:**
  - **NVIDIA Reflex:** On + Boost [50] [C], [4] [O];
  - **limite de FPS** no menu;
  - **modo:** tela cheia;
  - **V-Sync** desligado;
  - **DX11 ou DX12:** escolha do usuário, porque o resultado varia por hardware.
- **Segurança da edição:**
  - não há fonte oficial;
  - relatos antigos de jogadores dizem que editaram e não foram banidos [51] [C];
  - o BattlEye do PUBG já **bloqueou o ReShade** [52] [C], o que mostra baixa tolerância a ferramentas de imagem;
  - houve manchetes (não confirmadas) de banimento por config [51] [C];
  - o uso histórico do arquivo para tirar folhagem e efeitos o torna sensível.
- **Armadilhas:** o jogo regrava, e guias mandam usar somente leitura.
- **Veredito:** ⚠️ **cauteloso.** Na prática, edição de valores do menu provavelmente é inócua, mas não há fonte oficial. Recomendação: começar só **informando** (Reflex, limite e modo no menu) e não gravar o arquivo até haver aval da Krafton.

## 12. Dota 2

- **Anticheat:** VAC [O].
- **Arquivos:** `...\steamapps\common\dota 2 beta\game\dota\cfg\video.txt` e `boot.vcfg` (`DefaultRenderSystemOption`, a API padrão) [53][54] [C]. Ficam **dentro da pasta de instalação**, embora não façam parte do pacote verificado.
- **Opções de inicialização:** `localconfig.vdf`, AppID 570. As mais usadas: `+fps_max <n>` (o padrão é 120 [C]), `-dx11` ou `-vulkan` (nunca os dois juntos) [53] [C].
- **Settings de latência e frametime:**
  - **Limite de FPS:** `+fps_max`, ou o controle do menu (que tem teto de 240);
  - **API:** DX11, DX12 ou Vulkan, conforme o hardware. Não há regra universal [C];
  - **Modo:** tela cheia;
  - **V-Sync** desligado.
  
  O Dota 2 não tem Reflex [4] [O]. O fallback é o ULLM do driver.
- **Segurança da edição:** opções de inicialização da Steam são o mecanismo oficial do cliente. Os arquivos `cfg` ficam na pasta do jogo (regra 7).
- **Armadilhas:** a Steam precisa estar fechada para gravar o `localconfig.vdf` [5] [C], e o jogo regrava `video.txt`.
- **Veredito:** ✅ seguro **só pelas opções de inicialização** (`+fps_max`, API). Não editar `video.txt` nem `boot.vcfg`. Ganho **moderado**, porque o limite padrão de 120 FPS realmente segura máquinas boas.

## 13. Marvel Rivals

- **Anticheat:** sistema próprio da NetEase, que exige privilégio de administrador. O nível kernel é relatado de forma inconsistente [55] [C].
- **Arquivos:** `%LOCALAPPDATA%\Marvel\Saved\Config\Windows\GameUserSettings.ini` (e `Engine.ini`), INI do Unreal Engine 5 [56] [C].
- **Opções de inicialização:** Steam (AppID 2767030).
- **Settings de latência e frametime:**
  - **NVIDIA Reflex:** On + Boost [57] [O];
  - **limite de FPS** no menu;
  - **modo:** tela cheia;
  - **V-Sync** desligado.
- **Segurança da edição:** **proibido pela publisher.** A lista oficial de software e hardware proibidos (2025-02-26) inclui, entre as ações proibidas com o jogo aberto, *"Unauthorized modification of client configuration files"* e *"Modifying various 'xxx.ini' files related to graphics rendering to alter the game's display"*, com risco de *"removal from the game or stricter penalties"* [58] [O]. Antes disso, a NetEase já tinha pedido para não modificar nenhum arquivo, inclusive config [59] [O/C]. Jogadores também usavam o INI para cancelar partidas [60] [C].
- **Veredito:** ⛔ **não tocar.** Nem para valores do menu. O app só informa.

## 14. Deadlock

- **Anticheat:** VAC (Valve), com sistemas adicionais em desenvolvimento e forte dependência de denúncias de jogadores [61] [C].
- **Arquivos:** `...\steamapps\common\Deadlock\game\citadel\cfg\video.txt` (KeyValues) e `autoexec.cfg`, **dentro da pasta de instalação** [62] [C].
- **Opções de inicialização:** `localconfig.vdf`, AppID 1422450. Exemplo: `+fps_max <n>` [63] [C].
- **Settings de latência e frametime:**
  - **NVIDIA Reflex:** existe desde 2024 e reduz a latência em até 45% segundo a NVIDIA [64] [O];
  - **limite de FPS:** `fps_max` (e `fps_max_ui` para menus);
  - **modo:** tela cheia;
  - **V-Sync** desligado.
- **Segurança da edição:** o jogo ainda está em acesso antecipado e o mecanismo de inicialização da Steam é oficial. Os arquivos `cfg` ficam na pasta de instalação (regra 7). Há guias que trocam o `gameinfo.gi`, que é **arquivo do jogo**, proibido.
- **Armadilhas:** jogo em desenvolvimento (chaves mudam), Steam precisa estar fechada e o jogo regrava `video.txt`.
- **Veredito:** ✅ seguro **só pelas opções de inicialização**. Ganho **baixo a moderado**, porque Reflex e limite estão no menu.

## 15. The Finals

- **Anticheat:** Easy Anti-Cheat mais sistemas próprios da Embark, segundo a FAQ oficial (a página deu 404 na verificação direta; o conteúdo veio do resumo da busca) [65] [O]. Em 2025 foi anunciada uma camada **kernel** adicional [66] [C]. Há relatos de 2026 de migração para um sistema próprio (a Embark usa o Elytra em outros títulos), não confirmados. Houve banimentos de inocentes [67] [C].
- **Arquivos:** `%LOCALAPPDATA%\Discovery\Saved\Config\WindowsClient\GameUserSettings.ini` (INI do Unreal Engine 5) [68] [C].
- **Opções de inicialização:** Steam (AppID 2073850).
- **Settings de latência e frametime:**
  - **NVIDIA Reflex** [4] [O];
  - **`FrameRateLimit`** (no menu): teto estável, que melhora o frametime segundo guias [68] [C];
  - **modo:** tela cheia;
  - **V-Sync** desligado;
  - **DX12** é o padrão.
- **Segurança da edição:** não há aval oficial. O anticheat está em mudança e houve falsos positivos.
- **Veredito:** ⚠️ **evitar por ora.** O arquivo é do usuário, mas o anticheat está em transição e não há fonte oficial favorável.

---

## 16. Tabela-resumo

| Jogo | Anticheat | Arquivo de vídeo (por quê) | Onde | Reflex | Editar com o app? | Ganho |
|---|---|---|---|---|---|---|
| CS2 | VAC/Trusted Mode | `cs2_video.txt`, `*.vcfg` (KV) | `userdata\<id>\730\local` ou `remote` (conta Steam) | sim | ✅ jogo e Steam fechados | moderado |
| Valorant | Vanguard | `GameUserSettings.ini` | `%LOCALAPPDATA%\VALORANT\Saved\Config\<conta>` (servidor sincroniza) | sim | ⛔ só informar | baixo |
| League of Legends | Vanguard | `game.cfg`, `PersistedSettings.json` | **pasta de instalação** | não | ⛔ | baixo |
| Fortnite | EAC + BattlEye | `GameUserSettings.ini` | `%LOCALAPPDATA%\FortniteGame\...\WindowsClient` | sim | ✅ (Epic reconhece o arquivo) | alto |
| Apex Legends | EAC | `videoconfig.txt` | `Saved Games\Respawn\Apex\local` | sim | ⚠️ só `+fps_max` (oficial EA) | moderado |
| Overwatch 2 | Blizzard | `Settings_v0.ini` | `Documents\Overwatch\Settings` (OneDrive) | sim + Reduce Buffering | ⚠️ só informar por ora | baixo |
| Rainbow Six Siege | BattlEye + QArC | `GameSettings.ini` | `Documents\My Games\Rainbow Six - Siege\<conta>` | sim | ✅ (Ubisoft manda editar `FPSLimit`) | alto (CPU) |
| CoD Warzone/BO6/BO7 | Ricochet | `s.1.0.cod2x.txt`, `options.*` | `Documents\Call of Duty\players` | sim | ⛔ | — |
| Rocket League | EAC (2026) | `TASystemSettings.ini` | `Documents\My Games\Rocket League\TAGame\Config` | **não** | ✅ (Epic reconhece o arquivo) | moderado/alto |
| PUBG | BattlEye + Zakynthos | `GameUserSettings.ini` | `%LOCALAPPDATA%\TslGame\...\WindowsNoEditor` | sim | ⚠️ só informar | moderado |
| Dota 2 | VAC | `video.txt`, `boot.vcfg` | **pasta de instalação** | não | ✅ só opções de inicialização | moderado |
| Marvel Rivals | NetEase | `GameUserSettings.ini`, `Engine.ini` | `%LOCALAPPDATA%\Marvel\Saved\Config\Windows` | sim | ⛔ **proibido pela publisher** | — |
| Deadlock | VAC | `video.txt` | **pasta de instalação** | sim | ✅ só opções de inicialização | baixo/moderado |
| The Finals | EAC + próprio/kernel | `GameUserSettings.ini` | `%LOCALAPPDATA%\Discovery\...\WindowsClient` | sim | ⚠️ evitar por ora | moderado |

Regras de implementação que valem para todos os ✅:

1. Detectar por nome de processo (sem handle) que o jogo **e** o launcher (Steam, Epic, Ubisoft Connect) estão fechados.
2. Fazer backup do arquivo original, com hash, antes de gravar.
3. Gravar só chaves conhecidas, com valores que o menu oferece, preservando o resto do arquivo byte a byte (sem reformatar o INI ou VDF).
4. Nunca usar somente leitura.
5. Na próxima varredura, detectar se o jogo ou a nuvem reverteram o valor e **só informar**, sem regravar em loop.
6. Ter um botão "restaurar original".

---

## 17. Fontes

### Gerais
1. [M] Blur Busters, G-SYNC 101: In-game vs. External FPS Limiters: https://www.blurbusters.com/gsync/gsync101-input-lag-tests-and-settings/11/
2. [M] Blur Busters, G-SYNC 101: External FPS Limiter HOWTO: https://www.blurbusters.com/gsync/gsync101-input-lag-tests-and-settings/12/
3. [O] NVIDIA, System Latency Optimization Guide: https://www.nvidia.com/en-us/geforce/guides/system-latency-optimization-guide/
4. [O] NVIDIA, Reflex: jogos suportados: https://www.nvidia.com/en-us/geforce/technologies/reflex/supported-products/ · Reflex em Apex, Valorant, Fortnite e CS2: https://www.nvidia.com/en-gb/geforce/news/reflex-low-latency-apex-legends-valorant-fortnite-cs2
5. [C] Edição do `localconfig.vdf` exige Steam fechada: https://github.com/Fincer/steam-launchoptions · https://gist.github.com/Aetopia/4f1700e58f76b2a3704148aabc721136
6. [C/M] Blur Busters fórum, Reflex e Reduce Buffering no OW2 medidos com PresentMon: https://forums.blurbusters.com/viewtopic.php?p=116903 · Reflex: como funciona: https://forums.blurbusters.com/viewtopic.php?p=67264
7. [M] TechPowerUp, NVIDIA Reflex testado com LDAT v2: https://techpowerup.com/review/nvidia-reflex-review-test-ldat-v2/3.html

### CS2
8. [C] Steam Community, arquivos de configuração do CS2: https://steamcommunity.com/app/730/discussions/0/3881597253356393539 · repositório comunitário (chaves de `cs2_video.txt` sem efeito): https://www.github.com/kirintwn/cs2-config
9. [C] CSDB, CS2 Config & Settings Guide: https://csdb.gg/guides/config-guide/ · [O] Valve Developer Wiki, VCFG: https://developer.valvesoftware.com/wiki/VCFG
10. [C] Switchblade Gaming, CS2 e Steam Cloud (`remote\cfg`): https://www.switchbladegaming.com/cs2/config-file-backup/
11. [C] Blur Busters fórum, latência no CS2 (tela cheia × sem borda): https://forums.blurbusters.com/viewtopic.php?p=112130
12. [C/M] Igor's Lab, Reflex e Boost × limitador, janela × tela cheia: https://igorslab.de/en/nvidia-reflex-and-boost-vs-frame-limiter-windowed-mode-vs-full-screen-practice/4
- [O] Steam, CS2 Trusted Mode: https://help.steampowered.com/faqs/view/09A0-4879-4353-EF95 · VAC: https://help.steampowered.com/faqs/view/571A-97DA-70E9-FF74

### Valorant e League of Legends
13. [O] Riot, Reset configuration files (caminhos de Valorant e LoL): https://support.riotgames.com/en-us/riot/performance/reset-configuration-files
14. [O] Riot, "Error retrieving settings from server": https://support.riotgames.com/en-us/valorant/support-tools/how-to-fix-error-retrieving-settings-from-server · [C] caminho por conta: https://forums.blurbusters.com/viewtopic.php?p=94164
15. [C] Blur Busters fórum, Reflex Boost no Valorant: https://forums.blurbusters.com/viewtopic.php?p=94228
16. [C] Dexerto, resolução esticada no Valorant: https://dexerto.com/valorant/riot-games-dev-hints-that-valorant-wont-allow-stretched-res-1339837
17. [O] Riot, /dev: Vanguard x LoL: https://www.leagueoflegends.com/en-us/news/dev/dev-vanguard-x-lol/
18. [C] Esportstales, LoL: `game.cfg` e somente leitura: https://esportstales.com/league-of-legends/how-to-increase-fps-video-settings-client-options
19. [C] TGG, pasta de config do LoL: https://theglobalgaming.com/lol/lol-config-file-location

### Fortnite
20. [O] Epic, How do I adjust my FPS in Fortnite (cita o `GameUserSettings.ini` somente leitura): https://www.epicgames.com/help/c-202300000001636/c-202300000001719/fortnite-a202300000010554
21. [O] Epic, My rendering mode option was not saved: https://www.epicgames.com/help/c-34254770/c-38015632/a22616652
22. [O] Epic, Fortnite on PC: best settings for competitive play in 2026: https://store.epicgames.com/news/fortnite-on-pc-best-settings-for-competitive-play-in-2026
23. [C] Dot Esports, como reduzir o input delay no Fortnite: https://dotesports.com/fortnite/news/how-to-reduce-input-delay-in-fortnite · BetterFPS v41.30: https://betterfps.com/blog/fortnite-chapter-7-season-3-v41-30-best-settings-2026

### Apex Legends
24. [C] Esportstales, Apex: `videoconfig.txt`: https://www.esportstales.com/apex-legends/how-to-increase-fps-videoconfig-settings-launch-options
25. [O] EA, Apex Legends FPS guide (`+fps_max`): https://help.ea.com/en/articles/apex-legends/change-and-cap-fps/
26. [O] EA, Season 15 DX12 beta info: https://forums.ea.com/blog/apex-legends-game-info-hub-en/season-15-dx-12-beta-info/9465500 · [C] mudança do argumento: https://forums.ea.com/discussions/apex-legends-technical-issues-en/apex-legends-settingsdx12-json-not-found-ea-client/5788247
27. [C] EA app perde opções de inicialização: https://forums.ea.com/discussions/ea-app-technical-issues-en/ea-app-keeps-loosing-advanced-launch-options/7636189
28. [C] Fórum EA, "Config rules are unclear": https://forums.ea.com/discussions/apex-legends-feedback-en/re-config-rules-are-unclear-ea-staff-refuses-responses-and-abuses-power-when-i-a/8649142
29. [C] Fórum EA, possível banimento por comandos de inicialização: https://forums.ea.com/discussions/apex-legends-technical-issues-en/possible-false-gameplay-enhancement-ban-from-fps-launch-commands/13394265

### Overwatch 2
30. [C] Fórum Blizzard, Settings_v0.ini: https://us.forums.blizzard.com/en/overwatch/t/overwatch-settings-not-saving-missing-settings-files/668169
31. [C] Fórum Blizzard, OneDrive: https://us.forums.blizzard.com/en/overwatch/t/change-settings-directory-to-accommodate-onedrive/1003875
32. [C] Fórum Blizzard, configurações de herói no servidor: https://us.forums.blizzard.com/en/blizzard/t/overwatch-reset-my-hero-settings-after-log-in-bug/51658
33. [O] NVIDIA, Overwatch Reflex: https://www.nvidia.com/en-us/geforce/news/overwatch-reflex-out-now
34. [O/C] Fórum Blizzard, explicação do Reduce Buffering: https://us.forums.blizzard.com/en/overwatch/t/300fps-no-reduce-buffering-vs-display-based-cap-reduce-buffering/232721
35. [O] Blizzard, Defense Matrix: https://overwatch.blizzard.com/en-gb/news/24173281/defense-matrix-keeping-overwatch-2-fair-and-fun-for-every-player/

### Rainbow Six Siege
36. [O] Ubisoft, 100% CPU usage (`FPSLimit` no `GameSettings.ini`): https://www.ubisoft.com/en-us/game/rainbow-six/siege/news-updates/7MPHeuO6hPcy49Mdc3rbDg/100-cpu-usage
37. [C] Steam Community, caminho por conta e `FPSLimit`: https://steamcommunity.com/app/359550/discussions/0/3276824488726693041
38. [C] Sportskeeda, fim do DX11: https://sportskeeda.com/esports/news-rainbow-six-siege-officially-discontinues-dx11-support-ahead-the-new-update · TweakTown, Siege X só DX12: https://www.tweaktown.com/news/103643/rainbow-six-siege-moves-to-directx-12-on-pc-ubisoft-preps-massive-r6-siege-x-update/index.html
39. [O] NVIDIA, Reflex em Overwatch e Rainbow Six Siege: https://www.nvidia.com/en-us/geforce/news/nvidia-reflex-overwatch-rainbow-six-siege

### Call of Duty
40. [C] hone.gg, BO6: `s.1.0.cod24.txt` e `RendererWorkerCount`: https://hone.gg/blog/best-black-ops-6-settings-for-fps/ · Steam Community: https://steamcommunity.com/app/1938090/discussions/0/3470612993474811664
41. [C] Prima Games, pasta `players`: https://primagames.com/tips/call-of-duty-black-ops-6-save-file-location-on-pc · hone.gg BO7: https://hone.gg/blog/increase-fps-in-cod-black-ops-7/
42. [O] Ricochet, atualização da Season 03: https://www.callofduty.com/blog/2025/03/call-of-duty-ricochet-black-ops-6-warzone-season-three-anti-cheat-update · [C] PC Gamer, banimentos indevidos: https://pcgamer.com/it-looks-like-call-of-dutys-anti-cheat-is-permabanning-innocent-players
- [O] Ricochet no BO7 (TPM e Secure Boot): https://www.callofduty.com/blog/2025/09/call-of-duty-black-ops-7-beta-ricochet-anti-cheat-update

### Rocket League
43. [O] Epic, Easy Anti-Cheat is now available in Rocket League: https://www.epicgames.com/help/rocket-league-c-202300000001622/gameplay-c-202300000001682/easy-anti-cheat-is-now-available-in-rocket-league-a202300000084633
44. [C] PC Gamer, EAC encerra o BakkesMod: https://www.pcgamer.com/games/easy-anti-cheat-arrives-in-rocket-league-and-marks-the-end-of-the-games-most-popular-mod-the-right-time-to-bring-things-to-a-close/
45. [O] Epic, Rocket League doesn't save my settings (`TASystemSettings.ini`): https://www.epicgames.com/help/rocket-league-c-202300000001622/technical-support-c-202300000001679/what-do-i-do-if-rocket-league-doesn-t-save-my-settings-a202300000082700
46. [C] Tier1Settings, Rocket League sem Reflex: https://tier1settings.com/rocket-league-lowest-input-lag-settings/ · Liquipedia: https://liquipedia.net/rocketleague/Settings
47. [C] Steam Community, `OneFrameThreadLag`: https://steamcommunity.com/app/252950/discussions/0/3192489717248185958

### PUBG
48. [O] PUBG, 2021 Anti-Cheat Dev Letter (Zakynthos e BattlEye): https://pubg.com/en/news/1612
49. [C] Steam Community, caminho do `GameUserSettings.ini`: https://steamcommunity.com/app/578080/discussions/1/1697169163415940160
50. [C] Tier1Settings, PUBG com menor input lag: https://tier1settings.com/pubg-lowest-input-lag-settings/
51. [C] Steam Community, edição do `.ini` e banimento: https://steamcommunity.com/app/578080/discussions/1/135512625250840272/?l=english
52. [C] PC Gamer, PUBG bloqueia ReShade: https://www.pcgamer.com/au/pubg-banned-over-1-million-players-in-january-new-anti-cheat-measures-block-reshade

### Dota 2
53. [C] Esportstales, Dota 2: opções de inicialização: https://www.esportstales.com/dota-2/how-to-increase-fps-video-settings-launch-options · Dexerto: https://dexerto.com/dota2/best-dota-2-launch-options-optimize-game-performance-1792226
54. [C] Steam Community, `boot.vcfg` e `DefaultRenderSystemOption`: https://steamcommunity.com/app/570/discussions/0/4039231211460401864
- [O] Valve Developer Wiki, VAC: https://developer.valvesoftware.com/wiki/Valve_Anti-Cheat

### Marvel Rivals
55. [C] ExitLag, anticheat do Marvel Rivals: https://www.exitlag.com/blog/marvel-rivals-anti-cheat/ · OpenCritic: https://opencritic.com/news/31682/marvel-rivals-is-cracking-down-on-cheaters
56. [C] Steam Community, caminho do `GameUserSettings.ini`: https://steamcommunity.com/app/2767030/discussions/0/591759268792825779
57. [O] NVIDIA, Reflex no Marvel Rivals (Season 3): https://www.nvidia.com/en-us/geforce/news/reflex-marvel-rivals-3-season-3-coming-soon-to-more-games/
58. [O] NetEase, Prohibited Software & Hardware List of Marvel Rivals (2025-02-26): https://marvelrivals.com/guide/20250226/41348_1214569.html
59. [O/C] PCGamesN, aviso da NetEase para não modificar arquivos: https://www.pcgamesn.com/marvel-rivals/game-files
60. [C] PC Gamer, config usada para cancelar partidas: https://pcgamer.com/games/third-person-shooter/marvel-rivals-players-are-messing-with-their-config-files-to-cancel-games-with-no-consequence-before-they-get-a-loss

### Deadlock
61. [C] PC Gamer, anticheat do Deadlock: https://pcgamer.com/games/moba/valve-developer-has-to-keep-reminding-players-that-deadlock-s-anti-cheat-system-is-coming-and-very-high-priority-since-it-s-leaning-mostly-on-player-reports-right-now
62. [C] Fórum Deadlock, caminho do `video.txt`: https://forums.playdeadlock.com/threads/windowed-launch-option-not-working.103973
63. [C] Tier1Settings, Deadlock com menor input lag: https://tier1settings.com/deadlock-lowest-input-lag-settings/
64. [O] NVIDIA, Deadlock adiciona Reflex: https://www.nvidia.com/en-us/geforce/news/reflex-deadlock

### The Finals
65. [O] Embark, Anti-Cheat Policy FAQ (404 na verificação direta; conteúdo via resumo de busca): https://id.embark.games/the-finals/support/faq/44-anti-cheat-policy-faq
66. [C] GamingOnLinux, nova camada kernel no The Finals: https://www.gamingonlinux.com/2025/07/the-finals-is-getting-new-kernel-based-anti-cheat-likely-to-break-it-on-linux-steamos-steam-deck
67. [C] TechRadar, banimentos de inocentes: https://www.techradar.com/gaming/consoles-pc/innocent-players-are-getting-banned-as-the-finals-takes-the-war-to-cheaters
68. [C] gameplay.tips, `FrameRateLimit` no The Finals: https://gameplay.tips/guides/the-finals-how-to-limit-fps-for-more-consistent-frame-times.html

### Lacunas conhecidas

- O PCGamingWiki, a página de FPS do Fortnite da Epic e a página da Blur Busters bloquearam a leitura direta (403) durante a pesquisa. As citações vêm dos resumos de busca e devem ser conferidas manualmente.
- Os caminhos marcados [C] precisam ser confirmados em uma máquina real antes da implementação: CS2 (`local` × `remote`), CoD (nome do arquivo por título) e R6 (pasta por conta).
- Nenhuma publisher, exceto a Ubisoft (R6) e, indiretamente, a Epic (Fortnite e Rocket League), reconhece oficialmente a edição dos arquivos de config. A NetEase (Marvel Rivals) proíbe expressamente.
- A Valve só documenta as opções de inicialização como mecanismo do cliente. Não há declaração específica sobre `cs2_video.txt`.
- As chaves exatas de cada INI mudam com patches. A implementação deve ler o arquivo, reconhecer só chaves conhecidas e abortar diante de formato inesperado.
