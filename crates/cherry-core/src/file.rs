use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileSourceKind {
    Local,
    WebDav,
    S3,
    LanTransfer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub source: FileSourceKind,
    pub hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl FileEntry {
    pub fn local(
        name: impl Into<String>,
        path: impl Into<String>,
        mime_type: impl Into<String>,
        size_bytes: u64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            path: path.into(),
            mime_type: mime_type.into(),
            size_bytes,
            source: FileSourceKind::Local,
            hash: None,
            created_at: now,
            updated_at: now,
        }
    }
}
