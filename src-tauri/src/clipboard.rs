/// Clipboard read/write module.
///
/// Stub — will read and write image data to the Windows clipboard
/// so captured screenshots are immediately available for paste.

use tauri::State;
use std::sync::Mutex;

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

/// Copy image bytes to the system clipboard.
#[tauri::command]
pub async fn copy_to_clipboard(
    state: State<'_, Mutex<ClipboardState>>,
    _image_data: Vec<u8>,
) -> Result<(), String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    // TODO: write image to Windows clipboard
    guard.last_clipboard_content = Some("image_stub".into());
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
