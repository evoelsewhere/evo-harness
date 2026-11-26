//! Evo Mates' channels in the shell (docs/features/evo-mates/spec.md, FR-14 –
//! FR-19; plan 3.3): the `channels_*` commands, and the role a channel's root
//! chat runs as. The root's engine coordinates the channel
//! (`evo_engine::channel::coordinator`); its role carries the channel's rule,
//! lead, instructions and members, each with the role its seat runs as (its
//! mate's). The shell writes roles and stops engines; it decides nothing.

use evo_store::channels::{self, Channel, ChannelPatch, NewChannel};
use evo_store::logs::NewLog;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::db::Db;

/// The app setting for hand-offs per message of the person (FR-18).
pub const HANDOFF_LIMIT_KEY: &str = "mates.handoffLimit";
const HANDOFF_LIMIT: u64 = 6;

fn handoff_limit(db: &Db) -> u64 {
    db.with(|conn| evo_store::settings::get(conn, HANDOFF_LIMIT_KEY))
        .ok()
        .flatten()
        .and_then(|value| value.as_u64())
        .filter(|limit| (1..=50).contains(limit))
        .unwrap_or(HANDOFF_LIMIT)
}

/// The model the person chose for routing under Auto (FR-28), saved as
/// `{provider, model}`; `null` without a choice (the engine takes the
/// cheapest connected fit model).
pub const ROUTER_MODEL_KEY: &str = "mates.routerModel";

fn router_model(db: &Db) -> Value {
    db.with(|conn| evo_store::settings::get(conn, ROUTER_MODEL_KEY))
        .ok()
        .flatten()
        .filter(|value| value.get("provider").and_then(Value::as_str).is_some() && value.get("model").and_then(Value::as_str).is_some())
        .map(|value| json!({ "provider": value["provider"], "model": value["model"] }))
        .unwrap_or(Value::Null)
}

/// The role a channel's root runs as.
pub fn role_of(db: &Db, channel: &Channel) -> Value {
    let members: Vec<Value> = channel
        .members
        .iter()
        .filter_map(|member| {
            let mate = db.with(|conn| evo_store::mates::get_mate(conn, &member.mate_id)).ok().flatten()?;
            Some(json!({
                "mateId": mate.id,
                "name": mate.name,
                "role": mate.role,
                "seatChat": member.seat_chat_id,
                "seatRole": seat_role_of(&mate, &channel.name),
            }))
        })
        .collect();
    json!({
        "name": channel.name,
        "source": "channel",
        "access": "full",
        "channel": {
            "channelId": channel.id,
            "name": channel.name,
            "rule": channel.reply_rule,
            "lead": channel.lead_mate_id,
            "members": members,
            "instructions": channel.instructions,
            "handoffLimit": handoff_limit(db),
            "router": router_model(db),
        },
    })
}

/// The role a mate's seat in the channel `channel` runs as: its mate's, naming
/// the channel, so it asks in its reply rather than with a card (FR-72).
pub fn seat_role_of(mate: &evo_store::mates::Mate, channel: &str) -> Value {
    let mut role = super::role_of(mate);
    role["mate"]["channel"] = json!(channel);
    role
}

/// `role` (a channel root's) with `seat` as the seat of member `mate`.
pub fn with_seat(role: &Value, mate: &str, seat: &str) -> Value {
    let mut role = role.clone();
    if let Some(members) = role.pointer_mut("/channel/members").and_then(Value::as_array_mut) {
        for member in members.iter_mut().filter(|m| m["mateId"] == mate) {
            member["seatChat"] = json!(seat);
        }
    }
    role
}

/// Write the channel's role into its root chat, and restart the root's
/// engine once idle so it reads it; a seat whose role changed (its mate, the
/// channel's name) restarts too.
async fn write_role(app: &AppHandle, channel: &Channel) -> Result<(), String> {
    let db = app.state::<Db>();
    let role = role_of(&db, channel);
    db.with(|conn| evo_store::chats::set_role(conn, &channel.chat_id, Some(&role)).map(|_| ()))?;
    super::restart_when_idle(app, &channel.chat_id).await;
    for member in &channel.members {
        let Some(seat) = &member.seat_chat_id else { continue };
        let Some(mate) = db.with(|conn| evo_store::mates::get_mate(conn, &member.mate_id)).ok().flatten() else { continue };
        let role = seat_role_of(&mate, &channel.name);
        let current = db.with(|conn| evo_store::chats::get_chat(conn, seat)).ok().flatten().and_then(|chat| chat.role);
        if current.as_ref() != Some(&role) && db.with(|conn| evo_store::chats::set_role(conn, seat, Some(&role)).map(|_| ())).is_ok() {
            super::restart_when_idle(app, seat).await;
        }
    }
    Ok(())
}

/// Settings › Evo Mates changed: every channel reads its role again, so the
/// hand-off limit and the router apply without a restart of the app.
#[tauri::command]
pub async fn channels_reload(app: AppHandle) -> Result<(), String> {
    let db = app.state::<Db>();
    let list = db.with(|conn| channels::list_channels(conn))?;
    for channel in list {
        write_role(&app, &channel).await?;
    }
    Ok(())
}

/// A mate changed: its seats and every channel it is in read it again.
pub async fn mate_changed(app: &AppHandle, mate: &evo_store::mates::Mate) {
    let db = app.state::<Db>();
    let Ok(list) = db.with(|conn| channels::list_channels(conn)) else {
        return;
    };
    for channel in list.into_iter().filter(|c| c.members.iter().any(|m| m.mate_id == mate.id)) {
        let _ = write_role(app, &channel).await;
    }
}

fn changed(app: &AppHandle) {
    super::changed(app);
}

#[tauri::command]
pub fn channels_list(app: AppHandle) -> Result<Vec<Channel>, String> {
    app.state::<Db>().with(|conn| channels::list_channels(conn))
}

/// What the Mates window asks to create.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannel {
    pub name: String,
    #[serde(default)]
    pub instructions: String,
    pub reply_rule: Option<String>,
    pub lead_mate_id: Option<String>,
    pub folder: Option<String>,
    #[serde(default)]
    pub members: Vec<String>,
}

#[tauri::command]
pub async fn channels_create(app: AppHandle, input: CreateChannel) -> Result<Channel, String> {
    if input.members.is_empty() {
        return Err("A channel needs at least one mate.".into());
    }
    let db = app.state::<Db>();
    let new = NewChannel {
        id: super::new_id("channel"),
        chat_id: super::new_id("channelchat"),
        name: input.name,
        instructions: input.instructions,
        reply_rule: input.reply_rule,
        lead_mate_id: input.lead_mate_id,
        folder: input.folder.filter(|f| !f.trim().is_empty()),
        members: input.members,
    };
    let channel = db.with(|conn| channels::create_channel(conn, &new))?;
    write_role(&app, &channel).await?;
    db.log(NewLog::new("info", "mates", format!("created channel {}", channel.id)));
    crate::supervisor::chat_rows_changed(&app);
    changed(&app);
    Ok(channel)
}

#[tauri::command]
pub async fn channels_update(app: AppHandle, id: String, patch: ChannelPatch) -> Result<Option<Channel>, String> {
    let db = app.state::<Db>();
    let Some(channel) = db.with(|conn| channels::update_channel(conn, &id, &patch))? else {
        return Ok(None);
    };
    let only_looks = patch.pinned.is_some() || patch.hidden.is_some();
    let reads = patch.name.is_some()
        || patch.instructions.is_some()
        || patch.reply_rule.is_some()
        || patch.lead_mate_id.is_some()
        || patch.folder.is_some()
        || patch.members.is_some();
    if reads || !only_looks {
        write_role(&app, &channel).await?;
    }
    // Seats work in the channel's folder: they start again there (FR-25),
    // and the folder's project trust applies as in Harness (FR-23).
    if patch.folder.is_some() {
        for seat in channel.members.iter().filter_map(|m| m.seat_chat_id.clone()) {
            super::restart_when_idle(&app, &seat).await;
        }
    }
    changed(&app);
    Ok(Some(channel))
}

/// Delete a channel: its root's and seats' engines stop, their chats and
/// transcripts go. The mates stay.
#[tauri::command]
pub async fn channels_delete(app: AppHandle, id: String) -> Result<(), String> {
    let db = app.state::<Db>();
    let chats = db.with(|conn| channels::delete_channel(conn, &id))?;
    let engines = app.state::<crate::engine::EngineState>();
    for chat in &chats {
        engines.stop(chat).await;
        db.remove_transcript(chat);
    }
    db.log(NewLog::new("info", "mates", format!("deleted channel {id}")));
    crate::supervisor::chat_rows_changed(&app);
    changed(&app);
    Ok(())
}

/// Stop one member's work in a channel (FR-19): its seat's engine.
#[tauri::command]
pub async fn channels_stop_member(app: AppHandle, id: String, mate: String) -> Result<(), String> {
    let seat = app
        .state::<Db>()
        .with(|conn| channels::get_channel(conn, &id))?
        .and_then(|c| c.members.into_iter().find(|m| m.mate_id == mate))
        .and_then(|m| m.seat_chat_id);
    if let Some(seat) = seat {
        super::park(&app, &seat).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_channel_role_reads_as_a_session_role() {
        let role = serde_json::json!({
            "name": "Room",
            "source": "channel",
            "access": "full",
            "channel": {
                "channelId": "ch", "name": "Room", "rule": "auto", "lead": "m1",
                "members": [{ "mateId": "m1", "name": "Ada", "role": "plans", "seatChat": null,
                              "seatRole": { "name": "Ada", "mate": { "id": "m1", "name": "Ada" } } }],
                "instructions": "", "handoffLimit": 6
            }
        });
        let parsed: evo_mesh::wire::SessionRole = serde_json::from_value(role).unwrap();
        let channel = parsed.channel.unwrap();
        assert_eq!(channel.members[0].seat_role.as_ref().unwrap().mate.as_ref().unwrap().id, "m1");
        assert_eq!(channel.handoff_limit, 6);
        assert_eq!(channel.lead.as_deref(), Some("m1"));
    }

    // FR-14: a seat started while the channel runs is kept in the channel's
    // role, so the channel reuses it after a restart instead of starting
    // another (with none of its conversation).
    #[test]
    fn a_new_seat_is_kept_in_the_channels_role() {
        let role = serde_json::json!({ "channel": { "members": [
            { "mateId": "m1", "seatChat": "s1" },
            { "mateId": "m2", "seatChat": null },
        ] } });
        let role = super::with_seat(&role, "m2", "s2");
        assert_eq!(role["channel"]["members"][1]["seatChat"], "s2");
        assert_eq!(role["channel"]["members"][0]["seatChat"], "s1");
        assert_eq!(super::with_seat(&role, "ghost", "s9"), role, "not a member: unchanged");
    }

    // FR-72: a seat's role names its channel, so its engine asks in its reply.
    #[test]
    fn a_seat_role_names_its_channel() {
        let mate: evo_store::mates::Mate = serde_json::from_value(serde_json::json!({
            "id": "m", "name": "Ada", "avatar": {}, "role": "", "instructions": "",
            "provider": null, "model": null, "thinking": null, "tools": null, "connectors": null,
            "skills": null, "apps": null, "approvalMode": "auto", "sending": "send",
            "notifications": true, "initiative": "off", "unanswered": 0, "paused": false,
            "folder": "/m", "projectFolder": null, "chatId": "c", "template": null,
            "pinned": false, "hidden": false, "position": 0, "createdAt": 0, "updatedAt": 0
        }))
        .unwrap();
        let seat: evo_mesh::wire::SessionRole = serde_json::from_value(super::seat_role_of(&mate, "BDS")).unwrap();
        assert_eq!(seat.mate.unwrap().channel.as_deref(), Some("BDS"));
        let direct: evo_mesh::wire::SessionRole = serde_json::from_value(crate::mates::role_of(&mate)).unwrap();
        assert_eq!(direct.mate.unwrap().channel, None);
    }
}
