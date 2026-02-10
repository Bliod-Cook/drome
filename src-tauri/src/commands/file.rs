use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};
use tauri_plugin_shell::open::open;
use walkdir::WalkDir;

use crate::error::{DromeError, Result};
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadata {
  pub file_path: String,
  pub file_name: String,
  pub content: Option<Vec<u8>>,
}

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

pub fn resolve_path(path: String) -> Result<String> {
  Ok(normalize_path(&path).to_string_lossy().to_string())
}

pub fn is_path_inside(child_path: String, parent_path: String) -> Result<bool> {
  let child = PathBuf::from(child_path);
  let parent = PathBuf::from(parent_path);
  let child = child.canonicalize().unwrap_or(child);
  let parent = parent.canonicalize().unwrap_or(parent);
  Ok(child.starts_with(parent))
}

pub fn has_write_permission(path: String) -> Result<bool> {
  let path = normalize_path(&path);
  if !path.exists() {
    return Ok(false);
  }
  let meta = std::fs::metadata(&path)?;
  if meta.is_dir() {
    // Try creating a temp file.
    let test_path = path.join(".drome_write_test");
    match std::fs::OpenOptions::new().create(true).write(true).open(&test_path) {
      Ok(_) => {
        let _ = std::fs::remove_file(test_path);
        Ok(true)
      }
      Err(_) => Ok(false),
    }
  } else {
    Ok(meta.permissions().readonly() == false)
  }
}

fn is_allowed(state: &State<'_, AppState>, path: &Path) -> bool {
  if path.starts_with(&state.app_data_dir) {
    return true;
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

fn allow_dir(state: &State<'_, AppState>, dir: &Path) {
  if let Ok(mut dirs) = state.allowed_dirs.lock() {
    if !dirs.iter().any(|d| d == dir) {
      dirs.push(dir.to_path_buf());
    }
  }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirectoryListOptions {
  recursive: Option<bool>,
  max_depth: Option<usize>,
  include_hidden: Option<bool>,
  include_files: Option<bool>,
  include_directories: Option<bool>,
  max_entries: Option<usize>,
  search_pattern: Option<String>,
}

impl DirectoryListOptions {
  fn merged(options: Option<Value>) -> Self {
    let parsed = options
      .and_then(|v| serde_json::from_value::<DirectoryListOptions>(v).ok())
      .unwrap_or(DirectoryListOptions {
        recursive: None,
        max_depth: None,
        include_hidden: None,
        include_files: None,
        include_directories: None,
        max_entries: None,
        search_pattern: None,
      });

    DirectoryListOptions {
      recursive: Some(parsed.recursive.unwrap_or(true)),
      max_depth: Some(parsed.max_depth.unwrap_or(10)),
      include_hidden: Some(parsed.include_hidden.unwrap_or(false)),
      include_files: Some(parsed.include_files.unwrap_or(true)),
      include_directories: Some(parsed.include_directories.unwrap_or(true)),
      max_entries: Some(parsed.max_entries.unwrap_or(20)),
      search_pattern: Some(parsed.search_pattern.unwrap_or_else(|| ".".to_string())),
    }
  }
}

pub fn file_open(app: &AppHandle, state: &State<'_, AppState>, options: Option<Value>) -> Result<Option<FileMetadata>> {
  let filters = options
    .as_ref()
    .and_then(|v| v.get("filters"))
    .and_then(|v| v.as_array())
    .cloned()
    .unwrap_or_default();

  let mut dialog = tauri_plugin_dialog::DialogExt::dialog(app).file();
  if !filters.is_empty() {
    for f in filters {
      if let (Some(name), Some(exts)) = (f.get("name").and_then(|v| v.as_str()), f.get("extensions").and_then(|v| v.as_array())) {
        let exts: Vec<&str> = exts.iter().filter_map(|e| e.as_str()).collect();
        if !exts.is_empty() {
          dialog = dialog.add_filter(name, &exts);
        }
      }
    }
  }

  let selected = dialog.blocking_pick_file();
  let path = match selected {
    Some(p) => p.into_path().map_err(|e| DromeError::Message(e.to_string()))?,
    None => return Ok(None),
  };

  let parent = path.parent().unwrap_or(&path).to_path_buf();
  allow_dir(state, &parent);

  // Best-effort: always read bytes so existing restore/zip flows work.
  let bytes = std::fs::read(&path).ok();
  Ok(Some(FileMetadata {
    file_path: path.to_string_lossy().to_string(),
    file_name: path
      .file_name()
      .unwrap_or_default()
      .to_string_lossy()
      .to_string(),
    content: bytes,
  }))
}

pub fn file_select_folder(app: &AppHandle, state: &State<'_, AppState>, _options: Option<Value>) -> Result<Option<String>> {
  let selected = tauri_plugin_dialog::DialogExt::dialog(app).file().blocking_pick_folder();
  let path = match selected {
    Some(p) => p.into_path().map_err(|e| DromeError::Message(e.to_string()))?,
    None => return Ok(None),
  };
  allow_dir(state, &path);
  Ok(Some(path.to_string_lossy().to_string()))
}

pub fn file_save(app: &AppHandle, state: &State<'_, AppState>, args: Vec<Value>) -> Result<Option<String>> {
  let suggested_name = args
    .get(0)
    .and_then(|v| v.as_str())
    .unwrap_or("output.txt")
    .to_string();
  let content = args.get(1).cloned().unwrap_or(Value::Null);

  let path = tauri_plugin_dialog::DialogExt::dialog(app)
    .file()
    .set_file_name(&suggested_name)
    .blocking_save_file();
  let Some(path) = path else { return Ok(None) };
  let path = path.into_path().map_err(|e| DromeError::Message(e.to_string()))?;
  let parent = path.parent().unwrap_or(&path).to_path_buf();
  allow_dir(state, &parent);

  if !is_allowed(state, &path) {
    return Err(DromeError::Message("Path not allowed".into()));
  }

  match content {
    Value::String(s) => std::fs::write(&path, s.as_bytes())?,
    Value::Array(arr) => {
      let bytes: Vec<u8> = arr.into_iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect();
      std::fs::write(&path, bytes)?;
    }
    _ => return Err(DromeError::Message("Unsupported save content".into())),
  }

  Ok(Some(path.to_string_lossy().to_string()))
}

pub fn file_read(state: &State<'_, AppState>, file_id: String, _detect_encoding: bool) -> Result<String> {
  let path = normalize_path(&file_id);
  if !is_allowed(state, &path) {
    return Err(DromeError::Message("Path not allowed".into()));
  }
  let content = std::fs::read_to_string(path)?;
  Ok(content)
}

pub fn file_write(state: &State<'_, AppState>, file_path: String, data: Value) -> Result<()> {
  let path = normalize_path(&file_path);
  if !is_allowed(state, &path) {
    return Err(DromeError::Message("Path not allowed".into()));
  }
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }

  match data {
    Value::String(s) => std::fs::write(path, s.as_bytes())?,
    Value::Array(arr) => {
      let bytes: Vec<u8> = arr.into_iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect();
      std::fs::write(path, bytes)?;
    }
    _ => return Err(DromeError::Message("Unsupported write payload".into())),
  }
  Ok(())
}

pub fn file_mkdir(state: &State<'_, AppState>, dir_path: String) -> Result<()> {
  let path = normalize_path(&dir_path);
  if !is_allowed(state, &path) {
    return Err(DromeError::Message("Path not allowed".into()));
  }
  std::fs::create_dir_all(path)?;
  Ok(())
}

pub fn file_delete(state: &State<'_, AppState>, file_id: String) -> Result<()> {
  let path = normalize_path(&file_id);
  if !is_allowed(state, &path) {
    return Err(DromeError::Message("Path not allowed".into()));
  }
  if path.is_file() {
    std::fs::remove_file(path)?;
  }
  Ok(())
}

pub fn file_delete_dir(state: &State<'_, AppState>, dir_path: String) -> Result<()> {
  let path = normalize_path(&dir_path);
  if !is_allowed(state, &path) {
    return Err(DromeError::Message("Path not allowed".into()));
  }
  if path.is_dir() {
    std::fs::remove_dir_all(path)?;
  }
  Ok(())
}

pub fn file_is_directory(state: &State<'_, AppState>, file_path: String) -> Result<bool> {
  let path = normalize_path(&file_path);
  if !is_allowed(state, &path) {
    return Ok(false);
  }
  Ok(path.is_dir())
}

pub fn file_list_directory(
  state: &State<'_, AppState>,
  dir_path: String,
  options: Option<Value>,
) -> Result<Vec<String>> {
  let path = normalize_path(&dir_path);
  if !is_allowed(state, &path) {
    return Err(DromeError::Message("Path not allowed".into()));
  }
  if !path.is_dir() {
    return Err(DromeError::Message("Path is not a directory".into()));
  }

  let options = DirectoryListOptions::merged(options);
  let recursive = options.recursive.unwrap_or(true);
  let max_depth = options.max_depth.unwrap_or(10);
  let include_hidden = options.include_hidden.unwrap_or(false);
  let include_files = options.include_files.unwrap_or(true);
  let include_directories = options.include_directories.unwrap_or(true);
  let max_entries = options.max_entries.unwrap_or(20);
  let search_pattern = options.search_pattern.unwrap_or_else(|| ".".to_string()).to_lowercase();

  let effective_max_depth = if recursive { max_depth } else { 1 };

  let mut out: Vec<String> = Vec::new();
  for entry in WalkDir::new(&path)
    .max_depth(effective_max_depth)
    .follow_links(false)
    .into_iter()
    .filter_map(|e| e.ok())
  {
    if entry.depth() == 0 {
      continue;
    }

    let file_name = entry.file_name().to_string_lossy();
    if !include_hidden && file_name.starts_with('.') {
      continue;
    }

    let is_dir = entry.file_type().is_dir();
    if is_dir && !include_directories {
      continue;
    }
    if !is_dir && !include_files {
      continue;
    }

    if search_pattern != "." && !file_name.to_lowercase().contains(&search_pattern) {
      continue;
    }

    let p = entry.path().to_string_lossy().replace('\\', "/");
    out.push(p);
    if out.len() >= max_entries {
      break;
    }
  }

  Ok(out)
}

pub fn file_open_path(_app: &AppHandle, path: String) -> Result<()> {
  let path = strip_file_scheme(&path).to_string();
  open(None, path, None).map_err(|e| DromeError::Message(e.to_string()))?;
  Ok(())
}

pub fn file_show_in_folder(_app: &AppHandle, path: String) -> Result<()> {
  let path = normalize_path(&path);
  let folder = if path.is_dir() {
    path
  } else {
    path.parent().unwrap_or(&path).to_path_buf()
  };
  open(None, folder.to_string_lossy().to_string(), None).map_err(|e| DromeError::Message(e.to_string()))?;
  Ok(())
}
