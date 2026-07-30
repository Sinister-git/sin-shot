# Project agent memory

This file is the project's committed home for project-intrinsic agent knowledge: build, test, release, architecture, and sharp-edge notes that should travel with the code.

- Add durable project-specific notes here as they are discovered through real work.

## Sin Shot — Windows Screenshot Tool

- **Stack**: Tauri v2 + Svelte 5 (SvelteKit with static adapter) + Rust edition 2021
- **Target**: Windows only (DXGI/DirectX capture, NSIS bundle)
- **Frontend**: `npm run dev` (Vite), `npm run build` (static SPA to `build/`)
- **Tauri**: `npm run tauri dev`, `npm run tauri build` — uses `src-tauri/`
- **Rust modules**: `capture.rs` (DXGI), `hotkeys.rs` (global hotkeys), `overlay.rs` (window resize/show/hide + monitor enum), `clipboard.rs`, `save.rs` (save to Pictures/Sin Shot/), `settings.rs` (user prefs persistence), `upload.rs` (→ sinister.ovh)
- **Settings persistence**: JSON file at `app_data_dir()/settings.json`, `Settings` struct with `#[serde(default)]`, loaded via `get_settings` / persisted via `save_settings` Tauri commands
- **Settings window**: normal framed window (label `"settings"`, URL `/settings`), created on-demand in `show_settings` command, accessible from tray menu
- **Tray**: built in `setup()` with `TrayIconBuilder`, menu items "Settings" (spawns `show_settings`) and "Quit"
- **Rust check**: `cargo check --manifest-path src-tauri/Cargo.toml`
- **Rust lint**: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
- **Frontend test**: `npm test` (vitest), `npm run check` (svelte-check)
- **Window config**: frameless, transparent, always-on-top overlay for capture mode
- **Annotation editor components**: `AnnotationCanvas.svelte` (HTML5 Canvas, tools: pen/arrow/rect/text/blur/eraser, 50-op undo/redo), `Toolbar.svelte` (vertical, right side), `ActionBar.svelte` (copy/save/upload/cancel)
- **Svelte 5 runes**: `$state`, `$props`, `$bindable()`, `$effect` — no legacy stores
- **No-mistakes gate**: push via `git push no-mistakes <branch>`

## Sin Shot Server — Self-Hosted Upload Server

- **Stack**: Rust (axum + tokio), single binary, `server/` directory
- **Check/build**: `cargo check --manifest-path server/Cargo.toml`, `cargo build --release --manifest-path server/Cargo.toml`
- **Lint**: `cargo clippy --manifest-path server/Cargo.toml -- -D warnings`
- **Config**: env vars `PORT` (8080), `STORAGE_DIR` (./uploads), `MAX_FILE_SIZE_MB` (25), `BASE_URL` (https://sinister.ovh), `GOOGLE_CLIENT_ID` (optional, enables gallery auth), `CORS_ORIGINS` (comma-separated, defaults to BASE_URL origin)
- **Endpoints**: `POST /api/upload` (multipart, field `image`), `GET /<key>` (serves stored image), `GET /api/gallery` (list images, requires auth), `DELETE /api/image/<key>` (delete image, requires auth)
- **Auth**: Gallery endpoints require `Authorization: Bearer <google-id-token>`. Token verified against Google's tokeninfo endpoint. Auth disabled if `GOOGLE_CLIENT_ID` is empty.
- **Rate limit**: 10 req/60s per IP, token bucket
- **Formats**: PNG, JPEG, WebP — detected by magic bytes, stored as `<key>.<ext>`
- **Deploy**: Dockerfile (alpine/musl static) or systemd unit at `server/systemd/sin-shot-server.service`

## Web Gallery — SvelteKit Gallery Route

- **Route**: `src/routes/gallery/+page.svelte` — SPA client-side page at `/gallery`
- **API client**: `src/lib/gallery-api.ts` — typed fetch wrapper for gallery endpoints
- **Build-time env**: `VITE_GALLERY_API_URL` (default https://sinister.ovh), `VITE_GOOGLE_CLIENT_ID` (Google OAuth client ID)
- **Auth**: Google Sign-In via GIS (`accounts.google.com/gsi/client`), ID token stored in localStorage, sent as Bearer token
- **Deploy**: static SPA build (`npm run build` → `build/`), served from `screenshots.sinister.ovh` pointing to the `build/` directory, with nginx configured to fallback to `index.html` for SPA routing

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.
