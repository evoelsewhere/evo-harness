//! The journal: every change to a memory topic or a learned skill, by
//! whoever made it, each one undoable.
//!
//! The engine records one row per change with the whole text before and
//! after, so the Memory panel, a mate's panel and the "Learned" line under a
//! reply can show where a change came from and put the earlier text back.
//! Rows are kept per owner and subject: the newest [`KEEP`] of each.
//! The files themselves are the engine's; this is only their change list,
//! kept in `evo.db` where the agent's file tools cannot write.
//! Design: `docs/features/learning-layers/spec.md` (FR-1, FR-2, FR-18,
//! NFR-6), plan 3.4. Replaces the indexing-only change list of
//! `docs/features/memory-topics-and-chat-recall/spec.md` (FR-29).

use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::Serialize;

use crate::DbError;

/// How many rows are kept per owner and subject, newest first.
pub const KEEP: i64 = 500;

pub const SUBJECTS: [&str; 2] = ["memory", "skill"];
pub const ACTIONS: [&str; 5] = ["added", "updated", "deleted", "archived", "undone"];
pub const AUTHORS: [&str; 8] = [
    "agent", "person", "index", "tidy", "dream", "review", "retro", "undo",
];

/// One change to record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NewRow {
    pub at_ms: i64,
    /// `personal`, `project:<slug>` or `mate:<id>`.
    pub owner: String,
    /// `memory` or `skill`.
    pub subject: String,
    /// The topic or the skill's name.
    pub name: String,
    /// One of [`ACTIONS`].
    pub action: String,
    /// One of [`AUTHORS`].
    pub by: String,
    /// The chat the change came from.
    pub chat_id: Option<String>,
    /// The reply after which the change was made (a skill review's).
    pub reply_id: Option<String>,
    /// A short diff for the list: `-` and `+` lines.
    pub diff: Option<String>,
    /// The text before; `None` when it was added.
    pub before: Option<String>,
    /// The text after; `None` when it was deleted.
    pub after: Option<String>,
    /// `false` when the earlier text held a secret or was too long to keep.
    pub undoable: bool,
}

/// A recorded change, with its chat's title when the chat still exists.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    pub at_ms: i64,
    pub owner: String,
    pub subject: String,
    pub name: String,
    pub action: String,
    pub by: String,
    pub chat_id: Option<String>,
    pub chat_title: Option<String>,
    pub reply_id: Option<String>,
    pub diff: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub undoable: bool,
    pub undone_at: Option<i64>,
}

const COLUMNS: &str = "j.id, j.at_ms, j.owner, j.subject, j.name, j.action, j.by, j.chat_id,
     (SELECT title FROM chats c WHERE c.id = j.chat_id), j.reply_id, j.diff, j.before_text,
     j.after_text, j.undoable, j.undone_at";

fn entry_row(row: &Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get(0)?,
        at_ms: row.get(1)?,
        owner: row.get(2)?,
        subject: row.get(3)?,
        name: row.get(4)?,
        action: row.get(5)?,
        by: row.get(6)?,
        chat_id: row.get(7)?,
        chat_title: row.get(8)?,
        reply_id: row.get(9)?,
        diff: row.get(10)?,
        before: row.get(11)?,
        after: row.get(12)?,
        undoable: row.get::<_, i64>(13)? != 0,
        undone_at: row.get(14)?,
    })
}

/// Record a change and drop all but the newest [`KEEP`] of its owner and
/// subject. Returns its id.
pub fn record(conn: &mut Connection, row: &NewRow) -> Result<i64, DbError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO journal (at_ms, owner, subject, name, action, by, chat_id, reply_id, diff,
                              before_text, after_text, undoable)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            row.at_ms,
            row.owner,
            row.subject,
            row.name,
            row.action,
            row.by,
            row.chat_id,
            row.reply_id,
            row.diff,
            row.before,
            row.after,
            row.undoable as i64,
        ],
    )?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "DELETE FROM journal WHERE owner = ?1 AND subject = ?2 AND id <= (
             SELECT id FROM journal WHERE owner = ?1 AND subject = ?2
             ORDER BY id DESC LIMIT 1 OFFSET ?3)",
        params![row.owner, row.subject, KEEP],
    )?;
    tx.commit()?;
    Ok(id)
}

/// The newest rows of `subject` for any of `owners`, newest first.
pub fn list(
    conn: &Connection,
    owners: &[&str],
    subject: &str,
    limit: u32,
) -> Result<Vec<Entry>, DbError> {
    if owners.is_empty() {
        return Ok(Vec::new());
    }
    let marks = vec!["?"; owners.len()].join(", ");
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM journal j
         WHERE j.subject = ? AND j.owner IN ({marks})
         ORDER BY j.id DESC LIMIT ?"
    ))?;
    let mut values: Vec<rusqlite::types::Value> = vec![subject.to_string().into()];
    values.extend(owners.iter().map(|owner| owner.to_string().into()));
    values.push(i64::from(limit.clamp(1, KEEP as u32)).into());
    let rows = statement.query_map(rusqlite::params_from_iter(values), entry_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The rows a skill review or a retro made after reply `reply_id` of `chat_id`.
pub fn for_reply(conn: &Connection, chat_id: &str) -> Result<Vec<Entry>, DbError> {
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM journal j
         WHERE j.chat_id = ?1 AND j.reply_id IS NOT NULL AND j.subject = 'skill'
         ORDER BY j.id"
    ))?;
    let rows = statement.query_map([chat_id], entry_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<Entry>, DbError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM journal j WHERE j.id = ?1"),
            [id],
            entry_row,
        )
        .optional()?)
}

/// The newest row of `name` (`subject`, `owner`) that is not undone.
pub fn last_of(
    conn: &Connection,
    owner: &str,
    subject: &str,
    name: &str,
) -> Result<Option<Entry>, DbError> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT {COLUMNS} FROM journal j
                 WHERE j.owner = ?1 AND j.subject = ?2 AND j.name = ?3 AND j.undone_at IS NULL
                   AND j.action <> 'undone'
                 ORDER BY j.id DESC LIMIT 1"
            ),
            params![owner, subject, name],
            entry_row,
        )
        .optional()?)
}

/// Mark a row as undone at `at_ms`. `false` when there is no such row or it
/// was undone before.
pub fn mark_undone(conn: &Connection, id: i64, at_ms: i64) -> Result<bool, DbError> {
    Ok(conn.execute(
        "UPDATE journal SET undone_at = ?2 WHERE id = ?1 AND undone_at IS NULL",
        params![id, at_ms],
    )? == 1)
}

/// Forget every row of `subject` (memory was reset). Returns how many went.
pub fn clear(conn: &Connection, subject: &str) -> Result<usize, DbError> {
    Ok(conn.execute("DELETE FROM journal WHERE subject = ?1", [subject])?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chats::{self, NewChat};

    fn row(owner: &str, name: &str, by: &str) -> NewRow {
        NewRow {
            at_ms: 1,
            owner: owner.into(),
            subject: "memory".into(),
            name: name.into(),
            action: "updated".into(),
            by: by.into(),
            chat_id: Some("c".into()),
            before: Some("old".into()),
            after: Some("new".into()),
            undoable: true,
            ..Default::default()
        }
    }

    fn open() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        (dir, conn)
    }

    // FR-1, FR-2: rows by owner and author, undone once.
    #[test]
    fn journal_lists_by_owner_and_marks_undone() {
        let (_dir, mut conn) = open();
        chats::create_chat(
            &conn,
            &NewChat {
                id: "c".into(),
                title: "Release plan".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let personal = record(&mut conn, &row("personal", "role", "index")).unwrap();
        let mine = record(&mut conn, &row("project:C--work-app", "freeze", "agent")).unwrap();
        record(&mut conn, &row("project:C--other", "elsewhere", "person")).unwrap();
        record(&mut conn, &row("mate:m1", "style", "dream")).unwrap();

        let here = list(&conn, &["personal", "project:C--work-app"], "memory", 50).unwrap();
        assert_eq!(
            here.iter()
                .map(|e| (e.name.as_str(), e.by.as_str()))
                .collect::<Vec<_>>(),
            [("freeze", "agent"), ("role", "index")],
            "newest first; another owner's rows are not listed"
        );
        assert_eq!(here[0].chat_title.as_deref(), Some("Release plan"));
        assert_eq!(list(&conn, &["mate:m1"], "memory", 50).unwrap().len(), 1);
        assert!(list(&conn, &["mate:m1"], "skill", 50).unwrap().is_empty());

        assert!(mark_undone(&conn, mine, 9).unwrap());
        assert!(!mark_undone(&conn, mine, 10).unwrap(), "only once");
        assert!(!mark_undone(&conn, 999, 10).unwrap());
        assert_eq!(get(&conn, mine).unwrap().unwrap().undone_at, Some(9));
        assert_eq!(get(&conn, personal).unwrap().unwrap().undone_at, None);

        // The journal outlives its chat, without the title.
        chats::delete_chat(&mut conn, "c").unwrap();
        assert_eq!(get(&conn, mine).unwrap().unwrap().chat_title, None);

        assert_eq!(clear(&conn, "memory").unwrap(), 4);
        assert!(list(&conn, &["personal"], "memory", 50).unwrap().is_empty());
    }

    // NFR-6: the newest 500 per owner and subject.
    #[test]
    fn journal_keeps_500_per_owner_and_subject() {
        let (_dir, mut conn) = open();
        let other = record(&mut conn, &row("mate:m2", "kept", "agent")).unwrap();
        let mut skill = row("mate:m1", "s", "review");
        skill.subject = "skill".into();
        let skill_id = record(&mut conn, &skill).unwrap();
        let mut last = 0;
        for index in 0..(KEEP + 100) {
            last = record(&mut conn, &row("mate:m1", &format!("t{index}"), "agent")).unwrap();
        }
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM journal WHERE owner = 'mate:m1' AND subject = 'memory'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, KEEP);
        assert_eq!(list(&conn, &["mate:m1"], "memory", 1).unwrap()[0].id, last);
        assert_eq!(
            list(&conn, &["mate:m1"], "memory", 1000).unwrap().len(),
            500
        );
        assert!(
            get(&conn, other).unwrap().is_some(),
            "another owner keeps its rows"
        );
        assert!(
            get(&conn, skill_id).unwrap().is_some(),
            "another subject keeps its rows"
        );
    }

    #[test]
    fn journal_refuses_unknown_subjects_actions_and_authors() {
        let (_dir, mut conn) = open();
        let mut odd = row("personal", "x", "agent");
        odd.subject = "files".into();
        assert!(record(&mut conn, &odd).is_err());
        let mut odd = row("personal", "x", "agent");
        odd.action = "renamed".into();
        assert!(record(&mut conn, &odd).is_err());
        assert!(record(&mut conn, &row("personal", "x", "somebody")).is_err());
        for by in AUTHORS {
            record(&mut conn, &row("personal", "x", by)).unwrap();
        }
        assert!(SUBJECTS.contains(&"skill") && ACTIONS.contains(&"archived"));
    }

    // FR-18: the skill rows a reply led to, and the last live row of a name.
    #[test]
    fn journal_finds_a_replys_skill_rows_and_the_last_change() {
        let (_dir, mut conn) = open();
        let mut learned = row("mate:m1", "drafting", "review");
        learned.subject = "skill".into();
        learned.reply_id = Some("e9".into());
        let first = record(&mut conn, &learned).unwrap();
        learned.chat_id = Some("other".into());
        record(&mut conn, &learned).unwrap();
        let rows = for_reply(&conn, "c").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].id, rows[0].reply_id.as_deref()),
            (first, Some("e9"))
        );
        let last = last_of(&conn, "mate:m1", "skill", "drafting")
            .unwrap()
            .unwrap();
        assert_ne!(last.id, first);
        mark_undone(&conn, last.id, 5).unwrap();
        assert_eq!(
            last_of(&conn, "mate:m1", "skill", "drafting")
                .unwrap()
                .unwrap()
                .id,
            first
        );
    }
}
