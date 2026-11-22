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
        // While the agent works: read at its next step (`steer`) or after the run (`followUp`).
        "queue" => {
            let chat = chat_arg()?;
            let deliver = body["deliverAs"]
                .as_str()
                .filter(|d| matches!(*d, "steer" | "followUp"))
                .unwrap_or("steer");
            let (text, images) = prompt_parts(body);
            if text.trim().is_empty() && images.is_empty() {
                return Err("Nothing to send.".into());
            }
            ensure(app, &chat).await?;
            let mut payload = prompt_line("queue_message", &text, images);
            payload["deliverAs"] = json!(deliver);
            let kind = payload["type"]
                .as_str()
                .unwrap_or("queue_message")
                .to_string();
            let queued = engine_request(app, &chat, &kind, payload).await?;
            Ok(
                json!({ "op": "queued", "entryId": queued["entryId"], "text": text, "deliverAs": deliver }),
            )
        }
        "cancel_queued" => {
            let chat = chat_arg()?;
            engine_request(
                app,
                &chat,
                "cancel_queued",
                json!({ "entryId": body["entryId"] }),
            )
            .await?;
            Ok(json!({ "op": "unqueued", "entryId": body["entryId"] }))
        }
        "answer" => {
            answer(app, &chat_arg()?, body)?;
            Ok(json!({ "op": "answered" }))
        }
        // The agent's questions (`ask_user`), answered as the app's questions panel does.
        "answer_questions" => {
            answer_questions(app, &chat_arg()?, body)?;
            Ok(json!({ "op": "answered" }))
        }
        "models" => {
            let chat = chat_arg()?;
            ensure(app, &chat).await?;
            let models = engine_request(app, &chat, "get_available_models", json!({})).await?;
            let levels = engine_request(app, &chat, "get_available_thinking_levels", json!({}))
                .await
                .unwrap_or(Value::Null);
            let default = engine_request(app, &chat, "get_default_model", json!({}))
                .await
                .unwrap_or(Value::Null);
            // The same short list, default and effort levels the app's model menu shows (FR-26).
            Ok(json!({
                "op": "models",
                "models": models["models"],
                "levels": levels["levels"],
                "pinned": pinned_models(app),
                "default": model_key(&default),
            }))
        }
        "set_pinned" => {
            let keys: Vec<Value> = body["pinned"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|k| k.as_str().is_some_and(|k| k.contains('/')))
                .take(PINNED_MODELS_MAX)
                .collect();
            let value = json!(keys);
            app.state::<Db>()
                .with(|conn| evo_store::settings::set(conn, PINNED_MODELS_KEY, &value))?;
            // The app's windows keep the same short list.
            let _ = app.emit("models://pinned", json!({ "pinned": value }));
            app_setting_changed(app, PINNED_MODELS_KEY, &value);
            Ok(json!({ "op": "pinned", "pinned": value }))
        }
        "set_default" => {
            let chat = chat_arg()?;
            ensure(app, &chat).await?;
            let level = engine_request(app, &chat, "get_state", json!({}))
                .await
                .map(|s| s["thinkingLevel"].clone())
                .unwrap_or(Value::Null);
            let saved = engine_request(
                app,
                &chat,
                "set_default_model",
                json!({ "provider": body["provider"], "modelId": body["modelId"], "thinkingLevel": level }),
            )
            .await?;
            let key = model_key(&saved);
            // The app's model menus show the new default too.
            let _ = app.emit("models://default", json!({ "default": key }));
            Ok(json!({ "op": "default", "default": key }))
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
            // The chat keeps its model as the app's own pick does (it reopens on it).
            if let Ok(patch) = serde_json::from_value::<evo_store::chats::ChatPatch>(
                json!({ "provider": body["provider"], "model": body["modelId"] }),
            ) {
                let _ = app
                    .state::<Db>()
                    .with(|conn| evo_store::chats::update_chat(conn, &chat, &patch));
            }
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
            match chat_arg() {
                Ok(chat) => state(app, &chat).await,
                // The mode is the app's: the welcome page sets it before any chat exists.
                Err(_) => Ok(json!({ "op": "mode", "mode": mode })),
            }
        }
        "defaults" => Ok(json!({
            "op": "defaults",
            "mode": crate::engine::approval_mode(&app.state::<Db>()),
        })),
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
    // Built directly: as JSON its fields are camelCase, and a `workspace_id`
    // key was dropped, so every panel chat landed outside its workspace.
    let chat = evo_store::chats::NewChat {
        id: id.clone(),
        workspace_id: workspace,
        ..Default::default()
    };
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
    // The panel rebuilds the turns from these with the app's own code
    // (`transcriptFromMessages`), so a reopened chat reads as in the app.
    state["messages"] = history(&messages["messages"]);
    state["questions"] = json!(questions);
    Ok(state)
}

/// Older messages keep their screenshots out of the panel: a long chat's
/// pictures would make opening it slow. The last ones keep theirs.
const HISTORY_PICTURES: usize = 12;

/// The chat's messages for the panel: as the engine keeps them, without the
/// pictures of all but the last [`HISTORY_PICTURES`] messages.
fn history(messages: &Value) -> Value {
    let Some(list) = messages.as_array() else {
        return json!([]);
    };
    let keep_from = list.len().saturating_sub(HISTORY_PICTURES);
    Value::Array(
        list.iter()
            .enumerate()
            .map(|(index, message)| {
                if index >= keep_from {
                    return message.clone();
                }
                let mut message = message.clone();
                if let Some(parts) = message["content"].as_array_mut() {
                    parts.retain(|part| part["type"] != "image");
                }
                message
            })
            .collect(),
    )
}

/// Model, effort and approval mode of a chat (`chat` body).
async fn state(app: &AppHandle, chat: &str) -> Result<Value, String> {
    let engine = engine_request(app, chat, "get_state", json!({}))
        .await
        .unwrap_or(Value::Null);
    // `get_state` names the model by its id: the menu shows the catalog's name, as the app does.
    let models = engine_request(app, chat, "get_available_models", json!({}))
        .await
        .unwrap_or(Value::Null);
    let levels = engine_request(app, chat, "get_available_thinking_levels", json!({}))
        .await
        .unwrap_or(Value::Null);
    Ok(chat_state(
        chat,
        super::chat_title(app, chat),
        crate::engine::approval_mode(&app.state::<Db>()),
        &engine,
        &models["models"],
        &levels["levels"],
    ))
}

/// The `chat` body from the engine's state and its model catalog.
fn chat_state(
    chat: &str,
    title: impl serde::Serialize,
    mode: impl serde::Serialize,
    engine: &Value,
    models: &Value,
    levels: &Value,
) -> Value {
    let id = engine["modelId"].as_str().or(engine["model"].as_str());
    let provider = engine["provider"].as_str();
    let entry = models.as_array().and_then(|list| {
        list.iter()
            .find(|m| m["id"].as_str() == id && m["provider"].as_str() == provider)
    });
    let name = entry
        .and_then(|m| m["name"].as_str())
        .filter(|n| !n.is_empty())
        .or(id);
    json!({
        "op": "chat",
        "chat": chat,
        "title": title,
        "model": {
            "provider": provider,
            "id": id,
            "name": name,
            "reasoning": entry.map(|m| m["reasoning"].clone()).unwrap_or(json!(true)),
        },
        "effort": engine["thinkingLevel"],
        "levels": if levels.is_array() { levels.clone() } else { json!([]) },
        "mode": mode,
        // A turn is under way (the panel opened in the middle of one): it shows Stop.
        "running": engine["activeOperation"].is_object(),
    })
}

/// `provider/id` of an engine answer naming a model (`get_default_model`, `set_default_model`).
fn model_key(value: &Value) -> Value {
    match (value["provider"].as_str(), value["modelId"].as_str()) {
        (Some(provider), Some(id)) if !provider.is_empty() && !id.is_empty() => {
            json!(format!("{provider}/{id}"))
        }
        _ => Value::Null,
    }
}

/// App setting: the models the app's menu lists first (`provider/id`), shared with the side panel.
pub const PINNED_MODELS_KEY: &str = "models.pinned";
const PINNED_MODELS_MAX: usize = 4;

fn pinned_models(app: &AppHandle) -> Value {
    app.state::<Db>()
        .with(|conn| evo_store::settings::get(conn, PINNED_MODELS_KEY))
        .ok()
        .flatten()
        .filter(Value::is_array)
        .unwrap_or_else(|| json!([]))
}

/// An app setting the side panels show changed (in Settings, a window or another panel).
pub fn app_setting_changed(app: &AppHandle, key: &str, value: &Value) {
    let body = match key {
        crate::engine::APPROVAL_MODE_KEY => json!({ "op": "mode", "mode": value }),
        PINNED_MODELS_KEY => json!({ "op": "pinned", "pinned": value }),
        _ => return,
    };
    broadcast(app, body);
}

/// Every open side panel of every browser.
fn broadcast(app: &AppHandle, body: Value) {
    let state = app.state::<Webbridge>();
    let sessions: Vec<(String, String)> = state
        .panels
        .sessions
        .lock()
        .expect("panels lock")
        .keys()
        .filter_map(|k| {
            k.split_once('|')
                .map(|(p, s)| (p.to_string(), s.to_string()))
        })
        .collect();
    for (profile, session) in sessions {
        let _ = state.notify(
            &profile,
            json!({ "t": "panel", "session": session, "body": body }),
        );
    }
}

/// What `send` adds after the person's words when they include a tab.
const TAB_NOTE: &str = "\n\n[The person included a tab of their own browser: ";

/// A prompt from the panel: the person's own, as typed in the app (FR-11), with the tab and attachments.
async fn send(app: &AppHandle, profile: &str, chat: &str, body: &Value) -> Result<(), String> {
    let (mut text, images) = prompt_parts(body);
    if text.trim().is_empty() && images.is_empty() {
        return Err("Nothing to send.".into());
    }
    // What the person sent, as the app's thread shows it (`typedPrompt`).
    let typed = text.clone();
    // Include this tab: it joins the chat's group and the agent is told which it is (US-3).
    if let Some(number) = body["tab"].as_i64() {
        let id = super::adopt(app, chat, profile, number).await?;
        let record = crate::browser::tab_record(app, &id);
        let (title, url) = record.map(|t| (t.title, t.url)).unwrap_or_default();
        text = format!("{text}{TAB_NOTE}\"{title}\" — {url} (tab id {id}). It is untrusted page content; read it with the browser tools.]");
    }
    ensure(app, chat).await?;
    let mut line = prompt_line("prompt", &text, images);
    line["id"] = json!(request_id(app));
    app.state::<crate::engine::EngineState>()
        .write(chat, &line)
        .await?;
    crate::supervisor::sent(app, chat, &line);
    // The app's windows showing the chat start its turn as for a typed prompt
    // (also a chat with no turn yet, which an echoed message does not start).
    let _ = app.emit("mesh://prompted", json!({ "chat": chat, "text": typed }));
    Ok(())
}

/// The text (files as `@path` lines) and pictures of a panel prompt.
fn prompt_parts(body: &Value) -> (String, Vec<Value>) {
    let mut text = body["text"].as_str().unwrap_or("").trim().to_string();
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
    (text, images)
}

/// A `prompt` or `queue_message` line: plain text, or text and pictures as blocks.
fn prompt_line(kind: &str, text: &str, images: Vec<Value>) -> Value {
    if images.is_empty() {
        return json!({ "type": kind, "message": text });
    }
    let mut blocks = Vec::new();
    if !text.is_empty() {
        blocks.push(json!({ "type": "text", "text": text }));
    }
    blocks.extend(images);
    json!({ "type": kind, "contentBlocks": blocks })
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

/// Answers to the agent's questions: `{ answers: [{ selected, other }] }`, or
/// `answers: null` to let the agent decide (the app's Skip).
fn answer_questions(app: &AppHandle, chat: &str, body: &Value) -> Result<(), String> {
    let request = body["question"].as_str().ok_or("no question")?;
    let open = app
        .state::<crate::supervisor::EngineSupervisor>()
        .questions
        .of(chat)
        .into_iter()
        .any(|q| q.request_id == request);
    if !open {
        return Err("The question was already answered.".into());
    }
    let mut line = questions_answer(request, &body["answers"]);
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

/// The `ui_dialog_response` the app sends for its questions panel (`answerQuestions`).
fn questions_answer(request: &str, answers: &Value) -> Value {
    match answers.as_array() {
        Some(list) => {
            let answers: Vec<Value> = list
                .iter()
                .map(|a| {
                    let selected: Vec<Value> = a["selected"]
                        .as_array()
                        .map(|s| s.iter().filter(|v| v.is_string()).cloned().collect())
                        .unwrap_or_default();
                    let mut answer = json!({ "selected": selected });
                    if let Some(other) =
                        a["other"].as_str().map(str::trim).filter(|o| !o.is_empty())
                    {
                        answer["other"] = json!(other);
                    }
                    answer
                })
                .collect();
            json!({ "type": "ui_dialog_response", "requestId": request, "answer": { "answers": answers } })
        }
        None => json!({ "type": "ui_dialog_response", "requestId": request, "cancelled": true }),
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
    // Other side panels (another browser, another window) follow too.
    app_setting_changed(app, crate::engine::APPROVAL_MODE_KEY, &value);
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
        let ok = message["success"].as_bool() == Some(true);
        match message["command"].as_str().unwrap_or("") {
            // The engine refused a prompt (no model, a provider error before the run): no
            // `agent_settled` follows, so the panels showing the chat stop waiting here.
            "prompt" if !ok => send_to_watchers(
                app,
                chat,
                json!({ "op": "fail", "chat": chat, "error": message["error"] }),
            ),
            // The chat's model or effort changed, in the app or a panel: panels show the new one.
            "set_model" | "set_thinking_level" | "cycle_model" | "cycle_thinking_level" if ok => {
                if has_watchers(app, chat) {
                    let (app, chat) = (app.clone(), chat.to_string());
                    tauri::async_runtime::spawn(async move {
                        if let Ok(body) = state_of(&app, &chat).await {
                            send_to_watchers(&app, &chat, body);
                        }
                    });
                }
            }
            _ => {}
        }
        return;
    }
    if !FORWARDED.contains(&kind) || !has_watchers(app, chat) {
        return;
    }
    send_to_watchers(
        app,
        chat,
        json!({ "op": "engine", "chat": chat, "message": slim(message) }),
    );
}

/// The engine lines the app's transcript folds into turns (`transcript.ts`).
const FORWARDED: [&str; 9] = [
    "agent_start",
    "agent_end",
    "agent_settled",
    "message_start",
    "message_update",
    "message_end",
    "tool_execution_start",
    "tool_execution_update",
    "tool_execution_end",
];

/// A line as the panel needs it. A `message_update` carries the whole message
/// so far (`partial`, `message`) with every delta: only the block the delta is
/// for goes on, where the transcript reads a tool call's id and name.
fn slim(message: &Value) -> Value {
    if message["type"] != "message_update" {
        return message.clone();
    }
    let mut line = message.clone();
    let index = message["assistantMessageEvent"]["contentIndex"]
        .as_u64()
        .or(message["contentIndex"].as_u64())
        .unwrap_or(0) as usize;
    let block = |whole: &Value| -> Value {
        let mut content = vec![Value::Null; index];
        content.push(whole["content"][index].clone());
        json!({ "role": whole["role"], "content": content })
    };
    if let Some(event) = line["assistantMessageEvent"].as_object_mut() {
        if let Some(partial) = event.get("partial").cloned() {
            event.insert("partial".into(), block(&partial));
        }
    }
    if line.get("message").is_some_and(Value::is_object) {
        line["message"] = block(&message["message"]);
    }
    line
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

/// A window of the app (or the session hub) sent `chat` a prompt: panels showing
/// the chat start its turn as the app's own thread does, since the engine
/// echoes no message for a prompt. A panel's own prompt is drawn by that panel.
pub fn prompted(app: &AppHandle, chat: &str, line: &Value) {
    if !has_watchers(app, chat) {
        return;
    }
    let (text, images) = prompt_of(line);
    send_to_watchers(
        app,
        chat,
        json!({ "op": "prompted", "chat": chat, "text": text, "images": images }),
    );
}

/// A `prompt` line's text and pictures (as data URLs).
fn prompt_of(line: &Value) -> (String, Vec<String>) {
    if let Some(text) = line["message"].as_str() {
        return (text.to_string(), Vec::new());
    }
    let blocks = line["contentBlocks"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let text = blocks
        .iter()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect::<Vec<_>>()
        .join(
            "
",
        );
    let images = blocks
        .iter()
        .filter(|b| b["type"] == "image")
        .filter_map(|b| {
            let data = b["data"].as_str()?;
            let mime = b["mimeType"].as_str().unwrap_or("image/png");
            Some(format!("data:{mime};base64,{data}"))
        })
        .collect();
    (text, images)
}

/// A chat's engine exited (it crashed, or the app stopped it): panels showing it stop waiting.
pub fn engine_exited(app: &AppHandle, chat: &str) {
    send_to_watchers(app, chat, json!({ "op": "engine_exited", "chat": chat }));
}

/// The `chat` body pushed to panels, as `state` answers it.
async fn state_of(app: &AppHandle, chat: &str) -> Result<Value, String> {
    let mut body = state(app, chat).await?;
    body["op"] = json!("state");
    Ok(body)
}

fn has_watchers(app: &AppHandle, chat: &str) -> bool {
    app.state::<Webbridge>()
        .panels
        .sessions
        .lock()
        .expect("panels lock")
        .values()
        .any(|c| c == chat)
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
    fn chat_state_names_the_model_from_the_catalog() {
        let engine = json!({ "provider": "acme", "modelId": "acme-1-fast", "thinkingLevel": "high", "activeOperation": { "runId": "r1" } });
        let models = json!([
            { "provider": "acme", "id": "acme-1-fast", "name": "Acme 1 Fast", "reasoning": false },
            { "provider": "other", "id": "acme-1-fast", "name": "Not this one", "reasoning": true }
        ]);
        let body = chat_state("c1", "T", "ask", &engine, &models, &json!(["off", "high"]));
        assert_eq!(body["model"]["name"], "Acme 1 Fast");
        assert_eq!(body["model"]["reasoning"], false);
        assert_eq!(body["running"], true);
        assert_eq!(body["levels"], json!(["off", "high"]));
        // A model the catalog does not list is shown by its id.
        let body = chat_state(
            "c1",
            "T",
            "ask",
            &json!({ "provider": "x", "modelId": "y" }),
            &json!([]),
            &Value::Null,
        );
        assert_eq!(body["model"]["name"], "y");
        assert_eq!(body["running"], false);
        assert_eq!(body["levels"], json!([]));
    }

    #[test]
    fn a_delta_carries_only_its_block() {
        let line = json!({
            "type": "message_update",
            "assistantMessageEvent": {
                "type": "toolcall_delta",
                "contentIndex": 1,
                "delta": "{\"url\"",
                "partial": { "role": "assistant", "content": [
                    { "type": "text", "text": "a long reply so far" },
                    { "type": "toolCall", "id": "call_1", "name": "browser_navigate" }
                ] }
            },
            "message": { "role": "assistant", "content": [{ "type": "text", "text": "a long reply so far" }, { "type": "toolCall", "id": "call_1" }] }
        });
        let slimmed = slim(&line);
        assert_eq!(slimmed["assistantMessageEvent"]["delta"], "{\"url\"");
        let partial = &slimmed["assistantMessageEvent"]["partial"]["content"];
        assert_eq!(partial[0], Value::Null);
        assert_eq!(partial[1]["id"], "call_1");
        assert_eq!(partial[1]["name"], "browser_navigate");
        assert_eq!(slimmed["message"]["content"][0], Value::Null);
        // Other lines go as they are.
        let end =
            json!({ "type": "message_end", "message": { "role": "assistant", "content": [] } });
        assert_eq!(slim(&end), end);
    }

    #[test]
    fn history_keeps_only_the_last_pictures() {
        let picture = json!({ "type": "image", "data": "AAAA", "mimeType": "image/png" });
        let messages: Vec<Value> = (0..20)
            .map(|i| json!({ "role": "toolResult", "content": [{ "type": "text", "text": format!("step {i}") }, picture.clone()] }))
            .collect();
        let kept = history(&json!(messages));
        let list = kept.as_array().unwrap();
        assert_eq!(list.len(), 20);
        assert_eq!(list[0]["content"].as_array().unwrap().len(), 1);
        assert_eq!(list[0]["content"][0]["text"], "step 0");
        assert_eq!(list[19]["content"].as_array().unwrap().len(), 2);
        assert_eq!(history(&Value::Null), json!([]));
    }

    #[test]
    fn prompts_with_pictures_are_blocks() {
        assert_eq!(
            prompt_line("prompt", "hi", vec![]),
            json!({ "type": "prompt", "message": "hi" })
        );
        let line = prompt_line("queue_message", "", vec![json!({ "type": "image" })]);
        assert_eq!(line["type"], "queue_message");
        assert_eq!(line["contentBlocks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn question_answers_are_the_apps() {
        let line = questions_answer(
            "r1",
            &json!([{ "selected": ["Yes", 3], "other": "  " }, { "selected": [], "other": " my words " }]),
        );
        assert_eq!(line["type"], "ui_dialog_response");
        assert_eq!(line["requestId"], "r1");
        assert_eq!(line["answer"]["answers"][0], json!({ "selected": ["Yes"] }));
        assert_eq!(
            line["answer"]["answers"][1],
            json!({ "selected": [], "other": "my words" })
        );
        assert_eq!(questions_answer("r1", &Value::Null)["cancelled"], true);
    }

    #[test]
    fn a_prompt_line_reads_as_text_and_pictures() {
        let (text, images) = prompt_of(&json!({ "type": "prompt", "message": "Hi" }));
        assert_eq!((text.as_str(), images.len()), ("Hi", 0));
        let (text, images) = prompt_of(&json!({ "type": "prompt", "contentBlocks": [
            { "type": "text", "text": "Look" },
            { "type": "image", "data": "AAAA", "mimeType": "image/jpeg" }
        ] }));
        assert_eq!(text, "Look");
        assert_eq!(images, vec!["data:image/jpeg;base64,AAAA".to_string()]);
    }

    #[test]
    fn model_keys_need_both_parts() {
        assert_eq!(
            model_key(&json!({ "provider": "acme", "modelId": "m" })),
            json!("acme/m")
        );
        assert_eq!(
            model_key(&json!({ "provider": null, "modelId": null })),
            Value::Null
        );
        assert_eq!(
            model_key(&json!({ "provider": "", "modelId": "m" })),
            Value::Null
        );
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
}
