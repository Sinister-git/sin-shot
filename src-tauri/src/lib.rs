mod capture;
mod clipboard;
mod hotkeys;
mod upload;

use capture::CaptureState;
use clipboard::ClipboardState;
use hotkeys::HotkeyState;
use upload::UploadState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(std::sync::Mutex::new(CaptureState::new()))
        .manage(std::sync::Mutex::new(ClipboardState::new()))
        .manage(std::sync::Mutex::new(HotkeyState::new()))
        .manage(std::sync::Mutex::new(UploadState::new()))
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
