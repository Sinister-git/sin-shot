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
    /// Windows per-monitor scale factor. Bounds remain physical pixels.
    #[serde(default = "default_scale_factor")]
    pub scale_factor: f64,
}

/// Physical bounds and scale of the overlay *client* after placement.
///
/// `origin_x/y` and `width/height` come from `inner_position`/`inner_size`,
/// rather than from the rectangle requested from the native window manager.
/// They are therefore the coordinate system in which WebView CSS coordinates
/// begin at (0, 0). Monitor bounds and client bounds are both physical desktop
/// pixels; `scale_factor` is the single physical-pixel/CSS-pixel conversion at
/// the WebView boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayGeometry {
    pub monitors: Vec<MonitorInfo>,
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

fn default_scale_factor() -> f64 {
    1.0
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Resize and show the overlay window to cover all available monitors,
/// then emit a `capture-mode-started` event so the frontend can render
/// the appropriate overlay UI.
#[tauri::command]
pub async fn start_capture(app: AppHandle, mode: String) -> Result<OverlayGeometry, String> {
    tracing::info!("Starting capture mode: {}", mode);
    let window = get_main_window(&app)?;

    // Take one native snapshot for both window placement and frontend
    // rendering. Every coordinate below is a physical desktop pixel.
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
    let primary = window.primary_monitor().map_err(|e| {
        let msg = format!("primary_monitor: {e}");
        tracing::error!("{}", msg);
        msg
    })?;
    let monitor_info = monitors
        .iter()
        .map(|monitor| monitor_info_from_monitor(monitor, primary.as_ref()))
        .collect::<Vec<_>>();

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

    // A native shadow or DPI transition may leave the client rectangle
    // different from the requested outer rectangle. Correct both position and
    // size using the measured native geometry; this is not a CSS offset or
    // monitor-specific adjustment. The configured overlay shadow is disabled,
    // but the readback keeps this contract valid if Windows applies another
    // non-client inset.
    let measured_client = window.inner_position().map_err(|e| {
        let msg = format!("inner_position before alignment: {e}");
        tracing::error!("{}", msg);
        msg
    })?;
    let current_outer = window.outer_position().map_err(|e| {
        let msg = format!("outer_position before alignment: {e}");
        tracing::error!("{}", msg);
        msg
    })?;
    let current_outer_size = window.outer_size().map_err(|e| {
        let msg = format!("outer_size before alignment: {e}");
        tracing::error!("{}", msg);
        msg
    })?;
    let measured_client_size = window.inner_size().map_err(|e| {
        let msg = format!("inner_size before alignment: {e}");
        tracing::error!("{}", msg);
        msg
    })?;
    let desired_client = PhysicalSize {
        width: total_width,
        height: total_height,
    };
    let corrected_outer = corrected_outer_position(
        current_outer,
        measured_client,
        PhysicalPosition { x: min_x, y: min_y },
    );
    let corrected_size =
        corrected_outer_size(current_outer_size, measured_client_size, desired_client);
    if corrected_outer != current_outer {
        window
            .set_position(Position::Physical(corrected_outer))
            .map_err(|e| {
                let msg = format!("set_position client alignment: {e}");
                tracing::error!("{}", msg);
                msg
            })?;
    }
    if corrected_size != current_outer_size {
        window
            .set_size(Size::Physical(corrected_size))
            .map_err(|e| {
                let msg = format!("set_size client alignment: {e}");
                tracing::error!("{}", msg);
                msg
            })?;
    }

    // Query the *actual* client rectangle after placement. On Windows the
    // requested native rectangle is not necessarily the WebView rectangle:
    // frameless shadows/non-client insets and DPI transitions can change both
    // its origin and extent. CSS coordinates start at this client origin, so
    // returning the readback is the authoritative native/WebView contract.
    // `inner_position`/`inner_size` are supported by the Windows Tauri runtime;
    // failure is reported rather than silently reverting to guessed geometry.
    let client_position = window.inner_position().map_err(|e| {
        let msg = format!("inner_position after placement: {e}");
        tracing::error!("{}", msg);
        msg
    })?;
    let client_size = window.inner_size().map_err(|e| {
        let msg = format!("inner_size after placement: {e}");
        tracing::error!("{}", msg);
        msg
    })?;
    if !client_covers_bounds(
        client_position,
        client_size,
        PhysicalPosition { x: min_x, y: min_y },
        desired_client,
    ) {
        let msg = format!(
            "overlay client bounds do not cover virtual desktop: client=({}, {}) {}x{}, requested=({}, {}) {}x{}",
            client_position.x,
            client_position.y,
            client_size.width,
            client_size.height,
            min_x,
            min_y,
            total_width,
            total_height,
        );
        tracing::error!("{}", msg);
        return Err(msg);
    }

    let scale_factor = window.scale_factor().map_err(|e| {
        let msg = format!("scale_factor after placement: {e}");
        tracing::error!("{}", msg);
        msg
    })?;
    if scale_factor <= 0.0 {
        return Err(format!("invalid overlay scale factor: {scale_factor}"));
    }

    tracing::debug!(
        requested_x = min_x,
        requested_y = min_y,
        requested_width = total_width,
        requested_height = total_height,
        client_x = client_position.x,
        client_y = client_position.y,
        client_width = client_size.width,
        client_height = client_size.height,
        scale_factor,
        "overlay placement readback"
    );

    let geometry = OverlayGeometry {
        monitors: monitor_info,
        origin_x: client_position.x,
        origin_y: client_position.y,
        width: client_size.width,
        height: client_size.height,
        scale_factor,
    };

    // Notify the frontend with the same snapshot used for native placement.
    window
        .emit(
            "capture-mode-started",
            serde_json::json!({ "mode": mode, "geometry": geometry }),
        )
        .map_err(|e| {
            let msg = format!("emit capture-mode-started: {e}");
            tracing::error!("{}", msg);
            msg
        })?;

    Ok(geometry)
}

/// Hide the overlay window without changing capture state.
#[tauri::command]
pub async fn hide_capture_overlay(app: AppHandle) -> Result<(), String> {
    get_main_window(&app)?
        .hide()
        .map_err(|e| format!("hide: {e}"))
}

/// Show the overlay window without changing capture state.
#[tauri::command]
pub async fn show_capture_overlay(app: AppHandle) -> Result<(), String> {
    let window = get_main_window(&app)?;
    window.show().map_err(|e| format!("show: {e}"))?;
    window.set_focus().map_err(|e| format!("set_focus: {e}"))
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
        result.push(monitor_info_from_monitor(m, primary.as_ref()));
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn corrected_outer_size(
    current_outer: PhysicalSize<u32>,
    measured_client: PhysicalSize<u32>,
    desired_client: PhysicalSize<u32>,
) -> PhysicalSize<u32> {
    PhysicalSize {
        width: current_outer
            .width
            .saturating_add(desired_client.width.saturating_sub(measured_client.width))
            .max(desired_client.width),
        height: current_outer
            .height
            .saturating_add(desired_client.height.saturating_sub(measured_client.height))
            .max(desired_client.height),
    }
}

fn client_covers_bounds(
    client_position: PhysicalPosition<i32>,
    client_size: PhysicalSize<u32>,
    desired_position: PhysicalPosition<i32>,
    desired_size: PhysicalSize<u32>,
) -> bool {
    let client_right = i64::from(client_position.x) + i64::from(client_size.width);
    let client_bottom = i64::from(client_position.y) + i64::from(client_size.height);
    let desired_right = i64::from(desired_position.x) + i64::from(desired_size.width);
    let desired_bottom = i64::from(desired_position.y) + i64::from(desired_size.height);

    i64::from(client_position.x) <= i64::from(desired_position.x)
        && i64::from(client_position.y) <= i64::from(desired_position.y)
        && client_right >= desired_right
        && client_bottom >= desired_bottom
}

fn corrected_outer_position(
    current_outer: PhysicalPosition<i32>,
    measured_client: PhysicalPosition<i32>,
    desired_client: PhysicalPosition<i32>,
) -> PhysicalPosition<i32> {
    PhysicalPosition {
        x: current_outer.x + desired_client.x - measured_client.x,
        y: current_outer.y + desired_client.y - measured_client.y,
    }
}

fn monitor_info_from_monitor(monitor: &Monitor, primary: Option<&Monitor>) -> MonitorInfo {
    let size = monitor.size();
    let pos = monitor.position();
    let is_primary = primary.is_some_and(|pm| {
        let pp = pm.position();
        let ps = pm.size();
        pp.x == pos.x && pp.y == pos.y && ps.width == size.width && ps.height == size.height
    });

    MonitorInfo {
        name: monitor
            .name()
            .map(ToString::to_string)
            .unwrap_or_else(|| "Unknown".to_string()),
        width: size.width,
        height: size.height,
        x: pos.x,
        y: pos.y,
        is_primary,
        scale_factor: monitor.scale_factor(),
    }
}

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
    fn client_bounds_cover_requested_virtual_desktop() {
        assert!(client_covers_bounds(
            PhysicalPosition { x: -100, y: 20 },
            PhysicalSize {
                width: 2200,
                height: 1100,
            },
            PhysicalPosition { x: 0, y: 20 },
            PhysicalSize {
                width: 1920,
                height: 1080,
            },
        ));
        assert!(!client_covers_bounds(
            PhysicalPosition { x: 0, y: 20 },
            PhysicalSize {
                width: 1919,
                height: 1080,
            },
            PhysicalPosition { x: 0, y: 20 },
            PhysicalSize {
                width: 1920,
                height: 1080,
            },
        ));
        assert!(!client_covers_bounds(
            PhysicalPosition { x: 1, y: 20 },
            PhysicalSize {
                width: 1920,
                height: 1080,
            },
            PhysicalPosition { x: 0, y: 20 },
            PhysicalSize {
                width: 1920,
                height: 1080,
            },
        ));
    }

    #[test]
    fn monitor_info_serializes_correctly() {
        let info = MonitorInfo {
            name: "DELL U2719D".into(),
            width: 2560,
            height: 1440,
            x: 0,
            y: 0,
            is_primary: true,
            scale_factor: 1.0,
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
            scale_factor: 1.0,
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
            scale_factor: 1.0,
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
                scale_factor: 1.0,
            },
            MonitorInfo {
                name: "Right".into(),
                width: 1920,
                height: 1080,
                x: 1920,
                y: 0,
                is_primary: false,
                scale_factor: 1.0,
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
                scale_factor: 1.0,
            },
            MonitorInfo {
                name: "Primary".into(),
                width: 2560,
                height: 1440,
                x: 0,
                y: 0,
                is_primary: true,
                scale_factor: 1.0,
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
                scale_factor: 1.0,
            },
            MonitorInfo {
                name: "Bottom".into(),
                width: 1920,
                height: 1080,
                x: 0,
                y: 0,
                is_primary: true,
                scale_factor: 1.0,
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

    #[test]
    fn client_alignment_uses_measured_geometry_for_negative_origins() {
        let corrected = corrected_outer_position(
            PhysicalPosition { x: -1912, y: 108 },
            PhysicalPosition { x: -1904, y: 116 },
            PhysicalPosition { x: -1920, y: 100 },
        );
        assert_eq!(corrected.x, -1928);
        assert_eq!(corrected.y, 92);
    }

    #[test]
    fn client_alignment_is_identity_when_outer_and_client_match() {
        let position = PhysicalPosition { x: 0, y: 0 };
        assert_eq!(
            corrected_outer_position(position, position, position),
            position
        );
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
