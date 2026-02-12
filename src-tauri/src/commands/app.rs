use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_shell::open::open;

use crate::commands;
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

    let resources_dir = app
        .path()
        .resource_dir()
        .unwrap_or_else(|_| app_data_path.clone());
    let logs_dir = app
        .path()
        .app_log_dir()
        .unwrap_or_else(|_| app_data_path.join("Logs"));
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
    window
        .eval("window.location.reload()")
        .map_err(|e| DromeError::Message(e.to_string()))?;
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

    let size = fs2::total_space(&path).map_err(|e| DromeError::Message(e.to_string()))?;
    let free = fs2::available_space(&path).map_err(|e| DromeError::Message(e.to_string()))?;

    Ok(Some(serde_json::json!({ "size": size, "free": free })))
}

pub fn app_get_data_path_from_args() -> Result<Option<String>> {
    for arg in std::env::args() {
        if let Some(rest) = arg.strip_prefix("--new-data-path=") {
            if !rest.trim().is_empty() {
                return Ok(Some(rest.to_string()));
            }
        }
    }
    Ok(None)
}

pub fn app_select(
    app: &AppHandle,
    state: &State<'_, AppState>,
    options: Option<Value>,
) -> Result<Option<String>> {
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

    let can_create = props.iter().any(|p| p == "createDirectory");
    let open_dir = props.iter().any(|p| p == "openDirectory");

    let mut dialog = tauri_plugin_dialog::DialogExt::dialog(app).file();
    if let Some(t) = title {
        dialog = dialog.set_title(t);
    }
    if let Some(dir) = default_path {
        dialog = dialog.set_directory(dir);
    }
    if can_create {
        dialog = dialog.set_can_create_directories(true);
    }

    #[cfg(desktop)]
    let selected = if open_dir {
        dialog.blocking_pick_folder()
    } else {
        dialog.blocking_pick_file()
    };

    #[cfg(not(desktop))]
    let selected = dialog.blocking_pick_file();

    let path = match selected {
        Some(p) => p
            .into_path()
            .map_err(|e| DromeError::Message(e.to_string()))?,
        None => return Ok(None),
    };

    // Allow accessing the selected directory for subsequent operations.
    if let Ok(mut dirs) = state.allowed_dirs.lock() {
        let allowed = if path.is_dir() {
            path.clone()
        } else {
            path.parent().unwrap_or(&path).to_path_buf()
        };
        if !dirs.iter().any(|d| d == &allowed) {
            dirs.push(allowed);
        }
    }

    Ok(Some(path.to_string_lossy().to_string()))
}

pub fn app_is_not_empty_dir(path: String) -> Result<bool> {
    let path = normalize_path(&path);
    if !path.exists() || !path.is_dir() {
        return Ok(false);
    }
    Ok(path.read_dir()?.next().is_some())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyResult {
    pub success: bool,
    pub error: Option<String>,
}

fn is_inside_any(path: &std::path::Path, excluded: &[PathBuf]) -> bool {
    excluded.iter().any(|ex| path.starts_with(ex))
}

fn emit_copy_percent(window: &WebviewWindow, percent: u32) {
    let _ = window.emit(
        "directory-processing-percent",
        serde_json::json!({ "percent": percent }),
    );
}

pub fn app_copy(
    window: &WebviewWindow,
    old_path: String,
    new_path: String,
    occupied_dirs: Vec<String>,
) -> Result<CopyResult> {
    let old_dir = normalize_path(&old_path);
    let new_dir = normalize_path(&new_path);

    if !old_dir.exists() || !old_dir.is_dir() {
        return Ok(CopyResult {
            success: false,
            error: Some("Original path is not a directory".into()),
        });
    }

    std::fs::create_dir_all(&new_dir)?;

    let excluded: Vec<PathBuf> = occupied_dirs
        .into_iter()
        .map(|p| normalize_path(&p))
        .collect();

    // Count first (avoid storing huge entry lists in memory).
    let mut total: u64 = 0;
    for entry in walkdir::WalkDir::new(&old_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let p = entry.path();
        if is_inside_any(p, &excluded) {
            continue;
        }
        total += 1;
    }
    if total == 0 {
        total = 1;
    }

    let mut idx: u64 = 0;
    emit_copy_percent(window, 0);
    for entry in walkdir::WalkDir::new(&old_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let src = entry.path();
        if is_inside_any(src, &excluded) {
            continue;
        }

        let rel = src.strip_prefix(&old_dir).unwrap_or(src);
        let dest = new_dir.join(rel);

        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(src, &dest)?;
        }

        idx += 1;
        if idx % 50 == 0 {
            let percent = ((idx * 100) / total) as u32;
            emit_copy_percent(window, percent.min(99));
        }
    }
    emit_copy_percent(window, 100);

    Ok(CopyResult {
        success: true,
        error: None,
    })
}

pub fn app_set_app_data_path(state: &State<'_, AppState>, new_path: String) -> Result<()> {
    let new_dir = normalize_path(&new_path);
    std::fs::create_dir_all(&new_dir)?;
    std::fs::create_dir_all(new_dir.join("Data").join("Files"))?;
    std::fs::create_dir_all(new_dir.join("Data").join("Notes"))?;

    let path = store_path(state);
    let mut map = read_store(&path)?;
    map.insert(
        "appDataPath".into(),
        Value::String(new_dir.to_string_lossy().to_string()),
    );
    write_store(&path, &map)?;

    Ok(())
}

pub fn app_set_stop_quit_app(
    state: &State<'_, AppState>,
    stop: bool,
    reason: String,
) -> Result<()> {
    if let Ok(mut s) = state.stop_quit.lock() {
        s.enabled = stop;
        s.reason = reason;
    }
    Ok(())
}

pub fn app_flush_app_data(app: &AppHandle) -> Result<()> {
    // Best-effort: ask renderer to persist redux/db state.
    let _ = app.emit("app:save-data", serde_json::json!({}));
    Ok(())
}

pub fn app_relaunch_app(app: &AppHandle, options: Option<Value>) -> Result<()> {
    let extra_args: Vec<String> = options
        .as_ref()
        .and_then(|v| v.get("args"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut args_os: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();

    // Remove migration-only args to avoid loops.
    args_os.retain(|a| {
        let s = a.to_string_lossy();
        !s.starts_with("--new-data-path=")
    });

    for a in extra_args {
        args_os.push(std::ffi::OsString::from(a));
    }

    let binary = tauri::process::current_binary(&app.env())?;
    Command::new(binary).args(args_os).spawn()?;

    app.exit(0);
    Ok(())
}

pub fn app_set_full_screen(window: &WebviewWindow, value: bool) -> Result<()> {
    window
        .set_fullscreen(value)
        .map_err(|e| DromeError::Message(e.to_string()))?;
    let _ = window.emit("fullscreen-status-changed", value);
    Ok(())
}

pub fn app_is_full_screen(window: &WebviewWindow) -> Result<bool> {
    window
        .is_fullscreen()
        .map_err(|e| DromeError::Message(e.to_string()))
}

const THEME_CONFIG_KEY: &str = "theme";
const THEME_UPDATED_CHANNEL: &str = "theme:updated";
const THEME_DARK: &str = "dark";
const THEME_LIGHT: &str = "light";
const THEME_SYSTEM: &str = "system";

fn normalize_theme_mode(theme: &str) -> &'static str {
    match theme {
        THEME_DARK => THEME_DARK,
        THEME_LIGHT => THEME_LIGHT,
        _ => THEME_SYSTEM,
    }
}

fn to_tauri_theme(theme: &str) -> Option<tauri::Theme> {
    match theme {
        THEME_DARK => Some(tauri::Theme::Dark),
        THEME_LIGHT => Some(tauri::Theme::Light),
        _ => None,
    }
}

fn theme_payload_from_window(window: &WebviewWindow, requested_theme: &str) -> String {
    if requested_theme == THEME_DARK || requested_theme == THEME_LIGHT {
        return requested_theme.to_string();
    }

    window
        .theme()
        .map(|theme| {
            if theme == tauri::Theme::Dark {
                THEME_DARK.to_string()
            } else {
                THEME_LIGHT.to_string()
            }
        })
        .unwrap_or_else(|_| THEME_LIGHT.to_string())
}

pub fn app_set_theme(app: &AppHandle, state: &State<'_, AppState>, theme: String) -> Result<()> {
    let normalized_theme = normalize_theme_mode(&theme);
    let tauri_theme = to_tauri_theme(normalized_theme);

    commands::config::config_set(
        state,
        app,
        THEME_CONFIG_KEY.to_string(),
        Value::String(normalized_theme.to_string()),
        false,
    )?;

    let mut actual_theme: Option<String> = None;
    for (_label, window) in app.webview_windows() {
        let _ = window.set_theme(tauri_theme.clone());
        if actual_theme.is_none() {
            actual_theme = Some(theme_payload_from_window(&window, normalized_theme));
        }
    }

    let payload = actual_theme.unwrap_or_else(|| {
        if normalized_theme == THEME_SYSTEM {
            THEME_LIGHT.to_string()
        } else {
            normalized_theme.to_string()
        }
    });
    let _ = app.emit(THEME_UPDATED_CHANNEL, payload);

    Ok(())
}
