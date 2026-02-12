use serde_json::Value;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

use crate::error::{DromeError, Result};
use crate::state::AppState;

fn strip_file_scheme(input: &str) -> &str {
    input.strip_prefix("file://").unwrap_or(input)
}

fn normalize_path(path: &str) -> PathBuf {
    let path = strip_file_scheme(path);
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn is_allowed(app: &AppHandle, state: &State<'_, AppState>, path: &Path) -> bool {
    if path.starts_with(&state.app_data_dir) {
        return true;
    }
    if path.starts_with(&state.app_config_dir) {
        return true;
    }
    if let Ok(res_dir) = app.path().resource_dir() {
        if path.starts_with(&res_dir) {
            return true;
        }
    }

    let dirs = state.allowed_dirs.lock().ok();
    if let Some(dirs) = dirs {
        for allowed in dirs.iter() {
            if path.starts_with(allowed) {
                return true;
            }
        }
    }
    false
}

pub fn fs_read(
    app: &AppHandle,
    state: &State<'_, AppState>,
    path_or_url: String,
    encoding: Option<String>,
) -> Result<Value> {
    if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
        return Err(DromeError::Message(
            "fs.read for http(s) URLs is not implemented".into(),
        ));
    }

    let path = normalize_path(&path_or_url);
    if !is_allowed(app, state, &path) {
        return Err(DromeError::Message("Path not allowed".into()));
    }

    if encoding.is_some() {
        let content = std::fs::read_to_string(path)?;
        return Ok(Value::String(content));
    }

    let bytes = std::fs::read(path)?;
    Ok(serde_json::to_value(bytes)?)
}

pub fn fs_read_text(
    app: &AppHandle,
    state: &State<'_, AppState>,
    path_or_url: String,
) -> Result<String> {
    if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
        return Err(DromeError::Message(
            "fs.readText for http(s) URLs is not implemented".into(),
        ));
    }

    let path = normalize_path(&path_or_url);
    if !is_allowed(app, state, &path) {
        return Err(DromeError::Message("Path not allowed".into()));
    }

    Ok(std::fs::read_to_string(path)?)
}
