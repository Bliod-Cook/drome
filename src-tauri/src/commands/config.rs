use serde_json::Value;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

use crate::error::Result;
use crate::state::AppState;

fn store_path(state: &State<'_, AppState>) -> PathBuf {
  state.app_config_dir.join("store.json")
}

fn read_store(path: &PathBuf) -> Result<serde_json::Map<String, Value>> {
  if !path.exists() {
    return Ok(serde_json::Map::new());
  }
  let content = std::fs::read_to_string(path)?;
  let value: Value = serde_json::from_str(&content)?;
  Ok(value.as_object().cloned().unwrap_or_default())
}

fn write_store(path: &PathBuf, map: &serde_json::Map<String, Value>) -> Result<()> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let content = serde_json::to_string_pretty(map)?;
  std::fs::write(path, content)?;
  Ok(())
}

pub fn config_get(state: &State<'_, AppState>, key: String) -> Result<Value> {
  let path = store_path(state);
  let map = read_store(&path)?;
  Ok(map.get(&key).cloned().unwrap_or(Value::Null))
}

pub fn config_set(
  state: &State<'_, AppState>,
  app: &AppHandle,
  key: String,
  value: Value,
  notify: bool,
) -> Result<()> {
  let path = store_path(state);
  let mut map = read_store(&path)?;
  map.insert(key.clone(), value.clone());
  write_store(&path, &map)?;

  if notify {
    // Best-effort broadcast; renderer can subscribe later if needed.
    let _ = app.emit("config:updated", serde_json::json!({ "key": key, "value": value }));
  }
  Ok(())
}
