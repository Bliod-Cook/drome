use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageAttachment {
    pub id: Uuid,
    pub name: String,
    pub mime_type: String,
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub role: MessageRole,
    pub content: String,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub token_usage: Option<u32>,
    pub attachments: Vec<MessageAttachment>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub title: String,
    pub assistant_id: Option<Uuid>,
    pub archived: bool,
    pub pinned: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assistant {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub default_model: Option<String>,
    pub default_provider: Option<String>,
    pub temperature: f32,
    pub top_p: f32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Conversation {
    pub fn new(title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            assistant_id: None,
            archived: false,
            pinned: false,
            created_at: now,
            updated_at: now,
        }
    }
}

impl ChatMessage {
    pub fn user(conversation_id: Uuid, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            conversation_id,
            role: MessageRole::User,
            content: content.into(),
            model: None,
            provider: None,
            token_usage: None,
            attachments: Vec::new(),
            created_at: Utc::now(),
        }
    }

    pub fn assistant(
        conversation_id: Uuid,
        provider: impl Into<String>,
        model: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            conversation_id,
            role: MessageRole::Assistant,
            content: content.into(),
            model: Some(model.into()),
            provider: Some(provider.into()),
            token_usage: None,
            attachments: Vec::new(),
            created_at: Utc::now(),
        }
    }
}

impl Assistant {
    pub fn empty(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: String::new(),
            system_prompt: String::new(),
            default_model: None,
            default_provider: None,
            temperature: 0.7,
            top_p: 1.0,
            created_at: now,
            updated_at: now,
        }
    }
}
