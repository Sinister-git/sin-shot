use axum::{
    extract::{ConnectInfo, Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{fs, sync::Mutex};
use tower_http::limit::RequestBodyLimitLayer;

// ── Config ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct Config {
    port: u16,
    storage_dir: PathBuf,
    max_file_size: usize, // bytes
    base_url: String,
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

    /// Periodically clean up old entries
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

fn error_response(status: StatusCode, msg: impl Into<String>) -> Response {
    let body = Json(ErrorResponse { error: msg.into() });
    (status, body).into_response()
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
    let mut image_data: Option<(Vec<u8>, String)> = None; // (bytes, field_name)

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
        limiter: Mutex::new(RateLimiter::new(10, 60)), // 10 requests per 60s per IP
    });

    // Spawn a cleanup task for the rate limiter
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(300)).await; // Every 5 minutes
                let mut limiter = state.limiter.lock().await;
                limiter.cleanup();
            }
        });
    }

    let app = Router::new()
        .route("/api/upload", post(handle_upload))
        .route("/{key}", get(handle_serve))
        .layer(RequestBodyLimitLayer::new(
            config.max_file_size + 1024 * 1024,
        )) // +1MB for form overhead
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("Sin Shot upload server starting on http://{}", addr);
    tracing::info!(
        "Storage: {}, Max file size: {} MB",
        config.storage_dir.display(),
        config.max_file_size / (1024 * 1024)
    );

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
