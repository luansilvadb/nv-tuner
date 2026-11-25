# IPC entre UI e serviço

A UI (processo comum, sem privilégios) conversa com o serviço do Windows
(privilegiado) por um named pipe local. O protocolo é implementado no crate
`nvt-ipc`, que não depende do motor: os DTOs são próprios e o serviço os
converte para o seu modelo interno.

## Princípios

- **Só comandos de alto nível.** A UI pede "aplicar agora", "restaurar",
  "travar a setting X do perfil Y" — nunca "grave o valor V no ID DRS N".
- **Settings por nome simbólico.** No protocolo, uma setting é uma string
  como `"vsync"` ou `"dlss_preset"`. Só o serviço traduz o nome para um ID
  DRS, consultando a sua **lista permitida**; nome fora da lista resulta em
  `setting_nao_permitida`.
- **Valores só para exibição.** `antes`/`depois` em planos e histórico são
  textos legíveis; a UI não envia valores de settings.

## Transporte

- Pipe: `\\.\pipe\nv-tuner` (constante `NOME_PIPE`).
- Modo byte; o enquadramento é feito pelo protocolo.
- Uma instância; conexões atendidas **em sequência**. Um cliente que chega
  enquanto outro está conectado espera (`WaitNamedPipeW`) até o tempo limite
  de conexão.
- Tempos limite padrão:

  | Lado     | Conexão | Leitura | Escrita |
  |----------|---------|---------|---------|
  | Servidor | —       | 30 s    | 5 s     |
  | Cliente  | 2 s     | 60 s    | 5 s     |

  No servidor, o tempo limite de leitura também vale para o intervalo ocioso
  entre requisições: um cliente parado é desconectado e não bloqueia os
  demais. O cliente espera até 60 s porque aplicar um plano pode demorar.
- Limites de uso do servidor (`ConfigServidor`), contra abuso por um
  processo local:

  | Limite | Padrão | Ao exceder |
  |---|---|---|
  | Duração de uma conexão | 5 min | nenhuma leitura nova; a conexão é encerrada (vale também para quem manda um byte por vez) |
  | Requisições por conexão | 1000 | a seguinte recebe `ocupado` e a conexão é encerrada |
  | Taxa (balde de fichas do servidor) | 10/s, rajada de 20 | `ocupado` sem chamar o tratador; a conexão continua. O balde não se renova ao reconectar |

  A UI abre uma conexão por requisição e fica muito abaixo desses limites.
- A espera de uma parada (`Parada`) devolve `ConnectionAborted`, e não
  `Interrupted`, para que leituras com repetição automática não girem em
  falso depois da parada.
- O servidor pode ser parado a qualquer momento por um sinal (`Parada`), que
  interrompe tanto a espera por conexão quanto leituras em andamento.

### Por que `windows-sys` e não tokio

O serviço atende uma conexão por vez e a UI usa um cliente bloqueante em
thread de fundo; um runtime assíncrono não traria ganho e aumentaria o
tamanho e a superfície do binário. A implementação usa `windows-sys` com E/S
sobreposta (*overlapped*) apenas para ter **tempos limite** e **parada
cancelável** — o que a E/S síncrona de pipes não oferece — e porque o
descritor de segurança, `PIPE_REJECT_REMOTE_CLIENTS`,
`FILE_FLAG_FIRST_PIPE_INSTANCE`, a qualidade de serviço do cliente e a
verificação do dono do pipe exigem chamadas Win32 de qualquer forma.

## Enquadramento

```
+----------------------+---------------------------+
| tamanho: u32 (LE)    | payload: JSON (UTF-8)     |
+----------------------+---------------------------+
```

- Tamanho máximo do payload: **1 MiB** (`TAMANHO_MAXIMO`). Um cabeçalho acima
  do limite é recusado antes de qualquer alocação; o servidor responde
  `mensagem_grande` e encerra a conexão (o fluxo perde o alinhamento).
- Conexão encerrada no meio de um quadro = mensagem truncada; o servidor
  descarta a conexão.

## Envelope e versão

```json
{"versao": 3, "corpo": { "tipo": "...", ... }}
```

- `versao` é conferida **antes** de interpretar `corpo`. Versão diferente de
  `PROTOCOL_VERSION` é recusada: o servidor responde
  `versao_incompativel` (com a sua própria versão no envelope), e o cliente
  reporta `ErroIpc::VersaoIncompativel { recebida, esperada }` mesmo que o
  corpo lhe seja desconhecido.
- Envelope com campos extras, sem `versao` ou sem `corpo` é inválido.
- A versão é incrementada a cada mudança incompatível; não há negociação.
  UI e serviço são distribuídos juntos no mesmo instalador. Como todos os
  DTOs usam `deny_unknown_fields`, **acrescentar campo também é
  incompatível**.
- Ao receber `VersaoIncompativel`, a UI entra no modo de visualização e
  mostra um alerta com as duas versões e a instrução de atualizar o nv-tuner
  pelo instalador (que atualiza UI e serviço juntos).

| Versão | Mudança |
|---|---|
| 1 | protocolo inicial |
| 2 | `estado.avisos`; `plano.reversoes` e `plano.ignorados` |
| 3 | `plano.jogos` (fatos por jogo e explicação: regras aplicadas e não aplicadas com motivo, herança do global) |

## Requisições (`corpo.tipo`)

| Tipo                    | Campos                                                     |
|-------------------------|------------------------------------------------------------|
| `obter_estado`          | —                                                          |
| `obter_plano`           | —                                                          |
| `aplicar_agora`         | —                                                          |
| `restaurar_original`    | —                                                          |
| `definir_preferencias`  | `sincronismo`, `qualidade`, `limite_fps` (número ou `null`)|
| `listar_historico`      | `limite` (1 a 1000)                                        |
| `travar_setting`        | `perfil`, `setting`                                        |
| `destravar_setting`     | `perfil`, `setting`                                        |
| `reescanear_jogos`      | —                                                          |

- `sincronismo`: `desligado` \| `v_sync` \| `g_sync`.
- `qualidade`: `desempenho` \| `equilibrado` \| `qualidade`.
- `limite_fps`: 20 a 1000, ou `null` (sem limitador).
- `perfil`: 1 a 256 caracteres, sem caracteres de controle (`"global"` =
  perfil base).
- `setting`: 1 a 64 caracteres em `[a-z0-9_.]`, começando por letra.

Antes de chamar o tratador, o servidor aplica essa **validação de forma**
(`Requisicao::validar`) e responde `requisicao_invalida` se falhar. A
validação semântica (lista permitida, existência do perfil) é do serviço.

Exemplo:

```json
{"versao":3,"corpo":{"tipo":"travar_setting","perfil":"WARDOGS","setting":"dlss_preset"}}
```

## Respostas (`corpo.tipo`)

| Tipo        | Conteúdo                                                        |
|-------------|-----------------------------------------------------------------|
| `estado`    | `versao_servico`, `driver`, `gpu` (com `capacidades`), `preferencias`, `jogos`, `ultima_aplicacao`, `alertas_deriva`, `avisos` |
| `plano`     | `gerado_unix`, `perfis[]` → `perfil`, `nome_legivel`, `mudancas[]`; `reversoes[]`; `ignorados[]`; `jogos[]` |
| `historico` | `entradas[]`                                                    |
| `ok`        | —                                                               |
| `erro`      | `codigo`, `mensagem`                                            |

Cada mudança do plano traz `setting`, `nome_legivel`, `antes`, `depois`,
`motivo`, `regra`, `impacto_visual` (`nenhum` \| `baixo` \| `medio` \|
`alto`) e `status` (`pendente` \| `aplicada` \| `ja_no_valor` \| `travada` \|
`recusada` \| `falhou`).

Cada entrada do histórico traz `id`, `quando_unix`, `acao` (`aplicacao` \|
`restauracao` \| `rollback` \| `travamento` \| `destravamento` \|
`preferencias` \| `deriva_detectada`), `perfil`, `setting`, `antes`, `depois`
e `descricao`.

Alertas de deriva indicam settings cujo valor real no driver diverge do
último valor aplicado: `perfil`, `setting`, `nome_legivel`, `esperado`,
`encontrado`, `detectado_unix`.

### Avisos (`estado.avisos`)

Problemas que o usuário precisa ver, do mais grave para o mais recente
(`ordenar_avisos`): `nivel` (`info` \| `atencao` \| `erro`), `codigo`
(estável), `mensagem` (pt-BR, para exibição), `detalhe` (texto técnico
opcional, ex.: a frase original do motor) e `quando_unix`.

| `codigo` | Nível | Quando |
|---|---|---|
| `snapshot_indisponivel` | erro | `snapshot-original.json` ilegível: **restauração desabilitada** (a UI desabilita o botão e mostra a mensagem) |
| `erro_perfil_aplicacao` | erro | erro isolado num perfil de jogo na última aplicação (`detalhe` = erro) |
| `erro_perfil_deriva` | erro | erro isolado num perfil de jogo no último tratamento de deriva |
| `estado_corrompido` | atencao | `estado.json` inválido, guardado à parte como `.corrompido-<data>` |
| `estado_reiniciado` | atencao | sem cópia válida: o estado recomeçou limpo |
| `jogos_ignorados` | atencao | jogos fora do último plano (`detalhe` = “nome: motivo; …”) |
| `estado_aviso` | atencao | outro problema ao abrir/ler o estado |
| `snapshot_original_perdido` | atencao | marcador `snapshot-perdido.json`: o diretório de estado foi para a quarentena; o snapshot atual (ou o próximo) é uma **base pós-quarentena** e “voltar ao original” volta a ela |
| `estado_recuperado` | info (atencao se `estado.json` estava ausente) | estado recuperado de `estado.anterior.json` |

O código é a interface; a UI só trata `snapshot_indisponivel`
(`Estado::restauracao_bloqueada`) e exibe os demais. Para
`snapshot_original_perdido` ela mostra um texto próprio
(`textos::SNAPSHOT_ORIGINAL_PERDIDO`) no lugar da `mensagem`. Um código
desconhecido continua sendo exibido.

Todo `CodigoAviso` do motor tem constante em `nvt_ipc::codigo_aviso` com o
mesmo texto de `como_str()`; o teste
`todo_codigo_do_motor_tem_constante_no_protocolo_e_texto_na_ui` (`nvt-ui`)
garante isso e que a UI tem texto para cada um (`textos::texto_aviso_motor`,
correspondência exaustiva).

### Reversões e jogos ignorados (`plano`)

`reversoes[]`: settings que o app gravou e que o plano atual não quer mais
(preferência trocada, teto de qualidade, regra que deixou de casar). Cada
uma traz `perfil`, `nome_perfil`, `setting`, `nome_legivel`, `atual` (valor no
driver), `original` (valor que volta; `null` = sai do perfil), `acao`
(`gravar_original` \| `remover` \| `so_deixar_de_vigiar`), `motivo` (por que
saiu do plano) e `status` (`pendente` no `obter_plano`; `aplicada` ou
`falhou` na resposta de `aplicar_agora`).

`ignorados[]`: jogos deixados de fora por um erro só deles (nome/executável
inválido, falha da NVAPI no perfil): `nome`, `exe` (nome do arquivo) e
`motivo`. Os demais jogos e o perfil global seguem.

### Jogos (`plano.jogos`)

Jogos do plano (os ignorados ficam em `ignorados`), com os fatos detectados e
o que o app faz em cada um:

| Campo | Conteúdo |
|---|---|
| `nome`, `perfil` | nome no launcher; perfil do driver (existente ou a criar) |
| `origem_perfil` | `driver` \| `usuario` \| `criado_pelo_app` \| `a_criar` |
| `launcher` | ex.: `Steam`, `Riot` (vazio se desconhecido) |
| `api` | `dx9` \| `dx11` \| `dx12` \| `vulkan` \| `opengl` \| `desconhecida` |
| `dlss` | versão; `null` = sem DLSS; `""` = DLSS com versão ilegível |
| `reflex`, `anticheat_kernel` | `true`/`false`; `null` = não determinado |
| `resumo.aplicadas[]` | `regra`, `descricao`, `settings[]` (nomes simbólicos) |
| `resumo.nao_aplicadas[]` | `regra`, `descricao`, `motivo` |
| `resumo.herda_do_global[]` | settings do perfil global que o jogo herda |

`motivo` é um objeto com `tipo` (a UI gera o texto; semântica em
`docs/regras.md`, “Explicação por jogo”):

| `tipo` | Campos |
|---|---|
| `jogo_sem_dlss` | — |
| `arquitetura_nao_precisa` | `arquitetura` (`ada`, `blackwell`…) |
| `sem_tensor_cores` | — |
| `experimental` | — |
| `bloqueada_pelo_teto_de_qualidade` | `teto` (`desempenho` \| `equilibrado` \| `qualidade`) |
| `escolha_do_usuario` | `setting` |
| `conflito_com_regra_mais_especifica` | `regra` (id ou `preferencia:<nome>`) |
| `fato_desconhecido` | `fato` (nome da condição, ex.: `jogo_reflex`) |
| `condicao_nao_atendida` | `condicao`, `esperado`, `encontrado` (forma do TOML) |

Exemplo:

```json
{"nome":"WARDOGS","perfil":"WARDOGS","origem_perfil":"driver","launcher":"Steam",
 "api":"dx12","dlss":"310.6","reflex":true,"anticheat_kernel":true,
 "resumo":{"aplicadas":[{"regra":"dlss-preset-e-turing","descricao":"…",
   "settings":["dlss_override","dlss_perfil_preset","dlss_preset"]}],
  "nao_aplicadas":[{"regra":"dlss-preset-e-ampere","descricao":"…",
   "motivo":{"tipo":"condicao_nao_atendida","condicao":"arquitetura",
             "esperado":"ampere","encontrado":"turing"}}],
  "herda_do_global":["aa_modo","af_modo","…"]}}
```

O serviço monta a lista a partir do `Diff` (`nucleo::jogos_ipc`); o modo de
visualização da UI faz a mesma conversão localmente (`local::jogos_ipc`).

Datas são segundos desde a época Unix (UTC).

### Códigos de erro

| Código                  | Significado                                       |
|-------------------------|---------------------------------------------------|
| `versao_incompativel`   | versão do protocolo diferente da do serviço       |
| `mensagem_grande`       | quadro acima de 1 MiB                             |
| `mensagem_invalida`     | JSON/envelope inválido ou tipo desconhecido       |
| `requisicao_invalida`   | campo fora de faixa ou com formato inválido       |
| `setting_nao_permitida` | setting fora da lista permitida                   |
| `perfil_nao_encontrado` | perfil inexistente no driver                      |
| `driver_indisponivel`   | NVAPI/driver indisponível                         |
| `ocupado`               | outra operação em andamento ou limite de taxa; o pedido **não** foi executado |
| `interno`               | falha interna (inclui pânico do tratador e diretório de estado inseguro) |

O código é estável; a mensagem serve só para exibição.

## Política de segurança

### Descritor de segurança do pipe

```
D:P(D;;GA;;;NU)(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x0012008b;;;AU)
```

| ACE                     | Efeito                                                   |
|-------------------------|----------------------------------------------------------|
| `D:P`                   | DACL protegida, sem herança                              |
| `(D;;GA;;;NU)`          | **nega tudo** a `NT AUTHORITY\NETWORK` (logons de rede)  |
| `(A;;GA;;;SY)`          | SYSTEM: acesso total                                     |
| `(A;;GA;;;BA)`          | Administradores: acesso total                            |
| `(A;;0x0012008b;;;AU)`  | Usuários autenticados: `FILE_GENERIC_READ \| FILE_WRITE_DATA` |

Usuários autenticados **não** recebem `FILE_APPEND_DATA`, que em pipes
equivale a `FILE_CREATE_PIPE_INSTANCE`: assim não podem criar instâncias
concorrentes do pipe para se passar pelo serviço. Anônimos e convidados não
estão em `AU` e ficam sem acesso. O cliente abre o pipe pedindo exatamente
`FILE_GENERIC_READ | FILE_WRITE_DATA`.

### Demais proteções

- `PIPE_REJECT_REMOTE_CLIENTS`: conexões vindas de outra máquina são
  recusadas pelo kernel, além do ACE de negação a `NU`.
- `FILE_FLAG_FIRST_PIPE_INSTANCE`: o serviço falha ao iniciar se já existir
  um pipe com o mesmo nome, em vez de dividir o nome com um impostor.
- **Verificação do dono pelo cliente**: antes de enviar qualquer coisa, o
  cliente confere se o dono do objeto pipe é SYSTEM ou Administradores. Um
  processo sem privilégios só cria objetos cujo dono é ele mesmo; isso
  impede que um programa comum que crie `\\.\pipe\nv-tuner` enquanto o
  serviço está parado receba os comandos da UI
  (`ErroIpc::ServidorNaoConfiavel`).
- O cliente conecta com `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`:
  o servidor pode identificar o usuário, mas não personificá-lo.
- Limite de 1 MiB por mensagem, tempos limite de leitura/escrita, duração
  máxima da conexão, requisições por conexão, taxa por segundo e uma
  conexão por vez limitam o consumo de recursos por um cliente malicioso.
- Pânicos do tratador são capturados e viram `erro`/`interno`; o servidor
  continua atendendo. No `nvt-service`, o tratador do pipe só repassa a
  requisição à thread principal; lá, um pânico em `Nucleo::tratar` (ou nos
  temporizadores) também é capturado e registrado no log, a requisição
  recebe `interno`, e o núcleo é descartado e recriado do disco
  (`docs/servico.md`). O serviço não cai.
- Validação de forma no transporte e validação semântica (lista permitida,
  tipo e faixa dos valores) no serviço, conforme `docs/arquitetura.md`.

## Testes

- Unitários: ida e volta de todas as requisições e respostas, formato JSON
  estável (inclusive de avisos, reversões, jogos ignorados, jogos e todos os
  motivos de regra não aplicada; plano da versão 2 recusado), ordenação dos
  avisos e `restauracao_bloqueada`, validação de forma, enquadramento
  (quadro truncado, tamanho acima do limite na leitura e na escrita, versão
  errada, envelope malformado).
- Integração (`tests/pipe_local.rs`): servidor e cliente no mesmo processo,
  sem privilégios, com um tratador falso e um nome de pipe único por teste.
  Cobrem conexões sequenciais e concorrentes, erros pelo pipe (versão, JSON,
  quadro grande, truncado, cliente ocioso, pânico do tratador), limites de
  taxa, de requisições por conexão e de duração da conexão (cliente que
  manda um byte por vez), parada, segunda instância
  recusada, ACL sem `FILE_CREATE_PIPE_INSTANCE` e recusa de dono sem
  privilégio. Os dois últimos só fazem asserções quando o processo **não**
  está elevado.
