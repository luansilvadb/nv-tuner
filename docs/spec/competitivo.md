# Especificação — fase “desempenho competitivo”

Data: 2026-11-26. Fonte de verdade do escopo: [`docs/intent/competitivo.md`](../intent/competitivo.md).
Base técnica: [`docs/arquitetura.md`](../arquitetura.md), [`docs/regras.md`](../regras.md) e os
relatórios em `docs/pesquisa/competitivo-*.md`. Referências a código no formato
`arquivo:linha`, conforme a auditoria em
[`competitivo-estado-atual.md`](../pesquisa/competitivo-estado-atual.md).

Convenções deste documento:

- Requisitos numerados `T-nn` (transversais), `F1-nn`, `F2-nn` e `F3-nn` (por fase). Cada
  um traz a forma de **verificação** (teste automatizado, inspeção ou laboratório).
- **Laboratório** = a máquina do mantenedor: RTX 2060 SUPER (Turing), driver 621.40,
  1080p 240 Hz, PresentMon 2.6.0 fora do app, mediana de 3×60 s, como em
  `regras/base.toml:3-5`.
- Onde um relatório e a sua revisão divergem, vale a revisão, salvo prova em contrário no
  código ou na fonte. Cada uma dessas decisões está marcada com **Decisão (revisão)**.

---

## 1. Objetivo

Deixar jogos competitivos com sensação mais fluida e responsiva (frametime estável,
latência baixa, FPS real) em qualquer GPU NVIDIA, atuando por camadas seguras —
**driver**, **Windows** e **configuração do jogo** —, com toda alteração registrada,
explicada e reversível, como já acontece hoje com o perfil DRS.

O sucesso é a sensação percebida pelo jogador. Não há medição dentro do app. Cada regra
nova, porém, precisa de **evidência registrada** (fonte ou medição de laboratório) e de
**status** (`estavel`/`experimental`), como as regras atuais (`docs/regras.md`,
“Como contribuir com uma regra”).

A entrega é feita em três fases, lançadas em ordem; cada uma é lançável sem a seguinte
(a 3 depende da 2, a 2 depende da 1):

| Fase | Tema | Camadas novas | Lançável porque… |
|---|---|---|---|
| 1 | Driver + fatos | nenhuma (só o driver), modelo generalizado | Corrige o que já existe e acrescenta fatos e orientações; não depende das fases 2 e 3 |
| 2 | Windows | `windows` (sistema e usuário) | Usa o modelo de camadas e o agente do usuário introduzidos aqui; não depende da fase 3 |
| 3 | Configuração de jogo | `jogo` (arquivos do usuário e opções de inicialização da Steam) | Reaproveita o agente e o snapshot por camada da fase 2 |

## 2. Não-objetivos

Fora do escopo desta fase, em qualquer camada:

1. **Frame generation** em qualquer forma: DLSS FG/MFG, Smooth Motion, FSR 3 via
   OptiScaler, Lossless Scaling. Motivo: acrescenta latência; o público é competitivo.
2. **Jogos single-player/casuais como foco.** Eles continuam recebendo a base global, mas
   nenhuma regra nova é pensada para eles.
3. **Benchmark ou medição dentro do app.** A telemetria (`nvt-telemetry`) continua fora
   desta fase; a validação é feita no laboratório, com PresentMon externo.
4. **Overclock e undervolt.**
5. **Ajustes que mudam o que o jogador enxerga para obter vantagem**: vibrância digital
   (NVAPI DVC), filtros de nitidez e cor, remoção de fumaça, folhagem, névoa ou sombras,
   LOD extremo, FOV fora do menu, resolução esticada e o escalonamento “Perform scaling
   on” usado para ela.
6. **ULMB/ULMB 2 e strobing de monitor.** São ajustes do monitor, incompatíveis com
   G-SYNC; ficam registrados como fora do escopo.
7. **IFEO** (`Image File Execution Options`), inclusive `PerfOptions`
   (`CpuPriorityClass`, `IoPriority`). **Decisão (revisão)**: a revisão Windows/anticheat
   (§5) reclassifica o item 14 de `competitivo-windows.md` como fora do escopo pela
   política de anticheat.
8. **Prioridade, afinidade ou qualquer ação sobre o processo do jogo** (exige handle).
9. **Edição de arquivos de configuração** de Valorant, League of Legends, Marvel Rivals e
   Call of Duty. Apex Legends, Overwatch 2, PUBG e The Finals recebem **só orientação**,
   sem escrita (nem de arquivo nem de opção de inicialização).
10. **Recursos de segurança** (TPM, Secure Boot, VBS/HVCI, IOMMU/Kernel DMA Protection,
    Vulnerable Driver Blocklist, DEP, Defender, Windows Update): só diagnóstico, nunca
    alteração, em nenhuma direção.
11. **Mitos e itens prejudiciais** de `competitivo-windows.md` (timer resolution,
    `GlobalTimerResolutionRequests`, `disabledynamictick`, `useplatformtick`, forçar HPET,
    MSI mode/afinidade de interrupção, Nagle, `NetworkThrottlingIndex`, desligar serviços,
    `IDLEDISABLE`) e de `competitivo-driver.md` (Maximum Pre-Rendered Frames legado,
    Threaded Optimization forçado, filtragem “High performance”, LOD negativo, Fast Sync,
    Adaptive V-Sync, settings “Experimental - …”, Platform Boost).
12. **ReBAR forçado por perfil** (`0x000F00BA`): Turing não tem suporte oficial e não há
    medição por jogo; fica para uma fase com laboratório Ampere ou mais nova.
13. **Reflex 2/Frame Warp**: não lançado (`competitivo-driver.md` §2.1).

---

## 3. Regras de segurança anticheat

Restrição suprema: **zero risco** de banimento **e** de bloqueio (jogo que não abre,
partida derrubada). Política: *sem fonte oficial favorável, o ajuste não entra*
(`competitivo-anticheat.md` §4, regra 18). Os números entre colchetes remetem às regras
de `competitivo-anticheat.md` §4; as correções da revisão estão indicadas.

### 3.1 Nunca faça

| # | Regra | Origem |
|---|---|---|
| N1 | Nunca abrir handle para processo de jogo ou de anticheat, **nem** com `PROCESS_QUERY_LIMITED_INFORMATION`. Ficam proibidos por consequência `OpenProcess`, `QueryFullProcessImageNameW`, `GetPriorityClass`, `GetProcessTimes`, `SetPriorityClass`, `SetProcessAffinityMask`, `EnumProcessModules` e `ReadProcessMemory` sobre qualquer processo que não seja do próprio nv-tuner | [1]; revisão §8 (justificativa reescrita: “não há necessidade funcional nem fonte oficial”) |
| N2 | Nunca usar `CreateToolhelp32Snapshot` com `TH32CS_SNAPMODULE`, `TH32CS_SNAPMODULE32` ou `TH32CS_SNAPHEAPLIST`. Só `TH32CS_SNAPPROCESS` | revisão §7 |
| N3 | Nunca injetar DLL, instalar hook (`SetWindowsHookEx`, CBT, `AppInit_DLLs`), desenhar overlay ou colocar DLL proxy na pasta do jogo | [2] |
| N4 | Nunca empacotar, instalar ou carregar driver de kernel (WinRing0, inpout, PawnIO, `.sys` próprio). Sensores só por NVAPI/NVML/WMI | [3] |
| N5 | Nunca escrever em IFEO (`PerfOptions`, `Debugger`, `GlobalFlag`, `VerifierDlls`) | [4]; revisão §5 |
| N6 | Nunca alterar Secure Boot, TPM, VBS, HVCI, IOMMU, Vulnerable Driver Blocklist, DEP, Defender, Windows Update ou mitigações de CPU | [5], [17] |
| N7 | Nunca alterar no BCD `testsigning`, `nointegritychecks`, `debug`, `hypervisorlaunchtype`, `nx`. O único elemento do BCD que o app pode tocar é `useplatformclock` do objeto `{current}`: **remover** (F2-08) e, só na reversão, **regravar exatamente o valor do snapshot**; as duas operações têm as mesmas travas (BitLocker, confirmação, reinício) | [6] |
| N8 | Nunca alterar executáveis, DLLs ou qualquer arquivo dentro da pasta de instalação do jogo | [7] |
| N9 | Nunca aplicar configuração que altere visibilidade ou informação (ver §2, item 5) | [8] |
| N10 | Nunca recomendar nem adicionar `-allow_third_party_software` ou opção que desligue proteção do jogo | [9] |
| N11 | Nunca emular ou remapear entrada, nem automatizar ações no jogo | [10] |
| N12 | Nunca imitar Process Lasso, Special K, RTSS, ISLC ou TimerResolution | [11] |
| N13 | Nunca marcar arquivo de configuração como somente leitura para “travar” um valor | [13]; `competitivo-jogos.md` §16 |
| N14 | Nunca lançar processo auxiliar (inclusive `powercfg.exe`, `bcdedit.exe`, `reg.exe`) — com ou sem jogo aberto. O CS2 Trusted Mode pode desconectar a partida ao iniciar software incompatível | revisão §7; §6.3 deste documento |
| N15 | Nunca gravar valor que o menu do próprio jogo não ofereça | [13] |

### 3.2 Faça

| # | Regra | Origem |
|---|---|---|
| S1 | Toda gravação (DRS, Windows, arquivo de jogo, opções de inicialização), **inclusive a reaplicação automática por deriva e o “Restaurar original”**, só acontece com **nenhum jogo detectado nem processo de anticheat em execução**. A detecção é por nome, via `TH32CS_SNAPPROCESS` | [12]; revisão §7 |
| S2 | A justificativa de S1 é de **produto**, não de anticheat: evitar reset de vídeo, estado inconsistente, gravação sem efeito (o DRS é lido na criação do processo) e o jogo regravar o próprio arquivo ao fechar. A UI e a documentação não devem atribuir S1 a uma exigência dos anticheats | **Decisão (revisão)** §7 |
| S3 | Ler arquivos dos jogos com compartilhamento total (`FILE_SHARE_READ \| FILE_SHARE_WRITE \| FILE_SHARE_DELETE`), handle curto e leitura para buffer, sem manter mapeamento | [15]; revisão driver §8 |
| S4 | Editar só arquivos do usuário, com jogo (e, se for o caso, Steam) fechado, com backup e restauração | [13] |
| S5 | No driver, gravar só settings expostas pelo Painel NVIDIA ou neutras para a jogabilidade (latência, sincronismo, energia, cache) | [14] |
| S6 | Diagnóstico de requisitos de segurança só informa; links para as páginas oficiais | [17] |
| S7 | Binários assinados e instalados em `Program Files` (pendente: hoje o MSI sai sem assinatura, `docs/arquitetura.md`, “Distribuição”) | [16] |
| S8 | Reabrir as regras desta seção a cada categoria nova de ajuste e, antes de cada versão, reconferir as páginas de Riot, FACEIT e Valve (a pesquisa só obteve trechos indexados) | [18]; revisão, “Método e limite” |

Falso positivo da detecção por nome (outro `cs2.exe`) só adia gravações; é aceito. Não
“melhorar” a identificação abrindo handle (revisão §7).

---

## 4. Promessa pública (nova redação)

Substitui “Atua apenas na camada do driver: não modifica arquivos dos jogos nem injeta
código em processos” (`README.md:91-92`) e a seção “Escopo” de `docs/arquitetura.md:7-14`.
A atualização desses arquivos é feita na implementação da fase 1 (para o driver) e
estendida nas fases 2 e 3; esta especificação só define o texto.

**README (lista “O que é”):**

> - Ajusta, por mecanismos oficiais, três camadas: o **driver NVIDIA** (perfis DRS via
>   NVAPI), o **Windows** (configurações documentadas, por APIs do sistema) e, em jogos
>   selecionados, os **arquivos de configuração do usuário** e as opções de inicialização
>   da Steam — sempre com valores que o próprio jogo oferece no menu.
> - Toda alteração é registrada, explicada e pode ser desfeita com **Restaurar original**.
> - Nunca injeta código, nunca abre processos de jogos ou de anticheats, nunca altera
>   executáveis nem a pasta de instalação dos jogos, nunca mexe em recursos de segurança
>   (Secure Boot, TPM, VBS/HVCI) e adia qualquer gravação enquanto detecta um jogo
>   aberto.

A redação “adia … enquanto detecta” é proposital: a detecção é por nome (S1, R2) e pode
ter falso negativo; a promessa não pode ser absoluta.

Enquanto só a fase 1 estiver lançada, o primeiro item fica restrito ao driver (“Ajusta o
driver NVIDIA…; ajustes do Windows e de jogos chegam nas próximas versões”), para a
promessa nunca anunciar o que ainda não existe.

**`docs/arquitetura.md`, seção “Escopo”:**

> - Somente Windows e GPUs NVIDIA.
> - Atua em **camadas de ajuste**: `driver` (perfis DRS via NVAPI, FFI própria),
>   `windows` (registro, parâmetros do sistema, modo de vídeo e BCD, só itens da lista
>   permitida) e `jogo` (arquivos de configuração do usuário e opções de inicialização da
>   Steam, só jogos e chaves da lista permitida).
> - Cada camada tem snapshot do original, releitura após gravar, histórico e restauração.
> - Não injeta código, não abre handle para processos de jogos ou de anticheats, não
>   altera executáveis nem arquivos da pasta de instalação dos jogos e não altera
>   recursos de segurança do sistema.

---

## 5. Requisitos transversais

| # | Requisito | Verificação |
|---|---|---|
| T-01 | Nenhum crate chama as APIs listadas em N1 e N2 sobre processo alheio. Um teste de lint de código (`crates/nvt-core/tests/` ou um `xtask`) procura, como **palavra inteira**, `OpenProcess`, `TH32CS_SNAPMODULE`, `ReadProcessMemory`, `SetWindowsHookEx`, `SetPriorityClass`, `Image File Execution Options` e `Command::new`, e falha se encontrar em código de produção. Fora da busca: `tests/`, `examples/`, `build.rs` e módulos sob `#[cfg(test)]` (hoje `Command::new` aparece só nos testes de `crates/nvt-service/src/{diretorio,logs}.rs`). `OpenProcessToken` sobre `GetCurrentProcess()` (`crates/nvt-service/src/diretorio.rs:677`) é permitido e não pode casar com o padrão | teste automatizado |
| T-02 | Toda gravação passa por um único portão, `Engine::pode_gravar() -> Result<(), Adiamento>`, que consulta a lista de processos (F1-06) imediatamente antes de gravar | teste com `ProcessosFalsos` |
| T-03 | Toda regra nova tem `motivo`, `evidencia` (fonte ou medição, sem dados pessoais) e entra como `experimental`; só vira `estavel` com o critério de laboratório da fase | revisão de PR + `carrega_regras_reais` |
| T-04 | “Estabilidade > latência > FPS” ganha um eixo explícito na regra, além de `impacto_visual`: campo opcional `requer_reinicio` (bool) e `confirmacao` (`automatica` \| `usuario`). Regras com `confirmacao = "usuario"` nunca entram no plano automático: aparecem como ação pendente na UI | teste do avaliador |
| T-05 | A explicação “o que o app fez” (`plan.resumos`, `ResumoJogo`) cobre todas as camadas: para cada jogo, as mudanças no perfil do driver, as do arquivo de config e as orientações; para o sistema, um `ResumoSistema` com as mudanças de Windows | teste do avaliador + UI |
| T-06 | O núcleo continua sem gerar texto: motivos, orientações e diagnósticos são códigos; a UI traduz em `crates/nvt-ui/src/textos.rs` | inspeção |
| T-07 | `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` e `cargo fmt --check` passam em cada fase | CI |

---

## 6. Arquitetura

### 6.1 Camadas de ajuste (modelo generalizado)

Hoje tudo é `ProfileRef` + `id: u32` do DRS (`estado-atual` §4). O modelo passa a ser:

```rust
// nvt-core
pub enum Camada { Driver, Windows, Jogo }

pub enum Alvo {                       // substitui o Alvo atual (Global | Jogo)
    PerfilGlobal,                     // perfil base do DRS
    PerfilJogo(String),               // perfil DRS do jogo (nome do perfil)
    Sistema,                          // máquina: HKLM, BCD
    Usuario,                          // usuário interativo: HKCU, SPI, modo de vídeo
    ConfigJogo { jogo: IdJogo, arquivo: IdArquivo }, // arquivo do usuário ou LaunchOptions
}

pub enum Destino {                    // vem do catálogo; a regra nunca o escreve
    Drs { id: u32 },
    Registro { raiz: Raiz /* Hklm | Hkcu */, chave: String, valor: String, tipo: TipoReg },
    TokenRegistro { chave: String, valor: String, token: String }, // "k=v;" em REG_SZ
    Spi(Spi),                         // Spi::Mouse (trio MouseSpeed/Threshold1/2)
    ModoVideo(AtributoVideo),         // AtributoVideo::RefreshMaximoNativo
    Bcd { elemento: u32 },            // só BcdOSLoaderBoolean_UsePlatformClock
    Arquivo { formato: Formato /* Ini | KeyValues */, secao: String, chave: String },
    OpcaoInicializacao { appid: u32, token: String }, // ex.: "+fps_max"
}

pub enum Valor { Dword(u32), Inteiro(i64), Decimal(String), Texto(String), Ausente }
```

- `SettingChange` passa a ter `camada`, `alvo`, `destino` e `valor: Valor` em vez de
  `id`/`valor: u32` (`crates/nvt-core/src/plan.rs:38-51`). `Valor::Ausente` é o que permite
  restaurar “o valor não existia” no registro e em arquivo. `Decimal` guarda o texto
  exato do arquivo (ex.: `240.000000` do Unreal) para não perder formatação.
- O catálogo deixa de ser um arquivo só: `regras/settings.toml` (driver, formato atual),
  `regras/windows.toml` (fase 2) e `regras/jogos.toml` (base curada, fase 1, e chaves de
  arquivo, fase 3). Todos com `versao_schema = 2`; `settings.toml` aceita o formato 1 sem
  mudança (camada implícita `driver`). Os três continuam embutidos
  (`ConjuntoRegras::embutido()`) e formam a **lista permitida** de cada camada.
- `escopo` da regra ganha `sistema`, `usuario` e `config_jogo`, além de `global` e
  `por_jogo`. Uma regra só pode gravar settings da camada compatível com o escopo
  (erro de carga `ErroRegra::EscopoIncompativel`).
- `nvt-engine`: o trait `DriverStore` (`crates/nvt-engine/src/store.rs:24-59`) vira uma
  implementação de um trait por camada:

  ```rust
  pub trait ArmazemCamada {
      fn camada(&self) -> Camada;
      fn ler(&self, alvo: &Alvo, d: &Destino) -> Result<Valor, ErroCamada>;
      fn gravar(&mut self, alvo: &Alvo, d: &Destino, v: &Valor) -> Result<(), ErroCamada>;
      fn capturar_original(&mut self, alvo: &Alvo, d: &Destino) -> Result<Valor, ErroCamada>;
      fn confirmar(&mut self) -> Result<(), ErroCamada>; // ex.: DRS_SaveSettings
  }
  ```

  O `Engine` passa a coordenar vários armazéns. O padrão atual se mantém para todas as
  camadas: snapshot original imutável, valor “antes da gestão”, gravação, **releitura**,
  histórico e travas.
- **Estado** (`crates/nvt-engine/src/estado.rs`): `Esperado`, `Travada`,
  `RegistroHistorico` e `Reversao` passam a usar `ChaveAjuste { camada, alvo, setting }`.
  `VERSAO_ESTADO` sobe de 1 para 2, com migração automática de um `estado.json` v1 (todas
  as entradas viram `camada = Driver`). Snapshot por camada em `%ProgramData%\nv-tuner`:
  `snapshot-driver` (o atual `nvt_drs::Snapshot`) e `snapshot-sistema.json`. Os snapshots
  da camada `Usuario`/`ConfigJogo` ficam com o agente (6.2).
- **Deriva por camada**: driver como hoje; Windows e jogo: valor diferente do esperado
  vira escolha do usuário (travada) e é informado, **sem regravar em loop**
  (`competitivo-jogos.md` §16, regra 5).

### 6.2 Escritas no escopo do usuário: decisão

Problema: HKCU do usuário interativo e arquivos no perfil dele. O serviço roda como
LocalSystem e vê o HKCU do SYSTEM (`estado-atual` §3.4; `docs/revisao-v0.1.md:349-358`).
Gravar como SYSTEM em pasta controlada pelo usuário é vetor clássico de escalada.

| Critério | (a) Serviço impersona o usuário (`WTSQueryUserToken` + `ImpersonateLoggedOnUser`) | (b) Agente no contexto do usuário grava o escopo de usuário |
|---|---|---|
| `SystemParametersInfoW(SPI_SETMOUSE, …, SPIF_SENDCHANGE)` | **Não funciona de verdade**: a mensagem `WM_SETTINGCHANGE` vai para a sessão 0; a sessão do usuário só vê a mudança no próximo logon | Funciona na sessão do usuário, com efeito imediato |
| `SetDisplayConfig` (taxa de atualização) | **Não funciona** a partir da sessão 0 (não há desktop interativo) | Funciona |
| HKCU | Exige `RegOpenCurrentUser` sob impersonação e hive carregado; `HKEY_CURRENT_USER` em cache no processo é armadilha | Natural (`HKCU`) |
| Pastas conhecidas (Documentos no OneDrive) | `SHGetKnownFolderPath` com token; possível | Natural |
| Escalada de privilégio | Mitigada pela impersonação, mas um erro (esquecer `RevertToSelf`, pool de threads, I/O assíncrono) volta a gravar como SYSTEM; o parser de arquivos do usuário roda num processo SYSTEM | **Eliminada para as escritas de usuário**: o agente só tem os direitos do usuário. Resta a superfície do parser IPC do serviço, que já existe hoje para a UI |
| Links e reparse points | Precisa de `abrir_sem_reparse` (`crates/nvt-service/src/diretorio.rs:588`) e checagem de dono em cada caminho | Irrelevante para escalada (o usuário só alcança o que já alcança); continua usando abertura sem seguir reparse para não editar o arquivo errado |
| Vários usuários/sessões | O serviço escolhe a sessão ativa (`WTSGetActiveConsoleSessionId`) e lida com troca rápida de usuário | Um agente por sessão; cada um cuida do próprio usuário |
| Sem ninguém logado | Nada a fazer | Nada a fazer (mesmo resultado) |

**Decisão: (b), agente do usuário.** O fator decisivo é técnico: SPI e `SetDisplayConfig`
são APIs da sessão interativa, então (a) não atende os itens recomendados da fase 2 nem
com impersonação perfeita. Como bônus, (b) elimina a classe de escalada em vez de
mitigá-la.

Desenho:

- Novo binário **`nvt-agente`** (crate `crates/nvt-agente`), sem janela, sem privilégios,
  iniciado no logon por `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run`
  (`nv-tuner-agente`), gravado pelo instalador. Um por sessão (mutex
  `Local\nv-tuner-agente`).
- O **serviço continua sendo o único avaliador**: ele planeja todas as camadas. O agente
  conecta no mesmo pipe como cliente, envia os **fatos do usuário** que só ele enxerga
  (HKCU, modo de vídeo, caminhos de config) e busca as **tarefas do usuário** do plano.
  **Autenticação:** não há segredo compartilhado; a identidade é o token do cliente. O
  agente abre o pipe com `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION` (o nível de
  impersonação é escolhido pelo cliente, não pelo servidor); o serviço chama
  `ImpersonateNamedPipeClient`, `OpenThreadToken`, lê `TokenUser` e `TokenSessionId` e
  chama `RevertToSelf` em seguida. Token anônimo ou falha em qualquer passo → requisição
  de agente rejeitada. A chave das tarefas é **(SID, sessão)**: o mesmo usuário em duas
  sessões (console + RDP) tem dois agentes. Um processo qualquer do usuário que se passe
  pelo agente só consegue alterar configurações do próprio usuário — o que ele já pode
  fazer — e poluir o histórico do próprio SID; isso é aceito.
- **Fatos do agente nunca alimentam regras de `Driver` ou `Sistema`.** O refresh e o VRR
  usados no perfil DRS global (7.4) vêm da leitura do próprio serviço (F1-01/F1-02),
  não do agente; os fatos do agente (inclusive o modo de vídeo lido na sessão) só
  condicionam regras de escopo `usuario`/`config_jogo`.
- **Pipe de instância única** (`crates/nvt-ipc/src/pipe.rs`, `CreateNamedPipeW` com 1
  instância; conexão limitada a 5 min): o agente **não** mantém conexão aberta. Usa
  conexões curtas (conecta, envia, recebe, desconecta) no logon, após mudança de fato
  local e a cada 60 s, para não bloquear a UI nem os agentes de outras sessões. O
  serviço considera o agente presente se ele fez contato nos últimos 3 min; senão,
  `AgenteAusente`.
- O agente guarda o snapshot original e o backup das camadas `Usuario` e `ConfigJogo` em
  `%LOCALAPPDATA%\nv-tuner\` (dono: o usuário) e relata cada execução ao serviço, que a
  registra no histórico unificado com `origem = agente` e o SID.
- Dados vindos do agente **nunca** alimentam gravação privilegiada: só decidem tarefas da
  própria camada de usuário. O serviço valida forma e lista permitida mesmo assim.
- O serviço também não lê arquivos no perfil do usuário como SYSTEM (evita divulgação de
  informação por link); essa leitura é do agente.
- “Restaurar original” incrementa um contador `geracao_restauracao` no serviço; cada
  agente, ao ver geração maior que a sua (inclusive no próximo logon de outro usuário),
  restaura a própria camada. Sem agente presente, o serviço restaura driver e sistema na
  hora e a UI mostra “ajustes do seu usuário: restauração pendente até o agente rodar”.
- A UI continua sem gravar nada (`crates/nvt-ui/src/local.rs:1-13`).

### 6.3 Revisão das proibições de `docs/arquitetura.md:118-119`

Texto atual: “O serviço não executa comandos arbitrários nem lê/grava arquivos fora dos
seus próprios diretórios de estado.” Ele já é contrariado pela leitura das pastas dos
jogos (`estado-atual` §5.2). Nova redação:

> - **Nenhum componente executa processos externos** (nem `powercfg.exe`, `bcdedit.exe`,
>   `reg.exe`, `cmd.exe`): toda ação usa APIs Win32/NVAPI/WMI diretamente. Exceção: o
>   instalador.
> - O serviço **grava** apenas: no DRS (via NVAPI), nos seus diretórios de estado e nos
>   destinos `Sistema` da lista permitida (`regras/windows.toml`).
> - O serviço **lê** arquivos fora do seu estado só para detecção: pastas de instalação
>   dos jogos, com compartilhamento total, sem seguir reparse points e sem manter
>   mapeamento (S3). Não lê arquivos no perfil dos usuários.
> - O agente do usuário grava apenas os destinos `Usuario` e `ConfigJogo` da lista
>   permitida, com os direitos do próprio usuário.
> - Toda requisição continua validada contra a lista permitida da camada; destinos fora
>   dela são rejeitados mesmo vindos de regra válida.

Consequência concreta: o BCD (fase 2) é alterado pelo provedor WMI `root\WMI`, classes
`BcdStore`/`BcdObject` (`OpenStore`, `OpenObject`, `GetElement`, `DeleteElement`), não por
`bcdedit`. O plano de energia, se um dia entrar, usará `PowerGetActiveScheme`/
`PowerSetActiveScheme`/`PowerWriteACValueIndex`, não `powercfg`.

### 6.4 Novos módulos e crates

| Onde | O quê | Fase |
|---|---|---|
| `nvt-detect::processos` | `listar_processos() -> Vec<String>` com `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)` + `Process32FirstW/NextW`; só `szExeFile`, PID e PID pai. Feature `Win32_System_Diagnostics_ToolHelp` do `windows-sys` 0.61 | 1 |
| `nvt-detect::facts::pe` | leitura de PE para buffer com compartilhamento total (F1-08) | 1 |
| `nvt-drs::exibicao` | FFI de `NvAPI_DISP_GetDisplayConfig` (origem com `bGDIPrimary`, `refreshRate1K` e `displayId` do alvo), `NvAPI_Disp_GetVRRInfo` e `NvAPI_GPU_GetConnectedDisplayIds`. Roda no serviço (sessão 0), como o DRS | 1 |
| `nvt-detect::tela` | `QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS)` + `DisplayConfigGetDeviceInfo` (`DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME`, nome GDI da **origem**) para refresh racional; `EnumDisplaySettingsExW` para o maior refresh na resolução atual. **Só na sessão do usuário** (agente): `QueryDisplayConfig` devolve `ERROR_ACCESS_DENIED` sem acesso à sessão de console, e o `EnumDisplaySettingsExW` da sessão 0 enumera a tela virtual da sessão 0 | 2 (leitura e escrita no agente) |
| `nvt-core::jogos` | `BaseJogos` (carga e validação de `regras/jogos.toml`), `IdJogo`, `Orientacao` | 1 |
| `crates/nvt-windows` | leitura/escrita de registro por lista permitida, edição de token em `REG_SZ`, SPI, `SetDisplayConfig`, BCD via WMI, diagnósticos somente leitura | 2 |
| `crates/nvt-agente` | binário do agente do usuário | 2 |
| `crates/nvt-jogocfg` | editores que preservam bytes para INI (Unreal/Ubisoft) e KeyValues/VDF (Valve), resolvedor de caminhos, backup com SHA-256 | 3 |

O `unsafe` de FFI continua confinado: driver em `nvt-drs`; Windows em `nvt-windows`,
`nvt-detect::{processos,tela}` e no pipe. `docs/arquitetura.md` (“Camadas”) passa a listar
esses pontos em vez de “único lugar com `unsafe`”.

---

## 7. Fase 1 — driver + fatos

### 7.1 Requisitos

| # | Requisito | Verificação |
|---|---|---|
| F1-01 | **Refresh do monitor.** A detecção do **serviço** preenche `Capabilities::refresh_mhz` (hoje `None` fixo, `crates/nvt-engine/src/deteccao.rs:44,64`) com o refresh **atual** do monitor principal da GPU NVIDIA, por `NvAPI_DISP_GetDisplayConfig` (`refreshRate1K` do alvo cuja origem tem `bGDIPrimary`), guardado em milésimos de Hz; o avaliador conta os limites de FPS sobre o piso em Hz inteiros (`refresh_hz_piso`, 59,94 → 59) e arredonda só na exibição. As APIs Win32 de modo de vídeo não servem aqui: o serviço roda na sessão 0 (ver `nvt-detect::tela`, 6.4). Falha ou monitor principal fora da GPU NVIDIA = `None`. `refresh_max_hz` fica para a fase 2 (lido pelo agente) | teste com NVAPI falsa; laboratório L1-1 (240, lido pelo serviço) |
| F1-02 | **VRR.** Novo `Capabilities::vrr: EstadoVrr { possivel: Option<bool>, ativo: Option<bool> }` a partir de `NvAPI_Disp_GetVRRInfo` (`bIsVRRPossible`, `bIsVRREnabled`) no `displayId` obtido em F1-01. Falha de NVAPI = `None` (desconhecido, nunca casa). A UI deixa de mostrar `vrr: false` fixo (`crates/nvt-ui/src/local.rs:612-613`) | teste com NVAPI falsa; laboratório |
| F1-03 | **Base curada de jogos competitivos** em `regras/jogos.toml` (7.3), embutida e validada como as regras (`carrega_regras_reais`). Casa um jogo detectado por nome de exe **e**, quando houver, AppID da Steam, para não confundir exes genéricos; ID de outros launchers fica para quando a base trouxer esses IDs (§12) (`estado-atual` §6.13) | teste de carga + teste de casamento |
| F1-04 | **Fatos curados.** Para jogo casado na base: `jogo_competitivo = true`, `jogo_reflex` e `jogo_limitador_nativo` vêm da base (a base vence a detecção em disco, que não acha Reflex via NVAPI — `deteccao.rs:91`); `jogo_anticheat_kernel` passa a aceitar `false` quando a base declara o anticheat (ex.: VAC). Jogo fora da base: comportamento atual | teste do motor |
| F1-05 | **Novas condições** em `quando`: `vrr_possivel`, `vrr_ativo` (bool; não casam se desconhecidos), `jogo_competitivo`, `jogo_limitador_nativo` (bool, só `por_jogo`). Novo valor derivado `limite_fps = { limite_vrr = true }` = `min(Hz − 3, floor(Hz − Hz²/3600))`; exige `refresh_conhecido = true` e `vrr_ativo = true` em `quando` | testes do avaliador (tabela de 7.4) |
| F1-06 | **Detecção de jogo em execução** por `nvt-detect::processos` (N2). Conjunto vigiado = exes de todos os jogos detectados + `processos_protegidos` da base + lista fixa de processos de anticheat que só existem durante o jogo (`EasyAntiCheat.exe`, `EasyAntiCheat_EOS.exe`, `start_protected_game.exe`, `BEService.exe`, `BEService_x64.exe`; nomes a confirmar, §11). Comparação sem diferenciar maiúsculas. `vgc.exe` (Vanguard) **não** entra: roda desde o boot | teste unitário com lista falsa; laboratório |
| F1-07 | **Adiar gravações.** Com algum processo vigiado em execução, o portão T-02 devolve `Adiamento { processos, desde }`. Vale para aplicação automática, deriva (`crates/nvt-service/src/nucleo.rs:311-370`), “Aplicar agora” e “Restaurar original”. A ação fica pendente e o serviço reavalia a cada 15 s enquanto houver pendência (sem pendência, não há varredura periódica de processos). Ao fechar o jogo, executa a ação pendente mais recente. Pedidos da UI respondem `adiada` (não erro). A leitura da deriva continua normal | teste do núcleo com relógio e processos falsos; laboratório L1-6 |
| F1-08 | **Leitura de PE com compartilhamento total.** `imports()` e `versao()` (`crates/nvt-detect/src/facts/pe.rs:92-110`) deixam de usar `pelite::FileMap`: abrem com `OpenOptions::read(true).share_mode(FILE_SHARE_READ \| FILE_SHARE_WRITE \| FILE_SHARE_DELETE)` (`std::os::windows::fs::OpenOptionsExt`), leem para `Vec<u8>` com limite de 256 MiB, **fecham o handle** e então chamam `PeFile::from_bytes`. Acima do limite, o fato fica desconhecido | teste: com o handle de leitura aberto, outro handle consegue abrir para escrita com `FILE_SHARE_READ`; após a função retornar, um handle com `share_mode(0)` abre |
| F1-09 | **Não varrer jogo aberto.** Se o exe de um jogo está em execução, a varredura reaproveita os fatos anteriores desse jogo e não abre nenhum arquivo da pasta dele (inclui DLLs de anticheat lidas em `facts/anticheat.rs:101-117`) | teste com processos falsos |
| F1-10 | **Regras do driver revistas juntas** (7.4): baixa latência global e G-SYNC + limite | testes do avaliador + laboratório |
| F1-11 | **Energia por jogo.** `baseline-energia-desempenho-maximo` (global) é substituída por `energia-desempenho-maximo-por-jogo` (escopo `por_jogo`, só para `jogo_competitivo = true`, §11 item 19). O global volta ao valor **de antes da gestão** pelo mecanismo de “settings que saíram do plano” (`docs/engine.md`). O `motivo` fala só em consistência de clock e nunca em ganho de latência (revisão driver §6: ganho de latência refutado) | teste do motor (reversão do global) + inspeção do texto |
| F1-12 | **Orientações por jogo.** Cada jogo da base traz `orientacoes` (códigos, 7.3). O plano passa a ter `plan.resumos[id].orientacoes`, filtradas pelas preferências (ex.: com `sincronismo = gsync`, a orientação de V-Sync do jogo é “desligado no jogo, ligado no driver”) | teste do avaliador + UI |
| F1-13 | **Adiado para o início da fase 2** (§12). **Modelo generalizado** (6.1) implementado só com o armazém `Driver`, sem mudança de comportamento: os testes atuais de `nvt-core` e `nvt-engine` passam sem alteração de expectativa, e um `estado.json` v1 migra para v2 preservando travas, histórico e `esperado` | testes existentes + teste de migração (na fase 2) |
| F1-14 | **IPC v4** (7.6), só com os campos que a fase 1 precisa; as variantes de camada Windows/Jogo ficam para a v5 (fase 2, §12) | testes de `nvt-ipc` (ida e volta serde, rejeição de v3) |
| F1-15 | **Aviso de G-SYNC sem VRR.** Com `sincronismo = gsync` e `vrr.possivel = Some(false)`, o plano marca o aviso `GsyncSemVrr` e a UI orienta a escolher “sem sincronismo” (sem VRR, a combinação vira V-Sync comum, com fila). Com `possivel = true` e `ativo = false`, aviso `VrrDesligadoNoPainel` (“ative G-SYNC em Painel NVIDIA > Configurar G-SYNC”); o app não liga o VRR do monitor | teste do avaliador |

### 7.2 Novos fatos de detecção

| Fato | Tipo | Fonte | Desconhecido quando |
|---|---|---|---|
| `refresh_mhz` | inteiro, milésimos de Hz (o avaliador usa o piso em Hz) | `NvAPI_DISP_GetDisplayConfig` (serviço) | sem origem primária na GPU NVIDIA / falha |
| `refresh_max_hz` (fase 2) | inteiro | `EnumDisplaySettingsExW(i)` na resolução atual, no agente | sem agente / falha |
| `vrr.possivel`, `vrr.ativo` | bool | `NvAPI_Disp_GetVRRInfo` | NVAPI sem a função ou erro |
| `jogo_competitivo` | bool | base curada | nunca (fora da base = `false`) |
| `jogo_reflex` | bool | base curada; senão detecção em disco | jogo fora da base sem DLL de Reflex |
| `jogo_limitador_nativo` | bool | base curada | jogo fora da base |
| `jogo_anticheat_kernel` | bool | base curada (pode ser `false`); senão detecção atual | jogo fora da base sem indício |
| `em_execucao` (por jogo, não é condição de regra) | bool | `nvt-detect::processos` | — |

### 7.3 Base curada `regras/jogos.toml`

```toml
versao_schema = 1

[[jogo]]
id = "cs2"                                   # kebab-case, único
nome = "Counter-Strike 2"
executaveis = ["cs2.exe"]
steam_appid = 730
reflex = true
limitador_nativo = true                      # fps_max
anticheat = "vac"                            # vac | vanguard | eac | battleye | eac_battleye | ricochet | blizzard | netease | proprio
anticheat_kernel = false
processos_protegidos = []                    # além dos executáveis
config = "edicao"                            # edicao | opcoes_inicializacao | orientar | proibido (fase 3)
orientacoes = ["reflex_on_boost", "limitador_do_jogo", "tela_cheia", "vsync_jogo_off"]
evidencia = "NVIDIA Reflex supported products; competitivo-jogos.md §2"
```

Conteúdo inicial (nomes de executável marcados com † estão por confirmar, §11):

| `id` | Executáveis | Reflex | Limitador nativo | Anticheat (kernel?) | `config` | Orientações principais |
|---|---|---|---|---|---|---|
| `cs2` | `cs2.exe` | sim | sim | VAC (não) | `edicao` (F3) | Reflex On+Boost; limitador do jogo ou nenhum; tela cheia; V-Sync do jogo off |
| `valorant` | `VALORANT-Win64-Shipping.exe` | sim | sim | Vanguard (sim) | `proibido` | Reflex On+Boost; “Limitar FPS” off com Reflex; tela cheia |
| `lol` | `League of Legends.exe` | **não** | sim | Vanguard (sim) | `proibido` | limitador do jogo estável ou sem limite; tela cheia; V-Sync off (a fila fica com o Ultra do driver) |
| `fortnite` | `FortniteClient-Win64-Shipping.exe` | sim | sim | EAC + BattlEye (sim) | `edicao` (F3) | Reflex On+Boost (Epic); limitador do jogo; tela cheia; modo Performance só por escolha |
| `apex` | `r5apex.exe`, `r5apex_dx12.exe`† | sim | sim (`+fps_max`) | EAC (sim) | `orientar` | Reflex On+Boost; `+fps_max` pela opção oficial da EA; resolução adaptativa 0 |
| `overwatch2` | `Overwatch.exe` | sim | sim | Blizzard (não†) | `orientar` | Reflex On ou On+Boost **ou** Reduce Buffering (testar; não impor) |
| `r6-siege` | `RainbowSix.exe`, `RainbowSix_BE.exe`† | sim | sim (`FPSLimit`) | BattlEye (sim) | `edicao` (F3) | Reflex On+Boost; `FPSLimit` se CPU a 100% |
| `cod` | `cod.exe`† | sim | sim | Ricochet (sim) | `proibido` | Reflex On+Boost; limite no menu |
| `rocket-league` | `RocketLeague.exe` | **não** | sim | EAC (sim) | `edicao` (F3) | Max FPS estável; V-Sync off; One Frame Thread Lag off; o driver cobre com Ultra |
| `pubg` | `TslGame.exe` | sim | sim | BattlEye (sim) | `orientar` | Reflex On+Boost; limite no menu |
| `dota2` | `dota2.exe` | **não** | sim | VAC (não) | `opcoes_inicializacao` (F3) | limite padrão de 120 FPS é baixo: subir no menu ou `+fps_max` |
| `marvel-rivals` | `Marvel-Win64-Shipping.exe`† | sim | sim | NetEase (?) | `proibido` | Reflex On+Boost; limite no menu |
| `deadlock` | `deadlock.exe`† | sim | sim | VAC (não) | `opcoes_inicializacao` (F3) | Reflex; `fps_max` |
| `the-finals` | `Discovery.exe` | sim | sim | EAC + próprio (sim) | `orientar` | Reflex; `FrameRateLimit` estável no menu |

Códigos de orientação (traduzidos em `textos.rs`): `reflex_on`, `reflex_on_boost`,
`reduce_buffering`, `limitador_do_jogo`, `sem_limite_com_reflex`, `tela_cheia`,
`vsync_jogo_off`, `vsync_driver_com_gsync`, `fps_max_launch_ea`, `fps_max_padrao_baixo`,
`one_frame_thread_lag_off`, `resolucao_adaptativa_off`, `modo_performance_opcional`,
`fps_limit_cpu_100`. Validação de carga: `id` único, ao menos um executável, códigos de
orientação conhecidos, `config = "edicao"`/`"opcoes_inicializacao"` proibido para
`valorant`, `lol`, `marvel-rivals` e `cod` (trava do escopo, teste dedicado).

### 7.4 Regras do driver (decididas juntas)

**Decisão (revisão §1, §9.1).** O Ultra global já faz o driver limitar o FPS abaixo do
refresh no modo G-SYNC (G-SYNC + V-Sync + Ultra/Reflex), em DX9/DX11/DX12 sem Reflex e em
jogos com Reflex; o laboratório registrou 224,3 FPS a 240 Hz (`regras/base.toml:110`),
que é o limite automático, e não o teto. Trocar o Ultra por On/Off **removeria** esse
limite. Por isso:

1. **`baseline-baixa-latencia-ultra` fica como está** (global, estável). Não há medição
   que justifique trocar por On: a evidência contra o Ultra é de cenário com FPS
   limitado e GPU abaixo de 95% (revisão §3), e o laboratório (LoL, provavelmente
   limitado pela CPU) mediu melhora com Ultra. A frase “Ultra piora em CPU-bound” não
   será usada em lugar nenhum; a formulação correta é “com FPS limitado”. A pergunta
   Ultra × On com limite ativo vira medição L1-4.
2. **`sincronismo-gsync-limite` é corrigida** para servir os casos residuais em que o
   limite automático não existe (Vulkan/OpenGL sem Reflex, LLM travado pelo usuário,
   jogos em que o automático falha) e para **coincidir** com o limite automático onde
   ele existe, em vez de ficar acima dele (`Hz − 3` = 237 @240 > 224).

| Regra | Escopo | Condição (`quando`) | Valor (`aplicar`) | Evidência | Status de lançamento |
|---|---|---|---|---|---|
| `baseline-baixa-latencia-ultra` (inalterada) | global | — | `low_latency_cpl = ultra` (`0x0005F543`=2), `low_latency_enabled = on` (`0x10835000`=1) | atual + NVIDIA System Latency Guide (limite automático com G-SYNC) | estável |
| `sincronismo-gsync-limite` (corrigida) | global | `sincronismo = ["gsync"]`, `refresh_conhecido = true`, `vrr_ativo = true` | `limite_fps` (`0x10835002`) `= { limite_vrr = true }` | Blur Busters G-SYNC 101 (“pelo menos 3 FPS abaixo”); valores do limite automático ~59/116/138/224 @60/120/144/240; heurística `Hz − Hz²/3600` (Wccftech; fórum Blur Busters, sem fórmula oficial) | experimental → estável após L1-2 e L1-3 |
| `energia-desempenho-maximo-por-jogo` (substitui a global) | por jogo | `jogo_competitivo = true` | `energia = desempenho_maximo` (`0x1057EB71`=1) | atual (“sem regressão” em LoL e WARDOGS; sem medição de ganho) + `competitivo-driver.md` §2.6 (recomendado por perfil de jogo; ganho de consistência de clock, não de latência) | estável (mesma evidência, escopo mais estreito; só jogos competitivos, §11, item 19, resolvido) |
| `gsync-reflex-sem-limite-driver` (nova) | por jogo | `sincronismo = ["gsync"]`, `jogo_reflex = true`, `jogo_competitivo = true`, `limite_fps_definido = false` | `limite_fps = off` no perfil do jogo | Blur Busters: limitador do jogo/Reflex fica ½–1 quadro à frente do Max Frame Rate; NVIDIA: com Reflex + V-Sync + G-SYNC o próprio Reflex limita | experimental → estável após L1-5 |

Valores de `limite_vrr` (teste de tabela obrigatório):

| Hz | 60 | 100 | 120 | 144 | 165 | 240 | 360 | 480 |
|---|---|---|---|---|---|---|---|---|
| `Hz − 3` | 57 | 97 | 117 | 141 | 162 | 237 | 357 | 477 |
| `floor(Hz − Hz²/3600)` | 59 | 97 | 116 | 138 | 157 | 224 | 324 | 416 |
| **resultado (mínimo)** | **57** | **97** | **116** | **138** | **157** | **224** | **324** | **416** |

Observações:

- `gsync-reflex-sem-limite-driver` não desobedece a preferência do usuário: só age sem
  limite definido por ele (`limite_fps_definido = false`); com limite, a preferência
  direta continua valendo no global.
- As três regras de sincronismo continuam definindo por inteiro o que governam
  (`docs/regras.md`, fim de “Explicação por jogo”).
- Settings **não** incluídas nesta fase, com motivo: tamanho do shader cache
  (`0x00AC8497`, evidência só de fórum); Preferred refresh rate (`0x0064B541`, a fase 2
  trata o refresh na origem); ReBAR, Threaded Optimization, filtragem, Fast Sync, MPRF
  legado (§2). `vsync_tear_control` e `platform_boost` continuam no catálogo sem regra.
- Ultra × limite automático em **DX12** não tem evidência (revisão §4, §9.8): medição
  L1-3.
- V-Sync do painel em **janela sem bordas** com flip model: a NVIDIA diz que só vale em
  tela cheia, o que conflita com `sincronismo-gsync` (`base.toml:109`). Medição L1-7; se
  confirmado, a orientação `tela_cheia` passa a ser obrigatória no texto do modo G-SYNC.

### 7.5 Mudanças de arquitetura da fase 1

- Modelo generalizado (6.1) com armazém único `Driver`; `VERSAO_ESTADO = 2` — adiado
  para o início da fase 2 (F1-13, §12); na fase 1, `VERSAO_ESTADO` continua `1`.
- `nvt-detect::processos`, `nvt-drs::exibicao`, `nvt-core::jogos` (`nvt-detect::tela`
  entra na fase 2, no agente).
- `Engine::pode_gravar()` e estado `pendente: Option<AcaoPendente>` no núcleo do serviço
  (`crates/nvt-service/src/nucleo.rs`): `AplicarPlano`, `TratarDeriva`,
  `RestaurarOriginal`; a mais recente substitui as anteriores, exceto
  `RestaurarOriginal`, que vence qualquer outra e é persistida (§12).
- `JogoEntrada` (`crates/nvt-engine/src/deteccao.rs:12-24`) passa a carregar a pasta de
  instalação, o AppID e o `IdJogo` da base (necessários para F1-03 e para a fase 3).

### 7.6 IPC v4

`PROTOCOL_VERSION = 4` (`crates/nvt-ipc/src/lib.rs:25`). Sem negociação, como hoje: o MSI
instala UI, serviço e agente juntos. **Decisão do líder (§12):** a v4 leva só os campos
que a fase 1 precisa (`Capacidades.refresh_mhz`/`vrr`, `Jogo.competitivo`/`em_execucao`/
`orientacoes`, `adiamento`/`adiada`, os avisos `GsyncSemVrr`/`VrrDesligadoNoPainel` e os
motivos da fase 1). As variantes de camada (`camada`, `AlvoIpc::Sistema`/`Usuario`/
`ConfigJogo`, `ValorIpc` além de DWORD, mensagens do agente, `ConfirmarAcao`/`RecusarAcao`
e os itens marcados F2/F3) entram na **v5**, no início da fase 2, com o modelo
generalizado (F1-13). A tabela abaixo descreve o esquema completo; o que é de camada
Windows/Jogo é v5.

| Mudança | Detalhe |
|---|---|
| `Mudanca` | ganha `camada`, `alvo: AlvoIpc` e `valor: ValorIpc` (`Dword`, `Inteiro`, `Decimal`, `Texto`, `Ausente`), mantendo `valor_nome` e a explicação |
| `PlanoPerfil` → `PlanoAlvo` | `{ alvo: AlvoIpc, mudancas }`; `AlvoIpc = PerfilGlobal \| PerfilJogo { perfil } \| Sistema \| Usuario \| ConfigJogo { jogo, arquivo }` |
| `TravarSetting` / `DestravarSetting` | `{ alvo: AlvoIpc, setting }` (hoje `{ perfil, setting }`, `protocolo.rs:47`) |
| `Capacidades` | `refresh_mhz`, `vrr: { possivel, ativo }`; (F2) `refresh_max_hz` |
| `Jogo` | `competitivo: Option<String>` (id da base), `em_execucao`, `orientacoes: Vec<String>` (códigos) |
| `Estado` | `adiamento: Option<Adiamento { processos, desde, acao }>`; `avisos` ganha `GsyncSemVrr`, `VrrDesligadoNoPainel`; (F2) `reinicio_pendente`, `diagnosticos`, `agente: Option<EstadoAgente>` |
| `Aplicacao` (resposta de `AplicarAgora`/`RestaurarOriginal`) | `adiada: Option<Adiamento>` |
| `MotivoNaoAplicada` | novos: `VrrInativo`, `JogoComReflex`; (v5) `CamadaIndisponivel`; (F2) `RequerConfirmacao`, `RequerReinicio`, `AgenteAusente`, `BitLockerAtivo`; (F3) `ConfigPolitica { politica }`, `JanelaFechadaExigida { processos }`, `ArquivoNaNuvem`, `ArquivoSomenteLeitura`, `FormatoInesperado`, `ArquivoAlteradoPeloJogo`, `ContaAmbigua` |
| Agente (F2) | `RegistrarAgente { versao }`, `RelatarFatosUsuario { fatos }`, `ObterTarefasUsuario`, `RelatarExecucaoUsuario { resultados, geracao_restauracao }`; resposta `Encerrar` (desinstalação, F2-11) |
| Ações pendentes (F2) | `ConfirmarAcao { regra }`, `RecusarAcao { regra }` |

Jogo aberto não é motivo de não aplicação: o motor nunca deixa de aplicar uma regra
por jogo aberto, ele adia a gravação inteira (`Estado.adiamento`, respostas `adiada`);
por isso não há `MotivoNaoAplicada::JogoEmExecucao` (§12).

`docs/ipc.md` deve ser atualizado na implementação (fora desta especificação).

### 7.7 UI da fase 1

- Cartão de cada jogo competitivo: selo “competitivo”, seção **“O que o app fez”**
  (perfil do driver: mudanças próprias + “herda N ajustes globais”) e seção
  **“Faça no jogo”** com as orientações (ex.: “Ligue o Reflex em *On + Boost* e use o
  limitador do jogo”).
- Faixa de estado: “Jogo aberto (cs2.exe): as alterações serão aplicadas quando você
  fechar o jogo” quando houver adiamento.
- Capacidades: refresh atual, VRR possível/ativo; aviso de G-SYNC sem VRR ao
  lado da escolha de sincronismo.
- Textos novos em `crates/nvt-ui/src/textos.rs` para cada código de orientação, motivo e
  aviso.

### 7.8 Critérios de aceite da fase 1

1. Com monitor de 240 Hz e VRR ativo no modo `gsync`, o plano traz
   `limite_fps = 224` como sugestão experimental de `sincronismo-gsync-limite`
   (valor da tabela de 7.4), fora do plano global; sem VRR ativo, não há sugestão e
   há aviso. O limite entra no plano global só após a promoção da regra com L1-2 e
   L1-3 (§12).
2. Com um processo vigiado aberto, nenhuma chamada de gravação ao DRS ocorre (log e
   contador do `DriverFalso`); ao fechar, a ação pendente roda uma vez.
3. Nenhuma ocorrência das APIs de N1/N2 no código (T-01).
4. `pe.rs` não usa `FileMap`; teste de compartilhamento de F1-08 passa.
5. O perfil global volta ao valor de `energia` de antes da gestão e os perfis de jogos
   detectados passam a ter `desempenho_maximo`.
6. Estado v1 do laboratório migra sem perda (travas e histórico visíveis na UI).
7. Cada jogo da base, quando detectado, aparece com orientações (teste com detecção falsa
   dos 14); Valorant/LoL/Marvel Rivals/CoD sem nenhuma ação de arquivo prevista.
8. `README.md` e `docs/arquitetura.md` com a promessa da §4 (versão restrita ao driver),
   `docs/regras.md` com as novas condições e `docs/ipc.md` com a v4.
9. Laboratório: L1-1 a L1-8 feitos (L1-8 valida a mudança de energia, que já sai como
   `estavel`); regras experimentais promovidas só com resultado registrado na
   `evidencia`.

### 7.9 Plano de validação no laboratório (fase 1)

| # | O que | Jogo / condição | Medir | Sentir | Aprova se |
|---|---|---|---|---|---|
| L1-1 | Detecção de refresh e VRR **pelo serviço (sessão 0)** | desktop | `refresh_mhz`, `vrr` na UI vs. Painel NVIDIA | — | iguais; trocar para 144 Hz no Windows reflete na próxima detecção. Se a NVAPI de exibição não responder na sessão 0, F1-01/F1-02 voltam para decisão (§11, item 18) |
| L1-2 | `sincronismo-gsync-limite` em DX11 | LoL, modo `gsync` | FPS (deve ficar ≤ 224), frametime p99, entrada→fóton (PresentMon `MsPCLatency`/`MsClickToPhotonLatency` se disponível) | tearing ausente, sensação igual ou melhor que antes | FPS ≤ 224 e sem piora de p99 frente ao `gsync` atual |
| L1-3 | Limite manual onde o automático falta | Dota 2 com `-vulkan` (sem Reflex) e um DX12 sem Reflex (ex.: Fortnite DX12 com Reflex off), modo `gsync` | FPS máximo antes e depois; presença de tearing/encostar no teto | engasgo perto do teto | sem a regra, FPS encosta em 240; com a regra, fica ≤ 224 |
| L1-4 | Ultra × On com limite ativo | Rocket League (DX11, sem Reflex) com Max FPS no jogo abaixo do que a GPU sustenta | entrada→fóton média e p99, frametime p99 | resposta do carro | registrar; só muda a regra global se On vencer com margem maior que a variação entre sessões **e** se a mudança não quebrar o limite automático do modo `gsync` |
| L1-5 | `gsync-reflex-sem-limite-driver` | CS2 com Reflex On, modo `gsync`; e um jogo DX12 com Reflex (Fortnite DX12) com o Reflex **desligado** no jogo | entrada→fóton com e sem a regra; FPS máximo no caso com Reflex desligado | — | sem piora com Reflex On **e**, com Reflex Off em DX12, FPS continua abaixo do refresh (sem o limite do driver, o app depende de o usuário ligar o Reflex ou do limite automático do Ultra em DX12, que não tem evidência). Se encostar no teto, a regra não é promovida |
| L1-6 | Adiamento e ausência de interação | CS2 (VAC), Fortnite (EAC+BE), R6 (BE), LoL (Vanguard) | log do serviço; Process Explorer **sobre o `nvt-service`** (handles abertos por ele) | jogo abre e a partida transcorre sem erro de anticheat | nenhum handle do serviço para processo de jogo; mudança manual no Painel NVIDIA com o jogo aberto só é tratada após fechar |
| L1-7 | V-Sync do painel em janela sem bordas | Fortnite DX12 em sem bordas, modo `gsync` | tearing; PresentMon `PresentMode` | — | registrar em `docs/pesquisa` e ajustar a orientação |
| L1-8 | Energia por jogo | navegador com vídeo e jogo | clock/P-state em idle (GPU-Z ou NVML) | — | em idle com navegador a GPU desce de P-state; no jogo, comportamento de antes |

---

## 8. Fase 2 — camada Windows

Só entram como **escrita** os itens recomendados/condicionais que sobreviveram à revisão
(`competitivo-revisao-windows-anticheat.md`, §1 e resumo). O resto vira **diagnóstico**.

### 8.1 Requisitos

| # | Requisito | Verificação |
|---|---|---|
| F2-01 | Crate `nvt-windows` com armazéns `Sistema` (no serviço) e `Usuario` (no agente) implementando `ArmazemCamada`. Toda escrita confere o destino contra `regras/windows.toml` | testes com registro falso (`HKCU\Software\nv-tuner-teste`) |
| F2-02 | Agente `nvt-agente` (6.2): registro no logon, mutex por sessão, conexão ao pipe, envio de fatos, execução de tarefas, snapshot e histórico em `%LOCALAPPDATA%\nv-tuner\usuario\`, relato ao serviço. Sem agente conectado, tarefas de usuário ficam com motivo `AgenteAusente` | teste de integração com pipe local (`crates/nvt-ipc/tests/pipe_local.rs` como modelo) |
| F2-03 | O agente aplica o portão S1 localmente também (mesma lista vigiada do serviço, via `TH32CS_SNAPPROCESS`, que enumera todas as sessões sem abrir handle) antes de cada escrita | teste |
| F2-04 | **Refresh máximo** por `QueryDisplayConfig`/`SetDisplayConfig` (não `ChangeDisplaySettingsExW`: perde o racional, revisão §1a). Só modos enumerados na **resolução atual**; nunca muda resolução nem profundidade. O snapshot guarda o identificador estável do alvo (caminho do dispositivo do monitor, `DISPLAYCONFIG_TARGET_DEVICE_NAME.monitorDevicePath`) e o refresh racional original; os vetores `DISPLAYCONFIG_PATH_INFO`/`DISPLAYCONFIG_MODE_INFO` **não** são reaproveitados depois (LUIDs de adaptador e índices de modo mudam entre boots e atualizações de driver). Aplicar e reverter = ler a configuração atual, trocar só o refresh daquele alvo e chamar `SetDisplayConfig` com `SDC_APPLY \| SDC_USE_SUPPLIED_DISPLAY_CONFIG \| SDC_SAVE_TO_DATABASE`. Após aplicar, relê; se o refresh lido não for o pedido, reverte e registra falha | teste com fachada falsa; laboratório L2-1 |
| F2-05 | **Aceleração do mouse**: `SystemParametersInfoW(SPI_GETMOUSE)` lê o trio original; aplica `[0, 0, 0]` com `SPI_SETMOUSE` e `SPIF_UPDATEINIFILE \| SPIF_SENDCHANGE`; reverte para o trio original. Não toca `MouseSensitivity` | teste com fachada falsa; laboratório |
| F2-06 | **Otimizações para jogos em janela**: edita só o token `SwapEffectUpgradeEnable` em `HKCU\Software\Microsoft\DirectX\UserGpuPreferences` → `DirectXUserGlobalSettings` (REG_SZ `k=v;`), preservando os demais tokens (`VRROptimizeEnable`, `AutoHDREnable` etc.) e a ordem. Reverter restaura o token original ou o remove se não existia; valor vazio após remover → apaga o valor (ou mantém `Ausente`, conforme o original). Não toca valores por executável | testes de token (tabela de casos) |
| F2-07 | **Captura em segundo plano**: só se `HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR\HistoricalCaptureEnabled = 1`, grava `0` (DWORD). Não toca `AppCaptureEnabled`, `HKCU\System\GameConfigStore\GameDVR_Enabled` nem a política `AllowGameDVR` | teste |
| F2-08 | **`useplatformclock`**: o serviço lê o elemento `BcdOSLoaderBoolean_UsePlatformClock` (`0x260000F2`, a confirmar) do objeto `{current}` pelo WMI `root\WMI` (`BcdStore`); só se estiver `true`, propõe removê-lo (`DeleteElement`). Exige `confirmacao = "usuario"` e marca `reinicio_pendente`. Se o volume do sistema tiver BitLocker com proteção ativa (`Win32_EncryptableVolume.ProtectionStatus = 1`, namespace `root\CIMV2\Security\MicrosoftVolumeEncryption`), não age: motivo `BitLockerAtivo` e orientação para suspender a proteção por um reinício (`manage-bde -protectors -disable C: -RebootCount 1`, executado pelo usuário). A **reversão** (regravar `true` com `SetBooleanElement`, só se o snapshot tinha `true`) passa pelas mesmas travas: BitLocker, confirmação do usuário e `reinicio_pendente`; nunca roda sem confirmação, nem na desinstalação | teste com fachada WMI falsa |
| F2-09 | **Diagnósticos somente leitura** (8.3), apresentados com estado, recomendação e link oficial; nenhum gera escrita | teste: o armazém de diagnóstico não tem método de escrita |
| F2-10 | Deriva da camada Windows: verificação no início do serviço/agente e a cada 10 min; mudança manual vira travada e é informada | teste |
| F2-11 | Desinstalação: a restauração continua **opcional**, pela propriedade existente `RESTAURAR_PERFIL=1` (padrão `0`, `instalador/nv-tuner.wxs:83`). Com `1`, o MSI chama o serviço para restaurar driver (como hoje, `restore-original --yes`) e roda `nvt-agente --restaurar` como o usuário que desinstala (custom action diferida com `Impersonate="yes"`, `Return="ignore"`). A desinstalação **não** reverte o BCD (exige confirmação e reinício, F2-08): lista-o no log e a UI orienta antes. Na desinstalação, a regra S1 não pode adiar: com jogo detectado, a restauração é pulada e registrada no log do MSI. Antes de remover arquivos, o MSI pede ao agente que encerre (mensagem IPC `Encerrar`, ou Restart Manager), sem matar processo; a entrada `Run` é removida. Sem `RESTAURAR_PERFIL=1`, ou para outros usuários da máquina, as camadas de usuário ficam como estão (R10); a UI avisa que cada usuário deve usar “Restaurar original” antes | inspeção do `.wxs`; teste manual |
| F2-12 | Promessa pública com o item “Windows” (§4) | inspeção |

### 8.2 Regras e catálogo (`regras/windows.toml`)

Catálogo (lista permitida), um destino por entrada:

| `nome` | Camada / alvo | Destino | Valores |
|---|---|---|---|
| `tela_refresh` | windows / usuario | `ModoVideo(RefreshMaximoNativo)` | `maximo_nativo`, `original` |
| `mouse_aceleracao` | windows / usuario | `Spi(Mouse)` | `off = [0,0,0]`, `original` |
| `dx_swap_effect_upgrade` | windows / usuario | `TokenRegistro { HKCU\Software\Microsoft\DirectX\UserGpuPreferences, DirectXUserGlobalSettings, SwapEffectUpgradeEnable }` | `on = "1"`, `off = "0"`, `ausente` |
| `gamedvr_captura_historica` | windows / usuario | `Registro { HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR, HistoricalCaptureEnabled, DWORD }` | `off = 0`, `on = 1`, `ausente` |
| `bcd_useplatformclock` | windows / sistema | `Bcd { 0x260000F2 }` | `ausente` (aplicação); `original` (só reversão para o valor do snapshot, N7) |

Novas condições: `windows_build_min` (inteiro, `RtlGetVersion`), `refresh_abaixo_do_maximo`
(bool), `mouse_aceleracao_ligada` (bool), `swap_effect_upgrade` (`on`/`off`/`ausente`),
`captura_historica_ligada` (bool), `useplatformclock_ligado` (bool),
`bitlocker_protegendo_sistema` (bool). Fatos de usuário vêm do agente; sem agente, são
desconhecidos e nada casa.

| Regra | Escopo | Condição | Valor | Evidência | Status de lançamento |
|---|---|---|---|---|---|
| `windows-refresh-maximo` | usuario | `refresh_abaixo_do_maximo = true` | `tela_refresh = maximo_nativo` | física do display; Microsoft Support “Alterar a taxa de atualização” (inclusive quedas para 60 Hz após atualização); revisão §1a | experimental → estável após L2-1 |
| `windows-mouse-sem-aceleracao` | usuario | `mouse_aceleracao_ligada = true` | `mouse_aceleracao = off` | Microsoft Learn `SystemParametersInfoW` (`SPI_SETMOUSE`); raw input ignora a curva (efeito em menus, MOBAs, jogos sem raw input); revisão §1b | experimental → estável após L2-2 |
| `windows-otimizacoes-janela` | usuario | `windows_build_min = 22000`, `swap_effect_upgrade = ["off", "ausente"]` | `dx_swap_effect_upgrade = on` | DirectX Dev Blog (fev/2022) e Microsoft Support “Optimizations for windowed games”; chave por fonte comunitária (ElevenForum, TheWindowsClub); revisão §1e | experimental → estável após L2-3 |
| `windows-captura-historica-off` | usuario | `captura_historica_ligada = true` | `gamedvr_captura_historica = off` | Xbox Wire (2016): gravação em segundo plano custa desempenho e vem desligada por padrão; corretivo (revisão §1d) | experimental → estável após L2-4 |
| `windows-remover-useplatformclock` | sistema | `useplatformclock_ligado = true`, `bitlocker_protegendo_sistema = false` | `bcd_useplatformclock = ausente`; `requer_reinicio = true`; `confirmacao = "usuario"` | Microsoft Learn “Acquiring high-resolution time stamps” (timer de plataforma mais caro que o TSC); revisão §1f | experimental → estável após L2-5 |

Todas com `impacto_visual = "nenhum"`. Game Mode **não** é escrito: efeito misto (revisão
§1c); vira diagnóstico.

### 8.3 Diagnósticos (somente leitura)

| Código | Leitura | Mensagem/ação sugerida |
|---|---|---|
| `game_mode_desligado` | `HKCU\Software\Microsoft\GameBar\AutoGameModeEnabled = 0` | “Considere religar (padrão do Windows); obrigatório para Ryzen X3D dual-CCD” |
| `hags` | `HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers\HwSchMode` | informa; manter o padrão do sistema |
| `vrr_windows` | token `VRROptimizeEnable` em `DirectXUserGlobalSettings` | com VRR ativo e jogos DX11 sem VRR nativo, sugerir ligar em Configurações > Tela > Gráficos |
| `drr` | taxa dinâmica (Win11) — fonte de leitura a confirmar (§11) | sugerir desligar se limitar jogos sem VRR |
| `mpo` | `HKLM\SOFTWARE\Microsoft\Windows\Dwm\OverlayTestMode` | só informa se presente; artigo NVIDIA 5157 para sintomas |
| `fso_por_jogo` | `HKCU\Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers\<exe>` com `DISABLEDXMAXIMIZEDWINDOWEDMODE` | informa por jogo |
| `plano_energia` | `PowerGetActiveScheme` + `PowerGetActualOverlayScheme` | informa; nunca troca (X3D dual-CCD, Intel híbrido, Modern Standby — revisão §3) |
| `cpu_x3d_dual_ccd` | nome do processador + serviço `amd3dvcache` | contexto para os itens acima |
| `msi_gpu` | `HKLM\SYSTEM\CurrentControlSet\Enum\PCI\<VEN_10DE…>\…\MessageSignaledInterruptProperties\MSISupported` | só informa (revisão §2c) |
| `vbs_hvci` | WMI `root\Microsoft\Windows\DeviceGuard`, `Win32_DeviceGuard` (`VirtualizationBasedSecurityStatus`, `SecurityServicesRunning`) | com Vanguard/FACEIT instalado e HVCI desligado: avisar que o anticheat pode exigir (links Riot/FACEIT) |
| `tpm` | `Tbsi_GetDeviceInfo` | idem |
| `secure_boot` | `HKLM\SYSTEM\CurrentControlSet\Control\SecureBoot\State\UEFISecureBootEnabled` | idem; nota sobre certificados UEFI CA 2011 (a verificar, revisão §6) |
| `iommu` | `Win32_DeviceGuard.AvailableSecurityProperties` / Kernel DMA Protection | FACEIT acima de 3.000 de Elo, Vanguard On-Demand |
| `windows_11` | `RtlGetVersion` (build ≥ 22000) | FACEIT exige Windows 11 desde out/2026 |
| `polling_mouse` | — | orientação estática: ≥ 1000 Hz no software do mouse |

### 8.4 Arquitetura e IPC da fase 2

- Armazém `Sistema` no serviço (BCD) e armazém `Usuario` no agente; `snapshot-sistema.json`
  no serviço; `%LOCALAPPDATA%\nv-tuner\usuario\{estado.json,snapshot.json}` no agente,
  com dono = usuário e ACL herdada do perfil.
- `ResumoSistema` em `plan.resumos` (T-05).
- IPC v5 (7.6, §12): camadas, mensagens do agente e `ConfirmarAcao`/`RecusarAcao`;
  `Estado.reinicio_pendente`, `Estado.diagnosticos`, `Estado.agente`.
- `windows-sys` ganha as features `Win32_Devices_Display`, `Win32_UI_WindowsAndMessaging`,
  `Win32_System_Registry`, `Win32_System_Power`, `Win32_System_Diagnostics_ToolHelp`,
  `Win32_System_RemoteDesktop` e `Win32_Security`. WMI/COM pelo crate `windows`
  (Microsoft) no `nvt-windows`, sem crate de terceiros (decisão a confirmar no PR).

### 8.5 UI da fase 2

- Aba **Windows** com três blocos: “O que o app fez” (mudanças, cada uma com reverter),
  “Ações que pedem sua confirmação” (BCD, com aviso de reinício e de BitLocker) e
  “Diagnóstico” (somente leitura, com links oficiais).
- Indicador de agente: “ajustes do seu usuário: ativos / agente não está em execução”.
- Faixa “reinício pendente” quando houver.

### 8.6 Critérios de aceite da fase 2

1. Cada item de 8.2 aplica, relê e reverte exatamente ao original (inclusive “valor não
   existia”) em teste automatizado e no laboratório.
2. A UI segue sem gravar nada; o serviço não grava em HKCU nem no perfil de usuário; o
   agente não grava fora dos destinos `Usuario`.
3. Nenhum processo externo é lançado (T-01 cobre `Command::new`).
4. Diagnósticos sem nenhuma escrita (teste de tipo: armazém sem `gravar`).
5. “Restaurar original” restaura driver, sistema e o usuário atual; outro usuário
   restaura no próximo logon; o BCD só com confirmação (F2-08).
6. Laboratório L2-1 a L2-6 registrados.
7. Com UI aberta e dois agentes (duas sessões) ativos, a UI nunca recebe tempo esgotado
   do pipe por causa dos agentes (conexões curtas, 6.2).

### 8.7 Plano de validação no laboratório (fase 2)

| # | O que | Como | Aprova se |
|---|---|---|---|
| L2-1 | Refresh máximo | pôr o monitor em 60 Hz pelo Windows; deixar o app agir; reverter | volta a 240 Hz (racional igual ao do Windows); reverter retorna exatamente a 60 Hz; nenhum jogo aberto durante a troca |
| L2-2 | Mouse | ligar “Aprimorar precisão do ponteiro”; app desliga; LoL (cursor do sistema) | sensação de mira/cursor consistente; reverter religa sem logoff |
| L2-3 | Otimizações para jogos em janela | Rocket League e LoL em sem bordas (DX11), antes/depois | PresentMon `PresentMode` passa de `Composed: Copy…` para `…Independent Flip`; entrada→fóton igual ou menor |
| L2-4 | Captura histórica | ligar “Gravar o que aconteceu”; app desliga | valor volta a 0; Game Bar continua funcionando |
| L2-5 | `useplatformclock` | `bcdedit /set useplatformclock true` manual pelo mantenedor (BitLocker suspenso); app propõe; confirmar; reiniciar | elemento removido; frametime p99 em CS2 igual ou melhor |
| L2-6 | Anticheat | CS2, Fortnite, R6, LoL abertos com agente e serviço ativos | sem erro de anticheat; agente sem handle para processo de jogo (Process Explorer sobre `nvt-agente`) |

---

## 9. Fase 3 — configuração de jogo

### 9.1 Escopo por jogo

| Jogo | O que o app grava | Onde | Fechados antes de gravar |
|---|---|---|---|
| Rainbow Six Siege | `[DISPLAY] FPSLimit` | `%USERPROFILE%\Documents\My Games\Rainbow Six - Siege\<conta>\GameSettings.ini` (Documentos resolvido por `SHGetKnownFolderPath(FOLDERID_Documents)`) | jogo, processos BattlEye |
| Fortnite | `FrameRateLimit` (e, se confirmado, a chave de Reflex) | `%LOCALAPPDATA%\FortniteGame\Saved\Config\WindowsClient\GameUserSettings.ini`, seção `[/Script/FortniteGame.FortGameUserSettings]` | jogo, EAC, BattlEye |
| Rocket League | Max FPS (`MaxSmoothedFrameRate`/`bSmoothFrameRate`, a confirmar) e `OneFrameThreadLag` | `Documents\My Games\Rocket League\TAGame\Config\TASystemSettings.ini` | jogo, EAC |
| CS2 | opção de inicialização `+fps_max <n>`; (experimental) Reflex e V-Sync em `cs2_video.txt` | `Steam\userdata\<SteamID3>\config\localconfig.vdf` → `UserLocalConfigStore/Software/Valve/Steam/apps/730/LaunchOptions`; `…\730\local\cfg\cs2_video.txt` | jogo **e** `steam.exe` |
| Dota 2 | `+fps_max <n>` nas opções de inicialização | `localconfig.vdf`, app 570 | jogo e `steam.exe` |
| Deadlock | `+fps_max <n>` nas opções de inicialização | `localconfig.vdf`, app 1422450 | jogo e `steam.exe` |

Fora (só orientação, fase 1): Apex, Overwatch 2, PUBG, The Finals. Proibidos: Valorant,
LoL, Marvel Rivals, CoD. Nada na pasta de instalação (exclui `autoexec.cfg`, `video.txt`,
`boot.vcfg`, `gameinfo.gi`).

### 9.2 Requisitos

| # | Requisito | Verificação |
|---|---|---|
| F3-01 | Crate `nvt-jogocfg` com editor INI e editor KeyValues/VDF que **preservam bytes**: só o trecho do valor alvo muda; codificação (UTF-8 com/sem BOM, UTF-16LE), fim de linha, comentários, ordem e espaços preservados. Chave ausente → não cria (motivo `FormatoInesperado`); formato que o parser não reconhece por inteiro → aborta sem gravar | testes de ida e volta byte a byte com amostras reais anonimizadas |
| F3-02 | **Só valores do menu**: cada chave no catálogo (`regras/jogos.toml`, seção `[[arquivo]]`) tem `valores`/`faixa` iguais aos que o menu oferece (ex.: Dota 2 `fps_max` até 240, §11). Fora disso, a mudança é descartada | teste do avaliador |
| F3-03 | **Janela de edição**: grava só com o jogo, seus processos protegidos e, para `localconfig.vdf`, o `steam.exe` fechados (S1 + regra de Steam). Senão, `JanelaFechadaExigida` e adiamento até a próxima janela; o agente nunca fecha processo algum | teste |
| F3-04 | **Backup**: antes da primeira gravação, cópia íntegra do arquivo em `%LOCALAPPDATA%\nv-tuner\backups\<jogo>\<arquivo>.orig` com SHA-256, imutável (como o snapshot do DRS). A cada gravação, registro de `hash_antes`, `hash_depois` e das chaves alteradas | teste |
| F3-05 | **Gravação atômica**: escreve em arquivo temporário na mesma pasta e troca com `ReplaceFileW` (preserva atributos e ACL); abre sem seguir reparse point (`FILE_FLAG_OPEN_REPARSE_POINT`) e recusa se o caminho final for link/junção | teste com junção (modelo: `crates/nvt-service/src/diretorio.rs:747-796`) |
| F3-06 | **Somente leitura**: arquivo com `FILE_ATTRIBUTE_READONLY` não é tocado (`ArquivoSomenteLeitura`) e a UI orienta a desmarcar (Epic documenta que somente leitura quebra o menu); o app nunca marca nem desmarca | teste |
| F3-07 | **Restauração**: se o hash atual = `hash_depois`, restaura o arquivo original inteiro; senão (o jogo regravou), reverte só as chaves que ainda têm o valor gravado pelo app; se o jogo mudou a chave, não mexe e informa (`ArquivoAlteradoPeloJogo`) | teste |
| F3-08 | **Deriva**: na próxima janela fechada, se a chave não tem o valor esperado, vira escolha do usuário (travada) e é informada; nunca regrava em loop | teste |
| F3-09 | **Steam Cloud (CS2)**: o app só edita `cs2_video.txt` em `730\local\cfg`. Se o arquivo efetivo estiver em `730\remote\cfg` (sincronizado), não edita (`ArquivoNaNuvem`) e só orienta. `localconfig.vdf` não é afetado pelo Cloud do jogo (a confirmar, §11) | teste + laboratório L3-3 |
| F3-10 | **Conta Steam**: usa a conta com `MostRecent = "1"` em `Steam\config\loginusers.vdf`; `SteamID3 = SteamID64 − 76561197960265728`. Caminho da Steam por `HKCU\Software\Valve\Steam\SteamPath` | teste com VDF de exemplo |
| F3-11 | **Opções de inicialização**: edita por token dentro de `LaunchOptions`: acrescenta ou troca só `+fps_max <n>`; preserva o resto da string do usuário; nunca adiciona `-allow_third_party_software` (teste de trava, N10). Restaurar remove só o token do app (ou devolve o valor original se a string não mudou desde a gravação) | testes de token |
| F3-12 | **Conta Ubisoft (R6)**: com uma pasta de conta, usa-a; com várias, usa a do `GameSettings.ini` modificado mais recentemente e mostra qual na UI; empate → `ContaAmbigua`, nada gravado | teste |
| F3-13 | **Limite de FPS no jogo**: com `limite_fps` definido pelo usuário e jogo da fase 3 com limitador gravável, o limite vai para o arquivo/opção do jogo **e** o perfil DRS do jogo recebe `limite_fps = off` (não empilhar limitadores, `competitivo-jogos.md` §1.1). Sem limite do usuário, o app não cria limite no jogo (R6 com CPU a 100% fica só como orientação, pois não há medição no app). Única exceção: `jogo-dota2-fps-max-padrao`, que **sobe** o teto padrão de 120 do Dota 2 (não cria um limite mais baixo) | teste do avaliador |
| F3-14 | Promessa pública completa (§4) | inspeção |

### 9.3 Regras da fase 3

| Regra | Escopo | Condição | Valor | Evidência | Status de lançamento |
|---|---|---|---|---|---|
| `jogo-limite-no-jogo-r6` | config_jogo | jogo `r6-siege`, `limite_fps_definido = true` | `r6_fps_limit = { preferencia = "limite_fps" }` (mín. 30) + perfil DRS `limite_fps = off` | Ubisoft “100% CPU usage” manda editar `FPSLimit` [O]; Blur Busters (limitador do jogo) | experimental → estável após L3-1 |
| `jogo-limite-no-jogo-fortnite` | config_jogo | jogo `fortnite`, `limite_fps_definido = true` | `fortnite_frame_rate_limit = { preferencia = "limite_fps" }` (só valores da lista do menu; arredonda para baixo ao valor mais próximo) + DRS off | Epic “How do I adjust my FPS” cita o arquivo [O] | experimental → estável após L3-2 |
| `jogo-limite-no-jogo-rocket-league` | config_jogo | jogo `rocket-league`, `limite_fps_definido = true` | Max FPS do jogo = preferência (valores do menu) + DRS off | Epic “Rocket League doesn't save my settings” cita o arquivo [O] | experimental → estável após L3-4 |
| `jogo-rl-one-frame-thread-lag-off` | config_jogo | jogo `rocket-league` | `OneFrameThreadLag = False` | relato comunitário [C]; opção do menu | experimental (sem promoção nesta fase sem medição L3-4) |
| `jogo-limite-launch-source2` | config_jogo | jogo em `cs2`, `dota2`, `deadlock`, `limite_fps_definido = true` | token `+fps_max <n>` em `LaunchOptions` + DRS off | Valve: opções de inicialização são mecanismo do cliente [O]; `fps_max` é o limitador do motor | experimental → estável após L3-3 |
| `jogo-dota2-fps-max-padrao` | config_jogo | jogo `dota2`, `limite_fps_definido = false`, `refresh_conhecido = true` | `+fps_max = min(refresh, 240)` (o padrão 120 segura máquinas boas) | `competitivo-jogos.md` §12 [C] | experimental → estável após L3-5 |
| `jogo-cs2-reflex-video` | config_jogo | jogo `cs2`, arquivo em `local` | Reflex (chave a confirmar em `cs2_video.txt`) = Ativado | Steam/NVIDIA: Reflex no menu [O] | experimental (só após confirmar chave e Cloud) |

Todas com `impacto_visual = "nenhum"`. Nenhuma regra mexe em qualidade gráfica, modo de
renderização (Fortnite Performance fica como orientação), resolução ou FOV.

### 9.4 Arquitetura, IPC e UI da fase 3

- Armazém `ConfigJogo` no agente, sobre `nvt-jogocfg`; o resolvedor de caminhos roda no
  agente (pastas conhecidas da sessão, OneDrive incluso).
- IPC: motivos da fase 3 (7.6) e `AlvoIpc::ConfigJogo { jogo, arquivo }` já previstos.
- UI: no cartão do jogo, a seção “O que o app fez” ganha “Arquivo do jogo” (caminho
  exibido sem o nome do usuário: `%USERPROFILE%\…`), com o valor antes/depois, o estado
  do backup e o botão de reverter. Mensagens claras para “feche a Steam para aplicar”.

### 9.5 Critérios de aceite da fase 3

1. Ida e volta byte a byte em todas as amostras (F3-01).
2. Nenhuma escrita com jogo/Steam abertos; nenhuma escrita fora das chaves do catálogo.
3. Restauração devolve o arquivo idêntico ao original (hash) quando o jogo não regravou.
4. Valorant, LoL, Marvel Rivals, CoD, Apex, OW2, PUBG e The Finals sem nenhuma escrita
   possível (teste de trava sobre a base).
5. Laboratório L3-1 a L3-6 registrados.

### 9.6 Plano de validação no laboratório (fase 3)

| # | O que | Medir | Sentir | Aprova se |
|---|---|---|---|---|
| L3-1 | R6 `FPSLimit` vs. limite do driver | uso de CPU, frametime p99, entrada→fóton | fluidez em tiroteio | limite no jogo igual ou melhor que o do driver; o jogo mantém o valor após abrir e fechar |
| L3-2 | Fortnite `FrameRateLimit` | idem; valor persiste após partida | — | idem; arquivo idêntico fora da chave |
| L3-3 | CS2 `+fps_max` via `localconfig.vdf` | valor aplicado (`fps_max` no console), Steam Cloud não reverte | — | valor efetivo após abrir a Steam; restauração remove só o token |
| L3-4 | Rocket League Max FPS e One Frame Thread Lag | entrada→fóton e FPS com/sem `OneFrameThreadLag` | resposta do carro | promover só se a latência cair além da variação entre sessões |
| L3-5 | Dota 2 `+fps_max` acima de 120 | FPS, frametime | fluidez da câmera | FPS acima de 120 sem regressão de p99 |
| L3-6 | Anticheat | abrir cada jogo após a edição | — | nenhum erro de integridade (EAC “Untrusted file”, BattlEye, VAC) |

---

## 10. Riscos

| # | Risco | Mitigação |
|---|---|---|
| R1 | Uma fonte (Riot, FACEIT, Valve) mudar de posição | S8: reconferir páginas a cada versão; a base curada permite rebaixar um jogo para `orientar`/`proibido` numa versão menor |
| R2 | Detecção por nome com falso negativo (exe renomeado, launcher novo) → gravação com jogo aberto | lista de anticheat de jogo fixa como rede; falso negativo só afeta o efeito prático (o DRS vale na próxima abertura), não o anticheat; nunca abrir handle para “melhorar” |
| R3 | Corrida entre a checagem de processos e a gravação (jogo abre no intervalo de ms) | aceito; a checagem fica imediatamente antes de cada gravação |
| R4 | `NvAPI_Disp_GetVRRInfo` indisponível/instável em alguns drivers | fato `None`; regra não casa; diagnóstico mostra “VRR desconhecido” |
| R5 | `Hz − Hz²/3600` não bater com o limite automático real em outros refreshes | o mínimo com `Hz − 3` garante a margem da Blur Busters; L1-2 confere a 240 Hz |
| R6 | Migração de estado v1 → v2 corromper histórico | migração testada; cópia do `estado.json` v1 preservada como `estado.v1.json` |
| R7 | `SetDisplayConfig` escolher timing diferente (modos personalizados do Painel NVIDIA, overclock de monitor) | só modos enumerados na resolução atual; releitura; reversão automática em falha; nunca com jogo aberto |
| R8 | Remover ou restaurar `useplatformclock` disparar recuperação do BitLocker | não age (nem na reversão) com BitLocker protegendo; confirmação do usuário; nunca na desinstalação |
| R9 | Agente ausente (desativado no logon, política corporativa) | motivo `AgenteAusente` visível; camada do driver segue funcionando |
| R10 | Desinstalação deixar HKCU/arquivos alterados em outros usuários | F2-11 + aviso na UI; os itens de usuário escolhidos são todos de baixo impacto e fáceis de reverter manualmente |
| R11 | Jogo/Steam Cloud regravar o arquivo e o usuário achar que o app “não funciona” | deriva vira escolha do usuário com mensagem explicando quem mudou |
| R12 | Patch do jogo mudar chaves/formato | parser aborta diante de formato inesperado; chave ausente não é criada |
| R13 | Binário não assinado rodando como SYSTEM e no logon ser visto como suspeito (S7) | assinatura de código passa a ser pré-requisito para lançar a fase 2 (agente no `Run`) |
| R14 | Laboratório único (Turing, 240 Hz) generalizar mal para outras arquiteturas | regras sem condição de arquitetura só quando o mecanismo é independente de GPU; as demais ficam `experimental` fora de Turing |

## 11. Questões em aberto (verificar em máquina real)

1. **Monitor do laboratório**: `regras/base.toml:109` registra “VRR ativo em janela sem
   bordas”, o que indica G-SYNC/G-SYNC Compatible; confirmar no Painel NVIDIA. Sem isso,
   L1-2, L1-3, L1-5 e L1-7 exigem outro monitor.
2. **`NvAPI_Disp_GetVRRInfo`**: disponível na NVAPI pública e no driver 621.40? Campos e
   versão da estrutura (`NV_GET_VRR_INFO_VER1`). Alternativa: ler o DRS `0x10A879CE`
   (VRR Control) só para diagnóstico.
3. **Nomes de executável** marcados com † em 7.3 (Apex DX12, R6 com BattlEye, CoD atual,
   Marvel Rivals, Deadlock) e nomes dos processos de anticheat de F1-06
   (`EasyAntiCheat_EOS.exe`, `BEService_x64.exe`, `start_protected_game.exe`).
4. **Anticheat do Overwatch 2** é de kernel? (afeta só o fato `jogo_anticheat_kernel`).
5. **Limite automático do Ultra em DX12** (L1-3) e **V-Sync do painel em sem bordas**
   (L1-7).
6. **Monitor principal × monitor do jogo** em multimonitor com refreshes diferentes:
   o refresh do principal basta, ou é preciso um limite por jogo?
7. **Elemento BCD** `0x260000F2` (`BcdOSLoaderBoolean_UsePlatformClock`) e GUID de
   `{current}` pelo provedor WMI; se `useplatformclock` faz parte do perfil de validação
   do BitLocker.
8. **DRR (taxa dinâmica do Win11)**: onde ler o estado sem API pública.
9. **`SetDisplayConfig`** com `SDC_ALLOW_CHANGES` vs. modo totalmente fornecido para
   trocar só o refresh; se modos personalizados do Painel NVIDIA aparecem enumerados.
10. **`DirectXUserGlobalSettings`**: padrão do token por build do Win11 (as fontes
    divergem) e se a interface de Gráficos regrava a string inteira.
11. **Caminhos de config**: CS2 `local` × `remote` e se `LaunchOptions` sincroniza pela
    nuvem; Rocket League (cópia na pasta de instalação, chaves exatas de Max FPS);
    R6 (pasta por conta, nome da chave de Reflex); Fortnite (chave de Reflex, valores do
    menu de `FrameRateLimit`); chave de Reflex em `cs2_video.txt`.
12. **Faixas do menu**: Dota 2 (`fps_max` até 240?), Deadlock, Rocket League, Fortnite.
13. **Codificação real** dos arquivos (UTF-16LE em algum INI do Unreal?).
14. **Opt-in de regras experimentais**: o produto precisa de um caminho para o usuário
    aceitar uma sugestão, ou o laboratório basta (hoje se altera o status no TOML e
    compila localmente)? Fora desta especificação até decisão.
15. **Energia por jogo** em notebook na bateria: falta o fato `em_bateria`
    (`GetSystemPowerStatus`); decidir se a regra exige `em_bateria = false`.
16. **Páginas oficiais** de FACEIT, Steam (VAC/Trusted Mode) e Embark que retornaram 403/404
    na pesquisa: reconferir manualmente antes de cada lançamento (S8).
17. **Assinatura de código**: fornecedor e custo, pré-requisito da fase 2 (R13).
18. **NVAPI de exibição na sessão 0**: `NvAPI_DISP_GetDisplayConfig` e
    `NvAPI_Disp_GetVRRInfo` respondem no serviço (LocalSystem) como o DRS responde?
    Se não, a fase 1 precisa de outra fonte de refresh/VRR (antecipar um agente só de
    leitura, ou a UI relatar o fato) — decisão de arquitetura, não de implementação.
19. ~~**Energia por jogo: todos os jogos ou só `jogo_competitivo = true`?**~~
    **Resolvido (decisão do líder): só jogos competitivos.** A regra
    `energia-desempenho-maximo-por-jogo` exige `jogo_competitivo = true`; jogos fora da
    base não recebem a energia nem ganham perfil só por isso, e o global continua
    voltando ao valor de antes da gestão. Mais fiel ao §2, item 2, e reduz consumo.
20. **`gsync-reflex-sem-limite-driver` depende de o usuário ligar o Reflex**, que o app
    não detecta nem liga. Com Reflex desligado em DX12, o jogo fica sem limite algum
    (L1-5). Alternativa: só remover o limite do driver depois de o usuário confirmar a
    orientação “Reflex ligado” (`confirmacao = "usuario"`).
21. **Custo × valor do item `useplatformclock`**: exige WMI/COM, checagem de BitLocker,
    confirmação, reinício e reversão com as mesmas travas, para um caso corretivo raro.
    Alternativa mais barata e sem escrita no BCD: só diagnóstico com orientação. Decidir
    antes de implementar a fase 2.
22. **Corte de escopo sem perda de fluidez percebida** (candidatos): diagnósticos de
    segurança (`vbs_hvci`, `tpm`, `secure_boot`, `iommu`, `windows_11`), que não mexem
    em fluidez; `windows-captura-historica-off` (corretivo, padrão já é `0`);
    `jogo-rl-one-frame-thread-lag-off` (só [C]); `jogo-cs2-reflex-video` (o Reflex já
    está no menu e na orientação; chave e Cloud desconhecidos).
23. **Orientação `modo_performance_opcional` (Fortnite)**: o modo Performance reduz
    folhagem e efeitos; conferir se não colide com o §2, item 5 (vantagem visual). Se
    colidir, retirar o código de orientação.
24. **Regras experimentais não entram no plano** (`docs/regras.md`, “Status”): quase
    toda escrita das fases 2 e 3 nasce `experimental`, então o ganho percebido de cada
    fase depende de o laboratório promover as regras antes do lançamento. Os critérios
    de aceite exigem só o registro do laboratório; decidir se a promoção é condição de
    lançamento.
25. **Padrão de `RESTAURAR_PERFIL`** com três camadas: continua `0` (decisão herdada da
    v0.0.x) ou passa a `1`, já que agora há alterações de Windows e de arquivos?

## 12. Registro de decisões (revisão × relatório)

| Tema | Relatório | Revisão | Decisão |
|---|---|---|---|
| Impacto de `refresh_mhz = None` | “G-SYNC sem limite” | Ultra global já limita | revisão; corrigir a regra só para os casos residuais (7.4) |
| Fórmula do limite | `Hz·3600/(Hz+3600)` | `Hz − Hz²/3600`, piso `Hz − 3` | revisão (224 @240 bate com o observado) |
| Ultra × CPU-bound | “Ultra piora em CPU-bound” | “com FPS limitado”; laboratório contradiz | revisão; Ultra mantido, medição L1-4 |
| Energia | recomendado por jogo | regra global incoerente | revisão; regra por jogo (F1-11) |
| ULLM em OpenGL | “sem efeito” | “não documentado” | revisão (sem condição de API OpenGL nas regras) |
| Números ~16/18/24 ms | atribuídos à Blur Busters | não encontrados | não citados nesta especificação |
| IFEO `PerfOptions` | condicional | fora do escopo | revisão + decisão do líder |
| Taxa de atualização | `ChangeDisplaySettingsExW` | `SetDisplayConfig` | revisão |
| `HistoricalCaptureEnabled` | recomendado | corretivo | revisão (só age se `1`) |
| `GameDVR_Enabled` | espelho de `AppCaptureEnabled` | não escrever | revisão |
| Game Mode | recomendado | efeito misto | revisão; só diagnóstico |
| HAGS | condicional por DLSS FG | critério fraco no competitivo | revisão; diagnóstico, manter padrão |
| MSI mode | “drivers já usam MSI” | sem fonte NVIDIA | revisão; só diagnóstico, texto “segundo relatos” |
| Gravar só sem jogo aberto | exigência de anticheat | política de produto | revisão; regra mantida, justificativa de produto (S2) |
| Toolhelp | snapshot é seguro | só `TH32CS_SNAPPROCESS` | revisão (N2) |
| `PROCESS_QUERY_LIMITED_INFORMATION` | proibido (“visível ao anticheat”) | excesso técnico, defensável | proibição mantida com a justificativa da revisão (N1) |
| `GlobalTimerResolutionRequests` | [O] Microsoft | [C] comunidade | revisão (fora do escopo de qualquer forma) |
| Plano de energia | `powercfg` | sobreposição do Win11, Modern Standby | revisão; só diagnóstico com `PowerGetActualOverlayScheme` |
| Modelo generalizado e IPC de camadas na fase 1 | — | — (decisão do líder) | F1-13 (modelo generalizado de camadas, estado v2) e as variantes de IPC para as camadas Windows/Jogo ficam para o **início da fase 2**: não entregam valor perceptível na fase 1 e tocam quase todo o código. A fase 1 usa IPC v4 só com os campos que ela precisa; a fase 2 sobe para v5 (custo baixo: sem negociação, o MSI instala tudo junto) |
| Fonte do refresh na fase 1 | `QueryDisplayConfig` | — (revisão da especificação) | NVAPI no serviço: as APIs Win32 de modo de vídeo não atendem a sessão 0; Win32 só no agente (fase 2) |
| Reversão de `useplatformclock` | “só remover” | — (revisão da especificação) | “Tudo reversível” exige regravar o original; mesma trava de BitLocker e confirmação (N7, F2-08) |
| `MotivoNaoAplicada::JogoEmExecucao` (7.6) | — | — (decisão do líder) | Não criado: o adiamento (`Estado.adiamento` e respostas `adiada`) o substitui, pois o motor nunca deixa de aplicar uma regra por jogo aberto — ele adia a gravação inteira |
| Critério de aceite 7.8, item 1 | — | — (decisão do líder) | Reescrito: como `sincronismo-gsync-limite` sai experimental (7.4), na fase 1 o limite aparece como sugestão experimental com o valor da tabela (224 a 240 Hz); entra no plano global só após a promoção com L1-2 e L1-3 |
| Casamento da base (F1-03) | — | — (decisão do líder) | Na fase 1, nome do exe + AppID da Steam; ID de outros launchers fica para quando a base trouxer esses IDs |
| Nome do campo de refresh | — | — (decisão do líder) | O código e o IPC usam `refresh_mhz` (milésimos de Hz); a especificação foi alinhada. Os limites de FPS (`limite_vrr`, `refresh_menos`) contam sobre o piso em Hz inteiros (`refresh_hz_piso`); o arredondamento fica só na exibição |
| Falha ao ler a lista de processos | — | — (decisão do líder, commit `ddccbdb`) | Falha fechada: se a lista de processos não puder ser lida, o portão (T-02) adia e a detecção não lê pastas de jogos |
| Jogo fora da base com exe desconhecido | — | — (decisão do líder) | Risco residual aceito: jogo **fora** da base, de exe desconhecido, aberto sem nenhum processo vigiado (exes da base, `processos_protegidos`, `ANTICHEAT_SO_DURANTE_JOGO`), pode ter a pasta lida a partir da segunda varredura. Aceito porque a leitura usa compartilhamento total, sem mapear e sem tocar o processo (F1-08), e jogos com anticheat de kernel ou estão na base ou têm processo EAC/BattlEye vigiado; a alternativa (nunca analisar jogos fora da base) remove valor para muitos jogos |
| Pendência de `RestaurarOriginal` | — | — (correção de revisão) | A pendência `RestaurarOriginal` passa a ser persistida, e a CLI de restauração sai com código ≠ 0 quando adiada |
