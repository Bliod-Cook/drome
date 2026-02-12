use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use uuid::Uuid;

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpFetchStreamStartResponse {
    pub stream_id: String,
    pub ok: bool,
    pub status: u16,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpFetchStreamReadRequest {
    pub stream_id: String,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpFetchStreamReadResponse {
    pub done: bool,
    pub chunk_base64: Option<String>,
    pub error: Option<String>,
}

enum StreamChunk {
    Data(Vec<u8>),
    Done,
    Error(String),
}

struct StreamSession {
    rx: Receiver<StreamChunk>,
}

static HTTP_STREAMS: OnceLock<Mutex<HashMap<String, StreamSession>>> = OnceLock::new();

fn stream_sessions() -> &'static Mutex<HashMap<String, StreamSession>> {
    HTTP_STREAMS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_stream_sessions() -> Result<std::sync::MutexGuard<'static, HashMap<String, StreamSession>>>
{
    stream_sessions()
        .lock()
        .map_err(|_| DromeError::Message("HTTP stream state poisoned".to_string()))
}

fn build_request(req: HttpFetchRequest) -> Result<reqwest::blocking::RequestBuilder> {
    let method = req.method.unwrap_or_else(|| "GET".to_string());
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|e| DromeError::Message(format!("Invalid HTTP method: {e}")))?;

    let mut builder = reqwest::blocking::Client::builder();
    if let Some(ms) = req.timeout_ms {
        builder = builder.timeout(Duration::from_millis(ms));
    }
    let client = builder
        .build()
        .map_err(|e| DromeError::Message(e.to_string()))?;

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

    Ok(request)
}

fn collect_headers(response: &reqwest::blocking::Response) -> HashMap<String, String> {
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
    headers_out
}

pub fn http_fetch(req: HttpFetchRequest) -> Result<HttpFetchResponse> {
    let mut response = build_request(req)?
        .send()
        .map_err(|e| DromeError::Message(e.to_string()))?;
    let status = response.status();

    let headers_out = collect_headers(&response);

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

pub fn http_fetch_stream_start(req: HttpFetchRequest) -> Result<HttpFetchStreamStartResponse> {
    let response = build_request(req)?
        .send()
        .map_err(|e| DromeError::Message(e.to_string()))?;
    let status = response.status();
    let headers = collect_headers(&response);
    let stream_id = Uuid::new_v4().to_string();

    let (tx, rx) = mpsc::sync_channel::<StreamChunk>(32);

    std::thread::spawn(move || {
        let mut response = response;
        let mut buffer = [0u8; 8 * 1024];

        loop {
            match response.read(&mut buffer) {
                Ok(0) => {
                    let _ = tx.send(StreamChunk::Done);
                    break;
                }
                Ok(read_len) => {
                    if tx
                        .send(StreamChunk::Data(buffer[..read_len].to_vec()))
                        .is_err()
                    {
                        break;
                    }
                }
                Err(err) => {
                    let _ = tx.send(StreamChunk::Error(err.to_string()));
                    break;
                }
            }
        }
    });

    {
        let mut sessions = lock_stream_sessions()?;
        sessions.insert(stream_id.clone(), StreamSession { rx });
    }

    Ok(HttpFetchStreamStartResponse {
        stream_id,
        ok: status.is_success(),
        status: status.as_u16(),
        headers,
    })
}

pub fn http_fetch_stream_read(
    req: HttpFetchStreamReadRequest,
) -> Result<HttpFetchStreamReadResponse> {
    let timeout_ms = req.timeout_ms.unwrap_or(30_000).max(1);
    let session = {
        let mut sessions = lock_stream_sessions()?;
        sessions.remove(&req.stream_id)
    };

    let Some(session) = session else {
        return Ok(HttpFetchStreamReadResponse {
            done: true,
            chunk_base64: None,
            error: Some("HTTP stream not found".to_string()),
        });
    };

    match session.rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(StreamChunk::Data(bytes)) => {
            let chunk_base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

            let mut sessions = lock_stream_sessions()?;
            sessions.insert(req.stream_id, session);

            Ok(HttpFetchStreamReadResponse {
                done: false,
                chunk_base64: Some(chunk_base64),
                error: None,
            })
        }
        Ok(StreamChunk::Done) => Ok(HttpFetchStreamReadResponse {
            done: true,
            chunk_base64: None,
            error: None,
        }),
        Ok(StreamChunk::Error(err)) => Ok(HttpFetchStreamReadResponse {
            done: true,
            chunk_base64: None,
            error: Some(err),
        }),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let mut sessions = lock_stream_sessions()?;
            sessions.insert(req.stream_id, session);

            Ok(HttpFetchStreamReadResponse {
                done: false,
                chunk_base64: None,
                error: None,
            })
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Ok(HttpFetchStreamReadResponse {
            done: true,
            chunk_base64: None,
            error: None,
        }),
    }
}

pub fn http_fetch_stream_cancel(stream_id: String) -> Result<()> {
    let mut sessions = lock_stream_sessions()?;
    sessions.remove(&stream_id);

    Ok(())
}
