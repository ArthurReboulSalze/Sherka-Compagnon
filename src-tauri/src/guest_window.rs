//! The guest companion of a three-way conversation lives in its own small
//! transparent window. It exists only while a guest is set (session-only) and
//! all controls stay on the master's widget.
use crate::{Shared, with_core};
use sherka_core::window::Bounds;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use uuid::Uuid;

const LABEL: &str = "guest";
const DEFAULT_SIZE: (f64, f64) = (230.0, 280.0);
const GAP: i32 = 8;

/// Serializes window creation, repositioning and closing.
static GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// The guest whose geometry the window currently represents.
static CURRENT: Mutex<Option<Uuid>> = Mutex::new(None);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static WORKER: AtomicBool = AtomicBool::new(false);

fn current() -> Option<Uuid> {
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner())
}

fn set_current(value: Option<Uuid>) {
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = value;
}

fn work_areas(window: &WebviewWindow) -> tauri::Result<Vec<Bounds>> {
    Ok(window
        .available_monitors()?
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
        .collect())
}

/// Left of the widget, bottom aligned; on its right when the left side is off screen.
fn default_bounds(app: &AppHandle) -> tauri::Result<Option<Bounds>> {
    let Some(widget) = app.get_webview_window("widget") else {
        return Ok(None);
    };
    let scale = widget.scale_factor()?;
    let width = (DEFAULT_SIZE.0 * scale).round() as u32;
    let height = (DEFAULT_SIZE.1 * scale).round() as u32;
    let position = widget.outer_position()?;
    let size = widget.outer_size()?;
    let mut x = position.x - width as i32 - GAP;
    if let Some(monitor) = widget.current_monitor()?
        && x < monitor.work_area().position.x
    {
        x = position.x + size.width as i32 + GAP;
    }
    Ok(Some(Bounds {
        x,
        y: position.y + size.height as i32 - height as i32,
        width,
        height,
    }))
}

fn place(app: &AppHandle, window: &WebviewWindow, saved: Option<Bounds>) -> tauri::Result<()> {
    let areas = work_areas(window)?;
    let wanted = match saved {
        Some(bounds) => Some(bounds),
        None => default_bounds(app)?,
    };
    let scale = window.scale_factor()?;
    let fallback = (
        (DEFAULT_SIZE.0 * scale).round() as u32,
        (DEFAULT_SIZE.1 * scale).round() as u32,
    );
    if let Some(bounds) = sherka_core::window::recover(wanted, &areas, fallback) {
        window.set_size(PhysicalSize::new(bounds.width, bounds.height))?;
        window.set_position(PhysicalPosition::new(bounds.x, bounds.y))?;
    }
    Ok(())
}

async fn build(app: &AppHandle, always_on_top: bool, title: &str) -> Result<WebviewWindow, String> {
    // The previous window may still be leaving the registry on the main thread.
    let mut attempts = 0;
    loop {
        // Build from an async command, never a synchronous IPC/event handler:
        // WebView2 initialization itself needs the main event loop.
        let built =
            WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?view=guest".into()))
                .title(title)
                .inner_size(DEFAULT_SIZE.0, DEFAULT_SIZE.1)
                .min_inner_size(188.0, 228.0)
                .max_inner_size(600.0, 720.0)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .resizable(false)
                .always_on_top(always_on_top)
                .skip_taskbar(true)
                .visible(false)
                .focused(false)
                .background_color(tauri::window::Color(0, 0, 0, 0))
                .disable_drag_drop_handler()
                .build();
        match built {
            Ok(window) => return Ok(window),
            Err(_) if attempts < 5 && app.get_webview_window(LABEL).is_none() => {
                attempts += 1;
                tokio::time::sleep(Duration::from_millis(60)).await;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

/// Opens, repositions or closes the guest window to match the core's guest.
pub async fn sync(app: &AppHandle, shared: &Shared) -> Result<(), String> {
    let _gate = GATE.lock().await;
    let (guest, name, always_on_top, saved) = with_core(shared.clone(), |c| {
        let guest = c.guest();
        let name = match guest {
            Some(id) => Some(c.profile(id)?.details.name),
            None => None,
        };
        let saved = guest.and_then(|id| c.guest_bounds(id));
        Ok((guest, name, c.snapshot()?.settings.always_on_top, saved))
    })
    .await?;
    let existing = app.get_webview_window(LABEL);
    let (Some(guest), Some(name)) = (guest, name) else {
        set_current(None);
        if let Some(window) = existing {
            // destroy() skips CloseRequested, which would only hide the window.
            window.destroy().map_err(|e| e.to_string())?;
        }
        return Ok(());
    };
    let created = existing.is_none();
    let title = format!("Sherka — {name}");
    let window = match existing {
        Some(window) => window,
        None => build(app, always_on_top, &title).await?,
    };
    let _ = window.set_title(&title);
    if created || current() != Some(guest) {
        // No geometry is saved while the window is moved programmatically.
        set_current(None);
        place(app, &window, saved).map_err(|e| e.to_string())?;
        set_current(Some(guest));
    }
    if crate::windows::widget_visible(app).unwrap_or(false) {
        window.show().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Shows the window with the widget (it only exists while a guest is set).
pub fn show(app: &AppHandle) {
    if current().is_some()
        && let Some(window) = app.get_webview_window(LABEL)
    {
        let _ = window.set_ignore_cursor_events(false);
        let _ = window.show();
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

pub fn click_through(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.set_ignore_cursor_events(true);
    }
}

pub fn set_always_on_top(app: &AppHandle, value: bool) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.set_always_on_top(value);
    }
}

/// Saves the final position, then removes the window (application shutdown).
pub fn close(app: &AppHandle) {
    save_now(app);
    set_current(None);
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}

/// Keeps the window inside a monitor after a screen or scale change.
pub fn recover(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    let current = Bounds {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    if let Err(e) = place(app, &window, Some(current)) {
        eprintln!("Repositionnement impossible : {e}");
    }
}

/// Writes the window geometry for the current guest when it changed.
pub fn save_now(app: &AppHandle) {
    let Some(id) = current() else {
        return;
    };
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    // Finish all native window queries before acquiring the core lock.
    let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    if size.width == 0 || size.height == 0 {
        return;
    }
    let bounds = Bounds {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    let Some(shared) = app.try_state::<Shared>() else {
        return;
    };
    let Ok(mut core) = shared.0.lock() else {
        return;
    };
    if core.guest_bounds(id) == Some(bounds) {
        return;
    }
    // StaleOperation: the guest left meanwhile, so there is nothing to keep.
    let _ = core.save_guest_bounds(id, bounds);
}

/// Debounced save for move and resize events (never perform I/O in the callback).
pub fn geometry_changed(app: &AppHandle) {
    SEQUENCE.fetch_add(1, Ordering::SeqCst);
    if !WORKER.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            loop {
                let sequence = SEQUENCE.load(Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(450));
                if SEQUENCE.load(Ordering::SeqCst) != sequence {
                    continue;
                }
                save_now(&app);
                WORKER.store(false, Ordering::SeqCst);
                if SEQUENCE.load(Ordering::SeqCst) == sequence
                    || WORKER.swap(true, Ordering::SeqCst)
                {
                    break;
                }
            }
        });
    }
}
