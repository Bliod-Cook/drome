use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::error::{DromeError, Result};

fn mark_from_sync(action: &mut Value) -> Result<()> {
  let obj = action
    .as_object_mut()
    .ok_or_else(|| DromeError::Message("Invalid store sync action".into()))?;

  match obj.get_mut("meta") {
    Some(Value::Object(meta)) => {
      meta.insert("fromSync".into(), Value::Bool(true));
    }
    Some(_) => {
      let mut meta = Map::new();
      meta.insert("fromSync".into(), Value::Bool(true));
      obj.insert("meta".into(), Value::Object(meta));
    }
    None => {
      let mut meta = Map::new();
      meta.insert("fromSync".into(), Value::Bool(true));
      obj.insert("meta".into(), Value::Object(meta));
    }
  }

  Ok(())
}

pub fn store_sync_on_update(app: &AppHandle, sender: &WebviewWindow, mut action: Value) -> Result<()> {
  mark_from_sync(&mut action)?;

  let sender_label = sender.label().to_string();
  for (label, window) in app.webview_windows() {
    if label == sender_label {
      continue;
    }
    let _ = window.emit("store-sync:broadcast-sync", action.clone());
  }

  Ok(())
}

