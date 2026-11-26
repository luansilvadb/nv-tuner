# Interface (`nvt-ui`)

A interface é um processo comum, **sem privilégios de administrador**. Ela
encaminha pedidos de alteração ao serviço (`nvt-service`)
pelo named pipe (IPC v5, conforme `docs/ipc.md`). As gravações no Windows
cabem ao agente interativo. Feita em GPUI com
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
| `--tela <chave>` | tela inicial: `visao-geral`, `jogos`, `mudancas`, `windows`, `preferencias`, `historico`, `restaurar` |
| `--sair-apos <s>` | fecha sozinha depois de `s` segundos (capturas automatizadas) |
| `--expandir <jogo>` | abre já expandida, na tela Jogos, a linha do jogo com esse nome ou perfil (repetível; capturas) |

Só em build de depuração, a variável `NVT_UI_SIMULAR_GPU` (`turing`,
`gtx16`, `ampere`, `ada`, `blackwell`) troca a GPU informada ao motor **no
modo de visualização** (o driver real continua sendo lido, nada é gravado).
Serve para testes e capturas de outra arquitetura; não existe no binário de
release.

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
   thread de comunicação, antes de qualquer E/S. Aceites, ajustes, diagnósticos
   e histórico da camada Windows vêm da projeção do serviço para o SID que
   abriu o pipe. Ao perder a conexão, a UI descarta esses dados mesmo se a
   leitura local do driver falhar.

A lista de jogos vem do serviço (`plano.jogos`, protocolo v3) quando
conectado; no modo de visualização, do cálculo local, convertido para os
mesmos DTOs (`local::jogos_ipc`). Com o serviço conectado a UI não varre mais
as bibliotecas localmente.

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

Capturas em `docs/ui-capturas/` (L2, R-UI-13; modo de visualização nesta
máquina, `--pipe` para um pipe inexistente, só a área da janela):
`boas-vindas.png` (primeiro uso, janela inteira), `inicio.png`, `jogos.png` e
`avancado.png` (`--tela inicio|jogos|avancado`). As telas técnicas abaixo
(v0.2.0) hoje são grupos da seção Avançado.

### Visão geral (hoje no Avançado, “Sistema e recursos”)

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
**taxa de atualização** atual do monitor principal em Hz arredondados
(`refresh_mhz` 239 757 → “240 Hz”), **VRR (G-SYNC) suportado** e **VRR
(G-SYNC) ativo**; valor não detectado aparece como “desconhecido”, nunca
“?”); status do serviço, versão e última aplicação; **conformidade**
(mudanças pendentes, conformes, escolhas do usuário e falhas); alertas de
deriva (setting cujo valor no driver diverge do que foi aplicado). Botões
**Aplicar agora** (só com serviço), **Reescanear jogos** e **Atualizar**.

Cartão **Atualização do nv-tuner** (só com serviço, U-48): “Versão X · última
verificação há N min · próxima em N h”, mais, quando houver, “Reinicie o Windows
quando puder para concluir a atualização.” (instalador terminou com 3010; nunca
reinicia sozinho), a pendência (jogo aberto, lista de processos ilegível, outra
instalação do Windows, baixando, instalando), a falha de instalação com o código
(com nova tentativa ou abandonada) e o erro da última verificação (sem conexão,
recusa HTTP, assinatura, manifesto, disco). Não há opção de desligar, adiar ou
voltar de versão (U-49). Depois de uma atualização não vista pelo usuário
(`nao_vista`), a Visão geral mostra “Atualizado para a vX.Y.Z.” com **OK**, que
esconde o aviso e envia `MarcarAtualizacaoVista` (U-47).

Se o pipe cair depois de conectar enquanto o serviço baixava ou instalava uma
versão, ou se o serviço responder com versão diferente da interface (ou outro
protocolo depois de conectar), a UI mostra “O nv-tuner foi atualizado. Feche esta
janela e abra de novo…”, desabilita as ações e para de reconectar (U-42). Queda
fora disso é serviço parado: modo de visualização, como antes.

Os avisos `gsync_sem_vrr` e `vrr_desligado_no_painel` (F1-15) usam textos
próprios da UI (`textos::GSYNC_SEM_VRR`, `textos::VRR_DESLIGADO_NO_PAINEL`):
“Seu monitor não tem G-SYNC/VRR: escolha “sem sincronismo”…” e “Ative o
G-SYNC em Painel de Controle NVIDIA > Configurar G-SYNC…”. No modo de
visualização eles são calculados localmente a partir de `plano.avisos`
(`local::aviso_do_plano`, igual ao serviço).

### Faixa de jogo aberto (adiamento, IPC v4)

Com `estado.adiamento` presente (jogo ou anticheat vigiado em execução), uma
faixa azul **Jogo aberto** aparece no topo de todas as telas, com os
processos e a ação pendente (`textos::adiamento`):

| Ação | Texto |
|---|---|
| `aplicar_plano` | Jogo aberto (cs2.exe): as alterações serão aplicadas quando você fechar o jogo. |
| `tratar_deriva` | Jogo aberto (cs2.exe): a mudança detectada no driver será tratada quando você fechar o jogo. |
| `restaurar_original` | Jogo aberto (cs2.exe): os valores originais serão restaurados quando você fechar o jogo. |

Com `lista_indisponivel = true` (o serviço não conseguiu ler a lista de
processos), a faixa tem o título **Alterações pendentes** e o texto
`textos::ADIAMENTO_SEM_LISTA`: “Não foi possível verificar se há jogo
aberto; as alterações ficam pendentes.” — nunca “Jogo aberto (lista de
processos indisponível)”.

As respostas adiadas de **Aplicar agora** (`Plano.adiada`), **Restaurar
original** (`Resposta::Restauracao { adiada }`) e de **Preferências** e
**Destravar** (`Resposta::Adiada { adiamento }`) viram `Desfecho::Adiado` e
mostram essa mesma mensagem (tipo `TipoMensagem::Adiada`, em azul, com o
mesmo título da faixa), **não** erro nem “Preferências salvas.”/“Plano
aplicado”. Se a faixa do estado já diz o mesmo, a mensagem não é repetida.

### Jogos (técnico, no Avançado)

Tabela com jogo, perfil do driver usado (e a origem: perfil da NVIDIA,
existente, criado pelo app ou a criar), launcher, API gráfica, versão do
DLSS, Reflex, anticheat de kernel e **O que o app fez**: um texto curto
(`textos::resumo_curto`), “herda N ajustes globais” e, se o jogo tiver
orientações, “Faça no jogo: N itens”. Jogo da base curada de competitivos
(`jogo.competitivo`) ganha o selo **competitivo** ao lado do nome; jogo em
execução na última varredura (`jogo.em_execucao`), o selo discreto
**aberto**. Exemplos:

| Situação | Texto |
|---|---|
| Turing + jogo com DLSS | Preset E do DLSS (Turing) |
| Ampere + jogo com DLSS | Experimental: aguardando confirmação |
| Ada/Blackwell + jogo com DLSS | Nenhum ajuste específico — GPU Ada roda o DLSS padrão sem custo extra |
| GTX 16 | Nenhum ajuste específico — GPU sem Tensor Cores (sem DLSS) |
| jogo sem DLSS | Nenhum ajuste específico — jogo sem DLSS |
| teto Fidelidade | Nenhum ajuste específico — bloqueado pelo teto de qualidade “Fidelidade” |

Sem regra aplicada, o motivo mostrado é o mais relevante entre as regras não
aplicadas (experimental > escolha sua > teto > sem Tensor Cores > arquitetura
> jogo sem DLSS > fato desconhecido > conflito, VRR inativo ou jogo com
Reflex > condição genérica). Os motivos `vrr_inativo` (“G-SYNC (VRR) não
está ativo”, com a instrução do Painel NVIDIA no texto longo) e
`jogo_com_reflex` (“o jogo tem Reflex, que já cuida disso”) têm textos
próprios.

Clicar na linha expande a explicação completa: **Regras aplicadas no perfil
do jogo** (cada setting com valor e situação no plano), **Regras não
aplicadas** com o motivo por extenso (`textos::motivo_longo`, ex.: “GPU Ada
Lovelace: o modelo padrão do DLSS já roda com FP8, forçar outro preset não
ganharia desempenho.”), **Herdado do perfil global** (as settings) e, por
fim, **Faça no jogo**: as orientações (`jogo.orientacoes`, já filtradas pelo
sincronismo no núcleo) traduzidas por `textos::orientacao`, uma frase curta
e acionável por código, ex.:

| Código | Texto |
|---|---|
| `reflex_on_boost` | Ligue o NVIDIA Reflex em On + Boost (Ativado + Turbo). |
| `limitador_do_jogo` | Limite o FPS no próprio jogo, num valor que a máquina mantenha estável. |
| `vsync_driver_com_gsync` | Desligue o V-Sync no jogo: com G-SYNC, o app já liga o V-Sync no driver. |
| `fps_max_padrao_baixo` | O limite padrão é de 120 FPS: aumente-o no menu de vídeo ou use +fps_max nas opções de inicialização da Steam. |

Todo código de `nvt_core::Orientacao::TODAS` tem texto (teste); um código
desconhecido (serviço mais novo) aparece como o próprio código, sem quebrar a
tela. O modo de visualização mostra o mesmo (`local::jogos_ipc` preenche
`competitivo`, `em_execucao`, `orientacoes` e os fatos curados como o
serviço). Abaixo da tabela, se houver, **Ignorados
pelo plano**: jogos deixados de fora por um problema só deles, com executável
e motivo.

As capturas da v0.2.0 desta tela saíram no L2 (ver as capturas novas acima).

#### Arquivo do jogo (Fase 3)

Jogo com `jogo.config_jogo` (IPC v7) ganha, no fim da explicação expandida,
a seção **Arquivo do jogo** (`arquivo_jogo.rs`). Para cada arquivo: o nome,
o caminho já anonimizado pelo agente (`%USERPROFILE%\…`, nunca o nome do
usuário), a conta escolhida, o estado do backup (“sem backup (nada foi
gravado ainda)”, “original guardado e íntegro”, “backup ausente ou alterado:
o app não grava mais neste arquivo”) e cada chave como
`nome: antes → depois (situação)`, com o motivo quando não aplicada. Chave
aplicada com `reversivel` tem o botão **Reverter**, que envia
`ReverterConfigJogo` com a instância opaca projetada pelo serviço
(`ArquivoJogoUi.instancia`, a mesma do destino do agente).

Orientações (`config_jogo.orientacoes`), uma frase por item:

| Orientação | Texto |
|---|---|
| `fechar_steam` | Feche a Steam para aplicar. |
| `fechar_jogo` | Feche o jogo para aplicar (processos). |
| `desmarcar_somente_leitura` | Desmarque Somente leitura em `<arquivo>`. |
| `ajustar_no_jogo` | O arquivo é sincronizado na nuvem: faça o ajuste no menu do jogo. |
| `escolher_conta` | Mais de uma conta encontrada: abra o jogo com a conta que você usa. |

Os motivos da Fase 3 (`config_politica`, `janela_fechada_exigida` — “Feche a
Steam para aplicar” quando a lista tem `steam.exe`, senão “Feche o jogo para
aplicar” —, `arquivo_na_nuvem`, `arquivo_somente_leitura`,
`formato_inesperado`, `arquivo_alterado_pelo_jogo`, `conta_ambigua`,
`arquivo_ausente`, `reparse_recusado`) têm texto curto e longo próprios;
nenhum atribui a restrição a exigência de anticheat. Sugestões experimentais
de config do jogo (campo próprio `config_jogo.experimentais`, com regra,
destinos e estado `aceitavel`/`aceita`) aparecem na mesma seção com **Aceitar**/**Retirar** (`DefinirAceiteExperimental`, o
fluxo da Fase 2). A UI continua sem gravar nada.

### Mudanças

O plano do driver é agrupado por perfil (global primeiro). Os ajustes do
usuário ficam em um cartão Windows. Cada item apresenta o nome legível da
setting, camada (Driver ou Windows), situação (pendente, aplicada, já no
valor, falhou…) e estado experimental, além dos valores **antes → depois**, motivo,
regra de origem e impacto visual. Abaixo, **Saíram do plano (voltam ao
original)**: settings que o app gravou e o plano não quer mais, com perfil,
situação (a reverter, revertida, falhou), **atual → o que acontece** (volta
a um valor, sai do perfil ou só deixa de ser vigiada) e por que saiu do
plano. Depois, **Respeitadas por escolha sua**: settings travadas (mudança
manual detectada), com botão **Destravar** (só com serviço), que devolve a
setting ao automático. Por fim, se houver, **Ignorados pelo plano**.

As regras experimentais do driver aparecem nesta tela. **Aceitar** e
**Retirar** enviam `DefinirAceiteExperimental` com o ID da regra. O aceite
é por usuário. Só os aceites de quem estiver na sessão de console ativa
entram no plano do driver, cujos efeitos valem para todos na máquina.
Fora do console ativo, a tela informa que o aceite fica inativo.

### Windows

- **O que o app fez** mostra ajustes da camada do usuário. **Reverter** envia
  `ReverterAjusteUsuario` para o destino do ajuste. A reversão usa o valor
  anterior à gestão pelo aplicativo. Para a taxa de atualização, a UI lê o
  caminho estável do monitor primário da sessão e desabilita o botão se esse
  destino estiver indisponível.
- **Experimentais** mostra as sugestões para o usuário. **Aceitar** e
  **Retirar** usam o mesmo pedido que a tela de Mudanças. Regras aceitas
  exibem “experimental aceita por você”. Ao retirar o aceite, se o ajuste
  sair do plano, o serviço coordena a reversão ao valor anterior à gestão.
- **Diagnóstico** mostra valores disponíveis, leituras indisponíveis e
  orientações, com textos em português e documentação oficial por código.
  Códigos novos são exibidos como recebidos. A seção é somente leitura.

O cabeçalho informa se o agente está ativo na sessão e sinaliza quando
a restauração dos ajustes do usuário está pendente. Ao concluir a
restauração do driver, a interface indica separadamente a pendência do
usuário, quando houver.

### Preferências

- **Sincronismo**: Sem sincronização / G-SYNC / V-Sync, com explicação. Com
  G-SYNC no rascunho, um alerta amarelo ao lado da escolha
  (`EstadoUi::aviso_sincronismo`): monitor sem VRR (`vrr.possivel = false`)
  → texto de `gsync_sem_vrr`; VRR possível e desligado → texto de
  `vrr_desligado_no_painel`. Com o VRR desconhecido, vale o aviso que o
  serviço mandou para as preferências em vigor.
- **Qualidade visual**: Desempenho / Equilibrado / Fidelidade, com a
  explicação de cada teto de impacto visual.
- **Limite de FPS** opcional (20 a 1000): interruptor, ajuste fino (±1,
  ±10) e atalhos.

As escolhas ficam num rascunho; **Salvar preferências** envia
`DefinirPreferencias` (só com serviço) e **Descartar alterações** volta ao
valor em vigor. Uma atualização vinda do serviço não apaga um rascunho
alterado.

### Histórico

Lista (mais recentes primeiro) com data no fuso local, ação (aplicação,
restauração, rollback, travamento, destravamento, preferências, deriva
detectada), camada (Driver ou Windows), perfil e setting, antes → depois e motivo.

### Restaurar original

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

A partir da v5, o pedido cobre as duas camadas: o serviço restaura o driver
e avança a geração de restauração, e o agente de cada sessão restaura os
ajustes do Windows do seu usuário (no próximo contato ou no próximo logon,
se estiver ausente). A tela distingue driver concluído de usuário pendente.
Antes de desinstalar, cada usuário da máquina que tenha ajustes do Windows
deve usar este botão: a desinstalação com `RESTAURAR_PERFIL=1` só restaura
o usuário que a executa.

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
conformidade (experimentais não contam); valor e situação de cada setting aplicada; escrita
permitida só com serviço ocioso; queda do serviço libera a operação
pendente; erros viram mensagem; rascunho de preferências; jogos vêm do
serviço ou do cálculo local; textos de “O que o app fez” e dos motivos
(Turing, Ampere, Ada, GTX 16, sem DLSS, teto, escolha travada); jogos locais
no mesmo formato do serviço e GPU Ada simulada; `SomenteLeitura` recusa toda escrita (inclusive via
`Engine::aplicar`); mapeamentos de impacto, preferências e histórico;
datas (civil ↔ Unix, ISO 8601); snapshot indisponível desabilita só a
restauração; alerta de versão incompatível até reconectar; textos de
reversão iguais aos do serviço; leitura local do estado (válido,
corrompido com e sem cópia, versão não suportada, snapshot vazio, erro de
leitura) sem escrever; reversões e jogos ignorados calculados localmente
sem nenhuma gravação. Fase 1: todo código de orientação tem texto (e o
desconhecido não quebra); jogo local da base curada com `competitivo`, fatos
curados e orientações; respostas adiadas de Aplicar agora, Restaurar
original, Preferências e Destravar viram mensagem de adiamento, não erro nem
“feito”; lista de processos indisponível com texto e título próprios; faixa
de adiamento vinda do
estado sem repetir a mensagem; aviso de sincronismo conforme rascunho e VRR;
avisos do plano iguais aos do serviço; refresh em Hz e “desconhecido”;
textos próprios de VRR inativo e jogo com Reflex.
