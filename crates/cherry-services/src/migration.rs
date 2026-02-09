use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use cherry_core::{
    AppSettings, FileEntry, KnowledgeDocument, MessageRole, NoteEntry, ProviderConfig, ProviderKind,
};
use rusqlite::Connection;
use serde::{Deserialize, de::DeserializeOwned};

use crate::services::AppServices;

#[derive(Debug, Clone, Default)]
pub struct ImportReport {
    pub conversations: usize,
    pub messages: usize,
    pub providers: usize,
    pub notes: usize,
    pub files: usize,
    pub knowledge_documents: usize,
}

impl ImportReport {
    fn merge(&mut self, other: ImportReport) {
        self.conversations += other.conversations;
        self.messages += other.messages;
        self.providers += other.providers;
        self.notes += other.notes;
        self.files += other.files;
        self.knowledge_documents += other.knowledge_documents;
    }
}

#[derive(Debug, Deserialize)]
struct LegacyExport {
    settings: Option<AppSettings>,
    providers: Option<Vec<ProviderConfig>>,
    conversations: Option<Vec<LegacyConversation>>,
    notes: Option<Vec<NoteEntry>>,
    files: Option<Vec<FileEntry>>,
    knowledge_documents: Option<Vec<KnowledgeDocument>>,
}

#[derive(Debug, Deserialize)]
struct LegacyConversation {
    title: String,
    messages: Vec<LegacyMessage>,
}

#[derive(Debug, Deserialize)]
struct LegacyMessage {
    role: String,
    content: String,
    model: Option<String>,
    provider: Option<String>,
    token_usage: Option<u32>,
}

pub fn import_legacy_json(services: &AppServices, path: impl AsRef<Path>) -> Result<ImportReport> {
    let path = path.as_ref();
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let export: LegacyExport = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse json {}", path.display()))?;

    let mut report = ImportReport::default();

    if let Some(settings) = export.settings {
        services.save_settings(&settings)?;
    }

    if let Some(providers) = export.providers {
        services.save_providers(&providers)?;
        report.providers = providers.len();
    }

    if let Some(notes) = export.notes {
        for note in &notes {
            services.upsert_note(note)?;
        }
        report.notes = notes.len();
    }

    if let Some(files) = export.files {
        for file in &files {
            services.upsert_file(file)?;
        }
        report.files = files.len();
    }

    if let Some(documents) = export.knowledge_documents {
        for document in &documents {
            services.upsert_knowledge_document(document)?;
        }
        report.knowledge_documents = documents.len();
    }

    if let Some(conversations) = export.conversations {
        let (conversation_count, message_count) =
            import_legacy_conversations(services, conversations)?;
        report.conversations += conversation_count;
        report.messages += message_count;
    }

    Ok(report)
}

pub fn import_from_legacy_dir(
    services: &AppServices,
    dir: impl AsRef<Path>,
) -> Result<ImportReport> {
    let dir = dir.as_ref();
    if !dir.exists() {
        anyhow::bail!("legacy dir not found: {}", dir.display());
    }
    if !dir.is_dir() {
        anyhow::bail!("legacy path is not directory: {}", dir.display());
    }

    let mut report = ImportReport::default();

    if let Some(export_path) = find_first_existing(
        dir,
        &[
            "legacy-export.json",
            "legacy_export.json",
            "export.json",
            "cherry-export.json",
        ],
    ) {
        report.merge(import_legacy_json(services, export_path)?);
    }

    report.merge(import_split_json_files(services, dir)?);

    if let Some(sqlite_path) = find_first_existing(
        dir,
        &[
            "legacy.sqlite3",
            "legacy.db",
            "cherry-studio.db",
            "cherry_studio.db",
            "app.db",
            "app.sqlite3",
            "data/legacy.sqlite3",
            "data/legacy.db",
            "data/cherry-studio.db",
            "data/cherry_studio.db",
            "data/app.db",
            "data/app.sqlite3",
        ],
    ) {
        report.merge(import_from_legacy_sqlite(services, sqlite_path)?);
    }

    Ok(report)
}

pub fn import_from_legacy_sqlite(
    services: &AppServices,
    sqlite_path: impl AsRef<Path>,
) -> Result<ImportReport> {
    let sqlite_path = sqlite_path.as_ref();
    let conn = Connection::open(sqlite_path)
        .with_context(|| format!("failed to open sqlite {}", sqlite_path.display()))?;

    let mut report = ImportReport::default();

    let mut conversations_map: HashMap<String, uuid::Uuid> = HashMap::new();

    if table_exists(&conn, "settings") {
        let columns = read_table_columns(&conn, "settings")?;
        if columns.contains("key") && columns.contains("value_json") {
            let mut stmt =
                conn.prepare("SELECT value_json FROM settings WHERE key = 'app' LIMIT 1")?;
            let mut rows = stmt.query([])?;
            if let Some(row) = rows.next()? {
                let value_json: String = row.get(0)?;
                if let Ok(settings) = serde_json::from_str::<AppSettings>(&value_json) {
                    services.save_settings(&settings)?;
                }
            }
        }
    }

    if table_exists(&conn, "providers") {
        let columns = read_table_columns(&conn, "providers")?;
        let required = ["kind", "name", "base_url", "enabled", "models_json"];
        if required.iter().all(|column| columns.contains(*column)) {
            let mut stmt = conn.prepare(
                "
                SELECT
                  CAST(kind AS TEXT),
                  CAST(name AS TEXT),
                  CAST(base_url AS TEXT),
                  CASE WHEN api_key IS NULL THEN NULL ELSE CAST(api_key AS TEXT) END,
                  CAST(enabled AS INTEGER),
                  CAST(models_json AS TEXT)
                FROM providers
                ",
            )?;
            let mut rows = stmt.query([])?;
            let mut providers = Vec::new();
            while let Some(row) = rows.next()? {
                let kind: String = row.get(0)?;
                let name: String = row.get(1)?;
                let base_url: String = row.get(2)?;
                let api_key: Option<String> = row.get(3)?;
                let enabled_num: i64 = row.get(4)?;
                let models_json: String = row.get(5)?;

                let models = serde_json::from_str(&models_json)
                    .unwrap_or_else(|_| ProviderConfig::openai_default().models);

                providers.push(ProviderConfig {
                    id: uuid::Uuid::new_v4(),
                    kind: parse_provider_kind_local(&kind),
                    name,
                    base_url,
                    api_key,
                    enabled: enabled_num != 0,
                    models,
                });
            }

            if !providers.is_empty() {
                report.providers += providers.len();
                services.save_providers(&providers)?;
            }
        }
    }

    if table_exists(&conn, "conversations") {
        let columns = read_table_columns(&conn, "conversations")?;
        if columns.contains("id") {
            let title_expr = if columns.contains("title") {
                "COALESCE(NULLIF(CAST(title AS TEXT), ''), 'Imported Conversation')"
            } else {
                "'Imported Conversation'"
            };

            let sql = format!(
                "SELECT CAST(id AS TEXT) AS legacy_id, {} AS title FROM conversations ORDER BY rowid",
                title_expr
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let legacy_id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let conversation = services.create_conversation(title)?;
                conversations_map.insert(legacy_id, conversation.id);
                report.conversations += 1;
            }
        }
    }

    if table_exists(&conn, "messages") {
        let columns = read_table_columns(&conn, "messages")?;
        let conversation_col = find_column(&columns, &["conversation_id", "topic_id", "chat_id"]);
        let content_col = find_column(&columns, &["content", "text", "message"]);
        let role_col = find_column(&columns, &["role"]);
        let model_col = find_column(&columns, &["model"]);
        let provider_col = find_column(&columns, &["provider"]);
        let token_col = find_column(&columns, &["token_usage", "total_tokens"]);
        let created_col = find_column(&columns, &["created_at", "timestamp"]);

        if let (Some(conversation_col), Some(content_col)) = (conversation_col, content_col) {
            let role_expr = role_col
                .map(|column| format!("CAST({column} AS TEXT)"))
                .unwrap_or_else(|| "'user'".to_owned());
            let model_expr = model_col
                .map(|column| format!("CAST({column} AS TEXT)"))
                .unwrap_or_else(|| "NULL".to_owned());
            let provider_expr = provider_col
                .map(|column| format!("CAST({column} AS TEXT)"))
                .unwrap_or_else(|| "NULL".to_owned());
            let token_expr = token_col
                .map(|column| format!("CAST({column} AS INTEGER)"))
                .unwrap_or_else(|| "NULL".to_owned());
            let order_expr = created_col.unwrap_or("rowid");

            let sql = format!(
                "
                SELECT
                  CAST({conversation_col} AS TEXT) AS conversation_id,
                  {role_expr} AS role,
                  CAST({content_col} AS TEXT) AS content,
                  {model_expr} AS model,
                  {provider_expr} AS provider,
                  {token_expr} AS token_usage
                FROM messages
                ORDER BY {order_expr}
                "
            );

            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let legacy_conversation_id: String = row.get(0)?;
                let role: String = row.get(1)?;
                let content: String = row.get(2)?;
                let model: Option<String> = row.get(3)?;
                let provider: Option<String> = row.get(4)?;
                let token_usage: Option<i64> = row.get(5)?;

                let conversation_id =
                    if let Some(id) = conversations_map.get(&legacy_conversation_id) {
                        *id
                    } else {
                        let fallback = services
                            .create_conversation(format!("Imported {}", legacy_conversation_id))?;
                        conversations_map.insert(legacy_conversation_id.clone(), fallback.id);
                        report.conversations += 1;
                        fallback.id
                    };

                services.append_imported_message(
                    conversation_id,
                    legacy_role(&role),
                    content,
                    provider,
                    model,
                    token_usage.and_then(|value| u32::try_from(value).ok()),
                )?;
                report.messages += 1;
            }
        }
    }

    Ok(report)
}

fn legacy_role(value: &str) -> MessageRole {
    match value.to_lowercase().as_str() {
        "system" => MessageRole::System,
        "assistant" => MessageRole::Assistant,
        "tool" => MessageRole::Tool,
        _ => MessageRole::User,
    }
}

fn parse_provider_kind_local(value: &str) -> ProviderKind {
    match value.trim().to_lowercase().as_str() {
        "openai-compatible" | "openai" => ProviderKind::OpenAiCompatible,
        "anthropic" => ProviderKind::Anthropic,
        "gemini" => ProviderKind::Gemini,
        "openrouter" => ProviderKind::OpenRouter,
        "ollama" => ProviderKind::Ollama,
        "lmstudio" => ProviderKind::LmStudio,
        "siliconflow" => ProviderKind::SiliconFlow,
        _ => ProviderKind::Other,
    }
}

fn import_legacy_conversations(
    services: &AppServices,
    conversations: Vec<LegacyConversation>,
) -> Result<(usize, usize)> {
    let mut conversation_count = 0;
    let mut message_count = 0;

    for legacy_conversation in conversations {
        let conversation = services.create_conversation(legacy_conversation.title)?;
        conversation_count += 1;

        for legacy_message in legacy_conversation.messages {
            services.append_imported_message(
                conversation.id,
                legacy_role(&legacy_message.role),
                legacy_message.content,
                legacy_message.provider,
                legacy_message.model,
                legacy_message.token_usage,
            )?;
            message_count += 1;
        }
    }

    Ok((conversation_count, message_count))
}

fn import_split_json_files(services: &AppServices, dir: &Path) -> Result<ImportReport> {
    let mut report = ImportReport::default();

    if let Some(path) = find_first_existing(dir, &["settings.json"]) {
        let settings: AppSettings = load_json(path)?;
        services.save_settings(&settings)?;
    }

    if let Some(path) = find_first_existing(dir, &["providers.json"]) {
        let providers: Vec<ProviderConfig> = load_json(path)?;
        services.save_providers(&providers)?;
        report.providers += providers.len();
    }

    if let Some(path) = find_first_existing(dir, &["notes.json"]) {
        let notes: Vec<NoteEntry> = load_json(path)?;
        for note in &notes {
            services.upsert_note(note)?;
        }
        report.notes += notes.len();
    }

    if let Some(path) = find_first_existing(dir, &["files.json"]) {
        let files: Vec<FileEntry> = load_json(path)?;
        for file in &files {
            services.upsert_file(file)?;
        }
        report.files += files.len();
    }

    if let Some(path) = find_first_existing(dir, &["knowledge_documents.json"]) {
        let docs: Vec<KnowledgeDocument> = load_json(path)?;
        for doc in &docs {
            services.upsert_knowledge_document(doc)?;
        }
        report.knowledge_documents += docs.len();
    }

    if let Some(path) = find_first_existing(dir, &["conversations.json"]) {
        let conversations: Vec<LegacyConversation> = load_json(path)?;
        let (conversation_count, message_count) =
            import_legacy_conversations(services, conversations)?;
        report.conversations += conversation_count;
        report.messages += message_count;
    }

    Ok(report)
}

fn load_json<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
    let path = path.as_ref();
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}

fn find_first_existing(base: &Path, relative_paths: &[&str]) -> Option<PathBuf> {
    relative_paths
        .iter()
        .map(|relative_path| base.join(relative_path))
        .find(|path| path.exists() && path.is_file())
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    let Ok(mut stmt) =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1 LIMIT 1")
    else {
        return false;
    };
    let Ok(mut rows) = stmt.query([table]) else {
        return false;
    };
    matches!(rows.next(), Ok(Some(_)))
}

fn read_table_columns(conn: &Connection, table: &str) -> Result<HashSet<String>> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .with_context(|| format!("failed pragma table_info for {table}"))?;
    let mut rows = stmt.query([])?;
    let mut columns = HashSet::new();
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        columns.insert(name);
    }
    Ok(columns)
}

fn find_column<'a>(columns: &'a HashSet<String>, candidates: &[&'a str]) -> Option<&'a str> {
    candidates
        .iter()
        .find(|column| columns.contains(**column))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppServicesBuilder;
    use rusqlite::Connection;
    use std::io::Write;
    use uuid::Uuid;

    #[test]
    fn can_import_legacy_json() {
        let mut db_path = std::env::temp_dir();
        db_path.push(format!("cherry_rs_migrate_{}.db", Uuid::new_v4()));
        let services = AppServicesBuilder::new(db_path)
            .build()
            .expect("build app services");

        let mut json_path = std::env::temp_dir();
        json_path.push(format!("cherry_rs_migrate_{}.json", Uuid::new_v4()));
        let mut file = fs::File::create(&json_path).expect("create json");
        write!(
            file,
            r#"{{
  "conversations": [
    {{
      "title": "Imported",
      "messages": [
        {{"role":"user","content":"hello"}},
        {{"role":"assistant","content":"world","provider":"OpenAI","model":"gpt-4o-mini"}}
      ]
    }}
  ]
}}"#
        )
        .expect("write json");

        let report = import_legacy_json(&services, &json_path).expect("import");
        assert_eq!(report.conversations, 1);
        assert_eq!(report.messages, 2);

        fs::remove_file(&json_path).ok();
    }

    #[test]
    fn can_import_legacy_sqlite() {
        let mut db_path = std::env::temp_dir();
        db_path.push(format!("cherry_rs_migrate_sqlite_{}.db", Uuid::new_v4()));
        let services = AppServicesBuilder::new(db_path)
            .build()
            .expect("build app services");

        let mut legacy_sqlite = std::env::temp_dir();
        legacy_sqlite.push(format!("cherry_rs_legacy_{}.db", Uuid::new_v4()));
        let conn = Connection::open(&legacy_sqlite).expect("open legacy sqlite");
        conn.execute(
            "CREATE TABLE conversations (id TEXT PRIMARY KEY, title TEXT NOT NULL)",
            [],
        )
        .expect("create conversations");
        conn.execute(
            "CREATE TABLE messages (id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL, model TEXT, provider TEXT, token_usage INTEGER, created_at TEXT)",
            [],
        )
        .expect("create messages");
        conn.execute(
            "INSERT INTO conversations (id, title) VALUES ('c1', 'Imported SQLite')",
            [],
        )
        .expect("insert conversation");
        conn.execute(
            "INSERT INTO messages (id, conversation_id, role, content, model, provider, token_usage, created_at) VALUES ('m1', 'c1', 'user', 'hello sqlite', NULL, NULL, NULL, '2026-02-09T00:00:00Z')",
            [],
        )
        .expect("insert message");

        let report = import_from_legacy_sqlite(&services, &legacy_sqlite).expect("import sqlite");
        assert_eq!(report.conversations, 1);
        assert_eq!(report.messages, 1);

        fs::remove_file(&legacy_sqlite).ok();
    }

    #[test]
    fn can_import_legacy_dir() {
        let mut db_path = std::env::temp_dir();
        db_path.push(format!("cherry_rs_migrate_dir_{}.db", Uuid::new_v4()));
        let services = AppServicesBuilder::new(db_path)
            .build()
            .expect("build app services");

        let mut legacy_dir = std::env::temp_dir();
        legacy_dir.push(format!("cherry_rs_legacy_dir_{}", Uuid::new_v4()));
        fs::create_dir_all(&legacy_dir).expect("create legacy dir");

        let export_json = legacy_dir.join("legacy-export.json");
        let mut file = fs::File::create(&export_json).expect("create export json");
        write!(
            file,
            r#"{{
  "notes": [
    {{
      "id":"{}",
      "title":"legacy note",
      "markdown":"hello",
      "tags":[],
      "pinned":false,
      "created_at":"2026-02-09T00:00:00Z",
      "updated_at":"2026-02-09T00:00:00Z"
    }}
  ]
}}"#,
            Uuid::new_v4()
        )
        .expect("write export json");

        let report = import_from_legacy_dir(&services, &legacy_dir).expect("import directory");
        assert_eq!(report.notes, 1);

        fs::remove_file(export_json).ok();
        fs::remove_dir_all(legacy_dir).ok();
    }
}
