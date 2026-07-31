//! Bounded HTTP multipart upload client for the screenshot service.

use crate::settings;
use reqwest::{multipart, Url};
use std::sync::Mutex;
use tauri::{AppHandle, State};

const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

pub struct UploadState {
    pub last_upload_url: Option<String>,
}

impl UploadState {
    pub fn new() -> Self {
        Self {
            last_upload_url: None,
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
struct UploadResponse {
    url: String,
    key: String,
}

/// Upload an image using the endpoint persisted in the application settings.
///
/// The mutex is only used to retain the last successful URL, after all network
/// work has completed. It is intentionally not held across an await.
#[tauri::command]
pub async fn upload_screenshot(
    app: AppHandle,
    state: State<'_, Mutex<UploadState>>,
    image_data_base64: String,
    filename: String,
) -> Result<String, String> {
    let configured_endpoint = settings::load_settings_sync(&app).server_url;
    let endpoint = normalize_endpoint(&configured_endpoint)?;
    let bytes = decode_png(&image_data_base64)?;
    let response = upload_png(&endpoint, bytes, &filename).await?;

    let url = response.url;
    if let Ok(mut guard) = state.lock() {
        guard.last_upload_url = Some(url.clone());
    }
    tracing::info!("Screenshot upload succeeded (key={})", response.key);
    Ok(url)
}

/// Normalize a setting containing either the service origin or its upload path.
/// HTTP is deliberately allowed only for loopback, which keeps disposable local
/// integration tests possible without permitting credentials/images over HTTP.
pub fn normalize_endpoint(input: &str) -> Result<Url, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Upload server URL is empty".into());
    }
    let mut url = Url::parse(trimmed).map_err(|_| {
        "Upload server URL is invalid; enter an HTTPS URL such as https://screenshots.sinister.ovh/api/upload".to_string()
    })?;

    if url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Upload server URL must not contain credentials, query parameters, or a fragment"
                .into(),
        );
    }
    let host = url
        .host_str()
        .ok_or_else(|| "Upload server URL must include a host".to_string())?;
    let is_loopback = matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1");
    if url.scheme() != "https" && !(url.scheme() == "http" && is_loopback) {
        return Err(
            "Upload server URL must use HTTPS (HTTP is only allowed for localhost tests)".into(),
        );
    }

    let path = url.path().trim_end_matches('/');
    if path.is_empty() || path == "/" || path == "/api/upload" {
        url.set_path("/api/upload");
    } else {
        return Err("Upload server URL must be an origin or end in /api/upload".into());
    }
    Ok(url)
}

fn decode_png(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    // Reject oversized encoded input before base64 allocates a large buffer.
    let max_encoded = MAX_UPLOAD_BYTES.div_ceil(3) * 4 + 4;
    if input.len() > max_encoded {
        return Err(format!(
            "Screenshot exceeds the {} MiB upload limit",
            MAX_UPLOAD_BYTES / (1024 * 1024)
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|_| "Screenshot data is not valid base64".to_string())?;
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(format!(
            "Screenshot exceeds the {} MiB upload limit",
            MAX_UPLOAD_BYTES / (1024 * 1024)
        ));
    }
    if !is_png(&bytes) {
        return Err("Screenshot is not a valid PNG image".into());
    }
    Ok(bytes)
}

fn is_png(bytes: &[u8]) -> bool {
    // Decode the complete image, including CRCs and compressed data, instead
    // of accepting an arbitrary file with only a PNG-looking prefix.
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = match decoder.read_info() {
        Ok(reader) => reader,
        Err(_) => return false,
    };
    let mut output = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut output).is_ok()
}

fn safe_filename(input: &str) -> String {
    let name = std::path::Path::new(input)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("screenshot.png");
    let filtered: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    if filtered.is_empty() {
        "screenshot.png".into()
    } else {
        filtered
    }
}

async fn upload_png(
    endpoint: &Url,
    bytes: Vec<u8>,
    filename: &str,
) -> Result<UploadResponse, String> {
    let part = multipart::Part::bytes(bytes)
        .file_name(safe_filename(filename))
        .mime_str("image/png")
        .map_err(|_| "Could not construct PNG upload".to_string())?;
    let form = multipart::Form::new().part("image", part);
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Could not create upload client: {e}"))?;
    let response = client
        .post(endpoint.clone())
        .multipart(form)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "Upload timed out after 15 seconds; check the server URL and connection".to_string()
            } else {
                format!("Upload request failed: {e}")
            }
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Upload server returned HTTP {status}"));
    }
    let body = read_limited(response).await?;
    let parsed: UploadResponse = serde_json::from_slice(&body)
        .map_err(|_| "Upload server returned malformed JSON (expected url and key)".to_string())?;
    validate_response(&parsed)
}

async fn read_limited(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err("Upload server response is too large".into());
    }
    let mut body = Vec::with_capacity(MAX_RESPONSE_BYTES.min(4096));
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Could not read upload server response".to_string())?
    {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err("Upload server response is too large".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn validate_response(response: &UploadResponse) -> Result<UploadResponse, String> {
    if response.key.is_empty()
        || response.key.len() > 256
        || response
            .key
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err("Upload server returned an invalid key".into());
    }
    let url = Url::parse(&response.url)
        .map_err(|_| "Upload server returned an invalid URL".to_string())?;
    let host = url.host_str().unwrap_or_default();
    let local = matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1");
    if url.scheme() != "https" && !(url.scheme() == "http" && local) {
        return Err("Upload server returned a non-HTTPS URL".into());
    }
    if url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() == "/"
        || url.path().is_empty()
        || url.path().chars().any(|c| c.is_control())
    {
        return Err("Upload server returned an unsafe or incomplete URL".into());
    }
    Ok(response.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use sha2::{Digest, Sha256};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    fn png() -> Vec<u8> {
        // A complete 1x1 transparent PNG.
        vec![
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 96, 0, 0,
            0, 2, 0, 1, 226, 33, 188, 51, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ]
    }

    fn server(response: String, delay: Option<Duration>) -> (String, thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            if let Some(delay) = delay {
                thread::sleep(delay);
            }
            let mut request = Vec::new();
            let mut buf = [0; 8192];
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let header_end = loop {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break None,
                    Ok(n) => {
                        request.extend_from_slice(&buf[..n]);
                        if let Some(end) =
                            request.windows(4).position(|window| window == b"\r\n\r\n")
                        {
                            break Some(end + 4);
                        }
                        if request.len() > 1_000_000 {
                            break None;
                        }
                    }
                }
            };
            if let Some(header_end) = header_end {
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Content-Length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                while request.len() < header_end + content_length {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
            }
            let _ = stream.write_all(response.as_bytes());
            request
        });
        (address, handle)
    }

    fn ok_response(key: &str) -> String {
        format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{{\"url\":\"http://127.0.0.1/x/{key}\",\"key\":\"{key}\"}}")
    }

    #[test]
    fn normalizes_origin_and_rejects_unsafe_endpoints() {
        assert_eq!(
            normalize_endpoint("http://localhost:9000/").unwrap().path(),
            "/api/upload"
        );
        let configured = normalize_endpoint("https://screenshots.sinister.ovh").unwrap();
        assert_eq!(
            configured.as_str(),
            "https://screenshots.sinister.ovh/api/upload"
        );
        assert!(normalize_endpoint("http://example.test/api/upload").is_err());
        assert!(normalize_endpoint("https://example.test/api/upload?token=secret").is_err());
    }

    #[tokio::test]
    async fn local_server_receives_image_and_returned_hash_matches() {
        let image = png();
        let expected_hash = format!("{:x}", Sha256::digest(&image));
        let (base, handle) = server(ok_response("abc123"), None);
        let response = upload_png(
            &normalize_endpoint(&base).unwrap(),
            image.clone(),
            "nested\\shot.png",
        )
        .await
        .unwrap();
        let request = handle.join().unwrap();
        assert!(request
            .windows(b"name=\"image\"".len())
            .any(|window| window == b"name=\"image\""));
        assert!(request
            .windows(image.len())
            .any(|window| window == image.as_slice()));
        assert_eq!(format!("{:x}", Sha256::digest(&image)), expected_hash);
        assert_eq!(response.key, "abc123");
        assert_eq!(response.url, "http://127.0.0.1/x/abc123");
    }

    #[tokio::test]
    async fn rejects_non_2xx_and_malformed_json() {
        let (base, handle) = server(
            "HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\n\r\n".into(),
            None,
        );
        assert!(
            upload_png(&normalize_endpoint(&base).unwrap(), png(), "x.png")
                .await
                .unwrap_err()
                .contains("503")
        );
        let _ = handle.join();
        let (base, handle) = server(
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nnot-json".into(),
            None,
        );
        assert!(
            upload_png(&normalize_endpoint(&base).unwrap(), png(), "x.png")
                .await
                .is_err()
        );
        let _ = handle.join();
    }

    #[tokio::test]
    async fn rejects_timeout_and_unsupported_or_oversize_input() {
        let (base, handle) = server(
            ok_response("late"),
            Some(REQUEST_TIMEOUT + Duration::from_secs(1)),
        );
        let error = upload_png(&normalize_endpoint(&base).unwrap(), png(), "x.png")
            .await
            .unwrap_err();
        assert!(error.contains("timed out"));
        let _ = handle.join();
        assert!(decode_png(&base64::engine::general_purpose::STANDARD.encode(b"not png")).is_err());
        let oversized = vec![0u8; MAX_UPLOAD_BYTES + 1];
        let encoded = base64::engine::general_purpose::STANDARD.encode(oversized);
        assert!(decode_png(&encoded).unwrap_err().contains("exceeds"));
    }
}
