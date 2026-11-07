//! `usage_get`: tokens and cost per model or per day, from the same history the
//! Usage page reads (`usage_events` in `evo.db`, `evo_store::usage::summary`).
//! The periods are the page's ranges: today, the last 7 days (today included),
//! the last 30 days, all time; days start at local midnight.
//!
//! Spec: docs/features/agent-browser (FR-19, US-4); docs/features/usage-and-cost.

use evo_store::rusqlite::{params, Connection};
use evo_store::usage::{self, UsageQuery, UsageSummary, UsageTotals};
use evo_store::DbError;
use serde_json::{json, Value};
use tauri::Manager;

use super::super::{BoxFuture, Class, Tool, ToolCtx, ToolOutput};
use super::choice;
use crate::db::Db;

const PERIODS: [&str; 4] = ["today", "week", "month", "all"];
const GROUPS: [&str; 2] = ["model", "day"];
/// Most rows a table shows; the rest are counted.
const MAX_ROWS: usize = 40;

/// How many local days a period covers, today included; `None` for all time.
pub fn period_days(period: &str) -> Option<u32> {
    match period {
        "today" => Some(1),
        "week" => Some(7),
        "month" => Some(30),
        _ => None,
    }
}

/// SQLite date modifier that moves local midnight back to the first day of a range of `days` days.
fn start_modifier(days: u32) -> String {
    format!("-{} days", days.saturating_sub(1))
}

/// Local midnight of the first day of a `days`-day range, in ms since the epoch.
fn range_start_ms(conn: &Connection, days: u32) -> Result<i64, DbError> {
    Ok(conn.query_row(
        "SELECT CAST(strftime('%s', 'now', 'localtime', 'start of day', ?1, 'utc') AS INTEGER) * 1000",
        params![start_modifier(days)],
        |row| row.get(0),
    )?)
}

/// The history of `period`, with its series bucketed by day.
pub fn summary_for(conn: &Connection, period: &str) -> Result<UsageSummary, DbError> {
    let from_ms = match period_days(period) {
        Some(days) => Some(range_start_ms(conn, days)?),
        None => None,
    };
    usage::summary(conn, &UsageQuery { from_ms, bucket: Some("day".into()), ..Default::default() })
}

// ── The report ──────────────────────────────────────────────────────────────

/// One line of the table: a model or a day.
#[derive(Debug, Clone, Default, PartialEq)]
struct Row {
    label: String,
    requests: i64,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    cost: f64,
    /// Some responses of the row came from a model with no recorded cost.
    unpriced: bool,
}

impl Row {
    fn add(&mut self, totals: &UsageTotals) {
        self.requests += totals.requests;
        self.input += totals.input;
        self.output += totals.output;
        self.cache_read += totals.cache_read;
        self.cache_write += totals.cache_write;
        self.cost += totals.cost_usd;
    }
}

/// A model's cost is unknown when it used tokens and the history holds no cost for them.
fn is_unpriced(totals: &UsageTotals) -> bool {
    totals.cost_usd == 0.0 && totals.input + totals.output + totals.cache_read + totals.cache_write > 0
}

fn model_label(provider: &str, model: &str) -> String {
    if provider.is_empty() {
        model.to_string()
    } else {
        format!("{provider}/{model}")
    }
}

fn rows_by_model(summary: &UsageSummary) -> Vec<Row> {
    summary
        .by_model
        .iter()
        .map(|model| {
            let mut row = Row { label: model_label(&model.provider_id, &model.model_id), unpriced: is_unpriced(&model.totals), ..Default::default() };
            row.add(&model.totals);
            row
        })
        .collect()
}

/// Series points are per day and model: sum each day, oldest first.
fn rows_by_day(summary: &UsageSummary) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for point in &summary.series {
        if rows.last().map(|row| row.label.as_str()) != Some(point.bucket.as_str()) {
            rows.push(Row { label: point.bucket.clone(), ..Default::default() });
        }
        let row = rows.last_mut().expect("a row was just pushed");
        row.add(&point.totals);
        row.unpriced |= is_unpriced(&point.totals);
    }
    rows
}

/// 1234567 as `1,234,567`.
fn grouped(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 {
        out.insert(0, '-');
    }
    out
}

fn money(cost: f64) -> String {
    if cost < 1.0 {
        format!("${cost:.4}")
    } else {
        format!("${cost:.2}")
    }
}

fn cost_cell(row: &Row) -> String {
    match (row.unpriced, row.cost > 0.0) {
        (true, false) => "n/a".into(),
        (true, true) => format!("{}*", money(row.cost)),
        _ => money(row.cost),
    }
}

/// A plain-text table: the first column left, the others right aligned.
fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let widths: Vec<usize> = (0..headers.len())
        .map(|col| rows.iter().map(|row| row[col].chars().count()).chain([headers[col].chars().count()]).max().unwrap_or(0))
        .collect();
    let line = |cells: Vec<&str>| {
        cells
            .iter()
            .enumerate()
            .map(|(col, cell)| {
                let pad = widths[col].saturating_sub(cell.chars().count());
                if col == 0 {
                    format!("{cell}{}", " ".repeat(pad))
                } else {
                    format!("{}{cell}", " ".repeat(pad))
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_string()
    };
    let mut lines = vec![line(headers.to_vec())];
    lines.extend(rows.iter().map(|row| line(row.iter().map(String::as_str).collect())));
    lines.join("\n")
}

fn period_label(period: &str) -> &'static str {
    match period {
        "today" => "today",
        "week" => "the last 7 days (today included)",
        "month" => "the last 30 days (today included)",
        _ => "all time",
    }
}

/// The tool's answer for a period's summary.
fn format_report(period: &str, group_by: &str, summary: &UsageSummary) -> String {
    let label = period_label(period);
    let totals = &summary.totals;
    if totals.requests == 0 {
        return format!("No usage is recorded for {label}.");
    }
    let mut rows = if group_by == "day" { rows_by_day(summary) } else { rows_by_model(summary) };
    let any_unpriced = rows.iter().any(|row| row.unpriced);
    let dropped = rows.len().saturating_sub(MAX_ROWS);
    if dropped > 0 {
        if group_by == "day" {
            // Days are oldest first: the latest days matter most.
            rows.drain(..dropped);
        } else {
            rows.truncate(MAX_ROWS);
        }
    }
    let mut cells: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            vec![row.label.clone(), grouped(row.requests), grouped(row.input), grouped(row.output), grouped(row.cache_read), grouped(row.cache_write), cost_cell(row)]
        })
        .collect();
    cells.push(vec![
        "total".into(),
        grouped(totals.requests),
        grouped(totals.input),
        grouped(totals.output),
        grouped(totals.cache_read),
        grouped(totals.cache_write),
        money(totals.cost_usd),
    ]);
    let headers = [if group_by == "day" { "day" } else { "model" }, "responses", "input", "output", "cache read", "cache write", "cost"];
    let mut out = format!("Usage for {label}, by {group_by} (the Usage page's numbers; days are local):\n\n{}", table(&headers, &cells));
    if dropped > 0 {
        let what = if group_by == "day" { "earlier days" } else { "more models" };
        out.push_str(&format!("\n\n({dropped} {what} left out of the table; the total row covers everything.)"));
    }
    if any_unpriced {
        out.push_str(
            "\n\nn/a: no cost is recorded for this model (a free model, or one with no models.dev price when it ran). * marks a row whose cost leaves out such responses; the total cost does too.",
        );
    }
    out
}

// ── The tool ────────────────────────────────────────────────────────────────

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "period": { "type": "string", "enum": PERIODS, "description": "today, week (last 7 days, default), month (last 30 days) or all." },
            "groupBy": { "type": "string", "enum": GROUPS, "description": "model (default) or day." }
        },
        "additionalProperties": false
    })
}

async fn run(ctx: ToolCtx, args: Value) -> ToolOutput {
    let parsed = (|| -> Result<(&'static str, &'static str), String> {
        Ok((choice(&args, "period", &PERIODS, "week")?, choice(&args, "groupBy", &GROUPS, "model")?))
    })();
    let (period, group_by) = match parsed {
        Ok(parsed) => parsed,
        Err(message) => return ToolOutput::error(message),
    };
    let app = ctx.app.clone();
    let summary = tauri::async_runtime::spawn_blocking(move || app.state::<Db>().with(|conn| summary_for(conn, period))).await;
    match summary {
        Ok(Ok(summary)) => ToolOutput::text(format_report(period, group_by, &summary)),
        Ok(Err(error)) => ToolOutput::error(format!("Could not read the usage history: {error}")),
        Err(error) => ToolOutput::error(format!("Could not read the usage history: {error}")),
    }
}

fn run_tool(ctx: ToolCtx, args: Value) -> BoxFuture<ToolOutput> {
    Box::pin(run(ctx, args))
}

pub fn tools() -> Vec<Tool> {
    vec![Tool {
        name: "usage_get",
        description: "Read what the models used and cost, from the same history as the app's Usage page: input, output and cache tokens and cost per model or per day, with totals, for today, the last 7 or 30 days, or all time. Use it to answer \"what did I spend this week\". A model with no recorded cost shows n/a.",
        schema,
        class: Class::Read,
        run: run_tool,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use evo_store::usage::{ModelUsage, NewUsage, SeriesPoint};

    fn totals(requests: i64, input: i64, output: i64, cost: f64) -> UsageTotals {
        UsageTotals { requests, input, output, cache_read: 10, cache_write: 5, cost_usd: cost, ..Default::default() }
    }

    fn model(provider: &str, id: &str, totals: UsageTotals) -> ModelUsage {
        ModelUsage { provider_id: provider.into(), model_id: id.into(), last_ms: 0, totals }
    }

    fn point(bucket: &str, id: &str, totals: UsageTotals) -> SeriesPoint {
        SeriesPoint { bucket: bucket.into(), provider_id: "p".into(), model_id: id.into(), totals }
    }

    #[test]
    fn periods_follow_the_usage_page_ranges() {
        assert_eq!(period_days("today"), Some(1));
        assert_eq!(period_days("week"), Some(7));
        assert_eq!(period_days("month"), Some(30));
        assert_eq!(period_days("all"), None);
        // Today included: a 7-day range starts 6 days back.
        assert_eq!(start_modifier(1), "-0 days");
        assert_eq!(start_modifier(7), "-6 days");
        assert_eq!(start_modifier(30), "-29 days");
    }

    #[test]
    fn ranges_start_at_a_local_midnight_before_now() {
        let dir = std::env::temp_dir().join(format!("evo-usage-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = evo_store::open_at(&dir.join("evo.db")).unwrap();
        let now = evo_store::chats::now_ms();
        let today = range_start_ms(&conn, 1).unwrap();
        let week = range_start_ms(&conn, 7).unwrap();
        assert!(today <= now && now - today < 25 * 3_600_000, "today starts within a day of now");
        let days = (today - week) as f64 / 86_400_000.0;
        assert!((days - 6.0).abs() < 0.1, "a week starts 6 days before today: {days}");
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn summaries_cover_the_period_and_group_by_day() {
        let dir = std::env::temp_dir().join(format!("evo-usage-test-db-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut conn = evo_store::open_at(&dir.join("evo.db")).unwrap();
        let now = evo_store::chats::now_ms();
        let row = |at_ms: i64, model: &str, cost: f64| NewUsage {
            at_ms,
            chat_id: Some("c".into()),
            source: "chat".into(),
            provider_id: "p".into(),
            model_id: model.into(),
            input: 100,
            output: 20,
            cost_usd: cost,
            stop_reason: Some("stop".into()),
            ..Default::default()
        };
        usage::record(&mut conn, &[row(now, "big", 0.5), row(now - 40 * 86_400_000, "old", 9.0), row(now - 2 * 86_400_000, "free", 0.0)]).unwrap();
        let week = summary_for(&conn, "week").unwrap();
        assert_eq!(week.totals.requests, 2, "the 40-day-old response is outside the week");
        let all = summary_for(&conn, "all").unwrap();
        assert_eq!(all.totals.requests, 3);
        let report = format_report("week", "day", &week);
        assert!(report.contains("by day"), "{report}");
        assert!(report.contains("n/a"), "{report}");
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_model_table_shows_tokens_cost_and_totals() {
        let summary = UsageSummary {
            totals: totals(5, 1_234_000, 5_000, 1.5),
            by_model: vec![model("anthropic", "big", totals(3, 1_000_000, 4_000, 1.5)), model("local", "free", totals(2, 234_000, 1_000, 0.0))],
            ..Default::default()
        };
        let report = format_report("week", "model", &summary);
        assert!(report.starts_with("Usage for the last 7 days (today included), by model"), "{report}");
        assert!(report.contains("anthropic/big"), "{report}");
        assert!(report.contains("1,000,000"), "{report}");
        assert!(report.contains("$1.50"), "{report}");
        assert!(report.contains("total"), "{report}");
        // The model without a cost says so, and the note explains it.
        let free = report.lines().find(|line| line.starts_with("local/free")).unwrap();
        assert!(free.trim_end().ends_with("n/a"), "{free}");
        assert!(report.contains("no cost is recorded"), "{report}");
    }

    #[test]
    fn days_sum_their_models_oldest_first_and_mark_partial_costs() {
        let summary = UsageSummary {
            totals: totals(4, 400, 40, 0.3),
            series: vec![
                point("2026-11-06", "big", totals(1, 100, 10, 0.1)),
                point("2026-11-07", "big", totals(1, 100, 10, 0.2)),
                point("2026-11-07", "free", totals(2, 200, 20, 0.0)),
            ],
            ..Default::default()
        };
        let rows = rows_by_day(&summary);
        assert_eq!(rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["2026-11-06", "2026-11-07"]);
        assert_eq!(rows[1].requests, 3);
        assert!(!rows[0].unpriced && rows[1].unpriced);
        assert_eq!(cost_cell(&rows[0]), "$0.1000");
        assert_eq!(cost_cell(&rows[1]), "$0.2000*");
        let report = format_report("month", "day", &summary);
        assert!(report.contains("by day"), "{report}");
        assert!(report.contains("2026-11-07"), "{report}");
    }

    #[test]
    fn an_empty_history_says_so_and_long_tables_are_cut() {
        assert_eq!(format_report("today", "model", &UsageSummary::default()), "No usage is recorded for today.");
        let series: Vec<SeriesPoint> = (0..50).map(|day| point(&format!("2026-10-{:02}", day + 1), "m", totals(1, 1, 1, 0.01))).collect();
        let summary = UsageSummary { totals: totals(50, 50, 50, 0.5), series, ..Default::default() };
        let report = format_report("all", "day", &summary);
        assert!(report.contains("10 earlier days left out"), "{report}");
        assert!(!report.contains("2026-10-01 "), "the oldest days go first");
        assert!(report.contains("2026-10-50"));
    }

    #[test]
    fn numbers_and_tables_format_plainly() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1,000");
        assert_eq!(grouped(-1234567), "-1,234,567");
        assert_eq!(money(0.05), "$0.0500");
        assert_eq!(money(12.345), "$12.35");
        let text = table(&["a", "bb"], &[vec!["xxx".into(), "1".into()], vec!["y".into(), "22".into()]]);
        assert_eq!(text, "a    bb\nxxx   1\ny    22");
    }
}
