//! A mate's agenda: the open threads it comes back to and the check-ins the
//! person scheduled (docs/features/evo-mates/spec.md, FR-63 – FR-71; plan
//! 3.9), and the count of first messages for its daily budget.
//!
//! Times are ms since the epoch. A check-in's `repeat` is the shell
//! Scheduler's schedule as JSON; the shell computes its next time, so this
//! module never reads the local clock or time zone.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::chats::now_ms;
use crate::DbError;

pub const KINDS: [&str; 4] = ["promise", "event", "question", "checkin"];
pub const STATUSES: [&str; 4] = ["open", "done", "never", "paused"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub mate_id: String,
    /// `promise`, `event`, `question` or `checkin`.
    pub kind: String,
    pub text: String,
    pub due_at: i64,
    /// A check-in's schedule (the Scheduler's JSON); `None`: once.
    pub repeat: Option<String>,
    /// `open`, `done`, `never` or `paused`.
    pub status: String,
    pub chat_id: Option<String>,
    pub created_at: i64,
    pub fired_at: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewItem {
    pub id: String,
    pub mate_id: String,
    pub kind: String,
    pub text: String,
    pub due_at: i64,
    pub repeat: Option<String>,
    pub chat_id: Option<String>,
}

const COLUMNS: &str =
    "id, mate_id, kind, text, due_at, repeat, status, chat_id, created_at, fired_at";

fn item_row(row: &Row<'_>) -> rusqlite::Result<Item> {
    Ok(Item {
        id: row.get(0)?,
        mate_id: row.get(1)?,
        kind: row.get(2)?,
        text: row.get(3)?,
        due_at: row.get(4)?,
        repeat: row.get(5)?,
        status: row.get(6)?,
        chat_id: row.get(7)?,
        created_at: row.get(8)?,
        fired_at: row.get(9)?,
    })
}

/// Add an item. The text is trimmed; an item the mate marked "never" with the
/// same text is not added again (FR-70).
pub fn add_item(conn: &Connection, item: &NewItem) -> Result<Option<Item>, DbError> {
    if !KINDS.contains(&item.kind.as_str()) {
        return Err(DbError::Invalid(format!(
            "unknown agenda kind `{}`",
            item.kind
        )));
    }
    let text = item.text.trim();
    if text.is_empty() {
        return Err(DbError::Invalid("an agenda item needs a text".into()));
    }
    let never: bool = conn
        .query_row(
            "SELECT 1 FROM mate_agenda WHERE mate_id = ?1 AND status = 'never' AND lower(text) = lower(?2)",
            params![item.mate_id, text],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if never {
        return Ok(None);
    }
    conn.execute(
        "INSERT INTO mate_agenda (id, mate_id, kind, text, due_at, repeat, chat_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            item.id,
            item.mate_id,
            item.kind,
            text,
            item.due_at,
            item.repeat,
            item.chat_id,
            now_ms()
        ],
    )?;
    get_item(conn, &item.id)
}

pub fn get_item(conn: &Connection, id: &str) -> Result<Option<Item>, DbError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM mate_agenda WHERE id = ?1"),
            [id],
            item_row,
        )
        .optional()?)
}

/// A mate's open and paused items (and, with `all`, done and never ones), soonest first.
pub fn list_items(conn: &Connection, mate: &str, all: bool) -> Result<Vec<Item>, DbError> {
    let filter = if all {
        ""
    } else {
        " AND status IN ('open', 'paused')"
    };
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM mate_agenda WHERE mate_id = ?1{filter} ORDER BY due_at, created_at"
    ))?;
    let rows = statement.query_map([mate], item_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// A mate's open items due at `now`.
pub fn due_items(conn: &Connection, mate: &str, now: i64) -> Result<Vec<Item>, DbError> {
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM mate_agenda WHERE mate_id = ?1 AND status = 'open' AND due_at <= ?2 ORDER BY due_at"
    ))?;
    let rows = statement.query_map(params![mate, now], item_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The mates with an open item due at `now`.
pub fn mates_with_due(conn: &Connection, now: i64) -> Result<Vec<String>, DbError> {
    let mut statement =
        conn.prepare("SELECT DISTINCT mate_id FROM mate_agenda WHERE status = 'open' AND due_at <= ?1 ORDER BY mate_id")?;
    let rows = statement.query_map([now], |row| row.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn set_status(conn: &Connection, mate: &str, id: &str, status: &str) -> Result<bool, DbError> {
    if !STATUSES.contains(&status) {
        return Err(DbError::Invalid(format!(
            "unknown agenda status `{status}`"
        )));
    }
    Ok(conn.execute(
        "UPDATE mate_agenda SET status = ?3 WHERE id = ?1 AND mate_id = ?2",
        params![id, mate, status],
    )? > 0)
}

/// Change an item's text, time or repeat (a check-in the person edits).
pub fn update_item(
    conn: &Connection,
    mate: &str,
    id: &str,
    text: Option<&str>,
    due_at: Option<i64>,
    repeat: Option<Option<&str>>,
) -> Result<Option<Item>, DbError> {
    if let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) {
        conn.execute(
            "UPDATE mate_agenda SET text = ?3 WHERE id = ?1 AND mate_id = ?2",
            params![id, mate, text],
        )?;
    }
    if let Some(due) = due_at {
        conn.execute(
            "UPDATE mate_agenda SET due_at = ?3 WHERE id = ?1 AND mate_id = ?2",
            params![id, mate, due],
        )?;
    }
    if let Some(repeat) = repeat {
        conn.execute(
            "UPDATE mate_agenda SET repeat = ?3 WHERE id = ?1 AND mate_id = ?2",
            params![id, mate, repeat],
        )?;
    }
    get_item(conn, id).map(|item| item.filter(|i| i.mate_id == mate))
}

pub fn delete_item(conn: &Connection, mate: &str, id: &str) -> Result<bool, DbError> {
    Ok(conn.execute(
        "DELETE FROM mate_agenda WHERE id = ?1 AND mate_id = ?2",
        params![id, mate],
    )? > 0)
}

/// A heartbeat took `item` up at `now`: a one-off item is done (a mate comes
/// back to it once); a check-in moves to `next` (its next time), or is done
/// when it has none.
pub fn fired(conn: &Connection, id: &str, now: i64, next: Option<i64>) -> Result<(), DbError> {
    match next {
        Some(next) => conn.execute(
            "UPDATE mate_agenda SET fired_at = ?2, due_at = ?3 WHERE id = ?1",
            params![id, now, next],
        )?,
        None => conn.execute(
            "UPDATE mate_agenda SET fired_at = ?2, status = 'done' WHERE id = ?1",
            params![id, now],
        )?,
    };
    Ok(())
}

/// First messages `mate` sent on `day` (`YYYY-MM-DD`, local).
pub fn sent_on(conn: &Connection, mate: &str, day: &str) -> Result<i64, DbError> {
    Ok(conn
        .query_row(
            "SELECT sent FROM mate_initiative WHERE mate_id = ?1 AND day = ?2",
            params![mate, day],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

/// Count a first message: one more for the day, one more unanswered.
pub fn count_first_message(conn: &Connection, mate: &str, day: &str) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO mate_initiative (mate_id, day, sent) VALUES (?1, ?2, 1)
         ON CONFLICT (mate_id, day) DO UPDATE SET sent = sent + 1",
        params![mate, day],
    )?;
    conn.execute(
        "UPDATE mates SET unanswered = unanswered + 1 WHERE id = ?1",
        [mate],
    )?;
    Ok(())
}

/// The person wrote to the mate: it may write first again (FR-65).
pub fn answered(conn: &Connection, mate: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE mates SET unanswered = 0 WHERE id = ?1 AND unanswered <> 0",
        [mate],
    )?;
    Ok(())
}

/// First messages a day for an initiative level, and how many unanswered
/// ones in a row stop it (FR-65).
pub fn budget(initiative: &str) -> (i64, i64) {
    match initiative {
        "medium" => (5, 2),
        "low" => (2, 1),
        _ => (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mates::{create_mate, get_mate, NewMate};
    use serde_json::json;

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        create_mate(
            &mut conn,
            &NewMate {
                id: "m".into(),
                chat_id: "c".into(),
                name: "Ada".into(),
                avatar: json!({}),
                folder: "/m".into(),
                ..NewMate::default()
            },
            "ask",
        )
        .unwrap();
        (dir, conn)
    }

    fn item(id: &str, kind: &str, text: &str, due: i64) -> NewItem {
        NewItem {
            id: id.into(),
            mate_id: "m".into(),
            kind: kind.into(),
            text: text.into(),
            due_at: due,
            ..NewItem::default()
        }
    }

    #[test]
    fn a_new_mate_is_medium_and_items_come_due() {
        let (_dir, conn) = db();
        let mate = get_mate(&conn, "m").unwrap().unwrap();
        assert_eq!((mate.initiative.as_str(), mate.unanswered), ("medium", 0));
        add_item(&conn, &item("a", "event", " Interview at 10 ", 100))
            .unwrap()
            .unwrap();
        add_item(&conn, &item("b", "promise", "Send the draft", 300)).unwrap();
        assert!(add_item(&conn, &item("x", "dream", "?", 1)).is_err());
        assert_eq!(
            get_item(&conn, "a").unwrap().unwrap().text,
            "Interview at 10"
        );
        assert_eq!(
            due_items(&conn, "m", 200)
                .unwrap()
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            ["a"]
        );
        assert_eq!(mates_with_due(&conn, 200).unwrap(), ["m"]);
        assert!(mates_with_due(&conn, 50).unwrap().is_empty());
    }

    #[test]
    fn firing_closes_a_one_off_and_moves_a_check_in() {
        let (_dir, conn) = db();
        add_item(&conn, &item("a", "event", "Interview", 100)).unwrap();
        add_item(
            &conn,
            &NewItem {
                repeat: Some("{\"kind\":\"weekdays\",\"time\":\"09:00\"}".into()),
                ..item("c", "checkin", "Launch status", 100)
            },
        )
        .unwrap();
        fired(&conn, "a", 150, None).unwrap();
        fired(&conn, "c", 150, Some(86_400_100)).unwrap();
        let a = get_item(&conn, "a").unwrap().unwrap();
        assert_eq!((a.status.as_str(), a.fired_at), ("done", Some(150)));
        let c = get_item(&conn, "c").unwrap().unwrap();
        assert_eq!((c.status.as_str(), c.due_at), ("open", 86_400_100));
        assert!(due_items(&conn, "m", 200).unwrap().is_empty());
    }

    #[test]
    fn never_items_do_not_come_back() {
        let (_dir, conn) = db();
        add_item(&conn, &item("a", "question", "Ask about the gym", 100)).unwrap();
        assert!(set_status(&conn, "m", "a", "never").unwrap());
        assert!(
            add_item(&conn, &item("b", "question", "ask about the GYM", 200))
                .unwrap()
                .is_none()
        );
        assert!(list_items(&conn, "m", false).unwrap().is_empty());
        assert_eq!(list_items(&conn, "m", true).unwrap().len(), 1);
        assert!(set_status(&conn, "m", "a", "gone").is_err());
        assert!(
            !set_status(&conn, "other", "a", "done").unwrap(),
            "another mate's item is not touched"
        );
    }

    #[test]
    fn first_messages_count_and_an_answer_resets() {
        let (_dir, conn) = db();
        count_first_message(&conn, "m", "2026-11-26").unwrap();
        count_first_message(&conn, "m", "2026-11-26").unwrap();
        assert_eq!(sent_on(&conn, "m", "2026-11-26").unwrap(), 2);
        assert_eq!(sent_on(&conn, "m", "2026-11-27").unwrap(), 0);
        assert_eq!(get_mate(&conn, "m").unwrap().unwrap().unanswered, 2);
        answered(&conn, "m").unwrap();
        assert_eq!(get_mate(&conn, "m").unwrap().unwrap().unanswered, 0);
        assert_eq!(budget("medium"), (5, 2));
        assert_eq!(budget("off"), (0, 0));
    }

    #[test]
    fn items_change_and_go() {
        let (_dir, conn) = db();
        add_item(&conn, &item("c", "checkin", "Status", 100)).unwrap();
        let changed = update_item(
            &conn,
            "m",
            "c",
            Some("Launch status"),
            Some(500),
            Some(Some("{}")),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            (
                changed.text.as_str(),
                changed.due_at,
                changed.repeat.as_deref()
            ),
            ("Launch status", 500, Some("{}"))
        );
        assert!(delete_item(&conn, "m", "c").unwrap());
        assert!(get_item(&conn, "c").unwrap().is_none());
    }
}
