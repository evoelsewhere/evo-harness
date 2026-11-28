//! A channel's board (docs/features/evo-mates-threads-board/spec.md, FR-8 –
//! FR-15): the window's commands for the person, and the seat tools mates
//! work it with (`board_list`, `board_create`, `board_update`, `board_move`,
//! `board_comment`, FR-11). The data lives in `evo_store::boards`.
//!
//! Every change emits `mates://board` so open boards read again. A mate's
//! change also posts a short board line in the channel through its engine
//! (`channel_post_line`, FR-13); the person's own changes do not.

use evo_store::boards::{self, Board, Card, CardEvent, CardPatch, CheckItem, NewCard};
use evo_store::rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::db::Db;

/// The event a window reads a board again on.
pub const CHANGED: &str = "mates://board";

fn changed(app: &AppHandle, channel: &str) {
    let _ = app.emit(CHANGED, json!({ "channel": channel }));
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardView {
    pub board: Option<Board>,
    pub cards: Vec<Card>,
    /// A key for turning it on the first time (D6).
    pub proposed_key: String,
}

/// A card with where it lives, for views beyond one channel (FR-14).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedCard {
    pub card: Card,
    pub channel_name: String,
    pub chat_id: String,
    pub column: String,
}

fn view(conn: &Connection, channel: &str) -> Result<BoardView, evo_store::DbError> {
    let name = evo_store::channels::get_channel(conn, channel)?
        .map(|c| c.name)
        .unwrap_or_default();
    Ok(BoardView {
        board: boards::get_board(conn, channel)?,
        cards: boards::list_cards(conn, channel)?,
        proposed_key: boards::propose_key(&name),
    })
}

fn place(conn: &Connection, card: Card) -> Option<PlacedCard> {
    let channel = evo_store::channels::get_channel(conn, &card.channel_id).ok().flatten()?;
    let board = boards::get_board(conn, &card.channel_id).ok().flatten()?;
    let column = board.columns.iter().find(|c| c.id == card.column_id).map(|c| c.name.clone()).unwrap_or_default();
    Some(PlacedCard {
        card,
        channel_name: channel.name,
        chat_id: channel.chat_id,
        column,
    })
}

// ── the person's commands ───────────────────────────────────────────────────

#[tauri::command]
pub fn board_get(app: AppHandle, channel: String) -> Result<BoardView, String> {
    app.state::<Db>().with(|conn| view(conn, &channel))
}

/// Turn the board on (with `key` the first time) or off (FR-8).
#[tauri::command]
pub fn board_set(app: AppHandle, channel: String, on: bool, key: Option<String>) -> Result<BoardView, String> {
    let db = app.state::<Db>();
    let out = db.with(|conn| {
        if on {
            let key = match key {
                Some(key) => key,
                None => {
                    let name = evo_store::channels::get_channel(conn, &channel)?.map(|c| c.name).unwrap_or_default();
                    boards::propose_key(&name)
                }
            };
            boards::enable_board(conn, &channel, &key)?;
        } else {
            boards::disable_board(conn, &channel)?;
        }
        view(conn, &channel)
    })?;
    changed(&app, &channel);
    Ok(out)
}

/// Columns: `add` (name), `rename` (column, name), `move` (column, index), `remove` (column).
#[tauri::command]
pub fn board_column(
    app: AppHandle,
    channel: String,
    action: String,
    column: Option<String>,
    name: Option<String>,
    index: Option<usize>,
) -> Result<BoardView, String> {
    let column = column.unwrap_or_default();
    let name = name.unwrap_or_default();
    let out = app.state::<Db>().with(|conn| {
        match action.as_str() {
            "add" => {
                boards::add_column(conn, &channel, &name)?;
            }
            "rename" => boards::rename_column(conn, &channel, &column, &name)?,
            "move" => boards::move_column(conn, &channel, &column, index.unwrap_or(0))?,
            "remove" => boards::remove_column(conn, &channel, &column)?,
            other => return Err(evo_store::DbError::Invalid(format!("unknown column action {other}"))),
        }
        view(conn, &channel)
    })?;
    changed(&app, &channel);
    Ok(out)
}

#[tauri::command]
pub fn card_create(app: AppHandle, channel: String, card: NewCard) -> Result<Card, String> {
    let card = app.state::<Db>().with(|conn| boards::create_card(conn, &channel, &card, "person"))?;
    changed(&app, &channel);
    Ok(card)
}

#[tauri::command]
pub fn card_update(app: AppHandle, id: String, patch: CardPatch) -> Result<Card, String> {
    let card = app.state::<Db>().with(|conn| boards::update_card(conn, &id, &patch, "person"))?;
    changed(&app, &card.channel_id);
    Ok(card)
}

#[tauri::command]
pub fn card_move(app: AppHandle, id: String, column: String, index: Option<usize>) -> Result<Card, String> {
    let card = app.state::<Db>().with(|conn| boards::move_card(conn, &id, &column, index, "person"))?;
    changed(&app, &card.channel_id);
    Ok(card)
}

#[tauri::command]
pub fn card_delete(app: AppHandle, id: String) -> Result<(), String> {
    let channel = app.state::<Db>().with(|conn| {
        let card = boards::get_card(conn, &id)?;
        boards::delete_card(conn, &id)?;
        Ok(card.map(|c| c.channel_id))
    })?;
    if let Some(channel) = channel {
        changed(&app, &channel);
    }
    Ok(())
}

#[tauri::command]
pub fn card_comment(app: AppHandle, id: String, text: String) -> Result<CardEvent, String> {
    let (event, channel) = app.state::<Db>().with(|conn| {
        let event = boards::comment_card(conn, &id, "person", &text)?;
        let channel = boards::get_card(conn, &id)?.map(|c| c.channel_id).unwrap_or_default();
        Ok((event, channel))
    })?;
    changed(&app, &channel);
    Ok(event)
}

#[tauri::command]
pub fn card_events(app: AppHandle, id: String) -> Result<Vec<CardEvent>, String> {
    app.state::<Db>().with(|conn| boards::card_events(conn, &id))
}

/// A card by its key, with where it lives (`LAUNCH-12` links, FR-14).
#[tauri::command]
pub fn card_by_key(app: AppHandle, key: String) -> Result<Option<PlacedCard>, String> {
    app.state::<Db>().with(|conn| Ok(boards::card_by_key(conn, &key)?.and_then(|card| place(conn, card))))
}

/// Every board's key, for drawing `KEY-n` as links (FR-14).
#[tauri::command]
pub fn board_keys(app: AppHandle) -> Result<Vec<String>, String> {
    app.state::<Db>().with(|conn| {
        let mut stmt = conn.prepare("SELECT key FROM boards ORDER BY key")?;
        let keys = stmt.query_map([], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
        Ok(keys)
    })
}

/// Open cards assigned to `assignee` (`person` for My cards, or a mate's id)
/// across channels (FR-14).
#[tauri::command]
pub fn cards_assigned(app: AppHandle, assignee: String) -> Result<Vec<PlacedCard>, String> {
    app.state::<Db>().with(|conn| {
        Ok(boards::assigned_cards(conn, &assignee)?
            .into_iter()
            .filter_map(|card| place(conn, card))
            .collect())
    })
}

// ── mates' tools (FR-11) ────────────────────────────────────────────────────

pub const LIST: &str = "board_list";
pub const CREATE: &str = "board_create";
pub const UPDATE: &str = "board_update";
pub const MOVE: &str = "board_move";
pub const COMMENT: &str = "board_comment";
pub const TOOLS: [&str; 5] = [LIST, CREATE, UPDATE, MOVE, COMMENT];

/// The seat's channel and its board, when the board is on.
pub fn board_of_seat(app: &AppHandle, seat: &str) -> Option<(evo_store::channels::Channel, Board)> {
    app.try_state::<Db>()?
        .with(|conn| {
            let Some(channel) = super::reactions::channel_of_seat(conn, seat) else {
                return Ok(None);
            };
            let Some(channel) = evo_store::channels::get_channel(conn, &channel)? else {
                return Ok(None);
            };
            Ok(boards::board_on(conn, &channel.id)?.map(|board| (channel, board)))
        })
        .ok()
        .flatten()
}

pub fn definitions() -> Vec<Value> {
    let card = json!({ "type": "string", "description": "The card's key, e.g. \"LAUNCH-12\"." });
    let assignee = json!({ "type": ["string", "null"], "description": "A member's name, \"you\" for the person, or null for nobody." });
    let priority = json!({ "type": "string", "enum": ["none", "low", "medium", "high", "urgent"] });
    let due = json!({ "type": ["string", "null"], "description": "A local date, \"2026-12-04\", or date and time, \"2026-12-04T17:00\"; null clears it." });
    vec![
        json!({
            "name": LIST,
            "description": "List the cards on this channel's board, by column, with their keys, assignees, priorities and due dates. Optionally only one column or one assignee.",
            "inputSchema": { "type": "object", "properties": {
                "column": { "type": "string", "description": "A column's name." },
                "assignee": { "type": "string", "description": "A member's name, or \"you\" for the person." }
            }, "additionalProperties": false }
        }),
        json!({
            "name": CREATE,
            "description": "Add a card to this channel's board for a piece of work. The channel sees a line about it. Use it for work worth tracking, not for every small step.",
            "inputSchema": { "type": "object", "properties": {
                "title": { "type": "string" },
                "description": { "type": "string", "description": "Markdown." },
                "assignee": assignee,
                "priority": priority,
                "due": due,
                "labels": { "type": "array", "items": { "type": "string" } },
                "column": { "type": "string", "description": "A column's name; the first column when left out." }
            }, "required": ["title"], "additionalProperties": false }
        }),
        json!({
            "name": UPDATE,
            "description": "Change a card on this channel's board: title, description, assignee, priority, due date, labels or checklist. Only the fields given change.",
            "inputSchema": { "type": "object", "properties": {
                "card": card,
                "title": { "type": "string" },
                "description": { "type": "string" },
                "assignee": assignee,
                "priority": priority,
                "due": due,
                "labels": { "type": "array", "items": { "type": "string" } },
                "checklist": { "type": "array", "items": { "type": "object", "properties": { "text": { "type": "string" }, "done": { "type": "boolean" } }, "required": ["text"] } }
            }, "required": ["card"], "additionalProperties": false }
        }),
        json!({
            "name": MOVE,
            "description": "Move a card on this channel's board to another column, e.g. to \"In progress\" when you start it or \"Review\" when it is ready. The channel sees a line about it.",
            "inputSchema": { "type": "object", "properties": {
                "card": card,
                "column": { "type": "string", "description": "The column's name." }
            }, "required": ["card", "column"], "additionalProperties": false }
        }),
        json!({
            "name": COMMENT,
            "description": "Add a comment to a card's activity log: progress, a question, a decision.",
            "inputSchema": { "type": "object", "properties": {
                "card": card,
                "text": { "type": "string" }
            }, "required": ["card", "text"], "additionalProperties": false }
        }),
    ]
}

/// Who an assignee word names (FR-11): a member by name (any case), `you`
/// or `person` for the person; `Ok(None)` for nobody.
pub fn resolve_assignee(word: Option<&str>, members: &[(String, String)]) -> Result<Option<String>, String> {
    let Some(word) = word.map(|w| w.trim().trim_start_matches('@')).filter(|w| !w.is_empty()) else {
        return Ok(None);
    };
    let lower = word.to_lowercase();
    if ["you", "person", "the person", "me"].contains(&lower.as_str()) {
        return Ok(Some("person".into()));
    }
    members
        .iter()
        .find(|(id, name)| name.to_lowercase() == lower || id.to_lowercase() == lower)
        .map(|(id, _)| Some(id.clone()))
        .ok_or_else(|| {
            format!(
                "No member named \"{word}\" in this channel: use one of {} or \"you\".",
                members.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join(", ")
            )
        })
}

/// A due date from a tool's words: local `YYYY-MM-DD` (end of that day) or
/// `YYYY-MM-DDTHH:MM`.
pub fn parse_due(text: &str) -> Option<i64> {
    use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};
    let text = text.trim();
    let naive = NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M"))
        .ok()
        .or_else(|| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(23, 59, 0)))?;
    Local.from_local_datetime(&naive).earliest().map(|t| t.timestamp_millis())
}

fn day(ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|t| t.format("%a %d %b").to_string())
        .unwrap_or_default()
}

/// One card as a tool lists it.
pub fn card_line(card: &Card, name_of: &dyn Fn(&str) -> String) -> String {
    let mut out = format!("{} {}", card.key, card.title);
    let mut facts: Vec<String> = Vec::new();
    if let Some(a) = &card.assignee {
        facts.push(name_of(a));
    }
    if card.priority != "none" {
        facts.push(card.priority.clone());
    }
    if let Some(due) = card.due_ms {
        facts.push(format!("due {}", day(due)));
    }
    if !card.checklist.is_empty() {
        facts.push(format!("{}/{} done", card.checklist.iter().filter(|c| c.done).count(), card.checklist.len()));
    }
    if !facts.is_empty() {
        out.push_str(&format!(" ({})", facts.join(", ")));
    }
    out
}

fn members(conn: &Connection, channel: &evo_store::channels::Channel) -> Vec<(String, String)> {
    channel
        .members
        .iter()
        .map(|m| {
            let name = evo_store::mates::get_mate(conn, &m.mate_id)
                .ok()
                .flatten()
                .map(|mate| mate.name)
                .unwrap_or_else(|| m.mate_id.clone());
            (m.mate_id.clone(), name)
        })
        .collect()
}

/// Run a board tool for `mate` in its seat; the words the tool answers with.
pub async fn run_tool(app: &AppHandle, seat: &str, mate: &str, name: &str, args: &Value) -> Result<String, String> {
    let (channel, board) = board_of_seat(app, seat).ok_or_else(|| "This channel's board is off: board tools work only on the board of a channel you are in.".to_string())?;
    let db = app.state::<Db>();
    let str_arg = |key: &str| args.get(key).and_then(Value::as_str);
    let (people, name_of_mate) = {
        let people = db.with(|conn| Ok(members(conn, &channel)))?;
        let me = people.iter().find(|(id, _)| id == mate).map(|(_, n)| n.clone()).unwrap_or_else(|| mate.to_string());
        (people, me)
    };
    let name_of = |id: &str| -> String {
        if id == "person" {
            "the person".into()
        } else {
            people.iter().find(|(m, _)| m == id).map(|(_, n)| n.clone()).unwrap_or_else(|| id.to_string())
        }
    };
    let column_named = |word: &str| -> Result<String, String> {
        board
            .columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(word.trim()))
            .map(|c| c.id.clone())
            .ok_or_else(|| format!("No column \"{word}\": the columns are {}.", board.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")))
    };
    // A card of this channel's board only (FR-11).
    let card_of = |key: &str| -> Result<Card, String> {
        db.with(|conn| boards::card_by_key(conn, key))?
            .filter(|c| c.channel_id == channel.id)
            .ok_or_else(|| format!("No card {key} on this channel's board."))
    };
    match name {
        LIST => {
            let cards = db.with(|conn| boards::list_cards(conn, &channel.id))?;
            let only_column = str_arg("column").map(column_named).transpose()?;
            let only_assignee = resolve_assignee(str_arg("assignee"), &people)?;
            let mut out = format!("Board {} of #{}:\n", board.key, channel.name);
            for column in &board.columns {
                if only_column.as_ref().is_some_and(|c| *c != column.id) {
                    continue;
                }
                let here: Vec<&Card> = cards
                    .iter()
                    .filter(|c| c.column_id == column.id)
                    .filter(|c| only_assignee.is_none() || c.assignee == only_assignee)
                    .collect();
                out.push_str(&format!("\n{} ({})\n", column.name, here.len()));
                for card in here {
                    out.push_str(&format!("- {}\n", card_line(card, &name_of)));
                }
            }
            Ok(out)
        }
        CREATE => {
            let new = NewCard {
                title: str_arg("title").unwrap_or_default().to_string(),
                description: str_arg("description").unwrap_or_default().to_string(),
                assignee: resolve_assignee(str_arg("assignee"), &people)?,
                priority: str_arg("priority").map(str::to_string),
                due_ms: str_arg("due").and_then(parse_due),
                labels: args
                    .get("labels")
                    .and_then(Value::as_array)
                    .map(|l| l.iter().filter_map(Value::as_str).map(str::to_string).collect())
                    .unwrap_or_default(),
                column_id: str_arg("column").map(column_named).transpose()?,
                ..NewCard::default()
            };
            let card = db.with(|conn| boards::create_card(conn, &channel.id, &new, mate))?;
            changed(app, &channel.id);
            let mut line = format!("{name_of_mate} added {} {}", card.key, card.title);
            if let Some(a) = &card.assignee {
                line.push_str(&format!(" for {}", name_of(a)));
            }
            post_line(app, &channel.chat_id, &line, &card).await;
            Ok(format!("Added {}.", card_line(&card, &name_of)))
        }
        UPDATE => {
            let card = card_of(str_arg("card").unwrap_or_default())?;
            let mut patch = CardPatch {
                title: str_arg("title").map(str::to_string),
                description: str_arg("description").map(str::to_string),
                priority: str_arg("priority").map(str::to_string),
                labels: args.get("labels").and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).map(str::to_string).collect()),
                checklist: args
                    .get("checklist")
                    .and_then(|c| serde_json::from_value::<Vec<CheckItem>>(c.clone()).ok()),
                ..CardPatch::default()
            };
            if args.get("assignee").is_some() {
                patch.assignee = Some(resolve_assignee(str_arg("assignee"), &people)?);
            }
            if let Some(due) = args.get("due") {
                patch.due_ms = Some(due.as_str().and_then(parse_due));
            }
            let updated = db.with(|conn| boards::update_card(conn, &card.id, &patch, mate))?;
            changed(app, &channel.id);
            Ok(format!("Updated {}.", card_line(&updated, &name_of)))
        }
        MOVE => {
            let card = card_of(str_arg("card").unwrap_or_default())?;
            let column = column_named(str_arg("column").unwrap_or_default())?;
            if card.column_id == column {
                return Ok(format!("{} is already there.", card.key));
            }
            let moved = db.with(|conn| boards::move_card(conn, &card.id, &column, None, mate))?;
            changed(app, &channel.id);
            let to = board.columns.iter().find(|c| c.id == column).map(|c| c.name.clone()).unwrap_or_default();
            post_line(app, &channel.chat_id, &format!("{name_of_mate} moved {} to {to}", moved.key), &moved).await;
            Ok(format!("Moved {} to {to}.", moved.key))
        }
        COMMENT => {
            let card = card_of(str_arg("card").unwrap_or_default())?;
            db.with(|conn| boards::comment_card(conn, &card.id, mate, str_arg("text").unwrap_or_default()))?;
            changed(app, &channel.id);
            Ok(format!("Commented on {}.", card.key))
        }
        other => Err(format!("unknown board tool {other}")),
    }
}

/// A board line in the channel (FR-13): shown in the feed, never delivered.
async fn post_line(app: &AppHandle, chat: &str, text: &str, card: &Card) {
    use crate::supervisor::Host as _;
    let host = crate::supervisor::AppHost(app.clone());
    let _ = host
        .request(chat, "channel_post_line", json!({ "text": text, "card": card.key, "cardId": card.id }))
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignees_are_members_or_the_person() {
        let members = vec![("kai".to_string(), "Kai".to_string()), ("lin".to_string(), "Lin".to_string())];
        assert_eq!(resolve_assignee(Some("@kai"), &members), Ok(Some("kai".into())));
        assert_eq!(resolve_assignee(Some("You"), &members), Ok(Some("person".into())));
        assert_eq!(resolve_assignee(None, &members), Ok(None));
        assert!(resolve_assignee(Some("Mo"), &members).unwrap_err().contains("Kai, Lin"));
    }

    #[test]
    fn due_dates_read_as_local_time() {
        use chrono::TimeZone;
        let day = parse_due("2026-12-04").unwrap();
        let at = chrono::Local.timestamp_millis_opt(day).single().unwrap();
        assert_eq!(at.format("%Y-%m-%d %H:%M").to_string(), "2026-12-04 23:59");
        assert!(parse_due("2026-12-04T17:00").is_some());
        assert_eq!(parse_due("Friday"), None);
    }
}
