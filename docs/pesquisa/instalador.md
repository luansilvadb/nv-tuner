# Instalador MSI — spike

Pesquisa e protótipo de 2026-11-25. Objetivo: validar que um MSI instala `nvt-ui.exe`
(sem admin no uso) e `nvt-service.exe` (serviço LocalSystem, início automático sem atraso),
cria o atalho no Menu Iniciar e `%ProgramData%\nv-tuner` com ACL própria, e para/remove o
serviço na desinstalação. **O MSI foi gerado e validado (ICE), mas não foi executado.**

## 1. Ferramentas comparadas

| Ferramenta | Base | Licença | Serviço (ServiceInstall/Control) | Situação |
|---|---|---|---|---|
| `cargo-wix` | WiX **3** (`candle`/`light`) | MIT/Apache-2.0 (a ferramenta); WiX 3 MS-RL | Sim, editando o `.wxs` gerado (WiX 3 tem `ServiceInstall`/`ServiceControl`; atraso e recuperação só via `util:ServiceConfig` ou tabelas MSI 5) | WiX 3 descontinuado (última 3.14.x, sem correções). Exige os binários do WiX 3 instalados. Descartado. |
| `cargo-packager` | WiX 3 (baixa o `wix314`) ou NSIS | MIT/Apache-2.0 | Não tem suporte de primeira classe: exige template/fragmentos WiX próprios, ainda sobre WiX 3 | Bom para app simples multiplataforma; para serviço do Windows acaba sendo um `.wxs` escrito à mão com mais uma camada. Descartado. |
| **WiX 5.0.2** (`dotnet tool`) | WiX v4+ (esquema `v4/wxs`) | **MS-RL**, sem aceite de EULA | Nativo: `ServiceInstall`, `ServiceControl`, `ServiceConfig` (DelayedAutoStart), `PermissionEx Sddl`; extensões UI e Util | **Escolhida.** |
| WiX 6.0.x / 7.0.x (`dotnet tool`) | WiX v4+ | Binários sob **OSMF EULA** (`requireLicenseAcceptance=true`; taxa de manutenção para quem obtém receita com o projeto) | Igual ao 5 | Funciona, mas exige aceitar a EULA OSMF; não aceitei em nome do projeto. Migrar é trocar a versão no manifesto (o `.wxs` é o mesmo esquema). |

Por que WiX 5.0.2: é o último WiX moderno sob licença puramente open source (MS-RL), roda como
ferramenta local do .NET (versão fixada em `instalador\.config\dotnet-tools.json`, sem
instalação global nem admin), tem suporte nativo a serviço/ACL por SDDL e é o que o
`windows-latest` do GitHub Actions consegue rodar direto (o runner já traz .NET SDK).

## 2. Estado desta máquina (antes do spike)

- Sem .NET SDK/runtime (`dotnet` inexistente), sem WiX, sem `cargo-wix`/`cargo-packager`.
- Rust 1.100.0 / cargo 1.100.0; Windows Kits presente.

## 3. O que foi instalado (tudo sem admin, nada no sistema)

- **.NET SDK 8.0.428** via `dotnet-install.ps1` em
  `<scratchpad local>\msi-spike\dotnet` (pasta temporária fora do repositório)
  (`-NoPath`; `DOTNET_CLI_HOME` e `NUGET_PACKAGES` também redirecionados para o scratchpad, então
  nada foi gravado em `%USERPROFILE%\.dotnet` nem `%USERPROFILE%\.nuget`). É temporário: some
  numa limpeza do `%TEMP%`.
- **WiX 5.0.2** como ferramenta local (manifesto `instalador\.config\dotnet-tools.json`).
- Extensões **WixToolset.UI.wixext 5.0.2** e **WixToolset.Util.wixext 5.0.2** em
  `instalador\.wix\extensions\` (ignorado por `instalador\.gitignore`).

Para builds locais futuros: instalar o .NET SDK 8+ (por usuário, sem admin:
`dotnet-install.ps1 -Channel 8.0 -InstallDir %LOCALAPPDATA%\Microsoft\dotnet`; ou
`winget install Microsoft.DotNet.SDK.8`, que pede admin) ou apontar `-Dotnet`/`NVT_DOTNET`.

## 4. Arquivos

```
instalador\
  nv-tuner.wxs                 definição do MSI
  build-msi.ps1                cargo build --release + wix build + ICE + SHA256
  .config\dotnet-tools.json    WiX 5.0.2 fixado
  .gitignore                   .wix/ (cache de extensões), obj/
  licencas\licenca.rtf         licença MIT exibida no diálogo do instalador
  licencas\AVISOS-DE-TERCEIROS.txt   NVAPI (MIT), NPI (MIT), GPUI/gpui-component (Apache-2.0)
  licencas\Apache-2.0.txt      texto integral da Apache License 2.0
```

Uso: `powershell -ExecutionPolicy Bypass -File instalador\build-msi.ps1`
(opções: `-Versao`, `-SemCompilar`, `-SemValidar`, `-Dotnet`). A versão vem de
`[workspace.package].version` do `Cargo.toml` (sufixo de pré-release é descartado, pois o MSI
só aceita `major.minor.build`). Saída: `target\dist\nv-tuner-<versão>-x64.msi` e
`...msi.sha256` (formato do `sha256sum`); intermediários e `.wixpdb` em `target\wix\`.

## 5. O que o MSI faz

- **Escopo por máquina**, x64, idioma pt-BR (1046, codepage 1252), Windows 10 build 10240+.
- **UpgradeCode fixo** `A621CF92-2BA5-4F13-B1D8-EB6E6CAFAAB2` (nunca mudar) + `MajorUpgrade`
  (remove a versão anterior; bloqueia downgrade com mensagem em português). ProductCode novo a
  cada build.
- `C:\Program Files\nv-tuner\`: `nvt-service.exe`, `nvt-ui.exe`, `LICENSE.txt`,
  `AVISOS-DE-TERCEIROS.txt`, `Apache-2.0.txt`.
- **Serviço** `nvt-service` (nome exibido "nv-tuner (serviço)", com descrição): `ownProcess`, `auto`,
  conta LocalSystem, `Vital`, argumento `service` (entrada explícita do SCM).
  - Início automático **sem atraso**: tabela `MsiServiceConfig` (ConfigType 3). O WiX emite o
    aviso WIX1149 ("pode não funcionar como esperado", herdado do SDK antigo); a Util não
    oferece DelayedAutoStart. Conferir com `sc qc nvt-service` no teste em VM.
  - **Recuperação em falha** via `util:ServiceConfig` (custom action `Wix4SchedServiceConfig`):
    reinicia na 1ª e 2ª falha após 5 s, nada na 3ª, contador zera em 1 dia.
  - `ServiceControl`: inicia na instalação, para na instalação e na desinstalação (troca de
    binário em upgrade), remove na desinstalação, aguardando.
- **Atalho** "nv-tuner" no Menu Iniciar (todos os usuários), anunciado, apontando para `nvt-ui.exe`.
- **`%ProgramData%\nv-tuner`** com ACL protegida por SDDL (tabela `MsiLockPermissionsEx`,
  independente do idioma do Windows): `D:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)`
  — SYSTEM e Administradores controle total, Usuários leitura/execução, herdado por arquivos e
  subpastas. Na desinstalação a pasta só some se estiver vazia: backup do perfil original e
  histórico gravados pelo serviço **ficam**.
- **Preparação do diretório de estado**: custom action diferida (SYSTEM)
  `nvt-service.exe prepare-state-dir`, depois de `InstallFiles`, com **`Return="check"`**: código de
  saída diferente de 0 aborta a instalação, faz rollback e mostra o erro 1722 com texto próprio em
  português (pasta, comando e como obter o log). Instalar com o diretório num estado desconhecido
  deixaria o serviço parado sem explicação.
- **Restauração do perfil original na desinstalação**: custom action diferida (SYSTEM) via
  `WixQuietExec64` (WixToolset.Util) com `"nvt-service.exe" restore-original --yes`, antes de
  `StopServices`, só se `REMOVE="ALL"`, não em upgrade e com `RESTAURAR_PERFIL=1`.
  **`Return="ignore"`, mas registrado**: o `WixQuietExec64` grava no log do MSI (`msiexec /l*v`) a
  linha de comando, a saída do `nvt-service` e o código de erro. Não usamos `check` porque a falha
  mais provável (driver removido/trocado, snapshot ilegível, DRS ocupado) atinge justamente quem
  quer remover o app; bloquear a desinstalação deixaria o serviço instalado sem saída, e uma falha
  diferida não desfaz o que já foi gravado. O snapshot fica em `%ProgramData%\nv-tuner` (a pasta não
  é apagada), então dá para reinstalar o nv-tuner e restaurar pela interface. Padrão **0** (decisão de produto em aberto). Hoje:
  `msiexec /x <msi> RESTAURAR_PERFIL=1`.
- UI `WixUI_Minimal` em pt-BR (boas-vindas com a licença + progresso).

## 6. Resultado

- `wix build` OK (único aviso: WIX1149, explicado acima); `wix msi validate` (ICE) sem erros nem avisos.
- MSI de 811 008 bytes com os binários placeholder (`cargo build --release -p nvt-service -p nvt-ui`,
  compilados antes de surgir `crates/nvt-engine` sem alvo, que hoje quebra o workspace).
- Tabelas conferidas no MSI original do protótipo (leitura via `WindowsInstaller.Installer`, sem instalar):
  `ServiceInstall`, `ServiceControl`, `MsiServiceConfig` (`nvt-service | 5 | 3 | 1`),
  `Wix4ServiceConfig` (`restart|restart|none`, 1 dia, 5 s), `MsiLockPermissionsEx` (SDDL acima),
  `Shortcut`, `LaunchCondition`, CA `RestaurarPerfilOriginal` (seq. 1899, antes de `StopServices` 1900).
- **Não executado.** Com o placeholder atual o MSI falharia ao instalar: `nvt-service.exe` ainda
  não fala com o SCM, então `StartServices` daria erro 1920 e o `Vital` faria rollback.

## 7. Próximos passos

1. `nvt-service`: implementar o service dispatcher com o nome **`nvt-service`** (contrato com o
   `.wxs`), parada limpa em `SERVICE_CONTROL_STOP`/`SHUTDOWN` dentro do timeout do SCM, e o
   subcomando `restore-original` (idempotente, código de saída 0 quando não há backup).
2. Decidir o padrão de `RESTAURAR_PERFIL` (ou criar um diálogo de desinstalação perguntando;
   a desinstalação pelo "Aplicativos instalados" não passa propriedades, então o padrão vale para
   quase todos os usuários).
3. Conferir a linha `nvt-service | 5 | 3 | 0` na tabela `MsiServiceConfig` do novo MSI via
   `WindowsInstaller.Installer` em modo somente leitura. Em VM descartável, conferir que
   `sc qc nvt-service` mostra `AUTO_START` sem `(DELAYED)` na instalação limpa e no upgrade por cima
   (versão maior). Verificar também `sc qfailure nvt-service`, `icacls %ProgramData%\nv-tuner`, atalho,
   downgrade bloqueado, desinstalação com e sem `RESTAURAR_PERFIL=1` e logs com `msiexec /l*v`.
4. Ícone (`Icon` + `ARPPRODUCTICON`) e `ARPURLINFOABOUT` quando houver repositório público.
5. Avisos de terceiros: gerar a lista das crates transitivas (`cargo about`/`cargo deny`) e
   reconferir anos de copyright e arquivos NOTICE de Zed/gpui-component antes do primeiro release.
6. Sem assinatura: SmartScreen vai alertar. O SHA256 publicado no release é a verificação por
   ora; avaliar certificado (ou Azure Trusted Signing) depois.
7. **CI no GitHub Actions**: implementado em `.github/workflows/release.yml`. Em push de tag `v*`
   (`windows-latest`): confere que a tag bate com a versão do `Cargo.toml`, instala o .NET 8
   (`actions/setup-dotnet`), `dotnet tool restore` em `instalador\`, `cargo build --release`,
   `build-msi.ps1 -SemCompilar` (que acha o `dotnet` do `PATH`; inclui ICE), confere o SHA256 e
   cria uma GitHub Release **pre-release** com o MSI e o `.sha256` via `gh` CLI (permissão
   `contents: write` só nesse job). O `windows-latest` traz o Windows SDK (`fxc.exe` do GPUI).
   Pendente: fixar as actions por SHA. O atualizador do app (futuro) baixa o MSI do release,
   confere o `.sha256` e chama `msiexec /i` (o `MajorUpgrade` cuida da troca).
