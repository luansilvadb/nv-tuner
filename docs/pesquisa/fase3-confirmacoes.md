# Fase 3 — confirmações em fontes (pesquisa de 2026-11-26)

Escopo: pontos “a confirmar” de `docs/spec/competitivo.md` §9 e §11 (itens 11–13).
Legenda: **confirmado** / **corrigido** / **não confirmado** (com o que checar numa máquina real).
Nenhum arquivo real desta máquina foi lido. Fontes [O] = oficiais; [C] = comunidade.
Ressalva geral: quase todas as fontes encontradas são resultados de busca e trechos de página, não leitura integral; reconferir os links [O] antes do lançamento (S8).

## 1. Rainbow Six Siege

| Ponto | Veredito | Fonte |
|---|---|---|
| Arquivo e seção: `GameSettings.ini`, `[DISPLAY]`, `FPSLimit` | **confirmado** [O] | [Ubisoft — 100% CPU usage](https://www.ubisoft.com/en-us/game/rainbow-six/siege/news-updates/7MPHeuO6hPcy49Mdc3rbDg/100-cpu-usage) |
| Valores: inteiro; mínimo 30; abaixo de 30 desliga o limitador; exemplo `FPSLimit=60` | **confirmado** [O] (comentário do próprio arquivo citado pela Ubisoft) | idem |
| Caminho por conta: `Documents\My Games\Rainbow Six - Siege\<id da conta Ubisoft Connect>\GameSettings.ini` (o artigo de 2018 mostra o caminho sem subpasta) | **corrigido/confirmado**: há subpasta por conta (GUID, ex. `cf5ac81a-…`) | [PCGamingWiki (espelho)](https://gamecytus.netlify.app/host-https-www.pcgamingwiki.com/wiki/Tom_Clancy%27s_Rainbow_Six_Siege); [Ubisoft Support na Steam](https://steamcommunity.com/app/359550/discussions/0/135513549099344598) [O] |
| Registro adicional `HKCU\Software\Ubisoft\Rainbow Six - Siege\` | informativo (não editar) | PCGamingWiki (espelho) |
| Codificação | **não confirmado**: checar BOM e fim de linha numa amostra anonimizada | — |
| Regrava ao fechar | **não confirmado**: há relatos de “configurações resetam” ([devtrackers](https://devtrackers.gg/r6-siege/p/7d259ded-everytime-i-start-rainbow-six-siege-all-of-my-settings-reset)); L3-1 deve conferir que o valor persiste após abrir e fechar | — |
| OneDrive redirecionando Documentos | **não confirmado nas fontes**, mas é comportamento do Windows; `SHGetKnownFolderPath(FOLDERID_Documents)` (já na spec) cobre | — |
| Chave de Reflex | **não confirmada**; fica fora | — |
| Risco: um relato [C] de travamentos com `FPSLimit` acima de 100 | anedótico; manter L3-1 | [Steam](https://steamcommunity.com/app/359550/discussions/0/1733213724898470023) |

Catálogo proposto: `r6_fps_limit` → seção `DISPLAY`, chave `FPSLimit`, tipo inteiro, faixa 30–(teto do menu, a medir); nunca gravar < 30 (desligaria o limite).

## 2. Fortnite

| Ponto | Veredito | Fonte |
|---|---|---|
| Caminho `%LOCALAPPDATA%\FortniteGame\Saved\Config\WindowsClient\GameUserSettings.ini` | **confirmado** [C], arquivo citado por Epic [O] | [Epic — adjust my FPS](https://www.epicgames.com/help/c-202300000001636/c-202300000001719/fortnite-a202300000010554); [esportstales](https://www.esportstales.com/fortnite/how-to-increase-fps-video-options-gameusersettings) |
| `FrameRateLimit` em float (`240.000000`), `0` = ilimitado | **confirmado** [C] (formato float padrão do UE) | esportstales |
| Teto do menu 360 e opção “Ilimitado”; lobby limitado a 60/120 | **confirmado** [O] | Epic — adjust my FPS |
| Lista completa do menu | **não confirmado**: ler a lista no menu do build atual | — |
| Somente leitura quebra o menu | **confirmado** [O] | [Epic — low FPS](https://www.epicgames.com/help/c-34254770/a26749910) |
| Seção `[/Script/FortniteGame.FortGameUserSettings]` | **não confirmada** textualmente nas fontes lidas; checar em amostra | — |
| Chave de Reflex | **não confirmada** (`bLatencyFlash` é outra coisa — flash do indicador; sem documentação) | [Reddit (espelho)](https://lr.eu.psf.lt/r/FortNiteBR/comments/162qi0i/the_ultimate_optimization_guide_for_fortnite) |

Catálogo proposto: `fortnite_frame_rate_limit` → chave `FrameRateLimit`, tipo float com 6 casas, valores = lista do menu (a capturar), máx. 360; Reflex sai.

## 3. Rocket League

| Ponto | Veredito | Fonte |
|---|---|---|
| Caminho `Documents\My Games\Rocket League\TAGame\Config\TASystemSettings.ini` | **confirmado** [C] | [PCGH](https://www.pcgameshardware.de/Rocket-League-Spiel-54968/Tipps/60-Fps-Lock-aufheben-1168360/) |
| Max FPS do menu: chaves `bSmoothFrameRate` / `MaxSmoothedFrameRate` (seção `[SystemSettings]`) | **não confirmado**: só exemplo de usuário (`MaxSmoothedFrameRate=9999`); falta saber qual chave o menu grava e seus valores | [pastebin](https://pastebin.com/fMfewWE0); [Epic — limit frame rate](https://www.epicgames.com/help/rocket-league-c-202300000001622/gameplay-c-202300000001682/how-do-i-limit-the-frame-rate-in-rocket-league-a202300000015811) [O] (só menu) |
| `OneFrameThreadLag` (True/False), False reduz latência e custa FPS | **confirmado** [C] | [Steam](https://steamcommunity.com/app/252950/discussions/0/3192489717248185958) |
| Teto 250 do jogo | [C], não oficial | [esportstales](https://esportstales.com/rocket-league/how-to-increase-fps-tasystemsettings-video-options) |
| Regravação | o jogo regrava o arquivo (guias mandam marcar somente leitura) → F3-08 deriva | PCGH |

Catálogo: `OneFrameThreadLag` (bool `True`/`False`) pode entrar; Max FPS **sai** até comparar o arquivo antes/depois de mudar o menu.

## 4. Steam

| Ponto | Veredito | Fonte |
|---|---|---|
| `userdata\<SteamID3>\config\localconfig.vdf`, `UserLocalConfigStore/Software/Valve/Steam/apps/<appid>/LaunchOptions` | **confirmado** [C] (ferramentas que editam) | [gist frostworx](https://gist.github.com/frostworx/36bd76e705a0c87af523fa57cfeebaf8); [steam-launchoptions](https://fjordtek.com/git/Fincer/steam-launchoptions/blame/branch/master/steam_launchoptions.sh) |
| Steam aberta ignora/regrava a edição → exigir `steam.exe` fechado | **confirmado** [C] (ferramentas abortam ou fecham a Steam) | idem; [gist Aetopia](https://gist.github.com/Aetopia/4f1700e58f76b2a3704148aabc721136) |
| `LaunchOptions` sincroniza pela nuvem | **não confirmado**: há relato de partes de `localconfig.vdf` voltando da nuvem; `sharedconfig.vdf` sincroniza | [Steam forum](https://steamcommunity.com/discussions/forum/0/1747893292776876733); [Steam forum](https://steamcommunity.com/discussions/forum/1/601892123631957358) — L3-3 com dois PCs |
| `loginusers.vdf` / `MostRecent`, `HKCU\Software\Valve\Steam\SteamPath` | **não confirmado** em fonte lida nesta pesquisa (conhecimento comum; checar em amostra) | — |
| SteamID3 = SteamID64 − 76561197960265728 (32 bits baixos) | **confirmado** (documentação do formato) | [VDC — SteamID](https://developer.valvesoftware.com/wiki/SteamID) |

## 5. CS2

| Ponto | Veredito | Fonte |
|---|---|---|
| `cs2_video.txt` em `userdata\<id>\730\local\cfg` | **confirmado** [C] | [csdb.gg](https://csdb.gg/guides/config-guide/) |
| `local` × `remote`, Cloud | **não confirmado**: guia diz que editar só funciona com sincronização desligada → reforça F3-09 | csdb.gg |
| `setting.r_low_latency` (“0”/“1”; talvez “2” = Boost) em KeyValues; também `setting.fps_max` | **parcial** [C] (amostras de usuários) | [scribd](https://www.scribd.com/document/889449359/cs2-video) |
| Chave de V-Sync | **não confirmada** | — |
| `+fps_max <n>` na inicialização funciona; `0` = sem limite; exige V-Sync desligado | **confirmado** [C] | [totalcsgo](https://totalcsgo.com/commands/fpsmax); [Prima](https://primagames.com/tips/how-to-cap-fps-in-counter-strike) |
| Conflito: o menu também grava `setting.fps_max` em `cs2_video.txt` | **não confirmado** qual prevalece → L3-3 | — |

## 6. Dota 2 — **confirmado** [C]: padrão 120, menu até 240, `+fps_max` aceito e sobrepõe o menu ([esportstales](https://www.esportstales.com/dota-2/how-to-increase-fps-video-settings-launch-options); [Dexerto](https://dexerto.com/dota2/best-dota-2-launch-options-optimize-game-performance-1792226)). Catálogo: `fps_max` inteiro 60–240.

## 7. Deadlock — AppID 1422450 **confirmado** ([Steam](https://steamcommunity.com/app/1422450/discussions/0/4431066036853004082)); `fps_max` no console **confirmado** [C]; `+fps_max` na inicialização **não confirmado** (provável no Source 2, testar). Faixa do menu desconhecida.

## 8. Anticheat
Nenhum relato encontrado de EAC, BattlEye ou VAC reagindo à edição desses arquivos de usuário (a verificação de integridade de EAC/BattlEye cobre a pasta de instalação). Ausência de relato não é prova: manter L3-6 e as fontes oficiais de §11 item 16.

## Mudanças de escopo propostas
- Rocket League Max FPS e `jogo-limite-no-jogo-rocket-league`: suspensos até capturar a chave em máquina real; `OneFrameThreadLag` fica (experimental).
- Fortnite: Reflex sai; `FrameRateLimit` só após capturar a lista do menu.
- R6: Reflex sai; catálogo usa pasta por conta (F3-12 mantido).
- `jogo-cs2-reflex-video`: continua bloqueada (só `local`, Cloud desligada e valores confirmados).
- Deadlock: `+fps_max` só após teste L3-3 equivalente; sem faixa de menu, usar a do usuário limitada ao refresh.
