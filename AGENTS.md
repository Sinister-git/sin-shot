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
- **App flow states** (managed in `Overlay.svelte`): full-screen capture transitions from `capturing` to `annotating`; area capture transitions from `capturing` to a visible, movable selection frame, then uses `committing` only while Save, Copy, or Upload performs the native crop, before returning to annotation or `idle`. Cancel resets both flows; area export temporarily hides the overlay while capturing and restores it afterward.
- **Annotation image export**: `AnnotationCanvas.getAnnotatedImage(finalImage?)` composites annotations with either the loaded full-screen image or the native area image supplied at commit time, returning raw base64 PNG without a data-URL prefix. Call it before invoking clipboard/save/upload commands; area mode must first capture the current selection geometry.
- **Backend image parameter convention**: all three commands (`copy_to_clipboard`, `save_to_file`, `upload_screenshot`) accept `image_data_base64: String` — a base64-encoded PNG without the `data:image/png;base64,` prefix. Each decodes it internally via `base64::Engine`.
- **Svelte 5 runes**: `$state`, `$props`, `$bindable()`, `$effect` — no legacy stores
- **No-mistakes gate**: push via `git push no-mistakes <branch>`

## Sin Shot Server — Self-Hosted Upload Server

- **Stack**: Rust (axum + tokio), single binary, `server/` directory
- **Check/build**: `cargo check --manifest-path server/Cargo.toml`, `cargo build --release --manifest-path server/Cargo.toml`
- **Lint**: `cargo clippy --manifest-path server/Cargo.toml -- -D warnings`
- **Config**: env vars `PORT` (8080), `STORAGE_DIR` (./uploads), `STATIC_DIR` (./gallery-web/build), `MAX_FILE_SIZE_MB` (25), `BASE_URL` (https://sinister.ovh), `GOOGLE_CLIENT_ID` (optional, enables gallery auth), `CORS_ORIGINS` (comma-separated, defaults to BASE_URL origin)
- **Endpoints**: `POST /api/upload` (multipart, field `image`), `GET /<key>` (serves stored image), `GET /x/{key}` (short URL alias for serving images), `GET /api/gallery` (list images, requires auth), `DELETE /api/image/<key>` (delete image, requires auth)
- **Static files**: Serves `STATIC_DIR` (gallery-web/build) for `/` and `/_app/*` with SPA fallback to `index.html`. A `/favicon.png` route is registered before `/{key}` to avoid conflicting with the image key pattern.
- **Auth**: Gallery endpoints require `Authorization: Bearer <google-id-token>`. Token verified against Google's tokeninfo endpoint. Auth disabled if `GOOGLE_CLIENT_ID` is empty.
- **Rate limit**: 10 req/60s per IP, token bucket
- **Formats**: PNG, JPEG, WebP — detected by magic bytes, stored as `<key>.<ext>`
- **Deploy**: Dockerfile (alpine/musl static) or systemd unit at `server/systemd/sin-shot-server.service`

## Gallery Web — Standalone SvelteKit Web App

- **Stack**: SvelteKit with `@sveltejs/adapter-static`, builds to `gallery-web/build/`
- **Directory**: `gallery-web/` — separate from the desktop app's `src/`
- **Build**: `cd gallery-web && npm run build` (or `npm install && npm run build`)
- **Check**: `cd gallery-web && npm run check`
- **Root page**: `src/routes/+page.svelte` — sign-in screen (when not authenticated) and image grid gallery (when authenticated)
- **API client**: `src/lib/gallery-api.ts` — copied from desktop app, uses `fetch` + `localStorage`, no Tauri dependencies
- **Build-time env**: `VITE_GALLERY_API_URL` (default https://sinister.ovh), `VITE_GOOGLE_CLIENT_ID` (Google OAuth client ID)
- **Auth**: Google Sign-In via GIS (`accounts.google.com/gsi/client`), ID token stored in localStorage, sent as Bearer token
- **Deploy**: Server serves static files from `gallery-web/build/` with SPA fallback. Dockerfile builds both the Rust server and this frontend.

## Desktop App Gallery Route (legacy)

- **Route**: `src/routes/gallery/+page.svelte` — existing SPA page at `/gallery` inside the Tauri desktop app
- **Note**: This remains in the desktop app for in-app gallery access. The standalone gallery-web is the canonical web version.

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.
