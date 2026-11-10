//! What automatic indexing keeps per chat: a summary of the chat and how far
//! its transcript was read.
//!
//! The shell's index worker writes a row after each index run; past-chat
//! search matches the summary ([`crate::chats::find_chats`]). The row goes
//! with its chat. Design: `docs/features/memory-topics-and-chat-recall/plan.md`
//! (3.4, 3.5).

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::DbError;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatIndex {
    pub chat_id: String,
    /// What was asked, done, decided and left open.
    pub summary: Option<String>,
    /// The last transcript entry an index run read.
    pub indexed_through: Option<String>,
    pub indexed_at: Option<i64>,
    /// When the last run failed; cleared by the next good one.
    pub failed_at: Option<i64>,
    pub error: Option<String>,
}

pub fn get(conn: &Connection, chat_id: &str) -> Result<Option<ChatIndex>, DbError> {
    Ok(conn
        .query_row(
            "SELECT chat_id, summary, indexed_through, indexed_at, failed_at, error
             FROM chat_index WHERE chat_id = ?1",
            [chat_id],
            |row| {
                Ok(ChatIndex {
                    chat_id: row.get(0)?,
                    summary: row.get(1)?,
                    indexed_through: row.get(2)?,
                    indexed_at: row.get(3)?,
                    failed_at: row.get(4)?,
                    error: row.get(5)?,
                })
            },
        )
        .optional()?)
}

fn chat_exists(conn: &Connection, chat_id: &str) -> Result<bool, DbError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM chats WHERE id = ?1)",
        [chat_id],
        |row| row.get(0),
    )?)
}

/// Record a finished index run at `at_ms`. `summary` and `through` replace
/// what was kept when given and leave it when `None` (a run that found
/// nothing new). Clears an earlier failure. `false` when the chat is gone.
pub fn set_indexed(
    conn: &Connection,
    chat_id: &str,
    summary: Option<&str>,
    through: Option<&str>,
    at_ms: i64,
) -> Result<bool, DbError> {
    if !chat_exists(conn, chat_id)? {
        return Ok(false);
    }
    conn.execute(
        "INSERT INTO chat_index (chat_id, summary, indexed_through, indexed_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (chat_id) DO UPDATE SET
             summary = COALESCE(excluded.summary, summary),
             indexed_through = COALESCE(excluded.indexed_through, indexed_through),
             indexed_at = excluded.indexed_at,
             failed_at = NULL,
             error = NULL",
        params![chat_id, summary, through, at_ms],
    )?;
    Ok(true)
}

/// Record a failed index run; the summary and the mark stay as they were.
/// `false` when the chat is gone.
pub fn set_failed(
    conn: &Connection,
    chat_id: &str,
    at_ms: i64,
    error: &str,
) -> Result<bool, DbError> {
    if !chat_exists(conn, chat_id)? {
        return Ok(false);
    }
    conn.execute(
        "INSERT INTO chat_index (chat_id, failed_at, error) VALUES (?1, ?2, ?3)
         ON CONFLICT (chat_id) DO UPDATE SET failed_at = excluded.failed_at, error = excluded.error",
        params![chat_id, at_ms, error],
    )?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chats::{self, NewChat};

    #[test]
    fn chat_index_keeps_the_summary_and_the_mark() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        chats::create_chat(
            &conn,
            &NewChat {
                id: "c".into(),
                title: "C".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(get(&conn, "c").unwrap(), None);
        assert!(set_indexed(&conn, "c", Some("Chose SQLite."), Some("e5"), 10).unwrap());
        // A run that found nothing new keeps the summary and the mark.
        assert!(set_indexed(&conn, "c", None, None, 20).unwrap());
        let kept = get(&conn, "c").unwrap().unwrap();
        assert_eq!(
            (
                kept.summary.as_deref(),
                kept.indexed_through.as_deref(),
                kept.indexed_at
            ),
            (Some("Chose SQLite."), Some("e5"), Some(20))
        );
        // A failure keeps both; the next good run clears it.
        assert!(set_failed(&conn, "c", 30, "model gone").unwrap());
        let failed = get(&conn, "c").unwrap().unwrap();
        assert_eq!(
            (failed.failed_at, failed.error.as_deref(), failed.indexed_at),
            (Some(30), Some("model gone"), Some(20))
        );
        assert!(set_indexed(&conn, "c", Some("Chose SQLite; WAL on."), Some("e9"), 40).unwrap());
        let cleared = get(&conn, "c").unwrap().unwrap();
        assert_eq!((cleared.failed_at, cleared.error), (None, None));
        assert_eq!(cleared.indexed_through.as_deref(), Some("e9"));
        // A chat that is gone takes no row; its row went with it.
        chats::delete_chat(&mut conn, "c").unwrap();
        assert_eq!(get(&conn, "c").unwrap(), None);
        assert!(!set_indexed(&conn, "c", Some("x"), None, 50).unwrap());
        assert!(!set_failed(&conn, "c", 50, "x").unwrap());
    }
}
