use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use async_trait::async_trait;
use core_types::{
    McpClient, McpServerConfig, McpTransportConfig, PromptContent, PromptSpec, ResourceContent,
    ResourceSpec, ToolSpec, UnifiedMessage, UnifiedRole,
};
use rust_mcp_sdk::mcp_client::{ClientHandler, McpClientOptions, client_runtime};
use rust_mcp_sdk::schema::{
    CallToolRequestParams, ClientCapabilities, GetPromptRequestParams, Implementation,
    InitializeRequestParams, LATEST_PROTOCOL_VERSION, PaginatedRequestParams,
    ReadResourceRequestParams, Role,
};
use rust_mcp_sdk::{
    ClientSseTransport, ClientSseTransportOptions, McpClient as RustMcpClient, RequestOptions,
    StdioTransport, StreamableTransportOptions, ToMcpClientHandler, TransportOptions,
};
use serde_json::{Map, Value};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Default)]
struct NoopHandler;

#[async_trait]
impl ClientHandler for NoopHandler {}

#[derive(Default)]
pub struct RustMcpRuntime {
    clients: RwLock<HashMap<String, Arc<rust_mcp_sdk::mcp_client::ClientRuntime>>>,
}

impl RustMcpRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    fn client_details() -> InitializeRequestParams {
        InitializeRequestParams {
            capabilities: ClientCapabilities::default(),
            client_info: Implementation {
                name: "drome".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: Some("Drome MCP Client".into()),
                description: Some("Drome desktop MCP client runtime".into()),
                icons: vec![],
                website_url: None,
            },
            protocol_version: LATEST_PROTOCOL_VERSION.into(),
            meta: None,
        }
    }

    fn to_anyhow<E: std::fmt::Display>(error: E) -> anyhow::Error {
        anyhow!(error.to_string())
    }

    async fn build_client(
        &self,
        config: &McpServerConfig,
    ) -> Result<Arc<rust_mcp_sdk::mcp_client::ClientRuntime>> {
        let client_details = Self::client_details();
        let client = match &config.transport {
            McpTransportConfig::Stdio {
                command,
                args,
                cwd,
                env,
            } => {
                let mut env_map: HashMap<String, String> = env.iter().cloned().collect();
                if let Some(cwd) = cwd {
                    env_map.insert("PWD".to_string(), cwd.clone());
                }
                let transport = StdioTransport::create_with_server_launch(
                    command.clone(),
                    args.clone(),
                    if env_map.is_empty() {
                        None
                    } else {
                        Some(env_map)
                    },
                    TransportOptions::default(),
                )
                .map_err(Self::to_anyhow)?;
                client_runtime::create_client(McpClientOptions {
                    client_details,
                    transport,
                    handler: NoopHandler.to_mcp_client_handler(),
                    task_store: None,
                    server_task_store: None,
                })
            }
            McpTransportConfig::Sse { url, headers } => {
                let transport = ClientSseTransport::new(
                    url,
                    ClientSseTransportOptions {
                        request_timeout: Duration::from_millis(config.timeout_ms),
                        retry_delay: None,
                        max_retries: None,
                        custom_headers: if headers.is_empty() {
                            None
                        } else {
                            Some(headers.iter().cloned().collect())
                        },
                    },
                )
                .map_err(Self::to_anyhow)?;
                client_runtime::create_client(McpClientOptions {
                    client_details,
                    transport,
                    handler: NoopHandler.to_mcp_client_handler(),
                    task_store: None,
                    server_task_store: None,
                })
            }
            McpTransportConfig::StreamableHttp { url, headers } => {
                let transport_options = StreamableTransportOptions {
                    mcp_url: url.clone(),
                    request_options: RequestOptions {
                        request_timeout: Duration::from_millis(config.timeout_ms),
                        retry_delay: None,
                        max_retries: None,
                        custom_headers: if headers.is_empty() {
                            None
                        } else {
                            Some(headers.iter().cloned().collect())
                        },
                    },
                };
                client_runtime::with_transport_options(
                    client_details,
                    transport_options,
                    NoopHandler,
                    None,
                    None,
                )
            }
        };
        client.clone().start().await.map_err(Self::to_anyhow)?;
        Ok(client)
    }

    async fn get_client(
        &self,
        server_id: &str,
    ) -> Result<Arc<rust_mcp_sdk::mcp_client::ClientRuntime>> {
        self.clients
            .read()
            .await
            .get(server_id)
            .cloned()
            .ok_or_else(|| anyhow!("MCP server `{server_id}` not connected"))
    }
}

#[async_trait]
impl McpClient for RustMcpRuntime {
    async fn upsert_server(&self, server: McpServerConfig) -> Result<()> {
        if !server.enabled {
            return Ok(());
        }
        let existing = { self.clients.write().await.remove(&server.id) };
        if let Some(existing) = existing {
            let _ = existing.shut_down().await;
        }
        let client = self.build_client(&server).await?;
        self.clients.write().await.insert(server.id.clone(), client);
        info!(server_id = %server.id, "mcp server connected");
        Ok(())
    }

    async fn remove_server(&self, server_id: &str) -> Result<()> {
        let client = { self.clients.write().await.remove(server_id) };
        if let Some(client) = client {
            let _ = client.shut_down().await;
        }
        Ok(())
    }

    async fn healthcheck(&self, server_id: &str) -> Result<()> {
        let client = self.get_client(server_id).await?;
        client.ping(None, None).await.map_err(Self::to_anyhow)?;
        Ok(())
    }

    async fn list_tools(&self, server_id: &str) -> Result<Vec<ToolSpec>> {
        let client = self.get_client(server_id).await?;
        let tools = client
            .request_tool_list(None::<PaginatedRequestParams>)
            .await
            .map_err(Self::to_anyhow)?;
        Ok(tools
            .tools
            .into_iter()
            .map(|tool| ToolSpec {
                name: tool.name,
                description: tool.description.unwrap_or_default(),
                input_schema: serde_json::to_value(tool.input_schema).unwrap_or(Value::Null),
                server_id: Some(server_id.to_string()),
            })
            .collect())
    }

    async fn call_tool(&self, server_id: &str, name: &str, args_json: &str) -> Result<String> {
        let client = self.get_client(server_id).await?;
        let arguments_value: Value = if args_json.trim().is_empty() {
            Value::Object(Map::new())
        } else {
            serde_json::from_str(args_json)?
        };
        let args = arguments_value
            .as_object()
            .cloned()
            .ok_or_else(|| anyhow!("tool arguments must be a JSON object"))?;

        let result = client
            .request_tool_call(CallToolRequestParams {
                name: name.to_string(),
                arguments: Some(args),
                meta: None,
                task: None,
            })
            .await
            .map_err(Self::to_anyhow)?;

        let Some(first) = result.content.first() else {
            bail!("tool call returned no content");
        };
        Ok(serde_json::to_string(first).unwrap_or_else(|_| "".to_string()))
    }

    async fn list_resources(&self, server_id: &str) -> Result<Vec<ResourceSpec>> {
        let client = self.get_client(server_id).await?;
        let resources = client
            .request_resource_list(None::<PaginatedRequestParams>)
            .await
            .map_err(Self::to_anyhow)?;
        Ok(resources
            .resources
            .into_iter()
            .map(|resource| ResourceSpec {
                uri: resource.uri,
                name: resource.name,
                mime_type: resource.mime_type,
                description: resource.description,
            })
            .collect())
    }

    async fn read_resource(&self, server_id: &str, uri: &str) -> Result<ResourceContent> {
        let client = self.get_client(server_id).await?;
        let result = client
            .request_resource_read(ReadResourceRequestParams {
                uri: uri.to_string(),
                meta: None,
            })
            .await
            .map_err(Self::to_anyhow)?;

        let Some(first) = result.contents.first() else {
            bail!("resource `{uri}` returned empty content");
        };

        let (text, blob_base64, mime_type, resource_uri) = match first {
            rust_mcp_sdk::schema::ReadResourceContent::TextResourceContents(text_content) => (
                Some(text_content.text.clone()),
                None,
                text_content.mime_type.clone(),
                text_content.uri.clone(),
            ),
            rust_mcp_sdk::schema::ReadResourceContent::BlobResourceContents(blob_content) => (
                None,
                Some(blob_content.blob.clone()),
                blob_content.mime_type.clone(),
                blob_content.uri.clone(),
            ),
        };

        Ok(ResourceContent {
            uri: resource_uri,
            text,
            blob_base64,
            mime_type,
        })
    }

    async fn list_prompts(&self, server_id: &str) -> Result<Vec<PromptSpec>> {
        let client = self.get_client(server_id).await?;
        let prompts = client
            .request_prompt_list(None::<PaginatedRequestParams>)
            .await
            .map_err(Self::to_anyhow)?;
        Ok(prompts
            .prompts
            .into_iter()
            .map(|prompt| PromptSpec {
                name: prompt.name,
                description: prompt.description,
            })
            .collect())
    }

    async fn get_prompt(
        &self,
        server_id: &str,
        name: &str,
        args_json: &str,
    ) -> Result<PromptContent> {
        let client = self.get_client(server_id).await?;
        let args = parse_prompt_args(args_json)?;
        let result = client
            .request_prompt(GetPromptRequestParams {
                name: name.to_string(),
                arguments: args,
                meta: None,
            })
            .await
            .map_err(Self::to_anyhow)?;

        let messages = result
            .messages
            .into_iter()
            .map(|message| UnifiedMessage {
                id: uuid::Uuid::new_v4(),
                role: match message.role {
                    Role::Assistant => UnifiedRole::Assistant,
                    Role::User => UnifiedRole::User,
                },
                content: serde_json::to_string(&message.content).unwrap_or_default(),
                tool_call_id: None,
                tool_name: None,
                tool_arguments_json: None,
            })
            .collect();

        Ok(PromptContent {
            description: result.description,
            messages,
        })
    }
}

fn parse_prompt_args(args_json: &str) -> Result<Option<HashMap<String, String>>> {
    if args_json.trim().is_empty() {
        return Ok(None);
    }

    let args_value: Value = serde_json::from_str(args_json)
        .map_err(|err| anyhow!("invalid prompt args json: {err}"))?;
    let Some(args_obj) = args_value.as_object() else {
        bail!("prompt args must be a JSON object");
    };

    if args_obj.is_empty() {
        return Ok(None);
    }

    let args = args_obj
        .iter()
        .map(|(key, value)| {
            let value = value
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_else(|| value.to_string());
            (key.clone(), value)
        })
        .collect();
    Ok(Some(args))
}
