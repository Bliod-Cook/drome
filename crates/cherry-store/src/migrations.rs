pub const MIGRATIONS: &[&str] = &[
    r#"
    CREATE TABLE IF NOT EXISTS conversations (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        assistant_id TEXT,
        archived INTEGER NOT NULL,
        pinned INTEGER NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS messages (
        id TEXT PRIMARY KEY,
        conversation_id TEXT NOT NULL,
        role TEXT NOT NULL,
        content TEXT NOT NULL,
        model TEXT,
        provider TEXT,
        token_usage INTEGER,
        attachments_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS settings (
        key TEXT PRIMARY KEY,
        value_json TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS providers (
        id TEXT PRIMARY KEY,
        kind TEXT NOT NULL,
        name TEXT NOT NULL,
        base_url TEXT NOT NULL,
        api_key TEXT,
        enabled INTEGER NOT NULL,
        models_json TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS files (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        path TEXT NOT NULL,
        mime_type TEXT NOT NULL,
        size_bytes INTEGER NOT NULL,
        source TEXT NOT NULL,
        hash TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS notes (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        markdown TEXT NOT NULL,
        tags_json TEXT NOT NULL,
        pinned INTEGER NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS knowledge_documents (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        source_path TEXT NOT NULL,
        mime_type TEXT NOT NULL,
        status TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS knowledge_chunks (
        id TEXT PRIMARY KEY,
        document_id TEXT NOT NULL,
        content TEXT NOT NULL,
        token_count INTEGER NOT NULL,
        embedding_model TEXT,
        FOREIGN KEY(document_id) REFERENCES knowledge_documents(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS api_server_config (
        key TEXT PRIMARY KEY,
        value_json TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS mcp_servers (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        command TEXT NOT NULL,
        args_json TEXT NOT NULL,
        env_json TEXT NOT NULL,
        enabled INTEGER NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS tool_permissions (
        id TEXT PRIMARY KEY,
        tool_name TEXT NOT NULL UNIQUE,
        allowed INTEGER NOT NULL,
        scope TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS protocol_handlers (
        id TEXT PRIMARY KEY,
        scheme TEXT NOT NULL UNIQUE,
        command TEXT NOT NULL,
        args_json TEXT NOT NULL,
        enabled INTEGER NOT NULL
    )
    "#,
    r#"
    CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id, created_at)
    "#,
    r#"
    CREATE INDEX IF NOT EXISTS idx_notes_updated_at ON notes(updated_at)
    "#,
    r#"
    CREATE INDEX IF NOT EXISTS idx_knowledge_documents_updated_at ON knowledge_documents(updated_at)
    "#,
    r#"
    CREATE INDEX IF NOT EXISTS idx_mcp_servers_name ON mcp_servers(name)
    "#,
    r#"
    CREATE INDEX IF NOT EXISTS idx_tool_permissions_name ON tool_permissions(tool_name)
    "#,
    r#"
    CREATE INDEX IF NOT EXISTS idx_protocol_handlers_scheme ON protocol_handlers(scheme)
    "#,
];
