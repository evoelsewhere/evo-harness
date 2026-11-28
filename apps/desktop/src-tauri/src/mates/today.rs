//! Evo Mates' Today page (docs/features/evo-mates/spec.md, FR-73; plan
//! 3.10): what is coming up across every mate and what each mate did today.
//! Needs you and New come from the windows' own chat state; this reads Evo's
//! database only and makes no model call.

use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::db::Db;

/// One mate's work since the start of today.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MateDay {
    pub mate_id: String,
    /// Model answers in its conversation and its seats.
    pub replies: i64,
    pub cost_usd: f64,
}

/// The start of `now`'s day and the end of the next day, in ms.
pub fn window<Tz: TimeZone>(now: &DateTime<Tz>) -> (i64, i64) {
    let midnight = |date: chrono::NaiveDate| {
        now.timezone()
            .from_local_datetime(&date.and_time(NaiveTime::MIN))
            .earliest()
            .map(|at| at.timestamp_millis())
            .unwrap_or_else(|| now.timestamp_millis())
    };
    let today = now.date_naive();
    (midnight(today), midnight(today + Duration::days(2)))
}

/// Each mate's replies and cost in a usage summary.
pub fn mates_of(summary: evo_store::usage::UsageSummary) -> Vec<MateDay> {
    summary
        .by_mate
        .into_iter()
        .map(|m| MateDay {
            mate_id: m.mate_id,
            replies: m.totals.requests,
            cost_usd: m.totals.cost_usd,
        })
        .collect()
}

/// Each mate's replies and cost since `since` (ms).
pub fn mates_since(db: &Db, since: i64) -> Result<Vec<MateDay>, String> {
    let query = evo_store::usage::UsageQuery {
        from_ms: Some(since),
        ..Default::default()
    };
    Ok(mates_of(
        db.with(|conn| evo_store::usage::summary(conn, &query))?,
    ))
}

/// Coming up (until the end of tomorrow) and each mate's day so far.
#[tauri::command]
pub fn mates_today(app: AppHandle) -> Result<Value, String> {
    let (start, until) = window(&Local::now());
    let db = app.state::<Db>();
    let coming = db.with(|conn| evo_store::agenda::upcoming(conn, until))?;
    let team = mates_since(&db, start)?;
    Ok(serde_json::json!({ "coming": coming, "team": team, "since": start }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    #[test]
    fn the_window_runs_from_this_midnight_to_the_end_of_tomorrow() {
        let zone = FixedOffset::east_opt(7 * 3600).unwrap();
        let now = zone.with_ymd_and_hms(2026, 11, 28, 15, 30, 0).unwrap();
        let (start, until) = window(&now);
        assert_eq!(
            start,
            zone.with_ymd_and_hms(2026, 11, 28, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
        assert_eq!(
            until,
            zone.with_ymd_and_hms(2026, 11, 30, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
    }
}
