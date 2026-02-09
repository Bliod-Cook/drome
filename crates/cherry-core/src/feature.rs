#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppRoute {
    Home,
    Store,
    Paintings,
    Translate,
    Files,
    Notes,
    Knowledge,
    MiniApps,
    CodeTools,
    OpenClaw,
    Settings,
    Launchpad,
}

impl AppRoute {
    pub fn title(self) -> &'static str {
        match self {
            AppRoute::Home => "Assistants",
            AppRoute::Store => "Store",
            AppRoute::Paintings => "Paintings",
            AppRoute::Translate => "Translate",
            AppRoute::Files => "Files",
            AppRoute::Notes => "Notes",
            AppRoute::Knowledge => "Knowledge",
            AppRoute::MiniApps => "Mini Apps",
            AppRoute::CodeTools => "Code",
            AppRoute::OpenClaw => "OpenClaw",
            AppRoute::Settings => "Settings",
            AppRoute::Launchpad => "Launchpad",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FeatureArea {
    pub name: &'static str,
    pub capabilities: &'static [&'static str],
}

pub const FEATURE_AREAS: &[FeatureArea] = &[
    FeatureArea {
        name: "Conversation",
        capabilities: &[
            "Multi-provider chats",
            "Assistant presets",
            "Topic/message history",
            "Model switching",
            "Streaming output",
        ],
    },
    FeatureArea {
        name: "Knowledge",
        capabilities: &[
            "Knowledge base indexing",
            "Document parsing",
            "Rerank and retrieval",
            "Context injection",
        ],
    },
    FeatureArea {
        name: "Tools",
        capabilities: &[
            "MCP server lifecycle",
            "Web search",
            "Code tools",
            "Memory management",
            "API server",
        ],
    },
    FeatureArea {
        name: "Data",
        capabilities: &[
            "File management",
            "Notes",
            "Backup/restore",
            "WebDAV and S3",
            "LAN transfer",
        ],
    },
    FeatureArea {
        name: "Platform",
        capabilities: &[
            "Tray/window lifecycle",
            "Deep links",
            "Shortcuts",
            "Auto update",
            "Selection assistant",
        ],
    },
];
