# Interface (`nvt-ui`)

A interface é um processo comum, **sem privilégios de administrador**. Ela
nunca grava no driver: toda alteração é pedida ao serviço (`nvt-service`)
pelo named pipe, conforme `docs/ipc.md`. Feita em GPUI com
`gpui-component` (`gpui-kit = "=0.11.0"`, ver `docs/pesquisa/gpui-windows.md`),
em tema escuro.

## Execução

```text
cargo run -p nvt-ui                          # debug (shaders compilados em tempo de execução)
cargo build --release -p nvt-ui              # release (exige fxc.exe do Windows SDK)
target\release\nvt-ui.exe [opções]
```

| Opção | Efeito |
|---|---|
| `--pipe <nome>` | usa outro named pipe (testes); padrão `\\.\pipe\nv-tuner` |
| `--tela <chave>` | tela inicial: `visao-geral`, `jogos`, `mudancas`, `preferencias`, `historico`, `restaurar` |
| `--sair-apos <s>` | fecha sozinha depois de `s` segundos (capturas automatizadas) |

## Modos de operação

```
              ┌──────────── thread de fundo (nvt-ui-ipc) ────────────┐
 janela ──►  Comando ──► conecta ao pipe ──► requisição ──► Resposta  │
   ▲                         │ falhou (serviço ausente)               │
   │                         ▼                                       │
   └── Evento ◄── pipeline local SOMENTE LEITURA (nvt-engine) ◄──────┘
```

- **Serviço conectado**: estado, plano e histórico vêm do serviço
  (`ObterEstado`, `ObterPlano`, `ListarHistorico`), atualizados a cada 15 s
  e depois de cada ação. Os botões de escrita ficam habilitados.
- **Modo de visualização** (serviço ausente, impostor no pipe ou versão de
  protocolo incompatível): a UI calcula localmente o que seria feito
  (`Engine::planejar` e `Engine::verificar_deriva`) e mostra um aviso para
  instalar/iniciar o serviço. Todas as ações de escrita ficam desabilitadas.
  A thread tenta reconectar a cada 3 s; quando o serviço sobe, a UI passa
  sozinha para o modo conectado. Com **versão de protocolo diferente**, um
  alerta vermelho no topo mostra as duas versões, diz quem é mais novo e
  instrui a atualizar o nv-tuner pelo instalador (UI e serviço juntos); ele
  some quando um serviço compatível responde.
- Os avisos, as reversões e os jogos ignorados vêm do serviço
  (`estado.avisos`, `plano.reversoes`, `plano.ignorados`); no modo de
  visualização são calculados localmente com os mesmos DTOs
  (`local::interpretar_salvo`, `local::reversoes`, `local::ignorados`),
  somente leitura.

### Garantias do modo de visualização

1. Só `planejar` e `verificar_deriva` são chamados — nunca `aplicar`,
   `tratar_deriva`, `restaurar` ou `definir_preferencias`.
2. O motor recebe um `SomenteLeitura<DrsReal>`: `criar_perfil`,
   `criar_aplicacao`, `gravar_setting`, `remover_setting` e `salvar`
   devolvem erro sem tocar no driver (testado com o `DriverFalso`: zero
   gravações e zero salvamentos mesmo chamando `aplicar` de propósito).
3. `estado.json`, `estado.anterior.json` e `snapshot-original.json` são
   lidos como texto (`local::ler_salvo`) — **não** por `Armazem::abrir`,
   que renomeia um `estado.json` corrompido — e copiados para um
   `Armazem::em_memoria`, que não grava nada em disco. A decisão é a mesma do
   motor (estado inválido → cópia de segurança → estado limpo; snapshot
   ilegível → restauração bloqueada), só que vira aviso: `estado_corrompido`
   + `estado_recuperado`/`estado_reiniciado` ou `snapshot_indisponivel`.
   Como no motor, um `estado.json` **ausente** com `estado.anterior.json`
   válido mostra a cópia (aviso `estado_recuperado`, sem renomear nem gravar
   nada), e o marcador `snapshot-perdido.json` vira
   `snapshot_original_perdido` (`local::interpretar_marcador_quarentena`). O
   snapshot legível entra no armazém em memória para as reversões mostrarem
   o valor original certo.
4. Comandos de escrita recebidos sem serviço são recusados pela própria
   thread de comunicação, antes de qualquer E/S.

A lista de jogos não faz parte do protocolo; por isso ela é sempre obtida
pela detecção local (somente leitura), também com o serviço conectado. Nesse
caso, “o que o app aplicou” vem do plano do serviço (mudanças `aplicada` ou
`já no valor`); no modo de visualização, do estado salvo (`esperado`).

## Thread de comunicação

- O cliente do `nvt-ipc` é bloqueante; ele roda só na thread `nvt-ui-ipc`.
  A janela envia `Comando`s por canal e consome `Evento`s a cada 100 ms
  numa tarefa do GPUI — a renderização nunca espera E/S.
- Uma conexão por requisição (o serviço atende conexões em sequência).
- Erros aparecem num alerta no topo da tela, com o código traduzido
  (ex.: “o serviço está ocupado com outra operação”) e a mensagem do
  serviço. Falhas de transporte (tempo limite, pipe quebrado) também.
- Enquanto uma ação está em andamento, os botões ficam desabilitados e a
  tela mostra “Operação em andamento…”. Se o serviço cair no meio, a UI
  volta ao modo de visualização e libera os botões.

## Telas

Capturas em `docs/ui-capturas/` (modo de visualização nesta máquina).

### Visão geral (`visao-geral.png`)

Cartão **Avisos** logo abaixo dos botões: cada aviso com selo e barra
lateral na cor do nível (vermelho = erro, amarelo = atenção, azul =
informação), data, mensagem e detalhe técnico; “Nenhum aviso.” quando não
há. A captura atual foi feita no modo de visualização com um `%ProgramData%`
temporário (junções para as pastas de launchers e um `nv-tuner` com
`estado.json` truncado, `estado.anterior.json` válido e
`snapshot-original.json` vazio): mostra os três níveis, e os arquivos não
foram alterados. Depois:
GPU, arquitetura, VRAM, driver (versão e ramo), número de jogos detectados;
capacidades relevantes (DLSS, geração de quadros, Reflex, Resizable BAR,
VRR); status do serviço, versão e última aplicação; **conformidade**
(mudanças pendentes, conformes, escolhas do usuário e falhas); alertas de
deriva (setting cujo valor no driver diverge do que foi aplicado). Botões
**Aplicar agora** (só com serviço), **Reescanear jogos** e **Atualizar**.

No modo de visualização, VRR aparece como “Não” porque o monitor ainda não
é detectado localmente.

### Jogos (`jogos.png`)

Tabela com jogo, perfil do driver usado (e a origem: perfil da NVIDIA,
existente, criado pelo app ou a criar), launcher, API gráfica, versão do
DLSS, Reflex, anticheat de kernel e o que o app aplicou no perfil. Abaixo,
se houver, **Ignorados pelo plano**: jogos deixados de fora por um problema
só deles, com executável e motivo.

### Mudanças (`mudancas.png`)

Plano agrupado por perfil (global primeiro). Cada item: nome legível da
setting, situação (pendente, aplicada, já no valor, falhou…), estável ou
experimental (experimentais são só sugestões), **antes → depois**, motivo,
regra de origem e impacto visual. Abaixo, **Saíram do plano (voltam ao
original)**: settings que o app gravou e o plano não quer mais, com perfil,
situação (a reverter, revertida, falhou), **atual → o que acontece** (volta
a um valor, sai do perfil ou só deixa de ser vigiada) e por que saiu do
plano. Depois, **Respeitadas por escolha sua**: settings travadas (mudança
manual detectada), com botão **Destravar** (só com serviço), que devolve a
setting ao automático. Por fim, se houver, **Ignorados pelo plano**.

### Preferências (`preferencias.png`)

- **Sincronismo**: Sem sincronização / G-SYNC / V-Sync, com explicação.
- **Qualidade visual**: Desempenho / Equilibrado / Fidelidade, com a
  explicação de cada teto de impacto visual.
- **Limite de FPS** opcional (20 a 1000): interruptor, ajuste fino (±1,
  ±10) e atalhos.

As escolhas ficam num rascunho; **Salvar preferências** envia
`DefinirPreferencias` (só com serviço) e **Descartar alterações** volta ao
valor em vigor. Uma atualização vinda do serviço não apaga um rascunho
alterado.

### Histórico (`historico.png`)

Lista (mais recentes primeiro) com data no fuso local, ação (aplicação,
restauração, rollback, travamento, destravamento, preferências, deriva
detectada), perfil e setting, antes → depois e motivo.

### Restaurar original (`restaurar.png`)

Explica o que será desfeito e oferece **Restaurar original…**, que abre um
diálogo de confirmação; só ao confirmar a UI envia `RestaurarOriginal` ao
serviço. Indisponível no modo de visualização. Com o aviso
`snapshot_indisponivel` (snapshot original ilegível), o botão fica
desabilitado e um alerta “Restauração desabilitada” explica o motivo e o
que fazer, com o detalhe técnico abaixo. Com o aviso
`snapshot_original_perdido` (diretório de estado em quarentena), o cartão
Avisos mostra o texto próprio da UI (`textos::SNAPSHOT_ORIGINAL_PERDIDO`):
“Restaurar original” passa a voltar à base pós-quarentena, que pode já
conter mudanças do nv-tuner, e não ao estado anterior ao app.

## Textos e tradução

Todas as frases da interface estão em `crates/nvt-ui/src/textos.rs`
(pt-BR, com acentuação), inclusive os nomes legíveis das settings do
catálogo e dos seus valores. O resto do crate só referencia esse módulo.

## Ícone

O logo (`assets/logo.svg`) é original: uma curva de frametime em ciano
(`#38C8F0`) que oscila e se estabiliza numa linha reta, sobre um quadrado
escuro arredondado. `assets/gerar-icones/` (ferramenta fora do workspace)
gera `assets/png/nv-tuner-{16…256}.png` e o `assets/nv-tuner.ico`
multi-resolução:

```
cargo run --release --manifest-path assets/gerar-icones/Cargo.toml
```

O `build.rs` dos dois executáveis inclui `assets/recurso-win.rs`, que embute
o .ico como recurso Win32 de ID 1 e o VERSIONINFO (ProductName `nv-tuner`,
CompanyName `nv-tuner contributors`, versão do Cargo). O GPUI carrega o
ícone de ID 1 do próprio executável ao registrar a classe da janela, então a
janela e a barra de tarefas usam o logo sem código extra. O instalador usa o
mesmo .ico em `ARPPRODUCTICON` (Aplicativos instalados) e no atalho do Menu
Iniciar.

## Código

| Módulo | Papel |
|---|---|
| `main.rs` | opções da linha de comando, tema escuro, janela 1200×800 |
| `ui.rs` | renderização (GPUI + gpui-component): barra lateral e telas |
| `modelo.rs` | estado da UI sem janela (`EstadoUi`, `Evento`, `Comando`), conformidade, conversão do plano, datas |
| `servico.rs` | thread de comunicação, reconexão, despacho de comandos |
| `local.rs` | modo de visualização: `SomenteLeitura`, detecção e conversão para os DTOs do protocolo |
| `fuso.rs` | conversão de UTC para o fuso local do Windows |
| `textos.rs` | textos da interface |

## Testes

`cargo test -p nvt-ui` (sem janela): chaves das telas e opções da linha de
comando; conversão do plano em grupos e itens respeitados; contagem de
conformidade (experimentais não contam); “aplicado” por perfil; escrita
permitida só com serviço ocioso; queda do serviço libera a operação
pendente; erros viram mensagem; rascunho de preferências; jogos com
serviço usam o plano; `SomenteLeitura` recusa toda escrita (inclusive via
`Engine::aplicar`); mapeamentos de impacto, preferências e histórico;
datas (civil ↔ Unix, ISO 8601); snapshot indisponível desabilita só a
restauração; alerta de versão incompatível até reconectar; textos de
reversão iguais aos do serviço; leitura local do estado (válido,
corrompido com e sem cópia, versão não suportada, snapshot vazio, erro de
leitura) sem escrever; reversões e jogos ignorados calculados localmente
sem nenhuma gravação.
