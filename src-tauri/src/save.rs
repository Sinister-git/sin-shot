//! Validated local PNG export.
//!
//! The frontend owns annotation composition and passes a PNG to this module.
//! This module validates that contract before writing and uses the persisted
//! save folder rather than silently selecting a different directory.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use tauri::Emitter;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const MAX_IMAGE_PIXELS: u64 = 100_000_000;

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

/// Save a validated base64-encoded PNG to the persisted save folder.
///
/// The command deliberately accepts PNG only: annotation composition produces
/// PNG bytes, so pretending that the image-format setting changes this
/// command would silently produce the wrong format. The settings UI should
/// therefore leave PNG selected until other encoders are implemented.
#[tauri::command]
pub async fn save_to_file(
    app: tauri::AppHandle,
    image_data_base64: String,
) -> Result<String, String> {
    let bytes = base64_decode(&image_data_base64)?;
    let image = validate_png(&bytes)?;

    let settings = crate::settings::load_settings_sync(&app);
    if !settings.image_format.eq_ignore_ascii_case("png") {
        return Err("Save currently supports PNG only; select PNG in Settings".to_string());
    }
    let save_dir = configured_save_dir(&settings.save_folder)?;
    std::fs::create_dir_all(&save_dir).map_err(|e| {
        let msg = format!("Failed to create save directory: {e}");
        tracing::error!("{}", msg);
        msg
    })?;

    let filename = filename_from_pattern(&settings.filename_pattern);
    let path = unique_path(&save_dir, &filename);
    std::fs::write(&path, &bytes).map_err(|e| {
        let msg = format!("Failed to write PNG to configured save folder: {e}");
        tracing::error!("{}", msg);
        msg
    })?;

    // Re-read the file and validate it as well. This catches partial writes or
    // filesystem/filter-driver transformations before reporting success.
    let written = std::fs::read(&path).map_err(|e| {
        let msg = format!("Saved PNG could not be read back: {e}");
        tracing::error!("{}", msg);
        msg
    })?;
    let written_image = validate_png(&written)?;
    if written != bytes || written_image != image {
        let msg = "Saved PNG failed byte or pixel validation".to_string();
        tracing::error!("{}", msg);
        return Err(msg);
    }

    let path_str = path.to_string_lossy().to_string();
    let _ = app.emit("screenshot-saved", &path_str);
    tracing::info!(
        "Saved validated PNG ({}x{}) to {}",
        image.width,
        image.height,
        path_str
    );
    Ok(path_str)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ValidatedImage {
    width: u32,
    height: u32,
    /// Number of pixels with a non-zero alpha channel. RGB images are opaque.
    opaque_pixels: u64,
    /// Number of pixels with at least one non-zero RGB channel.
    non_black_pixels: u64,
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    if input.trim().is_empty() {
        return Err("Save payload is empty".to_string());
    }
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| format!("Save payload is not valid base64: {e}"))
}

/// Decode the PNG without retaining screenshot data in logs. A desktop
/// screenshot must contain at least one non-transparent pixel; this rejects
/// the confirmed regression where a correctly-sized all-transparent PNG was
/// reported as a successful save.
fn validate_png(bytes: &[u8]) -> Result<ValidatedImage, String> {
    if bytes.len() < PNG_SIGNATURE.len() || &bytes[..PNG_SIGNATURE.len()] != PNG_SIGNATURE {
        return Err("Save payload is not a PNG (invalid signature)".to_string());
    }

    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("Save payload PNG header is invalid: {e}"))?;
    let width = reader.info().width;
    let height = reader.info().height;
    let pixels = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || pixels > MAX_IMAGE_PIXELS {
        return Err("Save payload PNG dimensions are invalid or too large".to_string());
    }

    let (color_type, bit_depth) = reader.output_color_type();
    if bit_depth != png::BitDepth::Eight {
        return Err("Save payload PNG must use 8-bit pixels".to_string());
    }
    let output_size = reader.output_buffer_size();
    let mut decoded = vec![0u8; output_size];
    let frame = reader
        .next_frame(&mut decoded)
        .map_err(|e| format!("Save payload PNG pixels are not decodable: {e}"))?;
    let decoded = &decoded[..frame.buffer_size()];

    let channels = color_type.samples();
    if channels == 0 || decoded.len() != pixels as usize * channels {
        return Err("Save payload PNG has inconsistent pixel dimensions".to_string());
    }
    let has_alpha = matches!(
        color_type,
        png::ColorType::Rgba | png::ColorType::GrayscaleAlpha
    );
    let mut opaque_pixels = 0;
    let mut non_black_pixels = 0;
    for pixel in decoded.chunks_exact(channels) {
        let alpha = if has_alpha { pixel[channels - 1] } else { 255 };
        if alpha != 0 {
            opaque_pixels += 1;
        }
        if pixel[..channels.min(3)].iter().any(|channel| *channel != 0) {
            non_black_pixels += 1;
        }
    }
    if opaque_pixels == 0 {
        return Err("Save payload PNG contains no visible pixels (all alpha is zero)".to_string());
    }
    Ok(ValidatedImage {
        width,
        height,
        opaque_pixels,
        non_black_pixels,
    })
}

fn configured_save_dir(folder: &str) -> Result<PathBuf, String> {
    let trimmed = folder.trim();
    if trimmed.is_empty() {
        return Err("Save folder is not configured; choose a folder in Settings".to_string());
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err("Save folder must be an absolute path; update it in Settings".to_string());
    }
    Ok(path)
}

fn unique_path(dir: &Path, filename: &str) -> PathBuf {
    let initial = dir.join(filename);
    if !initial.exists() {
        return initial;
    }
    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("screenshot");
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("png");
    for suffix in 1..=u32::MAX {
        let candidate = dir.join(format!("{stem}-{suffix}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    // The loop cannot realistically exhaust, but retain a deterministic path
    // if a hostile filesystem makes every candidate appear to exist.
    dir.join(format!("{stem}-{}.{}", std::process::id(), extension))
}

fn filename_from_pattern(pattern: &str) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string();
    let raw = pattern
        .trim()
        .replace("{date}", &timestamp)
        .replace("{time}", &timestamp);
    let safe = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim_matches('.')
        .trim();
    let stem = if safe.is_empty() { "screenshot" } else { safe };
    let stem = stem.strip_suffix(".png").unwrap_or(stem);
    format!("{stem}.png")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use png::{BitDepth, ColorType, Encoder};

    fn png_fixture(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = Encoder::new(&mut bytes, width, height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        writer.write_image_data(pixels).expect("pixels");
        drop(writer);
        bytes
    }

    #[test]
    fn base64_decode_valid() {
        let encoded = base64::engine::general_purpose::STANDARD.encode(b"hello world");
        assert_eq!(base64_decode(&encoded).expect("decode"), b"hello world");
    }

    #[test]
    fn malformed_and_empty_payloads_are_rejected() {
        assert!(base64_decode("").is_err());
        assert!(base64_decode("!!!not-base64!!!").is_err());
        let encoded = base64::engine::general_purpose::STANDARD.encode(b"not png");
        assert!(validate_png(&base64_decode(&encoded).unwrap()).is_err());
    }

    #[test]
    fn opaque_fixture_has_dimensions_and_pixels() {
        let bytes = png_fixture(2, 1, &[255, 0, 0, 255, 0, 0, 0, 255]);
        let image = validate_png(&bytes).expect("valid PNG");
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.opaque_pixels, 2);
        assert_eq!(image.non_black_pixels, 1);
    }

    #[test]
    fn all_transparent_fixture_is_rejected() {
        let transparent = png_fixture(2, 2, &[0; 16]);
        assert!(validate_png(&transparent)
            .expect_err("transparent image must fail")
            .contains("no visible pixels"));
        let opaque_black = png_fixture(2, 2, &vec![0, 0, 0, 255].repeat(4));
        assert!(
            validate_png(&opaque_black).is_ok(),
            "black screenshots are valid"
        );
    }

    #[test]
    fn transparent_pixels_are_allowed_when_image_has_opaque_content() {
        let bytes = png_fixture(2, 1, &[10, 20, 30, 0, 10, 20, 30, 255]);
        let image = validate_png(&bytes).expect("valid mixed-alpha PNG");
        assert_eq!(image.opaque_pixels, 1);
    }

    #[test]
    fn configured_folder_requires_absolute_path() {
        assert!(configured_save_dir("").is_err());
        assert!(configured_save_dir("relative/path").is_err());
        assert!(configured_save_dir(&std::env::temp_dir().to_string_lossy()).is_ok());
    }

    #[test]
    fn filename_pattern_is_safe_and_png() {
        let name = filename_from_pattern("../secret\\capture");
        assert!(!name.contains('/') && !name.contains('\\'));
        assert!(name.ends_with(".png"));
    }
}
