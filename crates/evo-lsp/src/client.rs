//! A small Language Server Protocol client over stdio: requests and
//! responses, the server's own requests answered with defaults, published
//! diagnostics and work-done progress (so callers can wait for indexing).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::sync::{mpsc, oneshot, Notify};
use url::Url;

use crate::catalog::Server;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// The extension the catalog maps to a language id (`dockerfile` for a
/// `Dockerfile`).
fn extension_of(path: &Path) -> String {
    if path.file_name().is_some_and(|n| {
        n.to_string_lossy()
            .to_ascii_lowercase()
            .starts_with("dockerfile")
    }) {
        "dockerfile".to_string()
    } else {
        path.extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

pub fn uri_of(path: &Path) -> String {
    Url::from_file_path(path)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("file://{}", path.display()))
}

/// One spelling per file: servers write the same file as `file:///C:/a` or
/// `file:///c%3A/a`.
pub fn uri_key(uri: &str) -> String {
    match path_of(uri) {
        Some(path) if cfg!(windows) => path.to_string_lossy().to_lowercase(),
        Some(path) => path.to_string_lossy().into_owned(),
        None => uri.to_string(),
    }
}

pub fn path_of(uri: &str) -> Option<PathBuf> {
    Url::parse(uri).ok()?.to_file_path().ok()
}

struct Open {
    version: i64,
    text: String,
}

pub struct Client {
    pub server: &'static Server,
    pub root: PathBuf,
    /// Frames to write; one task owns the server's stdin.
    writer: mpsc::UnboundedSender<Vec<u8>>,
    child: Mutex<Option<Child>>,
    next_id: AtomicI64,
    pending: Mutex<HashMap<i64, oneshot::Sender<Result<Value, String>>>>,
    /// uri → (version the server reported, diagnostics, when they arrived).
    diagnostics: Mutex<HashMap<String, (Option<i64>, Vec<Value>, Instant, String)>>,
    diagnostics_changed: Notify,
    progress: Mutex<HashSet<String>>,
    progress_changed: Notify,
    /// Any work-done progress began since start.
    progress_seen: AtomicBool,
    /// rust-analyzer's `experimental/serverStatus`: idle when `true`.
    quiescent: Mutex<Option<bool>>,
    /// When a piece of the server's work last ended (a work-done progress,
    /// or rust-analyzer turning quiescent).
    work_ended: Mutex<Option<Instant>>,
    /// When the last file text was sent, until the next report arrives.
    sent_at: Mutex<Option<Instant>>,
    /// The longest the server took to report after a text was sent.
    slowest_report: Mutex<Duration>,
    /// Whether `textDocument/diagnostic` (pull) answers: `None` until tried.
    pull_works: Mutex<Option<bool>>,
    started: Instant,
    open: tokio::sync::Mutex<HashMap<String, Open>>,
    alive: AtomicBool,
    stderr: Mutex<VecDeque<String>>,
    pub capabilities: Mutex<Value>,
}

impl Client {
    /// Start `binary` for `server` in `root` and initialize it.
    pub async fn start(
        server: &'static Server,
        binary: &Path,
        env: Vec<(String, String)>,
        init_options: Value,
        root: &Path,
    ) -> Result<Arc<Client>, String> {
        let mut cmd = tokio::process::Command::new(binary);
        cmd.args(server.args)
            .current_dir(root)
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("{} could not start: {e}", server.name))?;
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        let (writer, mut frames) = mpsc::unbounded_channel::<Vec<u8>>();
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take();
        let client = Arc::new(Client {
            server,
            root: root.to_path_buf(),
            writer,
            child: Mutex::new(Some(child)),
            next_id: AtomicI64::new(1),
            pending: Mutex::new(HashMap::new()),
            diagnostics: Mutex::new(HashMap::new()),
            diagnostics_changed: Notify::new(),
            progress: Mutex::new(HashSet::new()),
            progress_changed: Notify::new(),
            progress_seen: AtomicBool::new(false),
            quiescent: Mutex::new(None),
            work_ended: Mutex::new(None),
            sent_at: Mutex::new(None),
            slowest_report: Mutex::new(Duration::ZERO),
            pull_works: Mutex::new(None),
            started: Instant::now(),
            open: tokio::sync::Mutex::new(HashMap::new()),
            alive: AtomicBool::new(true),
            stderr: Mutex::new(VecDeque::new()),
            capabilities: Mutex::new(Value::Null),
        });
        tokio::spawn(read_loop(Arc::downgrade(&client), stdout));
        {
            let weak = Arc::downgrade(&client);
            tokio::spawn(async move {
                while let Some(frame) = frames.recv().await {
                    if stdin.write_all(&frame).await.is_err() || stdin.flush().await.is_err() {
                        if let Some(c) = weak.upgrade() {
                            c.alive.store(false, Ordering::Release);
                        }
                        break;
                    }
                }
            });
        }
        if let Some(stderr) = stderr {
            let weak = Arc::downgrade(&client);
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let Some(client) = weak.upgrade() else { break };
                    let mut tail = client.stderr.lock().unwrap();
                    tail.push_back(line);
                    if tail.len() > 30 {
                        tail.pop_front();
                    }
                }
            });
        }
        let root_uri = uri_of(root);
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let result = client
            .request(
                "initialize",
                json!({
                    "processId": std::process::id(),
                    "clientInfo": { "name": "evo", "version": env!("CARGO_PKG_VERSION") },
                    "rootUri": root_uri,
                    "rootPath": root,
                    "workspaceFolders": [{ "uri": root_uri, "name": name }],
                    "initializationOptions": init_options,
                    "capabilities": {
                        "general": { "positionEncodings": ["utf-16"] },
                        "textDocument": {
                            "synchronization": { "didSave": true, "dynamicRegistration": false },
                            "publishDiagnostics": { "relatedInformation": false, "versionSupport": true },
                            "definition": { "linkSupport": true },
                            "typeDefinition": { "linkSupport": true },
                            "implementation": { "linkSupport": true },
                            "references": {},
                            "hover": { "contentFormat": ["markdown", "plaintext"] },
                            "documentSymbol": { "hierarchicalDocumentSymbolSupport": true }
                        },
                        "workspace": {
                            "configuration": true,
                            "workspaceFolders": true,
                            "symbol": {}
                        },
                        "window": { "workDoneProgress": true },
                        "experimental": { "serverStatusNotification": true }
                    }
                }),
                Duration::from_secs(90),
            )
            .await
            .map_err(|e| format!("{} did not initialize: {e}", server.name))?;
        *client.capabilities.lock().unwrap() =
            result.get("capabilities").cloned().unwrap_or(Value::Null);
        client.notify("initialized", json!({})).await?;
        Ok(client)
    }

    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// The server's last stderr lines, for error messages.
    pub fn stderr_tail(&self) -> String {
        self.stderr
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn write(&self, message: &Value) -> Result<(), String> {
        let body = serde_json::to_string(message).map_err(|e| e.to_string())?;
        let frame = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        self.writer
            .send(frame.into_bytes())
            .map_err(|_| format!("{} stopped", self.server.name))
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.write(&json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    pub async fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        if !self.alive() {
            return Err(format!("{} is not running", self.server.name));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        if let Err(e) =
            self.write(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
        {
            self.pending.lock().unwrap().remove(&id);
            return Err(e);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(format!(
                "{} stopped while answering {method}",
                self.server.name
            )),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                let _ = self.notify("$/cancelRequest", json!({ "id": id })).await;
                Err(format!(
                    "{} did not answer {method} within {}s",
                    self.server.name,
                    timeout.as_secs()
                ))
            }
        }
    }

    pub async fn request_default(&self, method: &str, params: Value) -> Result<Value, String> {
        self.request(method, params, REQUEST_TIMEOUT).await
    }

    /// Open `path` (or send its new text when it changed). Returns its uri and
    /// the version sent.
    pub async fn sync_file(&self, path: &Path) -> Result<(String, i64), String> {
        let text = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let uri = uri_of(path);
        let mut open = self.open.lock().await;
        match open.get_mut(&uri) {
            Some(doc) if doc.text == text => Ok((uri, doc.version)),
            Some(doc) => {
                doc.version += 1;
                doc.text = text.clone();
                let version = doc.version;
                drop(open);
                *self.sent_at.lock().unwrap() = Some(Instant::now());
                self.notify(
                    "textDocument/didChange",
                    json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": [{ "text": text }] }),
                )
                .await?;
                self.notify(
                    "textDocument/didSave",
                    json!({ "textDocument": { "uri": uri } }),
                )
                .await?;
                Ok((uri, version))
            }
            None => {
                let language = self
                    .server
                    .language_id(&extension_of(path))
                    .unwrap_or("plaintext");
                open.insert(
                    uri.clone(),
                    Open {
                        version: 1,
                        text: text.clone(),
                    },
                );
                drop(open);
                *self.sent_at.lock().unwrap() = Some(Instant::now());
                self.notify(
                    "textDocument/didOpen",
                    json!({ "textDocument": { "uri": uri, "languageId": language, "version": 1, "text": text } }),
                )
                .await?;
                // Servers that check on save (rust-analyzer runs `cargo check`)
                // report their full diagnostics after a save.
                self.notify(
                    "textDocument/didSave",
                    json!({ "textDocument": { "uri": uri } }),
                )
                .await?;
                Ok((uri, 1))
            }
        }
    }

    /// Open `path` in the server when it is not open yet, without saving it
    /// (a save would set off rust-analyzer's `cargo check` on every read).
    pub async fn open_file(&self, path: &Path) -> Result<(), String> {
        if self.open.lock().await.contains_key(&uri_of(path)) {
            return Ok(());
        }
        let text = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let uri = uri_of(path);
        let language = self
            .server
            .language_id(&extension_of(path))
            .unwrap_or("plaintext");
        {
            let mut open = self.open.lock().await;
            if open.contains_key(&uri) {
                return Ok(());
            }
            open.insert(
                uri.clone(),
                Open {
                    version: 1,
                    text: text.clone(),
                },
            );
        }
        *self.sent_at.lock().unwrap() = Some(Instant::now());
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": language, "version": 1, "text": text } }),
        )
        .await
    }

    /// The text last sent for `uri`.
    pub async fn text_of(&self, uri: &str) -> Option<String> {
        self.open.lock().await.get(uri).map(|d| d.text.clone())
    }

    pub fn indexing(&self) -> bool {
        !self.progress.lock().unwrap().is_empty() || *self.quiescent.lock().unwrap() == Some(false)
    }

    /// Whether the server is still in its first start (its first half
    /// minute): servers report many small pieces of work while they load,
    /// so a time is the plainer sign.
    pub fn starting(&self) -> bool {
        self.started.elapsed() < Duration::from_secs(30)
    }

    /// Whether the server finished some work after `at` and is idle now: what
    /// it published since then is all it has to say about a change sent at
    /// `at` (rust-analyzer does not republish a file whose diagnostics stayed
    /// the same).
    pub fn work_finished_since(&self, at: Instant) -> bool {
        !self.indexing() && self.work_ended.lock().unwrap().is_some_and(|t| t >= at)
    }

    /// Wait until no work-done progress is running (indexing), up to `max`.
    /// Just after start, it first gives the server a moment to begin: a
    /// request sent before indexing starts would find nothing.
    pub async fn wait_idle(&self, max: Duration) -> bool {
        let deadline = Instant::now() + max;
        const WARMUP: Duration = Duration::from_secs(5);
        while !self.progress_seen.load(Ordering::Acquire)
            && self.quiescent.lock().unwrap().is_none()
            && self.started.elapsed() < WARMUP
            && self.alive()
        {
            let _ =
                tokio::time::timeout(Duration::from_millis(200), self.progress_changed.notified())
                    .await;
        }
        loop {
            if !self.indexing() {
                return true;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            let _ = tokio::time::timeout(
                left.min(Duration::from_millis(500)),
                self.progress_changed.notified(),
            )
            .await;
        }
    }

    /// Diagnostics for `uri` at `version` (or newer), waiting up to `max`;
    /// `None` when none arrived in time.
    pub async fn diagnostics_for(
        &self,
        uri: &str,
        version: i64,
        sent_at: Instant,
        max: Duration,
    ) -> Option<Vec<Value>> {
        let deadline = Instant::now() + max;
        loop {
            if let Some((v, list, at, _)) = self.diagnostics.lock().unwrap().get(&uri_key(uri)) {
                let fresh = match v {
                    Some(v) => *v >= version,
                    None => *at >= sent_at,
                };
                if fresh {
                    return Some(list.clone());
                }
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            let _ = tokio::time::timeout(
                left.min(Duration::from_millis(300)),
                self.diagnostics_changed.notified(),
            )
            .await;
        }
    }

    /// The last diagnostics the server reported for `uri`, without waiting;
    /// `None` when it has not reported on that file.
    pub fn last_diagnostics(&self, uri: &str) -> Option<Vec<Value>> {
        self.diagnostics
            .lock()
            .unwrap()
            .get(&uri_key(uri))
            .map(|(_, list, _, _)| list.clone())
    }

    /// Keep a report for `uri` (pushed or pulled), and how long it took
    /// after the last text sent.
    fn store_report(&self, uri: &str, version: Option<i64>, list: Vec<Value>) {
        if let Some(sent) = self.sent_at.lock().unwrap().take() {
            let mut slowest = self.slowest_report.lock().unwrap();
            *slowest = (*slowest).max(sent.elapsed()).min(Duration::from_secs(8));
        }
        self.diagnostics.lock().unwrap().insert(
            uri_key(uri),
            (version, list, Instant::now(), uri.to_string()),
        );
        self.diagnostics_changed.notify_waiters();
    }

    /// The longest this server has taken to report after a text was sent.
    pub fn slowest_report(&self) -> Duration {
        *self.slowest_report.lock().unwrap()
    }

    /// Whether the server answers diagnostics only when asked (pull) as far
    /// as known: it offers `diagnosticProvider` and has not refused it.
    pub fn can_pull(&self) -> bool {
        let offers = self
            .capabilities
            .lock()
            .unwrap()
            .get("diagnosticProvider")
            .is_some_and(|p| !p.is_null());
        offers && *self.pull_works.lock().unwrap() != Some(false)
    }

    /// Ask the server for `path`'s diagnostics (pull), for the text last
    /// sent, and keep them as its report. `None` when it does not answer in
    /// `max` or does not handle pulls (then it is not asked again).
    pub async fn pull(&self, path: &Path, max: Duration) -> Option<Vec<Value>> {
        if !self.can_pull() {
            return None;
        }
        let uri = uri_of(path);
        let version = self.open.lock().await.get(&uri).map(|d| d.version)?;
        let answer = self
            .request(
                "textDocument/diagnostic",
                json!({ "textDocument": { "uri": uri } }),
                max,
            )
            .await;
        match answer {
            Ok(answer) => {
                *self.pull_works.lock().unwrap() = Some(true);
                let list = match answer.get("kind").and_then(Value::as_str) {
                    Some("unchanged") => self.last_diagnostics(&uri).unwrap_or_default(),
                    _ => answer
                        .get("items")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                };
                self.store_report(&uri, Some(version), list.clone());
                Some(list)
            }
            Err(e) => {
                // A server that offers pulls only to clients that say they
                // take them (the CSS server) refuses the method.
                if e.contains("Unhandled method") || e.contains("not found") {
                    *self.pull_works.lock().unwrap() = Some(false);
                }
                None
            }
        }
    }

    /// When the server last published diagnostics for any file.
    pub fn last_report_at(&self) -> Option<Instant> {
        self.diagnostics
            .lock()
            .unwrap()
            .values()
            .map(|(_, _, at, _)| *at)
            .max()
    }

    /// The last report for every file the server reported on, empty ones
    /// included, keyed by [`uri_key`]: `(uri as the server spelled it, list)`.
    pub fn reports(&self) -> HashMap<String, (String, Vec<Value>)> {
        self.diagnostics
            .lock()
            .unwrap()
            .iter()
            .map(|(key, (_, list, _, uri))| (key.clone(), (uri.clone(), list.clone())))
            .collect()
    }

    /// The last diagnostics for every file the server reported on.
    pub fn all_diagnostics(&self) -> Vec<(String, Vec<Value>)> {
        self.diagnostics
            .lock()
            .unwrap()
            .values()
            .filter(|(_, list, _, _)| !list.is_empty())
            .map(|(_, list, _, uri)| (uri.clone(), list.clone()))
            .collect()
    }

    pub async fn shutdown(&self) {
        if self.alive() {
            let _ = self
                .request("shutdown", Value::Null, Duration::from_secs(3))
                .await;
            let _ = self.notify("exit", Value::Null).await;
        }
        self.alive.store(false, Ordering::Release);
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.start_kill();
        }
    }

    fn handle(&self, message: Value) {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_string);
        let id = message.get("id").cloned();
        match (method, id) {
            // A response to one of our requests.
            (None, Some(id)) => {
                let Some(id) = id.as_i64() else { return };
                if let Some(tx) = self.pending.lock().unwrap().remove(&id) {
                    let result = match message.get("error") {
                        Some(error) => Err(error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("error")
                            .to_string()),
                        None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                    };
                    let _ = tx.send(result);
                }
            }
            // The server asks us something: answer with defaults.
            (Some(method), Some(id)) => {
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                let result = match method.as_str() {
                    "workspace/configuration" => {
                        let n = params
                            .get("items")
                            .and_then(Value::as_array)
                            .map_or(1, Vec::len);
                        // Defaults for every server, but the CSS server
                        // validates nothing with `null` settings and the HTML
                        // server stops checking scripts with `{}`.
                        let settings = if self.server.id == "css" {
                            json!({})
                        } else {
                            Value::Null
                        };
                        Value::Array(vec![settings; n])
                    }
                    "workspace/workspaceFolders" => json!([{
                        "uri": uri_of(&self.root),
                        "name": self.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                    }]),
                    _ => Value::Null,
                };
                let _ = self.write(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
            }
            // A notification.
            (Some(method), None) => {
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                match method.as_str() {
                    "textDocument/publishDiagnostics" => {
                        let uri = params
                            .get("uri")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        let version = params.get("version").and_then(Value::as_i64);
                        let list = params
                            .get("diagnostics")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default();
                        self.store_report(&uri, version, list);
                    }
                    "$/progress" => {
                        let token = params
                            .get("token")
                            .map(|t| t.to_string())
                            .unwrap_or_default();
                        match params.pointer("/value/kind").and_then(Value::as_str) {
                            Some("begin") => {
                                self.progress.lock().unwrap().insert(token);
                                self.progress_seen.store(true, Ordering::Release);
                                self.progress_changed.notify_waiters();
                            }
                            Some("end") => {
                                self.progress.lock().unwrap().remove(&token);
                                *self.work_ended.lock().unwrap() = Some(Instant::now());
                                self.progress_changed.notify_waiters();
                            }
                            _ => {}
                        }
                    }
                    // Kept with stderr: many servers log only this way, and
                    // it tells why one does not work.
                    "window/logMessage" | "window/showMessage" => {
                        if let Some(text) = params.get("message").and_then(Value::as_str) {
                            let mut tail = self.stderr.lock().unwrap();
                            tail.extend(text.lines().map(|l| format!("[log] {l}")));
                            while tail.len() > 30 {
                                tail.pop_front();
                            }
                        }
                    }
                    // rust-analyzer says when it has nothing left to do.
                    "experimental/serverStatus" => {
                        let quiescent = params.get("quiescent").and_then(Value::as_bool);
                        *self.quiescent.lock().unwrap() = quiescent;
                        if quiescent == Some(true) {
                            *self.work_ended.lock().unwrap() = Some(Instant::now());
                        }
                        self.progress_changed.notify_waiters();
                    }
                    _ => {}
                }
            }
            (None, None) => {}
        }
    }
}

async fn read_loop(client: std::sync::Weak<Client>, stdout: tokio::process::ChildStdout) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut length: Option<usize> = None;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) | Err(_) => {
                    if let Some(c) = client.upgrade() {
                        c.alive.store(false, Ordering::Release);
                        for (_, tx) in c.pending.lock().unwrap().drain() {
                            let _ = tx.send(Err(format!("{} stopped", c.server.name)));
                        }
                        c.progress.lock().unwrap().clear();
                        c.progress_changed.notify_waiters();
                    }
                    return;
                }
                Ok(_) => {}
            }
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length:") {
                length = value.trim().parse().ok();
            }
        }
        let Some(length) = length else { continue };
        let mut body = vec![0u8; length];
        if reader.read_exact(&mut body).await.is_err() {
            continue;
        }
        let Ok(message) = serde_json::from_slice::<Value>(&body) else {
            continue;
        };
        let Some(c) = client.upgrade() else { return };
        c.handle(message);
    }
}
