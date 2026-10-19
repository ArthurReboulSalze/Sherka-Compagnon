mod audio_diagnostics;
mod comfy_startup;
mod conversation;
mod desktop_tools;
mod illustrations;
mod memory_ops;
pub mod portable_update;
mod service_process;
mod speech_input;
mod startup;
mod updates;
mod voice_startup;
mod windows;

use base64::{Engine, engine::general_purpose::STANDARD};
use conversation::Voice;
use serde::Deserialize;
use sherka_core::{
    Companion, Error,
    catalogs::{CatalogClient, LanguageCatalog, VoiceCatalog},
    connections::Connections,
    model::{Preferences, ProfileDraft},
    runtime::Snapshot,
    storage::Message,
};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

#[derive(Clone)]
struct Shared(Arc<Mutex<Companion>>);

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Mutation {
    Theme {
        theme: sherka_core::model::Theme,
    },
    MoodPersistence {
        id: Uuid,
        revision: i64,
        enabled: bool,
    },
    ResetMood {
        id: Uuid,
    },
    ComfyuiStartup {
        config: sherka_core::model::ComfyStartupConfig,
    },
    Illustration {
        id: Uuid,
        revision: i64,
        config: sherka_core::illustration::Config,
    },
    WebSearch {
        mode: sherka_core::web::SearchMode,
        id: Option<Uuid>,
        revision: Option<i64>,
        config: Option<sherka_core::search_provider::Config>,
    },
    InterfaceLanguage {
        language: sherka_core::i18n::Language,
    },
    CreateProfile {
        draft: ProfileDraft,
    },
    UpdateProfile {
        id: Uuid,
        revision: i64,
        draft: ProfileDraft,
    },
    SwitchProfile {
        id: Uuid,
    },
    DuplicateProfile {
        id: Uuid,
    },
    DeleteProfile {
        id: Uuid,
    },
    SetAvatar {
        id: Uuid,
        revision: i64,
        #[serde(default)]
        emotion: sherka_core::avatar::Emotion,
        base64: Option<String>,
    },
    ImportCompanionArchive {
        base64: String,
    },
    ImportProfile {
        json: String,
    },
    Preferences {
        preferences: Preferences,
    },
    Connections {
        id: Uuid,
        revision: i64,
        connections: Connections,
    },
    PauseVoice,
    StopResponse,
    AudioSettings {
        audio: sherka_core::voice::AudioSettings,
    },
}

async fn with_core<T, F>(shared: Shared, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&mut Companion) -> sherka_core::Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let mut core = shared
            .0
            .lock()
            .map_err(|_| "Le moteur de Sherka doit être redémarré.".to_string())?;
        f(&mut core).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "La tâche locale n'a pas pu se terminer.".to_string())?
}

#[tauri::command]
async fn get_snapshot(shared: State<'_, Shared>) -> Result<Snapshot, String> {
    with_core(shared.inner().clone(), |core| core.snapshot()).await
}

#[tauri::command]
fn get_widget_visibility(app: AppHandle) -> Result<bool, String> {
    windows::widget_visible(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_voice_service(service: State<'_, voice_startup::VoiceStartup>) -> voice_startup::Status {
    service.status()
}

#[tauri::command]
async fn retry_voice_service(app: AppHandle) -> Result<(), String> {
    voice_startup::start_active(&app, true).await
}

#[tauri::command]
async fn switch_companion(
    app: AppHandle,
    shared: State<'_, Shared>,
    voice: State<'_, Voice>,
    id: Uuid,
) -> Result<Snapshot, String> {
    apply_mutation(app, shared, Mutation::SwitchProfile { id }, voice).await
}

#[tauri::command]
async fn apply_mutation(
    app: AppHandle,
    shared: State<'_, Shared>,
    mutation: Mutation,
    voice: State<'_, Voice>,
) -> Result<Snapshot, String> {
    windows::ensure_running()?;
    if let Mutation::InterfaceLanguage { language } = &mutation {
        let language = *language;
        let snapshot = with_core(shared.inner().clone(), move |core| {
            if core.interface_language_changes_active_companion(language)? {
                return Ok(None);
            }
            core.set_interface_language(language)?;
            core.snapshot().map(Some)
        })
        .await?;
        if let Some(snapshot) = snapshot {
            windows::set_language(&app, language).map_err(|error| error.to_string())?;
            app.emit("sherka:changed", ())
                .map_err(|error| error.to_string())?;
            return Ok(snapshot);
        }
    }
    let _gate = voice.gate.lock().await;
    windows::ensure_running()?;
    if !matches!(
        &mutation,
        Mutation::Theme { .. }
            | Mutation::Illustration { .. }
            | Mutation::MoodPersistence { .. }
            | Mutation::ResetMood { .. }
            | Mutation::ComfyuiStartup { .. }
    ) {
        voice.cancel_pending();
    }
    let interface_language = match &mutation {
        Mutation::InterfaceLanguage { language } => Some(*language),
        _ => None,
    };
    let preferences_changed = matches!(&mutation, Mutation::Preferences { .. });
    let stop_audio = matches!(
        &mutation,
        Mutation::PauseVoice | Mutation::AudioSettings { .. } | Mutation::InterfaceLanguage { .. }
    );
    let previous = with_core(shared.inner().clone(), |c| {
        Ok((
            c.session.snapshot().profile_id,
            c.store
                .profile(c.session.snapshot().profile_id)?
                .details
                .connections,
        ))
    })
    .await?;
    let snapshot = with_core(shared.inner().clone(), move |core| {
        match mutation {
            Mutation::Theme { theme } => core.set_theme(theme)?,
            Mutation::ComfyuiStartup { config } => core.set_comfy_startup_config(config)?,
            Mutation::MoodPersistence {
                id,
                revision,
                enabled,
            } => {
                let mut draft = core.store.profile(id)?.details;
                draft.mood_persistence = enabled;
                core.update_profile(id, revision, draft)?;
            }
            Mutation::ResetMood { id } => core.reset_mood(id)?,
            Mutation::Illustration {
                id,
                revision,
                config,
            } => {
                let mut draft = core.store.profile(id)?.details;
                draft.illustration = config;
                core.update_profile(id, revision, draft)?;
            }
            Mutation::WebSearch {
                mode,
                id,
                revision,
                config,
            } => {
                if let (Some(id), Some(revision)) = (id, revision) {
                    if let Some(config) = config {
                        let mut draft = core.store.profile(id)?.details;
                        draft.web_search = Some(mode);
                        draft.search = config;
                        core.update_profile(id, revision, draft)?;
                    } else {
                        core.set_profile_web_search(id, revision, mode)?;
                    }
                } else {
                    core.set_web_search_mode(mode)?;
                }
            }
            Mutation::InterfaceLanguage { language } => core.set_interface_language(language)?,
            Mutation::CreateProfile { draft } => {
                core.store.create(draft, None)?;
            }
            Mutation::UpdateProfile {
                id,
                revision,
                draft,
            } => core.update_profile(id, revision, draft)?,
            Mutation::SwitchProfile { id } => core.switch_profile(id)?,
            Mutation::DuplicateProfile { id } => {
                core.store.duplicate(id)?;
            }
            Mutation::DeleteProfile { id } => core.store.delete(id)?,
            Mutation::SetAvatar {
                id,
                revision,
                emotion,
                base64,
            } => {
                if base64.as_ref().is_some_and(|s| s.len() > 14 * 1024 * 1024) {
                    return Err(Error::Validation(
                        "L'image dépasse la limite de 10 Mio.".into(),
                    ));
                }
                let bytes = base64
                    .map(|s| {
                        STANDARD
                            .decode(s)
                            .map_err(|_| Error::Validation("L'image est invalide.".into()))
                    })
                    .transpose()?;
                core.store
                    .set_emotion_avatar(id, revision, emotion, bytes.as_deref())?;
            }
            Mutation::ImportCompanionArchive { base64 } => {
                if base64.len() > 64 * 1024 * 1024 {
                    return Err(Error::Validation(
                        "Ce fichier dépasse la limite de 48 Mio.".into(),
                    ));
                }
                let bytes = STANDARD.decode(base64).map_err(|_| {
                    Error::Validation(
                        "Archive de compagnon invalide ou version non prise en charge.".into(),
                    )
                })?;
                core.store.import_companion_archive(&bytes)?;
            }
            Mutation::ImportProfile { json } => {
                core.store.import_profile(&json)?;
            }
            Mutation::Preferences { preferences } => core.set_preferences(preferences)?,
            Mutation::Connections {
                id,
                revision,
                connections,
            } => {
                let mut draft = core.store.profile(id)?.details;
                draft.connections = connections;
                core.update_profile(id, revision, draft)?;
            }
            Mutation::PauseVoice => core.pause_voice()?,
            Mutation::StopResponse => core.stop_response()?,
            Mutation::AudioSettings { audio } => core.set_audio_settings(audio)?,
        }
        core.snapshot()
    })
    .await?;
    if let Some(active) = snapshot
        .profiles
        .iter()
        .find(|p| p.id == snapshot.session.profile_id)
        && previous.1.voice_studio_url != active.details.connections.voice_studio_url
    {
        app.state::<voice_startup::VoiceStartup>()
            .start(&active.details.connections.voice_studio_url, false)?;
    }
    if let Some(active) = snapshot
        .profiles
        .iter()
        .find(|p| p.id == snapshot.session.profile_id)
        && (previous.0 != active.id
            || previous.1.llm_provider != active.details.connections.llm_provider
            || previous.1.llm_api_url != active.details.connections.llm_api_url
            || previous.1.hermes_profile != active.details.connections.hermes_profile
            || previous.1.llm_model != active.details.connections.llm_model
            || previous.1.lm_studio_url != active.details.connections.lm_studio_url)
    {
        conversation::switch_model(
            app.clone(),
            shared.inner().clone(),
            voice.inner(),
            previous.1,
        );
    }
    if stop_audio {
        voice.stop_audio().await;
    }
    conversation::emit(&app, shared.inner());
    illustrations::synchronize(&app, shared.inner());
    if let Some(language) = interface_language {
        windows::set_language(&app, language).map_err(|error| error.to_string())?;
    }
    if preferences_changed {
        windows::apply_preferences(&app, &snapshot.settings).map_err(|e| e.to_string())?;
    }
    app.emit("sherka:changed", ()).map_err(|e| e.to_string())?;
    Ok(snapshot)
}

#[tauri::command]
async fn get_audio_devices() -> Result<sherka_core::audio::Devices, String> {
    tauri::async_runtime::spawn_blocking(|| {
        sherka_core::audio::devices().map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Impossible de lire les périphériques audio.".to_string())?
}

#[tauri::command]
async fn get_conversation(shared: State<'_, Shared>) -> Result<conversation::LiveState, String> {
    conversation::live(shared.inner())
}

#[tauri::command]
async fn send_message(
    app: AppHandle,
    shared: State<'_, Shared>,
    voice: State<'_, Voice>,
    profile_id: Uuid,
    text: String,
    speak: bool,
    attachments: Option<Vec<sherka_core::attachments::Input>>,
) -> Result<(), String> {
    conversation::send(
        app,
        shared.inner().clone(),
        voice.inner().clone(),
        profile_id,
        text,
        speak,
        attachments.unwrap_or_default(),
    )
    .await
}

#[tauri::command]
async fn get_attachment_support(
    shared: State<'_, Shared>,
    profile_id: Uuid,
) -> Result<sherka_core::attachments::Support, String> {
    let connections = with_core(shared.inner().clone(), move |c| {
        if c.session.snapshot().profile_id != profile_id {
            return Err(Error::StaleOperation);
        }
        Ok(c.store.profile(profile_id)?.details.connections)
    })
    .await?;
    sherka_core::providers::Providers::new(connections)
        .map_err(|e| e.to_string())?
        .image_support()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_web_source(
    shared: State<'_, Shared>,
    profile_id: Uuid,
    message_id: Option<String>,
    index: usize,
) -> Result<(), String> {
    let url = with_core(shared.inner().clone(), move |core| {
        let report = if let Some(id) = message_id {
            core.store
                .history(profile_id, 100)?
                .into_iter()
                .find(|message| message.id == id)
                .and_then(|message| message.web)
        } else if core.session.snapshot().profile_id == profile_id {
            core.live.web.clone()
        } else {
            None
        };
        let source = report
            .as_ref()
            .and_then(|report| report.sources.get(index))
            .ok_or_else(|| {
                sherka_core::Error::Validation(
                    "Cette source Internet n’est plus disponible.".into(),
                )
            })?;
        sherka_core::web::public_url(&source.url).ok_or_else(|| {
            sherka_core::Error::Validation("Cette adresse Internet n’est pas autorisée.".into())
        })
    })
    .await?;
    tauri::async_runtime::spawn_blocking(move || windows::open_web_url(&url))
        .await
        .map_err(|_| "Impossible d’ouvrir la source Internet.".to_owned())?
}

#[tauri::command]
async fn get_history(shared: State<'_, Shared>, profile_id: Uuid) -> Result<Vec<Message>, String> {
    with_core(shared.inner().clone(), move |core| {
        core.store.history(profile_id, 100)
    })
    .await
}

#[tauri::command]
async fn clear_conversation(
    app: AppHandle,
    shared: State<'_, Shared>,
    voice: State<'_, Voice>,
    profile_id: Uuid,
    erase_saved_facts: bool,
    erase_mood: Option<bool>,
) -> Result<(), String> {
    conversation::clear(
        &app,
        shared.inner().clone(),
        voice.inner(),
        profile_id,
        erase_saved_facts,
        erase_mood.unwrap_or(false),
        None,
    )
    .await
    .map(|_| ())
}

#[tauri::command]
async fn get_memories(
    shared: State<'_, Shared>,
    profile_id: Uuid,
    query: String,
) -> Result<sherka_core::memory::MemoryState, String> {
    with_core(shared.inner().clone(), move |c| {
        c.store.memories(profile_id, &query)
    })
    .await
}
#[tauri::command]
async fn search_archive(
    shared: State<'_, Shared>,
    profile_id: Uuid,
    query: String,
) -> Result<Vec<sherka_core::memory::Source>, String> {
    with_core(shared.inner().clone(), move |c| {
        c.store.search_archive(profile_id, &query)
    })
    .await
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum MemoryEdit {
    Write {
        id: Option<i64>,
        draft: sherka_core::memory::MemoryDraft,
    },
    Forget {
        id: i64,
    },
    Delete {
        id: i64,
    },
    Approve {
        id: i64,
    },
}
#[tauri::command]
async fn edit_memory(
    app: AppHandle,
    shared: State<'_, Shared>,
    profile_id: Uuid,
    epoch: i64,
    edit: MemoryEdit,
) -> Result<sherka_core::memory::MemoryState, String> {
    let result = with_core(shared.inner().clone(), move |c| {
        let guard = c.store.memory_guard(profile_id)?;
        if guard.epoch != epoch {
            return Err(Error::Conflict);
        }
        if c.session.snapshot().profile_id == profile_id {
            c.stop_response()?;
        }
        match edit {
            MemoryEdit::Write { id, draft } => {
                c.store.write_memory(&guard, id, &draft)?;
            }
            MemoryEdit::Forget { id } => c.store.forget_memory(&guard, id)?,
            MemoryEdit::Delete { id } => c.store.delete_memory(&guard, id)?,
            MemoryEdit::Approve { id } => c.store.approve_memory(&guard, id)?,
        }
        c.store.memories(profile_id, "")
    })
    .await?;
    conversation::emit(&app, shared.inner());
    let _ = app.emit("sherka:memory", profile_id);
    Ok(result)
}
#[tauri::command]
async fn export_memories(
    app: AppHandle,
    shared: State<'_, Shared>,
    profile_id: Uuid,
) -> Result<bool, String> {
    let json = with_core(shared.inner().clone(), move |c| {
        c.store.export_memories(profile_id)
    })
    .await?;
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(file) = app
            .dialog()
            .file()
            .set_title(sherka_core::i18n::text(
                "Exporter les souvenirs et leurs sources",
            ))
            .add_filter(sherka_core::i18n::text("Mémoire Sherka"), &["json"])
            .set_file_name("souvenirs.sherka.json")
            .blocking_save_file()
        {
            let path = file
                .into_path()
                .map_err(|_| "Chemin invalide.".to_owned())?;
            std::fs::write(path, json).map_err(|e| e.to_string())?;
            Ok(true)
        } else {
            Ok(false)
        }
    })
    .await
    .map_err(|_| "L'export n'a pas pu se terminer.".to_owned())?
}

#[tauri::command]
async fn discover_language_models(
    base_url: Option<String>,
    connections: Option<Connections>,
) -> Result<LanguageCatalog, String> {
    if let Some(config) = connections {
        return sherka_core::providers::Providers::new(config)
            .map_err(|e| e.to_string())?
            .catalog()
            .await
            .map_err(|e| e.to_string());
    }
    let base_url =
        base_url.ok_or_else(|| "Indique l’adresse de base de l’API du fournisseur.".to_owned())?;
    CatalogClient::new(&base_url)
        .map_err(|e| e.to_string())?
        .language_models()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_provider_key_status(connections: Connections) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        sherka_core::credentials::load(&connections)
            .map(|key| key.is_some())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| {
        "Le coffre de clés API n’est pas accessible. Vérifie le coffre de ta session.".to_owned()
    })?
}

#[tauri::command]
async fn search_key_status(
    provider: sherka_core::search_provider::Provider,
) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        sherka_core::credentials::load_search_key(provider)
            .map(|v| v.is_some())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Moteur indisponible.".to_string())?
}
#[tauri::command]
async fn test_search_config(
    config: sherka_core::search_provider::Config,
    language: String,
) -> Result<usize, String> {
    crate::windows::ensure_running()?;
    sherka_core::search_provider::search(
        &config,
        "Rust programming language official documentation",
        &language,
    )
    .await
    .map(|sources| sources.len())
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_search_key(
    provider: sherka_core::search_provider::Provider,
    key: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        sherka_core::credentials::set_search_key(provider, key.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Moteur indisponible.".to_string())?
}
#[tauri::command]
async fn set_provider_key(connections: Connections, key: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        sherka_core::credentials::set(&connections, key.as_deref()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| {
        "Le coffre de clés API n’est pas accessible. Vérifie le coffre de ta session.".to_owned()
    })?
}

#[tauri::command]
async fn discover_voices(base_url: String) -> Result<VoiceCatalog, String> {
    CatalogClient::new(&base_url)
        .map_err(|e| e.to_string())?
        .voices()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_hermes_profiles(
    shared: State<'_, Shared>,
    server: String,
) -> Result<Vec<String>, String> {
    let server = sherka_core::hermes::server_url(&server).map_err(|e| e.to_string())?;
    with_core(shared.inner().clone(), move |c| {
        c.store.hermes_profiles(&server)
    })
    .await
}

#[tauri::command]
async fn get_hermes_servers(shared: State<'_, Shared>) -> Result<Vec<String>, String> {
    with_core(shared.inner().clone(), |c| c.store.hermes_servers()).await
}

#[tauri::command]
async fn new_hermes_conversation(
    app: AppHandle,
    shared: State<'_, Shared>,
    voice: State<'_, Voice>,
    profile_id: Uuid,
) -> Result<Snapshot, String> {
    let _gate = voice.gate.lock().await;
    voice.cancel_pending();
    voice.playback.interrupt();
    let snapshot = with_core(shared.inner().clone(), move |c| {
        if c.session.snapshot().profile_id != profile_id
            || !c
                .store
                .profile(profile_id)?
                .details
                .connections
                .llm_provider
                .is_agent()
        {
            return Err(Error::StaleOperation);
        }
        c.stop_response()?;
        c.store.new_hermes_session(profile_id)?;
        c.session.switch_profile(profile_id);
        c.live = sherka_core::conversation::LiveConversation {
            notice: Some("Nouvelle conversation Hermes. Sa mémoire est conservée.".into()),
            ..Default::default()
        };
        c.snapshot()
    })
    .await?;
    conversation::emit(&app, shared.inner());
    let _ = app.emit("sherka:history", profile_id);
    let _ = app.emit("sherka:changed", ());
    Ok(snapshot)
}

#[tauri::command]
async fn test_hermes(
    shared: State<'_, Shared>,
    connections: Connections,
) -> Result<Vec<String>, String> {
    let provider =
        sherka_core::providers::Providers::new(connections).map_err(|e| e.to_string())?;
    provider.hermes_probe().await.map_err(|e| e.to_string())?;
    let config = provider.connections;
    with_core(shared.inner().clone(), move |c| {
        let server = config.llm_api_url.as_deref().ok_or(Error::StaleOperation)?;
        c.store.remember_hermes_profile(
            server,
            config.hermes_profile.as_deref().unwrap_or("default"),
        )?;
        c.store.hermes_profiles(server)
    })
    .await
}

#[tauri::command]
async fn answer_hermes_approval(
    app: AppHandle,
    shared: State<'_, Shared>,
    run_id: String,
    request_id: Option<String>,
    allow: bool,
) -> Result<(), String> {
    let (tag, approval, config) = with_core(shared.inner().clone(), move |c| {
        let tag = c.live.tag.ok_or(Error::StaleOperation)?;
        let approval = c
            .live
            .hermes_approval
            .clone()
            .ok_or(Error::StaleOperation)?;
        if approval.run_id != run_id || approval.request_id != request_id || !c.session.accepts(tag)
        {
            return Err(Error::StaleOperation);
        }
        let config = c.store.profile(tag.profile_id)?.details.connections;
        c.live.hermes_approval = None;
        Ok((tag, approval, config))
    })
    .await?;
    conversation::emit(&app, shared.inner());
    let result = sherka_core::providers::Providers::new(config)
        .map_err(|e| e.to_string())?
        .hermes_approve(&approval, allow)
        .await
        .map_err(|e| e.to_string());
    if result.is_err() {
        with_core(shared.inner().clone(), move |c| {
            if c.session.accepts(tag) && c.live.hermes_approval.is_none() {
                c.live.hermes_approval = Some(approval);
            }
            Ok(())
        })
        .await?;
        conversation::emit(&app, shared.inner());
    }
    result
}

#[tauri::command]
async fn export_profile(
    app: AppHandle,
    shared: State<'_, Shared>,
    profile_id: Uuid,
) -> Result<bool, String> {
    let resolver = app.asset_resolver();
    let archive = with_core(shared.inner().clone(), move |core| {
        core.store.export_companion_archive(profile_id, |path| {
            resolver.get(path.to_owned()).map(|asset| asset.bytes)
        })
    })
    .await?;
    tauri::async_runtime::spawn_blocking(move || {
        let selection = app
            .dialog()
            .file()
            .set_title(sherka_core::i18n::text(
                "Exporter le compagnon sans souvenirs",
            ))
            .add_filter(sherka_core::i18n::text("Profil Sherka"), &["zip"])
            .set_file_name("companion.sherka.zip")
            .blocking_save_file();
        if let Some(file) = selection {
            let path = file
                .into_path()
                .map_err(|_| "Le chemin choisi est invalide.".to_string())?;
            std::fs::write(path, archive).map_err(|e| e.to_string())?;
            Ok(true)
        } else {
            Ok(false)
        }
    })
    .await
    .map_err(|_| "L'export n'a pas pu se terminer.".to_string())?
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum WidgetAction {
    StopHermes,
    ShowPanel,
    ShowWidget,
    HideWidget,
    ToggleWidget,
    Quit,
    QuitWithoutUnloading,
    ClickThrough,
    RestoreWidget,
    StopResponse,
    PauseVoice,
    ToggleMicrophone,
}

#[tauri::command]
async fn widget_action(
    app: AppHandle,
    shared: State<'_, Shared>,
    action: WidgetAction,
    voice: State<'_, Voice>,
) -> Result<(), String> {
    match action {
        WidgetAction::StopHermes => {
            let _gate = voice.gate.lock().await;
            conversation::stop_hermes(&app, shared.inner().clone(), voice.inner()).await?;
        }
        WidgetAction::ShowPanel => windows::show_panel(&app).map_err(|e| e.to_string())?,
        WidgetAction::ShowWidget | WidgetAction::RestoreWidget => {
            windows::restore_widget(&app).map_err(|e| e.to_string())?
        }
        WidgetAction::HideWidget => {
            windows::hide_widget(&app).map_err(|e| e.to_string())?;
        }
        WidgetAction::ToggleWidget => windows::toggle_widget(&app).map_err(|e| e.to_string())?,
        WidgetAction::Quit => app.exit(0),
        WidgetAction::QuitWithoutUnloading => windows::quit_without_unloading(&app)?,
        WidgetAction::ClickThrough => {
            if app.tray_by_id("sherka").is_none() {
                return Err(
                    "La zone de notification doit être disponible pour reprendre le contrôle."
                        .into(),
                );
            }
            if let Some(w) = app.get_webview_window("widget") {
                w.set_ignore_cursor_events(true)
                    .map_err(|e| e.to_string())?;
            }
        }
        WidgetAction::ToggleMicrophone => {
            conversation::toggle(app, shared.inner().clone(), voice.inner().clone()).await?
        }
        WidgetAction::StopResponse | WidgetAction::PauseVoice => {
            let _gate = voice.gate.lock().await;
            conversation::pause(
                &app,
                shared.inner().clone(),
                voice.inner(),
                matches!(action, WidgetAction::PauseVoice),
            )
            .await?;
        }
    }
    Ok(())
}

pub fn run() {
    let mut context = tauri::generate_context!();
    // An explicit data directory also scopes singleton ownership, so tests
    // cannot restore or send commands to an already running personal copy.
    if let Some(directory) = std::env::var_os("SHERKA_DATA_DIR") {
        use std::hash::{Hash, Hasher};
        let path = std::path::PathBuf::from(directory);
        let path = std::fs::canonicalize(&path).unwrap_or_else(|_| {
            if path.is_absolute() {
                path.clone()
            } else {
                std::env::current_dir().unwrap_or_default().join(&path)
            }
        });
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut hash);
        context.config_mut().identifier =
            format!("fr.sherka.compagnon.isolated.h{:x}", hash.finish());
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if app.try_state::<startup::Issue>().is_some() {
                let _ = windows::show_panel(app);
                return;
            }
            if let Err(e) = windows::restore_widget(app) {
                eprintln!("Impossible de restaurer la fenêtre : {e}");
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            startup::get_startup_issue,
            startup::close_startup_recovery,
            get_snapshot,
            comfy_startup::get_comfy_service,
            comfy_startup::select_comfyui_path,
            illustrations::get_illustration,
            illustrations::discover_illustration,
            illustrations::refresh_illustration,
            desktop_tools::desktop_tool,
            desktop_tools::widget_surface,
            get_widget_visibility,
            get_voice_service,
            retry_voice_service,
            switch_companion,
            apply_mutation,
            get_history,
            clear_conversation,
            get_memories,
            memory_ops::optimize_memory,
            memory_ops::get_memory_optimization,
            memory_ops::finish_memory_optimization,
            edit_memory,
            search_archive,
            export_memories,
            export_profile,
            discover_language_models,
            get_hermes_profiles,
            get_hermes_servers,
            new_hermes_conversation,
            test_hermes,
            answer_hermes_approval,
            get_provider_key_status,
            search_key_status,
            set_search_key,
            test_search_config,
            illustrations::export_illustration_workflow,
            set_provider_key,
            discover_voices,
            widget_action,
            get_audio_devices,
            get_conversation,
            send_message,
            get_attachment_support,
            open_web_source,
            updates::get_update_status,
            updates::check_update,
            updates::install_update
        ])
        .setup(|app| {
            // Runtime override is useful for isolated local testing; it never
            // changes the default per-user application data directory.
            let directory = std::env::var_os("SHERKA_DATA_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or(app.path().app_local_data_dir()?);
            // Updates must remain available even if this version cannot load
            // data last opened by a newer installed or no-install executable.
            app.manage(updates::Updates::new());
            let (companion, snapshot) = match Companion::open(&directory)
                .and_then(|companion| companion.snapshot().map(|snapshot| (companion, snapshot)))
            {
                Ok(loaded) => loaded,
                Err(error) => {
                    startup::recover(app, &directory, &error)?;
                    return Ok(());
                }
            };
            sherka_core::i18n::set_ui_language(snapshot.settings.ui_language);
            let shared = Shared(Arc::new(Mutex::new(companion)));
            app.manage(
                voice_startup::VoiceStartup::new(app.handle().clone())
                    .map_err(std::io::Error::other)?,
            );
            app.manage(
                comfy_startup::ComfyStartup::new(app.handle().clone(), directory.clone())
                    .map_err(std::io::Error::other)?,
            );
            app.manage(shared.clone());
            app.manage(illustrations::Illustrations::new());
            app.manage(desktop_tools::Canvas::default());
            illustrations::synchronize(app.handle(), &shared);
            app.manage(Voice::setup(app.handle().clone(), shared, &directory));
            // Build webviews only after registering the engine. Config-created
            // webviews can execute JS before setup and race the first IPC call.
            for config in &app.config().app.windows.clone() {
                tauri::WebviewWindowBuilder::from_config(app, config)?.build()?;
            }
            windows::setup_tray(app)?;
            windows::set_language(app.handle(), snapshot.settings.ui_language)?;
            windows::apply_preferences(app.handle(), &snapshot.settings)?;
            windows::recover_widget(app.handle(), snapshot.settings.widget_bounds)?;
            // Only debug builds accept hidden startup for native WebView2
            // integration tests. Release builds always show the application.
            let hidden_test =
                cfg!(debug_assertions) && std::env::var("SHERKA_TEST_HIDDEN").as_deref() == Ok("1");
            if !hidden_test {
                windows::restore_widget(app.handle())?;
            }
            if let Some(active) = snapshot
                .profiles
                .iter()
                .find(|p| p.id == snapshot.session.profile_id)
            {
                app.state::<voice_startup::VoiceStartup>()
                    .start(&active.details.connections.voice_studio_url, false)
                    .map_err(std::io::Error::other)?;
            }
            Ok(())
        })
        .on_window_event(windows::on_window_event)
        .build(context)
        .expect("Sherka n'a pas pu démarrer")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event
                && !windows::exit_ready()
            {
                api.prevent_exit();
                windows::quit(app);
            }
        });
}
