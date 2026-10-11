use serde::Serialize;
use std::path::Path;
use tauri::Manager;

#[derive(Clone, Serialize)]
pub struct Issue {
    pub language: sherka_core::i18n::Language,
    pub error: String,
    pub data_directory: String,
}

#[tauri::command]
pub fn get_startup_issue(app: tauri::AppHandle) -> Option<Issue> {
    app.try_state::<Issue>().map(|issue| issue.inner().clone())
}

#[tauri::command]
pub fn close_startup_recovery(app: tauri::AppHandle) -> Result<(), String> {
    if app.try_state::<Issue>().is_none() {
        return Err("Le mode de récupération n’est pas actif.".into());
    }
    crate::windows::quit(&app);
    Ok(())
}

pub fn recover(
    app: &tauri::App,
    directory: &Path,
    error: &sherka_core::Error,
) -> tauri::Result<()> {
    let language = sherka_core::storage::Store::saved_language(directory);
    sherka_core::i18n::set_ui_language(language);
    app.manage(Issue {
        language,
        error: error.to_string(),
        data_directory: directory.display().to_string(),
    });
    let log = format!(
        "Sherka {}\nStartup failed: {error}\n",
        env!("CARGO_PKG_VERSION")
    );
    // No conversations, model requests or secrets are attached to diagnostics.
    let _ = std::fs::write(directory.join("startup-error.log"), log);
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == "panel")
        .cloned()
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    config.url = tauri::WebviewUrl::App("index.html?view=recovery".into());
    config.visible = true;
    config.title = sherka_core::i18n::text("Sherka — Récupération");
    tauri::WebviewWindowBuilder::from_config(app, &config)?.build()?;
    Ok(())
}
