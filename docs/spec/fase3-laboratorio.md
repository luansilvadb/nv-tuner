# Fase 3: registro do laboratório

Roteiro e registro dos testes em máquina real da configuração de jogo
(`docs/spec/competitivo.md` §9.6, `docs/spec/fase3-plano.md` L3-1 a L3-6).
Cada teste decide se a regra continua experimental ou é promovida a estável na
versão seguinte. Os campos em branco (`____`) são preenchidos por quem executa.

## Regras gerais

- **Ambiente:** registrar uma vez, na tabela abaixo, e repetir por teste só o
  que mudar.
- **Aceite:** toda regra da Fase 3 é experimental; aceitar na interface antes
  de cada teste e retirar no fim.
- **Janela de edição:** o agente só grava com o jogo, os processos protegidos
  e, nos jogos da Steam, o `steam.exe` fechados. Fechar tudo, esperar o ciclo
  (ou `nvt-service status`) e conferir a seção **Arquivo do jogo**.
- **Medição:** PresentMon com captura de pelo menos 60 s no mesmo trecho (mapa
  de treino ou replay), três repetições por condição; registrar média de FPS,
  frametime p99 e, quando houver, entrada→fóton (Reflex Latency Analyzer ou
  câmera de alta velocidade). Uma diferença só conta se for maior que a
  variação entre as três repetições.

### Como verificar o arquivo sem expor dados pessoais

Os caminhos contêm o nome do usuário do Windows, o SteamID ou o ID da conta
Ubisoft, e o `localconfig.vdf` contém dados da conta Steam. Nada disso entra no
registro.

1. Antes de aceitar a regra, abrir um PowerShell sem elevação e guardar o hash
   e só a linha da chave:

   ```powershell
   $a = "<caminho do arquivo>"
   (Get-FileHash $a -Algorithm SHA256).Hash.Substring(0,12)
   Select-String -Path $a -Pattern 'FPSLimit|OneFrameThreadLag|LaunchOptions' |
     ForEach-Object { $_.Line.Trim() }
   ```

2. Depois da gravação e depois da restauração, repetir os dois comandos.
3. No registro entram **só**: os 12 primeiros caracteres do hash, a linha da
   chave (no `LaunchOptions`, apenas o token `+fps_max <n>`; o resto da string
   do usuário é substituído por `…`) e o caminho escrito com variáveis
   (`%USERPROFILE%\Documents\…\<conta>\GameSettings.ini`,
   `Steam\userdata\<steamid3>\config\localconfig.vdf`).
4. Nunca anexar o arquivo inteiro, capturas com o caminho real, o SteamID, o
   nome da conta nem o backup (`%LOCALAPPDATA%\nv-tuner\backups`).
5. Conferência de restauração: o hash depois de restaurar deve ser igual ao de
   antes (quando o jogo não regravou o arquivo) ou, se diferente, a linha da
   chave deve ter voltado ao valor original.

## Ambiente

| Campo | Valor |
|---|---|
| Data | ____ |
| Versão do nv-tuner (commit) | ____ |
| GPU e driver | ____ |
| CPU, RAM | ____ |
| Monitor (Hz, G-SYNC/VRR) | ____ |
| Windows (build) | ____ |
| Ferramenta de medição | ____ |

## L3-1: Rainbow Six Siege, `FPSLimit` vs. limite do driver

Regra: `jogo-limite-no-jogo-r6`. Arquivo: `%USERPROFILE%\Documents\My
Games\Rainbow Six - Siege\<conta>\GameSettings.ini`, `[DISPLAY] FPSLimit`.

1. Definir o limite de FPS do usuário (ex.: 224) sem aceitar a regra; medir 3x
   com o limite do driver (condição A).
2. Fechar o jogo e o BattlEye; guardar hash e linha `FPSLimit`.
3. Aceitar a regra; aguardar a gravação; conferir na interface que o limite do
   driver do perfil do R6 ficou desligado e o arquivo mostra o valor.
4. Abrir o jogo, medir 3x (condição B); fechar e reabrir: `FPSLimit` mantém o
   valor?
5. Com duas contas Ubisoft na pasta: a interface mostra a conta usada, e ela é a
   do `GameSettings.ini` modificado mais recentemente?
6. Retirar a regra; conferir hash e linha (restauração) e o limite do driver de
   volta.

| Campo | Valor |
|---|---|
| CPU % / FPS / p99 / entrada→fóton (A, driver) | ____ |
| CPU % / FPS / p99 / entrada→fóton (B, jogo) | ____ |
| Valor persiste após abrir e fechar | ____ |
| Conta escolhida correta (duas contas) | ____ |
| Hash antes / depois / após restaurar | ____ |
| Resultado | ____ |
| Decisão (manter experimental / promover) | ____ |

**Critério:** B igual ou melhor que A em p99 e entrada→fóton; valor persiste;
conta correta; restauração com hash igual ao original.

## L3-3: Counter-Strike 2, `+fps_max` e Steam Cloud

Regra: `jogo-limite-launch-source2`. Arquivo:
`Steam\userdata\<steamid3>\config\localconfig.vdf`, `apps/730/LaunchOptions`.

1. Pôr nas opções de inicialização do CS2 um token qualquer do usuário (ex.:
   `-novid`) para conferir a preservação.
2. Fechar o CS2 e a Steam; guardar hash e o token `+fps_max` (ausente).
3. Aceitar a regra com limite definido; conferir que a interface pede para
   fechar a Steam enquanto ela estiver aberta e grava só depois.
4. Abrir a Steam: as propriedades do CS2 mostram `-novid +fps_max <n>`? Abrir
   o jogo e rodar `fps_max` no console: valor efetivo igual?
5. Steam Cloud: fechar tudo, reabrir a Steam duas vezes (e, se possível, em
   outra máquina com a mesma conta): `LaunchOptions` volta ao valor anterior?
6. Conferir onde está o `cs2_video.txt` efetivo: `730\local\cfg` ou
   `730\remote\cfg` (só registrar qual existe).
7. Retirar a regra: só o token `+fps_max` some e `-novid` fica.

| Campo | Valor |
|---|---|
| Valor no console após abrir a Steam | ____ |
| Steam Cloud reverteu `LaunchOptions` (sim/não) | ____ |
| `cs2_video.txt` em `local` / `remote` | ____ |
| Token do usuário preservado / restauração remove só o token | ____ |
| Hash antes / depois / após restaurar | ____ |
| Resultado | ____ |
| Decisão | ____ |

**Critério:** valor efetivo após abrir a Steam; Cloud não reverte; restauração
remove só o token.

## L3-4: Rocket League, `OneFrameThreadLag` e cópia na instalação

Regra: `jogo-rl-one-frame-thread-lag-off`. Arquivo: `%USERPROFILE%\Documents\My
Games\Rocket League\TAGame\Config\TASystemSettings.ini`.

1. Medir 3x com `OneFrameThreadLag=True` (condição A): FPS, p99 e
   entrada→fóton.
2. Fechar o jogo e o EAC; guardar hash e linha; anotar o hash do
   `TASystemSettings.ini` da pasta de instalação, se existir.
3. Aceitar a regra; conferir a linha `OneFrameThreadLag=False`.
4. Medir 3x (condição B); registrar a sensação de resposta do carro.
5. Conferir que o hash da cópia na pasta de instalação não mudou.
6. Retirar a regra; conferir restauração.

| Campo | Valor |
|---|---|
| FPS / p99 / entrada→fóton (A, True) | ____ |
| FPS / p99 / entrada→fóton (B, False) | ____ |
| Sensação (resposta do carro) | ____ |
| Cópia na instalação intocada (hash igual) | ____ |
| Hash antes / depois / após restaurar | ____ |
| Resultado | ____ |
| Decisão | ____ |

**Critério:** promover só se a latência cair além da variação entre sessões.

## L3-5: Dota 2, `+fps_max` acima de 120

Regra: `jogo-dota2-fps-max-padrao` (sem limite do usuário) e
`jogo-limite-launch-source2` (com limite). Arquivo: `localconfig.vdf`,
`apps/570/LaunchOptions`.

1. Sem limite do usuário e sem a regra: medir 3x (padrão 120).
2. Fechar o Dota 2 e a Steam; guardar hash e token.
3. Aceitar `jogo-dota2-fps-max-padrao`; conferir `+fps_max min(refresh, 240)`.
4. Medir 3x; conferir no menu do jogo o teto oferecido (registrar o máximo).
5. Definir um limite do usuário e aceitar a outra regra: o token assume o
   limite (60 a 240) e o limite do driver do perfil desliga.
6. Retirar as regras; conferir restauração.

| Campo | Valor |
|---|---|
| FPS / p99 (padrão 120) | ____ |
| FPS / p99 (`+fps_max` gravado) | ____ |
| Teto do menu confirmado | ____ |
| Sensação (câmera) | ____ |
| Hash antes / depois / após restaurar | ____ |
| Resultado | ____ |
| Decisão | ____ |

**Critério:** FPS acima de 120 sem regressão de p99; teto do menu igual a 240.

## L3-6: anticheat

Para R6 (BattlEye), CS2 e Dota 2 (VAC) e Rocket League (EAC):

1. Depois de cada gravação de L3-1, L3-3, L3-4 e L3-5, abrir o jogo e entrar
   numa partida (ou modo de treino online).
2. Depois de cada restauração, repetir.
3. Com o jogo aberto, conferir que nenhum processo do nv-tuner tem handle para
   o processo do jogo (Process Explorer, *Find Handle* pelo nome do exe do
   jogo, buscando nos processos `nvt-*`).

| Jogo | Após edição (erro de integridade?) | Após restauração | Handle do nv-tuner sobre o jogo |
|---|---|---|---|
| Rainbow Six Siege | ____ | ____ | ____ |
| Counter-Strike 2 | ____ | ____ | ____ |
| Dota 2 | ____ | ____ | ____ |
| Rocket League | ____ | ____ | ____ |

**Critério:** nenhum erro de integridade (EAC "Untrusted file", BattlEye, VAC) e
nenhum handle aberto pelo nv-tuner.

## Capturas pendentes (chaves `a_confirmar`)

Nada aqui é gravado pelo app. O objetivo é confirmar chave, valores e faixa
para que uma versão futura possa tirar o `a_confirmar`. Em todas: mudar a
opção **pelo menu do jogo**, fechar o jogo e comparar a linha antes/depois com
o procedimento de "Como verificar o arquivo"; registrar só a linha da chave.

### Fortnite: lista do menu de `FrameRateLimit` (L3-2)

Arquivo: `%LOCALAPPDATA%\FortniteGame\Saved\Config\WindowsClient\GameUserSettings.ini`.

1. Para cada valor oferecido em *Limite de taxa de quadros* no menu, escolher,
   fechar o jogo e anotar a linha `FrameRateLimit=` e a seção em que está.
2. Escolher *Ilimitado* e anotar o valor gravado.
3. Ligar e desligar o Reflex no menu e anotar qual linha muda.

| Campo | Valor |
|---|---|
| Seção exata | ____ |
| Valores do menu e valor gravado de cada um | ____ |
| Valor de *Ilimitado* | ____ |
| Chave e valores de Reflex | ____ |
| Arquivo idêntico fora da chave após uma partida | ____ |

### Rocket League: chave de Max FPS

Arquivo: `TASystemSettings.ini` (Documentos).

1. Mudar *Max FPS* no menu para três valores diferentes; anotar quais linhas
   mudam (`MaxSmoothedFrameRate`, `bSmoothFrameRate` ou outra).
2. Anotar a lista completa de valores do menu.

| Campo | Valor |
|---|---|
| Chave(s) que mudam | ____ |
| Valores do menu | ____ |

### Deadlock: faixa do `+fps_max`

1. Anotar o mínimo e o máximo do limitador no menu de vídeo.
2. Pôr `+fps_max 144` à mão nas opções de inicialização e conferir no console
   (`fps_max`) se o valor vale e se sobrepõe o menu.

| Campo | Valor |
|---|---|
| Faixa do menu | ____ |
| `+fps_max` vale na inicialização | ____ |
| Nome do executável confirmado | ____ |

### Chaves de Reflex

| Jogo | Arquivo | Como capturar | Chave e valores (Desligado / Ligado / Ligado + Boost) |
|---|---|---|---|
| Rainbow Six Siege | `GameSettings.ini`, `[DISPLAY]` | alternar Reflex no menu | ____ |
| Fortnite | `GameUserSettings.ini` | ver acima | ____ |
| Counter-Strike 2 | `cs2_video.txt` (`local` ou `remote`) | alternar no menu; anotar também em qual pasta mudou | ____ |

## Resumo das decisões

| Regra | Teste | Decisão para a versão seguinte |
|---|---|---|
| `jogo-limite-no-jogo-r6` | L3-1, L3-6 | ____ |
| `jogo-limite-launch-source2` | L3-3, L3-5, L3-6 | ____ |
| `jogo-dota2-fps-max-padrao` | L3-5, L3-6 | ____ |
| `jogo-rl-one-frame-thread-lag-off` | L3-4, L3-6 | ____ |
| Fortnite `FrameRateLimit` | captura L3-2 | ____ |
| Rocket League Max FPS | captura | ____ |
| Deadlock `+fps_max` | captura | ____ |
| Chaves de Reflex | captura | ____ |
