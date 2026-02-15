use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tauri_plugin_store::StoreExt;

mod audio;
mod soniox;
mod typer;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::task::JoinHandle;

const STORE_PATH: &str = "settings.json";

pub struct AppState {
    pub is_recording: Arc<AtomicBool>,
    pub stop_signal: Arc<AtomicBool>,
    pub settings: Mutex<AppSettings>,
    pub recording_task: Mutex<Option<JoinHandle<()>>>,
    pub target_window_id: Mutex<Option<String>>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AppSettings {
    pub api_key: String,
    pub hotkey: String,
    pub language_hints: Vec<String>,
    pub language_restrictions: Option<Vec<String>>,
}

const TRANSCRIPTIONS_STORE_PATH: &str = "transcriptions.json";

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TranscriptionEntry {
    pub text: String,
    pub timestamp: u64,
    pub language: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            hotkey: "Insert".to_string(),
            language_hints: vec!["en".to_string()],
            language_restrictions: None,
        }
    }
}

fn load_settings_from_store(app: &AppHandle) -> AppSettings {
    if let Ok(store) = app.store(STORE_PATH) {
        if let Some(settings_json) = store.get("settings") {
            if let Ok(settings) = serde_json::from_value::<AppSettings>(settings_json) {
                return settings;
            }
        }
    }
    AppSettings::default()
}

fn save_settings_to_store(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let store = app.store(STORE_PATH).map_err(|e| e.to_string())?;
    let settings_json = serde_json::to_value(settings).map_err(|e| e.to_string())?;
    store.set("settings", settings_json);
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> AppSettings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
async fn save_settings(
    app: AppHandle,
    state: tauri::State<'_, AppState>, 
    settings: AppSettings
) -> Result<(), String> {
    let old_hotkey = {
        let s = state.settings.lock().unwrap();
        s.hotkey.clone()
    };
    
    {
        let mut s = state.settings.lock().unwrap();
        *s = settings.clone();
    }
    
    save_settings_to_store(&app, &settings)?;
    
    // Re-register hotkey if it changed
    if old_hotkey != settings.hotkey {
        log::info!("Hotkey changed from '{}' to '{}', re-registering...", old_hotkey, settings.hotkey);
        let gs = app.global_shortcut();
        gs.unregister_all().ok();
        let new_shortcut: tauri_plugin_global_shortcut::Shortcut = settings.hotkey.parse()
            .map_err(|e| format!("Invalid hotkey '{}': {:?}", settings.hotkey, e))?;
        // Must use on_shortcut (not register) so the callback is attached
        gs.on_shortcut(new_shortcut, move |app_handle, _shortcut, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                let handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    let state: tauri::State<'_, AppState> = handle.state();
                    if let Err(e) = toggle_recording(handle.clone(), state).await {
                        log::error!("Hotkey toggle failed: {}", e);
                    }
                });
            }
        }).map_err(|e| format!("Failed to register hotkey: {}", e))?;
        log::info!("Re-registered hotkey '{}' with handler", settings.hotkey);
    }
    
    Ok(())
}

#[tauri::command]
fn get_recording_state(state: tauri::State<AppState>) -> bool {
    state.is_recording.load(Ordering::SeqCst)
}

#[tauri::command]
fn get_transcriptions(app: AppHandle) -> Vec<TranscriptionEntry> {
    if let Ok(store) = app.store(TRANSCRIPTIONS_STORE_PATH) {
        if let Some(entries_json) = store.get("entries") {
            if let Ok(entries) = serde_json::from_value::<Vec<TranscriptionEntry>>(entries_json) {
                return entries;
            }
        }
    }
    Vec::new()
}

#[tauri::command]
fn save_transcription(app: AppHandle, text: String, language_hints: Vec<String>) -> Result<(), String> {
    let store = app.store(TRANSCRIPTIONS_STORE_PATH).map_err(|e| e.to_string())?;
    let mut entries: Vec<TranscriptionEntry> = store
        .get("entries")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    let entry = TranscriptionEntry {
        text,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        language: language_hints.join(","),
    };
    entries.insert(0, entry);
    // Keep last 100 entries
    entries.truncate(100);

    let json = serde_json::to_value(&entries).map_err(|e| e.to_string())?;
    store.set("entries", json);
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn clear_transcriptions(app: AppHandle) -> Result<(), String> {
    let store = app.store(TRANSCRIPTIONS_STORE_PATH).map_err(|e| e.to_string())?;
    let empty: Vec<TranscriptionEntry> = Vec::new();
    let json = serde_json::to_value(&empty).map_err(|e| e.to_string())?;
    store.set("entries", json);
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

fn show_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        // Calculate centered horizontal position
        let overlay_width = 200.0_f64;
        let top_margin = 40.0_f64;
        
        let x = if let Ok(Some(monitor)) = window.current_monitor() {
            let screen_width = monitor.size().width as f64 / monitor.scale_factor();
            (screen_width - overlay_width) / 2.0
        } else {
            // Fallback: assume 1920px screen
            (1920.0 - overlay_width) / 2.0
        };
        
        // Set transparent background color to prevent black flash on Linux
        let _ = window.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)));
        
        let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(x, top_margin)));
        
        // Small delay to let webview render before showing (prevents black border flash on Linux)
        #[cfg(target_os = "linux")]
        std::thread::sleep(std::time::Duration::from_millis(50));
        
        let _ = window.show();
        let _ = window.set_always_on_top(true);
    }
}

fn hide_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
    }
}

async fn toggle_recording(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if state.is_recording.load(Ordering::SeqCst) {
        // Stop recording
        log::info!("Hotkey: stopping recording");
        state.stop_signal.store(true, Ordering::SeqCst);
        state.is_recording.store(false, Ordering::SeqCst);
        hide_overlay(&app);
        app.emit("recording-stopped", ()).ok();
    } else {
        // Start recording
        log::info!("Hotkey: starting recording");
        
        let settings = state.settings.lock().unwrap().clone();
        
        if settings.api_key.is_empty() {
            log::error!("API key is empty");
            app.emit("recording-error", "API key not configured. Please set your Soniox API key in settings.").ok();
            return Err("API key not configured".to_string());
        }
        
        // CRITICAL: Capture the target window FIRST - before any UI changes
        let target_window_id = match typer::capture_focused_window() {
            Ok(id) => id,
            Err(e) => {
                log::error!("Failed to capture target window: {}", e);
                app.emit("recording-error", format!("Failed to capture target window: {}", e)).ok();
                return Err(e);
            }
        };
        
        {
            let mut tw = state.target_window_id.lock().unwrap();
            *tw = Some(target_window_id.clone());
        }
        eprintln!("DEBUG: Target window captured via hotkey: {}", target_window_id);
        
        // Reset stop signal
        state.stop_signal.store(false, Ordering::SeqCst);
        state.is_recording.store(true, Ordering::SeqCst);
        
        // Show overlay AFTER capturing the target window
        show_overlay(&app);
        
        app.emit("recording-started", ()).ok();
        
        let stop_signal = state.stop_signal.clone();
        let is_recording = state.is_recording.clone();
        let api_key = settings.api_key.clone();
        let language_hints = settings.language_hints.clone();
        let language_restrictions = settings.language_restrictions.clone();
        let app_clone = app.clone();
        
        // Spawn recording in a separate task
        let handle = tokio::spawn(async move {
            log::info!("Starting audio capture in background task...");
            
            match audio::start_audio_capture(api_key, language_hints, language_restrictions, stop_signal.clone(), app_clone.clone(), target_window_id).await {
                Ok(_) => log::info!("Audio capture completed successfully"),
                Err(e) => {
                    log::error!("Audio capture failed: {}", e);
                    app_clone.emit("recording-error", e).ok();
                }
            }
            
            is_recording.store(false, Ordering::SeqCst);
            hide_overlay(&app_clone);
            app_clone.emit("recording-stopped", ()).ok();
        });
        
        // Store the handle
        {
            let mut task = state.recording_task.lock().unwrap();
            *task = Some(handle);
        }
    }
    
    Ok(())
}

#[tauri::command]
async fn start_recording(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    log::info!("start_recording called from button");
    toggle_recording(app, state).await
}

#[tauri::command]
fn stop_recording(
    app: AppHandle,
    state: tauri::State<AppState>
) -> Result<(), String> {
    log::info!("stop_recording called");
    state.stop_signal.store(true, Ordering::SeqCst);
    state.is_recording.store(false, Ordering::SeqCst);
    hide_overlay(&app);
    app.emit("recording-stopped", ()).ok();
    Ok(())
}

pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            let settings = load_settings_from_store(&app.handle());
            let hotkey_str = settings.hotkey.clone();
            
            let app_state = AppState {
                is_recording: Arc::new(AtomicBool::new(false)),
                stop_signal: Arc::new(AtomicBool::new(false)),
                settings: Mutex::new(settings),
                recording_task: Mutex::new(None),
                target_window_id: Mutex::new(None),
            };
            
            app.manage(app_state);
            
            // Hide overlay window initially
            if let Some(window) = app.get_webview_window("overlay") {
                let _ = window.hide();
            }

            // Close to tray: intercept close on main window and hide instead
            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        window_clone.hide().ok();
                    }
                });
            }
            
            // Register global hotkey in Rust (no JS/webview focus change)
            let gs = app.global_shortcut();
            
            let hotkey_shortcut: tauri_plugin_global_shortcut::Shortcut = hotkey_str.parse()
                .map_err(|e| {
                    log::error!("Invalid hotkey '{}': {:?}", hotkey_str, e);
                    format!("Invalid hotkey configured: {}", e)
                })?;
            
            gs.on_shortcut(hotkey_shortcut, |app, _shortcut, event| {
                if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, AppState> = app_handle.state();
                        if let Err(e) = toggle_recording(app_handle.clone(), state).await {
                            log::error!("Hotkey toggle failed: {}", e);
                        }
                    });
                }
            }).map_err(|e| {
                log::error!("Failed to register global hotkey: {}", e);
                format!("Failed to register hotkey: {}", e)
            })?;
            
            log::info!("Global hotkey '{}' registered in Rust backend", hotkey_str);
            
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Desktop Dictate - Click to configure")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            window.show().ok();
                            window.set_focus().ok();
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            window.show().ok();
                            window.set_focus().ok();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            get_recording_state,
            start_recording,
            stop_recording,
            get_transcriptions,
            save_transcription,
            clear_transcriptions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
