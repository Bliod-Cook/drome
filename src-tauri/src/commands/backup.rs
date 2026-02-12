use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State, WebviewWindow};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::ZipArchive;
use zip::ZipWriter;

use crate::commands::system;
use crate::error::{DromeError, Result};
use crate::state::AppState;

fn emit_progress(window: &WebviewWindow, channel: &str, stage: &str, progress: u32) {
    let _ = window.emit(
        channel,
        serde_json::json!({ "stage": stage, "progress": progress, "total": 100 }),
    );
}

fn data_dir(state: &State<'_, AppState>) -> PathBuf {
    state.app_data_dir.join("Data")
}

fn tmp_dir(state: &State<'_, AppState>) -> PathBuf {
    state.app_data_dir.join("Temp").join("backup")
}

fn ensure_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    Ok(())
}

fn allow_dir(state: &State<'_, AppState>, dir: &Path) {
    if let Ok(mut dirs) = state.allowed_dirs.lock() {
        if !dirs.iter().any(|d| d == dir) {
            dirs.push(dir.to_path_buf());
        }
    }
    let _ = system::add_allowed_dir_to_store(state, dir);
}

pub fn backup_backup(
    _app: &AppHandle,
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    args: Vec<Value>,
) -> Result<String> {
    // args: filename, dataJsonString, destinationPath, skipBackupFile
    let filename = args
        .get(0)
        .and_then(|v| v.as_str())
        .ok_or_else(|| DromeError::Message("filename required".into()))?;
    let data_json = args
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| DromeError::Message("data required".into()))?;
    let destination = args
        .get(2)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let skip_backup_file = args.get(3).and_then(|v| v.as_bool()).unwrap_or(false);

    let dest_dir = if destination.is_empty() {
        state.app_data_dir.clone()
    } else {
        PathBuf::from(destination)
    };
    ensure_dir(&dest_dir)?;

    let backup_path = dest_dir.join(filename);

    emit_progress(window, "backup-progress", "preparing", 0);

    let file = File::create(&backup_path)?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("data.json", opts)?;
    zip.write_all(data_json.as_bytes())?;

    if !skip_backup_file {
        emit_progress(window, "backup-progress", "copying_files", 20);
        let data_dir = data_dir(state);
        if data_dir.exists() {
            let entries: Vec<_> = WalkDir::new(&data_dir)
                .into_iter()
                .filter_map(|e| e.ok())
                .collect();
            let total = entries.len().max(1) as u32;
            for (idx, entry) in entries.into_iter().enumerate() {
                let path = entry.path();
                if path.is_dir() {
                    continue;
                }
                let rel = path.strip_prefix(&state.app_data_dir).unwrap_or(path);
                let rel_name = rel.to_string_lossy().replace('\\', "/");
                zip.start_file(rel_name, opts)?;
                let mut f = File::open(path)?;
                let mut buf = Vec::new();
                f.read_to_end(&mut buf)?;
                zip.write_all(&buf)?;

                let p = 20 + ((idx as u32 * 70) / total);
                if idx % 50 == 0 {
                    emit_progress(window, "backup-progress", "copying_files", p.min(90));
                }
            }
        }
    }

    zip.finish()?;
    emit_progress(window, "backup-progress", "completed", 100);
    Ok(backup_path.to_string_lossy().to_string())
}

pub fn backup_restore(
    _app: &AppHandle,
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    backup_path: String,
) -> Result<String> {
    let backup_path = PathBuf::from(backup_path);
    if !backup_path.exists() {
        return Err(DromeError::Message("Backup file not found".into()));
    }

    emit_progress(window, "restore-progress", "preparing", 0);

    let tmp = tmp_dir(state);
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp).ok();
    }
    ensure_dir(&tmp)?;
    let tmp_data_root = tmp.join("Data");

    let file = File::open(&backup_path)?;
    let mut archive = ZipArchive::new(file)?;

    emit_progress(window, "restore-progress", "extracting", 10);

    let mut data_json = String::new();
    let mut extracted_any_data = false;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_string();

        if name == "data.json" {
            file.read_to_string(&mut data_json)?;
            continue;
        }

        // Only restore Data/ subtree if present.
        if !name.starts_with("Data/") {
            continue;
        }

        extracted_any_data = true;
        let out_path = tmp.join(&name);
        if let Some(parent) = out_path.parent() {
            ensure_dir(parent)?;
        }
        let mut out = File::create(out_path)?;
        std::io::copy(&mut file, &mut out)?;
    }

    // Replace Data directory atomically (best-effort).
    if extracted_any_data && tmp_data_root.exists() {
        let target_data_dir = state.app_data_dir.join("Data");
        if target_data_dir.exists() {
            std::fs::remove_dir_all(&target_data_dir).ok();
        }
        ensure_dir(&state.app_data_dir)?;
        std::fs::rename(&tmp_data_root, &target_data_dir)?;
    }
    let _ = std::fs::remove_dir_all(&tmp);

    emit_progress(window, "restore-progress", "completed", 100);
    Ok(data_json)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalBackupConfig {
    local_backup_dir: Option<String>,
    skip_backup_file: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFileEntry {
    pub file_name: String,
    pub modified_time: String,
    pub size: u64,
}

pub fn backup_to_local_dir(
    app: &AppHandle,
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    data: String,
    file_name: String,
    local_config: Value,
) -> Result<String> {
    let config: LocalBackupConfig = serde_json::from_value(local_config)
        .map_err(|e| DromeError::Message(format!("Invalid local backup config: {e}")))?;
    let dir = config
        .local_backup_dir
        .clone()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| DromeError::Message("localBackupDir is required".into()))?;

    let dest_dir = PathBuf::from(dir);
    ensure_dir(&dest_dir)?;
    allow_dir(state, &dest_dir);

    let args = vec![
        Value::String(file_name),
        Value::String(data),
        Value::String(dest_dir.to_string_lossy().to_string()),
        Value::Bool(config.skip_backup_file.unwrap_or(false)),
    ];
    backup_backup(app, window, state, args)
}

pub fn restore_from_local_backup(
    app: &AppHandle,
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    file_name: String,
    local_backup_dir: Option<String>,
) -> Result<String> {
    let dir = local_backup_dir
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| DromeError::Message("localBackupDir is required".into()))?;
    let path = PathBuf::from(dir).join(file_name);
    backup_restore(app, window, state, path.to_string_lossy().to_string())
}

pub fn list_local_backup_files(local_backup_dir: Option<String>) -> Result<Vec<BackupFileEntry>> {
    let Some(dir) = local_backup_dir.filter(|s| !s.trim().is_empty()) else {
        return Ok(Vec::new());
    };
    let dir = PathBuf::from(dir);
    if !dir.exists() || !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut out: Vec<BackupFileEntry> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("zip") {
            continue;
        }
        let meta = std::fs::metadata(&path)?;
        let mtime = meta
            .modified()
            .ok()
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        out.push(BackupFileEntry {
            file_name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            modified_time: mtime,
            size: meta.len(),
        });
    }

    out.sort_by(|a, b| b.modified_time.cmp(&a.modified_time));
    Ok(out)
}

pub fn delete_local_backup_file(
    file_name: String,
    local_backup_dir: Option<String>,
) -> Result<bool> {
    let Some(dir) = local_backup_dir.filter(|s| !s.trim().is_empty()) else {
        return Ok(false);
    };
    let path = PathBuf::from(dir).join(file_name);
    if !path.exists() {
        return Ok(false);
    }
    std::fs::remove_file(path)?;
    Ok(true)
}

fn lan_transfer_dir(state: &State<'_, AppState>) -> PathBuf {
    state.app_data_dir.join("Temp").join("lan-transfer")
}

pub fn create_lan_transfer_backup(
    app: &AppHandle,
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    data: String,
) -> Result<String> {
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M").to_string();
    let file_name = format!("cherry-studio.{timestamp}.zip");
    let dir = lan_transfer_dir(state);
    ensure_dir(&dir)?;
    allow_dir(state, &dir);

    let args = vec![
        Value::String(file_name),
        Value::String(data),
        Value::String(dir.to_string_lossy().to_string()),
        Value::Bool(true), // skipBackupFile
    ];
    backup_backup(app, window, state, args)
}

pub fn delete_temp_backup(state: &State<'_, AppState>, file_path: String) -> Result<bool> {
    let base = lan_transfer_dir(state);
    let base = base.canonicalize().unwrap_or(base);
    let target = PathBuf::from(file_path);
    let target = target.canonicalize().unwrap_or(target);

    if !target.starts_with(&base) {
        return Ok(false);
    }

    if target.exists() {
        std::fs::remove_file(target)?;
        return Ok(true);
    }
    Ok(false)
}
