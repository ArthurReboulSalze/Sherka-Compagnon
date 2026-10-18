use crate::{Shared, with_core};
use serde::Serialize;
use sherka_core::{
    Error,
    illustration::{Client, Config, Scene},
    providers::Providers,
    storage::MessageRole,
};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone, Serialize)]
pub struct Status {
    pub profile_id: Option<Uuid>,
    pub phase: &'static str,
    pub image: Option<String>,
    pub images: Vec<String>,
    pub message: Option<String>,
}
struct Inner {
    sequence: u64,
    token: CancellationToken,
    status: Status,
    context: Option<(Uuid, Uuid, Config, sherka_core::connections::Connections)>,
}
#[derive(Clone)]
pub struct Illustrations {
    inner: Arc<Mutex<Inner>>,
    gate: Arc<AsyncMutex<()>>,
}
impl Illustrations {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                sequence: 0,
                token: CancellationToken::new(),
                context: None,
                status: Status {
                    profile_id: None,
                    phase: "disabled",
                    image: None,
                    images: vec![],
                    message: None,
                },
            })),
            gate: Arc::new(AsyncMutex::new(())),
        }
    }
    pub fn status(&self) -> Status {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status
            .clone()
    }
    fn emit(&self, app: &AppHandle) {
        let _ = app.emit("sherka:illustration", self.status());
    }
    pub fn synchronize(&self, app: &AppHandle, core: &sherka_core::Companion) {
        let snapshot = core.session.snapshot();
        let Ok(profile) = core.store.profile(snapshot.profile_id) else {
            return;
        };
        let mut config = profile.details.illustration;
        if profile.details.connections.llm_provider.is_agent() {
            config.enabled = false;
        }
        let context = (
            profile.id,
            snapshot.session_id,
            config.clone(),
            profile.details.connections.clone(),
        );
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner.context.as_ref() == Some(&context) {
            return;
        }
        inner.token.cancel();
        inner.sequence += 1;
        inner.context = Some(context);
        inner.status = Status {
            profile_id: Some(profile.id),
            phase: if config.enabled { "idle" } else { "disabled" },
            image: None,
            images: vec![],
            message: None,
        };
        drop(inner);
        self.emit(app);
    }
    pub fn cancel(&self, app: &AppHandle, clear: bool) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.token.cancel();
        inner.sequence += 1;
        if clear {
            inner.status.image = None;
            inner.status.images.clear();
        }
        inner.status.phase = if inner.context.as_ref().is_some_and(|c| c.2.enabled) {
            "idle"
        } else {
            "disabled"
        };
        inner.status.message = None;
        drop(inner);
        self.emit(app);
    }
    fn publish(
        &self,
        app: &AppHandle,
        sequence: u64,
        phase: &'static str,
        images: Option<Vec<String>>,
        message: Option<String>,
    ) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner.sequence != sequence || inner.token.is_cancelled() {
            return;
        }
        inner.status.phase = phase;
        inner.status.message = message;
        if let Some(images) = images {
            inner.status.image = images.first().cloned();
            inner.status.images = images;
        }
        drop(inner);
        self.emit(app);
    }
    pub fn request(&self, app: AppHandle, shared: Shared) {
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let prepared = with_core(shared.clone(), |c| {
                let session = c.session.snapshot();
                let profile = c.store.profile(session.profile_id)?;
                if profile.details.connections.llm_provider.is_agent()
                    || !profile.details.illustration.enabled
                    || (c.live.tag.is_some() && !c.live.text_complete)
                {
                    return Err(Error::StaleOperation);
                }
                let history = c.store.history(profile.id, 2)?;
                if history.len() != 2
                    || history[0].role != MessageRole::User
                    || history[1].role != MessageRole::Assistant
                {
                    return Err(Error::StaleOperation);
                }
                let limit = c.live.budget.as_ref().map(|b| b.limit).unwrap_or_else(|| {
                    profile.details.connections.context_limit.unwrap_or(8192) as u64
                });
                Ok((profile, session.session_id, history, limit))
            })
            .await;
            let Ok((profile, session, history, limit)) = prepared else {
                return;
            };
            let config = profile.details.illustration.clone();
            let context = (
                profile.id,
                session,
                config.clone(),
                profile.details.connections.clone(),
            );
            let (sequence, token) = {
                let mut inner = manager.inner.lock().unwrap_or_else(|e| e.into_inner());
                if inner.context.as_ref() != Some(&context) {
                    return;
                }
                inner.token.cancel();
                inner.sequence += 1;
                inner.token = CancellationToken::new();
                (inner.sequence, inner.token.clone())
            };
            manager.publish(&app, sequence, "planning", None, None);
            // One owned ComfyUI job at a time, with only the newest pending
            // exchange allowed to pass this gate. Running jobs may drain after
            // cancellation; no global interrupt is sent to another client.
            let gate = tokio::select! {biased;_=token.cancelled()=>return,guard=manager.gate.lock()=>guard};
            let last = history[1].id.clone();
            let valid = || {
                !token.is_cancelled()
                    && shared.0.lock().is_ok_and(|c| {
                        let current = c.session.snapshot();
                        current.profile_id == profile.id
                            && current.session_id == session
                            && (c.live.tag.is_none() || c.live.text_complete)
                            && c.store
                                .history(profile.id, 1)
                                .is_ok_and(|h| h.first().is_some_and(|m| m.id == last))
                            && c.store.profile(profile.id).is_ok_and(|p| {
                                p.details.illustration == config
                                    && p.details.connections == profile.details.connections
                                    && !p.details.connections.llm_provider.is_agent()
                            })
                    })
            };
            if !valid() {
                return;
            }
            let result=async {
                let mut provider=Providers::new(profile.details.connections.clone())?;
                if !provider.connections.llm_provider.is_local() { let model=provider.remote_model().await?;provider.use_api_model(&model); }
                let appearance=if config.physical_description.is_empty() {sherka_core::illustration::default_description(profile.details.avatar_preset)} else {&config.physical_description};
                let (messages,schema)=sherka_core::illustration::subject_request(&profile.details.name,&history[0].content,&history[1].content);
                let subject:sherka_core::illustration::Subject=tokio::select!{biased;_=token.cancelled()=>return Err(Error::StaleOperation),r=provider.structured_budgeted(messages,schema,limit,128)=>r?};
                let (messages,schema)=sherka_core::illustration::configured_scene_request(&config,if subject.include_companion {appearance}else{""},&history[0].content,&history[1].content);
                let scene:Scene=tokio::select!{biased;_=token.cancelled()=>return Err(Error::StaleOperation),r=provider.structured(messages,schema,limit)=>r.map_err(|e|if e.to_string().contains("mémoire"){Error::Validation("La préparation de la scène a échoué. Vérifie le modèle de conversation puis réessaie.".into())}else{e})?};
                if !valid() {return Err(Error::StaleOperation);}
                let client=Client::new(&config.server_url)?;
                let (_,info)=client.catalog(&config).await?;
                manager.publish(&app,sequence,"generating",None,None);
                let mut images=vec![];
                // One latent image per job avoids multiplying VRAM use. A batch
                // of variants reuses the scene and advances only owned jobs.
                for _ in 0..config.image_count {
                    if !valid(){return Err(Error::StaleOperation);}
                    let graph=sherka_core::illustration::workflow(&config,scene.clone(),&info,(Uuid::new_v4().as_u128() as u64)&0x7fff_ffff_ffff_ffff)?;
                    images.push(client.render(graph,token.clone()).await?);
                    if !valid(){return Err(Error::StaleOperation);}
                    manager.publish(&app,sequence,"generating",Some(images.clone()),None);
                }
                Ok(images)
            }.await;
            drop(gate);
            if !valid() {
                return;
            }
            match result {
                Ok(images) => manager.publish(&app, sequence, "ready", Some(images), None),
                Err(Error::StaleOperation) => {}
                Err(error) => {
                    manager.publish(&app, sequence, "error", None, Some(error.to_string()))
                }
            }
        });
    }
}

#[tauri::command]
pub fn get_illustration(state: State<'_, Illustrations>) -> Status {
    state.status()
}
#[tauri::command]
pub async fn discover_illustration(
    config: Config,
) -> Result<sherka_core::illustration::Catalog, String> {
    let config = config.validated().map_err(|e| e.to_string())?;
    Client::new(&config.server_url)
        .map_err(|e| e.to_string())?
        .catalog(&config)
        .await
        .map(|v| v.0)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn refresh_illustration(
    app: AppHandle,
    shared: State<'_, Shared>,
    state: State<'_, Illustrations>,
) -> Result<(), String> {
    crate::windows::ensure_running()?;
    state.request(app, shared.inner().clone());
    Ok(())
}

pub fn synchronize(app: &AppHandle, shared: &Shared) {
    if let Some(images) = app.try_state::<Illustrations>()
        && let Ok(core) = shared.0.lock()
    {
        images.synchronize(app, &core);
    }
}
