use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use core_types::{SessionId, UnifiedMessage, UnifiedRole};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::Path;
use std::str::FromStr;
use uuid::Uuid;

pub const CURRENT_DB_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: SessionId,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: Uuid,
    pub session_id: SessionId,
    pub role: UnifiedRole,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_arguments_json: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SqliteStorage {
    pool: SqlitePool,
}

impl SqliteStorage {
    pub async fn connect(path: impl AsRef<Path>) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}",
            path.as_ref().to_string_lossy()
        ))?
        .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        let storage = Self { pool };
        storage.migrate().await?;
        Ok(storage)
    }

    pub async fn in_memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        let storage = Self { pool };
        storage.migrate().await?;
        Ok(storage)
    }

    async fn migrate(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                tool_call_id TEXT,
                tool_name TEXT,
                tool_arguments_json TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tool_calls (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                arguments_json TEXT NOT NULL,
                result TEXT,
                is_error INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO metadata(key, value)
            VALUES ('schema_version', ?1)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
        )
        .bind(CURRENT_DB_SCHEMA_VERSION.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn schema_version(&self) -> Result<u32> {
        let row = sqlx::query("SELECT value FROM metadata WHERE key = 'schema_version'")
            .fetch_one(&self.pool)
            .await?;
        let version = row.get::<String, _>("value").parse::<u32>()?;
        Ok(version)
    }

    pub async fn create_session(&self, title: impl Into<String>) -> Result<ChatSession> {
        let now = Utc::now();
        let id = SessionId::new_v4();
        let title = title.into();
        sqlx::query(
            r#"INSERT INTO sessions(id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)"#,
        )
        .bind(id.to_string())
        .bind(&title)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(ChatSession {
            id,
            title,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn append_message(
        &self,
        session_id: SessionId,
        message: &UnifiedMessage,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO messages(
                id, session_id, role, content, tool_call_id, tool_name, tool_arguments_json, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(message.id.to_string())
        .bind(session_id.to_string())
        .bind(serde_json::to_string(&message.role)?)
        .bind(&message.content)
        .bind(&message.tool_call_id)
        .bind(&message.tool_name)
        .bind(&message.tool_arguments_json)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;

        sqlx::query(r#"UPDATE sessions SET updated_at = ?2 WHERE id = ?1"#)
            .bind(session_id.to_string())
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn list_sessions(&self) -> Result<Vec<ChatSession>> {
        let rows = sqlx::query(
            r#"
            SELECT id, title, created_at, updated_at
            FROM sessions
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(map_session_row).collect()
    }

    pub async fn list_messages(&self, session_id: SessionId) -> Result<Vec<StoredMessage>> {
        let rows = sqlx::query(
            r#"
            SELECT id, session_id, role, content, tool_call_id, tool_name, tool_arguments_json, created_at
            FROM messages
            WHERE session_id = ?1
            ORDER BY created_at ASC
            "#,
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(map_message_row).collect()
    }
}

fn map_session_row(row: sqlx::sqlite::SqliteRow) -> Result<ChatSession> {
    let created_at = parse_rfc3339(row.get::<String, _>("created_at"))?;
    let updated_at = parse_rfc3339(row.get::<String, _>("updated_at"))?;
    Ok(ChatSession {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        title: row.get("title"),
        created_at,
        updated_at,
    })
}

fn map_message_row(row: sqlx::sqlite::SqliteRow) -> Result<StoredMessage> {
    let role_str: String = row.get("role");
    let role: UnifiedRole = serde_json::from_str(&role_str).context("invalid role in database")?;
    Ok(StoredMessage {
        id: Uuid::parse_str(row.get::<String, _>("id").as_str())?,
        session_id: Uuid::parse_str(row.get::<String, _>("session_id").as_str())?,
        role,
        content: row.get("content"),
        tool_call_id: row.get("tool_call_id"),
        tool_name: row.get("tool_name"),
        tool_arguments_json: row.get("tool_arguments_json"),
        created_at: parse_rfc3339(row.get::<String, _>("created_at"))?,
    })
}

fn parse_rfc3339(value: String) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(&value)?.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use core_types::UnifiedMessage;

    use super::*;

    #[tokio::test]
    async fn creates_and_reads_session_messages() {
        let storage = SqliteStorage::in_memory().await.expect("storage");
        let schema_version = storage.schema_version().await.expect("schema version");
        assert_eq!(schema_version, CURRENT_DB_SCHEMA_VERSION);

        let session = storage.create_session("test").await.expect("session");

        let msg = UnifiedMessage::user("hello");
        storage
            .append_message(session.id, &msg)
            .await
            .expect("append message");

        let sessions = storage.list_sessions().await.expect("sessions");
        assert_eq!(sessions.len(), 1);

        let messages = storage.list_messages(session.id).await.expect("messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "hello");
    }
}
