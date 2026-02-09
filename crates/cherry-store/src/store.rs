use std::{path::Path, str::FromStr};

use anyhow::{Context, Result, anyhow};
use cherry_core::{
    ApiServerConfig, AppSettings, ChatMessage, Conversation, FileEntry, FileSourceKind,
    KnowledgeDocument, KnowledgeStatus, McpServerDefinition, MessageAttachment, MessageRole,
    NoteEntry, ProtocolHandler, ProviderConfig, ProviderKind, ToolPermission,
};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rusqlite::{Connection, params};
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::migrations::MIGRATIONS;

pub struct CherryStore {
    conn: Mutex<Connection>,
}

impl CherryStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create parent dir for {}", path.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("failed to open sqlite db {}", path.display()))?;
        conn.pragma_update(None, "foreign_keys", true)?;

        for sql in MIGRATIONS {
            conn.execute(sql, [])
                .with_context(|| format!("failed migration sql: {sql}"))?;
        }

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        self.set_json("app", settings)
    }

    pub fn load_settings(&self) -> Result<AppSettings> {
        self.get_json("app")?
            .ok_or_else(|| anyhow!("settings not found"))
    }

    pub fn load_settings_or_default(&self) -> Result<AppSettings> {
        match self.load_settings() {
            Ok(value) => Ok(value),
            Err(_) => {
                let default_value = AppSettings::default();
                self.save_settings(&default_value)?;
                Ok(default_value)
            }
        }
    }

    pub fn save_providers(&self, providers: &[ProviderConfig]) -> Result<()> {
        let conn = self.conn.lock();
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM providers", [])?;

        for provider in providers {
            let kind = match provider.kind {
                ProviderKind::OpenAiCompatible => "openai-compatible",
                ProviderKind::Anthropic => "anthropic",
                ProviderKind::Gemini => "gemini",
                ProviderKind::OpenRouter => "openrouter",
                ProviderKind::Ollama => "ollama",
                ProviderKind::LmStudio => "lmstudio",
                ProviderKind::SiliconFlow => "siliconflow",
                ProviderKind::Other => "other",
            };

            tx.execute(
                "
                INSERT INTO providers (id, kind, name, base_url, api_key, enabled, models_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ",
                params![
                    provider.id.to_string(),
                    kind,
                    provider.name,
                    provider.base_url,
                    provider.api_key,
                    provider.enabled,
                    serde_json::to_string(&provider.models)?,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn list_providers(&self) -> Result<Vec<ProviderConfig>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, kind, name, base_url, api_key, enabled, models_json FROM providers ORDER BY name",
        )?;

        let mut rows = stmt.query([])?;
        let mut output = Vec::new();

        while let Some(row) = rows.next()? {
            let kind = match row.get::<_, String>(1)?.as_str() {
                "openai-compatible" => ProviderKind::OpenAiCompatible,
                "anthropic" => ProviderKind::Anthropic,
                "gemini" => ProviderKind::Gemini,
                "openrouter" => ProviderKind::OpenRouter,
                "ollama" => ProviderKind::Ollama,
                "lmstudio" => ProviderKind::LmStudio,
                "siliconflow" => ProviderKind::SiliconFlow,
                _ => ProviderKind::Other,
            };

            output.push(ProviderConfig {
                id: Uuid::from_str(&row.get::<_, String>(0)?)?,
                kind,
                name: row.get(2)?,
                base_url: row.get(3)?,
                api_key: row.get(4)?,
                enabled: row.get(5)?,
                models: serde_json::from_str(&row.get::<_, String>(6)?)?,
            });
        }

        Ok(output)
    }

    pub fn upsert_file_entry(&self, file: &FileEntry) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "
            INSERT INTO files (id, name, path, mime_type, size_bytes, source, hash, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
              name = excluded.name,
              path = excluded.path,
              mime_type = excluded.mime_type,
              size_bytes = excluded.size_bytes,
              source = excluded.source,
              hash = excluded.hash,
              updated_at = excluded.updated_at
            ",
            params![
                file.id.to_string(),
                file.name,
                file.path,
                file.mime_type,
                file.size_bytes,
                serialize_file_source(file.source),
                file.hash,
                file.created_at.to_rfc3339(),
                file.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_file_entries(&self) -> Result<Vec<FileEntry>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, path, mime_type, size_bytes, source, hash, created_at, updated_at
             FROM files ORDER BY updated_at DESC",
        )?;

        let mut rows = stmt.query([])?;
        let mut output = Vec::new();

        while let Some(row) = rows.next()? {
            output.push(FileEntry {
                id: Uuid::from_str(&row.get::<_, String>(0)?)?,
                name: row.get(1)?,
                path: row.get(2)?,
                mime_type: row.get(3)?,
                size_bytes: row.get(4)?,
                source: parse_file_source(&row.get::<_, String>(5)?)?,
                hash: row.get(6)?,
                created_at: parse_date(&row.get::<_, String>(7)?)?,
                updated_at: parse_date(&row.get::<_, String>(8)?)?,
            });
        }

        Ok(output)
    }

    pub fn delete_file_entry(&self, file_id: Uuid) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM files WHERE id = ?1",
            params![file_id.to_string()],
        )?;
        Ok(())
    }

    pub fn upsert_note_entry(&self, note: &NoteEntry) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "
            INSERT INTO notes (id, title, markdown, tags_json, pinned, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(id) DO UPDATE SET
              title = excluded.title,
              markdown = excluded.markdown,
              tags_json = excluded.tags_json,
              pinned = excluded.pinned,
              updated_at = excluded.updated_at
            ",
            params![
                note.id.to_string(),
                note.title,
                note.markdown,
                serde_json::to_string(&note.tags)?,
                note.pinned,
                note.created_at.to_rfc3339(),
                note.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_note_entries(&self) -> Result<Vec<NoteEntry>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, title, markdown, tags_json, pinned, created_at, updated_at
             FROM notes ORDER BY pinned DESC, updated_at DESC",
        )?;

        let mut rows = stmt.query([])?;
        let mut output = Vec::new();

        while let Some(row) = rows.next()? {
            output.push(NoteEntry {
                id: Uuid::from_str(&row.get::<_, String>(0)?)?,
                title: row.get(1)?,
                markdown: row.get(2)?,
                tags: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                pinned: row.get(4)?,
                created_at: parse_date(&row.get::<_, String>(5)?)?,
                updated_at: parse_date(&row.get::<_, String>(6)?)?,
            });
        }

        Ok(output)
    }

    pub fn delete_note_entry(&self, note_id: Uuid) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM notes WHERE id = ?1",
            params![note_id.to_string()],
        )?;
        Ok(())
    }

    pub fn upsert_knowledge_document(&self, document: &KnowledgeDocument) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "
            INSERT INTO knowledge_documents (id, title, source_path, mime_type, status, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(id) DO UPDATE SET
              title = excluded.title,
              source_path = excluded.source_path,
              mime_type = excluded.mime_type,
              status = excluded.status,
              updated_at = excluded.updated_at
            ",
            params![
                document.id.to_string(),
                document.title,
                document.source_path,
                document.mime_type,
                serialize_knowledge_status(document.status),
                document.created_at.to_rfc3339(),
                document.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_knowledge_documents(&self) -> Result<Vec<KnowledgeDocument>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, title, source_path, mime_type, status, created_at, updated_at
             FROM knowledge_documents ORDER BY updated_at DESC",
        )?;

        let mut rows = stmt.query([])?;
        let mut output = Vec::new();

        while let Some(row) = rows.next()? {
            output.push(KnowledgeDocument {
                id: Uuid::from_str(&row.get::<_, String>(0)?)?,
                title: row.get(1)?,
                source_path: row.get(2)?,
                mime_type: row.get(3)?,
                status: parse_knowledge_status(&row.get::<_, String>(4)?)?,
                created_at: parse_date(&row.get::<_, String>(5)?)?,
                updated_at: parse_date(&row.get::<_, String>(6)?)?,
            });
        }

        Ok(output)
    }

    pub fn delete_knowledge_document(&self, document_id: Uuid) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM knowledge_documents WHERE id = ?1",
            params![document_id.to_string()],
        )?;
        Ok(())
    }

    pub fn save_api_server_config(&self, config: &ApiServerConfig) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "
            INSERT INTO api_server_config (key, value_json)
            VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json
            ",
            params!["api", serde_json::to_string(config)?],
        )?;
        Ok(())
    }

    pub fn load_api_server_config_or_default(&self) -> Result<ApiServerConfig> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT value_json FROM api_server_config WHERE key = ?1")?;
        let mut rows = stmt.query(params!["api"])?;

        let Some(row) = rows.next()? else {
            let default_value = ApiServerConfig::default();
            drop(rows);
            drop(stmt);
            drop(conn);
            self.save_api_server_config(&default_value)?;
            return Ok(default_value);
        };

        let value_json: String = row.get(0)?;
        Ok(serde_json::from_str(&value_json)?)
    }

    pub fn upsert_mcp_server(&self, server: &McpServerDefinition) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "
            INSERT INTO mcp_servers (id, name, command, args_json, env_json, enabled)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                command = excluded.command,
                args_json = excluded.args_json,
                env_json = excluded.env_json,
                enabled = excluded.enabled
            ",
            params![
                server.id.to_string(),
                server.name,
                server.command,
                serde_json::to_string(&server.args)?,
                serde_json::to_string(&server.env)?,
                server.enabled,
            ],
        )?;
        Ok(())
    }

    pub fn list_mcp_servers(&self) -> Result<Vec<McpServerDefinition>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, command, args_json, env_json, enabled FROM mcp_servers ORDER BY name",
        )?;

        let mut rows = stmt.query([])?;
        let mut output = Vec::new();
        while let Some(row) = rows.next()? {
            output.push(McpServerDefinition {
                id: Uuid::from_str(&row.get::<_, String>(0)?)?,
                name: row.get(1)?,
                command: row.get(2)?,
                args: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                env: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                enabled: row.get(5)?,
            });
        }

        Ok(output)
    }

    pub fn delete_mcp_server(&self, server_id: Uuid) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM mcp_servers WHERE id = ?1",
            params![server_id.to_string()],
        )?;
        Ok(())
    }

    pub fn upsert_tool_permission(&self, permission: &ToolPermission) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "
            INSERT INTO tool_permissions (id, tool_name, allowed, scope)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(tool_name) DO UPDATE SET
              allowed = excluded.allowed,
              scope = excluded.scope
            ",
            params![
                permission.id.to_string(),
                permission.tool_name,
                permission.allowed,
                permission.scope,
            ],
        )?;
        Ok(())
    }

    pub fn list_tool_permissions(&self) -> Result<Vec<ToolPermission>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, tool_name, allowed, scope FROM tool_permissions ORDER BY tool_name",
        )?;

        let mut rows = stmt.query([])?;
        let mut output = Vec::new();
        while let Some(row) = rows.next()? {
            output.push(ToolPermission {
                id: Uuid::from_str(&row.get::<_, String>(0)?)?,
                tool_name: row.get(1)?,
                allowed: row.get(2)?,
                scope: row.get(3)?,
            });
        }
        Ok(output)
    }

    pub fn delete_tool_permission(&self, permission_id: Uuid) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM tool_permissions WHERE id = ?1",
            params![permission_id.to_string()],
        )?;
        Ok(())
    }

    pub fn upsert_protocol_handler(&self, handler: &ProtocolHandler) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "
            INSERT INTO protocol_handlers (id, scheme, command, args_json, enabled)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(scheme) DO UPDATE SET
              command = excluded.command,
              args_json = excluded.args_json,
              enabled = excluded.enabled
            ",
            params![
                handler.id.to_string(),
                handler.scheme,
                handler.command,
                serde_json::to_string(&handler.args)?,
                handler.enabled,
            ],
        )?;
        Ok(())
    }

    pub fn list_protocol_handlers(&self) -> Result<Vec<ProtocolHandler>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, scheme, command, args_json, enabled FROM protocol_handlers ORDER BY scheme",
        )?;

        let mut rows = stmt.query([])?;
        let mut output = Vec::new();
        while let Some(row) = rows.next()? {
            output.push(ProtocolHandler {
                id: Uuid::from_str(&row.get::<_, String>(0)?)?,
                scheme: row.get(1)?,
                command: row.get(2)?,
                args: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                enabled: row.get(4)?,
            });
        }
        Ok(output)
    }

    pub fn delete_protocol_handler(&self, handler_id: Uuid) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM protocol_handlers WHERE id = ?1",
            params![handler_id.to_string()],
        )?;
        Ok(())
    }

    pub fn create_conversation(&self, title: impl Into<String>) -> Result<Conversation> {
        let conversation = Conversation::new(title);
        let conn = self.conn.lock();
        conn.execute(
            "
            INSERT INTO conversations (id, title, assistant_id, archived, pinned, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                conversation.id.to_string(),
                conversation.title,
                conversation.assistant_id.map(|value| value.to_string()),
                conversation.archived,
                conversation.pinned,
                conversation.created_at.to_rfc3339(),
                conversation.updated_at.to_rfc3339(),
            ],
        )?;

        Ok(conversation)
    }

    pub fn list_conversations(&self) -> Result<Vec<Conversation>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, title, assistant_id, archived, pinned, created_at, updated_at
             FROM conversations ORDER BY updated_at DESC",
        )?;

        let mut rows = stmt.query([])?;
        let mut output = Vec::new();

        while let Some(row) = rows.next()? {
            let assistant_id = row
                .get::<_, Option<String>>(2)?
                .map(|value| Uuid::from_str(&value))
                .transpose()?;

            output.push(Conversation {
                id: Uuid::from_str(&row.get::<_, String>(0)?)?,
                title: row.get(1)?,
                assistant_id,
                archived: row.get(3)?,
                pinned: row.get(4)?,
                created_at: parse_date(&row.get::<_, String>(5)?)?,
                updated_at: parse_date(&row.get::<_, String>(6)?)?,
            });
        }

        Ok(output)
    }

    pub fn append_message(&self, message: &ChatMessage) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "
            INSERT INTO messages (id, conversation_id, role, content, model, provider, token_usage, attachments_json, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ",
            params![
                message.id.to_string(),
                message.conversation_id.to_string(),
                serialize_role(&message.role),
                message.content,
                message.model,
                message.provider,
                message.token_usage,
                serde_json::to_string(&message.attachments)?,
                message.created_at.to_rfc3339(),
            ],
        )?;

        conn.execute(
            "UPDATE conversations SET updated_at = ?2 WHERE id = ?1",
            params![
                message.conversation_id.to_string(),
                message.created_at.to_rfc3339()
            ],
        )?;

        Ok(())
    }

    pub fn list_messages(&self, conversation_id: Uuid) -> Result<Vec<ChatMessage>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "
            SELECT id, conversation_id, role, content, model, provider, token_usage, attachments_json, created_at
            FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC
            ",
        )?;

        let mut rows = stmt.query(params![conversation_id.to_string()])?;
        let mut output = Vec::new();

        while let Some(row) = rows.next()? {
            let role = parse_role(&row.get::<_, String>(2)?)?;
            let attachments: Vec<MessageAttachment> =
                serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default();

            output.push(ChatMessage {
                id: Uuid::from_str(&row.get::<_, String>(0)?)?,
                conversation_id: Uuid::from_str(&row.get::<_, String>(1)?)?,
                role,
                content: row.get(3)?,
                model: row.get(4)?,
                provider: row.get(5)?,
                token_usage: row.get(6)?,
                attachments,
                created_at: parse_date(&row.get::<_, String>(8)?)?,
            });
        }

        Ok(output)
    }

    fn set_json<T: serde::Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO settings (key, value_json) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    fn get_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT value_json FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;

        let Some(row) = rows.next()? else {
            return Ok(None);
        };

        let value_json: String = row.get(0)?;
        let value = serde_json::from_str(&value_json)?;
        Ok(Some(value))
    }
}

fn serialize_role(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

fn serialize_file_source(source: FileSourceKind) -> &'static str {
    match source {
        FileSourceKind::Local => "local",
        FileSourceKind::WebDav => "webdav",
        FileSourceKind::S3 => "s3",
        FileSourceKind::LanTransfer => "lan",
    }
}

fn parse_file_source(value: &str) -> Result<FileSourceKind> {
    match value {
        "local" => Ok(FileSourceKind::Local),
        "webdav" => Ok(FileSourceKind::WebDav),
        "s3" => Ok(FileSourceKind::S3),
        "lan" => Ok(FileSourceKind::LanTransfer),
        _ => Err(anyhow!("unknown file source: {value}")),
    }
}

fn serialize_knowledge_status(status: KnowledgeStatus) -> &'static str {
    match status {
        KnowledgeStatus::Pending => "pending",
        KnowledgeStatus::Processing => "processing",
        KnowledgeStatus::Indexed => "indexed",
        KnowledgeStatus::Failed => "failed",
    }
}

fn parse_knowledge_status(value: &str) -> Result<KnowledgeStatus> {
    match value {
        "pending" => Ok(KnowledgeStatus::Pending),
        "processing" => Ok(KnowledgeStatus::Processing),
        "indexed" => Ok(KnowledgeStatus::Indexed),
        "failed" => Ok(KnowledgeStatus::Failed),
        _ => Err(anyhow!("unknown knowledge status: {value}")),
    }
}

fn parse_role(value: &str) -> Result<MessageRole> {
    match value {
        "system" => Ok(MessageRole::System),
        "user" => Ok(MessageRole::User),
        "assistant" => Ok(MessageRole::Assistant),
        "tool" => Ok(MessageRole::Tool),
        _ => Err(anyhow!("unknown role: {value}")),
    }
}

fn parse_date(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("invalid datetime {value}"))?
        .with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cherry_core::{
        ApiServerConfig, AppSettings, FileEntry, KnowledgeDocument, McpServerDefinition, NoteEntry,
        ProtocolHandler, ProviderConfig, ToolPermission,
    };

    #[test]
    fn store_can_roundtrip_core_data() {
        let store = CherryStore::open(":memory:").expect("open store");

        store
            .save_settings(&AppSettings::default())
            .expect("save settings");
        let _ = store.load_settings().expect("load settings");

        let providers = vec![ProviderConfig::openai_default()];
        store.save_providers(&providers).expect("save providers");
        let listed = store.list_providers().expect("list providers");
        assert_eq!(listed.len(), 1);

        let conversation = store
            .create_conversation("Hello")
            .expect("create conversation");
        let user_message = ChatMessage::user(conversation.id, "Hi");
        store.append_message(&user_message).expect("append message");

        let messages = store.list_messages(conversation.id).expect("list messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hi");

        let file = FileEntry::local("report.pdf", "/tmp/report.pdf", "application/pdf", 12_345);
        store.upsert_file_entry(&file).expect("upsert file");
        let files = store.list_file_entries().expect("list files");
        assert_eq!(files.len(), 1);
        store.delete_file_entry(file.id).expect("delete file");
        assert!(store.list_file_entries().expect("re-list files").is_empty());

        let note = NoteEntry::new("Meeting", "- agenda");
        store.upsert_note_entry(&note).expect("upsert note");
        let notes = store.list_note_entries().expect("list notes");
        assert_eq!(notes.len(), 1);
        store.delete_note_entry(note.id).expect("delete note");
        assert!(store.list_note_entries().expect("re-list notes").is_empty());

        let knowledge = KnowledgeDocument::pending("Manual", "/tmp/manual.md", "text/markdown");
        store
            .upsert_knowledge_document(&knowledge)
            .expect("upsert knowledge");
        let docs = store
            .list_knowledge_documents()
            .expect("list knowledge documents");
        assert_eq!(docs.len(), 1);
        store
            .delete_knowledge_document(knowledge.id)
            .expect("delete knowledge document");
        assert!(
            store
                .list_knowledge_documents()
                .expect("re-list knowledge docs")
                .is_empty()
        );

        store
            .save_api_server_config(&ApiServerConfig::default())
            .expect("save api server config");
        let api_config = store
            .load_api_server_config_or_default()
            .expect("load api server config");
        assert_eq!(api_config.port, 8787);

        let mcp_server = McpServerDefinition::new("filesystem", "npx");
        store
            .upsert_mcp_server(&mcp_server)
            .expect("upsert mcp server");
        let servers = store.list_mcp_servers().expect("list mcp servers");
        assert_eq!(servers.len(), 1);
        store
            .delete_mcp_server(mcp_server.id)
            .expect("delete mcp server");
        assert!(store.list_mcp_servers().expect("re-list mcp").is_empty());

        let tool_permission = ToolPermission::new("openclaw_query", true);
        store
            .upsert_tool_permission(&tool_permission)
            .expect("upsert tool permission");
        let tool_permissions = store
            .list_tool_permissions()
            .expect("list tool permissions");
        assert_eq!(tool_permissions.len(), 1);
        store
            .delete_tool_permission(tool_permission.id)
            .expect("delete tool permission");
        assert!(
            store
                .list_tool_permissions()
                .expect("re-list tool permissions")
                .is_empty()
        );

        let protocol_handler = ProtocolHandler::new("cherry", "open");
        store
            .upsert_protocol_handler(&protocol_handler)
            .expect("upsert protocol handler");
        let protocol_handlers = store
            .list_protocol_handlers()
            .expect("list protocol handlers");
        assert_eq!(protocol_handlers.len(), 1);
        store
            .delete_protocol_handler(protocol_handler.id)
            .expect("delete protocol handler");
        assert!(
            store
                .list_protocol_handlers()
                .expect("re-list protocol handlers")
                .is_empty()
        );
    }
}
