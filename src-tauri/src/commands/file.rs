use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Utc};
use md5::Context as Md5Context;
use mime_guess::MimeGuess;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_shell::open::open;
use uuid::Uuid;
use walkdir::WalkDir;
use zip::ZipArchive;

use crate::commands::system;
use crate::error::{DromeError, Result};
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOpenResult {
    pub file_path: String,
    pub file_name: String,
    pub content: Option<Vec<u8>>,
    pub size: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum FileTypes {
    Image,
    Video,
    Audio,
    Text,
    Document,
    Other,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct StoredFileMetadata {
    pub id: String,
    pub name: String,
    pub origin_name: String,
    pub path: String,
    pub size: u64,
    pub ext: String,
    #[serde(rename = "type")]
    pub file_type: FileTypes,
    pub created_at: String,
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Base64ImageResult {
    pub mime: String,
    pub base64: String,
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryDataResult {
    pub data: Vec<u8>,
    pub mime: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Base64FileResult {
    pub data: String,
    pub mime: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileNameGuardResult {
    pub safe_name: String,
    pub exists: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchUploadMarkdownResult {
    pub file_count: u32,
    pub folder_count: u32,
    pub skipped_files: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesTreeNode {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub tree_path: String,
    pub external_path: String,
    pub children: Option<Vec<NotesTreeNode>>,
    pub created_at: String,
    pub updated_at: String,
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

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn data_dir(state: &State<'_, AppState>) -> PathBuf {
    state.app_data_dir.join("Data")
}

fn files_dir(state: &State<'_, AppState>) -> PathBuf {
    data_dir(state).join("Files")
}

fn notes_dir(state: &State<'_, AppState>) -> PathBuf {
    data_dir(state).join("Notes")
}

fn temp_dir(state: &State<'_, AppState>) -> PathBuf {
    state.app_data_dir.join("Temp")
}

fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    Ok(())
}

fn is_allowed(state: &State<'_, AppState>, path: &Path) -> bool {
    if path.starts_with(&state.app_data_dir) {
        return true;
    }
    if path.starts_with(&state.app_config_dir) {
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
    let _ = system::add_allowed_dir_to_store(state, dir);
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
    let meta = fs::metadata(&path)?;
    if meta.is_dir() {
        let test_path = path.join(".drome_write_test");
        match fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&test_path)
        {
            Ok(_) => {
                let _ = fs::remove_file(test_path);
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    } else {
        Ok(!meta.permissions().readonly())
    }
}

fn is_absolute_like(input: &str) -> bool {
    let stripped = strip_file_scheme(input);
    if stripped.starts_with("~/") {
        return true;
    }
    Path::new(stripped).is_absolute()
}

fn storage_path_for_id(state: &State<'_, AppState>, id_or_path: &str) -> PathBuf {
    if is_absolute_like(id_or_path) {
        normalize_path(id_or_path)
    } else {
        files_dir(state).join(id_or_path)
    }
}

fn ext_lower(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{}", s.to_lowercase()))
        .unwrap_or_default()
}

fn system_time_iso(t: std::time::SystemTime) -> String {
    DateTime::<Utc>::from(t).to_rfc3339()
}

fn metadata_created_iso(meta: &fs::Metadata) -> String {
    if let Ok(created) = meta.created() {
        return system_time_iso(created);
    }
    if let Ok(modified) = meta.modified() {
        return system_time_iso(modified);
    }
    system_time_iso(std::time::SystemTime::now())
}

fn metadata_modified_iso(meta: &fs::Metadata) -> String {
    if let Ok(modified) = meta.modified() {
        return system_time_iso(modified);
    }
    metadata_created_iso(meta)
}

fn is_probably_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    if bytes.iter().any(|b| *b == 0) {
        return false;
    }
    let mut weird = 0usize;
    for &b in bytes.iter().take(8192) {
        let is_ok = matches!(b, b'\n' | b'\r' | b'\t') || (b >= 0x20 && b <= 0x7E);
        if !is_ok {
            weird += 1;
        }
    }
    weird * 100 / bytes.len().min(8192) < 30
}

fn file_type_by_ext_or_content(path: &Path) -> FileTypes {
    let ext = ext_lower(path);
    let ext_s = ext.as_str();

    const IMAGE_EXTS: &[&str] = &[".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg"];
    const VIDEO_EXTS: &[&str] = &[".mp4", ".mov", ".mkv", ".webm", ".avi"];
    const AUDIO_EXTS: &[&str] = &[".mp3", ".wav", ".m4a", ".ogg", ".flac"];
    const TEXT_EXTS: &[&str] = &[
        ".txt",
        ".md",
        ".markdown",
        ".json",
        ".csv",
        ".yaml",
        ".yml",
        ".log",
        ".html",
        ".htm",
        ".xml",
        ".js",
        ".ts",
        ".tsx",
        ".jsx",
        ".css",
        ".ini",
        ".toml",
        ".rs",
        ".py",
        ".java",
        ".go",
        ".c",
        ".cpp",
        ".h",
        ".hpp",
    ];
    const DOC_EXTS: &[&str] = &[
        ".pdf", ".doc", ".docx", ".ppt", ".pptx", ".xls", ".xlsx", ".epub", ".rtf",
    ];

    if IMAGE_EXTS.contains(&ext_s) {
        return FileTypes::Image;
    }
    if VIDEO_EXTS.contains(&ext_s) {
        return FileTypes::Video;
    }
    if AUDIO_EXTS.contains(&ext_s) {
        return FileTypes::Audio;
    }
    if TEXT_EXTS.contains(&ext_s) {
        return FileTypes::Text;
    }
    if DOC_EXTS.contains(&ext_s) {
        return FileTypes::Document;
    }

    // Best-effort: if it looks like text, treat as text.
    let mut buf = [0u8; 4096];
    if let Ok(mut f) = fs::File::open(path) {
        if let Ok(n) = f.read(&mut buf) {
            if is_probably_text(&buf[..n]) {
                return FileTypes::Text;
            }
        }
    }
    FileTypes::Other
}

fn sanitize_filename(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return "untitled".into();
    }

    let mut out = String::with_capacity(trimmed.len());
    for c in trimmed.chars() {
        let ok = c.is_ascii_alphanumeric()
            || matches!(
                c,
                ' ' | '-' | '_' | '.' | '(' | ')' | '[' | ']' | '、' | '·'
            );
        out.push(if ok { c } else { '_' });
    }

    // Avoid reserved names on Windows.
    let lower = out.to_lowercase();
    const RESERVED: &[&str] = &[
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "lpt1", "lpt2", "lpt3",
    ];
    if RESERVED.contains(&lower.as_str()) {
        return format!("_{out}");
    }

    out
}

fn unique_name(base_dir: &Path, name: &str, is_file: bool) -> String {
    let base = sanitize_filename(name);
    let mut counter = 0u32;
    loop {
        let candidate = if counter == 0 {
            base.clone()
        } else {
            format!("{base}{counter}")
        };
        let full = if is_file {
            base_dir.join(format!("{candidate}.md"))
        } else {
            base_dir.join(&candidate)
        };
        if !full.exists() {
            return candidate;
        }
        counter += 1;
    }
}

pub fn file_open(
    app: &AppHandle,
    state: &State<'_, AppState>,
    options: Option<Value>,
) -> Result<Option<FileOpenResult>> {
    let filters = options
        .as_ref()
        .and_then(|v| v.get("filters"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let title = options
        .as_ref()
        .and_then(|v| v.get("title"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let default_path = options
        .as_ref()
        .and_then(|v| v.get("defaultPath"))
        .and_then(|v| v.as_str())
        .map(|s| normalize_path(s));

    let mut dialog = tauri_plugin_dialog::DialogExt::dialog(app).file();
    if let Some(t) = title {
        dialog = dialog.set_title(t);
    }
    if let Some(p) = default_path {
        dialog = dialog.set_directory(p);
    }
    if !filters.is_empty() {
        for f in filters {
            if let (Some(name), Some(exts)) = (
                f.get("name").and_then(|v| v.as_str()),
                f.get("extensions").and_then(|v| v.as_array()),
            ) {
                let exts: Vec<&str> = exts.iter().filter_map(|e| e.as_str()).collect();
                if !exts.is_empty() {
                    dialog = dialog.add_filter(name, &exts);
                }
            }
        }
    }

    let selected = dialog.blocking_pick_file();
    let path = match selected {
        Some(p) => p
            .into_path()
            .map_err(|e| DromeError::Message(e.to_string()))?,
        None => return Ok(None),
    };

    let parent = path.parent().unwrap_or(&path).to_path_buf();
    allow_dir(state, &parent);

    let meta = fs::metadata(&path)?;
    let size = meta.len();
    // Read file bytes unless it's huge (keep behavior close to Electron which avoids >2GB).
    let content = if size < 2 * 1024 * 1024 * 1024 {
        fs::read(&path).ok()
    } else {
        None
    };

    Ok(Some(FileOpenResult {
        file_path: path_to_string(&path),
        file_name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        content,
        size,
    }))
}

pub fn file_open_path(_app: &AppHandle, path: String) -> Result<()> {
    let path = strip_file_scheme(&path).to_string();
    open(None, path, None).map_err(|e| DromeError::Message(e.to_string()))?;
    Ok(())
}

pub fn file_select_folder(
    app: &AppHandle,
    state: &State<'_, AppState>,
    options: Option<Value>,
) -> Result<Option<String>> {
    let title = options
        .as_ref()
        .and_then(|v| v.get("title"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let default_path = options
        .as_ref()
        .and_then(|v| v.get("defaultPath"))
        .and_then(|v| v.as_str())
        .map(|s| normalize_path(s));

    let mut dialog = tauri_plugin_dialog::DialogExt::dialog(app).file();
    if let Some(t) = title {
        dialog = dialog.set_title(t);
    }
    if let Some(p) = default_path {
        dialog = dialog.set_directory(p);
    }

    let selected = dialog.blocking_pick_folder();
    let path = match selected {
        Some(p) => p
            .into_path()
            .map_err(|e| DromeError::Message(e.to_string()))?,
        None => return Ok(None),
    };

    allow_dir(state, &path);
    Ok(Some(path_to_string(&path)))
}

pub fn file_save(
    app: &AppHandle,
    state: &State<'_, AppState>,
    args: Vec<Value>,
) -> Result<Option<String>> {
    let suggested_name = args
        .get(0)
        .and_then(|v| v.as_str())
        .unwrap_or("output.txt")
        .to_string();
    let content = args.get(1).cloned().unwrap_or(Value::Null);
    let options = args.get(2).cloned();

    let filters = options
        .as_ref()
        .and_then(|v| v.get("filters"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let title = options
        .as_ref()
        .and_then(|v| v.get("title"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut dialog = tauri_plugin_dialog::DialogExt::dialog(app)
        .file()
        .set_file_name(&suggested_name);
    if let Some(t) = title {
        dialog = dialog.set_title(t);
    }
    if !filters.is_empty() {
        for f in filters {
            if let (Some(name), Some(exts)) = (
                f.get("name").and_then(|v| v.as_str()),
                f.get("extensions").and_then(|v| v.as_array()),
            ) {
                let exts: Vec<&str> = exts.iter().filter_map(|e| e.as_str()).collect();
                if !exts.is_empty() {
                    dialog = dialog.add_filter(name, &exts);
                }
            }
        }
    }

    let path = dialog.blocking_save_file();
    let Some(path) = path else { return Ok(None) };
    let path = path
        .into_path()
        .map_err(|e| DromeError::Message(e.to_string()))?;

    let parent = path.parent().unwrap_or(&path).to_path_buf();
    allow_dir(state, &parent);

    if !is_allowed(state, &path) {
        return Err(DromeError::Message("Path not allowed".into()));
    }

    match content {
        Value::String(s) => fs::write(&path, s.as_bytes())?,
        Value::Array(arr) => {
            let bytes: Vec<u8> = arr
                .into_iter()
                .filter_map(|v| v.as_u64().map(|n| n as u8))
                .collect();
            fs::write(&path, bytes)?;
        }
        _ => return Err(DromeError::Message("Unsupported save content".into())),
    }

    Ok(Some(path_to_string(&path)))
}

pub fn file_select(
    app: &AppHandle,
    state: &State<'_, AppState>,
    options: Option<Value>,
) -> Result<Option<Vec<StoredFileMetadata>>> {
    let filters = options
        .as_ref()
        .and_then(|v| v.get("filters"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let title = options
        .as_ref()
        .and_then(|v| v.get("title"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let default_path = options
        .as_ref()
        .and_then(|v| v.get("defaultPath"))
        .and_then(|v| v.as_str())
        .map(|s| normalize_path(s));

    let props: Vec<String> = options
        .as_ref()
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let multi = props.iter().any(|p| p == "multiSelections");

    let mut dialog = tauri_plugin_dialog::DialogExt::dialog(app).file();
    if let Some(t) = title {
        dialog = dialog.set_title(t);
    }
    if let Some(p) = default_path {
        dialog = dialog.set_directory(p);
    }
    if !filters.is_empty() {
        for f in filters {
            if let (Some(name), Some(exts)) = (
                f.get("name").and_then(|v| v.as_str()),
                f.get("extensions").and_then(|v| v.as_array()),
            ) {
                let exts: Vec<&str> = exts.iter().filter_map(|e| e.as_str()).collect();
                if !exts.is_empty() {
                    dialog = dialog.add_filter(name, &exts);
                }
            }
        }
    }

    let selected = if multi {
        dialog.blocking_pick_files()
    } else {
        dialog.blocking_pick_file().map(|p| vec![p])
    };
    let paths = match selected {
        Some(p) => p,
        None => return Ok(None),
    };

    let mut out = Vec::with_capacity(paths.len());
    for fp in paths {
        let path = fp
            .into_path()
            .map_err(|e| DromeError::Message(e.to_string()))?;
        let parent = path.parent().unwrap_or(&path).to_path_buf();
        allow_dir(state, &parent);

        let meta = fs::metadata(&path)?;
        let ext = {
            let e = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if e.is_empty() {
                "".to_string()
            } else {
                format!(".{}", e.to_string())
            }
        };
        let file_type = file_type_by_ext_or_content(&path);
        out.push(StoredFileMetadata {
            id: Uuid::new_v4().to_string(),
            origin_name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            path: path_to_string(&path),
            created_at: metadata_created_iso(&meta),
            size: meta.len(),
            ext,
            file_type,
            count: 1,
        });
    }

    Ok(Some(out))
}

pub fn file_get(
    state: &State<'_, AppState>,
    file_path: String,
) -> Result<Option<StoredFileMetadata>> {
    let path = normalize_path(&file_path);
    if !path.exists() {
        return Ok(None);
    }

    // A dropped file path might not have been allowed via a dialog; allow its parent so subsequent reads/uploads work.
    let parent = path.parent().unwrap_or(&path).to_path_buf();
    allow_dir(state, &parent);

    if !is_allowed(state, &path) {
        return Err(DromeError::Message("Path not allowed".into()));
    }

    let meta = fs::metadata(&path)?;
    let ext = {
        let e = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if e.is_empty() {
            "".to_string()
        } else {
            format!(".{}", e.to_string())
        }
    };
    let file_type = file_type_by_ext_or_content(&path);
    Ok(Some(StoredFileMetadata {
        id: Uuid::new_v4().to_string(),
        origin_name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        path: path_to_string(&path),
        created_at: metadata_created_iso(&meta),
        size: meta.len(),
        ext,
        file_type,
        count: 1,
    }))
}

fn md5_hex(path: &Path) -> Result<String> {
    let mut ctx = Md5Context::new();
    let mut f = fs::File::open(path)?;
    let mut buf = [0u8; 8192];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        ctx.consume(&buf[..n]);
    }
    Ok(format!("{:x}", ctx.compute()))
}

fn find_duplicate_file(
    state: &State<'_, AppState>,
    source_path: &Path,
) -> Result<Option<StoredFileMetadata>> {
    let meta = fs::metadata(source_path)?;
    let size = meta.len();
    let src_hash = md5_hex(source_path)?;

    let dir = files_dir(state);
    if !dir.exists() {
        return Ok(None);
    }

    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let Ok(stored_meta) = fs::metadata(&p) else {
            continue;
        };
        if stored_meta.len() != size {
            continue;
        }

        if let Ok(stored_hash) = md5_hex(&p) {
            if stored_hash == src_hash {
                let ext = ext_lower(&p);
                let id = p
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                let file_type = file_type_by_ext_or_content(source_path);
                return Ok(Some(StoredFileMetadata {
                    id,
                    origin_name: p
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    name: p
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    path: path_to_string(&p),
                    created_at: metadata_created_iso(&stored_meta),
                    size,
                    ext,
                    file_type,
                    count: 2,
                }));
            }
        }
    }

    Ok(None)
}

pub fn file_upload(
    state: &State<'_, AppState>,
    file: StoredFileMetadata,
) -> Result<StoredFileMetadata> {
    let source = normalize_path(&file.path);
    if !source.exists() || !source.is_file() {
        return Err(DromeError::Message("Source file does not exist".into()));
    }

    // Ensure parent is allowed (drag-drop paths).
    let parent = source.parent().unwrap_or(&source).to_path_buf();
    allow_dir(state, &parent);

    if let Some(dup) = find_duplicate_file(state, &source)? {
        return Ok(dup);
    }

    ensure_dir(&files_dir(state))?;

    let origin_name = source
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = ext_lower(&source);
    let uuid = Uuid::new_v4().to_string();
    let dest = files_dir(state).join(format!("{uuid}{ext}"));

    fs::copy(&source, &dest)?;
    let meta = fs::metadata(&dest)?;
    let file_type = file_type_by_ext_or_content(&dest);

    Ok(StoredFileMetadata {
        id: uuid,
        origin_name,
        name: dest
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        path: path_to_string(&dest),
        created_at: metadata_created_iso(&meta),
        size: meta.len(),
        ext,
        file_type,
        count: 1,
    })
}

pub fn file_delete(state: &State<'_, AppState>, file_id: String) -> Result<()> {
    let path = files_dir(state).join(file_id);
    if path.exists() && path.is_file() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn file_delete_dir(state: &State<'_, AppState>, dir_id: String) -> Result<()> {
    let path = files_dir(state).join(dir_id);
    if path.exists() && path.is_dir() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

pub fn file_delete_external_file(state: &State<'_, AppState>, file_path: String) -> Result<()> {
    let path = normalize_path(&file_path);
    if !is_allowed(state, &path) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    if path.exists() && path.is_file() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn file_delete_external_dir(state: &State<'_, AppState>, dir_path: String) -> Result<()> {
    let path = normalize_path(&dir_path);
    if !is_allowed(state, &path) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    if path.exists() && path.is_dir() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

pub fn file_move(state: &State<'_, AppState>, file_path: String, new_path: String) -> Result<()> {
    let src = normalize_path(&file_path);
    let dest = normalize_path(&new_path);
    if !is_allowed(state, &src) || !is_allowed(state, &dest) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    if !src.exists() {
        return Err(DromeError::Message("Source does not exist".into()));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(src, dest)?;
    Ok(())
}

pub fn file_move_dir(
    state: &State<'_, AppState>,
    dir_path: String,
    new_dir_path: String,
) -> Result<()> {
    let src = normalize_path(&dir_path);
    let dest = normalize_path(&new_dir_path);
    if !is_allowed(state, &src) || !is_allowed(state, &dest) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(src, dest)?;
    Ok(())
}

pub fn file_rename(state: &State<'_, AppState>, file_path: String, new_name: String) -> Result<()> {
    let src = normalize_path(&file_path);
    if !is_allowed(state, &src) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    let parent = src
        .parent()
        .ok_or_else(|| DromeError::Message("Invalid path".into()))?;
    let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("");
    let dest = if ext.is_empty() {
        parent.join(&new_name)
    } else {
        parent.join(format!("{new_name}.{ext}"))
    };
    if !is_allowed(state, &dest) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    fs::rename(src, dest)?;
    Ok(())
}

pub fn file_rename_dir(
    state: &State<'_, AppState>,
    dir_path: String,
    new_name: String,
) -> Result<()> {
    let src = normalize_path(&dir_path);
    if !is_allowed(state, &src) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    let parent = src
        .parent()
        .ok_or_else(|| DromeError::Message("Invalid path".into()))?;
    let dest = parent.join(&new_name);
    if !is_allowed(state, &dest) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    fs::rename(src, dest)?;
    Ok(())
}

fn extract_docx_text(path: &Path) -> Result<String> {
    let f = fs::File::open(path)?;
    let mut zip = ZipArchive::new(f)?;
    let mut xml = String::new();
    let mut doc = zip
        .by_name("word/document.xml")
        .map_err(|_| DromeError::Message("Missing word/document.xml in docx".into()))?;
    doc.read_to_string(&mut xml)?;

    // Naive text extraction: capture w:t contents, preserve paragraph breaks.
    let mut out = String::new();
    let mut i = 0usize;
    while i < xml.len() {
        if xml[i..].starts_with("<w:tab") {
            out.push('\t');
        } else if xml[i..].starts_with("<w:br")
            || xml[i..].starts_with("<w:cr")
            || xml[i..].starts_with("</w:p")
        {
            out.push('\n');
        } else if let Some(start) = xml[i..].find("<w:t") {
            i += start;
            // find '>' then read until </w:t>
            if let Some(gt) = xml[i..].find('>') {
                i += gt + 1;
                if let Some(end) = xml[i..].find("</w:t>") {
                    out.push_str(&xml[i..i + end]);
                    i += end + "</w:t>".len();
                    continue;
                }
            }
        }
        i += 1;
    }

    Ok(out)
}

fn read_file_core(path: &Path, force_extract: bool) -> Result<String> {
    if !path.exists() || !path.is_file() {
        return Err(DromeError::Message("File does not exist".into()));
    }

    let ext = ext_lower(path);
    if ext == ".pdf" {
        return pdf_extract::extract_text(path)
            .map_err(|e| DromeError::Message(format!("Failed to extract pdf text: {e}")));
    }
    if ext == ".docx" {
        return extract_docx_text(path);
    }

    // Normal text read.
    if force_extract {
        let bytes = fs::read(path)?;
        return Ok(String::from_utf8_lossy(&bytes).to_string());
    }

    match fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(_) => {
            let bytes = fs::read(path)?;
            Ok(String::from_utf8_lossy(&bytes).to_string())
        }
    }
}

pub fn file_read(
    state: &State<'_, AppState>,
    file_id: String,
    detect_encoding: bool,
) -> Result<String> {
    let path = storage_path_for_id(state, &file_id);
    if !is_allowed(state, &path) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    read_file_core(&path, detect_encoding)
}

pub fn file_read_external(
    state: &State<'_, AppState>,
    file_path: String,
    detect_encoding: bool,
) -> Result<String> {
    let path = normalize_path(&file_path);
    if !is_allowed(state, &path) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    read_file_core(&path, detect_encoding)
}

pub fn file_clear(state: &State<'_, AppState>) -> Result<()> {
    let dir = files_dir(state);
    if dir.exists() {
        let _ = fs::remove_dir_all(&dir);
    }
    ensure_dir(&dir)?;
    ensure_dir(&notes_dir(state))?;
    Ok(())
}

pub fn file_create_temp_file(state: &State<'_, AppState>, file_name: String) -> Result<String> {
    let dir = temp_dir(state);
    ensure_dir(&dir)?;
    let path = dir.join(format!("temp_file_{}_{}", Uuid::new_v4(), file_name));
    Ok(path_to_string(&path))
}

pub fn file_write(state: &State<'_, AppState>, file_path: String, data: Value) -> Result<()> {
    let path = normalize_path(&file_path);
    if !is_allowed(state, &path) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    match data {
        Value::String(s) => fs::write(path, s.as_bytes())?,
        Value::Array(arr) => {
            let bytes: Vec<u8> = arr
                .into_iter()
                .filter_map(|v| v.as_u64().map(|n| n as u8))
                .collect();
            fs::write(path, bytes)?;
        }
        _ => return Err(DromeError::Message("Unsupported write payload".into())),
    }

    Ok(())
}

pub fn file_write_with_id(state: &State<'_, AppState>, id: String, content: String) -> Result<()> {
    let dir = files_dir(state);
    ensure_dir(&dir)?;
    let path = dir.join(id);
    fs::write(path, content.as_bytes())?;
    Ok(())
}

pub fn file_mkdir(state: &State<'_, AppState>, dir_path: String) -> Result<String> {
    let path = normalize_path(&dir_path);
    if !is_allowed(state, &path) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    fs::create_dir_all(&path)?;
    Ok(path_to_string(&path))
}

pub fn file_is_directory(state: &State<'_, AppState>, file_path: String) -> Result<bool> {
    let path = normalize_path(&file_path);
    if !is_allowed(state, &path) {
        return Ok(false);
    }
    Ok(path.is_dir())
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
    let search_pattern = options
        .search_pattern
        .unwrap_or_else(|| ".".to_string())
        .to_lowercase();

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

        out.push(path_to_string(entry.path()));
        if out.len() >= max_entries {
            break;
        }
    }

    Ok(out)
}

pub fn file_check_file_name(
    state: &State<'_, AppState>,
    dir_path: String,
    file_name: String,
    is_file: bool,
) -> Result<FileNameGuardResult> {
    let base = normalize_path(&dir_path);
    if !is_allowed(state, &base) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    let safe = unique_name(&base, &file_name, is_file);
    Ok(FileNameGuardResult {
        safe_name: safe,
        exists: false,
    })
}

pub fn file_validate_notes_directory(
    state: &State<'_, AppState>,
    dir_path: String,
) -> Result<bool> {
    let path = normalize_path(&dir_path);
    if dir_path.trim().is_empty() {
        return Ok(false);
    }
    if !path.exists() {
        return Ok(false);
    }
    if !path.is_dir() {
        return Ok(false);
    }

    // Prevent selecting app data directories.
    if path.starts_with(&state.app_data_dir) || path.starts_with(&state.app_config_dir) {
        return Ok(false);
    }

    // Best-effort writability check.
    let test = path.join(".drome_notes_write_test");
    match fs::OpenOptions::new().create(true).write(true).open(&test) {
        Ok(mut f) => {
            let _ = f.write_all(b"test");
            let _ = fs::remove_file(test);
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

fn scan_notes_dir(base: &Path, current: &Path, depth: usize) -> Result<Vec<NotesTreeNode>> {
    if depth > 10 {
        return Ok(Vec::new());
    }
    if !current.exists() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<_> = fs::read_dir(current)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    let mut out = Vec::new();
    for entry in entries {
        let name_os = entry.file_name();
        let name = name_os.to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }

        let path = entry.path();
        let meta = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let rel = path.strip_prefix(base).unwrap_or(&path);
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        let tree_path_dir = format!("/{}", rel_s.trim_start_matches('/'));

        if meta.is_dir() {
            let external = path_to_string(&path);
            let id = format!("{:x}", md5::compute(external.as_bytes()));
            let children = scan_notes_dir(base, &path, depth + 1)?;
            out.push(NotesTreeNode {
                id,
                name,
                node_type: "folder".into(),
                tree_path: tree_path_dir,
                external_path: external,
                children: Some(children),
                created_at: metadata_created_iso(&meta),
                updated_at: metadata_modified_iso(&meta),
            });
            continue;
        }

        if meta.is_file() {
            if ext_lower(&path) != ".md" {
                continue;
            }

            let external = path_to_string(&path);
            let id = format!("{:x}", md5::compute(external.as_bytes()));
            let file_stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&name)
                .to_string();

            let dir_rel = rel
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let tree_path = if dir_rel.is_empty() || dir_rel == "." {
                format!("/{file_stem}")
            } else {
                format!(
                    "/{}/{}",
                    dir_rel.trim_start_matches("./").trim_start_matches('/'),
                    file_stem
                )
            };

            out.push(NotesTreeNode {
                id,
                name: file_stem,
                node_type: "file".into(),
                tree_path,
                external_path: external,
                children: None,
                created_at: metadata_created_iso(&meta),
                updated_at: metadata_modified_iso(&meta),
            });
        }
    }

    Ok(out)
}

pub fn file_get_directory_structure(
    state: &State<'_, AppState>,
    dir_path: String,
) -> Result<Vec<NotesTreeNode>> {
    let base = normalize_path(&dir_path);
    if !is_allowed(state, &base) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    ensure_dir(&base)?;
    scan_notes_dir(&base, &base, 0)
}

pub fn file_open_with_relative_path(
    state: &State<'_, AppState>,
    file: StoredFileMetadata,
) -> Result<()> {
    let file_name = if !file.name.is_empty() {
        file.name.clone()
    } else {
        format!("{}{}", file.id, file.ext)
    };
    let path = files_dir(state).join(file_name);
    if path.exists() {
        open(None, path_to_string(&path), None).map_err(|e| DromeError::Message(e.to_string()))?;
    }
    Ok(())
}

pub fn file_is_text_file(state: &State<'_, AppState>, file_path: String) -> Result<bool> {
    let path = storage_path_for_id(state, &file_path);
    if !is_allowed(state, &path) {
        return Ok(false);
    }
    let bytes = fs::read(&path).unwrap_or_default();
    Ok(is_probably_text(&bytes))
}

pub fn file_save_image(
    app: &AppHandle,
    state: &State<'_, AppState>,
    name: String,
    data: String,
) -> Result<()> {
    let default_name = format!("{name}.png");
    let path = tauri_plugin_dialog::DialogExt::dialog(app)
        .file()
        .set_file_name(default_name)
        .add_filter("PNG", &["png"])
        .blocking_save_file();
    let Some(path) = path else { return Ok(()) };
    let path = path
        .into_path()
        .map_err(|e| DromeError::Message(e.to_string()))?;
    let parent = path.parent().unwrap_or(&path).to_path_buf();
    allow_dir(state, &parent);

    if !is_allowed(state, &path) {
        return Err(DromeError::Message("Path not allowed".into()));
    }

    let (mime, b64) = parse_data_url(&data);
    let _ = mime;
    let bytes = general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| DromeError::Message(format!("Invalid base64: {e}")))?;
    fs::write(path, bytes)?;
    Ok(())
}

fn parse_data_url(input: &str) -> (Option<String>, &str) {
    if let Some(rest) = input.strip_prefix("data:") {
        if let Some(idx) = rest.find(";base64,") {
            let mime = rest[..idx].to_string();
            let data = &rest[idx + ";base64,".len()..];
            return (Some(mime), data);
        }
        if let Some(idx) = rest.find(',') {
            let mime = rest[..idx].to_string();
            let data = &rest[idx + 1..];
            return (Some(mime), data);
        }
    }
    (None, input)
}

pub fn file_save_base64_image(
    state: &State<'_, AppState>,
    base64_data: String,
) -> Result<StoredFileMetadata> {
    let (mime, b64) = parse_data_url(&base64_data);
    let bytes = general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| DromeError::Message(format!("Invalid base64: {e}")))?;

    let ext = match mime.as_deref() {
        Some("image/jpeg") => ".jpg",
        Some("image/jpg") => ".jpg",
        Some("image/webp") => ".webp",
        Some("image/gif") => ".gif",
        Some("image/bmp") => ".bmp",
        _ => ".png",
    }
    .to_string();

    ensure_dir(&files_dir(state))?;
    let uuid = Uuid::new_v4().to_string();
    let dest = files_dir(state).join(format!("{uuid}{ext}"));
    fs::write(&dest, &bytes)?;

    let meta = fs::metadata(&dest)?;
    Ok(StoredFileMetadata {
        id: uuid,
        origin_name: dest
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        name: dest
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        path: path_to_string(&dest),
        created_at: system_time_iso(std::time::SystemTime::now()),
        size: meta.len(),
        ext,
        file_type: FileTypes::Image,
        count: 1,
    })
}

pub fn file_save_pasted_image(
    state: &State<'_, AppState>,
    image_data: Vec<u8>,
    extension: Option<String>,
) -> Result<StoredFileMetadata> {
    let ext = extension.unwrap_or_else(|| ".png".into());
    let ext = if ext.starts_with('.') {
        ext
    } else {
        format!(".{ext}")
    };

    ensure_dir(&files_dir(state))?;
    let uuid = Uuid::new_v4().to_string();
    let dest = files_dir(state).join(format!("{uuid}{ext}"));
    fs::write(&dest, &image_data)?;

    let meta = fs::metadata(&dest)?;
    Ok(StoredFileMetadata {
        id: uuid.clone(),
        origin_name: format!("pasted_image_{uuid}{ext}"),
        name: dest
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        path: path_to_string(&dest),
        created_at: system_time_iso(std::time::SystemTime::now()),
        size: meta.len(),
        ext,
        file_type: FileTypes::Image,
        count: 1,
    })
}

pub fn file_base64_file(state: &State<'_, AppState>, id: String) -> Result<Base64FileResult> {
    let path = files_dir(state).join(id);
    let bytes = fs::read(&path)?;
    let base64 = general_purpose::STANDARD.encode(bytes);
    let mime = MimeGuess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    Ok(Base64FileResult { data: base64, mime })
}

pub fn file_pdf_page_count(state: &State<'_, AppState>, id: String) -> Result<u32> {
    let path = files_dir(state).join(id);
    let doc = lopdf::Document::load(path)
        .map_err(|e| DromeError::Message(format!("Failed to load pdf: {e}")))?;
    Ok(doc.get_pages().len() as u32)
}

pub fn file_binary_image(state: &State<'_, AppState>, id: String) -> Result<BinaryDataResult> {
    let path = files_dir(state).join(id);
    let data = fs::read(&path)?;
    let mime = MimeGuess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    Ok(BinaryDataResult { data, mime })
}

pub fn file_base64_image(state: &State<'_, AppState>, id: String) -> Result<Base64ImageResult> {
    let path = files_dir(state).join(id);
    let data = fs::read(&path)?;
    let base64 = general_purpose::STANDARD.encode(&data);

    let ext = ext_lower(&path);
    let ext = if ext == ".jpg" {
        "jpeg".into()
    } else {
        ext.trim_start_matches('.').to_string()
    };
    let mime = format!("image/{ext}");
    Ok(Base64ImageResult {
        mime: mime.clone(),
        base64: base64.clone(),
        data: format!("data:{mime};base64,{base64}"),
    })
}

pub fn file_copy(state: &State<'_, AppState>, id: String, dest_path: String) -> Result<()> {
    let src = files_dir(state).join(id);
    let dest = normalize_path(&dest_path);
    if !is_allowed(state, &dest) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dest)?;
    Ok(())
}

pub fn file_download(
    state: &State<'_, AppState>,
    url: String,
    is_use_content_type: Option<bool>,
) -> Result<StoredFileMetadata> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; drome/0.1)")
        .build()
        .map_err(|e| DromeError::Message(e.to_string()))?;

    let resp = client
        .get(&url)
        .send()
        .map_err(|e| DromeError::Message(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(DromeError::Message(format!(
            "Download failed: HTTP {}",
            resp.status()
        )));
    }

    let headers = resp.headers().clone();
    let bytes = resp
        .bytes()
        .map_err(|e| DromeError::Message(e.to_string()))?
        .to_vec();

    // Filename from Content-Disposition or URL path.
    let mut filename = headers
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split("filename=").nth(1))
        .map(|s| s.trim_matches(['"', ';', ' ']))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            url.split('/')
                .last()
                .unwrap_or("download")
                .split('?')
                .next()
                .unwrap_or("download")
                .to_string()
        });

    let content_type = headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let needs_ext = is_use_content_type.unwrap_or(false) || !filename.contains('.');
    if needs_ext {
        if let Some(ct) = content_type.as_deref() {
            if let Some(ext) = mime_guess::get_mime_extensions_str(ct).and_then(|exts| exts.first())
            {
                if !filename.ends_with(ext) {
                    filename.push('.');
                    filename.push_str(ext);
                }
            }
        }
    }

    let ext = Path::new(&filename)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{}", s.to_lowercase()))
        .unwrap_or_else(|| ".bin".into());

    ensure_dir(&files_dir(state))?;
    let uuid = Uuid::new_v4().to_string();
    let dest = files_dir(state).join(format!("{uuid}{ext}"));
    fs::write(&dest, &bytes)?;

    let meta = fs::metadata(&dest)?;
    let file_type = file_type_by_ext_or_content(&dest);

    Ok(StoredFileMetadata {
        id: uuid,
        origin_name: filename,
        name: dest
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        path: path_to_string(&dest),
        created_at: system_time_iso(std::time::SystemTime::now()),
        size: meta.len(),
        ext,
        file_type,
        count: 1,
    })
}

pub fn file_show_in_folder(_app: &AppHandle, path: String) -> Result<()> {
    let path = normalize_path(&path);
    let folder = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(&path).to_path_buf()
    };
    open(None, path_to_string(&folder), None).map_err(|e| DromeError::Message(e.to_string()))?;
    Ok(())
}

struct FileWatcherState {
    watch_path: String,
    paused: Arc<AtomicBool>,
    _watcher: RecommendedWatcher,
    app: AppHandle,
    window_label: String,
}

static FILE_WATCHER: OnceLock<Mutex<Option<FileWatcherState>>> = OnceLock::new();

fn watcher_cell() -> &'static Mutex<Option<FileWatcherState>> {
    FILE_WATCHER.get_or_init(|| Mutex::new(None))
}

fn kind_to_event_type(kind: &EventKind) -> Option<&'static str> {
    match kind {
        EventKind::Create(_) => Some("add"),
        EventKind::Modify(_) => Some("change"),
        EventKind::Remove(_) => Some("unlink"),
        _ => None,
    }
}

fn emit_file_change(app: &AppHandle, window_label: &str, payload: Value) {
    if let Some(win) = app.get_webview_window(window_label) {
        let _ = win.emit("file-change", payload);
    } else {
        let _ = app.emit("file-change", payload);
    }
}

pub fn file_start_watcher(
    app: &AppHandle,
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    dir_path: String,
    _config: Option<Value>,
) -> Result<()> {
    let watch_path = normalize_path(&dir_path);
    ensure_dir(&watch_path)?;
    allow_dir(state, &watch_path);
    if !is_allowed(state, &watch_path) {
        return Err(DromeError::Message("Path not allowed".into()));
    }

    // Stop existing watcher first.
    let _ = file_stop_watcher();

    let paused = Arc::new(AtomicBool::new(false));
    let paused_cb = paused.clone();
    let app_cb = app.clone();
    let window_label = window.label().to_string();
    let window_label_cb = window_label.clone();
    let watch_path_s = path_to_string(&watch_path);
    let watch_path_payload_cb = watch_path_s.clone();
    let watch_path_refresh_file = watch_path_s.clone();
    let watch_path_refresh_watch = watch_path_s.clone();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
    if paused_cb.load(Ordering::Relaxed) {
      return;
    }
    let Ok(ev) = res else { return };
    let Some(event_type) = kind_to_event_type(&ev.kind) else { return };
    let watch_path_payload = watch_path_payload_cb.clone();
    for p in ev.paths {
      let file_path = path_to_string(&p);
      emit_file_change(
        &app_cb,
        &window_label_cb,
        serde_json::json!({ "eventType": event_type, "filePath": file_path, "watchPath": watch_path_payload.clone() }),
      );
    }
  })
  .map_err(|e| DromeError::Message(e.to_string()))?;

    watcher
        .watch(&watch_path, RecursiveMode::Recursive)
        .map_err(|e| DromeError::Message(e.to_string()))?;

    if let Ok(mut guard) = watcher_cell().lock() {
        *guard = Some(FileWatcherState {
            watch_path: watch_path_s,
            paused,
            _watcher: watcher,
            app: app.clone(),
            window_label,
        });
    }

    // Initial refresh.
    emit_file_change(
        app,
        window.label(),
        serde_json::json!({ "eventType": "refresh", "filePath": watch_path_refresh_file, "watchPath": watch_path_refresh_watch }),
    );

    Ok(())
}

pub fn file_stop_watcher() -> Result<()> {
    if let Ok(mut guard) = watcher_cell().lock() {
        *guard = None;
    }
    Ok(())
}

pub fn file_pause_watcher() -> Result<()> {
    if let Ok(guard) = watcher_cell().lock() {
        if let Some(w) = guard.as_ref() {
            w.paused.store(true, Ordering::Relaxed);
        }
    }
    Ok(())
}

pub fn file_resume_watcher() -> Result<()> {
    let mut to_emit: Option<(AppHandle, String, String)> = None;
    if let Ok(guard) = watcher_cell().lock() {
        if let Some(w) = guard.as_ref() {
            w.paused.store(false, Ordering::Relaxed);
            to_emit = Some((w.app.clone(), w.window_label.clone(), w.watch_path.clone()));
        }
    }

    if let Some((app, label, watch_path)) = to_emit {
        emit_file_change(
            &app,
            &label,
            serde_json::json!({ "eventType": "refresh", "filePath": watch_path, "watchPath": watch_path }),
        );
    }

    Ok(())
}

pub fn file_batch_upload_markdown(
    state: &State<'_, AppState>,
    file_paths: Vec<String>,
    target_path: String,
) -> Result<BatchUploadMarkdownResult> {
    let base = normalize_path(&target_path);
    if !is_allowed(state, &base) {
        return Err(DromeError::Message("Path not allowed".into()));
    }
    ensure_dir(&base)?;

    let mut skipped = 0u32;
    let mut file_count = 0u32;
    let mut folders_created: std::collections::HashSet<String> = std::collections::HashSet::new();

    for p in file_paths {
        let src = normalize_path(&p);
        let ext = ext_lower(&src);
        if ext != ".md" && ext != ".markdown" {
            skipped += 1;
            continue;
        }
        if !src.exists() || !src.is_file() {
            skipped += 1;
            continue;
        }

        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("note");
        let safe = unique_name(&base, stem, true);
        let dest = base.join(format!("{safe}.md"));
        if let Some(parent) = dest.parent() {
            if folders_created.insert(parent.to_string_lossy().to_string()) {
                let _ = fs::create_dir_all(parent);
            }
        }

        let content = fs::read_to_string(&src)?;
        fs::write(&dest, content.as_bytes())?;
        file_count += 1;
    }

    Ok(BatchUploadMarkdownResult {
        file_count,
        folder_count: folders_created.len() as u32,
        skipped_files: skipped,
    })
}
