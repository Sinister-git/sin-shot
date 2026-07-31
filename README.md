<div align="center">

# Sin Shot

**Capture the screen. Mark it up. Share it.**

A sharp, Windows-first screenshot workflow for grabbing a monitor or region,
annotating the result, and sending it where it needs to go.

[![Windows](https://img.shields.io/badge/platform-Windows-0078D4?logo=windows&logoColor=white)](https://www.microsoft.com/windows)
[![Tauri v2](https://img.shields.io/badge/Tauri-v2-24C8DB?logo=tauri&logoColor=white)](https://v2.tauri.app/)
[![Svelte 5](https://img.shields.io/badge/Svelte-5-FF3E00?logo=svelte&logoColor=white)](https://svelte.dev/)

</div>

---

## Why Sin Shot?

Sin Shot keeps the screenshot loop short: invoke a global shortcut, choose a
monitor or area, add just enough context, then copy, save, or upload. It is a
small Tauri desktop app backed by native Windows capture rather than a browser
snapshot.

## Highlights

- **Full-monitor capture** using Windows DXGI/Desktop Duplication.
- **Area selection** across the Windows virtual desktop, with a movable frame
  before the final crop is captured.
- **Fast annotation** with a canvas editor, custom colors, keyboard shortcuts,
  and up to 50 undo/redo history states.
- **Three handoff paths:** copy the finished PNG to the clipboard, save it to a
  configured folder, or upload it to a compatible Sin Shot server and copy a
  short share URL.
- **Tray-based workflow** with configurable capture shortcuts and persisted
  preferences.
- **Optional gallery stack** consisting of a self-hosted Rust upload server and
  a standalone SvelteKit web gallery.

## The workflow

1. Press a capture shortcut.
2. Click a monitor, or drag to select an area.
3. Annotate the captured frame.
4. Choose **Copy**, **Save**, or **Upload**.

For area captures, Sin Shot waits until an export action to perform the native
crop. This keeps the selection editable while you position it precisely.

## Annotation tools

| Tool | Use it for | Shortcut |
| --- | --- | :---: |
| Pen | Freehand marks | <kbd>P</kbd> |
| Arrow | Pointing out a detail | <kbd>A</kbd> |
| Rectangle | Framing an area | <kbd>R</kbd> |
| Text | Adding a label | <kbd>T</kbd> |
| Blur | Obscuring part of an image | <kbd>B</kbd> |
| Eraser | Removing annotation strokes | <kbd>E</kbd> |

The toolbar also includes preset colors and a custom color picker. In the
editor, <kbd>Ctrl</kbd>/<kbd>Cmd</kbd> + <kbd>Z</kbd> undoes and
<kbd>Ctrl</kbd>/<kbd>Cmd</kbd> + <kbd>Y</kbd> (or <kbd>Shift</kbd> +
<kbd>Ctrl</kbd>/<kbd>Cmd</kbd> + <kbd>Z</kbd>) redoes.

## Quick start for Windows

### Prerequisites

- Windows with a working native desktop development environment.
- [Node.js](https://nodejs.org/) and npm.
- [Rust](https://www.rust-lang.org/tools/install).
- The [Tauri v2 Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows).

### Run from source

```powershell
git clone https://github.com/Sinister-git/sin-shot.git
cd sin-shot
npm install
npm run tauri dev
```

The app registers its default global shortcuts when it starts:

| Shortcut | Action |
| --- | --- |
| <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>1</kbd> | Capture a full monitor |
| <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>2</kbd> | Select an area |

Shortcuts can be rebound from **Settings** in the system tray. Press
<kbd>Esc</kbd> to cancel an active capture or annotation session.

### Build the Windows installer

```powershell
npm run tauri build
```

The Tauri configuration targets an NSIS installer. Build artifacts are written
under `src-tauri/target/release/bundle/nsis/`.

## Configuration

Sin Shot exposes settings for the save folder, filename pattern, capture
shortcuts, upload endpoint, and a few workflow preferences. Settings are
stored as JSON in Tauri's platform-specific app data directory at
`settings.json`.

The defaults include:

| Setting | Default |
| --- | --- |
| Save folder | `%USERPROFILE%\\Pictures\\Sin Shot` on Windows |
| Filename pattern | `screenshot_{date}_{time}` |
| Image format | PNG (the desktop exporter currently supports PNG only) |
| Full-monitor shortcut | `Ctrl+Shift+1` |
| Area shortcut | `Ctrl+Shift+2` |
| Upload endpoint | `https://screenshots.sinister.ovh/api/upload` |
| Copy share URL after upload | Enabled |

The upload client requires HTTPS, except for loopback HTTP endpoints used for
local testing. The configured URL may be the server origin or end in
`/api/upload`. Desktop uploads are validated PNGs and are limited to 25 MiB.

## Optional upload server and gallery

The repository also contains a standalone server and web gallery:

- [`server/`](server/) is an Axum/Tokio service that accepts multipart image
  uploads, stores PNG/JPEG/WebP files, serves image URLs, and exposes an
  authenticated gallery API.
- [`gallery-web/`](gallery-web/) is a static SvelteKit gallery. The server
  serves its build output and falls back to the gallery's `index.html` for SPA
  routes.

To build the gallery and server locally:

```powershell
cd gallery-web
npm install
npm run build
cd ..
cargo run --manifest-path server/Cargo.toml
```

The server defaults to port `8080`, stores uploads in `./uploads`, serves
static files from `./gallery-web/build`, and allows files up to 25 MiB. These
values can be changed with `PORT`, `STORAGE_DIR`, `STATIC_DIR`,
`MAX_FILE_SIZE_MB`, `BASE_URL`, `GOOGLE_CLIENT_ID`, and `CORS_ORIGINS`.
See [`server/src/main.rs`](server/src/main.rs) and
[`server/Dockerfile`](server/Dockerfile) for the authoritative configuration
and container deployment path. When `GOOGLE_CLIENT_ID` is configured, gallery
listing and deletion require a Google ID token; upload and public image serving
remain separate routes.

## Development and validation

The desktop app combines Svelte 5 with Tauri v2 and Rust:

```text
src/                 Svelte desktop UI and annotation editor
src-tauri/           Tauri commands, Windows capture, save, clipboard, upload
server/              Self-hosted upload server
 gallery-web/        Standalone SvelteKit gallery
```

Useful checks from the repository root:

```powershell
npm run check
npm test
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo check --manifest-path server/Cargo.toml
cd gallery-web
npm run check
```

The native DXGI capture path, global hotkey registration, virtual-desktop
coordinates, and per-monitor DPI behavior require a Windows runtime to verify.
Non-Windows environments can run frontend checks and some platform-independent
Rust tests, but they do not provide a supported capture target.

Useful implementation entry points include:

- [`Overlay.svelte`](src/lib/components/Overlay.svelte) — capture state and
  export workflow.
- [`AnnotationCanvas.svelte`](src/lib/components/AnnotationCanvas.svelte) —
  image composition and annotation history.
- [`capture.rs`](src-tauri/src/capture.rs) — native monitor and area capture.
- [`settings.rs`](src-tauri/src/settings.rs) — persisted preferences and
  hotkey rebinding.

## Project status

Sin Shot is currently version **0.1.0** and is under active development. The
supported product target is Windows; there is no promise of cross-platform
native capture. The documented installation path is a build from source, and
native behavior should be validated on Windows before relying on it for
production workflows.

## Contributing

Contributions are welcome. Before opening a pull request:

1. Keep changes focused and describe the user-visible behavior.
2. Run the relevant frontend and Rust checks above.
3. For capture, DPI, hotkey, or installer changes, include Windows validation
   details.
4. Update this README when a user-facing command, setting, or supported workflow
   changes.

## License

The root package declares the [MIT license](package.json). A standalone
`LICENSE` file is not currently included in the repository.
