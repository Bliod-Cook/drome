use std::sync::Arc;

use anyhow::{Result, anyhow};
use core_types::{
    McpClient, ProviderAdapter, ProviderConfig, ToolSpec, UnifiedEvent, UnifiedGenerateRequest,
    UnifiedMessage,
};
use futures::StreamExt;
use tracing::warn;

pub struct Orchestrator {
    provider: Arc<dyn ProviderAdapter>,
    mcp: Option<Arc<dyn McpClient>>,
    max_rounds: usize,
}

impl Orchestrator {
    pub fn new(provider: Arc<dyn ProviderAdapter>, mcp: Option<Arc<dyn McpClient>>) -> Self {
        Self {
            provider,
            mcp,
            max_rounds: 4,
        }
    }

    pub fn with_max_rounds(mut self, max_rounds: usize) -> Self {
        self.max_rounds = max_rounds.max(1);
        self
    }

    pub async fn run_turn(
        &self,
        provider_config: &ProviderConfig,
        api_key: &str,
        mut request: UnifiedGenerateRequest,
    ) -> Result<Vec<UnifiedEvent>> {
        let mut output_events = Vec::new();

        for round in 0..self.max_rounds {
            let stream = self
                .provider
                .stream_generate(provider_config, api_key, request.clone())
                .await?;
            futures::pin_mut!(stream);

            let mut pending_calls = Vec::<(String, String, String)>::new();
            while let Some(item) = stream.next().await {
                let event = item?;
                if let UnifiedEvent::ToolCallRequested {
                    call_id,
                    name,
                    arguments_json,
                } = &event
                {
                    pending_calls.push((call_id.clone(), name.clone(), arguments_json.clone()));
                }
                output_events.push(event);
            }

            if pending_calls.is_empty() {
                break;
            }

            let Some(mcp) = self.mcp.as_ref() else {
                warn!("tool call requested but MCP runtime unavailable");
                output_events.push(UnifiedEvent::Failed {
                    code: "mcp_unavailable".to_string(),
                    message: "Tool call requested but MCP runtime is unavailable".to_string(),
                    retriable: false,
                });
                break;
            };

            for (call_id, name, arguments_json) in pending_calls {
                let tool = find_tool_spec(&request.tools, &name).ok_or_else(|| {
                    anyhow!("tool `{name}` was requested by model but is not registered")
                })?;
                let server_id = tool
                    .server_id
                    .as_ref()
                    .ok_or_else(|| anyhow!("tool `{name}` does not provide server_id"))?;

                let call_result = mcp.call_tool(server_id, &name, &arguments_json).await;
                match call_result {
                    Ok(output) => {
                        output_events.push(UnifiedEvent::ToolCallResult {
                            call_id: call_id.clone(),
                            output: output.clone(),
                            is_error: false,
                        });
                        request.messages.push(UnifiedMessage {
                            id: uuid::Uuid::new_v4(),
                            role: core_types::UnifiedRole::Assistant,
                            content: String::new(),
                            tool_call_id: Some(call_id.clone()),
                            tool_name: Some(name.clone()),
                            tool_arguments_json: Some(arguments_json.clone()),
                        });
                        request
                            .messages
                            .push(UnifiedMessage::tool_result(call_id, output, false));
                    }
                    Err(err) => {
                        output_events.push(UnifiedEvent::ToolCallResult {
                            call_id: call_id.clone(),
                            output: err.to_string(),
                            is_error: true,
                        });
                        request.messages.push(UnifiedMessage::tool_result(
                            call_id,
                            err.to_string(),
                            true,
                        ));
                    }
                }
            }

            if round + 1 == self.max_rounds {
                output_events.push(UnifiedEvent::Failed {
                    code: "max_tool_rounds".to_string(),
                    message: "Reached max MCP tool loop rounds".to_string(),
                    retriable: false,
                });
            }
        }

        Ok(output_events)
    }
}

fn find_tool_spec<'a>(tools: &'a [ToolSpec], name: &str) -> Option<&'a ToolSpec> {
    tools.iter().find(|tool| tool.name == name)
}
