//! Evo Mates' channels: a room where the person talks with several mates
//! (docs/features/evo-mates/spec.md, FR-14 – FR-19, FR-53 – FR-57; plan 3.4).
//!
//! A channel is a row here plus one root chat of kind `channel`, created
//! together in one transaction. Its members are mates; each member's seat (a
//! nested session of the root, kind `seat`) is recorded once it first works
//! in the channel. Deleting a channel returns its chats (the root and every
//! seat) so the caller can stop their engines and remove their transcripts.

use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::chats::{now_ms, NewChat};
use crate::DbError;

/// Who answers a message that mentions nobody (FR-15).
pub const REPLY_RULES: [&str; 4] = ["auto", "lead", "everyone", "mentioned"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub mate_id: String,
    /// The member's seat chat, once it first worked here.
    pub seat_chat_id: Option<String>,
    pub position: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub instructions: String,
    /// `auto`, `lead`, `everyone` or `mentioned`.
    pub reply_rule: String,
    pub lead_mate_id: Option<String>,
    /// A project folder the members work in; `None`: each mate's own.
    pub folder: Option<String>,
    /// The channel's root chat (kind `channel`).
    pub chat_id: String,
    pub pinned: bool,
    pub hidden: bool,
    pub position: i64,
    pub members: Vec<Member>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A channel to create, with its root chat's id.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewChannel {
    pub id: String,
    pub chat_id: String,
    pub name: String,
    #[serde(default)]
    pub instructions: String,
    pub reply_rule: Option<String>,
    pub lead_mate_id: Option<String>,
    pub folder: Option<String>,
    /// Member mate ids, in order.
    #[serde(default)]
    pub members: Vec<String>,
}

/// Fields to change; `None` leaves a field. For the optional columns,
/// `Some(None)` clears them. `members`, when given, replaces the list (seats
/// of members who stay are kept).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPatch {
    pub name: Option<String>,
    pub instructions: Option<String>,
    pub reply_rule: Option<String>,
    #[serde(default, with = "double_option")]
    pub lead_mate_id: Option<Option<String>>,
    #[serde(default, with = "double_option")]
    pub folder: Option<Option<String>>,
    pub pinned: Option<bool>,
    pub hidden: Option<bool>,
    pub members: Option<Vec<String>>,
}

mod double_option {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(deserializer).map(Some)
    }
}

const COLUMNS: &str = "id, name, instructions, reply_rule, lead_mate_id, folder, chat_id, pinned, hidden, position, created_at, updated_at";

fn channel_row(row: &Row<'_>) -> rusqlite::Result<Channel> {
    Ok(Channel {
        id: row.get(0)?,
        name: row.get(1)?,
        instructions: row.get(2)?,
        reply_rule: row.get(3)?,
        lead_mate_id: row.get(4)?,
        folder: row.get(5)?,
        chat_id: row.get(6)?,
        pinned: row.get(7)?,
        hidden: row.get(8)?,
        position: row.get(9)?,
        members: Vec::new(),
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn members_of(conn: &Connection, channel: &str) -> Result<Vec<Member>, DbError> {
    let mut statement = conn.prepare(
        "SELECT mate_id, seat_chat_id, position FROM channel_members WHERE channel_id = ?1 ORDER BY position, mate_id",
    )?;
    let rows = statement.query_map([channel], |row| {
        Ok(Member {
            mate_id: row.get(0)?,
            seat_chat_id: row.get(1)?,
            position: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn with_members(conn: &Connection, mut channel: Channel) -> Result<Channel, DbError> {
    channel.members = members_of(conn, &channel.id)?;
    Ok(channel)
}

fn check_rule(rule: &str) -> Result<(), DbError> {
    if REPLY_RULES.contains(&rule) {
        Ok(())
    } else {
        Err(DbError::Invalid(format!("unknown reply rule `{rule}`")))
    }
}

pub fn list_channels(conn: &Connection) -> Result<Vec<Channel>, DbError> {
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM channels ORDER BY position, created_at"
    ))?;
    let rows: Vec<Channel> = statement
        .query_map([], channel_row)?
        .collect::<Result<_, _>>()?;
    rows.into_iter()
        .map(|channel| with_members(conn, channel))
        .collect()
}

pub fn get_channel(conn: &Connection, id: &str) -> Result<Option<Channel>, DbError> {
    let channel = conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM channels WHERE id = ?1"),
            [id],
            channel_row,
        )
        .optional()?;
    channel
        .map(|channel| with_members(conn, channel))
        .transpose()
}

/// The channel whose root chat is `chat`.
pub fn channel_of_chat(conn: &Connection, chat: &str) -> Result<Option<Channel>, DbError> {
    let id: Option<String> = conn
        .query_row(
            "SELECT id FROM channels WHERE chat_id = ?1",
            [chat],
            |row| row.get(0),
        )
        .optional()?;
    match id {
        Some(id) => get_channel(conn, &id),
        None => Ok(None),
    }
}

fn write_members(conn: &Connection, channel: &str, members: &[String]) -> Result<(), DbError> {
    // Members who stay keep their seat.
    let kept = members_of(conn, channel)?;
    conn.execute(
        "DELETE FROM channel_members WHERE channel_id = ?1",
        [channel],
    )?;
    for (position, mate) in members.iter().enumerate() {
        let seat = kept
            .iter()
            .find(|m| &m.mate_id == mate)
            .and_then(|m| m.seat_chat_id.clone());
        conn.execute(
            "INSERT INTO channel_members (channel_id, mate_id, seat_chat_id, position) VALUES (?1, ?2, ?3, ?4)",
            params![channel, mate, seat, position as i64],
        )?;
    }
    Ok(())
}

/// Create a channel and its root chat together.
pub fn create_channel(conn: &mut Connection, channel: &NewChannel) -> Result<Channel, DbError> {
    if channel.name.trim().is_empty() {
        return Err(DbError::Invalid("a channel needs a name".into()));
    }
    let rule = channel.reply_rule.as_deref().unwrap_or("auto");
    check_rule(rule)?;
    let lead = channel
        .lead_mate_id
        .clone()
        .or_else(|| channel.members.first().cloned());
    let now = now_ms();
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    crate::chats::create_chat(
        &tx,
        &NewChat {
            id: channel.chat_id.clone(),
            title: channel.name.trim().to_string(),
            kind: Some("channel".into()),
            ..NewChat::default()
        },
    )?;
    let position: i64 = tx.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM channels",
        [],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO channels (id, name, instructions, reply_rule, lead_mate_id, folder, chat_id, position, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![
            channel.id,
            channel.name.trim(),
            channel.instructions,
            rule,
            lead,
            channel.folder,
            channel.chat_id,
            position,
            now
        ],
    )?;
    write_members(&tx, &channel.id, &channel.members)?;
    tx.commit()?;
    get_channel(conn, &channel.id)?
        .ok_or_else(|| DbError::Invalid("the new channel is gone".into()))
}

pub fn update_channel(
    conn: &mut Connection,
    id: &str,
    patch: &ChannelPatch,
) -> Result<Option<Channel>, DbError> {
    if let Some(rule) = &patch.reply_rule {
        check_rule(rule)?;
    }
    if patch
        .name
        .as_deref()
        .is_some_and(|name| name.trim().is_empty())
    {
        return Err(DbError::Invalid("a channel needs a name".into()));
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let exists: bool = tx
        .query_row("SELECT 1 FROM channels WHERE id = ?1", [id], |_| Ok(true))
        .optional()?
        .unwrap_or(false);
    if !exists {
        return Ok(None);
    }
    let now = now_ms();
    let set = |column: &str, value: &dyn rusqlite::ToSql| -> Result<(), DbError> {
        tx.execute(
            &format!("UPDATE channels SET {column} = ?1, updated_at = ?2 WHERE id = ?3"),
            params![value, now, id],
        )?;
        Ok(())
    };
    if let Some(name) = &patch.name {
        set("name", &name.trim())?;
        tx.execute(
            "UPDATE chats SET title = ?1 WHERE id = (SELECT chat_id FROM channels WHERE id = ?2)",
            params![name.trim(), id],
        )?;
    }
    if let Some(instructions) = &patch.instructions {
        set("instructions", instructions)?;
    }
    if let Some(rule) = &patch.reply_rule {
        set("reply_rule", rule)?;
    }
    if let Some(lead) = &patch.lead_mate_id {
        set("lead_mate_id", lead)?;
    }
    if let Some(folder) = &patch.folder {
        set("folder", folder)?;
    }
    if let Some(pinned) = patch.pinned {
        set("pinned", &pinned)?;
    }
    if let Some(hidden) = patch.hidden {
        set("hidden", &hidden)?;
    }
    if let Some(members) = &patch.members {
        write_members(&tx, id, members)?;
        // A lead who left is no lead.
        tx.execute(
            "UPDATE channels SET lead_mate_id = NULL WHERE id = ?1 AND lead_mate_id IS NOT NULL
               AND lead_mate_id NOT IN (SELECT mate_id FROM channel_members WHERE channel_id = ?1)",
            [id],
        )?;
        set("updated_at", &now)?;
    }
    tx.commit()?;
    get_channel(conn, id)
}

/// Record the seat a member works in.
pub fn set_seat(
    conn: &Connection,
    channel: &str,
    mate: &str,
    seat_chat: &str,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE channel_members SET seat_chat_id = ?3 WHERE channel_id = ?1 AND mate_id = ?2",
        params![channel, mate, seat_chat],
    )?;
    Ok(())
}

/// Delete a channel with its root chat and its seats' chats. Returns those
/// chats' ids (root first).
pub fn delete_channel(conn: &mut Connection, id: &str) -> Result<Vec<String>, DbError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let Some(root): Option<String> = tx
        .query_row("SELECT chat_id FROM channels WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .optional()?
    else {
        return Ok(Vec::new());
    };
    let mut chats = vec![root.clone()];
    {
        let mut statement =
            tx.prepare("SELECT seat_chat_id FROM channel_members WHERE channel_id = ?1 AND seat_chat_id IS NOT NULL")?;
        let seats = statement.query_map([id], |row| row.get::<_, String>(0))?;
        for seat in seats {
            chats.push(seat?);
        }
    }
    tx.execute("DELETE FROM channels WHERE id = ?1", [id])?;
    for chat in &chats {
        tx.execute("DELETE FROM chats WHERE id = ?1", [chat])?;
    }
    tx.commit()?;
    Ok(chats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mates::{create_mate, NewMate};
    use serde_json::json;

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        (dir, conn)
    }

    fn mate(conn: &mut Connection, id: &str) {
        create_mate(
            conn,
            &NewMate {
                id: id.into(),
                chat_id: format!("chat-{id}"),
                name: id.to_uppercase(),
                avatar: json!({}),
                folder: format!("/mates/{id}"),
                ..NewMate::default()
            },
            "ask",
        )
        .unwrap();
    }

    fn room(members: &[&str]) -> NewChannel {
        NewChannel {
            id: "ch".into(),
            chat_id: "room".into(),
            name: " Launch room ".into(),
            members: members.iter().map(|m| m.to_string()).collect(),
            ..NewChannel::default()
        }
    }

    #[test]
    fn a_channel_comes_with_its_root_chat_and_members() {
        let (_dir, mut conn) = db();
        mate(&mut conn, "a");
        mate(&mut conn, "b");
        let channel = create_channel(&mut conn, &room(&["a", "b"])).unwrap();
        assert_eq!(channel.name, "Launch room");
        assert_eq!(channel.reply_rule, "auto");
        assert_eq!(
            channel.lead_mate_id.as_deref(),
            Some("a"),
            "the first member leads by default"
        );
        assert_eq!(
            channel
                .members
                .iter()
                .map(|m| m.mate_id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        let chat = crate::chats::get_chat(&conn, "room").unwrap().unwrap();
        assert_eq!(chat.kind, "channel");
        assert_eq!(channel_of_chat(&conn, "room").unwrap().unwrap().id, "ch");
        assert!(create_channel(
            &mut conn,
            &NewChannel {
                reply_rule: Some("loud".into()),
                ..room(&[])
            }
        )
        .is_err());
    }

    #[test]
    fn members_change_and_keep_their_seats() {
        let (_dir, mut conn) = db();
        for id in ["a", "b", "c"] {
            mate(&mut conn, id);
        }
        create_channel(&mut conn, &room(&["a", "b"])).unwrap();
        set_seat(&conn, "ch", "b", "seat-b").unwrap();
        let patch = ChannelPatch {
            members: Some(vec!["c".into(), "b".into()]),
            reply_rule: Some("lead".into()),
            ..ChannelPatch::default()
        };
        let channel = update_channel(&mut conn, "ch", &patch).unwrap().unwrap();
        assert_eq!(channel.reply_rule, "lead");
        assert_eq!(
            channel
                .members
                .iter()
                .map(|m| m.mate_id.as_str())
                .collect::<Vec<_>>(),
            ["c", "b"]
        );
        assert_eq!(channel.members[1].seat_chat_id.as_deref(), Some("seat-b"));
        assert_eq!(channel.lead_mate_id, None, "the lead left");
        let renamed = update_channel(
            &mut conn,
            "ch",
            &ChannelPatch {
                name: Some("Room 2".into()),
                ..ChannelPatch::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(renamed.name, "Room 2");
        assert_eq!(
            crate::chats::get_chat(&conn, "room")
                .unwrap()
                .unwrap()
                .title,
            "Room 2"
        );
        assert!(update_channel(&mut conn, "nope", &ChannelPatch::default())
            .unwrap()
            .is_none());
    }

    #[test]
    fn deleting_a_channel_returns_its_root_and_seats() {
        let (_dir, mut conn) = db();
        mate(&mut conn, "a");
        create_channel(&mut conn, &room(&["a"])).unwrap();
        crate::chats::create_chat(
            &conn,
            &NewChat {
                id: "seat-a".into(),
                title: "A".into(),
                kind: Some("seat".into()),
                mate_id: Some("a".into()),
                ..NewChat::default()
            },
        )
        .unwrap();
        set_seat(&conn, "ch", "a", "seat-a").unwrap();
        assert_eq!(delete_channel(&mut conn, "ch").unwrap(), ["room", "seat-a"]);
        assert!(get_channel(&conn, "ch").unwrap().is_none());
        assert!(crate::chats::get_chat(&conn, "room").unwrap().is_none());
        assert!(
            crate::mates::get_mate(&conn, "a").unwrap().is_some(),
            "the mate stays"
        );
    }
}
