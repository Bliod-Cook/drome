use std::sync::Arc;
use std::{fs, path::Path};

use config::{AppConfig, ConfigStore};
use core_orchestrator::Orchestrator;
use gpui::{
    App, Application, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};
use i18n::I18n;
use mcp_runtime::RustMcpRuntime;
use provider_zed::ZedProviderAdapter;
use secrets::{SecretStore, default_secret_dir_from};
use storage_sqlite::SqliteStorage;
use tracing::error;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Chat,
    Settings,
}

struct DromeApp {
    page: Page,
    i18n: I18n,
    config: AppConfig,
    status: SharedString,
}

impl DromeApp {
    fn new(config: AppConfig, status: impl Into<SharedString>) -> Self {
        let i18n = I18n::new(config.language);
        Self {
            page: Page::Chat,
            i18n,
            config,
            status: status.into(),
        }
    }
}

impl Render for DromeApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let nav = div()
            .flex()
            .gap_2()
            .child(nav_item(self.i18n.t("nav.chat"), self.page == Page::Chat))
            .child(nav_item(
                self.i18n.t("nav.settings"),
                self.page == Page::Settings,
            ));

        let content = match self.page {
            Page::Chat => self.render_chat().into_any_element(),
            Page::Settings => self.render_settings(cx).into_any_element(),
        };

        div()
            .bg(rgb(0x121212))
            .text_color(rgb(0xeeeeee))
            .size_full()
            .flex()
            .flex_col()
            .p_4()
            .gap_3()
            .child(div().text_xl().child(self.i18n.t("app.title").to_string()))
            .child(nav)
            .child(content)
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x9a9a9a))
                    .child(self.status.clone()),
            )
    }
}

impl DromeApp {
    fn render_chat(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_base()
                    .child(self.i18n.t("chat.placeholder").to_string()),
            )
            .child(div().text_sm().child(format!(
                "{}: {}",
                self.i18n.t("settings.providers"),
                self.config.providers.len()
            )))
            .child(div().text_sm().child(format!(
                "{}: {}",
                self.i18n.t("settings.mcp"),
                self.config.mcp_servers.len()
            )))
    }

    fn render_settings(&mut self, _cx: &mut Context<Self>) -> impl IntoElement {
        let encryption_text = if self.config.security.local_encryption_enabled {
            self.i18n.t("settings.encryption.on")
        } else {
            self.i18n.t("settings.encryption.off")
        };

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().child(format!(
                "{}: {}",
                self.i18n.t("settings.providers"),
                self.config.providers.len()
            )))
            .child(div().child(format!(
                "{}: {}",
                self.i18n.t("settings.mcp"),
                self.config.mcp_servers.len()
            )))
            .child(div().child(format!(
                "{}: {}",
                self.i18n.t("settings.encryption"),
                encryption_text
            )))
            .child(div().bg(rgb(0x2d2d2d)).p_2().rounded_md().child("中文"))
            .child(div().bg(rgb(0x2d2d2d)).p_2().rounded_md().child("English"))
    }
}

fn nav_item(label: &str, selected: bool) -> impl IntoElement {
    div()
        .bg(if selected {
            rgb(0x3355aa)
        } else {
            rgb(0x2d2d2d)
        })
        .text_color(rgb(0xffffff))
        .rounded_md()
        .px_3()
        .py_1()
        .child(label.to_string())
}

fn main() {
    let mut data_dir = dirs::data_local_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    data_dir.push("drome");
    if let Err(err) = fs::create_dir_all(&data_dir) {
        eprintln!("failed to prepare data dir: {err}");
    }
    let _log_guard = init_local_logger(&data_dir.join("logs"));

    let config_store = ConfigStore::from_dir(data_dir.join("config"));
    let config = match config_store.load_or_init() {
        Ok(cfg) => cfg,
        Err(err) => {
            error!("failed to load config: {err}");
            AppConfig::default()
        }
    };

    let mut secret_store = SecretStore::new(default_secret_dir_from(&data_dir));
    if config.security.local_encryption_enabled {
        secret_store.set_password(Some("change_me".to_string()));
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("failed to create tokio runtime: {err}");
            return;
        }
    };

    let storage_status = runtime.block_on(async {
        let sqlite_path = data_dir.join("drome.db");
        match SqliteStorage::connect(&sqlite_path).await {
            Ok(_) => "storage ready",
            Err(_) => "storage init failed",
        }
    });

    let provider = Arc::new(ZedProviderAdapter::new());
    let mcp = Arc::new(RustMcpRuntime::new());
    let _orchestrator = Orchestrator::new(provider, Some(mcp));

    let status = format!("M1 bootstrap complete: {storage_status}");
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(980.0), px(700.0)), cx);
        let config_for_ui = config.clone();
        let status_for_ui = status.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Drome".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_, cx| cx.new(|_| DromeApp::new(config_for_ui, status_for_ui)),
        )
        .expect("open main window");
        cx.activate(true);
    });
}

fn init_local_logger(log_dir: &Path) -> tracing_appender::non_blocking::WorkerGuard {
    if let Err(err) = fs::create_dir_all(log_dir) {
        eprintln!("failed to create log dir `{}`: {err}", log_dir.display());
    }
    let file_appender = tracing_appender::rolling::daily(log_dir, "drome.log");
    let (writer, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,app_desktop=debug"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .json()
        .with_writer(writer)
        .init();

    guard
}
