//! Overlay window management.
//!
//! Commands that control the capture overlay window: show, hide, resize,
//! and enumerate monitors.

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, Position, Size,
    WebviewWindow,
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Structured monitor info returned to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct MonitorInfo {
    /// Human-readable name, e.g. "DELL U2719D".
    pub name: String,
    /// Monitor width in physical pixels.
    pub width: u32,
    /// Monitor height in physical pixels.
    pub height: u32,
    /// X offset of the monitor's top-left corner.
    pub x: i32,
    /// Y offset of the monitor's top-left corner.
    pub y: i32,
    /// Whether this is the primary monitor.
    pub is_primary: bool,
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Resize and show the overlay window to cover all available monitors,
/// then emit a `capture-mode-started` event so the frontend can render
/// the appropriate overlay UI.
#[tauri::command]
pub async fn start_capture(app: AppHandle, mode: String) -> Result<(), String> {
    let window = get_main_window(&app)?;

    // Compute the bounding box that encompasses all monitors.
    let monitors = window
        .available_monitors()
        .map_err(|e| format!("failed to enumerate monitors: {e}"))?;

    if monitors.is_empty() {
        return Err("no monitors available".into());
    }

    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for m in &monitors {
        let pos = m.position();
        let size = m.size();
        min_x = min_x.min(pos.x);
        min_y = min_y.min(pos.y);
        max_x = max_x.max(pos.x + size.width as i32);
        max_y = max_y.max(pos.y + size.height as i32);
    }

    let total_width = (max_x - min_x) as u32;
    let total_height = (max_y - min_y) as u32;

    // Resize and reposition the window to cover the entire monitor space.
    window
        .set_position(Position::Physical(PhysicalPosition {
            x: min_x,
            y: min_y,
        }))
        .map_err(|e| format!("set_position: {e}"))?;

    window
        .set_size(Size::Physical(PhysicalSize {
            width: total_width,
            height: total_height,
        }))
        .map_err(|e| format!("set_size: {e}"))?;

    // Bring the window to the foreground and show it.
    window.show().map_err(|e| format!("show: {e}"))?;
    window
        .set_focus()
        .map_err(|e| format!("set_focus: {e}"))?;

    // Notify the frontend which mode to display.
    window
        .emit("capture-mode-started", serde_json::json!({ "mode": mode }))
        .map_err(|e| format!("emit: {e}"))?;

    Ok(())
}

/// Hide the overlay window and reset it to its default size.
#[tauri::command]
pub async fn cancel_capture(app: AppHandle) -> Result<(), String> {
    let window = get_main_window(&app)?;

    window.hide().map_err(|e| format!("hide: {e}"))?;

    // Reset to default compact dimensions so the next show isn't full-screen.
    window
        .set_size(Size::Physical(PhysicalSize {
            width: 800,
            height: 600,
        }))
        .map_err(|e| format!("set_size: {e}"))?;

    // Reset position to center on primary monitor.
    if let Ok(Some(pm)) = window.primary_monitor() {
        let ppos = pm.position();
        let psize = pm.size();
        let cx = ppos.x + (psize.width as i32 - 800) / 2;
        let cy = ppos.y + (psize.height as i32 - 600) / 2;
        let _ = window.set_position(Position::Physical(PhysicalPosition {
            x: cx.max(0),
            y: cy.max(0),
        }));
    }

    window
        .emit("capture-mode-cancelled", ())
        .map_err(|e| format!("emit: {e}"))?;

    Ok(())
}

/// Return information about all connected monitors.
#[tauri::command]
pub async fn get_monitors(app: AppHandle) -> Result<Vec<MonitorInfo>, String> {
    let window = get_main_window(&app)?;

    let monitors = window
        .available_monitors()
        .map_err(|e| format!("failed to enumerate monitors: {e}"))?;

    let primary: Option<Monitor> = window
        .primary_monitor()
        .map_err(|e| format!("primary_monitor: {e}"))?;

    let mut result = Vec::new();
    for m in &monitors {
        let size = m.size();
        let pos = m.position();
        let is_primary = primary.as_ref().map_or(false, |pm| {
            let pp = pm.position();
            let ps = pm.size();
            pp.x == pos.x && pp.y == pos.y && ps.width == size.width && ps.height == size.height
        });
        result.push(MonitorInfo {
            name: m.name().map(|s| s.to_string()).unwrap_or_else(|| "Unknown".to_string()),
            width: size.width,
            height: size.height,
            x: pos.x,
            y: pos.y,
            is_primary,
        });
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get the main (currently the only) Tauri webview window.
fn get_main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())
}
