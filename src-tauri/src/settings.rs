//! Settings persistence module.
//!
//! Reads/writes user preferences to a JSON file in the app data directory.
//! Provides Tauri commands for getting/saving settings and hotkey introspection.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

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
    std::fs::write(&path, raw).map_err(|e| {
        let msg = format!("write settings file: {e}");
        tracing::error!("{}", msg);
        msg
    })?;
    tracing::info!("Settings saved to {}", path.display());

    // If hotkey combos changed, unregister the old and register the new.
    // On failure, re-register the old hotkey so the user isn't left without one.
    if old_settings.hotkey_full != settings.hotkey_full {
        let _ = hotkeys::unregister_hotkey_platform(&old_settings.hotkey_full);
        match hotkeys::register_hotkey_platform(&settings.hotkey_full, app.clone()) {
            Ok(()) => {
                if let Ok(mut guard) = state.lock() {
                    guard
                        .hotkeys_registered
                        .retain(|k| k != &old_settings.hotkey_full && k != &settings.hotkey_full);
                    guard.hotkeys_registered.push(settings.hotkey_full.clone());
                }
            }
            Err(e) => {
                tracing::error!(
                    "Failed to register hotkey '{}' after settings save: {}",
                    settings.hotkey_full,
                    e
                );
                // Re-register the old hotkey so the user still has a working combo
                let _ = hotkeys::register_hotkey_platform(&old_settings.hotkey_full, app.clone());
                return Err(format!(
                    "Failed to register hotkey '{}': {}",
                    settings.hotkey_full,
                    e
                ));
            }
        }
    }

    if old_settings.hotkey_area != settings.hotkey_area {
        let _ = hotkeys::unregister_hotkey_platform(&old_settings.hotkey_area);
        match hotkeys::register_hotkey_platform(&settings.hotkey_area, app.clone()) {
            Ok(()) => {
                if let Ok(mut guard) = state.lock() {
                    guard
                        .hotkeys_registered
                        .retain(|k| k != &old_settings.hotkey_area && k != &settings.hotkey_area);
                    guard.hotkeys_registered.push(settings.hotkey_area.clone());
                }
            }
            Err(e) => {
                tracing::error!(
                    "Failed to register hotkey '{}' after settings save: {}",
                    settings.hotkey_area,
                    e
                );
                // Re-register the old hotkey so the user still has a working combo
                let _ = hotkeys::register_hotkey_platform(&old_settings.hotkey_area, app.clone());
                return Err(format!(
                    "Failed to register hotkey '{}': {}",
                    settings.hotkey_area,
                    e
                ));
            }
        }
    }

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
}
