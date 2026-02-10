mod commands;
mod error;
mod state;

use commands::ipc::{ipc_emit, ipc_emit_global, ipc_invoke};
use state::AppState;
use tauri::{Emitter, Manager, WindowEvent};

fn main() {
  tauri::Builder::default()
    .plugin(tauri_plugin_shell::init())
    .plugin(tauri_plugin_dialog::init())
    .setup(|app| {
      let app_data_dir = app
        .path()
        .app_data_dir()
        .or_else(|_| app.path().app_local_data_dir())
        .or_else(|_| app.path().app_config_dir())
        .expect("failed to resolve app data dir");

      let app_config_dir = app.path().app_config_dir().unwrap_or_else(|_| app_data_dir.join("Config"));

      app.manage(AppState {
        app_data_dir,
        app_config_dir,
        allowed_dirs: std::sync::Mutex::new(Vec::new()),
      });

      let main = app.get_webview_window("main").expect("missing main window");
      let main_for_events = main.clone();

      // Emit save-data on close and allow a short flush window.
      // Also forward resize events for the existing renderer hooks.
      main.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
          api.prevent_close();
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
