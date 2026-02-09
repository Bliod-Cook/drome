use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use core_types::{McpServerConfig, ProviderConfig, ProviderId, SecretRef, UiLanguage};
use serde::{Deserialize, Serialize};
use tracing::warn;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub local_encryption_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    pub language: UiLanguage,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default)]
    pub security: SecurityConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            language: UiLanguage::ZhCn,
            providers: vec![
                ProviderConfig {
                    id: ProviderId::OpenAi,
                    base_url: "https://api.openai.com/v1".to_string(),
                    api_key_ref: SecretRef::new("provider", "openai_api_key"),
                    default_model: "gpt-4.1-mini".to_string(),
                    extra_headers: Vec::new(),
                    enabled: true,
                },
                ProviderConfig {
                    id: ProviderId::Anthropic,
                    base_url: "https://api.anthropic.com".to_string(),
                    api_key_ref: SecretRef::new("provider", "anthropic_api_key"),
                    default_model: "claude-sonnet-4".to_string(),
                    extra_headers: Vec::new(),
                    enabled: true,
                },
                ProviderConfig {
                    id: ProviderId::Gemini,
                    base_url: "https://generativelanguage.googleapis.com".to_string(),
                    api_key_ref: SecretRef::new("provider", "gemini_api_key"),
                    default_model: "gemini-2.5-flash".to_string(),
                    extra_headers: Vec::new(),
                    enabled: true,
                },
            ],
            mcp_servers: Vec::new(),
            security: SecurityConfig {
                local_encryption_enabled: false,
            },
        }
    }
}

pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn from_dir(dir: impl Into<PathBuf>) -> Self {
        Self {
            path: dir.into().join("config.json"),
        }
    }

    pub fn from_default_location() -> Result<Self> {
        let mut dir = dirs::config_dir().context("failed to resolve config_dir")?;
        dir.push("drome");
        Ok(Self::from_dir(dir))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_or_init(&self) -> Result<AppConfig> {
        if !self.path.exists() {
            let config = AppConfig::default();
            self.save(&config)?;
            return Ok(config);
        }

        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        let mut config: AppConfig =
            serde_json::from_str(&raw).context("failed to parse app config json")?;
        self.migrate(&mut config);
        self.save(&config)?;
        Ok(config)
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let text = serde_json::to_string_pretty(config).context("failed to serialize config")?;
        fs::write(&self.path, text)
            .with_context(|| format!("failed to write {}", self.path.display()))?;
        Ok(())
    }

    fn migrate(&self, config: &mut AppConfig) {
        if config.schema_version >= CURRENT_SCHEMA_VERSION {
            return;
        }

        warn!(
            from = config.schema_version,
            to = CURRENT_SCHEMA_VERSION,
            "migrating app config schema"
        );

        if config.providers.is_empty() {
            config.providers = AppConfig::default().providers;
        }
        config.schema_version = CURRENT_SCHEMA_VERSION;
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn creates_default_config_when_missing() {
        let dir = tempdir().expect("tempdir");
        let store = ConfigStore::from_dir(dir.path());
        let config = store.load_or_init().expect("load default");
        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(config.providers.len(), 3);
    }
}
