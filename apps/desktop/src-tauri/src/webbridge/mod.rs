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
pub mod groups;
pub mod ids;
pub mod page;
pub mod panel;
mod server;
pub mod setup;
pub mod sites;
pub mod wire;

use std::collections::{HashMap, HashSet};
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
/// App setting: the workspace new side panel chats start in (`null`: the last used one).
pub const NEW_CHAT_WORKSPACE_KEY: &str = "webbridge.newChatWorkspace";
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
    /// Leases on the debugging API and takeovers (`groups.rs`).
    holds: Mutex<groups::Holds>,
    /// Tabs attached with the page helper added (`page.rs`).
    prepared: Mutex<HashSet<String>>,
    /// Side panel sessions (`panel.rs`).
    panels: panel::Panels,
    /// Why recent tabs went (plan 3.6), for the agent's next call on them.
    pub(crate) gone: Mutex<groups::Gone>,
    /// Chat → the profile whose side panel last sent to it: that chat's pages open there by default.
    pub(crate) panel_browser: Mutex<HashMap<String, String>>,
    /// Tabs the person shared with a chat (Include this tab, choose): the agent reads and acts
    /// in them, but a navigation that names no tab opens a new one instead of taking them away.
    pub(crate) shared: Mutex<HashSet<String>>,
    /// Tab → where the agent last put the mouse and when, so a new document of the
    /// tab (a click that opened a page) shows the virtual cursor where it was.
    pub(crate) pointers: Mutex<HashMap<String, (f64, f64, std::time::Instant)>>,
}

/// Whether the person shared `tab` with its chat (it was not opened by the agent).
pub fn is_shared(app: &AppHandle, tab: &str) -> bool {
    app.state::<Webbridge>()
        .shared
        .lock()
        .expect("webbridge lock")
        .contains(tab)
}

/// Why a Webbridge tab the agent names is no longer there, if it went lately.
pub fn gone_reason(app: &AppHandle, tab: &str) -> Option<&'static str> {
    app.state::<Webbridge>()
        .gone
        .lock()
        .expect("webbridge lock")
        .reason(tab)
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

    fn profile_of(&self, connection: u64) -> Option<String> {
        self.connections
            .lock()
            .expect("webbridge lock")
            .get(&connection)
            .and_then(|c| c.profile.as_ref().map(|p| p.key.clone()))
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

/// One agent call on a Webbridge tab, for the logs (FR-24): the chat, the tool and
/// the site, never the page's content or what was typed.
pub fn log_act(app: &AppHandle, chat: &str, tool: &str, url: &str, outcome: &str) {
    let site = sites::site_of(url);
    let site = if site.is_empty() {
        "no page"
    } else {
        site.as_str()
    };
    log(
        app,
        "info",
        format!("chat {chat}: {tool} on {site} ({outcome})"),
    );
}

pub(crate) fn log(app: &AppHandle, level: &str, message: impl Into<String>) {
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

/// App setting: the browser the agent's browser tools use when a call names none:
/// `builtin` (the default) or a connected profile's key (`edge:p85ee…`).
pub const DEFAULT_BROWSER_KEY: &str = "webbridge.defaultBrowser";

/// The browser profile a tool's `target` names: `None` for the built-in
/// browser (`builtin`), else the key of a connected profile. No target means
/// the browser whose side panel `chat` was last sent from, else the default
/// browser the person picked in Settings → Browser.
pub fn resolve_target(
    app: &AppHandle,
    chat: &str,
    target: Option<&str>,
) -> Result<Option<String>, String> {
    let state = app.state::<Webbridge>();
    let profiles = state.profiles();
    let from_panel = state
        .panel_browser
        .lock()
        .expect("webbridge lock")
        .get(chat)
        .filter(|key| profiles.iter().any(|p| &p.key == *key))
        .cloned();
    let default = from_panel.or_else(|| {
        app.state::<Db>()
            .with(|conn| evo_store::settings::get(conn, DEFAULT_BROWSER_KEY))
            .ok()
            .flatten()
            .and_then(|v| v.as_str().map(str::to_string))
    });
    pick_profile(&profiles, target, default.as_deref())
}

fn pick_profile(
    profiles: &[Profile],
    target: Option<&str>,
    default: Option<&str>,
) -> Result<Option<String>, String> {
    // The person's default: a connected profile the browser lets Evo use, else the built-in browser.
    let default = default.and_then(|key| profiles.iter().find(|p| p.key == key && p.debugger));
    let target = match target.map(str::trim).filter(|t| !t.is_empty()) {
        None => return Ok(default.map(|p| p.key.clone())),
        Some("builtin") => return Ok(None),
        Some(target) => target.to_lowercase(),
    };
    let (browser, name) = match target.split_once(':') {
        Some((browser, name)) => (browser.to_string(), Some(name.to_string())),
        None => (target.clone(), None),
    };
    let label = setup::Browser::parse(&browser)
        .map(|b| b.label())
        .ok_or_else(|| format!("target must be builtin, chrome or edge (got {target})."))?;
    let of_browser: Vec<&Profile> = profiles.iter().filter(|p| p.browser == browser).collect();
    if of_browser.is_empty() {
        return Err(format!(
            "{label} is not connected to Evo. The person can set it up in Settings → Browser."
        ));
    }
    let matched: Vec<&Profile> = match &name {
        Some(name) => of_browser
            .iter()
            .copied()
            .filter(|p| p.profile.to_lowercase() == *name || p.label.to_lowercase() == *name)
            .collect(),
        None => match default.filter(|d| d.browser == browser) {
            // Several profiles of this browser: the default one, when it is one of them.
            Some(d) => vec![d],
            None => of_browser.clone(),
        },
    };
    match matched.as_slice() {
        [one] => {
            if !one.debugger {
                return Err(format!("{label}'s administrator turned off the debugging API, so Evo cannot work in it."));
            }
            Ok(Some(one.key.clone()))
        }
        [] => Err(format!(
            "No {label} profile is called {}. Connected: {}.",
            name.unwrap_or_default(),
            of_browser
                .iter()
                .map(|p| format!("{browser}:{}", p.label))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        _ => Err(format!(
            "{label} has several profiles connected; name one: {}.",
            of_browser
                .iter()
                .map(|p| format!("{browser}:{}", p.label))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// One of the person's open tabs outside Evo's groups: title and site only (FR-9).
pub struct OpenTab {
    pub tab: i64,
    pub title: String,
    pub site: String,
}

pub async fn list_tabs(app: &AppHandle, profile_key: &str) -> Result<Vec<OpenTab>, String> {
    let reply = app
        .state::<Webbridge>()
        .request(profile_key, json!({ "t": "list" }), REQUEST_TIMEOUT)
        .await?;
    Ok(reply
        .as_array()
        .map(|tabs| {
            tabs.iter()
                .filter_map(|t| {
                    Some(OpenTab {
                        tab: t["tab"].as_i64()?,
                        title: t["title"].as_str().unwrap_or("").chars().take(80).collect(),
                        site: t["site"].as_str().unwrap_or("").to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Hand one of the person's tabs to `chat` (it joins the chat's group).
pub async fn adopt(
    app: &AppHandle,
    chat: &str,
    profile_key: &str,
    number: i64,
) -> Result<String, String> {
    let reply = app
        .state::<Webbridge>()
        .request(
            profile_key,
            json!({ "t": "adopt", "tab": number, "chat": chat, "title": chat_title(app, chat) }),
            REQUEST_TIMEOUT,
        )
        .await?;
    let id = groups::tab_id(profile_key, number);
    app.state::<Webbridge>()
        .shared
        .lock()
        .expect("webbridge lock")
        .insert(id.clone());
    crate::browser::register_external(
        app,
        &id,
        chat,
        reply["url"].as_str().unwrap_or(""),
        reply["title"].as_str().unwrap_or(""),
        reply["loading"].as_bool().unwrap_or(false),
    );
    crate::browser::select_tab_for_tool(app, chat, &id);
    Ok(id)
}

/// The site list changed: Settings and every connected extension learn it.
pub fn sites_changed(app: &AppHandle) {
    let state = app.state::<Webbridge>();
    let message = sites_message();
    for profile in state.profiles() {
        let _ = state.notify(&profile.key, message.clone());
    }
    changed(app);
}

/// Webbridge settings changed: every connected extension learns them (voice input, FR-29).
pub fn settings_changed(app: &AppHandle) {
    let state = app.state::<Webbridge>();
    let message = settings_message(app);
    for profile in state.profiles() {
        let _ = state.notify(&profile.key, message.clone());
    }
}

/// App setting: the side panel shows a microphone (FR-29).
pub const VOICE_INPUT_KEY: &str = "webbridge.voiceInput";

fn settings_message(app: &AppHandle) -> Value {
    let voice = app
        .state::<Db>()
        .with(|conn| evo_store::settings::get(conn, VOICE_INPUT_KEY))
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    json!({ "t": "settings", "voiceInput": voice })
}

fn sites_message() -> Value {
    json!({ "t": "sites", "sites": sites::all() })
}

/// Tell a profile's extension which chats work in it now (FR-18).
pub fn push_working(app: &AppHandle, profile_key: &str) {
    let prefix = format!("{profile_key}:");
    let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
    for tab in crate::browser::external_tabs(app)
        .into_iter()
        .filter(|t| t.starts_with(&prefix))
    {
        if let Some(chat) = crate::browser::tab_record(app, &tab).and_then(|r| r.chat) {
            *counts.entry(chat).or_default() += 1;
        }
    }
    let chats: Vec<Value> = counts
        .into_iter()
        .map(|(chat, tabs)| json!({ "chat": chat, "title": chat_title(app, &chat), "tabs": tabs }))
        .collect();
    let _ = app
        .state::<Webbridge>()
        .notify(profile_key, json!({ "t": "working", "chats": chats }));
}

/// A button of the in-page indicator, or the side panel (spec FR-16 – FR-18).
pub fn control(app: &AppHandle, tab: &str, action: &str) {
    let state = app.state::<Webbridge>();
    let chat = crate::browser::tab_record(app, tab).and_then(|t| t.chat);
    match action {
        "takeover" => {
            state
                .holds
                .lock()
                .expect("webbridge lock")
                .set_taken(tab, true);
            log(app, "info", format!("the person took over {tab}"));
        }
        "handback" => {
            state
                .holds
                .lock()
                .expect("webbridge lock")
                .set_taken(tab, false);
            log(app, "info", format!("the person handed {tab} back"));
        }
        "stop" => {
            if let Some(chat) = &chat {
                stop_chat(app, chat);
            }
        }
        _ => return,
    }
    page::show_overlay(app, tab);
    changed(app);
}

/// Stop a chat's turn, as the app's Stop does.
pub fn stop_chat(app: &AppHandle, chat: &str) {
    log(app, "info", format!("stopped chat {chat} from the browser"));
    let (app, chat) = (app.clone(), chat.to_string());
    tauri::async_runtime::spawn(async move {
        let id = format!("webbridge-stop-{}", now_ms());
        let _ = app
            .state::<crate::engine::EngineState>()
            .write(&chat, &json!({ "type": "abort", "id": id }))
            .await;
    });
}

/// Stop all in one browser profile: every chat working there stops, and the debugging API is
/// let go of on every tab (the browser's notices go) (FR-18).
pub fn stop_all(app: &AppHandle, profile_key: &str) {
    let prefix = format!("{profile_key}:");
    let tabs: Vec<String> = crate::browser::external_tabs(app)
        .into_iter()
        .filter(|t| t.starts_with(&prefix))
        .collect();
    let mut chats: Vec<String> = tabs
        .iter()
        .filter_map(|t| crate::browser::tab_record(app, t).and_then(|r| r.chat))
        .collect();
    chats.sort();
    chats.dedup();
    for chat in &chats {
        stop_chat(app, chat);
    }
    let state = app.state::<Webbridge>();
    for tab in state
        .holds
        .lock()
        .expect("webbridge lock")
        .release_profile(profile_key)
    {
        state.prepared.lock().expect("webbridge lock").remove(&tab);
    }
    for tab in &tabs {
        page::let_go(app, tab);
    }
    log(app, "info", format!("Stop all in {profile_key}"));
    changed(app);
}

/// A profile's connection ended: what belonged to it ends too (FR-20).
fn profile_gone(app: &AppHandle, profile_key: &str) {
    let prefix = format!("{profile_key}:");
    for tab in crate::browser::external_tabs(app) {
        if tab.starts_with(&prefix) {
            page::forget(app, &tab);
        }
    }
    app.state::<Webbridge>()
        .holds
        .lock()
        .expect("webbridge lock")
        .release_profile(profile_key);
    panel::profile_gone(app, profile_key);
}

/// The chat's title, for its tab group's name.
pub fn chat_title(app: &AppHandle, chat: &str) -> String {
    let title = app
        .state::<Db>()
        .with(|conn| evo_store::chats::get_chat(conn, chat))
        .ok()
        .flatten()
        .map(|c| c.title)
        .unwrap_or_default();
    let title = title.trim();
    if title.is_empty() {
        "New chat".to_string()
    } else {
        title.chars().take(40).collect()
    }
}

/// Whether an engine line ends a chat's turn: the run settled, or the prompt was refused
/// before it ran (no model, say), which sends no `agent_settled`.
fn ends_turn(message: &Value) -> bool {
    match message.get("type").and_then(Value::as_str) {
        Some("agent_settled") => true,
        Some("response") => {
            message.get("command").and_then(Value::as_str) == Some("prompt")
                && message.get("success") == Some(&Value::Bool(false))
        }
        _ => false,
    }
}

/// An engine line: a chat's turn ended, so its tabs are let go after the lease (spec 6.2).
pub fn on_engine_line(app: &AppHandle, chat: &str, message: &Value) {
    panel::on_engine_line(app, chat, message);
    if ends_turn(message) {
        turn_ended(app, chat);
    }
}

/// A chat's engine exited: whatever its turn was doing has ended too.
pub fn on_engine_exit(app: &AppHandle, chat: &str) {
    panel::engine_exited(app, chat);
    turn_ended(app, chat);
}

/// Start the lease on the chat's attached tabs.
fn turn_ended(app: &AppHandle, chat: &str) {
    let tabs: Vec<String> = crate::browser::chat_tabs(app, chat)
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| groups::is_tab(id))
        .collect();
    if tabs.is_empty() {
        return;
    }
    let lease = lease_ms(app);
    app.state::<Webbridge>()
        .holds
        .lock()
        .expect("webbridge lock")
        .turn_ended(tabs.iter().map(String::as_str), now_ms(), lease);
    log(
        app,
        "debug",
        format!(
            "chat {chat}'s turn ended: its tabs are let go in {} s",
            lease / 1000
        ),
    );
}

/// App setting: how long a chat's tabs keep the debugging API after its turn (plan 3.4).
pub const LEASE_SECONDS_KEY: &str = "webbridge.leaseSeconds";

fn lease_ms(app: &AppHandle) -> u64 {
    let seconds = app
        .state::<Db>()
        .with(|conn| evo_store::settings::get(conn, LEASE_SECONDS_KEY))
        .ok()
        .flatten()
        .and_then(|v| v.as_f64());
    groups::lease_ms(seconds)
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Let go of tabs whose lease ran out, every few seconds while the pipe is served.
fn start_leases(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            page::release_due(&app, now_ms());
        }
    });
}

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
        "control" => {
            if let Some(profile) = state.profile_of(connection) {
                match message["action"].as_str() {
                    Some("stop_all") => stop_all(app, &profile),
                    Some(action) => {
                        if let Some(number) = message["tab"].as_i64() {
                            control(app, &groups::tab_id(&profile, number), action);
                        }
                    }
                    None => {}
                }
            }
            Ok(())
        }
        "panel" => {
            if let Some(profile) = state.profile_of(connection) {
                panel::on_message(app, &profile, &message);
            }
            Ok(())
        }
        "site" => {
            let site = message["site"].as_str().unwrap_or("");
            if let Some(permission) = message["permission"]
                .as_str()
                .and_then(sites::Permission::parse)
            {
                sites::set(site, permission)?;
                log(
                    app,
                    "info",
                    format!("{site} set to {permission:?} from the browser"),
                );
                sites_changed(app);
            }
            Ok(())
        }
        "event" | "tab" | "detached" | "group" => {
            let Some(profile) = state.profile_of(connection) else {
                return Ok(());
            };
            match kind {
                "event" => page::on_event(app, &profile, &message),
                "tab" => page::on_tab(app, &profile, &message),
                "detached" => page::on_detached(app, &profile, &message),
                _ => {}
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
    state.send_to(connection, &sites_message())?;
    state.send_to(connection, &settings_message(app))?;
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
    fn a_turn_ends_when_it_settles_or_its_prompt_is_refused() {
        assert!(ends_turn(
            &json!({ "type": "agent_settled", "outcome": "completed" })
        ));
        assert!(ends_turn(
            &json!({ "type": "response", "command": "prompt", "success": false, "error": "no model" })
        ));
        assert!(!ends_turn(
            &json!({ "type": "response", "command": "prompt", "success": true })
        ));
        assert!(!ends_turn(
            &json!({ "type": "response", "command": "get_state", "success": false })
        ));
        assert!(!ends_turn(&json!({ "type": "agent_end" })));
    }

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

    fn profile(browser: &str, id: &str, label: &str) -> Profile {
        Profile {
            key: format!("{browser}:{id}"),
            browser: browser.into(),
            profile: id.into(),
            label: label.into(),
            version: "0.1.0".into(),
            debugger: true,
        }
    }

    #[test]
    fn no_target_uses_the_default_browser_when_it_is_connected() {
        let two = [
            profile("chrome", "p1", "Work"),
            profile("edge", "p2", "Home"),
        ];
        assert_eq!(
            pick_profile(&two, None, Some("edge:p2"))
                .unwrap()
                .as_deref(),
            Some("edge:p2")
        );
        assert_eq!(
            pick_profile(&two, Some("builtin"), Some("edge:p2")).unwrap(),
            None
        );
        assert_eq!(
            pick_profile(&two, Some("chrome"), Some("edge:p2"))
                .unwrap()
                .as_deref(),
            Some("chrome:p1")
        );
        // A default that is not connected falls back to the built-in browser.
        assert_eq!(pick_profile(&two, None, Some("edge:gone")).unwrap(), None);
        assert_eq!(pick_profile(&two, None, Some("builtin")).unwrap(), None);
        // Several profiles of one browser: the default one among them.
        let chromes = [
            profile("chrome", "p1", "Work"),
            profile("chrome", "p2", "Home"),
        ];
        assert_eq!(
            pick_profile(&chromes, Some("chrome"), Some("chrome:p2"))
                .unwrap()
                .as_deref(),
            Some("chrome:p2")
        );
        assert!(pick_profile(&chromes, Some("chrome"), None)
            .unwrap_err()
            .contains("several profiles"));
    }

    #[test]
    fn a_target_picks_the_one_connected_profile_or_asks_for_a_name() {
        let one = [profile("edge", "p1", "Work")];
        assert_eq!(pick_profile(&one, None, None).unwrap(), None);
        assert_eq!(pick_profile(&one, Some("builtin"), None).unwrap(), None);
        assert_eq!(
            pick_profile(&one, Some("edge"), None).unwrap().as_deref(),
            Some("edge:p1")
        );
        assert!(pick_profile(&one, Some("chrome"), None)
            .unwrap_err()
            .contains("not connected"));
        assert!(pick_profile(&one, Some("firefox"), None).is_err());
        let two = [
            profile("chrome", "p1", "Work"),
            profile("chrome", "p2", "Personal"),
        ];
        assert!(pick_profile(&two, Some("chrome"), None)
            .unwrap_err()
            .contains("several profiles"));
        assert_eq!(
            pick_profile(&two, Some("chrome:personal"), None)
                .unwrap()
                .as_deref(),
            Some("chrome:p2")
        );
        assert_eq!(
            pick_profile(&two, Some("chrome:p1"), None)
                .unwrap()
                .as_deref(),
            Some("chrome:p1")
        );
        let mut blocked = profile("edge", "p1", "Work");
        blocked.debugger = false;
        assert!(pick_profile(&[blocked], Some("edge"), None)
            .unwrap_err()
            .contains("administrator"));
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
