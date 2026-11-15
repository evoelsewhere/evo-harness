//! The plugin bridge: the calls a view makes with `POST /__bridge` on its
//! own origin, which reach the shell through the scheme handler with the
//! caller's webview label (docs/features/plugin-ui/spec.md, FR-10 – FR-13,
//! FR-16, NFR-1; ADR 0021).
//!
//! Every call is checked in this order:
//! 1. the label is an open view, and its plugin's host key is the origin it
//!    called (a view cannot speak for another plugin);
//! 2. the call is one the bridge knows, with arguments of the right shape;
//! 3. the plugin has the call's permission, approved now (a permission taken
//!    back fails at once);
//! 4. the view is under its rate (a token bucket, 100 calls a second).
//!
//! Then it runs. What a view says (a chat id, a tool name) is never trusted
//! beyond that: chats must exist and not be incognito, tools go through the
//! agent tools' own gates. Engine events reach subscribed views through
//! [`on_engine_line`].

use std::path::Path;
use std::time::Instant;

use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter, Manager};

use super::views::dispatch;
use super::{Entry, PluginUiState};
use crate::db::Db;
use crate::supervisor::{AppHost, EngineSupervisor, EnsureRequest, Host, Reason};

/// Calls a second per view, and the burst it may spend at once.
const RATE: f64 = 100.0;
/// A plugin's storage, at most (FR-13).
pub const MAX_STORAGE: usize = 10 * 1024 * 1024;
const MAX_TEXT: usize = 100_000;
const MAX_KEY: usize = 256;
const MAX_SUBSCRIPTIONS: usize = 32;

/// A refused call: a stable code and a sentence for the developer.
pub type Failure = (&'static str, String);

/// `{ok: false, code, message}`.
pub fn refusal(code: &str, message: &str) -> Value {
    json!({ "ok": false, "code": code, "message": message })
}

/// A token bucket: [`RATE`] calls a second, bursts up to [`RATE`].
#[derive(Debug, Clone)]
pub struct Bucket {
    tokens: f64,
    at: Option<Instant>,
}

impl Bucket {
    pub fn new() -> Self {
        Self { tokens: RATE, at: None }
    }

    /// Spend one call at `now`; false when the view is over its rate.
    pub fn take(&mut self, now: Instant) -> bool {
        if let Some(at) = self.at {
            self.tokens = (self.tokens + now.saturating_duration_since(at).as_secs_f64() * RATE).min(RATE);
        }
        self.at = Some(now);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

impl Default for Bucket {
    fn default() -> Self {
        Self::new()
    }
}

/// The permission a call needs: `Some(None)` for none, `None` for a call the
/// bridge does not know (spec section 4.3).
pub fn permission_for(call: &str) -> Option<Option<&'static str>> {
    Some(match call {
        "chat.active" | "chat.list" | "chat.messages" | "chat.subscribe" | "chat.unsubscribe" => Some("chat:read"),
        "chat.send" => Some("chat:send"),
        "tools.call" => Some("tools:call"),
        "composer.insert" => Some("composer:write"),
        "storage.get" | "storage.set" | "storage.delete" | "storage.list" => Some("storage"),
        "ai.models" | "ai.run" | "ai.cancel" | "ai.follow" | "ai.runs" | "ai.toolResult" | "ai.threads" | "ai.thread" | "ai.deleteThread" => Some("ai"),
        "ui.notify" | "ui.open" | "theme.tokens" | "view.data" | "view.resize" | "permissions.query" | "permissions.request" | "backend.call" | "auth.status" | "auth.signIn" => None,
        _ => return None,
    })
}

/// One call: `{call, args}`.
#[derive(Debug, PartialEq)]
pub struct Call {
    pub name: String,
    pub args: Map<String, Value>,
}

/// Read a call body: an object with a string `call` and an object (or no) `args`.
pub fn parse(body: &[u8]) -> Result<Call, Failure> {
    let value: Value = serde_json::from_slice(body).map_err(|_| ("bad_call", "the call is not JSON".to_string()))?;
    let name = value.get("call").and_then(Value::as_str).ok_or(("bad_call", "the call has no `call` name".to_string()))?;
    let args = match value.get("args") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(args)) => args.clone(),
        Some(_) => return Err(("bad_args", format!("{name}: `args` must be an object"))),
    };
    Ok(Call { name: name.to_string(), args })
}

/// An account as a view sees it: the service, its state and the names of
/// its sign-ins; no record ids, scopes or secrets (plugin-settings FR-27).
pub(crate) fn public_account(account: Value) -> Value {
    let sign_ins: Vec<Value> = account["signIns"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| json!({ "label": r["label"], "state": r["state"], "default": r["default"] }))
        .collect();
    json!({ "id": account["id"], "service": account["service"], "kind": account["kind"], "state": account["state"], "signIns": sign_ins })
}

/// A string argument, trimmed of nothing, of 1..=`max` characters.
fn text(call: &Call, key: &str, max: usize) -> Result<String, Failure> {
    let value = call.args.get(key).and_then(Value::as_str).ok_or_else(|| ("bad_args", format!("{}: `{key}` must be a string", call.name)))?;
    if value.is_empty() || value.chars().count() > max {
        return Err(("bad_args", format!("{}: `{key}` must have 1 to {max} characters", call.name)));
    }
    Ok(value.to_string())
}

/// An optional string argument.
fn optional_text(call: &Call, key: &str, max: usize) -> Result<Option<String>, Failure> {
    match call.args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => text(call, key, max).map(Some),
    }
}

/// Answer one call from view `label`, served under host `key`.
pub async fn handle(app: AppHandle, label: String, key: String, body: Vec<u8>) -> Value {
    match run(&app, &label, &key, &body).await {
        Ok(value) => json!({ "ok": true, "value": value }),
        Err((code, message)) => refusal(code, &message),
    }
}

/// The view's facts the calls need, read once.
struct Caller {
    label: String,
    window: String,
    slot: String,
    data: Value,
    entry: std::sync::Arc<Entry>,
}

async fn run(app: &AppHandle, label: &str, key: &str, body: &[u8]) -> Result<Value, Failure> {
    let state = app.state::<PluginUiState>();
    // 1. The label is an open view of the plugin served under this origin.
    let (window, slot, data, own_key) = {
        let views = state.views.lock().expect("views lock");
        let live = views.get(label).ok_or(("unknown_view", "this view is not open".to_string()))?;
        (live.window.clone(), live.slot.clone(), live.data.clone(), live.key.clone())
    };
    if own_key != key {
        return Err(("wrong_origin", "a view calls the bridge on its own origin only".into()));
    }
    let entry = state.plugin(key).ok_or(("plugin_off", "the plugin is off".to_string()))?;
    // 2. A known call, well formed.
    let call = parse(body)?;
    let needed = permission_for(&call.name).ok_or_else(|| ("unknown_call", format!("there is no bridge call `{}`", call.name)))?;
    // 3. Its permission, approved now.
    if let Some(permission) = needed {
        if !entry.may(permission) {
            return Err(("permission", format!("{} needs the `{permission}` permission, and {} does not have it", call.name, entry.name)));
        }
    }
    // 4. The rate.
    {
        let mut views = state.views.lock().expect("views lock");
        let live = views.get_mut(label).ok_or(("unknown_view", "this view is not open".to_string()))?;
        if !live.bucket.take(Instant::now()) {
            return Err(("rate_limited", "more than 100 calls a second".into()));
        }
    }
    let caller = Caller { label: label.to_string(), window, slot, data, entry };
    dispatch_call(app, &caller, &call).await
}

async fn dispatch_call(app: &AppHandle, caller: &Caller, call: &Call) -> Result<Value, Failure> {
    let entry = &caller.entry;
    match call.name.as_str() {
        "chat.active" => Ok(json!({ "chat": active_chat(app, &caller.window) })),
        "chat.list" => chat_list(app, call.args.get("archived").and_then(Value::as_bool).unwrap_or(false)),
        "chat.messages" => {
            let chat = target_chat(app, caller, call)?;
            let limit = call.args.get("limit").and_then(Value::as_u64).unwrap_or(100).clamp(1, 500);
            chat_messages(app, &chat, limit).await
        }
        "chat.subscribe" | "chat.unsubscribe" => {
            // `""`: whatever chat the view's window shows, as it changes.
            let key = match optional_text(call, "chat", 128)? {
                Some(chat) => {
                    visible_chat(app, &chat)?;
                    chat
                }
                None => String::new(),
            };
            let state = app.state::<PluginUiState>();
            let mut views = state.views.lock().expect("views lock");
            let live = views.get_mut(&caller.label).ok_or(("unknown_view", "this view is not open".to_string()))?;
            if call.name == "chat.subscribe" {
                if live.subscriptions.len() >= MAX_SUBSCRIPTIONS && !live.subscriptions.contains(&key) {
                    return Err(("too_many", format!("a view follows at most {MAX_SUBSCRIPTIONS} chats")));
                }
                live.subscriptions.insert(key.clone());
            } else {
                live.subscriptions.remove(&key);
            }
            Ok(json!({ "subscription": key }))
        }
        "chat.send" => {
            let message = text(call, "text", MAX_TEXT)?;
            let chat = target_chat(app, caller, call)?;
            let row = visible_chat(app, &chat)?;
            if row.archived_at.is_some() {
                return Err(("archived", "that chat is archived".into()));
            }
            // As a scheduled run does: mail to itself, delivered as a prompt;
            // the hub starts its engine when none runs.
            let (hub_app, to) = (app.clone(), chat.clone());
            let effects = tauri::async_runtime::spawn_blocking(move || {
                hub_app.state::<crate::mesh::MeshHub>().0.post(&to, &to, evo_mesh::wire::MailBody::new(message, evo_mesh::wire::Deliver::Prompt))
            })
            .await
            .unwrap_or_default();
            crate::mesh::apply(app, effects).await;
            Ok(json!({ "chat": chat }))
        }
        "tools.call" => {
            let name = text(call, "name", 128)?;
            let args = call.args.get("args").cloned().unwrap_or(json!({}));
            if !args.is_object() {
                return Err(("bad_args", "tools.call: `args` must be an object".into()));
            }
            let chat = active_chat(app, &caller.window).ok_or(("no_chat", "tools are called in the open chat, and none is open".to_string()))?;
            let output = crate::agent_tools::call_for_plugin(app, &chat, &entry.name, &name, args).await;
            Ok(output.to_json())
        }
        "ui.notify" => {
            let body = text(call, "body", 2_000)?;
            let title = optional_text(call, "title", 200)?;
            let level = call.args.get("level").and_then(Value::as_str).filter(|level| ["info", "success", "warning", "error"].contains(level)).unwrap_or("info");
            let _ = app.emit_to(caller.window.as_str(), "plugin-ui://notify", json!({ "plugin": entry.id, "name": entry.name, "title": title, "body": body, "level": level }));
            Ok(Value::Null)
        }
        "ui.open" => {
            let view = text(call, "view", 64)?;
            let slot = if entry.view("sidebar", &view).is_some() {
                "sidebar"
            } else if entry.view("panels", &view).is_some() {
                "panels"
            } else {
                return Err(("bad_args", format!("{} has no sidebar page or panel `{view}`", entry.name)));
            };
            let _ = app.emit_to(caller.window.as_str(), "plugin-ui://open", json!({ "plugin": entry.id, "view": view, "slot": slot }));
            Ok(Value::Null)
        }
        "composer.insert" => {
            let insert = text(call, "text", MAX_TEXT)?;
            let _ = app.emit_to(caller.window.as_str(), "plugin-ui://composer-insert", json!({ "plugin": entry.id, "text": insert }));
            Ok(Value::Null)
        }
        "theme.tokens" => Ok(app.state::<PluginUiState>().theme()),
        // Its own plugin's backend (docs/features/plugin-settings/spec.md,
        // FR-14): no permission, started when it sleeps; the backend first,
        // then the plugin's other servers.
        // Its accounts' states, and starting a sign-in; never a token or
        // cookie (docs/features/plugin-settings/spec.md, FR-27).
        // The person's models, directly (docs/features/plugin-ai/spec.md,
        // FR-1 – FR-9, FR-24): events go to this view as `ai.event`.
        "ai.models" => crate::plugin_ai::models(&entry.id, false).await,
        "ai.run" => {
            let options = match call.args.get("options") {
                Some(Value::Object(map)) => map.clone(),
                _ => return Err(("bad_args", "ai.run: `options` must be an object".into())),
            };
            let run_id = crate::plugin_ai::start_run(&entry.id, options, crate::plugin_ai::Follower::View(caller.label.clone())).await?;
            Ok(json!({ "runId": run_id }))
        }
        "ai.cancel" => {
            let run_id = text(call, "runId", 64)?;
            crate::plugin_ai::cancel(Some(&entry.id), &run_id).await.map(|()| Value::Null)
        }
        "ai.follow" => {
            let run_id = text(call, "runId", 64)?;
            crate::plugin_ai::follow(&entry.id, &run_id, crate::plugin_ai::Follower::View(caller.label.clone()))
        }
        "ai.runs" => Ok(crate::plugin_ai::runs(&entry.id)),
        "ai.threads" => crate::plugin_ai::threads(&entry.id).await,
        "ai.thread" => {
            let thread = text(call, "thread", 128)?;
            crate::plugin_ai::thread(&entry.id, &thread).await
        }
        "ai.deleteThread" => {
            let thread = text(call, "thread", 128)?;
            crate::plugin_ai::delete_thread(&entry.id, &thread).await.map(|()| Value::Null)
        }
        "ai.toolResult" => {
            let run_id = text(call, "runId", 64)?;
            let call_id = text(call, "callId", 256)?;
            let result = call.args.get("result").cloned().unwrap_or(Value::Null);
            let is_error = call.args.get("isError").and_then(Value::as_bool).unwrap_or(false);
            crate::plugin_ai::view_tool_result(&entry.id, &run_id, &call_id, result, is_error).await.map(|()| Value::Null)
        }
        "auth.status" => {
            let id = entry.id.clone();
            let accounts = tauri::async_runtime::spawn_blocking(move || {
                let store = evo_plugins::Store::in_agent_dir(&crate::mcp::agent_dir());
                let installed = store.installed().into_iter().find(|i| i.id == id)?;
                let plugin = store.load(&installed);
                Some(crate::plugin_auth::status(&installed, &plugin))
            })
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
            Ok(json!({ "accounts": accounts.into_iter().map(public_account).collect::<Vec<_>>() }))
        }
        "auth.signIn" => {
            let account = text(call, "account", 64)?;
            match crate::plugin_auth::sign_in_and_wait(&entry.id, &account).await {
                Ok(_) => Ok(json!({ "state": "signedIn" })),
                Err(error) if error == "cancelled" => Ok(json!({ "state": "cancelled" })),
                Err(error) => Err(("sign_in", error)),
            }
        }
        "backend.call" => {
            let tool = text(call, "tool", 128)?;
            let args = match call.args.get("args") {
                None | Some(Value::Null) => None,
                Some(Value::Object(map)) => Some(map.clone()),
                Some(_) => return Err(("bad_args", "backend.call: `args` must be an object".into())),
            };
            let pool = app.state::<crate::mcp::McpState>().0.clone();
            let mut keys = pool.plugin_keys(&entry.name);
            keys.sort_by_key(|key| !key.ends_with(":backend"));
            if keys.is_empty() {
                return Err(("no_backend", format!("{} has no backend running", entry.name)));
            }
            let mut last = String::new();
            for key in keys {
                match pool.call_server(&key, &tool, args.clone()).await {
                    Ok(result) => return Ok(result),
                    Err(error) if error.contains("has no tool") => last = error,
                    Err(error) => return Err(("backend", error)),
                }
            }
            Err(("bad_args", last))
        }
        // Optional permissions (docs/features/plugin-settings/spec.md, FR-9):
        // asked for when needed, the person asked once, the answer kept.
        "permissions.query" | "permissions.request" => {
            let permission = text(call, "id", 300)?;
            let id = entry.id.clone();
            let (installed, optional) = tauri::async_runtime::spawn_blocking(move || {
                let store = evo_plugins::Store::in_agent_dir(&crate::mcp::agent_dir());
                let installed = store.installed().into_iter().find(|i| i.id == id);
                let optional = installed.as_ref().map(|i| store.load(i)).and_then(|p| p.permission_info.into_iter().find(|info| info.id == permission && info.optional));
                (installed, optional)
            })
            .await
            .map_err(|e| ("failed", e.to_string()))?;
            let installed = installed.ok_or(("plugin_off", "the plugin is not installed".to_string()))?;
            let Some(info) = optional else {
                return Err(("bad_args", format!("{} does not ask for `{}` as an optional permission", entry.name, text(call, "id", 300)?)));
            };
            let state = if installed.approved_permissions.contains(&info.id) {
                "granted"
            } else if installed.optional_answers.get(&info.id) == Some(&false) {
                "denied"
            } else {
                "prompt"
            };
            if call.name == "permissions.request" && state == "prompt" {
                let _ = app.emit_to(
                    "main",
                    "plugin-ui://permission-request",
                    json!({ "plugin": entry.id, "name": entry.name, "permission": info.id, "label": evo_plugins::ui::describe_permission(&info.id), "reason": info.reason }),
                );
                return Ok(json!({ "state": "prompted" }));
            }
            Ok(json!({ "state": state }))
        }
        "view.data" => Ok(caller.data.clone()),
        "view.resize" => {
            if caller.slot != "cards" && caller.slot != "composer" {
                return Err(("bad_args", "only cards and composer views size themselves".into()));
            }
            let height = call.args.get("height").and_then(Value::as_f64).ok_or(("bad_args", "view.resize: `height` must be a number".to_string()))?;
            let height = height.clamp(40.0, 2_000.0);
            let _ = app.emit_to(caller.window.as_str(), "plugin-ui://resize", json!({ "label": caller.label, "height": height }));
            Ok(json!({ "height": height }))
        }
        "storage.get" | "storage.set" | "storage.delete" | "storage.list" => storage(app, entry, call).await,
        _ => Err(("unknown_call", format!("there is no bridge call `{}`", call.name))),
    }
}

// ── Chats ───────────────────────────────────────────────────────────────────

/// The chat the window shows, unless it is incognito.
pub(crate) fn active_chat(app: &AppHandle, window: &str) -> Option<String> {
    let chat = crate::agent_tools::chat_of_window(app, window)?;
    visible_chat(app, &chat).ok().map(|row| row.id)
}

/// A chat a plugin may see: it exists and is not incognito.
pub(crate) fn visible_chat(app: &AppHandle, chat: &str) -> Result<evo_store::chats::Chat, Failure> {
    let row = app.state::<Db>().with(|conn| evo_store::chats::get_chat(conn, chat)).map_err(|e| ("failed", e.to_string()))?;
    match row {
        Some(row) if !row.incognito => Ok(row),
        _ => Err(("no_chat", format!("there is no chat {chat}"))),
    }
}

/// `args.chat`, or the chat the view's window shows.
fn target_chat(app: &AppHandle, caller: &Caller, call: &Call) -> Result<String, Failure> {
    match optional_text(call, "chat", 128)? {
        Some(chat) => visible_chat(app, &chat).map(|row| row.id),
        None => active_chat(app, &caller.window).ok_or(("no_chat", "no chat is open; name one".to_string())),
    }
}

fn chat_list(app: &AppHandle, archived: bool) -> Result<Value, Failure> {
    let rows = app.state::<Db>().with(|conn| evo_store::chats::list_chats(conn, archived)).map_err(|e| ("failed", e.to_string()))?;
    let chats: Vec<Value> = rows
        .into_iter()
        .filter(|row| !row.incognito)
        .map(|row| {
            json!({
                "id": row.id,
                "title": row.title,
                "preview": row.preview,
                "pinned": row.pinned,
                "archived": row.archived_at.is_some(),
                "parent": row.parent_chat_id,
                "turns": row.turn_count,
                "createdAt": row.created_at,
                "updatedAt": row.updated_at,
            })
        })
        .collect();
    Ok(json!({ "chats": chats }))
}

/// A chat's last `limit` messages, from its engine; one is started (it
/// reopens the saved session) when the chat has none, and stops again when
/// idle.
pub(crate) async fn chat_messages(app: &AppHandle, chat: &str, limit: u64) -> Result<Value, Failure> {
    let running = app.state::<crate::engine::EngineState>().running().await.iter().any(|c| c == chat);
    let host = AppHost(app.clone());
    if !running {
        let request = EnsureRequest { chat: chat.to_string(), cwd: None, options: None, restore: true, restart: false, reason: Reason::Plugin };
        app.state::<EngineSupervisor>().ensure(&host, request).await.map_err(|e| ("engine", e))?;
    }
    let reply = host.request(chat, "get_messages", json!({ "limit": limit })).await.map_err(|e| ("engine", e))?;
    let messages: Vec<Value> = reply.get("messages").and_then(Value::as_array).map(|list| list.iter().filter_map(simple_message).collect()).unwrap_or_default();
    Ok(json!({ "chat": chat, "messages": messages }))
}

/// The text of a message's content: a string, or its text parts joined.
fn content_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// A message as views get it: `{role, text, at, tools?, tool?, isError?}`.
pub fn simple_message(message: &Value) -> Option<Value> {
    let role = message.get("role").and_then(Value::as_str)?;
    let mut out = json!({ "role": role, "text": content_text(message.get("content")), "at": message.get("timestamp").cloned().unwrap_or(Value::Null) });
    match role {
        "assistant" => {
            let tools: Vec<Value> = message
                .get("content")
                .and_then(Value::as_array)
                .map(|parts| {
                    parts
                        .iter()
                        .filter(|part| part.get("type").and_then(Value::as_str) == Some("toolCall"))
                        .map(|part| json!({ "id": part.get("id"), "name": part.get("name") }))
                        .collect()
                })
                .unwrap_or_default();
            out["tools"] = Value::Array(tools);
        }
        "toolResult" => {
            out["tool"] = message.get("toolName").cloned().unwrap_or(Value::Null);
            out["isError"] = Value::Bool(message.get("isError").and_then(Value::as_bool).unwrap_or(false));
        }
        _ => {}
    }
    Some(out)
}

/// An engine line as a view's chat event, or `None` for lines views do not get.
pub fn chat_event(line: &Value) -> Option<Value> {
    let kind = line.get("type").and_then(Value::as_str)?;
    Some(match kind {
        "agent_start" => json!({ "type": "turn_start" }),
        "agent_end" => json!({ "type": "turn_end" }),
        "message_update" => {
            let event = line.get("assistantMessageEvent")?;
            if event.get("type").and_then(Value::as_str) != Some("text_delta") {
                return None;
            }
            json!({ "type": "text_delta", "delta": event.get("delta").and_then(Value::as_str).unwrap_or_default() })
        }
        "message_end" => json!({ "type": "message", "message": simple_message(line.get("message")?)? }),
        "tool_execution_start" => json!({ "type": "tool_start", "id": line.get("toolCallId"), "name": line.get("toolName") }),
        "tool_execution_end" => json!({
            "type": "tool_end",
            "id": line.get("toolCallId"),
            "name": line.get("toolName"),
            "isError": line.get("isError").and_then(Value::as_bool).unwrap_or(false),
        }),
        _ => return None,
    })
}

/// One line of `chat`'s engine: views that follow that chat (by name, or as
/// their window's open chat) and may still read chats get it as an event.
pub fn on_engine_line(app: &AppHandle, chat: &str, line: &Value) {
    let Some(state) = app.try_state::<PluginUiState>() else { return };
    // Who follows anything, without the event built yet: most lines go nowhere.
    let followers: Vec<(String, String, String, bool, bool)> = {
        let views = state.views.lock().expect("views lock");
        views
            .values()
            .filter(|live| !live.subscriptions.is_empty())
            .map(|live| (live.label.clone(), live.key.clone(), live.window.clone(), live.subscriptions.contains(chat), live.subscriptions.contains("")))
            .collect()
    };
    if followers.is_empty() {
        return;
    }
    let Some(event) = chat_event(line) else { return };
    if visible_chat(app, chat).is_err() {
        return;
    }
    for (label, key, window, by_name, by_window) in followers {
        if !state.plugin(&key).is_some_and(|entry| entry.may("chat:read")) {
            continue;
        }
        let shown = by_window && crate::agent_tools::chat_of_window(app, &window).as_deref() == Some(chat);
        for subscription in [by_name.then_some(chat), shown.then_some("")].into_iter().flatten() {
            dispatch(app, &label, &json!({ "kind": "chat.event", "subscription": subscription, "chat": chat, "event": event }));
        }
    }
}

// ── Storage ─────────────────────────────────────────────────────────────────

/// Apply a storage call to `store`: its answer, and whether `store` changed.
pub fn storage_apply(store: &mut Map<String, Value>, call: &Call) -> Result<(Value, bool), Failure> {
    match call.name.as_str() {
        "storage.list" => Ok((json!(store.keys().collect::<Vec<_>>()), false)),
        "storage.get" => {
            let key = text(call, "key", MAX_KEY)?;
            Ok((store.get(&key).cloned().unwrap_or(Value::Null), false))
        }
        "storage.delete" => {
            let key = text(call, "key", MAX_KEY)?;
            let gone = store.remove(&key).is_some();
            Ok((Value::Bool(gone), gone))
        }
        "storage.set" => {
            let key = text(call, "key", MAX_KEY)?;
            let value = call.args.get("value").cloned().unwrap_or(Value::Null);
            let mut next = store.clone();
            next.insert(key, value);
            if Value::Object(next.clone()).to_string().len() > MAX_STORAGE {
                return Err(("storage_full", "a plugin keeps at most 10 MB".into()));
            }
            *store = next;
            Ok((Value::Null, true))
        }
        _ => Err(("unknown_call", format!("there is no bridge call `{}`", call.name))),
    }
}

/// Read `storage.json` (empty when missing or unreadable).
fn read_store(file: &Path) -> Map<String, Value> {
    std::fs::read(file).ok().and_then(|bytes| serde_json::from_slice::<Map<String, Value>>(&bytes).ok()).unwrap_or_default()
}

/// Write `storage.json` whole: a temporary file, then a rename.
fn write_store(file: &Path, store: &Map<String, Value>) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temporary = file.with_extension("json.tmp");
    std::fs::write(&temporary, serde_json::to_vec(store)?)?;
    std::fs::rename(&temporary, file)
}

async fn storage(app: &AppHandle, entry: &Entry, call: &Call) -> Result<Value, Failure> {
    let state = app.state::<PluginUiState>();
    let _one_at_a_time = state.storage.lock().await;
    let file = entry.data.join("storage.json");
    let call = Call { name: call.name.clone(), args: call.args.clone() };
    tauri::async_runtime::spawn_blocking(move || {
        let mut store = read_store(&file);
        let (answer, changed) = storage_apply(&mut store, &call)?;
        if changed {
            write_store(&file, &store).map_err(|e| ("failed", format!("could not save: {e}")))?;
        }
        Ok(answer)
    })
    .await
    .map_err(|e| ("failed", e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn call(name: &str, args: Value) -> Call {
        Call { name: name.into(), args: args.as_object().cloned().unwrap_or_default() }
    }

    #[test]
    fn every_call_has_its_permission_and_unknown_ones_none() {
        assert_eq!(permission_for("chat.list"), Some(Some("chat:read")));
        assert_eq!(permission_for("chat.subscribe"), Some(Some("chat:read")));
        assert_eq!(permission_for("chat.send"), Some(Some("chat:send")));
        assert_eq!(permission_for("tools.call"), Some(Some("tools:call")));
        assert_eq!(permission_for("composer.insert"), Some(Some("composer:write")));
        assert_eq!(permission_for("storage.set"), Some(Some("storage")));
        for free in ["ui.notify", "ui.open", "theme.tokens", "view.data", "view.resize"] {
            assert_eq!(permission_for(free), Some(None), "{free}");
        }
        for unknown in ["shell.run", "chat", "", "tools.list", "app.invoke"] {
            assert_eq!(permission_for(unknown), None, "{unknown}");
        }
        // Every permission a call needs is one a plugin can ask for.
        for name in ["chat.list", "chat.send", "tools.call", "composer.insert", "storage.get"] {
            let permission = permission_for(name).unwrap().unwrap();
            assert!(evo_plugins::ui::PERMISSIONS.contains(&permission), "{permission}");
        }
    }

    #[test]
    fn calls_must_be_well_formed() {
        assert_eq!(parse(br#"{"call":"chat.list"}"#).unwrap(), call("chat.list", json!({})));
        assert_eq!(parse(br#"{"id":3,"call":"storage.get","args":{"key":"a"}}"#).unwrap().args["key"], "a");
        assert_eq!(parse(b"nope").unwrap_err().0, "bad_call");
        assert_eq!(parse(br#"{"args":{}}"#).unwrap_err().0, "bad_call");
        assert_eq!(parse(br#"{"call":"x","args":[1]}"#).unwrap_err().0, "bad_args");
        let send = call("chat.send", json!({ "text": "" }));
        assert_eq!(text(&send, "text", 10).unwrap_err().0, "bad_args");
        let send = call("chat.send", json!({ "text": 7 }));
        assert_eq!(text(&send, "text", 10).unwrap_err().0, "bad_args");
        let send = call("chat.send", json!({ "text": "hello world" }));
        assert!(text(&send, "text", 5).is_err() && text(&send, "text", 50).is_ok());
    }

    #[test]
    fn the_rate_is_a_hundred_calls_a_second() {
        let start = Instant::now();
        let mut bucket = Bucket::new();
        for _ in 0..100 {
            assert!(bucket.take(start));
        }
        assert!(!bucket.take(start), "the 101st call in the same instant");
        assert!(bucket.take(start + Duration::from_millis(20)), "two calls back after 20 ms");
        assert!(bucket.take(start + Duration::from_millis(20)));
        assert!(!bucket.take(start + Duration::from_millis(20)));
        assert!(bucket.take(start + Duration::from_secs(5)), "full again, never above the burst");
    }

    #[test]
    fn storage_keeps_values_and_its_limit() {
        let mut store = Map::new();
        assert_eq!(storage_apply(&mut store, &call("storage.set", json!({ "key": "a", "value": { "n": 1 } }))).unwrap(), (Value::Null, true));
        assert_eq!(storage_apply(&mut store, &call("storage.get", json!({ "key": "a" }))).unwrap().0, json!({ "n": 1 }));
        assert_eq!(storage_apply(&mut store, &call("storage.get", json!({ "key": "b" }))).unwrap().0, Value::Null);
        assert_eq!(storage_apply(&mut store, &call("storage.list", json!({}))).unwrap().0, json!(["a"]));
        let big = "x".repeat(MAX_STORAGE);
        assert_eq!(storage_apply(&mut store, &call("storage.set", json!({ "key": "big", "value": big }))).unwrap_err().0, "storage_full");
        assert!(!store.contains_key("big"), "a refused write changes nothing");
        assert_eq!(storage_apply(&mut store, &call("storage.delete", json!({ "key": "a" }))).unwrap(), (Value::Bool(true), true));
        assert_eq!(storage_apply(&mut store, &call("storage.delete", json!({ "key": "a" }))).unwrap(), (Value::Bool(false), false));
        assert_eq!(storage_apply(&mut store, &call("storage.get", json!({ "key": "" }))).unwrap_err().0, "bad_args");
    }

    #[test]
    fn storage_files_are_written_whole() {
        let dir = std::env::temp_dir().join(format!("evo-plugin-storage-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("storage.json");
        assert!(read_store(&file).is_empty(), "no file reads as empty");
        let mut store = Map::new();
        store.insert("k".into(), json!([1, 2]));
        write_store(&file, &store).unwrap();
        assert_eq!(read_store(&file), store);
        assert!(!dir.join("storage.json.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn engine_lines_become_plain_events() {
        assert_eq!(chat_event(&json!({ "type": "agent_start" })), Some(json!({ "type": "turn_start" })));
        assert_eq!(
            chat_event(&json!({ "type": "message_update", "assistantMessageEvent": { "type": "text_delta", "delta": "Hi" } })),
            Some(json!({ "type": "text_delta", "delta": "Hi" }))
        );
        assert_eq!(chat_event(&json!({ "type": "message_update", "assistantMessageEvent": { "type": "thinking_delta", "delta": "…" } })), None);
        let end = chat_event(&json!({ "type": "message_end", "message": { "role": "assistant", "content": [{ "type": "text", "text": "Done" }, { "type": "toolCall", "id": "c1", "name": "read" }], "timestamp": 5 } })).unwrap();
        assert_eq!(end["message"], json!({ "role": "assistant", "text": "Done", "at": 5, "tools": [{ "id": "c1", "name": "read" }] }));
        assert_eq!(chat_event(&json!({ "type": "tool_execution_end", "toolCallId": "c1", "toolName": "read", "isError": true })).unwrap()["isError"], true);
        for quiet in ["ready", "external_tool_request", "ui_dialog_request", "response"] {
            assert_eq!(chat_event(&json!({ "type": quiet })), None, "{quiet}");
        }
        assert_eq!(simple_message(&json!({ "role": "user", "content": "plain" })).unwrap()["text"], "plain");
    }
}
