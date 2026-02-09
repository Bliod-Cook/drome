use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use cherry_core::{MessageRole, ProviderConfig};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::provider::{ChatCompletionRequest, ChatCompletionResponse, ChatProvider};

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleClient {
    http: Client,
}

impl Default for OpenAiCompatibleClient {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAiCompatibleClient {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
        }
    }
}

#[async_trait]
impl ChatProvider for OpenAiCompatibleClient {
    async fn complete(
        &self,
        provider: &ProviderConfig,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        let api_key = provider
            .api_key
            .as_deref()
            .ok_or_else(|| anyhow!("provider {} missing api key", provider.name))?;

        let endpoint = format!(
            "{}/chat/completions",
            provider.base_url.trim_end_matches('/')
        );

        let mut messages = Vec::new();

        if let Some(system_prompt) = request.system_prompt.as_deref()
            && !system_prompt.trim().is_empty()
        {
            messages.push(OpenAiMessage {
                role: "system".to_owned(),
                content: system_prompt.to_owned(),
            });
        }

        messages.extend(request.turns.iter().map(|turn| OpenAiMessage {
            role: match turn.role {
                MessageRole::System => "system".to_owned(),
                MessageRole::User => "user".to_owned(),
                MessageRole::Assistant => "assistant".to_owned(),
                MessageRole::Tool => "tool".to_owned(),
            },
            content: turn.content.clone(),
        }));

        let payload = OpenAiChatRequest {
            model: request.selection.model.clone(),
            messages,
            stream: false,
            temperature: request.temperature,
        };

        let response = self
            .http
            .post(endpoint)
            .bearer_auth(api_key)
            .json(&payload)
            .send()
            .await
            .context("failed to request provider")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "provider {} request failed: {} {}",
                provider.name,
                status,
                text
            ));
        }

        let output: OpenAiChatResponse = response
            .json()
            .await
            .context("invalid provider response json")?;

        let content = output
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .unwrap_or_default();

        if content.is_empty() {
            warn!(provider = %provider.name, model = %request.selection.model, "empty completion content");
        }

        Ok(ChatCompletionResponse {
            content,
            provider: provider.name.clone(),
            model: request.selection.model.clone(),
            token_usage: output.usage.and_then(|usage| usage.total_tokens),
        })
    }
}

#[derive(Debug, Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    stream: bool,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiAssistantMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiAssistantMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    total_tokens: Option<u32>,
}
