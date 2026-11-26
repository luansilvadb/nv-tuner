# Laboratório — atualização automática

Roteiro da tarefa 6.1 de [`atualizacao.md`](atualizacao.md) (§12, §13; decisão A11).
Nenhum lançamento com atualizador sai antes de L-01..L-11 aprovados e registrados aqui.
Os campos **Resultado** ficam em branco até a execução.

> **Pré-requisito de código (atendido).** `nvt-service` tem a feature
> `laboratorio = ["nvt-atualizacao/laboratorio"]`: com ela, manifesto, assinatura e MSI
> são buscados sobre `NVT_ATUALIZACAO_URL_ENSAIO` e verificados com
> `NVT_ATUALIZACAO_CHAVE_ENSAIO` (ambas lidas **em compilação**), e o serviço registra
> ao iniciar `build de laboratório: URL e chave de ensaio`. Sem a feature nada muda
> (U-15a). Build otimizado com a feature falha (`build.rs` + `compile_error!`). Build da
> VM (debug), na raiz da árvore de laboratório:
>
> ```powershell
> $env:NVT_ATUALIZACAO_URL_ENSAIO = 'https://github.com/<conta-de-ensaio>/nv-tuner-ensaio'
> $env:NVT_ATUALIZACAO_CHAVE_ENSAIO = $chaveEnsaio   # 2ª linha de C:\lab\ensaio.pub
> cargo build -p nvt-service -p nvt-ui -p nvt-agente --features nvt-service/laboratorio
> ```
>
> Confira no log do serviço, após instalar, a linha de aviso acima; sem ela o build não
> é de laboratório.

## Ambiente

| Item | Valor |
|---|---|
| VM | Windows 11 x64 descartável (Hyper-V ou similar), conta administradora + uma conta comum, rede com NAT |
| Snapshot | `limpo` (Windows atualizado, sem nv-tuner) e, por caso, um snapshot com a versão base instalada |
| GPU | não exigida (sem NVIDIA o serviço sobe sem driver; o atualizador independe dele) |
| Ferramentas na VM | Rust stable MSVC, .NET SDK 8, Git, GitHub CLI (`gh`), [minisign](https://jedisct1.github.io/minisign/) 0.11+ |
| Repositório de ensaio | fork/repo público `https://github.com/<conta-de-ensaio>/nv-tuner-ensaio` (hosts reais `github.com` → `release-assets.githubusercontent.com`, exercitando U-04) |
| Data da execução | |
| Executor | |
| Commit testado | |

### Montar o servidor de ensaio

1. Chave de ensaio (nunca a oficial; fora do repositório):

   ```powershell
   New-Item -ItemType Directory C:\lab | Out-Null
   cargo run -p nvt-assinar -- gerar-chave --publica C:\lab\ensaio.pub --secreta C:\lab\ensaio.key
   $chaveEnsaio = (Get-Content C:\lab\ensaio.pub)[1]
   ```

2. Repositório de ensaio vazio com releases:

   ```powershell
   gh repo create <conta-de-ensaio>/nv-tuner-ensaio --public --description "ensaio do atualizador"
   ```

3. Árvore de trabalho só do laboratório (as mudanças de versão **não** são commitadas
   no repositório oficial):

   ```powershell
   git worktree add C:\lab\arvore HEAD
   cd C:\lab\arvore
   ```

### Gerar uma versão de laboratório X.Y.Z

A feature `laboratorio` só compila em **debug** (`compile_error!` em release, U-15a), e
o `build-msi.ps1` empacota `target\release`; por isso os executáveis de debug são
copiados para lá e o MSI é gerado com `-SemCompilar`. A versão instalada é a
`CARGO_PKG_VERSION`, então o `Cargo.toml` da árvore de laboratório é alterado.

```powershell
cd C:\lab\arvore
$v = '0.9.0'   # base; depois 0.9.1, 0.9.2…
(Get-Content -Raw Cargo.toml) -replace '(?ms)(^\[workspace\.package\].*?^version\s*=\s*)"[^"]+"', "`$1`"$v`"" | Set-Content -NoNewline -Encoding utf8 Cargo.toml
$env:NVT_ATUALIZACAO_URL_ENSAIO = 'https://github.com/<conta-de-ensaio>/nv-tuner-ensaio'
$env:NVT_ATUALIZACAO_CHAVE_ENSAIO = $chaveEnsaio
cargo build -p nvt-service -p nvt-ui -p nvt-agente --features nvt-service/laboratorio
New-Item -ItemType Directory -Force target\release | Out-Null
Copy-Item target\debug\nvt-service.exe, target\debug\nvt-ui.exe, target\debug\nvt-agente.exe target\release\
powershell -ExecutionPolicy Bypass -File instalador\build-msi.ps1 -SemCompilar
$msi = "target\dist\nv-tuner-$v-x64.msi"
$env:NV_TUNER_MINISIGN_KEY = (Get-Content C:\lab\ensaio.key)[1]
cargo run -p nvt-assinar -- assinar --versao $v --msi $msi
Remove-Item Env:\NV_TUNER_MINISIGN_KEY
minisign -Vm target\dist\atualizacao.json -p C:\lab\ensaio.pub
minisign -Vm $msi -p C:\lab\ensaio.pub
```

(`nvt-assinar verificar` só aceita a chave oficial; no laboratório a conferência é pelo
`minisign`.)

### Publicar no servidor de ensaio

```powershell
$arq = @($msi, "$msi.sha256", "$msi.minisig", 'target\dist\atualizacao.json', 'target\dist\atualizacao.json.minisig')
gh release create "v$v" @arq --repo <conta-de-ensaio>/nv-tuner-ensaio --title "ensaio v$v" --notes "ensaio" --latest
```

Retirar uma versão: `gh release delete "v$v" --repo <conta-de-ensaio>/nv-tuner-ensaio --cleanup-tag --yes`.

### Observar na VM

```powershell
& "$env:ProgramFiles\nv-tuner\nvt-service.exe" status          # como administrador
Get-Content "$env:ProgramData\nv-tuner\logs\nvt-service.$(Get-Date -Format yyyy-MM-dd).log" -Tail 50 -Wait
Get-ChildItem "$env:ProgramData\nv-tuner\atualizacao"            # como administrador
Select-String 'MainEngineThread is returning' "$env:ProgramData\nv-tuner\atualizacao\instalacao-*.log"
Restart-Service nvt-service    # antecipa: a 1ª verificação ocorre 5–15 min após iniciar
```

Antes de cada caso: restaurar o snapshot, instalar a versão base à mão
(`msiexec /i nv-tuner-0.9.0-x64.msi /l*v C:\lab\base.log`) e conferir com `status`.

---

## Casos

### L-01 — Upgrade vN → vN+1 sem UI nem jogo (U-01, U-39)

1. Base 0.9.0 instalada; aplicar um plano (ou aceitar um ajuste do Windows) e anotar os
   valores (Visão geral, `status`).
2. Publicar 0.9.1 no ensaio; `Restart-Service nvt-service`.
3. Esperar até 15 min + download; acompanhar o log.

Esperado: `status` mostra `instalando v0.9.0 → v0.9.1` e depois versão 0.9.1 com
`última atualização: v0.9.0 → v0.9.1 (código 0)`; perfil DRS, ajustes do Windows,
aceites e histórico iguais aos anotados; Windows não reiniciou.

| Campo | Resultado |
|---|---|
| Tempo até instalar | |
| Código no log `/l*v` | |
| Estado preservado (sim/não, diferenças) | |
| Aprovado | |

### L-02 — msiexec sobrevive ao stop do serviço; handle aberto não impede o pacote (U-22, Q1, Q2)

1. Como L-01, com 0.9.2 publicada.
2. Durante a instalação, `Get-Process msiexec` e `Get-Service nvt-service` a cada 2 s.

Esperado: o msiexec continua vivo depois que o serviço para; o log termina com
`MainEngineThread is returning 0`; nenhum erro de abertura do pacote. Se falhar, adotar
a alternativa de U-22 (fechar o handle e recalcular o hash antes do lançamento) e repetir.

| Campo | Resultado |
|---|---|
| msiexec vivo após o stop (sim/não) | |
| Erro de compartilhamento no log (sim/não) | |
| Código final | |
| Decisão Q1/Q2 | |
| Aprovado | |

### L-03 — Agente e UI abertos durante o upgrade; relançamento do agente (U-40..U-44)

1. Base instalada; logon da conta comum no console e de outra conta por RDP; abrir a UI
   nas duas; conferir `nvt-agente.exe` nas duas sessões (`Get-Process nvt-agente -IncludeUserName`).
2. Publicar a versão seguinte e esperar.

Esperado: RM fecha agente e UI sem 3010; a UI que ainda estiver viva mostra “O nv-tuner
foi atualizado. Feche e abra de novo.”; o serviço novo relança o agente **uma vez** em
cada sessão ativa com o mesmo SID (log: `agente relançado na sessão N`); ao reabrir, a
Visão geral mostra “Atualizado para a vX.Y.Z” até **OK**, uma vez por usuário.

| Campo | Resultado |
|---|---|
| Código final (≠ 3010?) | |
| Agente relançado (console / RDP) | |
| Mensagem da UI | |
| Banner uma vez por SID | |
| Aprovado | |

### L-04 — Serviço em job (U-32)

1. Na base, conferir no log do lançamento `breakaway_recusado` e `flags`.

Esperado: registro do caso; se em job, breakaway tentado e, se recusado, lançado sem o
flag (instalação conclui).

| Campo | Resultado |
|---|---|
| `flags` / `breakaway_recusado` | |
| Aprovado | |

### L-05 — Jogo vigiado aberto adia (U-26)

1. Base instalada; `Copy-Item $env:windir\System32\PING.EXE C:\lab\cs2.exe; C:\lab\cs2.exe -t 127.0.0.1`.
2. Publicar a versão seguinte e esperar o download.
3. Depois de ver `pronta, adiada` em `status` e “pendente: jogo aberto” na UI, fechar o `cs2.exe`.

Esperado: nada instalado com o processo aberto; instala em ≤ 60 s após fechá-lo.

| Campo | Resultado |
|---|---|
| Adiou (sim/não) | |
| Segundos até lançar após fechar | |
| Aprovado | |

### L-05b — Jogo aberto no meio do upgrade (U-39a)

1. Como L-05 sem jogo; assim que o msiexec aparecer, abrir o `cs2.exe` falso.

Esperado: o upgrade conclui; nenhuma gravação DRS no log do serviço novo até fechar o processo.

| Campo | Resultado |
|---|---|
| Gravação DRS com o jogo aberto (sim/não) | |
| Aprovado | |

### L-06 — Sem internet, proxy e DNS falho (U-02, U-09)

1. Sem internet: desconectar o adaptador da VM; `Restart-Service nvt-service`; esperar 15 min.
2. DNS falho: `Add-Content $env:windir\System32\drivers\etc\hosts "0.0.0.0 github.com"`; repetir; remover a linha.
3. Proxy: subir um proxy na rede da VM e `netsh winhttp set proxy <host>:<porta>`; repetir;
   `netsh winhttp reset proxy`.

Esperado: nada bloqueia (UI e aplicação normais); `status`/UI mostram “sem conexão com o
GitHub”; próximas tentativas em 30 min, 60 min… (log); com proxy, baixa e instala.

| Campo | Resultado |
|---|---|
| Sem internet: espera registrada | |
| DNS falho | |
| Com proxy | |
| Aprovado | |

### L-07 — Assinatura inválida (U-10..U-14)

Publicar, uma por vez, a versão seguinte com:

1. manifesto assinado por **outra chave** (`gerar-chave` em `C:\lab\outra.*` e `assinar` com ela);
2. assinatura **legada**: `minisign -S -l -s C:\lab\ensaio.key -m atualizacao.json -t "nv-tuner atualizacao formato=1 versao=X.Y.Z"` (exige a chave com cabeçalho completo);
3. **comentário trocado**: `minisign -S -s C:\lab\ensaio.key -m atualizacao.json -t "nv-tuner atualizacao formato=1 versao=9.9.9"`;
4. **MSI com 1 byte alterado** depois de assinado (`$b=[IO.File]::ReadAllBytes($msi); $b[-1]=$b[-1] -bxor 1; [IO.File]::WriteAllBytes($msi,$b)`), manifesto original.

Esperado: nada instalado; 1–3 → `erro_assinatura` sem baixar o MSI; 4 → hash divergente,
`.parcial` apagado; a UI mostra o motivo.

| Variante | Resultado |
|---|---|
| Outra chave | |
| Legada | |
| Comentário trocado | |
| MSI alterado | |
| Aprovado | |

### L-08 — Versão igual e menor recusadas (U-17)

1. Com 0.9.1 instalada, publicar como Latest uma release 0.9.1 (igual) e depois 0.9.0 (menor).

Esperado: `sem_novidade`, nada baixado nem instalado.

| Campo | Resultado |
|---|---|
| Igual | |
| Menor | |
| Aprovado | |

### L-09 — Outra instalação MSI em andamento (U-27, U-37)

1. Iniciar outra instalação e deixá-la parada num diálogo (ex.: `msiexec /i <outro.msi>`
   interativo); publicar a versão seguinte.

Esperado: `adiada_msi_ocupado`, nova tentativa em 30 min, sem contar falha; conclui
depois que a outra instalação termina.

| Campo | Resultado |
|---|---|
| Adiou sem contar falha | |
| Aprovado | |

### L-10 — MSI que falha 3 vezes (U-38, U-19)

1. Na árvore de laboratório, acrescentar ao `nv-tuner.wxs` uma custom action que sempre
   falhe (ex.: `CustomAction` com `Error` dentro de `InstallExecuteSequence`), gerar a
   versão seguinte e publicá-la.
2. Esperar três tentativas (antecipar com `Restart-Service`).
3. Publicar uma versão maior sem a custom action.

Esperado: `falhas` 1, 2, 3 com código 1603; versão em `abandonadas`; a maior volta a
ser tentada e instala.

| Campo | Resultado |
|---|---|
| Três falhas registradas | |
| Abandonada | |
| Versão maior instalada | |
| Aprovado | |

### L-11 — Rollback quando o serviço novo não inicia (Q12, D-09)

1. Gerar a versão seguinte trocando, antes do `build-msi.ps1 -SemCompilar`, o
   `target\release\nvt-service.exe` por uma cópia do `nvt-ui.exe` (não responde ao SCM).
2. Publicar e esperar.

Esperado: `ServiceControl Wait="yes"` falha o MSI, o rollback reinstala e religa o
serviço antigo, que lê o log e conta a falha; a máquina continua com serviço e atualizador.

| Campo | Resultado |
|---|---|
| Código no log | |
| Serviço antigo no ar | |
| Falha contada | |
| Aprovado | |

---

## Encerramento

1. `git worktree remove C:\lab\arvore --force` (descarta a versão alterada).
2. Apagar o repositório de ensaio (`gh repo delete <conta-de-ensaio>/nv-tuner-ensaio`)
   e a chave de ensaio `C:\lab\ensaio.*`.
3. Registrar aqui as decisões de Q1/Q2 e qualquer ajuste na spec.

| Campo | Resultado |
|---|---|
| Todos aprovados | |
| Pendências abertas | |
| Assinatura do executor | |
