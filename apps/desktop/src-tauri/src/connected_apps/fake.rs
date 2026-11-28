//! A fake Composio for tests (docs/features/connected-apps/tasks.md, T1):
//! a small HTTP server on 127.0.0.1 that answers with the recorded shapes
//! in `testdata/` (or what a test sets) and keeps every request it got.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

/// One request the fake got.
#[derive(Debug, Clone)]
pub struct Seen {
    pub method: String,
    /// Path and query, after the base (`/tool_router/session`).
    pub path: String,
    pub key: Option<String>,
    pub body: Value,
}

type Route = Arc<dyn Fn(&Seen) -> (u16, Value) + Send + Sync>;

#[derive(Clone)]
pub struct Fake {
    /// `http://127.0.0.1:<port>/api/v3.1`.
    pub base: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    routes: Arc<Mutex<Vec<(String, String, Route)>>>,
    /// The key it accepts.
    pub key: Arc<Mutex<String>>,
}

/// A recorded answer from `testdata/`.
pub fn fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/connected_apps/testdata")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

impl Fake {
    /// Start it; it accepts `key` and answers the standard routes.
    pub async fn start(key: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().unwrap().port();
        let fake = Fake {
            base: format!("http://127.0.0.1:{port}/api/v3.1"),
            seen: Arc::new(Mutex::new(Vec::new())),
            routes: Arc::new(Mutex::new(Vec::new())),
            key: Arc::new(Mutex::new(key.to_string())),
        };
        fake.standard_routes(port);
        let server = fake.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let server = server.clone();
                tokio::spawn(async move {
                    let _ = server.serve(stream).await;
                });
            }
        });
        fake
    }

    fn standard_routes(&self, port: u16) {
        let mcp = format!("http://127.0.0.1:{port}/tool_router/trs_fake/mcp");
        let session = {
            let mut session = fixture("session_create.json");
            session["session_id"] = json!("trs_fake");
            session["mcp"]["url"] = json!(mcp);
            session
        };
        let created = session.clone();
        self.route("POST", "/tool_router/session", move |_| {
            (201, created.clone())
        });
        self.route("GET", "/tool_router/session/", move |seen| {
            let id = seen
                .path
                .trim_start_matches("/tool_router/session/")
                .split(['/', '?'])
                .next()
                .unwrap_or("")
                .to_string();
            if id == "trs_fake" {
                (200, session.clone())
            } else {
                (404, json!({"error": {"message": "Session not found"}}))
            }
        });
        self.route("GET", "/connected_accounts", |_| {
            (200, json!({"items": [], "next_cursor": null}))
        });
    }

    /// Answer `method` requests whose path starts with `prefix` (later
    /// routes win).
    pub fn route(
        &self,
        method: &str,
        prefix: &str,
        answer: impl Fn(&Seen) -> (u16, Value) + Send + Sync + 'static,
    ) {
        self.routes.lock().unwrap().insert(
            0,
            (method.to_string(), prefix.to_string(), Arc::new(answer)),
        );
    }

    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    async fn serve(&self, stream: tokio::net::TcpStream) -> std::io::Result<()> {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let target = parts.next().unwrap_or("").to_string();
        let mut headers = HashMap::new();
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).await?;
            let header = header.trim_end();
            if header.is_empty() {
                break;
            }
            if let Some((name, value)) = header.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
            }
        }
        let length: usize = headers
            .get("content-length")
            .and_then(|l| l.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0; length];
        reader.read_exact(&mut body).await?;
        let path = target
            .strip_prefix("/api/v3.1")
            .unwrap_or(&target)
            .to_string();
        let seen = Seen {
            method,
            path,
            key: headers.get("x-api-key").cloned(),
            body: serde_json::from_slice(&body).unwrap_or(Value::Null),
        };
        self.seen.lock().unwrap().push(seen.clone());
        let (status, answer) = if seen.key.as_deref() != Some(self.key.lock().unwrap().as_str()) {
            (
                401,
                json!({"error": {"message": "Invalid API key", "status": 401}}),
            )
        } else {
            let route = self
                .routes
                .lock()
                .unwrap()
                .iter()
                .find(|(m, p, _)| *m == seen.method && seen.path.starts_with(p.as_str()))
                .map(|(_, _, r)| r.clone());
            match route {
                Some(route) => route(&seen),
                None => (
                    404,
                    json!({"error": {"message": format!("no route {} {}", seen.method, seen.path)}}),
                ),
            }
        };
        let text = answer.to_string();
        let response = format!("HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}", text.len());
        let stream = reader.get_mut();
        stream.write_all(response.as_bytes()).await?;
        stream.shutdown().await
    }
}
