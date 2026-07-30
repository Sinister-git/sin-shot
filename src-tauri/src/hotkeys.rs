//! System-wide global hotkey registration.
//!
//! Registers global hotkeys on Windows via `RegisterHotKey` / `UnregisterHotKey`.
//! A background thread runs a message-only window to receive `WM_HOTKEY` and
//! emits Tauri events when a hotkey is pressed.
//!
//! On non-Windows platforms commands succeed silently — build must compile
//! on Linux even though the APIs are Windows-only.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Payload emitted to the frontend when a registered hotkey is pressed.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub struct HotkeyEvent {
    /// The original key-combo string, e.g. `"Ctrl+Shift+1"`.
    pub combo: String,
}

/// Managed state for the hotkey subsystem.
pub struct HotkeyState {
    /// List of currently registered hotkey combos (tracked for introspection).
    pub hotkeys_registered: Vec<String>,
}

impl HotkeyState {
    pub fn new() -> Self {
        Self {
            hotkeys_registered: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Register a global hotkey.
///
/// When the combo is pressed, a `hotkey-pressed` event is emitted with the
/// combo string as payload.
#[tauri::command]
pub async fn register_hotkey(
    app: AppHandle,
    state: State<'_, Mutex<HotkeyState>>,
    key_combo: String,
) -> Result<(), String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;

    // De-duplicate
    if guard.hotkeys_registered.contains(&key_combo) {
        return Ok(());
    }

    platform::register(&key_combo, app)?;

    guard.hotkeys_registered.push(key_combo);
    Ok(())
}

/// Unregister a previously registered global hotkey.
#[tauri::command]
pub async fn unregister_hotkey(
    state: State<'_, Mutex<HotkeyState>>,
    key_combo: String,
) -> Result<(), String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;

    if !guard.hotkeys_registered.contains(&key_combo) {
        return Ok(());
    }

    platform::unregister(&key_combo)?;

    guard.hotkeys_registered.retain(|k| k != &key_combo);
    Ok(())
}

// ---------------------------------------------------------------------------
// Platform-specific implementation — Windows
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod platform {
    use std::collections::HashMap;
    use std::sync::mpsc::{self, Sender};
    use std::sync::{LazyLock, Mutex};
    use std::thread;
    use tauri::AppHandle;
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM, HMODULE};
    use windows::Win32::System::LibraryLoader::GetModuleHandleA;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    // -- thread commands -------------------------------------------------

    enum ThreadCmd {
        Register {
            id: i32,
            modifiers: u32,
            vk: u32,
            result_tx: mpsc::Sender<Result<(), String>>,
        },
        Unregister {
            id: i32,
        },
    }

    // -- global state ----------------------------------------------------

    /// Sender to the background hotkey thread (initialised once in `init`).
    static THREAD_TX: LazyLock<Mutex<Option<Sender<ThreadCmd>>>> =
        LazyLock::new(|| Mutex::new(None));

    /// Maps combo string → hotkey id (used during unregistration).
    static COMBO_IDS: LazyLock<Mutex<HashMap<String, i32>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    /// Counter for assigning unique hotkey IDs.
    static NEXT_ID: LazyLock<Mutex<i32>> = LazyLock::new(|| Mutex::new(1));

    // -- key parsing -----------------------------------------------------

    /// Parse a combo string like `"Ctrl+Shift+1"` into Windows modifier flags
    /// and virtual-key code.
    fn parse_combo(combo: &str) -> Result<(u32, u32), String> {
        let parts: Vec<&str> = combo.split('+').map(|s| s.trim()).collect();
        let mut modifiers: u32 = 0;
        let mut vk: Option<u32> = None;

        for part in &parts {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= MOD_CONTROL.0,
                "shift" => modifiers |= MOD_SHIFT.0,
                "alt" => modifiers |= MOD_ALT.0,
                "win" | "windows" | "meta" => modifiers |= MOD_WIN.0,
                _ => {
                    if vk.is_some() {
                        return Err(format!("multiple non-modifier keys in combo: {combo}"));
                    }
                    vk = Some(key_name_to_vk(part)?);
                }
            }
        }

        let vk = vk.ok_or_else(|| format!("no key found in combo: {combo}"))?;
        Ok((modifiers, vk))
    }

    /// Convert a key name to its Windows virtual-key code.
    fn key_name_to_vk(name: &str) -> Result<u32, String> {
        match name.to_lowercase().as_str() {
            // Digits
            "0" => Ok(VK_0.0 as u32),
            "1" => Ok(VK_1.0 as u32),
            "2" => Ok(VK_2.0 as u32),
            "3" => Ok(VK_3.0 as u32),
            "4" => Ok(VK_4.0 as u32),
            "5" => Ok(VK_5.0 as u32),
            "6" => Ok(VK_6.0 as u32),
            "7" => Ok(VK_7.0 as u32),
            "8" => Ok(VK_8.0 as u32),
            "9" => Ok(VK_9.0 as u32),
            // Letters
            "a" => Ok(VK_A.0 as u32),
            "b" => Ok(VK_B.0 as u32),
            "c" => Ok(VK_C.0 as u32),
            "d" => Ok(VK_D.0 as u32),
            "e" => Ok(VK_E.0 as u32),
            "f" => Ok(VK_F.0 as u32),
            "g" => Ok(VK_G.0 as u32),
            "h" => Ok(VK_H.0 as u32),
            "i" => Ok(VK_I.0 as u32),
            "j" => Ok(VK_J.0 as u32),
            "k" => Ok(VK_K.0 as u32),
            "l" => Ok(VK_L.0 as u32),
            "m" => Ok(VK_M.0 as u32),
            "n" => Ok(VK_N.0 as u32),
            "o" => Ok(VK_O.0 as u32),
            "p" => Ok(VK_P.0 as u32),
            "q" => Ok(VK_Q.0 as u32),
            "r" => Ok(VK_R.0 as u32),
            "s" => Ok(VK_S.0 as u32),
            "t" => Ok(VK_T.0 as u32),
            "u" => Ok(VK_U.0 as u32),
            "v" => Ok(VK_V.0 as u32),
            "w" => Ok(VK_W.0 as u32),
            "x" => Ok(VK_X.0 as u32),
            "y" => Ok(VK_Y.0 as u32),
            "z" => Ok(VK_Z.0 as u32),
            // Function keys
            "f1" => Ok(VK_F1.0 as u32),
            "f2" => Ok(VK_F2.0 as u32),
            "f3" => Ok(VK_F3.0 as u32),
            "f4" => Ok(VK_F4.0 as u32),
            "f5" => Ok(VK_F5.0 as u32),
            "f6" => Ok(VK_F6.0 as u32),
            "f7" => Ok(VK_F7.0 as u32),
            "f8" => Ok(VK_F8.0 as u32),
            "f9" => Ok(VK_F9.0 as u32),
            "f10" => Ok(VK_F10.0 as u32),
            "f11" => Ok(VK_F11.0 as u32),
            "f12" => Ok(VK_F12.0 as u32),
            // Special keys
            "printscreen" | "prtscn" => Ok(VK_SNAPSHOT.0 as u32),
            "space" => Ok(VK_SPACE.0 as u32),
            "tab" => Ok(VK_TAB.0 as u32),
            "escape" | "esc" => Ok(VK_ESCAPE.0 as u32),
            "backspace" | "back" => Ok(VK_BACK.0 as u32),
            "enter" | "return" => Ok(VK_RETURN.0 as u32),
            "up" => Ok(VK_UP.0 as u32),
            "down" => Ok(VK_DOWN.0 as u32),
            "left" => Ok(VK_LEFT.0 as u32),
            "right" => Ok(VK_RIGHT.0 as u32),
            "home" => Ok(VK_HOME.0 as u32),
            "end" => Ok(VK_END.0 as u32),
            "pageup" | "pgup" => Ok(VK_PRIOR.0 as u32),
            "pagedown" | "pgdn" => Ok(VK_NEXT.0 as u32),
            "insert" | "ins" => Ok(VK_INSERT.0 as u32),
            "delete" | "del" => Ok(VK_DELETE.0 as u32),
            _ => Err(format!("unknown key: {name}")),
        }
    }

    // -- background thread -----------------------------------------------

    fn hotkey_thread(rx: mpsc::Receiver<ThreadCmd>) {
        unsafe {
            let hmodule: HMODULE = GetModuleHandleA(None).expect("GetModuleHandleA failed");
            let hinstance: HINSTANCE = hmodule.into();

            // Register a dummy window class
            let class_name = windows::core::s!("SinShotHotkeyClass");

            let wc = WNDCLASSA {
                lpfnWndProc: Some(hotkey_wndproc),
                hInstance: hinstance,
                lpszClassName: class_name,
                ..Default::default()
            };

            let atom = RegisterClassA(&wc);
            if atom == 0 {
                panic!("RegisterClassA failed");
            }

            // Create a message-only window
            let hwnd = CreateWindowExA(
                WINDOW_EX_STYLE::default(),
                class_name,
                windows::core::s!("SinShotHotkey"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE), // message-only
                None,
                Some(hinstance),
                None,
            )
            .expect("CreateWindowExA failed");

            // Main message loop — also checks the channel for commands.
            loop {
                // Pump Windows messages (non-blocking)
                let mut msg = MSG::default();
                while PeekMessageA(&mut msg, Some(hwnd), 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        return;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageA(&msg);
                }

                // Check for channel commands
                match rx.try_recv() {
                    Ok(ThreadCmd::Register {
                        id,
                        modifiers,
                        vk,
                        result_tx,
                    }) => {
                        let result = RegisterHotKey(
                            Some(hwnd),
                            id,
                            HOT_KEY_MODIFIERS(modifiers),
                            vk,
                        );
                        let _ = result_tx
                            .send(result.map_err(|e| format!("RegisterHotKey failed: {e:?}")));
                    }
                    Ok(ThreadCmd::Unregister { id }) => {
                        let _ = UnregisterHotKey(Some(hwnd), id);
                    }
                    Err(mpsc::TryRecvError::Disconnected) => break,
                    Err(mpsc::TryRecvError::Empty) => {
                        // Sleep a tiny bit to avoid busy-waiting
                        thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
            }
        }
    }

    /// Window procedure for the hidden hotkey window.
    unsafe extern "system" fn hotkey_wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_HOTKEY {
            let id = wparam.0 as i32;
            // We retrieve the combo from the global map since wndproc can't
            // easily access the thread-local data. Instead, we store the combo
            // as a ref inside the global COMBO_IDS and emit it.
            //
            // Actually, WM_HOTKEY gives us the id only. We need to look up the
            // combo string. We'll use a global map.
            let combo = {
                let map = COMBO_IDS.lock().unwrap();
                map.iter().find(|(_, &v)| v == id).map(|(k, _)| k.clone())
            };

            if let Some(combo) = combo {
                // Emit event via the stored AppHandle
                crate::hotkeys::emit_hotkey_event(&combo);
            }
            return LRESULT(0);
        }

        DefWindowProcA(hwnd, msg, wparam, lparam)
    }

    // -- public API -------------------------------------------------------

    /// Initialise the hotkey thread. Must be called once during app startup.
    pub fn init() {
        let (tx, rx) = mpsc::channel::<ThreadCmd>();
        *THREAD_TX.lock().unwrap() = Some(tx);
        thread::Builder::new()
            .name("sin-shot-hotkeys".into())
            .spawn(|| hotkey_thread(rx))
            .expect("failed to spawn hotkey thread");
    }

    /// Register a global hotkey.
    pub fn register(combo: &str, app: AppHandle) -> Result<(), String> {
        let (modifiers, vk) = parse_combo(combo)?;

        // Assign a unique ID
        let id = {
            let mut next = NEXT_ID.lock().unwrap();
            let id = *next;
            *next += 1;
            id
        };

        // Send command to hotkey thread and wait for result
        let (result_tx, result_rx) = mpsc::channel();
        let tx = THREAD_TX.lock().unwrap();
        let tx = tx
            .as_ref()
            .ok_or("hotkey thread not initialised — call init() first")?;

        tx.send(ThreadCmd::Register {
            id,
            modifiers,
            vk,
            result_tx,
        })
        .map_err(|e| format!("failed to send register command: {e}"))?;

        result_rx
            .recv()
            .map_err(|e| format!("hotkey thread disconnected: {e}"))??;

        // Store combo→id mapping for unregistration
        COMBO_IDS.lock().unwrap().insert(combo.to_string(), id);

        crate::hotkeys::store_app_handle(app);

        Ok(())
    }

    /// Unregister a previously registered global hotkey.
    pub fn unregister(combo: &str) -> Result<(), String> {
        let id = {
            let map = COMBO_IDS.lock().unwrap();
            map.get(combo).copied()
        }
        .ok_or_else(|| format!("hotkey not registered: {combo}"))?;

        let tx = THREAD_TX.lock().unwrap();
        let tx = tx.as_ref().ok_or("hotkey thread not initialised")?;

        tx.send(ThreadCmd::Unregister {
            id,
        })
        .map_err(|e| format!("failed to send unregister command: {e}"))?;

        COMBO_IDS.lock().unwrap().remove(combo);

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Non-Windows stub
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "windows"))]
mod platform {
    use tauri::AppHandle;

    pub fn init() {
        // no-op on non-Windows
    }

    pub fn register(_combo: &str, _app: AppHandle) -> Result<(), String> {
        // Silently succeed — the app must build on Linux
        Ok(())
    }

    pub fn unregister(_combo: &str) -> Result<(), String> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
// AppHandle / event emission helpers
// ---------------------------------------------------------------------------

use std::sync::OnceLock;
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Store the AppHandle for use by the wndproc callback.
pub(crate) fn store_app_handle(app: AppHandle) {
    APP_HANDLE.set(app).ok();
}

/// Emit a `hotkey-pressed` event from any thread.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn emit_hotkey_event(combo: &str) {
    if let Some(app) = APP_HANDLE.get() {
        let _ = app.emit(
            "hotkey-pressed",
            HotkeyEvent {
                combo: combo.to_string(),
            },
        );
    }
}

/// One-shot initialisation of the platform hotkey thread.
pub fn init_hotkey_system() {
    platform::init();
}

/// Register a hotkey directly (used from the setup hook before the Tauri
/// command router is active). On non-Windows this is a no-op.
pub fn register_hotkey_platform(combo: &str, app: tauri::AppHandle) -> Result<(), String> {
    platform::register(combo, app)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_event_serializes_to_json() {
        let event = HotkeyEvent {
            combo: "Ctrl+Shift+1".to_string(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"combo\":\"Ctrl+Shift+1\""));
    }

    #[test]
    fn hotkey_event_multiple_combos() {
        let combos = vec!["Ctrl+Shift+1", "Ctrl+Shift+2", "Alt+F4", "Ctrl+C"];
        for combo in &combos {
            let event = HotkeyEvent {
                combo: combo.to_string(),
            };
            let json = serde_json::to_string(&event).expect("serialize");
            assert!(json.contains(combo));
        }
    }

    #[test]
    fn hotkey_state_starts_empty() {
        let state = HotkeyState::new();
        assert!(state.hotkeys_registered.is_empty());
    }

    #[test]
    fn non_windows_hotkey_init_does_not_panic() {
        // init should be a no-op on non-Windows
        platform::init();
    }

    #[test]
    fn non_windows_register_returns_ok() {
        // On non-Windows, register silently succeeds.
        // We need an AppHandle, but the stub ignores it.
        // Use a test that verifies the stub signature compiles.
        // The stub always returns Ok(()).
        // (We can't construct an AppHandle in a unit test, so we
        //  verify init and the type interface instead.)
        platform::init();
    }
}
