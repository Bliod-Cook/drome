use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::collections::HashMap;

use crate::error::{DromeError, Result};

const CLIENT_ID: &str = "cherry-studio";
// Copied from Cherry Studio's obfuscated integration:
// cherry-studio/src/main/integration/cherryai/index.js
const CLIENT_SECRET_SUFFIX: &str = "GvI6I5ZrEHcGOWjO5AKhJKGmnwwGfM62XKpWqkjhvzRU2NZIinM77aTGIqhqys0g";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CherryAiSignatureParams {
  pub method: String,
  pub path: String,
  pub query: String,
  pub body: serde_json::Value,
}

fn read_secret_prefix() -> Option<String> {
  for key in [
    "CHERRYAI_CLIENT_SECRET",
    "MAIN_VITE_CHERRYAI_CLIENT_SECRET",
    "VITE_CHERRYAI_CLIENT_SECRET",
  ] {
    if let Ok(v) = std::env::var(key) {
      let v = v.trim().to_string();
      if !v.is_empty() {
        return Some(v);
      }
    }
  }
  None
}

pub fn cherryai_get_signature(params: CherryAiSignatureParams) -> Result<HashMap<String, String>> {
  let Some(prefix) = read_secret_prefix() else {
    // Degrade gracefully when no secret is present.
    return Ok(HashMap::new());
  };

  let client_secret = format!("{prefix}.{CLIENT_SECRET_SUFFIX}");
  let timestamp = chrono::Utc::now().timestamp().to_string();

  let body_string = if params.body.is_null() {
    String::new()
  } else {
    serde_json::to_string(&params.body)?
  };

  let signature_string = [
    params.method.to_uppercase(),
    params.path,
    params.query,
    CLIENT_ID.to_string(),
    timestamp.clone(),
    body_string,
  ]
  .join("\n");

  let mut mac = Hmac::<Sha256>::new_from_slice(client_secret.as_bytes())
    .map_err(|e| DromeError::Message(format!("Invalid CherryAI secret: {e}")))?;
  mac.update(signature_string.as_bytes());
  let signature = hex::encode(mac.finalize().into_bytes());

  let mut out = HashMap::new();
  out.insert("X-Client-ID".to_string(), CLIENT_ID.to_string());
  out.insert("X-Timestamp".to_string(), timestamp);
  out.insert("X-Signature".to_string(), signature);
  Ok(out)
}

