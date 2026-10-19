use crate::{Shared, Voice, with_core};
use sherka_core::{
    Error,
    memory::{OptimizationPlan, OptimizationPreview, SummaryPlan},
    providers::Providers,
};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

pub async fn prepare(
    app: &AppHandle,
    shared: Shared,
    voice: &Voice,
    profile_id: Uuid,
) -> Result<OptimizationPreview, String> {
    crate::windows::ensure_running()?;
    let priority = app
        .state::<crate::illustrations::Illustrations>()
        .priority(app.clone(), shared.clone());
    priority.ready().await;
    let gate = voice.gate.lock().await;
    voice.playback.interrupt();
    let (profile, guard, entries, summary, session, token) = with_core(shared.clone(), move |c| {
        let session = c.session.snapshot();
        if session.profile_id != profile_id {
            return Err(Error::StaleOperation);
        }
        let profile = c.store.profile(profile_id)?;
        if profile.details.connections.llm_provider.is_agent() {
            return Err(Error::Validation(
                "Hermes gère sa mémoire sur le serveur.".into(),
            ));
        }
        c.stop_response()?;
        c.optimization = None;
        c.memory_job = tokio_util::sync::CancellationToken::new();
        c.live.notice = Some("Optimisation de la mémoire en cours…".into());
        Ok((
            profile,
            c.store.memory_guard(profile_id)?,
            c.store.optimization_memories(profile_id)?,
            c.store.memories(profile_id, "")?.summary,
            session.session_id,
            c.memory_job.clone(),
        ))
    })
    .await?;
    drop(gate);
    crate::conversation::emit(app, &shared);
    let job = async {
        let mut groups = vec![];
        let mut compact_summary = None;
        if !entries.is_empty() || !summary.is_empty() {
            let models = voice.models.clone();
            let connections = profile.details.connections.clone();
            let valid = token.clone();
            let model = tauri::async_runtime::spawn(async move {
                models
                    .activate(&connections, None, || !valid.is_cancelled())
                    .await
            })
            .await
            .map_err(|_| "Le chargement du modèle a échoué.".to_string())?
            .map_err(|e| e.to_string())?
            .ok_or("Choisis un modèle de conversation dans Compagnons.")?;
            let limit = if profile.details.connections.llm_provider.is_local() {
                model
                    .loaded_context_length
                    .ok_or("Contexte chargé inconnu.")?
            } else {
                model
                    .max_context_length
                    .unwrap_or(sherka_core::connections::FALLBACK_CONTEXT_TOKENS)
            }
            .min(
                profile
                    .details
                    .connections
                    .context_limit
                    .map(u64::from)
                    .unwrap_or(u64::MAX),
            );
            let mut provider =
                Providers::new(profile.details.connections.clone()).map_err(|e| e.to_string())?;
            provider.connections.llm_model = Some(model.id.clone());
            provider.use_api_model(&model);
            let mut start = 0;
            while start < entries.len() {
                let mut end = start + 1;
                let mut bytes = entries[start].draft.content.len() + 200;
                let capacity = limit.saturating_sub(4000).clamp(2000, 4000) as usize;
                while end < entries.len()
                    && end - start < 8
                    && entries[end].approved == entries[start].approved
                    && bytes + entries[end].draft.content.len() + 200 <= capacity
                {
                    bytes += entries[end].draft.content.len() + 200;
                    end += 1;
                }
                let (messages, schema) =
                    sherka_core::memory::optimization_request(&entries[start..end]);
                let plan: OptimizationPlan = provider
                    .structured_budgeted(messages, schema, limit, 3072)
                    .await
                    .map_err(|e| e.to_string())?;
                // A chunk cannot reference ids supplied in another chunk.
                if plan
                    .groups
                    .iter()
                    .flat_map(|g| &g.ids)
                    .any(|id| !entries[start..end].iter().any(|m| m.id == *id))
                {
                    return Err(
                        "La proposition d’optimisation est invalide. Les souvenirs sont conservés."
                            .into(),
                    );
                }
                groups.extend(plan.groups);
                start = end;
            }
            if !summary.is_empty() {
                let (messages, schema) = sherka_core::memory::summary_request(&summary);
                let plan: SummaryPlan = provider
                    .structured_budgeted(messages, schema, limit, 2048)
                    .await
                    .map_err(|e| e.to_string())?;
                compact_summary = Some(plan.summary);
            }
        }
        let check = token.clone();
        with_core(shared.clone(),move|c|{
            let current=c.session.snapshot();
            if check.is_cancelled() || current.profile_id!=profile_id || current.session_id!=session {return Err(Error::StaleOperation);}
            let preview=c.store.optimization_preview(&guard,session,groups,compact_summary)?;
            c.optimization=Some(preview.clone());
            c.live.notice=Some(if preview.changes.is_empty() && preview.summary.is_none(){"La mémoire est déjà concise. Aucun changement proposé."}else{"Optimisation prête à vérifier dans Mémoire. Dis « confirme l’optimisation » ou écris /apply-memory pour l’appliquer."}.into());
            Ok(preview)
        }).await
    };
    let result = tokio::select! {biased;_=token.cancelled()=>Err("Optimisation annulée. Les souvenirs sont conservés.".into()),result=job=>result};
    if let Err(error) = &result {
        let error = error.clone();
        let check = token.clone();
        let _ = with_core(shared.clone(), move |c| {
            let current = c.session.snapshot();
            if !check.is_cancelled()
                && current.profile_id == profile_id
                && current.session_id == session
            {
                c.live.notice = None;
                c.live.error = Some(error);
            }
            Ok(())
        })
        .await;
    }
    crate::conversation::emit(app, &shared);
    if result.is_ok() {
        let _ = app.emit("sherka:memory", profile_id);
    }
    result
}

pub async fn finish(
    app: &AppHandle,
    shared: Shared,
    profile_id: Uuid,
    apply: bool,
) -> Result<(), String> {
    with_core(shared.clone(), move |c| {
        if c.session.snapshot().profile_id != profile_id {
            return Err(Error::StaleOperation);
        }
        c.stop_response()?;
        if apply {
            let preview = c
                .optimization
                .as_ref()
                .ok_or_else(|| Error::Validation("Aucune optimisation à confirmer.".into()))?;
            if preview.guard.profile_id != profile_id
                || preview.session_id != c.session.snapshot().session_id
            {
                return Err(Error::StaleOperation);
            }
            c.store.apply_optimization(preview)?;
        }
        c.memory_job.cancel();
        c.optimization = None;
        c.live.notice = Some(
            if apply {
                "Mémoire optimisée. L’historique de conversation est conservé."
            } else {
                "Optimisation annulée. Les souvenirs sont conservés."
            }
            .into(),
        );
        Ok(())
    })
    .await?;
    crate::conversation::emit(app, &shared);
    let _ = app.emit("sherka:memory", profile_id);
    Ok(())
}

#[tauri::command]
pub async fn optimize_memory(
    app: AppHandle,
    shared: State<'_, Shared>,
    voice: State<'_, Voice>,
    profile_id: Uuid,
) -> Result<OptimizationPreview, String> {
    prepare(&app, shared.inner().clone(), voice.inner(), profile_id).await
}
#[tauri::command]
pub async fn finish_memory_optimization(
    app: AppHandle,
    shared: State<'_, Shared>,
    profile_id: Uuid,
    apply: bool,
) -> Result<(), String> {
    finish(&app, shared.inner().clone(), profile_id, apply).await
}
#[tauri::command]
pub async fn get_memory_optimization(
    shared: State<'_, Shared>,
    profile_id: Uuid,
) -> Result<Option<OptimizationPreview>, String> {
    with_core(shared.inner().clone(), move |c| {
        Ok(c.optimization
            .as_ref()
            .filter(|p| p.guard.profile_id == profile_id)
            .cloned())
    })
    .await
}
