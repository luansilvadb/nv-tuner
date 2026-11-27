//! Plano de restauração e volta ao snapshot original.

use super::*;

impl<S: DriverStore> Engine<S> {
    // ── Restauração ────────────────────────────────────────────────────────

    /// Plano para voltar ao snapshot original (`None` se ainda não há snapshot).
    /// Inclui os originais de perfis adicionais e limpa as settings do catálogo nos
    /// perfis criados pelo app. Somente leitura.
    ///
    /// Snapshot original ilegível ([`Armazem::snapshot_indisponivel`]): erro com o
    /// motivo; a restauração fica desabilitada até alguém conferir o arquivo.
    pub fn plano_restauracao(&self) -> Resultado<Option<Restauracao>> {
        if let Some(motivo) = self.armazem.snapshot_indisponivel() {
            return Err(ErroEngine::Io(format!("restauração recusada: {motivo}")));
        }
        let Some(original) = self.armazem.snapshot_original() else {
            return Ok(None);
        };
        let mut alvo = original.clone();
        for p in &self.armazem.estado.perfis_adicionais {
            if !alvo.profiles.iter().any(|x| x.profile == p.profile) {
                alvo.profiles.push(p.clone());
            }
        }
        let mut exes: Vec<String> = Vec::new();
        for p in &alvo.profiles {
            for a in &p.applications {
                if !exes.iter().any(|e| e.eq_ignore_ascii_case(a)) {
                    exes.push(a.clone());
                }
            }
        }
        let refs: Vec<&str> = exes.iter().map(String::as_str).collect();
        let atual = self.store.capturar(&refs)?;
        let plano = nvt_drs::restore_plan(&alvo, &atual);

        let mut ops = Vec::new();
        let mut ignoradas = Vec::new();
        for op in plano.ops {
            let id = match &op {
                RestoreOp::Set { id, .. } | RestoreOp::Reset { id, .. } => Some(*id),
                _ => None,
            };
            match id {
                Some(id) if self.validar(id, None).is_err() => ignoradas.push(op),
                _ => ops.push(op),
            }
        }
        for c in &self.armazem.estado.perfis_criados {
            let r = ProfileRef::Named(c.nome.clone());
            let settings = match self.store.settings_do_perfil(&r) {
                Ok(s) => s,
                Err(_) => continue,
            };
            for s in settings {
                if s.is_user_specified() && self.validar(s.id, None).is_ok() {
                    ops.push(RestoreOp::Reset {
                        profile: r.clone(),
                        id: s.id,
                        predefined_valid: false,
                    });
                }
            }
        }
        Ok(Some(Restauracao {
            ops,
            ignoradas,
            aplicada: false,
        }))
    }

    /// Restaura o snapshot original ("voltar tudo ao original"). **Só age com
    /// `confirmar = true`.** Diferente da reversão de uma setting que sai do
    /// plano (que volta ao valor de antes da gestão daquela setting), aqui vale
    /// sempre o snapshot inicial: o driver volta a como estava antes da primeira
    /// aplicação (ou à base pós-quarentena, ver [`Armazem::base_pos_quarentena`]).
    /// O estado esperado
    /// é zerado (nada mais é gerido até o próximo ciclo); as escolhas travadas ficam.
    pub fn restaurar(&mut self, confirmar: bool) -> Resultado<Option<Restauracao>> {
        let Some(mut r) = self.plano_restauracao()? else {
            return Ok(None);
        };
        if !confirmar || r.ops.is_empty() {
            return Ok(Some(r));
        }
        let licenca = self.licenca(&[])?;
        let agora = agora_iso();
        let mut registros = Vec::new();
        for op in &r.ops {
            match op {
                RestoreOp::CreateProfile { name } => {
                    self.store.escrita(&licenca).criar_perfil(name)?;
                }
                RestoreOp::CreateApplication { profile, exe } => {
                    self.store
                        .escrita(&licenca)
                        .criar_aplicacao(&nome_perfil(profile), exe)?;
                }
                RestoreOp::Set { profile, id, value } => {
                    let antes = self.ler_u32(profile, *id)?;
                    self.gravar_valor_original(&licenca, profile, *id, value)?;
                    registros.push((profile.clone(), *id, antes, value.as_u32()));
                }
                RestoreOp::Reset {
                    profile,
                    id,
                    predefined_valid,
                } => {
                    let antes = self.ler_u32(profile, *id)?;
                    self.remover_permitido(&licenca, profile, *id, *predefined_valid)?;
                    let depois = self.ler_u32(profile, *id)?;
                    registros.push((profile.clone(), *id, antes, depois));
                }
            }
        }
        self.store.escrita(&licenca).salvar()?;
        for (perfil, id, antes, depois) in registros {
            let setting = self.nome_setting(id);
            self.registrar(RegistroHistorico {
                data: agora.clone(),
                perfil: nome_perfil(&perfil),
                setting,
                id,
                antes,
                depois,
                origem: OrigemRegistro::Restauracao,
                motivo: "restauração do snapshot original".into(),
            });
        }
        self.armazem.estado.esperado.clear();
        self.armazem.salvar()?;
        r.aplicada = true;
        Ok(Some(r))
    }
}
