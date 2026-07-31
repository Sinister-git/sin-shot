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
