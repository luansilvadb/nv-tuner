# Regras

Regras declarativas que o nv-tuner usa para decidir como configurar o perfil
do driver NVIDIA (global e por jogo). Lidas e validadas pelo crate
`nvt-core` e embutidas no binário.

| Arquivo | Conteúdo |
|---|---|
| `settings.toml` | Catálogo de settings: nome simbólico → ID DRS e valores aceitos (também é a lista permitida do serviço) |
| `base.toml` | Regras base: baseline global, sincronismo conforme a preferência do usuário e override de DLSS |

Formato, semântica do avaliador e como contribuir: [`docs/regras.md`](../docs/regras.md).

## Princípios

- Regras são dados, não código: revisáveis em PR e testáveis isoladamente.
- Nenhuma regra sobrescreve uma escolha manual do usuário.
- Preferências do usuário (sincronismo, limite de FPS) mandam nas settings
  que governam.
- Toda regra tem motivo e evidência, sem dados pessoais.
- Regra nova começa como `experimental`; só `estavel` entra no plano
  automático.
