//! Plugin AI on the host API (docs/features/plugin-ai/spec.md, US-5;
//! plan 3.3): a plugin's backend and hooks use the same AI as its views.
//!
//! ```text
//! GET  /v1/ai/models               → [model]
//! POST /v1/ai/run  {…}             → NDJSON: {"type":"run","runId"} then the run's events, to `done`
//! GET  /v1/ai/runs/{id}            → {live} or {live:false, run, result, events}
//! POST /v1/ai/runs/{id}/cancel     → 204
//! GET  /v1/ai/threads              → [thread]
//! GET  /v1/ai/threads/{id}         → {id, title, messages}
//! DELETE /v1/ai/threads/{id}       → 204
//! ```
//!
//! The run's stream is a close-delimited HTTP/1.1 body (spike S2).

use std::sync::Arc;

use serde_json::{json, Map, Value};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use super::{Follower, Refusal};
use crate::plugin_host::{http, Backing, Caller, Host, Request, Response};

fn status(code: &str) -> u16 {
    match code {
        "unauthorized" => 401,
        "no_permission" => 403,
        "not_found" | "unknown_account" => 404,
        "busy" | "rate_limited" => 429,
        "budget" => 402,
        "bad_request" => 400,
        _ => 503,
    }
}

fn refused((code, message): Refusal) -> Response {
    Response { status: status(code), body: Some(json!({ "code": code, "message": message })) }
}

/// Whether `path` is one of these routes.
pub fn handles(path: &str) -> bool {
    path.starts_with("/v1/ai/")
}

/// The plugin a request comes from, when its key is good and its part may run.
async fn plugin_of(host: &'static Host, backing: Arc<dyn Backing>, request: &Request) -> Result<String, Response> {
    let refuse = |status: u16, code: &str, message: &str| Response { status, body: Some(json!({ "code": code, "message": message })) };
    if request.origin {
        return Err(refuse(403, "no_permission", "browser pages may not call the plugin host"));
    }
    let key = request.key.clone().ok_or_else(|| refuse(401, "unauthorized", "send `Authorization: Bearer $EVO_PLUGIN_KEY`"))?;
    let (plugin, caller) = host.grant_of(&key).ok_or_else(|| refuse(401, "unauthorized", "this key is not valid"))?;
    if !host.admit_key(&key) {
        return Err(refuse(429, "rate_limited", "too many calls; wait a few seconds"));
    }
    let id = plugin.clone();
    let found = tauri::async_runtime::spawn_blocking(move || backing.plugin(&id)).await.ok().flatten();
    let Some((installed, _)) = found else {
        return Err(refuse(401, "unauthorized", "the plugin is off or removed"));
    };
    let part = match caller {
        Caller::Backend => "runtime",
        Caller::Hooks => "hooks",
    };
    if !installed.approved_permissions.iter().any(|p| p == part) {
        return Err(refuse(403, "no_permission", "the plugin may not run this part now"));
    }
    Ok(plugin)
}

/// Answer one `/v1/ai/*` request on `stream`.
pub async fn serve(stream: &mut TcpStream, host: &'static Host, backing: Arc<dyn Backing>, request: Request) {
    let plugin = match plugin_of(host, backing, &request).await {
        Ok(plugin) => plugin,
        Err(response) => return write(stream, &response).await,
    };
    let parts: Vec<&str> = request.path.trim_matches('/').split('/').collect();
    let response = match (request.method.as_str(), parts.as_slice()) {
        ("GET", ["v1", "ai", "models"]) => match super::models(&plugin, false).await {
            Ok(models) => Response { status: 200, body: Some(models) },
            Err(refusal) => refused(refusal),
        },
        ("POST", ["v1", "ai", "run"]) => {
            let body: Map<String, Value> = match serde_json::from_slice::<Value>(&request.body) {
                Ok(Value::Object(map)) => map,
                _ => return write(stream, &refused(("bad_request", "send a JSON object: {messages, need?, …}".into()))).await,
            };
            let (tx, rx) = mpsc::unbounded_channel();
            match super::start_run(&plugin, body, Follower::Stream(tx)).await {
                Ok(run_id) => return stream_run(stream, &run_id, rx).await,
                Err(refusal) => refused(refusal),
            }
        }
        ("GET", ["v1", "ai", "runs", id]) => {
            let (tx, _rx) = mpsc::unbounded_channel();
            match super::follow(&plugin, id, Follower::Stream(tx)) {
                Ok(value) => Response { status: 200, body: Some(value) },
                Err(refusal) => refused(refusal),
            }
        }
        ("POST", ["v1", "ai", "runs", id, "cancel"]) => match super::cancel(Some(&plugin), id).await {
            Ok(()) => Response { status: 204, body: None },
            Err(refusal) => refused(refusal),
        },
        ("GET", ["v1", "ai", "threads"]) => match super::threads(&plugin).await {
            Ok(list) => Response { status: 200, body: Some(list) },
            Err(refusal) => refused(refusal),
        },
        ("GET", ["v1", "ai", "threads", thread @ ..]) if !thread.is_empty() => match super::thread(&plugin, &thread.join("/")).await {
            Ok(value) => Response { status: 200, body: Some(value) },
            Err(refusal) => refused(refusal),
        },
        ("DELETE", ["v1", "ai", "threads", thread @ ..]) if !thread.is_empty() => match super::delete_thread(&plugin, &thread.join("/")).await {
            Ok(()) => Response { status: 204, body: None },
            Err(refusal) => refused(refusal),
        },
        _ => Response { status: 404, body: Some(json!({ "code": "not_found", "message": format!("no {} {}", request.method, request.path) })) },
    };
    write(stream, &response).await;
}

async fn write(stream: &mut TcpStream, response: &Response) {
    let _ = stream.write_all(&http::write(response)).await;
    let _ = stream.shutdown().await;
}

/// The run's events as NDJSON until its `done`.
async fn stream_run(stream: &mut TcpStream, run_id: &str, mut rx: mpsc::UnboundedReceiver<Value>) {
    let head = "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n";
    if stream.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    let first = json!({ "type": "run", "runId": run_id }).to_string() + "\n";
    let _ = stream.write_all(first.as_bytes()).await;
    while let Some(event) = rx.recv().await {
        let done = event["type"] == "done";
        let line = event.to_string() + "\n";
        if stream.write_all(line.as_bytes()).await.is_err() {
            // The backend went away; the run goes on (FR-24).
            return;
        }
        if done {
            break;
        }
    }
    let _ = stream.shutdown().await;
}
