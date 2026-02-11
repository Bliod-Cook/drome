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

      app.manage(AppState {
        app_data_dir,
        app_config_dir,
        allowed_dirs: std::sync::Mutex::new(Vec::new()),
        stop_quit: std::sync::Mutex::new(Default::default()),
      });

      let main = app.get_webview_window("main").expect("missing main window");
      let main_for_events = main.clone();
      let app_handle = app.handle().clone();

      // Emit save-data on close and allow a short flush window.
      // Also forward resize events for the existing renderer hooks.
      main.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
          api.prevent_close();
          let stop_quit = app_handle
            .state::<AppState>()
            .stop_quit
            .lock()
            .map(|s| s.enabled)
            .unwrap_or(false);
          if stop_quit {
            // Renderer intentionally blocks quitting during critical operations (migration/copy).
            return;
          }
          let _ = main_for_events.emit("app:save-data", serde_json::json!({}));
          let win = main_for_events.clone();
          tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let _ = win.close();
          });
        }
        WindowEvent::Resized(size) => {
          let _ = main_for_events.emit("window:resize", vec![size.width, size.height]);
        }
        _ => {}
      });

      Ok(())
    })
    .invoke_handler(tauri::generate_handler![ipc_invoke, ipc_emit, ipc_emit_global])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
