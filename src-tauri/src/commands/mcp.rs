use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use rmcp::model::{
    CallToolRequest, CallToolRequestParams, ClientRequest, Content, GetPromptRequestParams,
    GetPromptResult, LoggingLevel, LoggingMessageNotificationParam, NumberOrString,
    PaginatedRequestParams, ProgressNotificationParam, ReadResourceRequestParams, ResourceContents,
    ServerResult,
};
use rmcp::service::{NotificationContext, Peer, PeerRequestOptions, RoleClient, RunningService};
use rmcp::transport::{
    streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
    TokioChildProcess,
};
use rmcp::{ClientHandler, ServiceError, ServiceExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

use crate::error::{DromeError, Result};

const MCP_SERVER_LOG_CHANNEL: &str = "mcp:server-log";
const MCP_PROGRESS_CHANNEL: &str = "mcp:progress";
const LOG_LIMIT: usize = 200;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub timeout: Option<f64>,
    #[serde(default)]
    pub long_running: Option<bool>,
    #[serde(default)]
    pub dxt_path: Option<String>,
    #[serde(default)]
    pub registry_url: Option<String>,
}

impl McpServer {
    fn transport_type(&self) -> &'static str {
        match self.r#type.as_deref() {
            Some("stdio") => "stdio",
            Some("sse") => "sse",
            Some("streamableHttp") => "streamableHttp",
            Some("inMemory") => "inMemory",
            Some(other) if other.contains("http") => "streamableHttp",
            Some(_) => {
                if self.base_url.is_some() {
                    "sse"
                } else {
                    "stdio"
                }
            }
            None => {
                if self.base_url.is_some() {
                    "sse"
                } else {
                    "stdio"
                }
            }
        }
    }

    fn request_timeout(&self) -> Duration {
        let long_running = self.long_running.unwrap_or(false);
        let default = if long_running { 600.0 } else { 60.0 };
        let secs = self.timeout.unwrap_or(default).max(1.0);
        Duration::from_secs_f64(secs)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInstallInfo {
    pub dir: String,
    pub uv_path: String,
    pub bun_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCallToolArgs {
    pub server: McpServer,
    pub name: String,
    #[serde(default)]
    pub args: Option<Value>,
    #[serde(default)]
    pub call_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpGetPromptArgs {
    pub server: McpServer,
    pub name: String,
    #[serde(default)]
    pub args: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpGetResourceArgs {
    pub server: McpServer,
    pub uri: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpUploadDxtResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPromptArgument {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPrompt {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<McpPromptArgument>>,
    pub server_id: String,
    pub server_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResource {
    pub server_id: String,
    pub server_name: String,
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub id: String,
    pub server_id: String,
    pub server_name: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetResourceResponse {
    pub contents: Vec<McpResource>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPromptMessage {
    pub role: String,
    pub content: McpMessageContent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpMessageContent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPromptResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub messages: Vec<McpPromptMessage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolResourcePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolResultContent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<McpToolResourcePayload>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCallToolResponse {
    pub content: Vec<McpToolResultContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum McpServerLogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Stderr,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerLogEntry {
    pub timestamp: u64,
    pub level: McpServerLogLevel,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpServerLogEvent {
    #[serde(flatten)]
    entry: McpServerLogEntry,
    server_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpProgressEvent {
    call_id: String,
    progress: f64,
}

#[derive(Debug)]
struct ActiveToolCall {
    server_id: String,
    request_id: rmcp::model::RequestId,
    peer: Peer<RoleClient>,
    progress_key: String,
    abort_signal: Arc<Notify>,
}

#[derive(Debug)]
struct ManagedClient {
    server_id: String,
    running: RunningService<RoleClient, Arc<TauriClientHandler>>,
    tools_cache: Option<Vec<McpTool>>,
    prompts_cache: Option<Vec<McpPrompt>>,
    resources_cache: Option<Vec<McpResource>>,
}

#[derive(Debug, Default)]
struct McpState {
    clients: HashMap<String, ManagedClient>,
    server_logs: HashMap<String, VecDeque<McpServerLogEntry>>,
    active_calls: HashMap<String, ActiveToolCall>,
    progress_to_call: HashMap<String, String>,
}

#[derive(Debug, Default)]
struct McpManager {
    state: Mutex<McpState>,
}

#[derive(Debug, Clone)]
struct TauriClientHandler {
    app: AppHandle,
    server_id: String,
    server_key: String,
    manager: Weak<McpManager>,
}

impl TauriClientHandler {
    fn new(
        app: AppHandle,
        server_id: String,
        server_key: String,
        manager: Weak<McpManager>,
    ) -> Self {
        Self {
            app,
            server_id,
            server_key,
            manager,
        }
    }

    async fn invalidate_cache(&self, tools: bool, prompts: bool, resources: bool) {
        let Some(manager) = self.manager.upgrade() else {
            return;
        };
        let mut state = manager.state.lock().await;
        if let Some(client) = state.clients.get_mut(&self.server_key) {
            if tools {
                client.tools_cache = None;
            }
            if prompts {
                client.prompts_cache = None;
            }
            if resources {
                client.resources_cache = None;
            }
        }
    }

    async fn append_log(&self, entry: McpServerLogEntry) {
        let Some(manager) = self.manager.upgrade() else {
            return;
        };
        manager
            .append_server_log(&self.app, &self.server_id, &self.server_key, entry)
            .await;
    }

    async fn emit_progress(&self, token: &rmcp::model::ProgressToken, progress: f64) {
        let Some(manager) = self.manager.upgrade() else {
            return;
        };

        let call_id = {
            let state = manager.state.lock().await;
            state
                .progress_to_call
                .get(&progress_token_key(token))
                .cloned()
        };

        let Some(call_id) = call_id else {
            return;
        };

        let payload = McpProgressEvent { call_id, progress };
        let _ = self.app.emit(MCP_PROGRESS_CHANNEL, payload);
    }
}

impl ClientHandler for TauriClientHandler {
    fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async move {
            let total = params.total.unwrap_or(0.0);
            let normalized = if total > 0.0 {
                (params.progress / total).clamp(0.0, 1.0)
            } else {
                params.progress.clamp(0.0, 1.0)
            };
            self.emit_progress(&params.progress_token, normalized).await;
        }
    }

    fn on_logging_message(
        &self,
        params: LoggingMessageNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async move {
            let level = match params.level {
                LoggingLevel::Debug => McpServerLogLevel::Debug,
                LoggingLevel::Info | LoggingLevel::Notice => McpServerLogLevel::Info,
                LoggingLevel::Warning => McpServerLogLevel::Warn,
                LoggingLevel::Error
                | LoggingLevel::Critical
                | LoggingLevel::Alert
                | LoggingLevel::Emergency => McpServerLogLevel::Error,
            };
            let message = match &params.data {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            self.append_log(McpServerLogEntry {
                timestamp: now_ms(),
                level,
                message,
                data: Some(params.data),
                source: params.logger,
            })
            .await;
        }
    }

    fn on_tool_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async move {
            self.invalidate_cache(true, false, false).await;
        }
    }

    fn on_prompt_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async move {
            self.invalidate_cache(false, true, false).await;
        }
    }

    fn on_resource_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async move {
            self.invalidate_cache(false, false, true).await;
        }
    }

    fn on_resource_updated(
        &self,
        _params: rmcp::model::ResourceUpdatedNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async move {
            self.invalidate_cache(false, false, true).await;
        }
    }
}

static MCP_MANAGER: OnceLock<Arc<McpManager>> = OnceLock::new();

fn manager() -> Arc<McpManager> {
    MCP_MANAGER
        .get_or_init(|| Arc::new(McpManager::default()))
        .clone()
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

fn binary_name(name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

pub fn mcp_get_install_info() -> Result<McpInstallInfo> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = home.join(".cherrystudio").join("bin");

    let uv_path = dir.join(binary_name("uv"));
    let bun_path = dir.join(binary_name("bun"));

    Ok(McpInstallInfo {
        dir: dir.to_string_lossy().to_string(),
        uv_path: uv_path.to_string_lossy().to_string(),
        bun_path: bun_path.to_string_lossy().to_string(),
    })
}

pub fn mcp_upload_dxt(_bytes: Vec<u8>, _filename: String) -> Result<McpUploadDxtResponse> {
    Ok(McpUploadDxtResponse {
        success: false,
        error: Some("DXT install is not supported in this Tauri build yet".to_string()),
        data: None,
    })
}

pub async fn mcp_remove_server(app: &AppHandle, server: McpServer) -> Result<()> {
    let manager = manager();
    manager
        .append_server_log(
            app,
            &server.id,
            &server_key(&server),
            McpServerLogEntry {
                timestamp: now_ms(),
                level: McpServerLogLevel::Info,
                message: "Removing server".to_string(),
                data: None,
                source: Some("client".to_string()),
            },
        )
        .await;
    manager.close_server(&server).await
}

pub async fn mcp_stop_server(app: &AppHandle, server: McpServer) -> Result<()> {
    let manager = manager();
    manager
        .append_server_log(
            app,
            &server.id,
            &server_key(&server),
            McpServerLogEntry {
                timestamp: now_ms(),
                level: McpServerLogLevel::Info,
                message: "Stopping server".to_string(),
                data: None,
                source: Some("client".to_string()),
            },
        )
        .await;
    manager.close_server(&server).await
}

pub async fn mcp_restart_server(app: &AppHandle, server: McpServer) -> Result<()> {
    let manager = manager();
    manager
        .append_server_log(
            app,
            &server.id,
            &server_key(&server),
            McpServerLogEntry {
                timestamp: now_ms(),
                level: McpServerLogLevel::Info,
                message: "Restarting server".to_string(),
                data: None,
                source: Some("client".to_string()),
            },
        )
        .await;
    manager.close_server(&server).await?;
    let _ = manager.get_peer(app, &server).await?;
    Ok(())
}

pub async fn mcp_check_connectivity(app: &AppHandle, server: McpServer) -> Result<bool> {
    let manager = manager();
    let peer = match manager.get_peer(app, &server).await {
        Ok((_, peer)) => peer,
        Err(_) => return Ok(false),
    };

    let response = peer
        .list_tools(Some(PaginatedRequestParams::default()))
        .await;

    if response.is_ok() {
        Ok(true)
    } else {
        let _ = manager.close_server(&server).await;
        Ok(false)
    }
}

pub async fn mcp_get_server_version(app: &AppHandle, server: McpServer) -> Result<Option<String>> {
    let manager = manager();
    let (_, peer) = manager.get_peer(app, &server).await?;
    Ok(peer
        .peer_info()
        .map(|info| info.server_info.version.clone()))
}

pub async fn mcp_get_server_logs(server: McpServer) -> Result<Vec<McpServerLogEntry>> {
    let manager = manager();
    let key = server_key(&server);
    let state = manager.state.lock().await;
    Ok(state
        .server_logs
        .get(&key)
        .map(|logs| logs.iter().cloned().collect())
        .unwrap_or_default())
}

pub async fn mcp_list_tools(app: &AppHandle, server: McpServer) -> Result<Vec<McpTool>> {
    let manager = manager();
    let key = server_key(&server);

    {
        let state = manager.state.lock().await;
        if let Some(client) = state.clients.get(&key) {
            if let Some(cached) = &client.tools_cache {
                return Ok(cached.clone());
            }
        }
    }

    let (_, peer) = manager.get_peer(app, &server).await?;
    let tools = peer.list_all_tools().await.map_err(map_service_error)?;
    let mapped = tools
        .into_iter()
        .map(|tool| map_tool(&server, tool))
        .collect::<Vec<_>>();

    let mut state = manager.state.lock().await;
    if let Some(client) = state.clients.get_mut(&key) {
        client.tools_cache = Some(mapped.clone());
    }

    Ok(mapped)
}

pub async fn mcp_list_prompts(app: &AppHandle, server: McpServer) -> Result<Vec<McpPrompt>> {
    let manager = manager();
    let key = server_key(&server);

    {
        let state = manager.state.lock().await;
        if let Some(client) = state.clients.get(&key) {
            if let Some(cached) = &client.prompts_cache {
                return Ok(cached.clone());
            }
        }
    }

    let (_, peer) = manager.get_peer(app, &server).await?;
    let prompts = match peer.list_all_prompts().await {
        Ok(value) => value,
        Err(err) if is_method_not_found(&err) => Vec::new(),
        Err(err) => return Err(map_service_error(err)),
    };

    let mapped = prompts
        .into_iter()
        .map(|prompt| McpPrompt {
            id: format!("p{}", Uuid::new_v4().simple()),
            name: prompt.name,
            description: prompt.description,
            arguments: prompt.arguments.map(|args| {
                args.into_iter()
                    .map(|arg| McpPromptArgument {
                        name: arg.name,
                        description: arg.description,
                        required: arg.required,
                    })
                    .collect()
            }),
            server_id: server.id.clone(),
            server_name: server.name.clone(),
        })
        .collect::<Vec<_>>();

    let mut state = manager.state.lock().await;
    if let Some(client) = state.clients.get_mut(&key) {
        client.prompts_cache = Some(mapped.clone());
    }

    Ok(mapped)
}

pub async fn mcp_get_prompt(app: &AppHandle, args: McpGetPromptArgs) -> Result<GetPromptResponse> {
    let manager = manager();
    let (_, peer) = manager.get_peer(app, &args.server).await?;

    let prompt_args = parse_optional_object(args.args)?;
    let result = peer
        .get_prompt(GetPromptRequestParams {
            meta: None,
            name: args.name,
            arguments: prompt_args,
        })
        .await
        .map_err(map_service_error)?;

    Ok(map_prompt_response(result))
}

pub async fn mcp_list_resources(app: &AppHandle, server: McpServer) -> Result<Vec<McpResource>> {
    let manager = manager();
    let key = server_key(&server);

    {
        let state = manager.state.lock().await;
        if let Some(client) = state.clients.get(&key) {
            if let Some(cached) = &client.resources_cache {
                return Ok(cached.clone());
            }
        }
    }

    let (_, peer) = manager.get_peer(app, &server).await?;
    let resources = match peer.list_all_resources().await {
        Ok(value) => value,
        Err(err) if is_method_not_found(&err) => Vec::new(),
        Err(err) => return Err(map_service_error(err)),
    };

    let mapped = resources
        .into_iter()
        .map(|resource| McpResource {
            server_id: server.id.clone(),
            server_name: server.name.clone(),
            uri: resource.uri.clone(),
            name: resource.name.clone(),
            description: resource.description.clone(),
            mime_type: resource.mime_type.clone(),
            size: resource.size,
            text: None,
            blob: None,
        })
        .collect::<Vec<_>>();

    let mut state = manager.state.lock().await;
    if let Some(client) = state.clients.get_mut(&key) {
        client.resources_cache = Some(mapped.clone());
    }

    Ok(mapped)
}

pub async fn mcp_get_resource(
    app: &AppHandle,
    args: McpGetResourceArgs,
) -> Result<GetResourceResponse> {
    let manager = manager();
    let (_, peer) = manager.get_peer(app, &args.server).await?;

    let result = peer
        .read_resource(ReadResourceRequestParams {
            meta: None,
            uri: args.uri.clone(),
        })
        .await
        .map_err(map_service_error)?;

    let contents = result
        .contents
        .into_iter()
        .map(|content| map_resource_content(&args.server, content))
        .collect::<Vec<_>>();

    Ok(GetResourceResponse { contents })
}

pub async fn mcp_call_tool(app: &AppHandle, args: McpCallToolArgs) -> Result<McpCallToolResponse> {
    let manager = manager();
    let call_id = args.call_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let (_, peer) = manager.get_peer(app, &args.server).await?;

    let parsed_arguments = parse_optional_object(args.args)?;
    let request = ClientRequest::CallToolRequest(CallToolRequest {
        method: Default::default(),
        params: CallToolRequestParams {
            meta: None,
            name: Cow::Owned(args.name.clone()),
            arguments: parsed_arguments,
            task: None,
        },
        extensions: Default::default(),
    });

    let request_handle = peer
        .send_cancellable_request(
            request,
            PeerRequestOptions {
                timeout: Some(args.server.request_timeout()),
                meta: None,
            },
        )
        .await
        .map_err(map_service_error)?;

    let progress_key = progress_token_key(&request_handle.progress_token);
    let abort_signal = Arc::new(Notify::new());

    {
        let mut state = manager.state.lock().await;
        state
            .progress_to_call
            .insert(progress_key.clone(), call_id.clone());
        state.active_calls.insert(
            call_id.clone(),
            ActiveToolCall {
                server_id: args.server.id.clone(),
                request_id: request_handle.id.clone(),
                peer: peer.clone(),
                progress_key,
                abort_signal: abort_signal.clone(),
            },
        );
    }

    let response = tokio::select! {
        _ = abort_signal.notified() => {
            Err(DromeError::Message("Tool call aborted".to_string()))
        }
        result = request_handle.await_response() => {
            result
                .map_err(map_service_error)
                .and_then(|response| {
                    match response {
                        ServerResult::CallToolResult(result) => Ok(map_call_tool_response(result)),
                        _ => Err(DromeError::Message("Unexpected MCP response type for tool call".to_string())),
                    }
                })
        }
    };

    {
        let mut state = manager.state.lock().await;
        if let Some(active) = state.active_calls.remove(&call_id) {
            state.progress_to_call.remove(&active.progress_key);
        }
    }

    response
}

pub async fn mcp_abort_tool(call_id: String) -> Result<bool> {
    let manager = manager();
    let active = {
        let mut state = manager.state.lock().await;
        if let Some(active) = state.active_calls.remove(&call_id) {
            state.progress_to_call.remove(&active.progress_key);
            Some(active)
        } else {
            None
        }
    };

    let Some(active) = active else {
        return Ok(false);
    };

    active.abort_signal.notify_waiters();

    let _ = active
        .peer
        .notify_cancelled(rmcp::model::CancelledNotificationParam {
            request_id: active.request_id,
            reason: Some("aborted by user".to_string()),
        })
        .await;

    Ok(true)
}

impl McpManager {
    async fn append_server_log(
        &self,
        app: &AppHandle,
        server_id: &str,
        server_key: &str,
        entry: McpServerLogEntry,
    ) {
        {
            let mut state = self.state.lock().await;
            let logs = state.server_logs.entry(server_key.to_string()).or_default();
            logs.push_back(entry.clone());
            while logs.len() > LOG_LIMIT {
                logs.pop_front();
            }
        }

        let event = McpServerLogEvent {
            entry,
            server_id: server_id.to_string(),
        };
        let _ = app.emit(MCP_SERVER_LOG_CHANNEL, event);
    }

    async fn get_peer(
        self: &Arc<Self>,
        app: &AppHandle,
        server: &McpServer,
    ) -> Result<(String, Peer<RoleClient>)> {
        let key = server_key(server);

        {
            let mut state = self.state.lock().await;
            if let Some(client) = state.clients.get_mut(&key) {
                if !client.running.is_closed() {
                    return Ok((key, client.running.peer().clone()));
                }
            }
            state.clients.remove(&key);
        }

        self.connect_client(app, server.clone()).await?;

        let state = self.state.lock().await;
        let client = state
            .clients
            .get(&key)
            .ok_or_else(|| DromeError::Message("MCP client initialization failed".to_string()))?;
        Ok((key, client.running.peer().clone()))
    }

    async fn connect_client(self: &Arc<Self>, app: &AppHandle, server: McpServer) -> Result<()> {
        let key = server_key(&server);
        let handler = Arc::new(TauriClientHandler::new(
            app.clone(),
            server.id.clone(),
            key.clone(),
            Arc::downgrade(self),
        ));

        let transport_type = server.transport_type();
        let mut stderr_stream = None;

        let running = match transport_type {
            "stdio" => {
                let (transport, stderr) = build_stdio_transport(&server)?;
                stderr_stream = stderr;
                handler.clone().serve(transport).await.map_err(|e| {
                    DromeError::Message(format!("Failed to connect MCP stdio server: {e}"))
                })?
            }
            "streamableHttp" | "sse" => {
                let transport = build_http_transport(&server)?;
                handler.clone().serve(transport).await.map_err(|e| {
                    DromeError::Message(format!("Failed to connect MCP HTTP server: {e}"))
                })?
            }
            "inMemory" => {
                if server.command.is_some() {
                    let (transport, stderr) = build_stdio_transport(&server)?;
                    stderr_stream = stderr;
                    handler.clone().serve(transport).await.map_err(|e| {
                        DromeError::Message(format!(
                            "Failed to connect inMemory(command) MCP server: {e}"
                        ))
                    })?
                } else {
                    return Err(DromeError::Message(
                        "In-memory MCP servers are not supported in this Tauri build".to_string(),
                    ));
                }
            }
            _ => {
                return Err(DromeError::Message(format!(
                    "Unsupported MCP transport type: {}",
                    server.transport_type()
                )));
            }
        };

        if let Some(stderr) = stderr_stream {
            self.spawn_stderr_reader(app.clone(), server.id.clone(), key.clone(), stderr);
        }

        self.append_server_log(
            app,
            &server.id,
            &key,
            McpServerLogEntry {
                timestamp: now_ms(),
                level: McpServerLogLevel::Info,
                message: "Server connected".to_string(),
                data: None,
                source: Some("client".to_string()),
            },
        )
        .await;

        let mut state = self.state.lock().await;
        state.clients.insert(
            key.clone(),
            ManagedClient {
                server_id: server.id,
                running,
                tools_cache: None,
                prompts_cache: None,
                resources_cache: None,
            },
        );

        Ok(())
    }

    fn spawn_stderr_reader(
        self: &Arc<Self>,
        app: AppHandle,
        server_id: String,
        server_key: String,
        stderr: tokio::process::ChildStderr,
    ) {
        let manager = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        let Some(manager) = manager.upgrade() else {
                            break;
                        };
                        manager
                            .append_server_log(
                                &app,
                                &server_id,
                                &server_key,
                                McpServerLogEntry {
                                    timestamp: now_ms(),
                                    level: McpServerLogLevel::Stderr,
                                    message: trimmed.to_string(),
                                    data: None,
                                    source: Some("stdio".to_string()),
                                },
                            )
                            .await;
                    }
                    Ok(None) => break,
                    Err(err) => {
                        let Some(manager) = manager.upgrade() else {
                            break;
                        };
                        manager
                            .append_server_log(
                                &app,
                                &server_id,
                                &server_key,
                                McpServerLogEntry {
                                    timestamp: now_ms(),
                                    level: McpServerLogLevel::Error,
                                    message: format!("stderr read failed: {err}"),
                                    data: None,
                                    source: Some("stdio".to_string()),
                                },
                            )
                            .await;
                        break;
                    }
                }
            }
        });
    }

    async fn close_server(self: &Arc<Self>, server: &McpServer) -> Result<()> {
        let key = server_key(server);
        let (mut removed_clients, removed_calls) = {
            let mut state = self.state.lock().await;

            let keys_to_remove = state
                .clients
                .iter()
                .filter_map(|(server_key, client)| {
                    if client.server_id == server.id || server_key == &key {
                        Some(server_key.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            let mut removed_clients = Vec::with_capacity(keys_to_remove.len());
            for key in keys_to_remove {
                if let Some(client) = state.clients.remove(&key) {
                    removed_clients.push(client);
                }
            }

            let call_ids = state
                .active_calls
                .iter()
                .filter_map(|(call_id, call)| {
                    if call.server_id == server.id {
                        Some(call_id.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            let mut removed_calls = Vec::with_capacity(call_ids.len());
            for call_id in call_ids {
                if let Some(call) = state.active_calls.remove(&call_id) {
                    state.progress_to_call.remove(&call.progress_key);
                    removed_calls.push(call);
                }
            }

            (removed_clients, removed_calls)
        };

        for active in removed_calls {
            active.abort_signal.notify_waiters();
            let _ = active
                .peer
                .notify_cancelled(rmcp::model::CancelledNotificationParam {
                    request_id: active.request_id,
                    reason: Some("server stopped".to_string()),
                })
                .await;
        }

        for client in &mut removed_clients {
            let _ = client.running.close().await;
        }

        Ok(())
    }
}

fn map_service_error(err: ServiceError) -> DromeError {
    DromeError::Message(format!("MCP request failed: {err}"))
}

fn is_method_not_found(err: &ServiceError) -> bool {
    matches!(err, ServiceError::McpError(data) if data.code.0 == -32601)
}

fn parse_optional_object(value: Option<Value>) -> Result<Option<Map<String, Value>>> {
    let Some(value) = value else {
        return Ok(None);
    };

    match value {
        Value::Null => Ok(None),
        Value::Object(map) => Ok(Some(map)),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(Some(Map::new()));
            }
            let parsed: Value = serde_json::from_str(trimmed).map_err(|e| {
                DromeError::Message(format!("Invalid JSON arguments for MCP call: {e}"))
            })?;
            if let Value::Object(map) = parsed {
                Ok(Some(map))
            } else {
                Err(DromeError::Message(
                    "MCP arguments must decode to a JSON object".to_string(),
                ))
            }
        }
        _ => Err(DromeError::Message(
            "MCP arguments must be an object, JSON string, or null".to_string(),
        )),
    }
}

fn to_camel_case(input: &str) -> String {
    let mut result = String::new();
    let mut upper_next = false;

    for ch in input.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            if upper_next {
                result.push(ch.to_ascii_uppercase());
                upper_next = false;
            } else {
                result.push(ch);
            }
        } else {
            upper_next = true;
        }
    }

    if let Some(first) = result.chars().next() {
        if first.is_ascii_digit() {
            result.insert(0, '_');
        }
    }

    result
}

fn truncate_to_len(mut value: String, max_len: usize) -> String {
    if value.len() <= max_len {
        return value;
    }
    value.truncate(max_len);
    while value.ends_with('_') {
        value.pop();
    }
    value
}

fn build_function_call_tool_name(server_name: &str, tool_name: &str) -> String {
    let server = to_camel_case(server_name);
    let tool = to_camel_case(tool_name);
    truncate_to_len(format!("mcp__{server}__{tool}"), 63)
}

fn map_tool(server: &McpServer, tool: rmcp::model::Tool) -> McpTool {
    McpTool {
        id: build_function_call_tool_name(&server.name, tool.name.as_ref()),
        server_id: server.id.clone(),
        server_name: server.name.clone(),
        name: tool.name.to_string(),
        description: tool.description.map(|s| s.to_string()),
        input_schema: Value::Object((*tool.input_schema).clone()),
        output_schema: tool
            .output_schema
            .map(|schema| Value::Object((*schema).clone())),
        kind: "mcp".to_string(),
    }
}

fn map_prompt_response(result: GetPromptResult) -> GetPromptResponse {
    let messages = result
        .messages
        .into_iter()
        .map(|message| {
            let role = match message.role {
                rmcp::model::PromptMessageRole::User => "user".to_string(),
                rmcp::model::PromptMessageRole::Assistant => "assistant".to_string(),
            };

            let content = match message.content {
                rmcp::model::PromptMessageContent::Text { text } => McpMessageContent {
                    kind: "text".to_string(),
                    text: Some(text),
                    data: None,
                    mime_type: None,
                },
                rmcp::model::PromptMessageContent::Image { image } => McpMessageContent {
                    kind: "image".to_string(),
                    text: None,
                    data: Some(image.data.clone()),
                    mime_type: Some(image.mime_type.clone()),
                },
                rmcp::model::PromptMessageContent::Resource { resource } => {
                    match &resource.resource {
                        ResourceContents::TextResourceContents {
                            text, mime_type, ..
                        } => McpMessageContent {
                            kind: "resource".to_string(),
                            text: Some(text.clone()),
                            data: None,
                            mime_type: mime_type.clone(),
                        },
                        ResourceContents::BlobResourceContents {
                            blob, mime_type, ..
                        } => McpMessageContent {
                            kind: "resource".to_string(),
                            text: None,
                            data: Some(blob.clone()),
                            mime_type: mime_type.clone(),
                        },
                    }
                }
                rmcp::model::PromptMessageContent::ResourceLink { link } => McpMessageContent {
                    kind: "resource".to_string(),
                    text: Some(link.uri.clone()),
                    data: None,
                    mime_type: link.mime_type.clone(),
                },
            };

            McpPromptMessage { role, content }
        })
        .collect::<Vec<_>>();

    GetPromptResponse {
        description: result.description,
        messages,
    }
}

fn map_resource_content(server: &McpServer, content: ResourceContents) -> McpResource {
    match content {
        ResourceContents::TextResourceContents {
            uri,
            mime_type,
            text,
            ..
        } => McpResource {
            server_id: server.id.clone(),
            server_name: server.name.clone(),
            name: uri.clone(),
            uri,
            description: None,
            mime_type,
            size: None,
            text: Some(text),
            blob: None,
        },
        ResourceContents::BlobResourceContents {
            uri,
            mime_type,
            blob,
            ..
        } => McpResource {
            server_id: server.id.clone(),
            server_name: server.name.clone(),
            name: uri.clone(),
            uri,
            description: None,
            mime_type,
            size: None,
            text: None,
            blob: Some(blob),
        },
    }
}

fn map_call_tool_response(result: rmcp::model::CallToolResult) -> McpCallToolResponse {
    let mut content = result
        .content
        .into_iter()
        .map(map_tool_content)
        .collect::<Vec<_>>();

    if content.is_empty() {
        if let Some(structured) = result.structured_content.as_ref() {
            content.push(McpToolResultContent {
                kind: "text".to_string(),
                text: Some(structured.to_string()),
                data: None,
                mime_type: None,
                resource: None,
            });
        }
    }

    McpCallToolResponse {
        content,
        is_error: result.is_error,
    }
}

fn map_tool_content(content: Content) -> McpToolResultContent {
    match content.raw {
        rmcp::model::RawContent::Text(text) => McpToolResultContent {
            kind: "text".to_string(),
            text: Some(text.text),
            data: None,
            mime_type: None,
            resource: None,
        },
        rmcp::model::RawContent::Image(image) => McpToolResultContent {
            kind: "image".to_string(),
            text: None,
            data: Some(image.data),
            mime_type: Some(image.mime_type),
            resource: None,
        },
        rmcp::model::RawContent::Audio(audio) => McpToolResultContent {
            kind: "audio".to_string(),
            text: None,
            data: Some(audio.data),
            mime_type: Some(audio.mime_type),
            resource: None,
        },
        rmcp::model::RawContent::Resource(resource) => {
            let payload = match resource.resource {
                ResourceContents::TextResourceContents {
                    uri,
                    mime_type,
                    text,
                    ..
                } => McpToolResourcePayload {
                    uri: Some(uri),
                    text: Some(text),
                    mime_type,
                    blob: None,
                },
                ResourceContents::BlobResourceContents {
                    uri,
                    mime_type,
                    blob,
                    ..
                } => McpToolResourcePayload {
                    uri: Some(uri),
                    text: None,
                    mime_type,
                    blob: Some(blob),
                },
            };

            McpToolResultContent {
                kind: "resource".to_string(),
                text: None,
                data: None,
                mime_type: None,
                resource: Some(payload),
            }
        }
        rmcp::model::RawContent::ResourceLink(link) => McpToolResultContent {
            kind: "resource".to_string(),
            text: None,
            data: None,
            mime_type: None,
            resource: Some(McpToolResourcePayload {
                uri: Some(link.uri),
                text: None,
                mime_type: link.mime_type,
                blob: None,
            }),
        },
    }
}

fn progress_token_key(token: &rmcp::model::ProgressToken) -> String {
    match &token.0 {
        NumberOrString::Number(num) => format!("n:{num}"),
        NumberOrString::String(text) => format!("s:{text}"),
    }
}

fn server_key(server: &McpServer) -> String {
    serde_json::to_string(&json!({
        "id": server.id,
        "name": server.name,
        "type": server.transport_type(),
        "baseUrl": server.base_url,
        "command": server.command,
        "args": server.args,
        "env": server.env,
        "headers": server.headers,
        "registryUrl": server.registry_url,
    }))
    .unwrap_or_else(|_| format!("{}:{}", server.id, server.name))
}

fn build_stdio_transport(
    server: &McpServer,
) -> Result<(TokioChildProcess, Option<tokio::process::ChildStderr>)> {
    let command = server
        .command
        .clone()
        .ok_or_else(|| DromeError::Message("MCP stdio server requires `command`".to_string()))?;

    let mut cmd = tokio::process::Command::new(command);
    if let Some(args) = &server.args {
        cmd.args(args);
    }
    if let Some(env) = &server.env {
        cmd.envs(env);
    }
    if let Some(cwd) = &server.dxt_path {
        cmd.current_dir(cwd);
    }

    TokioChildProcess::builder(cmd)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| DromeError::Message(format!("Failed to spawn MCP stdio process: {e}")))
}

fn build_http_transport(
    server: &McpServer,
) -> Result<StreamableHttpClientTransport<reqwest::Client>> {
    let url = server
        .base_url
        .clone()
        .ok_or_else(|| DromeError::Message("MCP HTTP server requires `baseUrl`".to_string()))?;

    let mut config = StreamableHttpClientTransportConfig::with_uri(url);
    let mut headers = HeaderMap::new();

    if let Some(custom_headers) = &server.headers {
        for (key, value) in custom_headers {
            if key.eq_ignore_ascii_case("authorization") {
                let bearer_prefix = "Bearer ";
                if let Some(token) = value.strip_prefix(bearer_prefix) {
                    config = config.auth_header(token.to_string());
                    continue;
                }
            }

            let name = HeaderName::from_bytes(key.as_bytes()).map_err(|e| {
                DromeError::Message(format!("Invalid HTTP header name `{key}`: {e}"))
            })?;
            let header_value = HeaderValue::from_str(value).map_err(|e| {
                DromeError::Message(format!("Invalid HTTP header value for `{key}`: {e}"))
            })?;
            headers.insert(name, header_value);
        }
    }

    let client = if headers.is_empty() {
        reqwest::Client::default()
    } else {
        reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| DromeError::Message(format!("Failed to build HTTP client: {e}")))?
    };

    Ok(StreamableHttpClientTransport::with_client(client, config))
}
