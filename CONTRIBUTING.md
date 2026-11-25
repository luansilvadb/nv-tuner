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

## Como lançar uma versão

A release é criada pelo workflow [`release.yml`](.github/workflows/release.yml)
quando uma tag `vX.Y.Z` chega ao GitHub: ele compila, gera o MSI e o
`.sha256` e publica a release como **normal e Latest**, com as notas montadas
de [`.github/release-notes-template.md`](.github/release-notes-template.md) +
a seção da versão no [`CHANGELOG.md`](CHANGELOG.md).

1. **CHANGELOG.** Renomeie `## [Não lançado]` para `## [X.Y.Z] - AAAA-MM-DD`,
   crie um `## [Não lançado]` vazio acima e atualize os links do rodapé.
   - Convenção do resumo (opcional): se a primeira linha não vazia da seção
     for um parágrafo em itálico (`*Texto*`), ele vira o subtítulo do título
     da release (`nv-tuner vX.Y.Z — Texto`) e não se repete nas notas.
   - Um item por linha (sem quebrar o texto): o GitHub mostra cada quebra de
     linha das notas como quebra de verdade.
   - Imagens: use URL `raw.githubusercontent.com/.../vX.Y.Z/...` da própria tag.
   - Sem a seção `## [X.Y.Z]`, o workflow falha antes de criar a release.
2. **Versão.** Suba `version` em `[workspace.package]` do `Cargo.toml` para
   `X.Y.Z` (a tag precisa bater, senão o workflow falha) e rode `cargo build`
   para atualizar o `Cargo.lock`.
3. **Commit** na `main`: `git commit -am "vX.Y.Z: <resumo>"` e `git push`.
4. **Tag:** `git tag vX.Y.Z` e `git push origin vX.Y.Z`.
5. Acompanhe o workflow em *Actions* e confira a release publicada.

## Licença

Ao contribuir, você concorda em licenciar sua contribuição sob a
[licença MIT](LICENSE) do projeto.
