# Serviço (`nvt-service`)

O `nvt-service` é o único componente que grava no driver. Roda como serviço
do Windows (LocalSystem, nome interno **`nvt-service`**, nome de exibição
**“nv-tuner (serviço)”**, contrato com `instalador/nv-tuner.wxs`, que o
registra com o argumento `service`), aplica o plano do `nvt-engine`, vigia o DRS para
tratar deriva e atende a UI pelo named pipe do `nvt-ipc`.

## Linha de comando

| Comando | Grava? | Uso |
|---|---|---|
| *(sem subcomando)* / `service` | sim | entrada do SCM; fora do SCM sai com código 2 e uma dica |
| `run [--sem-vigia]` | sim | mesma lógica no console (desenvolvimento). Ctrl+C ou uma linha `sair` no stdin encerra |
| `dry-run` | não | plano e diff por perfil (muda / conforme / travada), perfis a criar, sugestões experimentais e descartes |
| `status` | não | driver, GPU, capacidades, preferências, estado salvo, jogos detectados com perfil, conformidade e deriva |
| `apply --yes` | sim | aplica o plano agora e **reativa a autonomia**; exige administrador |
| `restore-original --yes` | sim | volta ao snapshot original e **pausa a autonomia**; exige administrador; código 0 também quando não há snapshot (idempotente) |
| `prepare-state-dir` | só o diretório | cria ou corrige o diretório de estado (ver “Diretório de estado seguro”); usado pelo instalador como SYSTEM; exige administrador (código 2 sem) |

Opções globais:

- `--estado-dir <DIR>`: diretório de estado (padrão `%ProgramData%\nv-tuner`).
  `status` e `dry-run` só leem; não criam o diretório.
- `--somente-leitura`: nenhuma gravação no driver nem no estado, mesmo em
  `run`/`service`. As requisições IPC que gravariam respondem `erro`/`interno`
  com a mensagem "modo somente leitura". Só os logs são escritos.
- `--pipe <NOME>`: nome do pipe (padrão `\\.\pipe\nv-tuner`). Aceita o nome
  curto (`nvt-teste` → `\\.\pipe\nvt-teste`), porque alguns shells comem as
  barras iniciais.
- `--detalhado`: logs em nível de depuração.

`apply`/`restore-original` sem `--yes` são recusados (código 2). Sem
`--estado-dir`, eles primeiro tentam pedir a ação ao serviço em execução
pelo pipe (`AplicarAgora`/`RestaurarOriginal`), para que o estado em memória
do serviço não sobrescreva o resultado; se o serviço não estiver no ar,
executam no próprio processo.

## Laço

Threads:

- **principal**: dona da NVAPI, da sessão DRS e do `Nucleo` (nada disso é
  `Send`); processa eventos em ordem e os temporizadores (passo de 500 ms);
- **IPC**: `nvt_ipc::pipe::Servidor`; cada requisição vira um evento para a
  principal, que responde por um canal. Se a principal não **começar** o
  pedido em 45 s, a resposta é `ocupado` e o pedido é **cancelado**: uma
  troca atômica decide entre “iniciado” (pela principal) e “cancelado”
  (pela thread do IPC), e a principal descarta o pedido cancelado sem
  executá-lo. Se já começou, a thread do IPC espera até 57 s no total
  (abaixo dos 60 s do cliente) e, passado isso, responde `interno` (“a
  operação começou e ainda não terminou”) — nunca `ocupado`, que significa
  “não executado”;
- **vigia**: `ReadDirectoryChangesW` (E/S sobreposta, cancelável) em
  `C:\ProgramData\NVIDIA Corporation\Drs`; se o diretório não existir, tenta
  de novo a cada minuto.

Se a NVAPI não carregar (driver ausente ou em atualização), o serviço fica no
ar respondendo `driver_indisponivel` e tenta de novo a cada minuto.

### Pânicos

Um pânico ao tratar uma requisição, num temporizador (`tick`), na
notificação do DRS ou ao iniciar o núcleo é capturado na thread principal
(`catch_unwind`) e registrado no log (um gancho de pânico também grava a
mensagem e o local). A requisição recebe `interno`; o `Nucleo` é descartado
(o estado em memória pode ter ficado pela metade; a sessão DRS sem
`SaveSettings` não grava nada) e, depois de 5 s respondendo `interno`, é
recriado a partir do disco. A thread dona da NVAPI continua viva e o serviço
não cai.

### Ciclo

Cada ciclo abre uma **sessão DRS nova** (a sessão só lê o driver ao abrir) e
relê o estado do disco (`estado.json`, `servico.json`), depois:

1. `verificar_deriva`; havendo divergências, registra alertas e, com
   autonomia, `tratar_deriva` (reset em massa reaplica; mudança pontual vira
   escolha travada — ver `docs/engine.md`);
2. com autonomia, `planejar` + `aplicar` (o avaliador só põe regras
   **estáveis** no plano; o motor captura o snapshot original antes da
   primeira gravação, relê e registra cada mudança).

Gatilhos:

| Gatilho | Quando |
|---|---|
| início | varredura de jogos + ciclo |
| mudança no DRS | notificação da vigia, depois de **3 s** sem novas notificações (debounce) |
| verificação periódica | a cada **10 min** desde o último ciclo |
| varredura periódica | a cada **6 h** (e sob pedido `ReescanearJogos`) + ciclo |

### Gravações próprias

Para não entrar em laço, uma notificação do DRS é ignorada se:

- chegar até **15 s** depois de uma gravação do próprio serviço (aplicação,
  reaplicação ou restauração); ou
- a assinatura do diretório do DRS (nome, tamanho e data de modificação de
  cada arquivo) for igual à registrada logo após a última gravação própria.

Mesmo que uma notificação própria passe, o ciclo não grava nada: o estado
esperado confere e o diff fica vazio. Mudanças externas dentro da janela são
pegas pela verificação periódica.

### Autonomia

- Ativa por padrão (exceto em `--somente-leitura`).
- `RestaurarOriginal` (IPC) e `restore-original --yes` **pausam** a
  autonomia antes de restaurar; a pausa fica em `servico.json` e sobrevive a
  reinícios. Com a autonomia pausada o serviço só verifica e alerta.
- `AplicarAgora` (IPC) e `apply --yes` reativam.

## IPC

Tratador de todas as `Requisicao` do `nvt-ipc` (`docs/ipc.md`):

| Requisição | Ação |
|---|---|
| `obter_estado` | driver (versão, ramo), GPU (nome, arquitetura, VRAM, capacidades), preferências, nº de jogos, última aplicação, alertas de deriva, avisos |
| `obter_plano` | diff atual; status `pendente`, `ja_no_valor` ou `travada`; reversões pendentes e jogos ignorados |
| `aplicar_agora` | reativa a autonomia, trata a deriva, aplica; devolve o plano com `aplicada`/`falhou` (também nas reversões) |
| `restaurar_original` | pausa a autonomia e restaura o snapshot original; `ok` também sem snapshot |
| `definir_preferencias` | com autonomia, trata antes a deriva pendente; grava e, com autonomia, aplica o novo plano |
| `listar_historico` | mais recentes primeiro; ação: plano/criação de perfil → `aplicacao`, reaplicação → `deriva_detectada`, mudança manual/trava → `travamento`, restauração → `restauracao` |
| `travar_setting` | vira escolha do usuário (sai do estado esperado, sem gravar no driver); `"global"` = perfil base; o nome do perfil é normalizado para o canônico do DRS (sem diferenciar maiúsculas: todas as grafias viram a mesma trava); setting fora do catálogo → `setting_nao_permitida`; perfil inexistente ou não gerenciável (não é de um jogo detectado, não foi criado pelo app nem tem setting gerida ou trava) → `perfil_nao_encontrado`; acima de `MAX_TRAVADAS` (500) → `interno` com “limite de 500 escolhas travadas atingido” |
| `destravar_setting` | perfil sem diferenciar maiúsculas; com autonomia, trata antes a deriva pendente, devolve ao automático e aplica |

**Deriva pendente antes do plano:** `aplicar_agora`, `definir_preferencias`
e `destravar_setting` chamam `verificar_deriva`/`tratar_deriva` antes de
`aplicar_plano` (como o ciclo). Sem isso, uma mudança manual feita no NPI e
ainda não vista pelo ciclo seria tratada como valor do app e revertida pelo
plano novo (ex.: limite 141 → usuário põe 100 → limite desligado ⇒ antes
ficava sem limite; agora 100 vira trava do usuário).
| `reescanear_jogos` | nova varredura + ciclo |

Capacidades no estado: `dlss` = Tensor Cores; `geracao_quadros` = FG da
arquitetura; `reflex` = arquitetura reconhecida; `rebar` = ativo, se lido,
senão suportado; `vrr` ainda não é lido (`false`).

Nomes legíveis de settings e valores vêm de
`docs/pesquisa/settings-referencia.json` (NVIDIA Profile Inspector, MIT),
filtrado no `build.rs` para os IDs do catálogo (não embute o arquivo
inteiro); na falta, usa a descrição e os nomes de valores de
`regras/settings.toml`. Marcadores do NPI como `${DlssVersion}` são removidos.

### Avisos

`Nucleo::avisos` monta `estado.avisos` (`docs/ipc.md`) a partir de:

- **`Armazem::avisos`** (estado corrompido, recuperado da cópia ou
  reiniciado, snapshot ilegível): avisos estruturados do motor
  (`docs/engine.md`), convertidos por `avisos_do_armazem` **pelo código**
  (`codigo_ipc`, correspondência exaustiva com `nvt_ipc::codigo_aviso`;
  nível, mensagem e detalhe vêm do motor). O código `estado_aviso` do
  protocolo continua válido, mas o serviço não o emite mais. Como o
  arquivo corrompido é renomeado na primeira abertura, os avisos são
  **acumulados** na memória do serviço (sem duplicar por código + detalhe) e
  valem até ele reiniciar; cada um é registrado no log uma vez.
- **Somente leitura**: com `--somente-leitura` (e em `status`/`dry-run`) o
  estado é aberto com `Armazem::abrir_somente_leitura`: um `estado.json`
  corrompido fica no lugar (nada é renomeado, criado nem gravado), o estado
  vem da cópia de segurança só em memória e o aviso `estado_corrompido`
  aparece a cada abertura, sem duplicar.
- **Estado recuperado gravado na hora**: se a abertura normal recuperou a
  cópia de segurança (ou recomeçou limpo), o próprio `Armazem::abrir` grava
  `estado.json` antes de devolver (nunca num armazém somente leitura). E,
  se `estado.json` faltar com `estado.anterior.json` válido, a abertura
  recupera da cópia em vez de começar vazia. Assim, nem um `status` /
  `dry-run` / `--somente-leitura` no meio, nem uma gravação perdida fazem o
  serviço perder travas e histórico.
- **Snapshot original perdido** (`snapshot_original_perdido`, nível
  `atencao`): o diretório de estado foi para a quarentena; o snapshot atual
  (ou o próximo) é só uma base pós-quarentena (`docs/engine.md`). O código
  ainda não tem constante em `nvt_ipc::codigo_aviso`; a UI o exibe como
  código desconhecido.
- **Snapshot indisponível** (`Armazem::snapshot_indisponivel`): aviso de
  nível `erro`; a UI desabilita “Restaurar original”.
- **Erros isolados por perfil** do último ciclo: `Aplicacao::erros` e
  `TratamentoDeriva::erros`, substituídos a cada aplicação/tratamento e
  limpos quando o plano fica vazio ou não há deriva. Registrados no log
  como erro.
- **Jogos ignorados** (`Diff::ignorados`) do último plano calculado: um aviso
  com a lista; cada jogo é registrado no log quando a lista muda (nome e
  motivo, sem caminho).

O plano IPC traz também `reversoes` (de `Diff::reverter`, com o motivo
derivado da origem da setting: regra ou preferência) e `ignorados`. Cada
reversão aplicada é registrada no log; a não confirmada, como aviso.
`status` e `dry-run` imprimem avisos, reversões e jogos ignorados.

Erros do motor: `ProfileNotFound` → `perfil_nao_encontrado`; demais erros
do DRS e GPU ausente → `driver_indisponivel`; ID/valor fora da lista →
`setting_nao_permitida`; estado/regras → `interno`.

## Estado e logs

Em `<estado-dir>`:

| Arquivo | Dono |
|---|---|
| `snapshot-original.json`, `estado.json`, `estado.anterior.json`, `perfis-criados.json` | `nvt-engine` (`docs/engine.md`) |
| `snapshot-perdido.json` | gravado por `preparar` logo depois de uma quarentena; lido pelo `nvt-engine` |
| `servico.json` | serviço: `autonomia_pausada`, `pausada_em_unix`, `ultima_aplicacao` |
| `logs\nvt-service.AAAA-MM-DD.log` | `tracing-appender`, rotação diária, últimos 14 arquivos |

Os logs registram ações (perfil, setting, contagens, erros) e o diretório de
estado; não registram usuário, caminhos de jogos nem outros dados pessoais.
`run` também escreve em stderr. `status` e `dry-run` não geram log.

O arquivo de log é aberto pelo próprio serviço (`logs.rs`) com
`FILE_FLAG_OPEN_REPARSE_POINT`: se `logs` ou o arquivo do dia for junção,
link simbólico ou hard link, o log em arquivo é recusado. A limpeza dos
antigos só apaga arquivos comuns. `servico.json` é lido do mesmo modo e
gravado num temporário criado do zero (`create_new`) antes do `rename`.

### Diretório de estado seguro

Em `C:\ProgramData`, usuários comuns podem criar subpastas e viram donos
delas; um diretório de estado pré-criado por um usuário daria a ele controle
sobre o snapshot original, o estado e o destino dos logs escritos como
SYSTEM. Por isso, antes de abrir logs, driver ou estado (`diretorio.rs`):

1. se o diretório não existe, é criado com
   `O:BAD:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)`: dono
   Administradores, DACL protegida, SYSTEM/Administradores com controle
   total e Usuários só com leitura;
2. o diretório e todo o conteúdo são percorridos **sem seguir reparse
   points**; cada item precisa: não ser reparse point (junção, link etc.),
   ter um único vínculo, ter dono SYSTEM ou Administradores e não conceder
   nenhum direito de escrita (dados, anexar, atributos, excluir, `WRITE_DAC`,
   `WRITE_OWNER`, genéricos) a outras contas;
3. se o único problema é o dono de itens internos (raiz segura, nenhuma
   escrita para outros), Administradores toma posse deles. Isso cobre
   arquivos criados por um administrador elevado cujo dono padrão era o
   próprio usuário; para evitar o caso, todo processo privilegiado do
   `nvt-service` define Administradores como dono padrão do seu token;
4. qualquer outro problema: o diretório inteiro vai para quarentena ao
   lado (`nv-tuner.inseguro-<unix>-<pid>`, nada é apagado) e é recriado
   vazio com o descritor seguro. O conteúdo de um diretório que já foi
   gravável por outros não é confiável (um snapshot forjado seria
   “restaurado”), por isso não basta reescrever a DACL. No diretório novo,
   `preparar` grava o marcador `snapshot-perdido.json`
   (`Armazem::marcar_snapshot_perdido`; se falhar, o serviço não opera): a
   próxima aplicação não captura em silêncio como “original” um driver que
   pode já ter mudanças do app — o snapshot novo é tratado como **base
   pós-quarentena**, com o aviso `snapshot_original_perdido`. Antes de
   corrigir, uma verificação com falha de leitura (`Io`) é repetida, e um
   item interno que **some** durante a verificação (rotação de log,
   `estado.json.tmp` renomeado pelo motor) não é problema (`NotFound` é
   benigno; só a raiz sumida é erro): isso nunca leva à quarentena;
5. se nem isso der certo, o serviço **não opera**: não usa driver nem
   estado, não escreve log em arquivo, responde a toda requisição com
   `interno` e a mensagem “diretório de estado inseguro…” (que a UI mostra)
   e tenta de novo a cada minuto.

Processos sem privilégio (`run` de desenvolvimento num console comum) só
verificam e avisam, sem criar nem mover nada; em `--somente-leitura`, um
processo privilegiado também só avisa e, se o diretório for inseguro, não
escreve log em arquivo. `apply --yes`/`restore-original --yes` exigem o
diretório seguro (com a mesma correção) antes de qualquer gravação. O
instalador chama `prepare-state-dir` como SYSTEM depois de copiar os
arquivos (`instalador/nv-tuner.wxs`). Os arquivos do `nvt-engine`
(`estado.json`, `snapshot-original.json`) ainda são abertos pelo motor sem
`FILE_FLAG_OPEN_REPARSE_POINT`; com o diretório verificado e só gravável
por SYSTEM/Administradores, ninguém além deles consegue plantar links lá.

## Testes

`cargo test -p nvt-service` (sobre o `DriverFalso`, diretório temporário e
relógio controlado, sem SCM):

- aplicação inicial: grava, captura snapshot, salva estado e `servico.json`;
  segundo ciclo não grava;
- reset em massa notificado pela vigia é reaplicado depois do debounce;
- mudança pontual é travada na verificação periódica e respeitada depois;
- `RestaurarOriginal` pausa a autonomia (inclusive após reinício) até
  `AplicarAgora`;
- notificações dentro da janela ou com a mesma assinatura são ignoradas;
- tratador IPC: estado, plano com nomes legíveis, trava/destrava, erros de
  setting e perfil, histórico, preferências, nova varredura;
- avisos: `estado.json` truncado com cópia válida → `estado_corrompido` +
  `estado_recuperado`, estado recuperado gravado e não perdido nas
  consultas seguintes, sem duplicar avisos; snapshot vazio →
  `snapshot_indisponivel` (restauração bloqueada); jogo com NUL no
  executável → `ignorados` no plano e aviso `jogos_ignorados`; mapeamento
  dos avisos do `Armazem` pelo código (com texto arbitrário) e motivo de
  reversão; `--somente-leitura` com `estado.json` truncado: avisos sem
  duplicar e diretório byte a byte igual;
- reversão: limite de FPS desligado por fora aparece como `pendente` no
  plano e `aplicada` no `aplicar_agora`, depois some;
- `--somente-leitura`: nenhuma gravação no driver nem no disco;
- laço completo pelo named pipe (nome único), com parada limpa;
- laço com um atendente que entra em pânico (no tratador e no ciclo): a
  requisição recebe `interno`, a thread principal sobrevive e um núcleo
  novo volta a atender;
- `ocupado` cancela o pedido (a principal não o executa depois); pedido já
  iniciado não vira `ocupado`;
- diretório de estado (pastas temporárias, `mklink /J`): junção dentro e
  como raiz detectada sem ser seguida, hard link, link simbólico (se o
  sistema permitir criar), pasta comum reprovada, criação com descritor
  seguro, quarentena com recriação, posse tomada (só elevado) e modo de
  aviso sem alterações;
- logs: arquivo do dia, limpeza dos antigos e recusa de `logs` como junção;
- reverificação (`rv*`, `docs/revisao-v0.1.md`): preferência e destravar com
  deriva pendente travam a escolha do usuário (#4); `status` somente
  leitura no meio de um estado truncado não faz perder travas nem
  histórico, e `estado.json` perdido é recuperado da cópia (#7); 256
  grafias × 2 settings = 2 travas com o nome canônico, perfil não
  gerenciável recusado e limite de 500 travas (#10); arquivo/diretório que
  some durante a verificação não é problema nem leva à quarentena, e a
  quarentena grava o marcador de snapshot perdido (item 6).

Cliente de teste (somente leitura) para um serviço em execução:

```text
cargo run -p nvt-service --example cliente_ipc -- nvt-teste estado --sem-verificar-dono
```

`--sem-verificar-dono` é necessário quando o servidor roda num console
(mesmo elevado, o dono do pipe é o usuário); contra o serviço instalado
(SYSTEM) a verificação normal funciona.

## Pendências conhecidas

- Histórico: destravar e mudança de preferências não geram entrada (o
  `nvt-engine` não registra esses eventos); ficam nos logs.
- Avisos acumulados do estado se perdem num reinício do serviço (o arquivo
  `.corrompido-<data>` continua no diretório).
