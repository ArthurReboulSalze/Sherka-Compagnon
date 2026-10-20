use crate::{Shared, illustrations::Illustrations, with_core};
use serde::Serialize;
use sherka_core::{
    avatar::Emotion,
    character_studio::{Draft, EMOTIONS},
    i18n::Language,
    illustration::Client,
    providers::Providers,
    studio_workflows::{self, Config, Stage},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone, Serialize, Default)]
pub struct Status {
    pub sequence: u64,
    pub revision: u64,
    pub busy: bool,
    pub phase: String,
    pub source_profile: Option<Uuid>,
    pub draft: Option<Draft>,
    pub config: Option<Config>,
    pub images: BTreeMap<Emotion, String>,
    pub transparent_images: BTreeMap<Emotion, String>,
    pub neutral_validated: bool,
    pub error: Option<String>,
    pub created_profile: Option<Uuid>,
}
#[derive(Default)]
struct Inner {
    status: Status,
    token: CancellationToken,
}
#[derive(Clone, Default)]
pub struct Studio(Arc<Mutex<Inner>>);
impl Studio {
    fn status(&self) -> Status {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status
            .clone()
    }
    fn emit(&self, app: &AppHandle) {
        let _ = app.emit_to("panel", "sherka:character-studio", self.status());
    }
    pub fn cancel(&self, app: &AppHandle) {
        let mut inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !inner.status.busy || inner.status.phase == "publishing" {
            return;
        }
        inner.token.cancel();
        inner.status.revision += 1;
        inner.status.sequence += 1;
        inner.status.busy = false;
        inner.status.phase = "stopped".into();
        drop(inner);
        self.emit(app);
    }
    fn begin(&self, app: &AppHandle, phase: &str) -> Result<(u64, CancellationToken), String> {
        crate::windows::ensure_running()?;
        if phase != "publishing" {
            ensure_conversation_idle(app)?;
        }
        let mut inner = self.0.lock().map_err(|_| "Studio indisponible.")?;
        if inner.status.busy {
            return Err("Une génération est déjà en cours.".into());
        }
        inner.token.cancel();
        inner.token = CancellationToken::new();
        inner.status.revision += 1;
        inner.status.sequence += 1;
        inner.status.busy = true;
        inner.status.error = None;
        inner.status.phase = phase.into();
        let run = (inner.status.revision, inner.token.clone());
        drop(inner);
        self.emit(app);
        Ok(run)
    }
    fn update(&self, app: &AppHandle, revision: u64, f: impl FnOnce(&mut Status)) {
        let mut inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if inner.status.revision != revision || inner.token.is_cancelled() {
            return;
        }
        f(&mut inner.status);
        inner.status.sequence += 1;
        drop(inner);
        self.emit(app);
    }
    fn finish(&self, app: &AppHandle, revision: u64, result: Result<(), String>) {
        self.update(app, revision, |s| {
            s.busy = false;
            if let Err(error) = result {
                s.error = Some(error);
                s.phase = "error".into();
            } else {
                s.phase = "ready".into();
            }
        });
    }
}
fn ensure_conversation_idle(app: &AppHandle) -> Result<(), String> {
    use sherka_core::session::{ResponseState, TranscriptionState};
    let state = crate::conversation::live(app.state::<Shared>().inner())?.session;
    if state.response != ResponseState::Idle || state.transcription != TranscriptionState::Idle {
        return Err("Attends la fin de l’échange avant de lancer la création.".into());
    }
    Ok(())
}
#[tauri::command]
pub fn get_character_studio(studio: State<'_, Studio>) -> Status {
    studio.status()
}
#[tauri::command]
pub fn cancel_character_studio(app: AppHandle, studio: State<'_, Studio>) {
    studio.cancel(&app);
}

#[tauri::command]
pub async fn plan_character(
    app: AppHandle,
    shared: State<'_, Shared>,
    studio: State<'_, Studio>,
    source_profile: Uuid,
    concept: String,
    language: Language,
    config: Config,
) -> Result<(), String> {
    let config = config.validated().map_err(|e| e.to_string())?;
    let (messages, schema) =
        sherka_core::character_studio::request(&concept, language, config.model_family)
            .map_err(|e| e.to_string())?;
    let source = with_core(shared.inner().clone(), move |c| c.profile(source_profile)).await?;
    if source.details.connections.llm_provider.is_agent() {
        return Err(
            "Choisis un compagnon classique avec un modèle connecté pour créer un personnage."
                .into(),
        );
    }
    let (revision, token) = studio.begin(&app, "planning")?;
    let studio = studio.inner().clone();
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn(async move {
        let result=async {
            let images=app.state::<Illustrations>().inner().clone();
            let _priority=images.studio_priority(app.clone(),shared);
            _priority.ready().await;
            let _gate=tokio::select!{biased;_=token.cancelled()=>return Err("Création interrompue.".into()),g=images.exclusive()=>g};
            ensure_conversation_idle(&app)?;
            let mut provider=Providers::new(source.details.connections.clone()).map_err(|e|e.to_string())?;
            let manager=app.state::<crate::conversation::Voice>().models.clone();
            let model=manager.activate(&provider.connections,None,||!token.is_cancelled())
                .await.map_err(|e|e.to_string())?.ok_or("Choisis un modèle de conversation dans Compagnons.")?;
            if provider.connections.llm_provider.is_local(){provider.connections.llm_model=Some(model.id.clone());}
            else{provider.use_api_model(&model);}
            let available=model.loaded_context_length.or(model.max_context_length).unwrap_or(64000);
            let limit=provider.connections.context_limit.map(u64::from).unwrap_or(available).min(available);
            let draft:Draft=tokio::select!{biased;_=token.cancelled()=>return Err("Création interrompue.".into()),r=provider.structured_budgeted(messages,schema,limit,3072)=>r.map_err(|e|e.to_string())?};
            draft.validate().map_err(|e|e.to_string())?;
            if draft.language!=language {return Err("La langue de la fiche générée est invalide.".into());}
            studio.update(&app,revision,|s|{s.source_profile=Some(source_profile);s.created_profile=None;s.draft=Some(draft);s.config=Some(config);s.images.clear();s.transparent_images.clear();s.neutral_validated=false;});
            Ok(())
        }.await;
        studio.finish(&app, revision, result);
    });
    Ok(())
}

#[tauri::command]
pub async fn discover_character_models(
    app: AppHandle,
    shared: State<'_, Shared>,
    server_url: String,
) -> Result<studio_workflows::Catalog, String> {
    let config = Config {
        server_url,
        ..Default::default()
    }
    .validated()
    .map_err(|e| e.to_string())?;
    let startup = with_core(shared.inner().clone(), |c| Ok(c.comfy_startup_config())).await?;
    let service = app
        .state::<crate::comfy_startup::ComfyStartup>()
        .inner()
        .clone();
    service
        .ensure(
            &config.server_url,
            &startup,
            &CancellationToken::new(),
            true,
        )
        .await?;
    let client = Client::new(&config.server_url).map_err(|e| e.to_string())?;
    Ok(studio_workflows::catalog(&client)
        .await
        .map_err(|e| e.to_string())?
        .0)
}
#[tauri::command]
pub fn get_character_workflow(stage: Stage) -> serde_json::Value {
    studio_workflows::template(stage)
}

fn portrait_matches(status: &Status, draft: &Draft, config: &Config) -> bool {
    status.neutral_validated
        && status
            .draft
            .as_ref()
            .is_some_and(|d| studio_workflows::same_portrait(d, draft))
        && status
            .config
            .as_ref()
            .is_some_and(|c| c.neutral_matches(config))
}
fn moods_match(status: &Status, draft: &Draft, config: &Config) -> bool {
    portrait_matches(status, draft, config)
        && status
            .config
            .as_ref()
            .is_some_and(|c| c.mood_matches(config))
        && status
            .draft
            .as_ref()
            .is_some_and(|d| d.emotion_prompts == draft.emotion_prompts)
}

#[tauri::command]
pub async fn generate_character(
    app: AppHandle,
    shared: State<'_, Shared>,
    studio: State<'_, Studio>,
    draft: Draft,
    config: Config,
    emotion: Option<Emotion>,
) -> Result<(), String> {
    draft.validate().map_err(|e| e.to_string())?;
    let config = config.validated().map_err(|e| e.to_string())?;
    let mut before = studio.status();
    if before.source_profile.is_none() || before.created_profile.is_some() {
        return Err("Prépare une nouvelle fiche avant de générer le personnage.".into());
    }
    let neutral = emotion == Some(Emotion::Neutral);
    if !neutral && !portrait_matches(&before, &draft, &config) {
        return Err("Valide le portrait neutre correspondant à cette fiche et à ces réglages avant de générer les émotions.".into());
    }
    if neutral {
        before.images.clear();
        before.transparent_images.clear();
        before.neutral_validated = false;
    } else {
        for e in &EMOTIONS[1..] {
            if !before
                .config
                .as_ref()
                .is_some_and(|c| c.mood_matches(&config))
                || before
                    .draft
                    .as_ref()
                    .is_none_or(|d| d.emotion_prompts.get(e) != draft.emotion_prompts.get(e))
            {
                before.images.remove(e);
                before.transparent_images.remove(e);
            }
        }
    }
    let jobs = emotion.map_or_else(
        || {
            EMOTIONS[1..]
                .iter()
                .copied()
                .filter(|e| !before.images.contains_key(e))
                .collect()
        },
        |e| vec![e],
    );
    let startup = with_core(shared.inner().clone(), |c| Ok(c.comfy_startup_config())).await?;
    let (revision, token) = studio.begin(&app, if neutral { "neutral" } else { "moods" })?;
    studio.update(&app, revision, |s| {
        s.draft = Some(draft.clone());
        s.config = Some(config.clone());
        s.images = before.images;
        s.transparent_images = before.transparent_images;
        s.neutral_validated = before.neutral_validated;
    });
    render_images(
        app,
        shared.inner().clone(),
        studio.inner().clone(),
        revision,
        token,
        draft,
        config,
        if neutral { Stage::Neutral } else { Stage::Mood },
        jobs,
        startup,
    );
    Ok(())
}

#[tauri::command]
pub async fn process_character_transparency(
    app: AppHandle,
    shared: State<'_, Shared>,
    studio: State<'_, Studio>,
    draft: Draft,
    config: Config,
) -> Result<(), String> {
    draft.validate().map_err(|e| e.to_string())?;
    let config = config.validated().map_err(|e| e.to_string())?;
    let before = studio.status();
    if before.created_profile.is_some()
        || !moods_match(&before, &draft, &config)
        || EMOTIONS.iter().any(|e| !before.images.contains_key(e))
    {
        return Err(
            "Valide le portrait neutre et génère les six émotions avant le détourage.".into(),
        );
    }
    let startup = with_core(shared.inner().clone(), |c| Ok(c.comfy_startup_config())).await?;
    let (revision, token) = studio.begin(&app, "transparency")?;
    let jobs = EMOTIONS
        .into_iter()
        .filter(|e| !before.transparent_images.contains_key(e))
        .collect();
    studio.update(&app, revision, |s| {
        s.draft = Some(draft.clone());
        s.config = Some(config.clone());
    });
    render_images(
        app,
        shared.inner().clone(),
        studio.inner().clone(),
        revision,
        token,
        draft,
        config,
        Stage::Transparency,
        jobs,
        startup,
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_images(
    app: AppHandle,
    shared: Shared,
    studio: Studio,
    revision: u64,
    token: CancellationToken,
    draft: Draft,
    config: Config,
    stage: Stage,
    jobs: Vec<Emotion>,
    startup: sherka_core::model::ComfyStartupConfig,
) {
    tauri::async_runtime::spawn(async move {
        let result=async {
            let images=app.state::<Illustrations>().inner().clone();
            let _priority=images.studio_priority(app.clone(),shared);
            _priority.ready().await;
            let _gate=tokio::select!{biased;_=token.cancelled()=>return Err("Création interrompue.".into()),g=images.exclusive()=>g};
            ensure_conversation_idle(&app)?;
            let service=app.state::<crate::comfy_startup::ComfyStartup>().inner().clone();
            service.ensure(&config.server_url,&startup,&token,true).await?;
            let client=Client::new(&config.server_url).map_err(|e|e.to_string())?;
            let (_,info)=studio_workflows::catalog(&client).await.map_err(|e|e.to_string())?;
            let raw=studio.status().images;
            let reference=if stage==Stage::Mood {
                let bytes=sherka_core::character_studio::portrait_png(raw.get(&Emotion::Neutral).ok_or("Portrait absent.")?).map_err(|e|e.to_string())?;
                Some(client.upload_image(&bytes).await.map_err(|e|e.to_string())?)
            }else{None};
            for emotion in jobs {
                if token.is_cancelled(){return Err("Création interrompue.".into());}
                studio.update(&app,revision,|s|s.phase=format!("{}_{}",if stage==Stage::Transparency{"transparency"}else{"portrait"},emotion.as_str()));
                let input=if stage==Stage::Transparency {
                    let bytes=sherka_core::character_studio::portrait_png(raw.get(&emotion).ok_or("Portrait absent.")?).map_err(|e|e.to_string())?;
                    Some(client.upload_image(&bytes).await.map_err(|e|e.to_string())?)
                }else{reference.clone()};
                let graph=studio_workflows::workflow(&config,&draft,stage,emotion,&info,(Uuid::new_v4().as_u128() as u64)&0x7fff_ffff_ffff_ffff,input.as_deref()).map_err(|e|e.to_string())?;
                let image=client.render(graph,token.clone()).await.map_err(|e|e.to_string())?;
                let image=tauri::async_runtime::spawn_blocking(move||{
                    use base64::Engine;
                    let bytes=if stage==Stage::Transparency{sherka_core::character_studio::transparent_png(&image)}else{sherka_core::character_studio::portrait_png(&image)}.map_err(|e|e.to_string())?;
                    Ok::<_,String>(format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes)))
                }).await.map_err(|_|"La tâche locale n'a pas pu se terminer.".to_string())??;
                studio.update(&app,revision,|s|{
                    if stage==Stage::Transparency{s.transparent_images.insert(emotion,image);}
                    else{s.images.insert(emotion,image);s.transparent_images.remove(&emotion);}
                });
            }
            Ok(())
        }.await;
        studio.finish(&app, revision, result);
    });
}

#[tauri::command]
pub fn validate_character_neutral(
    app: AppHandle,
    studio: State<'_, Studio>,
    revision: u64,
) -> Result<(), String> {
    let mut inner = studio.0.lock().map_err(|_| "Studio indisponible.")?;
    if inner.status.busy
        || inner.status.revision != revision
        || !inner.status.images.contains_key(&Emotion::Neutral)
    {
        return Err("Valide d’abord le portrait neutre.".into());
    }
    inner.status.neutral_validated = true;
    inner.status.sequence += 1;
    drop(inner);
    studio.emit(&app);
    Ok(())
}

#[tauri::command]
pub async fn finish_character(
    app: AppHandle,
    shared: State<'_, Shared>,
    studio: State<'_, Studio>,
    revision: u64,
    draft: Draft,
    config: Config,
) -> Result<Uuid, String> {
    draft.validate().map_err(|e| e.to_string())?;
    let config = config.validated().map_err(|e| e.to_string())?;
    let status = studio.status();
    if let Some(id) = status.created_profile {
        return Ok(id);
    }
    if status.busy
        || status.revision != revision
        || !status.neutral_validated
        || !moods_match(&status, &draft, &config)
        || status.config.as_ref().is_none_or(|c| c != &config)
        || EMOTIONS
            .iter()
            .any(|e| !status.transparent_images.contains_key(e))
    {
        return Err("Termine le détourage des sept images avant d’ajouter le compagnon.".into());
    }
    let source = status.source_profile.ok_or("Fiche absente.")?;
    // Capture only the source and store path under the core lock. Image
    // validation/encoding and atomic creation must not block conversation IPC.
    let (store, source) = with_core(shared.inner().clone(), move |core| {
        Ok((core.store.clone(), core.profile(source)?.details))
    })
    .await?;
    let (run, token) = studio.begin(&app, "publishing")?;
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
        if token.is_cancelled() {
            return Err(sherka_core::Error::StaleOperation.to_string());
        }
        let mut images = BTreeMap::new();
        for (emotion, data) in status.transparent_images {
            images.insert(
                emotion,
                Some(
                    sherka_core::character_studio::transparent_png(&data)
                        .map_err(|e| e.to_string())?,
                ),
            );
        }
        let neutral = images
            .remove(&Emotion::Neutral)
            .flatten()
            .ok_or("Portrait absent.")?;
        let image_config = config.illustration(&draft);
        let profile = draft
            .profile(&source, image_config)
            .map_err(|e| e.to_string())?;
        Ok(store
            .create_with_avatars(profile, Some(&neutral), images)
            .map_err(|e| e.to_string())?
            .id)
    })
    .await
    .unwrap_or_else(|_| Err("La tâche locale n'a pas pu se terminer.".into()));
    match result {
        Ok(id) => {
            studio.update(&app, run, |s| s.created_profile = Some(id));
            studio.finish(&app, run, Ok(()));
            let _ = app.emit("sherka:changed", ());
            Ok(id)
        }
        Err(error) => {
            studio.finish(&app, run, Err(error.clone()));
            Err(error)
        }
    }
}
