//! When the nightly steps are due: the tidy-up after 03:00 local, once a
//! day, never while its owner works; then the dream, when the owner has at
//! least two chats since its last one (docs/features/learning-layers/spec.md,
//! FR-6, FR-10; plan 3.5).
//!
//! The loop looks every minute. A computer that slept through 03:00 tidies
//! at the first look after it wakes: the day it last tidied is not today.

use std::time::Duration;

use chrono::Timelike;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::db::Db;

/// How often the loop looks.
pub const TICK: Duration = Duration::from_secs(60);
/// The nightly steps start after this local hour.
pub const NIGHT_HOUR: u32 = 3;
/// Failures in a row after which that night's batch is dropped (NFR-5).
pub const MAX_FAILURES: i64 = 3;

/// The tidy-up is due: after 03:00, not yet today, and the owner is idle.
pub fn tidy_due(tidied_on: Option<&str>, today: &str, hour: u32, working: bool) -> bool {
    hour >= NIGHT_HOUR && tidied_on != Some(today) && !working
}

/// The dream is due: after the owner's tidy-up today, not yet dreamed today,
/// with at least two chats since the last dream, and the owner is idle.
pub fn dream_due(tidied_on: Option<&str>, dreamed_on: Option<&str>, today: &str, chats: usize, working: bool) -> bool {
    tidied_on == Some(today) && dreamed_on != Some(today) && chats >= 2 && !working
}

/// The local day and hour now.
pub fn local_now() -> (String, u32) {
    let now = chrono::Local::now();
    (now.format("%Y-%m-%d").to_string(), now.hour())
}

/// The local day of a time in milliseconds.
pub fn local_day(ms: i64) -> Option<String> {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(ms).single().map(|at| at.format("%Y-%m-%d").to_string())
}

/// A memory owner the loop looks after, and whether it is working now.
#[derive(Debug, Clone, PartialEq)]
pub struct Watched {
    pub owner: String,
    pub working: bool,
}

/// Start the loop.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let _ = tokio::time::timeout(TICK, app.state::<super::jobs::Learning>().wake.notified()).await;
            look(&app).await;
        }
    });
}

/// One look: every owner whose nightly step is due gets it.
async fn look(app: &AppHandle) {
    let db = app.state::<Db>();
    let all = db.with(|conn| evo_store::settings::all(conn)).unwrap_or_default();
    let settings = super::settings_from(&all);
    let (today, hour) = local_now();
    for watched in super::owners::watched(app, &settings, &all) {
        let state = db.with(|conn| evo_store::learning::state(conn, &watched.owner)).unwrap_or_default();
        let state = evo_store::learning::State { owner: watched.owner.clone(), ..state };
        if tidy_due(state.tidied_on.as_deref(), &today, hour, watched.working) {
            let outcome = super::jobs::run_step(app, "tidy", &watched.owner, Vec::new()).await;
            settle(&db, &watched.owner, &today, outcome.is_ok(), |state| state.tidied_on = Some(today.clone()));
            continue;
        }
        let dreamed_on = state.dreamed_at_ms.and_then(local_day);
        if state.tidied_on.as_deref() == Some(today.as_str()) && dreamed_on.as_deref() != Some(today.as_str()) && !watched.working {
            let since = state.dreamed_at_ms.unwrap_or_else(|| evo_store::chats::now_ms() - super::FIRST_DREAM_LOOKBACK_MS);
            let chats = super::owners::chats_since(&db, &watched.owner, since, &settings);
            let key = (watched.owner.clone(), "dream".to_string(), today.clone());
            let learning = app.state::<super::jobs::Learning>();
            let checked = learning.checked.lock().unwrap_or_else(std::sync::PoisonError::into_inner).contains(&key);
            if !checked && dream_due(state.tidied_on.as_deref(), dreamed_on.as_deref(), &today, chats, watched.working) {
                let outcome = super::jobs::run_step(app, "dream", &watched.owner, Vec::new()).await;
                // Too few chats after all (the engine counts exactly): not again today.
                let skipped = outcome.as_ref().ok().and_then(|result| result.get("skipped")).and_then(Value::as_str).map(str::to_string);
                if skipped.is_some() {
                    learning.checked.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(key);
                    continue;
                }
                let now = evo_store::chats::now_ms();
                settle(&db, &watched.owner, &today, outcome.is_ok(), |state| state.dreamed_at_ms = Some(now));
            }
        }
    }
}

/// Store how a step went: done (or dropped after [`MAX_FAILURES`]) moves the
/// owner's state on; a failure is counted and the step is tried again at
/// the next look (NFR-5).
fn settle(db: &Db, owner: &str, _today: &str, ok: bool, done: impl FnOnce(&mut evo_store::learning::State)) {
    let _ = db.with(|conn| {
        let mut state = evo_store::learning::state(conn, owner)?;
        if ok || state.failures + 1 >= MAX_FAILURES {
            done(&mut state);
            state.failures = 0;
        } else {
            state.failures += 1;
        }
        evo_store::learning::put_state(conn, &state)
    });
}

/// The value of a JSON setting as a bool, for a mate's own switches.
pub fn flag(value: Option<&Value>, key: &str) -> Option<bool> {
    value?.get(key)?.as_bool()
}

#[cfg(test)]
mod tests {
    use super::*;

    // FR-6: once a night, after three.
    #[test]
    fn tidy_is_due_once_a_night_after_three() {
        assert!(!tidy_due(None, "2026-11-28", 2, false), "not before three");
        assert!(tidy_due(None, "2026-11-28", 3, false));
        assert!(tidy_due(Some("2026-11-27"), "2026-11-28", 14, false), "a computer that slept tidies when it wakes");
        assert!(!tidy_due(Some("2026-11-28"), "2026-11-28", 23, false), "once a day");
        assert!(tidy_due(Some("2026-11-28"), "2026-11-29", 3, false), "and again the next night");
    }

    // FR-6: never while the owner works.
    #[test]
    fn a_working_owner_is_not_tidied() {
        assert!(!tidy_due(None, "2026-11-28", 4, true));
        assert!(!dream_due(Some("2026-11-28"), None, "2026-11-28", 5, true));
    }

    // FR-10: the dream follows the tidy-up, with two chats or more.
    #[test]
    fn the_dream_runs_after_the_tidy_with_two_chats() {
        assert!(dream_due(Some("2026-11-28"), None, "2026-11-28", 2, false));
        assert!(dream_due(Some("2026-11-28"), Some("2026-11-27"), "2026-11-28", 3, false));
        assert!(!dream_due(Some("2026-11-28"), None, "2026-11-28", 1, false), "one chat is not enough");
        assert!(!dream_due(Some("2026-11-27"), None, "2026-11-28", 4, false), "not before tonight's tidy-up");
        assert!(!dream_due(Some("2026-11-28"), Some("2026-11-28"), "2026-11-28", 4, false), "once a night");
    }
}
