use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::time::Duration;

use crate::error::{DromeError, Result};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpFetchRequest {
  pub url: String,
  pub method: Option<String>,
  pub headers: Option<HashMap<String, String>>,
  pub body: Option<String>,
  pub body_base64: Option<String>,
  pub timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpFetchResponse {
  pub ok: bool,
  pub status: u16,
  pub headers: HashMap<String, String>,
  pub body_base64: String,
}

pub fn http_fetch(req: HttpFetchRequest) -> Result<HttpFetchResponse> {
  let method = req.method.unwrap_or_else(|| "GET".to_string());
  let method = reqwest::Method::from_bytes(method.as_bytes())
    .map_err(|e| DromeError::Message(format!("Invalid HTTP method: {e}")))?;

  let mut builder = reqwest::blocking::Client::builder();
  if let Some(ms) = req.timeout_ms {
    builder = builder.timeout(Duration::from_millis(ms));
  }
  let client = builder.build().map_err(|e| DromeError::Message(e.to_string()))?;

  let mut request = client.request(method, &req.url);

  if let Some(headers) = req.headers {
    let mut header_map = reqwest::header::HeaderMap::new();
    for (k, v) in headers {
      if k.trim().is_empty() {
        continue;
      }
      let Ok(name) = reqwest::header::HeaderName::from_bytes(k.as_bytes()) else {
        continue;
      };
      let Ok(value) = reqwest::header::HeaderValue::from_str(&v) else {
        continue;
      };
      header_map.append(name, value);
    }
    request = request.headers(header_map);
  }

  if let Some(body_b64) = req.body_base64 {
    let bytes = base64::engine::general_purpose::STANDARD
      .decode(body_b64.as_bytes())
      .map_err(|e| DromeError::Message(format!("Invalid base64 body: {e}")))?;
    request = request.body(bytes);
  } else if let Some(body) = req.body {
    request = request.body(body.into_bytes());
  }

  let mut response = request.send().map_err(|e| DromeError::Message(e.to_string()))?;
  let status = response.status();

  let mut headers_out: HashMap<String, String> = HashMap::new();
  for (name, value) in response.headers().iter() {
    let key = name.as_str().to_string();
    let val = value.to_str().unwrap_or_default().to_string();
    headers_out
      .entry(key)
      .and_modify(|existing| {
        if !existing.is_empty() {
          existing.push_str(", ");
        }
        existing.push_str(&val);
      })
      .or_insert(val);
  }

  let mut body = Vec::new();
  response
    .read_to_end(&mut body)
    .map_err(|e| DromeError::Message(e.to_string()))?;

  let body_base64 = base64::engine::general_purpose::STANDARD.encode(&body);

  Ok(HttpFetchResponse {
    ok: status.is_success(),
    status: status.as_u16(),
    headers: headers_out,
    body_base64,
  })
}

