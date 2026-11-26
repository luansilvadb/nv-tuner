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

## Chave de assinatura

A atualização automática só instala o que foi assinado com a chave Minisign do
projeto (`docs/spec/atualizacao.md` §5.5, §6.2 e §11). A chave pública fica no
repositório, em [`instalador/nv-tuner-minisign.pub`](instalador/nv-tuner-minisign.pub),
e é embutida nos binários por `include_str!`; a secreta fica só no secret
`NV_TUNER_MINISIGN_KEY` do Environment `release` e em backup offline.

> ⚠️ **Hoje o `.pub` contém um marcador provisório (`NVT-CHAVE-PROVISORIA`).**
> Enquanto ele existir, **todo release falha** (passo “Conferir chave oficial
> embutida”) e **todo serviço instalado recusa qualquer atualização**. O
> mantenedor faz os passos abaixo uma vez, antes do primeiro lançamento com
> atualizador.

### Gerar a chave (uma vez, máquina confiável, de preferência offline)

```powershell
# Fora do repositório; a ferramenta não sobrescreve arquivos existentes.
cargo run -p nvt-assinar -- gerar-chave --publica C:\chave\nv-tuner-minisign.pub --secreta C:\chave\nv-tuner-minisign.key
```

A chave é gerada **sem senha** (o workflow assina sem interação). Por uma falha
do crate `minisign` 0.7.9 no modo sem senha, a ferramenta usa
`generate_encrypted_keypair(Some(""))`; o resultado equivale a `minisign -G -W`.

### Gravar o secret (só a linha base64, sem imprimir a chave)

O `.key` tem duas linhas; o GitHub mascara os logs **por linha**, e a primeira
(“untrusted comment: …”) é texto comum. Grave **só a segunda linha** (base64): o
`nvt-assinar assinar` aceita essa linha e remonta a chave.

```powershell
# GitHub CLI autenticado como mantenedor; a linha vai pelo pipe, sem eco no terminal.
(Get-Content C:\chave\nv-tuner-minisign.key)[1] | gh secret set NV_TUNER_MINISIGN_KEY --env release --repo luansilvadb/nv-tuner
gh secret list --env release --repo luansilvadb/nv-tuner   # mostra só nome e data
```

**Não** crie o secret no nível do repositório: só o job ligado ao Environment
`release` pode lê-lo.

### Environment, ruleset de tags e proteção da `main` (Q10)

1. **Environment `release`**: *Settings → Environments → New environment* →
   `release`; em *Deployment branches and tags*, escolha *Selected branches and
   tags* e adicione só a regra de **tag** `v*`. Sem revisores obrigatórios (o
   lançamento continua automático). Pela CLI:

   ```powershell
   gh api -X PUT repos/luansilvadb/nv-tuner/environments/release -F "deployment_branch_policy[protected_branches]=false" -F "deployment_branch_policy[custom_branch_policies]=true"
   gh api -X POST repos/luansilvadb/nv-tuner/environments/release/deployment-branch-policies -f name="v*" -f type=tag
   ```

2. **Ruleset de tags**: *Settings → Rules → Rulesets → New tag ruleset*, alvo
   `v*`, com *Restrict creations*, *Restrict updates* e *Restrict deletions* e só
   o mantenedor na lista de *bypass*. Quem cria uma tag `v*` dispara a assinatura
   do que estiver naquele commit.
3. **Proteção da `main`**: *Settings → Rules → Rulesets → New branch ruleset*,
   alvo `main`, exigindo pull request e o status do workflow `CI` e bloqueando
   *force push* e exclusão. O release só publica commit que está na `main` com CI
   verde.

### Publicar a chave pública

```powershell
Copy-Item C:\chave\nv-tuner-minisign.pub instalador\nv-tuner-minisign.pub
cargo test -p nvt-atualizacao   # a chave decodifica e não é a de teste
git add instalador/nv-tuner-minisign.pub
git commit -m "Chave pública oficial de atualização"
```

A primeira versão lançada com essa chave é a primeira que aceita atualizações;
quem tem versão anterior instala essa à mão uma vez (bootstrap).

### Backup e perda

- Guarde `nv-tuner-minisign.key` em **dois meios offline** (ex.: pendrive
  cifrado + cofre de senhas) e apague a cópia da máquina de trabalho.
- **Sem a chave, nenhuma máquina instalada aceita atualizações.** Perdida a
  chave, é preciso gerar outra, publicar uma versão com a pública nova e
  **reinstalar à mão em todas as máquinas**: não há caminho automático.
- Vazamento: faça a rotação abaixo o quanto antes; não há revogação online.

### Rotação (`docs/spec/atualizacao.md` §5.5)

1. Gere a chave nova offline (`gerar-chave` em outra pasta) e faça backup.
2. Publique a versão **N** assinada pela chave **antiga** (secret ainda antigo),
   com a pública **nova** em `instalador/nv-tuner-minisign.pub`.
3. Com N já Latest, troque o secret pela linha base64 da chave nova; de N+1 em
   diante o workflow assina com ela.
4. Máquinas que pularem N continuam aceitando só a antiga: N é intermediária
   obrigatória enquanto houver máquinas antigas. Se a chave antiga vazou, essas
   máquinas precisam de reinstalação manual.

### Verificação manual (usuários)

Cada release traz `nv-tuner-X.Y.Z-x64.msi.minisig` e `atualizacao.json.minisig`.
Com o [minisign](https://jedisct1.github.io/minisign/) oficial e o
`nv-tuner-minisign.pub` do repositório:

```powershell
minisign -Vm nv-tuner-X.Y.Z-x64.msi -p nv-tuner-minisign.pub
minisign -Vm atualizacao.json -p nv-tuner-minisign.pub
```

O comentário confiável exibido deve ser `nv-tuner msi versao=X.Y.Z sha256=<hash>`
(e `nv-tuner atualizacao formato=1 versao=X.Y.Z` para o manifesto). Com o
repositório, `cargo run -p nvt-assinar -- verificar --dir <pasta com os cinco
arquivos>` usa o mesmo verificador do serviço.

## Como lançar uma versão

A release é criada pelo workflow [`release.yml`](.github/workflows/release.yml)
quando uma tag `vX.Y.Z` chega ao GitHub. O job roda no Environment `release` e,
em ordem: confere tag × `Cargo.toml`, a presença do secret, versão **maior** que a
Latest atual e a feature `laboratorio` desligada; compila em release (`--locked`,
sem cache); confere que a chave embutida **não** é a provisória; gera o MSI e o
`.sha256`; roda `nvt-assinar assinar --versao X.Y.Z --msi <msi>` (gera
`atualizacao.json`, `atualizacao.json.minisig` e `<msi>.minisig`); cria a release
em **rascunho** com os cinco arquivos; baixa os cinco para uma pasta limpa e roda
`nvt-assinar verificar --dir`; espera o workflow `CI` do mesmo commit **na `main`**
terminar com sucesso; e só então publica como **normal e Latest**. As notas saem
de [`.github/release-notes-template.md`](.github/release-notes-template.md) + a
seção da versão no [`CHANGELOG.md`](CHANGELOG.md). As máquinas instaladas pegam a
versão nova na verificação seguinte (até ~6 h).

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
   `X.Y.Z` (a tag precisa bater e ser maior que a Latest, senão o workflow
   falha) e rode `cargo build` para atualizar o `Cargo.lock`.
3. **Commit** na `main` (por pull request, com CI verde).
4. **Tag** num commit que já está na `main`: `git tag vX.Y.Z` e
   `git push origin vX.Y.Z`. Tag em commit fora da `main` nunca publica.
5. Acompanhe o workflow em *Actions* e confira a release publicada.

### Se o workflow falhar depois do rascunho

A release fica em **rascunho** (nunca vira Latest, então nenhuma máquina a vê).
Corrija a causa e, antes de reexecutar, apague o rascunho (a tag continua):

```powershell
gh release delete vX.Y.Z --repo luansilvadb/nv-tuner --yes
gh run rerun <id-da-execução> --repo luansilvadb/nv-tuner
```

Se a correção exigir commit novo, apague também a tag
(`gh release delete vX.Y.Z --repo luansilvadb/nv-tuner --cleanup-tag --yes` e
`git tag -d vX.Y.Z`) e crie-a de novo no commit corrigido. Nunca publique um
rascunho à mão: a verificação e a espera do CI são o que garante o que as
máquinas vão instalar.

## Licença

Ao contribuir, você concorda em licenciar sua contribuição sob a
[licença MIT](LICENSE) do projeto.
