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
| `status` | não | driver, GPU, capacidades, preferências, estado salvo, jogos detectados com perfil, conformidade e deriva; com o serviço no ar, resumo da config de jogo da própria sessão (arquivos geridos e pendências por jogo, com Steam ou jogo aberto), sem caminhos nem contas |
| `apply --yes` | sim | aplica o plano agora e **reativa a autonomia**; exige administrador; código 3 se o serviço adiar (jogo aberto) |
| `restore-original --yes` | sim | volta ao snapshot original e **pausa a autonomia**; exige administrador; código 0 também quando não há snapshot (idempotente); código 3 se o serviço adiar (jogo aberto: a restauração não foi feita) |
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
executam no próprio processo. Com jogo aberto ou lista de processos
ilegível (ver “Jogo aberto: gravações adiadas”): pelo serviço, a ação fica
pendente no serviço, nada é gravado agora e o comando sai com **código 3**
(`SAIDA_ADIADA`), escrevendo o motivo no stderr. Para `restore-original`, a
mensagem diz que a restauração **não** foi feita e, se for uma desinstalação
(`msiexec /x … RESTAURAR_PERFIL=1`, que remove o serviço logo depois), que é
preciso fechar os jogos e rodar a desinstalação de novo ou usar “Restaurar
original” antes de desinstalar; o MSI ignora o código, mas o log `/l*v`
registra a mensagem. No próprio processo não há quem execute a pendência,
então o comando recusa **antes** de mudar qualquer coisa (nem a autonomia) e
sai com código 1. Ainda no próprio processo, `apply --yes` com uma
restauração pendente deixada pelo serviço em `servico.json` a descarta (avisa
“restauração pendente descartada: você pediu para aplicar”) e só aplica.

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
| reavaliação do adiamento | a cada **15 s**, só enquanto houver ação pendente (jogo aberto) |

### Jogo aberto: gravações adiadas

Regra S1 de `docs/spec/competitivo.md` (F1-06, F1-07, T-02): nenhuma gravação
no driver com jogo detectado ou anticheat em execução. A justificativa é de
**produto** (o DRS é lido quando o processo do jogo nasce, a gravação não teria
efeito e pode resetar o vídeo), não uma exigência dos anticheats.

**Portão único.** O motor guarda a sessão DRS num invólucro que só dá acesso
de escrita (`criar_perfil`, `criar_aplicacao`, `gravar_setting`,
`remover_setting`, `salvar`) com uma licença, e a única forma de obter a
licença é o portão (`Engine::pode_gravar`, `nvt_engine::Portao`). Cada
operação que grava (`aplicar`, `tratar_deriva` com reaplicação, `restaurar`,
`gravar_permitido`) consulta a lista de processos **imediatamente antes** da
primeira alteração (inclusive do snapshot e do estado); recusada, devolve
`ErroEngine::Adiada { processos, desde }` sem ter mudado nada. Gravar no DRS
sem passar pelo portão não compila. Travar uma mudança manual (deriva
pontual, `travar_setting`) só mexe no estado do app e não passa pelo portão.

**Conjunto vigiado** = exes de todos os jogos detectados na última varredura
(`Portao::vigiar_jogos`, chamado pelo núcleo a cada varredura) + exes do plano
em aplicação + exes e processos protegidos da base curada
(`BaseJogos::processos_vigiados`) + anticheats que só existem durante o jogo
(`EasyAntiCheat.exe`, `EasyAntiCheat_EOS.exe`, `start_protected_game.exe`,
`BEService.exe`, `BEService_x64.exe`). `vgc.exe` (Vanguard) não entra: roda
desde o boot. Comparação por nome, sem diferenciar maiúsculas.

**Detecção só pela lista de processos.** `nvt_detect::processos` usa apenas
`CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS)` e lê o nome do exe; **nunca**
abre handle para processo (`OpenProcess`, nem com
`PROCESS_QUERY_LIMITED_INFORMATION`) nem enumera módulos. Falso positivo por
nome (outro `cs2.exe`) só adia. A varredura de jogos também não lê a pasta de
jogo em execução (reaproveita os fatos anteriores, `JogoEntrada::em_execucao`).
O teste `crates/nvt-core/tests/proibicoes.rs` barra `OpenProcess`,
`TH32CS_SNAPMODULE` e `Command::new` no código de produção.

**Ação pendente** (`Nucleo::pendente`: `AplicarPlano`, `TratarDeriva`,
`RestaurarOriginal`):

- vale para a aplicação automática (início, varredura, verificação
  periódica, preferências, destravar), o tratamento de deriva com
  reaplicação, `aplicar_agora` e `restaurar_original`;
- só uma por vez: a mais recente substitui as anteriores, exceto
  `RestaurarOriginal`, que vence qualquer outra (com restauração pendente, o
  ciclo só lê a deriva e `aplicar_agora` responde adiado com
  `acao = restaurar_original`);
- enquanto houver pendência, o portão é consultado a cada 15 s (cada novo
  adiamento reinicia a contagem); **sem pendência não há varredura periódica
  de processos** — a lista só é lida quando há algo a gravar;
- quando nenhum processo vigiado está mais em execução, a ação roda
  (`RestaurarOriginal` restaura; as outras rodam um ciclo completo:
  deriva → plano → aplicação) e a pendência só sai **depois** que ela der
  certo; se o jogo reabriu no meio-tempo, volta a ficar pendente;
- erro que não é adiamento (driver indisponível, falha de leitura etc.): a
  pendência continua e é tentada de novo no mesmo intervalo de 15 s, até
  `TENTATIVAS_PENDENTE` (3) falhas seguidas; esgotadas, é descartada, o log
  registra o erro e `estado.avisos` passa a ter `acao_pendente_descartada`
  (nível erro, `detalhe` = último erro), que some na próxima aplicação ou
  restauração bem-sucedida;
- lista de processos ilegível (falha fechada do portão): adia como jogo
  aberto; no IPC vai `lista_indisponivel = true` com `processos` vazio, nunca
  o marcador como nome de processo;
- a leitura da deriva continua normal (alertas atualizados); só o
  tratamento que grava espera;
- `restaurar_original` pausa a autonomia já no pedido (nada é aplicado por
  cima da restauração pendente); `aplicar_agora` reativa já no pedido;
- `AplicarPlano`/`TratarDeriva` pendentes ficam só na memória: um reinício
  as perde, e o próximo ciclo (com o jogo fechado) refaz a aplicação
  automática. **`RestaurarOriginal` pendente é persistida** em `servico.json`
  (`restauracao_pendente_desde_unix`) e sobrevive a reinícios: ao iniciar
  (`Nucleo::iniciar`), o serviço a retoma antes do primeiro ciclo — restaura
  se o portão liberar, senão segue pendente. Feita ou descartada, sai do
  arquivo. Em `--somente-leitura` ela não é retomada.

Pedidos da UI adiados respondem normalmente com o adiamento (não erro):
`aplicar_agora` → `plano` com `adiada`; `restaurar_original` →
`restauracao` com `adiada`; `definir_preferencias`/`destravar_setting` →
`adiada` com o `adiamento` (as preferências e a liberação da trava ficam
gravadas no estado; sem adiamento, `ok`). O estado traz o `adiamento` atual
(`docs/ipc.md`).

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
| `obter_estado` | driver (versão, ramo), GPU (nome, arquitetura, VRAM, capacidades com refresh e VRR reais), preferências, nº de jogos, última aplicação, alertas de deriva, avisos (inclusive os do plano), adiamento |
| `obter_plano` | diff atual; status `pendente`, `ja_no_valor` ou `travada`; reversões pendentes, jogos ignorados e jogos com fatos curados, `em_execucao` e orientações |
| `aplicar_agora` | reativa a autonomia, trata a deriva, aplica; devolve o plano com `aplicada`/`falhou` (também nas reversões). Jogo aberto: plano com `adiada` |
| `restaurar_original` | pausa a autonomia e restaura o snapshot original; `restauracao` (com `adiada = null`) também sem snapshot. Jogo aberto: `restauracao` com `adiada` |
| `definir_preferencias` | com autonomia, trata antes a deriva pendente; grava e, com autonomia, aplica o novo plano (jogo aberto: aplicação pendente, resposta `adiada`); sem autonomia, recalcula o plano só para os avisos e responde `ok` |
| `listar_historico` | mais recentes primeiro; ação: plano/criação de perfil → `aplicacao`, reaplicação → `deriva_detectada`, mudança manual/trava → `travamento`, restauração → `restauracao` |
| `travar_setting` | vira escolha do usuário (sai do estado esperado, sem gravar no driver); `"global"` = perfil base; o nome do perfil é normalizado para o canônico do DRS (sem diferenciar maiúsculas: todas as grafias viram a mesma trava); setting fora do catálogo → `setting_nao_permitida`; perfil inexistente ou não gerenciável (não é de um jogo detectado, não foi criado pelo app nem tem setting gerida ou trava) → `perfil_nao_encontrado`; acima de `MAX_TRAVADAS` (500) → `interno` com “limite de 500 escolhas travadas atingido” |
| `destravar_setting` | perfil sem diferenciar maiúsculas; com autonomia, trata antes a deriva pendente, devolve ao automático e aplica (jogo aberto: aplicação pendente, resposta `adiada`) |

**Deriva pendente antes do plano:** `aplicar_agora`, `definir_preferencias`
e `destravar_setting` chamam `verificar_deriva`/`tratar_deriva` antes de
`aplicar_plano` (como o ciclo). Sem isso, uma mudança manual feita no NPI e
ainda não vista pelo ciclo seria tratada como valor do app e revertida pelo
plano novo (ex.: limite 141 → usuário põe 100 → limite desligado ⇒ antes
ficava sem limite; agora 100 vira trava do usuário).
| `reescanear_jogos` | nova varredura + ciclo |

Requisições da camada Windows (v5; ver “Camada Windows” abaixo), todas
vinculadas à identidade do token do cliente:

| Requisição | Ação |
|---|---|
| `agente_consultar` | registra presença e fatos da sessão, avalia a camada Windows do SID e devolve `plano_agente` (revisão, geração de restauração, tarefa tipada e processos vigiados) |
| `agente_relatar` | valida SID/sessão, tarefa, revisão e tipos; aceita repetição idempotente; grava o recibo e responde `ok` |
| `definir_aceite_experimental` | grava o aceite ou a retirada da regra para o SID chamador; se for regra de driver e o chamador estiver no console ativo, reavalia o DRS sob o portão |
| `reverter_ajuste_usuario` | cria uma trava no destino e agenda a volta ao valor de antes da gestão |

Capacidades no estado: `dlss` = Tensor Cores; `geracao_quadros` = FG da
arquitetura; `reflex` = arquitetura reconhecida; `rebar` = ativo, se lido,
senão suportado; `refresh_mhz` e `vrr { possivel, ativo }` do monitor
principal, lidos via NVAPI (`NvAPI_DISP_GetDisplayConfig`,
`NvAPI_Disp_GetVRRInfo`; desconhecido = `null`).

Jogos no plano: jogo casado na base curada (`regras/jogos.toml`) leva
`competitivo` (id da base) e `reflex`/`anticheat_kernel` da base (que vence a
detecção em disco); `orientacoes` são os códigos de `ResumoJogo::orientacoes`
(já filtrados pelas preferências). Os motivos `condicao_nao_atendida` de
`vrr_ativo = true` e de `jogo_reflex = false` saem como `vrr_inativo` e
`jogo_com_reflex`.

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
- **Avisos do plano** (`Plan::avisos`) do último plano calculado:
  `GsyncSemVrr` → `gsync_sem_vrr`, `VrrDesligadoNoPainel` →
  `vrr_desligado_no_painel` (nível `atencao`); registrados no log quando
  mudam.

O plano IPC traz também `reversoes` (de `Diff::reverter`, com o motivo
derivado da origem da setting: regra ou preferência) e `ignorados`. Cada
reversão aplicada é registrada no log; a não confirmada, como aviso.
`status` e `dry-run` imprimem avisos, reversões e jogos ignorados.

Erros do motor: `ProfileNotFound` → `perfil_nao_encontrado`; demais erros
do DRS e GPU ausente → `driver_indisponivel`; ID/valor fora da lista →
`setting_nao_permitida`; estado/regras → `interno`; `Adiada` que escape da
conversão em pendência → `ocupado` (não executado).

## Camada Windows (Fase 2)

O serviço é o **único avaliador** também da camada Windows, mas não grava
nela: quem grava é o `nvt-agente` na sessão interativa (decisão D4).

### Coordenador por usuário

`src/usuario.rs` mantém, por SID e sessão, os fatos recebidos do agente, a
revisão do plano, a tarefa em voo e a presença (expira em 3 min sem
contato). O coordenador continua ativo enquanto o serviço espera o driver:
sem NVIDIA ou com o DRS indisponível, só o trabalho de driver fica suspenso,
e a camada Windows e os diagnósticos continuam consultáveis. Sem agente
presente ou sem fatos válidos, nenhuma tarefa é emitida; uma restauração com
snapshot continua pendente até ser atendida.

A gestão (valor de antes da gestão, esperado e travas) da camada Windows tem
uma única fonte de verdade: o estado do agente em `%LOCALAPPDATA%`. O
serviço não a grava em `usuarios.json`; a cada `agente_consultar` ele
reidrata a sessão com `fatos.gestao` (tipos conferidos contra o destino,
só para o próprio SID e sessão, sem apagar o que já conhece). Assim reboot,
logoff/logon e reinício do serviço não deixam ajuste órfão: retirar o aceite
ainda emite `Restaurar` e a reversão individual volta ao valor de antes da
gestão, inclusive “não existia”. Antes da primeira consulta após o reinício,
a reversão é recusada até o agente reportar a gestão.

O serviço não emite `encerrar` na desinstalação: o SCM manda o mesmo `Stop`
de uma parada comum e o pipe fecha logo em seguida. O próprio agente sai
quando o serviço fica indisponível por 2 min e o seu executável ou a
entrada `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run\nv-tuner-agente`
não existe mais (só leitura; erro de leitura conta como presente).

### Aceites por SID

Os aceites de regras experimentais ficam nas preferências por SID em
`usuarios.json` (conjunto inicialmente vazio; mudar outras preferências o
conserva). A camada Windows usa os aceites do próprio SID, inclusive em RDP.
Como o DRS é compartilhado pela máquina, **só os aceites do SID da sessão de
console ativa** entram no plano do driver: o serviço obtém essa identidade
pela sessão de console do Windows (`identidade_console`). Troca de console
ou identidade indisponível reavalia o DRS sob o portão; sem SID elegível, o
conjunto vale vazio. A UI indica quando um aceite de driver fica inativo.

### Limite de FPS no jogo e o DRS (F3-13)

O laço repassa ao coordenador as preferências do motor na inicialização e a
cada atendimento ou tick (`Coordenador::definir_preferencias`; mudança de
`limite_fps` invalida as tarefas de jogo em voo). Junto com os aceites DRS,
o laço obtém `Coordenador::limites_no_jogo_efetivos` do **SID da sessão de
console** e o entrega ao núcleo; ao montar o plano DRS, o núcleo aplica
`nvt_core::desligar_limite_drs` e reclassifica o `limite_fps` do perfil do
jogo contra o driver (`off` no lugar do limite do usuário). Quando a lista
muda (relato do agente com sucesso ou falha, aceite, troca de console), o
DRS é reavaliado sob o portão. Arquivo do jogo com falha, adiado ou ainda
não aplicado não entra na lista: o limite do driver permanece. Outros SIDs
não afetam o DRS.

### Diagnósticos

Os diagnósticos de máquina (HAGS, MPO, plano de energia, CPU X3D, MSI da
GPU, VBS/HVCI/IOMMU, TPM, Secure Boot, build) são lidos por uma thread
própria (`nvt-diagnosticos`), fora do atendimento do pipe, e guardados em
cache, renovado a cada 10 min; `obter_estado` devolve o cache. Os de sessão
(Game Mode, VRR do Windows, FSO por jogo conhecido) chegam pelo agente. A
interface usada só permite leitura; nada é alterado, nem o BCD.

### Restauração por geração persistida

`servico.json` guarda `geracao_restauracao` junto com a intenção de
restauração e a pausa da autonomia. Cada pedido efetivo de
**Restaurar original** (UI ou `restore-original --yes`) incrementa a geração
uma única vez, antes de confirmar ao cliente; repetir a intenção pendente não
avança de novo. Um agente que relata geração anterior recebe primeiro a
tarefa de restaurar seu snapshot, antes de qualquer aplicação, mesmo depois
de “Aplicar agora” ou no próximo logon. O DRS mantém seu próprio retry sob o
portão; o estado do agente mostra separadamente a pendência do usuário.

## Atualização automática

Especificação: [`docs/spec/atualizacao.md`](spec/atualizacao.md). Código:
`crates/nvt-atualizacao` (núcleo puro: manifesto, Minisign, versões, política e
máquina de estados `transicao`) e `crates/nvt-service/src/atualizacao/`
(`mod.rs` = `Atualizador`; `fachadas.rs`, `http.rs` WinHTTP, `arquivo.rs`,
`lancador.rs`). Não há subcomando, flag, chave de registro nem requisição IPC que
a desligue; só `--somente-leitura` não sobe o atualizador.

### Thread do atualizador e exclusão com o ciclo DRS

`Integracao::iniciar_atualizacao` (`laco.rs`) sobe a thread `nvt-atualizacao`
com as fachadas reais. `Atualizador::tick` é síncrono (rede e hash podem levar
minutos) e devolve o próximo prazo; a thread dorme até ele, acorda cedo no stop
(`cancelar`, que também faz a rede desistir) e recebe do laço as sessões com
agente. Um pânico no `tick` é contido e desliga a atualização até o próximo
início do serviço. O laço lê só a última projeção publicada (`estado.atualizacao`
do IPC v6; `nao_vista` é calculado por SID pelo coordenador).

A exclusão com o ciclo DRS é a `Exclusao` de `laco.rs`: a thread principal marca
`drs` durante cada requisição, notificação e temporizador do núcleo; o portão do
atualizador primeiro **reserva a instalação** (espera o ciclo DRS em curso
terminar) e só então consulta o mesmo `nvt_engine::Portao`, com a base curada e
os extras atuais. Reservada, nenhum ciclo DRS começa até o fim do `tick`. Portão
fechado (jogo, anticheat, lista ilegível, base indisponível) → fase `pronta` e
reavaliação a cada 60 s; mutex `Global\_MSIExecute` presente → 30 min.

### Pasta protegida e estado

Tudo fica em `%ProgramData%\nv-tuner\atualizacao\` (pasta obtida por
`SHGetKnownFolderPath`, independente de `--estado-dir`), criada com dono
Administradores e DACL protegida `D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)`, sem
leitura de Usuários; nenhum componente do caminho pode ser reparse point.

| Arquivo | Conteúdo |
|---|---|
| `estado.json` | estado persistido (§6.3 da spec): `maior_vista`, verificações, espera de rede, falhas por versão, abandonadas, `fase`, `ultima_atualizacao`, `ultima_falha` |
| `estado.json.bak` | mesma gravação, a cada mudança; se `estado.json` estiver ilegível, recupera ao menos `maior_vista` (nunca abaixo da versão instalada) |
| `nv-tuner-X.Y.Z-x64.msi(.parcial)` | download; `.parcial` criado com `CREATE_NEW`, sem reparse, compartilhamento só de leitura |
| `instalacao-X.Y.Z.log` | log `/l*v` do msiexec |

Gravação por arquivo privado + troca atômica; falha ao persistir desfaz a
transição em memória e nada é lançado. No início, a limpeza apaga `.parcial`,
MSIs e logs que não sejam da última instalação, sem seguir links.

O MSI é verificado (tamanho e SHA-256) lendo pelo **mesmo handle** que o
escreveu, renomeado pelo handle e trocado por um handle só de leitura que nega
escrita e exclusão a terceiros (via handle intermediário; o hash é recalculado no
handle final). Esse handle fica aberto até o serviço parar, a fase sair de
`pronta`/`instalando` ou 30 min após o lançamento.

### Portão antes do lançamento e o lançamento

Imediatamente antes de lançar: reserva contra o DRS, portão, persistência de
`fase = instalando` com `sessoes_agente` (`[{sessao, sid}]` das sessões com agente
conectado). Só então `lancador::lancar_msiexec` (única exceção à regra “nenhum
processo externo”, ver [`docs/arquitetura.md`](arquitetura.md#segurança-do-serviço)).
O serviço **não guarda handle do msiexec** nem espera por ele; o registro do
lançamento (de/para, SHA-256, caminhos, PID, flags, breakaway recusado, horário)
vai para o log do serviço.

### Depois do upgrade

O msiexec para o serviço (Restart Manager fecha agente e UI), troca os arquivos e
inicia o serviço novo. Com `fase = instalando`, o `Atualizador` lê o código da
linha `MainEngineThread is returning N` no log `/l*v` (até 8 MiB do fim, sem
reparse) e, sem código, presume sucesso se a própria versão for a de destino:

- 0/3010 (3010 = reinício pendente, só informado) → `ultima_atualizacao`, limpa
  as falhas da versão e relança o agente;
- 1618 → adia 30 min sem contar; 1625 → abandona; outros → conta falha (3ª
  abandona a versão até surgir uma maior);
- versão antiga e log ainda sem código (msiexec terminando, ou rollback) → relê o
  log a cada 60 s por até 2 h; depois disso conta falha com código `-1`.

O relançamento (`lancador::relancar_agente`) ocorre uma vez, só nas sessões
gravadas que estão ativas, sem agente presente e cujo token tem o mesmo SID; a
lista é apagada (persistida) antes. Falha só é registrada: o agente volta no
próximo logon pela entrada `Run`.

`status` mostra a versão instalada, a fase, a última e a próxima verificação, o
último resultado, a última atualização e a última falha, lendo o estado
persistido.

## Estado e logs

Em `<estado-dir>`:

| Arquivo | Dono |
|---|---|
| `snapshot-original.json`, `estado.json`, `estado.anterior.json`, `perfis-criados.json` | `nvt-engine` (`docs/engine.md`) |
| `snapshot-perdido.json` | gravado por `preparar` logo depois de uma quarentena; lido pelo `nvt-engine` |
| `servico.json` | serviço: `autonomia_pausada`, `pausada_em_unix`, `ultima_aplicacao`, `restauracao_pendente_desde_unix` (restauração adiada a retomar; ausente nos arquivos antigos), `geracao_restauracao` (Fase 2) |
| `usuarios.json` | coordenador de usuário: preferências por SID (aceites experimentais) e recibos de histórico da camada Windows; ACL restrita a SYSTEM/Administradores, fora dos arquivos legíveis pela UI local |
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
- jogo aberto (processos falsos via `Nucleo::novo_com_portao`): aplicação
  automática, `aplicar_agora`, verificação periódica e mudança no DRS com
  **zero** gravações e salvamentos no `DriverFalso`; reavaliação a cada 15 s;
  ao fechar, a ação roda uma vez (um único salvamento) e não se repete;
  `RestaurarOriginal` com anticheat aberto vence `AplicarAgora` e
  preferências posteriores; deriva com jogo aberto é lida (alertas) e o
  tratamento fica pendente, substituído pelo pedido mais recente; pendência
  com erro de driver é tentada de novo e, na 3.ª falha, descartada com o
  aviso `acao_pendente_descartada`; restauração pendente sobrevive ao
  reinício (jogo ainda aberto: segue pendente; jogo fechado: roda no
  início) e sai de `servico.json` depois de feita; preferências e destravar
  com jogo aberto respondem `adiada`; lista de processos ilegível vai como
  `lista_indisponivel`;
- CLI: restauração ou aplicação adiada pelo serviço sai com código 3 e a
  mensagem de desinstalação; no próprio processo, com 1;
- estado com refresh e VRR reais e o aviso `vrr_desligado_no_painel`;
  plano com fatos curados (`competitivo`, Reflex/anticheat da base,
  orientações);
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
