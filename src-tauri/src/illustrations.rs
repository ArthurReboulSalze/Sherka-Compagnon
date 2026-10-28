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

fn avatar_reference(
    app: &AppHandle,
    profile: &sherka_core::model::Profile,
) -> sherka_core::Result<Vec<u8>> {
    if let Some(data) = profile
        .avatar
        .as_deref()
        .and_then(sherka_core::storage::image_data_url_bytes)
    {
        return data.map_err(|_| Error::Validation("Le portrait neutre est invalide.".into()));
    }
    let path = profile
        .details
        .avatar_preset
        .map(|p| p.neutral())
        .unwrap_or("/assets/sherka.webp");
    app.asset_resolver()
        .get(path.to_owned())
        .map(|a| a.bytes)
        .ok_or_else(|| {
            Error::Validation("Choisis un portrait neutre pour utiliser IP-Adapter.".into())
        })
}

#[derive(Clone, Serialize)]
pub struct Status {
    pub profile_id: Option<Uuid>,
    pub phase: &'static str,
    pub image: Option<String>,
    pub images: Vec<String>,
    pub message: Option<String>,
    /// The exact prompts of the latest scene, shown and editable in the widget.
    pub prompt: Option<PromptView>,
}
#[derive(Clone, Serialize)]
pub struct PromptView {
    pub positive: String,
    pub negative: String,
    pub edited: bool,
}
/// Final prompts of the latest planned scene. An edited copy replaces scene
/// planning for the same exchange until a new reply arrives.
#[derive(Clone)]
struct Memo {
    profile: Uuid,
    message: String,
    include_companion: bool,
    prompts: sherka_core::illustration::Scene,
    edited: bool,
}
impl Memo {
    fn view(&self) -> PromptView {
        PromptView {
            positive: self.prompts.positive.clone(),
            negative: self.prompts.negative.clone(),
            edited: self.edited,
        }
    }
}
/// An image the user asked for while the generator was off: kept in memory only,
/// until the generator is switched on, the conversation moves on or the
/// companion changes.
#[derive(Default)]
struct Wish(Option<(Uuid, Uuid, String)>);
impl Wish {
    fn keep(&mut self, profile: Uuid, session: Uuid, text: &str) {
        self.0 = Some((profile, session, text.chars().take(2400).collect()));
    }
    fn exists(&self, profile: Uuid, session: Uuid) -> bool {
        self.0
            .as_ref()
            .is_some_and(|(p, s, _)| (*p, *s) == (profile, session))
    }
    /// The request, left in place: a job that is interrupted or restarted
    /// before it draws finds it again.
    fn peek(&self, profile: Uuid, session: Uuid) -> Option<String> {
        self.0
            .as_ref()
            .filter(|(p, s, _)| (*p, *s) == (profile, session))
            .map(|(_, _, text)| text.clone())
    }
    /// The job that used `text` starts drawing: the request is used up, unless
    /// the user has replaced it meanwhile.
    fn release(&mut self, profile: Uuid, session: Uuid, text: &str) {
        if self
            .0
            .as_ref()
            .is_some_and(|(p, s, t)| (*p, *s) == (profile, session) && t == text)
        {
            self.0 = None;
        }
    }
    fn retain_for(&mut self, profile: Uuid, session: Uuid) {
        if !self.exists(profile, session) {
            self.0 = None;
        }
    }
}
type Context = (Uuid, Uuid, Config, sherka_core::connections::Connections);
struct Inner {
    sequence: u64,
    token: CancellationToken,
    status: Status,
    memo: Option<Memo>,
    context: Option<Context>,
    priority: usize,
    /// An illustration was asked for and has not begun to draw: it starts (again)
    /// when the conversation releases its priority.
    pending: bool,
    wish: Wish,
}
impl Inner {
    fn new() -> Self {
        Self {
            sequence: 0,
            token: CancellationToken::new(),
            context: None,
            priority: 0,
            pending: false,
            wish: Wish::default(),
            memo: None,
            status: Status {
                profile_id: None,
                phase: "disabled",
                image: None,
                images: vec![],
                message: None,
                prompt: None,
            },
        }
    }
    /// The job is about to be cancelled by the conversation. One that has not
    /// begun to draw (it waits for ComfyUI to start, or plans the scene) is
    /// asked again when the conversation is quiet again.
    fn interrupt(&mut self) {
        if !self.token.is_cancelled() && self.status.phase == "planning" {
            self.pending = true;
        }
    }
    /// The last conversation lease was released: whether to start a job.
    fn resume(&self) -> bool {
        self.priority == 0
            && (self.pending
                || self
                    .context
                    .as_ref()
                    .is_some_and(|c| c.2.enabled && c.2.live))
    }
    /// A new companion, session, configuration or provider. Returns whether it
    /// changed; a changed context cancels the job and its queued request.
    fn apply_context(&mut self, context: Context) -> bool {
        if self.context.as_ref() == Some(&context) {
            return false;
        }
        let (profile, session, enabled) = (context.0, context.1, context.2.enabled);
        let switched_off = !enabled
            && self
                .context
                .as_ref()
                .is_some_and(|c| c.0 == profile && c.2.enabled);
        self.token.cancel();
        self.sequence += 1;
        self.context = Some(context);
        self.pending = false;
        if switched_off {
            // Turning the generator off withdraws what was asked of it.
            self.wish = Wish::default();
        } else {
            self.wish.retain_for(profile, session);
        }
        if self.memo.as_ref().is_some_and(|m| m.profile != profile) {
            self.memo = None;
        }
        self.status = Status {
            profile_id: Some(profile),
            phase: if enabled { "idle" } else { "disabled" },
            image: None,
            images: vec![],
            message: None,
            prompt: self.memo.as_ref().map(Memo::view),
        };
        true
    }
    /// Cancel the job; `clear` also forgets everything shown and asked.
    fn stop(&mut self, clear: bool) {
        if !clear {
            self.interrupt();
        }
        self.token.cancel();
        self.sequence += 1;
        if clear {
            self.pending = false;
            self.wish = Wish::default();
            self.status.image = None;
            self.status.images.clear();
            self.memo = None;
            self.status.prompt = None;
        }
        self.status.phase = if self.context.as_ref().is_some_and(|c| c.2.enabled) {
            "idle"
        } else {
            "disabled"
        };
        self.status.message = None;
    }
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
            inner.resume()
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
            inner: Arc::new(Mutex::new(Inner::new())),
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
        if let Some(studio) = app.try_state::<crate::character_studio::Studio>() {
            studio.cancel(&app);
        }
        self.studio_priority(app, shared)
    }
    /// An image request waits for the generator to be switched on.
    pub fn keep_wish(&self, profile: Uuid, session: Uuid, text: &str) {
        self.lock().wish.keep(profile, session, text);
    }
    pub fn has_wish(&self, profile: Uuid, session: Uuid) -> bool {
        self.lock().wish.exists(profile, session)
    }
    pub fn forget_wish(&self) {
        self.lock().wish = Wish::default();
    }
    /// The generator was just switched on inside a turn that must not be
    /// illustrated: nothing is resumed when the turn releases its priority.
    pub fn skip_resume(&self) {
        self.lock().pending = false;
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub async fn exclusive(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.gate.clone().lock_owned().await
    }
    pub fn studio_priority(&self, app: AppHandle, shared: Shared) -> Priority {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.priority += 1;
        inner.interrupt();
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
            config,
            profile.details.connections.clone(),
        );
        let changed = self.lock().apply_context(context);
        if changed {
            self.emit(app);
        }
    }
    pub fn cancel(&self, app: &AppHandle, clear: bool) {
        self.lock().stop(clear);
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
        // No pictures during a three-way conversation.
        if shared.0.lock().is_ok_and(|c| c.guest().is_some()) {
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
                    || c.guest().is_some()
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
            // An earlier request waiting for the generator replaces the user's
            // latest message; the companion's latest reply stays the context. It
            // stays queued until the job draws, so that a job interrupted while
            // ComfyUI starts, or while the scene is planned, is not lost.
            let wish = {
                let mut inner = manager.inner.lock().unwrap_or_else(|e| e.into_inner());
                if inner.sequence != sequence || token.is_cancelled() {
                    return;
                }
                if inner.context.as_ref() != Some(&context) || (inner.priority > 0 && config.live) {
                    inner.pending = true;
                    return;
                }
                inner.wish.peek(profile.id, session)
            };
            let asked = wish.as_deref().unwrap_or(&history[0].content);
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
                manager.publish(&app, sequence, "idle", None, None);
                return;
            }
            // An edited prompt for this same exchange replaces scene planning.
            let edited = manager
                .inner
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .memo
                .clone()
                .filter(|m| m.edited && m.profile == profile.id && m.message == last);
            let result=async {
                let (include_companion,prompts)=match edited {
                    Some(memo)=>(memo.include_companion,memo.prompts),
                    None=>{
                        let mut provider=Providers::new(profile.details.connections.clone())?;
                        if !provider.connections.llm_provider.is_local() { let model=provider.remote_model().await?;provider.use_api_model(&model); }
                        let appearance=if config.physical_description.is_empty() {sherka_core::illustration::default_description(profile.details.avatar_preset)} else {&config.physical_description};
                        let (messages,schema)=sherka_core::illustration::subject_request(&profile.details.name,asked,&history[1].content);
                        let subject:sherka_core::illustration::Subject=tokio::select!{biased;_=token.cancelled()=>return Err(Error::StaleOperation),r=provider.structured_budgeted(messages,schema,limit,128)=>r?};
                        let (messages,schema)=sherka_core::illustration::configured_scene_request(&config,if subject.include_companion {appearance}else{""},asked,&history[1].content);
                        let plan:ScenePlan=tokio::select!{biased;_=token.cancelled()=>return Err(Error::StaleOperation),r=provider.structured(messages,schema,limit)=>r.map_err(|e|if e.to_string().contains("mémoire"){Error::Validation("La préparation de la scène a échoué. Vérifie le modèle de conversation puis réessaie.".into())}else{e})?};
                        let prompts=sherka_core::illustration::final_prompts(&config,plan.into_scene(config.model_family),subject.include_companion,None)?;
                        let memo=Memo{profile:profile.id,message:last.clone(),include_companion:subject.include_companion,prompts:prompts.clone(),edited:false};
                        {
                            let mut inner=manager.inner.lock().unwrap_or_else(|e|e.into_inner());
                            if inner.sequence==sequence&&!token.is_cancelled(){inner.status.prompt=Some(memo.view());inner.memo=Some(memo);}
                        }
                        manager.emit(&app);
                        (subject.include_companion,prompts)
                    }
                };
                if !valid() {return Err(Error::StaleOperation);}
                let client=Client::new(&config.server_url)?;
                if config.live {client.require_live_support().await?;}
                let (_,info)=client.catalog(&config).await?;
                let reference=if include_companion && config.reference.enabled {
                    let bytes=avatar_reference(&app,&profile)?;
                    Some(client.upload_reference(&bytes).await?)
                }else{None};
                if let Some(text)=&wish {
                    let mut inner=manager.inner.lock().unwrap_or_else(|e|e.into_inner());
                    if inner.sequence==sequence&&!token.is_cancelled(){inner.wish.release(profile.id,session,text);}
                }
                manager.publish(&app,sequence,"generating",None,None);
                let mut images=vec![];
                // One latent image per job avoids multiplying VRAM use. A batch
                // of variants reuses the scene and advances only owned jobs.
                let mut count=0;
                loop {
                    if !valid(){return Err(Error::StaleOperation);}
                    let graph=sherka_core::image_pipeline::workflow_with_prompts(&config,prompts.clone(),&info,(Uuid::new_v4().as_u128() as u64)&0x7fff_ffff_ffff_ffff,include_companion,reference.as_deref(),false)?;
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
                manager.publish(&app, sequence, "idle", None, None);
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
pub async fn export_illustration_workflow(
    app: AppHandle,
    config: Config,
    transparent: Option<bool>,
) -> Result<bool, String> {
    let config = config.validated().map_err(|e| e.to_string())?;
    let graph = sherka_core::image_pipeline::template(&config, transparent.unwrap_or(false))
        .map_err(|e| e.to_string())?;
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
pub fn get_illustration_workflow(
    config: Config,
    transparent: Option<bool>,
) -> Result<serde_json::Value, String> {
    sherka_core::image_pipeline::template(&config, transparent.unwrap_or(false))
        .map_err(|e| e.to_string())
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
/// Regenerate the latest exchange with prompts edited in the widget. They are
/// sent verbatim and reused (including live variations) until a new reply.
#[tauri::command]
pub fn set_illustration_prompt(
    app: AppHandle,
    shared: State<'_, Shared>,
    state: State<'_, Illustrations>,
    positive: String,
    negative: String,
) -> Result<(), String> {
    crate::windows::ensure_running()?;
    let prompts = sherka_core::illustration::exact_prompts(&positive, &negative)
        .map_err(|e| e.to_string())?;
    {
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Illustrations indisponibles.")?;
        let current = inner.status.profile_id;
        let memo = inner
            .memo
            .as_mut()
            .filter(|m| Some(m.profile) == current)
            .ok_or("Attends la première image de cet échange avant de modifier son prompt.")?;
        memo.prompts = prompts;
        memo.edited = true;
        inner.status.prompt = inner.memo.as_ref().map(Memo::view);
        inner.token.cancel();
    }
    state.request(app, shared.inner().clone());
    Ok(())
}

/// Forget an edited prompt and plan the latest exchange again.
#[tauri::command]
pub fn reset_illustration_prompt(
    app: AppHandle,
    shared: State<'_, Shared>,
    state: State<'_, Illustrations>,
) -> Result<(), String> {
    crate::windows::ensure_running()?;
    {
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Illustrations indisponibles.")?;
        if let Some(memo) = inner.memo.as_mut() {
            memo.edited = false;
        }
        inner.status.prompt = inner.memo.as_ref().map(Memo::view);
        inner.token.cancel();
    }
    state.request(app, shared.inner().clone());
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn context(profile: Uuid, session: Uuid, enabled: bool) -> Context {
        let config = Config {
            enabled,
            ..Default::default()
        };
        (profile, session, config, Default::default())
    }
    /// A job that waits for ComfyUI to start or plans the scene.
    fn planning(inner: &mut Inner) {
        inner.sequence += 1;
        inner.token = CancellationToken::new();
        inner.status.phase = "planning";
    }

    #[test]
    fn a_request_waits_for_its_companion_and_session_and_stays_until_used() {
        let (profile, session, other) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let mut wish = Wish::default();
        assert!(!wish.exists(profile, session));
        wish.keep(profile, session, "un chat astronaute");
        assert!(wish.exists(profile, session));
        assert!(!wish.exists(other, session) && !wish.exists(profile, other));
        assert!(wish.peek(other, session).is_none() && wish.peek(profile, other).is_none());
        // Looking at it does not use it up: an interrupted job finds it again.
        for _ in 0..2 {
            assert_eq!(
                wish.peek(profile, session).as_deref(),
                Some("un chat astronaute")
            );
        }
        wish.release(profile, session, "un chat astronaute");
        assert!(!wish.exists(profile, session) && wish.peek(profile, session).is_none());
    }

    #[test]
    fn a_job_only_uses_up_the_request_it_drew() {
        let (profile, session) = (Uuid::new_v4(), Uuid::new_v4());
        let mut wish = Wish::default();
        wish.keep(profile, session, "a fox");
        wish.release(profile, session, "a deer");
        wish.release(profile, Uuid::new_v4(), "a fox");
        assert!(
            wish.exists(profile, session),
            "another request is untouched"
        );
        wish.keep(profile, session, "a deer");
        wish.release(profile, session, "a fox");
        assert_eq!(wish.peek(profile, session).as_deref(), Some("a deer"));
    }

    #[test]
    fn the_request_follows_the_session_and_the_latest_one_wins() {
        let (profile, session) = (Uuid::new_v4(), Uuid::new_v4());
        let mut wish = Wish::default();
        wish.keep(profile, session, "a fox");
        wish.retain_for(profile, session);
        assert!(wish.exists(profile, session));
        wish.keep(profile, session, "a deer");
        assert_eq!(wish.peek(profile, session).as_deref(), Some("a deer"));
        wish.retain_for(Uuid::new_v4(), session);
        assert!(!wish.exists(profile, session));
    }

    #[test]
    fn a_request_is_bounded() {
        let (profile, session) = (Uuid::new_v4(), Uuid::new_v4());
        let mut wish = Wish::default();
        wish.keep(profile, session, &"é".repeat(5000));
        assert_eq!(wish.peek(profile, session).unwrap().chars().count(), 2400);
    }

    #[test]
    fn a_job_waiting_for_comfyui_is_resumed_when_the_conversation_is_quiet() {
        let (profile, session) = (Uuid::new_v4(), Uuid::new_v4());
        let mut inner = Inner::new();
        assert!(inner.apply_context(context(profile, session, true)));
        inner.wish.keep(profile, session, "un mouton");
        // The turn holds its lease; the job waits for ComfyUI to be ready.
        inner.priority = 1;
        planning(&mut inner);
        // The user speaks again before ComfyUI is ready: the job is cancelled.
        inner.stop(false);
        assert!(inner.pending && inner.token.is_cancelled());
        assert_eq!(inner.status.phase, "idle");
        assert!(!inner.resume(), "not while the conversation has priority");
        inner.priority = 0;
        assert!(inner.resume(), "then it starts again");
        assert_eq!(
            inner.wish.peek(profile, session).as_deref(),
            Some("un mouton"),
            "and still knows what to draw"
        );
    }

    #[test]
    fn a_lease_taken_over_a_waiting_job_flags_it_for_resuming() {
        let mut inner = Inner::new();
        inner.apply_context(context(Uuid::new_v4(), Uuid::new_v4(), true));
        planning(&mut inner);
        // What studio_priority does: flag, then cancel.
        inner.priority += 1;
        inner.interrupt();
        inner.token.cancel();
        assert!(inner.pending && !inner.resume());
        inner.priority -= 1;
        assert!(inner.resume());
    }

    #[test]
    fn only_a_job_that_has_not_begun_to_draw_is_asked_again() {
        let mut inner = Inner::new();
        inner.apply_context(context(Uuid::new_v4(), Uuid::new_v4(), true));
        for phase in ["idle", "generating", "ready", "error", "disabled"] {
            inner.status.phase = phase;
            inner.interrupt();
            assert!(!inner.pending, "{phase}");
        }
        planning(&mut inner);
        inner.token.cancel();
        inner.interrupt();
        assert!(!inner.pending, "an already cancelled job is not revived");
        planning(&mut inner);
        inner.interrupt();
        assert!(inner.pending);
    }

    #[test]
    fn nothing_is_started_when_nothing_asked_for_an_image() {
        let mut inner = Inner::new();
        inner.apply_context(context(Uuid::new_v4(), Uuid::new_v4(), true));
        // Switched on, idle: a conversation going by does not start a job.
        inner.priority = 1;
        inner.interrupt();
        inner.stop(false);
        inner.priority = 0;
        assert!(!inner.pending && !inner.resume());
    }

    #[test]
    fn turning_the_generator_off_or_changing_companion_drops_the_queued_request() {
        let (profile, session) = (Uuid::new_v4(), Uuid::new_v4());
        let mut inner = Inner::new();
        inner.apply_context(context(profile, session, true));
        inner.wish.keep(profile, session, "un mouton");
        planning(&mut inner);
        inner.interrupt();
        // The same settings change nothing.
        assert!(!inner.apply_context(context(profile, session, true)));
        assert!(inner.pending && inner.wish.exists(profile, session));
        // Turned off: the job, its resume flag and the request are gone.
        assert!(inner.apply_context(context(profile, session, false)));
        assert!(inner.token.is_cancelled() && !inner.pending && !inner.resume());
        assert!(!inner.wish.exists(profile, session));
        assert_eq!(inner.status.phase, "disabled");
        // Another companion.
        inner.apply_context(context(profile, session, true));
        inner.wish.keep(profile, session, "un mouton");
        inner.apply_context(context(Uuid::new_v4(), Uuid::new_v4(), true));
        assert!(!inner.wish.exists(profile, session) && !inner.pending);
    }

    #[test]
    fn switching_on_with_a_waiting_request_keeps_it() {
        let (profile, session) = (Uuid::new_v4(), Uuid::new_v4());
        let mut inner = Inner::new();
        inner.apply_context(context(profile, session, false));
        // Asked while off: kept (the waiting request).
        inner.wish.keep(profile, session, "un mouton");
        assert!(inner.apply_context(context(profile, session, true)));
        assert_eq!(
            inner.wish.peek(profile, session).as_deref(),
            Some("un mouton")
        );
        assert_eq!(inner.status.phase, "idle");
        assert!(
            !inner.pending,
            "the reply asks for the image, not the switch"
        );
    }

    #[test]
    fn clearing_forgets_the_request_and_the_resume() {
        let (profile, session) = (Uuid::new_v4(), Uuid::new_v4());
        let mut inner = Inner::new();
        inner.apply_context(context(profile, session, true));
        inner.wish.keep(profile, session, "un mouton");
        planning(&mut inner);
        inner.stop(true);
        assert!(!inner.pending && !inner.wish.exists(profile, session));
        assert!(!inner.resume());
    }

    #[test]
    fn live_mode_resumes_without_a_request() {
        let mut inner = Inner::new();
        let mut live = context(Uuid::new_v4(), Uuid::new_v4(), true);
        live.2.live = true;
        inner.apply_context(live);
        assert!(inner.resume());
        inner.priority = 2;
        assert!(!inner.resume());
    }
}
