//! DXGI/DirectX screen capture module.
//!
//! Captures entire monitors or rectangular areas using the Windows
//! Desktop Duplication API (DXGI). Returns RGBA pixel data as base64.
//!
//! On non-Windows platforms, commands return an error — the frontend
//! already handles this by checking `__TAURI_INTERNALS__`.

use serde::Serialize;
use std::sync::Mutex;
use tauri::State;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Structured result returned to the frontend after a capture.
#[derive(Debug, Clone, Serialize)]
pub struct CaptureResult {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Base64-encoded RGBA pixel data (row-major, tightly packed).
    pub data: String,
}

/// Managed state holding platform-specific capture resources.
#[allow(dead_code)]
pub struct CaptureState {
    pub initialized: bool,
}

impl CaptureState {
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands — signatures match the scaffold exactly
// ---------------------------------------------------------------------------

/// Capture the entire monitor at `monitor_index`.
///
/// The primary monitor is index 0.
#[tauri::command]
pub async fn capture_full_screen(
    state: State<'_, Mutex<CaptureState>>,
    monitor_index: u32,
) -> Result<CaptureResult, String> {
    let _guard = state.lock().map_err(|e| e.to_string())?;
    platform::capture_monitor(monitor_index)
}

/// Capture a user-selected rectangular area from the primary monitor.
///
/// Coordinates are relative to the monitor origin (top-left).
#[tauri::command]
pub async fn capture_area(
    state: State<'_, Mutex<CaptureState>>,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<CaptureResult, String> {
    let _guard = state.lock().map_err(|e| e.to_string())?;
    platform::capture_rect(0, x, y, width as u32, height as u32)
}

// ---------------------------------------------------------------------------
// Platform-specific implementations
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod platform {
    use super::CaptureResult;
    use base64::Engine as _;
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Resource, ID3D11Texture2D,
        D3D11_BIND_FLAG, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
        D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
    };
    use windows::Win32::Graphics::Dxgi::Common::DXGI_OUTPUT_DESC;
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput, IDXGIOutput1,
        IDXGIOutputDuplication, IDXGIResource, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
    };

    // -- helpers ---------------------------------------------------------

    fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(bgra.len());
        for chunk in bgra.chunks_exact(4) {
            rgba.push(chunk[2]); // R ← B
            rgba.push(chunk[1]); // G ← G
            rgba.push(chunk[0]); // B ← R
            rgba.push(chunk[3]); // A ← A
        }
        rgba
    }

    /// Create a D3D11 device + immediate context.
    fn create_device() -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;

        let feature_levels = [];
        let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;

        unsafe {
            D3D11CreateDevice(
                None, // default adapter
                D3D_DRIVER_TYPE_HARDWARE,
                None, // no software module
                flags,
                &feature_levels,
                D3D11_SDK_VERSION as u32,
                Some(&mut device),
                None, // feature level out
                Some(&mut context),
            )
        }
        .map_err(|e| format!("D3D11CreateDevice failed: {e}"))?;

        let device = device.ok_or("D3D11 device is null")?;
        let context = context.ok_or("D3D11 context is null")?;
        Ok((device, context))
    }

    /// Get the output (monitor) at `monitor_index` by enumerating all
    /// adapters and outputs into a flat list, then indexing into it.
    fn get_output(
        factory: &IDXGIFactory1,
        monitor_index: u32,
    ) -> Result<(IDXGIOutput1, DXGI_OUTPUT_DESC), String> {
        unsafe {
            let mut outputs: Vec<(IDXGIOutput, DXGI_OUTPUT_DESC)> = Vec::new();
            let mut adapter_idx = 0u32;
            loop {
                let adapter: IDXGIAdapter1 = match factory.EnumAdapters1(adapter_idx) {
                    Ok(a) => a,
                    Err(_) => break,
                };
                let mut output_idx = 0u32;
                loop {
                    let output: IDXGIOutput = match adapter.EnumOutputs(output_idx) {
                        Ok(o) => o,
                        Err(_) => break,
                    };
                    let desc = output
                        .GetDesc()
                        .map_err(|e| format!("GetDesc failed: {e}"))?;
                    outputs.push((output, desc));
                    output_idx += 1;
                }
                adapter_idx += 1;
            }

            if (monitor_index as usize) >= outputs.len() {
                return Err(format!(
                    "monitor index {monitor_index} out of range (found {} outputs)",
                    outputs.len()
                ));
            }

            let (output, desc) = outputs.remove(monitor_index as usize);
            let output1: IDXGIOutput1 = output
                .cast()
                .map_err(|e| format!("Cast to IDXGIOutput1 failed: {e}"))?;

            Ok((output1, desc))
        }
    }

    /// Build the duplication object for the given output.
    fn duplicate_output(
        output1: &IDXGIOutput1,
        device: &ID3D11Device,
    ) -> Result<IDXGIOutputDuplication, String> {
        unsafe {
            output1
                .DuplicateOutput(device)
                .map_err(|e| format!("DuplicateOutput failed: {e}"))
        }
    }

    /// Acquire one frame and copy its BGRA pixels into a Vec<u8>.
    fn acquire_frame(
        dupl: &IDXGIOutputDuplication,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
    ) -> Result<(u32, u32, Vec<u8>), String> {
        unsafe {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource: Option<IDXGIResource> = None;

            loop {
                let hr = dupl.AcquireNextFrame(1000, &mut info, &mut resource);
                if hr == DXGI_ERROR_WAIT_TIMEOUT {
                    return Err(
                        "timeout acquiring next frame — screen may be locked or no updates".into(),
                    );
                }
                hr.map_err(|e| format!("AcquireNextFrame failed: {e}"))?;
                break;
            }

            let resource = resource.ok_or("AcquireNextFrame returned null resource")?;

            let result = process_acquired_frame(device, context, &resource);

            dupl.ReleaseFrame()
                .map_err(|e| format!("ReleaseFrame failed: {e}"))?;

            result
        }
    }

    /// Process a frame that has already been acquired via AcquireNextFrame.
    /// Copies BGRA pixels from the acquired texture to a CPU-readable Vec<u8>.
    unsafe fn process_acquired_frame(
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        resource: &IDXGIResource,
    ) -> Result<(u32, u32, Vec<u8>), String> {
        let tex: ID3D11Texture2D = resource
            .cast()
            .map_err(|e| format!("Cast resource to texture failed: {e}"))?;

        let mut tex_desc = D3D11_TEXTURE2D_DESC::default();
        tex.GetDesc(&mut tex_desc);

        let width = tex_desc.Width;
        let height = tex_desc.Height;

        // Create a staging texture for CPU read-back
        let mut staging_desc = tex_desc;
        staging_desc.Usage = windows::Win32::Graphics::Direct3D11::D3D11_USAGE_STAGING;
        staging_desc.BindFlags = D3D11_BIND_FLAG(0);
        staging_desc.CPUAccessFlags = windows::Win32::Graphics::Direct3D11::D3D11_CPU_ACCESS_READ;
        staging_desc.MiscFlags = 0;

        let staging_tex = {
            let mut tex: Option<ID3D11Texture2D> = None;
            device
                .CreateTexture2D(&staging_desc, None, Some(&mut tex))
                .map_err(|e| format!("CreateTexture2D (staging) failed: {e}"))?;
            tex.ok_or("CreateTexture2D returned null")?
        };

        context.CopyResource(&staging_tex, &tex);

        // Map the staging texture
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context
            .Map(&staging_tex, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .map_err(|e| format!("Map failed: {e}"))?;

        let row_pitch = mapped.RowPitch as usize;
        let data_ptr = mapped.pData as *const u8;

        // Copy row by row, respecting stride
        let row_bytes = (width as usize) * 4;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);

        for row in 0..(height as usize) {
            let src = data_ptr.add(row * row_pitch);
            let row_slice = std::slice::from_raw_parts(src, row_bytes);
            pixels.extend_from_slice(row_slice);
        }

        context.Unmap(&staging_tex, 0);

        Ok((width, height, pixels))
    }

    // -- public entry points ---------------------------------------------

    /// Capture the entire monitor, returning RGBA pixels as a pre-encoded
    /// CaptureResult.
    pub fn capture_monitor(monitor_index: u32) -> Result<CaptureResult, String> {
        let (width, height, rgba) = capture_monitor_rgba(monitor_index)?;
        let data = base64::engine::general_purpose::STANDARD.encode(&rgba);
        Ok(CaptureResult {
            width,
            height,
            data,
        })
    }

    /// Internal helper: capture the full monitor and return raw RGBA pixels.
    fn capture_monitor_rgba(monitor_index: u32) -> Result<(u32, u32, Vec<u8>), String> {
        unsafe {
            let factory: IDXGIFactory1 =
                CreateDXGIFactory1().map_err(|e| format!("CreateDXGIFactory1 failed: {e}"))?;

            let (output1, _desc) = get_output(&factory, monitor_index)?;
            let (device, context) = create_device()?;
            let dupl = duplicate_output(&output1, &device)?;

            let (width, height, bgra) = acquire_frame(&dupl, &device, &context)?;

            Ok((width, height, bgra_to_rgba(&bgra)))
        }
    }

    pub fn capture_rect(
        monitor_index: u32,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
    ) -> Result<CaptureResult, String> {
        let (full_width, full_height, rgba) = capture_monitor_rgba(monitor_index)?;

        let rx = x.max(0) as u32;
        let ry = y.max(0) as u32;
        let rw = w.min(full_width.saturating_sub(rx));
        let rh = h.min(full_height.saturating_sub(ry));

        if rw == 0 || rh == 0 {
            return Err("capture rectangle is empty or out of bounds".into());
        }

        let mut cropped = Vec::with_capacity((rw * rh * 4) as usize);
        for row in ry..(ry + rh) {
            let start = (row * full_width * 4 + rx * 4) as usize;
            let end = start + (rw * 4) as usize;
            cropped.extend_from_slice(&rgba[start..end]);
        }

        let data = base64::engine::general_purpose::STANDARD.encode(&cropped);

        Ok(CaptureResult {
            width: rw,
            height: rh,
            data,
        })
    }
}

// ---------------------------------------------------------------------------
// Non-Windows stub — build must succeed on Linux
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::CaptureResult;

    pub fn capture_monitor(_monitor_index: u32) -> Result<CaptureResult, String> {
        Err("screen capture is only supported on Windows".into())
    }

    pub fn capture_rect(
        _monitor_index: u32,
        _x: i32,
        _y: i32,
        _w: u32,
        _h: u32,
    ) -> Result<CaptureResult, String> {
        Err("screen capture is only supported on Windows".into())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    #[test]
    fn capture_result_serializes_to_json() {
        let result = CaptureResult {
            width: 1920,
            height: 1080,
            data: "iVBORw0KGgo".to_string(),
        };
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(json.contains("\"width\":1920"));
        assert!(json.contains("\"height\":1080"));
        assert!(json.contains("iVBORw0KGgo"));
    }

    #[test]
    fn capture_result_roundtrip_base64() {
        // Verify that base64 encoding used in the capture pipeline
        // produces decodable output (tests the base64 dependency is wired correctly).
        let pixels = vec![0u8; 1920 * 1080 * 4];
        let data = base64::engine::general_purpose::STANDARD.encode(&pixels);
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&data)
            .expect("decode");
        assert_eq!(decoded.len(), pixels.len());
    }

    #[test]
    fn capture_state_starts_uninitialized() {
        let state = CaptureState::new();
        assert!(!state.initialized);
    }

    #[test]
    fn non_windows_capture_stubs_return_error() {
        let r = platform::capture_monitor(0);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("Windows"));

        let r = platform::capture_rect(0, 0, 0, 100, 100);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("Windows"));
    }
}
