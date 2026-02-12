use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::error::{DromeError, Result};

const MINI_WINDOW_LABEL: &str = "miniWindow";

fn get_or_create(app: &AppHandle) -> Result<WebviewWindow> {
    if let Some(win) = app.get_webview_window(MINI_WINDOW_LABEL) {
        return Ok(win);
    }

    WebviewWindowBuilder::new(
        app,
        MINI_WINDOW_LABEL,
        WebviewUrl::App("miniWindow.html".into()),
    )
    .title("Mini Window")
    .inner_size(420.0, 680.0)
    .resizable(true)
    .build()
    .map_err(|e| DromeError::Message(e.to_string()))
}

pub fn mini_window_show(app: &AppHandle) -> Result<()> {
    let win = get_or_create(app)?;
    win.show().map_err(|e| DromeError::Message(e.to_string()))?;
    let _ = win.set_focus();
    Ok(())
}

pub fn mini_window_hide(app: &AppHandle) -> Result<()> {
    if let Some(win) = app.get_webview_window(MINI_WINDOW_LABEL) {
        win.hide().map_err(|e| DromeError::Message(e.to_string()))?;
    }
    Ok(())
}

pub fn mini_window_close(app: &AppHandle) -> Result<()> {
    if let Some(win) = app.get_webview_window(MINI_WINDOW_LABEL) {
        win.close()
            .map_err(|e| DromeError::Message(e.to_string()))?;
    }
    Ok(())
}

pub fn mini_window_toggle(app: &AppHandle) -> Result<()> {
    let win = get_or_create(app)?;
    let visible = win.is_visible().unwrap_or(true);
    if visible {
        win.hide().map_err(|e| DromeError::Message(e.to_string()))?;
    } else {
        win.show().map_err(|e| DromeError::Message(e.to_string()))?;
        let _ = win.set_focus();
    }
    Ok(())
}

pub fn mini_window_set_pin(app: &AppHandle, is_pinned: bool) -> Result<()> {
    let win = get_or_create(app)?;
    win.set_always_on_top(is_pinned)
        .map_err(|e| DromeError::Message(e.to_string()))?;
    Ok(())
}
