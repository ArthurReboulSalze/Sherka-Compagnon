//! Inline cells share the avatar's transparent native surface.
use crate::{Shared, with_core};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Text,
    Image,
}
#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Surface {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub base_width: f64,
    pub base_height: f64,
}
#[derive(Default)]
struct CanvasState {
    surface: Surface,
    correction: (f64, f64),
}
#[derive(Default)]
pub struct Canvas(Mutex<CanvasState>);
pub fn resize_avatar(app: &AppHandle, width: f64, height: f64) -> bool {
    let Some(canvas) = app.try_state::<Canvas>() else {
        return false;
    };
    if canvas
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .surface
        .base_width
        == 0.0
    {
        return false;
    }
    let _ = app.emit(
        "sherka:avatar-size",
        (width.clamp(188.0, 600.0), height.clamp(228.0, 720.0)),
    );
    true
}
pub async fn set_image_enabled(
    app: &AppHandle,
    shared: Shared,
    enabled: bool,
) -> Result<(), String> {
    with_core(shared.clone(), move |c| {
        let id = c.session.snapshot().profile_id;
        let p = c.store.profile(id)?;
        let mut draft = p.details;
        if enabled && draft.connections.llm_provider.is_agent() {
            return Err(sherka_core::Error::StaleOperation);
        }
        draft.illustration.enabled = enabled;
        c.update_profile(id, p.revision, draft)
    })
    .await?;
    crate::illustrations::synchronize(app, &shared);
    let _ = app.emit("sherka:changed", ());
    Ok(())
}
#[tauri::command]
pub async fn desktop_tool(
    app: AppHandle,
    shared: State<'_, Shared>,
    tool: Tool,
    open: bool,
) -> Result<(), String> {
    crate::windows::ensure_running()?;
    if matches!(tool, Tool::Image) {
        set_image_enabled(&app, shared.inner().clone(), open).await?;
        if open {
            app.state::<crate::illustrations::Illustrations>()
                .request(app.clone(), shared.inner().clone());
        }
    }
    Ok(())
}
#[tauri::command]
pub fn widget_surface(
    app: AppHandle,
    state: State<'_, Canvas>,
    surface: Option<Surface>,
) -> Result<Surface, String> {
    let window = app
        .get_webview_window("widget")
        .ok_or("Fenêtre indisponible.")?;
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let size = window.inner_size().map_err(|e| e.to_string())?;
    let (previous, correction) = {
        let mut value = state.0.lock().unwrap_or_else(|e| e.into_inner());
        if value.surface.base_width == 0.0 {
            value.surface.width = size.width as f64 / scale;
            value.surface.height = size.height as f64 / scale;
            value.surface.base_width = value.surface.width;
            value.surface.base_height = value.surface.height;
        }
        (value.surface, value.correction)
    };
    let Some(next) = surface else {
        return Ok(previous);
    };
    if [
        next.left,
        next.top,
        next.width,
        next.height,
        next.base_width,
        next.base_height,
    ]
    .iter()
    .any(|n| !n.is_finite())
        || !(-1200.0..=0.0).contains(&next.left)
        || !(-1200.0..=0.0).contains(&next.top)
        || !(188.0..=1800.0).contains(&next.width)
        || !(228.0..=1600.0).contains(&next.height)
        || !(188.0..=600.0).contains(&next.base_width)
        || !(228.0..=720.0).contains(&next.base_height)
        || next.width + next.left + 1.0 < next.base_width
        || next.height + next.top + 1.0 < next.base_height
    {
        return Err("La taille du widget est invalide.".into());
    }
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let desired_x = position.x as f64 + (next.left - previous.left) * scale - correction.0;
    let desired_y = position.y as f64 + (next.top - previous.top) * scale - correction.1;
    let (mut x, mut y) = (desired_x, desired_y);
    if let Ok(Some(monitor)) = window.current_monitor() {
        let area = monitor.work_area();
        x = x.clamp(
            area.position.x as f64,
            (area.position.x as f64 + area.size.width as f64 - next.width * scale)
                .max(area.position.x as f64),
        );
        y = y.clamp(
            area.position.y as f64,
            (area.position.y as f64 + area.size.height as f64 - next.height * scale)
                .max(area.position.y as f64),
        );
    }
    window
        .set_max_size::<tauri::LogicalSize<f64>>(None)
        .map_err(|e| e.to_string())?;
    window
        .set_size(tauri::LogicalSize::new(next.width, next.height))
        .map_err(|e| e.to_string())?;
    window
        .set_position(tauri::PhysicalPosition::new(
            x.round() as i32,
            y.round() as i32,
        ))
        .map_err(|e| e.to_string())?;
    *state.0.lock().unwrap_or_else(|e| e.into_inner()) = CanvasState {
        surface: next,
        correction: (x - desired_x, y - desired_y),
    };
    Ok(next)
}
pub fn avatar_bounds(app: &AppHandle) -> Option<sherka_core::window::Bounds> {
    let window = app.get_webview_window("widget")?;
    let state = app.try_state::<Canvas>()?;
    let value = state.0.lock().ok()?;
    let surface = value.surface;
    if surface.base_width == 0.0 {
        return None;
    }
    let position = window.outer_position().ok()?;
    let scale = window.scale_factor().ok()?;
    Some(sherka_core::window::Bounds {
        x: position.x - (surface.left * scale + value.correction.0).round() as i32,
        y: position.y - (surface.top * scale + value.correction.1).round() as i32,
        width: (surface.base_width * scale).round() as u32,
        height: (surface.base_height * scale).round() as u32,
    })
}
