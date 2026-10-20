use crate::{Shared, with_core};
use serde::Serialize;
use sherka_core::{
    Error,
    illustration::{Client, Config, ScenePlan},
    providers::Providers,
    storage::MessageRole,
};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
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
    priority: usize,
    pending: bool,
}
/// Conversation/STT/TTS work preempts our image job. Dropping the final lease
/// resumes the newest eligible exchange, including on error/cancellation.
pub struct Priority {
    manager: Illustrations,
    app: AppHandle,
    shared: Shared,
    ready: tokio::sync::OnceCell<()>,
    release: Option<Config>,
}
impl Priority {
    pub async fn ready(&self) {
        self.ready
            .get_or_init(|| async {
                if let Some(config) = &self.release
                    && let Ok(client) = Client::new(&config.server_url)
                {
                    let _gate = self.manager.gate.lock().await;
                    client.release_idle().await;
                }
            })
            .await;
    }
}
impl Drop for Priority {
    fn drop(&mut self) {
        let resume = {
            let mut inner = self.manager.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.priority = inner.priority.saturating_sub(1);
            inner.priority == 0
                && (inner.pending
                    || inner
                        .context
                        .as_ref()
                        .is_some_and(|c| c.2.enabled && c.2.live))
        };
        if resume {
            self.manager.request(self.app.clone(), self.shared.clone());
        }
    }
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
                priority: 0,
                pending: false,
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
    pub fn priority(&self, app: AppHandle, shared: Shared) -> Priority {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.priority += 1;
        inner.token.cancel();
        inner.sequence += 1;
        let release = inner
            .context
            .as_ref()
            .filter(|c| c.2.enabled && c.2.live)
            .map(|c| c.2.clone());
        inner.status.phase = if inner.context.as_ref().is_some_and(|c| c.2.enabled) {
            if inner.context.as_ref().is_some_and(|c| c.2.live) {
                "paused"
            } else {
                "idle"
            }
        } else {
            "disabled"
        };
        drop(inner);
        self.emit(&app);
        Priority {
            manager: self.clone(),
            app,
            shared,
            ready: Default::default(),
            release,
        }
    }
    pub fn synchronize(&self, app: &AppHandle, core: &sherka_core::Companion) {
        let snapshot = core.session.snapshot();
        let Ok(profile) = core.profile(snapshot.profile_id) else {
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
        inner.pending = false;
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
            inner.pending = false;
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
        if crate::windows::ensure_running().is_err() {
            return;
        }
        let (sequence, token) = {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if inner.priority > 0 && inner.context.as_ref().is_some_and(|c| c.2.live) {
                inner.pending = true;
                return;
            }
            inner.pending = false;
            if inner.context.as_ref().is_none_or(|c| !c.2.enabled)
                || (!inner.token.is_cancelled()
                    && matches!(inner.status.phase, "planning" | "generating"))
            {
                return;
            }
            inner.token.cancel();
            inner.sequence += 1;
            inner.token = CancellationToken::new();
            inner.status.phase = "planning";
            (inner.sequence, inner.token.clone())
        };
        self.emit(&app);
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let prepared = with_core(shared.clone(), |c| {
                let session = c.session.snapshot();
                let profile = c.profile(session.profile_id)?;
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
                manager.publish(&app, sequence, "idle", None, None);
                return;
            };
            let config = profile.details.illustration.clone();
            let context = (
                profile.id,
                session,
                config.clone(),
                profile.details.connections.clone(),
            );
            {
                let mut inner = manager.inner.lock().unwrap_or_else(|e| e.into_inner());
                if inner.sequence != sequence || token.is_cancelled() {
                    return;
                }
                if inner.context.as_ref() != Some(&context) || (inner.priority > 0 && config.live) {
                    inner.pending = true;
                    return;
                }
            }
            let startup_config = match shared.0.lock() {
                Ok(core) => core.comfy_startup_config(),
                Err(_) => return,
            };
            let startup = app
                .state::<crate::comfy_startup::ComfyStartup>()
                .inner()
                .clone();
            let ready = tokio::select! { biased; _ = token.cancelled() => return, result = startup.ensure(&config.server_url, &startup_config, &token, true) => result };
            if let Err(error) = ready {
                manager.publish(&app, sequence, "error", None, Some(error));
                return;
            }
            manager.publish(&app, sequence, "planning", None, None);
            // One owned job at a time. Cancellation drains/interrupts this exact
            // job before a high-priority conversation may acquire the gate.
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
                            && c.profile(profile.id).is_ok_and(|p| {
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
                let plan:ScenePlan=tokio::select!{biased;_=token.cancelled()=>return Err(Error::StaleOperation),r=provider.structured(messages,schema,limit)=>r.map_err(|e|if e.to_string().contains("mémoire"){Error::Validation("La préparation de la scène a échoué. Vérifie le modèle de conversation puis réessaie.".into())}else{e})?};
                let scene=plan.into_scene(config.model_family);
                if !valid() {return Err(Error::StaleOperation);}
                let client=Client::new(&config.server_url)?;
                if config.live {client.require_live_support().await?;}
                let (_,info)=client.catalog(&config).await?;
                manager.publish(&app,sequence,"generating",None,None);
                let mut images=vec![];
                // One latent image per job avoids multiplying VRAM use. A batch
                // of variants reuses the scene and advances only owned jobs.
                let mut count=0;
                loop {
                    if !valid(){return Err(Error::StaleOperation);}
                    let graph=sherka_core::illustration::workflow_for_subject(&config,scene.clone(),&info,(Uuid::new_v4().as_u128() as u64)&0x7fff_ffff_ffff_ffff,subject.include_companion)?;
                    let image=client.render(graph,token.clone()).await?;
                    if config.live {images.clear();}
                    images.push(image);
                    if !valid(){return Err(Error::StaleOperation);}
                    manager.publish(&app,sequence,"generating",Some(images.clone()),None);
                    if !config.live {count+=1;}
                    if !config.live && count>=config.image_count {break;}
                    if config.live {tokio::select!{biased;_=token.cancelled()=>{client.release_idle().await;return Err(Error::StaleOperation);},_=tokio::time::sleep(std::time::Duration::from_millis(250))=>{}}}
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
pub async fn export_illustration_workflow(app: AppHandle, config: Config) -> Result<bool, String> {
    let config = config.validated().map_err(|e| e.to_string())?;
    let graph = config
        .workflow
        .unwrap_or_else(sherka_core::illustration::default_workflow);
    sherka_core::illustration::validate_workflow(&graph).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&graph).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let file = app
            .dialog()
            .file()
            .set_title(sherka_core::i18n::text("Exporter le workflow API"))
            .add_filter("ComfyUI API JSON", &["json"])
            .set_file_name("sherka-workflow-api.json")
            .blocking_save_file();
        let Some(file) = file else {
            return Ok(false);
        };
        let path = file
            .into_path()
            .map_err(|_| "Chemin invalide.".to_string())?;
        std::fs::write(path, text).map_err(|_| "L’export du workflow a échoué.".to_string())?;
        Ok(true)
    })
    .await
    .map_err(|_| "L’export du workflow a échoué.".to_string())?
}

#[tauri::command]
pub fn get_illustration(state: State<'_, Illustrations>) -> Status {
    state.status()
}
#[tauri::command]
pub async fn discover_illustration(
    app: AppHandle,
    shared: State<'_, Shared>,
    config: Config,
    start_if_needed: Option<bool>,
) -> Result<sherka_core::illustration::Catalog, String> {
    let config = config.validated().map_err(|e| e.to_string())?;
    if start_if_needed.unwrap_or(false) {
        let startup_config = with_core(shared.inner().clone(), |core| {
            Ok(core.comfy_startup_config())
        })
        .await?;
        app.state::<crate::comfy_startup::ComfyStartup>()
            .ensure(
                &config.server_url,
                &startup_config,
                &CancellationToken::new(),
                true,
            )
            .await?;
    }
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
    crate::comfy_startup::synchronize(app, shared);
    if let Some(images) = app.try_state::<Illustrations>()
        && let Ok(core) = shared.0.lock()
    {
        images.synchronize(app, &core);
        let restart = images.inner.lock().is_ok_and(|i| {
            i.context.as_ref().is_some_and(|c| c.2.enabled && c.2.live)
                && matches!(i.status.phase, "idle" | "disabled")
        });
        drop(core);
        if restart {
            images.request(app.clone(), shared.clone());
        }
    }
}
