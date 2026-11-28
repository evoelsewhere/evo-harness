//! Evo Mates' mates in the shell (docs/features/evo-mates/spec.md, FR-6 –
//! FR-13, FR-25, FR-34; plan 3.2, 3.5).
//!
//! * A mate is a row in `mates` and one direct chat of kind `mate`
//!   (`evo_store::mates`). Its folder `<agent dir>/mates/<id>/work` is where
//!   it works unless it has a project folder; `<agent dir>/mates/<id>/memory`
//!   is its own memory.
//! * Its profile reaches its engine as a role (`chats.role`, written to
//!   `--role-file` at every start): its name, instructions and the mate's
//!   conduct, tools, model and thinking, who it is and its memory folder.
//!   A change rewrites the role and restarts the engine when it is idle, so
//!   it applies from the next turn (FR-10); a working engine restarts once
//!   its work settles ([`apply_pending`]).
//! * Gallery roles are Markdown files in `templates/` ([`template`]).

pub mod badge;
pub mod computer;
pub mod mentions;
pub mod reactions;
pub mod link_preview;
pub mod channels;
pub mod files;
pub mod grants;
pub mod initiative;
pub mod knowledge;
pub mod limit;
pub mod team_file;
pub mod teams;
pub mod secrets;
pub mod spend;
pub mod template;
pub mod today;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use evo_store::logs::NewLog;
use evo_store::mates::{self, Mate, MatePatch, NewMate};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::db::Db;

/// Mates whose profile changed while they worked: restart when idle.
#[derive(Default)]
pub struct MatesState {
    pending: Mutex<HashSet<String>>,
}

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `<agent dir>/mates/<id>`.
pub fn mate_dir(id: &str) -> PathBuf {
    crate::mcp::agent_dir().join("mates").join(id)
}

pub(crate) fn new_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = evo_store::chats::now_ms();
    format!("{prefix}-{now:x}{:04x}{n:x}", std::process::id() & 0xffff)
}

/// Who a mate is and what it does, from its profile, for its role's
/// instructions. How every mate behaves is the engine's
/// (`evo_engine::session_roles::MATE_CONDUCT`, FR-46 - FR-48), added after
/// these for any role that names a mate.
///
/// They end with the mate's Knowledge files, when it has any (FR-77).
pub fn instructions(mate: &Mate) -> String {
    let dir = knowledge::dir_of(&mate.id);
    instructions_with(mate, knowledge::block(&dir, &knowledge::list_in(&dir)))
}

/// [`instructions`] with `knowledge` (the Knowledge block) after the mate's own text.
pub fn instructions_with(mate: &Mate, knowledge: Option<String>) -> String {
    let role = if mate.role.trim().is_empty() { "a helpful teammate".to_string() } else { mate.role.trim().to_string() };
    let own = mate.instructions.trim();
    let own = if own.is_empty() { String::new() } else { format!("\n\n{own}") };
    let knowledge = knowledge.map(|text| format!("\n\n{text}")).unwrap_or_default();
    format!(
        "You are {name}, one of the person's mates in Evo Mates: an AI teammate with a role of your own. Your role: {role}.{own}{knowledge}",
        name = mate.name.trim(),
    )
}

/// The role a mate's direct chat runs as (`SessionRole`).
pub fn role_of(mate: &Mate) -> Value {
    let memory = mate_dir(&mate.id).join("memory");
    let mut role = json!({
        "name": mate.name,
        "source": "mate",
        "instructions": instructions(mate),
        "access": "full",
        "mate": { "id": mate.id, "name": mate.name, "role": mate.role },
        "memoryDir": memory.display().to_string(),
        // Its learned skills (docs/features/learning-layers/spec.md, FR-15).
        "learnedSkillsDir": mate_dir(&mate.id).join("skills").display().to_string(),
    });
    let object = role.as_object_mut().expect("role is an object");
    if let Some(tools) = &mate.tools {
        object.insert("tools".into(), json!(tools));
    }
    // Its skill switches: only the skills it names (FR-44).
    if let Some(skills) = &mate.skills {
        object.insert("skills".into(), json!(skills));
    }
    if let (Some(provider), Some(model)) = (&mate.provider, &mate.model) {
        object.insert("provider".into(), json!(provider));
        object.insert("model".into(), json!(model));
    }
    if let Some(thinking) = &mate.thinking {
        object.insert("thinking".into(), json!(thinking));
    }
    role
}

pub(crate) fn changed(app: &AppHandle) {
    let _ = app.emit("mates://changed", json!({}));
}

fn write_role(db: &Db, mate: &Mate) -> Result<(), String> {
    let role = role_of(mate);
    db.with(|conn| evo_store::chats::set_role(conn, &mate.chat_id, Some(&role)).map(|_| ()))
}

/// Stop `chat`'s engine on purpose and tell the windows (`engine://parked`):
/// a window that shows the chat keeps its messages and starts the engine
/// again with the next one, which reads the mate's role afresh.
pub async fn park(app: &AppHandle, chat: &str) {
    if app.state::<crate::engine::EngineState>().stop(chat).await {
        let _ = app.emit("engine://parked", json!({ "chat": chat }));
    }
}

/// Restart `chat`'s engine so it reads its role again: now when it is idle,
/// else once its work settles.
pub(crate) async fn restart_when_idle(app: &AppHandle, chat: &str) {
    let host = crate::supervisor::AppHost(app.clone());
    let working = app.state::<crate::supervisor::EngineSupervisor>().working(&host, chat);
    if working {
        locked(&app.state::<MatesState>().pending).insert(chat.to_string());
    } else {
        park(app, chat).await;
    }
}

/// Engines whose mate changed while they worked restart once idle (FR-10).
/// Called when the chats with work change.
pub fn apply_pending(app: &AppHandle) {
    let pending: Vec<String> = locked(&app.state::<MatesState>().pending).iter().cloned().collect();
    if pending.is_empty() {
        return;
    }
    let host = crate::supervisor::AppHost(app.clone());
    let supervisor = app.state::<crate::supervisor::EngineSupervisor>();
    let idle: Vec<String> = pending.into_iter().filter(|chat| !supervisor.working(&host, chat)).collect();
    if idle.is_empty() {
        return;
    }
    {
        let state = app.state::<MatesState>();
        let mut set = locked(&state.pending);
        for chat in &idle {
            set.remove(chat);
        }
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for chat in idle {
            park(&app, &chat).await;
        }
    });
}

/// A mate made from scratch is called this until it, or the person, names it
/// (FR-78: it may name itself only while it has this name).
pub const SCRATCH_NAME: &str = "New mate";

/// What the Mates window asks to create: a gallery role, from scratch, or a copy.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMate {
    /// A gallery role's id.
    pub template: Option<String>,
    pub name: Option<String>,
    pub avatar: Option<Value>,
    pub role: Option<String>,
    pub instructions: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub approval_mode: Option<String>,
}

/// The new mate's fields: the request over the role it names over defaults.
pub fn new_mate(id: &str, chat: &str, folder: &Path, request: &CreateMate, role: Option<&template::Template>) -> NewMate {
    let pick = |asked: &Option<String>, from_role: Option<String>, default: &str| {
        asked.clone().filter(|value| !value.trim().is_empty()).or(from_role).unwrap_or_else(|| default.to_string())
    };
    NewMate {
        id: id.into(),
        chat_id: chat.into(),
        name: pick(&request.name, role.map(|r| r.name.clone()), SCRATCH_NAME),
        avatar: request
            .avatar
            .clone()
            .or_else(|| role.map(|r| r.avatar()))
            .unwrap_or_else(|| json!({ "definition": template::DEFAULT_FACE, "hue": template::hue_of(id) })),
        role: pick(&request.role, role.map(|r| r.role.clone()), ""),
        instructions: pick(&request.instructions, role.map(|r| r.instructions.clone()), ""),
        provider: request.provider.clone(),
        model: request.model.clone(),
        thinking: request.thinking.clone(),
        tools: role.and_then(|r| r.tools.clone()),
        approval_mode: request.approval_mode.clone().or_else(|| role.and_then(|r| r.approval.clone())),
        folder: folder.display().to_string(),
        template: role.map(|r| r.id.clone()),
        // From a gallery role: only the connected apps it names (none yet), each off (connected-apps FR-10).
        apps: role.map(|_| Vec::new()),
        ..NewMate::default()
    }
}

fn create(app: &AppHandle, request: &CreateMate) -> Result<Mate, String> {
    let role = match &request.template {
        Some(id) => Some(template::find(id).ok_or_else(|| format!("no gallery role \"{id}\""))?),
        None => None,
    };
    let id = new_id("mate");
    let chat = new_id("matechat");
    let dir = mate_dir(&id);
    let work = dir.join("work");
    for folder in [&work, &dir.join("memory")] {
        std::fs::create_dir_all(folder).map_err(|error| format!("could not create {}: {error}", folder.display()))?;
    }
    let db = app.state::<Db>();
    let app_mode = crate::engine::approval_mode(&db);
    let mate = db.with(|conn| mates::create_mate(conn, &new_mate(&id, &chat, &work, request, role.as_ref()), app_mode))?;
    write_role(&db, &mate)?;
    // The role's check-ins arrive paused: nothing runs until the person resumes one (FR-85).
    for checkin in role.iter().flat_map(|r| r.checkins.iter()) {
        if let Ok(item) = initiative::new_checkin(&mate.id, &mate.chat_id, &checkin.text, &checkin.repeat, evo_store::chats::now_ms()) {
            let _ = db.with(|conn| {
                evo_store::agenda::add_item(conn, &item)?;
                evo_store::agenda::set_status(conn, &mate.id, &item.id, "paused")
            });
        }
    }
    db.log(NewLog::new("info", "mates", format!("created mate {}", mate.name)).chat(Some(&mate.chat_id)));
    Ok(mate)
}

/// Mates' conversations and channels whose words match `query` (FR-76).
/// Transcripts written since the last search are read first, off the
/// window's thread.
#[tauri::command]
pub async fn mates_search(app: AppHandle, query: String) -> Result<Vec<mates::MessageHit>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Db>();
        db.with(|conn| {
            evo_store::chats::refresh_search(conn, &db.transcripts)?;
            mates::search_messages(conn, &query, 20)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn mates_list(app: AppHandle) -> Result<Vec<Mate>, String> {
    app.state::<Db>().with(|conn| mates::list_mates(conn))
}

#[tauri::command]
pub fn mates_templates() -> Vec<template::Summary> {
    template::all().iter().map(template::Template::summary).collect()
}

#[tauri::command]
pub fn mates_create(app: AppHandle, input: CreateMate) -> Result<Mate, String> {
    let mate = create(&app, &input)?;
    changed(&app);
    Ok(mate)
}

#[tauri::command]
pub async fn mates_update(app: AppHandle, id: String, patch: MatePatch) -> Result<Option<Mate>, String> {
    let db = app.state::<Db>();
    let Some(mate) = db.with(|conn| mates::update_mate(conn, &id, &patch))? else {
        return Ok(None);
    };
    // Only fields the engine reads need a restart.
    let engine_reads = patch.name.is_some()
        || patch.role.is_some()
        || patch.instructions.is_some()
        || patch.provider.is_some()
        || patch.model.is_some()
        || patch.thinking.is_some()
        || patch.tools.is_some()
        || patch.skills.is_some()
        || patch.approval_mode.is_some()
        || patch.project_folder.is_some();
    if engine_reads {
        write_role(&db, &mate)?;
        restart_when_idle(&app, &mate.chat_id).await;
        // Its seats and the channels it is in read the change too.
        channels::mate_changed(&app, &mate).await;
    }
    // Paused: its work stops now; the window holds new messages until Resume (FR-45).
    if patch.paused == Some(true) {
        stop_mate(&app, &mate.id).await;
    }
    // Resumed over today's spending limit: it goes on until the day ends (FR-80).
    if patch.paused == Some(false) {
        spend::resumed(&app, &mate.id);
    }
    changed(&app);
    Ok(Some(mate))
}

/// Stop every engine of a mate: its direct chat and its seats.
async fn stop_mate(app: &AppHandle, id: &str) {
    let chats: Vec<String> = app
        .state::<Db>()
        .with(|conn| {
            let mut statement = conn.prepare("SELECT id FROM chats WHERE mate_id = ?1")?;
            let rows = statement.query_map([id], |row| row.get(0))?;
            Ok(rows.collect::<Result<Vec<String>, _>>()?)
        })
        .unwrap_or_default();
    for chat in chats {
        park(app, &chat).await;
    }
}

/// Pause or resume every mate (FR-45): from Evo Mates' menu and the tray.
#[tauri::command]
pub async fn mates_pause_all(app: AppHandle, paused: bool) -> Result<(), String> {
    let ids: Vec<String> = app.state::<Db>().with(|conn| mates::list_mates(conn))?.into_iter().map(|m| m.id).collect();
    for id in ids {
        app.state::<Db>().with(|conn| mates::update_mate(conn, &id, &MatePatch { paused: Some(paused), ..MatePatch::default() }))?;
        if paused {
            stop_mate(&app, &id).await;
        }
    }
    changed(&app);
    Ok(())
}

/// A copy of a mate: its profile, not its conversation or memory.
#[tauri::command]
pub fn mates_duplicate(app: AppHandle, id: String) -> Result<Mate, String> {
    let db = app.state::<Db>();
    let source = db.with(|conn| mates::get_mate(conn, &id))?.ok_or_else(|| format!("no mate {id}"))?;
    let copy = create(
        &app,
        &CreateMate {
            name: Some(format!("{} copy", source.name)),
            avatar: Some(source.avatar.clone()),
            role: Some(source.role.clone()),
            instructions: Some(source.instructions.clone()),
            provider: source.provider.clone(),
            model: source.model.clone(),
            thinking: source.thinking.clone(),
            approval_mode: Some(source.approval_mode.clone()),
            template: None,
        },
    )?;
    let copy = db
        .with(|conn| {
            mates::update_mate(
                conn,
                &copy.id,
                &MatePatch {
                    tools: Some(source.tools.clone()),
                    connectors: Some(source.connectors.clone()),
                    skills: Some(source.skills.clone()),
                    apps: Some(source.apps.clone()),
                    sending: Some(source.sending.clone()),
                    ..MatePatch::default()
                },
            )
        })?
        .unwrap_or(copy);
    write_role(&db, &copy)?;
    changed(&app);
    Ok(copy)
}

/// Delete a mate: its engines stop, its chats, transcripts, folder and
/// memory go (the window asked the person first, FR-13).
#[tauri::command]
pub async fn mates_delete(app: AppHandle, id: String) -> Result<(), String> {
    let db = app.state::<Db>();
    let chats = db.with(|conn| mates::delete_mate(conn, &id))?;
    let engines = app.state::<crate::engine::EngineState>();
    for chat in &chats {
        crate::memory_index::stop_running(&app, Some(chat));
        engines.stop(chat).await;
        db.remove_transcript(chat);
    }
    // Its secrets: the vault's key in the keychain goes with its file.
    let mate = id.clone();
    if let Ok(Err(error)) =
        tauri::async_runtime::spawn_blocking(move || secrets::forget_all(&mate)).await
    {
        db.log(NewLog::new(
            "warn",
            "mates",
            format!("could not remove the secrets of {id}: {error}"),
        ));
    }
    let dir = mate_dir(&id);
    if dir.is_dir() {
        if let Err(error) = std::fs::remove_dir_all(&dir) {
            db.log(NewLog::new("warn", "mates", format!("could not remove {}: {error}", dir.display())));
        }
    }
    db.log(NewLog::new("info", "mates", format!("deleted mate {id}")));
    crate::supervisor::chat_rows_changed(&app);
    changed(&app);
    Ok(())
}

/// The reactions in a mate's conversation (FR-60).
#[tauri::command]
pub fn mates_reactions(app: AppHandle, chat: String) -> Result<Vec<mates::Reaction>, String> {
    app.state::<Db>().with(|conn| mates::list_reactions(conn, &chat))
}

/// Set or (`None`) remove the person's reaction to a message.
#[tauri::command]
pub fn mates_react(app: AppHandle, chat: String, message: String, reaction: Option<String>, excerpt: String) -> Result<(), String> {
    app.state::<Db>().with(|conn| mates::set_reaction(conn, &chat, &message, reaction.as_deref(), &excerpt))
}

/// The reactions the mate has not heard of, now marked heard: the window
/// puts them in the mate's next message as a note (FR-60).
#[tauri::command]
pub fn mates_take_reactions(app: AppHandle, chat: String) -> Result<Vec<mates::Reaction>, String> {
    app.state::<Db>().with(|conn| mates::take_untold_reactions(conn, &chat))
}

/// A mate's Always-allow rules: the tools it may run without asking (FR-41).
#[tauri::command]
pub fn mates_grants(app: AppHandle, id: String) -> Result<Vec<String>, String> {
    app.state::<Db>().with(|conn| mates::list_grants(conn, &id))
}

#[tauri::command]
pub fn mates_remove_grant(app: AppHandle, id: String, tool: String) -> Result<(), String> {
    app.state::<Db>()
        .with(|conn| mates::remove_grant(conn, &id, &tool))?;
    changed(&app);
    Ok(())
}

/// Open a mate's folder in the system's file manager.
#[tauri::command]
pub fn mates_folder(app: AppHandle, id: String) -> Result<String, String> {
    let mate = app.state::<Db>().with(|conn| mates::get_mate(conn, &id))?.ok_or_else(|| format!("no mate {id}"))?;
    Ok(mate.project_folder.filter(|folder| !folder.is_empty()).unwrap_or(mate.folder))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mate(name: &str, role: &str, instructions: &str) -> Mate {
        Mate {
            id: "m1".into(),
            name: name.into(),
            avatar: json!({}),
            role: role.into(),
            instructions: instructions.into(),
            provider: Some("acme".into()),
            model: Some("m".into()),
            thinking: None,
            tools: Some(vec!["read".into()]),
            connectors: None,
            skills: None,
            apps: None,
            approval_mode: "ask".into(),
            sending: "ask".into(),
            notifications: true,
            initiative: "medium".into(),
            unanswered: 0,
            paused: false,
            folder: "/mates/m1/work".into(),
            project_folder: None,
            chat_id: "c1".into(),
            template: None,
            pinned: false,
            hidden: false,
            position: 0,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn a_mates_role_carries_who_it_is_and_its_memory() {
        let role = role_of(&mate("Writer", "Drafts copy", "Write in plain English."));
        assert_eq!(role["mate"]["name"], "Writer");
        assert_eq!(role["provider"], "acme");
        assert_eq!(role["tools"], json!(["read"]));
        assert!(role["memoryDir"].as_str().unwrap().replace('\\', "/").ends_with("mates/m1/memory"));
        let text = role["instructions"].as_str().unwrap();
        assert!(text.starts_with("You are Writer"));
        assert!(text.contains("Your role: Drafts copy."));
        assert!(text.contains("Write in plain English."));
        // How it behaves is the engine's, added for any role naming a mate.
        assert!(!text.contains("How you work as a mate"));
        // The engine reads it as a session role.
        let parsed: evo_mesh::wire::SessionRole = serde_json::from_value(role).unwrap();
        assert_eq!(parsed.mate.unwrap().name, "Writer");
        assert!(parsed.memory_dir.is_some());
        // Every skill unless it names some (FR-44).
        assert_eq!(parsed.skills, None);
        let limited = Mate { skills: Some(vec!["review".into()]), ..mate("Writer", "Drafts copy", "") };
        let parsed: evo_mesh::wire::SessionRole = serde_json::from_value(role_of(&limited)).unwrap();
        assert_eq!(parsed.skills, Some(vec!["review".to_string()]));
    }

    #[test]
    fn a_new_mate_takes_the_gallery_role_under_what_was_asked() {
        let role = template::find("researcher").expect("the researcher role");
        let asked = CreateMate { name: Some("Ada".into()), ..CreateMate::default() };
        let new = new_mate("m1", "c1", Path::new("/w"), &asked, Some(&role));
        assert_eq!(new.name, "Ada");
        assert_eq!(new.role, role.role);
        assert_eq!(new.template.as_deref(), Some("researcher"));
        let blank = new_mate("m2", "c2", Path::new("/w"), &CreateMate::default(), None);
        assert_eq!(blank.name, "New mate");
        assert!(blank.avatar.get("definition").is_some());
    }
}
