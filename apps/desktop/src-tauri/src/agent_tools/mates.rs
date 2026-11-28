//! App tools only a mate's chat has (docs/features/evo-mates/spec.md, FR-34,
//! FR-40):
//!
//! - `mate_profile` keeps the answers to the mate's setup questions in its
//!   profile (its instructions), where the mate panel shows them and every
//!   later turn reads them;
//! - `connect_app` asks the person to connect, sign in to or switch on an app
//!   the mate needs, with a card naming the app and the reason, then waits
//!   until the app works for the mate. The mate never signs in by itself.
//!
//! - `request_secret` asks the person for a secret through a secure card
//!   (FR-43): the value goes to the mate's vault, never to the transcript
//!   or the model; the mate refers to it as `{{secret:NAME}}`.
//!
//! They are a [`Provider`]: offered to a chat only when it is a mate's.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use super::{BoxFuture, Provider, ToolCtx, ToolOutput};
use crate::db::Db;

pub const PROFILE_TOOL: &str = "mate_profile";
pub const CONNECT_TOOL: &str = "connect_app";
pub const SECRET_TOOL: &str = "request_secret";
pub const AGENDA_TOOL: &str = "mate_agenda";
/// A mate's reaction to a channel line (docs/features/evo-mates-messaging/spec.md, FR-4).
pub const REACT_TOOL: &str = "react";

/// The line that starts the setup answers in a mate's instructions.
pub const SETUP_MARK: &str = "Setup answers (from the person):";

/// The setup answers' labels, in order, with their argument names.
const FIELDS: [(&str, &str); 4] = [
    ("callYou", "Call them"),
    ("focus", "Focus on"),
    ("style", "How they like the work done"),
    ("apps", "Apps they use"),
];

/// How long `connect_app` waits for the app, at most.
const WAIT_LIMIT: Duration = Duration::from_secs(15 * 60);

pub struct MateTools;

fn mate_of(app: &AppHandle, chat: &str) -> Option<evo_store::mates::Mate> {
    app.try_state::<Db>()?
        .with(|conn| evo_store::mates::mate_of_chat(conn, chat))
        .ok()
        .flatten()
}

/// `react`, offered in a seat (a mate's place in a channel) only.
fn react_definition() -> Value {
    json!({
        "name": REACT_TOOL,
        "description": "React to a line of this channel instead of writing a reply: acknowledge an assignment (like), flag a doubt (question), thank (heart). Name the line by the id in brackets before it in your recent lines. A reaction wakes nobody and is not a hand-off; its author hears of it as information. One reaction per line: a new one replaces yours, null removes it. Never react to your own lines.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "line": { "type": "string", "description": "The line's id, e.g. \"r-19c1a2b3c4d-7\"." },
                "reaction": { "type": ["string", "null"], "enum": ["like", "heart", "laugh", "emphasis", "question", "dislike", null] }
            },
            "required": ["line", "reaction"],
            "additionalProperties": false
        }
    })
}

/// Whether `chat` is a seat.
fn is_seat(app: &AppHandle, chat: &str) -> bool {
    app.try_state::<Db>()
        .and_then(|db| db.with(|conn| evo_store::chats::get_chat(conn, chat)).ok().flatten())
        .is_some_and(|row| row.kind == "seat")
}

fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": PROFILE_TOOL,
            "description": "Save the person's answers to your setup questions to your profile, where they see them and \
        you read them on every turn. Call it once they answered; leave out what they skipped. A later call changes only \
        the answers it gives.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Your own name, when the person gives you one, or the short name you choose while you are still called \"New mate\": you are renamed to it." },
                    "callYou": { "type": "string", "description": "What the person wants to be called." },
                    "focus": { "type": "string", "description": "What to focus on." },
                    "style": { "type": "string", "description": "How they like the work done." },
                    "apps": { "type": "array", "items": { "type": "string" }, "description": "The apps they use for this work." },
                    "role": { "type": "string", "description": "What you do, in one line, only while you are still called \"New mate\"." }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": CONNECT_TOOL,
            "description": "Ask the person to connect an app you need (mail, calendar, a drive…) that is not connected, \
        whose sign-in expired, or that is switched off for you. Shows a card with the app and your reason and waits until the \
        app works. Never sign in yourself or ask for a password in the chat.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app": { "type": "string", "description": "The app, as the person knows it, e.g. \"Gmail\"." },
                    "reason": { "type": "string", "description": "Why you need it, in one short line." }
                },
                "required": ["app", "reason"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": SECRET_TOOL,
            "description": "Ask the person for a secret (an API key, a token, a password a tool needs) through a secure card. The value is kept in your vault; you never see it. Use it as {{secret:NAME}} in a connector tool's arguments: the value is put in only inside that call. Never ask for a secret in the chat itself.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "A short name for it, e.g. \"GITHUB_TOKEN\"." },
                    "service": { "type": "string", "description": "The service it is for, e.g. \"GitHub\"." },
                    "reason": { "type": "string", "description": "Why you need it, in one short line." }
                },
                "required": ["name", "service", "reason"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": AGENDA_TOOL,
            "description": "Your agenda of things to come back to (evo-mates FR-63): add what you promised to do, events the person mentioned with their time, and questions left open, so you can follow up at the right moment; mark an item done when it is settled. A check-in the person asks for (\"every weekday at 9\") is an item with `repeat`. Never put secrets in it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["add", "done", "list"] },
                    "kind": { "type": "string", "enum": ["promise", "event", "question", "checkin"], "description": "With add." },
                    "text": { "type": "string", "description": "With add: what to come back to, in one short line." },
                    "due": { "type": "string", "description": "With add, when to come back to it: an ISO date and time in local time, e.g. \"2026-11-27T11:00\". For an event, a little after it ends. Use `list` first if you are not sure of today's date: it says the time now." },
                    "repeat": {
                        "type": "object",
                        "description": "With add, for a check-in: {\"kind\": \"daily\" | \"weekdays\" | \"weekly\", \"time\": \"09:00\", \"days\": [1] (weekly: 0 Sun - 6 Sat)}.",
                    },
                    "id": { "type": "string", "description": "With done: the item's id (from list)." }
                },
                "required": ["action"],
                "additionalProperties": false
            }
        }),
    ]
}

impl Provider for MateTools {
    fn definitions_for(
        &self,
        app: AppHandle,
        chat: String,
        _folder: Option<PathBuf>,
    ) -> BoxFuture<Vec<Value>> {
        Box::pin(async move {
            if mate_of(&app, &chat).is_some() {
                let mut all = definitions();
                if is_seat(&app, &chat) {
                    all.push(react_definition());
                }
                all
            } else {
                Vec::new()
            }
        })
    }

    fn claims(&self, name: &str) -> bool {
        name == PROFILE_TOOL || name == CONNECT_TOOL || name == SECRET_TOOL || name == AGENDA_TOOL || name == REACT_TOOL
    }

    fn run(&self, ctx: ToolCtx, name: String, args: Value) -> BoxFuture<ToolOutput> {
        Box::pin(async move {
            let Some(mate) = mate_of(&ctx.app, &ctx.chat) else {
                return ToolOutput::error(format!("{name} is only for a mate's own conversation."));
            };
            match name.as_str() {
                PROFILE_TOOL => save_profile(&ctx, &mate, &args).await,
                AGENDA_TOOL => agenda(&ctx, &mate, &args),
                REACT_TOOL => {
                    let line = args.get("line").and_then(Value::as_str).unwrap_or_default();
                    let reaction = args.get("reaction").and_then(Value::as_str);
                    match crate::mates::reactions::react_from_seat(&ctx.app, &ctx.chat, &mate.id, line, reaction).await {
                        Ok(text) => ToolOutput::text(text),
                        Err(error) => ToolOutput::error(error),
                    }
                }
                SECRET_TOOL => request_secret(&ctx, &mate, &args).await,
                _ => connect(&ctx, &mate, &args).await,
            }
        })
    }
}

// ── mate_profile ────────────────────────────────────────────────────────────

/// The answers in `args`, by argument name; a list joins with commas.
fn answers(args: &Value) -> Vec<(&'static str, String)> {
    FIELDS
        .iter()
        .filter_map(|(key, _)| {
            let value = match args.get(*key)? {
                Value::String(text) => text.trim().to_string(),
                Value::Array(items) => items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => return None,
            };
            // One line each: the block is read back line by line.
            let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
            (!value.is_empty()).then_some((*key, value))
        })
        .collect()
}

/// `instructions` with the setup answers block holding `given` over the
/// answers it had: the block goes last, after the person's own text.
pub fn with_setup(instructions: &str, given: &[(&str, String)]) -> String {
    let (own, block) = match instructions.find(SETUP_MARK) {
        Some(at) => (&instructions[..at], &instructions[at + SETUP_MARK.len()..]),
        None => (instructions, ""),
    };
    let mut kept: Vec<(&str, String)> = FIELDS
        .iter()
        .filter_map(|(key, label)| {
            let prefix = format!("- {label}: ");
            block.lines().find_map(|line| {
                line.trim()
                    .strip_prefix(&prefix)
                    .map(|v| (*key, v.trim().to_string()))
            })
        })
        .collect();
    for (key, value) in given {
        match kept.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value.clone(),
            None => kept.push((key, value.clone())),
        }
    }
    let lines: Vec<String> = FIELDS
        .iter()
        .filter_map(|(key, label)| {
            kept.iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| format!("- {label}: {v}"))
        })
        .collect();
    let own = own.trim_end();
    if lines.is_empty() {
        return own.to_string();
    }
    let gap = if own.is_empty() { "" } else { "\n\n" };
    format!("{own}{gap}{SETUP_MARK}\n{}", lines.join("\n"))
}

/// A name the person gives a mate, at most.
const NAME_CHARS: usize = 40;

/// A role a new mate gives itself, at most (FR-78).
const ROLE_CHARS: usize = 120;

/// What `mate_profile` changes: the setup answers in the instructions; its
/// name, when the person named the mate or a new mate names itself (FR-34,
/// FR-78); and its one-line role, only while it is still called "New mate",
/// so nothing a named mate reads later changes what it is for (FR-78).
fn profile_patch(current: &str, instructions: &str, args: &Value) -> Result<evo_store::mates::MatePatch, String> {
    let given = answers(args);
    let line = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|text| !text.is_empty())
    };
    let name = line("name")
        .map(|name| name.chars().take(NAME_CHARS).collect::<String>().trim().to_string())
        .filter(|name| !name.is_empty());
    let role = line("role");
    if role.is_some() && current != crate::mates::SCRATCH_NAME {
        return Err(format!("You are already called {current}: only the person changes your role, in your panel."));
    }
    if role.as_ref().is_some_and(|r| r.chars().count() > ROLE_CHARS) {
        return Err("Keep your role to one short line: 120 characters at most.".into());
    }
    if given.is_empty() && name.is_none() && role.is_none() {
        return Err("Nothing to save: give at least one answer.".into());
    }
    Ok(evo_store::mates::MatePatch {
        name,
        role,
        instructions: (!given.is_empty()).then(|| with_setup(instructions, &given)),
        ..Default::default()
    })
}

async fn save_profile(ctx: &ToolCtx, mate: &evo_store::mates::Mate, args: &Value) -> ToolOutput {
    let patch = match profile_patch(&mate.name, &mate.instructions, args) {
        Ok(patch) => patch,
        Err(error) => return ToolOutput::error(error),
    };
    match crate::mates::mates_update(ctx.app.clone(), mate.id.clone(), patch).await {
        Ok(Some(_)) => ToolOutput::text(
            "Saved to your profile. The person sees it in your panel's About tab; you read it from your next turn.",
        ),
        Ok(None) => ToolOutput::error("Your profile is gone."),
        Err(error) => ToolOutput::error(format!("Could not save your profile: {error}")),
    }
}

// ── request_secret ──────────────────────────────────────────────────────────

async fn request_secret(ctx: &ToolCtx, mate: &evo_store::mates::Mate, args: &Value) -> ToolOutput {
    use crate::mates::secrets;
    let text = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let Some(name) = secrets::clean_name(&text("name")) else {
        return ToolOutput::error("Give the secret a short name, e.g. GITHUB_TOKEN.");
    };
    let service = text("service");
    let reason = text("reason");
    let title = if service.is_empty() {
        format!("{} needs a secret", mate.name)
    } else {
        format!("{} needs your {service} secret", mate.name)
    };
    let description = if reason.is_empty() {
        String::new()
    } else {
        format!("{} needs it to {reason}", mate.name)
    };
    // The window draws a masked field; its value comes back through
    // `mates_secret_submit`, not with the answer.
    let ui = json!({ "kind": "secret", "name": name, "service": service, "reason": reason });
    let answer = ctx
        .ask_with(&title, &description, &["Save", "Cancel"], ui)
        .await;
    let staged = secrets::take_staged(&ctx.chat, &name);
    if answer < 0 {
        return ToolOutput::text("");
    }
    let Some(value) = staged.filter(|_| answer == 0) else {
        return ToolOutput::text(format!(
            "The person did not give {name}. Go on without it, or ask them what to do."
        ));
    };
    let mate_id = mate.id.clone();
    let key = name.clone();
    let saved = tokio::task::spawn_blocking(move || secrets::save(&mate_id, &key, &value)).await;
    match saved {
        Ok(Ok(())) => ToolOutput::text(format!(
            "Saved as {name} in your vault. Use it as {{{{secret:{name}}}}} in a connector tool's arguments; you never see its value."
        )),
        Ok(Err(error)) => ToolOutput::error(format!("Could not keep {name}: {error}")),
        Err(error) => ToolOutput::error(format!("Could not keep {name}: {error}")),
    }
}

// ── mate_agenda ─────────────────────────────────────────────────────────────

fn when(ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|t| t.format("%a %d %b %H:%M").to_string())
        .unwrap_or_default()
}

fn local_now() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M (%a)").to_string()
}

fn agenda(ctx: &ToolCtx, mate: &evo_store::mates::Mate, args: &Value) -> ToolOutput {
    use crate::agent_tools::scheduler::schedule::{describe_schedule, next_run, parse_moment, schedule_from};
    use evo_store::agenda;
    let db = ctx.app.state::<Db>();
    let text = |key: &str| args.get(key).and_then(Value::as_str).map(str::trim).unwrap_or_default().to_string();
    let changed = || {
        let _ = ctx.app.emit("mates://changed", json!({}));
    };
    match text("action").as_str() {
        "add" => {
            let kind = text("kind");
            let what = text("text");
            let now = evo_store::chats::now_ms();
            let (due, repeat, said) = match args.get("repeat").filter(|r| r.is_object()) {
                Some(repeat) => {
                    let schedule = match schedule_from(repeat) {
                        Ok(schedule) => schedule,
                        Err(error) => return ToolOutput::error(format!("That repeat cannot be read: {error}")),
                    };
                    let Some(next) = next_run(&schedule, now) else {
                        return ToolOutput::error("That repeat never comes round.");
                    };
                    let json = serde_json::to_string(&schedule).unwrap_or_default();
                    (next, Some(json), describe_schedule(&schedule))
                }
                None => match parse_moment(&text("due")) {
                    // A time long past is a wrong date: say what now is.
                    Some(due) if due < now - 6 * 3_600_000 => {
                        return ToolOutput::error(format!(
                            "That time is in the past. It is now {} (local time); give `due` again as an ISO date and time.",
                            local_now()
                        ))
                    }
                    Some(due) => (due, None, when(due)),
                    None => {
                        return ToolOutput::error(format!(
                            "Give `due` as an ISO date and time; it is now {} (local time).",
                            local_now()
                        ))
                    }
                },
            };
            let kind = if repeat.is_some() { "checkin".to_string() } else { kind };
            let item = agenda::NewItem {
                id: crate::mates::new_id("agenda"),
                mate_id: mate.id.clone(),
                kind,
                text: what,
                due_at: due,
                repeat,
                chat_id: Some(ctx.chat.clone()),
            };
            match db.with(|conn| agenda::add_item(conn, &item)) {
                Ok(Some(item)) => {
                    changed();
                    ToolOutput::text(format!(
                        "On your agenda ({}): \"{}\" — {said}. Id {}. It is now {}.",
                        item.kind,
                        item.text,
                        item.id,
                        local_now()
                    ))
                }
                Ok(None) => ToolOutput::text("The person asked you never to follow up on that: it was not added."),
                Err(error) => ToolOutput::error(format!("Could not add it: {error}")),
            }
        }
        "done" => match db.with(|conn| agenda::set_status(conn, &mate.id, &text("id"), "done")) {
            Ok(true) => {
                changed();
                ToolOutput::text("Done: it is off your agenda.")
            }
            Ok(false) => ToolOutput::error("No such item on your agenda; list them first."),
            Err(error) => ToolOutput::error(error),
        },
        "list" => match db.with(|conn| agenda::list_items(conn, &mate.id, false)) {
            Ok(items) if items.is_empty() => ToolOutput::text(format!("Your agenda is empty. It is now {}.", local_now())),
            Ok(items) => ToolOutput::text(
                items
                    .iter()
                    .map(|i| format!("- {} ({}, {}): {}{}", i.id, i.kind, when(i.due_at), i.text, if i.status == "paused" { " — paused" } else { "" }))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            Err(error) => ToolOutput::error(error),
        },
        _ => ToolOutput::error("`action` is add, done or list."),
    }
}

// ── connect_app ─────────────────────────────────────────────────────────────

/// Letters and digits only, lower case: "Google Drive" and "google-drive" match.
fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The connector `app` names in the Connectors' `servers`: by its key, name
/// or title, else one whose name holds it (or is held by it).
pub fn find_server<'a>(servers: &'a [Value], app: &str) -> Option<&'a Value> {
    let wanted = plain(app);
    if wanted.is_empty() {
        return None;
    }
    let names = |server: &Value| -> Vec<String> {
        [
            server.get("key"),
            server.get("name"),
            server.pointer("/server/title"),
        ]
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(plain)
        .filter(|name| !name.is_empty())
        .collect()
    };
    servers
        .iter()
        .find(|server| names(server).iter().any(|name| *name == wanted))
        .or_else(|| {
            servers.iter().find(|server| {
                names(server)
                    .iter()
                    .any(|name| name.contains(&wanted) || wanted.contains(name.as_str()))
            })
        })
}

/// Where an app stands for a mate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// It works: connected (or asleep, which a call wakes).
    Ready,
    /// Its sign-in is missing or expired.
    SignIn(String),
    /// It is turned off in Connectors, or stopped: it needs turning on.
    TurnOn(String),
    /// Connected, but switched off for this mate (its Apps tab).
    OffForMate(String),
    /// Evo has no such connector.
    Missing,
}

/// Where `app` stands among `servers` for a chat with `off` switched off.
pub fn standing(servers: &[Value], app: &str, off: &std::collections::HashSet<String>) -> Standing {
    let Some(server) = find_server(servers, app) else {
        return Standing::Missing;
    };
    let key = server
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let status = server
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match status {
        "needs-auth" => Standing::SignIn(key),
        "connected" | "sleeping" | "connecting" if off.contains(&key) => Standing::OffForMate(key),
        "connected" | "sleeping" => Standing::Ready,
        _ => Standing::TurnOn(key),
    }
}

fn servers(app: &AppHandle) -> Vec<Value> {
    let Some(state) = app.try_state::<crate::mcp::McpState>() else {
        return Vec::new();
    };
    state
        .0
        .snapshot()
        .get("servers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn title_of(servers: &[Value], key: &str, fallback: &str) -> String {
    servers
        .iter()
        .find(|s| s.get("key").and_then(Value::as_str) == Some(key))
        .and_then(|s| {
            s.pointer("/server/title")
                .or_else(|| s.get("name"))
                .and_then(Value::as_str)
        })
        .unwrap_or(fallback)
        .to_string()
}

/// Open Evo Harness at Customize → Connectors. A main window built just now
/// reads the request once it has loaded.
pub(crate) fn open_connectors(app: &AppHandle) {
    let built = crate::system::show_main(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if built {
            for _ in 0..50 {
                if app.get_window("main").is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        let _ = app.emit_to(
            "main",
            "app://open-customize",
            json!({ "section": "connectors" }),
        );
    });
}

async fn connect(ctx: &ToolCtx, mate: &evo_store::mates::Mate, args: &Value) -> ToolOutput {
    let asked = args
        .get("app")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let reason = args
        .get("reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if asked.is_empty() {
        return ToolOutput::error("Name the app to connect.");
    }
    // A connected-apps service (Gmail, Slack…): its own card (connected-apps FR-11).
    if let Some(output) = crate::connected_apps::run::connect_named(ctx, mate, &asked, &reason).await {
        return output;
    }
    let now = |ctx: &ToolCtx| {
        standing(
            &servers(&ctx.app),
            &asked,
            &super::mcp::switched_off(&ctx.chat),
        )
    };
    let first = now(ctx);
    let list = servers(&ctx.app);
    let (name, action, what) = match &first {
        Standing::Ready => {
            return ToolOutput::text(format!(
                "{asked} is already connected for you: go on with its tools."
            ));
        }
        Standing::SignIn(key) => {
            let name = title_of(&list, key, &asked);
            (
                name.clone(),
                "Reconnect",
                format!("{name}'s sign-in is missing or has expired."),
            )
        }
        Standing::TurnOn(key) => {
            let name = title_of(&list, key, &asked);
            (
                name.clone(),
                "Turn on",
                format!("{name} is turned off or stopped."),
            )
        }
        Standing::OffForMate(key) => {
            let name = title_of(&list, key, &asked);
            (
                name.clone(),
                "Open Apps",
                format!("{name} is switched off for {}.", mate.name),
            )
        }
        Standing::Missing => (
            asked.clone(),
            "Connect",
            format!("{asked} is not connected to Evo yet."),
        ),
    };
    let why = if reason.is_empty() {
        String::new()
    } else {
        format!(" {} needs it to {}.", mate.name, reason.trim_end_matches('.'))
    };
    let title = format!("{action} {name}");
    let description = format!("{what}{why} {} never signs in by itself.", mate.name);
    let picked = ctx.ask(&title, &description, &[action, "Not now"]).await;
    if picked < 0 {
        return ToolOutput::text("");
    }
    if picked != 0 {
        return ToolOutput::text(format!(
            "The person chose not to connect {name} now. Go on without it, or ask them what to do instead; do not ask to connect it again in this turn."
        ));
    }
    match &first {
        Standing::SignIn(key) => {
            let pool = ctx.app.state::<crate::mcp::McpState>().0.clone();
            let app = ctx.app.clone();
            let key = key.clone();
            tauri::async_runtime::spawn(async move {
                use tauri_plugin_opener::OpenerExt;
                let opener = app.clone();
                let _ = pool
                    .sign_in(&key, move |url| {
                        let _ = opener.opener().open_url(url, None::<&str>);
                    })
                    .await;
            });
        }
        Standing::TurnOn(key) => {
            let pool = ctx.app.state::<crate::mcp::McpState>().0.clone();
            let key = key.clone();
            tauri::async_runtime::spawn(async move {
                let _ = pool.set_enabled(&key, true).await;
                pool.connect(&key).await;
            });
        }
        Standing::OffForMate(_) => {
            let _ = ctx.app.emit(
                "mates://open-panel",
                json!({ "chat": ctx.chat, "tab": "apps" }),
            );
        }
        Standing::Missing => open_connectors(&ctx.app),
        Standing::Ready => {}
    }
    // Wait for the app, with a card the person can cancel; it shows the mate
    // as needing them meanwhile.
    let waiting = ToolCtx {
        cancel: ctx.cancel.child_token(),
        ..ctx.clone()
    };
    let hint = match &first {
        Standing::SignIn(_) => format!("Finish signing in to {name} in your browser."),
        Standing::OffForMate(_) => format!("Switch {name} on in {}'s Apps tab.", mate.name),
        Standing::Missing => format!("Add {name} in Evo Harness → Customize → Connectors."),
        _ => format!("{name} is starting."),
    };
    let card = waiting.clone();
    let title = format!("Waiting for {name}");
    let description = format!("{hint} {} goes on once it's connected.", mate.name);
    let ask = card.ask(&title, &description, &["Cancel"]);
    tokio::pin!(ask);
    let started = std::time::Instant::now();
    loop {
        tokio::select! {
            answer = &mut ask => {
                if answer < 0 && ctx.cancelled() {
                    return ToolOutput::text("");
                }
                return ToolOutput::text(format!("The person stopped connecting {name}. Go on without it, or ask them what to do."));
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                if now(ctx) == Standing::Ready {
                    waiting.cancel.cancel();
                    return ToolOutput::text(format!("{name} is connected: go on with its tools."));
                }
                if started.elapsed() > WAIT_LIMIT {
                    waiting.cancel.cancel();
                    return ToolOutput::text(format!("{name} was not connected within 15 minutes. Tell the person and go on without it."));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn server(key: &str, title: &str, status: &str) -> Value {
        json!({ "key": key, "name": key, "status": status, "server": { "title": title } })
    }

    #[test]
    fn a_new_mate_names_itself_and_sets_its_role_only_while_it_is_new() {
        let scratch = crate::mates::SCRATCH_NAME;
        let patch = profile_patch(scratch, "", &json!({ "name": "  Penny ", "role": "Keeps receipts\nand spending in order" })).unwrap();
        assert_eq!(patch.name.as_deref(), Some("Penny"));
        assert_eq!(patch.role.as_deref(), Some("Keeps receipts and spending in order"));
        assert!(profile_patch("Penny", "", &json!({ "role": "Something else" })).unwrap_err().contains("already"), "a named mate keeps its role");
        assert_eq!(profile_patch("Penny", "", &json!({ "name": "Eve" })).unwrap().name.as_deref(), Some("Eve"), "the person may still name it (FR-34)");
        assert!(profile_patch(scratch, "", &json!({ "role": "x".repeat(121) })).is_err(), "a short role");
    }

    #[test]
    fn setup_answers_go_last_and_merge() {
        let given = vec![
            ("focus", "the Q3 report".to_string()),
            ("callYou", "Hung".to_string()),
        ];
        let first = with_setup("Be brief.", &given);
        assert_eq!(
            first,
            format!("Be brief.\n\n{SETUP_MARK}\n- Call them: Hung\n- Focus on: the Q3 report")
        );
        let second = with_setup(
            &first,
            &[
                ("focus", "hiring".to_string()),
                ("apps", "Gmail, Drive".to_string()),
            ],
        );
        assert_eq!(
            second,
            format!("Be brief.\n\n{SETUP_MARK}\n- Call them: Hung\n- Focus on: hiring\n- Apps they use: Gmail, Drive")
        );
        assert_eq!(
            with_setup("", &[("style", "short".to_string())]),
            format!("{SETUP_MARK}\n- How they like the work done: short")
        );
    }

    // docs/features/evo-mates/spec.md (FR-34): the person names the mate in
    // setup; found playing a first-time user (qa-2026-11-28, A3).
    #[test]
    fn the_name_the_person_gives_renames_the_mate() {
        let patch = profile_patch("Writer", "Be brief.", &json!({ "name": "  Mai ", "focus": "planning" })).unwrap();
        assert_eq!(patch.name.as_deref(), Some("Mai"));
        assert!(patch.instructions.as_deref().is_some_and(|text| text.contains("- Focus on: planning")));
        // A name alone is enough; nothing at all is not.
        let named = profile_patch("Writer", "Be brief.", &json!({ "name": "Mai" })).unwrap();
        assert_eq!((named.name.as_deref(), named.instructions), (Some("Mai"), None));
        assert!(profile_patch("Writer", "Be brief.", &json!({ "name": "  " })).is_err());
        // A name is one short line.
        let long = profile_patch("Writer", "", &json!({ "name": "Mai\nthe very long name of a mate that goes on and on and on and on" })).unwrap();
        assert_eq!(long.name.as_deref().map(|n| n.chars().count() <= 40 && !n.contains('\n')), Some(true));
    }

    #[test]
    fn answers_are_one_line_each() {
        let args = json!({ "focus": "  two\nlines ", "apps": ["Gmail", " ", "Calendar"], "style": "" , "other": "x" });
        assert_eq!(
            answers(&args),
            vec![
                ("focus", "two lines".to_string()),
                ("apps", "Gmail, Calendar".to_string())
            ]
        );
    }

    #[test]
    fn an_app_is_found_by_key_name_or_title() {
        let servers = vec![
            server("google-drive", "Google Drive", "connected"),
            server("gmail", "Gmail", "needs-auth"),
        ];
        assert_eq!(
            find_server(&servers, "Google Drive").unwrap()["key"],
            "google-drive"
        );
        assert_eq!(find_server(&servers, "GMAIL").unwrap()["key"], "gmail");
        assert_eq!(
            find_server(&servers, "drive").unwrap()["key"],
            "google-drive"
        );
        assert!(find_server(&servers, "Slack").is_none());
        assert!(find_server(&servers, " - ").is_none());
    }

    #[test]
    fn where_an_app_stands() {
        let servers = vec![
            server("gmail", "Gmail", "needs-auth"),
            server("drive", "Drive", "connected"),
            server("notes", "Notes", "off"),
            server("cal", "Calendar", "sleeping"),
        ];
        let none = HashSet::new();
        assert_eq!(
            standing(&servers, "gmail", &none),
            Standing::SignIn("gmail".into())
        );
        assert_eq!(standing(&servers, "drive", &none), Standing::Ready);
        assert_eq!(standing(&servers, "calendar", &none), Standing::Ready);
        assert_eq!(
            standing(&servers, "notes", &none),
            Standing::TurnOn("notes".into())
        );
        assert_eq!(standing(&servers, "slack", &none), Standing::Missing);
        let off: HashSet<String> = ["drive".to_string()].into();
        assert_eq!(
            standing(&servers, "drive", &off),
            Standing::OffForMate("drive".into())
        );
    }
}
