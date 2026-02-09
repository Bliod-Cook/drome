use std::pin::Pin;

use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub type SessionId = Uuid;
pub type MessageId = Uuid;
pub type ToolCallId = String;
pub type UnifiedEventStream = Pin<Box<dyn Stream<Item = Result<UnifiedEvent>> + Send + 'static>>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum UiLanguage {
    ZhCn,
    EnUs,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    OpenAi,
    Anthropic,
    Gemini,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct SecretRef {
    pub namespace: String,
    pub key: String,
}

impl SecretRef {
    pub fn new(namespace: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            key: key.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: ProviderId,
    pub base_url: String,
    pub api_key_ref: SecretRef,
    pub default_model: String,
    #[serde(default)]
    pub extra_headers: Vec<(String, String)>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

const fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub streaming: bool,
    pub tools: bool,
    pub reasoning: bool,
    pub prompt_caching: bool,
    pub custom_base_url: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UnifiedRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMessage {
    pub id: MessageId,
    pub role: UnifiedRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<ToolCallId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_arguments_json: Option<String>,
}

impl UnifiedMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: MessageId::new_v4(),
            role: UnifiedRole::User,
            content: content.into(),
            tool_call_id: None,
            tool_name: None,
            tool_arguments_json: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            id: MessageId::new_v4(),
            role: UnifiedRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_name: None,
            tool_arguments_json: None,
        }
    }

    pub fn tool_result(
        tool_call_id: impl Into<String>,
        content: impl Into<String>,
        is_error: bool,
    ) -> Self {
        let prefix = if is_error { "error: " } else { "" };
        Self {
            id: MessageId::new_v4(),
            role: UnifiedRole::Tool,
            content: format!("{prefix}{}", content.into()),
            tool_call_id: Some(tool_call_id.into()),
            tool_name: None,
            tool_arguments_json: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub input_schema: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedGenerateRequest {
    pub session_id: SessionId,
    pub model: String,
    pub messages: Vec<UnifiedMessage>,
    pub tools: Vec<ToolSpec>,
    pub stream: bool,
    #[serde(default)]
    pub provider_options: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UnifiedEvent {
    TextDelta {
        text: String,
    },
    ReasoningDelta {
        text: String,
    },
    ToolCallRequested {
        call_id: ToolCallId,
        name: String,
        arguments_json: String,
    },
    ToolCallResult {
        call_id: ToolCallId,
        output: String,
        is_error: bool,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
        total_tokens: Option<u64>,
    },
    Completed,
    Failed {
        code: String,
        message: String,
        retriable: bool,
    },
}

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    async fn stream_generate(
        &self,
        provider: &ProviderConfig,
        api_key: &str,
        request: UnifiedGenerateRequest,
    ) -> Result<UnifiedEventStream>;

    fn capabilities(&self, provider: ProviderId) -> ProviderCapabilities;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportConfig {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(default)]
        env: Vec<(String, String)>,
    },
    Sse {
        url: String,
        #[serde(default)]
        headers: Vec<(String, String)>,
    },
    StreamableHttp {
        url: String,
        #[serde(default)]
        headers: Vec<(String, String)>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub transport: McpTransportConfig,
    pub timeout_ms: u64,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSpec {
    pub uri: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    pub uri: String,
    pub text: Option<String>,
    pub blob_base64: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptContent {
    pub description: Option<String>,
    #[serde(default)]
    pub messages: Vec<UnifiedMessage>,
}

#[async_trait]
pub trait McpClient: Send + Sync {
    async fn upsert_server(&self, server: McpServerConfig) -> Result<()>;
    async fn remove_server(&self, server_id: &str) -> Result<()>;
    async fn healthcheck(&self, server_id: &str) -> Result<()>;
    async fn list_tools(&self, server_id: &str) -> Result<Vec<ToolSpec>>;
    async fn call_tool(&self, server_id: &str, name: &str, args_json: &str) -> Result<String>;
    async fn list_resources(&self, server_id: &str) -> Result<Vec<ResourceSpec>>;
    async fn read_resource(&self, server_id: &str, uri: &str) -> Result<ResourceContent>;
    async fn list_prompts(&self, server_id: &str) -> Result<Vec<PromptSpec>>;
    async fn get_prompt(
        &self,
        server_id: &str,
        name: &str,
        args_json: &str,
    ) -> Result<PromptContent>;
}
