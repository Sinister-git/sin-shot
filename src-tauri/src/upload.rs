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
    image_data_base64: String,
    _filename: String,
) -> Result<String, String> {
    let mut guard = state.lock().map_err(|e| {
        let msg = format!("Failed to lock UploadState: {}", e);
        tracing::error!("{}", msg);
        msg
    })?;
    let _bytes = base64_decode(&image_data_base64).map_err(|e| {
        tracing::error!("upload_screenshot: base64 decode failed: {}", e);
        e
    })?;
    // TODO: POST multipart to sinister.ovh
    guard.last_upload_url = Some("https://sinister.ovh/stub.png".into());
    let url = guard.last_upload_url.clone().unwrap();
    tracing::info!("Uploaded screenshot to {}", url);
    Ok(url)
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| format!("Base64 decode failed: {e}"))
}
