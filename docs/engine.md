# Motor (`nvt-engine`)

O `nvt-engine` liga a detecção (`nvt-drs` + `nvt-detect`) ao avaliador
(`nvt-core`) e ao driver, com a rede de segurança decidida em
`docs/arquitetura.md`: só regras estáveis entram no plano, o estado original
é guardado antes da primeira mudança, toda gravação é relida e registrada com
motivo.

## Acesso ao driver: `DriverStore`

Todo acesso ao DRS passa pelo trait `DriverStore`:

| Operação | Uso |
|---|---|
| `versao_driver`, `gpus` | capacidades e detecção de atualização de driver |
| `perfil_do_exe` | `find_application` pelo exe principal do jogo |
| `perfil_existe`, `criar_perfil`, `criar_aplicacao` | jogo sem perfil |
| `nome_canonico` | nome do perfil como o DRS o guarda (a busca por nome não diferencia maiúsculas); usado para normalizar o perfil vindo do IPC |
| `ler_setting` | valor **efetivo** (próprio, herdado do base ou padrão do driver) |
| `gravar_setting`, `remover_setting` | escrita (remover = voltar ao predefinido ou apagar) |
| `settings_do_perfil` | settings gravadas no próprio perfil |
| `capturar` | snapshot (`nvt_drs::capture`) |
| `salvar` | `DRS_SaveSettings` (exige administrador no driver real) |
| `exibicao` | refresh e VRR do monitor principal (`nvt_drs::ler_exibicao_principal`); nunca falha, desconhecido vira `None`. A implementação padrão não lê nada (tudo desconhecido) |

Implementações:

- `DrsReal` — sessão DRS real (`DrsReal::abrir(&NvApi)`).
- `DriverFalso` — DRS em memória para testes: herança do perfil base,
  padrões do driver, valores predefinidos, simulação de mudança manual
  (`mudar_pelo_usuario`), de reset (`resetar_settings`) e de atualização
  (`mudar_versao`). Clonar compartilha o estado, para o teste manipular o
  "driver" enquanto o motor o usa.

## Pipeline

1. **Capacidades** (`capacidades`): primeira GPU da NVAPI →
   `nvt_detect::arch_facts` (arquitetura, Tensor Cores, FP8, FG, MFG, ReBAR) +
   VRAM + versão do driver + exibição (`DriverStore::exibicao`;
   `capacidades_com_exibicao` recebe a exibição já lida). A exibição vem só
   da NVAPI, porque o serviço roda na sessão 0: `NvAPI_DISP_GetDisplayConfig`
   acha o caminho cuja origem é o primário GDI e dá o `displayId` e o
   `refreshRate1K` do alvo (`refresh_mhz`, em milésimos de Hz, arredondado
   para Hz só no avaliador); `NvAPI_Disp_GetVRRInfo(displayId)` dá
   `vrr.possivel` e `vrr.ativo`. Falha da NVAPI, função ausente ou monitor
   principal fora da GPU NVIDIA deixam o fato desconhecido (`None`), e as
   condições sobre ele não casam. `rebar_ativo` continua desconhecido.
2. **Jogos** (`DetectorJogos`, `detectar_jogos_da_maquina` /
   `jogos_da_maquina`): varredura do `nvt-detect` → `JogoEntrada` (nome,
   launcher, exe principal, API, DLSS, Reflex, anticheat, pasta, AppID da
   Steam, id no launcher, `id_jogo` da base curada e `em_execucao`). Reflex só
   é afirmado com confiança média ou alta; anticheat de kernel só quando
   detectado (ausência vira "desconhecido").

   **Jogo em execução não tem a pasta lida** (spec competitivo F1-06/F1-09).
   Antes de analisar, o detector monta o conjunto vigiado — exes conhecidos
   de cada jogo (o da varredura anterior e, para jogo da Steam, os
   executáveis e `processos_protegidos` do jogo da base com o mesmo AppID),
   `processos_protegidos` de toda a base, anticheats que só existem durante o
   jogo (`nvt_detect::ANTICHEAT_SO_DURANTE_JOGO`: `EasyAntiCheat.exe`,
   `EasyAntiCheat_EOS.exe`, `start_protected_game.exe`, `BEService.exe`,
   `BEService_x64.exe`) e o exe indicado pelo launcher — e compara com os
   processos do sistema sem diferenciar maiúsculas. Se algum exe do jogo está
   aberto, `analisar_jogo_se_fechado` não abre nem lista nenhum arquivo da
   pasta (DLLs de DLSS, Reflex e anticheat incluídas): a entrada é a da
   varredura anterior (memória do detector, chave = launcher + pasta como
   veio do launcher, sem resolver o caminho no disco) ou, sem ela, fatos
   desconhecidos com o exe aberto como exe principal. A entrada vem com
   `em_execucao = true`, e `ResultadoDeteccao::processos_em_execucao` lista
   os processos vigiados que estavam rodando. O detector do processo
   (`detectar_jogos_da_maquina`) guarda a memória entre chamadas.
3. **Perfis** (`Engine::resolver_jogos`): para cada jogo, `find_application`
   pelo nome do exe. Origem do perfil: `Driver` (predefinido NVIDIA),
   `Usuario` (já existia, criado por outra ferramenta), `CriadoPeloApp` ou
   `ACriar`. Jogo sem perfil recebe um perfil com o nome do jogo (com sufixo
   `(nv-tuner)` se o nome estiver ocupado), criado só se houver mudança a
   fazer. O nome do perfil é o `GameFacts::id`.
   Os fatos do jogo saem de `fatos_do_jogo` sobre o casamento na base curada
   (`casar_na_base`: o `id_jogo` já casado ou `BaseJogos::casar` por exe +
   AppID; `docs/regras.md`, “Base curada de jogos”). Jogo casado:
   `competitivo = true`, Reflex, limitador nativo e anticheat de kernel da
   base (ela vence a detecção em disco) e as orientações dela. Fora da base:
   os fatos da detecção, `competitivo = false`. O motor usa a base embutida
   (`Engine::new`; `Engine::com_base` troca, `Engine::base_jogos` lê).
   Nome e executável vêm de bibliotecas que qualquer usuário pode editar, então
   são conferidos **antes** de qualquer chamada à NVAPI: executável vazio, com
   caractere de controle (inclusive NUL) ou com mais de 260 unidades UTF-16
   (`MAX_EXE`) é recusado; o nome do perfil perde caracteres de controle e é
   truncado em 200 unidades UTF-16 (`MAX_NOME_PERFIL`, folga para o sufixo
   dentro do limite de 2047 do DRS). Qualquer erro de um jogo (validação,
   `find_application`, `FindProfileByName`, leitura do perfil dele) só tira
   esse jogo do plano: ele vai para `Diff::ignorados` com o motivo
   (`Engine::resolver_jogos_isolado`), e os demais e o perfil global seguem.
4. **Avaliação**: `nvt_core::avaliar` com as preferências salvas e as
   escolhas travadas. O plano traz também a explicação por jogo
   (`plan.resumos`, chave = nome do perfil; ver `docs/regras.md`), que segue
   no `Diff` e vai para a UI em `plano.jogos` (`docs/ipc.md`), as
   orientações por jogo já filtradas (`plan.resumos[id].orientacoes`) e os
   avisos de G-SYNC sem VRR (`plan.avisos`: `GsyncSemVrr`,
   `VrrDesligadoNoPainel`). A energia em desempenho máximo só entra no perfil
   de **jogo competitivo** (`energia-desempenho-maximo-por-jogo`, com
   `jogo_competitivo = true`): jogo fora da base não ganha perfil só por ela.
5. **Diff** (`Engine::planejar`, somente leitura): cada mudança do plano é
   comparada com o valor efetivo no driver; só as diferentes entram em
   `Diff::mudancas`, com `valor_anterior` preenchido. As iguais vão para
   `Diff::conformes`. Settings do estado esperado que o plano não quer mais
   vão para `Diff::reverter` (ver “Settings que saíram do plano”).
6. **Aplicação** (`Engine::aplicar(&diff, confirmar)`): **nada acontece sem
   `confirmar = true`**. Ordem: valida o diff inteiro contra a lista
   permitida → snapshot original (se ainda não existe) → cria perfis e
   aplicativos → grava → reverte as que saíram do plano → `salvar` → relê
   cada setting → histórico e estado esperado → versão do driver vista.
   Mudanças cuja releitura não confere vão para `Aplicacao::falhas` e não
   entram no estado esperado. Erro num perfil de jogo (criação, gravação,
   releitura) vai para `Aplicacao::erros` e pula só esse perfil; erro no
   perfil global continua sendo erro da aplicação inteira.

Comparar pelo valor efetivo evita gravações redundantes; a consequência é
que uma setting por jogo herdada do global com o valor certo conta como
conforme.

### Settings que saíram do plano

Uma setting que o app gravou (está no estado esperado) e que o plano atual
não contém mais — preferência trocada (limite de FPS desligado, V-Sync no
lugar de G-SYNC), teto de qualidade mais alto (preset E em “Qualidade”),
regra que deixou de casar ou foi removida (ex.: a antiga energia global,
substituída pela energia por jogo competitivo) — **volta ao valor de antes da
gestão** e sai do estado esperado (`Diff::reverter`, `Aplicacao::revertidas`).

O valor de antes da gestão (`Esperado::antes_da_gestao`, `ValorAntes`) é
lido e guardado por (perfil, setting) no momento em que o app passa a gerir
a setting — antes da primeira gravação, ou ao registrar uma conforme —, e
gravações seguintes do app não o trocam: `Valor(v)` se o perfil tinha um
DWORD próprio, `Inexistente` se não tinha (ausente, herdada, predefinida, ou
perfil criado/inexistente). Assim, um valor que o usuário pôs **depois** do
snapshot e antes de o app gerir a setting é o que volta (primeira aplicação
→ usuário põe limite 90 → preferência 141 → preferência desligada ⇒ 90). Uma
trava do usuário encerra a gestão; se a setting for destravada, a gestão
recomeça e o valor de antes é lido de novo. Estados antigos sem o campo
usam o snapshot:

| Original | Ação (`AcaoReversao`) |
|---|---|
| `antes_da_gestao = Valor(v)` | `Gravar(v)` (nada a gravar se o perfil já tem `v`) |
| `antes_da_gestao = Inexistente` | `Remover` do perfil (ou só deixar de vigiar, se não há valor próprio) |
| *sem `antes_da_gestao` (estado antigo):* | |
| perfil criado pelo app | `Remover` (o perfil não tinha nada) |
| setting presente no snapshot original (ou em `perfis_adicionais`) | `Gravar` o valor original |
| perfil no snapshot sem essa setting | `Remover` do perfil (volta ao predefinido da NVIDIA, se houver) |
| perfil fora do snapshot (só tinha conformes), snapshot indisponível ou perfil que sumiu | `SoDeixarDeVigiar` |

Quando o driver já está como no original, nada é gravado; só deixa de
vigiar. A reversão é registrada no histórico como `Plano` com o motivo
“saiu do plano … revertida ao valor original”. Só entram o perfil global e os
perfis dos jogos presentes no plano: um jogo que sumiu da varredura
(biblioteca desconectada, erro isolado) mantém o que tinha. Sem isso, o
valor continuaria no driver e seria **regravado** no próximo reset em massa.

## Lista permitida

Só IDs do catálogo (`regras/settings.toml`) são gravados. A checagem fica no
motor (`Engine::validar` / `Engine::gravar_permitido`), antes de qualquer
chamada de escrita do `DriverStore`:

- ID fora do catálogo → `ErroEngine::IdNaoPermitido`;
- valor não aceito pelo catálogo (nas mudanças do plano) →
  `ErroEngine::ValorNaoPermitido`.

Um diff com qualquer item inválido é recusado por inteiro, antes do
snapshot e de qualquer gravação. Na restauração, os valores originais são
gravados como estavam (só o ID é conferido).

## Estado persistente

Diretório configurável (`Armazem::abrir(dir)`); padrão
`%ProgramData%\nv-tuner` (`Armazem::diretorio_padrao`). `Armazem::em_memoria`
não grava nada (testes e dry-run).

`Armazem::abrir_somente_leitura(dir)` (usado por `status`, `dry-run` e
`--somente-leitura` do serviço) lê os mesmos arquivos sem nenhum efeito no
disco: nunca renomeia, cria nem grava nada (nem o diretório). Um
`estado.json` inválido fica no lugar e o estado vem de
`estado.anterior.json` (se válido) ou começa limpo, só em memória;
`salvar` e `guardar_snapshot_original` devolvem `ErroEngine::SomenteLeitura`,
e o `Engine` sobre esse armazém recusa do mesmo jeito, antes de qualquer
chamada ao driver, `aplicar`/`tratar_deriva`/`restaurar` confirmados,
`definir_preferencias`, `liberar_travada` e `gravar_permitido`.
`Armazem::somente_leitura` diz o modo.

`estado.json`, `estado.anterior.json` e `snapshot-original.json` são abertos
com `FILE_FLAG_OPEN_REPARSE_POINT` e o atributo do próprio handle é
conferido: link simbólico, junção ou diretório no lugar de um deles é
**recusado** (erro de E/S na abertura; nada é seguido, lido nem renomeado).
Os temporários (`*.tmp`) são removidos e recriados com `create_new` sem
seguir reparse points, então um link plantado no lugar deles nunca recebe
os dados. É defesa em profundidade: o serviço já confere o diretório de
estado inteiro (`docs/servico.md`).

| Arquivo | Conteúdo |
|---|---|
| `snapshot-original.json` | snapshot do DRS antes da primeira aplicação (perfil base + perfis dos jogos detectados). Gravado em `snapshot-original.json.tmp` com `sync_all` e publicado por *hard link* (falha se o destino existir): **nunca é sobrescrito**, nem fica truncado |
| `estado.json` | `preferencias`, `travadas` (escolhas do usuário com data, valor e motivo), `historico` (data, perfil, setting, antes, depois, origem, motivo), `esperado` (último estado aplicado), `driver_visto`, `perfis_criados` e `perfis_adicionais` |
| `estado.anterior.json` | cópia de segurança da última versão válida de `estado.json` |
| `perfis-criados.json` | cópia redundante de `perfis_criados`, regravada quando muda; na abertura é **unida** à lista do estado, então sobrevive à perda de `estado.json` e `estado.anterior.json` (a restauração continua achando e limpando esses perfis) |
| `snapshot-perdido.json` | marcador gravado pelo serviço logo depois de pôr o diretório de estado em quarentena (`Armazem::marcar_snapshot_perdido`); enquanto existir, o snapshot capturado é uma **base pós-quarentena** (ver abaixo) |

Os perfis criados não são marcados no próprio DRS (o nome só ganha o sufixo
` (nv-tuner)` quando há colisão); a identificação depende de
`perfis-criados.json`, que fica no mesmo diretório. Se o diretório inteiro
for para a quarentena, o aviso de snapshot perdido cobre o caso.

`perfis_adicionais` guarda o original de perfis que passaram a ser geridos
depois do snapshot (jogo instalado depois), capturado antes da primeira
gravação neles. `estado.json` é gravado de forma atômica: temporário com
`sync_all` (dados no disco antes do `rename`) → cópia da versão anterior
válida para `estado.anterior.json` → `rename` sobre o destino.

### Arquivo corrompido

`Armazem::abrir` não falha por conteúdo inválido (só por erro de E/S, como
permissão), para o serviço nunca ficar preso:

- **`estado.json` inválido** (vazio, truncado, JSON ruim, versão não
  suportada): é renomeado para `estado.json.corrompido-<AAAA-MM-DDTHH-MM-SSZ>`
  (no modo somente leitura, fica no lugar); usa-se `estado.anterior.json` se
  for válido (lido antes de renomear), senão um estado limpo (com os perfis
  criados de `perfis-criados.json`). No modo normal, o estado recuperado (ou
  limpo) é **gravado na própria abertura**; se a gravação falhar, a abertura
  falha e nada se perde. O snapshot original é preservado.
- **`estado.json` ausente com `estado.anterior.json` válido** (recuperação
  interrompida, arquivo apagado): o estado vem da cópia (aviso
  `EstadoRecuperado`) e é gravado na hora; a abertura nunca volta a um
  estado vazio enquanto houver cópia válida. No somente leitura, só em
  memória.
- **`snapshot-original.json` inválido**: o snapshot fica **indisponível**
  (`Armazem::snapshot_indisponivel` com o motivo). `plano_restauracao` e
  `restaurar` devolvem erro com aviso claro (restauração desabilitada); o
  motor continua aplicando o plano e capturando o original de perfis novos
  em `perfis_adicionais`.

**Decisão registrada:** o snapshot ilegível **não é sobrescrito nem
renomeado automaticamente**. Ele é o único registro do estado do driver antes
do app; um snapshot novo capturado agora já conteria as mudanças do app e
passaria a ser tratado como “original”, e restaurar para ele não desfaria
nada. Um arquivo truncado ainda pode ser recuperado à mão. Cabe a uma pessoa
conferir o arquivo e, se for o caso, removê-lo (o próximo `aplicar` captura
um novo).

### Avisos estruturados

`Armazem::avisos` (e `Engine::avisos`, que os repassa) lista o que aconteceu
na abertura como `Aviso { nivel, codigo, mensagem, detalhe }`:

| `CodigoAviso` | `como_str()` (= `nvt_ipc::codigo_aviso`) | Nível | Quando |
|---|---|---|---|
| `EstadoCorrompido` | `estado_corrompido` | `Atencao` | `estado.json` inválido (movido ou, no somente leitura, mantido) |
| `EstadoRecuperado` | `estado_recuperado` | `Info` | estado lido de `estado.anterior.json` |
| `EstadoReiniciado` | `estado_reiniciado` | `Atencao` | sem cópia válida: estado limpo |
| `SnapshotIndisponivel` | `snapshot_indisponivel` | `Erro` | `snapshot-original.json` ilegível |
| `SnapshotOriginalPerdido` | `snapshot_original_perdido` (ainda sem constante em `nvt_ipc::codigo_aviso`) | `Atencao` | marcador `snapshot-perdido.json`: o diretório foi para a quarentena e o snapshot atual (ou o próximo) é só uma base pós-quarentena |

**Base pós-quarentena (decisão registrada):** quando o diretório de estado
é posto em quarentena, o snapshot original fica lá e não é confiável (pode
ser forjado). O serviço grava `snapshot-perdido.json` no diretório novo; a
próxima aplicação captura o driver atual como snapshot, mas ele **não** é o
estado anterior ao app — pode já ter mudanças do nv-tuner. O aviso
`SnapshotOriginalPerdido` aparece a cada abertura enquanto o marcador
existir, e “voltar tudo ao original” passa a voltar a essa base. Quem
quiser o original de fábrica confere a quarentena ou reinstala o driver;
apagar o marcador à mão encerra o aviso.

`mensagem` é a frase para exibição (pt-BR); `detalhe` é o texto técnico
(motivo do JSON, caminho do `.corrompido-<data>`), que pode mudar entre
versões. Quem consome decide **pelo código**, nunca pelo texto. Um código
novo exige o par correspondente no protocolo (`docs/ipc.md`).

### Histórico

Limitado a `LIMITE_HISTORICO_PADRAO` (2000) entradas, configurável por
`Armazem::definir_limite_historico`; acima do limite as mais antigas são
descartadas (também ao abrir um arquivo maior). Travamentos (`Usuario`)
idênticos ao último registro da mesma setting no mesmo perfil só atualizam a
data desse registro: travar/destravar em laço não faz o histórico crescer.

### Escolhas travadas

O perfil de uma trava é comparado **sem diferenciar maiúsculas**, como o DRS
trata nomes (`mesmo_alvo`, `Estado::travada`): `tratar_deriva` não duplica
uma trava por grafia, `liberar_travada` libera qualquer grafia, e a abertura
deduplica estados antigos. O serviço ainda normaliza o nome para o canônico
do DRS (`DriverStore::nome_canonico`) e limita o total a `MAX_TRAVADAS`
(500), recusando travar acima disso com erro claro (`docs/servico.md`).

## Deriva

`Engine::verificar_deriva` (somente leitura) relê cada setting do estado
esperado. Uma divergência "voltou ao padrão" quando a setting não está mais
gravada pelo usuário no perfil (ausente, herdada ou igual ao predefinido).

Classificação (`classificar_deriva`, pura):

1. **Reset em massa** se a versão do driver mudou desde a última
   aplicação (havendo algo gerido);
2. **Reset em massa** se `revertidas ≥ 3` (`LIMIAR_RESET_MIN`) **e**
   `revertidas ≥ 50 %` (`LIMIAR_RESET_PCT`) das settings geridas;
3. **Reset em massa** se todas as settings geridas foram revertidas e são
   pelo menos duas (conjuntos pequenos);
4. **Pontual** se houver qualquer outra divergência;
5. **Nenhuma** caso contrário.

Mudanças para outro valor gravado nunca contam como revertidas: são ação de
alguém, não reset. Exemplos testados: 3 de 15 → pontual; 7 de 15 →
pontual; 8 de 15 → massa; 2 de 3 → pontual; 3 de 3 e 2 de 2 → massa; 1 de 1
→ pontual.

`Engine::tratar_deriva(&deriva, confirmar)` (só age com `confirmar`):

- reset em massa → regrava as revertidas pela lista permitida, salva, relê
  e registra como `Reaplicacao`; as divergências não revertidas são tratadas
  como pontuais;
- pontual → cada divergência vira `Travada` (escolha do usuário), sai do
  estado esperado e é registrada como `Usuario`; o avaliador passa a
  colocá-la em `plan.respeitadas`. `Engine::liberar_travada` devolve a
  setting ao automático.

Em ambos os casos `driver_visto` passa a ser a versão atual.

Perfil de jogo que sumiu do driver (instalação limpa, apagado no NPI ou no
painel) não derruba o tratamento: no reset em massa, as settings dele saem do
estado esperado (`TratamentoDeriva::removidas`) e o próximo plano recria o
perfil e o aplicativo se alguma regra ainda o exigir. Outros erros de um
perfil de jogo vão para `TratamentoDeriva::erros` (e, na verificação, a
setting fica para o próximo ciclo); o perfil global sempre segue.

## Restauração

`Engine::plano_restauracao` (somente leitura) monta o alvo = snapshot
original + `perfis_adicionais`, captura o estado atual dos mesmos
executáveis e usa `nvt_drs::restore_plan`. Operações sobre IDs fora do
catálogo vão para `Restauracao::ignoradas` (o app nunca gravou nelas e não
as toca). Nos perfis criados pelo app, as settings do catálogo são apagadas
(o perfil em si continua existindo: o `nvt-drs` não expõe `DeleteProfile`).

`Engine::restaurar(confirmar)` aplica, salva, registra cada operação como
`Restauracao` e zera o estado esperado; as escolhas travadas ficam.

**Restauração × reversão:** “voltar tudo ao original” usa sempre o
**snapshot inicial** (o driver como estava antes da primeira aplicação, ou a
base pós-quarentena); um valor que o usuário pôs depois do snapshot é
desfeito também. Já a reversão de uma setting que sai do plano usa o valor
de antes da gestão **daquela** setting (ver “Settings que saíram do
plano”), para não apagar uma escolha do usuário feita antes de o app
passar a geri-la. Quem
chama deve desligar a autonomia em seguida, senão o próximo ciclo reaplica
o plano.

## Dry-run nesta máquina

```text
cargo run -p nvt-engine --example dry_run
```

Roda o pipeline real em modo somente leitura (não chama `aplicar`,
`tratar_deriva` nem `restaurar` e não grava estado): imprime capacidades,
jogos com o perfil encontrado, o diff por perfil (global e cada jogo) e a
deriva contra o estado salvo, se houver.

## Testes

`cargo test -p nvt-engine` (sobre o `DriverFalso`): plano em Turing + DLSS
aplica o preset E; segundo ciclo é no-op; reset em massa e atualização de
driver reaplicam; mudança pontual trava e pode ser liberada; restaurar volta
ao snapshot sem tocar IDs fora do catálogo; ID e valor fora da lista são
recusados (inclusive num diff adulterado, sem snapshot nem gravação); jogo
sem perfil ganha perfil + aplicativo; nome ocupado ganha sufixo; limiares de
deriva; persistência em disco e snapshot nunca sobrescrito.

`tests/revisao.rs` reproduz as provas da revisão adversarial
(`docs/revisao-v0.1.md`) e afirma o comportamento corrigido: perfil do app
apagado + driver novo não trava o ciclo e é recriado; erro num perfil e nomes
longos/com NUL não derrubam o plano; limite de FPS desligado volta ao
original e não é regravado no reset; Equilibrado → Qualidade remove as 3
settings do DLSS; todas as transições de sincronismo equivalem a aplicar do
zero; `estado.json` e snapshot vazios/truncados; histórico limitado e
coalescido.

`tests/reverificacao.rs` cobre a reverificação (seção “Reverificação” de
`docs/revisao-v0.1.md`): a reversão volta ao valor de antes da gestão (90) e
a restauração ao snapshot (R1); perfis criados sobrevivem às duas cópias do
estado corrompidas (R2); recuperação gravada na hora e `estado.json` ausente
recuperado da cópia (#7); travas com grafias diferentes viram uma só (#10);
marcador de quarentena e base pós-quarentena.

`tests/competitivo.rs` cobre a fase 1 da spec competitiva: capacidades leem
refresh e VRR da exibição (F1-01/F1-02); a base curada vence a detecção em
disco e exe genérico com AppID diferente não casa (F1-03/F1-04); o plano traz
orientações filtradas e avisos de VRR (F1-12/F1-15); a energia global antiga
volta ao valor de antes da gestão, o jogo competitivo fica com desempenho
máximo e o jogo fora da base não recebe nada (F1-11, §11 item 19); jogo em
execução reaproveita os fatos e não tem a pasta lida (F1-09); anticheat que
só existe durante o jogo aparece nos processos vigiados (F1-06).
