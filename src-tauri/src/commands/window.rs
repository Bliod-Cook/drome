use tauri::{Emitter, WebviewWindow};

use crate::error::{DromeError, Result};

pub fn window_minimize(window: &WebviewWindow) -> Result<()> {
    window
        .minimize()
        .map_err(|e| DromeError::Message(e.to_string()))?;
    Ok(())
}

pub fn window_maximize(window: &WebviewWindow) -> Result<()> {
    window
        .maximize()
        .map_err(|e| DromeError::Message(e.to_string()))?;
    let is_maximized = window.is_maximized().unwrap_or(true);
    let _ = window.emit("window:maximized-changed", is_maximized);
    Ok(())
}

pub fn window_unmaximize(window: &WebviewWindow) -> Result<()> {
    window
        .unmaximize()
        .map_err(|e| DromeError::Message(e.to_string()))?;
    let is_maximized = window.is_maximized().unwrap_or(false);
    let _ = window.emit("window:maximized-changed", is_maximized);
    Ok(())
}

pub fn window_close(window: &WebviewWindow) -> Result<()> {
    window
        .close()
        .map_err(|e| DromeError::Message(e.to_string()))?;
    Ok(())
}

pub fn window_is_maximized(window: &WebviewWindow) -> Result<bool> {
    window
        .is_maximized()
        .map_err(|e| DromeError::Message(e.to_string()))
}

pub fn window_set_minimum_size(window: &WebviewWindow, width: f64, height: f64) -> Result<()> {
    window
        .set_min_size(Some(tauri::LogicalSize::new(width, height)))
        .map_err(|e| DromeError::Message(e.to_string()))?;
    Ok(())
}

pub fn window_reset_minimum_size(window: &WebviewWindow) -> Result<()> {
    window
        .set_min_size::<tauri::LogicalSize<f64>>(None)
        .map_err(|e| DromeError::Message(e.to_string()))?;
    Ok(())
}

pub fn window_get_size(window: &WebviewWindow) -> Result<(u32, u32)> {
    let size = window
        .inner_size()
        .map_err(|e| DromeError::Message(e.to_string()))?;
    Ok((size.width, size.height))
}
