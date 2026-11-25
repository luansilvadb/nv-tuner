# Revisão adversarial da v0.1

Revisão somente leitura do código no commit `1935f46` (nenhum arquivo do
repositório foi alterado além deste relatório; nada foi gravado no DRS nem
instalado). Os achados marcados **[provado]** têm teste temporário fora do
repositório (`provas/src/lib.rs`, sobre o `DriverFalso` e o `nvt-drs` reais),
que passa demonstrando o defeito. Os demais têm evidência por leitura de
código, com o cenário descrito.

`cargo test --workspace --exclude nvt-ui`: todos os testes existentes passam.

## Resumo

| # | Gravidade | Área | Achado |
|---|---|---|---|
| 1 | crítico | motor | Perfil criado pelo app que some (instalação limpa do driver, ou apagado no NPI/painel) trava o ciclo para sempre **[provado]** |
| 2 | crítico | motor | Um único jogo com nome de perfil inválido derruba o plano inteiro (global incluso) **[provado]** |
| 3 | importante | segurança | `%ProgramData%\nv-tuner` pode ser pré-criado por usuário comum; o serviço não confere dono/ACL |
| 4 | importante | motor | Desligar o limite de FPS não desfaz o limite no driver e o reset em massa o regrava **[provado]** |
| 5 | importante | regras | Trocar G-SYNC → V-Sync deixa o G-SYNC ligado **[provado]** |
| 6 | importante | regras | Baixar o teto para “Qualidade/Fidelidade” não remove o preset E já gravado **[provado]** |
| 7 | importante | estado | `snapshot-original.json` ou `estado.json` corrompido deixa o serviço preso para sempre **[provado]** |
| 8 | importante | serviço | Pânico no tratador não é capturado (a captura do `nvt-ipc` não cobre a thread principal) |
| 9 | importante | serviço | Requisição que expira com `ocupado` continua na fila e é executada depois |
| 10 | importante | estado | Histórico sem limite; qualquer usuário local o infla via travar/destravar |
| 11 | menor | IPC | Um cliente local segura o pipe (instância única) indefinidamente |
| 12 | menor | serviço | `travar_setting` é sensível a maiúsculas e aceita `"Base Profile"` como jogo |
| 13 | menor | motor | `travar_setting` atualiza `driver_visto` e pode esconder uma atualização de driver |
| 14 | menor | detecção | Como LocalSystem, o HKCU é o do SYSTEM: jogos/launchers por usuário somem |
| 15 | menor | serviço | `reescanear_jogos` sem limite de frequência (varredura pesada como SYSTEM) |

Verificado e **sem defeito encontrado**: tamanhos e versões das structs da
FFI (asserções em tempo de compilação), terminador e limite de
`to_unicode_string`, `DestroySession` no `Drop`, `nvapi64.dll` carregada do
diretório do app (Program Files) ou System32; regras só embutidas no binário
(o serviço não lê regras do disco); `--estado-dir` não é controlável por
usuário (argumento fixo no SCM); validação de forma no IPC, limite de 1 MiB
antes da alocação, `PIPE_REJECT_REMOTE_CLIENTS`, ACL sem
`FILE_CREATE_PIPE_INSTANCE`, verificação de dono no cliente; lista permitida
antes de toda gravação; IDs e valores do catálogo conferem com
`docs/pesquisa/settings-referencia.json` (todos os 21 IDs existem e os valores
nomeados estão entre os valores do NPI); GTX 16 (TU116/TU117 ou nome “GTX”)
sem Tensor Cores não recebe regra de DLSS; arquitetura desconhecida não casa
regras de arquitetura; modo de visualização da UI (`SomenteLeitura` recusa
`criar_*`, `gravar_setting`, `remover_setting`, `salvar`; armazém em memória;
comandos de escrita bloqueados em `pode_executar`).

## Status das correções — motor, núcleo e regras

Achados que caem em `nvt-engine`, `nvt-core` e `regras/` (os de
`nvt-service`, `nvt-ipc` e instalador são tratados à parte). Regressões em
`crates/nvt-engine/tests/revisao.rs` (e `crates/nvt-core/tests/avaliador.rs`
para a regra de V-Sync); as seis provas temporárias, rodadas contra o código
corrigido, deixaram de reproduzir o defeito.

| # | Status | Correção | Testes |
|---|---|---|---|
| 1 | corrigido | `tratar_deriva`: perfil de jogo ausente tira as settings dele do esperado (`TratamentoDeriva::removidas`) e o próximo plano recria perfil + aplicativo se a regra ainda exigir; demais erros por perfil isolados (`erros`); o global sempre segue | `perfil_do_app_apagado_nao_trava_o_ciclo_e_e_recriado`, `erro_num_perfil_de_jogo_fica_isolado_e_o_global_segue` |
| 2 | corrigido | Exe validado (vazio, controle/NUL, > 260 UTF-16) e nome do perfil limpo e truncado (200 UTF-16) antes da NVAPI; falha de um jogo vai para `Diff::ignorados`; `aplicar` isola erros por perfil (`Aplicacao::erros`) | `jogo_com_nome_longo_nao_derruba_o_plano`, `erro_num_perfil_de_jogo_fica_isolado_e_o_global_segue` |
| 4, 6 | corrigido | Settings do esperado que saíram do plano voltam ao original do snapshot (ou são removidas, se não existiam / perfil criado pelo app) e o esperado é podado (`Diff::reverter`) | `desligar_limite_fps_volta_ao_original_e_reset_nao_regrava`, `limite_fps_volta_ao_valor_original_gravado_pelo_usuario`, `equilibrado_para_fidelidade_remove_as_settings_do_dlss`, `fidelidade_limpa_perfil_criado_pelo_app` |
| 5 | corrigido | `sincronismo-vsync` também desliga `gsync_global` e `gsync_app_state`; o resto volta ao original pela correção de #4 | `todas_as_transicoes_de_sincronismo_equivalem_a_aplicar_do_zero`, `gsync_para_vsync_desliga_gsync`, `vsync_liga_vsync_e_desliga_gsync` |
| 7 | corrigido (parte do motor) | Gravação atômica (temporário + `sync_all` + `rename`, cópia `estado.anterior.json`; snapshot por *hard link* sem substituir); `estado.json` inválido → `.corrompido-<data>` e cópia anterior ou estado limpo; snapshot inválido → indisponível, restauração recusada, arquivo não sobrescrito (decisão em `docs/engine.md`) | `estado_vazio_ou_truncado_vira_corrompido_e_inicia_limpo`, `estado_truncado_e_recuperado_da_copia_anterior`, `snapshot_truncado_ou_vazio_nao_trava_e_nao_e_sobrescrito`, `snapshot_e_gravado_sem_temporario_sobrando_e_nunca_substituido` |
| 10 | corrigido (parte do motor) | Histórico limitado (`LIMITE_HISTORICO_PADRAO` = 2000, configurável) e travamentos idênticos coalescidos | `historico_tem_limite_e_descarta_as_mais_antigas`, `limite_padrao_do_historico_e_2000`, `travar_destravar_repetidos_sao_coalescidos` |

Pendente fora deste escopo: o serviço ainda não registra `Armazem::avisos`,
não expõe `snapshot_indisponivel`, `Diff::reverter`/`ignorados` nem
`Aplicacao::erros`/`TratamentoDeriva::erros` no IPC/UI; o laço de nova
tentativa do serviço e a frequência de `travar_setting` (#10) são do
`nvt-service`.

## Status das correções — serviço, IPC e instalador

Achados em `nvt-service`, `nvt-ipc` e `instalador/` (detalhes em
`docs/servico.md` e `docs/ipc.md`). O MSI foi regenerado com
`instalador/build-msi.ps1` e passou na validação ICE; **não** foi instalado
nem executado.

| # | Status | Correção | Testes |
|---|---|---|---|
| 3 | corrigido | `diretorio.rs`: antes de logs, driver e estado, o diretório e todo o conteúdo são verificados sem seguir reparse points (reparse point, hard link para fora, dono ≠ SYSTEM/Administradores, escrita para outras contas). Correção: cria com dono Administradores e DACL protegida; toma posse se só o dono de itens internos estiver errado; senão, quarentena (`nv-tuner.inseguro-<unix>-<pid>`, nada apagado) e recriação; se falhar, o serviço não opera e responde `interno` com o motivo. Processos privilegiados usam Administradores como dono padrão. Logs e `servico.json` abertos com `FILE_FLAG_OPEN_REPARSE_POINT`. MSI: custom action `prepare-state-dir` como SYSTEM após `InstallFiles` | `juncao_e_detectada_sem_ser_seguida`, `diretorio_raiz_como_juncao_e_detectado` (com `mklink /J`), `hard_link_e_detectado_e_recusado_na_abertura`, `link_simbolico_e_recusado_na_abertura`, `pasta_temporaria_comum_nao_passa`, `criado_com_descritor_seguro_so_tem_problema_de_dono_sem_privilegio`, `exigir_poe_em_quarentena_e_recria`, `dono_errado_so_em_item_interno_toma_posse`, `avisar_nao_altera_nada`, `escreve_e_recusa_juncao_em_logs` |
| 8 | corrigido | Pânicos em `tratar`, `tick`, notificação do DRS e início do núcleo capturados na thread principal e registrados (também por gancho de pânico); a requisição recebe `interno`, o núcleo é descartado e recriado do disco; `docs/ipc.md` alinhado | `panico_no_tratador_responde_interno_e_encerra_o_nucleo`, `panico_no_ciclo_e_capturado`, `protegido_captura_panico`, `panico_do_tratador_vira_interno` (`nvt-ipc`) |
| 9 | corrigido | `Chamado` com estado atômico pendente → iniciado/cancelado: após 45 s na fila a thread do IPC cancela e responde `ocupado`, e a principal descarta o pedido; pedido já iniciado espera até 57 s e nunca vira `ocupado` | `ocupado_cancela_o_pedido`, `pedido_ja_iniciado_nao_vira_ocupado`, `pedido_atendido_a_tempo`, `principal_encerrada_responde_interno` |
| 11 | corrigido | Duração máxima da conexão (5 min) aplicada a cada leitura, inclusive a quem manda um byte por vez; máximo de 1000 requisições por conexão | `conexao_que_pinga_e_encerrada_na_duracao_maxima`, `limite_de_requisicoes_por_conexao_encerra_a_conexao` |
| 15 | mitigado | Taxa global de 10 requisições/s (rajada de 20, balde que não se renova ao reconectar): acima dela `ocupado` sem chamar o tratador. `reescanear_jogos` não tem limite próprio | `taxa_acima_do_limite_responde_ocupado_sem_chamar_o_tratador`, `balde_limita_rajada_e_repoe_com_o_tempo` |

Corrigido de passagem: a espera interrompida pela `Parada` devolvia
`ErrorKind::Interrupted`, que `ler_quadro`/`read_exact` repetem; com a nova
duração máxima isso aparecia como parada demorando até 5 min. Agora devolve
`ConnectionAborted`.

Pendente: `estado.json` e `snapshot-original.json` ainda são abertos pelo
`nvt-engine` sem `FILE_FLAG_OPEN_REPARSE_POINT` (fora deste escopo; com o
diretório verificado e só gravável por SYSTEM/Administradores, não há como
plantar links lá). O comportamento do MSI (custom action como SYSTEM,
quarentena de um diretório pré-criado) precisa ser conferido numa VM
descartável.

## Reverificação

Reverificação adversarial das correções no commit `c20974c` (cópia por
`git archive`; nada gravado no DRS nem instalado). As 6 provas originais
deixaram de reproduzir. Vereditos:

| # | Veredito | Observação |
|---|---|---|
| 1 | corrigido | perfil sumido no reset em massa é recriado; o ciclo não trava |
| 2 | corrigido | nome longo, exe longo, só caracteres de controle, emoji cortado: só o jogo é ignorado |
| 3 | corrigido | conferido sem privilégio (tarefa agendada `/RL LIMITED`) |
| 4 | parcial → corrigido | mudança manual pendente + `definir_preferencias`/`destravar` apagava a escolha do usuário |
| 5 | corrigido | G-SYNC → V-Sync desliga o G-SYNC |
| 6 | corrigido | teto “Qualidade” remove o preset E |
| 7 | parcial → corrigido | um processo somente leitura fazia o serviço perder o estado recuperado |
| 8 | corrigido | resta a thread do IPC sem supervisão (sem gatilho encontrado) |
| 9 | corrigido | sem variante |
| 10 | parcial → corrigido | `travadas`/`estado.json` cresciam sem limite via IPC (grafias do perfil) |
| R1 | regressão → corrigida | a reversão apagava um valor do usuário posterior ao snapshot |
| R2 | regressão menor → corrigida | estado limpo (duas cópias corrompidas) esquecia `perfis_criados` |
| Q | risco → corrigido | arquivo que some durante a verificação levava à quarentena, e a aplicação seguinte capturava como “original” um DRS já alterado |

Correções (depois do commit `60e9eeb`, que já tinha
`Armazem::abrir_somente_leitura` e a leitura de `estado.anterior.json` antes
de renomear — isso resolvia `status`/`dry-run` sem renomear, mas não a
abertura seguinte com `estado.json` ausente):

| # | Correção | Testes |
|---|---|---|
| 4 | `Nucleo::tratar_deriva_pendente` (verificar + tratar a deriva) antes de `aplicar_plano` em `definir_preferencias` e `destravar`, como em `aplicar_agora` e no ciclo | `rv4_preferencia_durante_deriva_pendente_trava_a_escolha_do_usuario`, `rv4_destravar_durante_deriva_pendente_nao_reverte_mudanca_manual` |
| 7 | `Armazem::abrir` grava o estado recuperado/limpo na própria abertura (erro se não conseguir); `estado.json` ausente com `estado.anterior.json` válido recupera da cópia (aviso `estado_recuperado`) em vez de voltar ao vazio; somente leitura continua sem efeito no disco | `rv7_somente_leitura_nao_faz_o_servico_perder_o_estado_recuperado`, `r7_recuperacao_e_gravada_na_hora_e_somente_leitura_nao_mexe`, `r7_estado_ausente_recupera_da_copia_anterior` |
| 10 | `alvo_de` normaliza o perfil para o nome canônico do DRS (`DriverStore::nome_canonico`) e exige perfil gerenciável (jogo detectado, criado pelo app, com setting gerida ou trava); deduplicação sem diferenciar maiúsculas (`mesmo_alvo`, `Estado::travada`) em `travar`, `tratar_deriva`, `liberar_travada` e na abertura; `MAX_TRAVADAS` = 500 com erro claro | `rv10_grafias_do_perfil_viram_uma_trava_por_setting` (256 grafias × 2 settings = 2 travas), `rv10_perfil_nao_gerenciavel_e_limite_de_travas`, `r10_travas_com_grafias_diferentes_sao_uma_so` |
| R1 | `Esperado::antes_da_gestao` (`ValorAntes::Valor`/`Inexistente`), lido antes da primeira gravação do app em cada (perfil, setting) e preservado nas seguintes; a reversão usa esse valor (snapshot só para estado antigo). “Voltar tudo ao original” continua no snapshot inicial (diferença em `docs/engine.md`) | `r1_reversao_volta_ao_valor_de_antes_da_gestao`, `r1_inexistente_e_preservado_entre_gravacoes` |
| R2 | `perfis-criados.json` (cópia redundante, unida à lista do estado na abertura); não há marca no próprio DRS | `r2_perfis_criados_sobrevivem_a_perda_das_duas_copias` |
| Q | `verificar`: `NotFound` de item interno é benigno; `preparar` repete a verificação com falha `Io` antes de corrigir; depois de uma quarentena grava `snapshot-perdido.json`, e o motor avisa `snapshot_original_perdido` e trata o snapshot novo como base pós-quarentena (`Armazem::base_pos_quarentena`) | `arquivo_que_some_durante_a_verificacao_nao_e_problema`, `exigir_nao_poe_em_quarentena_por_arquivo_sumido`, `exigir_poe_em_quarentena_e_recria`, `quarentena_marca_o_snapshot_novo_como_base_pos_quarentena` |

Pendente: o código `snapshot_original_perdido` ainda não tem constante em
`nvt_ipc::codigo_aviso` nem texto próprio na UI (fora do escopo; a UI o
exibe como código desconhecido); a UI (`nvt-ui/src/local.rs`) não recupera
de `estado.anterior.json` quando `estado.json` falta.

---

## 1. Crítico — perfil do app que some trava o ciclo para sempre

**Onde:** `crates/nvt-engine/src/engine.rs:672-674` (`tratar_deriva` →
`gravar_permitido(...)?`), chamado por `crates/nvt-service/src/nucleo.rs:296`
(ciclo) e `nucleo.rs:544` (`aplicar_agora`).

`verificar_deriva` trata `ProfileNotFound` como “voltou ao padrão”
(`engine.rs:617-621`). Em reset em massa (sempre que a versão do driver muda,
`engine.rs:180-188`), essas divergências vão para `reaplicar`, e
`gravar_permitido` num perfil inexistente devolve `ProfileNotFound`. O `?`
aborta o tratamento inteiro **antes** de `salvar` e antes de
`aplicar_plano`; o estado esperado continua apontando para o perfil sumido, e
o erro se repete em todo ciclo (10 min, cada notificação do DRS) e em
`aplicar_agora`. O perfil nunca é recriado (o `resolver_jogos` o recriaria,
mas não é alcançado), e nem o perfil global é reaplicado.

**Reprodução:** jogo sem perfil no driver com DLSS (o app cria o perfil) →
aplicar → instalação limpa do driver (settings resetadas, versão nova, perfil
de usuário apagado). Teste `perfil_do_app_apagado_trava_o_ciclo`:
`tratar_deriva` devolve `Err(ProfileNotFound)` na primeira e na segunda
tentativa. Também acontece se o usuário apagar o perfil no NPI e depois houver
qualquer atualização de driver.

## 2. Crítico — um jogo com nome inválido derruba o plano inteiro

**Onde:** `crates/nvt-engine/src/engine.rs:338-366` (`resolver_jogos`: `?` em
`perfil_do_exe`/`perfil_existe` para cada jogo), `engine.rs:488-492`
(`criar_perfil`/`criar_aplicacao` com `?`); limite em
`crates/nvt-drs/src/strings.rs` (`to_unicode_string` recusa mais de 2047
unidades UTF-16 ou NUL).

O nome do perfil a criar é o nome do jogo vindo do launcher
(`steam.rs:52`, campo `name` do `appmanifest_*.acf`). Um nome com mais de 2047
unidades UTF-16 faz `FindProfileByName` falhar com `InvalidString`, e o `?`
propaga: `planejar` falha para **todos** os jogos e para o perfil global, em
todo ciclo, e `obter_plano`/`aplicar_agora` respondem erro. Nenhum erro de
jogo individual é isolado.

**Vetor:** pastas de biblioteca da Steam costumam ser graváveis por usuário
comum; um manifesto forjado (com `StateFlags` 4 e uma pasta com
`nvngx_dlss.dll`) basta para desligar a autonomia do serviço para todos os
usuários da máquina. Também acontece sem atacante com qualquer erro de NVAPI
num único `find_application`.

**Reprodução:** teste `um_jogo_com_nome_longo_derruba_o_plano_inteiro`
(store que aplica o mesmo `to_unicode_string` do `DrsReal`): `planejar`
devolve `Err(InvalidString)` para a lista inteira.

## 3. Importante — diretório de estado pode ser “sequestrado” antes da instalação

**Onde:** `crates/nvt-engine/src/estado.rs:166-171` (caminho fixo
`%ProgramData%\nv-tuner`), `instalador/nv-tuner.wxs:170-172` (`PermissionEx`
sem `O:`), `crates/nvt-service/src/logs.rs:24-30`, `estado.rs:228-239`,
`estado.rs:253-255`, `nucleo.rs:111-116`.

Em `C:\ProgramData`, `BUILTIN\Usuários` tem `(CI)(WD,AD,WEA,WA)` (conferido
com `icacls`): qualquer usuário cria subpastas e vira **dono** delas. Se um
usuário comum criar `C:\ProgramData\nv-tuner` antes da instalação, o MSI só
troca a DACL (o SDDL não tem `O:`), e o dono continua sendo esse usuário, que
mantém `WRITE_DAC` implícito e pode voltar a se dar controle total. O serviço
não confere dono nem ACL do diretório e segue reparse points:

- `snapshot-original.json` pré-plantado: `create_new` falha, o original
  forjado nunca é substituído e a restauração grava os valores dele
  (`gravar_valor_original` só confere o ID, não o valor nem o tipo —
  `engine.rs:298-307`);
- `estado.json` editável: `perfis_adicionais`, `esperado`, `travadas` e
  preferências controlados pelo usuário;
- `logs` como junção (ou `*.tmp` como link) para outro diretório: o serviço,
  como SYSTEM, cria/anexa `nvt-service.AAAA-MM-DD.log` e apaga os antigos no
  destino — primitiva clássica de escrita arbitrária de arquivo para elevação
  de privilégio; o conteúdo do log inclui nomes de perfil/jogo, controláveis
  pelo usuário.

**Não provado em execução** (esta sessão está elevada; o dono vira
`Administradores`), mas decorre da semântica de dono/DACL do Windows.
Correção esperada: na inicialização, recusar o diretório se o dono não for
SYSTEM/Administradores ou se houver reparse points, e/ou definir o dono no
SDDL do instalador.

## 4. Importante — desligar o limite de FPS não desfaz o limite

**Onde:** `crates/nvt-core/src/evaluator.rs:101` (a preferência só gera
mudança quando `limite_fps` é `Some`); `crates/nvt-engine/src/engine.rs:521-556`
(o estado esperado só é inserido/atualizado, nunca podado).

Com `limite_fps = 141` aplicado, mudar para `null` não produz nenhuma mudança:
o driver continua limitado a 141, o valor fica no estado esperado e um reset
em massa (atualização de driver) **regrava 141**. Mesma classe de defeito para
qualquer setting que deixe de estar no plano (jogo desinstalado, regra que
deixa de casar, preferência trocada): o app continua vigiando, travando e
reaplicando um valor que o plano atual não quer.

**Reprodução:** teste `remover_limite_fps_nao_desfaz_no_driver`.

## 5. Importante — G-SYNC → V-Sync deixa o G-SYNC ligado

**Onde:** `regras/base.toml`, regra `sincronismo-vsync` (só `vsync =
"force_on"`), em contraste com `sincronismo-sem-sync`, que desliga
`gsync_global`/`gsync_app_state`.

Depois de “G-SYNC”, escolher “V-Sync” mantém `gsync_global = on`,
`gsync_app_mode = tela_cheia_e_janela` e `gsync_app_state = permitir` (e eles
continuam no estado esperado, ver #4). A preferência do usuário não é
respeitada.

**Reprodução:** teste `trocar_gsync_por_vsync_deixa_gsync_ligado`.

## 6. Importante — teto de qualidade não remove o que já foi gravado

**Onde:** `crates/nvt-core/src/evaluator.rs:59-70` (regra acima do teto vai
para `descartes`, sem gerar a mudança inversa) + #4.

Com o teto padrão o app grava `dlss-preset-e-turing` (impacto leve). Ao mudar
para “Qualidade” (`Fidelidade`, só impacto `nenhum`), o plano descarta a regra,
mas o preset E continua forçado nos perfis dos jogos — e é reaplicado em reset
em massa. O teto escolhido pelo usuário é violado.

**Reprodução:** teste `teto_fidelidade_nao_remove_preset_e`.

## 7. Importante — estado corrompido prende o serviço para sempre

**Onde:** `crates/nvt-engine/src/estado.rs:185-202` (`abrir` falha com JSON
inválido em qualquer dos dois arquivos), `estado.rs:228-239` (snapshot:
`create_new` → `write_all` → `sync_all`, sem temporário), `estado.rs:254`
(`fs::write` do temporário sem `sync_all` antes do `rename`),
`crates/nvt-service/src/laco.rs:154-178` (laço de nova tentativa).

Queda de energia ou disco cheio durante a gravação do snapshot deixa um
arquivo truncado que `create_new` nunca deixa substituir; no `estado.json`, o
`rename` pode ficar durável antes dos dados (arquivo vazio após queda). Nos
dois casos `Nucleo::novo` falha, o serviço fica respondendo
`driver_indisponivel` (“driver NVIDIA indisponível: …estado.json…”, mensagem
enganosa) a cada minuto, sem aplicar nem restaurar — inclusive o
`restore-original`, que depende do snapshot.

**Reprodução:** teste `snapshot_truncado_impede_abrir_estado`
(`Armazem::abrir` devolve erro com um snapshot truncado).

## 8. Importante — pânico no tratador derruba a thread principal

**Onde:** `crates/nvt-ipc/src/pipe.rs:430` (`catch_unwind` envolve só o
fechamento da thread do IPC, que apenas envia um evento por canal);
`crates/nvt-service/src/laco.rs:98` (`nucleo.tratar` roda na thread
principal, sem `catch_unwind`).

`docs/ipc.md` afirma que pânicos do tratador viram `erro`/`interno`; no
serviço real isso não vale. Um pânico em `Nucleo::tratar` (ou em `tick`)
encerra a thread dona da NVAPI e da sessão DRS; o IPC passa a responder
`interno`/“serviço encerrando” e a autonomia para, sem falha visível ao SCM
até o processo terminar. Não foi encontrado um gatilho concreto de pânico
alcançável pelo IPC; o defeito é a garantia documentada não existir.

## 9. Importante — `ocupado` não cancela a requisição

**Onde:** `crates/nvt-service/src/laco.rs:66-72` (após 55 s responde
`ocupado` e descarta o receptor), `laco.rs:96-100` (o evento continua na fila
e é executado; `resp.send` falha em silêncio).

Durante um ciclo demorado (varredura de jogos em disco lento, driver lento),
um `restaurar_original` ou `definir_preferencias` que recebe `ocupado` é
executado mais tarde mesmo assim. A UI mostra falha, o usuário repete, e as
ações se acumulam e rodam fora de ordem em relação ao que a UI exibe.

## 10. Importante — histórico sem limite

**Onde:** `crates/nvt-engine/src/engine.rs:496, 529, 690, 729, 848`
(`historico.push` sem teto); `crates/nvt-service/src/nucleo.rs:668`
(`travar` registra entrada); `estado.rs:246-256` (reescreve o arquivo
inteiro a cada mudança); `nucleo.rs:227-239` (relê o arquivo a cada
requisição/ciclo).

Qualquer usuário autenticado alterna `travar_setting`/`destravar_setting`
sem limite de frequência: cada par acrescenta entradas permanentes, e o custo
de cada requisição e ciclo cresce com o tamanho do `estado.json` (O(n²) em
E/S ao longo do tempo). Mesmo sem abuso, o uso normal cresce indefinidamente.

## 11. Menor — pipe de instância única pode ser monopolizado

**Onde:** `crates/nvt-ipc/src/pipe.rs:349` (1 instância), `pipe.rs:306`
(30 s por leitura, renovados a cada byte/requisição).

Um processo de qualquer usuário local que mande um byte (ou um `obter_estado`)
a cada 29 s mantém a conexão para sempre; a UI e o `apply --yes` dos demais
recebem tempo esgotado. Falta um limite total por conexão.

## 12. Menor — `travar_setting`: maiúsculas e “Base Profile”

**Onde:** `crates/nvt-service/src/nucleo.rs:606-617`.

`perfil_existe` (DRS) não diferencia maiúsculas, mas `Alvo::Jogo(perfil)` e
`ProfileRef::Named(perfil)` são comparados exatamente: `travar` com
`"wardogs"` para o perfil `"WARDOGS"` responde `ok`, mas a trava não casa com o
alvo do avaliador e o app continua gerindo a setting. `"Base Profile"` é aceito
como perfil de jogo (`Alvo::Jogo("Base Profile")`), também sem efeito.

## 13. Menor — `travar_setting` pode esconder uma atualização de driver

**Onde:** `crates/nvt-service/src/nucleo.rs:659-668` → `engine.rs:741`
(`driver_visto = Some(deriva.versao_driver)`).

Se o driver foi atualizado e ainda não houve ciclo (serviço recém-iniciado,
ciclo anterior falhou, autonomia pausada com esperado não vazio), um
`travar_setting` grava a versão nova em `driver_visto`. O ciclo seguinte não
vê mudança de versão; com poucas settings revertidas, a deriva sai “pontual”
e as settings resetadas pelo driver viram escolhas travadas do usuário
(falso positivo de trava).

## 14. Menor — detecção como LocalSystem usa o HKCU do SYSTEM

**Onde:** `crates/nvt-detect/src/registro.rs:58` (desinstalação em HKCU),
`crates/nvt-detect/src/launchers/steam.rs:14` (`SteamPath` em HKCU).

No serviço, HKCU é o perfil de `S-1-5-18`: entradas de desinstalação por
usuário e o caminho da Steam por usuário não aparecem (a Steam tem fallback
em HKLM; instalações por usuário de outros launchers não). O serviço e a UI
(modo de visualização, que roda como usuário) podem ver listas de jogos
diferentes.

## 15. Menor — `reescanear_jogos` sem limite de frequência

**Onde:** `crates/nvt-service/src/nucleo.rs:468-471`.

Cada pedido faz uma varredura completa das bibliotecas (leitura de PEs) e um
ciclo, como SYSTEM. Um cliente local em laço mantém disco e CPU ocupados.

---

## Testes temporários

Fora do repositório, em
`<scratchpad local>\revisao\provas\`
(crate com dependências por caminho em `nvt-engine`, `nvt-core` e `nvt-drs`;
`cargo test --offline` com `CARGO_TARGET_DIR` próprio). Seis testes, todos
passam afirmando o comportamento defeituoso:

- `perfil_do_app_apagado_trava_o_ciclo` (#1)
- `um_jogo_com_nome_longo_derruba_o_plano_inteiro` (#2)
- `remover_limite_fps_nao_desfaz_no_driver` (#4)
- `trocar_gsync_por_vsync_deixa_gsync_ligado` (#5)
- `teto_fidelidade_nao_remove_preset_e` (#6)
- `snapshot_truncado_impede_abrir_estado` (#7)

Observação sobre o `DriverFalso`: ele grava direto no estado compartilhado,
sem sessão; gravações feitas antes de um erro não são descartadas como no DRS
real (onde a sessão sem `SaveSettings` é destruída). Os testes só afirmam o
erro e o valor final que o DRS real também teria.
