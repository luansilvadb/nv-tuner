# Laboratório — interface para leigos e saúde do PC

Roteiro de [`interface-e-saude.md`](interface-e-saude.md) §10. Cada laboratório tem
uma planilha em branco para preencher à mão; copie a tabela para um arquivo datado
(fora do repositório se tiver dados da máquina) antes de começar.

Regras gerais:

- Lab-UI usa o **build normal** (MSI da release ou `instalador\build-msi.ps1`).
- Lab-L, Lab-M e Lab-A usam o **build de teste** (abaixo), só em máquina ou VM do
  mantenedor. O build de teste nunca é publicado nem entregue a usuários.
- Registre sempre: versão (`nvt-service --version`), Windows (versão e build), GPU,
  driver NVIDIA e data.

## 1. Build de teste

### 1.1 Gerar

Num PowerShell comum (sem `CI` nem `GITHUB_ACTIONS` definidos), na raiz do
repositório:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-teste.ps1
```

O script define `NVT_BUILD_DE_TESTE=1` só no próprio processo, compila em release
`nvt-service`, `nvt-agente` e `nvt-ui` com a feature `medicao-partidas` (que liga
também a limpeza em `Simulacao`) numa pasta própria, `target\teste\release` (nunca em
`target\release`), e gera
`target\dist-teste\nv-tuner-X.Y.Z-teste-x64.msi` com o `.sha256`. Parâmetros
opcionais: `-SemValidar` (pula a validação ICE) e `-Dotnet <caminho>`.

Conferir antes de instalar:

- [ ] o script mostrou o aviso "BUILD DE TESTE X.Y.Z+teste";
- [ ] o MSI está em `target\dist-teste\`, não em `target\dist\`.

### 1.2 Instalar

1. Feche jogos e a interface do nv-tuner.
2. Execute o MSI de teste como administrador (por cima da versão normal, ou numa
   VM limpa).
3. Confira:
   - [ ] `nvt-service --version` termina em `+teste`;
   - [ ] a interface mostra "Build de teste" na barra de título;
   - [ ] a lateral tem o item **Partidas**;
   - [ ] **Avançado > Manutenção** diz "Simulação".

### 1.3 Voltar ao normal

1. Feche jogos e a interface.
2. Gere o MSI normal (os binários de teste ficam em `target\teste` e não interferem;
   o script recusa binário `+teste` num MSI normal):

   ```powershell
   powershell -ExecutionPolicy Bypass -File instalador\build-msi.ps1
   ```

   Ou baixe o MSI da release publicada.
3. Desinstale o build de teste (Configurações > Aplicativos) e instale o MSI normal.
   Se a versão for a mesma, a instalação por cima pode ser recusada: desinstale antes.
4. Confira:
   - [ ] `nvt-service --version` sem `+teste`;
   - [ ] sem "Build de teste" na barra de título e sem o item **Partidas**;
   - [ ] **Avançado > Manutenção** diz que a limpeza está desligada;
   - [ ] `logman query -ets` não lista `nv-tuner-partidas`.
5. As partidas gravadas ficam em `%ProgramData%\nv-tuner\partidas\`. Apague-as pelo
   build de teste (tela **Partidas**, **Apagar histórico de partidas**) antes de desinstalá-lo, ou apague a
   pasta à mão como administrador.

## 2. Lab-UI — um leigo usando sem ajuda

Objetivo: um leigo entende sozinho como o PC está e como desfazer o que o app fez.

Preparação:

- Três pessoas que não conhecem o nv-tuner nem termos de placa de vídeo.
- Build normal instalado; primeira abertura da interface (apague
  `%LOCALAPPDATA%\nv-tuner\ui.json` para ver as boas-vindas de novo).
- Capturas de todas as telas, variações do veredito e estados da linha em 1200x800 e
  1920x1080, pelo modo de demonstração:

  ```powershell
  nvt-ui --demonstracao <cenário>   # otimizado, melhorias, piorou, jogo-aberto,
                                    # desligado, atualizado, sem-servico,
                                    # primeira-vez, saude
  ```

Regras para quem conduz: não explicar nada, não apontar a tela, só ler a tarefa e
cronometrar. Anotar onde a pessoa olhou primeiro, o que leu em voz alta e onde
hesitou.

Tarefas (uma por vez, nesta ordem):

| # | Tarefa lida para a pessoa | Aprovação |
|---|---|---|
| T1 | "Abra o programa pela primeira vez e comece a usar." | Passa das boas-vindas sem ajuda |
| T2 | "Seu PC está bem?" | Resposta correta em até 30 s |
| T3 | "Tem alguma coisa que o programa sugere melhorar? Se tiver, faça." | Encontra e usa **Aplicar melhorias recomendadas** |
| T4 | "Como você desfaz tudo o que o programa fez?" | Encontra **Desfazer tudo** em até 30 s |
| T5 | "Depois de desfazer, como ligar de novo?" | Usa **Voltar a ajustar** |
| T6 | "Algum programa novo está abrindo junto com o Windows?" | Encontra a lista em Saúde do PC |
| T7 | "Esse jogo está ajustado?" (aponte o nome de um jogo instalado) | Lê o status certo em **Jogos** |

O que observar: palavra que a pessoa não entendeu; botão procurado e não achado;
leitura errada do veredito; clique em algo do Avançado sem querer; reação ao
diálogo de **Desfazer tudo**; qualquer sigla que apareceu.

Planilha (uma por pessoa):

| Pessoa | Tarefa | Tempo (s) | Concluiu (S/N) | Ajuda pedida | Onde hesitou | Palavras não entendidas | Observação |
|---|---|---|---|---|---|---|---|
| | T1 | | | | | | |
| | T2 | | | | | | |
| | T3 | | | | | | |
| | T4 | | | | | | |
| | T5 | | | | | | |
| | T6 | | | | | | |
| | T7 | | | | | | |

Capturas (R-UI-13):

| Tela ou estado | 1200x800 conferida | 1920x1080 conferida | Arquivo em `docs/ui-capturas/` | Problema visto |
|---|---|---|---|---|
| Boas-vindas | | | | |
| Início otimizado | | | | |
| Início com melhorias | | | | |
| Início piorou | | | | |
| Início jogo aberto | | | | |
| Início desligado | | | | |
| Início atualizado | | | | |
| Início sem serviço | | | | |
| Início com saúde | | | | |
| Jogos e detalhe | | | | |
| Avançado (Inicialização, Manutenção) | | | | |

Aprovação: as três pessoas respondem T2 e T4 em até 30 s cada, sem ajuda.

## 3. Lab-L — dry-run da limpeza

Objetivo: provar que a limpeza só escolheria arquivos seguros antes de uma versão
posterior ligar o modo `Ativa` no build normal. No build de teste o modo é
`Simulacao` fixo: nada é apagado.

Preparação:

1. Build de teste em duas máquinas: uma de uso diário e uma recém-instalada.
2. Antes de instalar, inventário de `%TEMP%` (de cada usuário) e `C:\Windows\Temp`:
   quantidade de arquivos, bytes totais, quantos com 7 dias ou mais (criação **e**
   última gravação), quantos somente leitura, de sistema e reparse points. Guarde uma
   lista com hash para comparar no fim (fora do repositório).
3. Junção maliciosa: em `C:\Windows\Temp`, crie uma pasta de teste fora dele com
   arquivos antigos e uma junção antiga dentro de `C:\Windows\Temp` apontando para
   ela.

Durante sete dias de `Simulacao`:

- atualizar o driver NVIDIA;
- instalar um MSI qualquer (com a interface aberta, para ver se alguma passada
  coincide com `_MSIExecute`);
- jogar um título com anticheat;
- uso normal no resto do tempo.

O que conferir antes de ligar no normal:

- [ ] nenhum arquivo sumiu por ação do nv-tuner (comparar com o inventário);
- [ ] nenhum item do relatório tem menos de 7 dias, é reparse point, somente
      leitura ou de sistema;
- [ ] nenhuma passada com jogo ou anticheat aberto, nem durante instalação MSI;
- [ ] no máximo uma passada por dia e por alvo;
- [ ] duração de cada passada abaixo de 60 s e até 20.000 entradas;
- [ ] a junção não foi percorrida e a pasta de teste está intacta;
- [ ] o log tem só contagens e bytes, nenhum nome de arquivo;
- [ ] os textos de Manutenção não citam FPS, velocidade nem desempenho;
- [ ] números da simulação coerentes com o inventário.

Planilha (uma linha por passada, por máquina e alvo):

| Máquina | Alvo (`TempUsuario`/`TempWindows`) | Data e hora | Duração (ms) | Arquivos | Bytes | Pulados por motivo | Interrompida (motivo) | Jogo/MSI aberto no momento | Coerente com o inventário | Observação |
|---|---|---|---|---|---|---|---|---|---|---|
| | | | | | | | | | | |
| | | | | | | | | | | |
| | | | | | | | | | | |

Aprovação: zero violações nos itens acima e números coerentes. Só então uma versão
posterior muda o build normal para `Ativa`, depois de um ciclo curto em `Ativa` numa
máquina descartável.

## 4. Lab-M — custo de FPS da gravação

Objetivo: a gravação de partidas não custa FPS mensurável e mede o mesmo que o
PresentMon.

Jogos mínimos: Counter-Strike 2, VALORANT, Cyberpunk 2077 e Fortnite.

Preparação:

1. Mesma máquina, driver e configuração em todas as execuções; mesmo jogo e mesma
   cena reprodutível (benchmark interno, demo ou rota fixa de 120 s).
2. 5 min de repouso antes de cada execução; temperatura inicial da GPU semelhante
   (anote).
3. Referência externa: PresentMon de console ligado nas duas condições, sobre o
   processo do jogo, gravando em CSV.
4. Condições:
   - **A**: gravação desligada. A variável `NVT_MEDICAO_DESLIGADA=1` prevista na spec
     ainda não existe no código; até ela existir, use A = build de teste com o serviço
     parado (`sc.exe stop nvt-service` como administrador, perfil do driver já
     aplicado), e anote isso na planilha.
   - **B**: build de teste com gravação ligada (serviço rodando).
5. Ordem das execuções: **A B B A A B B A A B** (dez execuções por jogo).

Registrar por execução: FPS médio e 1% low pelo PresentMon; na condição B, também o
que o nv-tuner gravou (tela **Partidas**) e o uso de CPU do `nvt-service` (Gerenciador
de Tarefas ou `typeperf`).

Planilha (um bloco por jogo):

| Jogo | # | Condição | Temp. inicial GPU (°C) | FPS médio (PresentMon) | 1% low (PresentMon) | FPS médio (nv-tuner) | 1% low (nv-tuner) | Perda de eventos (%) | CPU do serviço (% de um núcleo) | Observação |
|---|---|---|---|---|---|---|---|---|---|---|
| | 1 | A | | | | — | — | — | | |
| | 2 | B | | | | | | | | |
| | 3 | B | | | | | | | | |
| | 4 | A | | | | — | — | — | | |
| | 5 | A | | | | — | — | — | | |
| | 6 | B | | | | | | | | |
| | 7 | B | | | | | | | | |
| | 8 | A | | | | — | — | — | | |
| | 9 | A | | | | — | — | — | | |
| | 10 | B | | | | | | | | |

Resumo por jogo:

| Jogo | Média A (FPS médio) | Desvio padrão A | Média B | B - A (%) | Média A (1% low) | Desvio padrão A | Média B | B - A (%) | Aprovado (S/N) |
|---|---|---|---|---|---|---|---|---|---|
| | | | | | | | | | |

Critério de aprovação:

- em FPS médio e em 1% low, a diferença entre as médias de B e A fica dentro do
  desvio padrão de A **e** abaixo de 1%;
- CPU do serviço abaixo de 1% de um núcleo durante a partida;
- exatidão: nv-tuner contra PresentMon com FPS médio dentro de 1% e 1% low dentro de
  3%; perda de eventos abaixo de 1%;
- nenhuma sessão `nv-tuner-partidas` sobrando (`logman query -ets`) depois de
  encerrar o serviço à força (`taskkill /F` no `nvt-service.exe`) e reiniciá-lo, nem
  fora de partida.

Questão Q1 a anotar: o NVML respondeu como LocalSystem na sessão 0 (uso e
temperatura presentes na tela Partidas)? [ ] sim [ ] não, GPU: ______

## 5. Lab-A — anticheat com a gravação ativa

Objetivo: a gravação (sessão ETW só de Present, lista de processos, sem handle para
o jogo) não causa nenhum incidente de anticheat.

Para cada anticheat, com o build de teste e a gravação ligada: três partidas
completas (casuais quando houver). Conferir em cada uma:

- o jogo abre normalmente;
- nenhuma desconexão, expulsão ou aviso do anticheat;
- nada nos logs do anticheat cita o nv-tuner (`nvt-service`, `nv-tuner-partidas`);
- a partida aparece gravada na tela **Partidas**.

Repetir depois de cada atualização do anticheat e reconferir as páginas oficiais
(S8 de [`competitivo.md`](competitivo.md)).

Planilha:

| Anticheat | Jogo | Versão do anticheat | Partida | Data | Abriu (S/N) | Desconexão ou aviso | Log cita nv-tuner (S/N) | Gravada (S/N) | Observação |
|---|---|---|---|---|---|---|---|---|---|
| Vanguard | VALORANT | | 1 | | | | | | |
| Vanguard | VALORANT | | 2 | | | | | | |
| Vanguard | VALORANT | | 3 | | | | | | |
| EAC | Fortnite ou Apex Legends | | 1 | | | | | | |
| EAC | Fortnite ou Apex Legends | | 2 | | | | | | |
| EAC | Fortnite ou Apex Legends | | 3 | | | | | | |
| BattlEye | Rainbow Six Siege ou PUBG | | 1 | | | | | | |
| BattlEye | Rainbow Six Siege ou PUBG | | 2 | | | | | | |
| BattlEye | Rainbow Six Siege ou PUBG | | 3 | | | | | | |
| VAC (Trusted Mode) | Counter-Strike 2 | | 1 | | | | | | |
| VAC (Trusted Mode) | Counter-Strike 2 | | 2 | | | | | | |
| VAC (Trusted Mode) | Counter-Strike 2 | | 3 | | | | | | |
| FACEIT | Counter-Strike 2 (cliente FACEIT) | | 1 | | | | | | |
| FACEIT | Counter-Strike 2 (cliente FACEIT) | | 2 | | | | | | |
| FACEIT | Counter-Strike 2 (cliente FACEIT) | | 3 | | | | | | |
| Ricochet | Call of Duty | | 1 | | | | | | |
| Ricochet | Call of Duty | | 2 | | | | | | |
| Ricochet | Call of Duty | | 3 | | | | | | |

Aprovação: zero incidentes. Um incidente tira aquele anticheat da gravação ou
encerra o recurso.

## 6. Resultado

| Laboratório | Data | Máquinas | Resultado (aprovado/reprovado) | Pendências |
|---|---|---|---|---|
| Lab-UI | | | | |
| Lab-L | | | | |
| Lab-M | | | | |
| Lab-A | | | | |
