use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use cherry_ai::{
    ChatCompletionRequest, ChatTurn, OpenAiCompatibleClient, ProviderRegistry,
    RuntimeModelSelection,
};
use cherry_core::{
    ApiServerConfig, ApiServerRuntimeStatus, AppSettings, ChatMessage, Conversation, FileEntry,
    FileSourceKind, KnowledgeDocument, McpCallResult, McpServerDefinition, MessageRole, NoteEntry,
    ProtocolHandler, ProviderConfig, ProviderKind, ToolPermission,
};
use cherry_store::CherryStore;
use parking_lot::Mutex;
use tracing::info;
use uuid::Uuid;

pub struct AppServicesBuilder {
    pub db_path: PathBuf,
}

impl AppServicesBuilder {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    pub fn build(self) -> Result<AppServices> {
        let store = CherryStore::open(self.db_path)?;
        let settings = store.load_settings_or_default()?;
        let api_server_config = store.load_api_server_config_or_default()?;

        let providers = {
            let existing = store.list_providers()?;
            if existing.is_empty() {
                let defaults = vec![ProviderConfig::openai_default()];
                store.save_providers(&defaults)?;
                defaults
            } else {
                existing
            }
        };

        let mut registry = ProviderRegistry::default();
        registry.register(
            ProviderKind::OpenAiCompatible,
            Arc::new(OpenAiCompatibleClient::new()),
        );
        registry.register(
            ProviderKind::OpenRouter,
            Arc::new(OpenAiCompatibleClient::new()),
        );
        registry.register(
            ProviderKind::Ollama,
            Arc::new(OpenAiCompatibleClient::new()),
        );
        registry.register(
            ProviderKind::LmStudio,
            Arc::new(OpenAiCompatibleClient::new()),
        );

        Ok(AppServices {
            store: Arc::new(store),
            settings: Arc::new(Mutex::new(settings)),
            providers: Arc::new(Mutex::new(providers)),
            api_server_config: Arc::new(Mutex::new(api_server_config)),
            api_server_status: Arc::new(Mutex::new(ApiServerRuntimeStatus::Stopped)),
            api_server_runtime: Arc::new(Mutex::new(None)),
            registry: Arc::new(registry),
        })
    }
}

#[derive(Clone)]
pub struct AppServices {
    store: Arc<CherryStore>,
    settings: Arc<Mutex<AppSettings>>,
    providers: Arc<Mutex<Vec<ProviderConfig>>>,
    api_server_config: Arc<Mutex<ApiServerConfig>>,
    api_server_status: Arc<Mutex<ApiServerRuntimeStatus>>,
    api_server_runtime: Arc<Mutex<Option<ApiServerRuntime>>>,
    registry: Arc<ProviderRegistry>,
}

struct ApiServerRuntime {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupChannel {
    Local,
    WebDav,
    S3,
    Lan,
}

impl BackupChannel {
    fn directory(self) -> &'static str {
        match self {
            BackupChannel::Local => "local",
            BackupChannel::WebDav => "webdav",
            BackupChannel::S3 => "s3",
            BackupChannel::Lan => "lan",
        }
    }

    fn label(self) -> &'static str {
        match self {
            BackupChannel::Local => "Local",
            BackupChannel::WebDav => "WebDAV",
            BackupChannel::S3 => "S3",
            BackupChannel::Lan => "LAN",
        }
    }
}

impl AppServices {
    pub fn list_conversations(&self) -> Result<Vec<Conversation>> {
        self.store.list_conversations()
    }

    pub fn ensure_default_conversation(&self) -> Result<Conversation> {
        let existing = self.store.list_conversations()?;
        if let Some(conversation) = existing.first() {
            return Ok(conversation.clone());
        }

        self.store.create_conversation("New Chat")
    }

    pub fn seed_demo_workspace_data(&self) -> Result<()> {
        if self.list_notes()?.is_empty() {
            let note = NoteEntry::new(
                "Migration Checklist",
                "- [x] Workspace scaffold\n- [x] Core store\n- [ ] GPUI interactive pages",
            );
            self.upsert_note(&note)?;
        }

        if self.list_files()?.is_empty() {
            let file = FileEntry::local(
                "migration-overview.md",
                "./docs/migration-overview.md",
                "text/markdown",
                1024,
            );
            self.upsert_file(&file)?;
        }

        if self.list_knowledge_documents()?.is_empty() {
            let doc = KnowledgeDocument::pending(
                "Cherry Studio Feature List",
                "./cherry-studio/README.md",
                "text/markdown",
            );
            self.upsert_knowledge_document(&doc)?;
        }

        if self.list_mcp_servers()?.is_empty() {
            let mut server = McpServerDefinition::new("filesystem", "mock");
            server.args = vec!["server-filesystem".to_owned()];
            self.upsert_mcp_server(&server)?;
        }

        if self.list_tool_permissions()?.is_empty() {
            self.set_tool_permission("openclaw_query", true)?;
            self.set_tool_permission("list_files", true)?;
        }

        if self.list_protocol_handlers()?.is_empty() {
            self.add_sample_protocol_handler()?;
        }

        let mut api_config = self.api_server_config();
        if api_config.host.trim().is_empty() {
            api_config.host = "127.0.0.1".to_owned();
            self.save_api_server_config(&api_config)?;
        }

        Ok(())
    }

    pub fn create_conversation(&self, title: impl Into<String>) -> Result<Conversation> {
        self.store.create_conversation(title)
    }

    pub fn list_messages(&self, conversation_id: Uuid) -> Result<Vec<ChatMessage>> {
        self.store.list_messages(conversation_id)
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        self.store.save_settings(settings)?;
        *self.settings.lock() = settings.clone();
        Ok(())
    }

    pub fn update_language(&self, language: impl Into<String>) -> Result<AppSettings> {
        let mut settings = self.settings();
        settings.display.language = language.into();
        self.save_settings(&settings)?;
        Ok(settings)
    }

    pub fn cycle_theme(&self) -> Result<AppSettings> {
        let mut settings = self.settings();
        settings.display.theme = match settings.display.theme.as_str() {
            "light" => "dark".to_owned(),
            "dark" => "system".to_owned(),
            _ => "light".to_owned(),
        };
        self.save_settings(&settings)?;
        Ok(settings)
    }

    pub fn toggle_spell_check(&self) -> Result<AppSettings> {
        let mut settings = self.settings();
        settings.runtime.enable_spell_check = !settings.runtime.enable_spell_check;
        self.save_settings(&settings)?;
        Ok(settings)
    }

    pub fn set_default_model(
        &self,
        provider_name: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<AppSettings> {
        let mut settings = self.settings();
        settings.default_provider = Some(provider_name.into());
        settings.default_model = Some(model.into());
        self.save_settings(&settings)?;
        Ok(settings)
    }

    pub fn save_providers(&self, providers: &[ProviderConfig]) -> Result<()> {
        self.store.save_providers(providers)?;
        *self.providers.lock() = providers.to_vec();
        Ok(())
    }

    pub fn append_imported_message(
        &self,
        conversation_id: Uuid,
        role: MessageRole,
        content: String,
        provider: Option<String>,
        model: Option<String>,
        token_usage: Option<u32>,
    ) -> Result<()> {
        let message = match role {
            MessageRole::User => ChatMessage::user(conversation_id, content),
            MessageRole::Assistant => ChatMessage {
                provider,
                model,
                token_usage,
                ..ChatMessage::assistant(
                    conversation_id,
                    "ImportedProvider",
                    "imported-model",
                    content,
                )
            },
            MessageRole::System | MessageRole::Tool => ChatMessage {
                id: Uuid::new_v4(),
                conversation_id,
                role,
                content,
                model,
                provider,
                token_usage,
                attachments: Vec::new(),
                created_at: chrono::Utc::now(),
            },
        };
        self.store.append_message(&message)
    }

    pub fn export_backup_json(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create backup dir {}", parent.display()))?;
        }

        let conversations = self.list_conversations()?;
        let mut conversation_exports = Vec::new();
        for conversation in conversations {
            let messages = self.list_messages(conversation.id)?;
            let message_exports = messages
                .into_iter()
                .map(|message| {
                    serde_json::json!({
                        "role": message_role_to_str(message.role),
                        "content": message.content,
                        "model": message.model,
                        "provider": message.provider,
                        "token_usage": message.token_usage,
                    })
                })
                .collect::<Vec<_>>();

            conversation_exports.push(serde_json::json!({
                "title": conversation.title,
                "messages": message_exports,
            }));
        }

        let payload = serde_json::json!({
            "settings": self.settings(),
            "providers": self.providers(),
            "conversations": conversation_exports,
            "notes": self.list_notes()?,
            "files": self.list_files()?,
            "knowledge_documents": self.list_knowledge_documents()?,
        });

        fs::write(path, serde_json::to_string_pretty(&payload)?)
            .with_context(|| format!("failed to write backup {}", path.display()))?;
        Ok(())
    }

    pub fn import_backup_json(&self, path: impl AsRef<Path>) -> Result<crate::ImportReport> {
        crate::import_legacy_json(self, path)
    }

    pub fn set_backup_channel_enabled(
        &self,
        channel: BackupChannel,
        enabled: bool,
    ) -> Result<AppSettings> {
        let mut settings = self.settings();
        match channel {
            BackupChannel::Local => {}
            BackupChannel::WebDav => settings.backup.webdav_enabled = enabled,
            BackupChannel::S3 => settings.backup.s3_enabled = enabled,
            BackupChannel::Lan => settings.backup.lan_transfer_enabled = enabled,
        }
        self.save_settings(&settings)?;
        Ok(settings)
    }

    pub fn export_backup_to_channel(&self, channel: BackupChannel) -> Result<PathBuf> {
        if !self.is_backup_channel_enabled(channel) {
            return Err(anyhow!(
                "{} backup channel is disabled in settings",
                channel.label()
            ));
        }

        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let mut path = PathBuf::from("./data/backup");
        path.push(channel.directory());
        path.push(format!("backup-{timestamp}.json"));

        self.export_backup_json(&path)?;
        Ok(path)
    }

    pub fn import_latest_backup_from_channel(
        &self,
        channel: BackupChannel,
    ) -> Result<crate::ImportReport> {
        if !self.is_backup_channel_enabled(channel) {
            return Err(anyhow!(
                "{} backup channel is disabled in settings",
                channel.label()
            ));
        }
        let backup_dir = PathBuf::from("./data/backup").join(channel.directory());
        let path = latest_backup_file_in_dir(&backup_dir)?;
        self.import_backup_json(path)
    }

    pub fn list_files(&self) -> Result<Vec<FileEntry>> {
        self.store.list_file_entries()
    }

    pub fn upsert_file(&self, file: &FileEntry) -> Result<()> {
        self.store.upsert_file_entry(file)
    }

    pub fn delete_file(&self, file_id: Uuid) -> Result<()> {
        self.store.delete_file_entry(file_id)
    }

    pub fn list_notes(&self) -> Result<Vec<NoteEntry>> {
        self.store.list_note_entries()
    }

    pub fn upsert_note(&self, note: &NoteEntry) -> Result<()> {
        self.store.upsert_note_entry(note)
    }

    pub fn delete_note(&self, note_id: Uuid) -> Result<()> {
        self.store.delete_note_entry(note_id)
    }

    pub fn list_knowledge_documents(&self) -> Result<Vec<KnowledgeDocument>> {
        self.store.list_knowledge_documents()
    }

    pub fn upsert_knowledge_document(&self, document: &KnowledgeDocument) -> Result<()> {
        self.store.upsert_knowledge_document(document)
    }

    pub fn delete_knowledge_document(&self, document_id: Uuid) -> Result<()> {
        self.store.delete_knowledge_document(document_id)
    }

    pub fn list_mcp_servers(&self) -> Result<Vec<McpServerDefinition>> {
        self.store.list_mcp_servers()
    }

    pub fn upsert_mcp_server(&self, server: &McpServerDefinition) -> Result<()> {
        self.store.upsert_mcp_server(server)
    }

    pub fn delete_mcp_server(&self, server_id: Uuid) -> Result<()> {
        self.store.delete_mcp_server(server_id)
    }

    pub fn list_tool_permissions(&self) -> Result<Vec<ToolPermission>> {
        self.store.list_tool_permissions()
    }

    pub fn set_tool_permission(
        &self,
        tool_name: impl Into<String>,
        allowed: bool,
    ) -> Result<ToolPermission> {
        let tool_name = tool_name.into();
        let existing = self
            .list_tool_permissions()?
            .into_iter()
            .find(|entry| entry.tool_name == tool_name);
        let mut permission = existing.unwrap_or_else(|| ToolPermission::new(&tool_name, allowed));
        permission.allowed = allowed;
        self.store.upsert_tool_permission(&permission)?;
        Ok(permission)
    }

    pub fn remove_tool_permission(&self, permission_id: Uuid) -> Result<()> {
        self.store.delete_tool_permission(permission_id)
    }

    pub fn list_protocol_handlers(&self) -> Result<Vec<ProtocolHandler>> {
        self.store.list_protocol_handlers()
    }

    pub fn upsert_protocol_handler(&self, handler: &ProtocolHandler) -> Result<()> {
        self.store.upsert_protocol_handler(handler)
    }

    pub fn remove_protocol_handler(&self, handler_id: Uuid) -> Result<()> {
        self.store.delete_protocol_handler(handler_id)
    }

    pub fn add_sample_protocol_handler(&self) -> Result<ProtocolHandler> {
        let mut handler = ProtocolHandler::new("cherry", "open");
        handler.args = vec!["--url".to_owned(), "{url}".to_owned()];
        self.upsert_protocol_handler(&handler)?;
        Ok(handler)
    }

    pub fn resolve_protocol_url(&self, url: &str) -> Result<String> {
        let scheme = url
            .split_once(':')
            .map(|(value, _)| value)
            .ok_or_else(|| anyhow!("invalid protocol url: {url}"))?;

        let handlers = self.list_protocol_handlers()?;
        let handler = handlers
            .into_iter()
            .find(|entry| entry.enabled && entry.scheme.eq_ignore_ascii_case(scheme))
            .ok_or_else(|| anyhow!("no protocol handler for scheme {scheme}"))?;

        let args = handler
            .args
            .iter()
            .map(|entry| entry.replace("{url}", url))
            .collect::<Vec<_>>()
            .join(" ");
        Ok(format!("{} {}", handler.command, args).trim().to_owned())
    }

    pub fn call_mcp_tool(
        &self,
        server_id: Uuid,
        tool_name: &str,
        args_json: &str,
    ) -> Result<McpCallResult> {
        if !self.is_tool_allowed(tool_name)? {
            return Err(anyhow!("tool denied by permission policy: {tool_name}"));
        }

        let server = self
            .list_mcp_servers()?
            .into_iter()
            .find(|entry| entry.id == server_id)
            .ok_or_else(|| anyhow!("mcp server not found: {server_id}"))?;

        if server.command == "mock" {
            return Ok(McpCallResult {
                server_name: server.name,
                tool_name: tool_name.to_owned(),
                output: format!("Mock MCP call for tool={} args={}", tool_name, args_json),
            });
        }

        let mut command = Command::new(&server.command);
        command.args(&server.args);
        for (key, value) in &server.env {
            command.env(key, value);
        }
        command.env("CHERRY_MCP_TOOL", tool_name);
        command.env("CHERRY_MCP_ARGS_JSON", args_json);

        let output = command
            .output()
            .with_context(|| format!("failed to run mcp command: {}", server.command))?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let merged_output = if !stdout.is_empty() {
            stdout
        } else if !stderr.is_empty() {
            stderr
        } else {
            format!(
                "command={} exited with status {}",
                server.command,
                output
                    .status
                    .code()
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned())
            )
        };

        Ok(McpCallResult {
            server_name: server.name,
            tool_name: tool_name.to_owned(),
            output: merged_output,
        })
    }

    pub fn api_server_config(&self) -> ApiServerConfig {
        self.api_server_config.lock().clone()
    }

    pub fn api_server_status(&self) -> ApiServerRuntimeStatus {
        *self.api_server_status.lock()
    }

    pub fn save_api_server_config(&self, config: &ApiServerConfig) -> Result<()> {
        self.store.save_api_server_config(config)?;
        *self.api_server_config.lock() = config.clone();
        Ok(())
    }

    pub fn start_api_server(&self) -> Result<()> {
        if self.api_server_status() == ApiServerRuntimeStatus::Running {
            return Ok(());
        }

        let config = self.api_server_config();
        if !config.enabled {
            return Err(anyhow!(
                "api server config disabled; enable it before start"
            ));
        }

        let listener =
            TcpListener::bind((config.host.as_str(), config.port)).with_context(|| {
                format!(
                    "failed to bind api server on {}:{}",
                    config.host, config.port
                )
            })?;
        listener
            .set_nonblocking(true)
            .context("failed to set api listener non-blocking")?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_loop = Arc::clone(&stop);
        let host = config.host.clone();
        let port = config.port;

        let join = thread::spawn(move || {
            loop {
                if stop_loop.load(Ordering::Relaxed) {
                    break;
                }

                match listener.accept() {
                    Ok((mut stream, _peer)) => {
                        let mut read_buffer = [0_u8; 1024];
                        let _ = stream.read(&mut read_buffer);

                        let body = format!(
                            "{{\"service\":\"cherry-studio-rs-api\",\"status\":\"ok\",\"host\":\"{}\",\"port\":{},\"timestamp\":\"{}\"}}",
                            host,
                            port,
                            chrono::Utc::now().to_rfc3339()
                        );
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => {
                        thread::sleep(Duration::from_millis(50));
                    }
                }
            }
        });

        *self.api_server_runtime.lock() = Some(ApiServerRuntime {
            stop,
            join: Some(join),
        });
        *self.api_server_status.lock() = ApiServerRuntimeStatus::Running;
        Ok(())
    }

    pub fn stop_api_server(&self) {
        if let Some(mut runtime) = self.api_server_runtime.lock().take() {
            runtime.stop.store(true, Ordering::Relaxed);
            if let Some(join) = runtime.join.take() {
                let _ = join.join();
            }
        }
        *self.api_server_status.lock() = ApiServerRuntimeStatus::Stopped;
    }

    pub fn restart_api_server(&self) -> Result<()> {
        self.stop_api_server();
        self.start_api_server()
    }

    pub fn toggle_api_server_enabled(&self) -> Result<ApiServerConfig> {
        let mut config = self.api_server_config();
        config.enabled = !config.enabled;
        self.save_api_server_config(&config)?;
        Ok(config)
    }

    pub fn add_sample_file_entry(&self) -> Result<FileEntry> {
        let timestamp = chrono::Utc::now().timestamp();
        let file = FileEntry::local(
            format!("sample-{timestamp}.md"),
            format!("./samples/sample-{timestamp}.md"),
            "text/markdown",
            2048,
        );
        self.upsert_file(&file)?;
        Ok(file)
    }

    pub fn upload_local_file(
        &self,
        path: impl AsRef<Path>,
        mime_type: impl Into<String>,
    ) -> Result<FileEntry> {
        let path = path.as_ref();
        let metadata = fs::metadata(path)
            .with_context(|| format!("failed to read file metadata {}", path.display()))?;
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .ok_or_else(|| anyhow!("invalid file path: {}", path.display()))?;
        let file = FileEntry::local(
            name,
            path.to_string_lossy().to_string(),
            mime_type,
            metadata.len(),
        );
        self.upsert_file(&file)?;
        Ok(file)
    }

    pub fn add_sample_channel_file_entry(&self, source: FileSourceKind) -> Result<FileEntry> {
        let timestamp = chrono::Utc::now().timestamp();
        let (path, prefix) = match source {
            FileSourceKind::Local => (format!("./samples/local-{timestamp}.md"), "local"),
            FileSourceKind::WebDav => (format!("webdav://archive/{timestamp}.md"), "webdav"),
            FileSourceKind::S3 => (format!("s3://backup-bucket/{timestamp}.md"), "s3"),
            FileSourceKind::LanTransfer => (format!("lan://peer/{timestamp}.md"), "lan"),
        };

        let now = chrono::Utc::now();
        let file = FileEntry {
            id: Uuid::new_v4(),
            name: format!("{prefix}-sample-{timestamp}.md"),
            path,
            mime_type: "text/markdown".to_owned(),
            size_bytes: 1024,
            source,
            hash: None,
            created_at: now,
            updated_at: now,
        };
        self.upsert_file(&file)?;
        Ok(file)
    }

    pub fn add_sample_note_entry(&self) -> Result<NoteEntry> {
        let timestamp = chrono::Utc::now().format("%H:%M:%S");
        let note = NoteEntry::new(
            format!("Sample Note {timestamp}"),
            format!("# Sample\n\ncreated at {timestamp}"),
        );
        self.upsert_note(&note)?;
        Ok(note)
    }

    pub fn add_sample_knowledge_document(&self) -> Result<KnowledgeDocument> {
        let timestamp = chrono::Utc::now().timestamp();
        let document = KnowledgeDocument::pending(
            format!("Knowledge {timestamp}"),
            format!("./knowledge/{timestamp}.md"),
            "text/markdown",
        );
        self.upsert_knowledge_document(&document)?;
        Ok(document)
    }

    pub fn mark_first_knowledge_indexed(&self) -> Result<Option<KnowledgeDocument>> {
        let docs = self.list_knowledge_documents()?;
        let Some(mut doc) = docs.first().cloned() else {
            return Ok(None);
        };
        doc.status = cherry_core::KnowledgeStatus::Indexed;
        doc.updated_at = chrono::Utc::now();
        self.upsert_knowledge_document(&doc)?;
        Ok(Some(doc))
    }

    pub fn remove_first_file_entry(&self) -> Result<Option<Uuid>> {
        let files = self.list_files()?;
        let Some(file) = files.first() else {
            return Ok(None);
        };
        self.delete_file(file.id)?;
        Ok(Some(file.id))
    }

    pub fn remove_first_note_entry(&self) -> Result<Option<Uuid>> {
        let notes = self.list_notes()?;
        let Some(note) = notes.first() else {
            return Ok(None);
        };
        self.delete_note(note.id)?;
        Ok(Some(note.id))
    }

    pub fn remove_first_knowledge_document(&self) -> Result<Option<Uuid>> {
        let docs = self.list_knowledge_documents()?;
        let Some(doc) = docs.first() else {
            return Ok(None);
        };
        self.delete_knowledge_document(doc.id)?;
        Ok(Some(doc.id))
    }

    pub fn add_sample_mcp_server(&self) -> Result<McpServerDefinition> {
        let timestamp = chrono::Utc::now().timestamp();
        let mut server = McpServerDefinition::new(format!("server-{timestamp}"), "mock");
        server.args = vec![
            "@modelcontextprotocol/server-filesystem".to_owned(),
            "--root".to_owned(),
            ".".to_owned(),
        ];
        self.upsert_mcp_server(&server)?;
        Ok(server)
    }

    pub fn remove_first_mcp_server(&self) -> Result<Option<Uuid>> {
        let servers = self.list_mcp_servers()?;
        let Some(server) = servers.first() else {
            return Ok(None);
        };
        self.delete_mcp_server(server.id)?;
        Ok(Some(server.id))
    }

    pub fn remove_first_protocol_handler(&self) -> Result<Option<Uuid>> {
        let handlers = self.list_protocol_handlers()?;
        let Some(handler) = handlers.first() else {
            return Ok(None);
        };
        self.remove_protocol_handler(handler.id)?;
        Ok(Some(handler.id))
    }

    pub fn is_backup_channel_enabled(&self, channel: BackupChannel) -> bool {
        let settings = self.settings();
        match channel {
            BackupChannel::Local => true,
            BackupChannel::WebDav => settings.backup.webdav_enabled,
            BackupChannel::S3 => settings.backup.s3_enabled,
            BackupChannel::Lan => settings.backup.lan_transfer_enabled,
        }
    }

    pub fn settings(&self) -> AppSettings {
        self.settings.lock().clone()
    }

    pub fn providers(&self) -> Vec<ProviderConfig> {
        self.providers.lock().clone()
    }

    pub fn is_tool_allowed(&self, tool_name: &str) -> Result<bool> {
        let permissions = self.list_tool_permissions()?;
        if let Some(permission) = permissions
            .into_iter()
            .find(|entry| entry.tool_name.eq_ignore_ascii_case(tool_name))
        {
            Ok(permission.allowed)
        } else {
            Ok(true)
        }
    }

    pub async fn send_user_message(
        &self,
        conversation_id: Uuid,
        content: String,
    ) -> Result<ChatMessage> {
        let user_message = ChatMessage::user(conversation_id, content);
        self.store.append_message(&user_message)?;

        let settings = self.settings();
        let providers = self.providers();
        let provider = pick_provider(&providers, settings.default_provider.as_deref())?;
        let model = pick_model(provider, settings.default_model.as_deref())?;

        let history = self
            .store
            .list_messages(conversation_id)?
            .into_iter()
            .map(|message| ChatTurn {
                role: message.role,
                content: message.content,
            })
            .collect::<Vec<_>>();

        let completion_request = ChatCompletionRequest {
            selection: RuntimeModelSelection {
                provider_name: provider.name.clone(),
                model: model.clone(),
            },
            system_prompt: None,
            turns: history,
            temperature: 0.7,
        };

        let completion = self
            .registry
            .complete(provider, &completion_request)
            .await
            .map_err(|error| anyhow!("failed provider completion: {error}"))?;

        let mut assistant_message = ChatMessage::assistant(
            conversation_id,
            completion.provider,
            completion.model,
            completion.content,
        );
        assistant_message.token_usage = completion.token_usage;

        self.store.append_message(&assistant_message)?;

        info!(conversation_id = %conversation_id, "chat completion saved");

        Ok(assistant_message)
    }

    pub fn seed_fake_reply(&self, conversation_id: Uuid, user_input: &str) -> Result<ChatMessage> {
        let message = ChatMessage::assistant(
            conversation_id,
            "MockProvider",
            "mock-model",
            format!(
                "[迁移中的占位回复] 你发送了：{}\n\n真实模型调用需要在设置里配置 API Key。",
                user_input
            ),
        );
        self.store.append_message(&message)?;
        Ok(message)
    }

    pub fn append_local_user_message(
        &self,
        conversation_id: Uuid,
        user_input: String,
    ) -> Result<()> {
        let message = ChatMessage::user(conversation_id, user_input);
        self.store.append_message(&message)
    }

    pub fn summarize_feature_progress(&self) -> String {
        let providers = self.providers();
        let enabled_count = providers.iter().filter(|provider| provider.enabled).count();
        let notes = self.list_notes().map(|items| items.len()).unwrap_or(0);
        let files = self.list_files().map(|items| items.len()).unwrap_or(0);
        let knowledge_docs = self
            .list_knowledge_documents()
            .map(|items| items.len())
            .unwrap_or(0);
        let mcp_servers = self
            .list_mcp_servers()
            .map(|items| items.len())
            .unwrap_or(0);
        let tool_permissions = self
            .list_tool_permissions()
            .map(|items| items.len())
            .unwrap_or(0);
        let protocol_handlers = self
            .list_protocol_handlers()
            .map(|items| items.len())
            .unwrap_or(0);
        let api_server_status = match self.api_server_status() {
            ApiServerRuntimeStatus::Stopped => "stopped",
            ApiServerRuntimeStatus::Running => "running",
        };
        format!(
            "Core ready: {} providers ({} enabled), {} notes, {} files, {} knowledge docs, {} mcp servers, {} tool perms, {} protocols, api:{}",
            providers.len(),
            enabled_count,
            notes,
            files,
            knowledge_docs,
            mcp_servers,
            tool_permissions,
            protocol_handlers,
            api_server_status
        )
    }

    pub fn can_call_real_model(&self) -> bool {
        self.providers().into_iter().any(|provider| {
            provider.enabled
                && provider.kind == ProviderKind::OpenAiCompatible
                && provider
                    .api_key
                    .as_deref()
                    .map(|key| !key.trim().is_empty())
                    .unwrap_or(false)
        })
    }

    pub async fn send_message_with_fallback(
        &self,
        conversation_id: Uuid,
        user_input: String,
    ) -> Result<ChatMessage> {
        if self.can_call_real_model() {
            self.send_user_message(conversation_id, user_input).await
        } else {
            self.append_local_user_message(conversation_id, user_input.clone())?;
            self.seed_fake_reply(conversation_id, &user_input)
        }
    }

    pub async fn send_message_streaming_with_fallback(
        &self,
        conversation_id: Uuid,
        user_input: String,
    ) -> Result<Vec<String>> {
        let message = self
            .send_message_with_fallback(conversation_id, user_input)
            .await?;

        let mut chunks = message
            .content
            .split_whitespace()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();

        if chunks.is_empty() {
            chunks.push(message.content);
        }

        Ok(chunks)
    }
}

fn message_role_to_str(role: MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

fn pick_provider<'a>(
    providers: &'a [ProviderConfig],
    wanted: Option<&str>,
) -> Result<&'a ProviderConfig> {
    if let Some(wanted) = wanted
        && let Some(provider) = providers.iter().find(|provider| provider.name == wanted)
    {
        return Ok(provider);
    }

    providers
        .iter()
        .find(|provider| provider.enabled)
        .or_else(|| providers.first())
        .ok_or_else(|| anyhow!("no providers configured"))
}

fn pick_model(provider: &ProviderConfig, wanted: Option<&str>) -> Result<String> {
    if let Some(model) = wanted
        && provider.models.iter().any(|entry| entry.id == model)
    {
        return Ok(model.to_owned());
    }

    provider
        .models
        .first()
        .map(|entry| entry.id.clone())
        .ok_or_else(|| anyhow!("provider {} has no models", provider.name))
}

fn latest_backup_file_in_dir(path: &Path) -> Result<PathBuf> {
    if !path.exists() {
        return Err(anyhow!("backup directory not found: {}", path.display()));
    }

    let mut backups = fs::read_dir(path)
        .with_context(|| format!("failed to read backup directory {}", path.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|entry| entry.extension().map(|ext| ext == "json").unwrap_or(false))
        .collect::<Vec<_>>();

    backups.sort();
    backups
        .pop()
        .ok_or_else(|| anyhow!("no backup json in {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread::sleep,
        time::Duration,
    };

    #[test]
    fn service_can_manage_route_data() {
        let mut db_path = std::env::temp_dir();
        db_path.push(format!("cherry_services_{}.db", Uuid::new_v4()));

        let services = AppServicesBuilder::new(db_path)
            .build()
            .expect("build services");

        services.add_sample_file_entry().expect("add file");
        assert_eq!(services.list_files().expect("list files").len(), 1);
        services.remove_first_file_entry().expect("remove file");
        assert!(services.list_files().expect("re-list files").is_empty());

        services.add_sample_note_entry().expect("add note");
        assert_eq!(services.list_notes().expect("list notes").len(), 1);
        services.remove_first_note_entry().expect("remove note");
        assert!(services.list_notes().expect("re-list notes").is_empty());

        services
            .add_sample_knowledge_document()
            .expect("add knowledge doc");
        assert_eq!(
            services
                .list_knowledge_documents()
                .expect("list knowledge docs")
                .len(),
            1
        );
        services
            .mark_first_knowledge_indexed()
            .expect("mark knowledge indexed");
        services
            .remove_first_knowledge_document()
            .expect("remove knowledge");
        assert!(
            services
                .list_knowledge_documents()
                .expect("re-list knowledge docs")
                .is_empty()
        );
    }

    #[test]
    fn service_can_manage_mcp_and_api_flags() {
        let mut db_path = std::env::temp_dir();
        db_path.push(format!("cherry_services_mcp_{}.db", Uuid::new_v4()));

        let services = AppServicesBuilder::new(db_path)
            .build()
            .expect("build services");

        services.add_sample_mcp_server().expect("add mcp");
        assert_eq!(services.list_mcp_servers().expect("list mcp").len(), 1);

        let first = services
            .list_mcp_servers()
            .expect("list mcp for call")
            .first()
            .cloned()
            .expect("has first mcp");
        let call = services
            .call_mcp_tool(first.id, "list_files", r#"{"path":"."}"#)
            .expect("call mcp");
        assert!(call.output.contains("Mock MCP call"));

        services.remove_first_mcp_server().expect("remove mcp");
        assert!(services.list_mcp_servers().expect("re-list mcp").is_empty());

        let mut config = services
            .toggle_api_server_enabled()
            .expect("toggle api enabled");
        assert!(config.enabled);
        config.host = "127.0.0.1".to_owned();
        config.port = free_port();
        services
            .save_api_server_config(&config)
            .expect("save api config");

        services.start_api_server().expect("start api");
        assert_eq!(
            services.api_server_status(),
            ApiServerRuntimeStatus::Running
        );

        let response = wait_http_response(config.port).expect("get http response");
        assert!(response.contains("200 OK"));
        assert!(response.contains("cherry-studio-rs-api"));

        services.stop_api_server();
        assert_eq!(
            services.api_server_status(),
            ApiServerRuntimeStatus::Stopped
        );
    }

    #[test]
    fn service_can_export_and_import_backup() {
        let mut source_db = std::env::temp_dir();
        source_db.push(format!("cherry_services_backup_src_{}.db", Uuid::new_v4()));
        let source = AppServicesBuilder::new(source_db)
            .build()
            .expect("build source services");
        source.seed_demo_workspace_data().expect("seed source");
        source.add_sample_note_entry().expect("add source note");
        source.add_sample_file_entry().expect("add source file");

        let mut backup_path = std::env::temp_dir();
        backup_path.push(format!("cherry_services_backup_{}.json", Uuid::new_v4()));
        source
            .export_backup_json(&backup_path)
            .expect("export backup");
        assert!(backup_path.exists());

        let mut target_db = std::env::temp_dir();
        target_db.push(format!("cherry_services_backup_tgt_{}.db", Uuid::new_v4()));
        let target = AppServicesBuilder::new(target_db)
            .build()
            .expect("build target services");

        let report = target
            .import_backup_json(&backup_path)
            .expect("import backup");
        assert!(report.notes >= 1);
        assert!(report.files >= 1);

        std::fs::remove_file(backup_path).ok();
    }

    #[test]
    fn service_can_chat_stream_and_upload_file() {
        let mut db_path = std::env::temp_dir();
        db_path.push(format!("cherry_services_chat_{}.db", Uuid::new_v4()));
        let services = AppServicesBuilder::new(db_path)
            .build()
            .expect("build services");

        let conversation = services
            .create_conversation("streaming")
            .expect("create conversation");
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let chunks = runtime
            .block_on(services.send_message_streaming_with_fallback(
                conversation.id,
                "你好，给出迁移总结".to_owned(),
            ))
            .expect("send streaming prompt");
        assert!(!chunks.is_empty());

        let messages = services
            .list_messages(conversation.id)
            .expect("list conversation messages");
        assert!(messages.len() >= 2);
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[1].role, MessageRole::Assistant);

        let mut local_file = std::env::temp_dir();
        local_file.push(format!("upload_{}.md", Uuid::new_v4()));
        std::fs::write(&local_file, "# upload content").expect("write upload file");
        let uploaded = services
            .upload_local_file(&local_file, "text/markdown")
            .expect("upload local file");
        assert!(uploaded.name.ends_with(".md"));
        assert!(
            services
                .list_files()
                .expect("list files after upload")
                .iter()
                .any(|entry| entry.id == uploaded.id)
        );
        std::fs::remove_file(local_file).ok();
    }

    #[test]
    fn service_can_manage_backup_channels_permissions_and_protocols() {
        let mut db_path = std::env::temp_dir();
        db_path.push(format!("cherry_services_platform_{}.db", Uuid::new_v4()));
        let services = AppServicesBuilder::new(db_path)
            .build()
            .expect("build services");
        services.seed_demo_workspace_data().expect("seed workspace");
        services.add_sample_note_entry().expect("add note");

        services
            .set_backup_channel_enabled(BackupChannel::WebDav, true)
            .expect("enable webdav");
        services
            .set_backup_channel_enabled(BackupChannel::S3, true)
            .expect("enable s3");
        services
            .set_backup_channel_enabled(BackupChannel::Lan, true)
            .expect("enable lan");

        let webdav = services
            .export_backup_to_channel(BackupChannel::WebDav)
            .expect("export webdav backup");
        let s3 = services
            .export_backup_to_channel(BackupChannel::S3)
            .expect("export s3 backup");
        let lan = services
            .export_backup_to_channel(BackupChannel::Lan)
            .expect("export lan backup");
        assert!(webdav.exists());
        assert!(s3.exists());
        assert!(lan.exists());

        let imported = services
            .import_latest_backup_from_channel(BackupChannel::WebDav)
            .expect("import webdav backup");
        assert!(imported.notes >= 1);

        services
            .set_tool_permission("openclaw_query", false)
            .expect("deny tool permission");
        let server = services.add_sample_mcp_server().expect("add sample mcp");
        let denied = services.call_mcp_tool(server.id, "openclaw_query", "{}");
        assert!(denied.is_err());

        services
            .set_tool_permission("openclaw_query", true)
            .expect("allow tool permission");
        let allowed = services
            .call_mcp_tool(server.id, "openclaw_query", "{}")
            .expect("call allowed tool");
        assert!(allowed.output.contains("Mock MCP call"));

        let handler = services
            .add_sample_protocol_handler()
            .expect("add protocol handler");
        let resolved = services
            .resolve_protocol_url("cherry://workspace/open")
            .expect("resolve protocol url");
        assert!(resolved.contains("open"));
        assert!(resolved.contains("cherry://workspace/open"));

        services
            .remove_protocol_handler(handler.id)
            .expect("remove protocol handler");
    }

    fn free_port() -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind free port");
        let port = listener.local_addr().expect("read free port addr").port();
        drop(listener);
        port
    }

    fn wait_http_response(port: u16) -> Result<String> {
        for _ in 0..40 {
            if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
                stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")?;
                stream.flush()?;
                let mut response = String::new();
                stream.read_to_string(&mut response)?;
                if !response.is_empty() {
                    return Ok(response);
                }
            }
            sleep(Duration::from_millis(30));
        }
        Err(anyhow!("api server response timeout"))
    }
}
