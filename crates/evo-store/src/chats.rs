//! Workspaces and the chats inside them, as the desktop sidebar shows them.
//!
//! A chat's `id` is its engine session id: its transcript is the file
//! `transcripts/<id>.db` ([`crate::transcripts`]), written by the chat's engine
//! through `--session-db`. Functions that copy or search transcripts take the
//! transcripts folder; deleting a chat's file is the caller's job, after its
//! engine has stopped (Windows refuses to delete a file in use). Times are
//! Unix milliseconds, the unit the UI works in.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::{transcripts, DbError};

/// Unix milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub path: String,
    pub name: String,
    pub collapsed: bool,
    pub position: i64,
    pub created_at: i64,
    pub opened_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Chat {
    pub id: String,
    /// `None`: the chat runs in the home folder ("This computer").
    pub workspace_id: Option<String>,
    pub title: String,
    /// `auto` follows the first prompt; `user` was renamed by hand.
    pub title_source: String,
    pub preview: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub pinned: bool,
    pub unread: bool,
    pub archived_at: Option<i64>,
    pub turn_count: i64,
    pub cost_usd: f64,
    pub tokens: i64,
    pub created_at: i64,
    pub updated_at: i64,
    /// The chat that started this one (nested sessions); `None` at top level.
    pub parent_chat_id: Option<String>,
    /// `user`, or `agent` when another chat's agent started it.
    pub origin: String,
    /// A managed worktree the chat works in instead of its workspace folder.
    pub worktree_path: Option<String>,
    /// The chat uses no memory, saves none and cannot search past chats
    /// (docs/features/memory-topics-and-chat-recall/spec.md, FR-6).
    pub memory_off: bool,
    /// As `memory_off`, and the chat leaves no history: it has a fixed title,
    /// no preview and no search text, and is removed when it ends (FR-19 – FR-22).
    pub incognito: bool,
    /// The Scheduler started it: indexing writes a summary but no topic (FR-30).
    pub scheduled: bool,
}

/// A chat to create.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewChat {
    pub id: String,
    pub workspace_id: Option<String>,
    pub title: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub parent_chat_id: Option<String>,
    /// `user` (default) or `agent`.
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub worktree_path: Option<String>,
    #[serde(default)]
    pub memory_off: bool,
    #[serde(default)]
    pub incognito: bool,
    #[serde(default)]
    pub scheduled: bool,
}

/// Fields to change on a chat; `None` leaves a field as it is.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPatch {
    /// A title the person chose (marks the title as `user`).
    pub title: Option<String>,
    /// A title derived from the conversation; ignored once the person renamed it.
    pub auto_title: Option<String>,
    pub pinned: Option<bool>,
    pub unread: Option<bool>,
    pub archived: Option<bool>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

/// What one finished turn adds to a chat.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnRecord {
    /// The start of the agent's reply, for the sidebar.
    pub preview: Option<String>,
    pub cost_usd: f64,
    pub tokens: i64,
    pub provider: Option<String>,
    pub model: Option<String>,
    /// Mark the chat unread (it finished while the person was elsewhere).
    pub unread: bool,
}

const WORKSPACE_COLUMNS: &str = "id, path, name, collapsed, position, created_at, opened_at";
const CHAT_COLUMNS: &str = "id, workspace_id, title, title_source, preview, provider, model, pinned, unread, \
     archived_at, turn_count, cost_usd, tokens, created_at, updated_at, parent_chat_id, origin, worktree_path, \
     memory_off, incognito, scheduled";

/// How many columns [`CHAT_COLUMNS`] names; a query that selects more reads
/// its own from this index on.
const CHAT_COLUMN_COUNT: usize = 21;

/// [`CHAT_COLUMNS`] for a query where `chats` is aliased `c`.
fn chat_columns_of_c() -> String {
    CHAT_COLUMNS
        .split(", ")
        .map(|column| format!("c.{}", column.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn workspace_row(row: &Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        path: row.get(1)?,
        name: row.get(2)?,
        collapsed: row.get(3)?,
        position: row.get(4)?,
        created_at: row.get(5)?,
        opened_at: row.get(6)?,
    })
}

fn chat_row(row: &Row<'_>) -> rusqlite::Result<Chat> {
    Ok(Chat {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        title: row.get(2)?,
        title_source: row.get(3)?,
        preview: row.get(4)?,
        provider: row.get(5)?,
        model: row.get(6)?,
        pinned: row.get(7)?,
        unread: row.get(8)?,
        archived_at: row.get(9)?,
        turn_count: row.get(10)?,
        cost_usd: row.get(11)?,
        tokens: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
        parent_chat_id: row.get(15)?,
        origin: row.get(16)?,
        worktree_path: row.get(17)?,
        memory_off: row.get(18)?,
        incognito: row.get(19)?,
        scheduled: row.get(20)?,
    })
}

fn trim_path(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() || trimmed.ends_with(':') {
        path
    } else {
        trimmed
    }
}

fn folder_name(path: &str) -> String {
    trim_path(path)
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn new_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{prefix}-{:x}{:04x}{:x}",
        now_ms(),
        std::process::id() & 0xffff,
        n
    )
}

pub fn list_workspaces(conn: &Connection) -> Result<Vec<Workspace>, DbError> {
    let mut statement = conn.prepare(&format!(
        "SELECT {WORKSPACE_COLUMNS} FROM workspaces ORDER BY position, name COLLATE NOCASE"
    ))?;
    let rows = statement.query_map([], workspace_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get_workspace(conn: &Connection, id: &str) -> Result<Option<Workspace>, DbError> {
    Ok(conn
        .query_row(
            &format!("SELECT {WORKSPACE_COLUMNS} FROM workspaces WHERE id = ?1"),
            [id],
            workspace_row,
        )
        .optional()?)
}

/// The workspace for `path`, created on first use; opening it bumps `opened_at`.
pub fn ensure_workspace(conn: &Connection, path: &str) -> Result<Workspace, DbError> {
    let path = trim_path(path);
    let now = now_ms();
    conn.execute(
        "INSERT INTO workspaces (id, path, name, collapsed, position, created_at, opened_at)
         VALUES (?1, ?2, ?3, 0, 0, ?4, ?4)
         ON CONFLICT (path) DO UPDATE SET opened_at = excluded.opened_at",
        params![new_id("ws"), path, folder_name(path), now],
    )?;
    Ok(conn.query_row(
        &format!("SELECT {WORKSPACE_COLUMNS} FROM workspaces WHERE path = ?1"),
        [path],
        workspace_row,
    )?)
}

pub fn set_workspace_collapsed(
    conn: &Connection,
    id: &str,
    collapsed: bool,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE workspaces SET collapsed = ?2 WHERE id = ?1",
        params![id, collapsed],
    )?;
    Ok(())
}

/// Remove a workspace from the sidebar together with its chats. Returns the
/// removed chat ids: stop their engines, then remove their transcript files.
pub fn remove_workspace(conn: &mut Connection, id: &str) -> Result<Vec<String>, DbError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let ids: Vec<String> = {
        let mut statement = tx.prepare("SELECT id FROM chats WHERE workspace_id = ?1")?;
        let rows = statement.query_map([id], |row| row.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    for chat in &ids {
        // A transcript an older build left in this file, if it was not moved.
        tx.execute("DELETE FROM sessions WHERE id = ?1", [chat])?;
    }
    tx.execute("DELETE FROM chats WHERE workspace_id = ?1", [id])?;
    tx.execute("DELETE FROM workspaces WHERE id = ?1", [id])?;
    tx.commit()?;
    Ok(ids)
}

/// Every chat, pinned first then most recently active; `archived` selects
/// the archive instead of the live list.
pub fn list_chats(conn: &Connection, archived: bool) -> Result<Vec<Chat>, DbError> {
    let filter = if archived {
        "archived_at IS NOT NULL ORDER BY archived_at DESC"
    } else {
        "archived_at IS NULL ORDER BY pinned DESC, updated_at DESC"
    };
    let mut statement =
        conn.prepare(&format!("SELECT {CHAT_COLUMNS} FROM chats WHERE {filter}"))?;
    let rows = statement.query_map([], chat_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get_chat(conn: &Connection, id: &str) -> Result<Option<Chat>, DbError> {
    Ok(conn
        .query_row(
            &format!("SELECT {CHAT_COLUMNS} FROM chats WHERE id = ?1"),
            [id],
            chat_row,
        )
        .optional()?)
}

/// The title of every incognito chat: none of its own text is kept here.
pub const INCOGNITO_TITLE: &str = "Incognito chat";

/// Create a chat (or return the existing one with that id).
pub fn create_chat(conn: &Connection, chat: &NewChat) -> Result<Chat, DbError> {
    let now = now_ms();
    let title = if chat.incognito {
        INCOGNITO_TITLE
    } else {
        chat.title.as_str()
    };
    conn.execute(
        "INSERT INTO chats (id, workspace_id, title, title_source, provider, model, created_at, updated_at, parent_chat_id, origin, worktree_path,
                            memory_off, incognito, scheduled)
         VALUES (?1, ?2, ?3, 'auto', ?4, ?5, ?6, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT (id) DO NOTHING",
        params![
            chat.id,
            chat.workspace_id,
            title,
            chat.provider,
            chat.model,
            now,
            chat.parent_chat_id,
            chat.origin.as_deref().unwrap_or("user"),
            chat.worktree_path,
            // An incognito chat never uses memory either.
            chat.memory_off || chat.incognito,
            chat.incognito,
            chat.scheduled
        ],
    )?;
    get_chat(conn, &chat.id)?.ok_or_else(|| DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows))
}

pub fn update_chat(
    conn: &Connection,
    id: &str,
    patch: &ChatPatch,
) -> Result<Option<Chat>, DbError> {
    // An incognito chat keeps its fixed title and is never pinned or archived
    // (docs/features/memory-topics-and-chat-recall/spec.md, FR-19).
    if let Some(title) = &patch.title {
        conn.execute(
            "UPDATE chats SET title = ?2, title_source = 'user' WHERE id = ?1 AND incognito = 0",
            params![id, title],
        )?;
    }
    if let Some(title) = &patch.auto_title {
        conn.execute(
            "UPDATE chats SET title = ?2 WHERE id = ?1 AND title_source = 'auto' AND incognito = 0",
            params![id, title],
        )?;
    }
    if let Some(pinned) = patch.pinned {
        conn.execute(
            "UPDATE chats SET pinned = ?2 WHERE id = ?1 AND incognito = 0",
            params![id, pinned],
        )?;
    }
    if let Some(unread) = patch.unread {
        conn.execute(
            "UPDATE chats SET unread = ?2 WHERE id = ?1",
            params![id, unread],
        )?;
    }
    if let Some(archived) = patch.archived {
        let at = archived.then(now_ms);
        conn.execute(
            "UPDATE chats SET archived_at = ?2, pinned = CASE WHEN ?2 IS NULL THEN pinned ELSE 0 END
             WHERE id = ?1 AND incognito = 0",
            params![id, at],
        )?;
    }
    if patch.provider.is_some() || patch.model.is_some() {
        conn.execute(
            "UPDATE chats SET provider = COALESCE(?2, provider), model = COALESCE(?3, model) WHERE id = ?1",
            params![id, patch.provider, patch.model],
        )?;
    }
    get_chat(conn, id)
}

/// Add one finished turn to a chat's totals and bump its activity time.
pub fn record_turn(
    conn: &Connection,
    id: &str,
    turn: &TurnRecord,
) -> Result<Option<Chat>, DbError> {
    let preview = turn.preview.as_deref().map(|text| {
        let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
        line.chars().take(200).collect::<String>()
    });
    conn.execute(
        "UPDATE chats SET
             turn_count = turn_count + 1,
             cost_usd = cost_usd + ?2,
             tokens = tokens + ?3,
             preview = CASE WHEN incognito THEN NULL ELSE COALESCE(?4, preview) END,
             provider = COALESCE(?5, provider),
             model = COALESCE(?6, model),
             unread = CASE WHEN ?7 THEN 1 ELSE unread END,
             updated_at = ?8
         WHERE id = ?1",
        params![
            id,
            turn.cost_usd,
            turn.tokens,
            preview.filter(|text| !text.is_empty()),
            turn.provider,
            turn.model,
            turn.unread,
            now_ms()
        ],
    )?;
    get_chat(conn, id)
}

/// A copy of chat `source` under `id`: the same folder, model and whole
/// transcript (a new file in `transcripts_dir` whose session is a child of the
/// source's), titled `title`. The copy starts with no spend of its own. The
/// source may be running: its transcript is copied as far as it was written.
/// `None` when `source` does not exist.
pub fn fork_chat(
    conn: &mut Connection,
    transcripts_dir: &Path,
    source: &str,
    id: &str,
    title: &str,
) -> Result<Option<Chat>, DbError> {
    // An incognito chat leaves no copy of itself.
    if get_chat(conn, source)?.is_none_or(|chat| chat.incognito) {
        return Ok(None);
    }
    let now = now_ms();
    // The file first: a chat row never points at a half-made copy.
    let copied = transcripts::fork(transcripts_dir, source, id, now)?;
    let inserted = (|| -> Result<usize, DbError> {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let inserted = tx.execute(
            "INSERT INTO chats (id, workspace_id, title, title_source, preview, provider, model, turn_count, created_at, updated_at, memory_off)
             SELECT ?2, workspace_id, ?3, 'user', preview, provider, model, turn_count, ?4, ?4, memory_off
             FROM chats WHERE id = ?1 AND incognito = 0",
            params![source, id, title, now],
        )?;
        tx.commit()?;
        Ok(inserted)
    })();
    match inserted {
        Ok(1) => get_chat(conn, id),
        other => {
            if copied {
                let _ = transcripts::remove(&transcripts::path_in(transcripts_dir, id)?);
            }
            other.map(|_| None)
        }
    }
}

/// The worktree a chat works in, or (`None`) its workspace folder again.
pub fn set_worktree(
    conn: &Connection,
    id: &str,
    worktree_path: Option<&str>,
) -> Result<Option<Chat>, DbError> {
    conn.execute(
        "UPDATE chats SET worktree_path = ?2 WHERE id = ?1",
        params![id, worktree_path],
    )?;
    get_chat(conn, id)
}

/// Link a chat to the chat that started it, or (`None`) detach it.
pub fn set_parent(
    conn: &Connection,
    id: &str,
    parent_chat_id: Option<&str>,
) -> Result<Option<Chat>, DbError> {
    if parent_chat_id == Some(id) {
        return get_chat(conn, id);
    }
    conn.execute(
        "UPDATE chats SET parent_chat_id = ?2 WHERE id = ?1",
        params![id, parent_chat_id],
    )?;
    get_chat(conn, id)
}

/// Who started a chat: `user`, or `agent` (a copy made for a nested session).
pub fn set_origin(conn: &Connection, id: &str, origin: &str) -> Result<Option<Chat>, DbError> {
    conn.execute(
        "UPDATE chats SET origin = ?2 WHERE id = ?1",
        params![id, origin],
    )?;
    get_chat(conn, id)
}

/// Move a chat to another workspace (`None`: "This computer").
pub fn move_chat(
    conn: &Connection,
    id: &str,
    workspace_id: Option<&str>,
) -> Result<Option<Chat>, DbError> {
    conn.execute(
        "UPDATE chats SET workspace_id = ?2 WHERE id = ?1",
        params![id, workspace_id],
    )?;
    get_chat(conn, id)
}

/// Delete a chat (its search text goes with it). Its transcript file is the
/// caller's to remove once the chat's engine has stopped.
pub fn delete_chat(conn: &mut Connection, id: &str) -> Result<(), DbError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // A transcript an older build left in this file, if it was not moved.
    tx.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
    tx.execute("DELETE FROM chats WHERE id = ?1", [id])?;
    tx.commit()?;
    Ok(())
}

/// Bring the search text of every chat up to date with its transcript file:
/// re-read the transcripts whose file changed since they were last read
/// (their [`transcripts::stamp`]), drop the text of chats whose file is gone.
/// Returns how many chats changed. Only changed files are read, so a search
/// after a few turns reads a few files.
pub fn refresh_search(conn: &mut Connection, transcripts_dir: &Path) -> Result<usize, DbError> {
    let known: Vec<(String, Option<String>)> = {
        // Never an incognito chat: its words stay out of this file (NFR-4).
        let mut statement = conn.prepare(
            "SELECT c.id, s.stamp FROM chats c LEFT JOIN chat_search s ON s.chat_id = c.id
             WHERE c.incognito = 0",
        )?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let mut changed: Vec<(String, Option<(String, String)>)> = Vec::new();
    for (id, recorded) in known {
        let Ok(path) = transcripts::path_in(transcripts_dir, &id) else {
            continue;
        };
        match transcripts::stamp(&path) {
            None if recorded.is_some() => changed.push((id, None)),
            None => {}
            Some(stamp) if recorded.as_deref() != Some(stamp.as_str()) => {
                // Stamp first, then read: a write in between leaves an older
                // stamp, so the next refresh reads the file again.
                let body = transcripts::read_state(&path, &id)
                    .ok()
                    .flatten()
                    .map(|state| transcripts::searchable_text(&state))
                    .unwrap_or_default();
                changed.push((id, Some((stamp, body))));
            }
            Some(_) => {}
        }
    }
    if changed.is_empty() {
        return Ok(0);
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for (id, text) in &changed {
        match text {
            Some((stamp, body)) => tx.execute(
                "INSERT INTO chat_search (chat_id, stamp, body) VALUES (?1, ?2, ?3)
                 ON CONFLICT (chat_id) DO UPDATE SET stamp = excluded.stamp, body = excluded.body",
                params![id, stamp, body],
            )?,
            None => tx.execute("DELETE FROM chat_search WHERE chat_id = ?1", [id])?,
        };
    }
    tx.commit()?;
    Ok(changed.len())
}

/// Chats whose title, preview or transcript mentions `query` (case-insensitive),
/// live and archived, most recent first. Refreshes the search text of changed
/// transcripts first ([`refresh_search`]).
pub fn search_chats(
    conn: &mut Connection,
    transcripts_dir: &Path,
    query: &str,
    limit: u32,
) -> Result<Vec<Chat>, DbError> {
    refresh_search(conn, transcripts_dir)?;
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    let columns = chat_columns_of_c();
    let mut statement = conn.prepare(&format!(
        "SELECT {columns} FROM chats c LEFT JOIN chat_search s ON s.chat_id = c.id
         WHERE c.incognito = 0
           AND (c.title LIKE ?1 ESCAPE '\\' OR c.preview LIKE ?1 ESCAPE '\\' OR s.body LIKE ?1 ESCAPE '\\')
         ORDER BY c.updated_at DESC LIMIT ?2"
    ))?;
    let rows = statement.query_map(params![pattern, limit], chat_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// ── Past chats for an agent ────────────────────────────────────────────────
//
// docs/features/memory-topics-and-chat-recall/spec.md (FR-11 – FR-13, FR-32),
// plan 3.5 "Past-chat search".

/// At most this many words of a query are looked for.
pub const MAX_SEARCH_WORDS: usize = 8;

/// Which past chats an agent asks for.
#[derive(Debug, Clone, Default)]
pub struct ChatQuery<'a> {
    /// Words to look for. Empty: list chats by time instead.
    pub query: &'a str,
    /// The asking chat's project: a workspace id, or `None` for the home
    /// folder. Chats of any other project are never returned.
    pub workspace: Option<&'a str>,
    /// The asking chat; never returned.
    pub exclude: Option<&'a str>,
    /// Only chats active at or after this time (ms).
    pub after_ms: Option<i64>,
    /// Only chats started before this time (ms).
    pub before_ms: Option<i64>,
    /// Without words: oldest first instead of newest first.
    pub oldest_first: bool,
    pub limit: u32,
}

/// A past chat an agent asked for.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundChat {
    pub chat: Chat,
    /// The summary indexing wrote, when there is one.
    pub summary: Option<String>,
    /// How well it matches (0 for a list by time).
    pub score: f64,
}

/// The words of a query: split at whitespace, punctuation around a word
/// dropped, repeats (ignoring case) dropped, at most [`MAX_SEARCH_WORDS`].
/// Their case is kept: `LIKE` ignores it for ASCII letters only.
pub fn search_words(query: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for raw in query.split_whitespace() {
        let word = raw.trim_matches(|c: char| !c.is_alphanumeric());
        if word.is_empty() {
            continue;
        }
        let folded = word.to_lowercase();
        if words.iter().any(|known| known.to_lowercase() == folded) {
            continue;
        }
        words.push(word.to_string());
        if words.len() == MAX_SEARCH_WORDS {
            break;
        }
    }
    words
}

fn like_pattern(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// The chats of one project a query may return: never an incognito chat or
/// the asking chat, inside the time window. `?1` workspace, `?2` asking chat,
/// `?3` after, `?4` before.
const FIND_SCOPE: &str = "c.incognito = 0 AND c.workspace_id IS ?1 AND c.id IS NOT ?2
     AND (?3 IS NULL OR c.updated_at >= ?3) AND (?4 IS NULL OR c.created_at < ?4)";

/// Past chats of one project for an agent: with words, the best matches first;
/// without, the chats by time.
///
/// A chat matches when any word is in its title, summary, preview or search
/// text. Score: 3 for a word in the title, 3 in the summary, 1 in the text;
/// 2 more when every word is somewhere; 3 more when the whole query appears
/// as one phrase; plus up to 1 for recency (`1 / (1 + age in days / 30)`).
/// Best first, then newest. Refreshes the search text of changed transcripts
/// first when there are words.
pub fn find_chats(
    conn: &mut Connection,
    transcripts_dir: &Path,
    query: &ChatQuery<'_>,
) -> Result<Vec<FoundChat>, DbError> {
    use rusqlite::types::Value as SqlValue;

    let columns = chat_columns_of_c();
    let mut values: Vec<SqlValue> = vec![
        query
            .workspace
            .map_or(SqlValue::Null, |id| SqlValue::Text(id.to_string())),
        query
            .exclude
            .map_or(SqlValue::Null, |id| SqlValue::Text(id.to_string())),
        query.after_ms.map_or(SqlValue::Null, SqlValue::Integer),
        query.before_ms.map_or(SqlValue::Null, SqlValue::Integer),
    ];
    let words = search_words(query.query);

    if words.is_empty() {
        let order = if query.oldest_first { "ASC" } else { "DESC" };
        values.push(SqlValue::Integer(i64::from(query.limit.max(1))));
        let mut statement = conn.prepare(&format!(
            "SELECT {columns}, i.summary FROM chats c LEFT JOIN chat_index i ON i.chat_id = c.id
             WHERE {FIND_SCOPE} ORDER BY c.updated_at {order} LIMIT ?5"
        ))?;
        let rows = statement.query_map(rusqlite::params_from_iter(values.iter()), |row| {
            Ok(FoundChat {
                chat: chat_row(row)?,
                summary: row.get(CHAT_COLUMN_COUNT)?,
                score: 0.0,
            })
        })?;
        return Ok(rows.collect::<Result<_, _>>()?);
    }

    refresh_search(conn, transcripts_dir)?;
    // ?5 is the whole query as one phrase; each word follows from ?6.
    let phrase = query.query.split_whitespace().collect::<Vec<_>>().join(" ");
    values.push(SqlValue::Text(like_pattern(&phrase)));
    let like = |column: &str, at: usize| format!("{column} LIKE ?{at} ESCAPE '\\'");
    let anywhere = |at: usize| {
        format!(
            "{} OR {} OR {} OR {}",
            like("c.title", at),
            like("i.summary", at),
            like("c.preview", at),
            like("s.body", at)
        )
    };
    let mut flags = vec![format!("COALESCE({}, 0)", anywhere(5))];
    let mut matches = Vec::new();
    for (index, word) in words.iter().enumerate() {
        let at = 6 + index;
        values.push(SqlValue::Text(like_pattern(word)));
        flags.push(format!("COALESCE({}, 0)", like("c.title", at)));
        flags.push(format!("COALESCE({}, 0)", like("i.summary", at)));
        flags.push(format!(
            "COALESCE({} OR {}, 0)",
            like("c.preview", at),
            like("s.body", at)
        ));
        matches.push(format!("({})", anywhere(at)));
    }
    let mut statement = conn.prepare(&format!(
        "SELECT {columns}, i.summary, {} FROM chats c
         LEFT JOIN chat_search s ON s.chat_id = c.id
         LEFT JOIN chat_index i ON i.chat_id = c.id
         WHERE {FIND_SCOPE} AND ({})",
        flags.join(", "),
        matches.join(" OR ")
    ))?;
    let now = now_ms();
    let several = words.len() > 1;
    let rows = statement.query_map(rusqlite::params_from_iter(values.iter()), |row| {
        let chat = chat_row(row)?;
        let phrase_found: bool = row.get(CHAT_COLUMN_COUNT + 1)?;
        let mut score = 0.0;
        let mut every_word = true;
        for index in 0..words.len() {
            let at = CHAT_COLUMN_COUNT + 2 + index * 3;
            let (title, summary, text): (bool, bool, bool) =
                (row.get(at)?, row.get(at + 1)?, row.get(at + 2)?);
            score += f64::from(u8::from(title)) * 3.0
                + f64::from(u8::from(summary)) * 3.0
                + f64::from(u8::from(text));
            every_word &= title || summary || text;
        }
        if several && every_word {
            score += 2.0;
        }
        if several && phrase_found {
            score += 3.0;
        }
        let age_days = (now - chat.updated_at).max(0) as f64 / 86_400_000.0;
        score += 1.0 / (1.0 + age_days / 30.0);
        Ok(FoundChat {
            summary: row.get(CHAT_COLUMN_COUNT)?,
            chat,
            score,
        })
    })?;
    let mut found: Vec<FoundChat> = rows.collect::<Result<_, _>>()?;
    found.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.chat.updated_at.cmp(&a.chat.updated_at))
    });
    found.truncate(query.limit.max(1) as usize);
    Ok(found)
}

// ── Incognito chats and indexing ───────────────────────────────────────────

/// Chat `id` and every chat nested under it, at any depth (the chat first).
pub fn with_nested(conn: &Connection, id: &str) -> Result<Vec<String>, DbError> {
    let mut statement = conn.prepare(
        "WITH RECURSIVE tree (id, depth) AS (
             SELECT id, 0 FROM chats WHERE id = ?1
             UNION
             SELECT c.id, t.depth + 1 FROM chats c JOIN tree t ON c.parent_chat_id = t.id
         )
         SELECT id FROM tree ORDER BY depth, id",
    )?;
    let rows = statement.query_map([id], |row| row.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Every incognito chat and the chats nested under one: the chats to remove
/// when evo quits or starts again (spec FR-22).
pub fn incognito_ids(conn: &Connection) -> Result<Vec<String>, DbError> {
    let mut statement = conn.prepare(
        "WITH RECURSIVE tree (id) AS (
             SELECT id FROM chats WHERE incognito = 1
             UNION
             SELECT c.id FROM chats c JOIN tree t ON c.parent_chat_id = t.id
         )
         SELECT id FROM tree ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| row.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Which chats automatic indexing may read now.
#[derive(Debug, Clone, Default)]
pub struct IndexFilter {
    /// Only chats active at or after this time: when indexing was switched on.
    pub since_ms: Option<i64>,
    /// Only chats last active before this time: the "earlier chats".
    pub before_ms: Option<i64>,
    /// Only chats idle since this time or longer.
    pub idle_since_ms: Option<i64>,
    /// A chat whose last run failed waits until its failure is this old.
    pub failed_before_ms: i64,
    pub limit: u32,
}

/// A chat with messages no index run has read yet.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexCandidate {
    pub chat: Chat,
    /// The last transcript entry an earlier run read.
    pub indexed_through: Option<String>,
}

/// The chats with new messages that indexing may read, longest waiting
/// first: top-level chats with a turn that are not incognito or memory-off,
/// updated since they were last indexed, inside `filter`'s times
/// (spec FR-26, FR-30, FR-33). `scheduled` is on each chat for the caller.
pub fn index_candidates(
    conn: &Connection,
    filter: &IndexFilter,
) -> Result<Vec<IndexCandidate>, DbError> {
    let columns = chat_columns_of_c();
    let mut statement = conn.prepare(&format!(
        "SELECT {columns}, i.indexed_through FROM chats c
         LEFT JOIN chat_index i ON i.chat_id = c.id
         WHERE c.incognito = 0 AND c.memory_off = 0 AND c.parent_chat_id IS NULL AND c.turn_count > 0
           AND (?1 IS NULL OR c.updated_at >= ?1)
           AND (?2 IS NULL OR c.updated_at < ?2)
           AND (?3 IS NULL OR c.updated_at <= ?3)
           AND (i.indexed_at IS NULL OR i.indexed_at < c.updated_at)
           AND (i.failed_at IS NULL OR i.failed_at <= ?4)
         ORDER BY c.updated_at ASC LIMIT ?5"
    ))?;
    let rows = statement.query_map(
        params![
            filter.since_ms,
            filter.before_ms,
            filter.idle_since_ms,
            filter.failed_before_ms,
            filter.limit.max(1)
        ],
        |row| {
            Ok(IndexCandidate {
                chat: chat_row(row)?,
                indexed_through: row.get(CHAT_COLUMN_COUNT)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        (dir, conn)
    }

    #[test]
    fn a_chats_origin_can_be_set() {
        let (_dir, conn) = db();
        let new = NewChat {
            id: "c".into(),
            title: "Copy".into(),
            ..Default::default()
        };
        assert_eq!(create_chat(&conn, &new).unwrap().origin, "user");
        let chat = set_origin(&conn, "c", "agent").unwrap().unwrap();
        assert_eq!(chat.origin, "agent");
        // Only the two origins the schema knows; a missing chat is `None`.
        assert!(set_origin(&conn, "c", "robot").is_err());
        assert_eq!(get_chat(&conn, "c").unwrap().unwrap().origin, "agent");
        assert_eq!(set_origin(&conn, "nobody", "agent").unwrap(), None);
    }

    #[test]
    fn nested_sessions_link_to_their_parent() {
        let (_dir, mut conn) = db();
        let parent = create_chat(
            &conn,
            &NewChat {
                id: "p".into(),
                title: "Parent".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            (parent.parent_chat_id.as_deref(), parent.origin.as_str()),
            (None, "user")
        );
        let child = create_chat(
            &conn,
            &NewChat {
                id: "c".into(),
                title: "Child".into(),
                parent_chat_id: Some("p".into()),
                origin: Some("agent".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            (child.parent_chat_id.as_deref(), child.origin.as_str()),
            (Some("p"), "agent")
        );
        assert_eq!(list_chats(&conn, false).unwrap().len(), 2);
        // Deleting the parent keeps the child, at the top level.
        delete_chat(&mut conn, "p").unwrap();
        let orphan = get_chat(&conn, "c").unwrap().unwrap();
        assert_eq!(orphan.parent_chat_id, None);
        // Linking and detaching; a chat is never its own parent.
        create_chat(
            &conn,
            &NewChat {
                id: "q".into(),
                title: "Q".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            set_parent(&conn, "c", Some("q"))
                .unwrap()
                .unwrap()
                .parent_chat_id
                .as_deref(),
            Some("q")
        );
        assert_eq!(
            set_parent(&conn, "c", Some("c"))
                .unwrap()
                .unwrap()
                .parent_chat_id
                .as_deref(),
            Some("q")
        );
        assert_eq!(
            set_parent(&conn, "c", None)
                .unwrap()
                .unwrap()
                .parent_chat_id,
            None
        );
        // A chat's worktree folder.
        assert_eq!(
            set_worktree(&conn, "c", Some("/w/1"))
                .unwrap()
                .unwrap()
                .worktree_path
                .as_deref(),
            Some("/w/1")
        );
        assert_eq!(
            set_worktree(&conn, "c", None)
                .unwrap()
                .unwrap()
                .worktree_path,
            None
        );
    }

    #[test]
    fn workspaces_are_unique_by_path() {
        let (_dir, conn) = db();
        let a = ensure_workspace(&conn, "C:\\code\\app\\").unwrap();
        let b = ensure_workspace(&conn, "c:\\code\\APP").unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.name, "app");
        assert_eq!(list_workspaces(&conn).unwrap().len(), 1);
    }

    #[test]
    fn chat_lifecycle() {
        let (_dir, mut conn) = db();
        let ws = ensure_workspace(&conn, "/tmp/proj").unwrap();
        let chat = create_chat(
            &conn,
            &NewChat {
                id: "c1".into(),
                workspace_id: Some(ws.id.clone()),
                title: "Fix the build".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(chat.title_source, "auto");

        let renamed = update_chat(
            &conn,
            "c1",
            &ChatPatch {
                title: Some("Build".into()),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(renamed.title_source, "user");
        let kept = update_chat(
            &conn,
            "c1",
            &ChatPatch {
                auto_title: Some("x".into()),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(kept.title, "Build");

        let after = record_turn(
            &conn,
            "c1",
            &TurnRecord {
                preview: Some("Done.\n\nAll  green".into()),
                cost_usd: 0.5,
                tokens: 10,
                unread: true,
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            (after.turn_count, after.tokens, after.unread),
            (1, 10, true)
        );
        assert_eq!(after.preview.as_deref(), Some("Done. All green"));

        update_chat(
            &conn,
            "c1",
            &ChatPatch {
                pinned: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        update_chat(
            &conn,
            "c1",
            &ChatPatch {
                archived: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(list_chats(&conn, false).unwrap().is_empty());
        let archived = list_chats(&conn, true).unwrap();
        assert!(!archived[0].pinned, "archiving unpins");

        let files = _dir.path().join(transcripts::DIR_NAME);
        let c1 = write_transcript(&files, "c1", r#"{"records":[{"text":"needle"}]}"#);
        assert_eq!(
            search_chats(&mut conn, &files, "NEEDLE", 10).unwrap().len(),
            1
        );
        assert_eq!(
            search_chats(&mut conn, &files, "100%", 10).unwrap().len(),
            0
        );

        let fork = fork_chat(&mut conn, &files, "c1", "c2", "Build (fork)")
            .unwrap()
            .unwrap();
        assert_eq!(
            (fork.title.as_str(), fork.turn_count, fork.tokens),
            ("Build (fork)", 1, 0)
        );
        assert_eq!(fork.workspace_id.as_deref(), Some(ws.id.as_str()));
        let copy = transcripts::path_in(&files, "c2").unwrap();
        let (parent, state): (Option<String>, String) = Connection::open(&copy)
            .unwrap()
            .query_row(
                "SELECT parent_session_id, state_json FROM sessions WHERE id = 'c2'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (parent.as_deref(), state.contains("needle")),
            (Some("c1"), true)
        );
        assert!(fork_chat(&mut conn, &files, "missing", "c3", "x")
            .unwrap()
            .is_none());
        assert!(!transcripts::path_in(&files, "c3").unwrap().exists());
        // A fork onto an existing chat id fails and leaves no stray file.
        create_chat(
            &conn,
            &NewChat {
                id: "taken".into(),
                title: "T".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(fork_chat(&mut conn, &files, "c1", "taken", "x").is_err());
        assert!(!transcripts::path_in(&files, "taken").unwrap().exists());
        delete_chat(&mut conn, "taken").unwrap();
        // A chat with no transcript yet forks into a chat with none.
        create_chat(
            &conn,
            &NewChat {
                id: "empty".into(),
                title: "E".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(fork_chat(&mut conn, &files, "empty", "empty2", "x")
            .unwrap()
            .is_some());
        assert!(!transcripts::path_in(&files, "empty2").unwrap().exists());
        delete_chat(&mut conn, "empty").unwrap();
        delete_chat(&mut conn, "empty2").unwrap();

        let moved = move_chat(&conn, "c2", None).unwrap().unwrap();
        assert_eq!(moved.workspace_id, None);
        delete_chat(&mut conn, "c2").unwrap();
        let indexed: i64 = conn
            .query_row("SELECT COUNT(*) FROM chat_search", [], |r| r.get(0))
            .unwrap();
        assert_eq!(indexed, 1, "a deleted chat's search text goes with it");

        let removed = remove_workspace(&mut conn, &ws.id).unwrap();
        assert_eq!(removed, vec!["c1".to_string()]);
        assert!(
            c1.exists(),
            "files are removed by the caller, after the engine stops"
        );
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM chat_search", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }

    fn write_transcript(dir: &Path, id: &str, state: &str) -> std::path::PathBuf {
        let path = transcripts::path_in(dir, id).unwrap();
        transcripts::prepare(&path).unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(transcripts::SESSION_SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, created_at, parent_session_id, state_json) VALUES (?1, 0, NULL, ?2)
             ON CONFLICT (id) DO UPDATE SET state_json = excluded.state_json",
            params![id, state],
        )
        .unwrap();
        path
    }

    fn chat(conn: &Connection, id: &str, title: &str, workspace: Option<&str>) -> Chat {
        create_chat(
            conn,
            &NewChat {
                id: id.into(),
                title: title.into(),
                workspace_id: workspace.map(str::to_string),
                ..Default::default()
            },
        )
        .unwrap()
    }

    // docs/features/memory-topics-and-chat-recall/spec.md (FR-19, FR-21, NFR-4).
    #[test]
    fn incognito_chats_are_never_searched() {
        let (dir, mut conn) = db();
        let files = dir.path().join(transcripts::DIR_NAME);
        let ghost = create_chat(
            &conn,
            &NewChat {
                id: "g".into(),
                title: "the secret plan".into(),
                incognito: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(ghost.title, INCOGNITO_TITLE, "its own words are not kept");
        assert!(ghost.incognito && ghost.memory_off);
        chat(&conn, "n", "Ordinary", None);
        write_transcript(&files, "g", r#"{"records":[{"text":"needle"}]}"#);
        write_transcript(&files, "n", r#"{"records":[{"text":"needle"}]}"#);

        assert_eq!(refresh_search(&mut conn, &files).unwrap(), 1);
        let found = search_chats(&mut conn, &files, "needle", 10).unwrap();
        assert_eq!(
            found.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["n"]
        );
        assert!(search_chats(&mut conn, &files, "incognito", 10)
            .unwrap()
            .is_empty());
        let texts: i64 = conn
            .query_row("SELECT COUNT(*) FROM chat_search", [], |r| r.get(0))
            .unwrap();
        assert_eq!(texts, 1, "no search text of the incognito chat");

        // No preview, no rename, pin or archive, no fork.
        let turn = record_turn(
            &conn,
            "g",
            &TurnRecord {
                preview: Some("about the needle".into()),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!((turn.preview, turn.turn_count), (None, 1));
        let same = update_chat(
            &conn,
            "g",
            &ChatPatch {
                title: Some("Needle".into()),
                auto_title: Some("Needle".into()),
                pinned: Some(true),
                archived: Some(true),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            (same.title.as_str(), same.pinned, same.archived_at),
            (INCOGNITO_TITLE, false, None)
        );
        assert!(fork_chat(&mut conn, &files, "g", "g2", "copy")
            .unwrap()
            .is_none());
        assert!(!transcripts::path_in(&files, "g2").unwrap().exists());
    }

    const DAY: i64 = 86_400_000;

    fn set_times(conn: &Connection, id: &str, created: i64, updated: i64) {
        conn.execute(
            "UPDATE chats SET created_at = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, created, updated],
        )
        .unwrap();
    }

    fn ids(found: &[FoundChat]) -> Vec<&str> {
        found.iter().map(|hit| hit.chat.id.as_str()).collect()
    }

    // docs/features/memory-topics-and-chat-recall/spec.md (FR-11).
    #[test]
    fn find_chats_matches_words_and_ranks() {
        let (dir, mut conn) = db();
        let files = dir.path().join(transcripts::DIR_NAME);
        for (id, title) in [
            ("asker", "Now"),
            ("both", "Planning"),
            ("one", "Later"),
            ("phrase", "Other"),
        ] {
            chat(&conn, id, title, None);
        }
        write_transcript(
            &files,
            "both",
            r#"{"records":[{"text":"we set the release date"},{"text":"and agreed a code freeze"}]}"#,
        );
        write_transcript(
            &files,
            "one",
            r#"{"records":[{"text":"a freeze warning for the garden"}]}"#,
        );
        write_transcript(
            &files,
            "phrase",
            r#"{"records":[{"text":"the release freeze starts in December"}]}"#,
        );
        write_transcript(
            &files,
            "asker",
            r#"{"records":[{"text":"what about the release freeze?"}]}"#,
        );
        let now = now_ms();
        set_times(&conn, "both", now - 90 * DAY, now - 90 * DAY);
        set_times(&conn, "phrase", now - 90 * DAY, now - 90 * DAY);
        set_times(&conn, "one", now, now);

        let query = ChatQuery {
            query: "release freeze",
            exclude: Some("asker"),
            limit: 10,
            ..Default::default()
        };
        let found = find_chats(&mut conn, &files, &query).unwrap();
        assert_eq!(
            ids(&found),
            ["phrase", "both", "one"],
            "the phrase first, then both words apart, then the newer chat with one word"
        );
        assert!(found[1].score > found[2].score);

        // Case and punctuation around the words do not matter.
        let loose = find_chats(
            &mut conn,
            &files,
            &ChatQuery {
                query: "FREEZE, release!",
                ..query.clone()
            },
        )
        .unwrap();
        assert_eq!(loose.len(), 3);
        assert_eq!(loose[2].chat.id, "one");

        // A word in the title counts for more than one in the text.
        chat(&conn, "titled", "Freeze checklist", None);
        set_times(&conn, "titled", now - 90 * DAY, now - 90 * DAY);
        let by_title = find_chats(
            &mut conn,
            &files,
            &ChatQuery {
                query: "freeze",
                ..query.clone()
            },
        )
        .unwrap();
        assert_eq!(by_title[0].chat.id, "titled");

        let limited = find_chats(&mut conn, &files, &ChatQuery { limit: 1, ..query }).unwrap();
        assert_eq!(ids(&limited), ["phrase"]);
        assert!(find_chats(
            &mut conn,
            &files,
            &ChatQuery {
                query: "nothing-like-it",
                limit: 10,
                ..Default::default()
            }
        )
        .unwrap()
        .is_empty());
    }

    // FR-32: a summary match ranks above a transcript match.
    #[test]
    fn find_chats_ranks_a_summary_match_above_a_transcript_match() {
        let (dir, mut conn) = db();
        let files = dir.path().join(transcripts::DIR_NAME);
        chat(&conn, "s", "Storage", None);
        chat(&conn, "t", "Ideas", None);
        let now = now_ms();
        set_times(&conn, "s", now, now);
        set_times(&conn, "t", now, now);
        // The transcript of `s` never uses the word; its summary does.
        write_transcript(
            &files,
            "s",
            r#"{"records":[{"text":"use the bigger database"}]}"#,
        );
        write_transcript(&files, "t", r#"{"records":[{"text":"postgres, maybe"}]}"#);
        crate::chat_index::set_indexed(&conn, "s", Some("Decided to move to Postgres."), None, 1)
            .unwrap();
        let found = find_chats(
            &mut conn,
            &files,
            &ChatQuery {
                query: "postgres",
                limit: 10,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(ids(&found), ["s", "t"]);
        assert_eq!(
            found[0].summary.as_deref(),
            Some("Decided to move to Postgres.")
        );
        assert_eq!(found[1].summary, None);
    }

    // FR-13: one project only; never the asking chat or an incognito chat.
    #[test]
    fn find_chats_stays_in_one_project() {
        let (dir, mut conn) = db();
        let files = dir.path().join(transcripts::DIR_NAME);
        let w1 = ensure_workspace(&conn, "/a").unwrap().id;
        let w2 = ensure_workspace(&conn, "/b").unwrap().id;
        chat(&conn, "mine", "needle here", Some(&w1));
        chat(&conn, "same", "needle same project", Some(&w1));
        chat(&conn, "other", "needle other project", Some(&w2));
        chat(&conn, "home", "needle in the home folder", None);
        create_chat(
            &conn,
            &NewChat {
                id: "ghost".into(),
                title: "needle".into(),
                workspace_id: Some(w1.clone()),
                incognito: true,
                ..Default::default()
            },
        )
        .unwrap();
        update_chat(
            &conn,
            "same",
            &ChatPatch {
                archived: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let here = ChatQuery {
            query: "needle",
            workspace: Some(&w1),
            exclude: Some("mine"),
            limit: 10,
            ..Default::default()
        };
        assert_eq!(
            ids(&find_chats(&mut conn, &files, &here).unwrap()),
            ["same"],
            "archived chats count; other projects, the asker and incognito chats do not"
        );
        let home = ChatQuery {
            workspace: None,
            ..here.clone()
        };
        assert_eq!(
            ids(&find_chats(&mut conn, &files, &home).unwrap()),
            ["home"]
        );
        // The list by time keeps the same scope.
        let listed = ChatQuery { query: "", ..here };
        assert_eq!(
            ids(&find_chats(&mut conn, &files, &listed).unwrap()),
            ["same"]
        );
    }

    // FR-12: without a query, chats by time.
    #[test]
    fn find_chats_lists_by_time() {
        let (dir, mut conn) = db();
        let files = dir.path().join(transcripts::DIR_NAME);
        let day2 = 1_000 * DAY;
        let day1 = day2 - DAY;
        for id in ["d1", "d2", "d2b", "span"] {
            chat(&conn, id, id, None);
        }
        set_times(&conn, "d1", day1 + 10, day1 + 20);
        set_times(&conn, "d2", day2 + 10, day2 + 20);
        set_times(&conn, "d2b", day2 + 30, day2 + 40);
        // Started on the first day, continued on the second.
        set_times(&conn, "span", day1 + 50, day2 + 5);
        let base = ChatQuery {
            limit: 10,
            ..Default::default()
        };
        let after = ChatQuery {
            after_ms: Some(day2),
            ..base.clone()
        };
        assert_eq!(
            ids(&find_chats(&mut conn, &files, &after).unwrap()),
            ["d2b", "d2", "span"],
            "newest first"
        );
        let oldest = ChatQuery {
            oldest_first: true,
            ..after.clone()
        };
        assert_eq!(
            ids(&find_chats(&mut conn, &files, &oldest).unwrap()),
            ["span", "d2", "d2b"]
        );
        let before = ChatQuery {
            before_ms: Some(day2),
            ..base.clone()
        };
        assert_eq!(
            ids(&find_chats(&mut conn, &files, &before).unwrap()),
            ["span", "d1"],
            "a chat active on both days is listed for both"
        );
        let one = ChatQuery { limit: 1, ..after };
        assert_eq!(ids(&find_chats(&mut conn, &files, &one).unwrap()), ["d2b"]);
        // A time window narrows a search by words too.
        let words_in_window = ChatQuery {
            query: "d1 d2",
            before_ms: Some(day2),
            ..base
        };
        assert_eq!(
            ids(&find_chats(&mut conn, &files, &words_in_window).unwrap()),
            ["d1"]
        );
    }

    // FR-22: the chats to remove with an incognito chat.
    #[test]
    fn incognito_ids_include_nested_chats() {
        let (_dir, conn) = db();
        let nested = |id: &str, parent: &str, incognito: bool| {
            create_chat(
                &conn,
                &NewChat {
                    id: id.into(),
                    title: id.into(),
                    parent_chat_id: Some(parent.into()),
                    origin: Some("agent".into()),
                    incognito,
                    ..Default::default()
                },
            )
            .unwrap();
        };
        create_chat(
            &conn,
            &NewChat {
                id: "g".into(),
                title: "g".into(),
                incognito: true,
                ..Default::default()
            },
        )
        .unwrap();
        chat(&conn, "plain", "Plain", None);
        nested("g1", "g", true);
        // A nested chat that lost the flag still goes with its parent.
        nested("g2", "g1", false);
        nested("p1", "plain", false);
        assert_eq!(incognito_ids(&conn).unwrap(), ["g", "g1", "g2"]);
        assert_eq!(with_nested(&conn, "g1").unwrap(), ["g1", "g2"]);
        assert_eq!(with_nested(&conn, "plain").unwrap(), ["plain", "p1"]);
        assert!(with_nested(&conn, "missing").unwrap().is_empty());
    }

    // FR-26, FR-30, FR-33: which chats indexing may read.
    #[test]
    fn index_candidates_are_idle_chats_with_new_messages() {
        let (_dir, conn) = db();
        let turn = |id: &str, at: i64| {
            conn.execute(
                "UPDATE chats SET turn_count = 1, updated_at = ?2 WHERE id = ?1",
                params![id, at],
            )
            .unwrap();
        };
        let candidates = |filter: &IndexFilter| -> Vec<String> {
            index_candidates(&conn, filter)
                .unwrap()
                .into_iter()
                .map(|candidate| candidate.chat.id)
                .collect()
        };
        for id in ["old", "fresh", "busy", "empty"] {
            chat(&conn, id, id, None);
        }
        for (id, flags) in [
            ("ghost", (true, false, false)),
            ("quiet", (false, true, false)),
            ("task", (false, false, true)),
        ] {
            create_chat(
                &conn,
                &NewChat {
                    id: id.into(),
                    title: id.into(),
                    incognito: flags.0,
                    memory_off: flags.1,
                    scheduled: flags.2,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        create_chat(
            &conn,
            &NewChat {
                id: "child".into(),
                title: "child".into(),
                parent_chat_id: Some("fresh".into()),
                ..Default::default()
            },
        )
        .unwrap();
        turn("old", 100);
        turn("fresh", 500);
        turn("busy", 900);
        turn("ghost", 500);
        turn("quiet", 500);
        turn("task", 600);
        turn("child", 500);

        let filter = IndexFilter {
            since_ms: Some(400),
            idle_since_ms: Some(800),
            failed_before_ms: 1_000,
            limit: 10,
            ..Default::default()
        };
        assert_eq!(
            candidates(&filter),
            ["fresh", "task"],
            "longest waiting first; not the chat before `since`, the one still active, \
             the one with no turn, or an incognito, memory-off or nested chat"
        );
        let task = &index_candidates(&conn, &filter).unwrap()[1];
        assert!(task.chat.scheduled);
        assert_eq!(task.indexed_through, None);

        // Indexed chats wait for new messages.
        crate::chat_index::set_indexed(&conn, "fresh", Some("s"), Some("e3"), 700).unwrap();
        assert_eq!(candidates(&filter), ["task"]);
        turn("fresh", 750);
        let again = index_candidates(&conn, &filter).unwrap();
        assert_eq!(again[1].chat.id, "fresh");
        assert_eq!(again[1].indexed_through.as_deref(), Some("e3"));

        // A failed chat waits until its failure is old enough.
        crate::chat_index::set_failed(&conn, "task", 1_200, "boom").unwrap();
        assert_eq!(candidates(&filter), ["fresh"]);
        let later = IndexFilter {
            failed_before_ms: 1_200,
            ..filter.clone()
        };
        assert_eq!(candidates(&later), ["task", "fresh"]);

        // "Index now" drops the idle rule; "earlier chats" are those before `since`.
        let now = IndexFilter {
            idle_since_ms: None,
            ..later.clone()
        };
        assert_eq!(candidates(&now), ["task", "fresh", "busy"]);
        let earlier = IndexFilter {
            since_ms: None,
            before_ms: Some(400),
            idle_since_ms: None,
            ..later
        };
        assert_eq!(candidates(&earlier), ["old"]);
    }

    #[test]
    fn search_words_are_trimmed_and_unique() {
        assert_eq!(
            search_words("  Release,  FREEZE! release  (v2) "),
            ["Release", "FREEZE", "v2"]
        );
        assert!(search_words(" ,, ! ").is_empty());
        assert_eq!(search_words("a b c d e f g h i j").len(), MAX_SEARCH_WORDS);
    }

    #[test]
    fn search_follows_transcript_changes() {
        let (dir, mut conn) = db();
        let files = dir.path().join(transcripts::DIR_NAME);
        for id in ["a", "b"] {
            create_chat(
                &conn,
                &NewChat {
                    id: id.into(),
                    title: format!("Chat {id}"),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        // No transcripts yet: titles still match, nothing is indexed.
        assert_eq!(
            search_chats(&mut conn, &files, "chat a", 10).unwrap().len(),
            1
        );
        assert_eq!(refresh_search(&mut conn, &files).unwrap(), 0);

        write_transcript(&files, "a", r#"{"records":[{"text":"alpha"}]}"#);
        write_transcript(&files, "b", r#"{"records":[{"text":"beta"}]}"#);
        assert_eq!(refresh_search(&mut conn, &files).unwrap(), 2);
        // Unchanged files are not read again.
        assert_eq!(refresh_search(&mut conn, &files).unwrap(), 0);
        let ids = |found: Vec<Chat>| found.into_iter().map(|chat| chat.id).collect::<Vec<_>>();
        assert_eq!(
            ids(search_chats(&mut conn, &files, "ALPHA", 10).unwrap()),
            vec!["a"]
        );

        // A new turn in `b` is found by the next search.
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_transcript(
            &files,
            "b",
            r#"{"records":[{"text":"beta"},{"text":"gamma ray"}]}"#,
        );
        assert_eq!(
            ids(search_chats(&mut conn, &files, "gamma", 10).unwrap()),
            vec!["b"]
        );
        // Identifiers and type tags are not searchable text.
        assert!(search_chats(&mut conn, &files, "records", 10)
            .unwrap()
            .is_empty());

        // A transcript removed from disk drops out of the search.
        transcripts::remove(&transcripts::path_in(&files, "a").unwrap()).unwrap();
        assert!(search_chats(&mut conn, &files, "alpha", 10)
            .unwrap()
            .is_empty());
        let indexed: i64 = conn
            .query_row("SELECT COUNT(*) FROM chat_search", [], |r| r.get(0))
            .unwrap();
        assert_eq!(indexed, 1);
    }
}
