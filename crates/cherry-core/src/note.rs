use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteEntry {
    pub id: Uuid,
    pub title: String,
    pub markdown: String,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl NoteEntry {
    pub fn new(title: impl Into<String>, markdown: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            markdown: markdown.into(),
            tags: Vec::new(),
            pinned: false,
            created_at: now,
            updated_at: now,
        }
    }
}
