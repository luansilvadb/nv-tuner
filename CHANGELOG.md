# Changelog

Todas as mudanças relevantes do nv-tuner ficam registradas aqui.

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/)
e o projeto adota o [Versionamento Semântico](https://semver.org/lang/pt-BR/).
As notas de cada release no GitHub são montadas a partir da seção da versão
(veja “Como lançar uma versão” no [CONTRIBUTING.md](CONTRIBUTING.md)).

## [Não lançado]

*Fase competitiva 1: driver e fatos dos jogos*

### Adicionado

- **Base curada de 14 jogos competitivos** (Counter-Strike 2, VALORANT, League of Legends, Fortnite, Apex Legends, Overwatch 2, Rainbow Six Siege, Call of Duty, Rocket League, PUBG: Battlegrounds, Dota 2, Marvel Rivals, Deadlock e THE FINALS), com Reflex, limitador próprio e anticheat de cada um. Esses fatos valem mais que os lidos do disco.
- **Orientações “Faça no jogo”** por jogo da base (ex.: Reflex em *On + Boost*, limitador do jogo, tela cheia), mostradas na seção **Faça no jogo** da tela Jogos. O app não as aplica: são ajustes para você fazer no menu do jogo. Os jogos da base ganham o selo **competitivo** e, se estavam em execução na última varredura, o selo **aberto**.
- **Taxa de atualização e VRR (G-SYNC)** do monitor principal lidos pela NVAPI e mostrados na **Visão geral**, com os avisos “G-SYNC sem VRR” e “VRR desligado no painel”.
- **Gravação adiada com jogo aberto.** Enquanto um jogo detectado, um jogo da base ou um anticheat (EasyAntiCheat, BattlEye) estiver em execução, nada é gravado no driver, nem a reaplicação por deriva nem o **Restaurar original**: a ação fica pendente (conferida a cada 15 s) e roda quando o jogo fecha, e a interface mostra a faixa **Jogo aberto** com o que ficou pendente. A pendência fica só na memória do serviço e se perde se ele reiniciar. Sem o serviço em execução, `apply`/`restore-original` pela linha de comando são recusados com jogo aberto. A detecção usa só a lista de processos, por nome, sem abrir os processos.
- Regras novas de G-SYNC: limite de FPS logo abaixo da taxa do monitor (ex.: 224 FPS a 240 Hz) e, em jogos competitivos com Reflex, sem limite do driver. Entram como **experimentais**: aparecem como sugestão e não têm efeito até a validação em laboratório.

### Alterado

- **Energia em desempenho máximo saiu do perfil global** e passa a valer só no perfil de cada jogo competitivo. O perfil global volta ao valor que tinha antes do nv-tuner.
- Os executáveis dos jogos são lidos com compartilhamento total e sem mapeamento, e a pasta de um jogo em execução não é lida (valem os dados da varredura anterior).
- Protocolo entre interface e serviço atualizado para a **v4**: interface e serviço precisam ser da mesma versão (o MSI instala os dois juntos).

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

[Não lançado]: https://github.com/luansilvadb/nv-tuner/compare/v0.0.2...HEAD
[0.0.2]: https://github.com/luansilvadb/nv-tuner/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/luansilvadb/nv-tuner/releases/tag/v0.0.1
