use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use core_types::{
    ProviderAdapter, ProviderCapabilities, ProviderConfig, ProviderId, UnifiedEvent,
    UnifiedEventStream, UnifiedGenerateRequest, UnifiedMessage, UnifiedRole,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Map, Value, json};

pub struct ZedProviderAdapter {
    client: reqwest::Client,
}

impl Default for ZedProviderAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ZedProviderAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ProviderAdapter for ZedProviderAdapter {
    async fn stream_generate(
        &self,
        provider: &ProviderConfig,
        api_key: &str,
        request: UnifiedGenerateRequest,
    ) -> Result<UnifiedEventStream> {
        let payloads = match provider.id {
            ProviderId::OpenAi => {
                if request
                    .provider_options
                    .get("endpoint")
                    .and_then(Value::as_str)
                    .unwrap_or("responses")
                    == "chat_completions"
                {
                    self.call_openai_chat(provider, api_key, &request).await?
                } else {
                    self.call_openai_responses(provider, api_key, &request)
                        .await?
                }
            }
            ProviderId::Anthropic => self.call_anthropic(provider, api_key, &request).await?,
            ProviderId::Gemini => self.call_gemini(provider, api_key, &request).await?,
        };

        let mut events = Vec::<Result<UnifiedEvent>>::new();
        for payload in payloads {
            events.extend(map_payload_to_events(provider.id, payload)?);
        }
        if !events
            .iter()
            .any(|e| matches!(e, Ok(UnifiedEvent::Completed)))
        {
            events.push(Ok(UnifiedEvent::Completed));
        }

        Ok(Box::pin(futures::stream::iter(events)))
    }

    fn capabilities(&self, provider: ProviderId) -> ProviderCapabilities {
        match provider {
            ProviderId::OpenAi => ProviderCapabilities {
                streaming: true,
                tools: true,
                reasoning: true,
                prompt_caching: true,
                custom_base_url: true,
            },
            ProviderId::Anthropic => ProviderCapabilities {
                streaming: true,
                tools: true,
                reasoning: true,
                prompt_caching: true,
                custom_base_url: true,
            },
            ProviderId::Gemini => ProviderCapabilities {
                streaming: true,
                tools: true,
                reasoning: true,
                prompt_caching: false,
                custom_base_url: true,
            },
        }
    }
}

impl ZedProviderAdapter {
    async fn call_openai_chat(
        &self,
        provider: &ProviderConfig,
        api_key: &str,
        request: &UnifiedGenerateRequest,
    ) -> Result<Vec<Value>> {
        let body = json!({
            "model": request.model,
            "messages": to_openai_chat_messages(&request.messages),
            "tools": to_openai_tools(&request.tools),
            "parallel_tool_calls": true,
            "stream": true,
            "temperature": request.provider_options.get("temperature"),
            "top_p": request.provider_options.get("top_p"),
            "max_completion_tokens": request.provider_options.get("max_tokens"),
        });
        let url = format!(
            "{}/chat/completions",
            provider.base_url.trim_end_matches('/')
        );
        let text = self
            .post_json_sse(&url, api_key, &provider.extra_headers, body)
            .await?;
        parse_sse_json(&text)
    }

    async fn call_openai_responses(
        &self,
        provider: &ProviderConfig,
        api_key: &str,
        request: &UnifiedGenerateRequest,
    ) -> Result<Vec<Value>> {
        let body = json!({
            "model": request.model,
            "input": to_openai_responses_input(&request.messages),
            "tools": to_openai_responses_tools(&request.tools),
            "parallel_tool_calls": true,
            "stream": true,
            "temperature": request.provider_options.get("temperature"),
            "top_p": request.provider_options.get("top_p"),
            "max_output_tokens": request.provider_options.get("max_tokens"),
        });
        let url = format!("{}/responses", provider.base_url.trim_end_matches('/'));
        let text = self
            .post_json_sse(&url, api_key, &provider.extra_headers, body)
            .await?;
        parse_sse_json(&text)
    }

    async fn call_anthropic(
        &self,
        provider: &ProviderConfig,
        api_key: &str,
        request: &UnifiedGenerateRequest,
    ) -> Result<Vec<Value>> {
        let body = json!({
            "model": request.model,
            "max_tokens": request.provider_options.get("max_tokens").and_then(Value::as_u64).unwrap_or(4096),
            "messages": to_anthropic_messages(&request.messages),
            "tools": to_anthropic_tools(&request.tools),
            "stream": true,
            "temperature": request.provider_options.get("temperature"),
            "top_p": request.provider_options.get("top_p"),
        });
        let url = format!("{}/v1/messages", provider.base_url.trim_end_matches('/'));
        let mut headers = provider.extra_headers.clone();
        headers.push(("anthropic-version".to_string(), "2023-06-01".to_string()));
        let text = self.post_json_sse(&url, api_key, &headers, body).await?;
        parse_sse_json(&text)
    }

    async fn call_gemini(
        &self,
        provider: &ProviderConfig,
        api_key: &str,
        request: &UnifiedGenerateRequest,
    ) -> Result<Vec<Value>> {
        let model = request.model.trim();
        if model.is_empty() {
            bail!("gemini model is required");
        }
        let body = json!({
            "contents": to_gemini_contents(&request.messages),
            "tools": to_gemini_tools(&request.tools),
            "generationConfig": {
                "temperature": request.provider_options.get("temperature"),
                "topP": request.provider_options.get("top_p"),
                "maxOutputTokens": request.provider_options.get("max_tokens"),
            }
        });
        let url = format!(
            "{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            provider.base_url.trim_end_matches('/'),
            model,
            api_key.trim()
        );
        let text = self
            .post_json_sse_no_auth(&url, &provider.extra_headers, body)
            .await?;
        parse_sse_json(&text)
    }

    async fn post_json_sse(
        &self,
        url: &str,
        api_key: &str,
        extra_headers: &[(String, String)],
        body: Value,
    ) -> Result<String> {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key.trim()))
                .context("invalid authorization header")?,
        );
        apply_extra_headers(&mut headers, extra_headers)?;
        let response = self
            .client
            .post(url)
            .headers(headers)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            bail!("provider request failed: {status} {text}");
        }
        Ok(text)
    }

    async fn post_json_sse_no_auth(
        &self,
        url: &str,
        extra_headers: &[(String, String)],
        body: Value,
    ) -> Result<String> {
        let mut headers = HeaderMap::new();
        apply_extra_headers(&mut headers, extra_headers)?;
        let response = self
            .client
            .post(url)
            .headers(headers)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            bail!("provider request failed: {status} {text}");
        }
        Ok(text)
    }
}

fn apply_extra_headers(headers: &mut HeaderMap, extra_headers: &[(String, String)]) -> Result<()> {
    for (key, value) in extra_headers {
        let name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|_| anyhow!("invalid header name: {key}"))?;
        let value =
            HeaderValue::from_str(value).map_err(|_| anyhow!("invalid header value for {key}"))?;
        headers.insert(name, value);
    }
    Ok(())
}

fn parse_sse_json(text: &str) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("data:") {
            continue;
        }
        let payload = line.trim_start_matches("data:").trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        out.push(serde_json::from_str(payload)?);
    }
    Ok(out)
}

fn map_payload_to_events(
    provider: ProviderId,
    payload: Value,
) -> Result<Vec<Result<UnifiedEvent>>> {
    let mut events = Vec::new();
    match provider {
        ProviderId::OpenAi => {
            if let Some(kind) = payload.get("type").and_then(Value::as_str) {
                match kind {
                    "response.output_text.delta" => {
                        if let Some(delta) = payload.get("delta").and_then(Value::as_str) {
                            events.push(Ok(UnifiedEvent::TextDelta {
                                text: delta.to_string(),
                            }));
                        }
                    }
                    "response.function_call_arguments.done" => {
                        let call_id = payload
                            .get("item_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let args = payload
                            .get("arguments")
                            .and_then(Value::as_str)
                            .unwrap_or("{}")
                            .to_string();
                        events.push(Ok(UnifiedEvent::ToolCallRequested {
                            call_id,
                            name: "function".to_string(),
                            arguments_json: args,
                        }));
                    }
                    "response.completed" => events.push(Ok(UnifiedEvent::Completed)),
                    _ => {}
                }
            } else {
                if let Some(choices) = payload.get("choices").and_then(Value::as_array) {
                    for choice in choices {
                        if let Some(delta) = choice.get("delta") {
                            if let Some(text) = delta.get("content").and_then(Value::as_str) {
                                events.push(Ok(UnifiedEvent::TextDelta {
                                    text: text.to_string(),
                                }));
                            }
                            if let Some(reasoning) =
                                delta.get("reasoning_content").and_then(Value::as_str)
                            {
                                events.push(Ok(UnifiedEvent::ReasoningDelta {
                                    text: reasoning.to_string(),
                                }));
                            }
                            if let Some(tool_calls) =
                                delta.get("tool_calls").and_then(Value::as_array)
                            {
                                for call in tool_calls {
                                    let call_id = call
                                        .get("id")
                                        .and_then(Value::as_str)
                                        .unwrap_or("call")
                                        .to_string();
                                    let name = call
                                        .get("function")
                                        .and_then(|f| f.get("name"))
                                        .and_then(Value::as_str)
                                        .unwrap_or("function")
                                        .to_string();
                                    let arguments_json = call
                                        .get("function")
                                        .and_then(|f| f.get("arguments"))
                                        .and_then(Value::as_str)
                                        .unwrap_or("{}")
                                        .to_string();
                                    events.push(Ok(UnifiedEvent::ToolCallRequested {
                                        call_id,
                                        name,
                                        arguments_json,
                                    }));
                                }
                            }
                        }
                    }
                }
                if let Some(usage) = payload.get("usage") {
                    events.push(Ok(UnifiedEvent::Usage {
                        input_tokens: usage
                            .get("prompt_tokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                        output_tokens: usage
                            .get("completion_tokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                        total_tokens: usage.get("total_tokens").and_then(Value::as_u64),
                    }));
                }
            }
        }
        ProviderId::Anthropic => {
            if let Some(kind) = payload.get("type").and_then(Value::as_str) {
                match kind {
                    "content_block_delta" => {
                        if let Some(text) = payload
                            .get("delta")
                            .and_then(|d| d.get("text"))
                            .and_then(Value::as_str)
                        {
                            events.push(Ok(UnifiedEvent::TextDelta {
                                text: text.to_string(),
                            }));
                        }
                        if let Some(text) = payload
                            .get("delta")
                            .and_then(|d| d.get("thinking"))
                            .and_then(Value::as_str)
                        {
                            events.push(Ok(UnifiedEvent::ReasoningDelta {
                                text: text.to_string(),
                            }));
                        }
                    }
                    "content_block_start" => {
                        let call_id = payload
                            .get("content_block")
                            .and_then(|c| c.get("id"))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        let name = payload
                            .get("content_block")
                            .and_then(|c| c.get("name"))
                            .and_then(Value::as_str)
                            .unwrap_or("function")
                            .to_string();
                        if !call_id.is_empty() {
                            let arguments_json = payload
                                .get("content_block")
                                .and_then(|c| c.get("input"))
                                .map(ToString::to_string)
                                .unwrap_or_else(|| "{}".to_string());
                            events.push(Ok(UnifiedEvent::ToolCallRequested {
                                call_id,
                                name,
                                arguments_json,
                            }));
                        }
                    }
                    "message_delta" => {
                        if let Some(usage) = payload.get("usage") {
                            events.push(Ok(UnifiedEvent::Usage {
                                input_tokens: usage
                                    .get("input_tokens")
                                    .and_then(Value::as_u64)
                                    .unwrap_or(0),
                                output_tokens: usage
                                    .get("output_tokens")
                                    .and_then(Value::as_u64)
                                    .unwrap_or(0),
                                total_tokens: None,
                            }));
                        }
                    }
                    "message_stop" => events.push(Ok(UnifiedEvent::Completed)),
                    "error" => {
                        let message = payload
                            .get("error")
                            .and_then(|e| e.get("message"))
                            .and_then(Value::as_str)
                            .unwrap_or("anthropic error");
                        events.push(Ok(UnifiedEvent::Failed {
                            code: "anthropic_error".to_string(),
                            message: message.to_string(),
                            retriable: false,
                        }));
                    }
                    _ => {}
                }
            }
        }
        ProviderId::Gemini => {
            if let Some(candidates) = payload.get("candidates").and_then(Value::as_array) {
                for candidate in candidates {
                    if let Some(parts) = candidate
                        .get("content")
                        .and_then(|c| c.get("parts"))
                        .and_then(Value::as_array)
                    {
                        for part in parts {
                            if let Some(text) = part.get("text").and_then(Value::as_str) {
                                events.push(Ok(UnifiedEvent::TextDelta {
                                    text: text.to_string(),
                                }));
                            }
                            if let Some(function_call) = part.get("functionCall") {
                                let name = function_call
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or("function")
                                    .to_string();
                                let arguments_json = function_call
                                    .get("args")
                                    .map(ToString::to_string)
                                    .unwrap_or_else(|| "{}".to_string());
                                events.push(Ok(UnifiedEvent::ToolCallRequested {
                                    call_id: format!("gemini_{name}"),
                                    name,
                                    arguments_json,
                                }));
                            }
                        }
                    }
                }
            }
            if let Some(usage) = payload.get("usageMetadata") {
                events.push(Ok(UnifiedEvent::Usage {
                    input_tokens: usage
                        .get("promptTokenCount")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    output_tokens: usage
                        .get("candidatesTokenCount")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    total_tokens: usage.get("totalTokenCount").and_then(Value::as_u64),
                }));
            }
        }
    }
    Ok(events)
}

fn to_openai_chat_messages(messages: &[UnifiedMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|msg| match msg.role {
            UnifiedRole::System => json!({"role":"system","content":msg.content}),
            UnifiedRole::User => json!({"role":"user","content":msg.content}),
            UnifiedRole::Assistant => {
                if let (Some(name), Some(arguments)) = (&msg.tool_name, &msg.tool_arguments_json) {
                    json!({
                        "role": "assistant",
                        "tool_calls": [{
                            "id": msg.tool_call_id.clone().unwrap_or_else(|| format!("call_{}", msg.id)),
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": arguments
                            }
                        }]
                    })
                } else {
                    json!({"role":"assistant","content":msg.content})
                }
            }
            UnifiedRole::Tool => json!({
                "role": "tool",
                "tool_call_id": msg.tool_call_id.clone().unwrap_or_default(),
                "content": msg.content
            }),
        })
        .collect()
}

fn to_openai_tools(tools: &[core_types::ToolSpec]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema
                }
            })
        })
        .collect()
}

fn to_openai_responses_input(messages: &[UnifiedMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|msg| match msg.role {
            UnifiedRole::Tool => json!({
                "type":"function_call_output",
                "call_id": msg.tool_call_id.clone().unwrap_or_default(),
                "output": msg.content,
            }),
            UnifiedRole::Assistant => {
                if let (Some(name), Some(arguments)) = (&msg.tool_name, &msg.tool_arguments_json) {
                    json!({
                        "type": "function_call",
                        "call_id": msg.tool_call_id.clone().unwrap_or_else(|| format!("call_{}", msg.id)),
                        "name": name,
                        "arguments": arguments
                    })
                } else {
                    json!({
                        "type":"message",
                        "role":"assistant",
                        "content":[{"type":"input_text", "text": msg.content}]
                    })
                }
            }
            UnifiedRole::System => json!({
                "type":"message",
                "role":"system",
                "content":[{"type":"input_text", "text": msg.content}]
            }),
            UnifiedRole::User => json!({
                "type":"message",
                "role":"user",
                "content":[{"type":"input_text", "text": msg.content}]
            }),
        })
        .collect()
}

fn to_openai_responses_tools(tools: &[core_types::ToolSpec]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.input_schema
            })
        })
        .collect()
}

fn to_anthropic_messages(messages: &[UnifiedMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|msg| match msg.role {
            UnifiedRole::User | UnifiedRole::System => json!({
                "role":"user",
                "content":[{"type":"text","text": msg.content}]
            }),
            UnifiedRole::Assistant => {
                if let (Some(name), Some(arguments)) = (&msg.tool_name, &msg.tool_arguments_json) {
                    json!({
                        "role":"assistant",
                        "content":[{
                            "type":"tool_use",
                            "id": msg.tool_call_id.clone().unwrap_or_else(|| format!("call_{}", msg.id)),
                            "name": name,
                            "input": serde_json::from_str::<Value>(arguments).unwrap_or(Value::Object(Map::new()))
                        }]
                    })
                } else {
                    json!({
                        "role":"assistant",
                        "content":[{"type":"text","text": msg.content}]
                    })
                }
            }
            UnifiedRole::Tool => json!({
                "role":"user",
                "content":[{
                    "type":"tool_result",
                    "tool_use_id": msg.tool_call_id.clone().unwrap_or_default(),
                    "is_error": msg.content.starts_with("error: "),
                    "content": msg.content
                }]
            }),
        })
        .collect()
}

fn to_anthropic_tools(tools: &[core_types::ToolSpec]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.input_schema
            })
        })
        .collect()
}

fn to_gemini_contents(messages: &[UnifiedMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|msg| {
            let role = if matches!(msg.role, UnifiedRole::Assistant) {
                "model"
            } else {
                "user"
            };
            if let (Some(name), Some(arguments)) = (&msg.tool_name, &msg.tool_arguments_json) {
                json!({
                    "role": role,
                    "parts": [{
                        "functionCall": {
                            "name": name,
                            "args": serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({}))
                        }
                    }]
                })
            } else {
                json!({
                    "role": role,
                    "parts": [{"text": msg.content}]
                })
            }
        })
        .collect()
}

fn to_gemini_tools(tools: &[core_types::ToolSpec]) -> Vec<Value> {
    if tools.is_empty() {
        return Vec::new();
    }

    vec![json!({
        "functionDeclarations": tools.iter().map(|tool| json!({
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema
        })).collect::<Vec<_>>()
    })]
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_types::ToolSpec;

    #[test]
    fn maps_openai_chat_payload_to_events() {
        let payload = json!({
            "choices": [{
                "delta": {
                    "content": "Hello",
                    "tool_calls": [{
                        "id": "call_1",
                        "function": {
                            "name": "sum",
                            "arguments": "{\"a\":1,\"b\":2}"
                        }
                    }]
                }
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 3,
                "total_tokens": 13
            }
        });
        let events = map_payload_to_events(ProviderId::OpenAi, payload).expect("events");
        assert!(events.iter().any(|e| matches!(
            e,
            Ok(UnifiedEvent::TextDelta { text }) if text == "Hello"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            Ok(UnifiedEvent::ToolCallRequested { name, .. }) if name == "sum"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            Ok(UnifiedEvent::Usage {
                total_tokens: Some(13),
                ..
            })
        )));
    }

    #[test]
    fn maps_anthropic_delta_to_text() {
        let payload = json!({
            "type": "content_block_delta",
            "delta": {
                "text": "你好"
            }
        });
        let events = map_payload_to_events(ProviderId::Anthropic, payload).expect("events");
        assert!(events.iter().any(|e| matches!(
            e,
            Ok(UnifiedEvent::TextDelta { text }) if text == "你好"
        )));
    }

    #[test]
    fn maps_gemini_function_call() {
        let payload = json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "functionCall": {
                            "name": "search_docs",
                            "args": {"query": "gpui"}
                        }
                    }]
                }
            }]
        });
        let events = map_payload_to_events(ProviderId::Gemini, payload).expect("events");
        assert!(events.iter().any(|e| matches!(
            e,
            Ok(UnifiedEvent::ToolCallRequested { name, .. }) if name == "search_docs"
        )));
    }

    #[test]
    fn builds_openai_tool_payload() {
        let tools = vec![ToolSpec {
            name: "weather".to_string(),
            description: "Get weather".to_string(),
            input_schema: json!({"type":"object"}),
            server_id: Some("server1".to_string()),
        }];
        let payload = to_openai_tools(&tools);
        assert_eq!(payload.len(), 1);
        assert_eq!(payload[0]["function"]["name"], "weather");
    }
}
