# Laboratório da Fase 2 — roteiro e registro

Roteiro para o mantenedor executar e **preencher** os testes de laboratório
L2-1 a L2-6 da [especificação competitiva](competitivo.md) (§8 e tabela de
laboratório da Fase 2) e os testes da Fase 1 que não dependem de G-SYNC
(L1-1, L1-4, L1-6, L1-8). L1-2, L1-3, L1-5 e L1-7 exigem G-SYNC e ficam fora
deste registro: o mantenedor não usa G-SYNC, então as regras
`sincronismo-gsync-limite` e `gsync-reflex-sem-limite-driver` continuam
`experimental`.

Os campos `____` ficam em branco até a execução. Não registre dados pessoais
(nome de usuário, caminhos com o nome da conta, números de série).

## 1. Ambiente

| Item | Valor |
|---|---|
| Data da execução | ____ |
| Versão / commit do nv-tuner | ____ |
| MSI usado (nome e SHA256) | ____ |
| Windows (edição, versão, build) | ____ |
| GPU e versão do driver NVIDIA | ____ |
| CPU | ____ |
| Monitor(es): modelo, resolução, taxas disponíveis | ____ |
| Segundo monitor disponível para L2-1 multimonitor? | ____ |
| Mouse (modelo, polling) | ____ |
| Contas de teste (quantas, sem nomes) | ____ |
| Acesso RDP disponível? | ____ |
| Ferramentas: Process Explorer (versão), PresentMon (versão), GPU-Z ou NVML | ____ |

**Estado inicial real da máquina do mantenedor** (leitura antes dos testes):
monitor já a **240 Hz**, mouse **sem aceleração**, token
`SwapEffectUpgradeEnable=1` em `DirectXUserGlobalSettings` e
`HistoricalCaptureEnabled=0`. Nesse estado nenhuma das quatro regras casa,
então **L2-1 a L2-4 exigem primeiro provocar o estado** que a regra corrige
(pró-forma: pôr 60 Hz, ligar a aceleração, pôr o token em 0 ou removê-lo,
ligar a captura histórica). Anote o valor original de cada item para
devolvê-lo ao fim, independentemente do app:

| Item | Valor original lido | Valor ao fim dos testes |
|---|---|---|
| Taxa do monitor principal (racional, ex.: 239,760 ou 240) | ____ | ____ |
| “Aumentar precisão do ponteiro” | ____ | ____ |
| `HKCU\Software\Microsoft\DirectX\UserGpuPreferences` → `DirectXUserGlobalSettings` (texto completo) | ____ | ____ |
| `HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR` → `HistoricalCaptureEnabled` | ____ | ____ |

Leitura sugerida do registro (só leitura, PowerShell):

```powershell
Get-ItemProperty 'HKCU:\Software\Microsoft\DirectX\UserGpuPreferences' -Name DirectXUserGlobalSettings -ErrorAction SilentlyContinue
Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\GameDVR' -Name HistoricalCaptureEnabled -ErrorAction SilentlyContinue
```

## 2. Preparação comum

### 2.1 Instalar o MSI

1. Gere o pacote (ou baixe o do CI) e confira o hash:

   ```powershell
   powershell -ExecutionPolicy Bypass -File instalador\build-msi.ps1
   (Get-FileHash .\target\dist\nv-tuner-<versão>-x64.msi -Algorithm SHA256).Hash.ToLower()
   Get-Content .\target\dist\nv-tuner-<versão>-x64.msi.sha256
   ```

2. Instale com log (o build de desenvolvimento não é assinado; o SmartScreen
   pode pedir **Mais informações → Executar assim mesmo**):

   ```powershell
   msiexec /i .\target\dist\nv-tuner-<versão>-x64.msi /l*v instalacao.log
   ```

3. Confira:
   - serviço `nvt-service` (“nv-tuner (serviço)”) em execução em
     `services.msc`;
   - entrada `nv-tuner-agente` em
     `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run`;
   - `nvt-agente.exe` em execução na sua sessão depois de um logoff/logon
     (Gerenciador de Tarefas → Detalhes, coluna Sessão);
   - na interface, aba **Windows**: o cabeçalho diz que o agente está ativo.

| Verificação | Resultado |
|---|---|
| Hash confere | ____ |
| Serviço em execução | ____ |
| Entrada Run presente | ____ |
| Agente ativo após logon | ____ |
| UI mostra agente ativo | ____ |

### 2.2 Aceitar uma regra

Na aba **Windows → Experimentais**, a regra só aparece com **Aceitar**
quando sua condição casa (por isso o estado precisa ser provocado antes).
Depois de aceitar, ela exibe “experimental aceita por você” e o botão
**Retirar**. O agente age no próximo contato com o serviço (até ~60 s, ou
logo após uma mudança de fatos).

### 2.3 Verificar handles com Process Explorer

Use o Process Explorer (Sysinternals) **elevado**:

1. **View → Lower Pane View → Handles** (Ctrl+H).
2. Selecione `nvt-service.exe` na árvore. No painel inferior, ordene pela
   coluna **Type** e procure entradas do tipo **Process** e **Thread**. Não
   pode haver nenhuma apontando para um processo de jogo ou de anticheat
   (`cs2.exe`, `FortniteClient-Win64-Shipping.exe`, `RainbowSix*.exe`,
   `League of Legends.exe`, `EasyAntiCheat*.exe`, `BEService*.exe`,
   `vgc.exe` etc.).
3. Repita selecionando `nvt-agente.exe`.
4. Confirmação cruzada: **Find → Find Handle or DLL** (Ctrl+F), digite o
   nome do executável do jogo (ex.: `cs2.exe`) e verifique que nenhum
   resultado tem `nvt-service.exe` ou `nvt-agente.exe` na coluna Process.
5. Faça a verificação com o jogo em partida, não só no menu.

### 2.4 Promover uma regra (experimental → estável)

Uma regra só é promovida com o teste correspondente **aprovado** neste
registro (T-03 da spec):

1. Em `regras/windows.toml` (ou `regras/base.toml`, para regras de driver),
   troque `status = "experimental"` por `status = "estavel"`.
2. Acrescente a evidência no campo `evidencia` sem dados pessoais, por
   exemplo: `"… Laboratório L2-1 (docs/spec/fase2-laboratorio.md, <data>):
   60→240 Hz com racional igual ao do Windows; reversão exata."`.
3. Rode `cargo test -p nvt-core` (o teste `carrega_regras_reais` valida o
   TOML), `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo fmt --check` e `cargo test --workspace`.
4. Registre a promoção no `CHANGELOG.md` (seção **Alterado**) e na tabela
   da §5 deste arquivo.
5. Efeito: a regra passa a entrar no plano **sem aceite** para todos os
   usuários. Aceites já gravados continuam no `usuarios.json`, sem efeito
   adicional.

## 3. Testes da Fase 2

### L2-1 — Taxa de atualização no máximo (`windows-refresh-maximo`)

- **Pré-condições:** nenhum jogo nem anticheat aberto; agente ativo; valor
  original anotado na §1.
- **Passos:**
  1. Em **Configurações → Sistema → Tela → Tela avançada**, ponha o monitor
     principal em **60 Hz** (mesma resolução).
  2. Na aba **Windows → Experimentais**, confirme que
     `windows-refresh-maximo` aparece e clique **Aceitar**.
  3. Aguarde até 60 s. Leia a taxa em **Tela avançada** e na **Visão
     geral** do app.
  4. Em **O que o app fez**, clique **Reverter** no ajuste de taxa.
  5. Leia a taxa novamente.
  6. Repita os passos 1–3 com um jogo aberto (ex.: LoL no cliente): a
     troca **não** pode acontecer até fechar o jogo.
  7. (Se houver) Multimonitor: repita 1–5 com dois monitores e confirme que
     só o principal muda e a topologia é preservada.
  8. (Se houver) Modo personalizado do Painel NVIDIA: repita e anote o
     comportamento de `SetDisplayConfig`.
  9. **Retire** o aceite e confirme a volta ao valor de antes da gestão.
- **Observar:** taxa racional após aplicar (igual à que o Windows oferece
  para 240 Hz); resolução e profundidade inalteradas; histórico com a
  entrada da camada Windows; nenhum piscar extra além da troca de modo.
- **Aprovação:** vai a 240 Hz com o mesmo racional do Windows; **Reverter**
  volta exatamente a 60 Hz; nada muda com jogo aberto; topologia e
  resolução preservadas.

| Campo | Resultado |
|---|---|
| Taxa após aplicar (racional) | ____ |
| Taxa após reverter | ____ |
| Adiou com jogo aberto? | ____ |
| Multimonitor | ____ |
| Modo personalizado | ____ |
| Retirada do aceite | ____ |
| Falhas / observações | ____ |
| **Aprovado?** | ____ |

### L2-2 — Aceleração do mouse (`windows-mouse-sem-aceleracao`)

- **Pré-condições:** sem jogo aberto; valor original anotado.
- **Passos:**
  1. Em **Painel de Controle → Mouse → Opções do ponteiro**, ligue
     **Aumentar precisão do ponteiro** e anote a velocidade do ponteiro.
  2. Aceite `windows-mouse-sem-aceleracao` na aba Windows; aguarde.
  3. Reabra **Opções do ponteiro**: a caixa deve estar desmarcada e a
     velocidade, igual.
  4. Abra o LoL (cursor do sistema) e avalie o cursor em menus/partida de
     treino.
  5. Feche o jogo, clique **Reverter**; confirme que a opção volta a ligar
     **sem logoff**.
- **Observar:** efeito imediato, sem logoff; sensibilidade inalterada.
- **Aprovação:** desliga e religa sem logoff, sensibilidade intocada,
  cursor consistente.

| Campo | Resultado |
|---|---|
| Desligou sem logoff | ____ |
| Velocidade preservada | ____ |
| Sensação no LoL | ____ |
| Reverter religou | ____ |
| Falhas / observações | ____ |
| **Aprovado?** | ____ |

### L2-3 — Otimizações para jogos em janela (`windows-otimizacoes-janela`)

- **Pré-condições:** Windows 11 (build ≥ 22000); PresentMon instalado; sem
  jogo aberto ao aceitar; texto original de `DirectXUserGlobalSettings`
  anotado.
- **Passos:**
  1. Provoque o estado: em **Configurações → Sistema → Tela → Elementos
     gráficos → Configurações gráficas padrão**, desligue **Otimizações para
     jogos em janela** (o token vai a `SwapEffectUpgradeEnable=0`). Se
     houver outros tokens no valor, anote-os.
  2. Rocket League e LoL em **janela sem bordas** (DX11): capture com
     PresentMon a coluna `PresentMode` e, se disponível, entrada→fóton.
  3. Feche os jogos; aceite `windows-otimizacoes-janela`; aguarde.
  4. Leia `DirectXUserGlobalSettings`: só `SwapEffectUpgradeEnable` mudou
     para 1; outros tokens e sua ordem intactos.
  5. Repita a captura do passo 2.
  6. Clique **Reverter** e confira que o texto volta ao original (inclusive
     ausência do valor, se era ausente).
- **Observar:** `PresentMode` de `Composed: Copy…` para
  `…Independent Flip`; latência.
- **Aprovação:** `PresentMode` passa a flip independente nos dois jogos,
  entrada→fóton igual ou menor, outros tokens preservados, reversão exata.

| Campo | Resultado |
|---|---|
| Build do Windows | ____ |
| `PresentMode` antes (RL / LoL) | ____ |
| `PresentMode` depois (RL / LoL) | ____ |
| Entrada→fóton antes/depois | ____ |
| Outros tokens preservados | ____ |
| Reversão exata | ____ |
| Falhas / observações | ____ |
| **Aprovado?** | ____ |

### L2-4 — Captura histórica (`windows-captura-historica-off`)

- **Pré-condições:** sem jogo aberto; valor original anotado (hoje 0).
- **Passos:**
  1. Em **Configurações → Jogos → Capturas**, ligue **Gravar o que
     aconteceu** (o valor vai a 1).
  2. Aceite `windows-captura-historica-off`; aguarde.
  3. Leia `HistoricalCaptureEnabled` (deve ser 0) e confira a opção nas
     Configurações.
  4. Abra o Game Bar (Win+G) e faça uma captura manual: deve funcionar.
  5. Clique **Reverter**: o valor volta a 1.
- **Aprovação:** valor volta a 0, Game Bar continua funcionando, reversão
  restaura 1 (ou a ausência, se era ausente).

| Campo | Resultado |
|---|---|
| Valor após aplicar | ____ |
| Game Bar funciona | ____ |
| Valor após reverter | ____ |
| Falhas / observações | ____ |
| **Aprovado?** | ____ |

### L2-5 — `useplatformclock` (somente diagnóstico)

- **Pré-condições:** nenhuma alteração no BCD durante o teste.
- **Passos:**
  1. Antes, como administrador, registre `bcdedit /enum {current}` (só
     leitura) e guarde a saída fora do repositório.
  2. Abra **Windows → Diagnóstico** e anote o que aparece para
     `useplatformclock` (valor lido ou orientação estática) e para TPM,
     Secure Boot, VBS/HVCI e DRR.
  3. Se possível, simule a indisponibilidade da leitura (ex.: conta sem
     acesso ou serviço WMI parado temporariamente) e anote o resultado.
  4. Depois, repita `bcdedit /enum {current}` e compare.
- **Aprovação:** diagnóstico compatível com a leitura real ou orientação
  estática; BCD idêntico antes e depois; nenhum botão de alteração.

| Campo | Resultado |
|---|---|
| Apresentação de `useplatformclock` | ____ |
| TPM / Secure Boot / VBS-HVCI exibidos | ____ |
| DRR (esperado: indisponível sem fonte confiável) | ____ |
| BCD idêntico | ____ |
| Falhas / observações | ____ |
| **Aprovado?** | ____ |

### L2-6 — Anticheat, sessões e instalador

- **Pré-condições:** serviço e agente ativos; pelo menos uma regra Windows
  aceita com estado provocado (pendente de aplicar); Process Explorer
  elevado.
- **Passos — jogos:** para cada jogo (CS2/VAC, Fortnite/EAC+BE,
  R6/BattlEye, LoL/Vanguard):
  1. Abra o jogo e entre numa partida (ou treino).
  2. Durante a partida, provoque uma mudança que o app corrigiria e peça
     **Aplicar agora** e **Restaurar original**: nada pode ser gravado; a
     UI mostra a pendência.
  3. Verifique handles de `nvt-service` e `nvt-agente` (§2.3).
  4. Com dois agentes (duas contas logadas, troca rápida de usuário),
     confirme que a UI não trava.
  5. Feche o jogo e confirme que a pendência é executada.
- **Passos — sessões:** troca de usuário; console ↔ RDP (aceite de driver
  fica inativo fora do console; a camada Windows usa os aceites do próprio
  SID); logoff com restauração pendente (deve restaurar no próximo logon);
  agente encerrado manualmente (UI mostra ausente; presença expira em
  ~3 min); reinício do serviço no meio de uma restauração (geração
  retomada).
- **Passos — instalador:**
  1. Reinstalação/upgrade por cima: nada é restaurado; agente fechado pelo
     Restart Manager sem término forçado; ajustes continuam.
  2. Desinstalação padrão (`RESTAURAR_PERFIL=0`): driver e Windows ficam
     como estão; entrada Run removida.
  3. Desinstalação `RESTAURAR_PERFIL=1` com jogo aberto: log `/l*v` registra
     restauração bloqueada; remoção conclui; backups preservados.
  4. Desinstalação `RESTAURAR_PERFIL=1` com jogo fechado: driver e Windows
     do usuário que desinstala voltam ao original. Outro usuário logado:
     seus ajustes **não** são restaurados e seu agente não é fechado pelo
     Restart Manager (documentado no README).

  ```powershell
  msiexec /x .\target\dist\nv-tuner-<versão>-x64.msi RESTAURAR_PERFIL=1 /l*v desinstalacao.log
  ```

- **Aprovação:** nenhum erro de anticheat; nenhum handle de
  `nvt-service`/`nvt-agente` para processos de jogo; nenhuma gravação com
  jogo aberto; UI responsiva com dois agentes; comportamentos de sessão e
  instalador conforme descrito.

| Jogo | Erro de anticheat? | Handles do serviço | Handles do agente | Gravou com jogo aberto? |
|---|---|---|---|---|
| CS2 | ____ | ____ | ____ | ____ |
| Fortnite | ____ | ____ | ____ | ____ |
| Rainbow Six Siege | ____ | ____ | ____ | ____ |
| League of Legends | ____ | ____ | ____ | ____ |

| Cenário | Resultado |
|---|---|
| Dois agentes, UI responsiva | ____ |
| Troca de usuário | ____ |
| Console ↔ RDP | ____ |
| Logoff com restauração pendente | ____ |
| Agente ausente | ____ |
| Retomada de geração após reinício | ____ |
| Upgrade | ____ |
| Desinstalação padrão 0 | ____ |
| Desinstalação 1, jogo aberto | ____ |
| Desinstalação 1, jogo fechado | ____ |
| Outro usuário na desinstalação | ____ |
| Falhas / observações | ____ |
| **Aprovado?** | ____ |

## 4. Testes da Fase 1 sem G-SYNC

### L1-1 — Refresh e VRR lidos pelo serviço (sessão 0)

- **Passos:** com o serviço instalado, compare `refresh_mhz` e VRR da
  **Visão geral** com o Painel NVIDIA; troque o monitor para 144 Hz (ou
  outra taxa) no Windows, peça **Reescanear** e compare de novo; volte a
  240 Hz.
- **Aprovação:** valores iguais aos do painel; a troca aparece na próxima
  detecção.

| Campo | Resultado |
|---|---|
| Refresh/VRR na UI × painel (240 Hz) | ____ |
| Após trocar para outra taxa | ____ |
| **Aprovado?** | ____ |

### L1-4 — Baixa latência Ultra × On com limite ativo

- **Passos:** Rocket League (DX11, sem Reflex) com Max FPS no jogo abaixo do
  que a GPU sustenta; meça entrada→fóton (média e p99) e frametime p99 com
  Ultra e com On, várias sessões de cada, intercaladas.
- **Aprovação:** registrar; a regra global só muda se On vencer com margem
  maior que a variação entre sessões.

| Campo | Resultado |
|---|---|
| Ultra: entrada→fóton média / p99, frametime p99 | ____ |
| On: entrada→fóton média / p99, frametime p99 | ____ |
| Variação entre sessões | ____ |
| Conclusão | ____ |

### L1-6 — Adiamento e ausência de interação (driver)

- **Passos:** com CS2, Fortnite, R6 e LoL (um por vez), mude uma setting no
  Painel NVIDIA com o jogo aberto; verifique handles do `nvt-service`
  (§2.3) e o log do serviço; feche o jogo.
- **Aprovação:** nenhum handle do serviço para processo de jogo; a mudança
  manual só é tratada após fechar; sem erro de anticheat.

| Jogo | Handles | Tratada só após fechar | Erro de anticheat |
|---|---|---|---|
| CS2 | ____ | ____ | ____ |
| Fortnite | ____ | ____ | ____ |
| Rainbow Six Siege | ____ | ____ | ____ |
| League of Legends | ____ | ____ | ____ |

### L1-8 — Energia por jogo

- **Passos:** com navegador reproduzindo vídeo, observe clock/P-state da GPU
  em idle (GPU-Z ou NVML); depois abra um jogo competitivo da base e
  observe.
- **Aprovação:** em idle com navegador a GPU desce de P-state; no jogo, o
  comportamento é o de antes.

| Campo | Resultado |
|---|---|
| P-state em idle com vídeo | ____ |
| P-state/clock no jogo | ____ |
| **Aprovado?** | ____ |

## 5. Conclusão e promoções

| Regra | Teste | Aprovado? | Promover para `estavel`? | Commit da promoção |
|---|---|---|---|---|
| `windows-refresh-maximo` | L2-1 | ____ | ____ | ____ |
| `windows-mouse-sem-aceleracao` | L2-2 | ____ | ____ | ____ |
| `windows-otimizacoes-janela` | L2-3 | ____ | ____ | ____ |
| `windows-captura-historica-off` | L2-4 | ____ | ____ | ____ |
| `sincronismo-gsync-limite` | L1-2, L1-3 (G-SYNC) | não executado | não | — |
| `gsync-reflex-sem-limite-driver` | L1-5 (G-SYNC) | não executado | não | — |

Falhas encontradas e ações abertas:

- ____

Ao fim, devolva os quatro itens da §1 ao valor original e anote na coluna
“Valor ao fim dos testes”.
