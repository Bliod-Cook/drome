use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug)]
pub struct AppState {
  pub app_data_dir: PathBuf,
  pub app_config_dir: PathBuf,
  pub allowed_dirs: Mutex<Vec<PathBuf>>,
}

