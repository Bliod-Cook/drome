use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiServerConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub token: Option<String>,
}

impl Default for ApiServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "127.0.0.1".to_owned(),
            port: 8787,
            token: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApiServerRuntimeStatus {
    Stopped,
    Running,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerDefinition {
    pub id: Uuid,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub enabled: bool,
}

impl McpServerDefinition {
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            command: command.into(),
            args: Vec::new(),
            env: Vec::new(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCallResult {
    pub server_name: String,
    pub tool_name: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPermission {
    pub id: Uuid,
    pub tool_name: String,
    pub allowed: bool,
    pub scope: String,
}

impl ToolPermission {
    pub fn new(tool_name: impl Into<String>, allowed: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            tool_name: tool_name.into(),
            allowed,
            scope: "global".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolHandler {
    pub id: Uuid,
    pub scheme: String,
    pub command: String,
    pub args: Vec<String>,
    pub enabled: bool,
}

impl ProtocolHandler {
    pub fn new(scheme: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            scheme: scheme.into(),
            command: command.into(),
            args: Vec::new(),
            enabled: true,
        }
    }
}
