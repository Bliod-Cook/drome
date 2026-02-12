use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::error::{DromeError, Result};
use crate::state::AppState;

const TRACE_WINDOW_LABEL: &str = "traceWindow";

fn trace_dir(state: &State<'_, AppState>) -> PathBuf {
    state.app_data_dir.join("Data").join("Trace")
}

fn ensure_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    Ok(())
}

fn topic_map_path(state: &State<'_, AppState>) -> PathBuf {
    trace_dir(state).join("topic_map.json")
}

fn entities_dir(state: &State<'_, AppState>) -> PathBuf {
    trace_dir(state).join("entities")
}

fn read_json_map(path: &Path) -> HashMap<String, String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    serde_json::from_str::<HashMap<String, String>>(&content).unwrap_or_default()
}

fn write_json_map(path: &Path, map: &HashMap<String, String>) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let content = serde_json::to_string_pretty(map)?;
    std::fs::write(path, content)?;
    Ok(())
}

fn span_id(entity: &Value) -> Option<String> {
    entity
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn trace_id(entity: &Value) -> Option<String> {
    entity
        .get("traceId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn model_name(entity: &Value) -> Option<String> {
    entity
        .get("modelName")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn entity_topic_id(state: &State<'_, AppState>, entity: &Value) -> Option<String> {
    if let Some(t) = entity.get("topicId").and_then(|v| v.as_str()) {
        if !t.trim().is_empty() {
            return Some(t.to_string());
        }
    }

    let Some(tid) = trace_id(entity) else {
        return None;
    };

    let map = read_json_map(&topic_map_path(state));
    map.get(&tid).cloned()
}

fn unbound_trace_path(state: &State<'_, AppState>, trace_id: &str) -> PathBuf {
    trace_dir(state)
        .join("_unbound")
        .join(format!("{trace_id}.json"))
}

fn trace_file_path(state: &State<'_, AppState>, topic_id: &str, trace_id: &str) -> PathBuf {
    trace_dir(state)
        .join(topic_id)
        .join(format!("{trace_id}.json"))
}

fn read_spans(path: &Path) -> Vec<Value> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<Value>>(&content).unwrap_or_default()
}

fn write_spans(path: &Path, spans: &[Value]) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let content = serde_json::to_string_pretty(spans)?;
    std::fs::write(path, content)?;
    Ok(())
}

fn upsert_span(spans: &mut Vec<Value>, entity: &Value) {
    let Some(id) = span_id(entity) else {
        spans.push(entity.clone());
        return;
    };

    if let Some(existing) = spans
        .iter_mut()
        .find(|v| v.get("id").and_then(|vv| vv.as_str()) == Some(id.as_str()))
    {
        *existing = entity.clone();
    } else {
        spans.push(entity.clone());
    }
}

pub fn trace_save_data(_state: &State<'_, AppState>, _topic_id: String) -> Result<()> {
    // No-op: entities are persisted eagerly via saveEntity.
    Ok(())
}

pub fn trace_bind_topic(
    state: &State<'_, AppState>,
    topic_id: String,
    trace_id: String,
) -> Result<()> {
    ensure_dir(&trace_dir(state))?;

    let path = topic_map_path(state);
    let mut map = read_json_map(&path);
    map.insert(trace_id.clone(), topic_id.clone());
    write_json_map(&path, &map)?;

    // If we have an unbound trace file, move it under the topic folder.
    let unbound = unbound_trace_path(state, &trace_id);
    if unbound.exists() {
        let dest = trace_file_path(state, &topic_id, &trace_id);
        if let Some(parent) = dest.parent() {
            ensure_dir(parent)?;
        }
        let _ = std::fs::rename(&unbound, &dest);
    }

    Ok(())
}

pub fn trace_get_data(
    state: &State<'_, AppState>,
    topic_id: String,
    trace_id: String,
    model_name: Option<String>,
) -> Result<Vec<Value>> {
    let path = trace_file_path(state, &topic_id, &trace_id);
    let mut spans = read_spans(&path);
    if let Some(m) = model_name {
        spans.retain(|v| v.get("modelName").and_then(|vv| vv.as_str()) == Some(m.as_str()));
    }
    Ok(spans)
}

pub fn trace_save_entity(state: &State<'_, AppState>, entity: Value) -> Result<()> {
    ensure_dir(&trace_dir(state))?;
    ensure_dir(&entities_dir(state))?;

    let sid = span_id(&entity).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let eid_path = entities_dir(state).join(format!("{sid}.json"));
    std::fs::write(&eid_path, serde_json::to_string_pretty(&entity)?)?;

    let Some(tid) = trace_id(&entity) else {
        return Ok(());
    };

    let topic_id = entity_topic_id(state, &entity);
    let trace_path = if let Some(ref topic_id) = topic_id {
        trace_file_path(state, topic_id, &tid)
    } else {
        unbound_trace_path(state, &tid)
    };

    let mut spans = read_spans(&trace_path);
    upsert_span(&mut spans, &entity);
    write_spans(&trace_path, &spans)?;
    Ok(())
}

pub fn trace_get_entity(state: &State<'_, AppState>, span_id: String) -> Result<Value> {
    let path = entities_dir(state).join(format!("{span_id}.json"));
    if !path.exists() {
        return Ok(Value::Null);
    }
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn trace_token_usage(state: &State<'_, AppState>, span_id: String, usage: Value) -> Result<()> {
    let path = entities_dir(state).join(format!("{span_id}.json"));
    if !path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&path)?;
    let mut entity: Value = serde_json::from_str(&content)?;
    if let Some(obj) = entity.as_object_mut() {
        obj.insert("usage".into(), usage);
    }
    std::fs::write(&path, serde_json::to_string_pretty(&entity)?)?;
    Ok(())
}

pub fn trace_clean_history(
    state: &State<'_, AppState>,
    topic_id: String,
    trace_id: String,
    model_name: Option<String>,
) -> Result<()> {
    let path = trace_file_path(state, &topic_id, &trace_id);
    if !path.exists() {
        return Ok(());
    }

    if model_name.is_none() {
        let _ = std::fs::remove_file(path);
        return Ok(());
    }

    let m = model_name.unwrap();
    let mut spans = read_spans(&path);
    spans.retain(|v| v.get("modelName").and_then(|vv| vv.as_str()) != Some(m.as_str()));
    write_spans(&path, &spans)?;
    Ok(())
}

pub fn trace_clean_topic(
    state: &State<'_, AppState>,
    topic_id: String,
    trace_id: Option<String>,
) -> Result<()> {
    let base = trace_dir(state).join(&topic_id);
    if let Some(tid) = trace_id {
        let path = trace_file_path(state, &topic_id, &tid);
        let _ = std::fs::remove_file(path);
        return Ok(());
    }
    if base.exists() {
        let _ = std::fs::remove_dir_all(base);
    }
    Ok(())
}

pub fn trace_clean_local_data(state: &State<'_, AppState>) -> Result<()> {
    let base = trace_dir(state);
    if base.exists() {
        let _ = std::fs::remove_dir_all(&base);
    }
    ensure_dir(&base)?;
    Ok(())
}

pub fn trace_add_end_message(
    _state: &State<'_, AppState>,
    _span_id: String,
    _model_name: String,
    _message: String,
) -> Result<()> {
    // Best-effort no-op for now.
    Ok(())
}

pub fn trace_add_stream_message(
    _state: &State<'_, AppState>,
    _span_id: String,
    _model_name: String,
    _context: String,
    _message: Value,
) -> Result<()> {
    // Best-effort no-op for now.
    Ok(())
}

fn store_language(state: &State<'_, AppState>) -> Option<String> {
    let store_path = state.app_config_dir.join("store.json");
    let content = std::fs::read_to_string(store_path).ok()?;
    let value: Value = serde_json::from_str(&content).ok()?;
    value
        .get("language")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn get_or_create_trace_window(app: &AppHandle) -> Result<WebviewWindow> {
    if let Some(win) = app.get_webview_window(TRACE_WINDOW_LABEL) {
        return Ok(win);
    }

    WebviewWindowBuilder::new(
        app,
        TRACE_WINDOW_LABEL,
        WebviewUrl::App("traceWindow.html".into()),
    )
    .title("Trace")
    .inner_size(900.0, 700.0)
    .resizable(true)
    .build()
    .map_err(|e| DromeError::Message(e.to_string()))
}

pub fn trace_open_window(
    app: &AppHandle,
    _window: &WebviewWindow,
    state: &State<'_, AppState>,
    topic_id: String,
    trace_id: String,
    _auto_open: Option<bool>,
    model_name: Option<String>,
) -> Result<()> {
    let win = get_or_create_trace_window(app)?;
    win.show().map_err(|e| DromeError::Message(e.to_string()))?;
    let _ = win.set_focus();

    let _ = win.emit(
        "set-trace",
        serde_json::json!({
          "topicId": topic_id,
          "traceId": trace_id,
          "modelName": model_name
        }),
    );

    let lang = store_language(state).unwrap_or_else(|| "en".into());
    let _ = win.emit("set-language", serde_json::json!({ "lang": lang }));
    Ok(())
}

pub fn trace_set_title(window: &WebviewWindow, title: String) -> Result<()> {
    window
        .set_title(&title)
        .map_err(|e| DromeError::Message(e.to_string()))?;
    Ok(())
}
