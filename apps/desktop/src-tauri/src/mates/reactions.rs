//! Reactions in channels (docs/features/evo-mates-messaging/spec.md, FR-3,
//! FR-4): the person's from the window, a mate's through its `react` tool
//! (`agent_tools/mates.rs`). A reaction never wakes anyone and never counts
//! as a hand-off; the line's author hears of it at its next delivery.

use evo_store::mates::{self, ChannelReaction};
use evo_store::rusqlite::{Connection, OptionalExtension};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::db::Db;

/// The event a window reads the channel's reactions again on.
pub const CHANGED: &str = "mates://reactions";

/// The channel a seat chat sits in.
pub fn channel_of_seat(conn: &Connection, seat: &str) -> Option<String> {
    conn.query_row("SELECT channel_id FROM channel_members WHERE seat_chat_id = ?1", [seat], |row| row.get(0))
        .optional()
        .ok()
        .flatten()
}

/// Every reaction in `channel` (a channel's id).
#[tauri::command]
pub fn channel_reactions(app: AppHandle, channel: String) -> Result<Vec<ChannelReaction>, String> {
    app.state::<Db>().with(|conn| mates::list_channel_reactions(conn, &channel))
}

/// Set or (`None`) remove the person's reaction to a line of `channel`.
#[tauri::command]
pub fn channel_react(app: AppHandle, channel: String, message: String, reaction: Option<String>) -> Result<(), String> {
    app.state::<Db>()
        .with(|conn| mates::set_channel_reaction(conn, &channel, &message, "person", reaction.as_deref()))?;
    let _ = app.emit(CHANGED, json!({ "channel": channel }));
    Ok(())
}

/// A mate's reaction from its seat in a channel; the words the tool answers with.
pub fn react_from_seat(app: &AppHandle, seat: &str, mate: &str, line: &str, reaction: Option<&str>) -> Result<String, String> {
    let line = line.trim();
    if line.is_empty() {
        return Err("Name the line by its id, as it shows in brackets in your recent lines.".into());
    }
    if let Some(reaction) = reaction {
        if !mates::REACTIONS.contains(&reaction) {
            return Err(format!("Unknown reaction \"{reaction}\": use one of {}.", mates::REACTIONS.join(", ")));
        }
    }
    let db = app.state::<Db>();
    let channel = db
        .with(|conn| Ok::<_, evo_store::DbError>(channel_of_seat(conn, seat)))?
        .ok_or_else(|| "react works in a channel, on its lines.".to_string())?;
    db.with(|conn| mates::set_channel_reaction(conn, &channel, line, mate, reaction))?;
    let _ = app.emit(CHANGED, json!({ "channel": channel }));
    Ok(match reaction {
        Some(reaction) => format!("Reacted {reaction} to {line}. Nobody is woken by it."),
        None => format!("Removed your reaction to {line}."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seat_finds_its_channel() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE channel_members (channel_id TEXT, mate_id TEXT, seat_chat_id TEXT);
             INSERT INTO channel_members VALUES ('ch', 'm1', 'seat-1');",
        )
        .unwrap();
        assert_eq!(channel_of_seat(&conn, "seat-1").as_deref(), Some("ch"));
        assert_eq!(channel_of_seat(&conn, "chat-m1"), None);
    }
}
