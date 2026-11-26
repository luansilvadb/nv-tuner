# Intenção — próxima fase: desempenho competitivo

Data: 2026-11-26

## Resultado

O nv-tuner deixa jogos competitivos com sensação mais fluida e responsiva
(frametime estável, latência baixa, FPS real) por qualquer camada segura:
driver, Windows e configuração do jogo.

## Usuário

Quem joga competitivo em qualquer GPU NVIDIA. A máquina do mantenedor (GPU
Turing) é o laboratório; o que funciona vira regra por arquitetura/máquina e
vale para todos.

## Por que agora

Hoje o app aplica uma base global no driver e o preset do DLSS; o objetivo é
entregar mais ganho perceptível.

## Sucesso

Após o nv-tuner, o jogo competitivo é percebido como mais liso e responsivo.
Não há exigência de medição/benchmark dentro do app.

## Restrições

- Zero risco com anticheat: nada de injeção, DLL, alteração de executável ou
  de arquivos do jogo além de arquivos de configuração.
- Tudo reversível, no espírito do snapshot e do **Restaurar original** atuais.

## Fora do escopo

- Frame generation em qualquer forma (DLSS FG, Smooth Motion, FSR 3 via
  OptiScaler, Lossless Scaling) — motivo: adiciona latência e o público é
  competitivo.
- Jogos single-player/casuais como foco.
- Benchmark/medição dentro do app.
- Overclock/undervolt.

## Nota

Isso amplia a promessa atual do README ("atua apenas na camada do driver"); a
nova promessa deve ser redigida na especificação.
