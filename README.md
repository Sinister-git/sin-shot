# Sin Shot

A sharp screenshot tool for Windows — capture, annotate, and share.

## Stack
- **Desktop app:** Tauri v2 + Rust + Svelte 5
- **Upload server:** Self-hosted Go/Rust binary
- **Web gallery:** screenshots.sinister.ovh

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
