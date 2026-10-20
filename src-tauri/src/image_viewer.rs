//! Full-screen illustrations use a separate background surface so the avatar
//! keeps its own native drag, resize and saved desktop geometry.
use crate::{Shared, with_core};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use uuid::Uuid;

#[derive(Clone, Serialize)]
pub struct Context {
    pub profile_id: Uuid,
    pub slide_seconds: u32,
    pub ui_language: sherka_core::i18n::Language,
    pub theme: sherka_core::model::Theme,
    #[serde(skip)]
    always_on_top: bool,
}

#[derive(Default)]
pub struct Viewer {
    context: Mutex<Option<Context>>,
    gate: tokio::sync::Mutex<()>,
}

#[tauri::command]
pub fn get_image_viewer(state: State<'_, Viewer>) -> Option<Context> {
    state
        .context
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

pub fn active(app: &AppHandle) -> bool {
    app.try_state::<Viewer>().is_some_and(|s| {
        s.context
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    })
}

pub fn set_top_preference(app: &AppHandle, value: bool) {
    if let Some(viewer) = app.try_state::<Viewer>()
        && let Some(context) = viewer
            .context
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
    {
        context.always_on_top = value;
    }
}

pub fn close(app: &AppHandle) -> Result<(), String> {
    let Some(viewer) = app.try_state::<Viewer>() else {
        return Ok(());
    };
    let context = viewer
        .context
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some(context) = context {
        if let Some(window) = app.get_webview_window("image-viewer") {
            window.set_fullscreen(false).map_err(|e| e.to_string())?;
            // Windows may show the restored window when leaving full screen.
            window.hide().map_err(|e| e.to_string())?;
        }
        if let Some(widget) = app.get_webview_window("widget") {
            widget
                .set_always_on_top(context.always_on_top)
                .map_err(|e| e.to_string())?;
        }
        let _ = app.emit("sherka:image-fullscreen", false);
    }
    Ok(())
}

#[tauri::command]
pub async fn image_fullscreen(
    app: AppHandle,
    shared: State<'_, Shared>,
    viewer: State<'_, Viewer>,
    open: bool,
    profile_id: Option<Uuid>,
) -> Result<(), String> {
    let _gate = viewer.gate.lock().await;
    if !open {
        return close(&app);
    }
    crate::windows::ensure_running()?;
    let context = with_core(shared.inner().clone(), move |core| {
        let id = core.session.snapshot().profile_id;
        let profile = core.profile(id)?;
        if profile_id != Some(id)
            || profile.details.connections.llm_provider.is_agent()
            || !profile.details.illustration.enabled
        {
            return Err(sherka_core::Error::StaleOperation);
        }
        let settings = core.snapshot()?.settings;
        Ok(Context {
            profile_id: id,
            slide_seconds: profile.details.illustration.slide_seconds,
            ui_language: settings.ui_language,
            theme: settings.theme,
            always_on_top: settings.always_on_top,
        })
    })
    .await?;
    let images = app.state::<crate::illustrations::Illustrations>().status();
    if images.profile_id != Some(context.profile_id) || images.images.is_empty() {
        return Err("Aucune image à afficher.".into());
    }
    let owner = context.profile_id;
    // Build from the async command, never a synchronous IPC/event handler:
    // WebView2 initialization itself needs the main event loop.
    let window = match app.get_webview_window("image-viewer") {
        Some(window) => window,
        None => WebviewWindowBuilder::new(
            &app,
            "image-viewer",
            WebviewUrl::App("index.html?view=image".into()),
        )
        .title("Sherka — Illustration")
        .decorations(false)
        .skip_taskbar(true)
        .visible(false)
        .background_color(tauri::window::Color(0, 0, 0, 255))
        .build()
        .map_err(|e| e.to_string())?,
    };
    let widget = app
        .get_webview_window("widget")
        .ok_or("Fenêtre indisponible.")?;
    let monitor = widget
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("Écran indisponible.")?;
    // The profile may have changed while the viewer was being created.
    if !crate::desktop_tools::is_owner(&app, owner) {
        return Err("Compagnon modifié.".into());
    }
    *viewer.context.lock().unwrap_or_else(|e| e.into_inner()) = Some(context);
    let result = (|| -> tauri::Result<()> {
        window.set_position(*monitor.position())?;
        window.set_size(*monitor.size())?;
        widget.set_always_on_top(true)?;
        window.set_fullscreen(true)?;
        window.show()?;
        window.set_focus()?;
        widget.show()?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = close(&app);
        return Err(error.to_string());
    }
    let _ = app.emit("sherka:image-fullscreen", true);
    Ok(())
}
