use serde::Serialize;
use std::path::PathBuf;

use crate::error::Result;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInstallInfo {
  pub dir: String,
  pub uv_path: String,
  pub bun_path: String,
}

fn binary_name(name: &str) -> String {
  if cfg!(target_os = "windows") {
    format!("{name}.exe")
  } else {
    name.to_string()
  }
}

pub fn mcp_get_install_info() -> Result<McpInstallInfo> {
  let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
  let dir = home.join(".cherrystudio").join("bin");

  let uv_path = dir.join(binary_name("uv"));
  let bun_path = dir.join(binary_name("bun"));

  Ok(McpInstallInfo {
    dir: dir.to_string_lossy().to_string(),
    uv_path: uv_path.to_string_lossy().to_string(),
    bun_path: bun_path.to_string_lossy().to_string(),
  })
}

