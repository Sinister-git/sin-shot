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
use tauri::Manager;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};
use upload::UploadState;
use std::path::PathBuf;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

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

    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout);

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .init();

    tracing::info!(
        "Sin Shot v{} starting — config path: {}",
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
                tracing::error!("Failed to register hotkey '{}': {}", persisted.hotkey_full, e);
            } else {
                tracing::info!("Registered hotkey: {}", persisted.hotkey_full);
            }
            if let Err(ref e) = r2 {
                tracing::error!("Failed to register hotkey '{}': {}", persisted.hotkey_area, e);
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
            hotkeys::register_hotkey,
            hotkeys::unregister_hotkey,
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
