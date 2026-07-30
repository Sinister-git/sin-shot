use axum::{
    extract::{ConnectInfo, Multipart, Path, State},
    http::{header, Method, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, get_service, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{fs, sync::Mutex};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    limit::RequestBodyLimitLayer,
    services::{ServeDir, ServeFile},
};

// ── Config ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct Config {
    port: u16,
    storage_dir: PathBuf,
    max_file_size: usize, // bytes
    base_url: String,
    google_client_id: String,
    static_dir: PathBuf,
}

fn load_config() -> Config {
    Config {
        port: std::env::var("PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8080),
        storage_dir: std::env::var("STORAGE_DIR")
            .unwrap_or_else(|_| "./uploads".into())
            .into(),
        max_file_size: std::env::var("MAX_FILE_SIZE_MB")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .map(|n| n * 1024 * 1024)
            .unwrap_or(25 * 1024 * 1024),
        base_url: std::env::var("BASE_URL")
            .unwrap_or_else(|_| "https://sinister.ovh".into())
            .trim_end_matches('/')
            .to_string(),
        google_client_id: std::env::var("GOOGLE_CLIENT_ID").unwrap_or_default(),
        static_dir: std::env::var("STATIC_DIR")
            .unwrap_or_else(|_| "./gallery-web/build".into())
            .into(),
    }
}

// ── Rate Limiter ────────────────────────────────────────────────────

struct RateLimiter {
    buckets: HashMap<String, (f64, Instant)>,
    max_requests: f64,
    window: Duration,
    rate: f64,
}

impl RateLimiter {
    fn new(max_requests: u32, window_secs: u64) -> Self {
        let max = max_requests as f64;
        let window = Duration::from_secs(window_secs);
        Self {
            buckets: HashMap::new(),
            max_requests: max,
            window,
            rate: max / window_secs as f64,
        }
    }

    fn allow(&mut self, ip: &str) -> bool {
        let now = Instant::now();
        let entry = self
            .buckets
            .entry(ip.to_string())
            .or_insert_with(|| (self.max_requests, now));

        let (tokens, last) = entry;
        let elapsed = now.duration_since(*last).as_secs_f64();
        *tokens = (*tokens + elapsed * self.rate).min(self.max_requests);
        *last = now;

        if *tokens >= 1.0 {
            *tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn cleanup(&mut self) {
        let now = Instant::now();
        self.buckets
            .retain(|_, (_, last)| now.duration_since(*last) < self.window * 5);
    }
}

// ── App State ───────────────────────────────────────────────────────

struct AppState {
    config: Config,
    limiter: Mutex<RateLimiter>,
}

// ── Magic Bytes MIME Detection ──────────────────────────────────────

const ALLOWED_EXTENSIONS: &[(&str, &str)] = &[
    ("image/png", ".png"),
    ("image/jpeg", ".jpg"),
    ("image/webp", ".webp"),
];

fn detect_mime(data: &[u8]) -> Option<(&'static str, &'static str)> {
    if data.len() >= 8 && data[0] == 0x89 && data[1] == b'P' && data[2] == b'N' && data[3] == b'G' {
        return Some(("image/png", ".png"));
    }
    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
        return Some(("image/jpeg", ".jpg"));
    }
    if data.len() >= 12
        && data[0] == b'R'
        && data[1] == b'I'
        && data[2] == b'F'
        && data[3] == b'F'
        && data[8] == b'W'
        && data[9] == b'E'
        && data[10] == b'B'
        && data[11] == b'P'
    {
        return Some(("image/webp", ".webp"));
    }
    None
}

fn generate_key(alphabet: &[char], length: usize) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..alphabet.len());
            alphabet[idx]
        })
        .collect()
}

// ── JSON Responses ──────────────────────────────────────────────────

#[derive(Serialize)]
struct UploadResponse {
    url: String,
    key: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Serialize)]
struct GalleryItem {
    key: String,
    url: String,
    filename: String,
    mime_type: String,
    size_bytes: u64,
    uploaded_at: String,
}

#[derive(Serialize)]
struct GalleryResponse {
    images: Vec<GalleryItem>,
}

#[derive(Deserialize)]
struct GoogleTokenInfo {
    #[serde(default)]
    #[allow(dead_code)]
    email: String,
    #[serde(default)]
    aud: String,
    #[serde(default)]
    #[allow(dead_code)]
    sub: String,
    #[serde(default)]
    error: String,
    #[serde(default, rename = "error_description")]
    #[allow(dead_code)]
    error_description: String,
}

fn error_response(status: StatusCode, msg: impl Into<String>) -> Response {
    let body = Json(ErrorResponse { error: msg.into() });
    (status, body).into_response()
}

// ── Auth: verify Google ID token ────────────────────────────────────

async fn verify_google_token(token: &str, client_id: &str) -> Result<GoogleTokenInfo, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "https://oauth2.googleapis.com/tokeninfo?id_token={}",
            token
        ))
        .send()
        .await
        .map_err(|e| format!("failed to verify token: {}", e))?;

    let info: GoogleTokenInfo = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse token info: {}", e))?;

    if !info.error.is_empty() {
        return Err(format!("invalid token: {}", info.error));
    }

    // Verify audience matches our client ID (if configured)
    if !client_id.is_empty() && info.aud != client_id {
        return Err("token audience mismatch".into());
    }

    Ok(info)
}

/// Extract and verify Bearer token from Authorization header.
async fn require_auth(
    headers: &axum::http::HeaderMap,
    client_id: &str,
) -> Result<GoogleTokenInfo, Response> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let token = auth_header.strip_prefix("Bearer ").unwrap_or("");

    if token.is_empty() {
        return Err(error_response(
            StatusCode::UNAUTHORIZED,
            "missing authorization token",
        ));
    }

    verify_google_token(token, client_id)
        .await
        .map_err(|e| error_response(StatusCode::UNAUTHORIZED, e))
}

// ── Handlers ────────────────────────────────────────────────────────

async fn handle_upload(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut multipart: Multipart,
) -> Response {
    // Rate limit check
    let ip = addr.ip().to_string();
    {
        let mut limiter = state.limiter.lock().await;
        if !limiter.allow(&ip) {
            return error_response(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
        }
    }

    // Parse the multipart form
    let mut image_data: Option<(Vec<u8>, String)> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "image" {
            match field.bytes().await {
                Ok(bytes) => {
                    if bytes.len() > state.config.max_file_size {
                        return error_response(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            "file exceeds maximum size",
                        );
                    }
                    image_data = Some((bytes.to_vec(), name));
                }
                Err(_) => {
                    return error_response(StatusCode::BAD_REQUEST, "failed to read file");
                }
            }
            break;
        }
    }

    let (data, _name) = match image_data {
        Some(d) => d,
        None => {
            return error_response(StatusCode::BAD_REQUEST, "missing 'image' field in form");
        }
    };

    if data.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "empty file");
    }

    // Detect MIME type from magic bytes
    let (mime_type, ext) = match detect_mime(&data) {
        Some(m) => m,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "unsupported file type (accepted: PNG, JPEG, WebP)",
            );
        }
    };

    // Generate unique key
    let alphabet: [char; 64] = [
        'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r',
        's', 't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J',
        'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '0', '1',
        '2', '3', '4', '5', '6', '7', '8', '9', '_', '-',
    ];
    let key = generate_key(&alphabet, 16);

    let filename = format!("{}{}", key, ext);
    let file_path = state.config.storage_dir.join(&filename);

    // Write to disk
    if let Err(e) = fs::write(&file_path, &data).await {
        tracing::error!("Failed to write file {}: {}", file_path.display(), e);
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, "failed to store file");
    }

    tracing::info!(
        "Uploaded: {} ({} bytes, {})",
        filename,
        data.len(),
        mime_type
    );

    let url = format!("{}/{}", state.config.base_url, key);
    Json(UploadResponse { url, key }).into_response()
}

async fn handle_serve(State(state): State<Arc<AppState>>, Path(key): Path<String>) -> Response {
    // Security: reject paths with slashes or empty keys
    if key.is_empty() || key.contains('/') || key.contains('\\') || key == "api" {
        return error_response(StatusCode::NOT_FOUND, "not found");
    }

    // Try each allowed extension
    for (_mime, ext) in ALLOWED_EXTENSIONS {
        let file_path = state.config.storage_dir.join(format!("{}{}", key, ext));
        match fs::read(&file_path).await {
            Ok(data) => {
                let ct = *_mime;
                return (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, ct),
                        (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
                    ],
                    data,
                )
                    .into_response();
            }
            Err(_) => continue,
        }
    }

    error_response(StatusCode::NOT_FOUND, "not found")
}

// ── Gallery API ─────────────────────────────────────────────────────

async fn handle_gallery(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
) -> Response {
    // Rate limit check
    {
        let mut limiter = state.limiter.lock().await;
        if !limiter.allow(&addr.ip().to_string()) {
            return error_response(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
        }
    }

    // Require authentication
    match require_auth(&headers, &state.config.google_client_id).await {
        Ok(_) => {} // authenticated
        Err(resp) => return resp,
    }

    let mut images: Vec<GalleryItem> = Vec::new();

    let mut entries = match fs::read_dir(&state.config.storage_dir).await {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Failed to read storage dir: {}", e);
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "failed to read gallery");
        }
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();

        // Only process files, skip directories
        if !path.is_file() {
            continue;
        }

        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(f) => f.to_string(),
            None => continue,
        };

        // Extract key from filename: "abc123.png" -> key="abc123", ext=".png"
        let (key, ext) = match filename.rfind('.') {
            Some(dot) => (filename[..dot].to_string(), filename[dot..].to_string()),
            None => continue,
        };

        // Only include allowed image types
        let mime_type = match ext.as_str() {
            ".png" => "image/png",
            ".jpg" | ".jpeg" => "image/jpeg",
            ".webp" => "image/webp",
            _ => continue,
        };

        // Get file metadata
        let metadata = match fs::metadata(&path).await {
            Ok(m) => m,
            Err(_) => continue,
        };

        let size_bytes = metadata.len();
        let uploaded_at: String = metadata
            .modified()
            .ok()
            .and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH).ok().and_then(|d| {
                    DateTime::<Utc>::from_timestamp(d.as_secs() as i64, d.subsec_nanos())
                        .map(|dt| dt.to_rfc3339())
                })
            })
            .unwrap_or_else(|| "unknown".to_string());

        images.push(GalleryItem {
            key: key.clone(),
            url: format!("{}/{}", state.config.base_url, key),
            filename,
            mime_type: mime_type.to_string(),
            size_bytes,
            uploaded_at,
        });
    }

    // Sort by upload time descending (newest first)
    images.sort_by(|a, b| b.uploaded_at.cmp(&a.uploaded_at));

    tracing::info!("Gallery: returning {} images", images.len());

    Json(GalleryResponse { images }).into_response()
}

async fn handle_delete(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Path(key): Path<String>,
) -> Response {
    // Rate limit check
    {
        let mut limiter = state.limiter.lock().await;
        if !limiter.allow(&addr.ip().to_string()) {
            return error_response(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
        }
    }

    // Require authentication
    match require_auth(&headers, &state.config.google_client_id).await {
        Ok(_) => {}
        Err(resp) => return resp,
    }

    // Security: reject paths with slashes
    if key.is_empty() || key.contains('/') || key.contains('\\') {
        return error_response(StatusCode::NOT_FOUND, "not found");
    }

    // Try to delete the file with any known extension
    let mut deleted = false;
    for (_mime, ext) in ALLOWED_EXTENSIONS {
        let file_path = state.config.storage_dir.join(format!("{}{}", key, ext));
        match fs::remove_file(&file_path).await {
            Ok(()) => {
                tracing::info!("Deleted: {}{}", key, ext);
                deleted = true;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                tracing::error!("Failed to delete {}{}: {}", key, ext, e);
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, "failed to delete image");
            }
        }
    }

    if deleted {
        Json(serde_json::json!({ "deleted": true })).into_response()
    } else {
        error_response(StatusCode::NOT_FOUND, "image not found")
    }
}

// ── Main ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sin_shot_server=info".into()),
        )
        .init();

    let config = load_config();

    // Create storage directory
    if let Err(e) = fs::create_dir_all(&config.storage_dir).await {
        tracing::error!(
            "Failed to create storage directory {}: {}",
            config.storage_dir.display(),
            e
        );
        std::process::exit(1);
    }

    let state = Arc::new(AppState {
        config: config.clone(),
        limiter: Mutex::new(RateLimiter::new(10, 60)),
    });

    // Spawn a cleanup task for the rate limiter
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(300)).await;
                let mut limiter = state.limiter.lock().await;
                limiter.cleanup();
            }
        });
    }

    // CORS — allow the gallery frontend origin(s)
    // CORS_ORIGINS can be a comma-separated list of allowed origins.
    // Falls back to the base_url origin, then to allowing any origin.
    let cors_origins: Vec<String> = std::env::var("CORS_ORIGINS")
        .ok()
        .map(|s| s.split(',').map(|o| o.trim().to_string()).collect())
        .unwrap_or_default();

    let allowed_origin: AllowOrigin = if !cors_origins.is_empty() {
        let origins: Vec<axum::http::HeaderValue> = cors_origins
            .iter()
            .filter_map(|o| o.parse::<axum::http::HeaderValue>().ok())
            .collect();
        if origins.is_empty() {
            AllowOrigin::any()
        } else {
            AllowOrigin::list(origins)
        }
    } else if let Ok(origin) = config.base_url.parse::<axum::http::HeaderValue>() {
        AllowOrigin::exact(origin)
    } else {
        AllowOrigin::any()
    };

    let cors = CorsLayer::new()
        .allow_origin(allowed_origin)
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT]);

    let favicon_path = config.static_dir.join("favicon.png");
    let index_path = config.static_dir.join("index.html");
    let static_dir = config.static_dir.clone();

    let app = Router::new()
        .route("/api/upload", post(handle_upload))
        .route("/api/gallery", get(handle_gallery))
        .route("/api/image/{key}", delete(handle_delete))
        // Static assets — must be before /{key} to avoid conflicting with image keys
        .route("/favicon.png", get_service(ServeFile::new(favicon_path)))
        // Image serving routes
        .route("/x/{key}", get(handle_serve))
        .route("/{key}", get(handle_serve))
        // Fallback: static files from gallery-web build, then index.html for SPA routing
        .fallback_service(ServeDir::new(static_dir).fallback(ServeFile::new(index_path)))
        .layer(cors)
        .layer(RequestBodyLimitLayer::new(
            config.max_file_size + 1024 * 1024,
        ))
        .with_state(state.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("Sin Shot server starting on http://{}", addr);
    tracing::info!(
        "Storage: {}, Max file size: {} MB",
        config.storage_dir.display(),
        config.max_file_size / (1024 * 1024)
    );
    tracing::info!("Static files: {}", config.static_dir.display());
    if !config.google_client_id.is_empty() {
        tracing::info!("Google auth enabled (client ID configured)");
    } else {
        tracing::warn!("Google auth disabled — set GOOGLE_CLIENT_ID to enable gallery auth");
    }

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
