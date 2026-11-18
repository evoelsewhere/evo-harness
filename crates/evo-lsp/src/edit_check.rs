//! The check after the agent changes a file: the errors the language server
//! reports for the new text, set against those it reported before the change
//! and the new errors it reports in other files
//! (docs/features/code-intelligence/spec.md, FR-15–FR-19, FR-22, FR-23).
//!
//! [`before_edit`] runs before the change and never waits or starts a
//! server; [`after_edit`] sends the new text and waits within a short budget.
//! With no server running, it starts one and, while the budget lasts, waits
//! for it (a server warmed by reading the file is often almost ready).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::client::{path_of, uri_key, uri_of, Client};
use crate::ops::{format_diagnostic, show};
use crate::pool::Pool;

/// The longest an edit waits for the server's report (FR-17).
pub const EDIT_WAIT: Duration = Duration::from_secs(4);
/// The longest an edit waits while its server is in its first start (FR-17).
pub const COLD_WAIT: Duration = Duration::from_secs(10);
/// The most errors listed after an edit (FR-15).
const MAX_LISTED: usize = 20;
/// After a first report, how long to look for a slower check starting.
const SLOWER_CHECK: Duration = Duration::from_millis(300);
/// A server that is idle and has published nothing for this long after a
/// change is taken to have nothing new to say (the TypeScript server neither
/// republishes an unchanged file nor reports progress).
const QUIET: Duration = Duration::from_secs(1);
/// How long a loading server must stay idle before the check trusts it.
const SETTLE: Duration = Duration::from_secs(1);
/// How often the check looks for a report or for the server going idle.
const POLL: Duration = Duration::from_millis(100);

/// A file about to change: its text and, when its server runs, that server
/// and the errors it reported for this text.
pub struct Before {
    file: PathBuf,
    /// `None` for a file that does not exist yet.
    text: Option<String>,
    client: Option<Arc<Client>>,
    /// `None` when the server had not reported on this text.
    errors: Option<Vec<Value>>,
    /// The server's errors for every file it had reported on, by
    /// [`uri_key`]; `None` when it was busy, so its reports were not settled
    /// (FR-23).
    others: Option<HashMap<String, Vec<Value>>>,
}

/// Every file's errors from `client`, when it is not busy.
fn settled_errors(client: &Client) -> Option<HashMap<String, Vec<Value>>> {
    if client.indexing() {
        return None;
    }
    Some(
        client
            .reports()
            .into_iter()
            .map(|(key, (_, list))| (key, errors_only(list)))
            .collect(),
    )
}

/// The errors `client` last reported for `before`'s file, when they are for
/// the text the file had before the change.
async fn errors_for(client: &Client, before: &Before) -> Option<Vec<Value>> {
    let Some(text) = &before.text else {
        return Some(Vec::new());
    };
    let uri = uri_of(&before.file);
    if client.text_of(&uri).await.as_deref() != Some(text.as_str()) {
        return None;
    }
    client.last_diagnostics(&uri).map(errors_only)
}

fn is_error(d: &Value) -> bool {
    d.get("severity").and_then(Value::as_u64) == Some(1)
}

/// The errors of a report, one per place and code: a server can report the
/// same error twice from two checks (rust-analyzer's own and `cargo check`'s);
/// the longer message is kept.
fn errors_only(list: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for d in list.into_iter().filter(is_error) {
        let same = |o: &Value| start_of(o) == start_of(&d) && o.get("code") == d.get("code");
        match out.iter_mut().find(|o| same(o)) {
            Some(o) if message_len(&d) > message_len(o) => *o = d,
            Some(_) => {}
            None => out.push(d),
        }
    }
    out
}

fn message_len(d: &Value) -> usize {
    d.get("message").and_then(Value::as_str).map_or(0, str::len)
}

/// What makes two errors the same across an edit: lines move, so only the
/// message, code and source count (FR-16).
fn key(d: &Value) -> (String, String, String) {
    let text = |k: &str| match d.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(v) => v.to_string(),
    };
    (text("message"), text("code"), text("source"))
}

fn start_of(d: &Value) -> (u64, u64) {
    let at = |p: &str| d.pointer(p).and_then(Value::as_u64).unwrap_or(0);
    (at("/range/start/line"), at("/range/start/character"))
}

/// Before the change: the file's text, the running server for it (none is
/// started) and its errors for that text. `None` when no server handles the
/// file or its server is switched off.
pub async fn before_edit(pool: &Pool, file: &Path, cwd: &Path) -> Option<Before> {
    pool.server_for(file)?;
    let text = tokio::fs::read_to_string(file).await.ok();
    let mut before = Before {
        file: file.to_path_buf(),
        text,
        client: pool.running_for(file, cwd),
        errors: None,
        others: None,
    };
    if let Some(client) = before.client.clone() {
        before.errors = errors_for(&client, &before).await;
        before.others = settled_errors(&client);
    }
    Some(before)
}

/// After the change: the text to add to the tool's result, if any, within
/// `wait`. With no running server, one is started and waited for while the
/// budget lasts; when it is not ready in time nothing is added.
pub async fn after_edit(
    pool: &'static Pool,
    cwd: &Path,
    file: &Path,
    before: Option<Before>,
    wait: Duration,
) -> Option<String> {
    let mut before = before?;
    let start = Instant::now();
    // A server in its first start gets longer, once (FR-17).
    let cold = before.client.as_ref().is_none_or(|c| c.starting());
    let deadline = start + if cold { wait.max(COLD_WAIT) } else { wait };
    let left = || deadline.saturating_duration_since(Instant::now());
    let client = match before.client.clone() {
        Some(client) => client,
        None => {
            pool.warm(file, cwd);
            let client = loop {
                if let Some(client) = pool.running_for(file, cwd) {
                    break client;
                }
                if left().is_zero() {
                    return None;
                }
                tokio::time::sleep(Duration::from_millis(150).min(left())).await;
            };
            // Opened when the agent read the file: its report is for the old text.
            before.errors = errors_for(&client, &before).await;
            client
        }
    };
    let sent_at = Instant::now();
    let (uri, version) = client.sync_file(file).await.ok()?;
    // Wait for a report on the new text, or for the server to finish the work
    // the change set off: a server that does not republish an unchanged file
    // (rust-analyzer) is done then, and the file's last report still holds.
    // A server that only went quiet has not said the check is over, so its
    // answer never claims what is left (`finished` stays false).
    let mut quiet = false;
    // A loading server pauses between its steps; wait for it to stay idle
    // before taking its silence or its idleness as an answer.
    let settled = client.starting() && settle(&client, deadline).await;
    let mut list = loop {
        if let Some(list) = client
            .diagnostics_for(&uri, version, sent_at, Duration::ZERO)
            .await
        {
            break list;
        }
        if client.work_finished_since(sent_at) && (settled || !client.starting()) {
            break client.last_diagnostics(&uri).unwrap_or_default();
        }
        let last = client
            .last_report_at()
            .map_or(sent_at, |at| at.max(sent_at));
        if (settled || !client.starting()) && !client.indexing() && last.elapsed() >= QUIET {
            quiet = true;
            break client.last_diagnostics(&uri).unwrap_or_default();
        }
        if left().is_zero() {
            return None;
        }
        tokio::time::sleep(POLL.min(left())).await;
    };
    // A quick first report can come before a slower check (`cargo check`):
    // while time is left, wait for it and take the newest report.
    if !quiet && !client.work_finished_since(sent_at) {
        tokio::time::sleep(SLOWER_CHECK.min(left())).await;
        if client.indexing() {
            let _ = tokio::time::timeout(left(), client.wait_idle(left())).await;
        }
    }
    if let Some(newer) = client
        .diagnostics_for(&uri, version, sent_at, Duration::ZERO)
        .await
    {
        list = newer;
    }
    let finished = !quiet && !client.indexing();
    // Without settled reports from before, every error in other files is
    // listed, said to perhaps predate the change.
    let none = HashMap::new();
    let old = before.others.as_ref().unwrap_or(&none);
    let others = other_files(&client, old, &uri_key(&uri), cwd);
    describe(
        client.server.name,
        &show(file, cwd),
        before.errors.as_deref(),
        &errors_only(list),
        &others,
        before.others.is_some(),
        finished,
    )
}

/// Wait until `client` has been idle for [`SETTLE`] in a row, or `deadline`;
/// whether it settled.
async fn settle(client: &Client, deadline: Instant) -> bool {
    let mut idle_from: Option<Instant> = None;
    loop {
        if client.indexing() {
            idle_from = None;
        } else if idle_from.get_or_insert_with(Instant::now).elapsed() >= SETTLE {
            return true;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return false;
        }
        tokio::time::sleep(POLL.min(left)).await;
    }
}

/// New errors in files other than the changed one (`changed`, a
/// [`uri_key`]), set against `old`; by file as shown, sorted (FR-23).
fn other_files(
    client: &Client,
    old: &HashMap<String, Vec<Value>>,
    changed: &str,
    cwd: &Path,
) -> Vec<(String, Vec<Value>)> {
    let mut out: Vec<(String, Vec<Value>)> = client
        .reports()
        .into_iter()
        .filter(|(key, _)| key != changed)
        .filter_map(|(key, (uri, list))| {
            let after = errors_only(list);
            let before = old.get(&key).map(Vec::as_slice).unwrap_or(&[]);
            let (new, _) = new_errors(before, &after);
            if new.is_empty() {
                return None;
            }
            let new: Vec<Value> = new.into_iter().cloned().collect();
            let shown = path_of(&uri).map(|p| show(&p, cwd)).unwrap_or(uri);
            Some((shown, new))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The errors of `after` that `before` does not account for (as a multiset
/// of [`key`]), and how many it does.
fn new_errors<'a>(before: &[Value], after: &'a [Value]) -> (Vec<&'a Value>, usize) {
    let mut left: HashMap<_, usize> = HashMap::new();
    for d in before {
        *left.entry(key(d)).or_default() += 1;
    }
    let mut new = Vec::new();
    for d in after {
        match left.get_mut(&key(d)) {
            Some(n) if *n > 0 => *n -= 1,
            _ => new.push(d),
        }
    }
    let old = after.len() - new.len();
    (new, old)
}

/// The words for the model (FR-16, FR-18, FR-19, FR-23). `before` is `None`
/// when the changed file's errors from before the change are not known;
/// `others` are the new errors in other files, `others_known` false when
/// their errors from before the change were not known; `finished` is false
/// when the server was still checking at the end of the wait.
pub fn describe(
    server: &str,
    file: &str,
    before: Option<&[Value]>,
    after: &[Value],
    others: &[(String, Vec<Value>)],
    others_known: bool,
    finished: bool,
) -> Option<String> {
    let plural = |n: usize| if n == 1 { "error" } else { "errors" };
    let (mut new, old) = match before {
        Some(before) => new_errors(before, after),
        None => (after.iter().collect(), 0),
    };
    let mut lines = Vec::new();
    if new.is_empty() {
        let had = before.map_or(0, <[Value]>::len);
        // Only a finished check may say what is left.
        let state = match (finished, had, old) {
            (false, _, _) | (true, 0, _) => None,
            (true, _, 0) => Some(format!("{server}: no errors left in {file}")),
            (true, _, 1) => Some(format!(
                "{server}: no new errors in {file} (1 error that was already there remains)"
            )),
            (true, _, old) => Some(format!(
                "{server}: no new errors in {file} ({old} errors that were already there remain)"
            )),
        };
        if others.is_empty() {
            return state.map(|s| format!("{s}."));
        }
        lines.push(match (state, others_known) {
            (Some(s), true) => format!("{s}, but this change caused new errors in other files:"),
            (None, true) => {
                format!("{server}: this change to {file} caused new errors in other files:")
            }
            (Some(s), false) => format!(
                "{s}; errors in other files after this change (some may have been there before):"
            ),
            (None, false) => format!(
                "{server}: errors in other files after this change to {file} (some may have been there before):"
            ),
        });
    } else {
        new.sort_by_key(|d| start_of(d));
        let count = new.len();
        let what = if before.is_some() {
            format!("{count} new {}", plural(count))
        } else {
            format!("{count} {}", plural(count))
        };
        let also = match (before, old) {
            (None, _) => " (some may have been there before)".to_string(),
            (Some(_), 0) => String::new(),
            (Some(_), 1) => " (1 other error was already there)".to_string(),
            (Some(_), old) => format!(" ({old} other errors were already there)"),
        };
        lines.push(format!(
            "{server}: {what} in {file} after this change{also}:"
        ));
    }
    // One cap for every file, the changed file first.
    let mut listed: Vec<Option<String>> = new
        .iter()
        .map(|d| Some(format_diagnostic(file, d)))
        .collect();
    if !others.is_empty() && !new.is_empty() {
        listed.push(None);
    }
    for (other, list) in others {
        let mut list: Vec<&Value> = list.iter().collect();
        list.sort_by_key(|d| start_of(d));
        listed.extend(list.iter().map(|d| Some(format_diagnostic(other, d))));
    }
    let errors = listed.iter().flatten().count();
    let mut shown = 0;
    for line in listed {
        match line {
            None if others_known => lines.push("New errors in other files:".into()),
            None => lines.push("Errors in other files (some may have been there before):".into()),
            Some(_) if shown == MAX_LISTED => {}
            Some(line) => {
                shown += 1;
                lines.push(line);
            }
        }
    }
    if errors > MAX_LISTED {
        lines.push(format!("… {} more", errors - MAX_LISTED));
    }
    if !finished {
        lines.push(format!("({server} was still checking; more may follow.)"));
    }
    lines.push("If the change is not finished yet, these may go away with your next edit.".into());
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn err(line: u64, message: &str) -> Value {
        json!({ "range": { "start": { "line": line, "character": 4 } }, "severity": 1, "message": message, "source": "rustc", "code": "E0308" })
    }

    #[test]
    fn edit_check_lists_only_new_errors_and_counts_old_ones() {
        let before = [err(3, "mismatched types")];
        // The old error moved down two lines; one new error.
        let after = [err(9, "cannot find value `x`"), err(5, "mismatched types")];
        let text = describe(
            "rust-analyzer",
            "src/a.rs",
            Some(&before),
            &after,
            &[],
            true,
            true,
        )
        .unwrap();
        assert_eq!(
            text,
            "rust-analyzer: 1 new error in src/a.rs after this change (1 other error was already there):\n\
             src/a.rs:10:5 error: cannot find value `x` [rustc E0308]\n\
             If the change is not finished yet, these may go away with your next edit."
        );
    }

    #[test]
    fn edit_check_counts_repeated_errors() {
        let before = [err(1, "mismatched types")];
        let after = [err(1, "mismatched types"), err(7, "mismatched types")];
        let text = describe("ra", "a.rs", Some(&before), &after, &[], true, true).unwrap();
        assert!(text.starts_with("ra: 1 new error in a.rs"), "{text}");
        assert!(text.contains("a.rs:8:5"), "{text}");
    }

    #[test]
    fn edit_check_says_when_errors_are_gone_or_remain() {
        let before = [err(1, "a"), err(2, "b")];
        assert_eq!(
            describe("ra", "a.rs", Some(&before), &[], &[], true, true).as_deref(),
            Some("ra: no errors left in a.rs.")
        );
        assert_eq!(
            describe("ra", "a.rs", Some(&before), &[err(4, "b")], &[], true, true).as_deref(),
            Some("ra: no new errors in a.rs (1 error that was already there remains).")
        );
        // Never claim what an unfinished check has not seen.
        assert_eq!(
            describe("ra", "a.rs", Some(&before), &[], &[], true, false),
            None
        );
    }

    #[test]
    fn edit_check_stays_quiet_on_a_clean_file() {
        assert_eq!(
            describe("ra", "a.rs", Some(&[]), &[], &[], true, true),
            None
        );
        assert_eq!(describe("ra", "a.rs", None, &[], &[], true, true), None);
        assert_eq!(describe("ra", "a.rs", None, &[], &[], true, false), None);
    }

    #[test]
    fn edit_check_without_a_baseline_lists_everything() {
        let text = describe(
            "ra",
            "a.rs",
            None,
            &[err(0, "a"), err(1, "b")],
            &[],
            true,
            false,
        )
        .unwrap();
        assert!(
            text.starts_with(
                "ra: 2 errors in a.rs after this change (some may have been there before):"
            ),
            "{text}"
        );
        assert!(
            text.contains("(ra was still checking; more may follow.)"),
            "{text}"
        );
        assert!(text.ends_with("may go away with your next edit."), "{text}");
    }

    #[test]
    fn edit_check_caps_the_list() {
        let after: Vec<Value> = (0..25).map(|i| err(i, &format!("e{i}"))).collect();
        let text = describe("ra", "a.rs", Some(&[]), &after, &[], true, true).unwrap();
        assert!(text.contains("… 5 more"), "{text}");
        assert_eq!(text.lines().filter(|l| l.contains(" error: ")).count(), 20);
    }

    #[test]
    fn edit_check_lists_new_errors_in_other_files() {
        let caller = vec![err(7, "mismatched types")];
        let others = vec![("src/main.rs".to_string(), caller)];
        // The changed file stays clean.
        let text = describe("ra", "src/math.rs", Some(&[]), &[], &others, true, true).unwrap();
        assert_eq!(
            text,
            "ra: this change to src/math.rs caused new errors in other files:\n\
             src/main.rs:8:5 error: mismatched types [rustc E0308]\n\
             If the change is not finished yet, these may go away with your next edit."
        );
        // Both: the changed file first, then the others under a heading.
        let text = describe(
            "ra",
            "src/math.rs",
            Some(&[]),
            &[err(1, "a")],
            &others,
            true,
            true,
        )
        .unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[1].starts_with("src/math.rs:2:5"), "{text}");
        assert_eq!(lines[2], "New errors in other files:");
        assert!(lines[3].starts_with("src/main.rs:8:5"), "{text}");
        // Fixed here, broken there.
        let text = describe(
            "ra",
            "src/math.rs",
            Some(&[err(1, "a")]),
            &[],
            &others,
            true,
            true,
        )
        .unwrap();
        assert!(
            text.starts_with(
                "ra: no errors left in src/math.rs, but this change caused new errors in other files:"
            ),
            "{text}"
        );
    }

    #[test]
    fn edit_check_hedges_other_files_without_a_baseline() {
        let others = vec![("src/main.rs".to_string(), vec![err(7, "mismatched types")])];
        let text = describe("ra", "src/math.rs", Some(&[]), &[], &others, false, true).unwrap();
        assert!(
            text.starts_with(
                "ra: errors in other files after this change to src/math.rs (some may have been there before):"
            ),
            "{text}"
        );
        let text = describe(
            "ra",
            "src/math.rs",
            Some(&[]),
            &[err(1, "a")],
            &others,
            false,
            true,
        )
        .unwrap();
        assert!(
            text.contains(
                "
Errors in other files (some may have been there before):
"
            ),
            "{text}"
        );
    }

    #[test]
    fn edit_check_caps_every_file_together() {
        let mine: Vec<Value> = (0..15).map(|i| err(i, &format!("m{i}"))).collect();
        let theirs: Vec<Value> = (0..10).map(|i| err(i, &format!("t{i}"))).collect();
        let others = vec![("b.rs".to_string(), theirs)];
        let text = describe("ra", "a.rs", Some(&[]), &mine, &others, true, true).unwrap();
        assert_eq!(text.lines().filter(|l| l.contains(" error: ")).count(), 20);
        assert!(text.contains("… 5 more"), "{text}");
    }

    #[test]
    fn edit_check_ignores_warnings() {
        let warning = json!({ "range": { "start": { "line": 0, "character": 0 } }, "severity": 2, "message": "unused" });
        assert!(errors_only(vec![warning, err(0, "a")]).len() == 1);
    }

    #[test]
    fn edit_check_reports_one_error_per_place_and_code() {
        let short = json!({ "range": { "start": { "line": 5, "character": 20 } }, "severity": 1, "message": "expected String, found i32", "source": "rust-analyzer", "code": "E0308" });
        let long = json!({ "range": { "start": { "line": 5, "character": 20 } }, "severity": 1, "message": "mismatched types\nexpected `String`, found `i32`", "source": "rustc", "code": "E0308" });
        let list = errors_only(vec![short, long.clone(), err(9, "other")]);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], long);
    }
}
