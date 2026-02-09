use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    OpenAiCompatible,
    Anthropic,
    Gemini,
    OpenRouter,
    Ollama,
    LmStudio,
    SiliconFlow,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub id: String,
    pub display_name: String,
    pub context_window: Option<u32>,
    pub supports_vision: bool,
    pub supports_tools: bool,
    pub supports_reasoning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: Uuid,
    pub kind: ProviderKind,
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub enabled: bool,
    pub models: Vec<ModelProfile>,
}

impl ProviderConfig {
    pub fn openai_default() -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: ProviderKind::OpenAiCompatible,
            name: "OpenAI".to_owned(),
            base_url: "https://api.openai.com/v1".to_owned(),
            api_key: None,
            enabled: true,
            models: vec![ModelProfile {
                id: "gpt-4o-mini".to_owned(),
                display_name: "GPT-4o mini".to_owned(),
                context_window: Some(128_000),
                supports_vision: true,
                supports_tools: true,
                supports_reasoning: true,
            }],
        }
    }
}
