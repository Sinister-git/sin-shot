/// System-wide global hotkey registration.
///
/// Stub — will register global hotkeys on Windows
/// (e.g. PrintScreen for full capture, Ctrl+Shift+S for area select).
/// Tauri command entry points for hotkey management live here.

use tauri::State;
use std::sync::Mutex;

pub struct HotkeyState {
    pub hotkeys_registered: Vec<String>,
}

impl HotkeyState {
    pub fn new() -> Self {
        Self {
            hotkeys_registered: Vec::new(),
        }
    }
}

/// Register a global hotkey.
#[tauri::command]
pub async fn register_hotkey(
    state: State<'_, Mutex<HotkeyState>>,
    key_combo: String,
) -> Result<(), String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    // TODO: register system-wide hotkey on Windows
    guard.hotkeys_registered.push(key_combo);
    Ok(())
}

/// Unregister a global hotkey.
#[tauri::command]
pub async fn unregister_hotkey(
    state: State<'_, Mutex<HotkeyState>>,
    key_combo: String,
) -> Result<(), String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    guard.hotkeys_registered.retain(|k| k != &key_combo);
    Ok(())
}
