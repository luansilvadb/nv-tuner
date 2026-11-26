# Auditoria de UX da interface (nvt-ui)

Data: 2026-11-26. Base: v0.2.0 (commit 81d8300). Somente leitura no código.
Capturas novas em `docs/pesquisa/ui-auditoria/` (modo de visualização, pipe
inexistente, release, 1216x839 com moldura): `visao-geral.png`, `jogos.png`,
`mudancas.png`, `windows.png`, `preferencias.png`, `historico.png`,
`restaurar.png`. Nesta máquina a varredura local não achou jogos e o driver
está conforme, então Jogos/Histórico/Windows aparecem vazios; para Jogos
cheio, ver `docs/ui-capturas/jogos.png` e `jogos-ada.png`.

`docs/intent/interface-e-saude.md` não existe; a auditoria usa a intenção
descrita no pedido (tela inicial de estado do PC + histórico de partidas,
"Aplicar melhorias recomendadas" e "Desfazer tudo", técnico no Avançado,
visual estilo Configurações do Windows 11 / NVIDIA App, sem siglas na tela
principal, sem emojis, GPUI).

## 1. Inventário da UI atual

Estrutura comum: barra lateral (`ui.rs:108`, `Sidebar` do gpui-component)
com título "nv-tuner", subtítulo, grupo "Navegação" e 7 itens; rodapé com
selo de conexão. Cabeçalho de cada tela com título, o mesmo selo repetido
(`ui.rs:1479`) e a linha "Ajustes do seu usuário: status indisponível sem o
serviço" (`textos.rs:121`). Tema escuro fixo (`main.rs`), janela 1200x800.

| Tela | O que mostra | Ações | Texto / jargão visível |
|---|---|---|---|
| Visão geral (`ui.rs:212`) | alerta de serviço; Avisos; Sistema (GPU, Arquitetura, VRAM, Driver "621.40 (r621_20)", jogos); Capacidades (DLSS, Geração de quadros, Reflex, Resizable BAR, Taxa, VRR suportado/ativo); Serviço (status, versão, última aplicação); Conformidade (4 números); Alertas de deriva; cartão de atualização | Aplicar agora, Reescanear jogos, Atualizar, OK (atualizado) | alto. Siglas: GPU, VRAM, DLSS, VRR, G-SYNC, BAR, r621_20; termos: "Conformidade", "conformes", "deriva", "escolhas suas", "Arquitetura Turing" |
| Jogos (`ui.rs:488`) | tabela: jogo, perfil do driver + origem, launcher, API, versão DLSS, Reflex, anticheat de kernel, "O que o app fez"; selos competitivo/aberto; linha expansível com regras aplicadas/não aplicadas/herdadas, Faça no jogo, Arquivo do jogo (`arquivo_jogo.rs`) | expandir, Reverter (chave de arquivo), Aceitar/Retirar experimental | muito alto. "Preset E do DLSS (Turing)", "Experimental: aguardando confirmação" (`textos.rs:81`), "FP8", "Tensor Cores", "%USERPROFILE%", "antes -> depois (situação)" |
| Mudanças (`ui.rs:876`) | plano por perfil; cada setting com 3 selos (situação, Driver/Windows, estável/experimental), antes -> depois, explicação, "Regra: baseline-..." e "Impacto visual"; Saíram do plano; Respeitadas por escolha sua; Ignorados | Aceitar/Retirar, Destravar | máximo. Texto repetido idêntico em cada item ("Forçar AF/AA... pipeline do jogo (TAA/DLSS)... padrão da NVAPI", captura `mudancas.png`); IDs internos de regra; "Controlado pelo aplicativo -> Controlado pelo aplicativo" (mudança nula exibida como mudança) |
| Windows (`ui.rs:986`) | O que o app fez; Experimentais; Diagnóstico; estado do agente | Reverter, Aceitar/Retirar | médio; "agente", "Experimentais", "Diagnóstico" sem explicação |
| Preferências (`ui.rs:1131`) | Sincronismo (Sem/G-SYNC/V-Sync), Qualidade visual (Desempenho/Equilibrado/Fidelidade), Limite de FPS (interruptor, -10/-1/+1/+10, atalhos) | Salvar, Descartar | médio. "tearing", "VRR", "FPS", "teto de impacto visual"; rádio em linha + descrição separada |
| Histórico (`ui.rs:1311`) | lista de eventos: data, ação (rollback, travamento, deriva...), camada, perfil/setting, antes -> depois, motivo | nenhuma | alto; log técnico, não histórico de partidas |
| Restaurar original (`ui.rs:1388`) | um cartão com explicação e botão com diálogo de confirmação | Restaurar original... | médio; "agente", "próximo contato com o serviço" |

Estados:

- Serviço ausente (modo de visualização): alerta amarelo de 3 linhas no topo de
  **toda** tela mandando abrir `services.msc` (`textos.rs:31`), selo amarelo
  duplicado (cabeçalho e rodapé lateral), botões desabilitados sem
  explicar o porquê no próprio botão (só tooltip). É o estado do leigo antes da
  instalação e o mais feio das capturas.
- Protocolo incompatível / atualizado: alerta vermelho ou "Feche esta janela e
  abra de novo" e ações desabilitadas (bom, mas fala em "protocolo/versões").
- Jogo aberto: faixa azul em todas as telas com nome do processo (`cs2.exe`).
- Atualização: cartão com "última verificação há N min · próxima em N h",
  códigos de falha (3010, HTTP, assinatura, manifesto).
- Agente ausente: linha de cabeçalho "Ajustes do seu usuário: status
  indisponível..." sempre visível.

Onde o leigo trava:

1. Primeira tela: não há resposta para "meu PC está bem? o que eu faço?". Os
   números de Conformidade (0/13/0/0) não dizem nada; "13 conformes" com 0 jogos.
2. "Aplicar agora" vs "Reescanear" vs "Atualizar": três botões de mesma
   hierarquia visual, dois são manutenção interna.
3. Mudanças: lista longa de settings já no valor, com texto técnico repetido; o
   usuário não sabe se precisa fazer algo.
4. "Experimental", "Aceitar/Retirar", "Destravar", "travada", "deriva": exigem
   entender o modelo interno do app.
5. Restaurar fica no mesmo nível de navegação de Jogos, sem contexto do que
   será perdido em linguagem simples.
6. Preferências: G-SYNC/V-Sync/VRR sem saber o que o monitor suporta naquela
   tela; o aviso só aparece depois de escolher.

## 2. Capturas

Script: abre `target\release\nvt-ui.exe --pipe \\.\pipe\nv-tuner-previa-inexistente
--tela <tela> --sair-apos 12`, espera 7 s, copia o retângulo da janela com
System.Drawing (`CopyFromScreen`) e fecha o processo. Nada foi gravado no driver.
Limitações: a janela inclui borda e um pedaço do fundo da área de trabalho nas
bordas; telas vazias porque a varredura local não encontrou jogos.

## 3. O que o GPUI do projeto oferece

- Pilha: `gpui-kit =0.11.0` (reexporta `gpui-pre 0.3.14` e `gpui-component
  0.11.0`), ver `crates/nvt-ui/Cargo.toml:20`.
- Tema: `Theme::change(ThemeMode::Dark)` fixo em `main.rs`; cores vêm só de
  tokens do tema (`tema.border`, `group_box`, `muted_foreground`, `info`,
  `warning`, `danger`); não há paleta própria nem cor de destaque da marca
  (o ciano `#38C8F0` do logo não é usado na UI). `Theme` tem `font_family` e
  `mono_font_family` configuráveis; dá para fixar Segoe UI Variable e ler o
  modo claro/escuro do Windows (`ThemeMode::Light` existe).
- Tipografia atual: basicamente `text_sm` + `font_semibold` em títulos de cartão
  (`ui.rs:1560`) e texto padrão no resto; falta escala (título de página,
  número grande, legenda).
- Componentes já usados: `Sidebar`, `Button` (primary/outline/ghost), `Tag`,
  `Switch`, `Radio`, diálogo, tooltip, `Alert`, tabela manual, função `cartao`
  própria (`ui.rs:1552`).
- Disponíveis e não usados, relevantes para o redesenho: `setting`
  (`SettingPage`/`SettingGroup`/`SettingItem` com campos, o padrão "linha de
  configuração" do Windows 11), `title_bar` (barra de título própria),
  `description_list`, `progress`, `badge`, `accordion`/`collapsible` (para o
  Avançado), `tab`, `skeleton`/`spinner`, `notification` (toasts),
  `empty` (estado vazio), `status_bar`, `stepper`.
- Gráfico de linha: há `chart::LineChart` (e `AreaChart`, `BarChart`) sobre o
  módulo `plot`, além de `canvas()` do GPUI com `PathBuilder` para desenhar
  uma linha simples à mão (sparkline de FPS/frametime por partida). Basta para
  um gráfico simples; não há eixos interativos sofisticados.
- Ícones: `IconName` do gpui-component (conjunto Lucide embutido em
  `gpui-kit-assets`); hoje Windows e Preferências usam o mesmo ícone
  (`ui.rs:1545-1546`) e os ícones nem aparecem na barra lateral (captura).
  `assets/` só tem o logo (svg, png 16-256, ico); nenhum ícone próprio.
- Navegação lateral: `Sidebar` com `SidebarGroup`/`SidebarMenuItem`; suporta
  ícone, grupos e rodapé, o suficiente para "Início / Jogos / Histórico /
  Avançado".
- Limites: sem Mica/acrylic nativo garantido; animações simples apenas; sem
  componente pronto de "card de saúde"; tudo que for visual próprio (anel de
  status, sparkline) é `div` + `canvas`.

## 4. Problemas priorizados e direção

| # | Problema | Exemplo | Direção |
|---|---|---|---|
| 1 | Não existe resposta de estado na tela inicial; painel de engenheiro | `visao-geral.png`: Sistema, Capacidades, Serviço, Conformidade lado a lado | Tela Início com um cartão grande: "Tudo certo" / "3 melhorias disponíveis" / "Precisa de atenção", uma frase e um botão primário "Aplicar melhorias recomendadas"; detalhes de hardware recolhidos |
| 2 | Siglas e jargão na tela principal | `textos.rs:41` "Alertas de deriva", `textos.rs:55-56` VRR, `ui.rs:281`; "r621_20", "VRAM", "Resizable BAR" | Linguagem de resultado ("Seu monitor aceita sincronismo adaptável: sim"); siglas só no Avançado, com nome por extenso |
| 3 | Alerta de serviço ausente gritante e repetido em toda tela, mandando usar services.msc | `textos.rs:31`, todas as capturas | Um estado vazio único no Início: "O nv-tuner não está ativo" + botão "Iniciar" ou "Reinstalar"; nas outras telas, faixa discreta de uma linha |
| 4 | Mudanças é um despejo do plano: itens sem mudança, texto idêntico repetido, IDs de regra | `mudancas.png` ("Controlado pelo aplicativo -> Controlado pelo aplicativo", "Regra: baseline-filtragem-pela-aplicacao") | Mostrar só o que muda; agrupar por benefício ("Menos atraso", "Imagem"); IDs, camada e regra no Avançado |
| 5 | Modelo interno exposto: experimental, aceitar/retirar, travada/destravar, agente, perfil global | `textos.rs:81,101-102,116`, `textos.rs:121` | "Sugestões em teste (opcional)" no Avançado; "Você mudou isto manualmente, mantemos sua escolha" com "Voltar ao recomendado" |
| 6 | Hierarquia de ações plana; ações de manutenção no mesmo peso | `ui.rs:434-455` Aplicar / Reescanear / Atualizar | Um primário por tela; Reescanear e Atualizar viram automáticos ou ação secundária no menu do Avançado |
| 7 | Visual genérico: cartões iguais com borda, sem escala tipográfica, sem cor da marca, sem ícones na lateral | `ui.rs:1552` (`cartao`), capturas | Padrão Configurações do Windows 11: `SettingPage`/`SettingItem`, título 28px, linhas com ícone + título + descrição + controle à direita, acento ciano do logo, Segoe UI Variable |
| 8 | Selo "Modo de visualização" e status do agente duplicados | cabeçalho e rodapé da lateral (`ui.rs:129`, `ui.rs:1479`) | Um único indicador, no Início |
| 9 | Histórico é log técnico (rollback, deriva, setting), não histórico de partidas | `ui.rs:1311`, `textos.rs:373` | Histórico de partidas (jogo, data, duração, sparkline com `LineChart`/`canvas`); log técnico no Avançado |
| 10 | Jogos: tabela com 8 colunas técnicas (API, DLSS, perfil, anticheat de kernel) | `docs/ui-capturas/jogos.png`, `ui.rs:488` | Lista de jogos com capa/nome, um status ("Otimizado", "Faça 2 ajustes no jogo") e detalhe ao clicar; colunas técnicas no Avançado |
| 11 | "Restaurar original" como item principal e texto sobre agente/contato | `restaurar.png` | "Desfazer tudo" no Início (secundário) e no Avançado, com confirmação em linguagem simples |
| 12 | Preferências pedem conhecimento de sincronismo sem mostrar o que o monitor suporta | `preferencias.png` | Recomendação pré-selecionada com base no monitor detectado ("Recomendado para seu monitor"); opções avançadas recolhidas |
| 13 | Números sem contexto: "13 conformes" com 0 jogos; "0 pendentes" em destaque colorido | `visao-geral.png` | Substituir por frase de estado; contadores só no Avançado |
| 14 | Mesmo ícone para Windows e Preferências e ícones invisíveis | `ui.rs:1545-1546` | Ícones distintos visíveis na lateral |
| 15 | Tema escuro fixo, ignora o tema do Windows | `main.rs` (`ThemeMode::Dark`) | Seguir o modo do sistema |
