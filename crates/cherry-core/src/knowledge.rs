use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum KnowledgeStatus {
    Pending,
    Processing,
    Indexed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    pub id: Uuid,
    pub title: String,
    pub source_path: String,
    pub mime_type: String,
    pub status: KnowledgeStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeChunk {
    pub id: Uuid,
    pub document_id: Uuid,
    pub content: String,
    pub token_count: u32,
    pub embedding_model: Option<String>,
}

impl KnowledgeDocument {
    pub fn pending(
        title: impl Into<String>,
        source_path: impl Into<String>,
        mime_type: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            source_path: source_path.into(),
            mime_type: mime_type.into(),
            status: KnowledgeStatus::Pending,
            created_at: now,
            updated_at: now,
        }
    }
}
