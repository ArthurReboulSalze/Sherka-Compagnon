use crate::{Shared, conversation::Voice};
use sherka_core::{model::Settings, window::Bounds};
use std::{
    sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    time::Duration,
};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WindowEvent,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

static GEOMETRY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static GEOMETRY_WORKER: AtomicBool = AtomicBool::new(false);
static MICROPHONE_ACTIVE: AtomicBool = AtomicBool::new(false);
// 0: running, 1: cleanup, 2: allowed to exit, 3: cleanup failed.
static QUIT_STATE: AtomicU8 = AtomicU8::new(0);
struct MicrophoneMenu(MenuItem<tauri::Wry>);
pub fn open_web_url(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
        let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
        let url: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
        // URL is selected from the stored search result and validated in Rust.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as isize > 32 {
            Ok(())
        } else {
            Err("Impossible d’ouvrir la source Internet.".into())
        }
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|_| "Impossible d’ouvrir la source Internet.".into())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = url;
        Err("Impossible d’ouvrir la source Internet.".into())
    }
}
struct LocalizedMenu(Vec<(MenuItem<tauri::Wry>, &'static str)>);

pub fn set_language(app: &AppHandle, language: sherka_core::i18n::Language) -> tauri::Result<()> {
    sherka_core::i18n::set_ui_language(language);
    if let Some(items) = app.try_state::<LocalizedMenu>() {
        for (item, source) in &items.0 {
            item.set_text(sherka_core::i18n::text(source))?;
        }
    }
    for (label, title) in [
        ("panel", "Sherka — Poste de bord"),
        ("widget", "Sherka — Compagnon"),
    ] {
        if let Some(window) = app.get_webview_window(label) {
            window.set_title(&sherka_core::i18n::text(title))?;
        }
    }
    paint_microphone(app, MICROPHONE_ACTIVE.load(Ordering::Relaxed));
    Ok(())
}

pub fn ensure_running() -> Result<(), String> {
    if matches!(QUIT_STATE.load(Ordering::SeqCst), 1 | 2) {
        Err("Sherka est en train de décharger son modèle avant de quitter.".into())
    } else {
        Ok(())
    }
}

pub fn exit_ready() -> bool {
    QUIT_STATE.load(Ordering::SeqCst) == 2
}

pub fn quit_without_unloading(app: &AppHandle) -> Result<(), String> {
    QUIT_STATE
        .compare_exchange(3, 2, Ordering::SeqCst, Ordering::SeqCst)
        .map_err(|_| "Le déchargement doit être tenté avant de quitter.".to_string())?;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        shutdown_services(&app).await;
        app.exit(0);
    });
    Ok(())
}

pub fn quit(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if prepare_shutdown(&app).await.is_ok() {
            app.exit(0);
        }
    });
}

pub fn update_failed() {
    QUIT_STATE.store(3, Ordering::SeqCst);
}

pub async fn prepare_shutdown(app: &AppHandle) -> Result<(), String> {
    QUIT_STATE
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |s| {
            matches!(s, 0 | 3).then_some(1)
        })
        .map_err(|_| "Une fermeture est déjà en cours.".to_string())?;
    if app.try_state::<crate::startup::Issue>().is_some() {
        // Recovery never initializes voice or owns a loaded model.
        QUIT_STATE.store(2, Ordering::SeqCst);
        return Ok(());
    }
    app.state::<crate::character_studio::Studio>().cancel(app);
    app.state::<crate::voice_startup::VoiceStartup>().cancel();
    crate::voice_warmup::stop();
    app.state::<crate::comfy_startup::ComfyStartup>().cancel();
    app.state::<crate::illustrations::Illustrations>()
        .cancel(app, true);
    let _ = app.emit("sherka:quit", serde_json::json!({"stage":"unloading"}));
    let voice = app.state::<Voice>().inner().clone();
    let shared = app.state::<Shared>().inner().clone();
    voice.playback.interrupt();
    let _gate = voice.gate.lock().await;
    let result = async {
        crate::conversation::pause(app, shared.clone(), &voice, true).await?;
        let active = crate::with_core(shared, |c| {
            Ok(c.profile(c.session.snapshot().profile_id)?
                .details
                .connections)
        })
        .await?;
        // Wait for any in-flight model load, then verify ownership and unloading.
        voice
            .models
            .shutdown(&active)
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    if result.is_ok() {
        shutdown_services(app).await;
    }
    match &result {
        Ok(()) => {
            crate::guest_window::close(app);
            save_geometry_now(app);
            QUIT_STATE.store(2, Ordering::SeqCst);
        }
        Err(error) => {
            QUIT_STATE.store(3, Ordering::SeqCst);
            let _ = show_panel(app);
            let _ = app.emit(
                "sherka:quit",
                serde_json::json!({"stage":"failed", "error":error}),
            );
        }
    }
    result
}

pub fn widget_visible(app: &AppHandle) -> tauri::Result<bool> {
    match app.get_webview_window("widget") {
        Some(window) => Ok(window.is_visible()? && !window.is_minimized()?),
        None => Ok(false),
    }
}

fn emit_widget_visibility(app: &AppHandle) -> tauri::Result<()> {
    app.emit("sherka:widget-visibility", widget_visible(app)?)
}

pub fn hide_widget(app: &AppHandle) -> tauri::Result<()> {
    let _ = crate::image_viewer::close(app);
    if let Some(window) = app.get_webview_window("widget") {
        window.hide()?;
    }
    crate::guest_window::hide(app);
    emit_widget_visibility(app)
}

pub fn toggle_widget(app: &AppHandle) -> tauri::Result<()> {
    if widget_visible(app)? {
        hide_widget(app)
    } else {
        restore_widget(app)
    }
}

pub fn show_panel(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("panel") {
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
    }
    Ok(())
}

pub fn restore_widget(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("widget") {
        window.set_ignore_cursor_events(false)?;
        window.unminimize()?;
        recover_widget(app, None)?;
        window.show()?;
        crate::guest_window::show(app);
    }
    emit_widget_visibility(app)
}

pub fn apply_preferences(app: &AppHandle, settings: &Settings) -> tauri::Result<()> {
    crate::image_viewer::set_top_preference(app, settings.always_on_top);
    if let Some(window) = app.get_webview_window("widget") {
        window.set_always_on_top(settings.always_on_top || crate::image_viewer::active(app))?;
        crate::guest_window::set_always_on_top(
            app,
            settings.always_on_top || crate::image_viewer::active(app),
        );
        let factor = f64::from(settings.widget_scale) / 100.0;
        if !crate::desktop_tools::resize_avatar(app, 290.0 * factor, 350.0 * factor) {
            window.set_size(tauri::LogicalSize::new(290.0 * factor, 350.0 * factor))?;
        }
    }
    Ok(())
}

pub fn recover_widget(app: &AppHandle, saved: Option<Bounds>) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window("widget") else {
        return Ok(());
    };
    let monitors = window.available_monitors()?;
    let areas: Vec<Bounds> = monitors
        .iter()
        .map(|monitor| {
            let area = monitor.work_area();
            Bounds {
                x: area.position.x,
                y: area.position.y,
                width: area.size.width,
                height: area.size.height,
            }
        })
        .collect();
    let size = window.outer_size()?;
    let position = window.outer_position()?;
    let current = saved.or(Some(Bounds {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    }));
    if let Some(bounds) = sherka_core::window::recover(current, &areas, (size.width, size.height)) {
        window.set_size(PhysicalSize::new(bounds.width, bounds.height))?;
        window.set_position(PhysicalPosition::new(bounds.x, bounds.y))?;
    }
    Ok(())
}

pub fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let restore = MenuItem::with_id(
        app,
        "restore",
        "Afficher Sherka / reprendre les clics",
        true,
        None::<&str>,
    )?;
    let panel = MenuItem::with_id(app, "panel", "Ouvrir le poste de bord", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause vocale globale", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "Arrêter la réponse", true, None::<&str>)?;
    let mic = MenuItem::with_id(app, "mic-status", "Microphone coupé", false, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    app.manage(MicrophoneMenu(mic.clone()));
    let quit = MenuItem::with_id(app, "quit", "Quitter Sherka", true, None::<&str>)?;
    app.manage(LocalizedMenu(vec![
        (restore.clone(), "Afficher Sherka / reprendre les clics"),
        (panel.clone(), "Ouvrir le poste de bord"),
        (pause.clone(), "Pause vocale globale"),
        (stop.clone(), "Arrêter la réponse"),
        (quit.clone(), "Quitter Sherka"),
    ]));
    let menu = Menu::with_items(
        app,
        &[&restore, &panel, &pause, &stop, &mic, &separator, &quit],
    )?;
    TrayIconBuilder::with_id("sherka")
        .icon(tauri::image::Image::from_bytes(include_bytes!(
            "../icons/32x32.png"
        ))?)
        .tooltip("Sherka · microphone coupé")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) && let Err(e) = restore_widget(tray.app_handle())
            {
                eprintln!("Restauration du widget impossible : {e}");
            }
        })
        .on_menu_event(|app, event| {
            let result = match event.id.as_ref() {
                "restore" => restore_widget(app),
                "panel" => show_panel(app),
                "pause" | "stop" => {
                    let shared = app.state::<Shared>().inner().clone();
                    let app = app.clone();
                    let pause = event.id.as_ref() == "pause";
                    crate::voice_warmup::cancel();
                    tauri::async_runtime::spawn(async move {
                        let voice = app.state::<Voice>().inner().clone();
                        let _gate = voice.gate.lock().await;
                        let result = crate::conversation::pause(&app, shared, &voice, pause).await;
                        if result.is_err() {
                            eprintln!("La commande de session a échoué.");
                        }
                        let _ = app.emit("sherka:changed", ());
                    });
                    Ok(())
                }
                "quit" => {
                    app.exit(0);
                    Ok(())
                }
                _ => Ok(()),
            };
            if let Err(e) = result {
                eprintln!("Commande de fenêtre impossible : {e}");
            }
        })
        .build(app)?;
    Ok(())
}

pub fn update_microphone(app: &AppHandle, listening: bool) {
    if MICROPHONE_ACTIVE.swap(listening, Ordering::Relaxed) == listening {
        return;
    }
    paint_microphone(app, listening);
}

fn paint_microphone(app: &AppHandle, listening: bool) {
    if let Some(tray) = app.tray_by_id("sherka") {
        let _ = tray.set_tooltip(Some(sherka_core::i18n::text(if listening {
            "Sherka · microphone actif, écoute continue"
        } else {
            "Sherka · microphone coupé"
        })));
    }
    if let Some(item) = app.try_state::<MicrophoneMenu>() {
        let _ = item.0.set_text(sherka_core::i18n::text(if listening {
            "Microphone actif · écoute continue"
        } else {
            "Microphone coupé"
        }));
    }
}

pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    match event {
        WindowEvent::CloseRequested { api, .. } if window.label() == "image-viewer" => {
            api.prevent_close();
            let _ = crate::image_viewer::close(window.app_handle());
        }
        WindowEvent::CloseRequested { api, .. } => {
            if window.app_handle().tray_by_id("sherka").is_some() {
                api.prevent_close();
                if let Err(e) = window.hide() {
                    eprintln!("Impossible de masquer la fenêtre : {e}");
                }
                if window.label() == "widget" {
                    let _ = emit_widget_visibility(window.app_handle());
                }
            }
        }
        WindowEvent::Moved(_) | WindowEvent::Resized(_) if window.label() == "widget" => {
            let _ = emit_widget_visibility(window.app_handle());
            // Debounce disk I/O, never perform it on the window event callback.
            GEOMETRY_SEQUENCE.fetch_add(1, Ordering::SeqCst);
            if !GEOMETRY_WORKER.swap(true, Ordering::SeqCst) {
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    loop {
                        let sequence = GEOMETRY_SEQUENCE.load(Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(450));
                        if GEOMETRY_SEQUENCE.load(Ordering::SeqCst) != sequence {
                            continue;
                        }
                        save_geometry_now(&app);
                        GEOMETRY_WORKER.store(false, Ordering::SeqCst);
                        if GEOMETRY_SEQUENCE.load(Ordering::SeqCst) == sequence
                            || GEOMETRY_WORKER.swap(true, Ordering::SeqCst)
                        {
                            break;
                        }
                    }
                });
            }
        }
        WindowEvent::Moved(_) | WindowEvent::Resized(_) if window.label() == "guest" => {
            crate::guest_window::geometry_changed(window.app_handle());
        }
        WindowEvent::ScaleFactorChanged { .. } if window.label() == "guest" => {
            crate::guest_window::recover(window.app_handle());
        }
        WindowEvent::ScaleFactorChanged { .. } if window.label() == "widget" => {
            if let Err(e) = recover_widget(window.app_handle(), None) {
                eprintln!("Repositionnement impossible : {e}");
            }
        }
        _ => {}
    }
}

pub fn save_geometry_now(app: &AppHandle) {
    // Finish all native window queries before acquiring the core lock.
    let Some((owner, bounds)) = crate::desktop_tools::avatar_bounds(app) else {
        return;
    };
    let Some(shared) = app.try_state::<Shared>() else {
        return;
    };
    let Ok(mut core) = shared.0.lock() else {
        return;
    };
    if core.session.snapshot().profile_id != owner || !crate::desktop_tools::is_owner(app, owner) {
        return;
    }
    if core.save_bounds(bounds).is_err() {
        eprintln!("La position du widget n'a pas été enregistrée.");
    }
}

pub async fn shutdown_services(app: &AppHandle) {
    let service = app
        .state::<crate::comfy_startup::ComfyStartup>()
        .inner()
        .clone();
    let closed = tauri::async_runtime::spawn_blocking(move || {
        let closed = crate::service_process::shutdown();
        service.shutdown();
        closed
    })
    .await
    .ok();
    record_voice_exit(app, closed);
}

/// Last chance, on the final exit event: a VoiceStudio server started by this
/// session never outlives Sherka, whatever path led here.
pub fn exit_cleanup(app: &AppHandle) {
    record_voice_exit(app, Some(crate::service_process::shutdown()));
}

// One fixed line in `audio-events.log`, once per run, telling what became of
// VoiceStudio so that the user can check after quitting.
fn record_voice_exit(app: &AppHandle, closed: Option<crate::service_process::Closed>) {
    static RECORDED: AtomicBool = AtomicBool::new(false);
    let (Some(voice), Some(startup)) = (
        app.try_state::<Voice>(),
        app.try_state::<crate::voice_startup::VoiceStartup>(),
    ) else {
        return;
    };
    if RECORDED.swap(true, Ordering::SeqCst) {
        return;
    }
    let running = startup.status().phase == "ready";
    if let Some(event) = crate::service_process::exit_event(closed.as_ref(), running) {
        voice.diagnostics.record(event, 0, 0, None);
    }
}
