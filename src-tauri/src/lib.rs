mod capture;
mod clipboard;
mod hotkeys;
mod upload;

use capture::CaptureState;
use clipboard::ClipboardState;
use hotkeys::HotkeyState;
use tauri::Manager;
use upload::UploadState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(std::sync::Mutex::new(CaptureState::new()))
        .manage(std::sync::Mutex::new(ClipboardState::new()))
        .manage(std::sync::Mutex::new(HotkeyState::new()))
        .manage(std::sync::Mutex::new(UploadState::new()))
        .setup(|app| {
            // Initialise the platform hotkey thread (Windows) / no-op (Linux).
            hotkeys::init_hotkey_system();

            // Register default global shortcuts.
            // Ctrl+Shift+1 → full-monitor capture
            // Ctrl+Shift+2 → area-select capture
            let handle = app.handle().clone();
            hotkeys::store_app_handle(handle.clone());

            // We cannot call async Tauri commands from setup directly, so we
            // invoke the platform register helpers directly with the AppHandle.
            let r1 = hotkeys::register_hotkey_platform("Ctrl+Shift+1", handle.clone());
            let r2 = hotkeys::register_hotkey_platform("Ctrl+Shift+2", handle);

            // Sync HotkeyState so the frontend can discover defaults
            let state = app.state::<std::sync::Mutex<HotkeyState>>();
            if let Ok(mut guard) = state.lock() {
                if r1.is_ok() {
                    guard.hotkeys_registered.push("Ctrl+Shift+1".to_string());
                }
                if r2.is_ok() {
                    guard.hotkeys_registered.push("Ctrl+Shift+2".to_string());
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            capture::capture_full_screen,
            capture::capture_area,
            clipboard::copy_to_clipboard,
            clipboard::read_from_clipboard,
            hotkeys::register_hotkey,
            hotkeys::unregister_hotkey,
            upload::upload_screenshot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
