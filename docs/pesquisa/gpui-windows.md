# Spike: GPUI no Windows

Data: 2026-11-25 · Máquina: Windows 11 Pro (26300), Rust 1.100 MSVC, Visual Studio 18 Community, Windows SDK 10.0.26100.

## Resumo

GPUI funciona no Windows **sem git e sem clonar o repositório do Zed**, usando crates publicadas no crates.io. O hello world (texto + botão com contador) compilou, abriu a janela, renderizou acentos corretamente e o clique incrementou o contador (verificado por captura de tela antes/depois de 2 cliques simulados). O mesmo vale para a variante com `gpui-component` (via `gpui-kit`).

## Fontes avaliadas

| Fonte | Versão | Situação | Observação |
|---|---|---|---|
| `gpui` (crates.io, oficial Zed) | 0.2.2 (22/10/2025) | Parada há ~13 meses | Incompatível com `gpui-component` atual. Não usar. |
| `gpui-pre` + `gpui-pre-platform` (crates.io) | **0.3.14** (23/11/2026) | Publicação semanal | Snapshot do Zed `zed@f126950`, publicado por huacnlee (mantenedor do gpui-component/Longbridge). É a base que o `gpui-component` 0.11.0 fixa (`=0.3.14`). |
| git `zed-industries/zed` rev `65bbc884…` | — | Abandonado | O `cargo` ficou preso em "Updating git repository" (repositório gigante). Desnecessário com `gpui-pre`. |

## Fonte a fixar

- **Com componentes (recomendado):** `gpui-kit = "=0.11.0"` (puxa `gpui-pre =0.3.14`, `gpui-base`, `gpui-component 0.11.0` e ícones Lucide).
- **Só o núcleo:** `gpui-pre = "=0.3.14"` + `gpui-pre-platform = "=0.3.14"`, renomeados para `gpui`/`gpui_platform` com `package =`.

Fixar sempre com `=` e atualizar `gpui-kit` e `gpui-pre` juntos: a API do GPUI muda entre snapshots e o `gpui-component` exige a versão exata.

## Cargo.toml mínimo

Variante A — só GPUI:

```toml
[dependencies]
gpui = { package = "gpui-pre", version = "=0.3.14", default-features = false, features = ["windows-manifest"] }
gpui_platform = { package = "gpui-pre-platform", version = "=0.3.14", default-features = false }

[profile.release]
strip = true
lto = "thin"
codegen-units = 16
```

`default-features = false` remove Wayland/X11/font-kit (inúteis no Windows). Entrada: `gpui_platform::application().run(|cx| { cx.open_window(...) })`.

Variante B — GPUI + gpui-component:

```toml
[dependencies]
gpui-kit = "=0.11.0"
```

Entrada: `gpui_kit::application().run(|cx| { gpui_kit::init(cx); gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| App)) })`. Componentes em `gpui_kit::component::{button::Button, label::Label, ...}` (60+ componentes: button, input, select, dialog, sidebar, table/list, slider, tabs, notification, tema claro/escuro etc.).

## Requisitos

- Toolchain Rust MSVC (`x86_64-pc-windows-msvc`) e Build Tools do Visual Studio (C++).
- **Windows SDK com `fxc.exe`** — obrigatório **só no build release**: o `build.rs` de `gpui-pre-windows` compila `shaders.hlsl` com `fxc.exe`, procurando-o no `PATH`, em `GPUI_FXC_PATH` ou na pasta do SDK mais recente indicada no registro (`HKLM\SOFTWARE\WOW6432Node\Microsoft\Microsoft SDKs\Windows\v10.0`). Nesta máquina foi encontrado em `C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\fxc.exe` — nada precisou ser instalado.
  - Se faltar: VS Installer → Modificar → Componentes individuais → "SDK do Windows 11 (10.0.26100.x)" (ou carga de trabalho "Desenvolvimento para desktop com C++", que já o inclui). Alternativa: apontar `GPUI_FXC_PATH` para um `fxc.exe` existente.
  - Build debug não usa `fxc.exe` (shaders compilados em tempo de execução via D3DCompiler, que vem no Windows).
- Execução: Direct3D 11 + DirectWrite (nativos do Windows 10/11); não exige runtime extra.

## Resultados medidos

| Métrica | A: gpui-pre puro | B: gpui-kit (gpui-component) |
|---|---|---|
| Build release frio (registro já baixado, `target` vazio) | 96 s | 141 s |
| Binário release (`strip`, LTO thin) | 10,1 MB | 19,0 MB |
| Memória (working set após ~5 s ocioso) | ~58 MB (privada ~65 MB) | ~62 MB |
| Janela abre / processo vivo após 5 s | sim | sim |
| Texto com acentos ("Olá") | ok | ok |
| Clique no botão incrementa contador | ok (0 → 2) | ok (0 → 2) |

Observações: o build frio não inclui o download das crates (já estavam parcialmente no cache do registro); em máquina limpa somar o download. Variante B com `WindowOptions::default()` abriu ocupando quase a tela toda e em tema claro — definir `window_bounds` e tema explicitamente no app real.

Código e logs do spike: pasta de scratchpad `gpui-spike` (`Cargo.toml`, `src/main.rs`, `build-release.log`; variante B em `componentes/`). O `Cargo.toml` antigo via git ficou em `Cargo.git-rev.toml.bak`.

## Riscos

- **Dependência de snapshot de terceiro:** `gpui-pre` não é publicado pela Zed, e sim pelo mantenedor do gpui-component. Se ele parar, a saída é fixar git rev do Zed (clone caro) ou fazer fork. Mitigação: fixar versões exatas e commitar `Cargo.lock`.
- **API instável:** GPUI é pré-1.0; atualizações semanais quebram API. Atualizar em bloco e raramente.
- **Pouca documentação** fora dos exemplos do Zed/gpui-kit; acessibilidade e IME no Windows menos maduros que em macOS.
- **Binário de 19 MB** com componentes (ícones e realce de sintaxe embutidos). Dá para reduzir desligando features do `gpui-component` que não forem usadas (ex.: `tree-sitter`, `assets`) e com `opt-level = "z"`/`lto = "fat"`; não foi testado.
- **`fxc.exe` no build release**: amigos que só baixam o binário não são afetados; quem compila precisa do Windows SDK.

## Recomendação

Usar **`gpui-kit = "=0.11.0"`** (que traz `gpui-pre 0.3.14` + `gpui-component 0.11.0`) direto do crates.io, sem dependência git. Commitar `Cargo.lock`, documentar o requisito do Windows SDK (`fxc.exe`) para build release e tratar atualização do GPUI como tarefa explícita (subir `gpui-kit` e, se usado diretamente, `gpui-pre` na mesma versão exigida).
