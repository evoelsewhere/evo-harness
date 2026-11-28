//! The daily spending limit per mate (docs/features/evo-mates/spec.md,
//! FR-80; plan 3.10). Engines write usage rows themselves, so the shell
//! checks after the fact, whenever the chats with work change: a mate whose
//! cost today reached the limit is paused (FR-45) with a notice. Resuming a
//! mate that is over the limit excuses it until the day ends.

use std::collections::HashSet;
use std::sync::Mutex;

use evo_store::logs::NewLog;
use evo_store::mates::{Mate, MatePatch};
use tauri::{AppHandle, Manager};

use super::today::MateDay;
use crate::db::Db;

/// The app setting: dollars a mate may spend a day; none or 0: no limit.
pub const LIMIT_KEY: &str = "mates.dailyLimitUsd";

/// Mates resumed over the limit, and the day they were.
static EXCUSED: Mutex<(String, Vec<String>)> = Mutex::new((String::new(), Vec::new()));

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn excused_today() -> HashSet<String> {
    let guard = EXCUSED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if guard.0 == today() {
        guard.1.iter().cloned().collect()
    } else {
        HashSet::new()
    }
}

/// The limit, when one is set.
pub fn limit(db: &Db) -> Option<f64> {
    db.with(|conn| evo_store::settings::get(conn, LIMIT_KEY))
        .ok()
        .flatten()
        .and_then(|v| v.as_f64())
        .filter(|v| *v > 0.0)
}

/// The mates to pause: at or over `limit` today, not paused, not excused.
pub fn over(
    mates: &[Mate],
    today: &[MateDay],
    limit: Option<f64>,
    excused: &HashSet<String>,
) -> Vec<String> {
    let Some(limit) = limit else {
        return Vec::new();
    };
    mates
        .iter()
        .filter(|mate| !mate.paused && !excused.contains(&mate.id))
        .filter(|mate| {
            today
                .iter()
                .any(|day| day.mate_id == mate.id && day.cost_usd >= limit)
        })
        .map(|mate| mate.id.clone())
        .collect()
}

/// "$2.00": the limit as the line and the notice show it.
pub fn money(value: f64) -> String {
    format!("${value:.2}")
}

/// Pause the mates over the limit today, each with a notice.
pub fn check(app: &AppHandle) {
    let db = app.state::<Db>();
    let Some(limit) = limit(&db) else {
        return;
    };
    let Ok(mates) = db.with(|conn| evo_store::mates::list_mates(conn)) else {
        return;
    };
    let (start, _) = super::today::window(&chrono::Local::now());
    let Ok(days) = super::today::mates_since(&db, start) else {
        return;
    };
    let ids = over(&mates, &days, Some(limit), &excused_today());
    for mate in mates.iter().filter(|m| ids.contains(&m.id)) {
        let patch = MatePatch {
            paused: Some(true),
            ..MatePatch::default()
        };
        if db
            .with(|conn| evo_store::mates::update_mate(conn, &mate.id, &patch))
            .is_err()
        {
            continue;
        }
        db.log(
            NewLog::new(
                "info",
                "mates",
                format!(
                    "{} paused: reached today's limit of {}",
                    mate.name,
                    money(limit)
                ),
            )
            .chat(Some(&mate.chat_id)),
        );
        if mate.notifications {
            let at = crate::supervisor::now_ms();
            let entry = crate::supervisor::notices::BellEntry {
                id: format!("spend-{at:x}-{}", mate.id),
                title: format!("Paused: reached today's limit of {}", money(limit)),
                body: mate.name.clone(),
                level: "warning".into(),
                source: "agent".into(),
                chat: mate.chat_id.clone(),
                key: format!("spend:{}", mate.chat_id),
                at,
            };
            crate::supervisor::notices::deliver(app, entry, Some(mate.chat_id.clone()));
        }
    }
    if !ids.is_empty() {
        super::changed(app);
    }
}

/// How long after the chats with work change the limit is checked: a turn's
/// last usage row is written by its engine after the run settles.
const SETTLE: std::time::Duration = std::time::Duration::from_secs(3);

/// [`check`] once the engines have written the usage of the run that settled.
pub fn check_soon(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SETTLE).await;
        check(&app);
    });
}

/// The person resumed `mate`: when it is over the limit today, let it go on until the day ends.
pub fn resumed(app: &AppHandle, mate: &str) {
    let db = app.state::<Db>();
    let Some(limit) = limit(&db) else {
        return;
    };
    let (start, _) = super::today::window(&chrono::Local::now());
    let over = super::today::mates_since(&db, start)
        .unwrap_or_default()
        .iter()
        .any(|day| day.mate_id == mate && day.cost_usd >= limit);
    if over {
        let mut guard = EXCUSED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let day = today();
        if guard.0 != day {
            *guard = (day, Vec::new());
        }
        guard.1.push(mate.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mate(id: &str, paused: bool) -> Mate {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": id, "avatar": {}, "role": "", "instructions": "",
            "provider": null, "model": null, "thinking": null, "tools": null, "connectors": null,
            "skills": null, "apps": null, "approvalMode": "ask", "sending": "ask",
            "notifications": true, "initiative": "medium", "unanswered": 0, "paused": paused,
            "folder": "/m", "projectFolder": null, "chatId": format!("chat-{id}"), "template": null,
            "pinned": false, "hidden": false, "position": 0, "createdAt": 0, "updatedAt": 0
        }))
        .unwrap()
    }

    fn day(id: &str, cost: f64) -> MateDay {
        MateDay {
            mate_id: id.into(),
            replies: 3,
            cost_usd: cost,
        }
    }

    #[test]
    fn mates_at_or_over_the_limit_are_paused_unless_excused_or_paused_already() {
        let mates = [
            mate("a", false),
            mate("b", false),
            mate("c", true),
            mate("d", false),
            mate("e", false),
        ];
        let days = [
            day("a", 0.6),
            day("b", 0.2),
            day("c", 9.0),
            day("d", 0.5),
            day("e", 1.0),
        ];
        let excused: HashSet<String> = ["e".to_string()].into();
        assert_eq!(over(&mates, &days, Some(0.5), &excused), ["a", "d"]);
        assert!(
            over(&mates, &days, None, &HashSet::new()).is_empty(),
            "no limit, nothing paused"
        );
        assert_eq!(money(2.0), "$2.00");
    }
}
