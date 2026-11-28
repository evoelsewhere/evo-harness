//! When a skill review starts: after a reply is delivered in a conversation
//! where the agent took at least the review step count of tool steps since
//! its last review (docs/features/learning-layers/spec.md, FR-16; plan 3.5).
//!
//! Evo Harness chats learn into the person's skills (owner `personal`),
//! a mate's conversation into the mate's. Chats with memory off, incognito
//! or scheduled runs are never read; channel seats learn through the retro.

use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::db::Db;

/// A review is due: `every` tool steps or more since the last one.
pub fn review_due(tool_steps: i64, last: Option<i64>, every: i64) -> bool {
    tool_steps - last.unwrap_or(0) >= every
}

/// The tool steps of a saved chat: its tool results.
pub fn tool_steps(state: &Value) -> i64 {
    use evo_store::transcripts::{messages, MessageKind, MessageLimits};
    messages(state, MessageLimits { tool_args: 0, tool_result: 0 }).iter().filter(|m| m.kind == MessageKind::ToolResult).count() as i64
}

/// The owner whose skills a chat's review writes, when its app learns skills.
fn owner_of(db: &Db, chat: &str) -> Option<String> {
    let found = db.with(|conn| evo_store::chats::get_chat(conn, chat)).ok().flatten()?;
    if found.memory_off || found.incognito || found.scheduled {
        return None;
    }
    let all = db.with(|conn| evo_store::settings::all(conn)).unwrap_or_default();
    let settings = super::settings_from(&all);
    match found.kind.as_str() {
        "chat" if settings.harness_skills => Some("personal".into()),
        "mate" => {
            let mate = found.mate_id?;
            super::owners::mate_on(&all, &mate, "skills", settings.mates_skills).then(|| format!("mate:{mate}"))
        }
        _ => None,
    }
}

/// A run of `chat` settled: start its review when one is due.
pub fn settled(app: &AppHandle, chat: &str, line: &Value) {
    if line.get("compact") == Some(&Value::Bool(true)) || line.get("outcome").and_then(Value::as_str) != Some("completed") {
        return;
    }
    let (app, chat) = (app.clone(), chat.to_string());
    tauri::async_runtime::spawn(async move {
        let db = app.state::<Db>();
        let Some(owner) = owner_of(&db, &chat) else { return };
        let Ok(path) = db.transcript(&chat) else { return };
        let Some(state) = evo_store::transcripts::read_state(&path, &chat).ok().flatten().and_then(|text| serde_json::from_str::<Value>(&text).ok()) else {
            return;
        };
        let steps = tool_steps(&state);
        let every = super::settings_from(&db.with(|conn| evo_store::settings::all(conn)).unwrap_or_default()).review_steps;
        let state = db.with(|conn| evo_store::learning::state(conn, &owner)).unwrap_or_default();
        if !review_due(steps, state.review_steps.get(&chat).copied(), every) {
            return;
        }
        // A review already running for this owner: the next settle tries again.
        if app.state::<super::jobs::Learning>().is_running(&owner) {
            return;
        }
        let folder = crate::agent_tools::chat_folder(&app, &chat);
        let outcome = super::jobs::run_step_in(&app, "review", &owner, vec!["--chat".into(), chat.clone()], Some(folder)).await;
        let _ = db.with(|conn| {
            let mut state = evo_store::learning::state(conn, &owner)?;
            if outcome.is_ok() || state.failures + 1 >= super::clock::MAX_FAILURES {
                state.review_steps.insert(chat.clone(), steps);
                state.failures = 0;
            } else {
                state.failures += 1;
            }
            evo_store::learning::put_state(conn, &state)
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // FR-16: after ten tool steps since the last review.
    #[test]
    fn a_review_starts_after_ten_tool_steps() {
        assert!(!review_due(9, None, 10));
        assert!(review_due(10, None, 10));
        assert!(!review_due(25, Some(20), 10), "counted from the last review");
        assert!(review_due(30, Some(20), 10));
        assert!(review_due(3, None, 3), "the step count is a setting");

        let entry = |id: &str, parent: Option<&str>, message: Value| json!({ "type": "message", "id": id, "parentId": parent, "timestamp": 1, "message": message });
        let result = |id: &str| json!({ "role": "toolResult", "toolCallId": id, "toolName": "read", "content": [{ "type": "text", "text": "x" }], "timestamp": 1 });
        let state = json!({ "entries": [
            entry("e1", None, json!({ "role": "user", "content": "go", "timestamp": 1 })),
            entry("e2", Some("e1"), result("t1")),
            entry("e3", Some("e2"), result("t2")),
            entry("e4", Some("e3"), json!({ "role": "assistant", "content": [{ "type": "text", "text": "done" }], "timestamp": 1 })),
        ], "lanes": { "main": "e4" } });
        assert_eq!(tool_steps(&state), 2);
    }
}
