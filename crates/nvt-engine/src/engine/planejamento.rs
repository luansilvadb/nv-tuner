//! Pipeline: resolução de jogos, planejamento do [`Diff`] e aplicação verificada.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use super::*;

/// Chave de perfil equivalente a [`mesmo_perfil`]: `None` é o global e nomes
/// comparam por `to_ascii_lowercase` (o mesmo que `eq_ignore_ascii_case`).
fn chave_perfil(p: &ProfileRef) -> Option<String> {
    match p {
        ProfileRef::Base => None,
        ProfileRef::Named(n) => Some(n.to_ascii_lowercase()),
    }
}

/// Índice (perfil -> id -> valor) do snapshot original seguido dos perfis
/// adicionais; em repetição vale a primeira entrada, como na busca linear.
type IndiceOriginal<'a> = HashMap<Option<String>, HashMap<u32, &'a SettingValue>>;

impl<S: DriverStore> Engine<S> {
    // ── Pipeline ───────────────────────────────────────────────────────────

    /// Acha o perfil de cada jogo pelo exe principal. Jogos que caem no mesmo perfil
    /// ficam só com o primeiro. Jogos com erro (ver [`Engine::resolver_jogos_isolado`])
    /// ficam de fora.
    pub fn resolver_jogos(&self, jogos: &[JogoEntrada]) -> Resultado<Vec<JogoResolvido>> {
        Ok(self.resolver_jogos_isolado(jogos).0)
    }

    /// Como [`Engine::resolver_jogos`], devolvendo também os jogos ignorados. Nome e
    /// executável vindos das bibliotecas são validados/limitados antes de qualquer
    /// chamada à NVAPI; um erro de um jogo nunca derruba os demais.
    pub fn resolver_jogos_isolado(
        &self,
        jogos: &[JogoEntrada],
    ) -> (Vec<JogoResolvido>, Vec<JogoIgnorado>) {
        let mut saida: Vec<JogoResolvido> = Vec::new();
        let mut ignorados = Vec::new();
        let mut nomes_novos: BTreeSet<String> = BTreeSet::new();
        // Perfis já em `saida`, por `to_ascii_lowercase` (= `eq_ignore_ascii_case`).
        let mut vistos: HashSet<String> = HashSet::new();
        for j in jogos {
            let (perfil, origem) = match self.resolver_um(j, &nomes_novos) {
                Ok(r) => r,
                Err(motivo) => {
                    ignorados.push(JogoIgnorado {
                        nome: j.nome.clone(),
                        exe: j.exe.clone(),
                        motivo,
                    });
                    continue;
                }
            };
            if origem == OrigemPerfil::ACriar {
                nomes_novos.insert(perfil.to_lowercase());
            }
            if !vistos.insert(perfil.to_ascii_lowercase()) {
                continue;
            }
            let fatos = fatos_do_jogo(perfil.clone(), j, casar_na_base(&self.base, j));
            saida.push(JogoResolvido {
                entrada: j.clone(),
                perfil,
                origem,
                fatos,
            });
        }
        (saida, ignorados)
    }

    fn resolver_um(
        &self,
        j: &JogoEntrada,
        nomes_novos: &BTreeSet<String>,
    ) -> Result<(String, OrigemPerfil), String> {
        validar_exe(&j.exe)?;
        let encontrado = self
            .store
            .perfil_do_exe(&j.exe)
            .map_err(|e| format!("busca do perfil do executável falhou: {e}"))?;
        if let Some(p) = encontrado {
            let origem = if self.armazem.estado.criado_pelo_app(&p.nome) {
                OrigemPerfil::CriadoPeloApp
            } else if p.predefinido {
                OrigemPerfil::Driver
            } else {
                OrigemPerfil::Usuario
            };
            return Ok((p.nome, origem));
        }
        let base = nome_perfil_seguro(&j.nome, &j.exe);
        let mut nome = base.clone();
        for n in 1..=MAX_SUFIXOS {
            let existe = self
                .store
                .perfil_existe(&nome)
                .map_err(|e| format!("consulta do perfil “{nome}” falhou: {e}"))?;
            let ocupado = (existe && !self.armazem.estado.criado_pelo_app(&nome))
                || nomes_novos.contains(&nome.to_lowercase());
            if !ocupado {
                return Ok((nome, OrigemPerfil::ACriar));
            }
            nome = if n == 1 {
                format!("{base} (nv-tuner)")
            } else {
                format!("{base} (nv-tuner {n})")
            };
        }
        Err(format!("nenhum nome livre para o perfil “{base}”"))
    }

    /// Detecta (capacidades + perfis), avalia e compara com o DRS. Somente leitura.
    ///
    /// Erros de um jogo (resolução ou leitura do perfil dele) só tiram esse jogo do
    /// plano ([`Diff::ignorados`]); o perfil global sempre segue.
    pub fn planejar(&self, jogos: &[JogoEntrada]) -> Resultado<Diff> {
        let caps = self.capacidades()?;
        let (resolvidos, mut ignorados) = self.resolver_jogos_isolado(jogos);
        let fatos: Vec<GameFacts> = resolvidos.iter().map(|r| r.fatos.clone()).collect();
        let prefs = self.armazem.estado.preferencias_efetivas();
        let plano = nvt_core::avaliar(&caps, &fatos, &prefs, &self.regras);

        let mut mudancas = Vec::new();
        let mut conformes = Vec::new();
        let mut criar = Vec::new();

        for m in &plano.global {
            self.classificar(
                Alvo::Global,
                ProfileRef::Base,
                m,
                true,
                &mut mudancas,
                &mut conformes,
            )?;
        }
        let mut validos = Vec::with_capacity(resolvidos.len());
        for r in resolvidos {
            let alvo = Alvo::Jogo(r.perfil.clone());
            let lista = plano.mudancas(&alvo);
            if lista.is_empty() {
                validos.push(r);
                continue;
            }
            let existe = r.origem != OrigemPerfil::ACriar;
            let perfil = ProfileRef::Named(r.perfil.clone());
            let mut m_jogo = Vec::new();
            let mut c_jogo = Vec::new();
            let lido = lista.iter().try_for_each(|m| {
                self.classificar(
                    alvo.clone(),
                    perfil.clone(),
                    m,
                    existe,
                    &mut m_jogo,
                    &mut c_jogo,
                )
            });
            if let Err(e) = lido {
                ignorados.push(JogoIgnorado {
                    nome: r.entrada.nome.clone(),
                    exe: r.entrada.exe.clone(),
                    motivo: format!("leitura do perfil “{}” falhou: {e}", r.perfil),
                });
                continue;
            }
            if !existe {
                criar.push(PerfilCriado {
                    nome: r.perfil.clone(),
                    exe: r.entrada.exe.clone(),
                });
            }
            mudancas.extend(m_jogo);
            conformes.extend(c_jogo);
            validos.push(r);
        }
        let reverter = self.reversoes(&validos, &mudancas, &conformes);
        Ok(Diff {
            capacidades: caps,
            jogos: validos,
            plano,
            criar,
            mudancas,
            conformes,
            reverter,
            ignorados,
        })
    }

    /// Settings vigiadas que o plano não quer mais. Só considera o perfil global e
    /// os perfis dos jogos presentes neste plano: um jogo que sumiu da varredura
    /// (biblioteca desconectada, erro isolado) mantém o que tinha.
    fn reversoes(
        &self,
        jogos: &[JogoResolvido],
        mudancas: &[MudancaPlanejada],
        conformes: &[MudancaPlanejada],
    ) -> Vec<Reversao> {
        let mut saida = Vec::new();
        // Conjuntos pré-calculados com a mesma comparação de `mesmo_perfil`.
        let perfis_jogos: HashSet<String> = jogos
            .iter()
            .map(|j| j.perfil.to_ascii_lowercase())
            .collect();
        let no_plano: HashSet<(Option<String>, u32)> = mudancas
            .iter()
            .chain(conformes)
            .map(|m| (chave_perfil(&m.perfil), m.mudanca.id))
            .collect();
        let mut indice: Option<IndiceOriginal<'_>> = None;
        for e in &self.armazem.estado.esperado {
            let chave = chave_perfil(&e.perfil);
            let considerado = match &chave {
                None => true,
                Some(n) => perfis_jogos.contains(n),
            };
            if !considerado || no_plano.contains(&(chave, e.id)) {
                continue;
            }
            // Erro de leitura: continua vigiada e tenta de novo no próximo ciclo.
            if let Ok(r) = self.reversao_de(e, &mut indice) {
                saida.push(r);
            }
        }
        saida
    }

    fn reversao_de<'a>(
        &'a self,
        e: &Esperado,
        indice: &mut Option<IndiceOriginal<'a>>,
    ) -> Resultado<Reversao> {
        let lido = match self.store.ler_setting(&e.perfil, e.id) {
            Ok(s) => s,
            Err(err) => {
                let err = ErroEngine::from(err);
                if matches!(&err, ErroEngine::Drs(e) if perfil_ausente(e)) {
                    return Ok(Reversao {
                        esperado: e.clone(),
                        atual: None,
                        acao: AcaoReversao::SoDeixarDeVigiar,
                    });
                }
                return Err(err);
            }
        };
        let atual = lido.as_ref().and_then(|s| s.value.as_u32());
        let propria = lido.as_ref().filter(|s| s.is_user_specified());
        // O valor de antes da gestão (registrado na primeira gravação do app)
        // vale mais que o snapshot inicial: o usuário pode ter mudado a
        // setting depois do snapshot e antes de o app passar a geri-la.
        let original = match e.antes_da_gestao {
            Some(ValorAntes::Valor(v)) => Original::Valor(SettingValue::Dword(v)),
            Some(ValorAntes::Inexistente) => Original::Ausente,
            None => self.original_de(&e.perfil, e.id, indice),
        };
        let acao = match original {
            Original::Desconhecido => AcaoReversao::SoDeixarDeVigiar,
            Original::Valor(v) if propria.is_some_and(|s| s.value == v) => {
                AcaoReversao::SoDeixarDeVigiar
            }
            Original::Valor(v) => AcaoReversao::Gravar(v),
            Original::Ausente => match propria {
                None => AcaoReversao::SoDeixarDeVigiar,
                Some(s) => AcaoReversao::Remover {
                    predefinido_valido: s.predefined.is_some(),
                },
            },
        };
        Ok(Reversao {
            esperado: e.clone(),
            atual,
            acao,
        })
    }

    /// Valor de (perfil, id) antes do app: perfil criado pelo app não tinha nada;
    /// senão vale o snapshot original (ou o original do perfil adicional).
    /// O índice é montado na primeira consulta e reaproveitado no resto do
    /// planejamento.
    fn original_de<'a>(
        &'a self,
        perfil: &ProfileRef,
        id: u32,
        indice: &mut Option<IndiceOriginal<'a>>,
    ) -> Original {
        if let ProfileRef::Named(n) = perfil
            && self.armazem.estado.criado_pelo_app(n)
        {
            return Original::Ausente;
        }
        let indice = indice.get_or_insert_with(|| self.indice_original());
        match indice.get(&chave_perfil(perfil)) {
            None => Original::Desconhecido,
            Some(settings) => match settings.get(&id) {
                Some(v) => Original::Valor((*v).clone()),
                None => Original::Ausente,
            },
        }
    }

    fn indice_original(&self) -> IndiceOriginal<'_> {
        let snap = self
            .armazem
            .snapshot_original()
            .into_iter()
            .flat_map(|s| s.profiles.iter());
        let mut indice: IndiceOriginal<'_> = HashMap::new();
        for p in snap.chain(self.armazem.estado.perfis_adicionais.iter()) {
            // Só o primeiro perfil equivalente conta (como o `find` de antes).
            if let Entry::Vacant(v) = indice.entry(chave_perfil(&p.profile)) {
                let mut settings = HashMap::new();
                for s in &p.settings {
                    settings.entry(s.id).or_insert(&s.value);
                }
                v.insert(settings);
            }
        }
        indice
    }

    pub(super) fn registrar(&mut self, r: RegistroHistorico) {
        let limite = self.armazem.limite_historico();
        self.armazem.estado.registrar_historico(r, limite);
    }

    fn classificar(
        &self,
        alvo: Alvo,
        perfil: ProfileRef,
        m: &SettingChange,
        perfil_existe: bool,
        mudancas: &mut Vec<MudancaPlanejada>,
        conformes: &mut Vec<MudancaPlanejada>,
    ) -> Resultado<()> {
        let atual = if perfil_existe {
            self.ler_u32(&perfil, m.id)?
        } else {
            None
        };
        let mut mudanca = m.clone();
        mudanca.valor_anterior = atual;
        let item = MudancaPlanejada {
            alvo,
            perfil,
            mudanca,
        };
        if atual == Some(m.valor) {
            conformes.push(item);
        } else {
            mudancas.push(item);
        }
        Ok(())
    }

    /// Aplica o diff. **Só age com `confirmar = true`.** Valida tudo contra a lista
    /// permitida antes de tocar no driver; captura o snapshot original antes da primeira
    /// mudança; cria perfis; grava; salva; relê para verificar; registra o histórico.
    pub fn aplicar(&mut self, diff: &Diff, confirmar: bool) -> Resultado<Aplicacao> {
        let mut rel = Aplicacao::default();
        if !confirmar || diff.vazio() {
            return Ok(rel);
        }
        self.exigir_gravacao()?;
        for m in &diff.mudancas {
            self.validar(m.mudanca.id, Some(m.mudanca.valor))?;
        }
        // Portão (T-02): antes de qualquer efeito (snapshot, perfis, settings).
        let exes: Vec<&str> = diff.jogos.iter().map(|j| j.entrada.exe.as_str()).collect();
        let licenca = self.licenca(&exes)?;

        rel.snapshot_capturado = self.garantir_snapshot(diff, &exes)?;

        let agora = agora_iso();
        // Perfis de jogo com erro nesta aplicação: o resto deles é pulado.
        let mut falhos: BTreeSet<String> = BTreeSet::new();
        let falho = |f: &BTreeSet<String>, p: &ProfileRef| match p {
            ProfileRef::Base => false,
            ProfileRef::Named(n) => f.contains(&n.to_lowercase()),
        };
        for c in &diff.criar {
            if let Err(e) = self.criar_perfil_e_app(&licenca, c) {
                rel.erros
                    .push(format!("criação do perfil “{}” falhou: {e}", c.nome));
                falhos.insert(c.nome.to_lowercase());
                continue;
            }
            if !self.armazem.estado.criado_pelo_app(&c.nome) {
                self.armazem.estado.perfis_criados.push(c.clone());
            }
            self.registrar(RegistroHistorico {
                data: agora.clone(),
                perfil: c.nome.clone(),
                setting: String::new(),
                id: 0,
                antes: None,
                depois: None,
                origem: OrigemRegistro::CriacaoPerfil,
                motivo: format!(
                    "jogo sem perfil no driver: criado com o executável {}",
                    c.exe
                ),
            });
            rel.perfis_criados.push(c.clone());
        }

        // Valor de antes da gestão, lido antes da primeira gravação do app em
        // cada (perfil, setting) ainda não gerida.
        let antes_da_gestao: Vec<Option<ValorAntes>> = diff
            .mudancas
            .iter()
            .map(|m| self.valor_antes_da_gestao(&m.perfil, m.mudanca.id))
            .collect();
        for m in &diff.mudancas {
            if falho(&falhos, &m.perfil) {
                continue;
            }
            let gravado = self.gravar_com(&licenca, &m.perfil, m.mudanca.id, m.mudanca.valor);
            let n = nome_perfil(&m.perfil);
            let msg =
                |e: &ErroEngine| format!("gravação de {} em “{n}” falhou: {e}", m.mudanca.setting);
            if isolar(&m.perfil, gravado, &mut rel.erros, msg)?.is_none() {
                falhos.insert(n.to_lowercase());
            }
        }
        let mut revertidas: Vec<&Reversao> = Vec::new();
        for r in &diff.reverter {
            let e = &r.esperado;
            if falho(&falhos, &e.perfil) {
                continue;
            }
            let feito = match &r.acao {
                AcaoReversao::Gravar(v) => self.gravar_valor_original(&licenca, &e.perfil, e.id, v),
                AcaoReversao::Remover { predefinido_valido } => {
                    self.remover_permitido(&licenca, &e.perfil, e.id, *predefinido_valido)
                }
                AcaoReversao::SoDeixarDeVigiar => Ok(()),
            };
            let msg = |err: &ErroEngine| {
                format!(
                    "reversão de {} em “{}” falhou: {err}",
                    e.setting,
                    nome_perfil(&e.perfil)
                )
            };
            if isolar(&e.perfil, feito, &mut rel.erros, msg)?.is_some() {
                revertidas.push(r);
            }
        }
        self.store.escrita(&licenca).salvar()?;
        rel.aplicada = true;

        for r in revertidas {
            let e = &r.esperado;
            self.armazem.estado.remover_esperado(&e.perfil, e.id);
            rel.revertidas.push(e.clone());
            if r.acao == AcaoReversao::SoDeixarDeVigiar {
                continue;
            }
            let depois = self.ler_u32(&e.perfil, e.id).unwrap_or(None);
            self.registrar(RegistroHistorico {
                data: agora.clone(),
                perfil: nome_perfil(&e.perfil),
                setting: e.setting.clone(),
                id: e.id,
                antes: r.atual,
                depois,
                origem: OrigemRegistro::Plano {
                    origem: e.origem.clone(),
                },
                motivo: "saiu do plano (preferência, teto de qualidade ou regra): \
                         revertida ao valor original"
                    .into(),
            });
        }

        for (m, antes) in diff.mudancas.iter().zip(&antes_da_gestao) {
            if falho(&falhos, &m.perfil) {
                continue;
            }
            let msg = |e: &ErroEngine| {
                format!(
                    "releitura de {} em “{}” falhou: {e}",
                    m.mudanca.setting,
                    nome_perfil(&m.perfil)
                )
            };
            let lido = self.ler_u32(&m.perfil, m.mudanca.id);
            let Some(lido) = isolar(&m.perfil, lido, &mut rel.erros, msg)? else {
                continue;
            };
            if lido == Some(m.mudanca.valor) {
                self.armazem.estado.registrar_esperado(m.esperado(*antes));
                self.registrar(RegistroHistorico {
                    data: agora.clone(),
                    perfil: nome_perfil(&m.perfil),
                    setting: m.mudanca.setting.clone(),
                    id: m.mudanca.id,
                    antes: m.mudanca.valor_anterior,
                    depois: Some(m.mudanca.valor),
                    origem: OrigemRegistro::Plano {
                        origem: m.mudanca.origem.clone(),
                    },
                    motivo: m.mudanca.explicacao.clone(),
                });
                rel.verificadas.push(m.clone());
            } else {
                rel.falhas.push((m.clone(), lido));
            }
        }
        // Conformes também passam a ser vigiadas pela detecção de deriva.
        for m in &diff.conformes {
            if falho(&falhos, &m.perfil) {
                continue;
            }
            let antes_da_gestao = self.valor_antes_da_gestao(&m.perfil, m.mudanca.id);
            self.armazem
                .estado
                .registrar_esperado(m.esperado(antes_da_gestao));
        }
        self.armazem.estado.driver_visto = Some(self.store.versao_driver()?.raw);
        self.armazem.salvar()?;
        Ok(rel)
    }

    /// Valor de (perfil, id) imediatamente antes de o app passar a geri-la.
    /// Já gerida: o registrado ([`Estado::registrar_esperado`] o preserva).
    /// Perfil criado pelo app ou inexistente: [`ValorAntes::Inexistente`].
    /// Valor próprio do perfil: [`ValorAntes::Valor`]; herdado, predefinido
    /// ou padrão do driver: [`ValorAntes::Inexistente`]. Erro de leitura ou
    /// valor que não é DWORD: `None` (a reversão usa o snapshot).
    fn valor_antes_da_gestao(&self, perfil: &ProfileRef, id: u32) -> Option<ValorAntes> {
        if let Some(e) = self
            .armazem
            .estado
            .esperado
            .iter()
            .find(|e| mesmo_perfil(&e.perfil, perfil) && e.id == id)
        {
            return e.antes_da_gestao;
        }
        if let ProfileRef::Named(n) = perfil
            && self.armazem.estado.criado_pelo_app(n)
        {
            return Some(ValorAntes::Inexistente);
        }
        match self.store.ler_setting(perfil, id) {
            Ok(Some(s)) if s.is_user_specified() => s.value.as_u32().map(ValorAntes::Valor),
            Ok(_) => Some(ValorAntes::Inexistente),
            Err(e) if perfil_ausente(&e) => Some(ValorAntes::Inexistente),
            Err(_) => None,
        }
    }

    /// Cria o perfil (se não existir) e associa o executável (se ainda não estiver nele).
    fn criar_perfil_e_app(&mut self, licenca: &Licenca, c: &PerfilCriado) -> Resultado<()> {
        if !self.store.perfil_existe(&c.nome)? {
            self.store.escrita(licenca).criar_perfil(&c.nome)?;
        }
        let ja_no_perfil = self
            .store
            .perfil_do_exe(&c.exe)?
            .is_some_and(|p| p.nome.eq_ignore_ascii_case(&c.nome));
        if !ja_no_perfil {
            self.store
                .escrita(licenca)
                .criar_aplicacao(&c.nome, &c.exe)?;
        }
        Ok(())
    }

    /// Snapshot original na primeira aplicação; depois, só o original de perfis novos
    /// que ainda não estavam cobertos. Devolve `true` se capturou o original agora.
    fn garantir_snapshot(&mut self, diff: &Diff, exes: &[&str]) -> Resultado<bool> {
        if self.armazem.snapshot_original().is_none() {
            let snap = self.store.capturar(exes)?;
            return self.armazem.guardar_snapshot_original(snap);
        }
        let mut mudou = false;
        for j in &diff.jogos {
            let afetado = diff
                .mudancas
                .iter()
                .any(|m| m.perfil == ProfileRef::Named(j.perfil.clone()));
            if !afetado
                || j.origem == OrigemPerfil::ACriar
                || j.origem == OrigemPerfil::CriadoPeloApp
            {
                continue;
            }
            let r = ProfileRef::Named(j.perfil.clone());
            let coberto = self
                .armazem
                .snapshot_original()
                .is_some_and(|s| s.profiles.iter().any(|p| p.profile == r))
                || self
                    .armazem
                    .estado
                    .perfis_adicionais
                    .iter()
                    .any(|p| p.profile == r);
            if coberto {
                continue;
            }
            let snap = self.store.capturar(&[j.entrada.exe.as_str()])?;
            if let Some(p) = snap.profiles.into_iter().find(|p| p.profile == r) {
                self.armazem.estado.perfis_adicionais.push(p);
                mudou = true;
            }
        }
        if mudou {
            self.armazem.salvar()?;
        }
        Ok(false)
    }
}
