mod conversation;
pub mod portable_update;
mod service_process;
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
        base64: Option<String>,
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
    let _gate = voice.gate.lock().await;
    windows::ensure_running()?;
    voice.cancel_pending();
    let preferences_changed = matches!(&mutation, Mutation::Preferences { .. });
    let stop_audio = matches!(
        &mutation,
        Mutation::PauseVoice | Mutation::AudioSettings { .. }
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
                core.store.set_avatar(id, revision, bytes.as_deref())?;
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
) -> Result<(), String> {
    conversation::send(
        app,
        shared.inner().clone(),
        voice.inner().clone(),
        profile_id,
        text,
        speak,
    )
    .await
}

#[tauri::command]
async fn get_history(shared: State<'_, Shared>, profile_id: Uuid) -> Result<Vec<Message>, String> {
    with_core(shared.inner().clone(), move |core| {
        core.store.history(profile_id, 100)
    })
    .await
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
            .set_title("Exporter les souvenirs et leurs sources")
            .add_filter("Mémoire Sherka", &["json"])
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
async fn discover_language_models(base_url: String) -> Result<LanguageCatalog, String> {
    CatalogClient::new(&base_url)
        .map_err(|e| e.to_string())?
        .language_models()
        .await
        .map_err(|e| e.to_string())
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
async fn export_profile(
    app: AppHandle,
    shared: State<'_, Shared>,
    profile_id: Uuid,
) -> Result<bool, String> {
    let json = with_core(shared.inner().clone(), move |core| {
        core.store.export_profile(profile_id)
    })
    .await?;
    tauri::async_runtime::spawn_blocking(move || {
        let selection = app
            .dialog()
            .file()
            .set_title("Exporter le compagnon sans souvenirs")
            .add_filter("Profil Sherka", &["json"])
            .set_file_name("compagnon.sherka.json")
            .blocking_save_file();
        if let Some(file) = selection {
            let path = file
                .into_path()
                .map_err(|_| "Le chemin choisi est invalide.".to_string())?;
            std::fs::write(path, json).map_err(|e| e.to_string())?;
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
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Err(e) = windows::restore_widget(app) {
                eprintln!("Impossible de restaurer la fenêtre : {e}");
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            get_widget_visibility,
            get_voice_service,
            retry_voice_service,
            switch_companion,
            apply_mutation,
            get_history,
            get_memories,
            edit_memory,
            search_archive,
            export_memories,
            export_profile,
            discover_language_models,
            discover_voices,
            widget_action,
            get_audio_devices,
            get_conversation,
            send_message,
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
            let companion = Companion::open(directory)?;
            let snapshot = companion.snapshot()?;
            let shared = Shared(Arc::new(Mutex::new(companion)));
            app.manage(
                voice_startup::VoiceStartup::new(app.handle().clone())
                    .map_err(std::io::Error::other)?,
            );
            app.manage(shared.clone());
            app.manage(updates::Updates::new());
            app.manage(Voice::setup(app.handle().clone(), shared));
            // Build webviews only after registering the engine. Config-created
            // webviews can execute JS before setup and race the first IPC call.
            for config in &app.config().app.windows.clone() {
                tauri::WebviewWindowBuilder::from_config(app, config)?.build()?;
            }
            windows::setup_tray(app)?;
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
        .build(tauri::generate_context!())
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
