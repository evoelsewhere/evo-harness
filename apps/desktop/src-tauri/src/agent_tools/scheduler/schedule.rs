//! When a scheduled task runs: the schedule kinds, the next run after a moment
//! (local time), and a plain description. Every repeating kind is a five-field
//! cron expression underneath (minute hour day-of-month month day-of-week; `*`,
//! lists, ranges and `/step`; Sunday is 0 or 7).
//!
//! A port of the webview's `src/lib/schedule.ts`, which the Scheduler page and the
//! runner keep using; the two must agree because the shell writes the
//! `nextRunAt` the runner waits for. Tasks store a schedule as the TypeScript
//! `Schedule` object (`{ kind, ... }`), which [`Schedule`] reads and writes as is.
//!
//! Spec: docs/features/agent-tools-layers (FR-5); docs/features/memory-and-scheduler (FR-11).

use std::collections::BTreeSet;

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, Local, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// When a task runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Schedule {
    /// Only on demand.
    Manual,
    /// At one moment (ms since the epoch).
    Once { at: i64 },
    Hourly { minute: u32 },
    Daily { time: String },
    Weekdays { time: String },
    Weekly { time: String, days: Vec<u32> },
    Cron { expression: String },
}

const DAY_NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// JavaScript's `Number.parseInt(text, 10)`: the leading integer, if any.
fn parse_int(text: &str) -> Option<i64> {
    let text = text.trim_start();
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let end = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
    if end == 0 {
        return None;
    }
    digits[..end].parse::<i64>().ok().map(|n| sign * n)
}

/// Hour and minute of "HH:MM", clamped; 09:00 for a part that is not a number.
fn hm(time: &str) -> (i64, i64) {
    let mut parts = time.split(':');
    let hour = parts.next().and_then(parse_int).map_or(9, |h| h.clamp(0, 23));
    let minute = parts.next().and_then(parse_int).map_or(0, |m| m.clamp(0, 59));
    (hour, minute)
}

/// The cron expression of a repeating schedule.
pub fn cron_of(schedule: &Schedule) -> Option<String> {
    match schedule {
        Schedule::Hourly { minute } => Some(format!("{minute} * * * *")),
        Schedule::Daily { time } => {
            let (h, m) = hm(time);
            Some(format!("{m} {h} * * *"))
        }
        Schedule::Weekdays { time } => {
            let (h, m) = hm(time);
            Some(format!("{m} {h} * * 1-5"))
        }
        Schedule::Weekly { time, days } => {
            let (h, m) = hm(time);
            let days: BTreeSet<u32> = days.iter().copied().collect();
            let days = if days.is_empty() {
                "1".to_string()
            } else {
                days.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
            };
            Some(format!("{m} {h} * * {days}"))
        }
        Schedule::Cron { expression } => Some(expression.trim().to_string()),
        Schedule::Manual | Schedule::Once { .. } => None,
    }
}

/// One cron field as the set of values it matches.
fn field(text: &str, min: i64, max: i64) -> Result<BTreeSet<i64>, String> {
    let mut out = BTreeSet::new();
    for part in text.split(',') {
        let mut pieces = part.split('/');
        let range = pieces.next().unwrap_or("");
        let step_text = pieces.next();
        let step = match step_text {
            None => 1,
            Some(step) => parse_int(step).unwrap_or(0),
        };
        if step < 1 {
            return Err(format!("bad step in \"{part}\""));
        }
        let (lo, hi) = if range == "*" {
            (Some(min), Some(max))
        } else if range.contains('-') {
            let mut ends = range.split('-');
            let lo = ends.next().and_then(parse_int);
            let hi = ends.next().and_then(parse_int);
            (lo, hi)
        } else {
            let lo = parse_int(range);
            (lo, if step_text.is_none() { lo } else { Some(max) })
        };
        match (lo, hi) {
            (Some(lo), Some(hi)) if lo >= min && hi <= max && lo <= hi => {
                let mut v = lo;
                while v <= hi {
                    out.insert(v);
                    v += step;
                }
            }
            _ => return Err(format!("\"{part}\" is outside {min}–{max}")),
        }
    }
    Ok(out)
}

/// A parsed cron expression.
#[derive(Debug, Clone)]
pub struct Cron {
    minutes: BTreeSet<i64>,
    hours: BTreeSet<i64>,
    days: BTreeSet<i64>,
    months: BTreeSet<i64>,
    weekdays: BTreeSet<i64>,
    any_day: bool,
    any_weekday: bool,
}

pub fn parse_cron(expression: &str) -> Result<Cron, String> {
    let parts: Vec<&str> = expression.split_whitespace().collect();
    if parts.len() != 5 {
        return Err("use five fields: minute hour day month weekday".into());
    }
    let mut weekdays = field(parts[4], 0, 7)?;
    if weekdays.contains(&7) {
        weekdays.insert(0);
    }
    Ok(Cron {
        minutes: field(parts[0], 0, 59)?,
        hours: field(parts[1], 0, 23)?,
        days: field(parts[2], 1, 31)?,
        months: field(parts[3], 1, 12)?,
        weekdays,
        any_day: parts[2] == "*",
        any_weekday: parts[4] == "*",
    })
}

/// The first minute after `after` that `cron` matches, on the local clock
/// (`after` and the result are wall-clock times, not instants), within about a year.
fn next_cron_naive(cron: &Cron, after: NaiveDateTime) -> Option<NaiveDateTime> {
    let mut t = after.with_second(0)?.with_nanosecond(0)? + Duration::minutes(1);
    let limit = after + Duration::days(400);
    while t < limit {
        if !cron.months.contains(&i64::from(t.month())) {
            let (year, month) = if t.month() == 12 { (t.year() + 1, 1) } else { (t.year(), t.month() + 1) };
            t = NaiveDate::from_ymd_opt(year, month, 1)?.and_time(NaiveTime::MIN);
            continue;
        }
        // Both day fields restricted: either matches (the usual cron rule).
        let dom = cron.days.contains(&i64::from(t.day()));
        let dow = cron.weekdays.contains(&i64::from(t.weekday().num_days_from_sunday()));
        let day_ok = match (cron.any_day, cron.any_weekday) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => dom || dow,
        };
        if !day_ok {
            t = t.date().succ_opt()?.and_time(NaiveTime::MIN);
            continue;
        }
        if !cron.hours.contains(&i64::from(t.hour())) {
            t = t.with_minute(0)? + Duration::hours(1);
            continue;
        }
        if !cron.minutes.contains(&i64::from(t.minute())) {
            t += Duration::minutes(1);
            continue;
        }
        return Some(t);
    }
    None
}

/// The instant of a local wall-clock time; the earlier one when it happens twice,
/// `None` when the clock skips it (a spring-forward gap).
fn local_instant(naive: NaiveDateTime) -> Option<i64> {
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(t) => Some(t.timestamp_millis()),
        LocalResult::Ambiguous(first, _) => Some(first.timestamp_millis()),
        LocalResult::None => None,
    }
}

fn next_cron(cron: &Cron, after_ms: i64) -> Option<i64> {
    let mut after = Local.timestamp_millis_opt(after_ms).single()?.naive_local();
    // A minute that does not exist on the local clock: look at the next one.
    for _ in 0..120 {
        let found = next_cron_naive(cron, after)?;
        if let Some(ms) = local_instant(found) {
            return Some(ms);
        }
        after = found;
    }
    None
}

/// When the task runs next after `after` (ms), or `None` (manual, past, or never).
pub fn next_run(schedule: &Schedule, after: i64) -> Option<i64> {
    match schedule {
        Schedule::Manual => None,
        Schedule::Once { at } => (*at > after).then_some(*at),
        _ => {
            let expression = cron_of(schedule)?;
            next_cron(&parse_cron(&expression).ok()?, after)
        }
    }
}

/// Why a schedule cannot run, or `None`.
pub fn schedule_problem(schedule: &Schedule) -> Option<String> {
    if let Schedule::Weekly { days, .. } = schedule {
        if days.is_empty() {
            return Some("pick at least one day".into());
        }
    }
    let expression = cron_of(schedule)?;
    parse_cron(&expression).err()
}

fn clock(time: &str) -> String {
    let (h, m) = hm(time);
    format!("{h:02}:{m:02}")
}

/// "Every weekday at 09:00", "Once, Fri 7 Nov 14:30", ...
pub fn describe_schedule(schedule: &Schedule) -> String {
    match schedule {
        Schedule::Manual => "Only when you run it".into(),
        Schedule::Once { at } => format!("Once, {}", format_when(*at, Utc::now().timestamp_millis())),
        Schedule::Hourly { minute: 0 } => "Every hour".into(),
        Schedule::Hourly { minute } => format!("Every hour at :{minute:02}"),
        Schedule::Daily { time } => format!("Every day at {}", clock(time)),
        Schedule::Weekdays { time } => format!("Every weekday at {}", clock(time)),
        Schedule::Weekly { time, days } => {
            let days: BTreeSet<u32> = days.iter().copied().collect();
            let names: Vec<&str> = days.iter().map(|d| DAY_NAMES[(*d % 7) as usize]).collect();
            let names = if names.is_empty() { "Mon".to_string() } else { names.join(", ") };
            format!("Every {names} at {}", clock(time))
        }
        Schedule::Cron { expression } => format!("Cron {}", expression.trim()),
    }
}

/// "Today 09:00", "Tomorrow 09:00", "Fri 7 Nov 09:00" (24-hour clock, local time).
pub fn format_when(at: i64, now: i64) -> String {
    let (Some(d), Some(n)) = (Local.timestamp_millis_opt(at).single(), Local.timestamp_millis_opt(now).single()) else {
        return String::new();
    };
    let time = d.format("%H:%M");
    match (d.date_naive() - n.date_naive()).num_days() {
        0 => format!("Today {time}"),
        1 => format!("Tomorrow {time}"),
        -1 => format!("Yesterday {time}"),
        _ => format!("{} {time}", d.format("%a %-d %b")),
    }
}

/// `new Date(ms).toISOString()`.
pub fn iso(ms: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(ms).map(|t| t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
}

/// `Date.parse` for the shapes an agent writes: RFC 3339, a local date-time
/// without an offset, or a bare date (UTC, as JavaScript reads it).
fn parse_moment(text: &str) -> Option<i64> {
    let text = text.trim();
    if let Ok(t) = DateTime::parse_from_rfc3339(text) {
        return Some(t.timestamp_millis());
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(text, format) {
            return local_instant(naive);
        }
    }
    let date = NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?;
    Some(Utc.from_utc_datetime(&date.and_time(NaiveTime::MIN)).timestamp_millis())
}

/// A JavaScript `Number(value)` that is a number, or `None`.
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Null => Some(0.0),
        _ => None,
    }
}

/// A schedule from loose input (the agent's tool arguments): `kind` plus fields.
/// `Err` holds a message for the agent.
pub fn schedule_from(input: &Value) -> Result<Schedule, String> {
    let get = |key: &str| input.get(key).filter(|v| !v.is_null());
    let kind = get("kind")
        .or_else(|| get("type"))
        .map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))
        .unwrap_or_else(|| "manual".into());
    let time = get("time").and_then(Value::as_str).unwrap_or("09:00").to_string();
    match kind.as_str() {
        "once" => {
            let at = match get("at") {
                Some(Value::Number(n)) => n.as_f64().filter(|f| f.is_finite()).map(|f| f as i64),
                Some(Value::String(s)) => parse_moment(s),
                _ => None,
            };
            at.map(|at| Schedule::Once { at })
                .ok_or_else(|| "a \"once\" schedule needs `at` (ISO date and time)".to_string())
        }
        "hourly" => {
            let minute = get("minute").and_then(number).filter(|n| !n.is_nan()).unwrap_or(0.0);
            Ok(Schedule::Hourly { minute: minute.clamp(0.0, 59.0) as u32 })
        }
        "daily" => Ok(Schedule::Daily { time }),
        "weekdays" => Ok(Schedule::Weekdays { time }),
        "weekly" => {
            let days: Vec<u32> = match get("days").and_then(Value::as_array) {
                Some(days) => days
                    .iter()
                    .filter_map(number)
                    .filter(|d| (0.0..=7.0).contains(d))
                    .map(|d| (d as u32) % 7)
                    .collect(),
                None => vec![1],
            };
            Ok(Schedule::Weekly { time, days: if days.is_empty() { vec![1] } else { days } })
        }
        "cron" => {
            let expression = get("expression").or_else(|| get("cron")).and_then(Value::as_str).unwrap_or("").to_string();
            parse_cron(&expression)?;
            Ok(Schedule::Cron { expression })
        }
        "manual" => Ok(Schedule::Manual),
        other => Err(format!("unknown schedule kind \"{other}\"")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MINUTE_MS: i64 = 60_000;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d).unwrap().and_hms_opt(h, mi, 0).unwrap()
    }

    fn next(expression: &str, after: NaiveDateTime) -> Option<NaiveDateTime> {
        next_cron_naive(&parse_cron(expression).unwrap(), after)
    }

    #[test]
    fn parse_int_reads_the_leading_integer_like_javascript() {
        assert_eq!(parse_int("15"), Some(15));
        assert_eq!(parse_int(" 30am"), Some(30));
        assert_eq!(parse_int("15.5"), Some(15));
        assert_eq!(parse_int("-3"), Some(-3));
        assert_eq!(parse_int(""), None);
        assert_eq!(parse_int("x1"), None);
    }

    #[test]
    fn time_of_day_is_clamped_and_defaults_to_nine() {
        assert_eq!(hm("07:05"), (7, 5));
        assert_eq!(hm("25:99"), (23, 59));
        assert_eq!(hm("soon"), (9, 0));
        assert_eq!(hm("8"), (8, 0));
        assert_eq!(clock("7:5"), "07:05");
    }

    #[test]
    fn every_kind_has_the_cron_the_page_gives_it() {
        assert_eq!(cron_of(&Schedule::Hourly { minute: 15 }).unwrap(), "15 * * * *");
        assert_eq!(cron_of(&Schedule::Daily { time: "09:30".into() }).unwrap(), "30 9 * * *");
        assert_eq!(cron_of(&Schedule::Weekdays { time: "08:00".into() }).unwrap(), "0 8 * * 1-5");
        assert_eq!(cron_of(&Schedule::Weekly { time: "18:00".into(), days: vec![5, 1, 5] }).unwrap(), "0 18 * * 1,5");
        assert_eq!(cron_of(&Schedule::Weekly { time: "18:00".into(), days: vec![] }).unwrap(), "0 18 * * 1");
        assert_eq!(cron_of(&Schedule::Cron { expression: " */5 * * * * ".into() }).unwrap(), "*/5 * * * *");
        assert_eq!(cron_of(&Schedule::Manual), None);
        assert_eq!(cron_of(&Schedule::Once { at: 1 }), None);
    }

    #[test]
    fn cron_fields_take_lists_ranges_and_steps() {
        let f = |t: &str, lo, hi| field(t, lo, hi).unwrap().into_iter().collect::<Vec<_>>();
        assert_eq!(f("*", 0, 3), vec![0, 1, 2, 3]);
        assert_eq!(f("1,3", 0, 5), vec![1, 3]);
        assert_eq!(f("2-4", 0, 5), vec![2, 3, 4]);
        assert_eq!(f("*/20", 0, 59), vec![0, 20, 40]);
        assert_eq!(f("10-30/10", 0, 59), vec![10, 20, 30]);
        assert_eq!(f("50/5", 0, 59), vec![50, 55]);
        assert!(field("60", 0, 59).unwrap_err().contains("outside 0–59"));
        assert!(field("5-2", 0, 59).is_err());
        assert!(field("*/0", 0, 59).unwrap_err().contains("bad step"));
        assert!(field("x", 0, 59).is_err());
    }

    #[test]
    fn a_cron_expression_needs_five_fields_and_seven_is_sunday() {
        assert_eq!(parse_cron("* * * *").unwrap_err(), "use five fields: minute hour day month weekday");
        assert!(parse_cron("0 0 1 13 *").is_err());
        let cron = parse_cron("0 0 * * 7").unwrap();
        assert!(cron.weekdays.contains(&0));
    }

    #[test]
    fn next_is_the_following_matching_minute() {
        // Strictly after the moment itself.
        assert_eq!(next("*/5 * * * *", at(2026, 11, 8, 9, 5)), Some(at(2026, 11, 8, 9, 10)));
        assert_eq!(next("0 9 * * *", at(2026, 11, 8, 8, 59)), Some(at(2026, 11, 8, 9, 0)));
        assert_eq!(next("0 9 * * *", at(2026, 11, 8, 9, 0)), Some(at(2026, 11, 9, 9, 0)));
        // Seconds are dropped.
        let with_seconds = at(2026, 11, 8, 9, 0) + Duration::seconds(30);
        assert_eq!(next("1 9 * * *", with_seconds), Some(at(2026, 11, 8, 9, 1)));
    }

    #[test]
    fn weekdays_skip_the_weekend_and_months_roll_over() {
        // 2026-11-07 is a Saturday.
        assert_eq!(next("0 9 * * 1-5", at(2026, 11, 7, 10, 0)), Some(at(2026, 11, 9, 9, 0)));
        assert_eq!(next("0 0 1 * *", at(2026, 12, 15, 0, 0)), Some(at(2027, 1, 1, 0, 0)));
        assert_eq!(next("0 0 1 1 *", at(2026, 12, 15, 0, 0)), Some(at(2027, 1, 1, 0, 0)));
        assert_eq!(next("30 23 31 12 *", at(2026, 11, 8, 0, 0)), Some(at(2026, 12, 31, 23, 30)));
    }

    #[test]
    fn day_of_month_and_day_of_week_combine_as_in_cron() {
        // Both restricted: either matches. Sunday 2026-11-08, then the 10th (Tuesday).
        assert_eq!(next("0 12 10 * 0", at(2026, 11, 8, 13, 0)), Some(at(2026, 11, 10, 12, 0)));
        assert_eq!(next("0 12 10 * 0", at(2026, 11, 10, 13, 0)), Some(at(2026, 11, 15, 12, 0)));
        // Only the day of month restricted: the weekday does not matter.
        assert_eq!(next("0 12 10 * *", at(2026, 11, 8, 13, 0)), Some(at(2026, 11, 10, 12, 0)));
    }

    #[test]
    fn a_date_that_never_comes_gives_up() {
        assert_eq!(next("0 0 30 2 *", at(2026, 1, 1, 0, 0)), None);
    }

    #[test]
    fn next_run_follows_the_kind() {
        assert_eq!(next_run(&Schedule::Manual, 0), None);
        assert_eq!(next_run(&Schedule::Once { at: 2_000 }, 1_000), Some(2_000));
        assert_eq!(next_run(&Schedule::Once { at: 1_000 }, 1_000), None);
        assert_eq!(next_run(&Schedule::Cron { expression: "nope".into() }, 0), None);
        // A repeating schedule is always after `after` and within the cron's period.
        let now = Utc::now().timestamp_millis();
        let hourly = next_run(&Schedule::Hourly { minute: 0 }, now).unwrap();
        assert!(hourly > now && hourly - now <= 60 * MINUTE_MS + 3_600_000);
        let every_minute = next_run(&Schedule::Cron { expression: "* * * * *".into() }, now).unwrap();
        assert!(every_minute > now && every_minute - now <= MINUTE_MS);
    }

    #[test]
    fn a_problem_is_named_before_the_task_exists() {
        assert_eq!(schedule_problem(&Schedule::Weekly { time: "09:00".into(), days: vec![] }).unwrap(), "pick at least one day");
        assert!(schedule_problem(&Schedule::Cron { expression: "1 2 3".into() }).unwrap().contains("five fields"));
        assert_eq!(schedule_problem(&Schedule::Daily { time: "09:00".into() }), None);
        assert_eq!(schedule_problem(&Schedule::Manual), None);
        assert_eq!(schedule_problem(&Schedule::Once { at: 5 }), None);
    }

    #[test]
    fn descriptions_read_like_the_page() {
        assert_eq!(describe_schedule(&Schedule::Manual), "Only when you run it");
        assert_eq!(describe_schedule(&Schedule::Hourly { minute: 0 }), "Every hour");
        assert_eq!(describe_schedule(&Schedule::Hourly { minute: 5 }), "Every hour at :05");
        assert_eq!(describe_schedule(&Schedule::Daily { time: "9:00".into() }), "Every day at 09:00");
        assert_eq!(describe_schedule(&Schedule::Weekdays { time: "17:45".into() }), "Every weekday at 17:45");
        assert_eq!(describe_schedule(&Schedule::Weekly { time: "08:00".into(), days: vec![5, 1] }), "Every Mon, Fri at 08:00");
        assert_eq!(describe_schedule(&Schedule::Weekly { time: "08:00".into(), days: vec![] }), "Every Mon at 08:00");
        assert_eq!(describe_schedule(&Schedule::Cron { expression: " 0 * * * * ".into() }), "Cron 0 * * * *");
        assert!(describe_schedule(&Schedule::Once { at: 0 }).starts_with("Once, "));
    }

    #[test]
    fn when_names_today_tomorrow_and_yesterday() {
        let now = Local.with_ymd_and_hms(2026, 11, 8, 12, 0, 0).unwrap();
        let ms = |days: i64, h: u32| {
            Local
                .from_local_datetime(&(now.date_naive() + Duration::days(days)).and_hms_opt(h, 30, 0).unwrap())
                .unwrap()
                .timestamp_millis()
        };
        let now_ms = now.timestamp_millis();
        assert_eq!(format_when(ms(0, 9), now_ms), "Today 09:30");
        assert_eq!(format_when(ms(1, 18), now_ms), "Tomorrow 18:30");
        assert_eq!(format_when(ms(-1, 7), now_ms), "Yesterday 07:30");
        assert_eq!(format_when(ms(3, 9), now_ms), "Wed 11 Nov 09:30");
    }

    #[test]
    fn iso_matches_to_iso_string() {
        assert_eq!(iso(0).unwrap(), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso(1_794_000_000_123).unwrap(), "2026-11-06T21:20:00.123Z");
    }

    #[test]
    fn moments_parse_as_javascript_reads_them() {
        assert_eq!(parse_moment("2026-11-09T09:00:00Z"), Some(1_794_214_800_000));
        assert_eq!(parse_moment("2026-11-09T11:00:00+02:00"), Some(1_794_214_800_000));
        assert_eq!(parse_moment("2026-11-09"), Some(1_794_182_400_000));
        // No offset: local time.
        let local = Local.with_ymd_and_hms(2026, 11, 9, 9, 0, 0).unwrap().timestamp_millis();
        assert_eq!(parse_moment("2026-11-09T09:00"), Some(local));
        assert_eq!(parse_moment("2026-11-09 09:00:00"), Some(local));
        assert_eq!(parse_moment("tomorrow"), None);
    }

    #[test]
    fn a_schedule_comes_from_loose_input() {
        let s = |v: Value| schedule_from(&v);
        assert_eq!(s(json!({})), Ok(Schedule::Manual));
        assert_eq!(s(json!({ "kind": "manual" })), Ok(Schedule::Manual));
        assert_eq!(s(json!({ "type": "daily" })), Ok(Schedule::Daily { time: "09:00".into() }));
        assert_eq!(s(json!({ "kind": "daily", "time": "06:30" })), Ok(Schedule::Daily { time: "06:30".into() }));
        assert_eq!(s(json!({ "kind": "weekdays", "time": "08:15" })), Ok(Schedule::Weekdays { time: "08:15".into() }));
        assert_eq!(s(json!({ "kind": "hourly", "minute": 90 })), Ok(Schedule::Hourly { minute: 59 }));
        assert_eq!(s(json!({ "kind": "hourly", "minute": "20" })), Ok(Schedule::Hourly { minute: 20 }));
        assert_eq!(s(json!({ "kind": "hourly", "minute": -4 })), Ok(Schedule::Hourly { minute: 0 }));
        assert_eq!(s(json!({ "kind": "hourly" })), Ok(Schedule::Hourly { minute: 0 }));
        assert_eq!(
            s(json!({ "kind": "weekly", "time": "10:00", "days": [7, 3, 9, -1] })),
            Ok(Schedule::Weekly { time: "10:00".into(), days: vec![0, 3] })
        );
        assert_eq!(s(json!({ "kind": "weekly" })), Ok(Schedule::Weekly { time: "09:00".into(), days: vec![1] }));
        assert_eq!(s(json!({ "kind": "weekly", "days": [] })), Ok(Schedule::Weekly { time: "09:00".into(), days: vec![1] }));
        assert_eq!(s(json!({ "kind": "cron", "expression": "0 9 * * 1" })), Ok(Schedule::Cron { expression: "0 9 * * 1".into() }));
        assert_eq!(s(json!({ "kind": "cron", "cron": "*/10 * * * *" })), Ok(Schedule::Cron { expression: "*/10 * * * *".into() }));
        assert_eq!(s(json!({ "kind": "once", "at": 1_794_214_800_000_i64 })), Ok(Schedule::Once { at: 1_794_214_800_000 }));
        assert_eq!(s(json!({ "kind": "once", "at": "2026-11-09T09:00:00Z" })), Ok(Schedule::Once { at: 1_794_214_800_000 }));
    }

    #[test]
    fn loose_input_that_cannot_run_is_refused_with_a_reason() {
        let s = |v: Value| schedule_from(&v).unwrap_err();
        assert_eq!(s(json!({ "kind": "once" })), "a \"once\" schedule needs `at` (ISO date and time)");
        assert_eq!(s(json!({ "kind": "once", "at": "whenever" })), "a \"once\" schedule needs `at` (ISO date and time)");
        assert_eq!(s(json!({ "kind": "yearly" })), "unknown schedule kind \"yearly\"");
        assert_eq!(s(json!({ "kind": "cron", "expression": "* * *" })), "use five fields: minute hour day month weekday");
        assert_eq!(s(json!({ "kind": "cron" })), "use five fields: minute hour day month weekday");
        assert!(s(json!({ "kind": "cron", "expression": "99 * * * *" })).contains("outside 0–59"));
    }

    #[test]
    fn a_schedule_is_stored_as_the_page_stores_it() {
        let stored = json!({ "kind": "weekly", "time": "09:00", "days": [1, 3] });
        let schedule: Schedule = serde_json::from_value(stored.clone()).unwrap();
        assert_eq!(schedule, Schedule::Weekly { time: "09:00".into(), days: vec![1, 3] });
        assert_eq!(serde_json::to_value(&schedule).unwrap(), stored);
        assert_eq!(serde_json::to_value(Schedule::Manual).unwrap(), json!({ "kind": "manual" }));
        assert_eq!(serde_json::to_value(Schedule::Once { at: 7 }).unwrap(), json!({ "kind": "once", "at": 7 }));
        assert_eq!(serde_json::to_value(Schedule::Hourly { minute: 3 }).unwrap(), json!({ "kind": "hourly", "minute": 3 }));
        assert_eq!(serde_json::to_value(Schedule::Cron { expression: "* * * * *".into() }).unwrap(), json!({ "kind": "cron", "expression": "* * * * *" }));
    }
}
