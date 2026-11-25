<p align="center"><img src="assets/logo.svg" alt="Logo do nv-tuner" width="96" height="96"></p>

# nv-tuner

Ajuste automático do perfil do driver NVIDIA no Windows — global e por jogo —
a partir de regras declarativas sobre as capacidades da sua GPU, os fatos de
cada jogo instalado e as suas preferências.

> ⚠️ **Aviso:** o nv-tuner altera configurações do perfil do driver NVIDIA
> (DRS) do seu sistema. Antes da primeira mudança ele guarda um snapshot dos
> valores originais, e tudo o que alterou pode ser desfeito em
> **Restaurar original** (veja [Instalação](#instalação)).

## Status

**Pré-alfa (v0.1).** Serviço, interface, motor de regras e instalador já
funcionam, mas o projeto foi testado em pouquíssimas máquinas (uma GPU Turing,
um driver). Espere bugs, mudanças de formato entre versões e regras ainda
`experimental`. Use por sua conta e risco, de preferência sabendo restaurar o
perfil do driver.

- O MSI **não é assinado** digitalmente (veja o aviso do SmartScreen abaixo).
- Os lançamentos são publicados como *pre-release*.
- Não há atualização automática ainda: baixe a nova versão e instale por cima
  (o instalador substitui a anterior).

## Instalação

Requisitos: Windows 10 ou 11 de 64 bits, GPU NVIDIA com o driver instalado e
conta de administrador (o instalador pede elevação pelo UAC).

1. Baixe `nv-tuner-<versão>-x64.msi` e `nv-tuner-<versão>-x64.msi.sha256` na
   página de [Releases](../../releases).
2. Confira o SHA256 no PowerShell, na pasta do download:

   ```powershell
   (Get-FileHash .\nv-tuner-<versão>-x64.msi -Algorithm SHA256).Hash.ToLower()
   Get-Content .\nv-tuner-<versão>-x64.msi.sha256
   ```

   Os dois valores precisam ser idênticos; se não forem, não instale.
3. Execute o MSI. Como ele não é assinado, o Windows SmartScreen pode mostrar
   “O Windows protegeu o computador”: clique em **Mais informações** →
   **Executar assim mesmo** (só depois de conferir o SHA256).
4. O instalador registra o serviço `nv-tuner (serviço)` e cria o atalho
   **nv-tuner** no Menu Iniciar. A interface roda sem privilégios.

### Desinstalar

Em **Configurações → Aplicativos → Aplicativos instalados → nv-tuner →
Desinstalar**, ou `msiexec /x nv-tuner-<versão>-x64.msi`.

A desinstalação **não** desfaz as mudanças no perfil do driver por padrão.
Para voltar aos valores originais:

- antes de desinstalar, use **Restaurar original** na interface; ou
- desinstale pela linha de comando (como administrador), com o mesmo MSI:

  ```powershell
  msiexec /x nv-tuner-<versão>-x64.msi RESTAURAR_PERFIL=1 /l*v desinstalacao.log
  ```

  Se a restauração falhar, a desinstalação continua mesmo assim, e o motivo
  fica registrado no log. Os dados do app (snapshot e histórico) ficam em
  `%ProgramData%\nv-tuner`: reinstalando o nv-tuner, **Restaurar original**
  volta a funcionar.

## Como reportar problemas

Abra uma [issue](../../issues) com:

- versão do nv-tuner, modelo da GPU, versão do driver e do Windows;
- o que você fez, o que esperava e o que aconteceu;
- os avisos da tela **Visão geral** e, se for o caso, o log do instalador
  (`/l*v`).

Revise o que for anexar: **não inclua dados pessoais** (nome de usuário,
caminhos da sua máquina, números de série). Vulnerabilidades de segurança
**não** vão em issue pública: veja [SECURITY.md](SECURITY.md).

## O que é

- Lê as capacidades da GPU e detecta os jogos instalados (bibliotecas como
  Steam/Epic/GOG + catálogo de perfis do driver).
- Avalia regras em TOML (pasta [`regras/`](regras/)) e gera um **plano de
  configuração** auditável.
- Um serviço do Windows aplica o plano via NVAPI DRS; a interface roda sem
  privilégios de administrador.
- Prioridade das decisões: **estabilidade > latência > FPS**, respeitando um
  teto de qualidade (padrão: "Equilibrado") e as escolhas manuais do usuário.
- Atua apenas na camada do driver: não modifica arquivos dos jogos nem injeta
  código em processos.

Plataforma: somente Windows com GPU NVIDIA. Licença: [MIT](LICENSE).

## Arquitetura em camadas

| Camada | Crate | Papel |
|---|---|---|
| Domínio | `nvt-core` | Capacidades, fatos, preferências, regras, avaliador e plano |
| Driver | `nvt-drs` | FFI própria da NVAPI DRS |
| Detecção | `nvt-detect` | GPU/capacidades e jogos/bibliotecas |
| Telemetria | `nvt-telemetry` | Medição passiva via PresentMon CLI (v0.2) |
| Protocolo | `nvt-ipc` | Mensagens do named pipe entre UI e serviço |
| Serviço | `nvt-service` | Serviço do Windows privilegiado que aplica planos |
| Interface | `nvt-ui` | Aplicativo do usuário, sem privilégios |

Detalhes em [`docs/arquitetura.md`](docs/arquitetura.md).

## Roteiro

- **v0.1** — FFI DRS, capacidades, detecção de jogos, motor de regras, serviço,
  UI básica e instalador MSI.
- **v0.2** — telemetria passiva com PresentMon CLI para validar planos.
- **v0.3** — testes A/B automáticos restritos a alavancas de baixo risco.
- **v0.4** — refinamento das regras e do catálogo, distribuição e atualizações.

## Compilação

Requer Rust stable (toolchain MSVC). O build release da interface precisa do
Windows SDK (`fxc.exe`); o MSI precisa do .NET SDK 8+ (só para rodar o WiX).

```powershell
cargo build --workspace
cargo test --workspace          # não exige GPU NVIDIA nem administrador
powershell -ExecutionPolicy Bypass -File instalador\build-msi.ps1   # MSI em target\dist\
```

Os testes que dependem do driver NVIDIA ou de administrador são `#[ignore]` e
rodam só sob pedido (`cargo test -p nvt-drs -- --ignored`).

## Contribuição

Veja [CONTRIBUTING.md](CONTRIBUTING.md). Issues e discussões sobre regras e
arquitetura são bem-vindas.
