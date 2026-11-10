//! The changes automatic indexing made to memory topics.
//!
//! The engine records one row per change, with the whole topic file before
//! and after, so the Memory panel can show each change with the chat it came
//! from and undo it. The list keeps the newest [`KEEP`] changes. The memory
//! files themselves are the engine's; this is only their change list.
//! Design: `docs/features/memory-topics-and-chat-recall/spec.md` (FR-29).

use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::Serialize;

use crate::DbError;

/// How many changes are kept, newest first.
pub const KEEP: i64 = 200;

pub const SCOPES: [&str; 2] = ["personal", "project"];
pub const ACTIONS: [&str; 3] = ["added", "updated", "deleted"];

/// One change to record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NewChange {
    pub at_ms: i64,
    /// `personal` or `project`.
    pub scope: String,
    /// The folder slug of a project topic; `None` for a personal one.
    pub project: Option<String>,
    pub name: String,
    /// `added`, `updated` or `deleted`.
    pub action: String,
    /// The chat whose index run made the change.
    pub chat_id: Option<String>,
    /// The topic file before; `None` when it was added.
    pub before: Option<String>,
    /// The topic file after; `None` when it was deleted.
    pub after: Option<String>,
}

/// A recorded change, with its chat's title when the chat still exists.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub id: i64,
    pub at_ms: i64,
    pub scope: String,
    pub project: Option<String>,
    pub name: String,
    pub action: String,
    pub chat_id: Option<String>,
    pub chat_title: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub undone_at: Option<i64>,
}

const COLUMNS: &str = "m.id, m.at_ms, m.scope, m.project, m.name, m.action, m.chat_id,
     (SELECT title FROM chats c WHERE c.id = m.chat_id), m.before_text, m.after_text, m.undone_at";

fn change_row(row: &Row<'_>) -> rusqlite::Result<Change> {
    Ok(Change {
        id: row.get(0)?,
        at_ms: row.get(1)?,
        scope: row.get(2)?,
        project: row.get(3)?,
        name: row.get(4)?,
        action: row.get(5)?,
        chat_id: row.get(6)?,
        chat_title: row.get(7)?,
        before: row.get(8)?,
        after: row.get(9)?,
        undone_at: row.get(10)?,
    })
}

/// Record a change and drop all but the newest [`KEEP`]. Returns its id.
pub fn record(conn: &mut Connection, change: &NewChange) -> Result<i64, DbError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO memory_changes (at_ms, scope, project, name, action, chat_id, before_text, after_text)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            change.at_ms,
            change.scope,
            change.project,
            change.name,
            change.action,
            change.chat_id,
            change.before,
            change.after
        ],
    )?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "DELETE FROM memory_changes
         WHERE id <= (SELECT id FROM memory_changes ORDER BY id DESC LIMIT 1 OFFSET ?1)",
        [KEEP],
    )?;
    tx.commit()?;
    Ok(id)
}

/// The newest changes to personal topics and to the topics of `project`
/// (a folder slug; `None`: personal only), newest first.
pub fn list(conn: &Connection, project: Option<&str>, limit: u32) -> Result<Vec<Change>, DbError> {
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM memory_changes m
         WHERE m.scope = 'personal' OR (m.scope = 'project' AND m.project IS ?1)
         ORDER BY m.id DESC LIMIT ?2"
    ))?;
    let rows = statement.query_map(params![project, limit.clamp(1, KEEP as u32)], change_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<Change>, DbError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM memory_changes m WHERE m.id = ?1"),
            [id],
            change_row,
        )
        .optional()?)
}

/// Mark a change as undone at `at_ms`. `false` when there is no such change
/// or it was undone before.
pub fn mark_undone(conn: &Connection, id: i64, at_ms: i64) -> Result<bool, DbError> {
    Ok(conn.execute(
        "UPDATE memory_changes SET undone_at = ?2 WHERE id = ?1 AND undone_at IS NULL",
        params![id, at_ms],
    )? == 1)
}

/// Forget the whole list (memory was reset). Returns how many rows went.
pub fn clear(conn: &Connection) -> Result<usize, DbError> {
    Ok(conn.execute("DELETE FROM memory_changes", [])?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chats::{self, NewChat};

    fn change(scope: &str, project: Option<&str>, name: &str) -> NewChange {
        NewChange {
            at_ms: 1,
            scope: scope.into(),
            project: project.map(str::to_string),
            name: name.into(),
            action: "updated".into(),
            chat_id: Some("c".into()),
            before: Some("old".into()),
            after: Some("new".into()),
        }
    }

    #[test]
    fn memory_log_lists_by_project_and_marks_undone() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        chats::create_chat(
            &conn,
            &NewChat {
                id: "c".into(),
                title: "Release plan".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let personal = record(&mut conn, &change("personal", None, "role")).unwrap();
        let mine = record(&mut conn, &change("project", Some("C--work-app"), "freeze")).unwrap();
        record(&mut conn, &change("project", Some("C--other"), "elsewhere")).unwrap();

        let here = list(&conn, Some("C--work-app"), 50).unwrap();
        assert_eq!(
            here.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["freeze", "role"],
            "newest first; another project's changes are not listed"
        );
        assert_eq!(here[0].chat_title.as_deref(), Some("Release plan"));
        assert_eq!(
            (here[0].before.as_deref(), here[0].after.as_deref()),
            (Some("old"), Some("new"))
        );
        assert_eq!(list(&conn, None, 50).unwrap().len(), 1, "personal only");

        assert!(mark_undone(&conn, mine, 9).unwrap());
        assert!(!mark_undone(&conn, mine, 10).unwrap(), "only once");
        assert!(!mark_undone(&conn, 999, 10).unwrap());
        assert_eq!(get(&conn, mine).unwrap().unwrap().undone_at, Some(9));
        assert_eq!(get(&conn, personal).unwrap().unwrap().undone_at, None);

        // The list outlives its chat, without the title.
        chats::delete_chat(&mut conn, "c").unwrap();
        assert_eq!(get(&conn, mine).unwrap().unwrap().chat_title, None);

        assert_eq!(clear(&conn).unwrap(), 3);
        assert!(list(&conn, Some("C--work-app"), 50).unwrap().is_empty());
    }

    #[test]
    fn memory_log_keeps_the_newest_changes() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        let mut last = 0;
        for index in 0..(KEEP + 5) {
            last = record(&mut conn, &change("personal", None, &format!("t{index}"))).unwrap();
        }
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM memory_changes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, KEEP);
        let newest = list(&conn, None, 1).unwrap();
        assert_eq!(newest[0].id, last);
        assert!(get(&conn, 1).unwrap().is_none(), "the oldest went");
    }

    #[test]
    fn memory_log_refuses_unknown_scopes_and_actions() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        assert!(record(&mut conn, &change("elsewhere", None, "x")).is_err());
        let mut odd = change("personal", None, "x");
        odd.action = "renamed".into();
        assert!(record(&mut conn, &odd).is_err());
        assert!(SCOPES.contains(&"project") && ACTIONS.contains(&"deleted"));
    }
}
