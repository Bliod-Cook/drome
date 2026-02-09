use std::{collections::HashMap, sync::Arc};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use cherry_core::{MessageRole, ProviderConfig, ProviderKind};

#[derive(Debug, Clone)]
pub struct ChatTurn {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeModelSelection {
    pub provider_name: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct ChatCompletionRequest {
    pub selection: RuntimeModelSelection,
    pub system_prompt: Option<String>,
    pub turns: Vec<ChatTurn>,
    pub temperature: f32,
}

#[derive(Debug, Clone)]
pub struct ChatCompletionResponse {
    pub content: String,
    pub provider: String,
    pub model: String,
    pub token_usage: Option<u32>,
}

#[async_trait]
pub trait ChatProvider: Send + Sync {
    async fn complete(
        &self,
        provider: &ProviderConfig,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse>;
}

#[derive(Default)]
pub struct ProviderRegistry {
    providers: HashMap<ProviderKind, Arc<dyn ChatProvider>>,
}

impl ProviderRegistry {
    pub fn register(&mut self, kind: ProviderKind, provider: Arc<dyn ChatProvider>) {
        self.providers.insert(kind, provider);
    }

    pub async fn complete(
        &self,
        provider_config: &ProviderConfig,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        let provider = self
            .providers
            .get(&provider_config.kind)
            .ok_or_else(|| anyhow!("provider {:?} not registered", provider_config.kind))?;

        provider.complete(provider_config, request).await
    }
}
