//! HTTP multipart upload client.
//!
//! Stub — will POST screenshots to sinister.ovh as multipart/form-data.
//! Uses reqwest for async HTTP.

use std::sync::Mutex;
use tauri::State;

#[allow(dead_code)]
pub struct UploadState {
    pub endpoint: String,
    pub last_upload_url: Option<String>,
}

impl UploadState {
    pub fn new() -> Self {
        Self {
            endpoint: "https://sinister.ovh".into(),
            last_upload_url: None,
        }
    }
}

/// Upload an image via multipart POST.
/// Returns the public URL of the uploaded screenshot.
#[tauri::command]
pub async fn upload_screenshot(
    state: State<'_, Mutex<UploadState>>,
    _image_data: Vec<u8>,
    _filename: String,
) -> Result<String, String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    // TODO: POST multipart to sinister.ovh
    guard.last_upload_url = Some("https://sinister.ovh/stub.png".into());
    Ok(guard.last_upload_url.clone().unwrap())
}
