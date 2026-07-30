mod capture;
mod clipboard;
mod hotkeys;
mod overlay;
mod save;
mod settings;
mod upload;

use capture::CaptureState;
use clipboard::ClipboardState;
use hotkeys::HotkeyState;
use save::SaveState;
use std::path::PathBuf;
use tauri::Manager;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use upload::UploadState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ── File + console logging ─────────────────────────────────────────
    let log_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.sinister.sin-shot")
        .join("logs");
    std::fs::create_dir_all(&log_dir).expect("failed to create log directory");

    let file_appender = tracing_appender::rolling::daily(&log_dir, "sin-shot.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    // Keep the flush guard alive for the lifetime of the application.
    let _log_guard = Box::leak(Box::new(guard));

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking);

    let stdout_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .init();

    tracing::info!(
        "Sin Shot v{} starting — data dir: {}",
        env!("CARGO_PKG_VERSION"),
        log_dir.parent().unwrap_or(&log_dir).display()
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(std::sync::Mutex::new(CaptureState::new()))
        .manage(std::sync::Mutex::new(ClipboardState::new()))
        .manage(std::sync::Mutex::new(HotkeyState::new()))
        .manage(std::sync::Mutex::new(UploadState::new()))
        .manage(std::sync::Mutex::new(SaveState::new()))
        .setup(|app| {
            // Initialise the platform hotkey thread (Windows) / no-op (Linux).
            hotkeys::init_hotkey_system();

            // Load persisted settings, falling back to defaults
            let handle = app.handle().clone();
            hotkeys::store_app_handle(handle.clone());
            let persisted = settings::load_settings_sync(&handle);

            // Register global shortcuts from persisted settings
            let r1 = hotkeys::register_hotkey_platform(&persisted.hotkey_full, handle.clone());
            let r2 = hotkeys::register_hotkey_platform(&persisted.hotkey_area, handle);

            // Log registration results so the user can diagnose conflicts
            if let Err(ref e) = r1 {
                tracing::error!(
                    "Failed to register hotkey '{}': {}",
                    persisted.hotkey_full,
                    e
                );
            } else {
                tracing::info!("Registered hotkey: {}", persisted.hotkey_full);
            }
            if let Err(ref e) = r2 {
                tracing::error!(
                    "Failed to register hotkey '{}': {}",
                    persisted.hotkey_area,
                    e
                );
            } else {
                tracing::info!("Registered hotkey: {}", persisted.hotkey_area);
            }

            // Sync HotkeyState so the frontend can discover registered hotkeys
            let state = app.state::<std::sync::Mutex<HotkeyState>>();
            if let Ok(mut guard) = state.lock() {
                if r1.is_ok() {
                    guard.hotkeys_registered.push(persisted.hotkey_full);
                }
                if r2.is_ok() {
                    guard.hotkeys_registered.push(persisted.hotkey_area);
                }
            }

            // Build system tray with Settings & Quit menu items
            let settings_item = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .item(&settings_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let handle_clone = app.handle().clone();
            let tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().unwrap())
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |_app, event| match event.id().as_ref() {
                    "settings" => {
                        let h = handle_clone.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = settings::show_settings(h).await;
                        });
                    }
                    "quit" => {
                        std::process::exit(0);
                    }
                    _ => {}
                })
                .build(app)?;
            app.manage(tray);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            capture::capture_full_screen,
            capture::capture_area,
            clipboard::copy_to_clipboard,
            clipboard::read_from_clipboard,
            overlay::start_capture,
            overlay::cancel_capture,
            overlay::get_monitors,
            settings::get_settings,
            settings::save_settings,
            settings::get_hotkeys,
            settings::show_settings,
            upload::upload_screenshot,
            save::save_to_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    /// Verify that the tracing subscriber can be initialised with a file
    /// appender and that log events are written to the expected log file.
    #[test]
    fn logging_writes_to_file() {
        let tmp = std::env::temp_dir().join("sin-shot-logging-test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create temp log dir");

        // Set up the same logging pipeline used in production.
        let file_appender = tracing_appender::rolling::daily(&tmp, "sin-shot.log");
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

        // Use a try_init so we only initialise once (subsequent calls are no-ops).
        let _ = tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new("info"))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(non_blocking),
            )
            .try_init();

        tracing::info!(
            "Sin Shot v{} starting — data dir: {}",
            env!("CARGO_PKG_VERSION"),
            tmp.display()
        );
        tracing::info!("Registered hotkey: Ctrl+Shift+S");
        tracing::info!("Capturing monitor: primary");
        tracing::info!("Saved screenshot to /fake/path.png");
        tracing::info!("Copied 12345 bytes to clipboard");
        tracing::error!("Failed to register hotkey 'Alt+X': already in use");

        // Flush pending writes so the file is readable.
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Locate today's log file (daily rolling appends YYYY-MM-DD).
        let mut found = false;
        for entry in std::fs::read_dir(&tmp).expect("read log dir") {
            let entry = entry.expect("entry");
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("sin-shot.log") {
                let mut f = std::fs::File::open(entry.path()).expect("open log");
                let mut contents = String::new();
                f.read_to_string(&mut contents).expect("read log");

                assert!(
                    contents.contains("Sin Shot v"),
                    "startup message missing: {}",
                    contents
                );
                assert!(
                    contents.contains("Registered hotkey: Ctrl+Shift+S"),
                    "hotkey missing"
                );
                assert!(
                    contents.contains("Capturing monitor: primary"),
                    "capture missing"
                );
                assert!(contents.contains("Saved screenshot to"), "save missing");
                assert!(
                    contents.contains("Copied 12345 bytes to clipboard"),
                    "clipboard missing"
                );
                assert!(
                    contents.contains("Failed to register hotkey"),
                    "error missing"
                );
                found = true;
                break;
            }
        }

        assert!(found, "no log file found in {:?}", tmp);

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
