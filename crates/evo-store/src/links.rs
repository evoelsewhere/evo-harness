//! Linked conversations and followed threads
//! (docs/features/evo-mates-threads-board/spec.md, FR-16, FR-18 – FR-21).
//!
//! A share records lines going from one conversation into another, so both
//! list each other under Linked. A thread's row says whether the person
//! follows it, whether it is resolved, and when it last had a reply and was
//! last read: a followed thread with a reply by someone else after the last
//! read is unread. The channel's coordinator touches the row on every line
//! of the thread; a new line reopens a resolved thread.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::chats::now_ms;
use crate::DbError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    pub id: String,
    pub from_chat: String,
    pub from_lines: Vec<String>,
    pub excerpt: String,
    pub to_chat: String,
    pub note: String,
    /// `person` or a mate's id.
    pub by: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewShare {
    pub from_chat: String,
    #[serde(default)]
    pub from_lines: Vec<String>,
    #[serde(default)]
    pub excerpt: String,
    pub to_chat: String,
    #[serde(default)]
    pub note: String,
    pub by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadFollow {
    pub chat_id: String,
    pub root: String,
    pub root_excerpt: String,
    pub following: bool,
    pub resolved: bool,
    pub replies: i64,
    pub last_ms: i64,
    pub last_author: String,
    pub last_read_ms: i64,
    pub unread: bool,
}

fn share_of(row: &Row) -> rusqlite::Result<Share> {
    Ok(Share {
        id: row.get(0)?,
        from_chat: row.get(1)?,
        from_lines: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(),
        excerpt: row.get(3)?,
        to_chat: row.get(4)?,
        note: row.get(5)?,
        by: row.get(6)?,
        at_ms: row.get(7)?,
    })
}

/// Record a share (FR-16, FR-18).
pub fn record_share(conn: &Connection, share: &NewShare) -> Result<Share, DbError> {
    if share.from_chat == share.to_chat {
        return Err(DbError::Invalid(
            "a share goes to another conversation".into(),
        ));
    }
    let id = crate::chats::new_id("share");
    conn.execute(
        "INSERT INTO mate_shares (id, from_chat, from_lines, excerpt, to_chat, note, by, at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            share.from_chat,
            serde_json::to_string(&share.from_lines).unwrap_or_else(|_| "[]".into()),
            share.excerpt,
            share.to_chat,
            share.note,
            share.by,
            now_ms()
        ],
    )?;
    Ok(conn.query_row(
        "SELECT id, from_chat, from_lines, excerpt, to_chat, note, by, at_ms FROM mate_shares WHERE id = ?1",
        [&id],
        share_of,
    )?)
}

/// Shares into or out of `chat`, newest first (FR-19).
pub fn linked(conn: &Connection, chat: &str) -> Result<Vec<Share>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, from_chat, from_lines, excerpt, to_chat, note, by, at_ms FROM mate_shares
         WHERE from_chat = ?1 OR to_chat = ?1 ORDER BY at_ms DESC, id DESC LIMIT 200",
    )?;
    let rows = stmt.query_map([chat], share_of)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn follow_of(row: &Row) -> rusqlite::Result<ThreadFollow> {
    let following = row.get::<_, i64>(3)? == 1;
    let last_ms: i64 = row.get(6)?;
    let last_author: String = row.get(7)?;
    let last_read_ms: i64 = row.get(8)?;
    Ok(ThreadFollow {
        chat_id: row.get(0)?,
        root: row.get(1)?,
        root_excerpt: row.get(2)?,
        following,
        resolved: row.get::<_, i64>(4)? == 1,
        replies: row.get(5)?,
        unread: following && last_author != "person" && last_ms > last_read_ms,
        last_ms,
        last_author,
        last_read_ms,
    })
}

const FOLLOW_COLUMNS: &str =
    "chat_id, root, root_excerpt, following, resolved, replies, last_ms, last_author, last_read_ms";

/// A line landed in the thread under `root` (FR-20, FR-21): one more reply,
/// the thread reopens. `follow_new` makes the person follow a thread seen for
/// the first time (they started it, wrote in it or were mentioned);
/// afterwards only `refollow` (their own line, or `@you`) follows it again,
/// so an unfollow holds (part C review #1). The person's own line counts as
/// read.
#[allow(clippy::too_many_arguments)]
pub fn thread_touched(
    conn: &Connection,
    chat: &str,
    root: &str,
    root_excerpt: &str,
    author: &str,
    at_ms: i64,
    follow_new: bool,
    refollow: bool,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO thread_follows (chat_id, root, root_excerpt, following, resolved, replies, last_ms, last_author, last_read_ms)
         VALUES (?1, ?2, ?3, ?4, 0, 1, ?5, ?6, CASE WHEN ?6 = 'person' THEN ?5 ELSE 0 END)
         ON CONFLICT (chat_id, root) DO UPDATE SET
           root_excerpt = CASE WHEN root_excerpt = '' THEN excluded.root_excerpt ELSE root_excerpt END,
           following = CASE WHEN ?7 THEN 1 ELSE following END,
           resolved = 0,
           replies = replies + 1,
           last_ms = MAX(last_ms, excluded.last_ms),
           last_author = excluded.last_author,
           last_read_ms = CASE WHEN excluded.last_author = 'person' THEN MAX(last_read_ms, excluded.last_ms) ELSE last_read_ms END",
        params![chat, root, root_excerpt, follow_new as i64, at_ms, author, refollow],
    )?;
    Ok(())
}

/// A thread's row from the channel's entries, for threads from before the
/// rows were kept (part C review #9). An existing row is left as it is.
#[allow(clippy::too_many_arguments)]
pub fn thread_backfill(
    conn: &Connection,
    chat: &str,
    root: &str,
    root_excerpt: &str,
    replies: i64,
    last_ms: i64,
    last_author: &str,
    following: bool,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO thread_follows (chat_id, root, root_excerpt, following, replies, last_ms, last_author, last_read_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?6) ON CONFLICT (chat_id, root) DO NOTHING",
        params![chat, root, root_excerpt, following as i64, replies, last_ms, last_author],
    )?;
    Ok(())
}

/// The person read the thread (opened it) up to `at_ms`.
pub fn thread_read(conn: &Connection, chat: &str, root: &str, at_ms: i64) -> Result<(), DbError> {
    conn.execute(
        "UPDATE thread_follows SET last_read_ms = MAX(last_read_ms, ?3) WHERE chat_id = ?1 AND root = ?2",
        params![chat, root, at_ms],
    )?;
    Ok(())
}

/// Follow or unfollow, resolve or reopen a thread (FR-20, FR-21).
pub fn thread_set(
    conn: &Connection,
    chat: &str,
    root: &str,
    following: Option<bool>,
    resolved: Option<bool>,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO thread_follows (chat_id, root, following, resolved) VALUES (?1, ?2, COALESCE(?3, 0), COALESCE(?4, 0))
         ON CONFLICT (chat_id, root) DO UPDATE SET
           following = COALESCE(?3, following),
           resolved = COALESCE(?4, resolved)",
        params![chat, root, following.map(|f| f as i64), resolved.map(|r| r as i64)],
    )?;
    Ok(())
}

/// The threads the person follows: unread first, then the latest (FR-20).
pub fn followed_threads(conn: &Connection) -> Result<Vec<ThreadFollow>, DbError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {FOLLOW_COLUMNS} FROM thread_follows WHERE following = 1 ORDER BY last_ms DESC"
    ))?;
    let mut all: Vec<ThreadFollow> = stmt.query_map([], follow_of)?.collect::<Result<_, _>>()?;
    all.sort_by_key(|t| !t.unread);
    Ok(all)
}

/// Every thread row of a channel's chat (resolved state, follows).
pub fn threads_of(conn: &Connection, chat: &str) -> Result<Vec<ThreadFollow>, DbError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {FOLLOW_COLUMNS} FROM thread_follows WHERE chat_id = ?1"
    ))?;
    let rows = stmt.query_map([chat], follow_of)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn thread_follow(
    conn: &Connection,
    chat: &str,
    root: &str,
) -> Result<Option<ThreadFollow>, DbError> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT {FOLLOW_COLUMNS} FROM thread_follows WHERE chat_id = ?1 AND root = ?2"
            ),
            params![chat, root],
            follow_of,
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chats::{create_chat, NewChat};

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        for id in ["room", "chat-kai", "other"] {
            create_chat(
                &conn,
                &NewChat {
                    id: id.into(),
                    title: id.into(),
                    ..NewChat::default()
                },
            )
            .unwrap();
        }
        (dir, conn)
    }

    #[test]
    fn a_share_links_both_conversations() {
        let (_dir, conn) = db();
        let share = record_share(
            &conn,
            &NewShare {
                from_chat: "room".into(),
                from_lines: vec!["k1".into(), "l1".into()],
                excerpt: "Build is green".into(),
                to_chat: "chat-kai".into(),
                note: "fyi".into(),
                by: "person".into(),
            },
        )
        .unwrap();
        assert_eq!(share.from_lines, ["k1", "l1"]);
        assert_eq!(linked(&conn, "room").unwrap().len(), 1);
        assert_eq!(linked(&conn, "chat-kai").unwrap()[0].id, share.id);
        assert!(linked(&conn, "other").unwrap().is_empty());
        assert!(record_share(
            &conn,
            &NewShare {
                from_chat: "room".into(),
                to_chat: "room".into(),
                by: "person".into(),
                ..NewShare::default()
            }
        )
        .is_err());
    }

    #[test]
    fn a_followed_thread_is_unread_until_read_and_a_reply_reopens_it() {
        let (_dir, conn) = db();
        // The person replies in Kai's thread: followed, read.
        thread_touched(
            &conn,
            "room",
            "k1",
            "Build is green",
            "person",
            10,
            true,
            true,
        )
        .unwrap();
        assert!(!followed_threads(&conn).unwrap()[0].unread);
        // Kai answers: unread.
        thread_touched(&conn, "room", "k1", "", "kai", 20, false, false).unwrap();
        let t = &followed_threads(&conn).unwrap()[0];
        assert_eq!(
            (t.unread, t.replies, t.root_excerpt.as_str()),
            (true, 2, "Build is green")
        );
        thread_read(&conn, "room", "k1", 25).unwrap();
        assert!(!followed_threads(&conn).unwrap()[0].unread);
        // Resolved, then a new line reopens it.
        thread_set(&conn, "room", "k1", None, Some(true)).unwrap();
        assert!(
            thread_follow(&conn, "room", "k1")
                .unwrap()
                .unwrap()
                .resolved
        );
        thread_touched(&conn, "room", "k1", "", "lin", 30, false, false).unwrap();
        let t = thread_follow(&conn, "room", "k1").unwrap().unwrap();
        assert!(!t.resolved && t.unread);
        // A thread nobody asked the person into is not followed, unless they follow it.
        thread_touched(&conn, "room", "m9", "Other", "kai", 40, false, false).unwrap();
        assert_eq!(followed_threads(&conn).unwrap().len(), 1);
        thread_set(&conn, "room", "m9", Some(true), None).unwrap();
        let list = followed_threads(&conn).unwrap();
        assert_eq!(
            list.iter().map(|t| t.root.as_str()).collect::<Vec<_>>(),
            ["m9", "k1"],
            "unread first, then the latest"
        );
        assert_eq!(threads_of(&conn, "room").unwrap().len(), 2);
    }

    // Part C review #1: an unfollow holds through replies; the person's own
    // line or an @you follows again.
    #[test]
    fn an_unfollow_holds_until_the_person_writes_or_is_mentioned() {
        let (_dir, conn) = db();
        thread_touched(&conn, "room", "p1", "My question", "kai", 10, true, false).unwrap();
        assert!(
            thread_follow(&conn, "room", "p1")
                .unwrap()
                .unwrap()
                .following,
            "the person started it"
        );
        thread_set(&conn, "room", "p1", Some(false), None).unwrap();
        thread_touched(&conn, "room", "p1", "", "kai", 20, true, false).unwrap();
        assert!(
            !thread_follow(&conn, "room", "p1")
                .unwrap()
                .unwrap()
                .following,
            "a reply does not re-follow"
        );
        thread_touched(&conn, "room", "p1", "", "kai", 30, true, true).unwrap();
        assert!(
            thread_follow(&conn, "room", "p1")
                .unwrap()
                .unwrap()
                .following,
            "an @you does"
        );
        // Backfill keeps an existing row and fills a missing one.
        thread_backfill(&conn, "room", "p1", "x", 99, 1, "lin", false).unwrap();
        assert_eq!(
            thread_follow(&conn, "room", "p1").unwrap().unwrap().replies,
            3
        );
        thread_backfill(&conn, "room", "old", "Old one", 4, 5, "kai", true).unwrap();
        let old = thread_follow(&conn, "room", "old").unwrap().unwrap();
        assert_eq!((old.replies, old.unread), (4, false));
    }
}
