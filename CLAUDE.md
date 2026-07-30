# Project agent memory

This file is the project's committed home for project-intrinsic agent knowledge: build, test, release, architecture, and sharp-edge notes that should travel with the code.

- Add durable project-specific notes here as they are discovered through real work.

## Sin Shot — Windows Screenshot Tool

- **Stack**: Tauri v2 + Svelte 5 (SvelteKit with static adapter) + Rust edition 2021
- **Target**: Windows only (DXGI/DirectX capture, NSIS bundle)
- **Frontend**: `npm run dev` (Vite), `npm run build` (static SPA to `build/`)
- **Tauri**: `npm run tauri dev`, `npm run tauri build` — uses `src-tauri/`
- **Rust modules**: `capture.rs` (DXGI), `hotkeys.rs` (global hotkeys), `clipboard.rs`, `upload.rs` (→ sinister.ovh)
- **Rust check**: `cargo check --manifest-path src-tauri/Cargo.toml`
- **Window config**: frameless, transparent, always-on-top overlay for capture mode
- **No-mistakes gate**: push via `git push no-mistakes <branch>`

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.
