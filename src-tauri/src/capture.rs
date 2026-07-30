/// DXGI/DirectX screen capture module.
///
/// Stub — will implement full-monitor and area-select capture
/// using the Windows Desktop Duplication API (DXGI).
/// Tauri command entry points for capture workflows live here.

use tauri::State;
use std::sync::Mutex;

#[allow(dead_code)]
pub struct CaptureState {
    /// Placeholder for DXGI output duplication state.
    pub initialized: bool,
}

impl CaptureState {
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

/// Capture the entire primary monitor.
#[tauri::command]
pub async fn capture_full_screen(state: State<'_, Mutex<CaptureState>>) -> Result<String, String> {
    let _guard = state.lock().map_err(|e| e.to_string())?;
    // TODO: implement DXGI full-screen capture
    Ok("full_screen_capture_stub".into())
}

/// Capture a user-selected rectangular area.
#[tauri::command]
pub async fn capture_area(
    state: State<'_, Mutex<CaptureState>>,
    _x: i32,
    _y: i32,
    _width: i32,
    _height: i32,
) -> Result<String, String> {
    let _guard = state.lock().map_err(|e| e.to_string())?;
    // TODO: implement DXGI area capture
    Ok("area_capture_stub".into())
}
