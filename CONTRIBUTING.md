# Como contribuir

Obrigado pelo interesse! O nv-tuner está em pré-alfa: issues, discussões e
pull requests pequenos e focados são bem-vindos.

## Propor uma regra

As regras ficam em [`regras/`](regras/) e o formato está em
[`docs/regras.md`](docs/regras.md) (seção “Como contribuir com uma regra”).
Resumo:

1. **Formato.** Siga o esquema de `docs/regras.md`; settings novas entram em
   `regras/settings.toml` com ID e valores de fonte verificável (NVAPI
   `NvApiDriverSettings.h` ou NVIDIA Profile Inspector).
2. **Evidência medida.** `motivo` e `evidencia` são obrigatórios. Evidência
   boa é medição (PresentMon, mediana de várias capturas, comparada com a
   variação entre sessões), informando GPU, driver e jogo, ou fonte oficial.
   Opinião ou “dica de fórum” não basta.
3. **Status `experimental` até confirmação.** Toda regra nova entra como
   `experimental` (só sugestão, fora do plano automático). Vira `estavel` só
   com medição em pelo menos uma GPU da arquitetura alvo e sem regressão de
   estabilidade (prioridade: estabilidade > latência > FPS).
4. **Impacto visual honesto** e condições explícitas, como descrito no guia.
5. **Sem dados pessoais** em regras, evidências, logs ou capturas: nomes de
   usuário, caminhos da sua máquina, números de série, IDs de hardware.

## Código

Antes de abrir o PR, rode (o CI roda o mesmo):

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Testes que precisam do driver NVIDIA ou de administrador devem ser
`#[ignore = "motivo"]`: o CI não tem GPU NVIDIA. Nada de gravar no DRS em
testes não ignorados.

Textos voltados ao usuário, documentação e mensagens de commit são em
português do Brasil, com acentuação correta.

## Licença

Ao contribuir, você concorda em licenciar sua contribuição sob a
[licença MIT](LICENSE) do projeto.
