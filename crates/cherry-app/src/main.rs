use std::{path::PathBuf, sync::Arc};

use cherry_core::{AppRoute, ChatMessage, Conversation};
use cherry_services::{AppServices, AppServicesBuilder, BackupChannel};
use gpui::{
    App, AppContext, Application, Bounds, Context, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, ParentElement, Render, Styled, Window, WindowBounds, WindowOptions, div, hsla,
    prelude::*, px, rgb, size,
};
use pulldown_cmark::{Event, Parser};
use tokio::runtime::Runtime;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsSection {
    Provider,
    Model,
    General,
    Display,
    Data,
    Mcp,
    WebSearch,
    Memory,
    ApiServer,
    DocProcess,
    QuickPhrase,
    Shortcut,
    QuickAssistant,
    SelectionAssistant,
    About,
}

impl SettingsSection {
    const ALL: [SettingsSection; 15] = [
        SettingsSection::Provider,
        SettingsSection::Model,
        SettingsSection::General,
        SettingsSection::Display,
        SettingsSection::Data,
        SettingsSection::Mcp,
        SettingsSection::WebSearch,
        SettingsSection::Memory,
        SettingsSection::ApiServer,
        SettingsSection::DocProcess,
        SettingsSection::QuickPhrase,
        SettingsSection::Shortcut,
        SettingsSection::QuickAssistant,
        SettingsSection::SelectionAssistant,
        SettingsSection::About,
    ];

    fn title(self) -> &'static str {
        match self {
            SettingsSection::Provider => "Provider",
            SettingsSection::Model => "Model",
            SettingsSection::General => "General",
            SettingsSection::Display => "Display",
            SettingsSection::Data => "Data",
            SettingsSection::Mcp => "MCP",
            SettingsSection::WebSearch => "WebSearch",
            SettingsSection::Memory => "Memory",
            SettingsSection::ApiServer => "API Server",
            SettingsSection::DocProcess => "DocProcess",
            SettingsSection::QuickPhrase => "QuickPhrase",
            SettingsSection::Shortcut => "Shortcut",
            SettingsSection::QuickAssistant => "QuickAssistant",
            SettingsSection::SelectionAssistant => "SelectionAssistant",
            SettingsSection::About => "About",
        }
    }

    fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|entry| *entry == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|entry| *entry == self)
            .unwrap_or(0);
        let previous = if index == 0 {
            Self::ALL.len() - 1
        } else {
            index - 1
        };
        Self::ALL[previous]
    }
}

struct CherryAppView {
    services: AppServices,
    runtime: Arc<Runtime>,
    route: AppRoute,
    routes: Vec<AppRoute>,
    settings_section: SettingsSection,
    conversations: Vec<Conversation>,
    active_conversation_id: Option<Uuid>,
    messages: Vec<ChatMessage>,
    streaming_preview: Vec<String>,
    status: String,
}

impl CherryAppView {
    fn new(services: AppServices, runtime: Arc<Runtime>) -> Self {
        let routes = [
            AppRoute::Home,
            AppRoute::Store,
            AppRoute::Paintings,
            AppRoute::Translate,
            AppRoute::Files,
            AppRoute::Notes,
            AppRoute::Knowledge,
            AppRoute::MiniApps,
            AppRoute::CodeTools,
            AppRoute::OpenClaw,
            AppRoute::Settings,
            AppRoute::Launchpad,
        ]
        .to_vec();

        let mut view = Self {
            services,
            runtime,
            route: AppRoute::Home,
            routes,
            settings_section: SettingsSection::Provider,
            conversations: Vec::new(),
            active_conversation_id: None,
            messages: Vec::new(),
            streaming_preview: Vec::new(),
            status: String::new(),
        };

        if let Err(error) = view.refresh_workspace() {
            view.status = format!("failed to initialize workspace: {error}");
        }

        view
    }

    fn refresh_workspace(&mut self) -> anyhow::Result<()> {
        let conversation = self.services.ensure_default_conversation()?;
        self.active_conversation_id = Some(conversation.id);
        self.conversations = self.services.list_conversations()?;
        self.messages = self.services.list_messages(conversation.id)?;
        self.streaming_preview.clear();
        self.status = format!(
            "{} | active conversation: {}",
            self.services.summarize_feature_progress(),
            conversation.id
        );
        Ok(())
    }

    fn switch_route(&mut self, route: AppRoute) {
        self.route = route;
        self.status = format!(
            "Switched to {} | {}",
            route.title(),
            self.services.summarize_feature_progress()
        );
    }

    fn load_messages_for_active(&mut self) -> anyhow::Result<()> {
        if let Some(conversation_id) = self.active_conversation_id {
            self.messages = self.services.list_messages(conversation_id)?;
        }
        Ok(())
    }

    fn create_new_conversation(&mut self) -> anyhow::Result<()> {
        let title = format!("New Chat {}", self.conversations.len() + 1);
        let conversation = self.services.create_conversation(title)?;
        self.active_conversation_id = Some(conversation.id);
        self.conversations = self.services.list_conversations()?;
        self.messages = self.services.list_messages(conversation.id)?;
        self.streaming_preview.clear();
        self.status = format!("Created conversation {}", conversation.id);
        Ok(())
    }

    fn send_quick_prompt(&mut self, prompt: &str) -> anyhow::Result<()> {
        let conversation_id = match self.active_conversation_id {
            Some(id) => id,
            None => {
                self.create_new_conversation()?;
                self.active_conversation_id
                    .ok_or_else(|| anyhow::anyhow!("conversation initialization failed"))?
            }
        };

        self.runtime.block_on(
            self.services
                .send_message_with_fallback(conversation_id, prompt.to_owned()),
        )?;

        self.load_messages_for_active()?;
        self.streaming_preview.clear();
        self.status = format!("Sent prompt: {prompt}");
        Ok(())
    }

    fn send_quick_prompt_streaming(&mut self, prompt: &str) -> anyhow::Result<()> {
        let conversation_id = match self.active_conversation_id {
            Some(id) => id,
            None => {
                self.create_new_conversation()?;
                self.active_conversation_id
                    .ok_or_else(|| anyhow::anyhow!("conversation initialization failed"))?
            }
        };

        self.streaming_preview = self.runtime.block_on(
            self.services
                .send_message_streaming_with_fallback(conversation_id, prompt.to_owned()),
        )?;
        self.load_messages_for_active()?;
        self.status = format!(
            "Streaming prompt done: {prompt} ({} chunks)",
            self.streaming_preview.len()
        );
        Ok(())
    }

    fn settings_section_summary(&self) -> String {
        let settings = self.services.settings();
        match self.settings_section {
            SettingsSection::Provider => format!(
                "Default provider: {:?}, total providers: {}",
                settings.default_provider,
                self.services.providers().len()
            ),
            SettingsSection::Model => format!("Default model: {:?}", settings.default_model),
            SettingsSection::General => format!(
                "LaunchOnBoot={}, LaunchToTray={}, CloseToTray={}",
                settings.runtime.launch_on_boot,
                settings.runtime.launch_to_tray,
                settings.runtime.close_to_tray
            ),
            SettingsSection::Display => format!(
                "Theme={}, Language={}, Font={} {}",
                settings.display.theme,
                settings.display.language,
                settings.display.font_family,
                settings.display.font_size
            ),
            SettingsSection::Data => format!(
                "files={}, notes={}, knowledge_docs={}, backup(webdav/s3/lan)={}/{}/{}",
                self.services.list_files().map(|v| v.len()).unwrap_or(0),
                self.services.list_notes().map(|v| v.len()).unwrap_or(0),
                self.services
                    .list_knowledge_documents()
                    .map(|v| v.len())
                    .unwrap_or(0),
                settings.backup.webdav_enabled,
                settings.backup.s3_enabled,
                settings.backup.lan_transfer_enabled
            ),
            SettingsSection::Mcp => format!(
                "MCP servers: {}, tool permissions: {}, protocols: {}",
                self.services
                    .list_mcp_servers()
                    .map(|v| v.len())
                    .unwrap_or(0),
                self.services
                    .list_tool_permissions()
                    .map(|v| v.len())
                    .unwrap_or(0),
                self.services
                    .list_protocol_handlers()
                    .map(|v| v.len())
                    .unwrap_or(0)
            ),
            SettingsSection::WebSearch => {
                "WebSearch adapter planned (placeholder via chat prompt).".to_owned()
            }
            SettingsSection::Memory => {
                format!(
                    "Memory placeholder using notes count={}",
                    self.services.list_notes().map(|v| v.len()).unwrap_or(0)
                )
            }
            SettingsSection::ApiServer => {
                let config = self.services.api_server_config();
                format!(
                    "enabled={}, status={:?}, endpoint={}:{}",
                    config.enabled,
                    self.services.api_server_status(),
                    config.host,
                    config.port
                )
            }
            SettingsSection::DocProcess => {
                "DocProcess pipeline placeholder: parse -> chunk -> embed -> retrieve".to_owned()
            }
            SettingsSection::QuickPhrase => "QuickPhrase placeholder stored in notes.".to_owned(),
            SettingsSection::Shortcut => {
                format!(
                    "Spell check enabled={}",
                    settings.runtime.enable_spell_check
                )
            }
            SettingsSection::QuickAssistant => {
                "QuickAssistant placeholder routed to quick prompt action.".to_owned()
            }
            SettingsSection::SelectionAssistant => {
                "SelectionAssistant placeholder routed to runtime settings.".to_owned()
            }
            SettingsSection::About => "Cherry Studio Rust migration build".to_owned(),
        }
    }

    fn apply_settings_section_action(&mut self) -> anyhow::Result<()> {
        match self.settings_section {
            SettingsSection::Provider => {
                self.services.set_default_model("OpenAI", "gpt-4o-mini")?;
                self.status = "Set default provider/model to OpenAI:gpt-4o-mini".to_owned();
            }
            SettingsSection::Model => {
                self.services.set_default_model("OpenAI", "gpt-4o-mini")?;
                self.status = "Applied default model selection".to_owned();
            }
            SettingsSection::General => {
                let mut settings = self.services.settings();
                settings.runtime.launch_to_tray = !settings.runtime.launch_to_tray;
                settings.runtime.close_to_tray = !settings.runtime.close_to_tray;
                self.services.save_settings(&settings)?;
                self.status = format!(
                    "Toggled tray options: launch_to_tray={} close_to_tray={}",
                    settings.runtime.launch_to_tray, settings.runtime.close_to_tray
                );
            }
            SettingsSection::Display => {
                let settings = self.services.cycle_theme()?;
                self.status = format!("Theme switched to {}", settings.display.theme);
            }
            SettingsSection::Data => {
                self.services
                    .set_backup_channel_enabled(BackupChannel::WebDav, true)?;
                self.services
                    .set_backup_channel_enabled(BackupChannel::S3, true)?;
                self.services
                    .set_backup_channel_enabled(BackupChannel::Lan, true)?;
                let path = self
                    .services
                    .export_backup_to_channel(BackupChannel::WebDav)?;
                self.status = format!(
                    "Enabled WebDAV/S3/LAN backups and exported backup to {}",
                    path.display()
                );
            }
            SettingsSection::Mcp => {
                let server = self.services.add_sample_mcp_server()?;
                self.services.set_tool_permission("openclaw_query", true)?;
                self.services.add_sample_protocol_handler()?;
                self.status = format!(
                    "Added MCP server {} and refreshed permission/protocol",
                    server.name
                );
            }
            SettingsSection::WebSearch => {
                self.send_quick_prompt("请根据当前会话给出一个 Web 搜索关键词建议。")?;
            }
            SettingsSection::Memory => {
                let note = self.services.add_sample_note_entry()?;
                self.status = format!("Memory note appended: {}", note.title);
            }
            SettingsSection::ApiServer => {
                let config = self.services.toggle_api_server_enabled()?;
                self.status = format!("API enabled switched to {}", config.enabled);
            }
            SettingsSection::DocProcess => {
                let doc = self.services.add_sample_knowledge_document()?;
                self.status = format!("DocProcess sample document added: {}", doc.title);
            }
            SettingsSection::QuickPhrase => {
                let mut note = self.services.add_sample_note_entry()?;
                note.title = format!("QuickPhrase: {}", note.title);
                self.services.upsert_note(&note)?;
                self.status = format!("QuickPhrase note created: {}", note.title);
            }
            SettingsSection::Shortcut => {
                let settings = self.services.toggle_spell_check()?;
                self.status = format!(
                    "Spell check switched to {}",
                    settings.runtime.enable_spell_check
                );
            }
            SettingsSection::QuickAssistant => {
                self.send_quick_prompt_streaming(
                    "你是 Quick Assistant，请给出当前任务的 3 个下一步。",
                )?;
            }
            SettingsSection::SelectionAssistant => {
                let language = if self.services.settings().display.language == "en-US" {
                    "zh-CN"
                } else {
                    "en-US"
                };
                let settings = self.services.update_language(language)?;
                self.status = format!("Language switched to {}", settings.display.language);
            }
            SettingsSection::About => {
                self.status = self.services.summarize_feature_progress();
            }
        }

        Ok(())
    }

    fn header_text(&self) -> String {
        format!(
            "Cherry Studio (Rust + GPUI)\nActive route: {}\n{}",
            self.route.title(),
            self.compact_status()
        )
    }

    fn compact_status(&self) -> String {
        let mut chars = self.status.chars();
        let preview: String = chars.by_ref().take(200).collect();
        if chars.next().is_some() {
            format!("{preview}…")
        } else {
            preview
        }
    }

    fn messages_text(&self) -> String {
        if self.messages.is_empty() {
            return "No messages yet. Use Home/Translate/Code actions to generate messages."
                .to_owned();
        }

        self.messages
            .iter()
            .rev()
            .take(8)
            .map(|message| format!("{:?}: {}", message.role, message.content))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn markdown_preview(&self, markdown: &str) -> String {
        let mut output = String::new();
        for event in Parser::new(markdown) {
            match event {
                Event::Text(text) | Event::Code(text) => output.push_str(&text),
                Event::SoftBreak | Event::HardBreak => output.push('\n'),
                _ => {}
            }
        }
        output
    }

    fn code_preview(&self, language: &str, code: &str) -> String {
        let mut output = format!("```{}\n", language);
        for (index, line) in code.lines().enumerate() {
            output.push_str(&format!("{:>2} | {}\n", index + 1, line));
        }
        output.push_str("```");
        output
    }

    fn mermaid_preview(&self, mermaid: &str) -> String {
        format!("[Mermaid Preview]\n{}", mermaid)
    }

    fn file_preview(&self) -> String {
        match self.services.list_files() {
            Ok(files) if files.is_empty() => "No files available for preview".to_owned(),
            Ok(files) => files
                .iter()
                .take(5)
                .map(|file| format!("- {} ({}, {:?})", file.name, file.mime_type, file.source))
                .collect::<Vec<_>>()
                .join("\n"),
            Err(error) => format!("load files failed: {error}"),
        }
    }

    fn summary_for_route(&self) -> String {
        match self.route {
            AppRoute::Home => format!(
                "Conversations: {} | Active: {}",
                self.conversations.len(),
                self.active_conversation_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "none".to_owned())
            ),
            AppRoute::Store => format!(
                "Store module: providers={} backup-ready(webdav/s3/lan={}/{}/{})",
                self.services.providers().len(),
                self.services
                    .is_backup_channel_enabled(BackupChannel::WebDav),
                self.services.is_backup_channel_enabled(BackupChannel::S3),
                self.services.is_backup_channel_enabled(BackupChannel::Lan)
            ),
            AppRoute::Paintings => "Paintings module skeleton with prompt presets".to_owned(),
            AppRoute::Translate => "Translate module skeleton with markdown preview".to_owned(),
            AppRoute::Files => format!(
                "Files module: {} entries",
                self.services
                    .list_files()
                    .map(|items| items.len())
                    .unwrap_or(0)
            ),
            AppRoute::Notes => format!(
                "Notes module: {} entries",
                self.services
                    .list_notes()
                    .map(|items| items.len())
                    .unwrap_or(0)
            ),
            AppRoute::Knowledge => format!(
                "Knowledge module: {} documents",
                self.services
                    .list_knowledge_documents()
                    .map(|items| items.len())
                    .unwrap_or(0)
            ),
            AppRoute::MiniApps => "MiniApps module skeleton".to_owned(),
            AppRoute::CodeTools => "CodeTools module skeleton with code preview".to_owned(),
            AppRoute::OpenClaw => format!(
                "OpenClaw: mcp={} tool_permissions={} protocols={}",
                self.services
                    .list_mcp_servers()
                    .map(|items| items.len())
                    .unwrap_or(0),
                self.services
                    .list_tool_permissions()
                    .map(|items| items.len())
                    .unwrap_or(0),
                self.services
                    .list_protocol_handlers()
                    .map(|items| items.len())
                    .unwrap_or(0)
            ),
            AppRoute::Settings => format!(
                "Settings section: {} | {}",
                self.settings_section.title(),
                self.settings_section_summary()
            ),
            AppRoute::Launchpad => "Launchpad module skeleton with migration tools".to_owned(),
        }
    }

    fn files_text(&self) -> String {
        match self.services.list_files() {
            Ok(files) if files.is_empty() => "No files".to_owned(),
            Ok(files) => files
                .iter()
                .take(8)
                .map(|file| format!("- {} ({})", file.name, file.path))
                .collect::<Vec<_>>()
                .join("\n"),
            Err(error) => format!("load files failed: {error}"),
        }
    }

    fn notes_text(&self) -> String {
        match self.services.list_notes() {
            Ok(notes) if notes.is_empty() => "No notes".to_owned(),
            Ok(notes) => notes
                .iter()
                .take(8)
                .map(|note| format!("- {}", note.title))
                .collect::<Vec<_>>()
                .join("\n"),
            Err(error) => format!("load notes failed: {error}"),
        }
    }

    fn knowledge_text(&self) -> String {
        match self.services.list_knowledge_documents() {
            Ok(docs) if docs.is_empty() => "No knowledge documents".to_owned(),
            Ok(docs) => docs
                .iter()
                .take(8)
                .map(|doc| format!("- {} [{:?}]", doc.title, doc.status))
                .collect::<Vec<_>>()
                .join("\n"),
            Err(error) => format!("load knowledge failed: {error}"),
        }
    }

    fn mcp_text(&self) -> String {
        match self.services.list_mcp_servers() {
            Ok(servers) if servers.is_empty() => "No MCP servers".to_owned(),
            Ok(servers) => servers
                .iter()
                .take(8)
                .map(|server| format!("- {} -> {} {:?}", server.name, server.command, server.args))
                .collect::<Vec<_>>()
                .join("\n"),
            Err(error) => format!("load mcp servers failed: {error}"),
        }
    }

    fn tool_permissions_text(&self) -> String {
        match self.services.list_tool_permissions() {
            Ok(permissions) if permissions.is_empty() => "No tool permissions".to_owned(),
            Ok(permissions) => permissions
                .iter()
                .map(|permission| format!("- {} => {}", permission.tool_name, permission.allowed))
                .collect::<Vec<_>>()
                .join("\n"),
            Err(error) => format!("load tool permissions failed: {error}"),
        }
    }

    fn protocol_handlers_text(&self) -> String {
        match self.services.list_protocol_handlers() {
            Ok(handlers) if handlers.is_empty() => "No protocol handlers".to_owned(),
            Ok(handlers) => handlers
                .iter()
                .map(|handler| {
                    format!(
                        "- {} -> {} {:?} (enabled={})",
                        handler.scheme, handler.command, handler.args, handler.enabled
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Err(error) => format!("load protocol handlers failed: {error}"),
        }
    }

    fn streaming_preview_text(&self) -> String {
        if self.streaming_preview.is_empty() {
            return "No streaming chunks yet".to_owned();
        }
        self.streaming_preview
            .iter()
            .take(32)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn key_to_route(key: &str) -> Option<AppRoute> {
        match key {
            "1" => Some(AppRoute::Home),
            "2" => Some(AppRoute::Store),
            "3" => Some(AppRoute::Paintings),
            "4" => Some(AppRoute::Translate),
            "5" => Some(AppRoute::Files),
            "6" => Some(AppRoute::Notes),
            "7" => Some(AppRoute::Knowledge),
            "8" => Some(AppRoute::MiniApps),
            "9" => Some(AppRoute::CodeTools),
            _ => None,
        }
    }
}

impl Render for CherryAppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut sidebar = div()
            .id("sidebar")
            .flex()
            .flex_col()
            .gap_1()
            .w(px(220.))
            .h_full()
            .p_3()
            .bg(rgb(0x1f2937))
            .text_color(rgb(0xf1f5f9))
            .border_r_1()
            .border_color(hsla(0.0, 0.0, 1.0, 0.14))
            .overflow_y_scroll()
            .child("Navigation (click or press 1-9):");
        for route in self.routes.iter().copied() {
            let active_prefix = if route == self.route { "● " } else { "○ " };
            let label = format!("{active_prefix}{}", route.title());
            sidebar = sidebar.child(
                div()
                    .p_1()
                    .rounded_sm()
                    .bg(if route == self.route {
                        rgb(0x334155)
                    } else {
                        rgb(0x111827)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.switch_route(route);
                            cx.notify();
                        }),
                    )
                    .child(label),
            );
        }

        let route_actions = match self.route {
            AppRoute::Home => {
                let mut home_actions = div()
                    .child("Home Quick Actions:")
                    .child(
                        div()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    if let Err(error) = this.create_new_conversation() {
                                        this.status = format!("create conversation failed: {error}");
                                    }
                                    cx.notify();
                                }),
                            )
                            .child("➕ New Conversation"),
                    )
                    .child(
                        div()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    if let Err(error) =
                                        this.send_quick_prompt("请总结当前迁移进度并给出下一步计划。")
                                    {
                                        this.status = format!("send prompt failed: {error}");
                                    }
                                    cx.notify();
                                }),
                            )
                            .child("▶ Send Progress Prompt"),
                    )
                    .child(
                        div()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    if let Err(error) = this.send_quick_prompt_streaming(
                                        "请以流式方式总结本周迁移进度。",
                                    ) {
                                        this.status = format!("stream prompt failed: {error}");
                                    }
                                    cx.notify();
                                }),
                            )
                            .child("⏩ Stream Progress Prompt"),
                    );

                if !self.conversations.is_empty() {
                    home_actions = home_actions.child("Conversations:");
                }
                for conversation in self.conversations.iter().take(10) {
                    let conversation_id = conversation.id;
                    let title = conversation.title.clone();
                    home_actions = home_actions.child(
                        div()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.active_conversation_id = Some(conversation_id);
                                    if let Err(error) = this.load_messages_for_active() {
                                        this.status = format!("switch conversation failed: {error}");
                                    } else {
                                        this.status = format!("Switched conversation: {title}");
                                    }
                                    cx.notify();
                                }),
                            )
                            .child(format!("- {}", conversation.title)),
                    );
                }
                home_actions
            }
            AppRoute::Store => div()
                .child("Store Actions:")
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                if let Err(error) =
                                    this.services.export_backup_to_channel(BackupChannel::Local)
                                {
                                    this.status = format!("export backup failed: {error}");
                                } else {
                                    this.status = "Exported local backup".to_owned();
                                }
                                cx.notify();
                            }),
                        )
                        .child("📦 Export Backup JSON"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                match this
                                    .services
                                    .import_latest_backup_from_channel(BackupChannel::Local)
                                {
                                    Ok(report) => {
                                        this.status = format!(
                                            "Imported local backup: conversations={} messages={}",
                                            report.conversations,
                                            report.messages
                                        );
                                    }
                                    Err(error) => {
                                        this.status = format!("import backup failed: {error}");
                                    }
                                }
                                cx.notify();
                            }),
                        )
                        .child("📥 Import Backup JSON"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                let result = (|| -> anyhow::Result<()> {
                                    this.services
                                        .set_backup_channel_enabled(BackupChannel::WebDav, true)?;
                                    this.services
                                        .set_backup_channel_enabled(BackupChannel::S3, true)?;
                                    this.services
                                        .set_backup_channel_enabled(BackupChannel::Lan, true)?;
                                    let path = this
                                        .services
                                        .export_backup_to_channel(BackupChannel::WebDav)?;
                                    this.status = format!("Exported WebDAV backup: {}", path.display());
                                    Ok(())
                                })();

                                if let Err(error) = result {
                                    this.status = format!("export webdav backup failed: {error}");
                                }
                                cx.notify();
                            }),
                        )
                        .child("☁️ Export WebDAV Backup"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                match this
                                    .services
                                    .import_latest_backup_from_channel(BackupChannel::WebDav)
                                {
                                    Ok(report) => {
                                        this.status = format!(
                                            "Imported WebDAV backup: conversations={} messages={}",
                                            report.conversations,
                                            report.messages
                                        );
                                    }
                                    Err(error) => {
                                        this.status =
                                            format!("import webdav backup failed: {error}");
                                    }
                                }
                                cx.notify();
                            }),
                        )
                        .child("☁️ Import WebDAV Backup"),
                )
                .child(format!("Provider count: {}", self.services.providers().len()))
                .child(self.markdown_preview(
                    "# Store\n- Provider templates\n- Assistant presets\n- Versioned artifacts",
                )),
            AppRoute::Paintings => div()
                .child("Paintings Actions:")
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                if let Err(error) = this.send_quick_prompt("请给我 3 个 Midjourney 风格绘图提示词。") {
                                    this.status = format!("send drawing prompt failed: {error}");
                                }
                                cx.notify();
                            }),
                        )
                        .child("🎨 Generate Painting Prompts"),
                )
                .child(self.code_preview(
                    "json",
                    r#"{"provider":"paintings","style":"cinematic","quality":"high"}"#,
                ))
                .child(self.mermaid_preview("flowchart LR\nA[Prompt] --> B[Model]\nB --> C[Image]")),
            AppRoute::Translate => div()
                .child("Translate Actions:")
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                if let Err(error) = this.send_quick_prompt("将下面这句话翻译成英文：我们正在将 Cherry Studio 迁移到 Rust。") {
                                    this.status = format!("send translate prompt failed: {error}");
                                }
                                cx.notify();
                            }),
                        )
                        .child("🌐 Translate Sample"),
                )
                .child(self.markdown_preview("## Translate Preview\n**Input**: 你好世界\n**Output**: Hello, world")),
            AppRoute::Files => div()
                .child("Files Actions:")
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                if let Err(error) = this.services.add_sample_file_entry() {
                                    this.status = format!("add file failed: {error}");
                                } else {
                                    this.status = "Added sample file".to_owned();
                                }
                                cx.notify();
                            }),
                        )
                        .child("➕ Add Sample File"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                match this.services.upload_local_file("./README.md", "text/markdown")
                                {
                                    Ok(file) => {
                                        this.status = format!("Uploaded local file {}", file.name)
                                    }
                                    Err(error) => {
                                        this.status = format!("upload local file failed: {error}")
                                    }
                                }
                                cx.notify();
                            }),
                        )
                        .child("📤 Upload ./README.md"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                match this
                                    .services
                                    .add_sample_channel_file_entry(cherry_core::FileSourceKind::WebDav)
                                {
                                    Ok(file) => {
                                        this.status =
                                            format!("Added remote file from WebDAV {}", file.name)
                                    }
                                    Err(error) => {
                                        this.status =
                                            format!("add webdav file entry failed: {error}")
                                    }
                                }
                                cx.notify();
                            }),
                        )
                        .child("☁️ Add WebDAV File"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                match this.services.remove_first_file_entry() {
                                    Ok(Some(id)) => this.status = format!("Removed file {id}"),
                                    Ok(None) => this.status = "No file to remove".to_owned(),
                                    Err(error) => this.status = format!("remove file failed: {error}"),
                                }
                                cx.notify();
                            }),
                        )
                        .child("➖ Remove First File"),
                )
                .child("Preview:")
                .child(self.file_preview())
                .child("Files:")
                .child(self.files_text()),
            AppRoute::Notes => div()
                .child("Notes Actions:")
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                if let Err(error) = this.services.add_sample_note_entry() {
                                    this.status = format!("add note failed: {error}");
                                } else {
                                    this.status = "Added sample note".to_owned();
                                }
                                cx.notify();
                            }),
                        )
                        .child("➕ Add Sample Note"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                match this.services.remove_first_note_entry() {
                                    Ok(Some(id)) => this.status = format!("Removed note {id}"),
                                    Ok(None) => this.status = "No note to remove".to_owned(),
                                    Err(error) => this.status = format!("remove note failed: {error}"),
                                }
                                cx.notify();
                            }),
                        )
                        .child("➖ Remove First Note"),
                )
                .child("Notes:")
                .child(self.notes_text()),
            AppRoute::Knowledge => div()
                .child("Knowledge Actions:")
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                if let Err(error) = this.services.add_sample_knowledge_document() {
                                    this.status = format!("add knowledge failed: {error}");
                                } else {
                                    this.status = "Added knowledge document".to_owned();
                                }
                                cx.notify();
                            }),
                        )
                        .child("➕ Add Knowledge Doc"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                match this.services.mark_first_knowledge_indexed() {
                                    Ok(Some(doc)) => this.status = format!("Marked indexed: {}", doc.title),
                                    Ok(None) => this.status = "No knowledge doc to mark".to_owned(),
                                    Err(error) => this.status = format!("mark indexed failed: {error}"),
                                }
                                cx.notify();
                            }),
                        )
                        .child("✅ Mark First Indexed"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                match this.services.remove_first_knowledge_document() {
                                    Ok(Some(id)) => this.status = format!("Removed knowledge {id}"),
                                    Ok(None) => this.status = "No knowledge doc to remove".to_owned(),
                                    Err(error) => this.status = format!("remove knowledge failed: {error}"),
                                }
                                cx.notify();
                            }),
                        )
                        .child("➖ Remove First Knowledge Doc"),
                )
                .child("Knowledge:")
                .child(self.knowledge_text()),
            AppRoute::MiniApps => div()
                .child("MiniApps Actions:")
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                if let Err(error) = this.services.add_sample_note_entry() {
                                    this.status = format!("create miniapp note failed: {error}");
                                } else {
                                    this.status = "Created miniapp config note".to_owned();
                                }
                                cx.notify();
                            }),
                        )
                        .child("🧩 Create MiniApp Note"),
                )
                .child(self.markdown_preview("MiniApp skeleton:\n- metadata\n- entrypoint\n- permissions")),
            AppRoute::CodeTools => div()
                .child("CodeTools Actions:")
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                if let Err(error) = this.send_quick_prompt("请审查以下 Rust 代码并给出改进建议。") {
                                    this.status = format!("send code review prompt failed: {error}");
                                }
                                cx.notify();
                            }),
                        )
                        .child("🛠 Send Code Review Prompt"),
                )
                .child(self.code_preview(
                    "rust",
                    "fn main() {\n    println!(\"hello from code tools\");\n}",
                )),
            AppRoute::OpenClaw => div()
                .child("OpenClaw Actions:")
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                match this.services.list_mcp_servers() {
                                    Ok(servers) => {
                                        if let Some(server) = servers.first() {
                                            match this.services.call_mcp_tool(
                                                server.id,
                                                "openclaw_query",
                                                "{\"query\":\"status\"}",
                                            ) {
                                                Ok(result) => {
                                                    this.status = format!(
                                                        "OpenClaw via MCP {}: {}",
                                                        result.server_name, result.output
                                                    );
                                                }
                                                Err(error) => this.status = format!("openclaw call failed: {error}"),
                                            }
                                        } else {
                                            this.status = "No MCP server available".to_owned();
                                        }
                                    }
                                    Err(error) => this.status = format!("list mcp failed: {error}"),
                                }
                                cx.notify();
                            }),
                        )
                        .child("🦀 Call OpenClaw MCP Bridge"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                let result = (|| -> anyhow::Result<()> {
                                    this.services.set_tool_permission("openclaw_query", true)?;
                                    this.services.set_tool_permission("list_files", true)?;
                                    Ok(())
                                })();
                                if let Err(error) = result {
                                    this.status = format!("set tool permissions failed: {error}");
                                } else {
                                    this.status =
                                        "Enabled tool permissions: openclaw_query/list_files"
                                            .to_owned();
                                }
                                cx.notify();
                            }),
                        )
                        .child("🔐 Allow MCP Tools"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                if let Err(error) = this.services.add_sample_protocol_handler() {
                                    this.status = format!("add protocol handler failed: {error}");
                                } else {
                                    this.status = "Protocol handler cherry:// registered".to_owned();
                                }
                                cx.notify();
                            }),
                        )
                        .child("🔗 Register cherry:// Handler"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                match this
                                    .services
                                    .resolve_protocol_url("cherry://openclaw?query=status")
                                {
                                    Ok(command) => {
                                        this.status =
                                            format!("Resolved protocol command: {}", command);
                                    }
                                    Err(error) => {
                                        this.status =
                                            format!("resolve protocol url failed: {error}");
                                    }
                                }
                                cx.notify();
                            }),
                        )
                        .child("🧭 Resolve cherry:// URL"),
                )
                .child("MCP Servers:")
                .child(self.mcp_text())
                .child("Tool Permissions:")
                .child(self.tool_permissions_text())
                .child("Protocol Handlers:")
                .child(self.protocol_handlers_text()),
            AppRoute::Settings => div()
                .child("Settings Sections:")
                .child(format!(
                    "Current: {} | {}",
                    self.settings_section.title(),
                    self.settings_section_summary()
                ))
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.settings_section = this.settings_section.previous();
                                this.status = format!(
                                    "Switched settings section: {}",
                                    this.settings_section.title()
                                );
                                cx.notify();
                            }),
                        )
                        .child("◀ Previous Section"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.settings_section = this.settings_section.next();
                                this.status = format!(
                                    "Switched settings section: {}",
                                    this.settings_section.title()
                                );
                                cx.notify();
                            }),
                        )
                        .child("▶ Next Section"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                if let Err(error) = this.apply_settings_section_action() {
                                    this.status = format!("apply section action failed: {error}");
                                }
                                cx.notify();
                            }),
                        )
                        .child("✅ Apply Section Action"),
                )
                .child("MCP Servers:")
                .child(self.mcp_text()),
            AppRoute::Launchpad => div()
                .child("Launchpad Actions:")
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                let legacy_dir = PathBuf::from("./legacy-data");
                                match cherry_services::import_from_legacy_dir(&this.services, &legacy_dir) {
                                    Ok(report) => {
                                        this.status = format!(
                                            "Legacy dir import done: conv={} msg={}",
                                            report.conversations, report.messages
                                        );
                                    }
                                    Err(error) => {
                                        this.status = format!("legacy dir import failed: {error}");
                                    }
                                }
                                cx.notify();
                            }),
                        )
                        .child("🚀 Import From ./legacy-data"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                let backup = PathBuf::from("./data/backup/launchpad-backup.json");
                                if let Err(error) = this.services.export_backup_json(&backup) {
                                    this.status = format!("launchpad backup failed: {error}");
                                } else {
                                    this.status = format!("Launchpad backup exported: {}", backup.display());
                                }
                                cx.notify();
                            }),
                        )
                        .child("💾 Export Launchpad Backup"),
                )
                .child(
                    div()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                let result = (|| -> anyhow::Result<()> {
                                    this.services
                                        .set_backup_channel_enabled(BackupChannel::WebDav, true)?;
                                    this.services
                                        .set_backup_channel_enabled(BackupChannel::S3, true)?;
                                    this.services
                                        .set_backup_channel_enabled(BackupChannel::Lan, true)?;
                                    let webdav =
                                        this.services.export_backup_to_channel(BackupChannel::WebDav)?;
                                    let s3 = this.services.export_backup_to_channel(BackupChannel::S3)?;
                                    let lan = this.services.export_backup_to_channel(BackupChannel::Lan)?;
                                    this.status = format!(
                                        "Launchpad channel backup exported: webdav={}, s3={}, lan={}",
                                        webdav.display(),
                                        s3.display(),
                                        lan.display()
                                    );
                                    Ok(())
                                })();
                                if let Err(error) = result {
                                    this.status = format!("launchpad channel backup failed: {error}");
                                }
                                cx.notify();
                            }),
                        )
                        .child("🌐 Export WebDAV/S3/LAN Backups"),
                )
                .child(self.markdown_preview(
                    "Launchpad modules:\n- migration\n- validation\n- deployment",
                )),
        };

        let content = div()
            .id("content")
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .p_3()
            .gap_2()
            .bg(rgb(0x0f172a))
            .text_color(rgb(0xe2e8f0))
            .whitespace_normal()
            .child(self.header_text())
            .child(self.summary_for_route())
            .child("Recent Messages:")
            .child(self.messages_text())
            .child("Streaming Preview:")
            .child(self.streaming_preview_text())
            .child(route_actions);

        div()
            .size_full()
            .flex()
            .bg(rgb(0x0b1120))
            .text_sm()
            .tab_index(0)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                if let Some(route) = Self::key_to_route(event.keystroke.key.as_str()) {
                    this.switch_route(route);
                    cx.notify();
                }
            }))
            .child(sidebar)
            .child(content)
    }
}

fn main() {
    init_tracing();

    let runtime = Arc::new(Runtime::new().expect("tokio runtime"));
    let db_path = default_db_path();
    let services = AppServicesBuilder::new(db_path)
        .build()
        .expect("initialize app services");
    services
        .seed_demo_workspace_data()
        .expect("seed workspace data");

    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.), px(860.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            {
                let services = services.clone();
                let runtime = runtime.clone();
                move |_window, cx| cx.new(|_| CherryAppView::new(services, runtime))
            },
        )
        .expect("open gpui window");
    });
}

fn default_db_path() -> PathBuf {
    let mut base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    base.push("data");
    base.push("cherry_studio_rs.sqlite3");
    base
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();
}
