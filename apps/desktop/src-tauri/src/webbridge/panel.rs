//! The browser's side panel as a client of Evo's chats (docs/features/browser-bridge/spec.md
//! US-3, US-8, FR-10, FR-11, FR-14, FR-25 – FR-28; plan 3.3 Side panel bodies).
//!
//! A panel session (one per open side panel) asks with `{t:"panel", session,
//! req, body:{op,…}}` and gets `{t:"panel", session, req, body}` back. The
//! chat it shows is a chat like any other: the shell hosts its engine
//! (`supervisor::engine_ensure`), a prompt from the panel is the person's own
//! prompt, and the engine lines the panel needs (text, tool activity, the end
//! of a turn) and the chat's open questions are passed on as `event`,
//! `question` and `settled_question` bodies.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;

use super::Webbridge;
use crate::db::Db;

/// Panel sessions and the engine requests they wait on.
#[derive(Default)]
pub struct Panels {
    /// `<profile>|<session>` → the chat it shows.
    sessions: Mutex<HashMap<String, String>>,
    /// Engine request id → its answer.
    waiting: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    next: AtomicU64,
}

fn key(profile: &str, session: &str) -> String {
    format!("{profile}|{session}")
}

/// A request from a side panel.
pub fn on_message(app: &AppHandle, profile: &str, message: &Value) {
    let Some(session) = message["session"].as_str().map(str::to_string) else {
        return;
    };
    let request = message["req"].clone();
    let body = message["body"].clone();
    let (app, profile) = (app.clone(), profile.to_string());
    tauri::async_runtime::spawn(async move {
        let answer = match handle(&app, &profile, &session, &body).await {
            Ok(value) => value,
            Err(error) => json!({ "op": "error", "message": error }),
        };
        let _ = app.state::<Webbridge>().notify(
            &profile,
            json!({ "t": "panel", "session": session, "req": request, "body": answer }),
        );
    });
}

/// A panel closed.
pub fn closed(app: &AppHandle, profile: &str, session: &str) {
    app.state::<Webbridge>()
        .panels
        .sessions
        .lock()
        .expect("panels lock")
        .remove(&key(profile, session));
}

/// Every panel of a profile closes with its connection.
pub fn profile_gone(app: &AppHandle, profile: &str) {
    let prefix = format!("{profile}|");
    app.state::<Webbridge>()
        .panels
        .sessions
        .lock()
        .expect("panels lock")
        .retain(|k, _| !k.starts_with(&prefix));
}

async fn handle(
    app: &AppHandle,
    profile: &str,
    session: &str,
    body: &Value,
) -> Result<Value, String> {
    let chat_arg = || {
        body["chat"]
            .as_str()
            .filter(|c| !c.is_empty())
            .map(str::to_string)
            .ok_or("no chat")
    };
    match body["op"].as_str().unwrap_or("") {
        "chats" => {
            Ok(json!({ "op": "chats", "items": chats(app, body["query"].as_str().unwrap_or(""))? }))
        }
        "open" => {
            let chat = chat_arg()?;
            watch(app, profile, session, &chat);
            open(app, &chat).await
        }
        "new" => {
            let chat = new_chat(app, body["workspace"].as_str())?;
            watch(app, profile, session, &chat);
            open(app, &chat).await
        }
        "send" => {
            let chat = chat_arg()?;
            // A new chat is named after its first message, as the app names its own (FR-11).
            let title = name_new_chat(app, &chat, body["text"].as_str().unwrap_or(""));
            watch(app, profile, session, &chat);
            // The chat's pages open in this browser unless the agent names another.
            app.state::<Webbridge>()
                .panel_browser
                .lock()
                .expect("webbridge lock")
                .insert(chat.clone(), profile.to_string());
            send(app, profile, &chat, body).await?;
            Ok(json!({ "op": "sent", "chat": chat, "title": title }))
        }
        "stop" => {
            super::stop_chat(app, &chat_arg()?);
            Ok(json!({ "op": "stopped" }))
        }
        "answer" => {
            answer(app, &chat_arg()?, body)?;
            Ok(json!({ "op": "answered" }))
        }
        "models" => {
            let chat = chat_arg()?;
            ensure(app, &chat).await?;
            let models = engine_request(app, &chat, "get_available_models", json!({})).await?;
            let levels = engine_request(app, &chat, "get_available_thinking_levels", json!({}))
                .await
                .unwrap_or(Value::Null);
            Ok(json!({ "op": "models", "models": models["models"], "levels": levels["levels"] }))
        }
        "set_model" => {
            let chat = chat_arg()?;
            ensure(app, &chat).await?;
            engine_request(
                app,
                &chat,
                "set_model",
                json!({ "provider": body["provider"], "modelId": body["modelId"] }),
            )
            .await?;
            model_changed(app, &chat);
            state(app, &chat).await
        }
        "set_effort" => {
            let chat = chat_arg()?;
            ensure(app, &chat).await?;
            engine_request(
                app,
                &chat,
                "set_thinking_level",
                json!({ "level": body["level"] }),
            )
            .await?;
            model_changed(app, &chat);
            state(app, &chat).await
        }
        "set_mode" => {
            let mode = body["mode"]
                .as_str()
                .filter(|m| matches!(*m, "ask" | "auto-edits" | "auto"))
                .ok_or("mode is ask, auto-edits or auto")?;
            set_mode(app, mode)?;
            state(app, &chat_arg()?).await
        }
        "attach" => {
            let name = body["name"].as_str().unwrap_or("attachment");
            let data = body["data"].as_str().ok_or("no data")?;
            // The app's own limit for a pasted file.
            if data.len() > 40 * 1024 * 1024 {
                return Err("The file is too large to attach (over 30 MB).".into());
            }
            let path =
                crate::files::fs_save_paste(app.clone(), name.to_string(), data.to_string())?;
            Ok(json!({ "op": "attached", "path": path, "name": name, "kind": body["kind"] }))
        }
        "open_in_app" => {
            let chat = chat_arg()?;
            use tauri::Emitter;
            let _ = app.emit("app://open-chat", json!({ "chat": chat }));
            crate::system::show_main(app);
            Ok(json!({ "op": "opened_in_app" }))
        }
        "close" => {
            closed(app, profile, session);
            Ok(json!({ "op": "closed" }))
        }
        other => Err(format!("unknown panel op {other}")),
    }
}

fn watch(app: &AppHandle, profile: &str, session: &str, chat: &str) {
    app.state::<Webbridge>()
        .panels
        .sessions
        .lock()
        .expect("panels lock")
        .insert(key(profile, session), chat.to_string());
}

/// Recent chats of every workspace, newest first (spec 6.2).
fn chats(app: &AppHandle, query: &str) -> Result<Vec<Value>, String> {
    let db = app.state::<Db>();
    let (list, workspaces) = db.with(|conn| {
        Ok((
            evo_store::chats::list_chats(conn, false)?,
            evo_store::chats::list_workspaces(conn)?,
        ))
    })?;
    let names: HashMap<String, String> = workspaces.into_iter().map(|w| (w.id, w.name)).collect();
    let query = query.trim().to_lowercase();
    let mut list: Vec<_> = list
        .into_iter()
        .filter(|c| c.parent_chat_id.is_none())
        .filter(|c| query.is_empty() || c.title.to_lowercase().contains(&query))
        .collect();
    list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(list
        .into_iter()
        .take(40)
        .map(|c| {
            json!({
                "chat": c.id,
                "title": if c.title.trim().is_empty() { "New chat".to_string() } else { c.title.clone() },
                "workspace": c.workspace_id.as_ref().and_then(|w| names.get(w)).cloned().unwrap_or_else(|| "This computer".to_string()),
                "updated": c.updated_at,
            })
        })
        .collect())
}

/// A new chat in the workspace set in Settings, else the last used one.
fn new_chat(app: &AppHandle, workspace: Option<&str>) -> Result<String, String> {
    let db = app.state::<Db>();
    let setting = db
        .with(|conn| evo_store::settings::get(conn, super::NEW_CHAT_WORKSPACE_KEY))
        .ok()
        .flatten()
        .and_then(|v| v.as_str().map(str::to_string));
    let workspace = match workspace.map(str::to_string).or(setting) {
        Some(w) => Some(w),
        None => db
            .with(|conn| evo_store::chats::list_chats(conn, false))
            .ok()
            .and_then(|mut list| {
                list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
                list.into_iter().find_map(|c| c.workspace_id)
            }),
    };
    let id = new_id();
    let chat: evo_store::chats::NewChat = serde_json::from_value(json!({ "id": id, "workspace_id": workspace, "title": "", "provider": null, "model": null }))
        .map_err(|e| e.to_string())?;
    db.with(|conn| evo_store::chats::create_chat(conn, &chat))?;
    announce(app, &id);
    Ok(id)
}

/// The app's windows show a chat the panel made or renamed (`chat://updated`).
fn announce(app: &AppHandle, chat: &str) {
    if let Ok(Some(row)) = app
        .state::<Db>()
        .with(|conn| evo_store::chats::get_chat(conn, chat))
    {
        let _ = app.emit("chat://updated", json!({ "from": "shell", "chat": row }));
    }
}

/// The title of a message as the app makes it: spaces folded, at most 80 characters.
fn title_from(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(80)
        .collect()
}

/// Name a chat that has no title yet after `text`; the title it has after.
fn name_new_chat(app: &AppHandle, chat: &str, text: &str) -> Option<String> {
    let db = app.state::<Db>();
    let row = db
        .with(|conn| evo_store::chats::get_chat(conn, chat))
        .ok()
        .flatten()?;
    if !row.title.trim().is_empty() {
        return Some(row.title);
    }
    let title = title_from(text);
    if title.is_empty() {
        return None;
    }
    let patch: evo_store::chats::ChatPatch =
        serde_json::from_value(json!({ "autoTitle": title })).ok()?;
    db.with(|conn| evo_store::chats::update_chat(conn, chat, &patch))
        .ok()?;
    announce(app, chat);
    Some(title)
}

/// A random id in the shape of the app's (`crypto.randomUUID()`).
fn new_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0u8; 16];
    for (i, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        hasher.write_usize(i);
        chunk.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// The chat's engine runs (started like a window's, reopening its session).
async fn ensure(app: &AppHandle, chat: &str) -> Result<(), String> {
    let restore = app
        .state::<Db>()
        .with(|conn| evo_store::chats::get_chat(conn, chat))
        .ok()
        .flatten()
        .is_some_and(|c| c.turn_count > 0);
    crate::supervisor::engine_ensure(
        app.clone(),
        chat.to_string(),
        None,
        None,
        Some(restore),
        None,
    )
    .await
    .map(|_| ())
}

/// The chat as the panel first shows it: its messages, model, effort, mode and open questions.
async fn open(app: &AppHandle, chat: &str) -> Result<Value, String> {
    ensure(app, chat).await?;
    let messages = engine_request(app, chat, "get_messages", json!({ "limit": 200 }))
        .await
        .unwrap_or(Value::Null);
    let questions = app
        .state::<crate::supervisor::EngineSupervisor>()
        .questions
        .of(chat);
    let mut state = state(app, chat).await?;
    state["op"] = json!("opened");
    state["messages"] = json!(reduce_messages(&messages["messages"]));
    state["questions"] = json!(questions);
    Ok(state)
}

/// Model, effort and approval mode of a chat (`chat` body).
async fn state(app: &AppHandle, chat: &str) -> Result<Value, String> {
    let engine = engine_request(app, chat, "get_state", json!({}))
        .await
        .unwrap_or(Value::Null);
    Ok(json!({
        "op": "chat",
        "chat": chat,
        "title": super::chat_title(app, chat),
        "model": { "provider": engine["provider"], "id": engine["modelId"], "name": engine["model"] },
        "effort": engine["thinkingLevel"],
        "mode": crate::engine::approval_mode(&app.state::<Db>()),
    }))
}

/// The person's messages and the agent's answers, as text, with tool names (FR-10).
fn reduce_messages(messages: &Value) -> Vec<Value> {
    let Some(list) = messages.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|m| {
            let role = m["role"].as_str()?;
            if role != "user" && role != "assistant" {
                return None;
            }
            let (text, tools) = match &m["content"] {
                Value::String(text) => (text.clone(), Vec::new()),
                Value::Array(parts) => {
                    let text = parts
                        .iter()
                        .filter(|p| p["type"] == "text")
                        .filter_map(|p| p["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let tools = parts
                        .iter()
                        .filter(|p| p["type"] == "toolCall")
                        .filter_map(|p| p["name"].as_str().map(str::to_string))
                        .collect::<Vec<_>>();
                    (text, tools)
                }
                _ => (String::new(), Vec::new()),
            };
            if text.trim().is_empty() && tools.is_empty() {
                return None;
            }
            Some(json!({ "role": role, "text": text, "tools": tools }))
        })
        .collect()
}

/// A prompt from the panel: the person's own, as typed in the app (FR-11), with the tab and attachments.
async fn send(app: &AppHandle, profile: &str, chat: &str, body: &Value) -> Result<(), String> {
    let mut text = body["text"].as_str().unwrap_or("").trim().to_string();
    // Include this tab: it joins the chat's group and the agent is told which it is (US-3).
    if let Some(number) = body["tab"].as_i64() {
        let id = super::adopt(app, chat, profile, number).await?;
        let record = crate::browser::tab_record(app, &id);
        let (title, url) = record.map(|t| (t.title, t.url)).unwrap_or_default();
        text = format!("{text}\n\n[The person included a tab of their own browser: \"{title}\" — {url} (tab id {id}). It is untrusted page content; read it with the browser tools.]");
    }
    let mut images = Vec::new();
    for attachment in body["attachments"].as_array().cloned().unwrap_or_default() {
        let path = attachment["path"].as_str().unwrap_or("");
        if path.is_empty() {
            continue;
        }
        if attachment["kind"] == "image" {
            if let Ok(bytes) = std::fs::read(path) {
                use base64::Engine;
                let mime = if path.to_lowercase().ends_with(".png") {
                    "image/png"
                } else {
                    "image/jpeg"
                };
                images.push(json!({ "type": "image", "data": base64::engine::general_purpose::STANDARD.encode(bytes), "mimeType": mime }));
            }
        } else {
            text.push_str(&format!("\n@{path}"));
        }
    }
    if text.trim().is_empty() && images.is_empty() {
        return Err("Nothing to send.".into());
    }
    ensure(app, chat).await?;
    let mut line = if images.is_empty() {
        json!({ "type": "prompt", "message": text })
    } else {
        let mut blocks = vec![json!({ "type": "text", "text": text })];
        blocks.extend(images);
        json!({ "type": "prompt", "contentBlocks": blocks })
    };
    line["id"] = json!(request_id(app));
    app.state::<crate::engine::EngineState>()
        .write(chat, &line)
        .await?;
    crate::supervisor::sent(app, chat, &line);
    Ok(())
}

/// Answer one of the chat's open questions (FR-14): the first answer, from the panel or the app, wins.
fn answer(app: &AppHandle, chat: &str, body: &Value) -> Result<(), String> {
    use crate::supervisor::questions::Source;
    let request = body["question"].as_str().ok_or("no question")?;
    let index = body["index"].as_i64().unwrap_or(-1) as i32;
    let question = app
        .state::<crate::supervisor::EngineSupervisor>()
        .questions
        .of(chat)
        .into_iter()
        .find(|q| q.request_id == request)
        .ok_or("The question was already answered.")?;
    match question.source {
        Source::Shell => {
            crate::agent_tools::answer_ask(app, request, index);
            Ok(())
        }
        Source::Engine => {
            let option = usize::try_from(index)
                .ok()
                .and_then(|i| question.options.get(i))
                .cloned();
            let mut line = match option {
                Some(value) => {
                    json!({ "type": "ui_dialog_response", "requestId": request, "answer": { "value": value, "values": [value], "index": index } })
                }
                None => {
                    json!({ "type": "ui_dialog_response", "requestId": request, "cancelled": true })
                }
            };
            line["id"] = json!(request_id(app));
            let (app, chat) = (app.clone(), chat.to_string());
            tauri::async_runtime::spawn(async move {
                if app
                    .state::<crate::engine::EngineState>()
                    .write(&chat, &line)
                    .await
                    .is_ok()
                {
                    crate::supervisor::sent(&app, &chat, &line);
                }
            });
            Ok(())
        }
    }
}

/// The approval mode is the app's (every chat follows it): set it as Settings does.
fn set_mode(app: &AppHandle, mode: &str) -> Result<(), String> {
    let value = json!(mode);
    app.state::<Db>()
        .with(|conn| evo_store::settings::set(conn, crate::engine::APPROVAL_MODE_KEY, &value))?;
    crate::supervisor::settings::on_setting(app, crate::engine::APPROVAL_MODE_KEY, &value);
    // The app's windows show the mode as their own switch does (FR-27).
    let _ = app.emit("approval://changed", json!({ "mode": mode }));
    Ok(())
}

/// The panel changed a chat's model or effort: the app's windows read its state again (FR-26).
fn model_changed(app: &AppHandle, chat: &str) {
    let _ = app.emit(
        "engine://reloaded",
        json!({ "chat": chat, "what": "model" }),
    );
}

fn request_id(app: &AppHandle) -> String {
    format!(
        "webbridge-panel-{}",
        app.state::<Webbridge>()
            .panels
            .next
            .fetch_add(1, Ordering::SeqCst)
            + 1
    )
}

/// Ask the chat's engine and wait for its `response` (answered in [`on_engine_line`]).
async fn engine_request(
    app: &AppHandle,
    chat: &str,
    kind: &str,
    mut payload: Value,
) -> Result<Value, String> {
    let id = request_id(app);
    payload["type"] = json!(kind);
    payload["id"] = json!(id);
    let (tx, rx) = oneshot::channel();
    app.state::<Webbridge>()
        .panels
        .waiting
        .lock()
        .expect("panels lock")
        .insert(id.clone(), tx);
    if let Err(error) = app
        .state::<crate::engine::EngineState>()
        .write(chat, &payload)
        .await
    {
        app.state::<Webbridge>()
            .panels
            .waiting
            .lock()
            .expect("panels lock")
            .remove(&id);
        return Err(error);
    }
    let reply = tokio::time::timeout(Duration::from_secs(20), rx)
        .await
        .map_err(|_| format!("the engine did not answer {kind}"))?
        .map_err(|_| "the engine went away".to_string())?;
    if reply["success"].as_bool() == Some(false) {
        return Err(reply["error"]
            .as_str()
            .unwrap_or("the engine refused")
            .to_string());
    }
    Ok(reply.get("data").cloned().unwrap_or(Value::Null))
}

/// An engine line: answers to the panel's requests, and what panels showing the chat follow.
pub fn on_engine_line(app: &AppHandle, chat: &str, message: &Value) {
    let kind = message["type"].as_str().unwrap_or("");
    let state = app.state::<Webbridge>();
    if kind == "response" {
        if let Some(id) = message["id"]
            .as_str()
            .filter(|id| id.starts_with("webbridge-panel-"))
        {
            if let Some(tx) = state.panels.waiting.lock().expect("panels lock").remove(id) {
                let _ = tx.send(message.clone());
            }
        }
        return;
    }
    let event = match kind {
        "agent_start" => json!({ "kind": "start" }),
        "message_update" => {
            let event = &message["assistantMessageEvent"];
            match event["type"].as_str().unwrap_or("") {
                "text_delta" => json!({ "kind": "text", "delta": event["delta"] }),
                "thinking_delta" => json!({ "kind": "thinking" }),
                _ => return,
            }
        }
        "message_start" if message["message"]["role"] == "user" => {
            let content = &message["message"]["content"];
            let text = match content {
                Value::String(t) => t.clone(),
                Value::Array(parts) => parts
                    .iter()
                    .filter_map(|p| p["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => String::new(),
            };
            json!({ "kind": "user", "text": text })
        }
        "message_end" if message["message"]["role"] == "assistant" => {
            json!({ "kind": "message_end" })
        }
        "tool_execution_start" => {
            json!({ "kind": "tool", "name": message["toolName"], "summary": tool_summary(&message["args"]) })
        }
        "tool_execution_end" => {
            json!({ "kind": "tool_end", "name": message["toolName"], "error": message["isError"] })
        }
        "agent_settled" => json!({ "kind": "settled", "outcome": message["outcome"] }),
        _ => return,
    };
    send_to_watchers(
        app,
        chat,
        json!({ "op": "event", "chat": chat, "event": event }),
    );
}

/// A short line for a tool's arguments (an address, a ref, a path).
fn tool_summary(args: &Value) -> String {
    for key in [
        "url",
        "ref",
        "path",
        "query",
        "command",
        "expression",
        "text",
    ] {
        if let Some(value) = args[key].as_str() {
            let value: String = value.chars().take(80).collect();
            return value;
        }
    }
    String::new()
}

/// A question of the chat opened or closed: panels showing it follow (FR-14).
pub fn question(app: &AppHandle, chat: &str, change: &crate::supervisor::questions::Change) {
    use crate::supervisor::questions::Change;
    let body = match change {
        Change::Opened(question) => json!({ "op": "question", "chat": chat, "question": question }),
        Change::Closed(request) => {
            json!({ "op": "settled_question", "chat": chat, "question": request })
        }
    };
    send_to_watchers(app, chat, body);
}

fn send_to_watchers(app: &AppHandle, chat: &str, body: Value) {
    let state = app.state::<Webbridge>();
    let watchers: Vec<(String, String)> = state
        .panels
        .sessions
        .lock()
        .expect("panels lock")
        .iter()
        .filter(|(_, c)| c.as_str() == chat)
        .filter_map(|(k, _)| {
            k.split_once('|')
                .map(|(p, s)| (p.to_string(), s.to_string()))
        })
        .collect();
    for (profile, session) in watchers {
        let _ = state.notify(
            &profile,
            json!({ "t": "panel", "session": session, "body": body }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_reduce_to_text_and_tool_names() {
        let messages = json!([
            { "role": "user", "content": "Export the invoices" },
            { "role": "assistant", "content": [{ "type": "thinking", "thinking": "…" }, { "type": "text", "text": "Opening Billing." }, { "type": "toolCall", "name": "browser_navigate" }] },
            { "role": "toolResult", "content": [{ "type": "text", "text": "Loaded" }] },
            { "role": "assistant", "content": [{ "type": "text", "text": "" }] }
        ]);
        let reduced = reduce_messages(&messages);
        assert_eq!(reduced.len(), 2);
        assert_eq!(reduced[0]["text"], "Export the invoices");
        assert_eq!(reduced[1]["text"], "Opening Billing.");
        assert_eq!(reduced[1]["tools"], json!(["browser_navigate"]));
    }

    #[test]
    fn a_title_is_the_first_message_folded_and_cut() {
        assert_eq!(
            title_from("  Summarize\n this   page  "),
            "Summarize this page"
        );
        assert_eq!(title_from(&"x".repeat(200)).len(), 80);
        assert_eq!(title_from("   "), "");
    }

    #[test]
    fn ids_look_like_the_apps() {
        let id = new_id();
        assert_eq!(id.len(), 36);
        assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
        assert_eq!(&id[14..15], "4");
        assert_ne!(id, new_id());
    }

    #[test]
    fn tool_lines_name_what_they_act_on() {
        assert_eq!(
            tool_summary(&json!({ "url": "https://example.com", "ref": "ref_2" })),
            "https://example.com"
        );
        assert_eq!(tool_summary(&json!({ "ref": "ref_2" })), "ref_2");
        assert_eq!(tool_summary(&json!({})), "");
    }
}
