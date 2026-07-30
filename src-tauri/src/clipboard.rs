//! Clipboard read/write module.
//!
//! Stub — will read and write image data to the Windows clipboard
//! so captured screenshots are immediately available for paste.

use std::sync::Mutex;
use tauri::State;

pub struct ClipboardState {
    pub last_clipboard_content: Option<String>,
}

impl ClipboardState {
    pub fn new() -> Self {
        Self {
            last_clipboard_content: None,
        }
    }
}

/// Copy a base64-encoded PNG image to the system clipboard.
#[tauri::command]
pub async fn copy_to_clipboard(
    state: State<'_, Mutex<ClipboardState>>,
    image_data_base64: String,
) -> Result<(), String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    let bytes = base64_decode(&image_data_base64)?;
    let len = bytes.len();
    // TODO: write image bytes to Windows clipboard
    guard.last_clipboard_content = Some(format!("image_{len}_bytes"));
    Ok(())
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| format!("Base64 decode failed: {e}"))
}

/// Copy text to the system clipboard.
#[tauri::command]
pub async fn copy_text_to_clipboard(
    state: State<'_, Mutex<ClipboardState>>,
    text: String,
) -> Result<(), String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    guard.last_clipboard_content = Some(text.clone());
    // TODO: write text to system clipboard
    Ok(())
}

/// Read image bytes from the system clipboard.
#[tauri::command]
pub async fn read_from_clipboard(
    state: State<'_, Mutex<ClipboardState>>,
) -> Result<Option<Vec<u8>>, String> {
    let _guard = state.lock().map_err(|e| e.to_string())?;
    // TODO: read image from Windows clipboard
    Ok(None)
}
