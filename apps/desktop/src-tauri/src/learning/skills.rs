//! Learned skills as the windows list and change them: a switch, **Undo last
//! change** and **Delete**, each a journal row (docs/features/learning-layers/
//! spec.md, FR-15, FR-18; plan 3.4). The engine writes them; the person's
//! answers are file changes the shell makes and journals.

use std::path::PathBuf;

use evo_store::journal::{self, NewRow};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};

use crate::db::Db;

/// A learned skill switched off: this file in its folder (as the engine reads it).
pub const OFF_MARKER: &str = ".off";

/// The learned-skills folder of `owner`: the person's for Evo Harness
/// (`personal` or any project), a mate's own.
pub fn dir_of(owner: &str) -> Result<PathBuf, String> {
    let agent = evo_store::agent_dir().map_err(|e| e.to_string())?;
    let safe = |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') && part != "..";
    match owner.split_once(':') {
        Some(("mate", id)) if safe(id) => Ok(agent.join("mates").join(id).join("skills")),
        Some(("project", slug)) if safe(slug) => Ok(agent.join("learned-skills")),
        None if owner == "personal" => Ok(agent.join("learned-skills")),
        _ => Err(format!("unknown owner `{owner}`")),
    }
}

/// The journal's owner of a learned skill: the person's skills are `personal`.
pub fn journal_owner(owner: &str) -> &str {
    if owner.starts_with("project:") {
        "personal"
    } else {
        owner
    }
}

fn valid_name(name: &str) -> Result<(), String> {
    if !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        Ok(())
    } else {
        Err(format!("`{name}` is not a skill name"))
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LearnedSkill {
    pub name: String,
    pub description: String,
    pub text: String,
    pub on: bool,
    /// The newest change that can be undone, by its journal row.
    pub last_change: Option<i64>,
    pub changed_at: Option<i64>,
    pub changed_by: Option<String>,
}

fn front(text: &str, key: &str) -> Option<String> {
    let body = text.trim_start_matches('\u{feff}').strip_prefix("---")?;
    let end = body.find("\n---")?;
    body[..end].lines().find_map(|line| line.strip_prefix(&format!("{key}:")).map(|value| value.trim().trim_matches('"').to_string()))
}

/// Every learned skill of `owner`.
pub fn list(db: &Db, owner: &str) -> Result<Vec<LearnedSkill>, String> {
    let dir = dir_of(owner)?;
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path().join("SKILL.md");
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let name = entry.file_name().to_string_lossy().into_owned();
            let last = db.with(|conn| journal::last_of(conn, journal_owner(owner), "skill", &name)).ok().flatten();
            out.push(LearnedSkill {
                description: front(&text, "description").unwrap_or_default(),
                on: !entry.path().join(OFF_MARKER).exists(),
                last_change: last.as_ref().filter(|row| row.undoable).map(|row| row.id),
                changed_at: last.as_ref().map(|row| row.at_ms),
                changed_by: last.map(|row| row.by),
                name,
                text,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn row(owner: &str, name: &str, action: &str, by: &str, before: Option<String>, after: Option<String>) -> NewRow {
    NewRow {
        at_ms: evo_store::chats::now_ms(),
        owner: journal_owner(owner).to_string(),
        subject: "skill".into(),
        name: name.to_string(),
        action: action.into(),
        by: by.into(),
        before,
        after,
        undoable: true,
        ..Default::default()
    }
}

fn write(path: &std::path::Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let partial = path.with_extension("md.partial");
    std::fs::write(&partial, text).map_err(|e| e.to_string())?;
    std::fs::rename(&partial, path).map_err(|e| e.to_string())
}

/// Undo journal row `id` of a learned skill: its earlier text comes back (a
/// skill it created goes), unless the skill changed since. The undo is a row.
pub fn undo(db: &Db, id: i64) -> Result<(), String> {
    let change = db.with(|conn| journal::get(conn, id))?.ok_or("no such change")?;
    if change.subject != "skill" || change.undone_at.is_some() || !change.undoable {
        return Err("that change cannot be undone".into());
    }
    valid_name(&change.name)?;
    let owner = if change.owner == "personal" { "personal".to_string() } else { change.owner.clone() };
    let folder = dir_of(&owner)?.join(&change.name);
    let path = folder.join("SKILL.md");
    let current = std::fs::read_to_string(&path).ok();
    if current != change.after {
        return Err(format!("`{}` changed since; undo its newer change first", change.name));
    }
    match &change.before {
        Some(text) => write(&path, text)?,
        None => std::fs::remove_dir_all(&folder).map_err(|e| e.to_string())?,
    }
    db.with(|conn| journal::mark_undone(conn, id, evo_store::chats::now_ms()))?;
    let mut undone = row(&owner, &change.name, "undone", "undo", change.after.clone(), change.before.clone());
    undone.chat_id = change.chat_id.clone();
    db.with(|conn| journal::record(conn, &undone))?;
    Ok(())
}

fn changed(app: &AppHandle, owner: &str) {
    let _ = app.emit("learning://changed", json!({ "owner": owner, "subject": "skill" }));
    crate::supervisor::settings::resources_changed(app);
}

#[tauri::command]
pub fn learned_skills(db: State<'_, Db>, owner: String) -> Result<Vec<LearnedSkill>, String> {
    list(&db, &owner)
}

/// The switch of a learned skill.
#[tauri::command]
pub fn learned_skill_set_on(app: AppHandle, owner: String, name: String, on: bool) -> Result<(), String> {
    valid_name(&name)?;
    let folder = dir_of(&owner)?.join(&name);
    if !folder.join("SKILL.md").exists() {
        return Err(format!("no learned skill `{name}`"));
    }
    let marker = folder.join(OFF_MARKER);
    if on {
        let _ = std::fs::remove_file(marker);
    } else {
        std::fs::write(marker, "").map_err(|e| e.to_string())?;
    }
    changed(&app, &owner);
    Ok(())
}

/// **Undo last change** of a learned skill, or a given change (the
/// "Learned" line's Undo, a retro card's).
#[tauri::command]
pub fn learned_skill_undo(app: AppHandle, db: State<'_, Db>, owner: String, name: Option<String>, change: Option<i64>) -> Result<(), String> {
    let id = match (change, name) {
        (Some(id), _) => id,
        (None, Some(name)) => db.with(|conn| journal::last_of(conn, journal_owner(&owner), "skill", &name))?.map(|row| row.id).ok_or("nothing to undo")?,
        (None, None) => return Err("say which skill or change".into()),
    };
    undo(&db, id)?;
    changed(&app, &owner);
    Ok(())
}

/// **Delete** a learned skill; the journal keeps its text, so it can be undone.
#[tauri::command]
pub fn learned_skill_delete(app: AppHandle, db: State<'_, Db>, owner: String, name: String) -> Result<(), String> {
    valid_name(&name)?;
    let folder = dir_of(&owner)?.join(&name);
    let before = std::fs::read_to_string(folder.join("SKILL.md")).map_err(|_| format!("no learned skill `{name}`"))?;
    std::fs::remove_dir_all(&folder).map_err(|e| e.to_string())?;
    db.with(|conn| journal::record(conn, &row(&owner, &name, "deleted", "person", Some(before), None)))?;
    changed(&app, &owner);
    Ok(())
}

/// The skill rows a review or a retro wrote after replies of `chat`: the
/// "Learned · <skill>" lines (FR-18).
#[tauri::command]
pub fn learned_for_chat(db: State<'_, Db>, chat: String) -> Result<Vec<journal::Entry>, String> {
    db.with(|conn| journal::for_reply(conn, &chat))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learned_skill_owners_and_names() {
        let mate = dir_of("mate:m1").unwrap();
        assert!(mate.ends_with(std::path::Path::new("mates").join("m1").join("skills")));
        assert_eq!(dir_of("project:C--app").unwrap(), dir_of("personal").unwrap(), "Evo Harness learns per person");
        assert!(dir_of("mate:../x").is_err() && dir_of("other").is_err());
        assert_eq!(journal_owner("project:C--app"), "personal");
        assert!(valid_name("summary-first").is_ok() && valid_name("../x").is_err());
        let text = "---\nname: summary-first\ndescription: \"Reports open with a summary\"\n---\nbody";
        assert_eq!(front(text, "description").as_deref(), Some("Reports open with a summary"));
    }
}
