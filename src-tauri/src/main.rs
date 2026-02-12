#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod commands;
mod error;
mod state;

use commands::ipc::{ipc_emit, ipc_emit_global, ipc_invoke};
use state::AppState;
use tauri::{Emitter, Manager, WindowEvent};

fn normalize_path(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

fn cli_arg_value(prefix: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(arg) = args.next() {
        if let Some(rest) = arg.strip_prefix(prefix) {
            return Some(rest.to_string());
        }
        if arg == prefix.trim_end_matches('=') {
            if let Some(next) = args.next() {
                return Some(next);
            }
        }
    }
    None
}

fn read_app_data_override(config_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let store_path = config_dir.join("store.json");
    let content = std::fs::read_to_string(store_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    let s = value.get("appDataPath")?.as_str()?.trim();
    if s.is_empty() {
        return None;
    }
    Some(normalize_path(s))
}

fn read_allowed_dirs(config_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let store_path = config_dir.join("store.json");
    let content = match std::fs::read_to_string(store_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let Some(arr) = value.get("allowedDirs").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|v| v.as_str())
        .map(|s| normalize_path(s))
        .collect()
}

fn read_theme(config_dir: &std::path::Path) -> Option<Option<tauri::Theme>> {
    let store_path = config_dir.join("store.json");
    let content = std::fs::read_to_string(store_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    let theme = value.get("theme")?.as_str()?.trim();
    match theme {
        "dark" => Some(Some(tauri::Theme::Dark)),
        "light" => Some(Some(tauri::Theme::Light)),
        "system" => Some(None),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn read_use_system_title_bar(config_dir: &std::path::Path) -> bool {
    let store_path = config_dir.join("store.json");
    let content = match std::fs::read_to_string(store_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };
    value
        .get("useSystemTitleBar")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let default_app_data_dir = app
                .path()
                .app_data_dir()
                .or_else(|_| app.path().app_local_data_dir())
                .or_else(|_| app.path().app_config_dir())
                .expect("failed to resolve app data dir");

            let app_config_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| default_app_data_dir.join("Config"));

            // Highest priority: CLI override (used by existing renderer migration flow).
            let app_data_dir = if let Some(path) = cli_arg_value("--user-data-dir=") {
                normalize_path(&path)
            } else if let Some(path) = read_app_data_override(&app_config_dir) {
                path
            } else {
                default_app_data_dir
            };

            let allowed_dirs = read_allowed_dirs(&app_config_dir);
            let saved_theme = read_theme(&app_config_dir);

            #[cfg(target_os = "linux")]
            let use_system_title_bar = read_use_system_title_bar(&app_config_dir);

            app.manage(AppState {
                app_data_dir,
                app_config_dir,
                allowed_dirs: std::sync::Mutex::new(allowed_dirs),
                stop_quit: std::sync::Mutex::new(Default::default()),
                zoom_factor: std::sync::Mutex::new(1.0),
            });

            let main = app.get_webview_window("main").expect("missing main window");

            #[cfg(target_os = "windows")]
            {
                let _ = main.set_decorations(false);
            }

            #[cfg(target_os = "linux")]
            {
                let _ = main.set_decorations(use_system_title_bar);
            }

            if let Some(theme) = saved_theme {
                let _ = main.set_theme(theme);
            }

            let main_for_events = main.clone();
            let app_handle = app.handle().clone();

            // Emit save-data on close and allow a short flush window.
            // Also forward resize events for the existing renderer hooks.
            main.on_window_event(move |event| match event {
                WindowEvent::CloseRequested { api, .. } => {
                    let stop_quit = app_handle
                        .state::<AppState>()
                        .stop_quit
                        .lock()
                        .map(|s| s.enabled)
                        .unwrap_or(false);
                    if stop_quit {
                        // Renderer intentionally blocks quitting during critical operations (migration/copy).
                        api.prevent_close();
                        return;
                    }
                    api.prevent_close();
                    let _ = main_for_events.emit("app:save-data", serde_json::json!({}));
                    let app_for_exit = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        app_for_exit.exit(0);
                    });
                }
                WindowEvent::Resized(size) => {
                    let _ = main_for_events.emit("window:resize", vec![size.width, size.height]);
                }
                WindowEvent::ThemeChanged(theme) => {
                    let actual_theme = if theme == &tauri::Theme::Dark {
                        "dark"
                    } else {
                        "light"
                    };
                    let _ = app_handle.emit("theme:updated", actual_theme);
                }
                _ => {}
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc_invoke,
            ipc_emit,
            ipc_emit_global
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
