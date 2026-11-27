# Changelog

Todas as mudanças relevantes do nv-tuner ficam registradas aqui.

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/)
e o projeto adota o [Versionamento Semântico](https://semver.org/lang/pt-BR/).
As notas de cada release no GitHub são montadas a partir da seção da versão
(veja “Como lançar uma versão” no [CONTRIBUTING.md](CONTRIBUTING.md)).

## [Não lançado]

### Alterado

- O serviço do nv-tuner inicia junto com o Windows, sem a espera de cerca de dois minutos. Vale após instalar o novo MSI, inclusive por cima da versão anterior.

### Corrigido

- Ao ligar o PC, o **Início** mostra "Iniciando o nv-tuner…" enquanto o serviço sobe. Se o serviço não estiver instalado, aparece "O nv-tuner não está ativo neste PC", com o pedido para reinstalar. Se estiver instalado, mas não responder, aparece "O nv-tuner não está respondendo", com orientação para reiniciar o PC e, se o problema persistir, reinstalar o nv-tuner.
- No modo só de informações, o nv-tuner termina a busca na primeira vez e mostra os jogos encontrados no **Início** e na tela **Jogos**, sem abrir arquivos de jogos em execução.

## [0.3.0] - 2026-11-26

*Interface nova e saúde do PC*

### Adicionado

- **Nova interface**, com três itens na lateral: **Início**, **Jogos** e **Avançado**. O Início mostra um único veredito sobre o PC e no máximo uma linha de estado no topo (serviço ausente, jogo aberto, atualização, agente ausente). Tema escuro, destaque ciano do logo, fonte Segoe UI Variable e nenhuma sigla técnica nas telas principais.
- **Boas-vindas** curta na primeira abertura, com um único botão **Começar**.
- **Aplicar melhorias recomendadas** no Início: aceita de uma vez todas as regras experimentais aplicáveis neste PC (driver, Windows e arquivo de jogo), numa única gravação de preferências (tudo ou nada), e aplica pelo mesmo caminho de "Aplicar agora". Com jogo aberto, os aceites ficam gravados e a aplicação espera o jogo fechar. Experimentais do driver só entram quando quem pede é o usuário da sessão de console.
- **Desfazer tudo** no Início, com confirmação (é o **Restaurar original**); depois dele, **Voltar a ajustar** reaplica, mantendo os aceites.
- **Saúde do PC** no Início, somente leitura: espaço livre, memória e processador em repouso, driver NVIDIA desatualizado (lista embutida, sem rede), plano de energia trocado, Sensor de Armazenamento desligado e programas de inicialização novos ou alterados desde a última vez, com **Marcar como visto**.
- **Abrir inicialização**: abre a página Inicialização das Configurações do Windows, só por clique e bloqueado com jogo aberto. Os programas de inicialização são apenas detectados, nunca desativados.
- Grupos **Inicialização** e **Manutenção** no Avançado.
- Crate **`nvt-manutencao`**: limpeza segura de temporários (`%TEMP%` pelo agente, `C:\Windows\Temp` pelo serviço), só arquivos com 7 dias ou mais, sem seguir links, sem jogo aberto nem instalação em andamento, no máximo uma passada por dia. **Desligada nesta versão**; quando ligada, só libera espaço em disco.
- A gravação do desempenho das partidas (tela **Partidas**) **não está nesta release**: existe só num build de teste interno.
- Modo de demonstração da interface (`nvt-ui --demonstracao <cenário>`), com dados fictícios, usado nas capturas de `docs/ui-capturas/`.

### Alterado

- Tudo o que a v0.2.0 fazia continua acessível, agora no **Avançado** (visão técnica, mudanças por perfil, Windows, arquivo do jogo, preferências, histórico, Restaurar original, reescanear e verificar atualização).
- A tela **Jogos** mostra por jogo um status simples e o detalhe ao clicar, com link para o Avançado.
- Sem serviço, o Início diz "O nv-tuner não está ativo neste PC" e orienta a reinstalar.
- Protocolo **v8**: requisições novas `AplicarRecomendadas`, `ObterSaude` e `MarcarInicioVisto`, campo `saude` no relato do agente e `build_de_teste` e `modo_limpeza` no estado, sem caminhos, linhas de comando nem identificadores de conta. As requisições de partidas respondem `RecursoAusente` nas releases. Interface, serviço e agente precisam ser da mesma versão; o MSI instala tudo junto, e a interface antiga pede para ser fechada e aberta de novo.

## [0.2.0] - 2026-11-26

*Configuração de jogos*

> Quem tem a v0.1.0 recebe esta versão sozinho, pela atualização automática (a primeira atualização automática real); não é preciso baixar o MSI.

### Adicionado

- **Configuração de jogos (experimental, só com Aceitar).** O agente grava no arquivo de configuração do usuário, nunca na pasta de instalação: `FPSLimit` do Rainbow Six Siege (30 a 360); `+fps_max` do Counter-Strike 2 (60 a 360) e do Dota 2 (60 a 240) nas opções de inicialização da Steam; no Dota 2 sem limite definido por você, o teto sobe acima do padrão 120 (até a taxa do monitor, máximo 240); `OneFrameThreadLag` desligado no Rocket League.
- **Só grava com o jogo, seus processos protegidos e a Steam fechados.** Antes da primeira gravação o arquivo ganha backup (com SHA-256) e pode ser restaurado; arquivo somente leitura, sincronizado pela Steam Cloud ou de conta ambígua não é tocado.
- **Seção "Arquivo do jogo"** no cartão do jogo, com valor antes/depois, estado do backup, caminho sem o nome do usuário e reversão individual; mensagens como "feche a Steam para aplicar".
- Crate **`nvt-jogocfg`**: editores INI e KeyValues/VDF que alteram só o trecho do valor (codificação, fim de linha, comentários e ordem preservados), gravação atômica sem seguir links e resolução das contas Steam e Ubisoft.
- Catálogo de arquivos e chaves em `regras/jogos.toml` (schema 2), com as faixas do menu e chaves `a_confirmar` que nunca são gravadas: `FrameRateLimit` do Fortnite, Max FPS do Rocket League, `+fps_max` do Deadlock e chaves de Reflex.
- `nvt-service status` mostra a config de jogo da própria sessão (arquivos geridos e pendências), sem caminhos nem contas.

### Alterado

- **Limite de FPS no jogo:** com limite definido e a regra aceita, o limite vai para o jogo e o limite do driver daquele perfil é desligado, só quando o limite no jogo foi aplicado.
- VALORANT, League of Legends, Marvel Rivals e Call of Duty (proibidos) e Apex Legends, Overwatch 2, PUBG e THE FINALS (só orientação) continuam sem nenhuma escrita possível em arquivo.
- **Privacidade:** a conta do jogo é identificada por um HMAC com segredo por usuário, que não revela o SteamID.
- Protocolo **v7**: interface, serviço e agente precisam ser da mesma versão; o MSI instala tudo junto.

## [0.1.0] - 2026-11-26

*Competitivo, ajustes do Windows e atualização automática*

> **Instale esta versão à mão uma vez.** A v0.1.0 é a primeira com atualização automática: quem tem a v0.0.2 (ou anterior) baixa o MSI desta release e instala por cima. A partir dela, as próximas versões chegam sozinhas.

> O MSI e os executáveis ainda **não** têm assinatura digital Authenticode (o SmartScreen pode avisar); as atualizações são verificadas com Minisign.

### Adicionado

- **Base curada de 14 jogos competitivos** (Counter-Strike 2, VALORANT, League of Legends, Fortnite, Apex Legends, Overwatch 2, Rainbow Six Siege, Call of Duty, Rocket League, PUBG: Battlegrounds, Dota 2, Marvel Rivals, Deadlock e THE FINALS), com Reflex, limitador próprio e anticheat de cada um; esses fatos valem mais que os lidos do disco. Os jogos da base ganham o selo **competitivo** e, se estavam abertos na última varredura, o selo **aberto**.
- **Orientações "Faça no jogo"** na tela Jogos (ex.: Reflex em *On + Boost*, limitador do jogo, tela cheia). O app não as aplica: são ajustes para você fazer no menu do jogo.
- **Taxa de atualização e VRR (G-SYNC)** do monitor principal na **Visão geral**, com os avisos "G-SYNC sem VRR" e "VRR desligado no painel". Regras novas de G-SYNC (limite de FPS logo abaixo da taxa do monitor; sem limite do driver em competitivos com Reflex).
- **Ajustes do Windows do seu usuário**: taxa de atualização no máximo compatível, aceleração do mouse desligada (sensibilidade intocada), otimizações para jogos em janela (Windows 11) e captura histórica do Game Bar desligada. Ficam na nova aba **Windows**, com **O que o app fez** (reversão individual), **Experimentais** e **Diagnóstico** somente leitura (Game Mode, VRR, HAGS, MPO, plano de energia, VBS/HVCI, TPM, Secure Boot e outros; o app nunca altera TPM, Secure Boot, VBS nem o BCD).
- **Regras experimentais exigem Aceitar.** Driver e Windows: cada experimental aparece como sugestão, sem efeito até você clicar em **Aceitar**, e pode ser **Retirada**, voltando ao valor anterior. O aceite é por usuário do Windows.
- **Agente do usuário** (`nvt-agente`), sem privilégios, iniciado no logon. Só ele grava a camada Windows; guarda o valor original, respeita mudanças manuais e confere deriva a cada 10 minutos.
- **Nada é gravado com jogo ou anticheat aberto**, no driver ou no Windows, inclusive reaplicação, **Restaurar original** e desinstalação. A ação fica pendente, aparece na faixa **Jogo aberto** e roda quando o jogo fecha; uma restauração pendente sobrevive ao reinício do serviço. A detecção usa só a lista de processos, por nome, sem abrir os processos.
- **Atualização automática, obrigatória e silenciosa.** O serviço verifica a release Latest do GitHub alguns minutos após iniciar e depois a cada ~6 h, e instala com `msiexec /qn /norestart`, sem reiniciar o Windows e sem apagar ajustes. Só aceita versão maior, com manifesto assinado com Minisign e MSI conferido por tamanho e SHA-256; nunca instala com jogo aberto; sem internet, nada bloqueia. A **Visão geral** e `nvt-service status` mostram o estado da atualização.

### Alterado

- **Energia em desempenho máximo saiu do perfil global** e passa a valer só no perfil de cada jogo competitivo; o perfil global volta ao valor de antes do nv-tuner.
- **Restaurar original** cobre o driver e o Windows; um agente que estava fora (logoff, reinício) restaura quando voltar. Com `RESTAURAR_PERFIL=1`, a desinstalação restaura também os ajustes do Windows de quem desinstala.
- Os executáveis dos jogos são lidos com compartilhamento total, e a pasta de um jogo em execução não é lida.
- Protocolo **v6**: interface, serviço e agente precisam ser da mesma versão; o MSI instala tudo junto.
- Instalador: inclui o agente e sua inicialização no logon, fecha o agente de forma cooperativa na atualização e o SCM reinicia o serviço em qualquer falha.
## [0.0.2] - 2026-11-25

*O que o app fez em cada jogo*

### Adicionado

- **"O que o app fez" em cada jogo.** A tela **Jogos** agora explica, linha a linha, o que foi aplicado e por quê. Exemplos:
  - `Preset E do DLSS (Turing)`: nas RTX 20, o modelo antigo do DLSS custa menos GPU (+17% de FPS medido no WARDOGS).
  - `Nenhum ajuste específico: GPU Ada roda o DLSS padrão sem custo extra`: nas RTX 40/50 o app não força nada.
  - `Nenhum ajuste específico: jogo sem DLSS`.
  - `herda 14 ajustes globais`, com clique na linha para ver cada regra aplicada ou não aplicada e o motivo.

### Alterado

- A tela Jogos passa a usar os dados do **serviço** quando ele está conectado.
- Protocolo entre interface e serviço atualizado para a **v3**. A interface avisa se as versões não baterem.
- GitHub Actions atualizadas (sem Node 20).

<p align="center">
  <img src="https://raw.githubusercontent.com/luansilvadb/nv-tuner/v0.0.2/docs/ui-capturas/jogos.png" width="720" alt="Tela Jogos">
</p>

## [0.0.1] - 2026-11-25

*Primeira versão pública (pré-alfa)*

### Adicionado

- **Detecção automática** da GPU por arquitetura (Turing, Ampere, Ada, Blackwell) e dos jogos instalados (Steam, Riot, Epic, GOG, Ubisoft, EA, Battle.net, Xbox), incluindo API gráfica, DLSS, Reflex e anticheat.
- **Regras declarativas** (TOML) sobre capacidades da GPU: base global estável (baixa latência, energia, shader cache, limpeza de filtragem forçada) e preset E do DLSS nas RTX 20.
- **Serviço do Windows** que aplica sozinho, vigia o banco de perfis do driver, reaplica após atualização de driver e respeita mudanças manuais.
- **Interface em GPUI** com 6 telas em português e restauração do estado original.
- Instalador MSI.

[Não lançado]: https://github.com/luansilvadb/nv-tuner/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/luansilvadb/nv-tuner/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/luansilvadb/nv-tuner/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/luansilvadb/nv-tuner/compare/v0.0.2...v0.1.0
[0.0.2]: https://github.com/luansilvadb/nv-tuner/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/luansilvadb/nv-tuner/releases/tag/v0.0.1
