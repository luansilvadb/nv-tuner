# Pesquisa: saúde e manutenção (limpeza de temporários e inicialização)

Data: 2026-11-26. Escopo: (a) limpeza automática e segura de temporários; (b) detectar e
mostrar, sem desligar, programas novos que iniciam com o Windows. Restrições do projeto:
`docs/arquitetura.md` (serviço LocalSystem + agente por sessão) e `docs/spec/competitivo.md`
§3 (S1: nada grava com jogo/anticheat aberto; N14: nenhum processo externo; N6: não tocar
em segurança do Windows nem no Windows Update).

Método e limite: busca na web com trechos indexados; vários pontos só têm fonte
comunitária (Microsoft Q&A, fóruns). Onde não há fonte oficial isso é dito. Itens marcados
"verificar" precisam de confirmação em máquina real.

## 1. Storage Sense

Fontes: [Storage Sense (Microsoft Learn)](https://learn.microsoft.com/windows/configuration/storage/storage-sense),
[Gerenciar espaço com o Storage Sense (Suporte)](https://support.microsoft.com/da-dk/windows/experience/storage-filemanagement/manage-drive-space-with-storage-sense),
[Windows 10 and Storage Sense (Tech Community)](https://techcommunity.microsoft.com/blog/filecab/windows-10-and-storage-sense/428270),
[Políticas via MDM (P. van der Woude)](https://www.petervanderwoude.nl/post/configure-storage-sense-via-windows-10-mdm/).

- O que apaga: temporários **que os aplicativos não estão usando** (critério "em uso", sem
  idade configurável para temporários), Lixeira após N dias (1/14/30/60), Downloads após N
  dias (desligado por padrão), conteúdo do OneDrive só local após N dias sem abrir.
- Cadência (`ConfigStorageSenseGlobalCadence`): 1 diário, 7 semanal, 30 mensal, 0 quando
  houver pouco espaço. Políticas em `HKLM\SOFTWARE\Policies\Microsoft\Windows\StorageSense`
  (incluindo `AllowStorageSenseTemporaryFilesCleanup`). Fonte oficial da lista de valores:
  ADMX; a página do Learn só lista as políticas. Verificar.
- Acionar em vez de reimplementar: configurar por **política** sobrepõe a escolha do
  usuário e trava o controle na UI do Windows ("gerenciado pela organização"). Isso é
  intrusivo e não reversível de forma óbvia para o usuário; não recomendado. Disparar
  uma execução manual exige a tarefa agendada / UI das Configurações (não há API pública
  documentada para "rodar agora") ou `cleanmgr.exe` (processo externo, proibido por N14).
- Conclusão: **mostrar** o estado do Storage Sense (ligado/desligado, cadência) e oferecer
  link para `ms-settings:storagesense`; não configurar por política. A limpeza própria do
  nv-tuner fica restrita a pastas triviais e critérios conservadores (seção 2).

Disk Cleanup (`cleanmgr`) e handlers: os handlers são objetos COM `IEmptyVolumeCache`
registrados em `HKLM\...\Explorer\VolumeCaches`
([IEmptyVolumeCache](https://learn.microsoft.com/windows/win32/api/emptyvc/nn-emptyvc-iemptyvolumecache)).
Chamá-los em processo seria possível em tese, mas carrega DLLs de sistema e de terceiros
no serviço; não recomendado. O `cleanmgr` está depreciado em favor do Storage Sense.

## 2. Pastas: o que limpar e o que nunca

| Pasta | Quem | Decisão | Motivo |
|---|---|---|---|
| `%TEMP%` do usuário (`%LOCALAPPDATA%\Temp`) | agente | automático, idade >= 7 dias | é o alvo do próprio Storage Sense; apps e instaladores usam e podem estar no meio de algo |
| `C:\Windows\Temp` | serviço | automático, idade >= 7 dias | mesmo critério; cuidado redobrado com instaladores (MSI, Windows Update, drivers NVIDIA extraem lá) |
| Lixeira | nenhum | nunca | é dado do usuário; o Storage Sense já cuida se ele quiser |
| `C:\Windows\Prefetch` | nenhum | **nunca** | limpar é mito: o Windows recria e o primeiro boot/abertura fica mais lento. Sem fonte oficial favorável à limpeza |
| `C:\Windows\SoftwareDistribution\Download` | nenhum | nunca automático | exige parar `wuauserv`/`bits` ([Q&A](https://learn.microsoft.com/en-us/answers/questions/5524630/clearing-software-distribution-folder)), o que é mexer no Windows Update (N6) e lançar/controlar serviço do sistema. Pode impedir reverter atualização ([N-able](https://documentation.n-able.com/N-central/userguide/Content/Automation/Policies/Maintenance/pol_CleanWindowsUpdateFolder.htm)). Só mostrar o tamanho |
| Delivery Optimization (cache) | nenhum | nunca automático | o próprio serviço gerencia o cache com limite e idade; apagar é mexer no Windows Update. Mostrar tamanho e link para Configurações |
| `%LOCALAPPDATA%\D3DSCache` (DirectX) | nenhum | **nunca** | é cache de shader: apagar força recompilação e **piora** engasgos até regenerar ([Q&A](https://learn.microsoft.com/en-us/answers/questions/3195734/directx-shader-cache): "will regenerate & fill again") |
| `%LOCALAPPDATA%\NVIDIA\DXCache`, `GLCache`, `ProgramData\NVIDIA Corporation\NV_Cache` | nenhum | **nunca** | idem; o driver já limita o tamanho. Limpar é só remédio pontual para cache corrompido, decisão manual ([relatos](https://www.pathofexile.com/forum/view-thread/3421152), [Gaijin](https://support.gaijin.net/hc/en-us/articles/30268449678994-How-to-clear-NVIDIA-and-DirectX-cache)) |
| WinSxS, `Installer`, `Logs`, `Minidump`, `LiveKernelReports` | nenhum | nunca | componentes do sistema ou evidência de diagnóstico |

Confirmado: shader cache limpo = recompilação = engasgo temporário. A literatura de
"limpar o cache para ganhar FPS" é sobre cache corrompido após atualização de driver/jogo,
não manutenção.

### Como apagar com segurança

- Idade: usar `LastWriteTime` **e** `CreationTime` (e `LastAccess` não, costuma estar
  desligado). Apagar só se ambos >= 7 dias; diretório só se vazio após a passada.
- Não seguir reparse points: enumerar com `FindFirstFileExW` e pular qualquer entrada com
  `FILE_ATTRIBUTE_REPARSE_POINT` (junção, symlink, placeholder de nuvem); abrir com
  `FILE_FLAG_OPEN_REPARSE_POINT` e conferir de novo pelo handle
  ([CreateFileW](https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-createfilew)).
  Isso fecha o ataque clássico em que um usuário cria uma junção em `Windows\Temp`
  apontando para `System32` e o serviço LocalSystem apaga o alvo.
- Arquivo em uso: abrir com `DELETE` e **`dwShareMode = 0`** (exclusivo) mais
  `FILE_FLAG_OPEN_REPARSE_POINT`; se falhar com violação de compartilhamento, pular. Com o
  handle exclusivo na mão, marcar a exclusão por
  `SetFileInformationByHandle(FileDispositionInfoEx, FILE_DISPOSITION_FLAG_DELETE)`
  ([FILE_DISPOSITION_INFO_EX](https://learn.microsoft.com/windows/win32/api/winbase/ns-winbase-file_disposition_info_ex))
  e fechar. Evitar `FILE_FLAG_DELETE_ON_CLOSE` sozinho com compartilhamento: se outro
  processo tiver o arquivo aberto com `FILE_SHARE_DELETE`, o arquivo some só quando ele
  fechar, e novas aberturas falham nesse meio tempo; com share 0 a abertura já falha
  antes, que é o desejado. Não usar `POSIX_SEMANTICS` nem `IGNORE_READONLY` (somente
  leitura = pular).
- Instaladores em andamento: pular tudo se houver instalação MSI em curso (mutex global
  `Global\_MSIExecute`; consulta por `OpenMutex`, sem processo externo) e pular
  subpastas recentes inteiras (qualquer filho com menos de 7 dias preserva o diretório).
  A idade de 7 dias já cobre a maior parte; o mutex cobre o resto.
- Portão: só com nenhum jogo nem anticheat detectado (S1) e, por gentileza, em ocioso.
  Limitar tempo e quantidade por passada; registrar total apagado e erros no log.
- Divisão: `%TEMP%` do usuário pelo agente (token do usuário, sem impersonação no
  serviço); `C:\Windows\Temp` pelo serviço. O serviço nunca apaga no perfil de usuários.

### Ganho real

Honestamente: **espaço em disco**, quase só isso. Temporários não deixam jogo nem
Windows mais rápidos de forma perceptível, exceto quando o disco está quase cheio (pouco
espaço afeta paginação, atualizações e SSDs com pouca área livre). Não prometer FPS.

## 3. Programas de inicialização (somente leitura)

Fontes: [Configurar aplicativos de inicialização (Suporte)](https://support.microsoft.com/en-us/windows/configure-startup-applications-in-windows-115a420a-0bff-4a6f-90e0-1934c844e473),
[Q&A sobre StartupApproved](https://learn.microsoft.com/en-us/answers/a/4712947),
[Run and RunOnce](https://learn.microsoft.com/windows/win32/setupapi/run-and-runonce-registry-keys),
[Task Scheduler ITaskService](https://learn.microsoft.com/windows/win32/api/taskschd/nn-taskschd-itaskservice).

Fontes a ler (todas só leitura):

- `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` e `RunOnce` (agente).
- `HKLM\...\CurrentVersion\Run`, `RunOnce` e `HKLM\Software\WOW6432Node\...\Run` (serviço
  ou agente; leitura em HKLM não exige privilégio).
- Estado ligado/desligado: `...\Explorer\StartupApproved\Run`, `\Run32` e
  `\StartupFolder` em HKCU e HKLM. Valor binário de 12 bytes; primeiro byte par (0x02,
  0x06) = habilitado, ímpar (0x03, 0x07) = desabilitado pelo usuário; bytes 4 a 11 =
  FILETIME da desativação. Formato não documentado oficialmente; verificar. Desativar
  pelo Gerenciador de Tarefas não remove a entrada `Run`
  ([TweakTown](https://www.tweaktown.com/guides/11602/i-stopped-trusting-the-startup-apps-tab-after-i-opened-this-utility-on-my-pc/index.html)).
- Pasta Inicializar do usuário (`FOLDERID_Startup`) e comum (`FOLDERID_CommonStartup`) via
  `SHGetKnownFolderPath`; resolver `.lnk` com `IShellLinkW` só para exibir o alvo.
- Tarefas agendadas com gatilho de logon (`TASK_TRIGGER_LOGON`) ou de boot: COM
  `ITaskService::Connect` + `GetFolder("\\")` + enumeração recursiva com
  `TASK_ENUM_HIDDEN`, lendo `Definition.Triggers` e `Enabled`. Só leitura, em processo.
  Filtrar `\Microsoft\` (são do sistema).
- Serviços automáticos de terceiros: `EnumServicesStatusExW` + `QueryServiceConfigW`
  (`SERVICE_AUTO_START`, imagem fora de `System32` e não assinada pela Microsoft).
  Mostrar em seção separada "serviços", sem sugerir desligar.
- Apps empacotados (MSIX `StartupTask`): aparecem na aba do Gerenciador de Tarefas;
  enumerá-los exige API de pacotes por usuário. Opcional, fase posterior.

"Novo desde a última vez": persistir no armazenamento do agente um conjunto de
identidades `(fonte, nome do valor ou tarefa, caminho normalizado do executável)`; um hash
do comando completo detecta alteração. Primeira execução = linha de base (nada é "novo").
Novo = identidade ausente da linha de base; mostrar a data de detecção. O usuário pode
"marcar como visto".

Impacto: o Gerenciador de Tarefas calcula a partir de dados de boot internos (Baixo =
CPU < 300 ms e disco < 292 KB, segundo o Suporte). **Não há API pública** para esse
valor. Alternativa: o log `Microsoft-Windows-Diagnostics-Performance/Operational`
(evento 100 e 101+ com "app de inicialização lento") via `EvtQuery`, sem processo
externo; só tem dados quando o Windows registra degradação. Recomendação: não estimar
impacto; mostrar "veja o impacto no Gerenciador de Tarefas".

Abrir a página: `ms-settings:startupapps` é URI documentado
([Launch Settings](https://learn.microsoft.com/windows/apps/develop/launch/launch-settings)).
`ShellExecuteW`/`LaunchUriAsync` de fato faz o Windows criar `SystemSettings.exe` (ou
`Taskmgr.exe` com `/0 /startup`), ou seja, cria processo; porém é processo do Windows,
iniciado pelo shell por ação explícita do usuário na UI, igual a abrir um link. Avaliação:
viola a letra de N14 se lido como "qualquer processo"; o espírito de N14 (não lançar
auxiliar em segundo plano, sobretudo com jogo aberto) é preservado se: só a UI, só por
clique, e bloqueado enquanto jogo/anticheat estiver aberto. Decisão a registrar na spec
antes de implementar; alternativa sem processo é só exibir o texto "Configurações >
Aplicativos > Inicialização".

## 4. Outros sinais de "PC piorando"

Mensuráveis sem risco (somente leitura):

- Espaço livre baixo no volume do sistema (`GetDiskFreeSpaceExW`): alerta < 10% ou
  < 20 GB. Sinal real.
- Memória comprometida em repouso alta (`GlobalMemoryStatusEx`) e CPU em repouso
  (`GetSystemTimes`, média de alguns minutos sem jogo). Sinal real, mas ruidoso logo
  após o logon; amostrar com atraso.
- Driver NVIDIA desatualizado: versão via NVAPI (já existe) contra a última conhecida;
  só informar.
- Plano de energia ativo trocado (`PowerGetActiveScheme`): só mostrar a deriva, a
  correção já é da camada `windows`.
- Pagefile desativado, SSD com TRIM desligado (`DisableDeleteNotify` lido do registro):
  informativos.

Mitos (não implementar): "limpar Prefetch", "limpar registro", "liberar RAM"/"otimizar
memória", "desfragmentar SSD", "limpar shader cache para ganhar FPS", "excluir
SoftwareDistribution por rotina".

## 5. Recomendações

Automatizar:

1. `%TEMP%` do usuário pelo agente e `C:\Windows\Temp` pelo serviço: arquivos com
   criação e escrita >= 7 dias, sem reparse point, abertura exclusiva com `DELETE` +
   `FileDispositionInfoEx`, pular em uso/somente leitura, pular se `Global\_MSIExecute`
   existir, só com nenhum jogo/anticheat aberto, no máximo uma vez por dia.

Só mostrar:

2. Estado do Storage Sense e tamanhos de Lixeira, SoftwareDistribution\Download,
   Delivery Optimization e shader caches (com aviso de que não devem ser limpos).
3. Lista de inicialização (Run/RunOnce HKCU/HKLM/WOW6432Node, StartupApproved, pastas
   Inicializar, tarefas de logon, serviços automáticos de terceiros) com destaque para
   "novo desde a linha de base"; sem "impacto" próprio.
4. Espaço livre baixo, memória/CPU em repouso, driver desatualizado, plano de energia.

Nunca:

5. Prefetch, shader caches (DirectX/NVIDIA), SoftwareDistribution, Delivery Optimization,
   Lixeira, WinSxS, logs/dumps; configurar o Storage Sense por política; chamar
   `cleanmgr`/handlers COM; desativar ou editar entradas de inicialização; prometer
   ganho de desempenho pela limpeza.
6. Abrir `ms-settings:startupapps` só depois de decisão explícita na spec sobre N14.
