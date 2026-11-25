//! Evo Mates' mates: lasting AI teammates, each a profile here and one direct
//! chat of kind `mate` (docs/features/evo-mates/spec.md, FR-6 – FR-13,
//! FR-25, FR-39 – FR-45; plan 3.4).
//!
//! A mate and its direct chat are created together, in one transaction, so a
//! mate never exists without its conversation. Deleting a mate returns the
//! chats that went with it (its direct chat and its channel seats) so the
//! caller can stop their engines and remove their transcripts. List fields
//! (tools, connectors, skills, apps) are JSON arrays; `None` means the
//! defaults (every app, every tool the mate's approval allows).

use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::chats::{now_ms, NewChat};
use crate::DbError;

/// What a mate may ask before acting (approval modes, docs/features/approval-modes).
pub const APPROVAL_MODES: [&str; 3] = ["ask", "auto-edits", "auto"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mate {
    pub id: String,
    pub name: String,
    /// The face: `{definition, hue, material}` or `{picture}`.
    pub avatar: Value,
    /// One line: what the mate does.
    pub role: String,
    pub instructions: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub tools: Option<Vec<String>>,
    pub connectors: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    /// `None`: every connected app (FR-39).
    pub apps: Option<Vec<Value>>,
    /// `ask`, `auto-edits` or `auto`.
    pub approval_mode: String,
    /// Sending on your behalf: `ask` or `send` (FR-42).
    pub sending: String,
    pub notifications: bool,
    pub paused: bool,
    /// The mate's own folder (FR-25).
    pub folder: String,
    /// A project folder it works in instead of its own.
    pub project_folder: Option<String>,
    /// Its direct conversation.
    pub chat_id: String,
    /// The gallery role it was made from.
    pub template: Option<String>,
    pub pinned: bool,
    pub hidden: bool,
    pub position: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A mate to create, with its direct chat's id.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMate {
    pub id: String,
    pub chat_id: String,
    pub name: String,
    pub avatar: Value,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub instructions: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub tools: Option<Vec<String>>,
    pub connectors: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub apps: Option<Vec<Value>>,
    /// The app's approval mode when unset (spec 6.2).
    pub approval_mode: Option<String>,
    pub folder: String,
    pub project_folder: Option<String>,
    pub template: Option<String>,
}

/// Fields to change on a mate; `None` leaves a field as it is. For the
/// optional columns, `Some(None)` clears them.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatePatch {
    pub name: Option<String>,
    pub avatar: Option<Value>,
    pub role: Option<String>,
    pub instructions: Option<String>,
    #[serde(default, with = "double_option")]
    pub provider: Option<Option<String>>,
    #[serde(default, with = "double_option")]
    pub model: Option<Option<String>>,
    #[serde(default, with = "double_option")]
    pub thinking: Option<Option<String>>,
    #[serde(default, with = "double_option")]
    pub tools: Option<Option<Vec<String>>>,
    #[serde(default, with = "double_option")]
    pub connectors: Option<Option<Vec<String>>>,
    #[serde(default, with = "double_option")]
    pub skills: Option<Option<Vec<String>>>,
    #[serde(default, with = "double_option")]
    pub apps: Option<Option<Vec<Value>>>,
    pub approval_mode: Option<String>,
    pub sending: Option<String>,
    pub notifications: Option<bool>,
    pub paused: Option<bool>,
    #[serde(default, with = "double_option")]
    pub project_folder: Option<Option<String>>,
    pub pinned: Option<bool>,
    pub hidden: Option<bool>,
}

/// `null` in a patch clears a field; a missing key leaves it.
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

const COLUMNS: &str = "id, name, avatar, role, instructions, provider, model, thinking, tools, connectors, skills, apps, \
     approval_mode, sending, notifications, paused, folder, project_folder, chat_id, template, pinned, hidden, position, \
     created_at, updated_at";

fn json_list<T: serde::de::DeserializeOwned>(text: Option<String>) -> Option<Vec<T>> {
    text.and_then(|text| serde_json::from_str(&text).ok())
}

fn to_json<T: Serialize>(value: &Option<Vec<T>>) -> Option<String> {
    value
        .as_ref()
        .map(|list| serde_json::to_string(list).unwrap_or_else(|_| "[]".into()))
}

fn mate_row(row: &Row<'_>) -> rusqlite::Result<Mate> {
    Ok(Mate {
        id: row.get(0)?,
        name: row.get(1)?,
        avatar: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or(Value::Null),
        role: row.get(3)?,
        instructions: row.get(4)?,
        provider: row.get(5)?,
        model: row.get(6)?,
        thinking: row.get(7)?,
        tools: json_list(row.get(8)?),
        connectors: json_list(row.get(9)?),
        skills: json_list(row.get(10)?),
        apps: json_list(row.get(11)?),
        approval_mode: row.get(12)?,
        sending: row.get(13)?,
        notifications: row.get(14)?,
        paused: row.get(15)?,
        folder: row.get(16)?,
        project_folder: row.get(17)?,
        chat_id: row.get(18)?,
        template: row.get(19)?,
        pinned: row.get(20)?,
        hidden: row.get(21)?,
        position: row.get(22)?,
        created_at: row.get(23)?,
        updated_at: row.get(24)?,
    })
}

fn check_mode(mode: &str) -> Result<(), DbError> {
    if APPROVAL_MODES.contains(&mode) {
        Ok(())
    } else {
        Err(DbError::Invalid(format!(
            "unknown approval mode \"{mode}\""
        )))
    }
}

/// Every mate, pinned first, then in the person's order, then newest.
pub fn list_mates(conn: &Connection) -> Result<Vec<Mate>, DbError> {
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM mates ORDER BY pinned DESC, position, created_at DESC"
    ))?;
    let rows = statement.query_map([], mate_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get_mate(conn: &Connection, id: &str) -> Result<Option<Mate>, DbError> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM mates WHERE id = ?1"),
            [id],
            mate_row,
        )
        .optional()?)
}

/// The mate a chat belongs to (its direct chat or a seat).
pub fn mate_of_chat(conn: &Connection, chat: &str) -> Result<Option<Mate>, DbError> {
    let mate: Option<String> = conn
        .query_row("SELECT mate_id FROM chats WHERE id = ?1", [chat], |row| {
            row.get(0)
        })
        .optional()?
        .flatten();
    match mate {
        Some(mate) => get_mate(conn, &mate),
        None => Ok(None),
    }
}

/// Create a mate and its direct chat (kind `mate`), together. `app_mode` is
/// the app's approval mode, taken when the new mate names none.
pub fn create_mate(conn: &mut Connection, mate: &NewMate, app_mode: &str) -> Result<Mate, DbError> {
    let mode = mate.approval_mode.as_deref().unwrap_or(app_mode);
    check_mode(mode)?;
    if mate.name.trim().is_empty() {
        return Err(DbError::Invalid("a mate needs a name".into()));
    }
    let now = now_ms();
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    crate::chats::create_chat(
        &tx,
        &NewChat {
            id: mate.chat_id.clone(),
            title: mate.name.clone(),
            provider: mate.provider.clone(),
            model: mate.model.clone(),
            kind: Some("mate".into()),
            mate_id: Some(mate.id.clone()),
            ..NewChat::default()
        },
    )?;
    let position: i64 = tx.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM mates",
        [],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO mates (id, name, avatar, role, instructions, provider, model, thinking, tools, connectors, skills,
                            apps, approval_mode, folder, project_folder, chat_id, template, position, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?19)",
        params![
            mate.id,
            mate.name.trim(),
            mate.avatar.to_string(),
            mate.role,
            mate.instructions,
            mate.provider,
            mate.model,
            mate.thinking,
            to_json(&mate.tools),
            to_json(&mate.connectors),
            to_json(&mate.skills),
            to_json(&mate.apps),
            mode,
            mate.folder,
            mate.project_folder,
            mate.chat_id,
            mate.template,
            position,
            now,
        ],
    )?;
    tx.commit()?;
    get_mate(conn, &mate.id)?.ok_or_else(|| DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows))
}

/// Change a mate's fields. A new name also renames its direct chat.
pub fn update_mate(
    conn: &mut Connection,
    id: &str,
    patch: &MatePatch,
) -> Result<Option<Mate>, DbError> {
    if let Some(mode) = &patch.approval_mode {
        check_mode(mode)?;
    }
    if let Some(sending) = &patch.sending {
        if sending != "ask" && sending != "send" {
            return Err(DbError::Invalid(format!(
                "unknown sending setting \"{sending}\""
            )));
        }
    }
    if patch
        .name
        .as_deref()
        .is_some_and(|name| name.trim().is_empty())
    {
        return Err(DbError::Invalid("a mate needs a name".into()));
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let Some(chat): Option<String> = tx
        .query_row("SELECT chat_id FROM mates WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .optional()?
    else {
        return Ok(None);
    };
    let mut sets: Vec<String> = Vec::new();
    let mut values: Vec<rusqlite::types::Value> = Vec::new();
    let mut set = |column: &str, value: rusqlite::types::Value| {
        values.push(value);
        sets.push(format!("{column} = ?{}", values.len()));
    };
    use rusqlite::types::Value as Sql;
    let text = |value: &Option<String>| value.clone().map_or(Sql::Null, Sql::Text);
    if let Some(name) = &patch.name {
        set("name", Sql::Text(name.trim().to_string()));
    }
    if let Some(avatar) = &patch.avatar {
        set("avatar", Sql::Text(avatar.to_string()));
    }
    if let Some(role) = &patch.role {
        set("role", Sql::Text(role.clone()));
    }
    if let Some(instructions) = &patch.instructions {
        set("instructions", Sql::Text(instructions.clone()));
    }
    if let Some(provider) = &patch.provider {
        set("provider", text(provider));
    }
    if let Some(model) = &patch.model {
        set("model", text(model));
    }
    if let Some(thinking) = &patch.thinking {
        set("thinking", text(thinking));
    }
    if let Some(tools) = &patch.tools {
        set("tools", text(&to_json(tools)));
    }
    if let Some(connectors) = &patch.connectors {
        set("connectors", text(&to_json(connectors)));
    }
    if let Some(skills) = &patch.skills {
        set("skills", text(&to_json(skills)));
    }
    if let Some(apps) = &patch.apps {
        set("apps", text(&to_json(apps)));
    }
    if let Some(mode) = &patch.approval_mode {
        set("approval_mode", Sql::Text(mode.clone()));
    }
    if let Some(sending) = &patch.sending {
        set("sending", Sql::Text(sending.clone()));
    }
    if let Some(on) = patch.notifications {
        set("notifications", Sql::Integer(on.into()));
    }
    if let Some(on) = patch.paused {
        set("paused", Sql::Integer(on.into()));
    }
    if let Some(folder) = &patch.project_folder {
        set("project_folder", text(folder));
    }
    if let Some(on) = patch.pinned {
        set("pinned", Sql::Integer(on.into()));
    }
    if let Some(on) = patch.hidden {
        set("hidden", Sql::Integer(on.into()));
    }
    set("updated_at", Sql::Integer(now_ms()));
    values.push(Sql::Text(id.to_string()));
    tx.execute(
        &format!(
            "UPDATE mates SET {} WHERE id = ?{}",
            sets.join(", "),
            values.len()
        ),
        rusqlite::params_from_iter(values),
    )?;
    if let Some(name) = &patch.name {
        tx.execute(
            "UPDATE chats SET title = ?1, title_source = 'user' WHERE id = ?2",
            params![name.trim(), chat],
        )?;
    }
    tx.commit()?;
    get_mate(conn, id)
}

/// Delete a mate with its direct chat and its channel seats. Returns the
/// chats that went (to stop their engines and remove their transcripts);
/// channels it was in lose it as a member (and as lead).
pub fn delete_mate(conn: &mut Connection, id: &str) -> Result<Vec<String>, DbError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let chats: Vec<String> = {
        let mut statement = tx.prepare("SELECT id FROM chats WHERE mate_id = ?1")?;
        let rows = statement.query_map([id], |row| row.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    // The mate row first: it references its direct chat.
    tx.execute("DELETE FROM mates WHERE id = ?1", [id])?;
    for chat in &chats {
        tx.execute("DELETE FROM chats WHERE id = ?1", [chat])?;
    }
    tx.commit()?;
    Ok(chats)
}

/// A mate's Always-allow rules (FR-41).
pub fn list_grants(conn: &Connection, mate: &str) -> Result<Vec<String>, DbError> {
    let mut statement =
        conn.prepare("SELECT tool FROM mate_grants WHERE mate_id = ?1 ORDER BY tool")?;
    let rows = statement.query_map([mate], |row| row.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn add_grant(conn: &Connection, mate: &str, tool: &str) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO mate_grants (mate_id, tool, created_at) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
        params![mate, tool, now_ms()],
    )?;
    Ok(())
}

pub fn remove_grant(conn: &Connection, mate: &str, tool: &str) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM mate_grants WHERE mate_id = ?1 AND tool = ?2",
        params![mate, tool],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::open_at(&dir.path().join("evo.db")).unwrap();
        (dir, conn)
    }

    fn writer(id: &str) -> NewMate {
        NewMate {
            id: id.into(),
            chat_id: format!("chat-{id}"),
            name: "Writer".into(),
            avatar: json!({"definition": "pebble", "hue": 210}),
            role: "Drafts copy".into(),
            folder: format!("/mates/{id}/work"),
            ..NewMate::default()
        }
    }

    #[test]
    fn a_mate_comes_with_its_direct_chat() {
        let (_dir, mut conn) = db();
        let mate = create_mate(&mut conn, &writer("m1"), "ask").unwrap();
        assert_eq!(mate.approval_mode, "ask");
        assert_eq!(mate.sending, "ask");
        assert!(mate.notifications && !mate.paused);
        let chat = crate::chats::get_chat(&conn, "chat-m1").unwrap().unwrap();
        assert_eq!(chat.kind, "mate");
        assert_eq!(chat.mate_id.as_deref(), Some("m1"));
        assert_eq!(chat.title, "Writer");
        assert_eq!(mate_of_chat(&conn, "chat-m1").unwrap().unwrap().id, "m1");
    }

    #[test]
    fn harness_lists_and_searches_only_its_own_chats() {
        let (dir, mut conn) = db();
        create_mate(&mut conn, &writer("m1"), "ask").unwrap();
        crate::chats::create_chat(
            &conn,
            &NewChat {
                id: "c1".into(),
                title: "Writer notes".into(),
                ..NewChat::default()
            },
        )
        .unwrap();
        let listed: Vec<String> = crate::chats::list_chats(&conn, false)
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(listed, vec!["c1".to_string()]);
        let found: Vec<String> = crate::chats::search_chats(&mut conn, dir.path(), "Writer", 10)
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(found, vec!["c1".to_string()]);
    }

    #[test]
    fn a_new_mate_takes_the_apps_approval_mode_unless_it_names_one() {
        let (_dir, mut conn) = db();
        assert_eq!(
            create_mate(&mut conn, &writer("m1"), "auto-edits")
                .unwrap()
                .approval_mode,
            "auto-edits"
        );
        let mut named = writer("m2");
        named.approval_mode = Some("ask".into());
        assert_eq!(
            create_mate(&mut conn, &named, "auto")
                .unwrap()
                .approval_mode,
            "ask"
        );
        let mut wrong = writer("m3");
        wrong.approval_mode = Some("full".into());
        assert!(create_mate(&mut conn, &wrong, "ask").is_err());
    }

    #[test]
    fn a_patch_changes_and_clears_fields_and_renames_the_chat() {
        let (_dir, mut conn) = db();
        let mut mate = writer("m1");
        mate.model = Some("some-model".into());
        create_mate(&mut conn, &mate, "ask").unwrap();
        let patch: MatePatch = serde_json::from_value(json!({
            "name": "Copy",
            "model": null,
            "apps": [{"service": "mail", "account": "work"}],
            "pinned": true
        }))
        .unwrap();
        let mate = update_mate(&mut conn, "m1", &patch).unwrap().unwrap();
        assert_eq!(mate.name, "Copy");
        assert_eq!(mate.model, None);
        assert_eq!(
            mate.apps,
            Some(vec![json!({"service": "mail", "account": "work"})])
        );
        assert!(mate.pinned);
        assert_eq!(mate.role, "Drafts copy", "untouched fields stay");
        assert_eq!(
            crate::chats::get_chat(&conn, "chat-m1")
                .unwrap()
                .unwrap()
                .title,
            "Copy"
        );
        assert!(update_mate(&mut conn, "nobody", &patch).unwrap().is_none());
    }

    #[test]
    fn deleting_a_mate_returns_its_chats_and_leaves_channels_without_it() {
        let (_dir, mut conn) = db();
        create_mate(&mut conn, &writer("m1"), "ask").unwrap();
        crate::chats::create_chat(
            &conn,
            &NewChat {
                id: "room".into(),
                title: "Room".into(),
                kind: Some("channel".into()),
                ..NewChat::default()
            },
        )
        .unwrap();
        crate::chats::create_chat(
            &conn,
            &NewChat {
                id: "seat".into(),
                title: "Writer in Room".into(),
                kind: Some("seat".into()),
                mate_id: Some("m1".into()),
                parent_chat_id: Some("room".into()),
                ..NewChat::default()
            },
        )
        .unwrap();
        conn.execute(
            "INSERT INTO channels (id, name, lead_mate_id, chat_id, created_at, updated_at) VALUES ('ch', 'Room', 'm1', 'room', 0, 0)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO channel_members (channel_id, mate_id, seat_chat_id) VALUES ('ch', 'm1', 'seat')", []).unwrap();
        add_grant(&conn, "m1", "web_search").unwrap();

        let mut gone = delete_mate(&mut conn, "m1").unwrap();
        gone.sort();
        assert_eq!(gone, vec!["chat-m1".to_string(), "seat".to_string()]);
        assert!(get_mate(&conn, "m1").unwrap().is_none());
        let members: i64 = conn
            .query_row("SELECT COUNT(*) FROM channel_members", [], |r| r.get(0))
            .unwrap();
        assert_eq!(members, 0);
        let lead: Option<String> = conn
            .query_row("SELECT lead_mate_id FROM channels", [], |r| r.get(0))
            .unwrap();
        assert_eq!(lead, None);
        assert!(list_grants(&conn, "m1").unwrap().is_empty());
        assert!(
            crate::chats::get_chat(&conn, "room").unwrap().is_some(),
            "the channel stays"
        );
    }

    #[test]
    fn grants_are_listed_and_removed() {
        let (_dir, mut conn) = db();
        create_mate(&mut conn, &writer("m1"), "ask").unwrap();
        add_grant(&conn, "m1", "web_search").unwrap();
        add_grant(&conn, "m1", "web_search").unwrap();
        add_grant(&conn, "m1", "read").unwrap();
        assert_eq!(
            list_grants(&conn, "m1").unwrap(),
            vec!["read".to_string(), "web_search".to_string()]
        );
        remove_grant(&conn, "m1", "read").unwrap();
        assert_eq!(
            list_grants(&conn, "m1").unwrap(),
            vec!["web_search".to_string()]
        );
    }

    #[test]
    fn usage_accepts_the_mates_source() {
        let (_dir, conn) = db();
        conn.execute(
            "INSERT INTO usage_events (at_ms, source, provider_id, model_id, plugin_id) VALUES (1, 'mates', 'p', 'm', NULL)",
            [],
        )
        .unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM usage_events WHERE source = 'mates'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }
}
