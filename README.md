# Sin Shot

A sharp screenshot tool for Windows — capture, annotate, and share.

## Stack
- **Desktop app:** Tauri v2 + Rust + Svelte 5
- **Upload server:** Self-hosted Rust binary (axum + tokio), serves gallery static files
- **Web gallery:** Standalone SvelteKit app in `gallery-web/`, built to static SPA

## Development
```bash
# Prerequisites
cargo install tauri-cli
npm install

# Run in dev mode
cargo tauri dev

# Build for Windows
cargo tauri build
```

## Validation note

The Rust unit tests cover hotkey ownership and rollback bookkeeping on every
platform, but the native `RegisterHotKey`/`UnregisterHotKey` message-window
lifecycle can only be validated on Windows. A Windows run is still required to
verify OS-level release and rebind behavior; F11-to-F10 is one regression case.

## Capture overlay geometry (Windows)

The overlay contract is native and physical-pixel based: the Rust side
enumerates Windows monitor bounds, places the frameless overlay, and then
returns the post-placement WebView client origin and size from Tauri's
`inner_position()`/`inner_size()` together with that WebView's scale factor.
The frontend applies only that one client-boundary scale; it does not infer
client bounds from the requested native outer rectangle or from individual
monitor DPI metadata. The overlay shadow is disabled, but the readback remains
authoritative for any Windows non-client behavior that remains.

This geometry path is intentionally Windows-runtime-specific. Linux/macOS
builds can compile and run the pure coordinate tests, but they do not validate
DXGI capture, Windows virtual-desktop coordinates, Windows per-monitor DPI
transitions, or Windows frameless-window client insets. A Windows validation
run must exercise negative virtual-desktop origins, monitor gaps and vertical
offsets, mixed per-monitor scales, differing resolutions/aspect ratios, and
all taskbar placements. It should compare the monitor outline's outer edge to
native monitor bounds after placement; no CSS calibration offset is valid.
