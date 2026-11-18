//! Evo Webbridge in the shell: the person's own Chrome or Edge, reached
//! through the Evo extension and `evo-bridge-host`
//! (docs/features/browser-bridge/spec.md; plan 3.2, ADR 0012).
//!
//! `server.rs` serves the per-user pipe and checks who connects; each
//! connection is one browser profile. This module keeps the connected
//! profiles, sends requests and matches their results, and routes the other
//! messages to the parts that own them. `setup.rs` registers the host with the
//! browsers; `ids.rs` says which extensions may connect.

pub mod commands;
pub mod ids;
mod server;
pub mod setup;
pub mod wire;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, oneshot};

use crate::db::Db;

/// App setting: Webbridge was set up, so the pipe is served (NFR-3).
pub const ENABLED_KEY: &str = "webbridge.enabled";
/// Event: something in [`commands::Status`] changed.
pub const CHANGED: &str = "webbridge://changed";
/// How long a request to the extension may take.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Evo's Webbridge folder in the agent dir.
pub fn dir() -> PathBuf {
    crate::mcp::agent_dir().join("webbridge")
}

/// A connected browser profile.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// `<browser>:<profile id>`, the key everything else uses.
    pub key: String,
    pub browser: String,
    /// The extension's id for this profile (random, kept in its storage).
    pub profile: String,
    pub label: String,
    pub version: String,
    /// The browser lets the extension use its debugging API (FR-23).
    pub debugger: bool,
}

struct Connection {
    sender: mpsc::UnboundedSender<Vec<u8>>,
    /// The host introduced an allowed extension.
    origin_ok: bool,
    profile: Option<Profile>,
}

type Reply = oneshot::Sender<Result<Value, String>>;

#[derive(Default)]
pub struct Webbridge {
    connections: Mutex<HashMap<u64, Connection>>,
    next_connection: AtomicU64,
    /// Request id → (connection, reply).
    pending: Mutex<HashMap<u64, (u64, Reply)>>,
    next_request: AtomicU64,
    running: AtomicBool,
    /// Why the last connection was refused, for Settings.
    refused: Mutex<Option<String>>,
}

impl Webbridge {
    /// The connected profiles.
    pub fn profiles(&self) -> Vec<Profile> {
        let connections = self.connections.lock().expect("webbridge lock");
        let mut list: Vec<Profile> = connections
            .values()
            .filter_map(|c| c.profile.clone())
            .collect();
        list.sort_by(|a, b| a.key.cmp(&b.key));
        list
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn refused(&self) -> Option<String> {
        self.refused.lock().expect("webbridge lock").clone()
    }

    fn connection_of(&self, profile_key: &str) -> Option<u64> {
        let connections = self.connections.lock().expect("webbridge lock");
        connections
            .iter()
            .find(|(_, c)| c.profile.as_ref().is_some_and(|p| p.key == profile_key))
            .map(|(id, _)| *id)
    }

    /// Send a message to a connection (split into chunks when large).
    fn send_to(&self, connection: u64, message: &Value) -> Result<(), String> {
        let connections = self.connections.lock().expect("webbridge lock");
        let connection = connections
            .get(&connection)
            .ok_or("the browser went away")?;
        for frame in wire::outgoing(
            message,
            self.next_request.fetch_add(1, Ordering::SeqCst) + 1,
        ) {
            connection
                .sender
                .send(frame)
                .map_err(|_| "the browser went away".to_string())?;
        }
        Ok(())
    }

    /// Send a message to a profile without waiting for an answer.
    pub fn notify(&self, profile_key: &str, message: Value) -> Result<(), String> {
        let connection = self
            .connection_of(profile_key)
            .ok_or("the browser is not connected to Evo")?;
        self.send_to(connection, &message)
    }

    /// Send a request to a profile and wait for its `result`.
    pub async fn request(
        &self,
        profile_key: &str,
        mut message: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let connection = self
            .connection_of(profile_key)
            .ok_or("the browser is not connected to Evo")?;
        let id = self.next_request.fetch_add(1, Ordering::SeqCst) + 1;
        message["id"] = json!(id);
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .expect("webbridge lock")
            .insert(id, (connection, tx));
        if let Err(error) = self.send_to(connection, &message) {
            self.pending.lock().expect("webbridge lock").remove(&id);
            return Err(error);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("the browser went away".into()),
            Err(_) => {
                self.pending.lock().expect("webbridge lock").remove(&id);
                Err(format!(
                    "the browser did not answer within {}s",
                    timeout.as_secs()
                ))
            }
        }
    }

    fn add_connection(&self, sender: mpsc::UnboundedSender<Vec<u8>>) -> u64 {
        let id = self.next_connection.fetch_add(1, Ordering::SeqCst) + 1;
        self.connections.lock().expect("webbridge lock").insert(
            id,
            Connection {
                sender,
                origin_ok: false,
                profile: None,
            },
        );
        id
    }

    /// A connection ended: its pending requests fail; its profile is gone.
    fn remove_connection(&self, id: u64) -> Option<Profile> {
        let removed = self.connections.lock().expect("webbridge lock").remove(&id);
        let mut pending = self.pending.lock().expect("webbridge lock");
        let ids: Vec<u64> = pending
            .iter()
            .filter(|(_, (c, _))| *c == id)
            .map(|(r, _)| *r)
            .collect();
        for request in ids {
            if let Some((_, reply)) = pending.remove(&request) {
                let _ = reply.send(Err("the browser went away".into()));
            }
        }
        removed.and_then(|c| c.profile)
    }
}

fn log(app: &AppHandle, level: &str, message: impl Into<String>) {
    app.state::<Db>()
        .log(evo_store::logs::NewLog::new(level, "webbridge", message));
}

fn changed(app: &AppHandle) {
    let _ = app.emit(CHANGED, json!({}));
}

/// Whether Webbridge was set up (the pipe is served only then).
pub fn enabled(app: &AppHandle) -> bool {
    app.state::<Db>()
        .with(|conn| evo_store::settings::get(conn, ENABLED_KEY))
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// At app start: serve the pipe when Webbridge was set up.
pub fn setup(app: &AppHandle) {
    if enabled(app) {
        server::start(app);
    }
}

/// A profile's connection ended: what belonged to it ends too (FR-20).
fn profile_gone(_app: &AppHandle, _profile_key: &str) {}

/// A frame arrived on `connection`.
fn on_frame(app: &AppHandle, connection: u64, frame: &[u8]) -> Result<(), String> {
    let message: Value = serde_json::from_slice(frame).map_err(|e| format!("not JSON: {e}"))?;
    let state = app.state::<Webbridge>();
    let kind = wire::kind(&message);
    // The host's introduction comes first; nothing else counts before it.
    let origin_ok = state
        .connections
        .lock()
        .expect("webbridge lock")
        .get(&connection)
        .is_some_and(|c| c.origin_ok);
    if !origin_ok {
        admit(&message)?;
        if let Some(c) = state
            .connections
            .lock()
            .expect("webbridge lock")
            .get_mut(&connection)
        {
            c.origin_ok = true;
        }
        return Ok(());
    }
    match kind {
        "hello" => on_hello(app, connection, &message),
        "result" => {
            let id = message["id"].as_u64().unwrap_or(0);
            if let Some((_, reply)) = state.pending.lock().expect("webbridge lock").remove(&id) {
                let result = if message["ok"].as_bool() == Some(true) {
                    Ok(message.get("value").cloned().unwrap_or(Value::Null))
                } else {
                    Err(error_text(&message))
                };
                let _ = reply.send(result);
            }
            Ok(())
        }
        // Messages of later versions are ignored (the wire's compatibility rule).
        _ => Ok(()),
    }
}

/// The words an agent reads for a failed request.
pub fn error_text(message: &Value) -> String {
    let detail = message["message"].as_str().unwrap_or("");
    match message["code"].as_str().unwrap_or("failed") {
        "gone" => "The tab is gone.".to_string(),
        "restricted" => "The browser does not let extensions use this page.".to_string(),
        "busy" => {
            format!("The browser refused: another debugger is attached to this tab. {detail}")
                .trim()
                .to_string()
        }
        "refused" => format!("The browser refused: {detail}"),
        _ => {
            if detail.is_empty() {
                "The browser could not do that.".to_string()
            } else {
                detail.to_string()
            }
        }
    }
}

/// The host's introduction: the first frame must name one of Evo's extensions.
fn admit(message: &Value) -> Result<(), String> {
    if wire::kind(message) != "host" {
        return Err("the host did not introduce the extension".into());
    }
    let origin = message["origin"].as_str().unwrap_or("");
    if !ids::is_allowed_origin(origin) {
        return Err(format!(
            "an extension that is not Evo's tried to connect: {origin}"
        ));
    }
    Ok(())
}

/// The profile a `hello` describes.
fn parse_hello(message: &Value) -> Result<Profile, String> {
    let browser = message["browser"]
        .as_str()
        .filter(|b| setup::Browser::parse(b).is_some())
        .ok_or("hello names no supported browser")?;
    let profile = message["profile"]
        .as_str()
        .filter(|p| {
            !p.is_empty()
                && p.len() <= 40
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        .ok_or("hello has no valid profile id")?;
    let label = message["label"]
        .as_str()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .unwrap_or("Default");
    Ok(Profile {
        key: format!("{browser}:{profile}"),
        browser: browser.to_string(),
        profile: profile.to_string(),
        label: label.chars().take(40).collect(),
        version: message["version"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(20)
            .collect(),
        debugger: message["debugger"].as_bool().unwrap_or(true),
    })
}

fn on_hello(app: &AppHandle, connection: u64, message: &Value) -> Result<(), String> {
    let state = app.state::<Webbridge>();
    let mut profile = parse_hello(message)?;
    if let Some(label) = commands::label_of(&profile.key) {
        profile.label = label;
    }
    let key = profile.key.clone();
    let browser = profile.browser.clone();
    let browser = browser.as_str();
    // A profile that reconnects replaces its older connection.
    let older = {
        let mut connections = state.connections.lock().expect("webbridge lock");
        let older: Vec<u64> = connections
            .iter()
            .filter(|(id, c)| {
                **id != connection && c.profile.as_ref().is_some_and(|p| p.key == key)
            })
            .map(|(id, _)| *id)
            .collect();
        if let Some(c) = connections.get_mut(&connection) {
            c.profile = Some(profile.clone());
        }
        older
    };
    for id in older {
        state.remove_connection(id);
    }
    state.send_to(connection, &json!({ "t": "welcome", "v": wire::VERSION, "app": env!("CARGO_PKG_VERSION"), "label": profile.label }))?;
    commands::remember(&profile);
    log(
        app,
        "info",
        format!(
            "{} profile \"{}\" connected (extension {})",
            setup::Browser::parse(browser).map_or(browser, |b| b.label()),
            profile.label,
            profile.version
        ),
    );
    if !profile.debugger {
        log(
            app,
            "warn",
            format!(
                "{} does not allow the debugging API (administrator policy)",
                profile.key
            ),
        );
    }
    changed(app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_introduction_of_an_evo_extension_is_admitted() {
        let evo = json!({ "t": "host", "origin": format!("chrome-extension://{}/", ids::UNPACKED_ID), "pid": 1 });
        assert!(admit(&evo).is_ok());
        assert!(admit(&json!({ "t": "host", "origin": "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/" })).is_err());
        // An extension cannot skip the host's introduction by speaking first.
        assert!(admit(&json!({ "t": "hello", "browser": "chrome", "profile": "p1" })).is_err());
    }

    #[test]
    fn a_hello_names_a_supported_browser_and_a_plain_profile_id() {
        let profile = parse_hello(&json!({ "t": "hello", "browser": "edge", "profile": "p85ee319123b3", "label": " Work ", "version": "0.1.0", "debugger": false })).unwrap();
        assert_eq!(
            (
                profile.key.as_str(),
                profile.label.as_str(),
                profile.debugger
            ),
            ("edge:p85ee319123b3", "Work", false)
        );
        assert_eq!(
            parse_hello(&json!({ "browser": "chrome", "profile": "p1" }))
                .unwrap()
                .label,
            "Default"
        );
        assert!(parse_hello(&json!({ "browser": "firefox", "profile": "p1" })).is_err());
        assert!(parse_hello(&json!({ "browser": "chrome", "profile": "../../x" })).is_err());
        assert!(parse_hello(&json!({ "browser": "chrome" })).is_err());
    }

    #[test]
    fn failed_requests_read_as_plain_words() {
        assert_eq!(error_text(&json!({ "code": "gone" })), "The tab is gone.");
        assert!(error_text(&json!({ "code": "busy", "message": "x" }))
            .starts_with("The browser refused: another debugger"));
        assert_eq!(
            error_text(&json!({ "code": "restricted" })),
            "The browser does not let extensions use this page."
        );
    }
}
