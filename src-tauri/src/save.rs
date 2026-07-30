//! Local file-save module.
//!
//! Saves annotated screenshots to the user's Pictures directory.

use std::path::PathBuf;
use tauri::Emitter;

/// Managed state for file-save operations.
#[allow(dead_code)]
pub struct SaveState {
    pub last_save_dir: Option<PathBuf>,
}

impl SaveState {
    pub fn new() -> Self {
        Self {
            last_save_dir: None,
        }
    }
}

/// Save a base64-encoded PNG image to the user's Pictures directory.
///
/// Returns the full path to the saved file.
#[tauri::command]
pub async fn save_to_file(
    app: tauri::AppHandle,
    image_data_base64: String,
) -> Result<String, String> {
    let bytes = base64_decode(&image_data_base64)?;

    let pictures = pictures_dir().ok_or("Could not find Pictures directory")?;
    let save_dir = pictures.join("Sin Shot");
    std::fs::create_dir_all(&save_dir)
        .map_err(|e| format!("Failed to create save directory: {e}"))?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let filename = format!("sin-shot-{}.png", timestamp);
    let path = save_dir.join(&filename);

    std::fs::write(&path, &bytes).map_err(|e| format!("Failed to write file: {e}"))?;

    let path_str = path.to_string_lossy().to_string();
    let _ = app.emit("screenshot-saved", &path_str);

    Ok(path_str)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| format!("Base64 decode failed: {e}"))
}

fn pictures_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Pictures"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Pictures"))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decode_valid() {
        use base64::Engine as _;
        let data = b"hello world";
        let encoded = base64::engine::general_purpose::STANDARD.encode(data);
        let decoded = base64_decode(&encoded).expect("decode");
        assert_eq!(decoded, data);
    }

    #[test]
    fn base64_decode_invalid() {
        assert!(base64_decode("!!!not-base64!!!").is_err());
    }
}
