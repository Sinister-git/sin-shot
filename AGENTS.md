# Project agent memory

This file is the project's committed home for project-intrinsic agent knowledge: build, test, release, architecture, and sharp-edge notes that should travel with the code.

- Add durable project-specific notes here as they are discovered through real work.

## Sin Shot — Windows Screenshot Tool

- **Stack**: Tauri v2 + Svelte 5 (SvelteKit with static adapter) + Rust edition 2021
- **Target**: Windows only (DXGI/DirectX capture, NSIS bundle)
- **Frontend**: `npm run dev` (Vite), `npm run build` (static SPA to `build/`)
- **Tauri**: `npm run tauri dev`, `npm run tauri build` — uses `src-tauri/`
- **Rust modules**: `capture.rs` (DXGI), `hotkeys.rs` (global hotkeys), `overlay.rs` (window resize/show/hide + monitor enum), `clipboard.rs`, `settings.rs` (user prefs persistence), `upload.rs` (→ sinister.ovh)
- **Settings persistence**: JSON file at `app_data_dir()/settings.json`, `Settings` struct with `#[serde(default)]`, loaded via `get_settings` / persisted via `save_settings` Tauri commands
- **Settings window**: normal framed window (label `"settings"`, URL `/settings`), created on-demand in `show_settings` command, accessible from tray menu
- **Tray**: built in `setup()` with `TrayIconBuilder`, menu items "Settings" (spawns `show_settings`) and "Quit"
- **Rust check**: `cargo check --manifest-path src-tauri/Cargo.toml`
- **Window config**: frameless, transparent, always-on-top overlay for capture mode
- **No-mistakes gate**: push via `git push no-mistakes <branch>`

## Sin Shot Server — Self-Hosted Upload Server

- **Stack**: Rust (axum + tokio), single binary, `server/` directory
- **Check/build**: `cargo check --manifest-path server/Cargo.toml`, `cargo build --release --manifest-path server/Cargo.toml`
- **Config**: env vars `PORT` (8080), `STORAGE_DIR` (./uploads), `MAX_FILE_SIZE_MB` (25), `BASE_URL` (https://sinister.ovh)
- **Endpoints**: `POST /api/upload` (multipart, field `image`), `GET /<key>` (serves stored image)
- **Rate limit**: 10 req/60s per IP, token bucket
- **Formats**: PNG, JPEG, WebP — detected by magic bytes, stored as `<key>.<ext>`
- **Deploy**: Dockerfile (alpine/musl static) or systemd unit at `server/systemd/sin-shot-server.service`

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.
