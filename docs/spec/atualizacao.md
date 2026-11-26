# Especificação — atualização automática

Data: 2026-11-26. Fonte de verdade do escopo: [`docs/intent/atualizacao.md`](../intent/atualizacao.md).
Base técnica: [`docs/pesquisa/atualizacao-nv-tuner.md`](../pesquisa/atualizacao-nv-tuner.md)
(verificada em fontes primárias; citada como **P§n**) e
[`docs/pesquisa/atualizacao-orquestrador.md`](../pesquisa/atualizacao-orquestrador.md)
(engenharia reversa do mecanismo de inspiração; citada como **O§n**), além de
[`docs/arquitetura.md`](../arquitetura.md), [`docs/servico.md`](../servico.md) e
[`docs/ipc.md`](../ipc.md). As decisões do líder técnico estão marcadas **A1–A11**.

Convenções deste documento:

- Requisitos numerados `U-nn`. Cada um traz a forma de **verificação**: teste automatizado
  (**T**), inspeção de código ou de arquivo (**I**) ou laboratório em VM (**L**, §13).
- **Testes normais nunca acessam a rede nem instalam nada.** Rede, disco protegido,
  relógio, portão, lançador de processos e WTS ficam atrás de fachadas (traits) com
  implementações falsas em memória, como já se faz com `DriverStore` e `FonteProcessos`.
- “Versão” é sempre `X.Y.Z` numérico (o MSI compara só três campos, P§2); comparação
  numérica por campo, sem pré-release (o canal só vê releases estáveis).

---

## 1. Objetivo

O nv-tuner se atualiza sozinho para a última release estável publicada no GitHub: o
serviço verifica em segundo plano, baixa o MSI, confere a assinatura Minisign e instala
em silêncio, sem o usuário fazer nada; a interface só informa depois (“atualizado para a
vX”). Sucesso = publicar uma tag nova e, pouco depois, as máquinas estarem na versão nova.

## 2. Não-objetivos

1. Bloquear o app sem internet (o orquestrador bloqueia, O§1; aqui falha nunca bloqueia).
2. Canal de pré-release/beta.
3. Assinatura Authenticode (pendência D3 separada, `docs/arquitetura.md`, “Distribuição”).
4. Voltar de versão (downgrade), inclusive por comando.
5. Atualizar qualquer coisa além do próprio nv-tuner (driver, jogos, Windows).
6. Opção de desligar a atualização (A1: obrigatória).
7. Reiniciar o Windows, em qualquer código de saída (A6).
8. Relançar a UI depois do upgrade (A7).
9. Chave Minisign reserva embutida nesta versão (A3; a rotação é documentada, §5.5).
10. Retomar download interrompido (o MSI tem poucos MiB; baixa de novo).

---

## 3. Visão geral do fluxo

```
serviço (LocalSystem)
  agendador ── 5–15 min após iniciar, depois 6 h ± 30 min ──┐
                                                             ▼
  HTTP (WinHTTP) ── atualizacao.json + .minisig ── verificar manifesto (Minisign, versão)
                                                             │ versão > instalada e > maior vista
                                                             ▼
  HTTP ── MSI ──► %ProgramData%\nv-tuner\atualizacao\*.parcial (pasta só SYSTEM/Admins)
                                                             │ tamanho + SHA-256 no arquivo
                                                             ▼
  portão de processos (mesmo de portao.rs) ── jogo/anticheat/lista ilegível ─► adia, reavalia
                                                             │ liberado
                                                             ▼
  persistir "instalando" + sessões com agente ─► lancador: CreateProcessW(System32\msiexec.exe
                                                 /i <msi> /qn /norestart /l*v <log>) e não espera
                                                             │
  msiexec: Restart Manager fecha agente/UI ─► StopServices ─► troca arquivos ─► StartServices
                                                             ▼
serviço NOVO ao iniciar: lê estado "instalando" + log + versão própria ─► registra resultado
                         ─► relança agente (uma vez) nas sessões gravadas ─► IPC v6 informa a UI
```

---

## 4. Decisões e justificativas

| # | Decisão | Justificativa (pesquisa) |
|---|---|---|
| A1 | O serviço verifica; 1ª vez 5–15 min após iniciar; depois 6 h ± 30 min; falha de rede: espera exponencial a partir de 30 min, teto 6 h; 403/429 respeitam `Retry-After` (e `x-ratelimit-reset`). Sem opção de desligar | Só o serviço roda com a UI fechada e tem direito de instalar MSI perMachine. A pesquisa sugeria 24 h (P§5); 6 h atende “pouco depois” da intenção com custo desprezível: a URL fixa em `github.com` não é a API REST (cota de 60/h por IP só da API, P§5), e 4 pedidos/dia por máquina ficam muito abaixo de qualquer limite mesmo atrás de NAT. O atraso inicial evita competir com o boot (o serviço já é *delayed auto start*) |
| A2 | WinHTTP via `windows-sys`, `WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY`, só HTTPS, redirecionamento manual só para hosts permitidos, timeouts, limites de tamanho, User-Agent genérico | Schannel, revogação, proxy do sistema/WPAD e raízes corporativas de graça, sem dependência nova; `ureq` só lê proxy de variáveis de ambiente, que o SYSTEM não tem (P§3). `DEFAULT_PROXY` é obsoleto (P§3) |
| A3 | Manifesto `atualizacao.json` assinado (Minisign pré-hash `ED`), `minisign-verify` 0.3 com `allow_legacy=false`, chave pública embutida, comentário confiável com a mesma versão; MSI confere tamanho e SHA-256; `.minisig` do MSI publicado; antidowngrade/replay pela maior versão vista | O orquestrador assina só o instalador e aceita legado (O§1); assinar o manifesto amarra versão, nome, tamanho e hash num único objeto autenticado, e o comentário confiável impede reaproveitar uma assinatura antiga sob outro manifesto (P§4). Novas implementações devem exigir `ED` (P§4) |
| A4 | Download para `%ProgramData%\nv-tuner\atualizacao\` (DACL protegida SYSTEM/Administradores), sem seguir reparse points, verificação sobre o arquivo já na pasta, handle mantido sem compartilhamento de escrita | `%windir%\Temp` e `%TEMP%` têm histórico de plantio (P§3); fluxo anti-TOCTOU de P§3. Viabilidade do handle aberto: §8.3 |
| A5 | Portão de processos imediatamente antes de lançar o msiexec | Mesmo padrão L4 “portão imediatamente antes de cada gravação” (commit 01152b7; P§6) |
| A6 | Único módulo auditado `nvt-service::atualizacao::lancador` chama `CreateProcessW` (msiexec) e `CreateProcessAsUserW` (agente); argumentos fixos; destacado; não espera; resultado lido pelo serviço novo | `MsiInstallProduct` no serviço deadlocka com o `StopServices` do próprio pacote (P§1); tarefa agendada e auxiliar fora de Program Files só aumentam superfície (P§1) |
| A7 | Restart Manager fecha o agente; o serviço novo relança o agente uma vez nas sessões que tinham agente; a UI mostra “reabra” | RM em `/qn` sempre fecha aplicativos, não relança os de SYSTEM (P§2); `WTSQueryUserToken` + `CreateProcessAsUserW` é o único relançamento automático imediato (P§2, opção a) |
| A8 | IPC v6 com `Estado.atualizacao` | O envelope recusa versão diferente (`crates/nvt-ipc/src/quadro.rs:68`): mudança de campo `deny_unknown_fields` exige bump |
| A9 | Ferramenta própria `nvt-assinar` (crate `minisign` fixado) assina e verifica com o mesmo código de verificação do serviço; release em rascunho, verificação de volta, CI verde, só então Latest | `-W` (sem senha) é o único modo não interativo documentado do minisign (P§4); o binário próprio evita baixar ferramenta externa no runner (minisign não está confirmado no `windows-latest`, P§4) |
| A10 | Bootstrap manual: quem tem v0.0.2 instala uma vez a primeira versão com atualizador | v0.0.2 não tem código de atualização |
| A11 | Laboratório obrigatório em VM antes do primeiro lançamento | Sobrevivência do msiexec ao stop do serviço não é documentada (P§1, pendência 1) |

### 4.1 Desvios da implementação (registrados em 2026-11-26, lotes 1–3)

O texto dos requisitos abaixo já foi alinhado; esta tabela guarda o que mudou e por quê.

| # | Original | Implementado | Justificativa |
|---|---|---|---|
| D-01 | `CHAVE_OFICIAL` como literal em `chave.rs`, entregue pelo Lote 0 | `PUB_OFICIAL = include_str!("instalador/nv-tuner-minisign.pub")` e `CHAVE_OFICIAL` = segunda linha desse arquivo; hoje o arquivo traz o marcador `NVT-CHAVE-PROVISORIA` e `Chave::oficial()` falha fechada (`ErroVerificacao::ChaveProvisoria`). **Lote 0 adiado** | Uma única fonte para a chave (binários, README, verificação manual), sem copiar a linha à mão; o marcador permite implementar e testar tudo antes de o mantenedor gerar a chave, sem risco: enquanto existir, todo release falha (passo do `release.yml`) e todo serviço recusa atualização |
| D-02 | U-22: `ReOpenFile(GENERIC_READ, FILE_SHARE_READ)` direto do handle de escrita | Troca por um handle **intermediário** de leitura que compartilha tudo (`ReOpenFile`); fecha-se o de escrita; o handle final é `ReOpenFile(intermediário, GENERIC_READ, FILE_SHARE_READ)`; **SHA-256 e tamanho recalculados no handle final** e comparados | A abertura direta é violação de compartilhamento (o novo handle negaria a escrita que o antigo já tem); a janela entre fechar o de escrita e abrir o final exige escrever numa pasta só SYSTEM/Administradores e é detectada pelo novo hash |
| D-03 | A6: msiexec “destacado” | `CREATE_NO_WINDOW \| CREATE_NEW_PROCESS_GROUP \| CREATE_UNICODE_ENVIRONMENT` (+ `CREATE_BREAKAWAY_FROM_JOB` se em job, com nova tentativa sem o flag), **sem** `DETACHED_PROCESS`, ambiente mínimo (`SystemRoot`, `windir`, `TEMP`/`TMP` = pasta protegida) | `DETACHED_PROCESS` não combina com `CREATE_NO_WINDOW` (P§1); o ambiente mínimo não herda variáveis do serviço |
| D-04 | U-33: guardar o handle do processo msiexec e consultar `WaitForSingleObject` | O lançador **fecha** os handles de processo e thread logo após `CreateProcessW`; o resultado vem só do log `/l*v`, relido a cada 60 s (`RELER_LOG`) enquanto `fase = instalando`, com teto de 2 h | Menos estado e menos handles vivos no serviço que será parado; o log já é a fonte de verdade do serviço novo (U-35), então o mesmo caminho cobre o serviço antigo ainda vivo (ex.: 1618) |
| D-05 | U-41: agente sai com código próprio ao receber `versao_incompativel` | O agente trata serviço/protocolo diferente como **indisponível temporário** (não grava, não aplica, tenta de novo com recuo) e isso não conta como desinstalação; só sai sozinho com serviço sem responder por 120 s **e** executável ou entrada `Run` ausentes continuamente por mais 120 s | Sair na primeira resposta incompatível derrubaria o agente em toda janela de upgrade mesmo quando o RM não o fechou; o relançamento (U-43) continua sendo o caminho normal |
| D-06 | U-60/U-61: subcomandos `manifesto`, `assinar --chave-env … <arq> --comentario`, `verificar --dir --versao` | `gerar-chave --publica <pub> --secreta <key>`; `assinar --versao X.Y.Z --msi <msi>` (gera `atualizacao.json`, `atualizacao.json.minisig` e `<msi>.minisig` ao lado do MSI, comentários montados pela ferramenta; chave só de `NV_TUNER_MINISIGN_KEY`, aceitando a linha base64 ou o `.key` inteiro); `verificar --dir <pasta>` (versão lida do manifesto verificado) | Menos parâmetros livres no workflow (comentário e nomes não são digitados), e a geração da chave usa o mesmo crate fixado, sem baixar o minisign no runner nem na máquina do mantenedor (Q11 resolvida: secret = só a linha base64) |
| D-07 | `minisign` 0.11 com `-G -W` para gerar a chave | `nvt-assinar gerar-chave`, que usa `minisign` **0.7.9** com `KeyPair::generate_encrypted_keypair(Some(""))` | `generate_unencrypted_keypair` do 0.7.9 não grava a soma de conferência da chave secreta (bug do modo sem senha); com senha vazia o resultado é equivalente a `-W` e carrega em `SecretKey` sem interação |
| D-08 | §7: transições no laço principal, rede/hash numa thread de trabalho devolvendo `Evento::Atualizacao(..)` | O `Atualizador` inteiro roda numa **thread própria** (`nvt-atualizacao`, `ThreadAtualizacao` em `laco.rs`); `tick` é síncrono; a exclusão com o ciclo DRS é a `Exclusao` (o portão do atualizador reserva a instalação esperando o ciclo DRS em curso; reservada, nenhum ciclo DRS começa até o fim do `tick`); o laço só lê a última projeção e repassa as sessões com agente | Mantém o laço DRS livre de downloads de minutos e de estado de atualização; a garantia que importava (“nenhuma instalação começa durante uma gravação DRS”) fica explícita num único ponto testável |
| D-09 | §11.1: recuperação do SCM inalterada (Q12) | `ThirdFailureActionType="restart"` e `REBOOT=ReallySuppress` no WiX | Sem serviço não há atualizador (Q12); nunca reiniciar o Windows também em instalação interativa |
| D-10 | U-64: “commit na `main`” e “CI verde” conferidos **depois** da assinatura, antes de publicar; Environment `release` sem revisores | `release.yml` em dois jobs: `verificar` (sem Environment/secret) faz fetch explícito da `main`, exige `merge-base --is-ancestor` com `origin/main`, espera o CI de push do mesmo SHA na `main` (até 30 min; falha sem run ou com conclusão ≠ `success`) e só então confere tag × versão, versão > Latest, feature `laboratorio` e chave não provisória; `msi` (Environment `release`, `needs: verificar`) faz checkout do SHA exato verificado, confere que a tag ainda aponta para ele e só depois compila, assina, cria o rascunho, verifica e publica. Environment `release` com **revisores obrigatórios** (o mantenedor) | Achado ALTO de revisão: uma tag `v*` sobre commit fora da `main` entregava `NV_TUNER_MINISIGN_KEY` ao `nvt-assinar` compilado desse commit não revisado. O resto do risco (conta comprometida, push de tag) fica com a proteção da `main`, o ruleset de tags `v*` e a aprovação manual do Environment (CONTRIBUTING.md) |

---

## 5. Modelo de ameaças e mitigações

Ativos: execução de código como SYSTEM (o MSI roda como SYSTEM), integridade do estado
em `%ProgramData%\nv-tuner`, disponibilidade (não derrubar partida nem a máquina),
privacidade.

| # | Ameaça | Mitigação | Requisitos |
|---|---|---|---|
| M1 | Rede/proxy/TLS adulterados entregam manifesto ou MSI falsos | Minisign sobre o manifesto com chave embutida; MSI amarrado por SHA-256 e tamanho do manifesto assinado; TLS do sistema; só HTTPS | U-10..U-16 |
| M2 | Replay: manifesto antigo assinado, ou MSI antigo sob manifesto novo | Versão do comentário confiável = versão do manifesto; só instala `> instalada` e `> maior_vista` persistida; nome do MSI derivado da versão | U-12, U-17, U-18 |
| M3 | Conta GitHub comprometida publica release | Sem a chave privada a assinatura falha; chave só em secret; release só sai de tag e com CI verde | U-12, U-60..U-66 |
| M4 | Vazamento da chave privada | Rotação por release assinada pela chave antiga (§5.5); sem revogação online (documentado) | U-67 |
| M5 | Usuário local planta/troca o MSI entre verificar e instalar (TOCTOU), junção/link na pasta | Pasta com DACL protegida só SYSTEM/Administradores, sem reparse points, verificação no arquivo já na pasta, handle sem compartilhamento de escrita/exclusão até o msiexec abrir | U-20..U-25 |
| M6 | Execução de binário errado (PATH, diretório atual, argumentos injetados) | Caminho por `GetSystemDirectoryW`, `lpApplicationName` absoluto, argumentos fixos, caminhos só da pasta protegida com nome validado por regex, `bInheritHandles=FALSE`, diretório atual = System32 | U-30..U-34 |
| M7 | Atualização no meio de partida (bloqueio por anticheat) | Portão imediatamente antes do lançamento; falha fechada com lista ilegível | U-26 |
| M8 | Loop de instalação falhando | Contador por versão; 3 falhas abandonam a versão até surgir maior; 1618 e portão não contam | U-37, U-38 |
| M9 | Respostas gigantes ou lentas (DoS) | Manifesto ≤ 16 KiB, assinatura ≤ 4 KiB, MSI ≤ 64 MiB e = tamanho assinado; timeouts; teto de redirecionamentos | U-04..U-07 |
| M10 | Rastreamento | User-Agent sem dados pessoais, sem identificador, sem cookies; nenhuma telemetria | U-08 |
| M11 | Relançamento do agente vira primitivo de execução como usuário | Só o executável fixo `<INSTALLFOLDER>\nvt-agente.exe` (pasta de Program Files lida do caminho do próprio serviço), só nas sessões gravadas antes do upgrade, uma vez | U-43..U-46 |

### 5.5 Rotação de chave (documentação, sem código nesta versão)

1. O mantenedor gera a chave nova offline e faz backup.
2. Publica a versão N assinada pela chave **antiga**, embutindo a pública **nova** em
   `instalador/nv-tuner-minisign.pub` (lida por `include_str!`, D-01).
3. A partir de N+1, o workflow assina com a chave nova (troca do secret).
4. Máquinas que pularam N continuam aceitando só a antiga: N precisa ficar como
   intermediária obrigatória enquanto houver máquinas antigas; se a chave antiga vazou,
   essas máquinas precisam de reinstalação manual (A10). Não há revogação online.

---

## 6. Formatos

### 6.1 Manifesto `atualizacao.json`

Publicado em toda release; obtido por
`https://github.com/luansilvadb/nv-tuner/releases/latest/download/atualizacao.json`
(e `…/atualizacao.json.minisig`), que aponta para a release marcada Latest, nunca
rascunho nem pré-release (O§1, P§5).

```json
{
  "formato": 1,
  "produto": "nv-tuner",
  "versao": "0.1.0",
  "msi": "nv-tuner-0.1.0-x64.msi",
  "tamanho": 4718592,
  "sha256": "3f2a9c…64 dígitos hexadecimais minúsculos…"
}
```

Regras de leitura (`deny_unknown_fields`, UTF-8 sem BOM, ≤ 16 KiB):

- `formato` = 1; outro valor → recusa (“formato desconhecido”), sem contar falha.
- `produto` = `"nv-tuner"` exato.
- `versao` casa `^(0|[1-9]\d{0,2})\.(0|[1-9]\d{0,2})\.(0|[1-9]\d{0,4})$` e respeita os
  limites do MSI (255.255.65535, P§2).
- `msi` = exatamente `nv-tuner-<versao>-x64.msi` (o nome que `instalador/build-msi.ps1:88`
  gera). A URL do MSI é **derivada**, nunca lida do manifesto:
  `https://github.com/luansilvadb/nv-tuner/releases/download/v<versao>/<msi>`.
- `tamanho` inteiro em `1..=67108864` (64 MiB).
- `sha256` casa `^[0-9a-f]{64}$`.

### 6.2 Assinaturas e comentário confiável

- Algoritmo: Minisign pré-hash `ED` (BLAKE2b-512 + Ed25519). Verificação com
  `minisign-verify` 0.3, `pk.verify(bytes, &sig, false)` (P§4).
- `atualizacao.json.minisig` — comentário confiável **exato**:

  ```
  nv-tuner atualizacao formato=1 versao=0.1.0
  ```

  O cliente exige igualdade byte a byte com o texto montado a partir do manifesto já
  decodificado (`format!("nv-tuner atualizacao formato={} versao={}", …)`).
- `nv-tuner-0.1.0-x64.msi.minisig` — comentário confiável:

  ```
  nv-tuner msi versao=0.1.0 sha256=<sha256>
  ```

  Publicado para conferência manual (`minisign -Vm nv-tuner-0.1.0-x64.msi -P <pub>`);
  o serviço não o baixa (o MSI já está amarrado pelo SHA-256 do manifesto assinado).
- `.minisig` ≤ 4 KiB; comentário não confiável ignorado.
- Chave pública: `instalador/nv-tuner-minisign.pub` é a única fonte, embutida em
  compilação por `include_str!` (`PUB_OFICIAL` em `crates/nvt-atualizacao/src/chave.rs`);
  `CHAVE_OFICIAL` é a sua segunda linha (base64, `RW…`). Nunca lida de arquivo em
  execução nem de variável de ambiente em build de release. Enquanto o arquivo tiver o
  marcador `NVT-CHAVE-PROVISORIA` (ou a linha não decodificar), `Chave::oficial()`
  falha fechada e o `release.yml` se recusa a publicar (D-01).

### 6.3 Estado persistido `%ProgramData%\nv-tuner\atualizacao\estado.json`

Gravado com `diretorio::criar_arquivo_privado` + troca atômica (mesmo padrão de
`servico.json`); ilegível → tratado como vazio **exceto** `maior_vista`, que é recuperada
de `estado.json.bak` (cópia gravada a cada mudança); perder ambos só pode baixar a
proteção até a versão instalada, nunca abaixo (U-18).

```json
{
  "formato": 1,
  "maior_vista": "0.1.0",
  "ultima_verificacao_unix": 1764158400,
  "ultimo_resultado": "sem_novidade",
  "proxima_verificacao_unix": 1764180000,
  "espera_rede_s": 0,
  "falhas": { "0.1.0": 1 },
  "abandonadas": ["0.0.9"],
  "fase": {
    "tipo": "instalando",
    "de": "0.0.3",
    "para": "0.1.0",
    "sha256": "…",
    "msi": "nv-tuner-0.1.0-x64.msi",
    "log": "instalacao-0.1.0.log",
    "pid": 4242,
    "inicio_unix": 1764158460,
    "sessoes_agente": [{ "sessao": 1, "sid": "S-1-5-21-…-1001" }]
  },
  "ultima_atualizacao": { "de": "0.0.3", "para": "0.1.0", "quando_unix": 1764158520, "codigo": 0 },
  "ultima_falha": { "versao": "0.1.0", "codigo": 1603, "quando_unix": 1764158520 }
}
```

`ultimo_resultado` ∈ `sem_novidade | baixada | adiada_portao | adiada_msi_ocupado |
erro_rede | erro_limite | erro_assinatura | erro_manifesto | erro_disco`. Vistos por SID
ficam em `usuarios.json` (§10).

Arquivos da pasta: `estado.json`, `estado.json.bak`, `nv-tuner-X.Y.Z-x64.msi(.parcial)`,
`instalacao-X.Y.Z.log`. Limpeza no início do serviço: apaga `.parcial`, MSIs e logs que
não sejam do último resultado (U-25).

---

## 7. Máquina de estados

Estados (`fase.tipo`), um por vez; transições pela função pura
`nvt_atualizacao::transicao`, aplicadas pelo `Atualizador` numa **thread própria**
(`nvt-atualizacao`, `ThreadAtualizacao` em `crates/nvt-service/src/laco.rs`), onde rede e
hash rodam de forma síncrona dentro do `tick`. A exclusão com o ciclo DRS é a
`Exclusao` de `laco.rs`: o portão do atualizador reserva a instalação esperando o
ciclo DRS em curso, e reservada nenhum ciclo DRS começa até o fim do `tick`; então
nenhuma instalação começa durante uma gravação DRS (D-08).

| De | Evento | Para | Persistir |
|---|---|---|---|
| `ocioso` | prazo `proxima_verificacao` | `verificando` | — |
| `verificando` | rede/HTTP falha | `ocioso` | `espera_rede_s` = min(max(30 min, 2×anterior), 6 h); 403/429: max(espera, `Retry-After`) |
| `verificando` | manifesto inválido / assinatura inválida | `ocioso` | `ultimo_resultado`, próxima em 6 h ± 30 min (não é falha de rede) |
| `verificando` | versão ≤ instalada, < `maior_vista` ou abandonada (igual a `maior_vista` segue para `baixando`, U-17: senão um download interrompido nunca seria retomado) | `ocioso` | `sem_novidade`, próxima em 6 h ± 30 min |
| `verificando` | versão nova válida | `baixando` | `maior_vista` = versão (antes de baixar) |
| `baixando` | erro / tamanho / hash | `ocioso` | erro; espera exponencial (rede) ou 6 h (hash/tamanho) |
| `baixando` | MSI verificado | `pronta` | `baixada` |
| `pronta` | portão adia (jogo, anticheat, lista ilegível) | `pronta` | `adiada_portao` + desde; reavalia a cada 60 s (mesmo passo do adiamento DRS) |
| `pronta` | `Global\_MSIExecute` existente | `pronta` | `adiada_msi_ocupado`; reavalia em 30 min |
| `pronta` | portão libera | `instalando` | `fase` completa **antes** de `CreateProcessW` |
| `pronta` | handle perdido / arquivo alterado / > 24 h | `ocioso` | descarta; nova verificação |
| `instalando` | `CreateProcessW` falha | `ocioso` | conta falha (código = erro Win32) |
| `instalando` | serviço novo inicia com versão própria = `para` | `ocioso` | `ultima_atualizacao` (código do log, 0/3010); limpa `falhas[para]`; relança agentes |
| `instalando` | serviço inicia com versão = `de` e log com código | `ocioso` | 1618 → `adiada_msi_ocupado` sem contar; outro → `falhas[para]`+1; 3ª → `abandonadas` |
| `instalando` | serviço inicia com versão = `de`, sem código no log e ≤ 2 h (ex.: serviço antigo religado pelo rollback enquanto o msiexec ainda termina) | `instalando` | nada; relê o log a cada 60 s; nenhuma verificação nova nem lançamento enquanto estiver aqui |
| `instalando` | serviço inicia, sem código no log e > 2 h | `ocioso` | conta falha com código `-1` (“resultado desconhecido”) |
| `instalando` | serviço antigo ainda vivo e o log relido (a cada 60 s) já traz o código (ex.: 1618) | `ocioso` | código do log, como nas linhas acima (D-04) |

A versão instalada é `env!("CARGO_PKG_VERSION")` do próprio serviço (o MSI é o único
caminho de instalação e instala os três binários juntos).

---

## 8. Requisitos

### 8.1 Agendamento e rede (`U-01..U-09`)

- **U-01** Primeira verificação entre 5 e 15 min após o serviço iniciar (uniforme);
  depois a cada 6 h ± 30 min. O jitter vem de `BCryptGenRandom`, nunca persistido nem
  enviado. Prazos contam em relógio monotônico (`Instant`); `proxima_verificacao_unix`
  persistido é só informativo e, ao iniciar, um valor no passado ou a mais de 7 h no
  futuro (relógio ajustado) é descartado e recalculado. Relógio errado que quebre o TLS
  cai em U-02 (falha de rede). **T** (relógio falso, inclusive salto para trás de anos).
- **U-02** Falha de rede/HTTP 5xx: espera 30 min, 60, 120, 240, 360 (teto 6 h); zera no
  primeiro sucesso. 403/429: espera = max(espera exponencial, `Retry-After` em segundos
  ou data HTTP, `x-ratelimit-reset`), teto 24 h. **T**.
- **U-03** Nenhuma configuração, flag, chave de registro ou requisição IPC desliga a
  verificação. **I** (busca no código e na CLI).
- **U-04** WinHTTP com `WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY`, `WINHTTP_FLAG_SECURE_DEFAULTS`
  quando disponível, `WINHTTP_OPTION_REDIRECT_POLICY = NEVER` e redirecionamento manual
  (≤ 5 saltos) só para `https://` e hosts exatos: `github.com`,
  `objects.githubusercontent.com`, `release-assets.githubusercontent.com` (P§3). Host fora
  da lista, `http://`, porta ≠ 443 ou credenciais na URL → erro. **T** (política de URL
  pura) + **L**.
- **U-05** Timeouts: resolução 15 s, conexão 15 s, envio/recebimento 30 s por operação,
  total 60 s para manifesto e 10 min para o MSI; cancelado no stop do serviço. **T**.
- **U-06** Limites: manifesto ≤ 16 KiB, `.minisig` ≤ 4 KiB, MSI ≤ 64 MiB e igual ao
  `tamanho` assinado; `Content-Length` presente e divergente → aborta antes de ler; leitura
  para ao exceder. **T**.
- **U-07** Só status 200 é sucesso (302/301/307/308 tratados por U-04); 404 do manifesto =
  “nenhuma release com atualizador” (sem novidade, sem espera exponencial). **T**.
- **U-08** Cabeçalhos enviados: só `User-Agent: nv-tuner-atualizador/<versão>` e
  `Accept: */*`; sem cookies (`WINHTTP_DISABLE_COOKIES`), sem identificadores, sem
  credenciais automáticas (`WINHTTP_AUTOLOGON_SECURITY_LEVEL_HIGH`). **T** (montagem de
  cabeçalhos) + **I**.
- **U-09** Sem internet, nada é bloqueado nem exibido como erro intrusivo: o restante do
  serviço segue; a UI mostra só “última verificação” e o motivo. **T** + **L**.

### 8.2 Manifesto e assinatura (`U-10..U-19`)

- **U-10** O manifesto só é decodificado depois de a assinatura Minisign dos **bytes
  recebidos** verificar com `CHAVE_OFICIAL` e `allow_legacy=false`. **T** (vetores
  gerados no teste com chave de teste via `minisign`, dev-dependency).
- **U-11** Assinatura legada (`Ed`), de outra chave, truncada, ausente ou com comentário
  global adulterado → recusa, `erro_assinatura`, nada baixado. **T**.
- **U-12** Comentário confiável ≠ `nv-tuner atualizacao formato=1 versao=<versao>` →
  recusa. **T**.
- **U-13** Campos e formatos conforme §6.1; qualquer desvio → `erro_manifesto`. **T**.
- **U-14** MSI: tamanho e SHA-256 calculados sobre o arquivo na pasta protegida, lendo
  pelo mesmo handle que o escreveu (não do buffer de rede). Divergência apaga o
  `.parcial`. **T** (sistema de arquivos falso) + **T** Windows em pasta temporária.
- **U-15** A chave embutida não é a de teste e decodifica como pública Minisign válida;
  em build de release não existe forma de substituí-la (sem `option_env!` fora da
  feature `laboratorio`). Enquanto `instalador/nv-tuner-minisign.pub` tiver o marcador
  provisório, toda verificação oficial falha fechada (`ChaveProvisoria`) e o
  `release.yml` falha no passo “Conferir chave oficial embutida” (D-01). **T** + **I**.
- **U-15a** Feature `laboratorio` (URL base e chave de ensaio) não vaza para release:
  (a) teste normal confere que `CHAVE_OFICIAL` é igual à linha base64 de
  `instalador/nv-tuner-minisign.pub` (`include_str!`) e diferente da chave de ensaio;
  (b) a chave e a URL de ensaio só existem em código `#[cfg(feature = "laboratorio")]`,
  e o crate tem `#[cfg(all(feature = "laboratorio", not(debug_assertions)))] compile_error!`;
  (c) o `release.yml` compila com `--locked` sem `--features`/`--all-features` e um passo
  falha se `cargo metadata`/`cargo tree -e features` mostrar `laboratorio` ativa; (d)
  `nvt-assinar verificar` na release (U-63) usa `CHAVE_OFICIAL`, então um MSI de
  laboratório assinado com a chave de ensaio nunca passa. **T** + **I**.
- **U-16** O verificador é o mesmo código no serviço e em `nvt-assinar verificar`
  (crate `nvt-atualizacao`). **I**.
- **U-17** Só instala se `versao > instalada` **e** `versao ≥ maior_vista` (igualdade só
  quando a própria `maior_vista` é a que está sendo tentada), e não está em
  `abandonadas`. **T** (tabela de casos: igual, menor, maior, abaixo da maior vista).
- **U-18** `maior_vista` persiste antes do download e nunca diminui; é no mínimo a
  versão instalada; perda do estado cai para a versão instalada. **T**.
- **U-19** Um manifesto que surgir com versão maior que uma `abandonada` volta a ser
  tentado normalmente. **T**.

### 8.3 Download e arquivo protegido (`U-20..U-25`)

- **U-20** A pasta `%ProgramData%\nv-tuner\atualizacao\` é criada por
  `diretorio::criar_seguro` com DACL protegida `D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)`
  (sem a leitura de Usuários que a raiz concede) e dono Administradores; a cada uso,
  `diretorio::verificar` não pode acusar problema; reparse point → recusa e
  `erro_disco`. **T** (Windows, pasta temporária).
- **U-21** O `.parcial` é criado com `CreateFileW(CREATE_NEW, GENERIC_READ|GENERIC_WRITE|DELETE,
  FILE_SHARE_READ, FILE_FLAG_OPEN_REPARSE_POINT)` via `diretorio::abrir_sem_reparse`. **T**.
- **U-22** Após verificar, o arquivo é renomeado pelo próprio handle
  (`SetFileInformationByHandle(FileRenameInfo)`, sem substituir) para o nome final; o
  handle de escrita é trocado por um handle só de leitura (`GENERIC_READ`,
  `FILE_SHARE_READ`) **via handle intermediário** (D-02), com SHA-256 e tamanho
  recalculados no handle final; esse handle permanece aberto até: o serviço parar (o MSI
  para o serviço durante o upgrade), a fase sair de `pronta`/`instalando`, ou 30 min
  após o lançamento. **T** (o handle impede escrita e exclusão por outro handle) + **L**.

  *Viabilidade (A4).* O cliente `msiexec` e o `msiserver` abrem o pacote só para leitura
  (armazenamento estruturado em modo de leitura, negando escrita); isso é compatível com
  um handle nosso com `FILE_SHARE_READ` e acesso de leitura+escrita? **Não**: quem abre
  depois com negação de escrita (`FILE_SHARE_READ` sem `FILE_SHARE_WRITE`) falha se já
  existe handle com `GENERIC_WRITE`. Por isso, antes de lançar, o handle de escrita é
  trocado por um handle **só de leitura** (`GENERIC_READ`, `FILE_SHARE_READ`). Abri-lo
  direto do handle de escrita é violação de compartilhamento; a implementação abre por
  `ReOpenFile` um intermediário de leitura que compartilha tudo (mesmo objeto, sem nova
  resolução de caminho), fecha o de escrita, abre o final por
  `ReOpenFile(intermediário, GENERIC_READ, FILE_SHARE_READ)` e recalcula o hash nele
  (D-02). Esse handle de leitura nega escrita e
  exclusão a terceiros e é compatível com a abertura de leitura do Windows Installer.
  A compatibilidade real é **inferência** sobre o modo de abertura do Windows Installer
  (não documentado): o laboratório (L-02) confirma. **Alternativa segura se falhar**:
  fechar o handle antes do `CreateProcessW` e recalcular o SHA-256 por um handle novo
  sem reparse imediatamente antes do lançamento; a janela residual exige escrever numa
  pasta que só SYSTEM e Administradores escrevem, ou seja, já ser administrador (P§3).
- **U-23** Nome final validado por regex (§6.1) e caminho montado como
  `<pasta canonicalizada>\<nome>`; nada vindo da rede vira caminho sem essa validação. **T**.
- **U-24** Disco cheio/erro de E/S → `erro_disco`, `.parcial` apagado, próxima tentativa
  pela espera exponencial. **T**.
- **U-25** Limpeza ao iniciar: apaga `.parcial`; mantém só o MSI e o log da última
  instalação; nunca segue links. **T**.

### 8.4 Portão e instalação (`U-26..U-39`)

- **U-26** Imediatamente antes de `CreateProcessW`, o serviço chama
  `Portao::verificar(&base, &extras)` (`crates/nvt-engine/src/portao.rs:187`) com a mesma
  base e os mesmos extras das gravações DRS; `Err(Adiamento)` (inclusive lista
  indisponível) → estado `pronta`, sem lançar. **T** (portão falso).
- **U-27** Com `OpenMutexW(SYNCHRONIZE, FALSE, "Global\\_MSIExecute")` bem-sucedido →
  adia 30 min sem contar falha. **T** (fachada) + **L**.
- **U-28** Antes de lançar, persiste `fase = instalando` com `sessoes_agente` (sessões
  com agente presente no coordenador, `crates/nvt-service/src/usuario.rs`). Falha ao
  persistir → não lança. **T**.
- **U-29** `crate::atualizacao::lancador` é o único arquivo de produção com
  `CreateProcessW`/`CreateProcessAsUserW`; `ShellExecute*`, `WinExec`, `CreateProcessA`,
  `CreateProcessWithTokenW`, `CreateProcessWithLogonW` e `Command::new` continuam
  proibidos em todo lugar. **T** (`crates/nvt-core/tests/proibicoes.rs`, §9.6).
- **U-30** Executável: `GetSystemDirectoryW()` + `\msiexec.exe`, passado em
  `lpApplicationName`; nunca busca no PATH. **T** (montagem pura) + **I**.
- **U-31** Linha de comando exata:
  `"<System32>\msiexec.exe" /i "<pasta>\nv-tuner-X.Y.Z-x64.msi" /qn /norestart /l*v "<pasta>\instalacao-X.Y.Z.log"`.
  Nenhuma propriedade pública (`RESTAURAR_PERFIL` nunca é passada). Caminhos sem aspas
  internas (garantido por U-23). A pasta vem de `SHGetKnownFolderPath(FOLDERID_ProgramData)`
  (nunca de variável de ambiente) e é passada **sem** o prefixo `\\?\` que
  `fs::canonicalize` devolve; a montagem recusa caminho com `"`, `%`, controle, ou que
  não comece por letra de unidade + `:\`. **T** (inclusive pasta com espaços).
- **U-32** `bInheritHandles = FALSE`; flags `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP |
  CREATE_UNICODE_ENVIRONMENT` (não combina com `DETACHED_PROCESS`, P§1);
  `CREATE_BREAKAWAY_FROM_JOB` só se `IsProcessInJob` = verdadeiro, e falha nessa
  tentativa é registrada e lança sem o flag; `lpCurrentDirectory` = System32; ambiente
  mínimo (`SystemRoot`, `windir`, `TEMP`/`TMP` = pasta protegida), em vez de
  `DETACHED_PROCESS` e do ambiente herdado (D-03). **T** (escolha das flags e montagem
  do ambiente puras) + **L**.
- **U-33** O serviço não espera o msiexec nem guarda seu handle: o lançador fecha os
  handles de processo e thread logo após criar o processo e devolve só o PID. Enquanto
  `fase = instalando`, o `Atualizador` relê o log `/l*v` a cada 60 s (`RELER_LOG`); se o
  msiexec terminar antes do stop (ex.: 1618), o código sai do log e aplica U-36..U-38;
  sem código em 2 h, conta falha `-1` (D-04). **T** (fachada).
- **U-34** Cada lançamento registra no log do serviço: de/para, SHA-256, caminho, PID,
  horário, flags. **T** (captura de `tracing`).
- **U-35** Serviço novo, ao iniciar com `fase = instalando`: determina o código pela
  ordem (1) linha `MainEngineThread is returning N` do log `/l*v` (lido com limite de
  8 MiB do fim, sem seguir reparse), (2) versão própria = `para` ⇒ sucesso presumido
  (código 0); registra `ultima_atualizacao` ou `ultima_falha`. **T** (logs de exemplo).
- **U-36** Códigos 0 e 3010 = sucesso (3010: registra “reinício pendente do Windows”,
  nunca reinicia); 1641 = sucesso com aviso no log (não deveria ocorrer com
  `/norestart`). **T**.
- **U-37** 1618 = adia 30 min sem contar falha; 1625 (política) = abandona a versão sem
  esperar 3 falhas e registra. **T**.
- **U-38** Demais códigos (1603, 1619, 1602, desconhecido) contam falha; na 3ª falha da
  mesma versão vai para `abandonadas` até surgir versão maior. **T**.
- **U-39** O upgrade preserva estado e ajustes: as ações de restauração do WiX já não
  rodam com `UPGRADINGPRODUCTCODE` (`instalador/nv-tuner.wxs`, `InstallExecuteSequence`)
  e nada nesta especificação as aciona. **I** + **L**.
- **U-39a** Jogo aberto **durante** o upgrade: o msiexec só troca arquivos e para/inicia
  o serviço e o agente; nada no pacote nem no atualizador escreve no DRS ou no registro de
  vídeo. O serviço novo, ao iniciar, só grava pelo caminho comum, que já consulta o portão
  imediatamente antes de cada gravação (L4). **I** + **L** (L-05b: abrir o processo
  vigiado no meio do upgrade; nenhuma gravação DRS no log até fechá-lo).

### 8.5 Agente e UI após o upgrade (`U-40..U-49`)

- **U-40** O agente responde a `WM_QUERYENDSESSION`/`WM_CLOSE` encerrando limpo
  (`crates/nvt-agente/src/windows/janela.rs:73-74`, já existente); o laboratório confirma
  que o RM o fecha sem 3010. **L**.
- **U-41** O agente que receber `versao_incompativel` do serviço (envelope ou erro)
  trata o serviço como **indisponível temporário**: não grava, não aplica, não toca no
  estado local e tenta de novo com recuo; isso não conta como desinstalação. Só sai
  sozinho com o serviço sem responder por 120 s **e** o executável ou a entrada `Run`
  ausentes de forma contínua por mais 120 s (o MSI remove e regrava os dois no upgrade;
  ausência momentânea não basta). O caminho normal no upgrade é o RM fechá-lo e U-43
  relançá-lo (D-05). **T**.
- **U-42** A UI, ao perder o pipe depois de já ter conectado, ou ao receber
  `versao_servico` diferente da própria `CARGO_PKG_VERSION`, mostra
  “O nv-tuner foi atualizado. Feche e abra de novo.” e para de reconectar; não se
  relança. **T** (`crates/nvt-ui/src/modelo.rs`).
- **U-43** O serviço novo, ao concluir U-35 com sucesso, relança o agente **uma vez** em
  cada sessão de `sessoes_agente` que esteja `WTSActive` (`WTSEnumerateSessionsW`) **e**
  cujo token (`WTSQueryUserToken`) tenha o mesmo SID gravado com a sessão antes do upgrade
  (`sessoes_agente` passa a guardar `[{sessao, sid}]`; id de sessão reaproveitado por
  outro usuário não recebe agente), sem agente já presente (mutex), e
  então apaga `sessoes_agente` (persistido antes de relançar, para não repetir após
  queda). **T** (fachada WTS).
- **U-44** Relançamento: `WTSQueryUserToken` → `CreateEnvironmentBlock(token, FALSE)` →
  `CreateProcessAsUserW(token, <INSTALLFOLDER>\nvt-agente.exe, linha "\"…\\nvt-agente.exe\"",
  bInheritHandles=FALSE, CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
  lpDesktop="winsta0\\default", lpCurrentDirectory=<INSTALLFOLDER>)`; fecha token,
  ambiente e handles. **T** (montagem) + **L**.
- **U-45** `<INSTALLFOLDER>` = diretório do executável do próprio serviço
  (`GetModuleFileNameW`), conferido como filho de `%ProgramFiles%` (`SHGetKnownFolderPath`)
  e sem reparse; fora disso não relança (instalação de desenvolvimento). **T**.
- **U-46** Falha de relançamento é registrada e não repetida; o agente volta no próximo
  logon pela entrada `Run` (mutex `Local\nv-tuner-agente` evita duplicata). **T**.
- **U-47** A UI mostra “Atualizado para a vX.Y.Z” uma vez por SID: banner na Visão geral
  com “OK”, que envia `MarcarAtualizacaoVista { versao }`; o serviço grava em
  `usuarios.json` para o SID do token. **T**.
- **U-48** A Visão geral mostra sempre uma linha de estado: “Versão X.Y.Z · última
  verificação há N min”, mais “atualização vX pendente: jogo aberto”, “aguardando outra
  instalação do Windows”, “falha ao instalar vX (código N); nova tentativa em …” ou
  “sem conexão com o GitHub; nova tentativa em …”. **T** (`textos.rs`).
- **U-49** Nenhum texto da UI oferece desligar, adiar manualmente ou voltar de versão. **I**.

### 8.6 IPC v6 (`U-50..U-54`)

- **U-50** `PROTOCOL_VERSION` = 6 (`crates/nvt-ipc/src/lib.rs:27`); v5 é recusada pelo
  envelope antes de desserializar o corpo. **T**.
- **U-51** `Estado.atualizacao: EstadoAtualizacao` (§10). **T** (ida e volta serde,
  `deny_unknown_fields`).
- **U-52** Nova requisição `MarcarAtualizacaoVista { versao: String }`, resposta `Ok`;
  identidade só do token; versão diferente da `ultima_atualizacao.para` → erro
  `entrada_invalida`. **T**.
- **U-53** `Estado.atualizacao.nao_vista` é calculado por SID do chamador. **T**.
- **U-54** `docs/ipc.md` documenta v6, campos e a requisição nova. **I**.

### 8.7 Release (`U-60..U-68`)

- **U-60** `crates/nvt-assinar` (binário, `publish = false`, nunca no MSI) com
  subcomandos (D-06):
  `gerar-chave --publica <arq.pub> --secreta <arq.key>` (sem senha, recusa sobrescrever;
  D-07), `assinar --versao X.Y.Z --msi <nv-tuner-X.Y.Z-x64.msi>` (gera, ao lado do MSI,
  `atualizacao.json`, `atualizacao.json.minisig` e `<msi>.minisig` com os comentários
  confiáveis de §6.2) e `verificar --dir <pasta>` (usa `nvt-atualizacao` com
  `CHAVE_OFICIAL`; confere manifesto, comentário, tamanho, SHA-256 e o `.minisig` do MSI;
  recusa primeiro se a chave embutida for a provisória). Crate `minisign = "=0.7.9"` e
  `--locked`. **T**.
- **U-61** A chave secreta vem só de `NV_TUNER_MINISIGN_KEY`: de preferência só a linha
  base64 do `.key` (Q11; a ferramenta remonta a caixa), aceitando também o `.key` de duas
  linhas; sem senha; nunca é gravada em disco pela ferramenta (só `gerar-chave` cria o
  `.key`); mensagens de erro não a ecoam. **T** + **I**.
- **U-62** Sem `NV_TUNER_MINISIGN_KEY`, o job falha antes de compilar com
  “Secret NV_TUNER_MINISIGN_KEY ausente: a release não é publicada sem assinatura (ver
  CONTRIBUTING.md)”. **I** (workflow) + teste manual numa tag de ensaio em fork.
- **U-63** O workflow cria a release em **rascunho**, sobe `nv-tuner-X.Y.Z-x64.msi`,
  `.msi.sha256`, `.msi.minisig`, `atualizacao.json`, `atualizacao.json.minisig`; baixa
  todos de volta (`gh release download`) para pasta vazia e roda `nvt-assinar verificar`.
  **I**.
- **U-64** Antes de publicar, espera o workflow `CI` do mesmo commit (`gh run list
  --workflow ci.yml --commit $GITHUB_SHA`, `gh run watch`) concluir com sucesso; sem
  execução de CI (commit fora da `main`) ou falha → mantém rascunho e falha o job. **I**.
- **U-65** Publica com `gh release edit vX.Y.Z --draft=false --latest --prerelease=false`. **I**.
- **U-66** A versão da tag é maior que a da release Latest atual (comparação `X.Y.Z`),
  senão falha antes de compilar (como o orquestrador, O§2). **I**.
- **U-67** `CONTRIBUTING.md` documenta geração da chave, gravação do secret, backup e
  rotação (§11.3). **I**.
- **U-68** O `ci.yml` compila e testa `nvt-assinar` e `nvt-atualizacao` como os demais
  (já coberto por `--workspace`). **I**.

---

## 9. Módulos e responsabilidades

### 9.1 Crate novo `crates/nvt-atualizacao` (puro, sem Windows, sem rede)

| Arquivo | Responsabilidade |
|---|---|
| `Cargo.toml` | deps: `serde`, `serde_json`, `minisign-verify = "=0.3.0"`, `sha2` (versão fixa); dev-dep `minisign` (vetores) |
| `src/lib.rs` | reexportações; `PRODUTO`, URLs fixas, hosts permitidos |
| `src/chave.rs` | `CHAVE_OFICIAL` (base64) e teste de validade |
| `src/versao.rs` | `Versao` (X.Y.Z, limites MSI), `Ord` |
| `src/manifesto.rs` | `Manifesto`, `verificar_manifesto(bytes, sig, chave) -> Result<Manifesto, ErroVerificacao>`, comentário confiável, nome do MSI, URL derivada |
| `src/politica.rs` | decisão de instalar (U-17..U-19), tabela de códigos MSI (U-36..U-38), agendamento/espera (U-01, U-02), `politica_url` (U-04) |
| `src/estado.rs` | `EstadoAtualizacaoPersistido` (§6.3), transições da §7 como função pura `transicao(estado, evento, agora) -> (estado, efeitos)` |
| `src/log_msi.rs` | extrair código de `MainEngineThread is returning N` |

### 9.2 `crates/nvt-service/src/atualizacao/`

| Arquivo | Responsabilidade |
|---|---|
| `mod.rs` | `Atualizador<R: Rede, D: Disco, L: Lancador, W: Sessoes>`: aplica efeitos de `nvt_atualizacao::estado`, integra portão, persistência, logs |
| `fachadas.rs` | traits `Rede`, `Disco`, `Lancador`, `Sessoes`, `Relogio` e implementações falsas `#[cfg(test)]` |
| `http.rs` | WinHTTP (`Win32_Networking_WinHttp`): sessão, proxy automático, redirecionamento manual, limites, timeouts, cancelamento |
| `arquivo.rs` | pasta protegida, `.parcial`, hash pelo handle, renomeação por handle, `ReOpenFile`, limpeza |
| `lancador.rs` | **único** ponto de `CreateProcessW` (msiexec) e `CreateProcessAsUserW` (agente), `IsProcessInJob`, `GetSystemDirectoryW`, mutex `_MSIExecute`, WTS e bloco de ambiente |
| `testes.rs` | testes do `Atualizador` com fachadas falsas |

Integração: `crates/nvt-service/src/laco.rs` (`ThreadAtualizacao`, que roda o
`Atualizador` e dorme até o prazo devolvido por `tick`; `Exclusao` com o ciclo DRS;
`portao_atualizacao`; sem `Evento::Atualizacao` nem handle do msiexec, D-04/D-08), `nucleo.rs`
(projeção `Estado.atualizacao`, `MarcarAtualizacaoVista`), `usuario.rs` (lista de
sessões com agente presente; `atualizacao_vista` por SID), `main.rs` (nenhum
subcomando novo; `status` mostra o estado da atualização via `relatorio.rs`),
`Cargo.toml` (`nvt-atualizacao`, features `Win32_Networking_WinHttp`,
`Win32_Security_Cryptography`, `Win32_System_Environment`, `Win32_UI_Shell` já existe).

### 9.3 `crates/nvt-ipc`

`src/lib.rs` (v6), `src/protocolo.rs` (`EstadoAtualizacao`, `MarcarAtualizacaoVista`).

### 9.4 `crates/nvt-agente`

`src/ciclo.rs`: sair limpo em `versao_incompativel` (U-41).

### 9.5 `crates/nvt-ui`

`src/servico.rs` (queda do pipe e versão divergente → evento “atualizado”),
`src/modelo.rs` (estado da atualização, banner uma vez), `src/textos.rs` (textos U-42,
U-47, U-48), `src/ui.rs` (`visao_geral`, linha de estado e banner).

### 9.6 `crates/nvt-core/tests/proibicoes.rs`

Acrescenta a `PROIBIDOS`: `CreateProcessW`, `CreateProcessA`, `CreateProcessAsUserW`,
`CreateProcessAsUserA`, `CreateProcessWithTokenW`, `CreateProcessWithLogonW`,
`ShellExecuteW`, `ShellExecuteA`, `ShellExecuteExW`, `ShellExecuteExA`, `WinExec`.
Nova constante:

```rust
/// Exceções exatas: (arquivo relativo à raiz, termo, ocorrências).
const PERMITIDOS: &[(&str, &str, usize)] = &[
    ("crates/nvt-service/src/atualizacao/lancador.rs", "CreateProcessW", 1),
    ("crates/nvt-service/src/atualizacao/lancador.rs", "CreateProcessAsUserW", 1),
];
```

O teste falha se o termo aparecer em outro arquivo, se a contagem no arquivo permitido
for diferente (mais **ou menos**: exceção sem uso também falha) ou se `lancador.rs` não
contiver a constante `MSIEXEC: &str = "msiexec.exe"` e a montagem por
`GetSystemDirectoryW`. Hoje nenhum crate usa esses termos (verificado em 2026-11-26).

### 9.7 Documentação

`docs/arquitetura.md` (“Segurança do serviço”: exceção da regra “nenhum processo
externo”, motivo, alternativa rejeitada `MsiInstallProduct`; “Distribuição”: atualização
automática), `docs/servico.md` (seção “Atualização”), `docs/ipc.md` (v6), `README.md`
(privacidade: contato com o GitHub a cada ~6 h; verificação manual com a `.pub`),
`CHANGELOG.md`, `CONTRIBUTING.md` (§11.3).

---

## 10. IPC v6

```rust
pub struct EstadoAtualizacao {
    pub versao_instalada: String,
    pub ultima_verificacao_unix: Option<u64>,
    pub proxima_verificacao_unix: Option<u64>,
    pub ultima_atualizacao: Option<AtualizacaoConcluida>, // { de, para, quando_unix, reinicio_pendente: bool }
    pub nao_vista: bool,                                  // para o SID do chamador
    pub pendente: Option<AtualizacaoPendente>,           // { versao, motivo: MotivoPendente, desde_unix }
    pub falha: Option<FalhaAtualizacao>,                 // { versao, codigo: i64, tentativas: u8, abandonada: bool, quando_unix }
    pub erro_verificacao: Option<ErroVerificacao>,       // sem_conexao | recusado_http(u16) | assinatura | manifesto | disco
}
pub enum MotivoPendente { JogoAberto, ListaIndisponivel, OutraInstalacao, Baixando, Instalando }
```

Requisição: `MarcarAtualizacaoVista { versao }`. `usuarios.json` ganha
`atualizacao_vista: Option<String>` por SID (`#[serde(default)]`).

---

## 11. Mudanças no WiX, no workflow e no CONTRIBUTING

### 11.1 `instalador/nv-tuner.wxs`

- Sem mudança obrigatória de sequência: `MajorUpgrade` (`afterInstallValidate`),
  `MSIRESTARTMANAGERCONTROL=0`, `MSIRMSHUTDOWN=0`, `ServiceControl Stop="both"` e as
  condições `NOT UPGRADINGPRODUCTCODE` já atendem (P§2).
- Comentário novo documentando que o upgrade automático roda `/qn /norestart` como
  SYSTEM e que a pasta `atualizacao\` é criada pelo serviço com DACL própria.
- `AllowSameVersionUpgrades` continua `no` (o cliente nunca tenta versão igual, U-17).
- `ARPURLINFOABOUT` = URL do repositório (agora público).

### 11.2 `.github/workflows/release.yml`

Ordem nova: (1) conferir tag × `Cargo.toml`; (2) **conferir secret** (U-62); (3) conferir
versão > Latest (U-66); (4) compilar `nvt-service nvt-ui nvt-agente nvt-assinar`; (5)
MSI; (6) SHA-256; (7) `nvt-assinar assinar --versao X.Y.Z --msi <msi>` (manifesto e as
duas assinaturas; antes, (4b) `nvt-assinar verificar` numa pasta vazia falha o job se a
chave embutida for a provisória); (8) notas;
(9) `gh release create --draft` com os cinco arquivos; (10) `gh release download` para
pasta nova + `nvt-assinar verificar`; (11) esperar CI verde (U-64; `permissions:
actions: read`); (12) `gh release edit --draft=false --latest`. O secret entra só nos
passos 2 e 7, via `env:`. `concurrency` mantido.

Endurecimento obrigatório: `NV_TUNER_MINISIGN_KEY` é secret de **Environment** `release`
com regra de implantação só para tags `v*` (sem aprovação manual, então não contraria
“sem ninguém fazer nada”); sem isso qualquer workflow alterado num branch com permissão
de push leria o secret. O job de release não usa `Swatinem/rust-cache` (cache
compartilhado pode ser envenenado por outro ref e entraria no binário assinado). Passo
(1b): falha se a feature `laboratorio` estiver ativa (U-15a).

### 11.3 `CONTRIBUTING.md` (“Chave de assinatura” e “Como lançar”)

Comandos exatos (implementados com `nvt-assinar`, D-06/D-07; documentados em
`CONTRIBUTING.md`, “Chave de assinatura”):

```powershell
# 1. Gerar (uma vez, máquina confiável, offline de preferência), sem senha:
cargo run -p nvt-assinar -- gerar-chave --publica C:\chave\nv-tuner-minisign.pub --secreta C:\chave\nv-tuner-minisign.key
# 2. Gravar só a linha base64 (Q11) no secret do Environment release, sem eco:
(Get-Content C:\chave\nv-tuner-minisign.key)[1] | gh secret set NV_TUNER_MINISIGN_KEY --env release --repo luansilvadb/nv-tuner
# 3. Copiar o .pub para instalador/nv-tuner-minisign.pub (substitui o marcador
#    provisório; include_str! leva a chave aos binários); commit.
# 4. Backup: guardar nv-tuner-minisign.key em dois meios offline (ex.: pendrive cifrado
#    + cofre de senhas); apagar a cópia da máquina de trabalho.
```

Também: criar o Environment `release` com regra só para tags `v*`, ruleset de tags
`v*` restrito ao mantenedor e proteção da `main` (Q10); como apagar um rascunho que
falhou antes de reexecutar.

E o aviso: sem a chave, nenhuma máquina instalada aceita atualizações; perda da chave
exige reinstalação manual em todas as máquinas; procedimento de rotação (§5.5).
Verificação manual por usuários: `minisign -Vm nv-tuner-X.Y.Z-x64.msi -P <chave>`.

---

## 12. Plano de lotes

Lotes sequenciais; tarefas de um mesmo lote rodam em paralelo e escrevem em **arquivos
disjuntos**. Pronto = critério testável + `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings` e `cargo test --workspace` verdes.

### Lote 0 — chave (mantenedor, fora de código)

| Tarefa | Escreve | Pronto quando |
|---|---|---|
| 0.1 Gerar chave, gravar secret, backup (§11.3) — **adiada** (D-01): o lote 1 seguiu com o marcador provisório em `instalador/nv-tuner-minisign.pub`; agora é pré-requisito do lote 6 | `instalador/nv-tuner-minisign.pub` | `gh secret list --env release` mostra `NV_TUNER_MINISIGN_KEY`; `.pub` oficial commitado; backup confirmado pelo mantenedor |

### Lote 1 — núcleo puro e contratos (depende de 0)

| Tarefa | Escreve | Pronto quando |
|---|---|---|
| 1.1 Crate `nvt-atualizacao` | `crates/nvt-atualizacao/**` | testes de U-10..U-19, U-01, U-02, U-04 (política), U-36..U-38, §7 (transições) e `log_msi` passam; chave oficial válida |
| 1.2 IPC v6 | `crates/nvt-ipc/src/lib.rs`, `crates/nvt-ipc/src/protocolo.rs`, `docs/ipc.md`, `crates/nvt-service/src/{laco,nucleo,usuario_testes}.rs` (só o literal de `Estado`) | U-50..U-52 (serde ida e volta, v5 recusada) |
| 1.3 Proibições | `crates/nvt-core/tests/proibicoes.rs` | termos e `PERMITIDOS` (§9.6); teste unitário do próprio detector cobre contagem a mais, a menos e arquivo errado; enquanto `lancador.rs` não existe, a lista fica com contagem 0 marcada `// lote 2` e sobe para 1 no lote 2 |

Observação: o campo novo de `Estado` quebraria os literais existentes em
`crates/nvt-service/src/laco.rs:754`, `nucleo.rs:1136` e `usuario_testes.rs:148`. A
tarefa 1.2 também escreve nesses três arquivos, só para preencher
`atualizacao: EstadoAtualizacao::default()`; nenhuma outra tarefa do lote 1 toca
`crates/nvt-service`, e a projeção real entra na tarefa 3.2.

### Lote 2 — Windows: rede, arquivo e lançador (depende de 1)

| Tarefa | Escreve | Pronto quando |
|---|---|---|
| 2.1 HTTP WinHTTP | `crates/nvt-service/src/atualizacao/http.rs` | U-05..U-08 em testes sem rede (montagem de cabeçalhos, limites sobre leitor falso, política de redirecionamento); teste `#[ignore]` manual contra o GitHub |
| 2.2 Arquivo protegido | `crates/nvt-service/src/atualizacao/arquivo.rs`, `crates/nvt-service/src/diretorio.rs` (só função nova de DACL da subpasta) | U-20..U-25 em pasta temporária (reparse recusado, escrita de terceiro negada pelo handle, `ReOpenFile`, limpeza) |
| 2.3 Lançador | `crates/nvt-service/src/atualizacao/lancador.rs`, `crates/nvt-core/tests/proibicoes.rs` (contagens 0→1) | U-29..U-32, U-44, U-45 por funções puras de montagem; nenhum teste chama `CreateProcess*`; `proibicoes` verde com contagem 1 |
| 2.4 Fachadas | `crates/nvt-service/src/atualizacao/fachadas.rs`, `crates/nvt-service/src/atualizacao/mod.rs` (esqueleto com `mod` e traits), `crates/nvt-service/Cargo.toml` | compila com as três tarefas acima; falsas de rede/disco/lançador/WTS/relógio disponíveis |

2.1–2.3 dependem só das assinaturas de traits; 2.4 publica as traits primeiro (meio
lote) e as demais implementam.

### Lote 3 — integração no serviço (depende de 2)

| Tarefa | Escreve | Pronto quando |
|---|---|---|
| 3.1 Atualizador | `crates/nvt-service/src/atualizacao/mod.rs`, `crates/nvt-service/src/atualizacao/testes.rs` | fluxo completo com fachadas falsas: U-09, U-14, U-17, U-26..U-28, U-33..U-38, U-43, U-46 |
| 3.2 Laço e núcleo | `crates/nvt-service/src/laco.rs`, `crates/nvt-service/src/nucleo.rs`, `crates/nvt-service/src/usuario.rs`, `crates/nvt-service/src/relatorio.rs`, `crates/nvt-service/src/testes.rs`, `crates/nvt-service/src/usuario_testes.rs` | `Evento::Atualizacao`; projeção U-51/U-53; `MarcarAtualizacaoVista` U-52 grava por SID; `status` mostra o estado; nenhuma instalação inicia durante ciclo DRS (teste de ordem) |

3.1 expõe `Atualizador::{novo, tick, ao_evento, estado_ipc}` combinado no início do lote.

### Lote 4 — agente e UI (depende de 1.2; paralelo ao lote 3)

| Tarefa | Escreve | Pronto quando |
|---|---|---|
| 4.1 Agente | `crates/nvt-agente/src/ciclo.rs`, `crates/nvt-agente/src/ciclo_testes.rs` | U-41 |
| 4.2 UI | `crates/nvt-ui/src/servico.rs`, `crates/nvt-ui/src/modelo.rs`, `crates/nvt-ui/src/textos.rs`, `crates/nvt-ui/src/ui.rs` | U-42, U-47, U-48 em testes de modelo/textos |

### Lote 5 — release e documentação (depende de 1.1; paralelo aos lotes 2–4; 5.2 depende de 5.1, pois o ensaio roda `nvt-assinar`)

| Tarefa | Escreve | Pronto quando |
|---|---|---|
| 5.1 `nvt-assinar` | `crates/nvt-assinar/**` | U-60, U-61: teste gera chave temporária em memória, assina, verifica (com chave injetada no teste) e recusa adulteração |
| 5.2 Workflow | `.github/workflows/release.yml` | §11.2; `actionlint` limpo; ensaio em fork com tag `v0.0.99`: sem secret falha (U-62), com secret fica em rascunho até CI verde e publica |
| 5.3 WiX | `instalador/nv-tuner.wxs`, `instalador/nv-tuner-minisign.pub` | §11.1; `build-msi.ps1` gera MSI com ICE limpo |
| 5.4 Documentação | `docs/arquitetura.md`, `docs/servico.md`, `docs/ipc.md`, `docs/ui.md`, `README.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, esta spec (§4.1) e `docs/spec/atualizacao-laboratorio.md` (roteiro, sem resultados) | U-54 (se 1.2 já não cobriu), U-67, §9.7 |

### Lote 6 — laboratório e primeiro lançamento (depende de tudo)

| Tarefa | Escreve | Pronto quando |
|---|---|---|
| 6.1 Roteiro e execução | `docs/spec/atualizacao-laboratorio.md` | L-01..L-10 (§13) aprovados e registrados |
| 6.2 Lançamento vN (bootstrap, A10) | `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md` | release Latest com os cinco arquivos; instalada à mão nas máquinas v0.0.2 |
| 6.3 Prova ponta a ponta | — | publicar vN+1 e ver a máquina com vN subir sozinha (L-01 em produção) |

---

## 13. Plano de laboratório (VM descartável, Windows 11, snapshot antes de cada caso)

Servidor de teste: build de laboratório com `URL_BASE` e `CHAVE` trocadas **só** por
feature de compilação `laboratorio` (desligada em release; teste U-15 garante), servindo
de um repositório de ensaio no GitHub (fork) para exercitar redirecionamentos reais.
Hoje o crate `nvt-atualizacao` define `laboratorio::{URL_BASE_ENSAIO, CHAVE_ENSAIO}`
(de `NVT_ATUALIZACAO_URL_ENSAIO`/`NVT_ATUALIZACAO_CHAVE_ENSAIO` em compilação) e
`Chave::ensaio()`, mas o serviço ainda usa só `URL_MANIFESTO` e `Chave::oficial()` e não
repassa a feature: a ligação é pendência de código antes do laboratório. Roteiro
detalhado, com campos de resultado: [`atualizacao-laboratorio.md`](atualizacao-laboratorio.md).

| # | Caso | Esperado |
|---|---|---|
| L-01 | Upgrade vN → vN+1 sem UI/jogo | instala em ≤ 15 min + download; `ultima_atualizacao` gravada; estado DRS/Windows preservado (U-39) |
| L-02 | **msiexec sobrevive ao stop do serviço** e o handle aberto (U-22) não impede a abertura do pacote | instalação conclui; log `/l*v` com `MainEngineThread is returning 0`; se falhar, adota a alternativa de U-22 e repete |
| L-03 | Agente e UI abertos durante o upgrade | RM fecha ambos sem 3010; UI mostra “reabra” se ainda viva; agente relançado (U-43) uma vez por sessão; console + RDP simultâneos |
| L-04 | Serviço em job (`IsProcessInJob`) | registro do caso; breakaway tentado conforme U-32 |
| L-05 | Jogo vigiado aberto (ex.: `cs2.exe` falso na lista) | adia; instala ≤ 60 s após fechar |
| L-05b | Jogo vigiado aberto no meio do upgrade | upgrade conclui; nenhuma gravação DRS até fechar o jogo (U-39a) |
| L-06 | Sem internet / proxy WinHTTP configurado / DNS falho | nada bloqueia; espera 30 min → 60 min…; com proxy, baixa |
| L-07 | Assinatura inválida, de outra chave, legada, comentário trocado, MSI com 1 byte alterado | nada instalado; motivo visível |
| L-08 | Manifesto com versão igual e menor | recusado (U-17) |
| L-09 | Outra instalação MSI em andamento | adia sem contar falha (1618/mutex) |
| L-10 | MSI que falha (custom action forçada a 1603) 3 vezes | abandona a versão; publica maior → volta a tentar |
| L-11 | Serviço novo que não inicia (Q12) | `ServiceControl Wait="yes"` falha o MSI, rollback religa o antigo, que lê o log e conta a falha |

---

## 14. Questões em aberto

1. **Q1 — abertura do pacote pelo Windows Installer com nosso handle de leitura**
   (U-22): modo de compartilhamento não documentado; L-02 decide entre handle mantido e
   rehash antes do lançamento.
2. **Q2 — sobrevivência do msiexec ao stop do serviço** e existência de job do SCM: não
   documentadas (P§1); L-02/L-04.
3. **Q3 — `releases/latest/download` fora da cota da API e hosts finais dos assets**: não
   documentados (P§5); se o GitHub mudar o host de download, a lista de U-04 precisa de
   release nova (máquinas ficariam presas: considerar aceitar `*.githubusercontent.com`
   com TLS válido?).
4. **Q4 — chave sem senha (`-W`) protegida só pelo secret**: aceita por A9; avaliar
   Environment `release` com aprovação manual (P§4), que contraria “sem ninguém fazer
   nada” no lançamento.
5. **Q5 — CI do commit da tag**: o `ci.yml` só roda em `push` na `main` e em PR; tag
   feita em commit que não passou pela `main` nunca publica (U-64). Aceitar ou acrescentar
   `push: tags` ao CI?
6. **Q6 — 3010 com arquivo em uso**: se o agente não fechar, a troca fica para o reboot
   e o serviço novo pode rodar com binários mistos até reiniciar; tratar como sucesso
   (A6) ou como “pendente de reinício” visível na UI (proposto: `reinicio_pendente`).
7. **Q7 — Authenticode ausente**: SmartScreen não age em `msiexec` por SYSTEM, mas
   políticas corporativas (AppLocker/WDAC para MSI) podem bloquear (1625): só registro.
8. **Q8 — `prepare-state-dir` com a subpasta `atualizacao\`**: `diretorio::verificar`
   aceita DACL mais restrita que a raiz e o limite `Problema::Demais` não deve ser
   atingido por MSIs antigos (limpeza U-25); confirmar no lote 2.2.
9. **Q9 — `maior_vista` versus release removida/retirada**: se o mantenedor publicar vX
   por engano e apagar, as máquinas que a viram recusam tudo abaixo de vX; a correção é
   publicar versão maior (coerente com “só sobe de versão”).
10. **Q10 — commit/tag malicioso**: quem tem push cria tag e o workflow assina o que
    estiver no commit; a assinatura não distingue. Proposto: ruleset de tags `v*` só para
    o mantenedor e branch protection na `main`; ações do workflow fixadas por SHA
    (hoje por tag móvel, `@v7`, `@stable`, `@v2`).
11. **Q11 — mascaramento do secret de várias linhas** (resolvida, D-06): o `.key` tem
    duas linhas; o GitHub mascara por linha e a primeira (“untrusted comment: …”) é
    texto comum. O secret guarda só a linha base64 e o `nvt-assinar` remonta a caixa.
12. **Q12 — serviço novo que não inicia**: `ServiceControl Wait="yes"` faz o MSI falhar e
    reverter (rollback religa o antigo, que conta a falha via log); se iniciar e cair
    depois, a recuperação do SCM (reinicia 2×, depois nada) deixa a máquina sem serviço e
    sem atualizador até reinstalação manual. Decidido (D-09): `ThirdFailureActionType`
    também `restart`, com `ResetPeriodInDays="1"`; L-11 do laboratório confirma o
    rollback.
13. **Q13 — Q3**: recomenda-se **não** aceitar `*.githubusercontent.com`; a lista exata
    é defesa em profundidade e mudança do GitHub vira falha de rede visível, não risco.
