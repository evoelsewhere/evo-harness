//! Mates that speak first (docs/features/evo-mates/spec.md, US-10, FR-63 –
//! FR-71; plan 3.9): a heartbeat wakes each mate that has a due agenda item
//! or check-in, and the mate either writes one message or stays silent.
//!
//! The shell keeps time and starts the turn, as its Scheduler does; the mate
//! decides what to say. A heartbeat turn is an ordinary prompt to the mate's
//! direct chat through the hub, marked `<evo-mates-heartbeat>` so the thread
//! hides it; a reply of [`SILENT`] is hidden too and is no first message.
//! While it runs the turn is read-only (FR-68): the mate's approvals read as
//! `ask` and every sending tool asks.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use evo_store::agenda::{self, Item};
use evo_store::logs::NewLog;
use evo_store::mates::Mate;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::db::Db;

/// What a mate answers when nothing is worth saying.
pub const SILENT: &str = "NOTHING_TO_SAY";
/// The mark that starts a heartbeat prompt.
pub const MARK: &str = "<evo-mates-heartbeat";
/// The app setting for the heartbeat's period, in minutes.
pub const PERIOD_KEY: &str = "mates.heartbeatMinutes";
const PERIOD_MINUTES: u64 = 60;

/// Why a mate is not woken now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    Paused,
    Off,
    DoNotDisturb,
    /// Its first messages for today are spent.
    Budget,
    /// Too many first messages in a row went unanswered.
    Unanswered,
    /// It is working (or already in a heartbeat turn).
    Busy,
    NothingDue,
}

/// Whether to wake `mate` with `due` items now (FR-64, FR-65, FR-67, FR-71).
pub fn decide(mate: &Mate, due: &[Item], sent_today: i64, dnd: bool, busy: bool) -> Result<(), Skip> {
    if mate.paused {
        return Err(Skip::Paused);
    }
    let (per_day, back_off) = agenda::budget(&mate.initiative);
    if per_day == 0 {
        return Err(Skip::Off);
    }
    if due.is_empty() {
        return Err(Skip::NothingDue);
    }
    if dnd {
        return Err(Skip::DoNotDisturb);
    }
    if sent_today >= per_day {
        return Err(Skip::Budget);
    }
    // A check-in the person asked for goes out even after silence.
    let asked_for = due.iter().any(|item| item.kind == "checkin");
    if mate.unanswered >= back_off && !asked_for {
        return Err(Skip::Unanswered);
    }
    if busy {
        return Err(Skip::Busy);
    }
    Ok(())
}

/// The heartbeat prompt for `due` items, at local time `now`.
pub fn prompt(due: &[Item], now: &str) -> String {
    let ids: Vec<&str> = due.iter().map(|i| i.id.as_str()).collect();
    let mut out = format!("{MARK} at=\"{now}\" items=\"{}\">\n", ids.join(","));
    out.push_str("Due now from your agenda:\n");
    for item in due {
        let label = match item.kind.as_str() {
            "promise" => "you promised",
            "event" => "the person mentioned",
            "question" => "left open",
            _ => "check-in the person asked for",
        };
        out.push_str(&format!("- {} ({label}): {}\n", item.id, item.text.trim()));
    }
    out.push_str(&format!(
        "Write to the person only if something is worth saying now: one short message that says first why you are writing. \
If not, answer exactly {SILENT} and nothing else. This turn is read-only: read, search and draft as you need, but change, \
send or post nothing without asking. Mark settled items done with `mate_agenda`.\n</evo-mates-heartbeat>"
    ));
    out
}

/// Whether a run's final text says nothing is worth saying.
pub fn is_silent(text: &str) -> bool {
    text.trim().trim_matches(|c: char| c == '.' || c == '`' || c == '*').eq_ignore_ascii_case(SILENT)
}

/// Chats in a heartbeat turn now, with the items it took up.
fn in_flight() -> &'static Mutex<HashMap<String, Vec<String>>> {
    static IN_FLIGHT: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(Default::default)
}

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether `chat` runs a heartbeat turn now (its approvals read as `ask`, FR-68).
pub fn in_heartbeat(chat: &str) -> bool {
    locked(in_flight()).contains_key(chat)
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn local_now() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M").to_string()
}

/// Start the heartbeat: a first tick a minute after start, then every period.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        loop {
            tick(&app).await;
            let minutes = app
                .state::<Db>()
                .with(|conn| evo_store::settings::get(conn, PERIOD_KEY))
                .ok()
                .flatten()
                .and_then(|v| v.as_u64())
                .filter(|m| (5..=24 * 60).contains(m))
                .unwrap_or(PERIOD_MINUTES);
            tokio::time::sleep(Duration::from_secs(minutes * 60)).await;
        }
    });
}

/// One heartbeat: wake every mate that should speak now.
pub async fn tick(app: &AppHandle) {
    let db = app.state::<Db>();
    let now = evo_store::chats::now_ms();
    let Ok(mates) = db.with(|conn| agenda::mates_with_due(conn, now)) else {
        return;
    };
    let dnd = crate::agent_tools::user::muted();
    for id in mates {
        let Ok(Some(mate)) = db.with(|conn| evo_store::mates::get_mate(conn, &id)) else {
            continue;
        };
        let due = db.with(|conn| agenda::due_items(conn, &mate.id, now)).unwrap_or_default();
        let sent = db.with(|conn| agenda::sent_on(conn, &mate.id, &today())).unwrap_or(0);
        let host = crate::supervisor::AppHost(app.clone());
        let busy = in_heartbeat(&mate.chat_id) || app.state::<crate::supervisor::EngineSupervisor>().working(&host, &mate.chat_id);
        match decide(&mate, &due, sent, dnd, busy) {
            Ok(()) => wake(app, &mate, due).await,
            Err(skip) => db.log(NewLog::new("debug", "mates", format!("heartbeat skipped {}: {skip:?}", mate.id)).chat(Some(&mate.chat_id))),
        }
    }
}

/// Post the heartbeat prompt to the mate's chat.
async fn wake(app: &AppHandle, mate: &Mate, due: Vec<Item>) {
    let text = prompt(&due, &local_now());
    locked(in_flight()).insert(mate.chat_id.clone(), due.iter().map(|i| i.id.clone()).collect());
    app.state::<Db>().log(NewLog::new("info", "mates", format!("heartbeat woke {} for {} item(s)", mate.id, due.len())).chat(Some(&mate.chat_id)));
    let (hub_app, chat) = (app.clone(), mate.chat_id.clone());
    let effects = tauri::async_runtime::spawn_blocking(move || {
        hub_app.state::<crate::mesh::MeshHub>().0.post(&chat, &chat, evo_mesh::wire::MailBody::new(text, evo_mesh::wire::Deliver::Prompt))
    })
    .await
    .unwrap_or_default();
    crate::mesh::apply(app, effects).await;
}

/// A run of `chat` settled (`agent_settled`): when it was a heartbeat turn,
/// its items are taken up and, unless it was silent, it counts as a first
/// message. True when the turn was silent (no notice, not unread).
pub fn settled(app: &AppHandle, chat: &str, line: &Value) -> bool {
    let Some(items) = locked(in_flight()).remove(chat) else {
        return false;
    };
    let db = app.state::<Db>();
    let Ok(Some(mate)) = db.with(|conn| evo_store::mates::mate_of_chat(conn, chat)) else {
        return false;
    };
    let text = line.get("finalText").and_then(Value::as_str).unwrap_or_default();
    let completed = line.get("outcome").and_then(Value::as_str) == Some("completed");
    let silent = completed && is_silent(text);
    let now = evo_store::chats::now_ms();
    for id in &items {
        let Ok(Some(item)) = db.with(|conn| agenda::get_item(conn, id)) else { continue };
        let next = item.repeat.as_deref().and_then(|repeat| {
            let schedule = crate::agent_tools::scheduler::schedule::schedule_from(&serde_json::from_str(repeat).ok()?).ok()?;
            crate::agent_tools::scheduler::schedule::next_run(&schedule, now)
        });
        let _ = db.with(|conn| agenda::fired(conn, id, now, next));
    }
    if completed && !silent {
        let _ = db.with(|conn| agenda::count_first_message(conn, &mate.id, &today()));
    }
    db.log(
        NewLog::new("info", "mates", if silent { "heartbeat: nothing to say".to_string() } else { "heartbeat: wrote first".to_string() })
            .chat(Some(chat)),
    );
    super::changed(app);
    silent
}

/// The person wrote in `chat`: its mate may write first again (FR-65).
pub fn person_wrote(app: &AppHandle, chat: &str) {
    let db = app.state::<Db>();
    if let Ok(Some(mate)) = db.with(|conn| evo_store::mates::mate_of_chat(conn, chat)) {
        if mate.unanswered > 0 {
            let _ = db.with(|conn| agenda::answered(conn, &mate.id));
        }
    }
}

/// A mate's Following up list: open and paused items, soonest first (FR-70).
#[tauri::command]
pub fn mates_agenda(app: AppHandle, id: String) -> Result<Vec<Item>, String> {
    app.state::<Db>().with(|conn| agenda::list_items(conn, &id, false))
}

/// Set an item's status: `done` (remove), `never` (never follow up on it),
/// `paused` or `open` (FR-70).
#[tauri::command]
pub fn mates_agenda_set(app: AppHandle, id: String, item: String, status: String) -> Result<(), String> {
    app.state::<Db>().with(|conn| agenda::set_status(conn, &id, &item, &status))?;
    super::changed(&app);
    Ok(())
}

#[tauri::command]
pub fn mates_agenda_delete(app: AppHandle, id: String, item: String) -> Result<(), String> {
    app.state::<Db>().with(|conn| agenda::delete_item(conn, &id, &item))?;
    super::changed(&app);
    Ok(())
}

/// A check-in's schedule in words ("Weekdays at 09:00"), for the panel.
#[tauri::command]
pub fn mates_agenda_repeat_words(repeat: String) -> Option<String> {
    let value: Value = serde_json::from_str(&repeat).ok()?;
    let schedule = crate::agent_tools::scheduler::schedule::schedule_from(&value).ok()?;
    Some(crate::agent_tools::scheduler::schedule::describe_schedule(&schedule))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mate(initiative: &str, unanswered: i64, paused: bool) -> Mate {
        serde_json::from_value(serde_json::json!({
            "id": "m", "name": "Ada", "avatar": {}, "role": "", "instructions": "",
            "provider": null, "model": null, "thinking": null, "tools": null, "connectors": null,
            "skills": null, "apps": null, "approvalMode": "auto", "sending": "send",
            "notifications": true, "initiative": initiative, "unanswered": unanswered, "paused": paused,
            "folder": "/m", "projectFolder": null, "chatId": "c", "template": null,
            "pinned": false, "hidden": false, "position": 0, "createdAt": 0, "updatedAt": 0
        }))
        .unwrap()
    }

    fn item(kind: &str) -> Item {
        Item {
            id: format!("i-{kind}"),
            mate_id: "m".into(),
            kind: kind.into(),
            text: "Interview at 10".into(),
            due_at: 0,
            repeat: None,
            status: "open".into(),
            chat_id: None,
            created_at: 0,
            fired_at: None,
        }
    }

    #[test]
    fn who_is_woken() {
        let due = vec![item("event")];
        assert_eq!(decide(&mate("medium", 0, false), &due, 0, false, false), Ok(()));
        assert_eq!(decide(&mate("medium", 0, true), &due, 0, false, false), Err(Skip::Paused));
        assert_eq!(decide(&mate("off", 0, false), &due, 0, false, false), Err(Skip::Off));
        assert_eq!(decide(&mate("medium", 0, false), &[], 0, false, false), Err(Skip::NothingDue));
        assert_eq!(decide(&mate("medium", 0, false), &due, 0, true, false), Err(Skip::DoNotDisturb));
        assert_eq!(decide(&mate("medium", 0, false), &due, 5, false, false), Err(Skip::Budget));
        assert_eq!(decide(&mate("low", 0, false), &due, 2, false, false), Err(Skip::Budget));
        assert_eq!(decide(&mate("low", 0, false), &due, 1, false, false), Ok(()));
        assert_eq!(decide(&mate("medium", 2, false), &due, 0, false, false), Err(Skip::Unanswered));
        assert_eq!(decide(&mate("medium", 1, false), &due, 0, false, false), Ok(()));
        assert_eq!(decide(&mate("medium", 0, false), &due, 0, false, true), Err(Skip::Busy));
        // A check-in the person asked for goes out after silence too.
        assert_eq!(decide(&mate("medium", 2, false), &[item("checkin")], 0, false, false), Ok(()));
    }

    #[test]
    fn the_prompt_lists_due_items_and_the_rules() {
        let text = prompt(&[item("event"), item("promise")], "2026-11-26T11:00");
        assert!(text.starts_with("<evo-mates-heartbeat at=\"2026-11-26T11:00\" items=\"i-event,i-promise\">\n"), "{text}");
        assert!(text.contains("- i-event (the person mentioned): Interview at 10\n"));
        assert!(text.contains("- i-promise (you promised): Interview at 10\n"));
        assert!(text.contains(&format!("answer exactly {SILENT}")));
        assert!(text.contains("read-only"));
        assert!(text.ends_with("</evo-mates-heartbeat>"));
    }

    #[test]
    fn silence_is_read_loosely() {
        assert!(is_silent("NOTHING_TO_SAY"));
        assert!(is_silent(" `nothing_to_say`. "));
        assert!(!is_silent("How did the interview go?"));
    }
}
