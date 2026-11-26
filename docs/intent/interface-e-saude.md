# Intenção — nv-tuner autônomo, saúde do PC e interface para leigos

Data: 2026-11-26

## Origem

Insatisfação do mantenedor com a interface atual ("amadora", "AI slop", exige
tutorial) e a ideia de um usuário leigo: a tela inicial como fonte única da
verdade do PC, com histórico de desempenho gravado sem overlay.

## Resultado

O app mantém o PC otimizado sozinho e mostra, em linguagem simples, como o PC
e os jogos estão indo. Ninguém precisa de tutorial.

## Usuário

Quem joga e não entende de computador. A parte técnica continua no
**Avançado**.

## Tela inicial

- Estado atual do PC: otimizado, ou o que piorou e por quê.
- Histórico de partidas gravado em silêncio, sem overlay, com:
  - veredito por partida ("estável" / "N engasgos");
  - FPS médio e FPS dos piores momentos;
  - temperatura da GPU;
  - gráfico pequeno;
  - comparação entre semanas.
- Botões **Aplicar melhorias recomendadas** (aceita todas as experimentais de
  uma vez) e **Desfazer tudo**.

## Automático, sem botão

- Os ajustes que o app já faz.
- A atualização.
- Limpeza segura de arquivos temporários: só arquivos antigos, nunca em uso,
  nunca com jogo aberto.

## Só mostrado, com sugestão

- Programas novos iniciando com o Windows.
- Outras causas de lentidão que não são seguras de mexer sozinho.

## Avançado

Regras, **Aceitar** individual, mudanças por perfil, histórico técnico,
diagnóstico e métricas detalhadas (frametime, 1% low).

## Visual

Estilo Configurações do Windows 11 / NVIDIA App: escuro, pouco conteúdo por
tela, cartões com título curto e uma frase, sem siglas na tela principal, sem
emojis.

## Sucesso

Um leigo instala, abre e entende sozinho como o PC está e, depois de jogar,
como foi a partida.

## Restrições

- Dados só no PC; nada é enviado para fora.
- Medição sem tocar no jogo: eventos do Windows (ETW) e NVAPI, sem overlay nem
  injeção. A segurança com anticheat deve ser confirmada nas fontes antes de
  implementar.
- A segurança atual (portões, restauração, atualização) não muda.
- Continua no GPUI.
- No máximo uma tela curta de boas-vindas.

## Fora do escopo

- Overlay durante o jogo.
- Telemetria enviada para fora.
- Desligar programas de inicialização sozinho.
- Outras "limpezas de PC" (registro, serviços).
- Outros idiomas.

## Nota

Isso reverte a decisão de `docs/intent/competitivo.md` de que medição/benchmark
dentro do app estava fora do escopo. O "sucesso por sensação" continua: a
medição é para o usuário acompanhar, não para o app provar ganho.
