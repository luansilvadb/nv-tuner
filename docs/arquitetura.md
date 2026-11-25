# Arquitetura do nv-tuner

Este documento resume as decisões de arquitetura. Ele evolui com o projeto;
mudanças relevantes devem ser registradas aqui.

## Escopo

- Somente Windows e GPUs NVIDIA.
- Atua **apenas na camada do driver**: perfis DRS (global e por aplicativo)
  via NVAPI, com FFI própria em Rust — sem depender de ferramentas externas
  ou de .NET.
- Não altera arquivos de configuração dos jogos e não injeta código em
  processos.

## Camadas

1. **`nvt-drs`** — FFI da NVAPI DRS. Único lugar com `unsafe` ligado ao
   driver: sessão DRS, leitura e gravação de configurações e perfis.
2. **`nvt-detect`** — detecção de GPU e capacidades; varredura de bibliotecas
   de jogos cruzada com o catálogo de perfis do DRS.
3. **`nvt-core`** — domínio puro e testável: capacidades, fatos, preferências,
   regras, avaliador e plano de configuração.
4. **`nvt-telemetry`** — medição passiva via PresentMon CLI (a partir da v0.2).
5. **`nvt-ipc`** — protocolo versionado (serde) do named pipe.
6. **`nvt-service`** — serviço do Windows com privilégios; único componente que
   grava no driver.
7. **`nvt-ui`** — interface sem privilégios de administrador.

O UAC é solicitado apenas na instalação (registro do serviço). No uso diário a
UI não precisa de elevação.

O serviço é registrado com o nome interno **`nvt-service`** e o nome de
exibição **“nv-tuner (serviço)”** (o que aparece no `services.msc`).

### Privilégio de gravação no DRS (descoberta)

No driver **621.40** (branch `r621_20`), gravar no DRS pela NVAPI
(`NvAPI_DRS_SaveSettings`) **funcionou num processo de integridade média, sem
administrador**: a escrita é feita pelo serviço do driver
(`NVDisplay.ContainerLocalSystem`), não pelo processo chamador. Detalhes e
procedimento em `docs/pesquisa/nvapi-drs.md`, seção 9.

Isso **não é garantido em outros drivers** (o esperado era a recusa `-137` sem
admin), então o app não conta com essa permissão nem a usa como barreira: a
política de quem grava fica no próprio app, e só o serviço grava. O serviço
continua justificado por:

- **medição passiva**: a captura ETW (PresentMon) exige administrador ou o
  grupo *Performance Log Users*;
- **reaplicação sem a UI aberta** (após atualização de driver, novo jogo etc.);
- **vigilância de deriva** contínua do DRS.

## Fluxo

```
detecção ──► fatos + capacidades ──► regras (TOML) + preferências
                                              │
                                              ▼
                                          avaliador
                                              │
                                              ▼
                                     plano de configuração
                                              │  (named pipe)
                                              ▼
                                 serviço: valida ─► aplica ─► registra
                                              │
                                              ▼
                                telemetria passiva (v0.2) ─► validação/rollback
```

1. **Detecção** levanta a GPU, o driver e os jogos instalados.
2. Os dados viram **fatos** (por jogo) e **capacidades** (por GPU) tipados.
3. O **avaliador** aplica as **regras** declarativas sobre fatos, capacidades e
   preferências.
4. O resultado é um **plano de configuração**: lista auditável de alterações
   com valor anterior, valor novo e regra de origem.
5. A UI envia o plano ao **serviço**, que valida e aplica, guardando o
   histórico para rollback.

## Modelo de decisão

Modelo híbrido:

- **Regras** declarativas são a base de toda decisão.
- **Medição passiva** (PresentMon CLI) confirma se o plano melhorou ou piorou.
- **Testes A/B** automáticos apenas em alavancas classificadas como de baixo
  risco.

### Função objetivo

Ordem de prioridade: **estabilidade > latência > FPS**. Uma regra que ganhe
FPS às custas de estabilidade perde para uma regra conservadora. A qualidade
visual é limitada por um teto escolhido pelo usuário (padrão: "Equilibrado").

## Política de autonomia

- O app pode aplicar planos automaticamente, desde que toda alteração seja
  registrada e reversível (rollback por jogo e global).
- Regressões detectadas pela telemetria disparam rollback para o último estado
  bom.
- O estado real do driver é sempre relido após aplicar, em vez de presumir que
  a gravação funcionou.

## Mudanças manuais

Quando o usuário altera uma configuração manualmente (no app ou em outra
ferramenta), ela passa a ser uma **escolha do usuário**: o avaliador não a
sobrescreve nas execuções seguintes, até que o usuário a libere
explicitamente.

## Segurança do serviço

- O serviço só aceita conexões locais no named pipe, com ACL restrita.
- Toda requisição é validada contra uma **lista permitida de IDs de
  configuração DRS**; IDs fora da lista são rejeitados, mesmo que venham de
  uma regra válida.
- Valores são validados por tipo e faixa antes da gravação.
- O serviço não executa comandos arbitrários nem lê/grava arquivos fora dos
  seus próprios diretórios de estado.

## Distribuição

Instalador MSI publicado em GitHub Releases com hash SHA-256 para conferência.
Inicialmente sem certificado de assinatura de código.
