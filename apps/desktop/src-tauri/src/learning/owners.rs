//! The memory owners the learning loop looks after, by app and switch, and
//! how many chats each has had since a time (docs/features/learning-layers/
//! spec.md, FR-10, FR-23, FR-25; plan 3.1).
//!
//! Owners: `personal` and `project:<slug>` in Evo Harness (when its Memory
//! upkeep is on), `mate:<id>` per mate in Evo Mates (when that app's switch
//! and the mate's own are on).

use serde_json::{Map, Value};
use tauri::{AppHandle, Manager};

use super::clock::Watched;
use super::Settings;
use crate::db::Db;
use crate::engine::EngineState;

/// A mate's own learning switches: `learning.mate.<id>` = `{upkeep?, skills?}`
/// (FR-25). Absent: the app's switch decides.
pub fn mate_key(id: &str) -> String {
    format!("learning.mate.{id}")
}

/// Whether `which` (`upkeep` or `skills`) is on for mate `id`.
pub fn mate_on(all: &Map<String, Value>, id: &str, which: &str, app_on: bool) -> bool {
    app_on && super::clock::flag(all.get(&mate_key(id)), which) != Some(false)
}

/// `C:\work\app` → `C--work-app`, as the engine names project memory.
pub fn slug(path: &str) -> String {
    path.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// The owners whose nightly steps may run now.
pub fn watched(app: &AppHandle, settings: &Settings, all: &Map<String, Value>) -> Vec<Watched> {
    let db = app.state::<Db>();
    let engines = app.state::<EngineState>();
    let mut out = Vec::new();
    if settings.mates_upkeep {
        let mates = db.with(|conn| evo_store::mates::list_mates(conn)).unwrap_or_default();
        for mate in mates.iter().filter(|mate| !mate.paused && mate_on(all, &mate.id, "upkeep", true)) {
            let chats = mate_chats(&db, &mate.id);
            let working = chats.iter().any(|chat| engines.is_working(chat));
            out.push(Watched { owner: format!("mate:{}", mate.id), working });
        }
    }
    if settings.harness_upkeep {
        let working = engines.working() > 0;
        out.push(Watched { owner: "personal".into(), working });
        if let Ok(agent) = evo_store::agent_dir() {
            if let Ok(projects) = std::fs::read_dir(agent.join("projects")) {
                for project in projects.flatten() {
                    if project.path().join("memory").is_dir() {
                        let slug = project.file_name().to_string_lossy().into_owned();
                        out.push(Watched { owner: format!("project:{slug}"), working });
                    }
                }
            }
        }
    }
    out
}

/// A mate's chats: its conversation and its channel seats.
pub fn mate_chats(db: &Db, mate: &str) -> Vec<String> {
    db.with(|conn| {
        let mut statement = conn.prepare("SELECT id FROM chats WHERE mate_id = ?1")?;
        let rows = statement.query_map([mate], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .unwrap_or_default()
}

/// Roughly how many chats `owner` has had since `since_ms`; the engine
/// counts exactly when the dream starts. A mate's conversation is one chat
/// that can hold several days, so it counts as two when it moved (FR-10).
pub fn chats_since(db: &Db, owner: &str, since_ms: i64, settings: &Settings) -> usize {
    let counted = db.with(|conn| {
        let total = match owner.split_once(':') {
            Some(("mate", id)) => {
                let count = |kind: &str| -> Result<i64, evo_store::DbError> {
                    Ok(conn.query_row(
                        "SELECT COUNT(*) FROM chats WHERE mate_id = ?1 AND kind = ?2 AND updated_at > ?3",
                        evo_store::rusqlite::params![id, kind, since_ms],
                        |row| row.get(0),
                    )?)
                };
                let seats = if settings.learn_from_channels { count("seat")? } else { 0 };
                count("mate")? * 2 + seats
            }
            _ => {
                // Harness chats with the person's messages, memory on, not scheduled.
                let mut statement = conn.prepare(
                    "SELECT w.path FROM chats c LEFT JOIN workspaces w ON w.id = c.workspace_id
                     WHERE c.kind = 'chat' AND c.memory_off = 0 AND c.incognito = 0 AND c.scheduled = 0
                       AND c.turn_count > 0 AND c.updated_at > ?1",
                )?;
                let paths: Vec<Option<String>> = statement.query_map([since_ms], |row| row.get(0))?.collect::<Result<_, _>>()?;
                match owner.split_once(':') {
                    Some(("project", wanted)) => paths.iter().flatten().filter(|path| slug(path) == wanted).count() as i64,
                    _ => paths.len() as i64,
                }
            }
        };
        Ok(total)
    });
    counted.unwrap_or(0).max(0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // FR-25: a mate switched off stays off whatever the app says.
    #[test]
    fn a_mate_switched_off_is_not_learned_for() {
        let all = json!({ "learning.mate.m1": { "upkeep": false }, "learning.mate.m2": { "skills": false } });
        let all = all.as_object().unwrap();
        assert!(!mate_on(all, "m1", "upkeep", true));
        assert!(mate_on(all, "m1", "skills", true));
        assert!(mate_on(all, "m2", "upkeep", true));
        assert!(!mate_on(all, "m2", "skills", true));
        assert!(mate_on(all, "m3", "upkeep", true));
        assert!(!mate_on(all, "m3", "upkeep", false), "the app's switch off: off for every mate");
        assert_eq!(slug("C:\\work\\app"), "C--work-app");
    }
}
