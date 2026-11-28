//! A board per channel (docs/features/evo-mates-threads-board/spec.md,
//! FR-8 – FR-15; plan 3.2).
//!
//! A board is opt-in: turning it on makes its key and four columns; turning
//! it off keeps the cards. Cards are numbered per board (`KEY-n`, never
//! reused). Every change to a card writes an event in the same transaction,
//! so the activity log always matches the card. Actors and assignees are
//! `person` or a mate's id; event texts name mates by name and the person as
//! "you" (the log is the person's).

use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::chats::now_ms;
use crate::DbError;

/// The columns a new board starts with (FR-8).
pub const DEFAULT_COLUMNS: [&str; 4] = ["To do", "In progress", "Review", "Done"];
/// A card's priorities, lowest first (FR-9).
pub const PRIORITIES: [&str; 5] = ["none", "low", "medium", "high", "urgent"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub id: String,
    pub name: String,
    pub position: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    pub channel_id: String,
    pub key: String,
    pub enabled: bool,
    pub next_number: i64,
    pub columns: Vec<Column>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckItem {
    pub text: String,
    #[serde(default)]
    pub done: bool,
}

/// What a card links to: a channel line, a file or a thread (FR-9, FR-15).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardLink {
    /// `message`, `file`, `thread` or `chat`.
    pub kind: String,
    pub target: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub id: String,
    pub channel_id: String,
    /// `LAUNCH-12`.
    pub key: String,
    pub number: i64,
    pub column_id: String,
    pub position: f64,
    pub title: String,
    pub description: String,
    pub assignee: Option<String>,
    pub priority: String,
    pub due_ms: Option<i64>,
    pub labels: Vec<String>,
    pub checklist: Vec<CheckItem>,
    pub links: Vec<CardLink>,
    pub thread_root: Option<String>,
    pub source: Option<Value>,
    pub created_by: String,
    pub created_ms: i64,
    pub updated_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardEvent {
    pub id: i64,
    pub card_id: String,
    pub actor: String,
    /// `created`, `moved`, `updated`, `comment`.
    pub kind: String,
    pub text: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCard {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub assignee: Option<String>,
    pub priority: Option<String>,
    pub due_ms: Option<i64>,
    #[serde(default)]
    pub labels: Vec<String>,
    /// The column to add it to; `None`: the first.
    pub column_id: Option<String>,
    /// Put it at the column's top (quick add) instead of its end.
    #[serde(default)]
    pub top: bool,
    pub thread_root: Option<String>,
    #[serde(default)]
    pub links: Vec<CardLink>,
    pub source: Option<Value>,
}

/// Fields to change; `None` leaves a field, `Some(None)` clears an optional one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(default, with = "double_option")]
    pub assignee: Option<Option<String>>,
    pub priority: Option<String>,
    #[serde(default, with = "double_option")]
    pub due_ms: Option<Option<i64>>,
    pub labels: Option<Vec<String>>,
    pub checklist: Option<Vec<CheckItem>>,
    pub links: Option<Vec<CardLink>>,
    #[serde(default, with = "double_option")]
    pub thread_root: Option<Option<String>>,
}

/// `null` clears, a missing field leaves (serde reads both as `None` otherwise).
mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T: Serialize, S: Serializer>(
        value: &Option<Option<T>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            None => s.serialize_none(),
            Some(inner) => inner.serialize(s),
        }
    }

    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<Option<T>>, D::Error> {
        Ok(Some(Option::deserialize(d)?))
    }
}

/// A key proposed from a channel's name (D6): its words' first letters, or
/// its first letters, 2 – 6 capitals.
pub fn propose_key(name: &str) -> String {
    let words: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            w.chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect();
    let first = words
        .first()
        .cloned()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let key: String = if first.len() >= 4 || words.len() < 2 {
        first.chars().take(6).collect()
    } else {
        words
            .iter()
            .filter_map(|w| w.chars().next())
            .collect::<String>()
            .to_ascii_uppercase()
    };
    let key: String = key
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(6)
        .collect();
    if key.len() >= 2 {
        key
    } else {
        format!("{key}BOARD").chars().take(6).collect()
    }
}

/// Whether `key` may name a board: 2 – 6 capital letters or digits, a letter first.
pub fn valid_key(key: &str) -> bool {
    (2..=6).contains(&key.len())
        && key.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn new_id(prefix: &str) -> String {
    crate::chats::new_id(prefix)
}

fn columns(conn: &Connection, channel: &str) -> Result<Vec<Column>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, position FROM board_columns WHERE channel_id = ?1 ORDER BY position, id",
    )?;
    let rows = stmt.query_map([channel], |row| {
        Ok(Column {
            id: row.get(0)?,
            name: row.get(1)?,
            position: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The channel's board, on or off; `None` when it never had one.
pub fn get_board(conn: &Connection, channel: &str) -> Result<Option<Board>, DbError> {
    let row = conn
        .query_row(
            "SELECT key, enabled, next_number FROM boards WHERE channel_id = ?1",
            [channel],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)? == 1,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((key, enabled, next_number)) = row else {
        return Ok(None);
    };
    Ok(Some(Board {
        channel_id: channel.to_string(),
        key,
        enabled,
        next_number,
        columns: columns(conn, channel)?,
    }))
}

/// The channel's board when it is on (what mates' tools and the coordinator act on).
pub fn board_on(conn: &Connection, channel: &str) -> Result<Option<Board>, DbError> {
    Ok(get_board(conn, channel)?.filter(|b| b.enabled))
}

/// Turn the board on (FR-8): a new board takes `key` and the four columns;
/// an existing one keeps its key and cards.
pub fn enable_board(conn: &mut Connection, channel: &str, key: &str) -> Result<Board, DbError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let exists: bool = tx
        .query_row(
            "SELECT 1 FROM boards WHERE channel_id = ?1",
            [channel],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if exists {
        tx.execute(
            "UPDATE boards SET enabled = 1 WHERE channel_id = ?1",
            [channel],
        )?;
    } else {
        let key = key.trim().to_ascii_uppercase();
        if !valid_key(&key) {
            return Err(DbError::Invalid(
                "a board's key is 2 to 6 capital letters or digits, a letter first".into(),
            ));
        }
        let taken: bool = tx
            .query_row("SELECT 1 FROM boards WHERE key = ?1", [&key], |_| Ok(()))
            .optional()?
            .is_some();
        if taken {
            return Err(DbError::Invalid(format!(
                "another channel's board uses the key {key}"
            )));
        }
        tx.execute(
            "INSERT INTO boards (channel_id, key, enabled, next_number, created_ms) VALUES (?1, ?2, 1, 1, ?3)",
            params![channel, key, now_ms()],
        )?;
        for (i, name) in DEFAULT_COLUMNS.iter().enumerate() {
            tx.execute(
                "INSERT INTO board_columns (id, channel_id, name, position) VALUES (?1, ?2, ?3, ?4)",
                params![new_id("col"), channel, name, i as i64],
            )?;
        }
    }
    tx.commit()?;
    get_board(conn, channel)?.ok_or_else(|| DbError::Invalid("no board".into()))
}

/// Turn the board off: the tab goes, the cards stay (FR-8).
pub fn disable_board(conn: &Connection, channel: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE boards SET enabled = 0 WHERE channel_id = ?1",
        [channel],
    )?;
    Ok(())
}

pub fn rename_column(
    conn: &Connection,
    channel: &str,
    column: &str,
    name: &str,
) -> Result<(), DbError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DbError::Invalid("a column needs a name".into()));
    }
    conn.execute(
        "UPDATE board_columns SET name = ?3 WHERE channel_id = ?1 AND id = ?2",
        params![channel, column, name],
    )?;
    Ok(())
}

pub fn add_column(conn: &Connection, channel: &str, name: &str) -> Result<Column, DbError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DbError::Invalid("a column needs a name".into()));
    }
    let position: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position) + 1, 0) FROM board_columns WHERE channel_id = ?1",
        [channel],
        |row| row.get(0),
    )?;
    let id = new_id("col");
    conn.execute(
        "INSERT INTO board_columns (id, channel_id, name, position) VALUES (?1, ?2, ?3, ?4)",
        params![id, channel, name, position],
    )?;
    Ok(Column {
        id,
        name: name.to_string(),
        position,
    })
}

/// Put `column` at `index` among the board's columns.
pub fn move_column(
    conn: &mut Connection,
    channel: &str,
    column: &str,
    index: usize,
) -> Result<(), DbError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut ids: Vec<String> = columns(&tx, channel)?.into_iter().map(|c| c.id).collect();
    let Some(at) = ids.iter().position(|id| id == column) else {
        return Err(DbError::Invalid("no such column".into()));
    };
    let id = ids.remove(at);
    ids.insert(index.min(ids.len()), id);
    for (i, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE board_columns SET position = ?2 WHERE id = ?1",
            params![id, i as i64],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Remove a column; its cards move to the first remaining column. The last
/// column cannot go.
pub fn remove_column(conn: &mut Connection, channel: &str, column: &str) -> Result<(), DbError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let cols = columns(&tx, channel)?;
    let Some(to) = cols.iter().find(|c| c.id != column) else {
        return Err(DbError::Invalid("a board keeps at least one column".into()));
    };
    let start: f64 = tx.query_row(
        "SELECT COALESCE(MAX(position), 0) FROM board_cards WHERE column_id = ?1",
        [&to.id],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE board_cards SET column_id = ?2, position = ?3 + position + 1 WHERE column_id = ?1",
        params![column, to.id, start],
    )?;
    tx.execute(
        "DELETE FROM board_columns WHERE channel_id = ?1 AND id = ?2",
        params![channel, column],
    )?;
    tx.commit()?;
    Ok(())
}

const CARD_COLUMNS: &str = "c.id, c.channel_id, b.key, c.number, c.column_id, c.position, c.title, c.description, c.assignee, c.priority, c.due_ms, c.labels, c.checklist, c.links, c.thread_root, c.source, c.created_by, c.created_ms, c.updated_ms";

fn card_of(row: &Row) -> rusqlite::Result<Card> {
    let json = |i: usize| -> rusqlite::Result<String> { row.get::<_, String>(i) };
    let key: String = row.get(2)?;
    let number: i64 = row.get(3)?;
    Ok(Card {
        id: row.get(0)?,
        channel_id: row.get(1)?,
        key: format!("{key}-{number}"),
        number,
        column_id: row.get(4)?,
        position: row.get(5)?,
        title: row.get(6)?,
        description: row.get(7)?,
        assignee: row.get(8)?,
        priority: row.get(9)?,
        due_ms: row.get(10)?,
        labels: serde_json::from_str(&json(11)?).unwrap_or_default(),
        checklist: serde_json::from_str(&json(12)?).unwrap_or_default(),
        links: serde_json::from_str(&json(13)?).unwrap_or_default(),
        thread_root: row.get(14)?,
        source: row
            .get::<_, Option<String>>(15)?
            .and_then(|s| serde_json::from_str(&s).ok()),
        created_by: row.get(16)?,
        created_ms: row.get(17)?,
        updated_ms: row.get(18)?,
    })
}

/// The board's cards, by column then position.
pub fn list_cards(conn: &Connection, channel: &str) -> Result<Vec<Card>, DbError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CARD_COLUMNS} FROM board_cards c JOIN boards b ON b.channel_id = c.channel_id
         JOIN board_columns col ON col.id = c.column_id
         WHERE c.channel_id = ?1 ORDER BY col.position, c.position, c.number"
    ))?;
    let rows = stmt.query_map([channel], card_of)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get_card(conn: &Connection, id: &str) -> Result<Option<Card>, DbError> {
    Ok(conn
        .query_row(
            &format!("SELECT {CARD_COLUMNS} FROM board_cards c JOIN boards b ON b.channel_id = c.channel_id WHERE c.id = ?1"),
            [id],
            card_of,
        )
        .optional()?)
}

/// A card by its key, `LAUNCH-12` (FR-14).
pub fn card_by_key(conn: &Connection, key: &str) -> Result<Option<Card>, DbError> {
    let Some((board, number)) = key.trim().rsplit_once('-') else {
        return Ok(None);
    };
    let Ok(number) = number.parse::<i64>() else {
        return Ok(None);
    };
    Ok(conn
        .query_row(
            &format!(
                "SELECT {CARD_COLUMNS} FROM board_cards c JOIN boards b ON b.channel_id = c.channel_id
                 WHERE b.key = ?1 AND c.number = ?2"
            ),
            params![board.to_ascii_uppercase(), number],
            card_of,
        )
        .optional()?)
}

/// Open cards assigned to `assignee` on boards that are on, across channels
/// (FR-14): a card is open until it reaches its board's last column. Due
/// first (overdue at the top), then newest.
pub fn assigned_cards(conn: &Connection, assignee: &str) -> Result<Vec<Card>, DbError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CARD_COLUMNS} FROM board_cards c JOIN boards b ON b.channel_id = c.channel_id
         JOIN board_columns col ON col.id = c.column_id
         WHERE c.assignee = ?1 AND b.enabled = 1
           AND col.position < (SELECT MAX(position) FROM board_columns WHERE channel_id = c.channel_id)
         ORDER BY c.due_ms IS NULL, c.due_ms, c.updated_ms DESC"
    ))?;
    let rows = stmt.query_map([assignee], card_of)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// How a log names someone: mates by name, the person as "you".
fn who(conn: &Connection, id: &str) -> String {
    if id == "person" {
        return "you".into();
    }
    conn.query_row("SELECT name FROM mates WHERE id = ?1", [id], |row| {
        row.get(0)
    })
    .optional()
    .ok()
    .flatten()
    .unwrap_or_else(|| id.to_string())
}

fn column_name(conn: &Connection, column: &str) -> Result<String, DbError> {
    Ok(conn.query_row(
        "SELECT name FROM board_columns WHERE id = ?1",
        [column],
        |row| row.get(0),
    )?)
}

fn log(conn: &Connection, card: &str, actor: &str, kind: &str, text: &str) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO board_events (card_id, actor, kind, text, at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![card, actor, kind, text, now_ms()],
    )?;
    Ok(())
}

fn check_priority(priority: &str) -> Result<(), DbError> {
    if PRIORITIES.contains(&priority) {
        Ok(())
    } else {
        Err(DbError::Invalid(format!(
            "priority is one of {}",
            PRIORITIES.join(", ")
        )))
    }
}

/// A position at `index` in `column` (between its neighbours), leaving out `skip`.
fn position_at(
    conn: &Connection,
    column: &str,
    index: Option<usize>,
    skip: Option<&str>,
) -> Result<f64, DbError> {
    let mut stmt = conn
        .prepare("SELECT id, position FROM board_cards WHERE column_id = ?1 ORDER BY position")?;
    let positions: Vec<f64> = stmt
        .query_map([column], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?
        .filter_map(Result::ok)
        .filter(|(id, _)| Some(id.as_str()) != skip)
        .map(|(_, p)| p)
        .collect();
    let index = index.unwrap_or(positions.len()).min(positions.len());
    Ok(
        match (
            index.checked_sub(1).map(|i| positions[i]),
            positions.get(index),
        ) {
            (None, None) => 0.0,
            (None, Some(next)) => next - 1.0,
            (Some(prev), None) => prev + 1.0,
            (Some(prev), Some(next)) => (prev + next) / 2.0,
        },
    )
}

/// Add a card (FR-9, FR-10, FR-11) by `actor`; logs "created KEY-n".
pub fn create_card(
    conn: &mut Connection,
    channel: &str,
    card: &NewCard,
    actor: &str,
) -> Result<Card, DbError> {
    let title = card.title.trim();
    if title.is_empty() {
        return Err(DbError::Invalid("a card needs a title".into()));
    }
    let priority = card.priority.clone().unwrap_or_else(|| "none".into());
    check_priority(&priority)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let board = board_on(&tx, channel)?
        .ok_or_else(|| DbError::Invalid("this channel's board is off".into()))?;
    let column = match &card.column_id {
        Some(id) => board
            .columns
            .iter()
            .find(|c| &c.id == id)
            .ok_or_else(|| DbError::Invalid("no such column".into()))?,
        None => board
            .columns
            .first()
            .ok_or_else(|| DbError::Invalid("the board has no column".into()))?,
    }
    .clone();
    let position = position_at(&tx, &column.id, if card.top { Some(0) } else { None }, None)?;
    let id = new_id("card");
    let now = now_ms();
    tx.execute(
        "INSERT INTO board_cards (id, channel_id, number, column_id, position, title, description, assignee, priority, due_ms, labels, checklist, links, thread_root, source, created_by, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, '[]', ?12, ?13, ?14, ?15, ?16, ?16)",
        params![
            id,
            channel,
            board.next_number,
            column.id,
            position,
            title,
            card.description.trim(),
            card.assignee,
            priority,
            card.due_ms,
            serde_json::to_string(&card.labels).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&card.links).unwrap_or_else(|_| "[]".into()),
            card.thread_root,
            card.source.as_ref().map(Value::to_string),
            actor,
            now,
        ],
    )?;
    tx.execute(
        "UPDATE boards SET next_number = next_number + 1 WHERE channel_id = ?1",
        [channel],
    )?;
    let key = format!("{}-{}", board.key, board.next_number);
    let mut text = format!("created {key} in {}", column.name);
    if let Some(assignee) = &card.assignee {
        text.push_str(&format!(", assigned to {}", who(&tx, assignee)));
    }
    log(&tx, &id, actor, "created", &text)?;
    tx.commit()?;
    get_card(conn, &id)?.ok_or_else(|| DbError::Invalid("no card".into()))
}

/// Change a card's fields (FR-9 – FR-11); logs what changed.
pub fn update_card(
    conn: &mut Connection,
    id: &str,
    patch: &CardPatch,
    actor: &str,
) -> Result<Card, DbError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let card = get_card(&tx, id)?.ok_or_else(|| DbError::Invalid("no such card".into()))?;
    let mut said: Vec<String> = Vec::new();
    if let Some(title) = &patch.title {
        let title = title.trim();
        if title.is_empty() {
            return Err(DbError::Invalid("a card needs a title".into()));
        }
        if title != card.title {
            tx.execute(
                "UPDATE board_cards SET title = ?2 WHERE id = ?1",
                params![id, title],
            )?;
            said.push(format!("renamed it \"{title}\""));
        }
    }
    if let Some(description) = &patch.description {
        if description.trim() != card.description {
            tx.execute(
                "UPDATE board_cards SET description = ?2 WHERE id = ?1",
                params![id, description.trim()],
            )?;
            said.push("changed the description".into());
        }
    }
    if let Some(assignee) = &patch.assignee {
        if *assignee != card.assignee {
            tx.execute(
                "UPDATE board_cards SET assignee = ?2 WHERE id = ?1",
                params![id, assignee],
            )?;
            said.push(match assignee {
                Some(a) => format!("assigned it to {}", who(&tx, a)),
                None => "unassigned it".into(),
            });
        }
    }
    if let Some(priority) = &patch.priority {
        check_priority(priority)?;
        if *priority != card.priority {
            tx.execute(
                "UPDATE board_cards SET priority = ?2 WHERE id = ?1",
                params![id, priority],
            )?;
            said.push(format!("set the priority to {priority}"));
        }
    }
    if let Some(due) = &patch.due_ms {
        if *due != card.due_ms {
            tx.execute(
                "UPDATE board_cards SET due_ms = ?2 WHERE id = ?1",
                params![id, due],
            )?;
            said.push(if due.is_some() {
                "set the due date".into()
            } else {
                "cleared the due date".into()
            });
        }
    }
    if let Some(labels) = &patch.labels {
        if *labels != card.labels {
            tx.execute(
                "UPDATE board_cards SET labels = ?2 WHERE id = ?1",
                params![id, serde_json::to_string(labels).unwrap_or_default()],
            )?;
            said.push("changed the labels".into());
        }
    }
    if let Some(checklist) = &patch.checklist {
        if *checklist != card.checklist {
            tx.execute(
                "UPDATE board_cards SET checklist = ?2 WHERE id = ?1",
                params![id, serde_json::to_string(checklist).unwrap_or_default()],
            )?;
            let done = checklist.iter().filter(|c| c.done).count();
            said.push(format!(
                "updated the checklist ({done} of {})",
                checklist.len()
            ));
        }
    }
    if let Some(links) = &patch.links {
        if *links != card.links {
            tx.execute(
                "UPDATE board_cards SET links = ?2 WHERE id = ?1",
                params![id, serde_json::to_string(links).unwrap_or_default()],
            )?;
            said.push("changed the links".into());
        }
    }
    if let Some(root) = &patch.thread_root {
        if *root != card.thread_root {
            tx.execute(
                "UPDATE board_cards SET thread_root = ?2 WHERE id = ?1",
                params![id, root],
            )?;
            said.push(if root.is_some() {
                "linked a thread".into()
            } else {
                "unlinked its thread".into()
            });
        }
    }
    if !said.is_empty() {
        tx.execute(
            "UPDATE board_cards SET updated_ms = ?2 WHERE id = ?1",
            params![id, now_ms()],
        )?;
        log(
            &tx,
            id,
            actor,
            "updated",
            &format!("{} on {}", said.join(", "), card.key),
        )?;
    }
    tx.commit()?;
    get_card(conn, id)?.ok_or_else(|| DbError::Invalid("no such card".into()))
}

/// Move a card to `column` at `index` (end when `None`) (FR-10, FR-11);
/// logs "moved KEY to Column" when the column changes.
pub fn move_card(
    conn: &mut Connection,
    id: &str,
    column: &str,
    index: Option<usize>,
    actor: &str,
) -> Result<Card, DbError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let card = get_card(&tx, id)?.ok_or_else(|| DbError::Invalid("no such card".into()))?;
    let same_board: bool = tx
        .query_row(
            "SELECT 1 FROM board_columns WHERE id = ?1 AND channel_id = ?2",
            params![column, card.channel_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !same_board {
        return Err(DbError::Invalid(
            "no such column on this card's board".into(),
        ));
    }
    let position = position_at(&tx, column, index, Some(id))?;
    tx.execute(
        "UPDATE board_cards SET column_id = ?2, position = ?3, updated_ms = ?4 WHERE id = ?1",
        params![id, column, position, now_ms()],
    )?;
    if card.column_id != column {
        let name = column_name(&tx, column)?;
        log(
            &tx,
            id,
            actor,
            "moved",
            &format!("moved {} to {name}", card.key),
        )?;
    }
    tx.commit()?;
    get_card(conn, id)?.ok_or_else(|| DbError::Invalid("no such card".into()))
}

/// A comment in a card's log (FR-11).
pub fn comment_card(
    conn: &Connection,
    id: &str,
    actor: &str,
    text: &str,
) -> Result<CardEvent, DbError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(DbError::Invalid("a comment needs words".into()));
    }
    if get_card(conn, id)?.is_none() {
        return Err(DbError::Invalid("no such card".into()));
    }
    log(conn, id, actor, "comment", text)?;
    conn.execute(
        "UPDATE board_cards SET updated_ms = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    let event_id = conn.last_insert_rowid();
    Ok(conn.query_row(
        "SELECT id, card_id, actor, kind, text, at_ms FROM board_events WHERE id = ?1",
        [event_id],
        event_of,
    )?)
}

/// Delete a card: the person only (mates have no tool for it, D7).
pub fn delete_card(conn: &Connection, id: &str) -> Result<(), DbError> {
    conn.execute("DELETE FROM board_cards WHERE id = ?1", [id])?;
    Ok(())
}

fn event_of(row: &Row) -> rusqlite::Result<CardEvent> {
    Ok(CardEvent {
        id: row.get(0)?,
        card_id: row.get(1)?,
        actor: row.get(2)?,
        kind: row.get(3)?,
        text: row.get(4)?,
        at_ms: row.get(5)?,
    })
}

/// A card's activity log, oldest first (FR-9).
pub fn card_events(conn: &Connection, id: &str) -> Result<Vec<CardEvent>, DbError> {
    let mut stmt = conn.prepare("SELECT id, card_id, actor, kind, text, at_ms FROM board_events WHERE card_id = ?1 ORDER BY at_ms, id")?;
    let rows = stmt.query_map([id], event_of)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The column an assignment's card moves to when its member reports back
/// (FR-12, D8): Review, else the last but one, else the last.
pub fn review_column(board: &Board) -> Option<&Column> {
    board
        .columns
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case("review"))
        .or_else(|| {
            if board.columns.len() > 2 {
                board.columns.get(board.columns.len() - 2)
            } else {
                board.columns.last()
            }
        })
}

/// The newest open card made from an assignment of `from` to `to` in the
/// channel (FR-12): open means before the review column.
pub fn assignment_card(
    conn: &Connection,
    channel: &str,
    from: &str,
    to: &str,
) -> Result<Option<Card>, DbError> {
    let Some(board) = board_on(conn, channel)? else {
        return Ok(None);
    };
    let review = review_column(&board)
        .map(|c| c.position)
        .unwrap_or(i64::MAX);
    let cards = list_cards(conn, channel)?;
    let positions: std::collections::HashMap<&str, i64> = board
        .columns
        .iter()
        .map(|c| (c.id.as_str(), c.position))
        .collect();
    Ok(cards
        .into_iter()
        .filter(|c| c.assignee.as_deref() == Some(to))
        .filter(|c| {
            positions
                .get(c.column_id.as_str())
                .copied()
                .unwrap_or(i64::MAX)
                < review
        })
        .filter(|c| {
            c.source
                .as_ref()
                .and_then(|s| s.get("from"))
                .and_then(Value::as_str)
                == Some(from)
        })
        .max_by_key(|c| c.created_ms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::{create_channel, NewChannel};
    use crate::mates::{create_mate, NewMate};
    use serde_json::json;

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        for id in ["kai", "lin"] {
            create_mate(
                &mut conn,
                &NewMate {
                    id: id.into(),
                    chat_id: format!("chat-{id}"),
                    name: format!("{}{}", id[..1].to_uppercase(), &id[1..]),
                    avatar: json!({}),
                    folder: format!("/mates/{id}"),
                    ..NewMate::default()
                },
                "ask",
            )
            .unwrap();
        }
        create_channel(
            &mut conn,
            &NewChannel {
                id: "ch".into(),
                chat_id: "room".into(),
                name: "Launch room".into(),
                members: vec!["kai".into(), "lin".into()],
                ..NewChannel::default()
            },
        )
        .unwrap();
        (dir, conn)
    }

    #[test]
    fn keys_are_proposed_from_the_name() {
        assert_eq!(propose_key("Launch room"), "LAUNCH");
        assert_eq!(propose_key("QA"), "QA");
        assert_eq!(propose_key("go to market"), "GTM");
        assert_eq!(propose_key("x"), "XBOARD");
        assert!(
            valid_key("LAUNCH")
                && valid_key("Q4")
                && !valid_key("4Q")
                && !valid_key("A")
                && !valid_key("launch")
        );
    }

    #[test]
    fn a_board_turns_on_with_four_columns_and_off_keeps_its_cards() {
        let (_dir, mut conn) = db();
        assert_eq!(get_board(&conn, "ch").unwrap(), None);
        assert!(
            enable_board(&mut conn, "ch", "launch!").is_err(),
            "a bad key is refused"
        );
        let board = enable_board(&mut conn, "ch", "launch").unwrap();
        assert_eq!(board.key, "LAUNCH");
        assert_eq!(
            board
                .columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            DEFAULT_COLUMNS
        );
        let card = create_card(
            &mut conn,
            "ch",
            &NewCard {
                title: "Cut the build".into(),
                ..NewCard::default()
            },
            "person",
        )
        .unwrap();
        assert_eq!(card.key, "LAUNCH-1");
        disable_board(&conn, "ch").unwrap();
        assert!(create_card(
            &mut conn,
            "ch",
            &NewCard {
                title: "x".into(),
                ..NewCard::default()
            },
            "kai"
        )
        .is_err());
        let again = enable_board(&mut conn, "ch", "OTHER").unwrap();
        assert_eq!(again.key, "LAUNCH", "an old board keeps its key");
        assert_eq!(list_cards(&conn, "ch").unwrap().len(), 1);
    }

    #[test]
    fn numbers_are_never_reused_and_moves_are_logged() {
        let (_dir, mut conn) = db();
        let board = enable_board(&mut conn, "ch", "LAUNCH").unwrap();
        let a = create_card(
            &mut conn,
            "ch",
            &NewCard {
                title: "A".into(),
                assignee: Some("kai".into()),
                ..NewCard::default()
            },
            "person",
        )
        .unwrap();
        delete_card(&conn, &a.id).unwrap();
        let b = create_card(
            &mut conn,
            "ch",
            &NewCard {
                title: "B".into(),
                ..NewCard::default()
            },
            "person",
        )
        .unwrap();
        assert_eq!(b.key, "LAUNCH-2");
        let review = &board.columns[2];
        let moved = move_card(&mut conn, &b.id, &review.id, None, "lin").unwrap();
        assert_eq!(moved.column_id, review.id);
        let log = card_events(&conn, &b.id).unwrap();
        assert_eq!(
            log.last().map(|e| (e.actor.as_str(), e.text.as_str())),
            Some(("lin", "moved LAUNCH-2 to Review"))
        );
        assert_eq!(
            card_by_key(&conn, "launch-2").unwrap().map(|c| c.id),
            Some(b.id.clone())
        );
        assert_eq!(card_by_key(&conn, "LAUNCH-1").unwrap(), None);
    }

    #[test]
    fn cards_order_within_a_column_and_quick_add_goes_on_top() {
        let (_dir, mut conn) = db();
        let board = enable_board(&mut conn, "ch", "LAUNCH").unwrap();
        let todo = board.columns[0].id.clone();
        let one = create_card(
            &mut conn,
            "ch",
            &NewCard {
                title: "one".into(),
                ..NewCard::default()
            },
            "person",
        )
        .unwrap();
        let two = create_card(
            &mut conn,
            "ch",
            &NewCard {
                title: "two".into(),
                ..NewCard::default()
            },
            "person",
        )
        .unwrap();
        let _top = create_card(
            &mut conn,
            "ch",
            &NewCard {
                title: "top".into(),
                top: true,
                ..NewCard::default()
            },
            "person",
        )
        .unwrap();
        let titles = |conn: &Connection| {
            list_cards(conn, "ch")
                .unwrap()
                .into_iter()
                .map(|c| c.title)
                .collect::<Vec<_>>()
        };
        assert_eq!(titles(&conn), ["top", "one", "two"]);
        move_card(&mut conn, &two.id, &todo, Some(1), "person").unwrap();
        assert_eq!(titles(&conn), ["top", "two", "one"]);
        assert!(
            card_events(&conn, &two.id)
                .unwrap()
                .iter()
                .all(|e| e.kind != "moved"),
            "a reorder is not a move"
        );
        let _ = one;
    }

    #[test]
    fn updates_and_comments_go_to_the_log() {
        let (_dir, mut conn) = db();
        enable_board(&mut conn, "ch", "LAUNCH").unwrap();
        let card = create_card(
            &mut conn,
            "ch",
            &NewCard {
                title: "Notes".into(),
                ..NewCard::default()
            },
            "person",
        )
        .unwrap();
        let patch: CardPatch =
            serde_json::from_value(json!({ "assignee": "lin", "priority": "high", "dueMs": 5 }))
                .unwrap();
        let card = update_card(&mut conn, &card.id, &patch, "kai").unwrap();
        assert_eq!(
            (
                card.assignee.as_deref(),
                card.priority.as_str(),
                card.due_ms
            ),
            (Some("lin"), "high", Some(5))
        );
        let cleared: CardPatch = serde_json::from_value(json!({ "dueMs": null })).unwrap();
        assert_eq!(
            update_card(&mut conn, &card.id, &cleared, "person")
                .unwrap()
                .due_ms,
            None
        );
        assert!(update_card(
            &mut conn,
            &card.id,
            &CardPatch {
                priority: Some("huge".into()),
                ..CardPatch::default()
            },
            "person"
        )
        .is_err());
        comment_card(&conn, &card.id, "lin", "Drafting now").unwrap();
        let log = card_events(&conn, &card.id).unwrap();
        assert_eq!(
            log[1].text,
            "assigned it to Lin, set the priority to high, set the due date on LAUNCH-1"
        );
        assert_eq!(log.last().map(|e| e.kind.as_str()), Some("comment"));
    }

    #[test]
    fn my_cards_are_open_cards_on_boards_that_are_on() {
        let (_dir, mut conn) = db();
        let board = enable_board(&mut conn, "ch", "LAUNCH").unwrap();
        let mine = create_card(
            &mut conn,
            "ch",
            &NewCard {
                title: "Confirm links".into(),
                assignee: Some("person".into()),
                due_ms: Some(10),
                ..NewCard::default()
            },
            "kai",
        )
        .unwrap();
        let done = create_card(
            &mut conn,
            "ch",
            &NewCard {
                title: "Old".into(),
                assignee: Some("person".into()),
                ..NewCard::default()
            },
            "kai",
        )
        .unwrap();
        move_card(&mut conn, &done.id, &board.columns[3].id, None, "person").unwrap();
        assert_eq!(
            assigned_cards(&conn, "person")
                .unwrap()
                .into_iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
            vec![mine.id]
        );
        disable_board(&conn, "ch").unwrap();
        assert!(assigned_cards(&conn, "person").unwrap().is_empty());
    }

    #[test]
    fn a_column_goes_with_its_cards_moved_and_an_assignment_card_is_found() {
        let (_dir, mut conn) = db();
        let board = enable_board(&mut conn, "ch", "LAUNCH").unwrap();
        let card = create_card(
            &mut conn,
            "ch",
            &NewCard {
                title: "Build".into(),
                assignee: Some("kai".into()),
                source: Some(json!({ "from": "lin", "to": "kai" })),
                column_id: Some(board.columns[1].id.clone()),
                ..NewCard::default()
            },
            "lin",
        )
        .unwrap();
        assert_eq!(
            assignment_card(&conn, "ch", "lin", "kai")
                .unwrap()
                .map(|c| c.id),
            Some(card.id.clone())
        );
        assert_eq!(
            review_column(&board).map(|c| c.name.as_str()),
            Some("Review")
        );
        remove_column(&mut conn, "ch", &board.columns[1].id).unwrap();
        assert_eq!(
            get_card(&conn, &card.id).unwrap().unwrap().column_id,
            board.columns[0].id
        );
        move_column(&mut conn, "ch", &board.columns[3].id, 0).unwrap();
        assert_eq!(
            get_board(&conn, "ch").unwrap().unwrap().columns[0].name,
            "Done"
        );
        add_column(&conn, "ch", "Blocked").unwrap();
        rename_column(&conn, "ch", &board.columns[0].id, "Backlog").unwrap();
        let names: Vec<String> = get_board(&conn, "ch")
            .unwrap()
            .unwrap()
            .columns
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, ["Done", "Backlog", "Review", "Blocked"]);
    }
}
