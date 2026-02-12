use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tauri::{State, WebviewWindow};

use crate::error::Result;
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBashPathInfo {
  pub path: Option<String>,
  pub source: Option<String>,
}

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

pub fn system_get_device_type() -> Result<String> {
  if cfg!(target_os = "windows") {
    Ok("windows".to_string())
  } else if cfg!(target_os = "macos") {
    Ok("mac".to_string())
  } else {
    Ok("linux".to_string())
  }
}

pub fn system_get_hostname() -> Result<String> {
  let host = gethostname::gethostname().to_string_lossy().to_string();
  Ok(host)
}

pub fn system_get_cpu_name() -> Result<String> {
  // Best-effort cross-platform CPU brand/model.
  let mut sys = sysinfo::System::new();
  sys.refresh_cpu_all();
  let name = sys
    .cpus()
    .first()
    .map(|cpu| cpu.brand().to_string())
    .filter(|s| !s.trim().is_empty())
    .unwrap_or_else(|| "Unknown CPU".to_string());
  Ok(name)
}

fn validate_git_bash_path(path: &str) -> Option<String> {
  if path.trim().is_empty() {
    return None;
  }
  let resolved = PathBuf::from(path);
  if !resolved.exists() {
    return None;
  }
  let s = resolved.to_string_lossy().to_string();
  if !s.to_lowercase().ends_with("bash.exe") {
    return None;
  }
  Some(s)
}

#[cfg(target_os = "windows")]
fn common_git_bash_candidates() -> Vec<PathBuf> {
  let mut out = Vec::new();

  // Program Files
  if let Ok(pf) = std::env::var("ProgramFiles") {
    let pf = PathBuf::from(pf);
    out.push(pf.join("Git").join("bin").join("bash.exe"));
    out.push(pf.join("Git").join("usr").join("bin").join("bash.exe"));
  }
  if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
    let pf86 = PathBuf::from(pf86);
    out.push(pf86.join("Git").join("bin").join("bash.exe"));
    out.push(pf86.join("Git").join("usr").join("bin").join("bash.exe"));
  }

  // LocalAppData
  if let Ok(local) = std::env::var("LocalAppData") {
    let local = PathBuf::from(local);
    out.push(local.join("Programs").join("Git").join("bin").join("bash.exe"));
    out.push(
      local
        .join("Programs")
        .join("Git")
        .join("usr")
        .join("bin")
        .join("bash.exe"),
    );
  }

  // Fallback common
  out.push(PathBuf::from(r"C:\Program Files\Git\bin\bash.exe"));
  out.push(PathBuf::from(r"C:\Program Files\Git\usr\bin\bash.exe"));
  out.push(PathBuf::from(r"C:\Program Files (x86)\Git\bin\bash.exe"));
  out.push(PathBuf::from(r"C:\Program Files (x86)\Git\usr\bin\bash.exe"));

  out
}

#[cfg(not(target_os = "windows"))]
fn common_git_bash_candidates() -> Vec<PathBuf> {
  Vec::new()
}

fn find_git_bash() -> Option<String> {
  #[cfg(target_os = "windows")]
  {
    for candidate in common_git_bash_candidates() {
      if candidate.exists() {
        return validate_git_bash_path(&candidate.to_string_lossy());
      }
    }
  }

  None
}

fn auto_discover_git_bash(state: &State<'_, AppState>) -> Result<Option<String>> {
  if !cfg!(target_os = "windows") {
    return Ok(None);
  }

  // 1) Env override
  if let Ok(env_override) = std::env::var("CLAUDE_CODE_GIT_BASH_PATH") {
    if let Some(validated) = validate_git_bash_path(&env_override) {
      return Ok(Some(validated));
    }
  }

  // 2) Store configured path
  let path = store_path(state);
  let map = read_store(&path)?;
  if let Some(Value::String(existing)) = map.get("gitBashPath") {
    if let Some(validated) = validate_git_bash_path(existing) {
      return Ok(Some(validated));
    }
  }

  // 3) Auto-discovery
  let discovered = find_git_bash();
  if let Some(ref p) = discovered {
    let mut map = map.clone();
    map.insert("gitBashPath".into(), Value::String(p.clone()));
    map.insert("gitBashPathSource".into(), Value::String("auto".into()));
    write_store(&path, &map)?;
  }
  Ok(discovered)
}

pub fn system_check_git_bash(state: &State<'_, AppState>) -> Result<bool> {
  if !cfg!(target_os = "windows") {
    return Ok(true);
  }
  Ok(auto_discover_git_bash(state)?.is_some())
}

pub fn system_get_git_bash_path(state: &State<'_, AppState>) -> Result<Option<String>> {
  if !cfg!(target_os = "windows") {
    return Ok(None);
  }
  let path = store_path(state);
  let map = read_store(&path)?;
  Ok(map.get("gitBashPath").and_then(|v| v.as_str()).map(|s| s.to_string()))
}

pub fn system_get_git_bash_path_info(state: &State<'_, AppState>) -> Result<GitBashPathInfo> {
  if !cfg!(target_os = "windows") {
    return Ok(GitBashPathInfo { path: None, source: None });
  }

  let path = store_path(state);
  let map = read_store(&path)?;

  let mut path_value = map.get("gitBashPath").and_then(|v| v.as_str()).map(|s| s.to_string());
  let mut source_value = map
    .get("gitBashPathSource")
    .and_then(|v| v.as_str())
    .map(|s| s.to_string());

  if path_value.as_deref().unwrap_or("").is_empty() {
    path_value = auto_discover_git_bash(state)?;
    source_value = if path_value.is_some() { Some("auto".into()) } else { None };
  }

  Ok(GitBashPathInfo { path: path_value, source: source_value })
}

pub fn system_set_git_bash_path(state: &State<'_, AppState>, new_path: Option<String>) -> Result<bool> {
  if !cfg!(target_os = "windows") {
    return Ok(false);
  }

  let store = store_path(state);
  let mut map = read_store(&store)?;

  match new_path {
    None => {
      map.insert("gitBashPath".into(), Value::Null);
      map.insert("gitBashPathSource".into(), Value::Null);
      write_store(&store, &map)?;
      // Re-run auto discovery to restore an auto path if possible.
      let _ = auto_discover_git_bash(state)?;
      Ok(true)
    }
    Some(p) => {
      let Some(validated) = validate_git_bash_path(&p) else {
        return Ok(false);
      };
      map.insert("gitBashPath".into(), Value::String(validated));
      map.insert("gitBashPathSource".into(), Value::String("manual".into()));
      write_store(&store, &map)?;
      Ok(true)
    }
  }
}

pub fn system_toggle_devtools(window: &WebviewWindow) -> Result<()> {
  // Devtools APIs are only available on debug builds in Tauri by default.
  #[cfg(debug_assertions)]
  {
    let is_open = window.is_devtools_open();
    if is_open {
      window.close_devtools();
    } else {
      window.open_devtools();
    }
  }

  #[cfg(not(debug_assertions))]
  {
    let _ = window;
  }
  Ok(())
}

pub fn add_allowed_dir_to_store(state: &State<'_, AppState>, dir: &Path) -> Result<()> {
  let store = store_path(state);
  let mut map = read_store(&store)?;

  let mut dirs: Vec<String> = map
    .get("allowedDirs")
    .and_then(|v| v.as_array())
    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
    .unwrap_or_default();

  let dir_str = dir.to_string_lossy().to_string();
  if !dirs.iter().any(|d| d == &dir_str) {
    dirs.push(dir_str);
  }

  map.insert(
    "allowedDirs".into(),
    Value::Array(dirs.into_iter().map(Value::String).collect()),
  );
  write_store(&store, &map)?;
  Ok(())
}
