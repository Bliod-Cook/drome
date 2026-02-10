use serde_json::Value;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State, WebviewWindow};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::ZipArchive;
use zip::ZipWriter;

use crate::error::{DromeError, Result};
use crate::state::AppState;

fn emit_progress(window: &WebviewWindow, channel: &str, stage: &str, progress: u32) {
  let _ = window.emit(channel, serde_json::json!({ "stage": stage, "progress": progress, "total": 100 }));
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

pub fn backup_backup(_app: &AppHandle, window: &WebviewWindow, state: &State<'_, AppState>, args: Vec<Value>) -> Result<String> {
  // args: filename, dataJsonString, destinationPath, skipBackupFile
  let filename = args.get(0).and_then(|v| v.as_str()).ok_or_else(|| DromeError::Message("filename required".into()))?;
  let data_json = args.get(1).and_then(|v| v.as_str()).ok_or_else(|| DromeError::Message("data required".into()))?;
  let destination = args.get(2).and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_default();
  let skip_backup_file = args.get(3).and_then(|v| v.as_bool()).unwrap_or(false);

  let dest_dir = if destination.is_empty() { state.app_data_dir.clone() } else { PathBuf::from(destination) };
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
      let entries: Vec<_> = WalkDir::new(&data_dir).into_iter().filter_map(|e| e.ok()).collect();
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

pub fn backup_restore(_app: &AppHandle, window: &WebviewWindow, state: &State<'_, AppState>, backup_path: String) -> Result<String> {
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
