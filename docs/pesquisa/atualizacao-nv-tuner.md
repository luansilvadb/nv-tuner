# Pesquisa: atualização automática obrigatória e silenciosa do nv-tuner

Data: 2026-11-26. Escopo: o `nvt-service` (LocalSystem) verifica a última release no GitHub, baixa o MSI, confere Minisign e instala, sem bloquear em falha, nunca com jogo aberto, só subindo de versão e preservando o estado. Documento de pesquisa: nenhum código foi alterado.

Base no projeto: `instalador/nv-tuner.wxs` (WiX 5.0.2, `Scope="perMachine"`, `UpgradeCode` fixo `A621CF92-…`, `<MajorUpgrade>` com `DowngradeErrorMessage`, `MSIRESTARTMANAGERCONTROL=0`, `MSIRMSHUTDOWN=0`, ações `RestaurarPerfilOriginal`/`RestaurarAjustesUsuario` condicionadas a `REMOVE="ALL" AND NOT UPGRADINGPRODUCTCODE`, `PrepararDiretorioEstado` com `Return="check"`), `.github/workflows/release.yml` (tag `v*` = versão do `Cargo.toml`, publica MSI + `.sha256` como Latest), `docs/arquitetura.md` (regra "nenhum componente executa processos externos"), `crates/nvt-core/tests/proibicoes.rs`, `crates/nvt-engine/src/portao.rs`.

---

## 1. Como o serviço dispara o upgrade do próprio MSI

### Opções

| Opção | Funciona? | Problema |
|---|---|---|
| `MsiInstallProduct` (msi.dll) dentro do serviço | Não recomendado | A chamada é síncrona no processo do serviço. O MSI executa `StopServices` sobre o `nvt-service`: o SCM envia `SERVICE_CONTROL_STOP` ao próprio processo que está esperando a API. Se o serviço obedece e sai, o cliente da instalação morre no meio (o msiserver continua a sessão de execução, mas perde-se o código de retorno e o log de alto nível; em casos de dependência a instalação pode ficar esperando o stop com timeout e falhar com erro 1920/1921). Se o serviço não obedece, `StopServices` espera e falha. Também carrega a lógica do Windows Installer no processo privilegiado. |
| `msiexec.exe /i … /qn` via `CreateProcessW` destacado | **Recomendado** | O `msiexec` vira um processo independente (sem herança de handles, `DETACHED_PROCESS`/`CREATE_NO_WINDOW`, `CREATE_BREAKAWAY_FROM_JOB` se houver job). O serviço pode ser parado pelo MSI sem afetar a instalação. Exige exceção à regra "nenhum processo externo". |
| Tarefa agendada (Task Scheduler COM, `ITaskService`) | Funciona | Mais superfície (COM, registro de tarefa persistente, limpeza), e na prática a tarefa também executa `msiexec`: é o mesmo processo externo por um caminho indireto e menos auditável. |
| Binário auxiliar "atualizador" copiado para fora de Program Files | Funciona, mas desnecessário | Só faz sentido quando o próprio binário seria substituído enquanto executa a lógica. Aqui quem substitui arquivos é o msiexec; um auxiliar adiciona um executável fora do diretório protegido (risco de plantio/DLL hijack em pasta gravável) sem ganho. |

### Recomendação concreta

1. Serviço baixa e verifica (seções 3–4), depois chama `CreateProcessW` com:
   - `lpApplicationName` = caminho absoluto `%windir%\System32\msiexec.exe` obtido por `GetSystemDirectoryW` (nunca busca no PATH);
   - linha de comando: `msiexec.exe /i "C:\ProgramData\nv-tuner\atualizacao\nv-tuner-X.Y.Z.msi" /qn /norestart REBOOT=ReallySuppress /l*v "C:\ProgramData\nv-tuner\atualizacao\instalacao-X.Y.Z.log"`;
   - `bInheritHandles = FALSE`, `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP`, `lpCurrentDirectory` = `%windir%\System32`, ambiente mínimo. **[Corrigido]** `CREATE_BREAKAWAY_FROM_JOB` só é aceito se o job do chamador tiver `JOB_OBJECT_LIMIT_BREAKAWAY_OK`; não passá-lo incondicionalmente: chamar `IsProcessInJob` e, se o serviço estiver em job, tentar com o flag e registrar a falha. `CREATE_NO_WINDOW` é ignorado junto com `DETACHED_PROCESS` (não combinar os dois). A documentação não diz que o SCM põe serviços em job object nem que mata filhos ao parar o serviço: a sobrevivência do msiexec ao stop do serviço é **inferência não documentada** — validar em VM antes de depender dela. Fontes: https://learn.microsoft.com/windows/win32/procthread/process-creation-flags ; https://learn.microsoft.com/windows/win32/procthread/job-objects
2. Antes de lançar, grava em `%ProgramData%\nv-tuner\atualizacao\estado.json` um registro `{versao_alvo, sha256, inicio, tentativas}` (para o novo serviço, ao iniciar, saber que houve upgrade e ler o resultado).
3. O serviço **não espera** o msiexec terminar (ele vai ser parado). O código de saída é recuperado depois: o novo serviço lê o log `/l*v` (linha `MainEngineThread is returning N` / `Product: … -- Installation completed successfully` / `Installation operation completed successfully`) e o Evento 11707/11708/1033 do log de aplicativo (fonte MsiInstaller). Alternativa opcional: o serviço antigo espera com `WaitForSingleObject` até ser parado; se o msiexec terminar antes (ex.: 1618), registra o código.
4. "Proteção contra o serviço ser morto no meio": como o msiexec é independente, matar o serviço não afeta a instalação. A transação do Windows Installer tem rollback próprio: falha na fase diferida desfaz tudo e o produto antigo continua instalado (inclusive o serviço, que `StartServices`/rollback religa). Com o `MajorUpgrade` padrão do WiX (`Schedule="afterInstallValidate"`) a remoção da versão antiga e a instalação da nova estão na mesma transação, logo o rollback restaura a versão antiga.
5. **[Confirmado no wxs]** já existe `util:ServiceConfig` (reinicia na 1ª e 2ª falha após 5 s) e `ServiceControl Start="install" Stop="both" Remove="uninstall" Wait="yes"`; `Stop="both"` faz o `StopServices` parar o `nvt-service` no upgrade.

### Códigos de saída (Windows Installer)

- `0` ERROR_SUCCESS: sucesso.
- `3010` ERROR_SUCCESS_REBOOT_REQUIRED: sucesso, reinício pendente — **não** reiniciar; registrar e seguir.
- `1641` ERROR_SUCCESS_REBOOT_INITIATED: sucesso com reinício iniciado; não deve ocorrer com `REBOOT=ReallySuppress` (suprime reinícios de `ForceReboot` e do fim da instalação; `/norestart` equivale a ele); se ocorrer, registrar. **[Confirmado]** códigos 0/1602/1603/1618/1619/1625/1638/1641/3010 — https://learn.microsoft.com/windows/win32/msi/error-codes ; https://learn.microsoft.com/windows/win32/msi/reboot ; https://learn.microsoft.com/windows/win32/msi/standard-installer-command-line-options . Atenção: sem opção de reboot, `/quiet` reinicia sem aviso quando necessário — nunca omitir `/norestart`.
- `1618` ERROR_INSTALL_ALREADY_RUNNING: outra instalação em andamento (inclusive Windows Update). Adiar com backoff (ex.: 30 min), sem contar como falha do pacote. Pode-se pré-checar o mutex global `Global\_MSIExecute` (abrir com `OpenMutexW`, só leitura) antes de lançar.
- `1638` outra versão do produto já instalada (seria versão igual com ProductCode diferente; não ocorre com MajorUpgrade, mas tratar como "sem upgrade").
- `1603` falha fatal; `1602`/`1223` cancelado; `1625` proibido por política; `1619` pacote não abre (MSI corrompido/assinatura). Qualquer código ≠ 0/3010: incrementar contador de falhas para aquela versão (seção 6).

### Reinicialização

Sempre `REBOOT=ReallySuppress` + `/norestart`. Com `MSIRESTARTMANAGERCONTROL=0` e `MSIRMSHUTDOWN=0` atuais, o Windows Installer sempre usa o Restart Manager e, em nível de UI silencioso, **sempre fecha** aplicativos e serviços que usam arquivos afetados (UI/agente) — ver seção 2. Nunca `/forcerestart`.

### Conciliação com a regra "nenhum processo externo"

Proposta de exceção mínima e auditável:

- Um único módulo (ex.: `crates/nvt-service/src/atualizacao/lancador.rs`) é o único lugar autorizado a chamar `CreateProcessW`; a função aceita só um tipo `MsiVerificado` (construível apenas pelo verificador Minisign, com caminho dentro da pasta protegida e handle aberto), monta a linha de comando com argumentos fixos e o executável fixo `System32\msiexec.exe`.
- `proibicoes.rs` passa a ter uma lista de permissões com **um** arquivo e **uma** ocorrência (contagem exata), falhando se aparecer em qualquer outro lugar ou se o executável deixar de ser a constante.
- `docs/arquitetura.md` registra a exceção, o motivo e a alternativa rejeitada (`MsiInstallProduct` no serviço).
- Cada lançamento grava no log do serviço: versão de/para, SHA-256, caminho, PID, horário.

Fontes: Command-Line Options (msiexec) — https://learn.microsoft.com/windows/win32/msi/command-line-options ; Standard Installer Command-Line Options — https://learn.microsoft.com/windows/win32/msi/standard-installer-command-line-options ; Error codes — https://learn.microsoft.com/windows/win32/msi/error-codes ; REBOOT property — https://learn.microsoft.com/windows/win32/msi/reboot ; `_MSIExecute` mutex — https://learn.microsoft.com/windows/win32/msi/-msiexecute-mutex ; MsiInstallProductW — https://learn.microsoft.com/windows/win32/api/msi/nf-msi-msiinstallproductw ; CreateProcessW / Process Creation Flags — https://learn.microsoft.com/windows/win32/procthread/process-creation-flags ; Rollback — https://learn.microsoft.com/windows/win32/msi/rollback-installation ; StopServices — https://learn.microsoft.com/windows/win32/msi/stopservices-action .

---

## 2. Upgrade MSI no WiX atual

- **UpgradeCode** fixo e **ProductCode** novo por build (`Id` implícito `*` no WiX v4/v5): condição para major upgrade. Manter.
- **Versão**: o Windows Installer compara só os 3 primeiros campos (`major.minor.build`, máximos 255.255.65535). `build-msi.ps1` já gera 3 campos a partir do `Cargo.toml`. Não usar um 4º campo para diferenciar builds. **[Confirmado]** "uses only the first three fields" — https://learn.microsoft.com/windows/win32/msi/productversion
- **Mesma versão**: com `<MajorUpgrade>` padrão (`AllowSameVersionUpgrades="no"`), instalar a mesma versão com outro ProductCode gera duas entradas no "Aplicativos instalados" (o FindRelatedProducts não detecta). O atualizador deve **comparar versão antes** e só instalar se `remota > instalada` (semver estrito); `AllowSameVersionUpgrades="yes"` blindaria reinstalações manuais, **mas** (doc do WiX) aplica `VersionMaxInclusive` e pode permitir downgrade quando só o 4º campo difere; com 3 campos o risco é nulo, mas a mudança é opcional. **[Confirmado]** padrões `Schedule="afterInstallValidate"` e `AllowSameVersionUpgrades="no"` — https://docs.firegiant.com/wix/schema/wxs/majorupgrade/
- **Downgrade**: `DowngradeErrorMessage` já aborta; em `/qn` resulta em 1603 — o atualizador nunca deve tentar (checar antes).
- **%ProgramData%\nv-tuner**: não é apagada na desinstalação (comentário do wxs) e `PrepararDiretorioEstado` roda em `NOT (REMOVE="ALL")`, ou seja, roda no upgrade e só corrige ACL/dono; o estado é preservado. A subpasta `atualizacao\` precisa obedecer às regras do `prepare-state-dir` (dono Administradores, sem links) para não ser movida para `nv-tuner.inseguro-*`.
- **Ações só de remoção**: as duas condições já têm `NOT UPGRADINGPRODUCTCODE`; no major upgrade a versão antiga é removida com `UPGRADINGPRODUCTCODE` definido, então `RESTAURAR_PERFIL` não roda (além de valer 0 por padrão). Correto.
- **Restart Manager e o agente/UI**: com `MSIRESTARTMANAGERCONTROL=0`/`MSIRMSHUTDOWN=0`, o `InstallValidate` pede ao RM para fechar processos que mantêm arquivos do pacote abertos (`nvt-agente.exe`, `nvt-ui.exe`). Em `/qn` isso acontece sem diálogo. **[Corrigido]** `MSIRESTARTMANAGERCONTROL=0` é o padrão ("sempre tenta usar o Restart Manager"); `MSIRMSHUTDOWN=0` **fecha** os processos mas **não força** — o encerramento forçado de quem não responde é o valor `1`. A documentação afirma: "Silent UI level installations always shut down applications and services". Logo o agente deve tratar fechamento limpo (janela oculta que responda a `WM_QUERYENDSESSION`/`WM_CLOSE`); se não responder, o arquivo fica em uso e o resultado tende a 3010 (substituição no reboot). Fontes: https://learn.microsoft.com/windows/win32/msi/msirestartmanagercontrol ; https://learn.microsoft.com/windows/win32/msi/msirmshutdown ; https://learn.microsoft.com/windows/win32/msi/using-windows-installer-with-restart-manager Como o RM só **relança** aplicações registradas com `RegisterApplicationRestart` e só no contexto interativo, após um upgrade por SYSTEM o agente não volta sozinho. Recomenda-se: agente e UI tratam `WM_ENDSESSION`/fechamento limpo; o agente pode chamar `RegisterApplicationRestart` (ajuda, mas não garantido em instalação por SYSTEM).
- **Relançar o agente na sessão do usuário** (a entrada `HKLM\…\Run` só roda no logon):
  - (a) **Serviço novo relança** via `WTSEnumerateSessionsW` → para cada sessão `WTSActive`, `WTSQueryUserToken` (exige LocalSystem + `SE_TCB_NAME`, que o serviço tem) → `CreateEnvironmentBlock` → `CreateProcessAsUserW` com `lpDesktop="winsta0\\default"` do `nvt-agente.exe` instalado. **[Confirmado com ressalvas]** `WTSQueryUserToken` exige LocalSystem + `SE_TCB_NAME`; `CreateProcessAsUserW` exige `SE_INCREASE_QUOTA_NAME` (LocalSystem tem), não ajusta o ambiente sozinho (usar `CreateEnvironmentBlock` + `CREATE_UNICODE_ENVIRONMENT`; `NULL` herdaria o ambiente do SYSTEM) e não carrega o perfil (o usuário já está logado). Fontes: https://learn.microsoft.com/windows/win32/api/wtsapi32/nf-wtsapi32-wtsqueryusertoken ; https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessasuserw . É a única forma automática e imediata; é um segundo ponto de criação de processo (mesmo módulo de exceção, executável fixo = binário do próprio pacote).
  - (b) Agente se relançar: impossível, pois ele já foi encerrado pelo RM.
  - (c) Aceitar até o próximo logon: zero exceção adicional; o efeito é que a gestão da camada Windows por usuário fica inativa até o logon (o commit 6e49d5d já reidrata o estado no agente, então nada se perde, só atrasa).
  - **Recomendação**: (a) restrito — o serviço novo, ao detectar em `estado.json` que acabou de ocorrer upgrade, relança o agente apenas em sessões ativas que tinham o agente conectado antes (lista gravada pelo serviço antigo antes do upgrade), uma vez. Se for preferível não ampliar a exceção, (c) é aceitável como primeira versão.
- **UI aberta durante o upgrade**: será fechada pelo RM (ou, se mantiver arquivo aberto e recusar, o arquivo é substituído com reinício pendente → 3010). Recomenda-se a UI perceber a queda do pipe IPC, mostrar "nv-tuner foi atualizado; reabra" e sair; não relançar a UI automaticamente.

Fontes: WiX MajorUpgrade — https://wixtoolset.org/docs/schema/wxs/majorupgrade/ ; ProductVersion — https://learn.microsoft.com/windows/win32/msi/productversion ; Major Upgrades — https://learn.microsoft.com/windows/win32/msi/major-upgrades ; UPGRADINGPRODUCTCODE — https://learn.microsoft.com/windows/win32/msi/upgradingproductcode ; Using Windows Installer with Restart Manager — https://learn.microsoft.com/windows/win32/msi/using-windows-installer-with-restart-manager ; MSIRMSHUTDOWN — https://learn.microsoft.com/windows/win32/msi/msirmshutdown ; WTSQueryUserToken — https://learn.microsoft.com/windows/win32/api/wtsapi32/nf-wtsapi32-wtsqueryusertoken ; CreateProcessAsUserW — https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessasuserw ; RegisterApplicationRestart — https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-registerapplicationrestart .

---

## 3. Download seguro como SYSTEM

- **Pasta**: `%ProgramData%\nv-tuner\atualizacao\` criada pelo serviço com DACL protegida (sem herança) só `SYSTEM:F` e `Administradores:F`, dono Administradores, conferida sem seguir junções/reparse points (reaproveitar a lógica do `prepare-state-dir`). Não usar `%windir%\Temp` (gravável por usuários com criação de arquivos; histórico de ataques de plantio) nem `%TEMP%` do SYSTEM.
- **Fluxo anti-TOCTOU**:
  1. Baixar para `nome.msi.parcial` criado com `CreateFileW(CREATE_NEW, FILE_SHARE_READ)` na pasta protegida, com `FILE_FLAG_OPEN_REPARSE_POINT` recusando reparse.
  2. Ao terminar, sem fechar o handle, calcular SHA-256 e verificar Minisign sobre os bytes lidos **do próprio arquivo** (não do buffer de rede).
  3. Renomear (`SetFileInformationByHandle` / `MoveFileExW` sem `REPLACE_EXISTING`) para o nome final; reabrir com `FILE_SHARE_READ` apenas (nega escrita/exclusão) e manter esse handle aberto até o `msiexec` abrir o pacote; o msiexec abre para leitura, compatível com o compartilhamento. Como só SYSTEM/Administradores escrevem na pasta, a janela residual exige já ser administrador.
  4. Limpeza: apagar MSIs e logs antigos, mantendo o último log e o último MSI instalado (útil para reparo; o Windows Installer também guarda cópia em `%windir%\Installer`).
- **Limites**: manifesto ≤ 16 KiB; MSI ≤ 64 MiB (o MSI atual tem poucos MiB) e igual ao `tamanho` declarado no manifesto assinado; abortar ao exceder. Exigir `Content-Length` coerente quando presente.
- **HTTPS**: duas boas opções:
  - **WinHTTP via `windows-sys`** (`WinHttpOpen` com `WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY`, Windows 8.1+ — **[confirmado]**: "uses system and per-user proxy settings"; `DEFAULT_PROXY` está obsoleto): TLS do sistema (Schannel), proxy do sistema/WPAD, sem dependências novas; mais código unsafe.
  - **`ureq` 3.x** (atual 3.5.2, publicada 2026-11-12, ativa) — **[corrigido]**: padrão é `rustls`; `native-tls` exige feature + `TlsProvider::NativeTls` no agente; `platform-verifier` exige `RootCerts::PlatformVerifier`. Proxy: lê só variáveis de ambiente (`ALL_PROXY`/`HTTPS_PROXY`/`HTTP_PROXY`/`NO_PROXY`), que o SYSTEM normalmente não tem; existe a feature `win-system-proxy`, não descrita na página — não confiar sem teste. Fontes: https://docs.rs/ureq/latest/ureq/ ; https://crates.io/crates/ureq `reqwest` traz tokio/hyper — excesso para um serviço que faz uma requisição por dia.
  - **Recomendação**: WinHTTP (sistema cuida de TLS, revogação, proxy corporativo e inspeção TLS com raiz instalada). Se o projeto preferir Rust seguro, `ureq` com `native-tls` (Schannel). Em ambos, só HTTPS, seguir redirecionamentos apenas para `github.com`, `api.github.com`, `objects.githubusercontent.com`/`release-assets.githubusercontent.com` (o download de assets redireciona para esses hosts).
- **Proxy**: como SYSTEM não há proxy do IE do usuário; usar `WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY` (lê configuração WinHTTP `netsh winhttp` e WPAD). Falha de proxy = tentar no próximo ciclo.
- **Timeouts**: resolução/conexão 15 s, envio/recebimento 30 s por operação, teto total do download 10 min; cancelar no stop do serviço.
- **User-Agent**: genérico e sem dados pessoais: `nv-tuner-atualizador/X.Y.Z` (a API do GitHub exige User-Agent). Sem ID de máquina, usuário ou GPU; sem telemetria.

Fontes: WinHttpOpen — https://learn.microsoft.com/windows/win32/api/winhttp/nf-winhttp-winhttpopen ; WinHTTP proxy — https://learn.microsoft.com/windows/win32/winhttp/winhttp-autoproxy-support ; CreateFileW (share mode, reparse) — https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-createfilew ; ureq — https://docs.rs/ureq ; windows-sys — https://docs.rs/windows-sys ; GitHub User-Agent obrigatório — https://docs.github.com/rest/using-the-rest-api/getting-started-with-the-rest-api#user-agent .

---

## 4. Minisign em Rust

- **Crate**: `minisign-verify` (Frank Denis, autor do Minisign), só verificação, **zero dependências**, sem `unsafe` relevante, licença MIT; **[Corrigido]** versão atual **0.3.0** (publicada 2026-09-25), não 0.2.x. Mantida pelo autor do formato; API **[confirmada]**: `PublicKey::from_base64`/`decode`/`from_file`, `Signature::decode`, `pk.verify(&bytes, &sig, allow_legacy)` — usar `allow_legacy = false` (rejeita `Ed` legado); `Signature::trusted_comment()` (no `Signature`); `pk.verify_stream(&sig)` → `StreamVerifier::update/finalize`, só para assinaturas pré-hash. Fontes: https://docs.rs/minisign-verify/latest/minisign_verify/ ; https://crates.io/crates/minisign-verify
- **Chave pública**: texto `untrusted comment: …` + linha base64 de 42 bytes: `Ed` (algoritmo) + key id (8 bytes) + chave Ed25519 (32 bytes). Embutir **só a linha base64** como constante no binário do serviço (não ler de arquivo).
- **Assinatura**: 4 linhas: `untrusted comment`, base64 (`ED` = pré-hash BLAKE2b-512 ou `Ed` legado + key id + assinatura de 64 bytes), `trusted comment: …` e base64 da assinatura global (assina assinatura + trusted comment). O **trusted comment é autenticado**: serve para carregar metadados.
- **O que assinar** (impedir downgrade/replay de MSI antigo assinado):
  - Assinar um **manifesto** `atualizacao.json` (fixo, publicado em toda release): `{"produto":"nv-tuner","versao":"0.3.0","arquivo":"nv-tuner-0.3.0.msi","tamanho":N,"sha256":"…","publicado":"2026-…Z","min_versao_origem":"0.0.1"}` → `atualizacao.json.minisig`.
  - Assinar também o MSI (`nv-tuner-0.3.0.msi.minisig`) com trusted comment `nv-tuner 0.3.0 sha256=…` — defesa em profundidade e verificação manual por usuários.
  - Regras no cliente: rejeitar se `versao` ≤ instalada (anti-downgrade e anti-replay: um manifesto antigo assinado nunca sobe versão), se `produto` ≠ esperado, se SHA-256/tamanho divergirem, se a versão do manifesto não bater com a tag da release consultada e com a `ProductVersion` lida do MSI (opcional, via `MsiOpenDatabase` sem instalar). Guardar em `estado.json` a maior versão já vista para não aceitar regressão mesmo se o estado do instalador for perdido.
  - Algoritmo pré-hash (`ED`, padrão do minisign; `-l` gera o legado) — **[confirmado]**: novas implementações "must use the hashed signature format"; o cliente deve exigir `ED` (`allow_legacy=false`). Fonte: https://jedisct1.github.io/minisign/
- **Chave**: gerar offline: `minisign -G -p nv-tuner.pub -s nv-tuner.key` (ou `rsign generate`, do crate `rsign2`), com senha forte. Guardar a chave secreta (cifrada) + senha em dois secrets do GitHub (`MINISIGN_KEY` em base64, `MINISIGN_PASSWORD`), num **Environment** `release` com aprovação obrigatória e restrito a tags `v*`; backup offline da chave.
- **CI (windows-latest)**: instalar o minisign (release oficial `minisign-win64.zip` de https://github.com/jedisct1/minisign/releases com hash fixado no workflow, ou `cargo install rsign2 --locked --version …`), escrever a chave em `$env:RUNNER_TEMP`, assinar: `minisign -S -s key -m nv-tuner-X.msi -t "nv-tuner X sha256=…"` e o manifesto; **[Parcialmente confirmado]** a doc do minisign não documenta senha por stdin; `-W` cria/usa chave **sem senha**. Opções: chave `-W` protegida só pelo secret do Environment, ou `rsign2` (atual 0.6.7) / crate `minisign` (0.10.0) num pequeno binário de CI. Binário oficial Windows: `minisign-<versão>-win64.zip` nas releases do jedisct1 (também Scoop/Chocolatey). Não confirmado que o runner `windows-latest` traga minisign pré-instalado (página do runner-images indisponível) — assumir que não. Apagar a chave ao final; anexar `.minisig` e `atualizacao.json` à release. Testar no próprio job a verificação com a chave pública do repositório.

Fontes: Minisign — https://jedisct1.github.io/minisign/ ; minisign-verify — https://crates.io/crates/minisign-verify , https://docs.rs/minisign-verify ; rsign2 — https://crates.io/crates/rsign2 ; GitHub Environments/secrets — https://docs.github.com/actions/deployment/targeting-different-environments/using-environments-for-deployment .

---

## 5. GitHub

- `GET https://api.github.com/repos/{owner}/{repo}/releases/latest`: devolve a release publicada mais recente que **não** é draft nem pré-release (marcada Latest). Cabeçalhos: `Accept: application/vnd.github+json`, `X-GitHub-Api-Version: 2022-11-28`, `User-Agent`.
- **[Confirmado]** `releases/latest` = "most recent non-prerelease, non-draft release, sorted by the created_at attribute" (data do commit, não da publicação). **Limite sem token** **[confirmado]**: 60 requisições/hora por IP (compartilhado por NAT/CGNAT); excedido → 403/429. **[Não confirmado]** que `304 Not Modified` não conte: a página atual de rate limits não afirma isso. Tratar `403/429` com `x-ratelimit-reset`/`retry-after`. Nunca embutir token.
- **Alternativa recomendada**: dispensar a API e baixar o manifesto fixo `https://github.com/{owner}/{repo}/releases/latest/download/atualizacao.json` (+ `.minisig`): aponta para o asset da release Latest (**[confirmado]** o formato da URL em linking-to-releases; **[não confirmado]** na doc que fique fora da cota da API REST — é github.com, não api.github.com, mas tratar 403/429 igualmente), e o MSI vem pela URL `…/releases/download/vX.Y.Z/<arquivo>` derivada do manifesto assinado. Usar a API só como fallback/diagnóstico.
- **Intervalo**: primeira verificação 10–30 min após o boot/início do serviço (aleatório), depois a cada **24 h** com jitter de ±2 h (aleatório por máquina, sem identificador persistente enviado). Após falha de rede: backoff 1 h → 2 h → 4 h → … até 24 h. Atualização baixada mas adiada pelo portão: reavaliar o portão a cada 5–10 min, sem rebaixar.

Fontes: Releases API — https://docs.github.com/rest/releases/releases#get-the-latest-release ; Rate limits — https://docs.github.com/rest/using-the-rest-api/rate-limits-for-the-rest-api ; Conditional requests — https://docs.github.com/rest/using-the-rest-api/best-practices-for-using-the-rest-api#use-conditional-requests-if-appropriate ; Linking to releases (`/releases/latest/download/`) — https://docs.github.com/repositories/releasing-projects-on-github/linking-to-releases .

---

## 6. Riscos e mitigações

| Risco | Mitigação |
|---|---|
| Chave Minisign vazada | Chave só no Environment protegido; rotação: embutir **duas** chaves públicas (atual + próxima de reserva guardada offline) e aceitar qualquer uma; release de rotação assinada pela atual remove a vazada. Sem revogação online — documentar que vazamento exige release imediata de rotação. |
| Release maliciosa (conta GitHub comprometida) | Sem a chave Minisign o MSI é rejeitado; 2FA/branch protection/tags protegidas; aprovação manual no Environment. Opcional: assinatura Authenticode do MSI (verificável com `WinVerifyTrust`). |
| MSI corrompido/truncado | Tamanho + SHA-256 do manifesto assinado + Minisign do MSI; arquivo `.parcial` até verificar. |
| Upgrade falha no meio | Transação do Windows Installer faz rollback e mantém a versão antiga (MajorUpgrade `afterInstallValidate`); `PrepararDiretorioEstado` com `Return="check"` aborta antes de deixar estado indefinido. Novo serviço lê log e registra. Se o serviço não voltar (falha fora da transação), ação de recuperação do SCM reinicia. |
| Loop de tentativas | Contador por versão em `estado.json`: após 3 falhas da mesma versão, desistir dela até surgir versão maior (ou 7 dias). 1618 e portão não contam como falha. |
| Máquina sem internet | Falha silenciosa, backoff, nenhuma mensagem intrusiva; UI mostra "última verificação" e o erro no diagnóstico. |
| Atualização durante partida | Consultar `portao.rs` **imediatamente antes** de lançar o msiexec (mesmo padrão do L4 "portão antes de cada gravação"); com jogo/anticheat aberto ou lista ilegível: adiar. Upgrade não deve ocorrer nem com o serviço em meio a uma gravação DRS: adquirir o mesmo lock das gravações. |
| Outra instalação MSI em andamento | Checar `Global\_MSIExecute`; tratar 1618 como adiamento (30 min). |
| Compatibilidade UI/serviço/agente na janela do upgrade | O IPC v5 já tem identidade/versão: o serviço novo aceita clientes da versão anterior durante uma janela (N-1) ou recusa com mensagem clara; UI/agente reconectam com backoff e, se a versão do serviço mudou, a UI pede reabertura e o agente antigo (se sobreviveu) sai para ser relançado (seção 2). Ordem no upgrade: RM fecha UI/agente → arquivos trocados → serviço novo sobe → relança agente. |
| Remoção de versão em uso por política corporativa | Respeitar `DisableMSI`/1625: registrar e parar de tentar. |
| Privacidade | Requisições anônimas, User-Agent genérico, sem IDs; documentar no README que o serviço contata o GitHub uma vez por dia. |

Fontes: Windows Installer rollback — https://learn.microsoft.com/windows/win32/msi/rollback-installation ; WinVerifyTrust — https://learn.microsoft.com/windows/win32/api/wintrust/nf-wintrust-winverifytrust ; Service recovery (`SERVICE_FAILURE_ACTIONS`) — https://learn.microsoft.com/windows/win32/api/winsvc/ns-winsvc-service_failure_actionsw ; WiX util:ServiceConfig — https://wixtoolset.org/docs/schema/util/serviceconfig/ .

---

## Observação sobre a verificação das fontes

Verificação em fontes primárias feita em 2026-11-26 (marcas **[Confirmado]/[Corrigido]/[Não confirmado]** no texto). Pendências: (1) sobrevivência do msiexec ao stop do serviço e existência de job do SCM — não documentado, testar em VM; (2) 304 e `releases/latest/download` fora da cota — não documentados; (3) minisign no runner Windows — não verificado.
