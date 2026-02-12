use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Default, Clone)]
pub struct StopQuitState {
  pub enabled: bool,
  pub reason: String,
}

#[derive(Debug)]
pub struct AppState {
  pub app_data_dir: PathBuf,
  pub app_config_dir: PathBuf,
  pub allowed_dirs: Mutex<Vec<PathBuf>>,
  pub stop_quit: Mutex<StopQuitState>,
  pub zoom_factor: Mutex<f64>,
}
