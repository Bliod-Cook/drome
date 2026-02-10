use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_shell::open::open;

use crate::error::{DromeError, Result};
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
  pub version: String,
  pub is_packaged: bool,
  pub app_path: String,
  pub files_path: String,
  pub notes_path: String,
  pub config_path: String,
  pub app_data_path: String,
  pub resources_path: String,
  pub logs_path: String,
  pub arch: String,
  pub is_portable: bool,
  pub install_path: String,
  pub platform: String,
}

fn ensure_dir(path: &PathBuf) -> Result<()> {
  std::fs::create_dir_all(path)?;
  Ok(())
}

pub fn app_info(app: &AppHandle, state: &State<'_, AppState>) -> Result<AppInfo> {
  let version = app.package_info().version.to_string();
  let is_packaged = !cfg!(debug_assertions);

  let app_data_path = state.app_data_dir.clone();
  let data_dir = app_data_path.join("Data");
  let files_dir = data_dir.join("Files");
  let notes_dir = data_dir.join("Notes");

  ensure_dir(&files_dir)?;
  ensure_dir(&notes_dir)?;

  let config_dir = state.app_config_dir.clone();
  ensure_dir(&config_dir)?;

  let resources_dir = app.path().resource_dir().unwrap_or_else(|_| app_data_path.clone());
  let logs_dir = app.path().app_log_dir().unwrap_or_else(|_| app_data_path.join("Logs"));
  ensure_dir(&logs_dir)?;

  let install_path = std::env::current_exe()
    .ok()
    .and_then(|p| p.parent().map(|pp| pp.to_path_buf()))
    .unwrap_or_else(|| resources_dir.clone());

  let app_path = install_path.clone();

  Ok(AppInfo {
    version,
    is_packaged,
    app_path: app_path.to_string_lossy().to_string(),
    files_path: files_dir.to_string_lossy().to_string(),
    notes_path: notes_dir.to_string_lossy().to_string(),
    config_path: config_dir.to_string_lossy().to_string(),
    app_data_path: app_data_path.to_string_lossy().to_string(),
    resources_path: resources_dir.to_string_lossy().to_string(),
    logs_path: logs_dir.to_string_lossy().to_string(),
    arch: std::env::consts::ARCH.to_string(),
    is_portable: false,
    install_path: install_path.to_string_lossy().to_string(),
    platform: std::env::consts::OS.to_string(),
  })
}

pub fn app_reload(window: &WebviewWindow) -> Result<()> {
  window.eval("window.location.reload()").map_err(|e| DromeError::Message(e.to_string()))?;
  Ok(())
}

pub fn app_quit(app: &AppHandle) -> Result<()> {
  app.exit(0);
  Ok(())
}

pub fn open_website(_app: &AppHandle, url: String) -> Result<()> {
  open(None, url, None).map_err(|e| DromeError::Message(e.to_string()))?;
  Ok(())
}

pub fn open_path(_app: &AppHandle, path: String) -> Result<()> {
  open(None, path, None).map_err(|e| DromeError::Message(e.to_string()))?;
  Ok(())
}

pub fn app_log_to_main(_args: Vec<Value>) -> Result<()> {
  // Best-effort: renderer already logs to console; keep this as a hook for future structured logging.
  Ok(())
}

pub fn app_get_disk_info(directory_path: String) -> Result<Option<serde_json::Value>> {
  let path = PathBuf::from(directory_path);
  if !path.exists() {
    return Ok(None);
  }

  #[cfg(unix)]
  {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(&path)?;
    let size = meta.size();
    // Free space is non-trivial cross-platform without extra deps; return null for free for now.
    return Ok(Some(serde_json::json!({ "size": size, "free": 0 })));
  }

  #[cfg(not(unix))]
  {
    let meta = std::fs::metadata(&path)?;
    return Ok(Some(serde_json::json!({ "size": meta.len(), "free": 0 })));
  }
}

pub fn app_get_data_path_from_args() -> Result<Option<String>> {
  // TODO: parse CLI args for a future data-dir override.
  Ok(None)
}
