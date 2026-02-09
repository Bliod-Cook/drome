pub mod chat;
pub mod feature;
pub mod file;
pub mod integration;
pub mod knowledge;
pub mod note;
pub mod provider;
pub mod settings;

pub use chat::{Assistant, ChatMessage, Conversation, MessageAttachment, MessageRole};
pub use feature::{AppRoute, FEATURE_AREAS, FeatureArea};
pub use file::{FileEntry, FileSourceKind};
pub use integration::{
    ApiServerConfig, ApiServerRuntimeStatus, McpCallResult, McpServerDefinition, ProtocolHandler,
    ToolPermission,
};
pub use knowledge::{KnowledgeChunk, KnowledgeDocument, KnowledgeStatus};
pub use note::NoteEntry;
pub use provider::{ModelProfile, ProviderConfig, ProviderKind};
pub use settings::{AppSettings, BackupSettings, DisplaySettings, RuntimeSettings};
