use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplaySettings {
    pub theme: String,
    pub language: String,
    pub font_family: String,
    pub font_size: u8,
    pub sidebar_position: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSettings {
    pub launch_on_boot: bool,
    pub launch_to_tray: bool,
    pub close_to_tray: bool,
    pub enable_spell_check: bool,
    pub auto_update: bool,
    pub use_system_title_bar: bool,
    pub disable_hardware_acceleration: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSettings {
    pub webdav_enabled: bool,
    pub s3_enabled: bool,
    pub lan_transfer_enabled: bool,
    pub auto_backup_interval_minutes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub display: DisplaySettings,
    pub runtime: RuntimeSettings,
    pub backup: BackupSettings,
    pub proxy: Option<String>,
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            display: DisplaySettings {
                theme: "system".to_owned(),
                language: "en-US".to_owned(),
                font_family: "System".to_owned(),
                font_size: 14,
                sidebar_position: "left".to_owned(),
            },
            runtime: RuntimeSettings {
                launch_on_boot: false,
                launch_to_tray: false,
                close_to_tray: true,
                enable_spell_check: true,
                auto_update: true,
                use_system_title_bar: false,
                disable_hardware_acceleration: false,
            },
            backup: BackupSettings {
                webdav_enabled: false,
                s3_enabled: false,
                lan_transfer_enabled: false,
                auto_backup_interval_minutes: None,
            },
            proxy: None,
            default_provider: Some("OpenAI".to_owned()),
            default_model: Some("gpt-4o-mini".to_owned()),
        }
    }
}
