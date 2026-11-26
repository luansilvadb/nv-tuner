# Atualização automática do orquestrador — engenharia reversa

Fonte: `github.com/luansilvadb/orquestrador`, commit `6c515083f1b77f47d1bca9171f16e1befff6ff60` (clone raso, só lido). Referências `arquivo:linha` são desse commit. Objetivo: reproduzir a ideia no nv-tuner.

## 1. Fluxo ponta a ponta no app

### Quando verifica

- A cada abertura do app, dentro do "portão" que bloqueia o uso: `crates/app/src/terminal/portao.rs:395-397` (`rodar` → `garantir`), que numa thread chama `crate::atualizacao::rodada` (`portao.rs:88-96`) antes de preparar os CLIs `claude`/`codex`.
- Não há verificação periódica nem em segundo plano. Decisão declarada: se não conseguir verificar ou instalar (por exemplo, sem internet), o app **não abre** (`docs/atualizacao.md:5`; `portao.rs` mostra "Não foi possível atualizar. O uso fica bloqueado até a atualização dar certo.").

### De onde baixa

- Manifesto: `https://github.com/luansilvadb/orquestrador/releases/latest/download/latest.json` (`crates/app/src/atualizacao/mod.rs:10`). É o redirecionamento `latest/download` do GitHub (não a API REST): aponta para o asset `latest.json` da release marcada como *latest* estável. Sem token (repositório público, `docs/atualizacao.md:7`). Rascunhos e pré-releases não aparecem nesse endereço.
- Instalador: a URL vem do próprio manifesto, `https://github.com/luansilvadb/orquestrador/releases/download/vX.Y.Z/Orquestrador_X.Y.Z_x64-setup.exe` (gerada em `crates/app/empacotamento/empacotar.ps1:71` e conferida por `crates/xtask/src/release.rs:438-440,446-477`).
- Canal, chave e versão substituíveis só em compilação, para o teste de ponta a ponta: `ORQ_ATUALIZACAO_URL`, `ORQ_ATUALIZACAO_CHAVE`, `ORQ_VERSAO` via `option_env!` (`mod.rs:14-25`). Um teste garante que o build normal usa os oficiais (`mod.rs:131-134`).

### Formato do manifesto (`latest.json`)

Formato do `tauri-plugin-updater` 2 (estático), herdado porque a v0.1.48 instalada foi feita com Tauri e precisa continuar aceitando os artefatos novos. Gerado por `crates/app/empacotamento/assinador/src/main.rs:73-85`:

```json
{
  "version": "0.2.0",
  "notes": "# Orquestrador v0.2.0\n\n## Resumo\n...",
  "pub_date": "2026-11-15T00:00:00Z",
  "platforms": {
    "windows-x86_64":      { "signature": "<base64 do .sig minisign>", "url": "https://github.com/.../releases/download/v0.2.0/Orquestrador_0.2.0_x64-setup.exe" },
    "windows-x86_64-nsis": { "signature": "<idem>", "url": "<idem>" }
  }
}
```

- `notes` = texto completo de `docs/releases/vX.Y.Z.md` com CRLF→LF (`assinador/src/main.rs:75`); `pub_date` RFC 3339 sem nanossegundos (`:76-80`).
- Leitura no app (`crates/app/src/atualizacao/manifesto.rs:28-65`): aceita `version` ou o alias `name`; procura a plataforma na ordem `windows-x86_64-nsis`, `windows-x86_64` (`manifesto.rs:9,56`); também aceita o formato "dinâmico" do Tauri com `url`/`signature` no topo (`:59-62`). `notes` e `pub_date` são ignorados pelo app.
- Corpo da consulta limitado a 1 MiB (`mod.rs:71`); HTTP 204 = sem atualização (`mod.rs:68-70`, comportamento do Tauri para servidores dinâmicos).

### Comparação de versões

- Crate `semver` 1 (`crates/app/Cargo.toml:27`). Remove `v` inicial e espaços (`manifesto.rs:43-45`). Só atualiza se a publicada for **estritamente maior** que `VERSAO` (`manifesto.rs:51-53`); igual ou menor → `None` (sem downgrade, equivalente a `allowDowngrades: false`). Pré-release semver é comparado normalmente (`manifesto.rs:109-110`), mas o canal só vê releases estáveis.
- No pipeline, a versão nova tem de ser maior que `releases/latest` (`.github/workflows/release.yml:56-66`, comparação com `[version]` do PowerShell, só `X.Y.Z`).

### Download

- HTTP: `ureq` 3 com features padrão (TLS rustls) (`crates/app/Cargo.toml:25`). Um `Agent` por etapa com `timeout_global`: 30 s para a consulta, 15 min para o download inteiro (`mod.rs:28-30,58-64`). Redirecionamentos do GitHub seguidos pelo padrão do ureq.
- Baixa **inteiro para a memória** (`Vec<u8>`), em blocos de 64 KiB, teto de 1 GiB (`mod.rs:34,75-94`); progresso a cada 256 KiB e no fim (`mod.rs:32,88-91`), mostrado na tela do portão como % ou MB (`portao.rs` acima de `:60`).
- Não grava parcial no disco, não retoma download, não confere hash separado: a integridade é só a assinatura.

### Assinatura Minisign

- Crate `minisign-verify` 0.3 no app (`crates/app/Cargo.toml:26`), `base64` 0.22.
- **O que é assinado: só o instalador `.exe`.** O `latest.json` não tem assinatura; ele apenas transporta a assinatura do instalador. A amarração da versão vem do comentário confiável.
- Chave pública embutida como constante (`mod.rs:12`, `PUBKEY_OFICIAL`): base64 do texto do arquivo `.pub` do minisign, ou seja, decodificado fica:
  ```
  untrusted comment: minisign public key: 2BBDB1F87D55B355
  RWRVs1V9+LG9K1aQMiGM8PO2QR7U00BpKhy+5UUP62Tj/dcZOG/NeGJH
  ```
  (mesmo formato do `pubkey` do `tauri.conf.json`; mesma chave do updater da v0.1.48, `crates/app/empacotamento/compat-tauri/updater-0.1.48.json:5`).
- Assinatura (`signature` no manifesto e arquivo `.sig`): base64 do texto completo do `.sig` minisign:
  ```
  untrusted comment: signature from tauri secret key
  RUR...            (assinatura prehashed, algoritmo ED + BLAKE2b)
  trusted comment: timestamp:<unix>\tfile:<nome do exe>\tversion:<X.Y.Z>
  <assinatura global, que cobre a assinatura + o comentário confiável>
  ```
- Verificação (`manifesto.rs:74-84`): decodifica base64 → `PublicKey::decode` / `Signature::decode` → `pk.verify(dados, &sig, true)` (o terceiro argumento é `allow_legacy`, que também aceita assinaturas não pré-hasheadas; o Tauri usa `true`) → extrai `version:` do comentário confiável (separado por tab) e exige igualdade com a versão anunciada (`requireSignedVersion`). Isso impede reaproveitar um instalador antigo, validamente assinado, sob um manifesto que anuncia versão nova (testado em `manifesto.rs:133`).
- Ordem no app: baixa → verifica → só então grava no disco e instala (`mod.rs:50-54`).

### Instalação e reinício

- Grava em `%TEMP%\Orquestrador-<versão>-updater-<pid>\Orquestrador_<versão>_x64-setup.exe` (`mod.rs:98-102`).
- Lança o instalador NSIS com `/S /UPDATE /R` via `std::process::Command::spawn` (sem esperar) (`mod.rs:103`), igual ao `installMode: "quiet"` do Tauri (`updater-0.1.48.json:8`). Em seguida o app chama `std::process::exit(0)` (`mod.rs:55`).
- No instalador (`crates/app/empacotamento/installer.nsi`, modelo do cargo-packager 0.11.8 adaptado):
  - `/S` = silencioso; instalação por usuário (`RequestExecutionLevel user`, `:73-75`; pasta `%LOCALAPPDATA%\Orquestrador`, `:408-410`), sem UAC.
  - `CheckIfAppIsRunning` (`:445-474`) mata `orquestrador.exe` se ainda estiver vivo (cobre a corrida com o `exit(0)`); se não conseguir, aborta.
  - Em modo silencioso, downgrade é abortado (`:422-439`).
  - `/UPDATE` evita recriar atalhos (`:552-559`, ponto marcado "Orquestrador").
  - `/R` faz `.onInstSuccess` executar `$INSTDIR\orquestrador.exe` (`:565-576`): **é o instalador quem reabre o app**, não um processo auxiliar.
  - `hooks.nsh` regrava associações e menus de contexto em HKCU após instalar (`hooks.nsh:13-86`).
- O `.exe` temporário não é apagado.

### Tratamento de falha

- Toda falha vira uma mensagem em português e bloqueia o portão (`mod.rs:45-56,106-124`):
  - timeout → "o GitHub não respondeu a tempo"; DNS/conexão recusada → "sem conexão com o GitHub";
  - 404 na consulta → "nenhuma versão estável publicada foi encontrada" (repositório sem release) — também bloqueia;
  - outro HTTP → "o servidor recusou o download (HTTP n)";
  - assinatura → "a assinatura da atualização não confere, então ela foi recusada"; sem plataforma Windows; manifesto inválido.
- `panic` na thread é capturado (`portao.rs`, `catch_unwind`). Não há retentativa automática; a nova tentativa é na próxima abertura.
- Falha **depois** do `spawn` (instalador abortado, app morto e não reaberto) não é detectada pelo app — fica com o NSIS.

## 2. Pipeline de release

### Disparo e ordem (`.github/workflows/release.yml`)

Push de tag `v*.*.*` (`:7-10`), `windows-latest`, `permissions: contents: write, actions: read` (`:12-14`), concorrência por tag (`:16-18`).

1. **Conferências antes de qualquer build** (`:35-69`): tag `vX.Y.Z`; `cargo xtask release check-versions`; release já publicada para a tag → erro (rascunho é refeito); versão > `releases/latest`; `check-notes` (notas com 7 seções, mínimos de tamanho, sem placeholder).
2. Cache: `Swatinem/rust-cache` com `shared-key: release` e três workspaces (`:74-83`); `cargo-packager` 0.11.8 instalado uma vez e guardado em `actions/cache` (`:86-103`).
3. Compila as duas ferramentas fora do workspace (`:106-110`): `assinador` (assina e gera manifesto) e `compat-tauri` (prova com o updater antigo).
4. **Gera e sobe em rascunho** (`:114-140`): `empacotar.ps1 -Versao -Notas -Saida` com `ORQ_CHAVE_ASSINATURA = secrets.TAURI_SIGNING_PRIVATE_KEY`; cria `gh release create --draft --verify-tag` com `Orquestrador_X.Y.Z_x64-setup.exe`, `.exe.sig` e `latest.json` (ou `upload --clobber` se o rascunho já existe).
5. **Confere o rascunho baixado** (`:145-171`): `gh release download` dos três assets; `xtask release verify-manifest` (versão, URL pública exata, assinatura = conteúdo do `.sig`, notes = notas); extrai `PUBKEY_OFICIAL` do fonte `mod.rs` por regex e roda `assinador verificar` e `compat-tauri verificar` (o `tauri-plugin-updater` 2.14.1 tem de aceitar o instalador e recusar uma cópia com 1 byte alterado, `compat-tauri/src/main.rs:35-51`); `verify-body`.
6. **Publica** (`:174-188`): `xtask release wait-ci <SHA>` espera até 30 min a CI de push em `main` do mesmo SHA ficar verde (`release.rs:237-303`); então `gh release edit --draft=false --latest` e confere que não ficou rascunho/pré-release. Só aqui o `latest/download/latest.json` passa a apontar para a versão nova.

### Empacotamento e assinatura (`crates/app/empacotamento/empacotar.ps1`)

- Compila `orquestrador-app` em release com `ORQ_VERSAO` (`:30-34`), monta config do cargo-packager (`nsis`, `installMode: currentUser`, template próprio) e gera o setup (`:43-64`); renomeia para `Orquestrador_<v>_x64-setup.exe`, nome do tauri-bundler (`:69-70`).
- Assina com a **ferramenta própria** `orquestrador-assinador` (não o `tauri signer`): `assinar <chave> <exe> <versão>` e `manifesto ...` (`:74-78`).
- `assinador` (`assinador/src/main.rs`): crate `minisign` **=0.7.3** fixado (`assinador/Cargo.toml:11-12`, porque 0.7.4 assina com key id errado e 0.7.5+ recusa chave de senha vazia, que é o formato do Tauri CLI). A chave privada é base64 de uma "rsign encrypted secret key" com senha vazia (scrypt); `ORQ_CHAVE_SENHA` opcional (`:55-60`). Comentário confiável `timestamp:…\tfile:…\tversion:…` e não confiável `signature from tauri secret key` (`:62-67`). Grava `<exe>.sig` = base64 do texto da assinatura (`:68-70`).
- Chave privada: `%USERPROFILE%\.tauri\orquestrador-updater.key`, sem senha, com backup; entra no secret por `gh secret set TAURI_SIGNING_PRIVATE_KEY` via stdin (`docs/atualizacao.md:13-21`). Perder a chave = reinstalação manual em todos.

### `cargo xtask release` (`crates/xtask/src/release.rs`)

Comandos (`:505-555`): `wait-ci <SHA>`; `check-versions [X.Y.Z]` (Cargo.toml `[workspace.package]` e Cargo.lock iguais); `check-notes X.Y.Z [anterior]` (7 seções obrigatórias `:10-18`, ≥1500 caracteres no total e ≥60 por seção `:25-26`, sem `TODO/TBD/XXX/{{/<!--` `:20-22`, comparação cita a anterior); `verify-manifest` (`:446-477`); `release-body` (notas sem o H1, `:484-492`); `verify-body` (`:494-499`). `wait-ci` usa `gh run list --workflow ci.yml --commit SHA --branch main --event push`, valida campos e pega a tentativa mais recente (`:250-302`).

### `aquecer-release.yml`

Em push em `main` (e manual): compila exatamente o que a release compila (xtask, app em release, assinador, compat-tauri) com a mesma configuração de `rust-cache` e guarda o `cargo-packager`, porque cache salvo numa tag só é lido pela própria tag e o de `main` é lido por todas (`.github/workflows/aquecer-release.yml:1-3,31-66`). Sem segredos, sem assinatura, sem publicação; fora do `ci.yml` para não atrasar o `wait-ci`.

## 3. Testes

- `manifesto.rs:86-136`, com dados fixos em `crates/app/src/atualizacao/teste/`: `chave.pub` (par minisign descartável, key id `DD9EDAFDAF8F1263`, senha vazia), `dados.bin`, `dados.sig` (assina `dados.bin` com `version:0.2.0`), `dados-0.1.0.sig` (mesmos dados, `version:0.1.0`). Casos:
  - só versão maior atualiza; igual/menor → `None`; `v` inicial e pré-release (`:104-111`);
  - escolha da plataforma Windows / `SemPlataforma` / formato inválido (`:113-119`); manifesto dinâmico (`:121-125`);
  - assinatura válida passa; dados trocados, versão anunciada diferente, assinatura de outra versão reaproveitada e chave oficial (outra chave) falham (`:127-135`).
- `mod.rs:126-185`: canal e chave do build = oficiais e iguais ao `updater-0.1.48.json`, e a chave decodifica para `untrusted comment: minisign public key:` (`:130-147`); mensagens de erro (`:149-155`); download contra servidor TCP local de uma resposta por conexão: progresso começa em 0 e termina no total, ≥3 avisos, HTTP 500 recusado (`:157-184`).
- `assinador/src/main.rs:114-134`: abre uma chave sem senha no formato do Tauri CLI (constante `SK`), assina e verifica com `minisign-verify`.
- `compat-tauri`: binário (não teste unitário) que roda o `tauri-plugin-updater` 2.14.1 real com `MockRuntime` contra os artefatos (`verificar`) ou faz check+download+install (`instalar`) (`compat-tauri/src/main.rs:1-68`).
- `crates/xtask/src/release_testes.rs`: `wait-ci` (só push de `ci.yml` em main para o SHA exato, pendências, tentativa mais recente, prazo), o `release.yml` (o único passo que publica é o que roda `wait-ci` antes, `:103-124`), notas, versões sincronizadas no repositório real, `verify-manifest` (URL de rascunho `api.github.com/.../assets`, assinatura trocada, notes antigas, `:339-350`) com assinaturas sintéticas `sig_for` (`:238-243`, só texto, não criptograficamente válidas: o xtask compara strings, não verifica).
- `crates/app/empacotamento/e2e.ps1`: ponta a ponta manual local com chave temporária e produto `OrquestradorTeste`; servidor HTTP Python; o updater 0.1.48 (compat) instala a `0.2.0-teste.1`, que abre sozinha (`/R`), acha a `.2`, baixa, valida, instala e reabre; confere registro e desinstala (`e2e.ps1:1-4,30-49,87`).

## 4. Decisões e limites declarados

- Atualizar a cada abertura e bloquear o uso se não der (`docs/atualizacao.md:5`).
- Repositório público, sem token; se ficar privado a atualização para (`:7`).
- Sem Authenticode (só Minisign conferida pelo app); sem canal de pré-release (`:9`).
- Sem downgrade; versão publicada nunca é refeita, correção vira `X.Y.(Z+1)` (`docs/releases/README.md:7-9`).
- Falha durante `--draft=false --latest`: não rodar de novo nem mover a tag; conferir `releases/latest` (`README.md:66-70`). Conferir a latest pela API, não por `isLatest` do `gh release view` (`README.md:52`).
- Compatibilidade com o updater Tauri da v0.1.48 é requisito: formato de manifesto, nomes de plataforma, comentário da assinatura, argumentos `/S /UPDATE /R` e nome do instalador foram mantidos por isso.

## 5. O que não se transfere para o nv-tuner (serviço LocalSystem + MSI WiX + agente por usuário)

- **Toda a camada de compatibilidade Tauri**: `compat-tauri`, `updater-0.1.48.json`, plataforma duplicada `windows-x86_64-nsis`, alias `name`, formato dinâmico, texto "signature from tauri secret key", nome do secret `TAURI_SIGNING_PRIVATE_KEY` e a fixação `minisign =0.7.3` (só necessária para ler chave do Tauri CLI com senha vazia; uma chave nova gerada pelo `minisign`/`rsign2` atual dispensa isso).
- **NSIS e seus argumentos**: `/S /UPDATE /R`, `CheckIfAppIsRunning`, `.onInstSuccess` reabrindo o exe e `hooks.nsh` não existem num MSI. O equivalente é `msiexec /i <msi> /qn /norestart` (com log `/l*v`), upgrade via `MajorUpgrade`/`UpgradeCode` do WiX, e o próprio MSI parando/reiniciando o serviço (`ServiceControl`). O instalador não "reabre" o agente: isso cabe ao serviço ou ao Run/agendamento do agente.
- **Quem instala**: no orquestrador, o próprio app do usuário (sem privilégio, instalação em `%LOCALAPPDATA%`) baixa, lança o instalador e sai. No nv-tuner o MSI é por máquina e precisa de elevação: o natural é o **serviço LocalSystem** baixar, verificar e rodar o `msiexec`. Isso exige um diretório não gravável por usuários (ex.: `%ProgramData%\<app>\updates` com ACL restrita ou `%windir%\Temp` do SYSTEM), nunca o `%TEMP%` do usuário, para não abrir TOCTOU/plantio entre verificar e executar. Como o MSI vai parar o próprio serviço, o `msiexec` tem de ser lançado desanexado (processo independente, fora do job object do serviço) antes de o serviço sair.
- **Matar o processo pelo nome** (`KillProcess orquestrador.exe`) não serve com várias sessões: os agentes por usuário precisam ser avisados por IPC para encerrar e o reinício por usuário precisa de mecanismo próprio (o agente relançado no logon ou pelo serviço na sessão).
- **Bloquear o uso sem internet** é decisão do orquestrador; para um serviço de sistema que aplica configuração de GPU não cabe — a falha de atualização deveria ser não bloqueante.
- **Verificação só na abertura da GUI**: o serviço roda sempre; o gatilho natural é periódico/no início do serviço, com o agente só mostrando estado ou pedindo consentimento.
- **Comparação de versão**: MSI usa `ProductVersion` de 3 campos (o 4º é ignorado no upgrade); a regra "estritamente maior" do semver continua útil, mas pré-release semver não se mapeia no `ProductVersion`.
- **Instalação por usuário sem UAC**, chaves HKCU de associação e menus de contexto: específico do orquestrador.
- **Portão dos CLIs `claude`/`codex`** e o `wait-ci` contra o `ci.yml` do orquestrador: específicos; a ideia do portão de CI antes de publicar se transfere.
- O que **se transfere bem**: manifesto `latest.json` em `releases/latest/download`, assinatura Minisign do binário com `version:` no comentário confiável e verificação antes de gravar/executar, chave pública como constante compilada, `option_env!` só para teste E2E, limites de tamanho e prazo, rascunho → conferência dos assets baixados → CI verde → `--latest`, e o `aquecer-release` para cache.

## 6. User-Agent e privacidade

- User-Agent fixo `orquestrador/<CARGO_PKG_VERSION>` (`mod.rs:61`) nas duas requisições. Nota: usa `CARGO_PKG_VERSION`, não `VERSAO` (podem diferir só no build de teste).
- Requisições: um GET ao `latest/download/latest.json` (redireciona para `objects.githubusercontent.com`/`release-assets`) e, se houver versão nova, um GET ao instalador. Sem token, cookies, identificador de máquina, usuário ou telemetria; o GitHub vê IP, horário, User-Agent (com a versão instalada) e quais assets foram baixados (contagem de downloads pública por asset).
- Frequência = uma consulta por abertura do app.
