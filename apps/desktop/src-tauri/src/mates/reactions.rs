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

/// The channel's chat (its coordinator's), for a channel id.
fn channel_chat(conn: &Connection, channel: &str) -> Option<String> {
    conn.query_row("SELECT chat_id FROM channels WHERE id = ?1", [channel], |row| row.get(0)).optional().ok().flatten()
}

/// Why a mate may not react to a line written by `author` (`None` when it may).
pub fn refuse_line(mate: &str, author: Option<&str>) -> Option<String> {
    match author {
        None => Some("No such line in this channel: name a line by the id in brackets before it in your recent lines.".into()),
        Some(author) if author == mate => Some("You cannot react to your own line.".into()),
        Some(_) => None,
    }
}

/// A mate's reaction from its seat in a channel; the words the tool answers with.
pub async fn react_from_seat(app: &AppHandle, seat: &str, mate: &str, line: &str, reaction: Option<&str>) -> Result<String, String> {
    let line = line.trim();
    if line.is_empty() {
        return Err("Name the line by its id, as it shows in brackets in your recent lines.".into());
    }
    if let Some(reaction) = reaction {
        if !mates::REACTIONS.contains(&reaction) {
            return Err(format!("Unknown reaction \"{reaction}\": use one of {}.", mates::REACTIONS.join(", ")));
        }
    }
    let (channel, chat) = {
        let db = app.state::<Db>();
        let channel = db
            .with(|conn| Ok::<_, evo_store::DbError>(channel_of_seat(conn, seat)))?
            .ok_or_else(|| "react works in a channel, on its lines.".to_string())?;
        let chat = db
            .with(|conn| Ok::<_, evo_store::DbError>(channel_chat(conn, &channel)))?
            .ok_or_else(|| "react works in a channel, on its lines.".to_string())?;
        (channel, chat)
    };
    // The channel's coordinator knows its lines and who wrote them.
    use crate::supervisor::Host as _;
    let host = crate::supervisor::AppHost(app.clone());
    let author = host
        .request(&chat, "channel_line", json!({ "line": line }))
        .await
        .ok()
        .and_then(|answer| answer.get("author").and_then(|a| a.as_str()).map(str::to_string));
    if let Some(why) = refuse_line(mate, author.as_deref()) {
        return Err(why);
    }
    app.state::<Db>().with(|conn| mates::set_channel_reaction(conn, &channel, line, mate, reaction))?;
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
    fn only_another_members_line_takes_a_reaction() {
        assert!(refuse_line("kai", None).unwrap().contains("No such line"));
        assert!(refuse_line("kai", Some("kai")).unwrap().contains("own line"));
        assert!(refuse_line("kai", Some("lin")).is_none());
        assert!(refuse_line("kai", Some("person")).is_none());
    }

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
