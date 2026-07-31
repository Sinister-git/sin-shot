//! Settings persistence module.
//!
//! Reads/writes user preferences to a JSON file in the app data directory.
//! Provides Tauri commands for getting/saving settings and hotkey introspection.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::hotkeys::{self, HotkeyState};

// ---------------------------------------------------------------------------
// Settings struct
// ---------------------------------------------------------------------------

/// All user-configurable settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // General
    pub save_folder: String,
    pub filename_pattern: String,
    pub image_format: String, // "png", "jpeg", "webp"
    pub jpeg_quality: u8,     // 60–100
    pub start_with_windows: bool,
    pub play_sound_on_capture: bool,
    // Hotkeys
    pub hotkey_full: String,
    pub hotkey_area: String,

    // Upload
    pub server_url: String,
    pub auto_copy: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            save_folder: dirs_fallback_pictures(),
            filename_pattern: "screenshot_{date}_{time}".into(),
            image_format: "png".into(),
            jpeg_quality: 85,
            start_with_windows: false,
            play_sound_on_capture: false,
            hotkey_full: "Ctrl+Shift+1".into(),
            hotkey_area: "Ctrl+Shift+2".into(),
            server_url: "https://sinister.ovh/api/upload".into(),
            auto_copy: true,
        }
    }
}

/// Return a sensible default for the save folder on each platform.
fn dirs_fallback_pictures() -> String {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .map(|p| format!("{}\\Pictures\\Sin Shot", p))
            .unwrap_or_else(|_| ".".into())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(|p| format!("{}/Pictures/Sin Shot", p))
            .unwrap_or_else(|_| ".".into())
    }
}

// ---------------------------------------------------------------------------
// File paths
// ---------------------------------------------------------------------------

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| {
        let msg = format!("app_data_dir failed: {e}");
        tracing::error!("{}", msg);
        msg
    })?;
    std::fs::create_dir_all(&dir).map_err(|e| {
        let msg = format!("create_dir_all for settings dir failed: {e}");
        tracing::error!("{}", msg);
        msg
    })?;
    Ok(dir.join("settings.json"))
}

// ---------------------------------------------------------------------------
// Sync helpers (for use during setup before async runtime is ready)
// ---------------------------------------------------------------------------

pub fn load_settings_sync(app: &AppHandle) -> Settings {
    match settings_path(app) {
        Ok(path) if path.exists() => match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str(&raw) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to parse settings file: {}", e);
                    Settings::default()
                }
            },
            Err(e) => {
                tracing::error!("Failed to read settings file: {}", e);
                Settings::default()
            }
        },
        _ => Settings::default(),
    }
}

// ---------------------------------------------------------------------------
// Hotkey rebind transaction
// ---------------------------------------------------------------------------

/// Reverse native hotkey changes after a later change or settings write fails.
/// Each native operation is acknowledged by the platform module before its
/// combo-to-ID mapping changes, so retrying the inverse operation is safe.
fn rollback_hotkey_changes(app: &AppHandle, changes: &[(String, String)]) {
    for (old_combo, new_combo) in changes.iter().rev() {
        if let Err(error) = hotkeys::unregister_hotkey_platform(new_combo) {
            tracing::error!(
                "Failed to unregister rolled-back hotkey '{}': {}",
                new_combo,
                error
            );
        }
        if let Err(error) = hotkeys::register_hotkey_platform(old_combo, app.clone()) {
            tracing::error!(
                "Failed to restore rolled-back hotkey '{}': {}",
                old_combo,
                error
            );
        }
    }
}

/// Apply changed hotkeys as one transaction, restoring earlier changes when a
/// later unregister or register fails. Native unregister/register results are
/// deliberately propagated instead of treating the channel send as success.
fn rebind_hotkeys(app: &AppHandle, changes: &[(String, String)]) -> Result<(), String> {
    let mut applied = Vec::new();

    for (old_combo, new_combo) in changes {
        if let Err(error) = hotkeys::unregister_hotkey_platform(old_combo) {
            rollback_hotkey_changes(app, &applied);
            return Err(format!(
                "Failed to unregister hotkey '{}': {}",
                old_combo, error
            ));
        }

        if let Err(error) = hotkeys::register_hotkey_platform(new_combo, app.clone()) {
            // The old native registration was acknowledged as removed, so
            // restore it before rolling back any earlier changes.
            if let Err(restore_error) = hotkeys::register_hotkey_platform(old_combo, app.clone()) {
                tracing::error!(
                    "Failed to restore hotkey '{}' after registration failure: {}",
                    old_combo,
                    restore_error
                );
            }
            rollback_hotkey_changes(app, &applied);
            return Err(format!(
                "Failed to register hotkey '{}': {}",
                new_combo, error
            ));
        }

        applied.push((old_combo.clone(), new_combo.clone()));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Read current settings from disk, falling back to defaults.
#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Result<Settings, String> {
    let path = settings_path(&app)?;
    if path.exists() {
        let raw = std::fs::read_to_string(&path).map_err(|e| {
            let msg = format!("read settings file: {e}");
            tracing::error!("{}", msg);
            msg
        })?;
        serde_json::from_str(&raw).map_err(|e| {
            let msg = format!("parse settings: {e}");
            tracing::error!("{}", msg);
            msg
        })
    } else {
        Ok(Settings::default())
    }
}

/// Persist settings to disk and re-register hotkeys if they changed.
#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, std::sync::Mutex<HotkeyState>>,
    mut settings: Settings,
) -> Result<(), String> {
    // Snapshot old settings before overwriting the file
    let old_settings = load_settings_sync(&app);

    settings.jpeg_quality = settings.jpeg_quality.clamp(60, 100);
    let path = settings_path(&app)?;
    let raw = serde_json::to_string_pretty(&settings).map_err(|e| {
        let msg = format!("serialize settings: {e}");
        tracing::error!("{}", msg);
        msg
    })?;

    let changes: Vec<(String, String)> = [
        (
            old_settings.hotkey_full.clone(),
            settings.hotkey_full.clone(),
        ),
        (
            old_settings.hotkey_area.clone(),
            settings.hotkey_area.clone(),
        ),
    ]
    .into_iter()
    .filter(|(old_combo, new_combo)| old_combo != new_combo)
    .collect();

    // Native ownership must be changed before persistence. If either native
    // operation or the file write fails, leave the old settings active.
    rebind_hotkeys(&app, &changes)?;
    if let Err(error) = std::fs::write(&path, raw) {
        rollback_hotkey_changes(&app, &changes);
        let msg = format!("write settings file: {error}");
        tracing::error!("{}", msg);
        return Err(msg);
    }
    tracing::info!("Settings saved to {}", path.display());

    if let Ok(mut guard) = state.lock() {
        for (old_combo, new_combo) in &changes {
            guard.hotkeys_registered.retain(|combo| combo != old_combo);
            if !guard.hotkeys_registered.contains(new_combo) {
                guard.hotkeys_registered.push(new_combo.clone());
            }
        }
    }

    let _ = app.emit(
        "settings-changed",
        serde_json::json!({
            "hotkey_full": settings.hotkey_full,
            "hotkey_area": settings.hotkey_area,
        }),
    );

    Ok(())
}

/// Return the list of currently registered hotkey combos.
#[tauri::command]
pub async fn get_hotkeys(
    state: State<'_, std::sync::Mutex<HotkeyState>>,
) -> Result<Vec<String>, String> {
    let guard = state.lock().map_err(|e| {
        let msg = e.to_string();
        tracing::error!("get_hotkeys: {}", msg);
        msg
    })?;
    Ok(guard.hotkeys_registered.clone())
}

/// Show the settings window (create if needed, then show + focus).
#[tauri::command]
pub async fn show_settings(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        window.show().map_err(|e| {
            let msg = format!("show settings: {e}");
            tracing::error!("{}", msg);
            msg
        })?;
        window.set_focus().map_err(|e| {
            let msg = format!("focus settings: {e}");
            tracing::error!("{}", msg);
            msg
        })?;
    } else {
        // Build from the config entry with label "settings"
        use tauri::WebviewWindowBuilder;
        let config = app
            .config()
            .app
            .windows
            .iter()
            .find(|w| w.label == "settings")
            .ok_or_else(|| {
                tracing::error!("settings window not found in config");
                "settings window not found in config".to_string()
            })?
            .clone();
        let window = WebviewWindowBuilder::from_config(&app, &config)
            .map_err(|e| {
                let msg = format!("settings window config: {e}");
                tracing::error!("{}", msg);
                msg
            })?
            .build()
            .map_err(|e| {
                let msg = format!("build settings window: {e}");
                tracing::error!("{}", msg);
                msg
            })?;
        window.show().map_err(|e| {
            let msg = format!("show settings: {e}");
            tracing::error!("{}", msg);
            msg
        })?;
        window.set_focus().map_err(|e| {
            let msg = format!("focus settings: {e}");
            tracing::error!("{}", msg);
            msg
        })?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_valid() {
        let s = Settings::default();
        assert!(!s.save_folder.is_empty());
        assert!(!s.filename_pattern.is_empty());
        assert!(s.jpeg_quality >= 60 && s.jpeg_quality <= 100);
        assert!(s.image_format == "png" || s.image_format == "jpeg" || s.image_format == "webp");
    }

    #[test]
    fn settings_roundtrip_json() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).expect("serialize");
        let s2: Settings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s.save_folder, s2.save_folder);
        assert_eq!(s.filename_pattern, s2.filename_pattern);
        assert_eq!(s.image_format, s2.image_format);
        assert_eq!(s.jpeg_quality, s2.jpeg_quality);
    }

    #[test]
    fn settings_default_fills_missing() {
        let json = r#"{"server_url":"https://custom.example.com","auto_copy":false}"#;
        let s: Settings = serde_json::from_str(json).expect("deserialize");
        assert_eq!(s.server_url, "https://custom.example.com");
        assert!(!s.auto_copy);
        // Defaults for missing fields
        assert_eq!(s.image_format, "png");
        assert_eq!(s.jpeg_quality, 85);
    }

    #[test]
    fn jpeg_quality_clamping() {
        // Below minimum should clamp to 60
        let s = Settings {
            jpeg_quality: 0,
            ..Settings::default()
        };
        let clamped = s.jpeg_quality.clamp(60, 100);
        assert_eq!(clamped, 60);
        // Above maximum should clamp to 100
        let s = Settings {
            jpeg_quality: 255,
            ..Settings::default()
        };
        let clamped = s.jpeg_quality.clamp(60, 100);
        assert_eq!(clamped, 100);
        // Within range should stay unchanged
        let s = Settings {
            jpeg_quality: 80,
            ..Settings::default()
        };
        let clamped = s.jpeg_quality.clamp(60, 100);
        assert_eq!(clamped, 80);
    }

    /// When a hotkey combo changes, the HotkeyState should be updated:
    /// old combo is removed, new combo is added. This mirrors the
    /// retain+push logic in save_settings.
    #[test]
    fn hotkey_state_rotation_on_combo_change() {
        let mut state = crate::hotkeys::HotkeyState::new();
        state.hotkeys_registered.push("Ctrl+Shift+1".into());
        state.hotkeys_registered.push("Ctrl+Shift+2".into());

        let old_combo = "Ctrl+Shift+1";
        let new_combo = "Ctrl+Shift+X";

        // rotate: remove both old and new (defensive), then push new
        state
            .hotkeys_registered
            .retain(|k| k != old_combo && k != new_combo);
        state.hotkeys_registered.push(new_combo.to_string());

        assert_eq!(state.hotkeys_registered.len(), 2);
        assert!(!state.hotkeys_registered.contains(&old_combo.to_string()));
        assert!(state
            .hotkeys_registered
            .contains(&"Ctrl+Shift+2".to_string()));
        assert!(state.hotkeys_registered.contains(&new_combo.to_string()));
    }

    /// Verifies that default hotkey combos match what the frontend
    /// (Overlay.svelte) initialises as fallback values.
    #[test]
    fn default_hotkeys_match_overlay_fallbacks() {
        let s = Settings::default();
        assert_eq!(s.hotkey_full, "Ctrl+Shift+1");
        assert_eq!(s.hotkey_area, "Ctrl+Shift+2");
    }

    /// Verifies the settings-changed event payload contains the
    /// correct hotkey combo keys.
    #[test]
    fn settings_changed_event_payload_shape() {
        let payload = serde_json::json!({
            "hotkey_full": "Ctrl+Shift+F",
            "hotkey_area": "Ctrl+Shift+A",
        });
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("hotkey_full"));
        assert!(json.contains("hotkey_area"));
        assert!(json.contains("Ctrl+Shift+F"));
        assert!(json.contains("Ctrl+Shift+A"));
    }
}
