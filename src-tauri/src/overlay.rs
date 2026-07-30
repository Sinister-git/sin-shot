//! Overlay window management.
//!
//! Commands that control the capture overlay window: show, hide, resize,
//! and enumerate monitors.

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, Position, Size,
    WebviewWindow,
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Structured monitor info returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    tracing::info!("Starting capture mode: {}", mode);
    let window = get_main_window(&app)?;

    // Compute the bounding box that encompasses all monitors.
    let monitors = window.available_monitors().map_err(|e| {
        let msg = format!("failed to enumerate monitors: {e}");
        tracing::error!("{}", msg);
        msg
    })?;

    if monitors.is_empty() {
        tracing::error!("No monitors available for capture");
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
        .set_position(Position::Physical(PhysicalPosition { x: min_x, y: min_y }))
        .map_err(|e| {
            let msg = format!("set_position: {e}");
            tracing::error!("{}", msg);
            msg
        })?;

    window
        .set_size(Size::Physical(PhysicalSize {
            width: total_width,
            height: total_height,
        }))
        .map_err(|e| {
            let msg = format!("set_size: {e}");
            tracing::error!("{}", msg);
            msg
        })?;

    // Bring the window to the foreground and show it.
    window.show().map_err(|e| {
        let msg = format!("show: {e}");
        tracing::error!("{}", msg);
        msg
    })?;
    window.set_focus().map_err(|e| {
        let msg = format!("set_focus: {e}");
        tracing::error!("{}", msg);
        msg
    })?;

    // Notify the frontend which mode to display.
    window
        .emit("capture-mode-started", serde_json::json!({ "mode": mode }))
        .map_err(|e| {
            let msg = format!("emit capture-mode-started: {e}");
            tracing::error!("{}", msg);
            msg
        })?;

    Ok(())
}

/// Hide the overlay window and reset it to its default size.
#[tauri::command]
pub async fn cancel_capture(app: AppHandle) -> Result<(), String> {
    let window = get_main_window(&app)?;

    window.hide().map_err(|e| {
        let msg = format!("hide: {e}");
        tracing::error!("{}", msg);
        msg
    })?;

    // Reset to default compact dimensions so the next show isn't full-screen.
    window
        .set_size(Size::Physical(PhysicalSize {
            width: 800,
            height: 600,
        }))
        .map_err(|e| {
            let msg = format!("set_size: {e}");
            tracing::error!("{}", msg);
            msg
        })?;

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

    window.emit("capture-mode-cancelled", ()).map_err(|e| {
        let msg = format!("emit capture-mode-cancelled: {e}");
        tracing::error!("{}", msg);
        msg
    })?;

    Ok(())
}

/// Return information about all connected monitors.
#[tauri::command]
pub async fn get_monitors(app: AppHandle) -> Result<Vec<MonitorInfo>, String> {
    let window = get_main_window(&app)?;

    let monitors = window.available_monitors().map_err(|e| {
        let msg = format!("failed to enumerate monitors: {e}");
        tracing::error!("{}", msg);
        msg
    })?;

    let primary: Option<Monitor> = window.primary_monitor().map_err(|e| {
        let msg = format!("primary_monitor: {e}");
        tracing::error!("{}", msg);
        msg
    })?;

    let mut result = Vec::new();
    for m in &monitors {
        let size = m.size();
        let pos = m.position();
        let is_primary = primary.as_ref().is_some_and(|pm| {
            let pp = pm.position();
            let ps = pm.size();
            pp.x == pos.x && pp.y == pos.y && ps.width == size.width && ps.height == size.height
        });
        result.push(MonitorInfo {
            name: m
                .name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Unknown".to_string()),
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_info_serializes_correctly() {
        let info = MonitorInfo {
            name: "DELL U2719D".into(),
            width: 2560,
            height: 1440,
            x: 0,
            y: 0,
            is_primary: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("DELL U2719D"));
        assert!(json.contains("2560"));
        assert!(json.contains("1440"));
        assert!(json.contains("\"is_primary\":true"));
    }

    #[test]
    fn monitor_info_is_primary_false() {
        let info = MonitorInfo {
            name: "Secondary".into(),
            width: 1920,
            height: 1080,
            x: 2560,
            y: 0,
            is_primary: false,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"is_primary\":false"));
    }

    /// Verify we can deserialize MonitorInfo JSON — the frontend receives
    /// this as the result of get_monitors().
    #[test]
    fn monitor_info_deserializes_from_json() {
        let json = r#"{"name":"Test","width":1920,"height":1080,"x":0,"y":0,"is_primary":true}"#;
        let info: MonitorInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.name, "Test");
        assert_eq!(info.width, 1920);
        assert_eq!(info.height, 1080);
        assert_eq!(info.x, 0);
        assert_eq!(info.y, 0);
        assert!(info.is_primary);
    }

    /// Test bounding-box computation for the overlay window.
    /// This mirrors the logic in start_capture.
    #[test]
    fn bounding_box_single_monitor() {
        let monitors = vec![MonitorInfo {
            name: "Main".into(),
            width: 1920,
            height: 1080,
            x: 0,
            y: 0,
            is_primary: true,
        }];
        let (min_x, min_y, max_x, max_y) = compute_bounds(&monitors);
        assert_eq!(min_x, 0);
        assert_eq!(min_y, 0);
        assert_eq!(max_x, 1920);
        assert_eq!(max_y, 1080);
        assert_eq!((max_x - min_x) as u32, 1920);
        assert_eq!((max_y - min_y) as u32, 1080);
    }

    #[test]
    fn bounding_box_dual_monitor_horizontal() {
        // Primary on left, secondary on right.
        let monitors = vec![
            MonitorInfo {
                name: "Left".into(),
                width: 1920,
                height: 1080,
                x: 0,
                y: 0,
                is_primary: true,
            },
            MonitorInfo {
                name: "Right".into(),
                width: 1920,
                height: 1080,
                x: 1920,
                y: 0,
                is_primary: false,
            },
        ];
        let (min_x, min_y, max_x, max_y) = compute_bounds(&monitors);
        assert_eq!(min_x, 0);
        assert_eq!(min_y, 0);
        assert_eq!(max_x, 3840);
        assert_eq!(max_y, 1080);
        assert_eq!((max_x - min_x) as u32, 3840);
        assert_eq!((max_y - min_y) as u32, 1080);
    }

    #[test]
    fn bounding_box_dual_monitor_with_negative_offset() {
        // Primary centered, secondary to the left with negative x.
        let monitors = vec![
            MonitorInfo {
                name: "Left".into(),
                width: 1920,
                height: 1080,
                x: -1920,
                y: 0,
                is_primary: false,
            },
            MonitorInfo {
                name: "Primary".into(),
                width: 2560,
                height: 1440,
                x: 0,
                y: 0,
                is_primary: true,
            },
        ];
        let (min_x, min_y, max_x, max_y) = compute_bounds(&monitors);
        assert_eq!(min_x, -1920);
        assert_eq!(min_y, 0);
        assert_eq!(max_x, 2560);
        assert_eq!(max_y, 1440);
        assert_eq!((max_x - min_x) as u32, 4480); // 1920 + 2560
        assert_eq!((max_y - min_y) as u32, 1440);
    }

    #[test]
    fn bounding_box_dual_monitor_vertical_stack() {
        // Primary on bottom, secondary above with negative y.
        let monitors = vec![
            MonitorInfo {
                name: "Top".into(),
                width: 1920,
                height: 1080,
                x: 0,
                y: -1080,
                is_primary: false,
            },
            MonitorInfo {
                name: "Bottom".into(),
                width: 1920,
                height: 1080,
                x: 0,
                y: 0,
                is_primary: true,
            },
        ];
        let (min_x, min_y, max_x, max_y) = compute_bounds(&monitors);
        assert_eq!(min_x, 0);
        assert_eq!(min_y, -1080);
        assert_eq!(max_x, 1920);
        assert_eq!(max_y, 1080);
        assert_eq!((max_x - min_x) as u32, 1920);
        assert_eq!((max_y - min_y) as u32, 2160); // 1080 + 1080
    }

    #[test]
    fn bounding_box_empty_monitors_returns_zeros() {
        let monitors: Vec<MonitorInfo> = vec![];
        let (min_x, min_y, max_x, max_y) = compute_bounds(&monitors);
        // Start values should remain at extremes.
        assert_eq!(min_x, i32::MAX);
        assert_eq!(min_y, i32::MAX);
        assert_eq!(max_x, i32::MIN);
        assert_eq!(max_y, i32::MIN);
    }

    /// Pure-function bounding-box helper extracted from start_capture logic.
    fn compute_bounds(monitors: &[MonitorInfo]) -> (i32, i32, i32, i32) {
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        for m in monitors {
            min_x = min_x.min(m.x);
            min_y = min_y.min(m.y);
            max_x = max_x.max(m.x + m.width as i32);
            max_y = max_y.max(m.y + m.height as i32);
        }

        (min_x, min_y, max_x, max_y)
    }
}
