use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State, WebviewWindow};
use walkdir::WalkDir;

use crate::error::{DromeError, Result};
use crate::state::AppState;

const HOME_CHERRY_DIR: &str = ".cherrystudio";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationDetectResult {
    pub cherry_config_path: String,
    pub detected_old_data_dirs: Vec<String>,
}

fn cherry_config_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(
        home.join(HOME_CHERRY_DIR)
            .join("config")
            .join("config.json"),
    )
}

fn read_cherry_data_paths(config_path: &Path) -> Vec<PathBuf> {
    let Ok(content) = std::fs::read_to_string(config_path) else {
        return vec![];
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return vec![];
    };

    let app_data_path = value.get("appDataPath");
    match app_data_path {
        Some(serde_json::Value::String(s)) => vec![PathBuf::from(s)],
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|item| {
                item.get("dataPath")
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from)
            })
            .collect(),
        _ => vec![],
    }
}

pub fn migration_detect(state: &State<'_, AppState>) -> Result<MigrationDetectResult> {
    let Some(cfg) = cherry_config_path() else {
        return Ok(MigrationDetectResult {
            cherry_config_path: String::new(),
            detected_old_data_dirs: vec![],
        });
    };

    if !cfg.exists() {
        return Ok(MigrationDetectResult {
            cherry_config_path: cfg.to_string_lossy().to_string(),
            detected_old_data_dirs: vec![],
        });
    }

    let mut dirs = read_cherry_data_paths(&cfg);

    // Add common default fallback candidates (best-effort).
    if let Some(home) = dirs::home_dir() {
        #[cfg(target_os = "linux")]
        {
            dirs.push(home.join(".config").join("CherryStudio"));
        }
        #[cfg(target_os = "macos")]
        {
            dirs.push(
                home.join("Library")
                    .join("Application Support")
                    .join("CherryStudio"),
            );
        }
        #[cfg(target_os = "windows")]
        {
            // Unknown in portable scenarios; keep only config-provided paths on Windows.
        }
    }

    dirs.sort();
    dirs.dedup();

    let detected: Vec<String> = dirs
        .into_iter()
        .filter(|p| p.join("Data").exists())
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    Ok(MigrationDetectResult {
        cherry_config_path: cfg.to_string_lossy().to_string(),
        detected_old_data_dirs: detected,
    })
}

fn emit_percent(window: &WebviewWindow, percent: u32) {
    let _ = window.emit(
        "directory-processing-percent",
        serde_json::json!({ "percent": percent }),
    );
}

pub fn migration_copy_data_dir(
    app: &AppHandle,
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    old_user_data_dir: String,
) -> Result<()> {
    let old_dir = PathBuf::from(old_user_data_dir.clone()).join("Data");
    if !old_dir.exists() {
        return Err(DromeError::Message("Old Data directory not found".into()));
    }

    let new_dir = state.app_data_dir.join("Data");
    std::fs::create_dir_all(&new_dir)?;

    let entries: Vec<_> = WalkDir::new(&old_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .collect();
    let total = entries.len().max(1) as u32;
    for (idx, entry) in entries.into_iter().enumerate() {
        let src_path = entry.path();
        let rel = src_path.strip_prefix(&old_dir).unwrap_or(src_path);
        let dest_path = new_dir.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest_path)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(src_path, &dest_path)?;
        }
        if idx % 50 == 0 {
            let percent = (idx as u32 * 100) / total;
            emit_percent(window, percent);
        }
    }
    emit_percent(window, 100);

    // Record migration marker
    let migration_path = state.app_config_dir.join("migration.json");
    let migrated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let marker = serde_json::json!({
      "migratedAt": migrated_at,
      "sourceUserDataDir": old_user_data_dir
    });
    std::fs::create_dir_all(&state.app_config_dir)?;
    std::fs::write(migration_path, serde_json::to_string_pretty(&marker)?)?;

    Ok(())
}
