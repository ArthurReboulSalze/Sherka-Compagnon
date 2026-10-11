//! Serial model handover. Only the former companion's model and instances used
//! by this manager are retired; unrelated LM Studio work is left alone.
use crate::{
    Error, Result, catalogs::LanguageModel, connections::Connections, providers::Providers,
};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct ModelManager {
    gate: Mutex<Option<Connections>>,
    closing: AtomicBool,
}
pub fn selected<'a>(models: &'a [LanguageModel], requested: &str) -> Result<&'a LanguageModel> {
    if let Some(model) = models.iter().find(|m| m.id == requested) {
        return Ok(model);
    }
    let matches: Vec<_> = models.iter().filter(|m| m.key == requested).collect();
    match matches.as_slice() {
        [model]=>Ok(model),
        []=>Err(Error::Validation("Le modèle choisi n'est plus disponible dans LM Studio. Vérifie ce profil.".into())),
        _=>Err(Error::Validation("Plusieurs instances de ce modèle sont chargées. Choisis l'instance voulue dans les réglages.".into())),
    }
}
impl ModelManager {
    pub async fn activate(
        &self,
        next: &Connections,
        previous: Option<&Connections>,
        current: impl Fn() -> bool,
    ) -> Result<Option<LanguageModel>> {
        self.reconcile(next, previous, || {
            !self.closing.load(Ordering::SeqCst) && current()
        })
        .await
    }

    /// Wait for any in-flight load to finish, then retire both the tracked
    /// instance and the active preset (also covers quitting before the first turn).
    /// Closing blocks queued activations from reloading a model after cleanup.
    pub async fn shutdown(&self, active: &Connections) -> Result<()> {
        self.closing.store(true, Ordering::SeqCst);
        let mut empty = active.clone();
        empty.llm_model = None;
        empty.llm_model_key = None;
        let result = self
            .reconcile(&empty, Some(active), || true)
            .await
            .map(|_| ());
        if result.is_err() {
            self.closing.store(false, Ordering::SeqCst);
        }
        result
    }

    async fn reconcile(
        &self,
        next: &Connections,
        previous: Option<&Connections>,
        current: impl Fn() -> bool,
    ) -> Result<Option<LanguageModel>> {
        let mut owned = self.gate.lock().await;
        if !current() {
            return Err(Error::StaleOperation);
        }
        let provider = Providers::new(next.clone())?;
        let catalog = if next.llm_provider.is_local() && next.llm_model.is_some() {
            Some(provider.catalog().await?)
        } else {
            None
        };
        let target = match (&catalog, &next.llm_model) {
            (Some(c), Some(id)) => Some(
                selected(&c.models, id)
                    .or_else(|e| match &next.llm_model_key {
                        Some(key) => selected(&c.models, key),
                        None => Err(e),
                    })?
                    .clone(),
            ),
            _ if !next.llm_provider.is_local() && next.llm_model.is_some() => {
                Some(provider.remote_model().await?)
            }
            _ => None,
        };
        // On app restart the selected old profile is our only provenance.
        // During a rapid switch, 'owned' also catches a load that finished
        // after its original turn/profile became stale.
        let mut old = vec![];
        if let Some(c) = owned.as_ref() {
            old.push(c.clone());
        }
        if let Some(c) = previous.filter(|c| c.llm_provider.is_local())
            && !old.iter().any(|x| {
                x.lm_studio_url == c.lm_studio_url
                    && (x.llm_model == c.llm_model
                        || (c.llm_model.is_some() && x.llm_model_key == c.llm_model))
            })
        {
            old.push(c.clone());
        }
        // Resolve every candidate before unloading anything. Re-resolving a
        // key after its instance disappears could select another app's alias.
        let mut retired: Vec<(Providers, String)> = Vec::new();
        for c in old {
            let Some(ref id) = c.llm_model else {
                continue;
            };
            if next.llm_provider.is_local()
                && c.lm_studio_url == next.lm_studio_url
                && target.as_ref().is_some_and(|m| &m.id == id || &m.key == id)
            {
                continue;
            }
            let p = Providers::new(c.clone())?;
            let list = p.catalog().await?;
            if !current() {
                return Err(Error::StaleOperation);
            }
            let old_id = if list.models.iter().any(|m| &m.id == id || &m.key == id) {
                id
            } else {
                c.llm_model_key.as_ref().unwrap_or(id)
            };
            let candidates: Vec<_> = list
                .models
                .iter()
                .filter(|m| m.loaded == Some(true) && (&m.id == old_id || &m.key == old_id))
                .collect();
            // A saved alias names an exact instance. A bare key is accepted
            // only when it resolves unambiguously.
            let candidates: Vec<_> = if candidates.iter().any(|m| &m.id == id) {
                candidates.into_iter().filter(|m| &m.id == id).collect()
            } else {
                candidates
            };
            if candidates.len() > 1 {
                return Err(Error::Validation("Impossible d'identifier l'ancienne instance à décharger ; choisis son identifiant précis dans LM Studio.".into()));
            }
            for m in candidates {
                if next.llm_provider.is_local()
                    && c.lm_studio_url == next.lm_studio_url
                    && target.as_ref().is_some_and(|target| target.id == m.id)
                {
                    continue;
                }
                if !retired.iter().any(|(provider, id)| {
                    provider.connections.lm_studio_url == c.lm_studio_url && id == &m.id
                }) {
                    retired.push((p.clone(), m.id.clone()));
                }
            }
        }
        for (p, id) in retired {
            if !current() {
                return Err(Error::StaleOperation);
            }
            p.unload_model(&id).await?;
            if p.catalog()
                .await?
                .models
                .iter()
                .any(|x| x.id == id && x.loaded == Some(true))
            {
                return Err(Error::Validation("LM Studio n'a pas confirmé le déchargement de l'ancien modèle. Le nouveau n'a pas été chargé.".into()));
            }
        }
        if !current() {
            return Err(Error::StaleOperation);
        }
        let Some(mut model) = target else {
            *owned = None;
            return Ok(None);
        };
        if !next.llm_provider.is_local() {
            *owned = None;
            return Ok(Some(model));
        }
        let mut tracking = next.clone();
        tracking.llm_model = Some(model.key.clone());
        tracking.llm_model_key = Some(model.key.clone());
        // Write ownership before awaiting load; cancellation cannot orphan it.
        *owned = Some(tracking);
        if model.loaded != Some(true) {
            provider.load_model(&model.key).await?;
            model = selected(&provider.catalog().await?.models, &model.key)?.clone();
        }
        if model.loaded != Some(true) || model.loaded_context_length.is_none() {
            return Err(Error::Validation(
                "LM Studio n'a pas confirmé le modèle et son contexte chargé.".into(),
            ));
        }
        if let Some(c) = owned.as_mut() {
            c.llm_model = Some(model.id.clone());
        }
        if !current() {
            return Err(Error::StaleOperation);
        }
        Ok(Some(model))
    }
}
