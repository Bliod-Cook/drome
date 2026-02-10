use serde_json::Value;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::commands;
use crate::error::{DromeError, Result};
use crate::state::AppState;

fn arg<T: serde::de::DeserializeOwned>(args: &[Value], idx: usize) -> Result<T> {
  let value = args
    .get(idx)
    .ok_or_else(|| DromeError::Message(format!("Missing arg at index {idx}")))?
    .clone();
  serde_json::from_value(value).map_err(|e| DromeError::Message(format!("Invalid arg at index {idx}: {e}")))
}

fn to_value<T: serde::Serialize>(value: T) -> Result<Value> {
  Ok(serde_json::to_value(value)?)
}

#[tauri::command]
pub async fn ipc_invoke(
  app: AppHandle,
  window: WebviewWindow,
  state: State<'_, AppState>,
  channel: String,
  args: Vec<Value>,
) -> std::result::Result<Value, String> {
  (|| -> Result<Value> {
    match channel.as_str() {
      // App
      "app:info" => to_value(commands::app::app_info(&app, &state)?),
      "app:reload" => to_value(commands::app::app_reload(&window)?),
      "app:quit" => to_value(commands::app::app_quit(&app)?),
      "open:website" => to_value(commands::app::open_website(&app, arg::<String>(&args, 0)?)?),
      "open:path" => to_value(commands::app::open_path(&app, arg::<String>(&args, 0)?)?),
      "app:log-to-main" => to_value(commands::app::app_log_to_main(args)?),
      "app:get-disk-info" => to_value(commands::app::app_get_disk_info(arg::<String>(&args, 0)?)?),
      "app:get-data-path-from-args" => to_value(commands::app::app_get_data_path_from_args()?),
      "redux-store-ready" => Ok(Value::Null),

      // Window controls
      "window:minimize" => to_value(commands::window::window_minimize(&window)?),
      "window:maximize" => to_value(commands::window::window_maximize(&window)?),
      "window:unmaximize" => to_value(commands::window::window_unmaximize(&window)?),
      "window:close" => to_value(commands::window::window_close(&window)?),
      "window:is-maximized" => to_value(commands::window::window_is_maximized(&window)?),
      "window:set-minimum-size" => to_value(commands::window::window_set_minimum_size(
        &window,
        arg::<f64>(&args, 0)?,
        arg::<f64>(&args, 1)?,
      )?),
      "window:reset-minimum-size" => to_value(commands::window::window_reset_minimum_size(&window)?),
      "window:get-size" => to_value(commands::window::window_get_size(&window)?),

      // Config
      "config:get" => to_value(commands::config::config_get(&state, arg::<String>(&args, 0)?)?),
      "config:set" => to_value(commands::config::config_set(
        &state,
        &app,
        arg::<String>(&args, 0)?,
        args.get(1).cloned().unwrap_or(Value::Null),
        args.get(2).and_then(|v| v.as_bool()).unwrap_or(false),
      )?),

      // File dialogs and fs
      "file:open" => to_value(commands::file::file_open(&app, &state, args.get(0).cloned())?),
      "file:openPath" => to_value(commands::file::file_open_path(&app, arg::<String>(&args, 0)?)?),
      "file:selectFolder" => to_value(commands::file::file_select_folder(&app, &state, args.get(0).cloned())?),
      "file:save" => to_value(commands::file::file_save(&app, &state, args)?),
      "file:read" => to_value(commands::file::file_read(
        &state,
        arg::<String>(&args, 0)?,
        args.get(1).and_then(|v| v.as_bool()).unwrap_or(false),
      )?),
      "file:write" => to_value(commands::file::file_write(
        &state,
        arg::<String>(&args, 0)?,
        arg::<Value>(&args, 1)?,
      )?),
      "file:mkdir" => to_value(commands::file::file_mkdir(&state, arg::<String>(&args, 0)?)?),
      "file:delete" => to_value(commands::file::file_delete(&state, arg::<String>(&args, 0)?)?),
      "file:deleteDir" => to_value(commands::file::file_delete_dir(&state, arg::<String>(&args, 0)?)?),
      "file:isDirectory" => to_value(commands::file::file_is_directory(&state, arg::<String>(&args, 0)?)?),
      "file:listDirectory" => to_value(commands::file::file_list_directory(
        &state,
        arg::<String>(&args, 0)?,
        args.get(1).cloned(),
      )?),
      "file:showInFolder" => to_value(commands::file::file_show_in_folder(&app, arg::<String>(&args, 0)?)?),

      // FS (read-only convenience)
      "fs:read" => to_value(commands::fs::fs_read(
        &app,
        &state,
        arg::<String>(&args, 0)?,
        args.get(1).and_then(|v| v.as_str()).map(|s| s.to_string()),
      )?),
      "fs:readText" => to_value(commands::fs::fs_read_text(&app, &state, arg::<String>(&args, 0)?)?),
      "app:resolve-path" => to_value(commands::file::resolve_path(arg::<String>(&args, 0)?)?),
      "app:is-path-inside" => to_value(commands::file::is_path_inside(
        arg::<String>(&args, 0)?,
        arg::<String>(&args, 1)?,
      )?),
      "app:has-write-permission" => to_value(commands::file::has_write_permission(arg::<String>(&args, 0)?)?),

      // Zip (gzip)
      "zip:compress" => to_value(commands::zip::zip_compress(arg::<String>(&args, 0)?)?),
      "zip:decompress" => to_value(commands::zip::zip_decompress(arg::<Vec<u8>>(&args, 0)?)?),

      // Backup (zip)
      "backup:backup" => to_value(commands::backup::backup_backup(&app, &window, &state, args)?),
      "backup:restore" => to_value(commands::backup::backup_restore(
        &app,
        &window,
        &state,
        arg::<String>(&args, 0)?,
      )?),

      // Migration
      "drome:migration-detect" => to_value(commands::migration::migration_detect(&state)?),
      "drome:migration-copy-data" => to_value(commands::migration::migration_copy_data_dir(
        &app,
        &window,
        &state,
        arg::<String>(&args, 0)?,
      )?),

      _ => Err(DromeError::Message(format!("Not implemented IPC channel: {channel}"))),
    }
  })()
  .map_err(String::from)
}

#[tauri::command]
pub async fn ipc_emit(window: WebviewWindow, channel: String, payload: Value) -> std::result::Result<(), String> {
  window.emit(&channel, payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ipc_emit_global(app: AppHandle, channel: String, payload: Value) -> std::result::Result<(), String> {
  app.emit(&channel, payload).map_err(|e| e.to_string())
}
