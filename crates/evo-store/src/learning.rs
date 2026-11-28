//! What the learning layers keep per memory owner: when it was last tidied
//! and dreamed, how many runs failed in a row, how far each chat was
//! reviewed, and the ideas the dream offered the person.
//!
//! Owners are `personal`, `project:<slug>` and `mate:<id>`.
//! Design: `docs/features/learning-layers/spec.md` (FR-6, FR-10, FR-14,
//! FR-16, NFR-5), plan 3.4.

use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;

use crate::DbError;

/// One owner's learning state.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub owner: String,
    /// The local day (`YYYY-MM-DD`) of the last nightly tidy-up.
    pub tidied_on: Option<String>,
    /// When the last dream ran.
    pub dreamed_at_ms: Option<i64>,
    /// Runs that failed in a row (NFR-5).
    pub failures: i64,
    /// Per chat: the tool-step count at its last skill review (FR-16).
    pub review_steps: BTreeMap<String, i64>,
}

/// The state of `owner`; a fresh one when it has none yet.
pub fn state(conn: &Connection, owner: &str) -> Result<State, DbError> {
    let found = conn
        .query_row(
            "SELECT tidied_on, dreamed_at_ms, failures, review_steps FROM learning_state
             WHERE owner = ?1",
            [owner],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    Ok(match found {
        Some((tidied_on, dreamed_at_ms, failures, steps)) => State {
            owner: owner.to_string(),
            tidied_on,
            dreamed_at_ms,
            failures,
            review_steps: serde_json::from_str(&steps).unwrap_or_default(),
        },
        None => State {
            owner: owner.to_string(),
            ..Default::default()
        },
    })
}

/// Store `state` under its owner.
pub fn put_state(conn: &Connection, state: &State) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO learning_state (owner, tidied_on, dreamed_at_ms, failures, review_steps)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (owner) DO UPDATE SET tidied_on = ?2, dreamed_at_ms = ?3, failures = ?4,
             review_steps = ?5",
        params![
            state.owner,
            state.tidied_on,
            state.dreamed_at_ms,
            state.failures,
            serde_json::to_string(&state.review_steps).unwrap_or_else(|_| "{}".into()),
        ],
    )?;
    Ok(())
}

/// Every owner with a stored state.
pub fn owners(conn: &Connection) -> Result<Vec<String>, DbError> {
    let mut statement = conn.prepare("SELECT owner FROM learning_state ORDER BY owner")?;
    let rows = statement.query_map([], |row| row.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub const SUGGESTION_KINDS: [&str; 2] = ["schedule", "skill"];

/// An idea the dream offers (FR-14).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub id: i64,
    pub owner: String,
    /// `schedule` or `skill`.
    pub kind: String,
    pub text: String,
    pub from_chats: Vec<String>,
    /// `pending`, `accepted` or `declined`.
    pub status: String,
    pub at_ms: i64,
}

fn suggestion_row(row: &Row<'_>) -> rusqlite::Result<Suggestion> {
    Ok(Suggestion {
        id: row.get(0)?,
        owner: row.get(1)?,
        kind: row.get(2)?,
        text: row.get(3)?,
        from_chats: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
        status: row.get(5)?,
        at_ms: row.get(6)?,
    })
}

/// The same idea, however it is spaced, cased or punctuated.
fn folded(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Offer an idea. `None` when the same idea was offered before, whatever
/// the person answered: a declined idea is not offered again.
pub fn add_suggestion(
    conn: &Connection,
    owner: &str,
    kind: &str,
    text: &str,
    from_chats: &[String],
    at_ms: i64,
) -> Result<Option<i64>, DbError> {
    let wanted = folded(text);
    let mut statement = conn.prepare("SELECT text FROM learning_suggestions WHERE owner = ?1")?;
    let known: Vec<String> = statement
        .query_map([owner], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    if wanted.is_empty() || known.iter().any(|text| folded(text) == wanted) {
        return Ok(None);
    }
    conn.execute(
        "INSERT INTO learning_suggestions (owner, kind, text, from_chats, at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            owner,
            kind,
            text,
            serde_json::to_string(from_chats).unwrap_or_else(|_| "[]".into()),
            at_ms
        ],
    )?;
    Ok(Some(conn.last_insert_rowid()))
}

/// The ideas still waiting for an answer, newest first.
pub fn pending(conn: &Connection, owners: &[&str]) -> Result<Vec<Suggestion>, DbError> {
    if owners.is_empty() {
        return Ok(Vec::new());
    }
    let marks = vec!["?"; owners.len()].join(", ");
    let mut statement = conn.prepare(&format!(
        "SELECT id, owner, kind, text, from_chats, status, at_ms FROM learning_suggestions
         WHERE status = 'pending' AND owner IN ({marks}) ORDER BY id DESC"
    ))?;
    let rows = statement.query_map(rusqlite::params_from_iter(owners.iter()), suggestion_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn suggestion(conn: &Connection, id: i64) -> Result<Option<Suggestion>, DbError> {
    Ok(conn
        .query_row(
            "SELECT id, owner, kind, text, from_chats, status, at_ms FROM learning_suggestions
             WHERE id = ?1",
            [id],
            suggestion_row,
        )
        .optional()?)
}

/// Answer a pending idea: `accepted` or `declined`. `false` when it was
/// answered before or does not exist.
pub fn answer(conn: &Connection, id: i64, accepted: bool) -> Result<bool, DbError> {
    Ok(conn.execute(
        "UPDATE learning_suggestions SET status = ?2 WHERE id = ?1 AND status = 'pending'",
        params![id, if accepted { "accepted" } else { "declined" }],
    )? == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        (dir, conn)
    }

    #[test]
    fn learning_state_round_trips_per_owner() {
        let (_dir, conn) = open();
        let fresh = state(&conn, "mate:m1").unwrap();
        assert_eq!((fresh.tidied_on.as_deref(), fresh.failures), (None, 0));
        let mut changed = fresh.clone();
        changed.tidied_on = Some("2026-11-28".into());
        changed.dreamed_at_ms = Some(9);
        changed.failures = 2;
        changed.review_steps.insert("c1".into(), 12);
        put_state(&conn, &changed).unwrap();
        assert_eq!(state(&conn, "mate:m1").unwrap(), changed);
        assert_eq!(state(&conn, "personal").unwrap().tidied_on, None);
        changed.failures = 0;
        put_state(&conn, &changed).unwrap();
        assert_eq!(state(&conn, "mate:m1").unwrap().failures, 0);
        assert_eq!(owners(&conn).unwrap(), ["mate:m1"]);
    }

    // FR-14: an idea is offered once; a declined one never again.
    #[test]
    fn suggestions_are_offered_once_and_answered_once() {
        let (_dir, conn) = open();
        let chats = vec!["c1".to_string(), "c2".to_string()];
        let id = add_suggestion(
            &conn,
            "mate:m1",
            "schedule",
            "Send the week's numbers on Mondays.",
            &chats,
            5,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            add_suggestion(
                &conn,
                "mate:m1",
                "schedule",
                "send the week's numbers on mondays",
                &chats,
                6
            )
            .unwrap(),
            None,
            "the same idea is not offered twice"
        );
        let waiting = pending(&conn, &["mate:m1"]).unwrap();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].from_chats, chats);
        assert!(pending(&conn, &["personal"]).unwrap().is_empty());
        assert!(answer(&conn, id, false).unwrap());
        assert!(!answer(&conn, id, true).unwrap(), "answered once");
        assert!(pending(&conn, &["mate:m1"]).unwrap().is_empty());
        assert_eq!(suggestion(&conn, id).unwrap().unwrap().status, "declined");
        assert_eq!(
            add_suggestion(
                &conn,
                "mate:m1",
                "schedule",
                "Send the week's numbers on Mondays!",
                &chats,
                7
            )
            .unwrap(),
            None,
            "a declined idea is not offered again"
        );
        // Another owner may get the same idea.
        assert!(add_suggestion(
            &conn,
            "personal",
            "skill",
            "Send the week's numbers on Mondays.",
            &chats,
            8
        )
        .unwrap()
        .is_some());
    }
}
