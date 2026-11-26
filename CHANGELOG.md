# Changelog

Todas as mudanças relevantes do nv-tuner ficam registradas aqui.

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/)
e o projeto adota o [Versionamento Semântico](https://semver.org/lang/pt-BR/).
As notas de cada release no GitHub são montadas a partir da seção da versão
(veja “Como lançar uma versão” no [CONTRIBUTING.md](CONTRIBUTING.md)).

## [Não lançado]

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

[Não lançado]: https://github.com/luansilvadb/nv-tuner/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/luansilvadb/nv-tuner/compare/v0.0.2...v0.1.0
[0.0.2]: https://github.com/luansilvadb/nv-tuner/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/luansilvadb/nv-tuner/releases/tag/v0.0.1
